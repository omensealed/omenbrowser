use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::runtime::{LxmfDeliveryEvidence, LxmfDeliveryEvidenceKind};
use crate::runtime::{RuntimeBusEvent, RuntimeLxmfDeliveryState, RuntimeLxmfDeliveryUpdate};

use super::{
    EvidenceAuthority, OperationAction, OperationDomain, OperationEvidence, OperationEvidenceKind,
    OperationHistory, OperationId, OperationModelError, OperationRecord, OperationState,
    OperationTarget, OperationTargetKind, OPERATION_EVIDENCE_MAX_ITEMS, OPERATION_TEXT_MAX_BYTES,
};

const LXMF_REASON_CODE_MAX_BYTES: usize = 512;
const UNKNOWN_PEER_TARGET: &str = "peer unavailable";

#[cfg(test)]
fn record_lxmf_runtime_event(
    history: &mut OperationHistory,
    event: &RuntimeBusEvent,
) -> Result<bool, LxmfOperationError> {
    record_lxmf_runtime_event_at(history, event, 0)
}

pub fn record_lxmf_runtime_event_at(
    history: &mut OperationHistory,
    event: &RuntimeBusEvent,
    observed_at_unix_ms: i64,
) -> Result<bool, LxmfOperationError> {
    match event {
        RuntimeBusEvent::SdkDeliveryUpdated(update) => record_sdk_delivery_update(history, update),
        RuntimeBusEvent::LxmfDeliveryEvidence(evidence) => {
            record_native_delivery_evidence(history, evidence, observed_at_unix_ms)
        }
        _ => Ok(false),
    }
}

fn record_sdk_delivery_update(
    history: &mut OperationHistory,
    update: &RuntimeLxmfDeliveryUpdate,
) -> Result<bool, LxmfOperationError> {
    validate_terminal_semantics(update)?;
    let message_id = validate_identifier(&update.message_id)?;
    let id = lxmf_operation_id(message_id);
    let existing = history.records().find(|record| record.id == id).cloned();
    let updated_at_unix_ms =
        i64::try_from(update.last_updated_ms).map_err(|_| LxmfOperationError::InvalidTimestamp)?;
    if existing.as_ref().is_some_and(|record| {
        updated_at_unix_ms < record.updated_at_unix_ms
            || (updated_at_unix_ms == record.updated_at_unix_ms
                && record
                    .event_cursor
                    .is_some_and(|cursor| update.seq_no <= cursor))
            || (record.state.is_terminal() && !update.state.is_terminal())
    }) {
        return Ok(false);
    }
    let target = match update.peer_hash.as_deref() {
        Some(peer) => normalize_peer(peer)?,
        None => existing
            .as_ref()
            .map(|record| record.target.label.clone())
            .unwrap_or_else(|| UNKNOWN_PEER_TARGET.into()),
    };
    let (state, authority, evidence_kind, detail) = state_projection(update);
    let created_at_unix_ms = existing
        .as_ref()
        .map_or(updated_at_unix_ms, |record| record.created_at_unix_ms);
    let mut evidence = existing
        .as_ref()
        .map(|record| record.evidence.clone())
        .unwrap_or_default();
    if evidence.len() >= OPERATION_EVIDENCE_MAX_ITEMS {
        evidence.remove(0);
    }
    evidence.push(OperationEvidence {
        kind: evidence_kind,
        authority,
        at_unix_ms: updated_at_unix_ms,
        detail: Some(detail.into()),
    });
    let last_error = if update.state.is_failure_terminal() {
        bounded_reason_code(update.reason_code.as_deref())
    } else {
        None
    };
    let record = OperationRecord {
        id,
        target: OperationTarget {
            kind: OperationTargetKind::Peer,
            label: target,
        },
        state,
        authority,
        evidence,
        progress: None,
        attempt_count: update.attempts,
        stamp_cost: None,
        propagation_node: None,
        created_at_unix_ms,
        updated_at_unix_ms,
        last_error,
        event_cursor: Some(update.seq_no),
        valid_actions: vec![OperationAction::CopyDiagnostics],
    };
    history.upsert(record)?;
    Ok(true)
}

