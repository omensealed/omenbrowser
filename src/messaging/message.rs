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
pub const OUTBOUND_PROPAGATION_FALLBACK_FIELD: &str = "native_lxmf_allow_propagation_fallback";
pub const OUTBOUND_AUTOMATIC_PROPAGATION_FALLBACK_FIELD: &str =
    "native_lxmf_automatic_propagation_fallback";
pub const OUTBOUND_MAX_AUTOMATIC_DIRECT_STAMP_COST_FIELD: &str =
    "native_lxmf_max_automatic_direct_stamp_cost";
pub const OUTBOUND_ASK_ABOVE_DIRECT_STAMP_COST_FIELD: &str =
    "native_lxmf_ask_above_direct_stamp_cost";
pub const OUTBOUND_APPROVED_DIRECT_STAMP_COST_FIELD: &str =
    "native_lxmf_approved_direct_stamp_cost";
pub const OUTBOUND_DEFAULT_MAX_AUTOMATIC_DIRECT_STAMP_COST: u8 = 8;
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
    #[serde(default = "default_allow_propagation_fallback")]
    pub allow_propagation_fallback: bool,
    #[serde(default)]
    pub automatic_propagation_fallback: bool,
    #[serde(default = "default_max_automatic_direct_stamp_cost")]
    pub max_automatic_direct_stamp_cost: u8,
    #[serde(default)]
    pub ask_above_direct_stamp_cost: Option<u8>,
    #[serde(default)]
    pub approved_direct_stamp_cost: Option<u8>,
}

const fn default_allow_propagation_fallback() -> bool {
    true
}

const fn default_max_automatic_direct_stamp_cost() -> u8 {
    OUTBOUND_DEFAULT_MAX_AUTOMATIC_DIRECT_STAMP_COST
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
        let mut operation = Self::validated_with_deadline(
            idempotency_key,
            correlation_id,
            deadline.0,
            deadline.1,
            deadline.2,
        )?;
        operation.allow_propagation_fallback =
            match message.fields.get(OUTBOUND_PROPAGATION_FALLBACK_FIELD) {
                Some(value) if value == "true" => true,
                Some(value) if value == "false" => false,
                Some(_) => return None,
                None => true,
            };
        operation.automatic_propagation_fallback = match message
            .fields
            .get(OUTBOUND_AUTOMATIC_PROPAGATION_FALLBACK_FIELD)
        {
            Some(value) if value == "true" => true,
            Some(value) if value == "false" => false,
            Some(_) => return None,
            None => false,
        };
        operation.max_automatic_direct_stamp_cost = match message
            .fields
            .get(OUTBOUND_MAX_AUTOMATIC_DIRECT_STAMP_COST_FIELD)
        {
            Some(value) => value.parse().ok()?,
            None => OUTBOUND_DEFAULT_MAX_AUTOMATIC_DIRECT_STAMP_COST,
        };
        operation.ask_above_direct_stamp_cost =
            parse_optional_u8_field(&message.fields, OUTBOUND_ASK_ABOVE_DIRECT_STAMP_COST_FIELD)?;
        operation.approved_direct_stamp_cost =
            parse_optional_u8_field(&message.fields, OUTBOUND_APPROVED_DIRECT_STAMP_COST_FIELD)?;
        Some(operation)
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
                allow_propagation_fallback: true,
                automatic_propagation_fallback: false,
                max_automatic_direct_stamp_cost: OUTBOUND_DEFAULT_MAX_AUTOMATIC_DIRECT_STAMP_COST,
                ask_above_direct_stamp_cost: None,
                approved_direct_stamp_cost: None,
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
        fields.insert(
            OUTBOUND_PROPAGATION_FALLBACK_FIELD.into(),
            self.allow_propagation_fallback.to_string(),
        );
        fields.insert(
            OUTBOUND_AUTOMATIC_PROPAGATION_FALLBACK_FIELD.into(),
            self.automatic_propagation_fallback.to_string(),
        );
        fields.insert(
            OUTBOUND_MAX_AUTOMATIC_DIRECT_STAMP_COST_FIELD.into(),
            self.max_automatic_direct_stamp_cost.to_string(),
        );
        if let Some(cost) = self.ask_above_direct_stamp_cost {
            fields.insert(
                OUTBOUND_ASK_ABOVE_DIRECT_STAMP_COST_FIELD.into(),
                cost.to_string(),
            );
        }
        if let Some(cost) = self.approved_direct_stamp_cost {
            fields.insert(
                OUTBOUND_APPROVED_DIRECT_STAMP_COST_FIELD.into(),
                cost.to_string(),
            );
        }
    }
}

