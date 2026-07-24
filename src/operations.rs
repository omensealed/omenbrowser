use std::collections::{BTreeSet, VecDeque};

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const OPERATION_HISTORY_MAX_ITEMS: usize = 512;
pub const OPERATION_HISTORY_MAX_BYTES: usize = 512 * 1024;
pub const OPERATION_RECORD_MAX_BYTES: usize = 8 * 1024;
pub const OPERATION_EVIDENCE_MAX_ITEMS: usize = 16;
pub const OPERATION_TEXT_MAX_BYTES: usize = 1024;
pub const OPERATION_ACTION_MAX_ITEMS: usize = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum OperationDomain {
    PathDiscovery,
    LinkEstablishment,
    LxmfMessage,
    ResourceTransfer,
    OmenChatConnection,
    OmenChatMutation,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct OperationId {
    pub domain: OperationDomain,
    pub local_id: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum OperationTargetKind {
    Destination,
    Peer,
    Server,
    Room,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperationTarget {
    pub kind: OperationTargetKind,
    pub label: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EvidenceAuthority {
    Authoritative,
    Inferred,
    Stale,
    Uncertain,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum OperationState {
    Waiting,
    Queued,
    Dispatching,
    TransportAccepted,
    ReceiptObserved,
    Delivered,
    Transferring,
    Active,
    Reconciling,
    EventGap,
    Cancelled,
    Failed,
    Expired,
    Rejected,
}

impl OperationState {
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Delivered | Self::Cancelled | Self::Failed | Self::Expired | Self::Rejected
        )
    }

    pub fn claims_peer_delivery(self) -> bool {
        self == Self::Delivered
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum OperationEvidenceKind {
    QueueAdmission,
    Dispatch,
    TransportAcceptance,
    Receipt,
    PeerDelivery,
    ResourceProgress,
    Cancellation,
    Failure,
    Expiration,
    Rejection,
    EventGap,
    Reconciliation,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperationEvidence {
    pub kind: OperationEvidenceKind,
    pub authority: EvidenceAuthority,
    pub at_unix_ms: i64,
    pub detail: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthoritativeProgress {
    pub completed_bytes: u64,
    pub total_bytes: u64,
}

impl AuthoritativeProgress {
    pub fn new(completed_bytes: u64, total_bytes: u64) -> Result<Self, OperationModelError> {
        if total_bytes == 0 || completed_bytes > total_bytes {
            return Err(OperationModelError::InvalidProgress);
        }
        Ok(Self {
            completed_bytes,
            total_bytes,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum OperationAction {
    Cancel,
    Reconcile,
    ExplicitSafeRetry,
    CopyDiagnostics,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperationRecord {
    pub id: OperationId,
    pub target: OperationTarget,
    pub state: OperationState,
    pub authority: EvidenceAuthority,
    pub evidence: Vec<OperationEvidence>,
    pub progress: Option<AuthoritativeProgress>,
    pub attempt_count: u32,
    pub stamp_cost: Option<u16>,
    pub propagation_node: Option<String>,
    pub created_at_unix_ms: i64,
    pub updated_at_unix_ms: i64,
    pub last_error: Option<String>,
    pub event_cursor: Option<u64>,
    pub valid_actions: Vec<OperationAction>,
}

impl OperationRecord {
    pub fn validate(&self) -> Result<usize, OperationModelError> {
        if self.target.label.is_empty()
            || self.target.label.len() > OPERATION_TEXT_MAX_BYTES
            || self
                .propagation_node
                .as_ref()
                .is_some_and(|value| value.len() > OPERATION_TEXT_MAX_BYTES)
            || self
                .last_error
                .as_ref()
                .is_some_and(|value| value.len() > OPERATION_TEXT_MAX_BYTES)
        {
            return Err(OperationModelError::TextLimit);
        }
        if self.evidence.len() > OPERATION_EVIDENCE_MAX_ITEMS
            || self
                .evidence
                .iter()
                .filter_map(|evidence| evidence.detail.as_ref())
                .any(|detail| detail.len() > OPERATION_TEXT_MAX_BYTES)
        {
            return Err(OperationModelError::EvidenceLimit);
        }
        if self.valid_actions.len() > OPERATION_ACTION_MAX_ITEMS
            || self
                .valid_actions
                .iter()
                .copied()
                .collect::<BTreeSet<_>>()
                .len()
                != self.valid_actions.len()
        {
            return Err(OperationModelError::ActionLimit);
        }
        if self.updated_at_unix_ms < self.created_at_unix_ms {
            return Err(OperationModelError::InvalidTimestamp);
        }
        if self.progress.is_some_and(|progress| {
            progress.total_bytes == 0 || progress.completed_bytes > progress.total_bytes
        }) {
            return Err(OperationModelError::InvalidProgress);
        }
        if self.progress.is_some()
            && !matches!(
                self.id.domain,
                OperationDomain::ResourceTransfer | OperationDomain::LxmfMessage
            )
        {
            return Err(OperationModelError::UnsupportedProgress);
        }
        if self.state == OperationState::Delivered
            && !self.evidence.iter().any(|evidence| {
                evidence.kind == OperationEvidenceKind::PeerDelivery
                    && evidence.authority == EvidenceAuthority::Authoritative
            })
        {
            return Err(OperationModelError::MissingDeliveryEvidence);
        }
        let bytes = operation_record_bytes(self)?;
        if bytes > OPERATION_RECORD_MAX_BYTES {
            return Err(OperationModelError::RecordByteLimit);
        }
        Ok(bytes)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct OperationHistoryMetrics {
    pub items: usize,
    pub bytes: usize,
    pub rejected: u64,
    pub evicted_terminal: u64,
}

#[derive(Debug)]
pub struct OperationHistory {
    records: VecDeque<OperationRecord>,
    retained_bytes: usize,
    rejected: u64,
    evicted_terminal: u64,
    max_items: usize,
    max_bytes: usize,
}

impl Default for OperationHistory {
    fn default() -> Self {
        Self::new(OPERATION_HISTORY_MAX_ITEMS, OPERATION_HISTORY_MAX_BYTES)
    }
}

impl OperationHistory {
    pub fn new(max_items: usize, max_bytes: usize) -> Self {
        Self {
            records: VecDeque::new(),
            retained_bytes: 0,
            rejected: 0,
            evicted_terminal: 0,
            max_items: max_items.max(1),
            max_bytes: max_bytes.max(1),
        }
    }

    pub fn records(&self) -> impl DoubleEndedIterator<Item = &OperationRecord> {
        self.records.iter()
    }

    pub fn metrics(&self) -> OperationHistoryMetrics {
        OperationHistoryMetrics {
            items: self.records.len(),
            bytes: self.retained_bytes,
            rejected: self.rejected,
            evicted_terminal: self.evicted_terminal,
        }
    }

    pub fn upsert(&mut self, record: OperationRecord) -> Result<(), OperationModelError> {
        let new_bytes = match record.validate() {
            Ok(bytes) => bytes,
            Err(error) => {
                self.rejected = self.rejected.saturating_add(1);
                return Err(error);
            }
        };
        let existing_bytes = self
            .records
            .iter()
            .find(|existing| existing.id == record.id)
            .map(operation_record_bytes)
            .transpose()?
            .unwrap_or(0);
        let mut projected_items = self
            .records
            .len()
            .saturating_sub(usize::from(existing_bytes > 0))
            .saturating_add(1);
        let mut projected_bytes = self
            .retained_bytes
            .saturating_sub(existing_bytes)
            .saturating_add(new_bytes);
        let mut evict = BTreeSet::new();
        for existing in &self.records {
            if projected_items <= self.max_items && projected_bytes <= self.max_bytes {
                break;
            }
            if existing.id != record.id && existing.state.is_terminal() {
                projected_items = projected_items.saturating_sub(1);
                projected_bytes = projected_bytes.saturating_sub(operation_record_bytes(existing)?);
                evict.insert(existing.id);
            }
        }
        if projected_items > self.max_items || projected_bytes > self.max_bytes {
            self.rejected = self.rejected.saturating_add(1);
            return Err(OperationModelError::HistoryCapacity);
        }
        let evicted = evict.len() as u64;
        self.records
            .retain(|existing| existing.id != record.id && !evict.contains(&existing.id));
        self.records.push_back(record);
        self.retained_bytes = projected_bytes;
        self.evicted_terminal = self.evicted_terminal.saturating_add(evicted);
        Ok(())
    }

    pub fn expire_terminal_before(&mut self, cutoff_unix_ms: i64, max_remove: usize) -> usize {
        if max_remove == 0 {
            return 0;
        }
        let mut removed = 0usize;
        let mut removed_bytes = 0usize;
        self.records.retain(|record| {
            if removed < max_remove
                && record.state.is_terminal()
                && record.updated_at_unix_ms < cutoff_unix_ms
            {
                if let Ok(bytes) = operation_record_bytes(record) {
                    removed = removed.saturating_add(1);
                    removed_bytes = removed_bytes.saturating_add(bytes);
                    false
                } else {
                    true
                }
            } else {
                true
            }
        });
        self.retained_bytes = self.retained_bytes.saturating_sub(removed_bytes);
        removed
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum OperationModelError {
    #[error("operation text exceeds its bound")]
    TextLimit,
    #[error("operation evidence exceeds its bound")]
    EvidenceLimit,
    #[error("operation actions exceed their bound or contain duplicates")]
    ActionLimit,
    #[error("operation timestamps are invalid")]
    InvalidTimestamp,
    #[error("operation progress requires a nonzero authoritative total")]
    InvalidProgress,
    #[error("operation domain does not support byte progress")]
    UnsupportedProgress,
    #[error("delivered operation lacks authoritative peer-delivery evidence")]
    MissingDeliveryEvidence,
    #[error("operation record exceeds its byte bound")]
    RecordByteLimit,
    #[error("operation history is full of unresolved work")]
    HistoryCapacity,
    #[error("operation size accounting overflowed")]
    SizeOverflow,
}

fn operation_record_bytes(record: &OperationRecord) -> Result<usize, OperationModelError> {
    let mut bytes = 128usize
        .checked_add(record.target.label.len())
        .ok_or(OperationModelError::SizeOverflow)?;
    for evidence in &record.evidence {
        bytes = bytes
            .checked_add(48)
            .and_then(|value| {
                value.checked_add(
                    evidence
                        .detail
                        .as_ref()
                        .map(String::len)
                        .unwrap_or_default(),
                )
            })
            .ok_or(OperationModelError::SizeOverflow)?;
    }
    for value in [record.propagation_node.as_ref(), record.last_error.as_ref()]
        .into_iter()
        .flatten()
    {
        bytes = bytes
            .checked_add(value.len())
            .ok_or(OperationModelError::SizeOverflow)?;
    }
    bytes
        .checked_add(record.valid_actions.len().saturating_mul(8))
        .ok_or(OperationModelError::SizeOverflow)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(local_id: u64, state: OperationState, updated_at_unix_ms: i64) -> OperationRecord {
        let evidence = if state == OperationState::Delivered {
            vec![OperationEvidence {
                kind: OperationEvidenceKind::PeerDelivery,
                authority: EvidenceAuthority::Authoritative,
                at_unix_ms: 1,
                detail: None,
            }]
        } else {
            vec![OperationEvidence {
                kind: OperationEvidenceKind::QueueAdmission,
                authority: EvidenceAuthority::Authoritative,
                at_unix_ms: 1,
                detail: None,
            }]
        };
        OperationRecord {
            id: OperationId {
                domain: OperationDomain::LxmfMessage,
                local_id,
            },
            target: OperationTarget {
                kind: OperationTargetKind::Peer,
                label: format!("peer-{local_id}"),
            },
            state,
            authority: EvidenceAuthority::Authoritative,
            evidence,
            progress: None,
            attempt_count: 1,
            stamp_cost: None,
            propagation_node: None,
            created_at_unix_ms: 1,
            updated_at_unix_ms,
            last_error: None,
            event_cursor: None,
            valid_actions: vec![OperationAction::CopyDiagnostics],
        }
    }

    #[test]
    fn queue_transport_receipt_and_delivery_are_distinct_states() {
        for state in [
            OperationState::Queued,
            OperationState::TransportAccepted,
            OperationState::ReceiptObserved,
        ] {
            assert!(!state.claims_peer_delivery());
            assert!(!state.is_terminal());
        }
        assert!(OperationState::Delivered.claims_peer_delivery());
        assert!(OperationState::Delivered.is_terminal());
        let mut unsupported_claim = record(9, OperationState::Active, 2);
        unsupported_claim.state = OperationState::Delivered;
        assert_eq!(
            unsupported_claim.validate(),
            Err(OperationModelError::MissingDeliveryEvidence)
        );
    }

    #[test]
    fn progress_requires_an_authoritative_nonzero_total() {
        assert_eq!(
            AuthoritativeProgress::new(1, 0),
            Err(OperationModelError::InvalidProgress)
        );
        assert_eq!(
            AuthoritativeProgress::new(2, 1),
            Err(OperationModelError::InvalidProgress)
        );
        assert_eq!(
            AuthoritativeProgress::new(1, 2).expect("valid progress"),
            AuthoritativeProgress {
                completed_bytes: 1,
                total_bytes: 2,
            }
        );
        let mut malformed = record(1, OperationState::Active, 2);
        malformed.progress = Some(AuthoritativeProgress {
            completed_bytes: 1,
            total_bytes: 0,
        });
        assert_eq!(
            malformed.validate(),
            Err(OperationModelError::InvalidProgress)
        );
        let mut unsupported = record(1, OperationState::Active, 2);
        unsupported.id.domain = OperationDomain::PathDiscovery;
        unsupported.progress = Some(AuthoritativeProgress::new(1, 2).expect("progress"));
        assert_eq!(
            unsupported.validate(),
            Err(OperationModelError::UnsupportedProgress)
        );
    }

    #[test]
    fn history_coalesces_updates_and_evicts_only_terminal_records() {
        let record_bytes = record(1, OperationState::Active, 2)
            .validate()
            .expect("record bytes");
        let mut history = OperationHistory::new(2, record_bytes.saturating_mul(3));
        history
            .upsert(record(1, OperationState::Active, 2))
            .expect("active");
        history
            .upsert(record(1, OperationState::Transferring, 3))
            .expect("coalesced progress");
        assert_eq!(history.metrics().items, 1);
        history
            .upsert(record(2, OperationState::Delivered, 4))
            .expect("terminal");
        history
            .upsert(record(3, OperationState::Active, 5))
            .expect("evict terminal");
        assert_eq!(
            history
                .records()
                .map(|record| record.id.local_id)
                .collect::<Vec<_>>(),
            vec![1, 3]
        );
        assert_eq!(history.metrics().evicted_terminal, 1);
        assert_eq!(
            history.upsert(record(4, OperationState::Active, 6)),
            Err(OperationModelError::HistoryCapacity)
        );
        assert_eq!(history.metrics().rejected, 1);
        assert_eq!(
            history
                .records()
                .map(|record| record.id.local_id)
                .collect::<Vec<_>>(),
            vec![1, 3]
        );
    }

    #[test]
    fn history_incrementally_expires_completed_records() {
        let mut history = OperationHistory::new(8, OPERATION_HISTORY_MAX_BYTES);
        for local_id in 1..=3 {
            history
                .upsert(record(local_id, OperationState::Delivered, local_id as i64))
                .expect("terminal record");
        }
        history
            .upsert(record(4, OperationState::Active, 1))
            .expect("active record");
        assert_eq!(history.expire_terminal_before(4, 2), 2);
        assert_eq!(
            history
                .records()
                .map(|record| record.id.local_id)
                .collect::<Vec<_>>(),
            vec![3, 4]
        );
        assert!(history.metrics().bytes <= OPERATION_HISTORY_MAX_BYTES);
    }

    #[test]
    fn history_byte_saturation_preserves_unresolved_work() {
        let first = record(1, OperationState::Active, 2);
        let first_bytes = first.validate().expect("first bytes");
        let mut second = record(2, OperationState::Active, 3);
        second.target.label = "p".repeat(256);
        let second_bytes = second.validate().expect("second bytes");
        let mut history = OperationHistory::new(
            8,
            first_bytes.saturating_add(second_bytes).saturating_sub(1),
        );
        history.upsert(first.clone()).expect("first active");
        assert_eq!(
            history.upsert(second),
            Err(OperationModelError::HistoryCapacity)
        );
        assert_eq!(history.records().collect::<Vec<_>>(), vec![&first]);
        assert_eq!(history.metrics().bytes, first_bytes);
        assert_eq!(history.metrics().rejected, 1);
    }

    #[test]
    fn record_rejects_unbounded_text_evidence_and_actions() {
        let mut oversized = record(1, OperationState::Active, 2);
        oversized.target.label = "x".repeat(OPERATION_TEXT_MAX_BYTES + 1);
        assert_eq!(oversized.validate(), Err(OperationModelError::TextLimit));

        let mut evidence = record(2, OperationState::Active, 2);
        evidence.evidence = (0..=OPERATION_EVIDENCE_MAX_ITEMS)
            .map(|_| OperationEvidence {
                kind: OperationEvidenceKind::Dispatch,
                authority: EvidenceAuthority::Authoritative,
                at_unix_ms: 1,
                detail: None,
            })
            .collect();
        assert_eq!(evidence.validate(), Err(OperationModelError::EvidenceLimit));

        let mut duplicate_action = record(3, OperationState::Active, 2);
        duplicate_action.valid_actions = vec![
            OperationAction::CopyDiagnostics,
            OperationAction::CopyDiagnostics,
        ];
        assert_eq!(
            duplicate_action.validate(),
            Err(OperationModelError::ActionLimit)
        );

        let mut record_bytes = record(4, OperationState::Active, 2);
        record_bytes.evidence = (0..8)
            .map(|_| OperationEvidence {
                kind: OperationEvidenceKind::Dispatch,
                authority: EvidenceAuthority::Authoritative,
                at_unix_ms: 1,
                detail: Some("e".repeat(OPERATION_TEXT_MAX_BYTES)),
            })
            .collect();
        assert_eq!(
            record_bytes.validate(),
            Err(OperationModelError::RecordByteLimit)
        );
    }
}
