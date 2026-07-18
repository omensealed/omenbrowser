use std::collections::VecDeque;

use crate::runtime::event::{
    RuntimeBusEvent, RuntimeEventGap, RuntimeEventGapReason, RuntimeEventSource,
};

const EVENT_DEDUP_MAX_ITEMS: usize = 512;
const EVENT_DEDUP_MAX_BYTES: usize = 256 * 1024;
const EVENT_DEDUP_MAX_KEY_BYTES: usize = 16 * 1024;
const EVENT_DEDUP_CURSOR_WINDOW: u64 = 64;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RuntimeEventWorkerMetrics {
    pub cursor: u64,
    pub accepted_events: u64,
    pub duplicate_events: u64,
    pub dropped_events: u64,
    pub recovery_attempts: u64,
}

#[derive(Debug, Default)]
pub struct RuntimeEventWorkerState {
    metrics: RuntimeEventWorkerMetrics,
    recent_keys: VecDeque<(u64, String)>,
    recent_key_bytes: usize,
}

impl RuntimeEventWorkerState {
    pub fn observe(&mut self, event: &RuntimeBusEvent) -> bool {
        self.metrics.cursor = self.metrics.cursor.saturating_add(1);
        let Some(key) = dedup_key(event) else {
            self.metrics.accepted_events = self.metrics.accepted_events.saturating_add(1);
            return true;
        };
        if self.recent_keys.iter().any(|(cursor, known)| {
            self.metrics.cursor.saturating_sub(*cursor) <= EVENT_DEDUP_CURSOR_WINDOW
                && known == &key
        }) {
            self.metrics.duplicate_events = self.metrics.duplicate_events.saturating_add(1);
            return false;
        }
        self.recent_key_bytes = self.recent_key_bytes.saturating_add(key.len());
        self.recent_keys.push_back((self.metrics.cursor, key));
        while self.recent_keys.len() > EVENT_DEDUP_MAX_ITEMS
            || self.recent_key_bytes > EVENT_DEDUP_MAX_BYTES
        {
            let Some((_, evicted)) = self.recent_keys.pop_front() else {
                break;
            };
            self.recent_key_bytes = self.recent_key_bytes.saturating_sub(evicted.len());
        }
        self.metrics.accepted_events = self.metrics.accepted_events.saturating_add(1);
        true
    }

    pub fn source_gap(&mut self, dropped_count: u64) -> RuntimeEventGap {
        let last_cursor = self.metrics.cursor;
        self.metrics.cursor = self.metrics.cursor.saturating_add(dropped_count);
        self.metrics.dropped_events = self.metrics.dropped_events.saturating_add(dropped_count);
        self.metrics.recovery_attempts = self.metrics.recovery_attempts.saturating_add(1);
        RuntimeEventGap {
            source: RuntimeEventSource::IntegratedBroadcast,
            reason: RuntimeEventGapReason::SourceLag,
            dropped_count,
            last_cursor,
            next_cursor: self.metrics.cursor.saturating_add(1),
            upstream_cursor: None,
        }
    }

    pub fn downstream_byte_gap(&mut self) -> RuntimeEventGap {
        self.metrics.dropped_events = self.metrics.dropped_events.saturating_add(1);
        self.metrics.recovery_attempts = self.metrics.recovery_attempts.saturating_add(1);
        RuntimeEventGap {
            source: RuntimeEventSource::IntegratedBroadcast,
            reason: RuntimeEventGapReason::DownstreamByteBudget,
            dropped_count: 1,
            last_cursor: self.metrics.cursor.saturating_sub(1),
            next_cursor: self.metrics.cursor.saturating_add(1),
            upstream_cursor: None,
        }
    }

    pub fn metrics(&self) -> RuntimeEventWorkerMetrics {
        self.metrics
    }
}

fn dedup_key(event: &RuntimeBusEvent) -> Option<String> {
    if !matches!(
        event,
        RuntimeBusEvent::StatusChanged(_)
            | RuntimeBusEvent::Announce(_)
            | RuntimeBusEvent::PathUpdated(_)
            | RuntimeBusEvent::MessageReceived(_)
            | RuntimeBusEvent::MessageDeliveryUpdated(_)
            | RuntimeBusEvent::LxmfDeliveryEvidence(_)
            | RuntimeBusEvent::PropagationStatus(_)
            | RuntimeBusEvent::InterfaceStats(_)
            | RuntimeBusEvent::ResourceProgress(_)
            | RuntimeBusEvent::ResourceLifecycle(_)
            | RuntimeBusEvent::SdkRpcEvent(_)
            | RuntimeBusEvent::SdkDeliveryUpdated(_)
    ) {
        return None;
    }
    let key = serde_json::to_string(event).ok()?;
    (key.len() <= EVENT_DEDUP_MAX_KEY_BYTES).then_some(key)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::event::PathEvent;

    fn path_event(destination: &str) -> RuntimeBusEvent {
        RuntimeBusEvent::PathUpdated(PathEvent {
            destination_hash: destination.into(),
            known: true,
            hops: Some(1),
        })
    }

    #[test]
    fn exact_control_event_duplicates_are_bounded_and_suppressed() {
        let mut state = RuntimeEventWorkerState::default();
        let event = path_event("00112233445566778899aabbccddeeff");

        assert!(state.observe(&event));
        assert!(!state.observe(&event));
        assert_eq!(state.metrics().cursor, 2);
        assert_eq!(state.metrics().accepted_events, 1);
        assert_eq!(state.metrics().duplicate_events, 1);

        for index in 0..EVENT_DEDUP_MAX_ITEMS + 1 {
            assert!(state.observe(&path_event(&format!("{index:032x}"))));
        }
        assert!(state.recent_keys.len() <= EVENT_DEDUP_MAX_ITEMS);
        assert!(state.recent_key_bytes <= EVENT_DEDUP_MAX_BYTES);
    }

    #[test]
    fn payload_and_debug_events_are_never_deduplicated() {
        let mut state = RuntimeEventWorkerState::default();
        let event = RuntimeBusEvent::Debug("repeatable diagnostic".into());

        assert!(state.observe(&event));
        assert!(state.observe(&event));
        assert_eq!(state.metrics().accepted_events, 2);
        assert_eq!(state.metrics().duplicate_events, 0);
    }

    #[test]
    fn source_lag_advances_cursor_and_records_recovery() {
        let mut state = RuntimeEventWorkerState::default();
        assert!(state.observe(&path_event("00112233445566778899aabbccddeeff")));

        let gap = state.source_gap(3);
        assert_eq!(gap.last_cursor, 1);
        assert_eq!(gap.next_cursor, 5);
        assert_eq!(gap.dropped_count, 3);
        assert_eq!(state.metrics().cursor, 4);
        assert_eq!(state.metrics().dropped_events, 3);
        assert_eq!(state.metrics().recovery_attempts, 1);
    }

    #[test]
    fn downstream_byte_rejection_is_an_explicit_gap() {
        let mut state = RuntimeEventWorkerState::default();
        assert!(state.observe(&RuntimeBusEvent::Debug("payload placeholder".into())));

        let gap = state.downstream_byte_gap();
        assert_eq!(gap.reason, RuntimeEventGapReason::DownstreamByteBudget);
        assert_eq!(gap.dropped_count, 1);
        assert_eq!(gap.last_cursor, 0);
        assert_eq!(gap.next_cursor, 2);
        assert_eq!(state.metrics().dropped_events, 1);
        assert_eq!(state.metrics().recovery_attempts, 1);
    }
}
