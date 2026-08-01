use super::*;

use std::sync::atomic::{AtomicU64, Ordering};

const PRODUCER_INTERVAL: Duration = Duration::from_millis(1);
const CONSUMER_INTERVAL: Duration = Duration::from_millis(20);
const RESOURCE_BYTES: usize = 64 * 1024;
const LINK_COUNT: usize = 8;
const RSS_OVERHEAD_MARGIN_BYTES: usize = 64 * 1024 * 1024;
const CONTROL_DEADLINE: Duration = Duration::from_millis(250);

fn soak_seconds() -> u64 {
    std::env::var("OMENCHATD_QUEUE_SOAK_SECONDS")
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

fn update_max(target: &AtomicU64, value: u64) {
    target.fetch_max(value, Ordering::Relaxed);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "explicit 60-second production-queue soak; run through scripts/measure-omenchatd-backpressure.sh"]
async fn production_queues_bound_slow_resource_consumers_and_keep_control_responsive() {
    let duration = Duration::from_secs(soak_seconds());
    let deadline = Instant::now() + duration;
    let root = std::env::temp_dir().join(format!(
        "omenchatd-queue-soak.{}-{}",
        std::process::id(),
        current_epoch_ms()
    ));
    std::fs::create_dir_all(&root).expect("create isolated soak root");

    let transport_budget = QueueBudget::new(TRANSPORT_QUEUE_BYTES, TRANSPORT_PER_LINK_BYTES);
    let (transport_payload_tx, mut transport_payload_rx) =
        mpsc::channel::<Queued<TransportCommand>>(TRANSPORT_QUEUE_ITEMS);
    let (transport_control_tx, mut transport_control_rx) =
        mpsc::channel::<Queued<TransportCommand>>(TRANSPORT_CONTROL_ITEMS);
    let mut transport = ReticulumOmenchatTransport {
        tx: transport_payload_tx,
        control_tx: transport_control_tx,
        queue_budget: transport_budget.clone(),
        sent_frames: Arc::new(AtomicU64::new(0)),
        offered_resources: Arc::new(AtomicU64::new(0)),
        sent_frame_bytes: Arc::new(AtomicU64::new(0)),
        offered_resource_bytes: Arc::new(AtomicU64::new(0)),
    };

    let event_budget = QueueBudget::new(EVENT_QUEUE_BYTES, EVENT_PER_LINK_BYTES);
    let (event_payload_tx, mut event_payload_rx) =
        mpsc::channel::<Queued<OmenchatLinkEvent>>(EVENT_QUEUE_ITEMS);
    let (event_control_tx, mut event_control_rx) =
        mpsc::channel::<Queued<OmenchatLinkEvent>>(EVENT_CONTROL_ITEMS);
    let event_sender = EventQueueSender {
        payload_tx: event_payload_tx,
        control_tx: event_control_tx,
        budget: event_budget.clone(),
        log_path: root.join("runtime.log"),
    };

    let transport_consumed = Arc::new(AtomicU64::new(0));
    let event_consumed = Arc::new(AtomicU64::new(0));
    let transport_control_acks = Arc::new(AtomicU64::new(0));
    let event_control_acks = Arc::new(AtomicU64::new(0));

    let transport_consumer = {
        let consumed = transport_consumed.clone();
        let control_acks = transport_control_acks.clone();
        tokio::spawn(async move {
            loop {
                let queued = tokio::select! {
                    biased;
                    queued = transport_control_rx.recv() => queued,
                    queued = transport_payload_rx.recv() => queued,
                };
                let Some(queued) = queued else { break };
                match queued.value {
                    TransportCommand::CloseLink { .. } => {
                        control_acks.fetch_add(1, Ordering::Relaxed);
                    }
                    TransportCommand::OfferResource { .. } | TransportCommand::SendFrame { .. } => {
                        consumed.fetch_add(1, Ordering::Relaxed);
                        tokio::time::sleep(CONSUMER_INTERVAL).await;
                    }
                }
            }
        })
    };
    let event_consumer = {
        let consumed = event_consumed.clone();
        let control_acks = event_control_acks.clone();
        tokio::spawn(async move {
            loop {
                let queued = tokio::select! {
                    biased;
                    queued = event_control_rx.recv() => queued,
                    queued = event_payload_rx.recv() => queued,
                };
                let Some(queued) = queued else { break };
                match queued.value {
                    OmenchatLinkEvent::LinkClosed { .. }
                    | OmenchatLinkEvent::LinkOpened { .. }
                    | OmenchatLinkEvent::PeerIdentified { .. }
                    | OmenchatLinkEvent::ResourceTerminal { .. } => {
                        control_acks.fetch_add(1, Ordering::Relaxed);
                    }
                    OmenchatLinkEvent::LinkData { .. }
                    | OmenchatLinkEvent::ResourceReceived { .. } => {
                        consumed.fetch_add(1, Ordering::Relaxed);
                        tokio::time::sleep(CONSUMER_INTERVAL).await;
                    }
                }
            }
        })
    };

    let transport_attempted = Arc::new(AtomicU64::new(0));
    let event_attempted = Arc::new(AtomicU64::new(0));
    let max_control_latency_ms = Arc::new(AtomicU64::new(0));
    let transport_producer = {
        let attempted = transport_attempted.clone();
        let control_acks = transport_control_acks.clone();
        let control_latency = max_control_latency_ms.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(PRODUCER_INTERVAL);
            let mut sequence = 0usize;
            while Instant::now() < deadline {
                interval.tick().await;
                let link_id = [(sequence % LINK_COUNT) as u8 + 1; 16];
                attempted.fetch_add(1, Ordering::Relaxed);
                let _ = transport.offer_resource(
                    link_id,
                    format!("resource-{sequence}"),
                    vec![sequence as u8; RESOURCE_BYTES],
                    b"omenchat-resource:queue-soak".to_vec(),
                );
                sequence += 1;
                if sequence % 100 == 0 {
                    let acks_before = control_acks.load(Ordering::Relaxed);
                    let started = Instant::now();
                    transport
                        .close_link(link_id)
                        .expect("transport reconnect control admission");
                    tokio::time::timeout(CONTROL_DEADLINE, async {
                        while control_acks.load(Ordering::Relaxed) == acks_before {
                            tokio::task::yield_now().await;
                        }
                    })
                    .await
                    .expect("transport reconnect control response");
                    update_max(&control_latency, started.elapsed().as_millis() as u64);
                }
            }
            transport
        })
    };
    let event_producer = {
        let attempted = event_attempted.clone();
        let sender = event_sender.clone();
        let control_acks = event_control_acks.clone();
        let control_latency = max_control_latency_ms.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(PRODUCER_INTERVAL);
            let mut sequence = 0usize;
            while Instant::now() < deadline {
                interval.tick().await;
                let link_id = [(sequence % LINK_COUNT) as u8 + 17; 16];
                attempted.fetch_add(1, Ordering::Relaxed);
                sender.try_send_payload(
                    link_id,
                    RESOURCE_BYTES,
                    OmenchatLinkEvent::ResourceReceived {
                        link_id,
                        resource_hash: [sequence as u8; 32],
                        data: vec![sequence as u8; RESOURCE_BYTES],
                        metadata: Some(b"omenchat-resource:queue-soak".to_vec()),
                    },
                );
                sequence += 1;
                if sequence % 100 == 0 {
                    let acks_before = control_acks.load(Ordering::Relaxed);
                    let started = Instant::now();
                    sender
                        .send_control(OmenchatLinkEvent::LinkClosed {
                            link_id,
                            reason: Some("queue soak reconnect storm".into()),
                        })
                        .await;
                    tokio::time::timeout(CONTROL_DEADLINE, async {
                        while control_acks.load(Ordering::Relaxed) == acks_before {
                            tokio::task::yield_now().await;
                        }
                    })
                    .await
                    .expect("event reconnect control response");
                    update_max(&control_latency, started.elapsed().as_millis() as u64);
                }
            }
            sender
        })
    };

    let baseline_rss = process_rss_bytes();
    let mut peak_rss = baseline_rss;
    let mut peak_fds = process_fd_count();
    let mut max_transport = QueueMetricsSnapshot::default();
    let mut max_events = QueueMetricsSnapshot::default();
    let mut sample = 0u64;
    while Instant::now() < deadline {
        tokio::time::sleep(Duration::from_secs(1)).await;
        sample += 1;
        let transport_snapshot = transport_budget.snapshot();
        let event_snapshot = event_budget.snapshot();
        max_transport.queued_items = max_transport
            .queued_items
            .max(transport_snapshot.queued_items);
        max_transport.queued_bytes = max_transport
            .queued_bytes
            .max(transport_snapshot.queued_bytes);
        max_transport.oldest_age_ms = max_transport
            .oldest_age_ms
            .max(transport_snapshot.oldest_age_ms);
        max_events.queued_items = max_events.queued_items.max(event_snapshot.queued_items);
        max_events.queued_bytes = max_events.queued_bytes.max(event_snapshot.queued_bytes);
        max_events.oldest_age_ms = max_events.oldest_age_ms.max(event_snapshot.oldest_age_ms);
        peak_rss = peak_rss.max(process_rss_bytes());
        peak_fds = peak_fds.max(process_fd_count());
        println!(
            "SOAK_SAMPLE second={sample} rss_bytes={} fds={} transport_items={} transport_bytes={} transport_oldest_ms={} transport_rejected={} event_items={} event_bytes={} event_oldest_ms={} event_rejected={}",
            process_rss_bytes(),
            process_fd_count(),
            transport_snapshot.queued_items,
            transport_snapshot.queued_bytes,
            transport_snapshot.oldest_age_ms,
            transport_snapshot.rejected_items,
            event_snapshot.queued_items,
            event_snapshot.queued_bytes,
            event_snapshot.oldest_age_ms,
            event_snapshot.rejected_items,
        );
    }

    let mut transport = transport_producer.await.expect("transport producer");
    let event_sender = event_producer.await.expect("event producer");
    let transport_controls_started = transport_control_acks.load(Ordering::Relaxed);
    let event_controls_started = event_control_acks.load(Ordering::Relaxed);
    let control_started = Instant::now();
    transport
        .close_link([0xee; 16])
        .expect("transport control accepted while payload saturated");
    event_sender
        .send_control(OmenchatLinkEvent::LinkClosed {
            link_id: [0xef; 16],
            reason: Some("queue soak reconnect probe".into()),
        })
        .await;
    tokio::time::timeout(CONTROL_DEADLINE, async {
        while transport_control_acks.load(Ordering::Relaxed) == transport_controls_started
            || event_control_acks.load(Ordering::Relaxed) == event_controls_started
        {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("control lanes must respond while resource lanes are saturated");
    update_max(
        &max_control_latency_ms,
        control_started.elapsed().as_millis() as u64,
    );

    drop(transport);
    drop(event_sender);
    transport_consumer.abort();
    event_consumer.abort();
    let _ = transport_consumer.await;
    let _ = event_consumer.await;
    tokio::task::yield_now().await;

    let transport_final = transport_budget.snapshot();
    let event_final = event_budget.snapshot();
    let rss_delta = peak_rss.saturating_sub(baseline_rss);
    let allowed_delta = TRANSPORT_QUEUE_BYTES
        .saturating_add(EVENT_QUEUE_BYTES)
        .saturating_add(RSS_OVERHEAD_MARGIN_BYTES);
    let transport_attempted = transport_attempted.load(Ordering::Relaxed);
    let event_attempted = event_attempted.load(Ordering::Relaxed);
    let transport_consumed = transport_consumed.load(Ordering::Relaxed);
    let event_consumed = event_consumed.load(Ordering::Relaxed);
    let transport_controls = transport_control_acks.load(Ordering::Relaxed);
    let event_controls = event_control_acks.load(Ordering::Relaxed);

    println!(
        "SOAK_SUMMARY duration_seconds={} producer_consumer_target=20 transport_attempted={} transport_consumed={} transport_controls={} transport_max_items={} transport_max_bytes={} transport_max_oldest_ms={} transport_rejected={} event_attempted={} event_consumed={} event_controls={} event_max_items={} event_max_bytes={} event_max_oldest_ms={} event_rejected={} baseline_rss_bytes={} peak_rss_bytes={} rss_delta_bytes={} allowed_rss_delta_bytes={} peak_fds={} max_control_latency_ms={} final_transport_items={} final_transport_bytes={} final_event_items={} final_event_bytes={}",
        duration.as_secs(),
        transport_attempted,
        transport_consumed,
        transport_controls,
        max_transport.queued_items,
        max_transport.queued_bytes,
        max_transport.oldest_age_ms,
        transport_final.rejected_items,
        event_attempted,
        event_consumed,
        event_controls,
        max_events.queued_items,
        max_events.queued_bytes,
        max_events.oldest_age_ms,
        event_final.rejected_items,
        baseline_rss,
        peak_rss,
        rss_delta,
        allowed_delta,
        peak_fds,
        max_control_latency_ms.load(Ordering::Relaxed),
        transport_final.queued_items,
        transport_final.queued_bytes,
        event_final.queued_items,
        event_final.queued_bytes,
    );

    assert!(transport_attempted >= transport_consumed.saturating_mul(10));
    assert!(event_attempted >= event_consumed.saturating_mul(10));
    assert!(transport_final.rejected_items > 0);
    assert!(event_final.rejected_items > 0);
    assert!(transport_controls > 1);
    assert!(event_controls > 1);
    assert!(max_transport.queued_items <= TRANSPORT_QUEUE_ITEMS + TRANSPORT_CONTROL_ITEMS);
    assert!(max_transport.queued_bytes <= TRANSPORT_QUEUE_BYTES);
    assert!(max_events.queued_items <= EVENT_QUEUE_ITEMS + EVENT_CONTROL_ITEMS);
    assert!(max_events.queued_bytes <= EVENT_QUEUE_BYTES);
    assert!(max_transport.oldest_age_ms > 0);
    assert!(max_events.oldest_age_ms > 0);
    assert!(rss_delta <= allowed_delta);
    assert_eq!(transport_final.queued_items, 0);
    assert_eq!(transport_final.queued_bytes, 0);
    assert_eq!(event_final.queued_items, 0);
    assert_eq!(event_final.queued_bytes, 0);

    let _ = std::fs::remove_dir_all(root);
}
