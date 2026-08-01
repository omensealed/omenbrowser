use std::collections::VecDeque;
use std::fmt;

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const LXMF_RESOURCE_REFERENCE_PROTOCOL: &str = "omenbrowser.lxmf.resource-reference";
pub const LXMF_RESOURCE_REFERENCE_VERSION: u16 = 1;
pub const LXMF_RESOURCE_REFERENCE_CAPABILITY: &str = "omen-lxmf-resource-reference-v1";
pub const LXMF_RESOURCE_REFERENCE_MAX_ENCODED_BYTES: usize = 2 * 1024;
pub const LXMF_RESOURCE_REFERENCE_CONTENT_HASH_HEX_CHARS: usize = 64;
pub const LXMF_RESOURCE_REFERENCE_IDENTITY_HEX_CHARS: usize = 32;
pub const LXMF_RESOURCE_REFERENCE_ID_HEX_CHARS: usize = 32;
pub const LXMF_RESOURCE_REFERENCE_DISPLAY_NAME_MAX_BYTES: usize = 200;
pub const LXMF_RESOURCE_REFERENCE_MEDIA_TYPE_MAX_BYTES: usize = 128;
pub const LXMF_RESOURCE_REFERENCE_MAX_DECLARED_BYTES: u64 = 64 * 1024 * 1024;
pub const LXMF_RESOURCE_REFERENCE_MAX_LIFETIME_SECS: u64 = 24 * 60 * 60;
pub const LXMF_RESOURCE_REFERENCE_CLOCK_SKEW_SECS: u64 = 5 * 60;
pub const LXMF_RESOURCE_PENDING_MAX_ITEMS: usize = 32;
pub const LXMF_RESOURCE_PENDING_MAX_ACCOUNTED_BYTES: usize = 64 * 1024;
pub const LXMF_RESOURCE_PENDING_MAX_PER_PEER_ITEMS: usize = 8;
pub const LXMF_RESOURCE_PENDING_MAX_PER_PEER_BYTES: usize = 16 * 1024;
pub const LXMF_RESOURCE_PENDING_MAX_PER_CONVERSATION_ITEMS: usize = 8;
pub const LXMF_RESOURCE_PENDING_MAX_PER_CONVERSATION_BYTES: usize = 16 * 1024;
pub const LXMF_RESOURCE_PENDING_CONVERSATION_KEY_MAX_BYTES: usize = 128;
pub const LXMF_RESOURCE_PENDING_RATE_WINDOW_SECS: u64 = 10 * 60;
pub const LXMF_RESOURCE_PENDING_MAX_PER_PEER_WINDOW: usize = 8;
pub const LXMF_RESOURCE_PENDING_MAX_GLOBAL_PER_WINDOW: usize = 64;
pub const LXMF_RESOURCE_PENDING_RATE_MAX_ITEMS: usize = 256;
pub const LXMF_RESOURCE_PENDING_PRUNE_BATCH: usize = 8;

/// A dormant signed-LXMF attachment offer. `resource_reference` is an
/// application correlation identifier, not a redeemable Reticulum Resource
/// hash. A separately authenticated accept exchange must bind a future Link
/// Resource transfer to this offer before any bytes move.
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct LxmfResourceReferenceEnvelope {
    pub protocol: String,
    pub version: u16,
    pub content_hash: String,
    pub declared_size: u64,
    pub media_type: String,
    pub display_name: String,
    pub sender_identity: String,
    pub created_at_unix: u64,
    pub expires_at_unix: u64,
    pub resource_reference: String,
}

impl fmt::Debug for LxmfResourceReferenceEnvelope {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("LxmfResourceReferenceEnvelope")
            .field("protocol", &self.protocol)
            .field("version", &self.version)
            .field("content_hash", &self.content_hash)
            .field("declared_size", &self.declared_size)
            .field("media_type", &self.media_type)
            .field("display_name", &self.display_name)
            .field("sender_identity", &self.sender_identity)
            .field("created_at_unix", &self.created_at_unix)
            .field("expires_at_unix", &self.expires_at_unix)
            .field("resource_reference", &"[redacted]")
            .finish()
    }
}

impl LxmfResourceReferenceEnvelope {
    pub fn encode_at(&self, now_unix: u64) -> Result<Vec<u8>, LxmfResourceReferenceError> {
        self.validate_at(now_unix)?;
        let encoded =
            serde_json::to_vec(self).map_err(|_| LxmfResourceReferenceError::Malformed)?;
        if encoded.len() > LXMF_RESOURCE_REFERENCE_MAX_ENCODED_BYTES {
            return Err(LxmfResourceReferenceError::TooLarge);
        }
        Ok(encoded)
    }

