use super::*;

use std::sync::atomic::{AtomicU64, Ordering};

use crate::protocol::codec::encode_frame;
use crate::protocol::{ChatOp, Frame, FrameBody};
use crate::session::SessionLimits;
use crate::store::ServerRoomEventKind;

const PRODUCERS: usize = 8;
const SUBMIT_INTERVAL: Duration = Duration::from_millis(10);
const RESPONSIVENESS_INTERVAL: Duration = Duration::from_millis(10);
const RESPONSIVENESS_LIMIT: Duration = Duration::from_millis(250);
const RSS_GROWTH_LIMIT_BYTES: usize = 64 * 1024 * 1024;

#[derive(Default)]
struct DiscardingTransport {
    frames: u64,
    frame_bytes: u64,
    resources: u64,
    resource_bytes: u64,
}

impl OmenchatTransport for DiscardingTransport {
    fn send_frame(&mut self, _link_id: LinkId, frame_bytes: Vec<u8>) -> ServerResult<()> {
        self.frames = self.frames.saturating_add(1);
        self.frame_bytes = self.frame_bytes.saturating_add(frame_bytes.len() as u64);
        Ok(())
    }

    fn offer_resource(
        &mut self,
        _link_id: LinkId,
        _resource_id: String,
        payload: Vec<u8>,
        _metadata: Vec<u8>,
    ) -> ServerResult<()> {
        self.resources = self.resources.saturating_add(1);
        self.resource_bytes = self.resource_bytes.saturating_add(payload.len() as u64);
        Ok(())
    }

    fn sent_frame_count(&self) -> u64 {
        self.frames
    }

    fn offered_resource_count(&self) -> u64 {
        self.resources
    }

    fn sent_frame_bytes(&self) -> u64 {
        self.frame_bytes
    }

    fn offered_resource_bytes(&self) -> u64 {
        self.resource_bytes
    }
}

