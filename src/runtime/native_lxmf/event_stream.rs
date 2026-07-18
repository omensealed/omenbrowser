use std::collections::VecDeque;
use std::future::poll_fn;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use lxmf_sdk::LxmfSdkAsync;
use tokio::sync::broadcast;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use crate::runtime::{
    RuntimeBusEvent, RuntimeEventGap, RuntimeEventGapReason, RuntimeEventSource,
    RuntimeLxmfDeliveryState, RuntimeLxmfDeliveryUpdate, RuntimeSdkRpcEvent,
};

const ASYNC_EVENTS_CAPABILITY: &str = "sdk.capability.async_events";
const LOCAL_MAX_EVENT_BYTES: usize = 256 * 1024;
const RECENT_EVENT_IDS_MAX_ITEMS: usize = 512;
const RECENT_EVENT_IDS_MAX_BYTES: usize = 128 * 1024;
const DELIVERY_METADATA_MAX_BYTES: usize = 4 * 1024;
const RECONNECT_BASE_MS: u64 = 250;
const RECONNECT_MAX_MS: u64 = 30_000;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum NativeLxmfSdkEventStreamState {
    #[default]
    Disabled,
    Connecting,
    Connected,
    Unsupported,
    Disconnected,
    Stopped,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NativeLxmfSdkEventStreamSnapshot {
    pub state: NativeLxmfSdkEventStreamState,
    pub negotiated: bool,
    pub upstream_cursor: Option<String>,
    pub reconnects: u64,
    pub accepted_events: u64,
    pub duplicate_events: u64,
    pub dropped_events: u64,
}

#[derive(Default)]
pub struct NativeLxmfSdkEventWorker {
    status: Arc<Mutex<NativeLxmfSdkEventStreamSnapshot>>,
    task: Mutex<Option<JoinHandle<()>>>,
    cancellation: Mutex<Option<CancellationToken>>,
}

impl NativeLxmfSdkEventWorker {
    pub fn start(&self, endpoint: String, event_tx: broadcast::Sender<RuntimeBusEvent>) -> bool {
        let mut task = self.task.lock().expect("native SDK event task lock");
        if task.as_ref().is_some_and(|task| !task.is_finished()) {
            return false;
        }
        if let Some(old_task) = task.take() {
            old_task.abort();
        }
        let cancellation = CancellationToken::new();
        *self
            .cancellation
            .lock()
            .expect("native SDK event cancellation lock") = Some(cancellation.clone());
        let status = Arc::clone(&self.status);
        *task = Some(tokio::spawn(run_rpc_event_worker(
            endpoint,
            event_tx,
            status,
            cancellation,
        )));
        true
    }

    pub fn cancel(&self) {
        if let Some(cancellation) = self
            .cancellation
            .lock()
            .expect("native SDK event cancellation lock")
            .as_ref()
        {
            cancellation.cancel();
        }
    }

    pub async fn stop(&self) {
        self.cancel();
        let task = self.task.lock().expect("native SDK event task lock").take();
        if let Some(mut task) = task {
            if tokio::time::timeout(Duration::from_secs(2), &mut task)
                .await
                .is_err()
            {
                task.abort();
                let _ = task.await;
            }
        }
        *self
            .cancellation
            .lock()
            .expect("native SDK event cancellation lock") = None;
        self.status
            .lock()
            .expect("native SDK event status lock")
            .state = NativeLxmfSdkEventStreamState::Stopped;
    }

    pub fn snapshot(&self) -> NativeLxmfSdkEventStreamSnapshot {
        self.status
            .lock()
            .expect("native SDK event status lock")
            .clone()
    }
}

impl Drop for NativeLxmfSdkEventWorker {
    fn drop(&mut self) {
        if let Ok(cancellation) = self.cancellation.get_mut() {
            if let Some(cancellation) = cancellation.as_ref() {
                cancellation.cancel();
            }
        }
        if let Ok(task) = self.task.get_mut() {
            if let Some(task) = task.take() {
                task.abort();
            }
        }
    }
}

async fn run_rpc_event_worker(
    endpoint: String,
    event_tx: broadcast::Sender<RuntimeBusEvent>,
    status: Arc<Mutex<NativeLxmfSdkEventStreamSnapshot>>,
    cancellation: CancellationToken,
) {
    let mut tracker = SdkRpcEventTracker::default();
    let mut attempt = 0_u32;
    loop {
        if cancellation.is_cancelled() {
            break;
        }
        status.lock().expect("native SDK event status lock").state =
            NativeLxmfSdkEventStreamState::Connecting;

        let backend = lxmf_sdk::RpcBackendClient::new(endpoint.clone());
        let client = Arc::new(lxmf_sdk::Client::new(backend));
        let request = lxmf_sdk::StartRequest::new(lxmf_sdk::SdkConfig::desktop_local_default())
            .with_requested_capability(ASYNC_EVENTS_CAPABILITY);
        let handle = tokio::select! {
            _ = cancellation.cancelled() => break,
            result = client.start_async(request) => match result {
                Ok(handle) => handle,
                Err(_) => {
                    mark_disconnected(&status, false);
                    attempt = attempt.saturating_add(1);
                    if !wait_for_reconnect(&cancellation, attempt).await { break; }
                    continue;
                }
            }
        };
        let async_events = handle
            .effective_capabilities
            .iter()
            .any(|capability| capability == ASYNC_EVENTS_CAPABILITY);
        if !async_events {
            status.lock().expect("native SDK event status lock").state =
                NativeLxmfSdkEventStreamState::Unsupported;
            attempt = attempt.saturating_add(1);
            if !wait_for_reconnect(&cancellation, attempt).await {
                break;
            }
            continue;
        }

        let client_for_subscription = Arc::clone(&client);
        let mut subscription = match tokio::task::spawn_blocking(move || {
            client_for_subscription
                .subscribe_events(lxmf_sdk::SubscriptionStart::Tail)
                .map_err(|_| ())
        })
        .await
        {
            Ok(Ok(subscription)) => subscription,
            _ => {
                mark_disconnected(&status, true);
                attempt = attempt.saturating_add(1);
                if !wait_for_reconnect(&cancellation, attempt).await {
                    break;
                }
                continue;
            }
        };
        if let Some(cursor) = tracker.cursor.clone() {
            subscription.cursor = Some(lxmf_sdk::EventCursor(cursor));
        }
        let Some(mut stream) = client.open_event_stream(&subscription).ok().flatten() else {
            status.lock().expect("native SDK event status lock").state =
                NativeLxmfSdkEventStreamState::Unsupported;
            attempt = attempt.saturating_add(1);
            if !wait_for_reconnect(&cancellation, attempt).await {
                break;
            }
            continue;
        };

        {
            let mut snapshot = status.lock().expect("native SDK event status lock");
            snapshot.state = NativeLxmfSdkEventStreamState::Connected;
            snapshot.negotiated = true;
        }
        attempt = 0;
        let max_event_bytes = handle
            .effective_limits
            .max_event_bytes
            .clamp(1, LOCAL_MAX_EVENT_BYTES);

        loop {
            let next = tokio::select! {
                _ = cancellation.cancelled() => return,
                next = poll_fn(|cx| stream.as_mut().poll_next(cx)) => next,
            };
            match next {
                Some(Ok(event)) => {
                    let outcome = tracker.observe(event, max_event_bytes);
                    update_status_from_tracker(&status, &tracker);
                    if let Some(gap) = outcome.gap {
                        let _ = event_tx.send(RuntimeBusEvent::StreamGap(gap));
                    }
                    if let Some(event) = outcome.event {
                        let _ = event_tx.send(event);
                    }
                }
                Some(Err(_)) | None => {
                    mark_disconnected(&status, true);
                    break;
                }
            }
        }
        attempt = attempt.saturating_add(1);
        if !wait_for_reconnect(&cancellation, attempt).await {
            break;
        }
    }
    status.lock().expect("native SDK event status lock").state =
        NativeLxmfSdkEventStreamState::Stopped;
}

fn mark_disconnected(status: &Arc<Mutex<NativeLxmfSdkEventStreamSnapshot>>, negotiated: bool) {
    let mut snapshot = status.lock().expect("native SDK event status lock");
    snapshot.state = NativeLxmfSdkEventStreamState::Disconnected;
    snapshot.negotiated |= negotiated;
    snapshot.reconnects = snapshot.reconnects.saturating_add(1);
}

fn update_status_from_tracker(
    status: &Arc<Mutex<NativeLxmfSdkEventStreamSnapshot>>,
    tracker: &SdkRpcEventTracker,
) {
    let mut snapshot = status.lock().expect("native SDK event status lock");
    snapshot.upstream_cursor = tracker.cursor.clone();
    snapshot.accepted_events = tracker.accepted_events;
    snapshot.duplicate_events = tracker.duplicate_events;
    snapshot.dropped_events = tracker.dropped_events;
}

async fn wait_for_reconnect(cancellation: &CancellationToken, attempt: u32) -> bool {
    tokio::select! {
        _ = cancellation.cancelled() => false,
        _ = tokio::time::sleep(Duration::from_millis(reconnect_delay_ms(attempt))) => true,
    }
}

fn reconnect_delay_ms(attempt: u32) -> u64 {
    let exponent = attempt.saturating_sub(1).min(7);
    let base = RECONNECT_BASE_MS
        .saturating_mul(1_u64 << exponent)
        .min(RECONNECT_MAX_MS);
    let jitter = base.saturating_mul(u64::from(attempt.wrapping_mul(37) % 21)) / 100;
    base.saturating_add(jitter).min(RECONNECT_MAX_MS)
}

#[derive(Default)]
struct SdkRpcEventTracker {
    cursor: Option<String>,
    runtime_id: Option<String>,
    stream_id: Option<String>,
    last_seq_no: Option<u64>,
    recent_event_ids: VecDeque<String>,
    recent_event_id_bytes: usize,
    accepted_events: u64,
    duplicate_events: u64,
    dropped_events: u64,
}

struct SdkRpcEventOutcome {
    event: Option<RuntimeBusEvent>,
    gap: Option<RuntimeEventGap>,
}

impl SdkRpcEventTracker {
    fn observe(&mut self, event: lxmf_sdk::SdkEvent, max_event_bytes: usize) -> SdkRpcEventOutcome {
        let cursor = format!(
            "v2:{}:{}:{}",
            event.runtime_id, event.stream_id, event.seq_no
        );
        if self
            .recent_event_ids
            .iter()
            .any(|known| known == &event.event_id)
            || (self.runtime_id.as_deref() == Some(event.runtime_id.as_str())
                && self.stream_id.as_deref() == Some(event.stream_id.as_str())
                && self.last_seq_no.is_some_and(|seq| event.seq_no <= seq))
        {
            self.duplicate_events = self.duplicate_events.saturating_add(1);
            return SdkRpcEventOutcome {
                event: None,
                gap: None,
            };
        }

        let explicit_gap = event.event_type.eq_ignore_ascii_case("StreamGap");
        let same_stream = self.runtime_id.as_deref() == Some(event.runtime_id.as_str())
            && self.stream_id.as_deref() == Some(event.stream_id.as_str());
        let sequence_gap = same_stream
            && self
                .last_seq_no
                .is_some_and(|seq| event.seq_no > seq.saturating_add(1));
        let dropped_count = if explicit_gap {
            event
                .payload
                .get("dropped_count")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(1)
        } else if sequence_gap {
            event
                .seq_no
                .saturating_sub(self.last_seq_no.unwrap_or(event.seq_no))
                .saturating_sub(1)
        } else {
            0
        };
        let gap = (dropped_count > 0).then(|| RuntimeEventGap {
            source: RuntimeEventSource::SdkRpc,
            reason: RuntimeEventGapReason::UpstreamStreamGap,
            dropped_count,
            last_cursor: self.last_seq_no.unwrap_or(0),
            next_cursor: event.seq_no,
            upstream_cursor: Some(cursor.clone()),
        });
        self.dropped_events = self.dropped_events.saturating_add(dropped_count);
        self.runtime_id = Some(event.runtime_id.clone());
        self.stream_id = Some(event.stream_id.clone());
        self.last_seq_no = Some(event.seq_no);
        self.cursor = Some(cursor.clone());

        let encoded_len = serde_json::to_vec(&event).map_or(usize::MAX, |bytes| bytes.len());
        if encoded_len > max_event_bytes.min(LOCAL_MAX_EVENT_BYTES) {
            self.dropped_events = self.dropped_events.saturating_add(1);
            return SdkRpcEventOutcome {
                event: None,
                gap: Some(RuntimeEventGap {
                    source: RuntimeEventSource::SdkRpc,
                    reason: RuntimeEventGapReason::DownstreamByteBudget,
                    dropped_count: 1,
                    last_cursor: event.seq_no.saturating_sub(1),
                    next_cursor: event.seq_no.saturating_add(1),
                    upstream_cursor: Some(cursor),
                }),
            };
        }

        self.recent_event_id_bytes = self
            .recent_event_id_bytes
            .saturating_add(event.event_id.len());
        self.recent_event_ids.push_back(event.event_id.clone());
        while self.recent_event_ids.len() > RECENT_EVENT_IDS_MAX_ITEMS
            || self.recent_event_id_bytes > RECENT_EVENT_IDS_MAX_BYTES
        {
            let Some(evicted) = self.recent_event_ids.pop_front() else {
                break;
            };
            self.recent_event_id_bytes = self.recent_event_id_bytes.saturating_sub(evicted.len());
        }
        self.accepted_events = self.accepted_events.saturating_add(1);
        let sdk_event = RuntimeSdkRpcEvent {
            event_id: event.event_id,
            runtime_id: event.runtime_id,
            stream_id: event.stream_id,
            seq_no: event.seq_no,
            contract_version: event.contract_version,
            ts_ms: event.ts_ms,
            event_type: event.event_type,
            severity: format!("{:?}", event.severity).to_ascii_lowercase(),
            source_component: event.source_component,
            operation_id: event.operation_id,
            message_id: event.message_id,
            peer_id: event.peer_id,
            correlation_id: event.correlation_id,
            payload: event.payload,
            cursor: cursor.clone(),
        };
        let runtime_event = runtime_delivery_update(&sdk_event)
            .map(RuntimeBusEvent::SdkDeliveryUpdated)
            .unwrap_or_else(|| RuntimeBusEvent::SdkRpcEvent(sdk_event));
        SdkRpcEventOutcome {
            event: Some(runtime_event),
            gap,
        }
    }
}

fn runtime_delivery_update(event: &RuntimeSdkRpcEvent) -> Option<RuntimeLxmfDeliveryUpdate> {
    let state_value = match event.event_type.as_str() {
        "DeliveryStateTransition" => event
            .payload
            .get("to")
            .or_else(|| event.payload.get("state"))
            .cloned(),
        "delivery_cancelled" => Some(serde_json::Value::String("cancelled".into())),
        _ => None,
    }?;
    let upstream_state = serde_json::from_value::<lxmf_sdk::DeliveryState>(state_value).ok()?;
    let state = map_upstream_delivery_state(upstream_state);
    let message_id = event
        .message_id
        .clone()
        .filter(|value| value.len() <= DELIVERY_METADATA_MAX_BYTES)
        .or_else(|| payload_string(&event.payload, &["message_id", "id"]))
        .or_else(|| {
            event
                .payload
                .get("message")
                .and_then(|message| payload_string(message, &["message_id", "id"]))
        })?;
    let peer_hash = event
        .peer_id
        .clone()
        .filter(|value| value.len() <= DELIVERY_METADATA_MAX_BYTES)
        .or_else(|| payload_string(&event.payload, &["peer", "peer_id", "destination_hash"]));
    let previous_state = event
        .payload
        .get("from")
        .cloned()
        .and_then(|value| serde_json::from_value::<lxmf_sdk::DeliveryState>(value).ok())
        .map(map_upstream_delivery_state);
    let terminal = event
        .payload
        .get("terminal")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or_else(|| state.is_terminal());
    let attempts = event
        .payload
        .get("attempts")
        .and_then(serde_json::Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())
        .unwrap_or(0);
    let reason_code = payload_string(&event.payload, &["reason_code", "reason"]);
    if event.event_id.len() > DELIVERY_METADATA_MAX_BYTES
        || event.cursor.len() > DELIVERY_METADATA_MAX_BYTES
    {
        return None;
    }
    Some(RuntimeLxmfDeliveryUpdate {
        message_id,
        peer_hash,
        previous_state,
        state,
        terminal,
        attempts,
        reason_code,
        last_updated_ms: event.ts_ms,
        event_id: event.event_id.clone(),
        seq_no: event.seq_no,
        cursor: event.cursor.clone(),
    })
}

fn payload_string(payload: &serde_json::Value, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| {
        payload
            .get(*key)
            .and_then(serde_json::Value::as_str)
            .filter(|value| value.len() <= DELIVERY_METADATA_MAX_BYTES)
            .map(str::to_owned)
    })
}