    pub fn decode_authenticated_at(
        encoded: &[u8],
        authenticated_sender: Option<&str>,
        now_unix: u64,
    ) -> Result<Self, LxmfResourceReferenceError> {
        if encoded.len() > LXMF_RESOURCE_REFERENCE_MAX_ENCODED_BYTES {
            return Err(LxmfResourceReferenceError::TooLarge);
        }
        let envelope = serde_json::from_slice::<Self>(encoded)
            .map_err(|_| LxmfResourceReferenceError::Malformed)?;
        envelope.validate_at(now_unix)?;
        let sender = authenticated_sender
            .filter(|value| canonical_lower_hex(value, LXMF_RESOURCE_REFERENCE_IDENTITY_HEX_CHARS))
            .ok_or(LxmfResourceReferenceError::UnauthenticatedSender)?;
        if sender != envelope.sender_identity {
            return Err(LxmfResourceReferenceError::SenderMismatch);
        }
        Ok(envelope)
    }

    pub fn validate_at(&self, now_unix: u64) -> Result<(), LxmfResourceReferenceError> {
        if self.protocol != LXMF_RESOURCE_REFERENCE_PROTOCOL
            || self.version != LXMF_RESOURCE_REFERENCE_VERSION
        {
            return Err(LxmfResourceReferenceError::UnsupportedProtocol);
        }
        if !canonical_lower_hex(
            &self.content_hash,
            LXMF_RESOURCE_REFERENCE_CONTENT_HASH_HEX_CHARS,
        ) {
            return Err(LxmfResourceReferenceError::InvalidContentHash);
        }
        if self.declared_size == 0
            || self.declared_size > LXMF_RESOURCE_REFERENCE_MAX_DECLARED_BYTES
        {
            return Err(LxmfResourceReferenceError::InvalidDeclaredSize);
        }
        if !valid_media_type_hint(&self.media_type) {
            return Err(LxmfResourceReferenceError::InvalidMediaType);
        }
        if !valid_display_name_hint(&self.display_name) {
            return Err(LxmfResourceReferenceError::InvalidDisplayName);
        }
        if !canonical_lower_hex(
            &self.sender_identity,
            LXMF_RESOURCE_REFERENCE_IDENTITY_HEX_CHARS,
        ) {
            return Err(LxmfResourceReferenceError::InvalidSenderIdentity);
        }
        if !canonical_lower_hex(
            &self.resource_reference,
            LXMF_RESOURCE_REFERENCE_ID_HEX_CHARS,
        ) {
            return Err(LxmfResourceReferenceError::InvalidResourceReference);
        }
        if self.created_at_unix > now_unix.saturating_add(LXMF_RESOURCE_REFERENCE_CLOCK_SKEW_SECS) {
            return Err(LxmfResourceReferenceError::CreatedInFuture);
        }
        if self.expires_at_unix <= self.created_at_unix
            || self.expires_at_unix.saturating_sub(self.created_at_unix)
                > LXMF_RESOURCE_REFERENCE_MAX_LIFETIME_SECS
        {
            return Err(LxmfResourceReferenceError::InvalidExpiry);
        }
        if self
            .expires_at_unix
            .saturating_add(LXMF_RESOURCE_REFERENCE_CLOCK_SKEW_SECS)
            < now_unix
        {
            return Err(LxmfResourceReferenceError::Expired);
        }
        Ok(())
    }

    /// The eventual private storage key is derived only from the verified
    /// content hash. The untrusted display name is never used as a path.
    pub fn storage_file_name(&self) -> &str {
        self.content_hash.as_str()
    }

    pub const fn allows_automatic_transfer(&self) -> bool {
        false
    }

    pub const fn allows_automatic_decode(&self) -> bool {
        false
    }