fn record_native_delivery_evidence(
    history: &mut OperationHistory,
    evidence: &LxmfDeliveryEvidence,
    fallback_observed_at_unix_ms: i64,
) -> Result<bool, LxmfOperationError> {
    let Some(message_id) = evidence.message_id.as_deref() else {
        return Ok(false);
    };
    let message_id = validate_identifier(message_id)?;
    let id = lxmf_operation_id(message_id);
    let existing = history.records().find(|record| record.id == id).cloned();
    let updated_at_unix_ms =
        native_evidence_timestamp(evidence.observed_at, fallback_observed_at_unix_ms)?;
    let (projected_state, authority, evidence_kind, detail) =
        native_evidence_projection(evidence.kind);
    if existing.as_ref().is_some_and(|record| {
        updated_at_unix_ms < record.updated_at_unix_ms
            || (record.state.is_terminal() && projected_state != OperationState::Delivered)
            || record.evidence.last().is_some_and(|last| {
                last.at_unix_ms == updated_at_unix_ms && last.kind == evidence_kind
            })
    }) {
        return Ok(false);
    }
    let target = normalize_peer(&evidence.peer_hash)?;
    let created_at_unix_ms = existing
        .as_ref()
        .map_or(updated_at_unix_ms, |record| record.created_at_unix_ms);
    let mut retained_evidence = existing
        .as_ref()
        .map(|record| record.evidence.clone())
        .unwrap_or_default();
    if retained_evidence.len() >= OPERATION_EVIDENCE_MAX_ITEMS {
        retained_evidence.remove(0);
    }
    retained_evidence.push(OperationEvidence {
        kind: evidence_kind,
        authority,
        at_unix_ms: updated_at_unix_ms,
        detail: Some(detail.into()),
    });
    let state = existing
        .as_ref()
        .map(|record| preserve_stronger_nonterminal_state(record.state, projected_state))
        .unwrap_or(projected_state);
    let last_error = if matches!(
        evidence.kind,
        LxmfDeliveryEvidenceKind::PropagationNodeFailed
            | LxmfDeliveryEvidenceKind::LxmfRouterFailed
    ) {
        Some(detail.into())
    } else {
        None
    };
    let record = OperationRecord {
        id,
        target: OperationTarget {
            kind: OperationTargetKind::Peer,
            label: target,
        },
        state,
        authority,
        evidence: retained_evidence,
        progress: None,
        attempt_count: existing.as_ref().map_or(0, |record| record.attempt_count),
        stamp_cost: None,
        propagation_node: None,
        created_at_unix_ms,
        updated_at_unix_ms,
        last_error,
        event_cursor: existing.as_ref().and_then(|record| record.event_cursor),
        valid_actions: vec![OperationAction::CopyDiagnostics],
    };
    history.upsert(record)?;
    Ok(true)
}

fn native_evidence_timestamp(
    observed_at: Option<f64>,
    fallback_observed_at_unix_ms: i64,
) -> Result<i64, LxmfOperationError> {
    let Some(observed_at) = observed_at else {
        return (fallback_observed_at_unix_ms >= 0)
            .then_some(fallback_observed_at_unix_ms)
            .ok_or(LxmfOperationError::InvalidTimestamp);
    };
    let millis = observed_at * 1_000.0;
    if !millis.is_finite() || millis < 0.0 || millis > i64::MAX as f64 {
        return Err(LxmfOperationError::InvalidTimestamp);
    }
    Ok(millis.round() as i64)
}

