use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::runtime::{
    ResourceLifecycleEvent, ResourceLifecycleState, ResourceProgressEvent, RuntimeBusEvent,
};

use super::{
    AuthoritativeProgress, EvidenceAuthority, OperationAction, OperationDomain, OperationEvidence,
    OperationEvidenceKind, OperationHistory, OperationId, OperationModelError, OperationRecord,
    OperationState, OperationTarget, OperationTargetKind,
};

pub const RESOURCE_OPERATION_TRANSFER_ID_MAX_BYTES: usize = 1024;

pub fn record_resource_runtime_event(
    history: &mut OperationHistory,
    event: &RuntimeBusEvent,
    observed_at_unix_ms: i64,
) -> Result<bool, ResourceOperationError> {
    match event {
        RuntimeBusEvent::ResourceProgress(progress) => {
            record_progress(history, progress, observed_at_unix_ms)
        }
        RuntimeBusEvent::ResourceLifecycle(lifecycle) => {
            record_lifecycle(history, lifecycle, observed_at_unix_ms)
        }
        _ => Ok(false),
    }
}

fn record_progress(
    history: &mut OperationHistory,
    progress: &ResourceProgressEvent,
    observed_at_unix_ms: i64,
) -> Result<bool, ResourceOperationError> {
    let id = resource_operation_id(&progress.transfer_id)?;
    let existing = existing_record(history, id);
    if existing
        .as_ref()
        .is_some_and(|record| record.state.is_terminal())
    {
        return Ok(false);
    }
    if existing
        .as_ref()
        .and_then(|record| record.progress)
        .is_some_and(|previous| progress.received < previous.completed_bytes)
    {
        return Ok(false);
    }
    let authoritative_progress = match progress.total {
        Some(total) => Some(AuthoritativeProgress::new(progress.received, total)?),
        None => existing
            .as_ref()
            .and_then(|record| record.progress)
            .map(|previous| AuthoritativeProgress::new(progress.received, previous.total_bytes))
            .transpose()?,
    };
    let detail = match authoritative_progress {
        Some(progress) => format!(
            "{} of {} byte(s) observed",
            progress.completed_bytes, progress.total_bytes
        ),
        None => format!("{} byte(s) observed; total unavailable", progress.received),
    };
    let record = updated_record(
        existing,
        id,
        ResourceRecordUpdate {
            target: target_from_metadata(
                progress.source.as_deref(),
                progress.purpose.as_deref(),
                progress.direction.as_deref(),
                progress.peer.as_deref(),
            ),
            state: OperationState::Transferring,
            evidence_kind: OperationEvidenceKind::ResourceProgress,
            detail,
            progress: authoritative_progress,
            last_error: None,
            observed_at_unix_ms,
        },
    );
    history.upsert(record)?;
    Ok(true)
}

fn record_lifecycle(
    history: &mut OperationHistory,
    lifecycle: &ResourceLifecycleEvent,
    observed_at_unix_ms: i64,
) -> Result<bool, ResourceOperationError> {
    let id = resource_operation_id(&lifecycle.transfer_id)?;
    let existing = existing_record(history, id);
    if existing
        .as_ref()
        .is_some_and(|record| record.state.is_terminal())
    {
        return Ok(false);
    }
    if lifecycle.state == ResourceLifecycleState::Offered
        && existing
            .as_ref()
            .is_some_and(|record| record.state != OperationState::Waiting)
    {
        return Ok(false);
    }
    let (state, evidence_kind, detail, last_error, progress) = match lifecycle.state {
        ResourceLifecycleState::Offered => (
            OperationState::Waiting,
            OperationEvidenceKind::ResourceOffer,
            lifecycle
                .bytes
                .map(|bytes| format!("resource offered; {bytes} byte(s) declared"))
                .unwrap_or_else(|| "resource offered; size unavailable".into()),
            None,
            None,
        ),
        ResourceLifecycleState::Complete => (
            OperationState::Completed,
            OperationEvidenceKind::ResourceCompletion,
            lifecycle
                .bytes
                .map(|bytes| format!("resource completed; {bytes} byte(s) observed"))
                .unwrap_or_else(|| "resource completed; byte count unavailable".into()),
            None,
            lifecycle
                .bytes
                .filter(|bytes| *bytes > 0)
                .map(|bytes| AuthoritativeProgress::new(bytes, bytes))
                .transpose()?,
        ),
        ResourceLifecycleState::Failed => {
            let reason = lifecycle
                .reason
                .clone()
                .unwrap_or_else(|| "resource transfer failed without detail".into());
            (
                OperationState::Failed,
                OperationEvidenceKind::Failure,
                reason.clone(),
                Some(reason),
                existing.as_ref().and_then(|record| record.progress),
            )
        }
        ResourceLifecycleState::Cancelled => {
            let reason = lifecycle
                .reason
                .clone()
                .unwrap_or_else(|| "resource transfer cancelled without detail".into());
            (
                OperationState::Cancelled,
                OperationEvidenceKind::Cancellation,
                reason.clone(),
                Some(reason),
                existing.as_ref().and_then(|record| record.progress),
            )
        }
    };
    let record = updated_record(
        existing,
        id,
        ResourceRecordUpdate {
            target: target_from_metadata(
                lifecycle.source.as_deref(),
                lifecycle.purpose.as_deref(),
                lifecycle.direction.as_deref(),
                lifecycle.peer.as_deref(),
            ),
            state,
            evidence_kind,
            detail,
            progress,
            last_error,
            observed_at_unix_ms,
        },
    );
    history.upsert(record)?;
    Ok(true)
}

