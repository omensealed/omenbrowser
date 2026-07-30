use thiserror::Error;

use crate::chat::mutation_intents::{OutboundMutationIntent, OutboundMutationState};

use super::{
    EvidenceAuthority, OperationAction, OperationDomain, OperationEvidence, OperationEvidenceKind,
    OperationId, OperationModelError, OperationRecord, OperationState, OperationTarget,
    OperationTargetKind,
};

const PREPARED_DETAIL: &str = "persisted prepared mutation; not transmitted";
const UNCERTAIN_DETAIL: &str = "persisted uncertain mutation; server outcome unknown";
const EXPIRED_DETAIL: &str = "persisted expiry reached; explicit resolution required";

pub fn recovered_mutation_record(
    intent: &OutboundMutationIntent,
    observed_at_unix_seconds: i64,
    explicit_transmission_available: bool,
) -> Result<OperationRecord, OmenChatOperationError> {
    let (state, authority, attempt_count, detail, transmission_action) = match intent.state {
        OutboundMutationState::Prepared => (
            OperationState::Waiting,
            EvidenceAuthority::Authoritative,
            0,
            PREPARED_DETAIL,
            OperationAction::ExplicitSend,
        ),
        OutboundMutationState::SentUncertain => (
            OperationState::Reconciling,
            EvidenceAuthority::Uncertain,
            1,
            UNCERTAIN_DETAIL,
            OperationAction::ExplicitSafeRetry,
        ),
        state => return Err(OmenChatOperationError::UnsupportedRecoveredState(state)),
    };
    let past_expiry = intent.expires_at <= observed_at_unix_seconds;
    let created_at_unix_ms = seconds_to_millis(intent.created_at);
    let expires_at_unix_ms = seconds_to_millis(intent.expires_at);
    let mut evidence = vec![OperationEvidence {
        kind: OperationEvidenceKind::Reconciliation,
        authority,
        at_unix_ms: created_at_unix_ms,
        detail: Some(detail.into()),
    }];
    if past_expiry {
        evidence.push(OperationEvidence {
            kind: OperationEvidenceKind::Expiration,
            authority: EvidenceAuthority::Authoritative,
            at_unix_ms: expires_at_unix_ms,
            detail: Some(EXPIRED_DETAIL.into()),
        });
    }
    let mut valid_actions = vec![OperationAction::Reconcile, OperationAction::CopyDiagnostics];
    if explicit_transmission_available && !past_expiry {
        valid_actions.insert(0, transmission_action);
    }
    let target = OperationTarget {
        kind: if intent.room_id.is_some() {
            OperationTargetKind::Room
        } else {
            OperationTargetKind::Server
        },
        label: match intent.room_id {
            Some(room_id) => format!("{} / room {room_id}", intent.server_destination),
            None => intent.server_destination.clone(),
        },
    };
    let record = OperationRecord {
        id: OperationId::opaque_128(
            OperationDomain::OmenChatMutation,
            intent.mutation_id.into_bytes(),
        ),
        target,
        state: if past_expiry {
            OperationState::Reconciling
        } else {
            state
        },
        authority: if past_expiry && authority == EvidenceAuthority::Authoritative {
            EvidenceAuthority::Stale
        } else {
            authority
        },
        evidence,
        progress: None,
        attempt_count,
        stamp_cost: None,
        propagation_node: None,
        created_at_unix_ms,
        updated_at_unix_ms: if past_expiry {
            expires_at_unix_ms.max(created_at_unix_ms)
        } else {
            created_at_unix_ms
        },
        last_error: None,
        event_cursor: None,
        valid_actions,
    };
    record.validate()?;
    Ok(record)
}