fn native_evidence_projection(
    kind: LxmfDeliveryEvidenceKind,
) -> (
    OperationState,
    EvidenceAuthority,
    OperationEvidenceKind,
    &'static str,
) {
    match kind {
        LxmfDeliveryEvidenceKind::PacketSubmitted => (
            OperationState::TransportAccepted,
            EvidenceAuthority::Authoritative,
            OperationEvidenceKind::TransportAcceptance,
            "packet submitted to Reticulum transport; peer delivery unconfirmed",
        ),
        LxmfDeliveryEvidenceKind::RnsPacketProof => (
            OperationState::ReceiptObserved,
            EvidenceAuthority::Authoritative,
            OperationEvidenceKind::Receipt,
            "RNS packet proof observed; peer delivery unconfirmed",
        ),
        LxmfDeliveryEvidenceKind::PropagationNodeAccepted => (
            OperationState::TransportAccepted,
            EvidenceAuthority::Authoritative,
            OperationEvidenceKind::TransportAcceptance,
            "propagation node accepted the payload; peer delivery unconfirmed",
        ),
        LxmfDeliveryEvidenceKind::PropagationNodeFailed => (
            OperationState::Failed,
            EvidenceAuthority::Authoritative,
            OperationEvidenceKind::Failure,
            "propagation node transfer failed",
        ),
        LxmfDeliveryEvidenceKind::PropagationSyncNoPayloads => (
            OperationState::Reconciling,
            EvidenceAuthority::Uncertain,
            OperationEvidenceKind::Reconciliation,
            "propagation sync returned no peer payload; delivery remains unconfirmed",
        ),
        LxmfDeliveryEvidenceKind::LxmfRouterDelivered => (
            OperationState::Delivered,
            EvidenceAuthority::Authoritative,
            OperationEvidenceKind::PeerDelivery,
            "LXMF router reports delivery",
        ),
        LxmfDeliveryEvidenceKind::LxmfRouterFailed => (
            OperationState::Failed,
            EvidenceAuthority::Authoritative,
            OperationEvidenceKind::Failure,
            "LXMF router reports delivery failure",
        ),
        LxmfDeliveryEvidenceKind::InboundPeerMessage => (
            OperationState::Reconciling,
            EvidenceAuthority::Inferred,
            OperationEvidenceKind::Reconciliation,
            "peer activity observed after send; this message's delivery is not established",
        ),
        LxmfDeliveryEvidenceKind::NoReceiptObserved => (
            OperationState::Reconciling,
            EvidenceAuthority::Uncertain,
            OperationEvidenceKind::Reconciliation,
            "no LXMF receipt observed; delivery remains unconfirmed",
        ),
    }
}

fn preserve_stronger_nonterminal_state(
    current: OperationState,
    projected: OperationState,
) -> OperationState {
    if projected == OperationState::Reconciling || projected.is_terminal() {
        return projected;
    }
    let rank = |state| match state {
        OperationState::Queued => 1,
        OperationState::Dispatching | OperationState::Active => 2,
        OperationState::TransportAccepted => 3,
        OperationState::ReceiptObserved => 4,
        _ => 0,
    };
    if rank(current) > rank(projected) {
        current
    } else {
        projected
    }
}

fn validate_terminal_semantics(
    update: &RuntimeLxmfDeliveryUpdate,
) -> Result<(), LxmfOperationError> {
    let terminal_consistent = match update.state {
        RuntimeLxmfDeliveryState::Sent => true,
        RuntimeLxmfDeliveryState::Delivered
        | RuntimeLxmfDeliveryState::Failed
        | RuntimeLxmfDeliveryState::Cancelled
        | RuntimeLxmfDeliveryState::Expired
        | RuntimeLxmfDeliveryState::Rejected => update.terminal,
        RuntimeLxmfDeliveryState::Queued
        | RuntimeLxmfDeliveryState::Dispatching
        | RuntimeLxmfDeliveryState::InFlight
        | RuntimeLxmfDeliveryState::Unknown => !update.terminal,
    };
    if terminal_consistent {
        Ok(())
    } else {
        Err(LxmfOperationError::InconsistentTerminalState)
    }
}

