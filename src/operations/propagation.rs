use thiserror::Error;

use crate::runtime::{
    PropagationSyncEvent, PropagationSyncEventStatus, PropagationSyncStage, RuntimeBusEvent,
};

use super::{
    EvidenceAuthority, OperationAction, OperationDomain, OperationEvidence, OperationEvidenceKind,
    OperationHistory, OperationId, OperationModelError, OperationRecord, OperationState,
    OperationTarget, OperationTargetKind, OPERATION_EVIDENCE_MAX_ITEMS, OPERATION_TEXT_MAX_BYTES,
};

const UNKNOWN_PROPAGATION_TARGET: &str = "selected propagation node";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PropagationOperationCounts {
    pub queued: usize,
    pub in_flight: usize,
    pub settled: usize,
    pub failed: usize,
    pub expired: usize,
    pub cancelled: usize,
    pub uncertain: usize,
}

pub fn propagation_operation_counts(history: &OperationHistory) -> PropagationOperationCounts {
    let mut counts = PropagationOperationCounts::default();
    for record in history.records().filter(|record| {
        matches!(
            record.id.domain,
            OperationDomain::LxmfMessage | OperationDomain::PropagationSync
        )
    }) {
        match record.state {
            OperationState::Waiting | OperationState::Queued => counts.queued += 1,
            OperationState::Dispatching
            | OperationState::TransportAccepted
            | OperationState::ReceiptObserved
            | OperationState::Transferring
            | OperationState::Active
            | OperationState::Reconciling
            | OperationState::EventGap => counts.in_flight += 1,
            OperationState::Delivered | OperationState::Completed => counts.settled += 1,
            OperationState::Failed | OperationState::Rejected => counts.failed += 1,
            OperationState::Expired => counts.expired += 1,
            OperationState::Cancelled => counts.cancelled += 1,
        }
        if record.authority == EvidenceAuthority::Uncertain
            || matches!(
                record.state,
                OperationState::Reconciling | OperationState::EventGap
            )
        {
            counts.uncertain += 1;
        }
    }
    counts
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PropagationSyncOutcome<'a> {
    pub succeeded: bool,
    pub selected_node: Option<&'a str>,
    pub messages_received: usize,
    pub delivery_updates: usize,
}

pub fn begin_propagation_sync(
    history: &mut OperationHistory,
    generation: u64,
    selected_node: Option<&str>,
    observed_at_unix_ms: i64,
) -> Result<bool, PropagationOperationError> {
    validate_timestamp(observed_at_unix_ms)?;
    let id = propagation_operation_id(generation);
    if history.records().any(|record| record.id == id) {
        return Ok(false);
    }
    let record = OperationRecord {
        id,
        target: propagation_target(selected_node)?,
        state: OperationState::Queued,
        authority: EvidenceAuthority::Authoritative,
        evidence: vec![OperationEvidence {
            kind: OperationEvidenceKind::QueueAdmission,
            authority: EvidenceAuthority::Authoritative,
            at_unix_ms: observed_at_unix_ms,
            detail: Some("propagation synchronization queued by the application".into()),
        }],
        progress: None,
        attempt_count: 1,
        stamp_cost: None,
        propagation_node: None,
        created_at_unix_ms: observed_at_unix_ms,
        updated_at_unix_ms: observed_at_unix_ms,
        last_error: None,
        event_cursor: None,
        valid_actions: vec![OperationAction::CopyDiagnostics],
    };
    history.upsert(record)?;
    Ok(true)
}

pub fn record_propagation_sync_runtime_event(
    history: &mut OperationHistory,
    generation: Option<u64>,
    event: &RuntimeBusEvent,
    observed_at_unix_ms: i64,
) -> Result<bool, PropagationOperationError> {
    let RuntimeBusEvent::PropagationSync(event) = event else {
        return Ok(false);
    };
    let Some(generation) = generation else {
        return Ok(false);
    };
    if event.stage == PropagationSyncStage::Complete
        && event.status == PropagationSyncEventStatus::Progress
    {
        return Ok(false);
    }
    record_typed_progress(history, generation, event, observed_at_unix_ms)
}