fn existing_record(history: &OperationHistory, id: OperationId) -> Option<OperationRecord> {
    history.records().find(|record| record.id == id).cloned()
}

struct ResourceRecordUpdate {
    target: Option<OperationTarget>,
    state: OperationState,
    evidence_kind: OperationEvidenceKind,
    detail: String,
    progress: Option<AuthoritativeProgress>,
    last_error: Option<String>,
    observed_at_unix_ms: i64,
}

fn updated_record(
    existing: Option<OperationRecord>,
    id: OperationId,
    update: ResourceRecordUpdate,
) -> OperationRecord {
    let ResourceRecordUpdate {
        target,
        state,
        evidence_kind,
        detail,
        progress,
        last_error,
        observed_at_unix_ms,
    } = update;
    let created_at_unix_ms = existing
        .as_ref()
        .map_or(observed_at_unix_ms, |record| record.created_at_unix_ms);
    let updated_at_unix_ms = existing.as_ref().map_or(observed_at_unix_ms, |record| {
        observed_at_unix_ms.max(record.updated_at_unix_ms)
    });
    let target = target
        .or_else(|| existing.as_ref().map(|record| record.target.clone()))
        .unwrap_or_else(|| OperationTarget {
            kind: OperationTargetKind::Destination,
            label: "resource transfer".into(),
        });
    let mut evidence = existing
        .as_ref()
        .map(|record| record.evidence.clone())
        .unwrap_or_default();
    evidence.retain(|item| item.kind != evidence_kind);
    evidence.push(OperationEvidence {
        kind: evidence_kind,
        authority: EvidenceAuthority::Authoritative,
        at_unix_ms: updated_at_unix_ms,
        detail: Some(detail),
    });

    OperationRecord {
        id,
        target,
        state,
        authority: EvidenceAuthority::Authoritative,
        evidence,
        progress,
        attempt_count: existing.as_ref().map_or(1, |record| record.attempt_count),
        stamp_cost: None,
        propagation_node: None,
        created_at_unix_ms,
        updated_at_unix_ms,
        last_error,
        event_cursor: None,
        valid_actions: vec![OperationAction::CopyDiagnostics],
    }
}

fn target_from_metadata(
    source: Option<&str>,
    purpose: Option<&str>,
    direction: Option<&str>,
    peer: Option<&str>,
) -> Option<OperationTarget> {
    let mut parts = Vec::new();
    for (name, value) in [
        ("source", source),
        ("purpose", purpose),
        ("direction", direction),
        ("peer", peer),
    ] {
        if let Some(value) = value.filter(|value| !value.is_empty()) {
            parts.push(format!("{name}={value}"));
        }
    }
    (!parts.is_empty()).then(|| OperationTarget {
        kind: if peer.is_some() {
            OperationTargetKind::Peer
        } else {
            OperationTargetKind::Destination
        },
        label: parts.join(" | "),
    })
}

