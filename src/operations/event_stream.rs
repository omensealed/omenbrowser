use thiserror::Error;

use crate::runtime::{RuntimeBusEvent, RuntimeEventGap, RuntimeEventRecovery, RuntimeEventSource};

use super::{
    EvidenceAuthority, OperationAction, OperationDomain, OperationEvidence, OperationEvidenceKind,
    OperationHistory, OperationId, OperationModelError, OperationRecord, OperationState,
    OperationTarget, OperationTargetKind, OPERATION_EVIDENCE_MAX_ITEMS,
};

pub fn record_event_stream_runtime_event(
    history: &mut OperationHistory,
    event: &RuntimeBusEvent,
    observed_at_unix_ms: i64,
) -> Result<bool, EventStreamOperationError> {
    match event {
        RuntimeBusEvent::StreamGap(gap) => record_gap(history, gap, observed_at_unix_ms),
        RuntimeBusEvent::StreamRecovered(recovery) => {
            record_recovery(history, recovery, observed_at_unix_ms)
        }
        _ => Ok(false),
    }
}

fn record_gap(
    history: &mut OperationHistory,
    gap: &RuntimeEventGap,
    observed_at_unix_ms: i64,
) -> Result<bool, EventStreamOperationError> {
    if observed_at_unix_ms < 0 {
        return Err(EventStreamOperationError::InvalidTimestamp);
    }
    if gap.dropped_count == 0 {
        return Err(EventStreamOperationError::EmptyGap);
    }
    let id = source_operation_id(&gap.source);
    let existing = history.records().find(|record| record.id == id).cloned();
    let recovery_cursor = gap.next_cursor.saturating_sub(1);
    if existing.as_ref().is_some_and(|record| {
        observed_at_unix_ms < record.updated_at_unix_ms
            || record
                .event_cursor
                .is_some_and(|cursor| recovery_cursor <= cursor)
    }) {
        return Ok(false);
    }
    let detail = format!(
        "{} event stream dropped {} event(s) ({})",
        source_label(&gap.source),
        gap.dropped_count,
        gap_reason_label(gap)
    );
    let record = OperationRecord {
        id,
        target: OperationTarget {
            kind: OperationTargetKind::Runtime,
            label: source_target(&gap.source).into(),
        },
        state: OperationState::EventGap,
        authority: EvidenceAuthority::Authoritative,
        evidence: append_evidence(
            existing.as_ref(),
            OperationEvidence {
                kind: OperationEvidenceKind::EventGap,
                authority: EvidenceAuthority::Authoritative,
                at_unix_ms: observed_at_unix_ms,
                detail: Some(detail),
            },
        ),
        progress: None,
        attempt_count: existing
            .as_ref()
            .map_or(1, |record| record.attempt_count.saturating_add(1)),
        stamp_cost: None,
        propagation_node: None,
        created_at_unix_ms: existing
            .as_ref()
            .map_or(observed_at_unix_ms, |record| record.created_at_unix_ms),
        updated_at_unix_ms: observed_at_unix_ms,
        last_error: Some("runtime event gap requires snapshot reconciliation".into()),
        event_cursor: Some(recovery_cursor),
        valid_actions: vec![OperationAction::CopyDiagnostics],
    };
    history.upsert(record)?;
    Ok(true)
}