fn state_projection(
    update: &RuntimeLxmfDeliveryUpdate,
) -> (
    OperationState,
    EvidenceAuthority,
    OperationEvidenceKind,
    &'static str,
) {
    match update.state {
        RuntimeLxmfDeliveryState::Queued => (
            OperationState::Queued,
            EvidenceAuthority::Authoritative,
            OperationEvidenceKind::QueueAdmission,
            "LXMF SDK queued the message",
        ),
        RuntimeLxmfDeliveryState::Dispatching => (
            OperationState::Dispatching,
            EvidenceAuthority::Authoritative,
            OperationEvidenceKind::Dispatch,
            "LXMF SDK is dispatching the message",
        ),
        RuntimeLxmfDeliveryState::InFlight => (
            OperationState::Dispatching,
            EvidenceAuthority::Authoritative,
            OperationEvidenceKind::Dispatch,
            "LXMF SDK reports the message in flight",
        ),
        RuntimeLxmfDeliveryState::Sent if update.terminal => (
            OperationState::Completed,
            EvidenceAuthority::Authoritative,
            OperationEvidenceKind::TransportAcceptance,
            "LXMF SDK terminal sent state; peer delivery not established",
        ),
        RuntimeLxmfDeliveryState::Sent => (
            OperationState::TransportAccepted,
            EvidenceAuthority::Authoritative,
            OperationEvidenceKind::TransportAcceptance,
            "LXMF SDK sent the message; peer delivery pending",
        ),
        RuntimeLxmfDeliveryState::Delivered => (
            OperationState::Delivered,
            EvidenceAuthority::Authoritative,
            OperationEvidenceKind::PeerDelivery,
            "LXMF SDK reports peer delivery",
        ),
        RuntimeLxmfDeliveryState::Failed => (
            OperationState::Failed,
            EvidenceAuthority::Authoritative,
            OperationEvidenceKind::Failure,
            "LXMF SDK reports delivery failure",
        ),
        RuntimeLxmfDeliveryState::Cancelled => (
            OperationState::Cancelled,
            EvidenceAuthority::Authoritative,
            OperationEvidenceKind::Cancellation,
            "LXMF SDK reports cancellation",
        ),
        RuntimeLxmfDeliveryState::Expired => (
            OperationState::Expired,
            EvidenceAuthority::Authoritative,
            OperationEvidenceKind::Expiration,
            "LXMF SDK reports expiry",
        ),
        RuntimeLxmfDeliveryState::Rejected => (
            OperationState::Rejected,
            EvidenceAuthority::Authoritative,
            OperationEvidenceKind::Rejection,
            "LXMF SDK reports rejection",
        ),
        RuntimeLxmfDeliveryState::Unknown => (
            OperationState::Reconciling,
            EvidenceAuthority::Uncertain,
            OperationEvidenceKind::Reconciliation,
            "LXMF SDK delivery state is unknown",
        ),
    }
}

fn validate_identifier(identifier: &str) -> Result<&str, LxmfOperationError> {
    let identifier = identifier.trim();
    if identifier.is_empty()
        || identifier.len() > OPERATION_TEXT_MAX_BYTES
        || identifier.chars().any(char::is_control)
    {
        return Err(LxmfOperationError::InvalidMessageId);
    }
    Ok(identifier)
}

fn normalize_peer(peer: &str) -> Result<String, LxmfOperationError> {
    let peer = peer.trim();
    if peer.is_empty()
        || peer.len() > OPERATION_TEXT_MAX_BYTES
        || peer.chars().any(char::is_control)
    {
        return Err(LxmfOperationError::InvalidPeer);
    }
    Ok(peer.to_ascii_lowercase())
}

fn bounded_reason_code(reason: Option<&str>) -> Option<String> {
    let reason = reason?.trim();
    if reason.is_empty() {
        None
    } else if reason.len() <= LXMF_REASON_CODE_MAX_BYTES && !reason.chars().any(char::is_control) {
        Some(format!("reason code: {reason}"))
    } else {
        Some("reason code omitted: invalid or over bound".into())
    }
}