fn soak_seconds() -> u64 {
    std::env::var("OMENCHATD_DB_SOAK_SECONDS")
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

fn process_fd_count() -> usize {
    std::fs::read_dir("/proc/self/fd")
        .map(|entries| entries.filter_map(Result::ok).count())
        .unwrap_or(0)
}

fn file_set_bytes(path: &Path) -> u64 {
    ["", "-wal", "-shm"]
        .iter()
        .filter_map(|suffix| std::fs::metadata(format!("{}{suffix}", path.display())).ok())
        .map(|metadata| metadata.len())
        .sum()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "explicit 60-second persistent SQLite/live-worker soak; run through scripts/measure-omenchatd-db.sh"]
async fn persistent_sqlite_worker_stays_responsive_and_commits_monotonic_events_under_load() {
    let duration = Duration::from_secs(soak_seconds());
    let deadline = Instant::now() + duration;
    let root = std::env::temp_dir().join(format!(
        "omenchatd-db-soak.{}-{}",
        std::process::id(),
        current_epoch_ms()
    ));
    std::fs::create_dir_all(&root).expect("create isolated database soak root");
    let database_path = root.join("omenchat.sqlite");
    let store = OmenchatStore::open(&database_path).expect("open persistent soak database");
    store
        .ensure_room("lobby", Some("Database soak lobby"))
        .expect("ensure soak room");
    let engine = SessionEngine::with_limits(
        store,
        SessionLimits {
            rate_messages_per_minute: 1_000_000,
            rate_commands_per_minute: 1_000_000,
            ..SessionLimits::default()
        },
    );
    let worker = Arc::new(LiveServerWorker::new(OmenchatLiveServer::new(
        engine,
        DiscardingTransport::default(),
    )));

    for producer in 0..PRODUCERS {
        let link_id = [producer as u8 + 1; 16];
        worker
            .handle_event(OmenchatLinkEvent::LinkOpened {
                link_id,
                peer: ServerPeer {
                    identity_hash: format!("db-soak-peer-{producer}").into_bytes(),
                    display_name: format!("DB Soak Peer {producer}"),
                    lxmf_destination: None,
                },
            })
            .await
            .expect("open soak link");
        worker
            .handle_event(OmenchatLinkEvent::LinkData {
                link_id,
                context: OMENCHAT_LINK_CONTEXT,
                data: encode_frame(&Frame::new(
                    ChatOp::JoinRoom,
                    1,
                    None,
                    FrameBody::Text("lobby".into()),
                ))
                .expect("encode soak join"),
            })
            .await
            .expect("join soak room");
    }
    let setup_completed = worker.worker_metrics().completed;

    let accepted = Arc::new(AtomicU64::new(0));
    let observed_busy = Arc::new(AtomicU64::new(0));
    let mut producers = Vec::with_capacity(PRODUCERS);
    for producer in 0..PRODUCERS {
        let worker = worker.clone();
        let accepted = accepted.clone();
        let observed_busy = observed_busy.clone();
        producers.push(tokio::spawn(async move {
            let link_id = [producer as u8 + 1; 16];
            let mut interval = tokio::time::interval(SUBMIT_INTERVAL);
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            let mut sequence = 2u32;
            while Instant::now() < deadline {
                interval.tick().await;
                let data = encode_frame(&Frame::new(
                    ChatOp::RoomMessage,
                    sequence,
                    Some(1),
                    FrameBody::Text(format!("db-soak-{producer}-{sequence}")),
                ))
                .expect("encode soak message");
                match worker
                    .handle_event(OmenchatLinkEvent::LinkData {
                        link_id,
                        context: OMENCHAT_LINK_CONTEXT,
                        data,
                    })
                    .await
                {
                    Ok(()) => {
                        accepted.fetch_add(1, Ordering::Relaxed);
                    }
                    Err(error) if error.to_string().contains("worker is busy") => {
                        observed_busy.fetch_add(1, Ordering::Relaxed);
                    }
                    Err(error) => panic!("unexpected live-worker error: {error}"),
                }
                sequence = sequence.saturating_add(1);
            }
        }));
    }

    let heartbeat_max_us = Arc::new(AtomicU64::new(0));
    let heartbeat_ticks = Arc::new(AtomicU64::new(0));
    let heartbeat = {
        let max_us = heartbeat_max_us.clone();
        let ticks = heartbeat_ticks.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(RESPONSIVENESS_INTERVAL);
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            while Instant::now() < deadline {
                let scheduled = interval.tick().await;
                let lateness = Instant::now()
                    .saturating_duration_since(scheduled.into())
                    .as_micros()
                    .try_into()
                    .unwrap_or(u64::MAX);
                max_us.fetch_max(lateness, Ordering::Relaxed);
                ticks.fetch_add(1, Ordering::Relaxed);
            }
        })
    };

    let baseline_rss = process_rss_bytes();
    let baseline_fds = process_fd_count();
    let mut peak_rss = baseline_rss;
    let mut peak_fds = baseline_fds;
    let mut max_in_flight = 0usize;
    let mut sample = 0u64;
    while Instant::now() < deadline {
        tokio::time::sleep(Duration::from_secs(1)).await;
        sample = sample.saturating_add(1);
        let metrics = worker.worker_metrics();
        let rss = process_rss_bytes();
        let fds = process_fd_count();
        peak_rss = peak_rss.max(rss);
        peak_fds = peak_fds.max(fds);
        max_in_flight = max_in_flight.max(metrics.in_flight);
        println!(
            "DB_SOAK_SAMPLE second={sample} accepted={} busy={} worker_completed={} worker_rejected={} worker_in_flight={} worker_avg_us={} worker_max_us={} heartbeat_max_us={} rss_bytes={rss} fds={fds} database_bytes={}",
            accepted.load(Ordering::Relaxed),
            observed_busy.load(Ordering::Relaxed),
            metrics.completed,
            metrics.rejected,
            metrics.in_flight,
            metrics.average_micros,
            metrics.max_micros,
            heartbeat_max_us.load(Ordering::Relaxed),
            file_set_bytes(&database_path),
        );
    }

    for producer in producers {
        producer.await.expect("database soak producer");
    }
    heartbeat.await.expect("database soak heartbeat");
    let metrics = worker.worker_metrics();
    let stats = worker.stats().expect("worker stats");
    let accepted = accepted.load(Ordering::Relaxed);
    let observed_busy = observed_busy.load(Ordering::Relaxed);
    let heartbeat_max_us = heartbeat_max_us.load(Ordering::Relaxed);
    let heartbeat_ticks = heartbeat_ticks.load(Ordering::Relaxed);
    let final_rss = process_rss_bytes();
    let final_fds = process_fd_count();
    let database_bytes = file_set_bytes(&database_path);

    let worker = match Arc::try_unwrap(worker) {
        Ok(worker) => worker,
        Err(_) => panic!("database soak retained a worker owner"),
    };
    drop(worker);

    let reopened = OmenchatStore::open(&database_path).expect("reopen database after soak");
    let events = reopened
        .latest_events(1, accepted as usize + 128)
        .expect("read committed soak events after restart");
    let soak_events = events
        .iter()
        .filter(|event| {
            matches!(
                &event.kind,
                ServerRoomEventKind::Message { body } if body.starts_with("db-soak-")
            )
        })
        .count();
    assert!(events
        .windows(2)
        .all(|pair| pair[1].event_id == pair[0].event_id + 1));
    assert_eq!(soak_events as u64, accepted);
    drop(reopened);

    let integrity: String = rusqlite::Connection::open(&database_path)
        .expect("open integrity connection")
        .query_row("PRAGMA integrity_check", [], |row| row.get(0))
        .expect("run integrity check");
    let rss_delta = peak_rss.saturating_sub(baseline_rss);
    println!(
        "DB_SOAK_SUMMARY duration_seconds={} producers={} submit_interval_ms=10 accepted={} busy_rejected={} setup_completed={} worker_completed={} worker_rejected={} worker_average_us={} worker_max_us={} max_in_flight={} heartbeat_ticks={} heartbeat_max_us={} baseline_rss_bytes={} peak_rss_bytes={} final_rss_bytes={} rss_delta_bytes={} allowed_rss_delta_bytes={} baseline_fds={} peak_fds={} final_fds={} database_bytes={} committed_soak_events={} integrity={}",
        duration.as_secs(),
        PRODUCERS,
        accepted,
        observed_busy,
        setup_completed,
        metrics.completed,
        metrics.rejected,
        metrics.average_micros,
        metrics.max_micros,
        max_in_flight,
        heartbeat_ticks,
        heartbeat_max_us,
        baseline_rss,
        peak_rss,
        final_rss,
        rss_delta,
        RSS_GROWTH_LIMIT_BYTES,
        baseline_fds,
        peak_fds,
        final_fds,
        database_bytes,
        soak_events,
        integrity,
    );

    assert!(accepted >= duration.as_secs().saturating_mul(10));
    assert!(observed_busy > 0);
    assert_eq!(metrics.completed, setup_completed + accepted);
    assert_eq!(metrics.rejected, observed_busy);
    assert_eq!(metrics.in_flight, 0);
    assert!(max_in_flight <= 1);
    assert!(heartbeat_max_us <= RESPONSIVENESS_LIMIT.as_micros() as u64);
    assert!(rss_delta <= RSS_GROWTH_LIMIT_BYTES);
    assert!(peak_fds <= baseline_fds.saturating_add(6));
    assert!(final_fds <= baseline_fds.saturating_add(2));
    assert_eq!(stats.protocol_errors, 0);
    assert_eq!(integrity, "ok");

    std::fs::remove_dir_all(root).expect("remove isolated database soak root");
}
