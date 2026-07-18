use super::*;

use std::time::{Duration, Instant};

use crate::protocol::codec::encode_frame;
use crate::store::OmenchatStore;

const RESIDENT_IDENTIFIED_LINKS: usize = ACTIVE_LINK_MAX_ITEMS - PENDING_HANDSHAKE_MAX_ITEMS;
const RSS_GROWTH_LIMIT_BYTES: usize = 64 * 1024 * 1024;
const FD_GROWTH_LIMIT: usize = 4;
const TASK_GROWTH_LIMIT: usize = 2;
const CLOSE_DEADLINE: Duration = Duration::from_millis(250);

fn soak_seconds() -> u64 {
    std::env::var("OMENCHATD_LINK_SOAK_SECONDS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(60)
        .clamp(1, 600)
}

fn process_rss_bytes() -> usize {
    let Ok(status) = std::fs::read_to_string("/proc/self/status") else {
        return 0;
    };
    status
        .lines()
        .find_map(|line| {
            let kib = line.strip_prefix("VmRSS:")?.split_whitespace().next()?;
            kib.parse::<usize>().ok()
        })
        .unwrap_or(0)
        .saturating_mul(1024)
}

fn proc_entry_count(path: &str) -> usize {
    std::fs::read_dir(path)
        .map(|entries| entries.filter_map(Result::ok).count())
        .unwrap_or(0)
}

#[derive(Debug, Default)]
struct CountingTransport {
    sent_frames: u64,
    sent_frame_bytes: u64,
    offered_resources: u64,
    offered_resource_bytes: u64,
    closed_links: u64,
}

impl OmenchatTransport for CountingTransport {
    fn send_frame(&mut self, _link_id: LinkId, frame_bytes: Vec<u8>) -> ServerResult<()> {
        self.sent_frames = self.sent_frames.saturating_add(1);
        self.sent_frame_bytes = self
            .sent_frame_bytes
            .saturating_add(frame_bytes.len() as u64);
        Ok(())
    }

    fn offer_resource(
        &mut self,
        _link_id: LinkId,
        _resource_id: String,
        payload: Vec<u8>,
        _metadata: Vec<u8>,
    ) -> ServerResult<()> {
        self.offered_resources = self.offered_resources.saturating_add(1);
        self.offered_resource_bytes = self
            .offered_resource_bytes
            .saturating_add(payload.len() as u64);
        Ok(())
    }

    fn sent_frame_count(&self) -> u64 {
        self.sent_frames
    }

    fn offered_resource_count(&self) -> u64 {
        self.offered_resources
    }

    fn sent_frame_bytes(&self) -> u64 {
        self.sent_frame_bytes
    }

    fn offered_resource_bytes(&self) -> u64 {
        self.offered_resource_bytes
    }

    fn close_link(&mut self, _link_id: LinkId) -> ServerResult<()> {
        self.closed_links = self.closed_links.saturating_add(1);
        Ok(())
    }
}

#[test]
#[ignore = "explicit 60-second live-link admission/reconnect soak; run through scripts/measure-omenchatd-links.sh"]
fn live_link_admission_expires_slow_handshakes_and_recovers_under_reconnect_storm() {
    assert!(std::path::Path::new("/proc/self/status").is_file());
    assert!(std::path::Path::new("/proc/self/fd").is_dir());
    assert!(std::path::Path::new("/proc/self/task").is_dir());

    let duration = Duration::from_secs(soak_seconds());
    let store = OmenchatStore::in_memory().expect("isolated in-memory soak store");
    let engine = SessionEngine::new(store);
    let mut live = OmenchatLiveServer::new(engine, CountingTransport::default());
    let identities = (0..RESIDENT_IDENTIFIED_LINKS)
        .map(indexed_identity)
        .collect::<Vec<_>>();
    let mut sequence = 1u64;

    for (index, identity_hash) in identities.iter().enumerate() {
        let link_id = indexed_link_id(sequence);
        sequence = sequence.saturating_add(1);
        open_identified_link(&mut live, link_id, *identity_hash, index as u32 + 1);
    }
    assert_eq!(live.stats().active_links, RESIDENT_IDENTIFIED_LINKS);
    assert_eq!(live.stats().pending_handshakes, 0);

    let baseline_rss = process_rss_bytes();
    let baseline_fds = proc_entry_count("/proc/self/fd");
    let baseline_tasks = proc_entry_count("/proc/self/task");
    let mut peak_rss = baseline_rss;
    let mut peak_fds = baseline_fds;
    let mut peak_tasks = baseline_tasks;
    let mut peak_active = live.stats().active_links;
    let mut peak_pending = 0usize;
    let mut max_close_micros = 0u128;
    let mut cycles = 0u64;
    let started = Instant::now();
    let deadline = started + duration;
    let mut last_sample = 0u64;

    while Instant::now() < deadline {
        let opened_at = 100i64.saturating_add(cycles as i64 * 31);
        for _ in 0..PENDING_HANDSHAKE_MAX_ITEMS {
            let link_id = indexed_link_id(sequence);
            sequence = sequence.saturating_add(1);
            live.handle_event(OmenchatLinkEvent::LinkOpened {
                link_id,
                peer: provisional_peer(link_id),
            })
            .expect("admit slow handshake");
            live.link_opened_at.insert(link_id, opened_at);
        }
        let saturated = live.stats();
        peak_active = peak_active.max(saturated.active_links);
        peak_pending = peak_pending.max(saturated.pending_handshakes);
        assert_eq!(saturated.active_links, ACTIVE_LINK_MAX_ITEMS);
        assert_eq!(saturated.pending_handshakes, PENDING_HANDSHAKE_MAX_ITEMS);

        let overflow = indexed_link_id(sequence);
        sequence = sequence.saturating_add(1);
        let close_started = Instant::now();
        live.handle_event(OmenchatLinkEvent::LinkOpened {
            link_id: overflow,
            peer: provisional_peer(overflow),
        })
        .expect("reject saturated handshake admission");
        max_close_micros = max_close_micros.max(close_started.elapsed().as_micros());

        let expiry_started = Instant::now();
        assert_eq!(
            live.expire_pending_handshakes(opened_at + HANDSHAKE_TIMEOUT_SECONDS),
            PENDING_HANDSHAKE_MAX_ITEMS
        );
        max_close_micros = max_close_micros.max(expiry_started.elapsed().as_micros());

        let identity_index = cycles as usize % identities.len();
        let replacement = indexed_link_id(sequence);
        sequence = sequence.saturating_add(1);
        let replacement_started = Instant::now();
        live.handle_event(OmenchatLinkEvent::LinkOpened {
            link_id: replacement,
            peer: provisional_peer(replacement),
        })
        .expect("open reconnect replacement");
        live.handle_event(OmenchatLinkEvent::PeerIdentified {
            link_id: replacement,
            identity_hash: identities[identity_index],
        })
        .expect("identify reconnect replacement");
        live.handle_event(OmenchatLinkEvent::LinkData {
            link_id: replacement,
            context: OMENCHAT_LINK_CONTEXT,
            data: session_open_frame(cycles as u32 + 1),
        })
        .expect("negotiate reconnect replacement");
        max_close_micros = max_close_micros.max(replacement_started.elapsed().as_micros());

        let stats = live.stats();
        peak_active = peak_active.max(stats.active_links);
        peak_pending = peak_pending.max(stats.pending_handshakes);
        assert_eq!(stats.active_links, RESIDENT_IDENTIFIED_LINKS);
        assert_eq!(stats.pending_handshakes, 0);
        cycles = cycles.saturating_add(1);

        let elapsed_seconds = started.elapsed().as_secs();
        if elapsed_seconds > last_sample {
            last_sample = elapsed_seconds;
            let rss = process_rss_bytes();
            let fds = proc_entry_count("/proc/self/fd");
            let tasks = proc_entry_count("/proc/self/task");
            peak_rss = peak_rss.max(rss);
            peak_fds = peak_fds.max(fds);
            peak_tasks = peak_tasks.max(tasks);
            println!(
                "LINK_SOAK_SAMPLE second={elapsed_seconds} cycles={cycles} active={} pending={} rejected={} expired={} rss_bytes={rss} fds={fds} tasks={tasks} close_max_us={max_close_micros}",
                stats.active_links,
                stats.pending_handshakes,
                stats.link_admission_rejected,
                stats.handshake_expired,
            );
        }
        std::thread::sleep(Duration::from_millis(10));
    }

    for identity_hash in &identities {
        assert_eq!(live.disconnect_identity(identity_hash), 1);
    }
    let final_stats = live.stats();
    let final_rss = process_rss_bytes();
    let final_fds = proc_entry_count("/proc/self/fd");
    let final_tasks = proc_entry_count("/proc/self/task");
    peak_rss = peak_rss.max(final_rss);
    peak_fds = peak_fds.max(final_fds);
    peak_tasks = peak_tasks.max(final_tasks);
    let rss_delta = peak_rss.saturating_sub(baseline_rss);
    let fd_growth = peak_fds.saturating_sub(baseline_fds);
    let task_growth = peak_tasks.saturating_sub(baseline_tasks);
    let expected_rejected = cycles;
    let expected_expired = cycles.saturating_mul(PENDING_HANDSHAKE_MAX_ITEMS as u64);
    let expected_closed = final_stats
        .links_closed
        .saturating_add(final_stats.link_admission_rejected);

    println!(
        "LINK_SOAK_SUMMARY duration_seconds={} cycles={} resident_links={} pending_limit={} active_limit={} peak_active={} peak_pending={} rejected={} expired={} links_closed={} transport_closes={} max_close_us={} close_deadline_us={} baseline_rss_bytes={} peak_rss_bytes={} final_rss_bytes={} rss_delta_bytes={} allowed_rss_delta_bytes={} baseline_fds={} peak_fds={} final_fds={} fd_growth={} allowed_fd_growth={} baseline_tasks={} peak_tasks={} final_tasks={} task_growth={} allowed_task_growth={} final_active={} final_pending={}",
        duration.as_secs(),
        cycles,
        RESIDENT_IDENTIFIED_LINKS,
        PENDING_HANDSHAKE_MAX_ITEMS,
        ACTIVE_LINK_MAX_ITEMS,
        peak_active,
        peak_pending,
        final_stats.link_admission_rejected,
        final_stats.handshake_expired,
        final_stats.links_closed,
        live.transport().closed_links,
        max_close_micros,
        CLOSE_DEADLINE.as_micros(),
        baseline_rss,
        peak_rss,
        final_rss,
        rss_delta,
        RSS_GROWTH_LIMIT_BYTES,
        baseline_fds,
        peak_fds,
        final_fds,
        fd_growth,
        FD_GROWTH_LIMIT,
        baseline_tasks,
        peak_tasks,
        final_tasks,
        task_growth,
        TASK_GROWTH_LIMIT,
        final_stats.active_links,
        final_stats.pending_handshakes,
    );

    assert!(cycles >= duration.as_secs().saturating_mul(10));
    assert_eq!(final_stats.link_admission_rejected, expected_rejected);
    assert_eq!(final_stats.handshake_expired, expected_expired);
    assert_eq!(live.transport().closed_links, expected_closed);
    assert!(peak_active <= ACTIVE_LINK_MAX_ITEMS);
    assert!(peak_pending <= PENDING_HANDSHAKE_MAX_ITEMS);
    assert!(max_close_micros <= CLOSE_DEADLINE.as_micros());
    assert!(rss_delta <= RSS_GROWTH_LIMIT_BYTES);
    assert!(fd_growth <= FD_GROWTH_LIMIT);
    assert!(task_growth <= TASK_GROWTH_LIMIT);
    assert_eq!(final_stats.active_links, 0);
    assert_eq!(final_stats.pending_handshakes, 0);
}

fn open_identified_link(
    live: &mut OmenchatLiveServer<CountingTransport>,
    link_id: LinkId,
    identity_hash: [u8; 16],
    seq: u32,
) {
    live.handle_event(OmenchatLinkEvent::LinkOpened {
        link_id,
        peer: ServerPeer {
            identity_hash: identity_hash.to_vec(),
            display_name: format!("Peer {seq}"),
            lxmf_destination: None,
        },
    })
    .expect("open identified resident link");
    live.handle_event(OmenchatLinkEvent::LinkData {
        link_id,
        context: OMENCHAT_LINK_CONTEXT,
        data: session_open_frame(seq),
    })
    .expect("negotiate resident session");
}

fn session_open_frame(seq: u32) -> Vec<u8> {
    encode_frame(&Frame::new(
        ChatOp::SessionOpen,
        seq,
        None,
        FrameBody::Empty,
    ))
    .expect("encode session open")
}

fn indexed_link_id(index: u64) -> LinkId {
    let mut link_id = [0u8; 16];
    link_id[..8].copy_from_slice(&index.to_be_bytes());
    link_id[8..].copy_from_slice(&(!index).to_be_bytes());
    link_id
}

fn indexed_identity(index: usize) -> [u8; 16] {
    let mut identity = [0u8; 16];
    identity[..8].copy_from_slice(&(index as u64 + 1).to_be_bytes());
    identity[8..].copy_from_slice(&0x4f4d454e43484154u64.to_be_bytes());
    identity
}

fn provisional_peer(link_id: LinkId) -> ServerPeer {
    ServerPeer {
        identity_hash: link_id.to_vec(),
        display_name: format!("link-{}", short_link_id(&link_id)),
        lxmf_destination: None,
    }
}