fn record_recovery(
    history: &mut OperationHistory,
    recovery: &RuntimeEventRecovery,
    observed_at_unix_ms: i64,
) -> Result<bool, EventStreamOperationError> {
    if observed_at_unix_ms < 0 {
        return Err(EventStreamOperationError::InvalidTimestamp);
    }
    let id = source_operation_id(&recovery.source);
    let Some(existing) = history.records().find(|record| record.id == id).cloned() else {
        return Ok(false);
    };
    if !matches!(
        existing.state,
        OperationState::EventGap | OperationState::Reconciling
    ) {
        return Ok(false);
    }
    let missing_snapshots = [
        recovery.status_recovered,
        recovery.interfaces_recovered,
        recovery.network_snapshot_recovered,
        recovery.propagation_recovered,
    ]
    .into_iter()
    .filter(|recovered| !recovered)
    .count();
    let issue_count = recovery.errors.len().saturating_add(missing_snapshots);
    let complete = issue_count == 0;
    let projected_state = if complete {
        OperationState::Completed
    } else {
        OperationState::Reconciling
    };
    if observed_at_unix_ms < existing.updated_at_unix_ms
        || existing
            .event_cursor
            .is_some_and(|cursor| recovery.cursor < cursor)
        || (existing.event_cursor == Some(recovery.cursor) && existing.state == projected_state)
    {
        return Ok(false);
    }
    let authority = if complete {
        EvidenceAuthority::Authoritative
    } else {
        EvidenceAuthority::Uncertain
    };
    let detail = if complete {
        format!(
            "{} event stream snapshot recovery completed (directory={}, messages={})",
            source_label(&recovery.source),
            recovery.directory_entries_recovered,
            recovery.messages_recovered
        )
    } else {
        format!(
            "{} event stream recovery remains incomplete ({} issue(s))",
            source_label(&recovery.source),
            issue_count
        )
    };
    let last_error = (!complete).then(|| {
        format!(
            "runtime event snapshot recovery reported {} issue(s); inspect Network Doctor",
            issue_count
        )
    });
    let record = OperationRecord {
        id,
        target: existing.target.clone(),
        state: projected_state,
        authority,
        evidence: append_evidence(
            Some(&existing),
            OperationEvidence {
                kind: OperationEvidenceKind::Reconciliation,
                authority,
                at_unix_ms: observed_at_unix_ms,
                detail: Some(detail),
            },
        ),
        progress: None,
        attempt_count: existing.attempt_count,
        stamp_cost: None,
        propagation_node: None,
        created_at_unix_ms: existing.created_at_unix_ms,
        updated_at_unix_ms: observed_at_unix_ms,
        last_error,
        event_cursor: Some(recovery.cursor),
        valid_actions: vec![OperationAction::CopyDiagnostics],
    };
    history.upsert(record)?;
    Ok(true)
}

fn append_evidence(
    existing: Option<&OperationRecord>,
    evidence: OperationEvidence,
) -> Vec<OperationEvidence> {
    let mut retained = existing
        .map(|record| record.evidence.clone())
        .unwrap_or_default();
    if retained.len() >= OPERATION_EVIDENCE_MAX_ITEMS {
        retained.remove(0);
    }
    retained.push(evidence);
    retained
}

fn source_operation_id(source: &RuntimeEventSource) -> OperationId {
    let key = match source {
        RuntimeEventSource::IntegratedBroadcast => 1,
        RuntimeEventSource::SdkRpc => 2,
    };
    OperationId::numeric(OperationDomain::RuntimeEventStream, key)
}

fn source_target(source: &RuntimeEventSource) -> &'static str {
    match source {
        RuntimeEventSource::IntegratedBroadcast => "integrated runtime event stream",
        RuntimeEventSource::SdkRpc => "SDK/RPC event stream",
    }
}

fn source_label(source: &RuntimeEventSource) -> &'static str {
    match source {
        RuntimeEventSource::IntegratedBroadcast => "integrated runtime",
        RuntimeEventSource::SdkRpc => "SDK/RPC",
    }
}

