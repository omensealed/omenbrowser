use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

static OUTBOUND_OPERATION_SEQUENCE: AtomicU64 = AtomicU64::new(1);
pub const OUTBOUND_OPERATION_ID_MAX_BYTES: usize = 192;
pub const OUTBOUND_IDEMPOTENCY_FIELD: &str = "native_lxmf_sdk_idempotency_key";
pub const OUTBOUND_CORRELATION_FIELD: &str = "native_lxmf_sdk_correlation_id";
pub const OUTBOUND_TTL_FIELD: &str = "native_lxmf_sdk_ttl_ms";
pub const OUTBOUND_CREATED_AT_FIELD: &str = "native_lxmf_sdk_created_at_ms";
pub const OUTBOUND_EXPIRES_AT_FIELD: &str = "native_lxmf_sdk_expires_at_ms";
pub const OUTBOUND_DEFAULT_TTL_MS: u64 = 86_400_000;
pub const OUTBOUND_MIN_TTL_MS: u64 = 1_000;
pub const OUTBOUND_MAX_TTL_MS: u64 = 86_400_000;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct OutboundOperationIdentity {
    pub idempotency_key: String,
    pub correlation_id: String,
    pub created_at_ms: u64,
    pub ttl_ms: u64,
    pub expires_at_ms: u64,
}

impl OutboundOperationIdentity {
    pub fn generate() -> Self {
        Self::generate_at(current_epoch_ms(), OUTBOUND_DEFAULT_TTL_MS)
            .expect("default outbound TTL must be valid")
    }

    pub fn generate_at(created_at_ms: u64, ttl_ms: u64) -> Option<Self> {
        if !(OUTBOUND_MIN_TTL_MS..=OUTBOUND_MAX_TTL_MS).contains(&ttl_ms) {
            return None;
        }
        let now_nanos = u128::from(created_at_ms).saturating_mul(1_000_000);
        let sequence = OUTBOUND_OPERATION_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let suffix = format!("{now_nanos:032x}-{:x}-{sequence:x}", std::process::id());
        Self::validated_with_deadline(
            format!("omen-send-{suffix}"),
            format!("omen-corr-{suffix}"),
            created_at_ms,
            ttl_ms,
            created_at_ms.checked_add(ttl_ms)?,
        )
    }

    pub fn from_message(message: &MessageSummary) -> Option<Self> {
        Self::from_message_at(message, current_epoch_ms())
    }

    pub fn from_message_at(message: &MessageSummary, now_ms: u64) -> Option<Self> {
        let idempotency_key = message.fields.get(OUTBOUND_IDEMPOTENCY_FIELD)?.clone();
        let correlation_id = message.fields.get(OUTBOUND_CORRELATION_FIELD)?.clone();
        let deadline = match (
            message.fields.get(OUTBOUND_CREATED_AT_FIELD),
            message.fields.get(OUTBOUND_TTL_FIELD),
            message.fields.get(OUTBOUND_EXPIRES_AT_FIELD),
        ) {
            (None, None, None) => (
                now_ms,
                OUTBOUND_DEFAULT_TTL_MS,
                now_ms.checked_add(OUTBOUND_DEFAULT_TTL_MS)?,
            ),
            (Some(created_at), Some(ttl), Some(expires_at)) => (
                created_at.parse().ok()?,
                ttl.parse().ok()?,
                expires_at.parse().ok()?,
            ),
            _ => return None,
        };
        Self::validated_with_deadline(
            idempotency_key,
            correlation_id,
            deadline.0,
            deadline.1,
            deadline.2,
        )
    }

    pub fn validated(idempotency_key: String, correlation_id: String) -> Option<Self> {
        let now_ms = current_epoch_ms();
        Self::validated_with_deadline(
            idempotency_key,
            correlation_id,
            now_ms,
            OUTBOUND_DEFAULT_TTL_MS,
            now_ms.checked_add(OUTBOUND_DEFAULT_TTL_MS)?,
        )
    }

    pub fn validated_with_deadline(
        idempotency_key: String,
        correlation_id: String,
        created_at_ms: u64,
        ttl_ms: u64,
        expires_at_ms: u64,
    ) -> Option<Self> {
        if operation_id_valid(&idempotency_key)
            && operation_id_valid(&correlation_id)
            && (OUTBOUND_MIN_TTL_MS..=OUTBOUND_MAX_TTL_MS).contains(&ttl_ms)
            && created_at_ms.checked_add(ttl_ms) == Some(expires_at_ms)
        {
            Some(Self {
                idempotency_key,
                correlation_id,
                created_at_ms,
                ttl_ms,
                expires_at_ms,
            })
        } else {
            None
        }
    }

    pub fn remaining_ttl_ms_at(&self, now_ms: u64) -> Option<u64> {
        let remaining = self.expires_at_ms.checked_sub(now_ms)?;
        (remaining > 0).then_some(remaining)
    }

    pub fn remaining_ttl_ms(&self) -> Option<u64> {
        self.remaining_ttl_ms_at(current_epoch_ms())
    }

    pub fn insert_fields(&self, fields: &mut std::collections::BTreeMap<String, String>) {
        fields.insert(
            OUTBOUND_IDEMPOTENCY_FIELD.into(),
            self.idempotency_key.clone(),
        );
        fields.insert(
            OUTBOUND_CORRELATION_FIELD.into(),
            self.correlation_id.clone(),
        );
        fields.insert(
            OUTBOUND_CREATED_AT_FIELD.into(),
            self.created_at_ms.to_string(),
        );
        fields.insert(OUTBOUND_TTL_FIELD.into(), self.ttl_ms.to_string());
        fields.insert(
            OUTBOUND_EXPIRES_AT_FIELD.into(),
            self.expires_at_ms.to_string(),
        );
    }
}

