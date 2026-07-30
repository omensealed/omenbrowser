use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::runtime::{PathEvent, RuntimeBusEvent};

use super::{
    EvidenceAuthority, OperationAction, OperationDomain, OperationEvidence, OperationEvidenceKind,
    OperationHistory, OperationId, OperationModelError, OperationRecord, OperationState,
    OperationTarget, OperationTargetKind,
};

pub const PATH_OPERATION_DESTINATION_MAX_BYTES: usize = 1024;

pub fn record_path_runtime_event(
    history: &mut OperationHistory,
    event: &RuntimeBusEvent,
    observed_at_unix_ms: i64,
) -> Result<bool, PathOperationError> {
    let RuntimeBusEvent::PathUpdated(path) = event else {
        return Ok(false);
    };
    record_path_observation(history, path, observed_at_unix_ms)
}

fn record_path_observation(
    history: &mut OperationHistory,
    path: &PathEvent,
    observed_at_unix_ms: i64,
) -> Result<bool, PathOperationError> {
    let destination = normalize_destination(&path.destination_hash)?;
    let id = path_operation_id(&destination);
    let existing = history.records().find(|record| record.id == id).cloned();
    if existing
        .as_ref()
        .is_some_and(|record| observed_at_unix_ms < record.updated_at_unix_ms)
    {
        return Ok(false);
    }
    let detail = if path.known {
        path.hops
            .map(|hops| format!("path known; {hops} hop(s)"))
            .unwrap_or_else(|| "path known; hop count unavailable".into())
    } else {
        "path unknown; hop count unavailable".into()
    };
    let state = if path.known {
        OperationState::Completed
    } else {
        OperationState::Waiting
    };
    let created_at_unix_ms = existing
        .as_ref()
        .map_or(observed_at_unix_ms, |record| record.created_at_unix_ms);
    let mut evidence = existing
        .as_ref()
        .map(|record| record.evidence.clone())
        .unwrap_or_default();
    evidence.retain(|item| item.kind != OperationEvidenceKind::PathObservation);
    evidence.push(OperationEvidence {
        kind: OperationEvidenceKind::PathObservation,
        authority: EvidenceAuthority::Authoritative,
        at_unix_ms: observed_at_unix_ms,
        detail: Some(detail),
    });
    let record = OperationRecord {
        id,
        target: OperationTarget {
            kind: OperationTargetKind::Destination,
            label: destination,
        },
        state,
        authority: EvidenceAuthority::Authoritative,
        evidence,
        progress: None,
        attempt_count: existing.as_ref().map_or(1, |record| record.attempt_count),
        stamp_cost: None,
        propagation_node: None,
        created_at_unix_ms,
        updated_at_unix_ms: observed_at_unix_ms,
        last_error: None,
        event_cursor: None,
        valid_actions: vec![OperationAction::CopyDiagnostics],
    };
    history.upsert(record)?;
    Ok(true)
}

fn normalize_destination(destination: &str) -> Result<String, PathOperationError> {
    let destination = destination.trim();
    if destination.is_empty()
        || destination.len() > PATH_OPERATION_DESTINATION_MAX_BYTES
        || destination.chars().any(char::is_control)
    {
        return Err(PathOperationError::InvalidDestination);
    }
    Ok(destination.to_ascii_lowercase())
}