pub fn finish_propagation_sync(
    history: &mut OperationHistory,
    generation: u64,
    outcome: PropagationSyncOutcome<'_>,
    observed_at_unix_ms: i64,
) -> Result<bool, PropagationOperationError> {
    validate_timestamp(observed_at_unix_ms)?;
    let id = propagation_operation_id(generation);
    let Some(existing) = history.records().find(|record| record.id == id).cloned() else {
        return Ok(false);
    };
    if observed_at_unix_ms < existing.updated_at_unix_ms {
        return Ok(false);
    }
    let state = if outcome.succeeded {
        OperationState::Completed
    } else {
        OperationState::Failed
    };
    if existing.state == state && existing.updated_at_unix_ms == observed_at_unix_ms {
        return Ok(false);
    }
    let (evidence_kind, detail, last_error) = if outcome.succeeded {
        (
            OperationEvidenceKind::OperationCompletion,
            format!(
                "propagation synchronization completed; received {} message(s) and applied {} delivery update(s)",
                outcome.messages_received, outcome.delivery_updates
            ),
            None,
        )
    } else {
        (
            OperationEvidenceKind::Failure,
            "propagation synchronization ended with a blocker; peer delivery is unchanged".into(),
            Some("propagation synchronization blocked or failed; inspect diagnostics".into()),
        )
    };
    let record = OperationRecord {
        id,
        target: outcome
            .selected_node
            .and_then(|node| propagation_target(Some(node)).ok())
            .unwrap_or_else(|| existing.target.clone()),
        state,
        authority: EvidenceAuthority::Authoritative,
        evidence: append_evidence(
            &existing,
            OperationEvidence {
                kind: evidence_kind,
                authority: EvidenceAuthority::Authoritative,
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
        event_cursor: None,
        valid_actions: vec![OperationAction::CopyDiagnostics],
    };
    history.upsert(record)?;
    Ok(true)
}

fn record_typed_progress(
    history: &mut OperationHistory,
    generation: u64,
    event: &PropagationSyncEvent,
    observed_at_unix_ms: i64,
) -> Result<bool, PropagationOperationError> {
    validate_timestamp(observed_at_unix_ms)?;
    let id = propagation_operation_id(generation);
    let Some(existing) = history.records().find(|record| record.id == id).cloned() else {
        return Ok(false);
    };
    let (state, authority, evidence_kind, detail, last_error) = project_event(event);
    if observed_at_unix_ms < existing.updated_at_unix_ms
        || existing.state.is_terminal()
        || existing.evidence.last().is_some_and(|evidence| {
            evidence.at_unix_ms == observed_at_unix_ms
                && evidence.kind == evidence_kind
                && evidence.detail.as_deref() == Some(detail.as_str())
        })
    {
        return Ok(false);
    }
    let target = event
        .destination_hash
        .as_deref()
        .map(|node| propagation_target(Some(node)))
        .transpose()?
        .unwrap_or(existing.target.clone());
    let record = OperationRecord {
        id,
        target,
        state,
        authority,
        evidence: append_evidence(
            &existing,
            OperationEvidence {
                kind: evidence_kind,
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
        event_cursor: None,
        valid_actions: vec![OperationAction::CopyDiagnostics],
    };
    history.upsert(record)?;
    Ok(true)
}

fn project_event(
    event: &PropagationSyncEvent,
) -> (
    OperationState,
    EvidenceAuthority,
    OperationEvidenceKind,
    String,
    Option<String>,
) {
    let stage = stage_label(&event.stage);
    match event.status {
        PropagationSyncEventStatus::Started => (
            OperationState::Dispatching,
            EvidenceAuthority::Authoritative,
            OperationEvidenceKind::Dispatch,
            format!("propagation synchronization started {stage}"),
            None,
        ),
        PropagationSyncEventStatus::Progress => (
            OperationState::Active,
            EvidenceAuthority::Authoritative,
            OperationEvidenceKind::OperationProgress,
            format!("propagation synchronization is processing {stage}"),
            None,
        ),
        PropagationSyncEventStatus::Complete if event.stage == PropagationSyncStage::Complete => (
            OperationState::Completed,
            EvidenceAuthority::Authoritative,
            OperationEvidenceKind::OperationCompletion,
            "propagation runtime reports synchronization complete".into(),
            None,
        ),
        PropagationSyncEventStatus::Complete => (
            OperationState::Active,
            EvidenceAuthority::Authoritative,
            OperationEvidenceKind::OperationProgress,
            format!("propagation synchronization completed {stage}"),
            None,
        ),
        PropagationSyncEventStatus::Blocked => (
            OperationState::Reconciling,
            EvidenceAuthority::Authoritative,
            OperationEvidenceKind::Reconciliation,
            format!("propagation synchronization is blocked at {stage}"),
            Some("propagation synchronization is blocked; inspect diagnostics".into()),
        ),
        PropagationSyncEventStatus::Failed => (
            OperationState::Failed,
            EvidenceAuthority::Authoritative,
            OperationEvidenceKind::Failure,
            format!("propagation synchronization failed at {stage}"),
            Some("propagation synchronization failed; inspect diagnostics".into()),
        ),
    }
}

fn append_evidence(
    existing: &OperationRecord,
    evidence: OperationEvidence,
) -> Vec<OperationEvidence> {
    let mut retained = existing.evidence.clone();
    if let Some(last) = retained.last_mut().filter(|last| {
        last.kind == evidence.kind
            && last.authority == evidence.authority
            && last.detail == evidence.detail
    }) {
        *last = evidence;
        return retained;
    }
    if retained.len() >= OPERATION_EVIDENCE_MAX_ITEMS {
        retained.remove(0);
    }
    retained.push(evidence);
    retained
}

fn propagation_target(
    selected_node: Option<&str>,
) -> Result<OperationTarget, PropagationOperationError> {
    let label = match selected_node {
        Some(node) => normalize_node(node)?,
        None => UNKNOWN_PROPAGATION_TARGET.into(),
    };
    Ok(OperationTarget {
        kind: OperationTargetKind::Destination,
        label,
    })
}

fn normalize_node(node: &str) -> Result<String, PropagationOperationError> {
    let node = node.trim();
    if node.is_empty()
        || node.len() > OPERATION_TEXT_MAX_BYTES
        || node.chars().any(char::is_control)
    {
        return Err(PropagationOperationError::InvalidNode);
    }
    Ok(node.to_ascii_lowercase())
}

fn validate_timestamp(observed_at_unix_ms: i64) -> Result<(), PropagationOperationError> {
    if observed_at_unix_ms < 0 {
        Err(PropagationOperationError::InvalidTimestamp)
    } else {
        Ok(())
    }
}

fn propagation_operation_id(generation: u64) -> OperationId {
    OperationId::numeric(OperationDomain::PropagationSync, generation)
}

fn stage_label(stage: &PropagationSyncStage) -> &'static str {
    match stage {
        PropagationSyncStage::SelectNode => "node selection",
        PropagationSyncStage::PathCheck => "path check",
        PropagationSyncStage::AppDataCheck => "announce metadata check",
        PropagationSyncStage::IdentityLoad => "identity loading",
        PropagationSyncStage::LinkEstablish => "link establishment",
        PropagationSyncStage::LinkIdentify => "link identification",
        PropagationSyncStage::CacheLoad => "cache loading",
        PropagationSyncStage::ListRequest => "message-list request",
        PropagationSyncStage::ListResponse => "message-list response",
        PropagationSyncStage::GetRequest => "message request",
        PropagationSyncStage::GetResponse => "message response",
        PropagationSyncStage::Decode => "message decoding",
        PropagationSyncStage::AckRequest => "acknowledgement request",
        PropagationSyncStage::Complete => "completion",
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum PropagationOperationError {
    #[error("propagation node is empty, contains controls, or exceeds its bound")]
    InvalidNode,
    #[error("propagation sync observation timestamp is invalid")]
    InvalidTimestamp,
    #[error(transparent)]
    Model(#[from] OperationModelError),
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    const NODE: &str = "AABBCCDDEEFF00112233445566778899";

    fn event(stage: PropagationSyncStage, status: PropagationSyncEventStatus) -> RuntimeBusEvent {
        RuntimeBusEvent::PropagationSync(PropagationSyncEvent {
            stage,
            status,
            destination_hash: Some(NODE.into()),
            detail: "private link and identity detail".into(),
            counts: BTreeMap::from([("private_message_id".into(), 99)]),
        })
    }

    fn only_record(history: &OperationHistory) -> &OperationRecord {
        let records = history.records().collect::<Vec<_>>();
        assert_eq!(records.len(), 1);
        records[0]
    }

    #[test]
    fn status_counts_are_domain_scoped_and_keep_uncertainty_orthogonal() {
        let mut history = OperationHistory::default();
        begin_propagation_sync(&mut history, 1, Some(NODE), 10).expect("queued");
        begin_propagation_sync(&mut history, 2, Some(NODE), 10).expect("reconciling");
        let mut reconciling = history
            .records()
            .find(|record| record.id == propagation_operation_id(2))
            .expect("record")
            .clone();
        reconciling.state = OperationState::Reconciling;
        reconciling.authority = EvidenceAuthority::Uncertain;
        reconciling.updated_at_unix_ms = 11;
        history.upsert(reconciling).expect("update");

        let counts = propagation_operation_counts(&history);
        assert_eq!(counts.queued, 1);
        assert_eq!(counts.in_flight, 1);
        assert_eq!(counts.uncertain, 1);
        assert_eq!(counts.settled, 0);
    }

    #[test]
    fn lifecycle_uses_app_generation_and_omits_runtime_detail_and_counts() {
        let mut history = OperationHistory::default();
        begin_propagation_sync(&mut history, 7, Some(NODE), 10).expect("begin");
        assert!(record_propagation_sync_runtime_event(
            &mut history,
            Some(7),
            &event(
                PropagationSyncStage::LinkEstablish,
                PropagationSyncEventStatus::Started,
            ),
            20,
        )
        .expect("started"));
        let record = only_record(&history);
        assert_eq!(record.id.domain, OperationDomain::PropagationSync);
        assert_eq!(record.state, OperationState::Dispatching);
        assert_eq!(record.target.label, NODE.to_ascii_lowercase());
        assert!(!record.state.claims_peer_delivery());
        assert!(!record.evidence.iter().any(|evidence| evidence
            .detail
            .as_deref()
            .is_some_and(|detail| detail.contains("private") || detail.contains("99"))));
    }

    #[test]
    fn unrelated_events_and_uncorrelated_sync_events_are_ignored() {
        let mut history = OperationHistory::default();
        begin_propagation_sync(&mut history, 7, None, 10).expect("begin");
        assert!(!record_propagation_sync_runtime_event(
            &mut history,
            None,
            &event(
                PropagationSyncStage::Complete,
                PropagationSyncEventStatus::Progress,
            ),
            20,
        )
        .expect("uncorrelated"));
        assert!(!record_propagation_sync_runtime_event(
            &mut history,
            Some(7),
            &RuntimeBusEvent::Debug("propagation sync complete".into()),
            20,
        )
        .expect("unrelated"));
        assert!(!record_propagation_sync_runtime_event(
            &mut history,
            Some(7),
            &event(
                PropagationSyncStage::Complete,
                PropagationSyncEventStatus::Progress,
            ),
            20,
        )
        .expect("ambiguous completion progress"));
        assert_eq!(only_record(&history).state, OperationState::Queued);
    }

    #[test]
    fn intermediate_complete_is_active_and_final_complete_is_local_only() {
        let mut history = OperationHistory::default();
        begin_propagation_sync(&mut history, 7, None, 10).expect("begin");
        record_propagation_sync_runtime_event(
            &mut history,
            Some(7),
            &event(
                PropagationSyncStage::ListResponse,
                PropagationSyncEventStatus::Complete,
            ),
            20,
        )
        .expect("list complete");
        assert_eq!(only_record(&history).state, OperationState::Active);
        record_propagation_sync_runtime_event(
            &mut history,
            Some(7),
            &event(
                PropagationSyncStage::Complete,
                PropagationSyncEventStatus::Complete,
            ),
            30,
        )
        .expect("sync complete");
        let record = only_record(&history);
        assert_eq!(record.state, OperationState::Completed);
        assert!(!record.state.claims_peer_delivery());
    }

    #[test]
    fn final_task_outcome_resolves_blocked_or_failed_typed_state() {
        let mut history = OperationHistory::default();
        begin_propagation_sync(&mut history, 7, None, 10).expect("begin");
        record_propagation_sync_runtime_event(
            &mut history,
            Some(7),
            &event(
                PropagationSyncStage::PathCheck,
                PropagationSyncEventStatus::Blocked,
            ),
            20,
        )
        .expect("blocked");
        assert_eq!(only_record(&history).state, OperationState::Reconciling);
        finish_propagation_sync(
            &mut history,
            7,
            PropagationSyncOutcome {
                succeeded: false,
                selected_node: Some(NODE),
                messages_received: 0,
                delivery_updates: 0,
            },
            30,
        )
        .expect("final blocked result");
        assert_eq!(only_record(&history).state, OperationState::Failed);

        finish_propagation_sync(
            &mut history,
            7,
            PropagationSyncOutcome {
                succeeded: true,
                selected_node: Some(NODE),
                messages_received: 2,
                delivery_updates: 1,
            },
            40,
        )
        .expect("authoritative final success");
        let record = only_record(&history);
        assert_eq!(record.state, OperationState::Completed);
        assert!(record.last_error.is_none());
        assert!(record.evidence.last().is_some_and(|evidence| evidence
            .detail
            .as_deref()
            .is_some_and(|detail| detail.contains("2 message") && detail.contains("1 delivery"))));

        finish_propagation_sync(
            &mut history,
            7,
            PropagationSyncOutcome {
                succeeded: true,
                selected_node: Some("bad\nnode"),
                messages_received: 2,
                delivery_updates: 1,
            },
            50,
        )
        .expect("invalid late target is omitted");
        assert_eq!(
            only_record(&history).target.label,
            NODE.to_ascii_lowercase()
        );
    }

    #[test]
    fn malformed_stale_duplicate_and_saturated_history_fail_closed() {
        let mut history = OperationHistory::default();
        assert_eq!(
            begin_propagation_sync(&mut history, 7, Some("bad\nnode"), 10),
            Err(PropagationOperationError::InvalidNode)
        );
        assert_eq!(
            begin_propagation_sync(&mut history, 7, None, -1),
            Err(PropagationOperationError::InvalidTimestamp)
        );
        begin_propagation_sync(&mut history, 7, None, 10).expect("begin");
        assert!(!begin_propagation_sync(&mut history, 7, None, 11).expect("duplicate begin"));
        let progress = event(
            PropagationSyncStage::GetRequest,
            PropagationSyncEventStatus::Progress,
        );
        record_propagation_sync_runtime_event(&mut history, Some(7), &progress, 20)
            .expect("progress");
        assert!(
            !record_propagation_sync_runtime_event(&mut history, Some(7), &progress, 20,)
                .expect("duplicate progress")
        );
        assert_eq!(
            record_propagation_sync_runtime_event(&mut history, Some(7), &progress, -1),
            Err(PropagationOperationError::InvalidTimestamp)
        );

        let mut saturated = OperationHistory::new(1, super::super::OPERATION_HISTORY_MAX_BYTES);
        begin_propagation_sync(&mut saturated, 1, None, 1).expect("first unresolved");
        assert!(matches!(
            begin_propagation_sync(&mut saturated, 2, None, 2),
            Err(PropagationOperationError::Model(
                OperationModelError::HistoryCapacity
            ))
        ));
        assert_eq!(
            saturated.records().next().expect("first retained").id,
            propagation_operation_id(1)
        );
    }

    #[test]
    fn repeated_progress_coalesces_evidence_and_terminal_event_resists_late_updates() {
        let mut history = OperationHistory::default();
        begin_propagation_sync(&mut history, 7, None, 0).expect("begin");
        for index in 1..=OPERATION_EVIDENCE_MAX_ITEMS as i64 + 2 {
            record_propagation_sync_runtime_event(
                &mut history,
                Some(7),
                &event(
                    PropagationSyncStage::GetResponse,
                    PropagationSyncEventStatus::Progress,
                ),
                index,
            )
            .expect("progress");
        }
        assert_eq!(only_record(&history).evidence.len(), 2);
        assert_eq!(
            only_record(&history)
                .evidence
                .last()
                .expect("coalesced progress")
                .at_unix_ms,
            OPERATION_EVIDENCE_MAX_ITEMS as i64 + 2
        );
        for index in 1..=OPERATION_EVIDENCE_MAX_ITEMS as i64 + 2 {
            let stage = if index % 2 == 0 {
                PropagationSyncStage::LinkEstablish
            } else {
                PropagationSyncStage::LinkIdentify
            };
            record_propagation_sync_runtime_event(
                &mut history,
                Some(7),
                &event(stage, PropagationSyncEventStatus::Started),
                30 + index,
            )
            .expect("distinct progress");
        }
        assert_eq!(
            only_record(&history).evidence.len(),
            OPERATION_EVIDENCE_MAX_ITEMS
        );
        record_propagation_sync_runtime_event(
            &mut history,
            Some(7),
            &event(
                PropagationSyncStage::Complete,
                PropagationSyncEventStatus::Complete,
            ),
            100,
        )
        .expect("complete");
        assert!(!record_propagation_sync_runtime_event(
            &mut history,
            Some(7),
            &event(
                PropagationSyncStage::Complete,
                PropagationSyncEventStatus::Progress,
            ),
            101,
        )
        .expect("late progress"));
        assert!(!record_propagation_sync_runtime_event(
            &mut history,
            Some(7),
            &event(
                PropagationSyncStage::Complete,
                PropagationSyncEventStatus::Failed,
            ),
            102,
        )
        .expect("late failure"));
        assert_eq!(only_record(&history).state, OperationState::Completed);
    }
}