fn map_upstream_delivery_state(state: lxmf_sdk::DeliveryState) -> RuntimeLxmfDeliveryState {
    match state {
        lxmf_sdk::DeliveryState::Queued => RuntimeLxmfDeliveryState::Queued,
        lxmf_sdk::DeliveryState::Dispatching => RuntimeLxmfDeliveryState::Dispatching,
        lxmf_sdk::DeliveryState::InFlight => RuntimeLxmfDeliveryState::InFlight,
        lxmf_sdk::DeliveryState::Sent => RuntimeLxmfDeliveryState::Sent,
        lxmf_sdk::DeliveryState::Delivered => RuntimeLxmfDeliveryState::Delivered,
        lxmf_sdk::DeliveryState::Failed => RuntimeLxmfDeliveryState::Failed,
        lxmf_sdk::DeliveryState::Cancelled => RuntimeLxmfDeliveryState::Cancelled,
        lxmf_sdk::DeliveryState::Expired => RuntimeLxmfDeliveryState::Expired,
        lxmf_sdk::DeliveryState::Rejected => RuntimeLxmfDeliveryState::Rejected,
        _ => RuntimeLxmfDeliveryState::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sdk_event(seq_no: u64, event_id: &str, event_type: &str) -> lxmf_sdk::SdkEvent {
        serde_json::from_value(serde_json::json!({
            "event_id": event_id,
            "runtime_id": "runtime-1",
            "stream_id": "stream-1",
            "seq_no": seq_no,
            "contract_version": 2,
            "ts_ms": seq_no,
            "event_type": event_type,
            "severity": "info",
            "source_component": "test",
            "operation_id": null,
            "message_id": null,
            "peer_id": null,
            "correlation_id": null,
            "trace_id": null,
            "payload": {},
            "extensions": {}
        }))
        .expect("SDK event fixture")
    }

    #[test]
    fn tracker_preserves_cursor_and_deduplicates_replayed_events() {
        let mut tracker = SdkRpcEventTracker::default();
        let first = tracker.observe(sdk_event(7, "event-7", "RuntimeStateChanged"), 64 * 1024);
        assert!(first.event.is_some());
        assert_eq!(tracker.cursor.as_deref(), Some("v2:runtime-1:stream-1:7"));

        let duplicate = tracker.observe(sdk_event(7, "event-7", "RuntimeStateChanged"), 64 * 1024);
        assert!(duplicate.event.is_none());
        assert!(duplicate.gap.is_none());
        assert_eq!(tracker.duplicate_events, 1);
    }

    #[test]
    fn tracker_reports_sequence_and_explicit_stream_gaps() {
        let mut tracker = SdkRpcEventTracker::default();
        tracker.observe(sdk_event(2, "event-2", "RuntimeStateChanged"), 64 * 1024);
        let sequence_gap =
            tracker.observe(sdk_event(6, "event-6", "RuntimeStateChanged"), 64 * 1024);
        assert_eq!(sequence_gap.gap.expect("sequence gap").dropped_count, 3);

        let mut explicit = sdk_event(7, "event-7", "StreamGap");
        explicit.payload = serde_json::json!({ "dropped_count": 4 });
        let explicit_gap = tracker.observe(explicit, 64 * 1024);
        assert_eq!(explicit_gap.gap.expect("explicit gap").dropped_count, 4);
    }

    #[test]
    fn tracker_rejects_events_over_the_negotiated_byte_limit() {
        let mut tracker = SdkRpcEventTracker::default();
        let mut event = sdk_event(1, "large", "DeliveryStateTransition");
        event.payload = serde_json::json!({ "body": "x".repeat(4096) });
        let outcome = tracker.observe(event, 1024);
        assert!(outcome.event.is_none());
        assert_eq!(
            outcome.gap.expect("byte gap").reason,
            RuntimeEventGapReason::DownstreamByteBudget
        );
    }

    #[test]
    fn delivery_transition_maps_through_upstream_typed_state() {
        let mut tracker = SdkRpcEventTracker::default();
        let mut event = sdk_event(9, "delivery-9", "DeliveryStateTransition");
        event.message_id = Some("message-9".into());
        event.peer_id = Some("peer-9".into());
        event.payload = serde_json::json!({
            "from": "in_flight",
            "to": "sent",
            "terminal": false,
            "attempts": 2
        });

        let outcome = tracker.observe(event, 64 * 1024);
        let Some(RuntimeBusEvent::SdkDeliveryUpdated(update)) = outcome.event else {
            panic!("typed delivery update");
        };
        assert_eq!(update.message_id, "message-9");
        assert_eq!(update.peer_hash.as_deref(), Some("peer-9"));
        assert_eq!(
            update.previous_state,
            Some(RuntimeLxmfDeliveryState::InFlight)
        );
        assert_eq!(update.state, RuntimeLxmfDeliveryState::Sent);
        assert!(!update.terminal);
        assert_eq!(update.attempts, 2);
    }

    #[test]
    fn delivery_transition_preserves_terminal_and_forward_compatible_states() {
        let mut delivered = sdk_event(10, "delivery-10", "DeliveryStateTransition");
        delivered.message_id = Some("message-10".into());
        delivered.payload = serde_json::json!({ "to": "delivered" });
        let delivered = RuntimeSdkRpcEvent {
            event_id: delivered.event_id,
            runtime_id: delivered.runtime_id,
            stream_id: delivered.stream_id,
            seq_no: delivered.seq_no,
            contract_version: delivered.contract_version,
            ts_ms: delivered.ts_ms,
            event_type: delivered.event_type,
            severity: "info".into(),
            source_component: delivered.source_component,
            operation_id: delivered.operation_id,
            message_id: delivered.message_id,
            peer_id: delivered.peer_id,
            correlation_id: delivered.correlation_id,
            payload: delivered.payload,
            cursor: "v2:runtime-1:stream-1:10".into(),
        };
        let update = runtime_delivery_update(&delivered).expect("delivered update");
        assert_eq!(update.state, RuntimeLxmfDeliveryState::Delivered);
        assert!(update.terminal);

        let mut future = delivered;
        future.payload = serde_json::json!({ "to": "future_delivery_state" });
        let future = runtime_delivery_update(&future).expect("forward-compatible update");
        assert_eq!(future.state, RuntimeLxmfDeliveryState::Unknown);
        assert!(!future.terminal);
    }

    #[test]
    fn reconnect_backoff_is_bounded_and_varied() {
        assert!(reconnect_delay_ms(1) >= RECONNECT_BASE_MS);
        assert!(reconnect_delay_ms(2) > reconnect_delay_ms(1));
        assert_eq!(reconnect_delay_ms(u32::MAX), RECONNECT_MAX_MS);
    }

    #[tokio::test]
    async fn worker_cancellation_joins_the_owned_reconnect_task() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("reserve local port");
        let endpoint = format!(
            "tcp://127.0.0.1:{}/rpc",
            listener.local_addr().expect("local address").port()
        );
        drop(listener);
        let worker = NativeLxmfSdkEventWorker::default();
        let (event_tx, _) = broadcast::channel(4);
        assert!(worker.start(endpoint, event_tx));
        assert!(!worker.start("tcp://127.0.0.1:1/rpc".into(), broadcast::channel(4).0));

        tokio::time::timeout(Duration::from_secs(1), worker.stop())
            .await
            .expect("worker stop timeout");
        assert_eq!(
            worker.snapshot().state,
            NativeLxmfSdkEventStreamState::Stopped
        );
    }
}