fn path_operation_id(destination: &str) -> OperationId {
    let mut hasher = Sha256::new();
    hasher.update(b"omenbrowser-path-operation-v1\0");
    hasher.update(destination.as_bytes());
    let digest = hasher.finalize();
    let mut opaque = [0u8; 16];
    opaque.copy_from_slice(&digest[..16]);
    OperationId::opaque_128(OperationDomain::PathDiscovery, opaque)
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum PathOperationError {
    #[error("path destination is empty, contains controls, or exceeds its bound")]
    InvalidDestination,
    #[error(transparent)]
    Model(#[from] OperationModelError),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::operations::{OperationKey, OperationTargetKind};
    use crate::runtime::{ResourceProgressEvent, RuntimeBusEvent};

    fn path(destination_hash: &str, known: bool, hops: Option<u32>) -> RuntimeBusEvent {
        RuntimeBusEvent::PathUpdated(PathEvent {
            destination_hash: destination_hash.into(),
            known,
            hops,
        })
    }

    fn only_record(history: &OperationHistory) -> &OperationRecord {
        let records = history.records().collect::<Vec<_>>();
        assert_eq!(records.len(), 1);
        records[0]
    }

    #[test]
    fn path_identity_is_normalized_stable_opaque_and_bounded() {
        let mut history = OperationHistory::default();
        record_path_runtime_event(
            &mut history,
            &path(" AABBCCDDEEFF00112233445566778899 ", false, None),
            10,
        )
        .expect("path observation");
        let first = only_record(&history).clone();
        assert_eq!(first.target.label, "aabbccddeeff00112233445566778899");
        assert_eq!(first.target.kind, OperationTargetKind::Destination);
        assert!(matches!(first.id.key, OperationKey::Opaque128(_)));
        record_path_runtime_event(
            &mut history,
            &path("aabbccddeeff00112233445566778899", true, Some(2)),
            20,
        )
        .expect("same normalized destination");
        assert_eq!(history.metrics().items, 1);
        assert_eq!(only_record(&history).id, first.id);

        for invalid in [
            String::new(),
            "line\nbreak".into(),
            "x".repeat(PATH_OPERATION_DESTINATION_MAX_BYTES + 1),
        ] {
            assert_eq!(
                record_path_runtime_event(
                    &mut OperationHistory::default(),
                    &path(&invalid, false, None),
                    10,
                ),
                Err(PathOperationError::InvalidDestination)
            );
        }
    }

    #[test]
    fn unknown_path_is_waiting_authoritative_and_not_failed() {
        let mut history = OperationHistory::default();
        assert!(
            record_path_runtime_event(&mut history, &path("destination", false, Some(99)), 10,)
                .expect("unknown observation")
        );

        let record = only_record(&history);
        assert_eq!(record.state, OperationState::Waiting);
        assert_eq!(record.authority, EvidenceAuthority::Authoritative);
        assert!(!record.state.is_terminal());
        assert!(record.last_error.is_none());
        assert_eq!(record.valid_actions, vec![OperationAction::CopyDiagnostics]);
        assert_eq!(
            record.evidence[0].detail.as_deref(),
            Some("path unknown; hop count unavailable")
        );
    }

    #[test]
    fn known_path_is_locally_completed_without_delivery_claim() {
        let mut history = OperationHistory::default();
        record_path_runtime_event(&mut history, &path("destination", true, Some(3)), 10)
            .expect("known observation");

        let record = only_record(&history);
        assert_eq!(record.state, OperationState::Completed);
        assert!(record.state.is_terminal());
        assert!(!record.state.claims_peer_delivery());
        assert_eq!(
            record.evidence[0].kind,
            OperationEvidenceKind::PathObservation
        );
        assert_eq!(
            record.evidence[0].detail.as_deref(),
            Some("path known; 3 hop(s)")
        );
        assert!(!record
            .evidence
            .iter()
            .any(|evidence| evidence.kind == OperationEvidenceKind::PeerDelivery));
    }

    #[test]
    fn path_observations_coalesce_and_can_reopen_after_route_loss() {
        let mut history = OperationHistory::default();
        record_path_runtime_event(&mut history, &path("destination", false, None), 10)
            .expect("unknown");
        record_path_runtime_event(&mut history, &path("destination", true, Some(1)), 20)
            .expect("known");
        record_path_runtime_event(&mut history, &path("destination", false, None), 30)
            .expect("route lost");

        let record = only_record(&history);
        assert_eq!(record.state, OperationState::Waiting);
        assert_eq!(record.created_at_unix_ms, 10);
        assert_eq!(record.updated_at_unix_ms, 30);
        assert_eq!(record.evidence.len(), 1);
        assert_eq!(
            record.evidence[0].detail.as_deref(),
            Some("path unknown; hop count unavailable")
        );
    }

    #[test]
    fn stale_observation_and_unrelated_runtime_event_do_not_mutate_history() {
        let mut history = OperationHistory::default();
        record_path_runtime_event(&mut history, &path("destination", true, Some(1)), 20)
            .expect("known");
        assert!(
            !record_path_runtime_event(&mut history, &path("destination", false, None), 19,)
                .expect("stale ignored")
        );
        assert!(!record_path_runtime_event(
            &mut history,
            &RuntimeBusEvent::ResourceProgress(ResourceProgressEvent {
                transfer_id: "resource".into(),
                received: 1,
                total: Some(2),
                operation_id: None,
                source: None,
                purpose: None,
                direction: None,
                peer: None,
            }),
            30,
        )
        .expect("unrelated ignored"));
        assert_eq!(only_record(&history).state, OperationState::Completed);
        assert_eq!(only_record(&history).updated_at_unix_ms, 20);
    }

    #[test]
    fn saturated_history_rejects_without_dropping_unresolved_work() {
        let existing = OperationRecord {
            id: OperationId::numeric(OperationDomain::OmenChatConnection, 1),
            target: OperationTarget {
                kind: OperationTargetKind::Server,
                label: "existing connection".into(),
            },
            state: OperationState::Active,
            authority: EvidenceAuthority::Authoritative,
            evidence: Vec::new(),
            progress: None,
            attempt_count: 1,
            stamp_cost: None,
            propagation_node: None,
            created_at_unix_ms: 1,
            updated_at_unix_ms: 1,
            last_error: None,
            event_cursor: None,
            valid_actions: Vec::new(),
        };
        let existing_bytes = existing.validate().expect("existing size");
        let mut history = OperationHistory::new(1, existing_bytes);
        history.upsert(existing.clone()).expect("existing record");

        assert_eq!(
            record_path_runtime_event(&mut history, &path("destination", false, None), 10,),
            Err(PathOperationError::Model(
                OperationModelError::HistoryCapacity
            ))
        );
        assert_eq!(history.metrics().items, 1);
        assert_eq!(history.metrics().rejected, 1);
        assert_eq!(history.metrics().bytes, existing_bytes);
        assert_eq!(history.records().next(), Some(&existing));
    }
}