fn parse_optional_u8_field(
    fields: &std::collections::BTreeMap<String, String>,
    name: &str,
) -> Option<Option<u8>> {
    match fields.get(name) {
        Some(value) => Some(Some(value.parse().ok()?)),
        None => Some(None),
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
        let mut first = OutboundOperationIdentity::generate();
        first.allow_propagation_fallback = false;
        first.automatic_propagation_fallback = true;
        first.max_automatic_direct_stamp_cost = 4;
        first.ask_above_direct_stamp_cost = Some(1);
        first.approved_direct_stamp_cost = Some(4);
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
        message
            .fields
            .insert(OUTBOUND_PROPAGATION_FALLBACK_FIELD.into(), "invalid".into());
        assert!(OutboundOperationIdentity::from_message(&message).is_none());
        message.fields.remove(OUTBOUND_PROPAGATION_FALLBACK_FIELD);
        let legacy =
            OutboundOperationIdentity::from_message(&message).expect("legacy operation metadata");
        assert!(
            legacy.allow_propagation_fallback,
            "operation metadata written before the policy field must preserve legacy fallback"
        );
        message
            .fields
            .remove(OUTBOUND_AUTOMATIC_PROPAGATION_FALLBACK_FIELD);
        assert!(
            !OutboundOperationIdentity::from_message(&message)
                .expect("legacy operation metadata")
                .automatic_propagation_fallback,
            "legacy operation metadata must never opt into automatic fallback"
        );
        message.fields.insert(
            OUTBOUND_AUTOMATIC_PROPAGATION_FALLBACK_FIELD.into(),
            "invalid".into(),
        );
        assert!(OutboundOperationIdentity::from_message(&message).is_none());
        message
            .fields
            .remove(OUTBOUND_AUTOMATIC_PROPAGATION_FALLBACK_FIELD);
        message
            .fields
            .remove(OUTBOUND_MAX_AUTOMATIC_DIRECT_STAMP_COST_FIELD);
        message
            .fields
            .remove(OUTBOUND_ASK_ABOVE_DIRECT_STAMP_COST_FIELD);
        message
            .fields
            .remove(OUTBOUND_APPROVED_DIRECT_STAMP_COST_FIELD);
        let legacy =
            OutboundOperationIdentity::from_message(&message).expect("legacy operation metadata");
        assert_eq!(
            legacy.max_automatic_direct_stamp_cost,
            OUTBOUND_DEFAULT_MAX_AUTOMATIC_DIRECT_STAMP_COST
        );
        assert_eq!(legacy.ask_above_direct_stamp_cost, None);
        assert_eq!(legacy.approved_direct_stamp_cost, None);
        message.fields.insert(
            OUTBOUND_MAX_AUTOMATIC_DIRECT_STAMP_COST_FIELD.into(),
            "999".into(),
        );
        assert!(OutboundOperationIdentity::from_message(&message).is_none());
        message.fields.insert(
            OUTBOUND_MAX_AUTOMATIC_DIRECT_STAMP_COST_FIELD.into(),
            "8".into(),
        );
        message.fields.insert(
            OUTBOUND_ASK_ABOVE_DIRECT_STAMP_COST_FIELD.into(),
            "invalid".into(),
        );
        assert!(OutboundOperationIdentity::from_message(&message).is_none());
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