fn gap_reason_label(gap: &RuntimeEventGap) -> &'static str {
    match gap.reason {
        crate::runtime::RuntimeEventGapReason::SourceLag => "source lag",
        crate::runtime::RuntimeEventGapReason::DownstreamByteBudget => "downstream byte budget",
        crate::runtime::RuntimeEventGapReason::UpstreamStreamGap => "upstream stream gap",
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum EventStreamOperationError {
    #[error("runtime event observation timestamp is invalid")]
    InvalidTimestamp,
    #[error("runtime event gap reports no dropped events")]
    EmptyGap,
    #[error(transparent)]
    Model(#[from] OperationModelError),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::{RuntimeEventGapReason, RuntimeEventSource};

    fn gap(
        source: RuntimeEventSource,
        reason: RuntimeEventGapReason,
        dropped_count: u64,
        last_cursor: u64,
        next_cursor: u64,
    ) -> RuntimeBusEvent {
        RuntimeBusEvent::StreamGap(RuntimeEventGap {
            source,
            reason,
            dropped_count,
            last_cursor,
            next_cursor,
            upstream_cursor: Some("private-upstream-cursor".into()),
        })
    }

    fn recovery(source: RuntimeEventSource, cursor: u64, errors: &[&str]) -> RuntimeBusEvent {
        RuntimeBusEvent::StreamRecovered(RuntimeEventRecovery {
            source,
            cursor,
            status_recovered: true,
            interfaces_recovered: true,
            network_snapshot_recovered: true,
            propagation_recovered: true,
            directory_entries_recovered: 3,
            messages_recovered: 2,
            errors: errors.iter().map(|error| (*error).into()).collect(),
        })
    }

    fn only_record(history: &OperationHistory) -> &OperationRecord {
        let records = history.records().collect::<Vec<_>>();
        assert_eq!(records.len(), 1);
        records[0]
    }

    #[test]
    fn gap_and_successful_recovery_use_one_bounded_source_record() {
        let mut history = OperationHistory::default();
        assert!(record_event_stream_runtime_event(
            &mut history,
            &gap(
                RuntimeEventSource::IntegratedBroadcast,
                RuntimeEventGapReason::SourceLag,
                3,
                7,
                11,
            ),
            10,
        )
        .expect("gap"));
        let gap_record = only_record(&history);
        assert_eq!(gap_record.id.domain, OperationDomain::RuntimeEventStream);
        assert_eq!(gap_record.state, OperationState::EventGap);
        assert_eq!(gap_record.event_cursor, Some(10));
        assert_eq!(gap_record.attempt_count, 1);
        assert!(!gap_record.state.claims_peer_delivery());

        assert!(record_event_stream_runtime_event(
            &mut history,
            &recovery(RuntimeEventSource::IntegratedBroadcast, 10, &[]),
            20,
        )
        .expect("recovery"));
        let recovered = only_record(&history);
        assert_eq!(recovered.state, OperationState::Completed);
        assert_eq!(recovered.authority, EvidenceAuthority::Authoritative);
        assert!(recovered.last_error.is_none());
        assert_eq!(recovered.evidence.len(), 2);
        assert!(!record_event_stream_runtime_event(
            &mut history,
            &gap(
                RuntimeEventSource::IntegratedBroadcast,
                RuntimeEventGapReason::SourceLag,
                3,
                7,
                11,
            ),
            30,
        )
        .expect("delayed duplicate gap"));
        assert!(!record_event_stream_runtime_event(
            &mut history,
            &recovery(
                RuntimeEventSource::IntegratedBroadcast,
                10,
                &["delayed private recovery error"],
            ),
            40,
        )
        .expect("unsolicited recovery"));
        assert_eq!(only_record(&history).state, OperationState::Completed);
    }

    #[test]
    fn incomplete_recovery_is_uncertain_and_omits_raw_errors_and_cursor() {
        let mut history = OperationHistory::default();
        record_event_stream_runtime_event(
            &mut history,
            &gap(
                RuntimeEventSource::SdkRpc,
                RuntimeEventGapReason::UpstreamStreamGap,
                1,
                8,
                10,
            ),
            10,
        )
        .expect("gap");
        record_event_stream_runtime_event(
            &mut history,
            &recovery(
                RuntimeEventSource::SdkRpc,
                9,
                &["private identity and endpoint detail"],
            ),
            20,
        )
        .expect("incomplete recovery");
        let record = only_record(&history);
        assert_eq!(record.state, OperationState::Reconciling);
        assert_eq!(record.authority, EvidenceAuthority::Uncertain);
        assert!(record
            .last_error
            .as_deref()
            .is_some_and(|error| { error.contains("1 issue") && !error.contains("private") }));
        assert!(!record.evidence.iter().any(|evidence| evidence
            .detail
            .as_deref()
            .is_some_and(|detail| detail.contains("private"))));
    }

    #[test]
    fn missing_typed_snapshot_flag_prevents_false_completion() {
        let mut history = OperationHistory::default();
        record_event_stream_runtime_event(
            &mut history,
            &gap(
                RuntimeEventSource::IntegratedBroadcast,
                RuntimeEventGapReason::SourceLag,
                1,
                4,
                6,
            ),
            10,
        )
        .expect("gap");
        let mut incomplete = recovery(RuntimeEventSource::IntegratedBroadcast, 5, &[]);
        let RuntimeBusEvent::StreamRecovered(ref mut recovery) = incomplete else {
            unreachable!("recovery fixture")
        };
        recovery.network_snapshot_recovered = false;
        record_event_stream_runtime_event(&mut history, &incomplete, 20)
            .expect("incomplete recovery");
        let record = only_record(&history);
        assert_eq!(record.state, OperationState::Reconciling);
        assert_eq!(record.authority, EvidenceAuthority::Uncertain);
        assert!(record
            .last_error
            .as_deref()
            .is_some_and(|error| error.contains("1 issue")));
    }

    #[test]
    fn source_records_are_independent_and_recovery_without_gap_is_ignored() {
        let mut history = OperationHistory::default();
        assert!(!record_event_stream_runtime_event(
            &mut history,
            &recovery(RuntimeEventSource::SdkRpc, 3, &[]),
            5,
        )
        .expect("orphan recovery"));
        for source in [
            RuntimeEventSource::IntegratedBroadcast,
            RuntimeEventSource::SdkRpc,
        ] {
            record_event_stream_runtime_event(
                &mut history,
                &gap(source, RuntimeEventGapReason::DownstreamByteBudget, 1, 3, 5),
                10,
            )
            .expect("source gap");
        }
        assert_eq!(history.records().count(), 2);
    }

    #[test]
    fn stale_duplicate_and_invalid_events_preserve_current_state() {
        let mut history = OperationHistory::default();
        let first = gap(
            RuntimeEventSource::SdkRpc,
            RuntimeEventGapReason::UpstreamStreamGap,
            2,
            8,
            11,
        );
        record_event_stream_runtime_event(&mut history, &first, 10).expect("gap");
        assert!(
            !record_event_stream_runtime_event(&mut history, &first, 11).expect("duplicate gap")
        );
        assert!(!record_event_stream_runtime_event(
            &mut history,
            &recovery(RuntimeEventSource::SdkRpc, 9, &[]),
            12,
        )
        .expect("stale recovery"));
        assert_eq!(only_record(&history).state, OperationState::EventGap);
        assert_eq!(
            record_event_stream_runtime_event(
                &mut history,
                &gap(
                    RuntimeEventSource::SdkRpc,
                    RuntimeEventGapReason::SourceLag,
                    0,
                    10,
                    11,
                ),
                20,
            ),
            Err(EventStreamOperationError::EmptyGap)
        );
        assert_eq!(
            record_event_stream_runtime_event(&mut history, &first, -1),
            Err(EventStreamOperationError::InvalidTimestamp)
        );
    }

    #[test]
    fn completed_stream_can_reopen_and_evidence_remains_bounded() {
        let mut history = OperationHistory::default();
        for cycle in 0..=OPERATION_EVIDENCE_MAX_ITEMS {
            let cursor = u64::try_from(cycle).expect("cursor") * 2;
            record_event_stream_runtime_event(
                &mut history,
                &gap(
                    RuntimeEventSource::IntegratedBroadcast,
                    RuntimeEventGapReason::SourceLag,
                    1,
                    cursor,
                    cursor.saturating_add(2),
                ),
                i64::try_from(cycle * 2).expect("time"),
            )
            .expect("gap");
            record_event_stream_runtime_event(
                &mut history,
                &recovery(
                    RuntimeEventSource::IntegratedBroadcast,
                    cursor.saturating_add(1),
                    &[],
                ),
                i64::try_from(cycle * 2 + 1).expect("time"),
            )
            .expect("recovery");
        }
        let record = only_record(&history);
        assert_eq!(record.state, OperationState::Completed);
        assert_eq!(record.evidence.len(), OPERATION_EVIDENCE_MAX_ITEMS);
        assert_eq!(
            record.attempt_count,
            u32::try_from(OPERATION_EVIDENCE_MAX_ITEMS + 1).expect("attempts")
        );
    }
}