fn seconds_to_millis(seconds: i64) -> i64 {
    seconds.saturating_mul(1_000)
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum OmenChatOperationError {
    #[error("OMENchat recovered mutation has unsupported terminal state {0:?}")]
    UnsupportedRecoveredState(OutboundMutationState),
    #[error(transparent)]
    InvalidRecord(#[from] OperationModelError),
}

#[cfg(test)]
mod tests {
    use omenchat_protocol::{ChatOp, ClientInstanceId, FrameBody, MutationId, RequestHash};

    use super::*;
    use crate::operations::{OperationKey, OPERATION_TEXT_MAX_BYTES};

    fn hex_text(bytes: &[u8]) -> String {
        bytes.iter().map(|byte| format!("{byte:02x}")).collect()
    }

    fn intent(state: OutboundMutationState) -> OutboundMutationIntent {
        OutboundMutationIntent {
            server_destination: "a".repeat(32),
            authenticated_identity_hash: vec![0x11; 16],
            client_instance_id: ClientInstanceId::new([0x22; 16]),
            mutation_id: MutationId::new([0x33; 16]),
            request_hash: RequestHash::new([0x44; 32]),
            op: ChatOp::RoomMessage,
            room_id: Some(7),
            body: FrameBody::Text("private message body".into()),
            state,
            created_at: 1_000,
            expires_at: 2_000,
            correlation_id: Some("private-correlation".into()),
        }
    }

    #[test]
    fn prepared_intent_projects_as_persisted_but_not_transmitted() {
        let source = intent(OutboundMutationState::Prepared);
        let record = recovered_mutation_record(&source, 1_500, true).expect("projection");
        assert_eq!(
            record.id.key,
            OperationKey::Opaque128(source.mutation_id.into_bytes())
        );
        assert_eq!(record.state, OperationState::Waiting);
        assert_eq!(record.authority, EvidenceAuthority::Authoritative);
        assert_eq!(record.attempt_count, 0);
        assert_eq!(
            record.valid_actions,
            vec![
                OperationAction::ExplicitSend,
                OperationAction::Reconcile,
                OperationAction::CopyDiagnostics,
            ]
        );
        assert!(!record.evidence.iter().any(|evidence| matches!(
            evidence.kind,
            OperationEvidenceKind::Dispatch
                | OperationEvidenceKind::TransportAcceptance
                | OperationEvidenceKind::Receipt
                | OperationEvidenceKind::PeerDelivery
        )));
    }

    #[test]
    fn uncertain_intent_never_fabricates_transport_or_delivery_evidence() {
        let record =
            recovered_mutation_record(&intent(OutboundMutationState::SentUncertain), 1_500, true)
                .expect("projection");
        assert_eq!(record.state, OperationState::Reconciling);
        assert_eq!(record.authority, EvidenceAuthority::Uncertain);
        assert_eq!(record.attempt_count, 1);
        assert!(record
            .valid_actions
            .contains(&OperationAction::ExplicitSafeRetry));
        assert!(!record.state.is_terminal());
        assert!(!record.evidence.iter().any(|evidence| matches!(
            evidence.kind,
            OperationEvidenceKind::TransportAcceptance
                | OperationEvidenceKind::Receipt
                | OperationEvidenceKind::PeerDelivery
        )));
    }

    #[test]
    fn expired_recovery_remains_unresolved_and_cannot_be_retried() {
        for state in [
            OutboundMutationState::Prepared,
            OutboundMutationState::SentUncertain,
        ] {
            let record =
                recovered_mutation_record(&intent(state), 2_001, true).expect("projection");
            assert_eq!(record.state, OperationState::Reconciling);
            assert!(!record.state.is_terminal());
            assert_eq!(
                record
                    .evidence
                    .iter()
                    .filter(|evidence| evidence.kind == OperationEvidenceKind::Expiration)
                    .count(),
                1
            );
            assert_eq!(
                record.valid_actions,
                vec![OperationAction::Reconcile, OperationAction::CopyDiagnostics]
            );
        }
    }

    #[test]
    fn retry_guard_controls_transmission_actions_without_hiding_resolution() {
        for state in [
            OutboundMutationState::Prepared,
            OutboundMutationState::SentUncertain,
        ] {
            let record =
                recovered_mutation_record(&intent(state), 1_500, false).expect("projection");
            assert_eq!(
                record.valid_actions,
                vec![OperationAction::Reconcile, OperationAction::CopyDiagnostics]
            );
        }
    }

    #[test]
    fn projection_retains_no_message_hash_correlation_or_identity_text() {
        let source = intent(OutboundMutationState::SentUncertain);
        let record = recovered_mutation_record(&source, 1_500, true).expect("projection");
        let retained_text = std::iter::once(record.target.label.as_str())
            .chain(
                record
                    .evidence
                    .iter()
                    .filter_map(|evidence| evidence.detail.as_deref()),
            )
            .chain(record.last_error.as_deref())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(!retained_text.contains("private message body"));
        assert!(!retained_text.contains("private-correlation"));
        assert!(!retained_text.contains(&hex_text(source.request_hash.as_bytes())));
        assert!(!retained_text.contains(&hex_text(&source.authenticated_identity_hash)));
        assert!(!retained_text.contains(&hex_text(source.mutation_id.as_bytes())));
        assert!(retained_text.len() < OPERATION_TEXT_MAX_BYTES);
    }

    #[test]
    fn redraw_time_does_not_mutate_an_unchanged_projection() {
        let source = intent(OutboundMutationState::SentUncertain);
        assert_eq!(
            recovered_mutation_record(&source, 1_500, true).expect("first projection"),
            recovered_mutation_record(&source, 1_999, true).expect("later projection")
        );
    }

    #[test]
    fn terminal_intent_states_are_not_misrepresented_as_recovery_work() {
        for state in [
            OutboundMutationState::Acknowledged,
            OutboundMutationState::Conflict,
            OutboundMutationState::Expired,
            OutboundMutationState::Abandoned,
        ] {
            assert_eq!(
                recovered_mutation_record(&intent(state), 1_500, true),
                Err(OmenChatOperationError::UnsupportedRecoveredState(state))
            );
        }
    }
}