fn resource_operation_id(transfer_id: &str) -> Result<OperationId, ResourceOperationError> {
    if transfer_id.is_empty() || transfer_id.len() > RESOURCE_OPERATION_TRANSFER_ID_MAX_BYTES {
        return Err(ResourceOperationError::InvalidTransferId);
    }
    let mut hasher = Sha256::new();
    hasher.update(b"omenbrowser-resource-operation-v1\0");
    hasher.update(transfer_id.as_bytes());
    let digest = hasher.finalize();
    let mut opaque = [0u8; 16];
    opaque.copy_from_slice(&digest[..16]);
    Ok(OperationId::opaque_128(
        OperationDomain::ResourceTransfer,
        opaque,
    ))
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ResourceOperationError {
    #[error("resource transfer identifier is empty or exceeds its bound")]
    InvalidTransferId,
    #[error(transparent)]
    Model(#[from] OperationModelError),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::operations::{OperationKey, OperationTargetKind};

    fn progress(transfer_id: &str, received: u64, total: Option<u64>) -> RuntimeBusEvent {
        RuntimeBusEvent::ResourceProgress(ResourceProgressEvent {
            transfer_id: transfer_id.into(),
            received,
            total,
            operation_id: Some("browser-operation-not-retained".into()),
            source: Some("nomadnet-page".into()),
            purpose: Some("page-response".into()),
            direction: Some("inbound".into()),
            peer: Some("peer-visible".into()),
        })
    }

    fn lifecycle(
        transfer_id: &str,
        state: ResourceLifecycleState,
        bytes: Option<u64>,
        reason: Option<&str>,
    ) -> RuntimeBusEvent {
        RuntimeBusEvent::ResourceLifecycle(ResourceLifecycleEvent {
            transfer_id: transfer_id.into(),
            state,
            bytes,
            reason: reason.map(str::to_owned),
            operation_id: Some("browser-operation-not-retained".into()),
            source: Some("nomadnet-page".into()),
            purpose: Some("page-response".into()),
            direction: Some("inbound".into()),
            peer: Some("peer-visible".into()),
        })
    }

    fn only_record(history: &OperationHistory) -> &OperationRecord {
        let records = history.records().collect::<Vec<_>>();
        assert_eq!(records.len(), 1);
        records[0]
    }

    #[test]
    fn resource_identity_is_stable_opaque_and_bounded() {
        let id = resource_operation_id("transfer-secret").expect("resource id");
        assert_eq!(
            id,
            resource_operation_id("transfer-secret").expect("same resource id")
        );
        assert_ne!(
            id,
            resource_operation_id("different-transfer").expect("different resource id")
        );
        assert!(matches!(id.key, OperationKey::Opaque128(_)));
        assert_eq!(id.domain, OperationDomain::ResourceTransfer);
        assert_eq!(
            resource_operation_id(""),
            Err(ResourceOperationError::InvalidTransferId)
        );
        assert_eq!(
            resource_operation_id(&"x".repeat(RESOURCE_OPERATION_TRANSFER_ID_MAX_BYTES + 1)),
            Err(ResourceOperationError::InvalidTransferId)
        );
    }

    #[test]
    fn offer_and_progress_coalesce_without_retaining_private_correlation() {
        let mut history = OperationHistory::default();
        assert!(record_resource_runtime_event(
            &mut history,
            &lifecycle(
                "transfer-secret",
                ResourceLifecycleState::Offered,
                Some(12),
                None
            ),
            10,
        )
        .expect("offer"));
        assert!(record_resource_runtime_event(
            &mut history,
            &progress("transfer-secret", 3, Some(12)),
            20,
        )
        .expect("progress"));
        assert!(record_resource_runtime_event(
            &mut history,
            &progress("transfer-secret", 6, Some(12)),
            30,
        )
        .expect("coalesced progress"));

        let record = only_record(&history);
        assert_eq!(record.state, OperationState::Transferring);
        assert_eq!(record.authority, EvidenceAuthority::Authoritative);
        assert_eq!(
            record.progress,
            Some(AuthoritativeProgress {
                completed_bytes: 6,
                total_bytes: 12,
            })
        );
        assert_eq!(record.target.kind, OperationTargetKind::Peer);
        assert!(record.target.label.contains("source=nomadnet-page"));
        assert!(record.target.label.contains("peer=peer-visible"));
        assert_eq!(
            record
                .evidence
                .iter()
                .filter(|evidence| evidence.kind == OperationEvidenceKind::ResourceProgress)
                .count(),
            1
        );
        let retained = serde_json::to_string(record).expect("serialize retained record");
        assert!(!retained.contains("transfer-secret"));
        assert!(!retained.contains("browser-operation-not-retained"));
    }

    #[test]
    fn unknown_total_uses_prior_authoritative_total_without_guessing() {
        let mut history = OperationHistory::default();
        record_resource_runtime_event(&mut history, &progress("transfer", 3, Some(12)), 10)
            .expect("known total");
        record_resource_runtime_event(&mut history, &progress("transfer", 6, None), 20)
            .expect("retained total");

        assert_eq!(
            only_record(&history).progress,
            Some(AuthoritativeProgress {
                completed_bytes: 6,
                total_bytes: 12,
            })
        );
    }

    #[test]
    fn resource_completion_is_terminal_but_never_peer_delivery() {
        let mut history = OperationHistory::default();
        record_resource_runtime_event(&mut history, &progress("transfer", 3, Some(7)), 10)
            .expect("progress");
        assert!(record_resource_runtime_event(
            &mut history,
            &lifecycle("transfer", ResourceLifecycleState::Complete, Some(7), None,),
            20,
        )
        .expect("complete"));

        let record = only_record(&history);
        assert_eq!(record.state, OperationState::Completed);
        assert!(record.state.is_terminal());
        assert!(!record.state.claims_peer_delivery());
        assert!(record.evidence.iter().any(|evidence| {
            evidence.kind == OperationEvidenceKind::ResourceCompletion
                && evidence.authority == EvidenceAuthority::Authoritative
        }));
        assert!(!record
            .evidence
            .iter()
            .any(|evidence| evidence.kind == OperationEvidenceKind::PeerDelivery));
        assert!(!record_resource_runtime_event(
            &mut history,
            &progress("transfer", 7, Some(7)),
            30,
        )
        .expect("late progress is ignored"));
        assert_eq!(only_record(&history).updated_at_unix_ms, 20);
    }

    #[test]
    fn failed_and_cancelled_resources_are_distinct_terminal_outcomes() {
        for (lifecycle_state, operation_state, evidence_kind) in [
            (
                ResourceLifecycleState::Failed,
                OperationState::Failed,
                OperationEvidenceKind::Failure,
            ),
            (
                ResourceLifecycleState::Cancelled,
                OperationState::Cancelled,
                OperationEvidenceKind::Cancellation,
            ),
        ] {
            let mut history = OperationHistory::default();
            record_resource_runtime_event(
                &mut history,
                &lifecycle("transfer", lifecycle_state, None, Some("bounded reason")),
                10,
            )
            .expect("terminal");
            let record = only_record(&history);
            assert_eq!(record.state, operation_state);
            assert_eq!(record.last_error.as_deref(), Some("bounded reason"));
            assert!(record
                .evidence
                .iter()
                .any(|evidence| evidence.kind == evidence_kind));
        }
    }

    #[test]
    fn regressive_or_invalid_progress_preserves_the_last_valid_record() {
        let mut history = OperationHistory::default();
        record_resource_runtime_event(&mut history, &progress("transfer", 6, Some(12)), 20)
            .expect("initial progress");
        assert!(!record_resource_runtime_event(
            &mut history,
            &progress("transfer", 5, Some(12)),
            30,
        )
        .expect("regression ignored"));
        assert_eq!(only_record(&history).updated_at_unix_ms, 20);
        assert_eq!(
            record_resource_runtime_event(&mut history, &progress("transfer", 13, Some(12)), 40,),
            Err(ResourceOperationError::Model(
                OperationModelError::InvalidProgress
            ))
        );
        assert_eq!(
            only_record(&history).progress,
            Some(AuthoritativeProgress {
                completed_bytes: 6,
                total_bytes: 12,
            })
        );
    }

    #[test]
    fn saturated_history_rejects_without_dropping_unresolved_work() {
        let existing = OperationRecord {
            id: OperationId::numeric(OperationDomain::PathDiscovery, 1),
            target: OperationTarget {
                kind: OperationTargetKind::Destination,
                label: "existing path".into(),
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
            record_resource_runtime_event(&mut history, &progress("transfer", 1, Some(2)), 10,),
            Err(ResourceOperationError::Model(
                OperationModelError::HistoryCapacity
            ))
        );
        assert_eq!(history.metrics().items, 1);
        assert_eq!(history.metrics().rejected, 1);
        assert_eq!(history.metrics().bytes, existing_bytes);
        assert_eq!(history.records().next(), Some(&existing));
    }
}