fn lxmf_operation_id(message_id: &str) -> OperationId {
    let mut hasher = Sha256::new();
    hasher.update(b"omenbrowser-lxmf-operation-v1\0");
    hasher.update(message_id.as_bytes());
    let digest = hasher.finalize();
    let mut opaque = [0u8; 16];
    opaque.copy_from_slice(&digest[..16]);
    OperationId::opaque_128(OperationDomain::LxmfMessage, opaque)
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum LxmfOperationError {
    #[error("LXMF message identifier is empty, contains controls, or exceeds its bound")]
    InvalidMessageId,
    #[error("LXMF peer target is empty, contains controls, or exceeds its bound")]
    InvalidPeer,
    #[error("LXMF delivery update has inconsistent state and terminal flag")]
    InconsistentTerminalState,
    #[error("LXMF delivery update timestamp exceeds the supported range")]
    InvalidTimestamp,
    #[error(transparent)]
    Model(#[from] OperationModelError),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::operations::{OperationKey, OperationTargetKind};

    fn update(
        state: RuntimeLxmfDeliveryState,
        terminal: bool,
        last_updated_ms: u64,
        seq_no: u64,
    ) -> RuntimeBusEvent {
        RuntimeBusEvent::SdkDeliveryUpdated(RuntimeLxmfDeliveryUpdate {
            message_id: "private-message-id".into(),
            peer_hash: Some(" AABBCCDDEEFF00112233445566778899 ".into()),
            previous_state: None,
            state,
            terminal,
            attempts: 2,
            reason_code: None,
            last_updated_ms,
            event_id: "private-event-id".into(),
            seq_no,
            cursor: "private-cursor".into(),
        })
    }

    fn native_evidence(
        kind: LxmfDeliveryEvidenceKind,
        message_id: Option<&str>,
        observed_at: Option<f64>,
    ) -> RuntimeBusEvent {
        RuntimeBusEvent::LxmfDeliveryEvidence(LxmfDeliveryEvidence {
            peer_hash: "AABBCCDDEEFF00112233445566778899".into(),
            message_id: message_id.map(str::to_string),
            kind,
            detail: Some("private_packet:secret;private_link:secret;private_failure:secret".into()),
            rtt: Some(0.25),
            observed_at,
        })
    }

    fn only_record(history: &OperationHistory) -> &OperationRecord {
        let records = history.records().collect::<Vec<_>>();
        assert_eq!(records.len(), 1);
        records[0]
    }

    #[test]
    fn typed_sdk_states_preserve_delivery_boundaries() {
        let cases = [
            (
                RuntimeLxmfDeliveryState::Queued,
                false,
                OperationState::Queued,
                OperationEvidenceKind::QueueAdmission,
            ),
            (
                RuntimeLxmfDeliveryState::Dispatching,
                false,
                OperationState::Dispatching,
                OperationEvidenceKind::Dispatch,
            ),
            (
                RuntimeLxmfDeliveryState::InFlight,
                false,
                OperationState::Dispatching,
                OperationEvidenceKind::Dispatch,
            ),
            (
                RuntimeLxmfDeliveryState::Sent,
                false,
                OperationState::TransportAccepted,
                OperationEvidenceKind::TransportAcceptance,
            ),
            (
                RuntimeLxmfDeliveryState::Sent,
                true,
                OperationState::Completed,
                OperationEvidenceKind::TransportAcceptance,
            ),
            (
                RuntimeLxmfDeliveryState::Delivered,
                true,
                OperationState::Delivered,
                OperationEvidenceKind::PeerDelivery,
            ),
            (
                RuntimeLxmfDeliveryState::Failed,
                true,
                OperationState::Failed,
                OperationEvidenceKind::Failure,
            ),
            (
                RuntimeLxmfDeliveryState::Cancelled,
                true,
                OperationState::Cancelled,
                OperationEvidenceKind::Cancellation,
            ),
            (
                RuntimeLxmfDeliveryState::Expired,
                true,
                OperationState::Expired,
                OperationEvidenceKind::Expiration,
            ),
            (
                RuntimeLxmfDeliveryState::Rejected,
                true,
                OperationState::Rejected,
                OperationEvidenceKind::Rejection,
            ),
            (
                RuntimeLxmfDeliveryState::Unknown,
                false,
                OperationState::Reconciling,
                OperationEvidenceKind::Reconciliation,
            ),
        ];
        for (state, terminal, operation_state, evidence_kind) in cases {
            let mut history = OperationHistory::default();
            record_lxmf_runtime_event(&mut history, &update(state, terminal, 10, 1))
                .expect("typed update");
            let record = only_record(&history);
            assert_eq!(record.state, operation_state);
            assert_eq!(record.evidence[0].kind, evidence_kind);
            assert_eq!(record.target.kind, OperationTargetKind::Peer);
            assert_eq!(record.target.label, "aabbccddeeff00112233445566778899");
            assert_eq!(record.attempt_count, 2);
            assert_eq!(record.event_cursor, Some(1));
            assert_eq!(
                record.authority,
                if state == RuntimeLxmfDeliveryState::Unknown {
                    EvidenceAuthority::Uncertain
                } else {
                    EvidenceAuthority::Authoritative
                }
            );
            assert_eq!(
                record.state.claims_peer_delivery(),
                state == RuntimeLxmfDeliveryState::Delivered
            );
        }
    }

    #[test]
    fn message_identity_is_opaque_and_private_event_fields_are_not_retained() {
        let mut history = OperationHistory::default();
        record_lxmf_runtime_event(
            &mut history,
            &update(RuntimeLxmfDeliveryState::Queued, false, 10, 1),
        )
        .expect("queued");
        let record = only_record(&history);
        assert!(matches!(record.id.key, OperationKey::Opaque128(_)));
        let retained = std::iter::once(record.target.label.as_str())
            .chain(
                record
                    .evidence
                    .iter()
                    .filter_map(|evidence| evidence.detail.as_deref()),
            )
            .collect::<Vec<_>>()
            .join("\n");
        assert!(!retained.contains("private-message-id"));
        assert!(!retained.contains("private-event-id"));
        assert!(!retained.contains("private-cursor"));
    }

    #[test]
    fn sent_terminal_is_completed_without_peer_delivery_claim() {
        let mut history = OperationHistory::default();
        record_lxmf_runtime_event(
            &mut history,
            &update(RuntimeLxmfDeliveryState::Sent, true, 10, 1),
        )
        .expect("terminal sent");
        let record = only_record(&history);
        assert_eq!(record.state, OperationState::Completed);
        assert!(record.state.is_terminal());
        assert!(!record.state.claims_peer_delivery());
        assert!(!record
            .evidence
            .iter()
            .any(|evidence| evidence.kind == OperationEvidenceKind::PeerDelivery));
    }

    #[test]
    fn transitions_coalesce_and_terminal_state_resists_late_regression() {
        let mut history = OperationHistory::default();
        record_lxmf_runtime_event(
            &mut history,
            &update(RuntimeLxmfDeliveryState::Queued, false, 10, 1),
        )
        .expect("queued");
        record_lxmf_runtime_event(
            &mut history,
            &update(RuntimeLxmfDeliveryState::Delivered, true, 20, 2),
        )
        .expect("delivered");
        assert!(!record_lxmf_runtime_event(
            &mut history,
            &update(RuntimeLxmfDeliveryState::Dispatching, false, 30, 3),
        )
        .expect("terminal regression ignored"));
        assert!(!record_lxmf_runtime_event(
            &mut history,
            &update(RuntimeLxmfDeliveryState::Queued, false, 20, 2),
        )
        .expect("duplicate ignored"));
        let record = only_record(&history);
        assert_eq!(record.state, OperationState::Delivered);
        assert_eq!(record.evidence.len(), 2);
        assert_eq!(record.event_cursor, Some(2));
    }

    #[test]
    fn inconsistent_terminality_and_invalid_identifiers_are_rejected() {
        for (state, terminal) in [
            (RuntimeLxmfDeliveryState::Queued, true),
            (RuntimeLxmfDeliveryState::Delivered, false),
            (RuntimeLxmfDeliveryState::Failed, false),
        ] {
            assert_eq!(
                record_lxmf_runtime_event(
                    &mut OperationHistory::default(),
                    &update(state, terminal, 10, 1),
                ),
                Err(LxmfOperationError::InconsistentTerminalState)
            );
        }
        let mut invalid_event = update(RuntimeLxmfDeliveryState::Queued, false, 10, 1);
        let RuntimeBusEvent::SdkDeliveryUpdated(ref mut invalid_message) = invalid_event else {
            unreachable!()
        };
        invalid_message.message_id = "line\nbreak".into();
        assert_eq!(
            record_lxmf_runtime_event(&mut OperationHistory::default(), &invalid_event),
            Err(LxmfOperationError::InvalidMessageId)
        );
    }

    #[test]
    fn missing_peer_preserves_known_target_and_failure_reason_is_bounded() {
        let mut history = OperationHistory::default();
        record_lxmf_runtime_event(
            &mut history,
            &update(RuntimeLxmfDeliveryState::Queued, false, 10, 1),
        )
        .expect("queued");
        let mut failed = match update(RuntimeLxmfDeliveryState::Failed, true, 20, 2) {
            RuntimeBusEvent::SdkDeliveryUpdated(update) => update,
            _ => unreachable!(),
        };
        failed.peer_hash = None;
        failed.reason_code = Some("x".repeat(LXMF_REASON_CODE_MAX_BYTES + 1));
        record_lxmf_runtime_event(&mut history, &RuntimeBusEvent::SdkDeliveryUpdated(failed))
            .expect("failed");
        let record = only_record(&history);
        assert_eq!(record.target.label, "aabbccddeeff00112233445566778899");
        assert_eq!(
            record.last_error.as_deref(),
            Some("reason code omitted: invalid or over bound")
        );
    }

    #[test]
    fn evidence_history_and_operation_history_remain_bounded() {
        let mut history = OperationHistory::default();
        for seq in 1..=(OPERATION_EVIDENCE_MAX_ITEMS as u64 + 5) {
            record_lxmf_runtime_event(
                &mut history,
                &update(
                    if seq % 2 == 0 {
                        RuntimeLxmfDeliveryState::Dispatching
                    } else {
                        RuntimeLxmfDeliveryState::Queued
                    },
                    false,
                    seq,
                    seq,
                ),
            )
            .expect("transition");
        }
        assert_eq!(
            only_record(&history).evidence.len(),
            OPERATION_EVIDENCE_MAX_ITEMS
        );

        let existing = OperationRecord {
            id: OperationId::numeric(OperationDomain::PathDiscovery, 1),
            target: OperationTarget {
                kind: OperationTargetKind::Destination,
                label: "existing path".into(),
            },
            state: OperationState::Waiting,
            authority: EvidenceAuthority::Authoritative,
            evidence: Vec::new(),
            progress: None,
            attempt_count: 0,
            stamp_cost: None,
            propagation_node: None,
            created_at_unix_ms: 1,
            updated_at_unix_ms: 1,
            last_error: None,
            event_cursor: None,
            valid_actions: Vec::new(),
        };
        let existing_bytes = existing.validate().expect("existing size");
        let mut saturated = OperationHistory::new(1, existing_bytes);
        saturated.upsert(existing.clone()).expect("existing");
        assert_eq!(
            record_lxmf_runtime_event(
                &mut saturated,
                &update(RuntimeLxmfDeliveryState::Queued, false, 10, 1),
            ),
            Err(LxmfOperationError::Model(
                OperationModelError::HistoryCapacity
            ))
        );
        assert_eq!(saturated.records().next(), Some(&existing));
        assert_eq!(saturated.metrics().rejected, 1);
    }

    #[test]
    fn native_packet_and_proof_evidence_correlate_without_claiming_peer_delivery() {
        let mut history = OperationHistory::default();
        record_lxmf_runtime_event_at(
            &mut history,
            &native_evidence(
                LxmfDeliveryEvidenceKind::PacketSubmitted,
                Some("private-message-id"),
                Some(0.010),
            ),
            999,
        )
        .expect("packet submitted");
        record_lxmf_runtime_event_at(
            &mut history,
            &native_evidence(
                LxmfDeliveryEvidenceKind::RnsPacketProof,
                Some("private-message-id"),
                Some(0.020),
            ),
            999,
        )
        .expect("RNS proof");

        let record = only_record(&history);
        assert_eq!(record.state, OperationState::ReceiptObserved);
        assert!(!record.state.claims_peer_delivery());
        assert_eq!(record.evidence.len(), 2);
        assert_eq!(
            record.evidence.last().map(|evidence| evidence.kind),
            Some(OperationEvidenceKind::Receipt)
        );
        let retained = record
            .evidence
            .iter()
            .filter_map(|evidence| evidence.detail.as_deref())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(!retained.contains("private_packet"));
        assert!(!retained.contains("private_link"));
        assert!(!retained.contains("private_failure"));
    }

    #[test]
    fn propagation_acceptance_is_not_delivery_and_only_router_delivery_claims_it() {
        let mut history = OperationHistory::default();
        record_lxmf_runtime_event_at(
            &mut history,
            &native_evidence(
                LxmfDeliveryEvidenceKind::PropagationNodeAccepted,
                Some("private-message-id"),
                Some(0.010),
            ),
            999,
        )
        .expect("propagation accepted");
        assert_eq!(
            only_record(&history).state,
            OperationState::TransportAccepted
        );
        assert!(!only_record(&history).state.claims_peer_delivery());

        record_lxmf_runtime_event_at(
            &mut history,
            &native_evidence(
                LxmfDeliveryEvidenceKind::LxmfRouterDelivered,
                Some("private-message-id"),
                Some(0.020),
            ),
            999,
        )
        .expect("router delivered");
        let record = only_record(&history);
        assert_eq!(record.state, OperationState::Delivered);
        assert!(record.state.claims_peer_delivery());
        assert_eq!(
            record.evidence.last().map(|evidence| evidence.kind),
            Some(OperationEvidenceKind::PeerDelivery)
        );
    }

    #[test]
    fn uncertain_native_evidence_stays_unresolved_and_requires_exact_message_id() {
        let mut history = OperationHistory::default();
        assert!(!record_lxmf_runtime_event_at(
            &mut history,
            &native_evidence(
                LxmfDeliveryEvidenceKind::InboundPeerMessage,
                None,
                Some(0.010),
            ),
            999,
        )
        .expect("uncorrelated peer activity omitted"));
        for (kind, authority) in [
            (
                LxmfDeliveryEvidenceKind::InboundPeerMessage,
                EvidenceAuthority::Inferred,
            ),
            (
                LxmfDeliveryEvidenceKind::NoReceiptObserved,
                EvidenceAuthority::Uncertain,
            ),
            (
                LxmfDeliveryEvidenceKind::PropagationSyncNoPayloads,
                EvidenceAuthority::Uncertain,
            ),
        ] {
            let mut history = OperationHistory::default();
            record_lxmf_runtime_event_at(
                &mut history,
                &native_evidence(kind, Some("private-message-id"), Some(0.010)),
                999,
            )
            .expect("uncertain evidence");
            let record = only_record(&history);
            assert_eq!(record.state, OperationState::Reconciling);
            assert_eq!(record.authority, authority);
            assert!(!record.state.claims_peer_delivery());
        }
    }

    #[test]
    fn native_evidence_obeys_timestamps_terminal_precedence_and_fallback_time() {
        let mut history = OperationHistory::default();
        record_lxmf_runtime_event_at(
            &mut history,
            &native_evidence(
                LxmfDeliveryEvidenceKind::LxmfRouterDelivered,
                Some("private-message-id"),
                None,
            ),
            20,
        )
        .expect("delivered with fallback time");
        assert_eq!(only_record(&history).updated_at_unix_ms, 20);
        assert!(!record_lxmf_runtime_event_at(
            &mut history,
            &native_evidence(
                LxmfDeliveryEvidenceKind::LxmfRouterFailed,
                Some("private-message-id"),
                Some(0.030),
            ),
            30,
        )
        .expect("terminal failure cannot replace delivery"));
        assert!(!record_lxmf_runtime_event_at(
            &mut history,
            &native_evidence(
                LxmfDeliveryEvidenceKind::RnsPacketProof,
                Some("private-message-id"),
                Some(0.010),
            ),
            10,
        )
        .expect("stale proof ignored"));
        assert_eq!(only_record(&history).state, OperationState::Delivered);

        let mut resolved_failure = OperationHistory::default();
        record_lxmf_runtime_event_at(
            &mut resolved_failure,
            &native_evidence(
                LxmfDeliveryEvidenceKind::LxmfRouterFailed,
                Some("private-message-id"),
                Some(0.010),
            ),
            10,
        )
        .expect("router failure");
        record_lxmf_runtime_event_at(
            &mut resolved_failure,
            &native_evidence(
                LxmfDeliveryEvidenceKind::LxmfRouterDelivered,
                Some("private-message-id"),
                Some(0.020),
            ),
            20,
        )
        .expect("later delivery");
        assert_eq!(
            only_record(&resolved_failure).state,
            OperationState::Delivered
        );
        assert!(only_record(&resolved_failure).last_error.is_none());

        let mut invalid = match native_evidence(
            LxmfDeliveryEvidenceKind::PacketSubmitted,
            Some("another-message"),
            Some(f64::NAN),
        ) {
            RuntimeBusEvent::LxmfDeliveryEvidence(evidence) => evidence,
            _ => unreachable!(),
        };
        assert_eq!(
            record_lxmf_runtime_event_at(
                &mut history,
                &RuntimeBusEvent::LxmfDeliveryEvidence(invalid.clone()),
                40,
            ),
            Err(LxmfOperationError::InvalidTimestamp)
        );
        invalid.observed_at = None;
        assert_eq!(
            record_lxmf_runtime_event_at(
                &mut history,
                &RuntimeBusEvent::LxmfDeliveryEvidence(invalid),
                -1,
            ),
            Err(LxmfOperationError::InvalidTimestamp)
        );
    }
}
