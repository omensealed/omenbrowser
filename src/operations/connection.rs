use thiserror::Error;

use crate::chat::{ChatConnectionState, ChatSessionId};

use super::{
    EvidenceAuthority, OperationAction, OperationDomain, OperationEvidence, OperationEvidenceKind,
    OperationHistory, OperationId, OperationModelError, OperationRecord, OperationState,
    OperationTarget, OperationTargetKind, OPERATION_TEXT_MAX_BYTES,
};

pub fn record_omenchat_connection_state(
    history: &mut OperationHistory,
    session_id: ChatSessionId,
    server_destination: &str,
    state: ChatConnectionState,
    observed_at_unix_ms: i64,
) -> Result<bool, ConnectionOperationError> {
    let target = normalize_target(server_destination)?;
    let id = OperationId::numeric(OperationDomain::OmenChatConnection, session_id);
    let existing = history.records().find(|record| record.id == id).cloned();
    if existing
        .as_ref()
        .is_some_and(|record| observed_at_unix_ms < record.updated_at_unix_ms)
    {
        return Ok(false);
    }
    let (operation_state, evidence_kind, detail) = state_projection(state);
    let created_at_unix_ms = existing
        .as_ref()
        .map_or(observed_at_unix_ms, |record| record.created_at_unix_ms);
    let record = OperationRecord {
        id,
        target: OperationTarget {
            kind: OperationTargetKind::Server,
            label: target,
        },
        state: operation_state,
        authority: EvidenceAuthority::Authoritative,
        evidence: vec![OperationEvidence {
            kind: evidence_kind,
            authority: EvidenceAuthority::Authoritative,
            at_unix_ms: observed_at_unix_ms,
            detail: Some(detail.into()),
        }],
        progress: None,
        attempt_count: existing.as_ref().map_or(0, |record| record.attempt_count),
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

pub fn remove_omenchat_connection(
    history: &mut OperationHistory,
    session_id: ChatSessionId,
) -> Option<OperationRecord> {
    history.remove(OperationId::numeric(
        OperationDomain::OmenChatConnection,
        session_id,
    ))
}

fn normalize_target(target: &str) -> Result<String, ConnectionOperationError> {
    let target = target.trim();
    if target.is_empty()
        || target.len() > OPERATION_TEXT_MAX_BYTES
        || target.chars().any(char::is_control)
    {
        return Err(ConnectionOperationError::InvalidTarget);
    }
    Ok(target.to_ascii_lowercase())
}

fn state_projection(
    state: ChatConnectionState,
) -> (OperationState, OperationEvidenceKind, &'static str) {
    match state {
        ChatConnectionState::Disconnected => (
            OperationState::Waiting,
            OperationEvidenceKind::ConnectionState,
            "OMENchat connection disconnected",
        ),
        ChatConnectionState::Resolving => (
            OperationState::Waiting,
            OperationEvidenceKind::ConnectionState,
            "OMENchat connection resolving path",
        ),
        ChatConnectionState::Connecting => (
            OperationState::Active,
            OperationEvidenceKind::ConnectionState,
            "OMENchat connection opening link",
        ),
        ChatConnectionState::Authenticating => (
            OperationState::Active,
            OperationEvidenceKind::ConnectionState,
            "OMENchat connection authenticating",
        ),
        ChatConnectionState::Joined => (
            OperationState::Active,
            OperationEvidenceKind::ConnectionState,
            "OMENchat connection joined",
        ),
        ChatConnectionState::Reconnecting => (
            OperationState::Reconciling,
            OperationEvidenceKind::ConnectionState,
            "OMENchat connection reconnecting",
        ),
        ChatConnectionState::Draining => (
            OperationState::Active,
            OperationEvidenceKind::ConnectionState,
            "OMENchat connection draining",
        ),
        ChatConnectionState::Failed { retryable: true } => (
            OperationState::Failed,
            OperationEvidenceKind::Failure,
            "OMENchat connection failed; retry available",
        ),
        ChatConnectionState::Failed { retryable: false } => (
            OperationState::Failed,
            OperationEvidenceKind::Failure,
            "OMENchat connection failed; retry unavailable",
        ),
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ConnectionOperationError {
    #[error("OMENchat connection target is empty, contains controls, or exceeds its bound")]
    InvalidTarget,
    #[error(transparent)]
    Model(#[from] OperationModelError),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::operations::{OperationKey, OperationTargetKind};

    fn only_record(history: &OperationHistory) -> &OperationRecord {
        let records = history.records().collect::<Vec<_>>();
        assert_eq!(records.len(), 1);
        records[0]
    }

    #[test]
    fn typed_connection_states_map_without_transport_or_delivery_claims() {
        let cases = [
            (ChatConnectionState::Disconnected, OperationState::Waiting),
            (ChatConnectionState::Resolving, OperationState::Waiting),
            (ChatConnectionState::Connecting, OperationState::Active),
            (ChatConnectionState::Authenticating, OperationState::Active),
            (ChatConnectionState::Joined, OperationState::Active),
            (
                ChatConnectionState::Reconnecting,
                OperationState::Reconciling,
            ),
            (ChatConnectionState::Draining, OperationState::Active),
            (
                ChatConnectionState::Failed { retryable: true },
                OperationState::Failed,
            ),
            (
                ChatConnectionState::Failed { retryable: false },
                OperationState::Failed,
            ),
        ];
        for (index, (connection_state, expected_state)) in cases.into_iter().enumerate() {
            let mut history = OperationHistory::default();
            record_omenchat_connection_state(
                &mut history,
                7,
                " AABBCCDDEEFF00112233445566778899 ",
                connection_state,
                index as i64,
            )
            .expect("connection projection");
            let record = only_record(&history);
            assert_eq!(record.state, expected_state);
            assert_eq!(record.authority, EvidenceAuthority::Authoritative);
            assert_eq!(record.target.kind, OperationTargetKind::Server);
            assert_eq!(record.target.label, "aabbccddeeff00112233445566778899");
            assert_eq!(record.id.key, OperationKey::Numeric(7));
            assert!(!record.state.claims_peer_delivery());
            assert!(!record.evidence.iter().any(|evidence| matches!(
                evidence.kind,
                OperationEvidenceKind::TransportAcceptance
                    | OperationEvidenceKind::Receipt
                    | OperationEvidenceKind::PeerDelivery
            )));
        }
    }

    #[test]
    fn connection_transitions_coalesce_and_stale_state_is_ignored() {
        let mut history = OperationHistory::default();
        record_omenchat_connection_state(
            &mut history,
            9,
            "destination",
            ChatConnectionState::Connecting,
            10,
        )
        .expect("connecting");
        record_omenchat_connection_state(
            &mut history,
            9,
            "destination",
            ChatConnectionState::Joined,
            20,
        )
        .expect("joined");
        assert!(!record_omenchat_connection_state(
            &mut history,
            9,
            "destination",
            ChatConnectionState::Disconnected,
            19,
        )
        .expect("stale ignored"));

        let record = only_record(&history);
        assert_eq!(record.state, OperationState::Active);
        assert_eq!(record.created_at_unix_ms, 10);
        assert_eq!(record.updated_at_unix_ms, 20);
        assert_eq!(record.evidence.len(), 1);
        assert_eq!(
            record.evidence[0].detail.as_deref(),
            Some("OMENchat connection joined")
        );
    }

    #[test]
    fn invalid_target_rejects_without_retaining_private_or_unbounded_text() {
        for invalid in [
            String::new(),
            "line\nbreak".into(),
            "x".repeat(OPERATION_TEXT_MAX_BYTES + 1),
        ] {
            assert_eq!(
                record_omenchat_connection_state(
                    &mut OperationHistory::default(),
                    1,
                    &invalid,
                    ChatConnectionState::Connecting,
                    10,
                ),
                Err(ConnectionOperationError::InvalidTarget)
            );
        }
    }

    #[test]
    fn explicit_session_removal_releases_the_connection_record() {
        let mut history = OperationHistory::default();
        record_omenchat_connection_state(
            &mut history,
            11,
            "destination",
            ChatConnectionState::Joined,
            10,
        )
        .expect("joined");
        assert!(history.metrics().bytes > 0);

        let removed = remove_omenchat_connection(&mut history, 11).expect("removed");
        assert_eq!(removed.id.domain, OperationDomain::OmenChatConnection);
        assert_eq!(history.metrics().items, 0);
        assert_eq!(history.metrics().bytes, 0);
    }

    #[test]
    fn saturated_history_preserves_existing_unresolved_work() {
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
        let mut history = OperationHistory::new(1, existing_bytes);
        history.upsert(existing.clone()).expect("existing record");

        assert_eq!(
            record_omenchat_connection_state(
                &mut history,
                1,
                "destination",
                ChatConnectionState::Connecting,
                10,
            ),
            Err(ConnectionOperationError::Model(
                OperationModelError::HistoryCapacity
            ))
        );
        assert_eq!(history.metrics().items, 1);
        assert_eq!(history.metrics().rejected, 1);
        assert_eq!(history.records().next(), Some(&existing));
    }
}