    pub const fn allows_executable_launch(&self) -> bool {
        false
    }
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum LxmfResourceReferenceError {
    #[error("LXMF Resource-reference envelope exceeds its encoded byte limit")]
    TooLarge,
    #[error("LXMF Resource-reference envelope is malformed")]
    Malformed,
    #[error("LXMF Resource-reference protocol or version is unsupported")]
    UnsupportedProtocol,
    #[error("LXMF Resource-reference content hash is invalid")]
    InvalidContentHash,
    #[error("LXMF Resource-reference declared size is invalid")]
    InvalidDeclaredSize,
    #[error("LXMF Resource-reference media-type hint is invalid")]
    InvalidMediaType,
    #[error("LXMF Resource-reference display-name hint is invalid")]
    InvalidDisplayName,
    #[error("LXMF Resource-reference sender identity is invalid")]
    InvalidSenderIdentity,
    #[error("LXMF Resource-reference identifier is invalid")]
    InvalidResourceReference,
    #[error("LXMF Resource-reference creation time is too far in the future")]
    CreatedInFuture,
    #[error("LXMF Resource-reference expiry is invalid")]
    InvalidExpiry,
    #[error("LXMF Resource-reference envelope has expired")]
    Expired,
    #[error("LXMF Resource-reference sender is not authenticated")]
    UnauthenticatedSender,
    #[error("LXMF Resource-reference sender does not match its authenticated identity")]
    SenderMismatch,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LxmfResourcePendingOffer {
    envelope: LxmfResourceReferenceEnvelope,
    authenticated_sender: String,
    conversation_key: String,
    received_at_unix: u64,
    accounted_bytes: usize,
}

impl LxmfResourcePendingOffer {
    pub fn envelope(&self) -> &LxmfResourceReferenceEnvelope {
        &self.envelope
    }

    pub fn authenticated_sender(&self) -> &str {
        self.authenticated_sender.as_str()
    }

    pub fn conversation_key(&self) -> &str {
        self.conversation_key.as_str()
    }

    pub fn received_at_unix(&self) -> u64 {
        self.received_at_unix
    }

    pub const fn allows_transfer(&self) -> bool {
        false
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct LxmfResourcePendingRateRecord {
    authenticated_sender: String,
    admitted_at_unix: u64,
}

#[derive(Clone, Debug, Default)]
pub struct LxmfResourcePendingOfferOwner {
    pending: VecDeque<LxmfResourcePendingOffer>,
    pending_accounted_bytes: usize,
    rate_records: VecDeque<LxmfResourcePendingRateRecord>,
}

impl LxmfResourcePendingOfferOwner {
    pub fn pending(&self) -> impl Iterator<Item = &LxmfResourcePendingOffer> {
        self.pending.iter()
    }

    pub fn pending_accounted_bytes(&self) -> usize {
        self.pending_accounted_bytes
    }

    pub fn rate_record_count(&self) -> usize {
        self.rate_records.len()
    }

    pub fn admit_at(
        &mut self,
        encoded: &[u8],
        authenticated_sender: Option<&str>,
        conversation_key: &str,
        received_at_unix: u64,
    ) -> Result<(), LxmfResourcePendingOfferError> {
        self.prune_at(received_at_unix);
        if !valid_conversation_key(conversation_key) {
            return Err(LxmfResourcePendingOfferError::InvalidConversation);
        }
        let envelope = LxmfResourceReferenceEnvelope::decode_authenticated_at(
            encoded,
            authenticated_sender,
            received_at_unix,
        )?;
        if envelope.expires_at_unix <= received_at_unix {
            return Err(LxmfResourceReferenceError::Expired.into());
        }
        let authenticated_sender =
            authenticated_sender.ok_or(LxmfResourceReferenceError::UnauthenticatedSender)?;

        if let Some(existing) = self.pending.iter().find(|offer| {
            offer.authenticated_sender == authenticated_sender
                && offer.envelope.resource_reference == envelope.resource_reference
        }) {
            return if existing.envelope == envelope && existing.conversation_key == conversation_key
            {
                Err(LxmfResourcePendingOfferError::Duplicate)
            } else {
                Err(LxmfResourcePendingOfferError::ReferenceConflict)
            };
        }

        let window_start = received_at_unix.saturating_sub(LXMF_RESOURCE_PENDING_RATE_WINDOW_SECS);
        if self
            .rate_records
            .iter()
            .filter(|record| {
                record.authenticated_sender == authenticated_sender
                    && record.admitted_at_unix >= window_start
            })
            .count()
            >= LXMF_RESOURCE_PENDING_MAX_PER_PEER_WINDOW
        {
            return Err(LxmfResourcePendingOfferError::PeerRateLimited);
        }
        if self
            .rate_records
            .iter()
            .filter(|record| record.admitted_at_unix >= window_start)
            .count()
            >= LXMF_RESOURCE_PENDING_MAX_GLOBAL_PER_WINDOW
        {
            return Err(LxmfResourcePendingOfferError::GlobalRateLimited);
        }
        if self.rate_records.len() >= LXMF_RESOURCE_PENDING_RATE_MAX_ITEMS {
            return Err(LxmfResourcePendingOfferError::RateCapacity);
        }

        let accounted_bytes = encoded
            .len()
            .saturating_add(authenticated_sender.len())
            .saturating_add(conversation_key.len());
        let (peer_items, peer_bytes) =
            self.pending
                .iter()
                .fold((0usize, 0usize), |(items, bytes), offer| {
                    if offer.authenticated_sender == authenticated_sender {
                        (items + 1, bytes.saturating_add(offer.accounted_bytes))
                    } else {
                        (items, bytes)
                    }
                });
        if peer_items >= LXMF_RESOURCE_PENDING_MAX_PER_PEER_ITEMS
            || peer_bytes.saturating_add(accounted_bytes) > LXMF_RESOURCE_PENDING_MAX_PER_PEER_BYTES
        {
            return Err(LxmfResourcePendingOfferError::PeerCapacity);
        }
        let (conversation_items, conversation_bytes) =
            self.pending
                .iter()
                .fold((0usize, 0usize), |(items, bytes), offer| {
                    if offer.conversation_key == conversation_key {
                        (items + 1, bytes.saturating_add(offer.accounted_bytes))
                    } else {
                        (items, bytes)
                    }
                });
        if conversation_items >= LXMF_RESOURCE_PENDING_MAX_PER_CONVERSATION_ITEMS
            || conversation_bytes.saturating_add(accounted_bytes)
                > LXMF_RESOURCE_PENDING_MAX_PER_CONVERSATION_BYTES
        {
            return Err(LxmfResourcePendingOfferError::ConversationCapacity);
        }
        if self.pending.len() >= LXMF_RESOURCE_PENDING_MAX_ITEMS
            || self.pending_accounted_bytes.saturating_add(accounted_bytes)
                > LXMF_RESOURCE_PENDING_MAX_ACCOUNTED_BYTES
        {
            return Err(LxmfResourcePendingOfferError::GlobalCapacity);
        }

        self.pending.push_back(LxmfResourcePendingOffer {
            envelope,
            authenticated_sender: authenticated_sender.into(),
            conversation_key: conversation_key.into(),
            received_at_unix,
            accounted_bytes,
        });
        self.pending_accounted_bytes = self.pending_accounted_bytes.saturating_add(accounted_bytes);
        self.rate_records.push_back(LxmfResourcePendingRateRecord {
            authenticated_sender: authenticated_sender.into(),
            admitted_at_unix: received_at_unix,
        });
        Ok(())
    }

    /// Removes a local preview only. No network acknowledgement, rejection, or
    /// transfer cancellation is sent by this dormant owner.
    pub fn reject_local(&mut self, authenticated_sender: &str, resource_reference: &str) -> bool {
        let Some(index) = self.pending.iter().position(|offer| {
            offer.authenticated_sender == authenticated_sender
                && offer.envelope.resource_reference == resource_reference
        }) else {
            return false;
        };
        if let Some(removed) = self.pending.remove(index) {
            self.pending_accounted_bytes = self
                .pending_accounted_bytes
                .saturating_sub(removed.accounted_bytes);
            true
        } else {
            false
        }
    }

    pub fn prune_at(&mut self, now_unix: u64) -> usize {
        let mut pruned = 0usize;
        let mut index = 0usize;
        while index < self.pending.len() && pruned < LXMF_RESOURCE_PENDING_PRUNE_BATCH {
            if self.pending[index].envelope.expires_at_unix <= now_unix {
                if let Some(removed) = self.pending.remove(index) {
                    self.pending_accounted_bytes = self
                        .pending_accounted_bytes
                        .saturating_sub(removed.accounted_bytes);
                }
                pruned += 1;
            } else {
                index += 1;
            }
        }
        let rate_cutoff = now_unix.saturating_sub(LXMF_RESOURCE_PENDING_RATE_WINDOW_SECS);
        let mut index = 0usize;
        while index < self.rate_records.len() && pruned < LXMF_RESOURCE_PENDING_PRUNE_BATCH {
            if self.rate_records[index].admitted_at_unix < rate_cutoff {
                self.rate_records.remove(index);
                pruned += 1;
            } else {
                index += 1;
            }
        }
        pruned
    }

    pub fn clear_ephemeral(&mut self) {
        self.pending.clear();
        self.pending_accounted_bytes = 0;
        self.rate_records.clear();
    }
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum LxmfResourcePendingOfferError {
    #[error(transparent)]
    Envelope(#[from] LxmfResourceReferenceError),
    #[error("LXMF Resource-reference conversation key is invalid")]
    InvalidConversation,
    #[error("LXMF Resource-reference offer is a duplicate")]
    Duplicate,
    #[error("LXMF Resource-reference identifier was reused with different metadata")]
    ReferenceConflict,
    #[error("LXMF Resource-reference peer rate limit was reached")]
    PeerRateLimited,
    #[error("LXMF Resource-reference global rate limit was reached")]
    GlobalRateLimited,
    #[error("LXMF Resource-reference rate accounting is full")]
    RateCapacity,
    #[error("LXMF Resource-reference peer capacity is full")]
    PeerCapacity,
    #[error("LXMF Resource-reference conversation capacity is full")]
    ConversationCapacity,
    #[error("LXMF Resource-reference global capacity is full")]
    GlobalCapacity,
}

fn canonical_lower_hex(value: &str, chars: usize) -> bool {
    value.len() == chars
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn valid_conversation_key(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= LXMF_RESOURCE_PENDING_CONVERSATION_KEY_MAX_BYTES
        && value.bytes().all(|byte| byte.is_ascii_graphic())
}

fn valid_media_type_hint(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= LXMF_RESOURCE_REFERENCE_MEDIA_TYPE_MAX_BYTES
        && value.bytes().all(|byte| byte.is_ascii_graphic())
        && value.split_once('/').is_some_and(|(kind, subtype)| {
            !kind.is_empty() && !subtype.is_empty() && !subtype.contains('/')
        })
}

fn valid_display_name_hint(value: &str) -> bool {
    if value.is_empty()
        || value.len() > LXMF_RESOURCE_REFERENCE_DISPLAY_NAME_MAX_BYTES
        || value.trim() != value
        || value.ends_with('.')
        || value.chars().any(|character| {
            character.is_control()
                || matches!(
                    character,
                    '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*'
                )
        })
    {
        return false;
    }
    let stem = value
        .split('.')
        .next()
        .unwrap_or(value)
        .to_ascii_uppercase();
    !matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        && !is_numbered_windows_device(&stem, "COM")
        && !is_numbered_windows_device(&stem, "LPT")
}

fn is_numbered_windows_device(stem: &str, prefix: &str) -> bool {
    stem.strip_prefix(prefix)
        .is_some_and(|suffix| suffix.len() == 1 && matches!(suffix.as_bytes()[0], b'1'..=b'9'))
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: u64 = 2_000_000_000;
    const SENDER: &str = "0123456789abcdef0123456789abcdef";

    fn envelope() -> LxmfResourceReferenceEnvelope {
        LxmfResourceReferenceEnvelope {
            protocol: LXMF_RESOURCE_REFERENCE_PROTOCOL.into(),
            version: LXMF_RESOURCE_REFERENCE_VERSION,
            content_hash: "a".repeat(LXMF_RESOURCE_REFERENCE_CONTENT_HASH_HEX_CHARS),
            declared_size: 16 * 1024 * 1024,
            media_type: "application/octet-stream".into(),
            display_name: "field-report.bin".into(),
            sender_identity: SENDER.into(),
            created_at_unix: NOW,
            expires_at_unix: NOW + 3600,
            resource_reference: "fedcba9876543210fedcba9876543210".into(),
        }
    }

    fn sender(value: u128) -> String {
        format!("{value:032x}")
    }

    fn encoded_offer(sender_value: u128, reference_value: u128, created_at: u64) -> Vec<u8> {
        let mut offer = envelope();
        offer.sender_identity = sender(sender_value);
        offer.resource_reference = format!("{reference_value:032x}");
        offer.content_hash = format!("{reference_value:064x}");
        offer.created_at_unix = created_at;
        offer.expires_at_unix = created_at + LXMF_RESOURCE_REFERENCE_MAX_LIFETIME_SECS;
        offer.encode_at(created_at).expect("encode pending offer")
    }

    #[test]
    fn authenticated_envelope_round_trips_without_authorizing_transfer() {
        let offer = envelope();
        let encoded = offer.encode_at(NOW).expect("encode offer");
        let decoded =
            LxmfResourceReferenceEnvelope::decode_authenticated_at(&encoded, Some(SENDER), NOW)
                .expect("authenticated offer");
        assert_eq!(decoded, offer);
        assert_eq!(decoded.storage_file_name(), decoded.content_hash);
        assert!(!decoded.allows_automatic_transfer());
        assert!(!decoded.allows_automatic_decode());
        assert!(!decoded.allows_executable_launch());
        let debug = format!("{decoded:?}");
        assert!(!debug.contains(&decoded.resource_reference));
        assert!(debug.contains("[redacted]"));
    }

    #[test]
    fn sender_evidence_is_required_and_must_match_the_signed_claim() {
        let encoded = envelope().encode_at(NOW).expect("encode offer");
        assert_eq!(
            LxmfResourceReferenceEnvelope::decode_authenticated_at(&encoded, None, NOW),
            Err(LxmfResourceReferenceError::UnauthenticatedSender)
        );
        assert_eq!(
            LxmfResourceReferenceEnvelope::decode_authenticated_at(
                &encoded,
                Some("11111111111111111111111111111111"),
                NOW,
            ),
            Err(LxmfResourceReferenceError::SenderMismatch)
        );
    }

    #[test]
    fn metadata_size_hash_and_time_bounds_fail_closed() {
        let mut invalid = envelope();
        invalid.declared_size = LXMF_RESOURCE_REFERENCE_MAX_DECLARED_BYTES + 1;
        assert_eq!(
            invalid.validate_at(NOW),
            Err(LxmfResourceReferenceError::InvalidDeclaredSize)
        );
        invalid = envelope();
        invalid.content_hash = "A".repeat(LXMF_RESOURCE_REFERENCE_CONTENT_HASH_HEX_CHARS);
        assert_eq!(
            invalid.validate_at(NOW),
            Err(LxmfResourceReferenceError::InvalidContentHash)
        );
        invalid = envelope();
        invalid.expires_at_unix = NOW + LXMF_RESOURCE_REFERENCE_MAX_LIFETIME_SECS + 1;
        assert_eq!(
            invalid.validate_at(NOW),
            Err(LxmfResourceReferenceError::InvalidExpiry)
        );
        invalid = envelope();
        invalid.created_at_unix = NOW + LXMF_RESOURCE_REFERENCE_CLOCK_SKEW_SECS + 1;
        invalid.expires_at_unix = invalid.created_at_unix + 1;
        assert_eq!(
            invalid.validate_at(NOW),
            Err(LxmfResourceReferenceError::CreatedInFuture)
        );
    }

    #[test]
    fn display_name_is_only_a_cross_platform_hint_never_a_storage_path() {
        for rejected in [
            "../secret",
            "folder/file",
            "CON.txt",
            "LPT1",
            "trail.",
            " spaced",
        ] {
            let mut invalid = envelope();
            invalid.display_name = rejected.into();
            assert_eq!(
                invalid.validate_at(NOW),
                Err(LxmfResourceReferenceError::InvalidDisplayName),
                "accepted unsafe name {rejected:?}"
            );
        }
        let mut executable_hint = envelope();
        executable_hint.display_name = "report.exe".into();
        assert!(executable_hint.validate_at(NOW).is_ok());
        assert_eq!(
            executable_hint.storage_file_name(),
            executable_hint.content_hash
        );
        assert!(!executable_hint.allows_executable_launch());
    }

    #[test]
    fn unknown_fields_invalid_reference_and_predecode_byte_overflow_are_rejected() {
        let encoded = envelope().encode_at(NOW).expect("encode fixture");
        let mut value: serde_json::Value = serde_json::from_slice(&encoded).expect("fixture JSON");
        value["thumbnail_reference"] = serde_json::json!("not-enabled");
        assert_eq!(
            LxmfResourceReferenceEnvelope::decode_authenticated_at(
                &serde_json::to_vec(&value).expect("unknown-field fixture"),
                Some(SENDER),
                NOW,
            ),
            Err(LxmfResourceReferenceError::Malformed)
        );
        let mut invalid = envelope();
        invalid.resource_reference = "not-a-resource-hash".into();
        assert_eq!(
            invalid.validate_at(NOW),
            Err(LxmfResourceReferenceError::InvalidResourceReference)
        );
        assert_eq!(
            LxmfResourceReferenceEnvelope::decode_authenticated_at(
                &vec![b'x'; LXMF_RESOURCE_REFERENCE_MAX_ENCODED_BYTES + 1],
                Some(SENDER),
                NOW,
            ),
            Err(LxmfResourceReferenceError::TooLarge)
        );
    }

    #[test]
    fn pending_owner_creates_only_an_inert_preview_and_rejects_locally() {
        let encoded = encoded_offer(1, 1, NOW);
        let mut owner = LxmfResourcePendingOfferOwner::default();
        owner
            .admit_at(&encoded, Some(&sender(1)), "peer:one", NOW)
            .expect("admit preview");
        let preview = owner.pending().next().expect("pending preview");
        assert_eq!(preview.authenticated_sender(), sender(1));
        assert_eq!(preview.conversation_key(), "peer:one");
        assert_eq!(preview.received_at_unix(), NOW);
        assert!(!preview.allows_transfer());
        assert!(owner.reject_local(&sender(1), &format!("{:032x}", 1)));
        assert_eq!(owner.pending().count(), 0);
        assert_eq!(owner.pending_accounted_bytes(), 0);
        assert_eq!(owner.rate_record_count(), 1);
    }

    #[test]
    fn duplicate_and_conflicting_reference_reuse_fail_closed() {
        let encoded = encoded_offer(1, 1, NOW);
        let mut owner = LxmfResourcePendingOfferOwner::default();
        owner
            .admit_at(&encoded, Some(&sender(1)), "peer:one", NOW)
            .expect("admit original");
        assert_eq!(
            owner.admit_at(&encoded, Some(&sender(1)), "peer:one", NOW),
            Err(LxmfResourcePendingOfferError::Duplicate)
        );

        let mut conflicting = envelope();
        conflicting.sender_identity = sender(1);
        conflicting.resource_reference = format!("{:032x}", 1);
        conflicting.content_hash = "b".repeat(LXMF_RESOURCE_REFERENCE_CONTENT_HASH_HEX_CHARS);
        let conflicting = conflicting.encode_at(NOW).expect("encode conflict");
        assert_eq!(
            owner.admit_at(&conflicting, Some(&sender(1)), "peer:one", NOW),
            Err(LxmfResourcePendingOfferError::ReferenceConflict)
        );
        assert_eq!(
            owner.admit_at(&encoded, Some(&sender(1)), "peer:other", NOW),
            Err(LxmfResourcePendingOfferError::ReferenceConflict)
        );
    }

    #[test]
    fn rejected_offers_still_consume_bounded_peer_rate_evidence() {
        let mut owner = LxmfResourcePendingOfferOwner::default();
        for reference in 1..=LXMF_RESOURCE_PENDING_MAX_PER_PEER_WINDOW {
            let encoded = encoded_offer(1, reference as u128, NOW);
            owner
                .admit_at(&encoded, Some(&sender(1)), "peer:one", NOW)
                .expect("admit rate fixture");
            assert!(owner.reject_local(&sender(1), &format!("{reference:032x}")));
        }
        assert_eq!(owner.pending().count(), 0);
        assert_eq!(
            owner.admit_at(
                &encoded_offer(1, 100, NOW),
                Some(&sender(1)),
                "peer:one",
                NOW,
            ),
            Err(LxmfResourcePendingOfferError::PeerRateLimited)
        );
    }

    #[test]
    fn rejected_offers_cannot_bypass_the_global_rate_limit() {
        let mut owner = LxmfResourcePendingOfferOwner::default();
        for index in 0..LXMF_RESOURCE_PENDING_MAX_GLOBAL_PER_WINDOW {
            let sender_value = index as u128 + 1;
            let encoded = encoded_offer(sender_value, sender_value, NOW);
            owner
                .admit_at(
                    &encoded,
                    Some(&sender(sender_value)),
                    &format!("conversation:{index}"),
                    NOW,
                )
                .expect("admit global-rate fixture");
            assert!(owner.reject_local(&sender(sender_value), &format!("{sender_value:032x}")));
        }
        assert_eq!(
            owner.admit_at(
                &encoded_offer(1000, 1000, NOW),
                Some(&sender(1000)),
                "conversation:overflow",
                NOW,
            ),
            Err(LxmfResourcePendingOfferError::GlobalRateLimited)
        );
    }

    #[test]
    fn pending_capacity_is_bounded_per_peer_conversation_and_globally() {
        let mut peer_owner = LxmfResourcePendingOfferOwner::default();
        for index in 0..LXMF_RESOURCE_PENDING_MAX_PER_PEER_ITEMS {
            let at = NOW + index as u64 * (LXMF_RESOURCE_PENDING_RATE_WINDOW_SECS + 1);
            peer_owner
                .admit_at(
                    &encoded_offer(1, index as u128 + 1, at),
                    Some(&sender(1)),
                    &format!("conversation:{index}"),
                    at,
                )
                .expect("admit peer-capacity fixture");
        }
        let next_at = NOW
            + LXMF_RESOURCE_PENDING_MAX_PER_PEER_ITEMS as u64
                * (LXMF_RESOURCE_PENDING_RATE_WINDOW_SECS + 1);
        assert_eq!(
            peer_owner.admit_at(
                &encoded_offer(1, 100, next_at),
                Some(&sender(1)),
                "conversation:overflow",
                next_at,
            ),
            Err(LxmfResourcePendingOfferError::PeerCapacity)
        );

        let mut conversation_owner = LxmfResourcePendingOfferOwner::default();
        for index in 0..LXMF_RESOURCE_PENDING_MAX_PER_CONVERSATION_ITEMS {
            conversation_owner
                .admit_at(
                    &encoded_offer(index as u128 + 1, index as u128 + 1, NOW),
                    Some(&sender(index as u128 + 1)),
                    "shared-conversation",
                    NOW,
                )
                .expect("admit conversation-capacity fixture");
        }
        assert_eq!(
            conversation_owner.admit_at(
                &encoded_offer(100, 100, NOW),
                Some(&sender(100)),
                "shared-conversation",
                NOW,
            ),
            Err(LxmfResourcePendingOfferError::ConversationCapacity)
        );

        let mut global_owner = LxmfResourcePendingOfferOwner::default();
        for index in 0..LXMF_RESOURCE_PENDING_MAX_ITEMS {
            global_owner
                .admit_at(
                    &encoded_offer(index as u128 + 1, index as u128 + 1, NOW),
                    Some(&sender(index as u128 + 1)),
                    &format!("conversation:{index}"),
                    NOW,
                )
                .expect("admit global-capacity fixture");
        }
        assert_eq!(
            global_owner.admit_at(
                &encoded_offer(100, 100, NOW),
                Some(&sender(100)),
                "conversation:overflow",
                NOW,
            ),
            Err(LxmfResourcePendingOfferError::GlobalCapacity)
        );
    }

    #[test]
    fn peer_accounted_byte_limit_rejects_padded_metadata_without_growth() {
        let mut owner = LxmfResourcePendingOfferOwner::default();
        let mut rejected = false;
        for index in 0..LXMF_RESOURCE_PENDING_MAX_PER_PEER_ITEMS {
            let at = NOW + index as u64 * (LXMF_RESOURCE_PENDING_RATE_WINDOW_SECS + 1);
            let mut encoded = encoded_offer(1, index as u128 + 1, at);
            encoded.resize(LXMF_RESOURCE_REFERENCE_MAX_ENCODED_BYTES, b' ');
            let result = owner.admit_at(
                &encoded,
                Some(&sender(1)),
                &format!("conversation:{index}"),
                at,
            );
            if result == Err(LxmfResourcePendingOfferError::PeerCapacity) {
                rejected = true;
                break;
            }
            result.expect("admit padded offer before byte boundary");
        }
        assert!(rejected, "peer byte bound must reject padded metadata");
        assert!(owner.pending_accounted_bytes() <= LXMF_RESOURCE_PENDING_MAX_PER_PEER_BYTES);
    }

    #[test]
    fn conversation_and_global_accounted_byte_limits_reject_without_growth() {
        let mut conversation_owner = LxmfResourcePendingOfferOwner::default();
        let mut conversation_rejected = false;
        for index in 0..LXMF_RESOURCE_PENDING_MAX_PER_CONVERSATION_ITEMS {
            let sender_value = index as u128 + 1;
            let mut encoded = encoded_offer(sender_value, sender_value, NOW);
            encoded.resize(LXMF_RESOURCE_REFERENCE_MAX_ENCODED_BYTES, b' ');
            let result = conversation_owner.admit_at(
                &encoded,
                Some(&sender(sender_value)),
                "shared-conversation",
                NOW,
            );
            if result == Err(LxmfResourcePendingOfferError::ConversationCapacity) {
                conversation_rejected = true;
                break;
            }
            result.expect("admit padded conversation offer before byte boundary");
        }
        assert!(
            conversation_rejected,
            "conversation byte bound must reject padded metadata"
        );
        assert!(
            conversation_owner.pending_accounted_bytes()
                <= LXMF_RESOURCE_PENDING_MAX_PER_CONVERSATION_BYTES
        );

        let mut global_owner = LxmfResourcePendingOfferOwner::default();
        let mut global_rejected = false;
        for index in 0..LXMF_RESOURCE_PENDING_MAX_ITEMS {
            let sender_value = index as u128 + 1;
            let mut encoded = encoded_offer(sender_value, sender_value, NOW);
            encoded.resize(LXMF_RESOURCE_REFERENCE_MAX_ENCODED_BYTES, b' ');
            let result = global_owner.admit_at(
                &encoded,
                Some(&sender(sender_value)),
                &format!("conversation:{index}"),
                NOW,
            );
            if result == Err(LxmfResourcePendingOfferError::GlobalCapacity) {
                global_rejected = true;
                break;
            }
            result.expect("admit padded global offer before byte boundary");
        }
        assert!(
            global_rejected,
            "global byte bound must reject padded metadata"
        );
        assert!(
            global_owner.pending_accounted_bytes() <= LXMF_RESOURCE_PENDING_MAX_ACCOUNTED_BYTES
        );
    }

    #[test]
    fn pruning_is_incremental_and_clear_releases_all_ephemeral_state() {
        let mut owner = LxmfResourcePendingOfferOwner::default();
        for index in 0..12 {
            let sender_value = index as u128 / LXMF_RESOURCE_PENDING_MAX_PER_PEER_ITEMS as u128 + 1;
            owner
                .admit_at(
                    &encoded_offer(sender_value, index as u128 + 1, NOW),
                    Some(&sender(sender_value)),
                    &format!(
                        "conversation:{}",
                        index / LXMF_RESOURCE_PENDING_MAX_PER_CONVERSATION_ITEMS
                    ),
                    NOW,
                )
                .expect("admit prune fixture");
        }
        let prune_at = NOW + LXMF_RESOURCE_REFERENCE_MAX_LIFETIME_SECS + 1;
        assert_eq!(owner.prune_at(prune_at), LXMF_RESOURCE_PENDING_PRUNE_BATCH);
        assert!(owner.pending().count() + owner.rate_record_count() > 0);
        while owner.prune_at(prune_at) > 0 {}
        assert_eq!(owner.pending().count(), 0);
        assert_eq!(owner.pending_accounted_bytes(), 0);
        assert_eq!(owner.rate_record_count(), 0);
        owner.clear_ephemeral();
    }

    #[test]
    fn invalid_conversation_context_is_rejected_before_retention() {
        let mut owner = LxmfResourcePendingOfferOwner::default();
        assert_eq!(
            owner.admit_at(&encoded_offer(1, 1, NOW), Some(&sender(1)), "room key", NOW,),
            Err(LxmfResourcePendingOfferError::InvalidConversation)
        );
        assert_eq!(owner.pending().count(), 0);
        assert_eq!(owner.rate_record_count(), 0);
    }
}