fn current_epoch_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| u64::try_from(duration.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or_default()
}

fn operation_id_valid(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= OUTBOUND_OPERATION_ID_MAX_BYTES
        && value.bytes().all(|byte| byte.is_ascii_graphic())
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TransportMethod {
    Direct,
    Propagated,
    Unknown(String),
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DeliveryMode {
    Direct,
    Propagated,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DeliveryState {
    Incoming,
    Pending,
    Delivered,
    Failed,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct AttachmentSummary {
    pub name: String,
    pub size: u64,
    pub path: Option<PathBuf>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct MessageSummary {
    pub peer_hash: String,
    pub peer_label: String,
    pub title: String,
    pub content: String,
    pub timestamp: f64,
    pub transport_method: TransportMethod,
    pub delivered: bool,
    pub failed: bool,
    pub incoming: bool,
    pub unread: bool,
    pub message_id: Option<String>,
    pub fields: std::collections::BTreeMap<String, String>,
    pub attachments: Vec<AttachmentSummary>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct NativeLxmfReplyTicket {
    pub ticket: Vec<u8>,
    pub expires: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct MessageEnvelope {
    pub peer_hash: String,
    pub title: String,
    pub body: String,
    pub delivery_mode: DeliveryMode,
    pub include_ticket: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub native_reply_ticket: Option<NativeLxmfReplyTicket>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operation: Option<OutboundOperationIdentity>,
    pub attachments: Vec<PathBuf>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outbound_operation_identity_is_unique_and_round_trips_through_message_fields() {
        let first = OutboundOperationIdentity::generate();
        let second = OutboundOperationIdentity::generate();
        assert_ne!(first, second);

        let mut message = MessageSummary {
            peer_hash: "peer".into(),
            peer_label: "Peer".into(),
            title: "Title".into(),
            content: "Body".into(),
            timestamp: 1.0,
            transport_method: TransportMethod::Direct,
            delivered: false,
            failed: false,
            incoming: false,
            unread: false,
            message_id: Some("message".into()),
            fields: std::collections::BTreeMap::new(),
            attachments: Vec::new(),
        };
        first.insert_fields(&mut message.fields);
        assert_eq!(
            OutboundOperationIdentity::from_message(&message),
            Some(first)
        );
    }

    #[test]
    fn outbound_operation_identity_rejects_unbounded_or_non_graphic_values() {
        assert!(OutboundOperationIdentity::validated("idem".into(), "corr".into()).is_some());
        assert!(OutboundOperationIdentity::validated("has space".into(), "corr".into()).is_none());
        assert!(OutboundOperationIdentity::validated(
            "x".repeat(OUTBOUND_OPERATION_ID_MAX_BYTES + 1),
            "corr".into(),
        )
        .is_none());
    }

    #[test]
    fn outbound_operation_ttl_is_absolute_bounded_and_restart_safe() {
        let operation =
            OutboundOperationIdentity::generate_at(10_000, 5_000).expect("bounded operation TTL");
        assert_eq!(operation.remaining_ttl_ms_at(10_000), Some(5_000));
        assert_eq!(operation.remaining_ttl_ms_at(14_999), Some(1));
        assert_eq!(operation.remaining_ttl_ms_at(15_000), None);

        let mut message = MessageSummary {
            peer_hash: "peer".into(),
            peer_label: "Peer".into(),
            title: "Title".into(),
            content: "Body".into(),
            timestamp: 1.0,
            transport_method: TransportMethod::Direct,
            delivered: false,
            failed: false,
            incoming: false,
            unread: false,
            message_id: Some("message".into()),
            fields: std::collections::BTreeMap::new(),
            attachments: Vec::new(),
        };
        operation.insert_fields(&mut message.fields);
        assert_eq!(
            OutboundOperationIdentity::from_message_at(&message, 12_000),
            Some(operation)
        );
    }

    #[test]
    fn outbound_operation_ttl_rejects_invalid_or_partial_deadlines() {
        assert!(OutboundOperationIdentity::generate_at(10, OUTBOUND_MIN_TTL_MS - 1).is_none());
        assert!(OutboundOperationIdentity::generate_at(10, OUTBOUND_MAX_TTL_MS + 1).is_none());
        assert!(OutboundOperationIdentity::validated_with_deadline(
            "idem".into(),
            "corr".into(),
            10,
            OUTBOUND_MIN_TTL_MS,
            11,
        )
        .is_none());

        let mut message = MessageSummary {
            peer_hash: "peer".into(),
            peer_label: "Peer".into(),
            title: String::new(),
            content: String::new(),
            timestamp: 1.0,
            transport_method: TransportMethod::Direct,
            delivered: false,
            failed: false,
            incoming: false,
            unread: false,
            message_id: Some("message".into()),
            fields: std::collections::BTreeMap::from([
                (OUTBOUND_IDEMPOTENCY_FIELD.into(), "idem".into()),
                (OUTBOUND_CORRELATION_FIELD.into(), "corr".into()),
                (OUTBOUND_TTL_FIELD.into(), "5000".into()),
            ]),
            attachments: Vec::new(),
        };
        assert!(OutboundOperationIdentity::from_message_at(&message, 10_000).is_none());

        message.fields.remove(OUTBOUND_TTL_FIELD);
        let migrated = OutboundOperationIdentity::from_message_at(&message, 10_000)
            .expect("legacy operation IDs receive one bounded migration window");
        assert_eq!(migrated.created_at_ms, 10_000);
        assert_eq!(migrated.ttl_ms, OUTBOUND_DEFAULT_TTL_MS);
    }
}
