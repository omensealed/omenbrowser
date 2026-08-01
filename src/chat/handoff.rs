use std::collections::VecDeque;
use std::fmt;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::messaging::{MessageSummary, LXMF_SOURCE_AUTHENTICATED_FIELD};

pub const OMENCHAT_INVITE_PROTOCOL: &str = "omenchat.lxmf.invite";
pub const OMENCHAT_INVITE_VERSION: u16 = 1;
pub const OMENCHAT_RESOURCE_METADATA_PREFIX: &str = "omenchat-resource:";
pub const OMENCHAT_INVITE_MAX_ENCODED_BYTES: usize = 4 * 1024;
pub const OMENCHAT_INVITE_DESTINATION_HEX_BYTES: usize = 32;
pub const OMENCHAT_INVITE_ROOM_ID_MAX_BYTES: usize = 64;
pub const OMENCHAT_INVITE_ROOM_DISPLAY_MAX_BYTES: usize = 256;
pub const OMENCHAT_INVITE_INVITER_DISPLAY_MAX_BYTES: usize = 256;
pub const OMENCHAT_INVITE_TOKEN_MAX_BYTES: usize = 256;
pub const OMENCHAT_INVITE_INTRO_MAX_BYTES: usize = 1024;
pub const OMENCHAT_INVITE_EXPIRY_SKEW_SECS: u64 = 5 * 60;
pub const OMENCHAT_LXMF_INVITE_REPLAY_MAX_ITEMS: usize = 64;
pub const OMENCHAT_LXMF_INVITE_REPLAY_MAX_ACCOUNTED_BYTES: usize = 64 * 1024;
pub const OMENCHAT_LXMF_INVITE_REPLAY_MAX_AGE_SECS: u64 = 7 * 24 * 60 * 60;
pub const OMENCHAT_LXMF_INVITE_PRUNE_BATCH: usize = 8;
pub const OMENCHAT_LXMF_INVITE_SENDER_WINDOW_SECS: u64 = 5 * 60;
pub const OMENCHAT_LXMF_INVITE_MAX_PER_SENDER_WINDOW: usize = 4;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum OmenChatInviteRole {
    #[default]
    Guest,
    Member,
    Mod,
    Admin,
}

impl OmenChatInviteRole {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Guest => "guest",
            Self::Member => "member",
            Self::Mod => "moderator claim (not granted)",
            Self::Admin => "administrator claim (not granted)",
        }
    }
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct OmenChatInvitePayload {
    pub protocol: String,
    pub version: u16,
    pub server_destination: String,
    pub room_id: String,
    pub room_display_name: String,
    pub inviter_display_name: String,
    pub inviter_destination: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub invite_token: Option<String>,
    pub room_password_required: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at_unix: Option<u64>,
    pub requested_role: OmenChatInviteRole,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub intro_message: Option<String>,
}

impl fmt::Debug for OmenChatInvitePayload {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OmenChatInvitePayload")
            .field("protocol", &self.protocol)
            .field("version", &self.version)
            .field("server_destination", &self.server_destination)
            .field("room_id", &self.room_id)
            .field("room_display_name", &self.room_display_name)
            .field("inviter_display_name", &self.inviter_display_name)
            .field("inviter_destination", &self.inviter_destination)
            .field(
                "invite_token",
                &self.invite_token.as_ref().map(|_| "[redacted]"),
            )
            .field("room_password_required", &self.room_password_required)
            .field("expires_at_unix", &self.expires_at_unix)
            .field("requested_role", &self.requested_role)
            .field(
                "intro_message",
                &self.intro_message.as_ref().map(|_| "[redacted]"),
            )
            .finish()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OmenChatInviteReplayPolicy {
    BoundedDuplicatePresentation,
    RequiresServerTokenConsumption,
}

impl OmenChatInviteReplayPolicy {
    pub fn label(self) -> &'static str {
        match self {
            Self::BoundedDuplicatePresentation => "duplicate presentation is suppressed locally",
            Self::RequiresServerTokenConsumption => {
                "token included; single-use is unproven without server consumption"
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OmenChatInviteSenderEvidence {
    AuthenticatedMatch,
    AuthenticatedMismatch,
    AuthenticatedSenderUnavailable,
}

impl OmenChatInviteSenderEvidence {
    pub fn label(self) -> &'static str {
        match self {
            Self::AuthenticatedMatch => "authenticated LXMF sender matches the inviter claim",
            Self::AuthenticatedMismatch => {
                "authenticated LXMF sender conflicts with the inviter claim"
            }
            Self::AuthenticatedSenderUnavailable => {
                "authenticated LXMF sender evidence is unavailable"
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OmenChatLxmfInvitePreview {
    pub payload: OmenChatInvitePayload,
    pub sender_evidence: OmenChatInviteSenderEvidence,
    pub replay_policy: OmenChatInviteReplayPolicy,
    pub received_at_unix: u64,
}

impl OmenChatLxmfInvitePreview {
    pub fn allows_confirmation(&self) -> bool {
        !matches!(
            self.sender_evidence,
            OmenChatInviteSenderEvidence::AuthenticatedMismatch
        )
    }
}

#[derive(Debug, Error)]
pub enum OmenChatInvitePayloadError {
    #[error("OMENchat LXMF invitation exceeds its encoded byte limit")]
    TooLarge,
    #[error("OMENchat LXMF invitation is malformed")]
    Malformed(#[source] serde_json::Error),
    #[error("OMENchat LXMF invitation protocol or version is unsupported")]
    UnsupportedProtocol,
    #[error("OMENchat LXMF invitation contains an invalid destination")]
    InvalidDestination,
    #[error("OMENchat LXMF invitation contains an invalid room identifier")]
    InvalidRoom,
    #[error("OMENchat LXMF invitation contains invalid display text")]
    InvalidDisplay,
    #[error("OMENchat LXMF invitation token is invalid")]
    InvalidToken,
    #[error("OMENchat LXMF invitation introduction is invalid")]
    InvalidIntroduction,
    #[error("OMENchat LXMF invitation has expired")]
    Expired,
    #[error("system time is unavailable for OMENchat invitation validation")]
    Clock,
}

#[derive(Debug, Error)]
pub enum OmenChatLxmfInviteAdmissionError {
    #[error(transparent)]
    Payload(#[from] OmenChatInvitePayloadError),
    #[error("authenticated LXMF invitation sender is invalid")]
    InvalidAuthenticatedSender,
    #[error("duplicate LXMF invitation presentation was suppressed")]
    Duplicate,
    #[error("LXMF invitation presentation rate limit was reached")]
    RateLimited,
    #[error("LXMF invitation replay evidence is at its bounded capacity")]
    ReplayCapacity,
    #[error("LXMF invitation message source is not authenticated")]
    UnauthenticatedMessage,
    #[error("LXMF invitation messages cannot contain attachments")]
    UnexpectedAttachments,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct OmenChatInviteReplayRecord {
    digest: [u8; 32],
    sender_destination: [u8; OMENCHAT_INVITE_DESTINATION_HEX_BYTES],
    seen_at_unix: u64,
    accounted_bytes: usize,
}

#[derive(Clone, Debug, Default)]
pub struct OmenChatLxmfInvitePreviewOwner {
    pending: Option<OmenChatLxmfInvitePreview>,
    replay_records: VecDeque<OmenChatInviteReplayRecord>,
    replay_accounted_bytes: usize,
}

impl OmenChatLxmfInvitePreviewOwner {
    pub fn pending(&self) -> Option<&OmenChatLxmfInvitePreview> {
        self.pending.as_ref()
    }

    pub fn cancel(&mut self) -> bool {
        self.pending.take().is_some()
    }

    pub fn replay_items(&self) -> usize {
        self.replay_records.len()
    }

    pub fn replay_accounted_bytes(&self) -> usize {
        self.replay_accounted_bytes
    }

    pub fn admit(
        &mut self,
        encoded: &[u8],
        authenticated_sender: Option<&str>,
    ) -> Result<(), OmenChatLxmfInviteAdmissionError> {
        self.admit_at(encoded, authenticated_sender, current_unix_time()?)
    }

    pub fn admit_at(
        &mut self,
        encoded: &[u8],
        authenticated_sender: Option<&str>,
        now_unix: u64,
    ) -> Result<(), OmenChatLxmfInviteAdmissionError> {
        let payload = OmenChatInvitePayload::decode_at(encoded, now_unix)?;
        let authenticated_sender = authenticated_sender.map(destination_bytes).transpose()?;
        let claimed_sender = destination_bytes(&payload.inviter_destination)?;
        let sender_evidence = match authenticated_sender {
            Some(sender) if sender == claimed_sender => {
                OmenChatInviteSenderEvidence::AuthenticatedMatch
            }
            Some(_) => OmenChatInviteSenderEvidence::AuthenticatedMismatch,
            None => OmenChatInviteSenderEvidence::AuthenticatedSenderUnavailable,
        };
        let rate_sender = authenticated_sender.unwrap_or(claimed_sender);

        self.prune_expired(now_unix);
        let canonical =
            serde_json::to_vec(&payload).map_err(OmenChatInvitePayloadError::Malformed)?;
        let digest: [u8; 32] = Sha256::digest(&canonical).into();
        if self
            .replay_records
            .iter()
            .any(|record| record.digest == digest)
        {
            return Err(OmenChatLxmfInviteAdmissionError::Duplicate);
        }
        let sender_window_start = now_unix.saturating_sub(OMENCHAT_LXMF_INVITE_SENDER_WINDOW_SECS);
        if self
            .replay_records
            .iter()
            .filter(|record| {
                record.sender_destination == rate_sender
                    && record.seen_at_unix >= sender_window_start
            })
            .take(OMENCHAT_LXMF_INVITE_MAX_PER_SENDER_WINDOW)
            .count()
            >= OMENCHAT_LXMF_INVITE_MAX_PER_SENDER_WINDOW
        {
            return Err(OmenChatLxmfInviteAdmissionError::RateLimited);
        }
        if self.replay_records.len() >= OMENCHAT_LXMF_INVITE_REPLAY_MAX_ITEMS
            || self.replay_accounted_bytes.saturating_add(encoded.len())
                > OMENCHAT_LXMF_INVITE_REPLAY_MAX_ACCOUNTED_BYTES
        {
            return Err(OmenChatLxmfInviteAdmissionError::ReplayCapacity);
        }

        self.replay_records.push_back(OmenChatInviteReplayRecord {
            digest,
            sender_destination: rate_sender,
            seen_at_unix: now_unix,
            accounted_bytes: encoded.len(),
        });
        self.replay_accounted_bytes = self.replay_accounted_bytes.saturating_add(encoded.len());
        self.pending = Some(OmenChatLxmfInvitePreview {
            replay_policy: payload.replay_policy(),
            payload,
            sender_evidence,
            received_at_unix: now_unix,
        });
        Ok(())
    }

    pub fn admit_message_at(
        &mut self,
        message: &MessageSummary,
        now_unix: u64,
    ) -> Result<bool, OmenChatLxmfInviteAdmissionError> {
        if message.title != OMENCHAT_INVITE_PROTOCOL {
            return Ok(false);
        }
        if !message.attachments.is_empty() {
            return Err(OmenChatLxmfInviteAdmissionError::UnexpectedAttachments);
        }
        let sender = authenticated_lxmf_sender(message)
            .ok_or(OmenChatLxmfInviteAdmissionError::UnauthenticatedMessage)?;
        self.admit_at(message.content.as_bytes(), Some(sender), now_unix)?;
        Ok(true)
    }

    fn prune_expired(&mut self, now_unix: u64) {
        let cutoff = now_unix.saturating_sub(OMENCHAT_LXMF_INVITE_REPLAY_MAX_AGE_SECS);
        for _ in 0..OMENCHAT_LXMF_INVITE_PRUNE_BATCH {
            let Some(record) = self.replay_records.front() else {
                break;
            };
            if record.seen_at_unix >= cutoff {
                break;
            }
            if let Some(removed) = self.replay_records.pop_front() {
                self.replay_accounted_bytes = self
                    .replay_accounted_bytes
                    .saturating_sub(removed.accounted_bytes);
            }
        }
    }
}

fn destination_bytes(
    value: &str,
) -> Result<[u8; OMENCHAT_INVITE_DESTINATION_HEX_BYTES], OmenChatLxmfInviteAdmissionError> {
    if !canonical_destination(value) {
        return Err(OmenChatLxmfInviteAdmissionError::InvalidAuthenticatedSender);
    }
    let mut bytes = [0u8; OMENCHAT_INVITE_DESTINATION_HEX_BYTES];
    bytes.copy_from_slice(value.as_bytes());
    Ok(bytes)
}

pub fn authenticated_lxmf_sender(message: &MessageSummary) -> Option<&str> {
    (message.incoming
        && message
            .fields
            .get(LXMF_SOURCE_AUTHENTICATED_FIELD)
            .is_some_and(|value| value == "true")
        && canonical_destination(&message.peer_hash))
    .then_some(message.peer_hash.as_str())
}

pub fn is_lxmf_omenchat_invitation_message(message: &MessageSummary) -> bool {
    message.title == OMENCHAT_INVITE_PROTOCOL
}

impl OmenChatInvitePayload {
    pub fn new(
        server_destination: impl Into<String>,
        room_id: impl Into<String>,
        room_display_name: impl Into<String>,
        inviter_display_name: impl Into<String>,
        inviter_destination: impl Into<String>,
    ) -> Self {
        Self {
            protocol: OMENCHAT_INVITE_PROTOCOL.into(),
            version: OMENCHAT_INVITE_VERSION,
            server_destination: server_destination.into(),
            room_id: room_id.into(),
            room_display_name: room_display_name.into(),
            inviter_display_name: inviter_display_name.into(),
            inviter_destination: inviter_destination.into(),
            invite_token: None,
            room_password_required: false,
            expires_at_unix: None,
            requested_role: OmenChatInviteRole::Guest,
            intro_message: None,
        }
    }

    pub fn encode(&self) -> Result<Vec<u8>, OmenChatInvitePayloadError> {
        self.validate_at(current_unix_time()?)?;
        let encoded = serde_json::to_vec(self).map_err(OmenChatInvitePayloadError::Malformed)?;
        if encoded.len() > OMENCHAT_INVITE_MAX_ENCODED_BYTES {
            return Err(OmenChatInvitePayloadError::TooLarge);
        }
        Ok(encoded)
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, OmenChatInvitePayloadError> {
        Self::decode_at(bytes, current_unix_time()?)
    }

    pub fn decode_at(bytes: &[u8], now_unix: u64) -> Result<Self, OmenChatInvitePayloadError> {
        if bytes.len() > OMENCHAT_INVITE_MAX_ENCODED_BYTES {
            return Err(OmenChatInvitePayloadError::TooLarge);
        }
        let payload: Self =
            serde_json::from_slice(bytes).map_err(OmenChatInvitePayloadError::Malformed)?;
        payload.validate_at(now_unix)?;
        Ok(payload)
    }

    pub fn validate_at(&self, now_unix: u64) -> Result<(), OmenChatInvitePayloadError> {
        if self.protocol != OMENCHAT_INVITE_PROTOCOL || self.version != OMENCHAT_INVITE_VERSION {
            return Err(OmenChatInvitePayloadError::UnsupportedProtocol);
        }
        if !canonical_destination(&self.server_destination)
            || !canonical_destination(&self.inviter_destination)
        {
            return Err(OmenChatInvitePayloadError::InvalidDestination);
        }
        if self.room_id.is_empty()
            || self.room_id.len() > OMENCHAT_INVITE_ROOM_ID_MAX_BYTES
            || !self.room_id.bytes().all(|byte| {
                byte.is_ascii_alphanumeric() || matches!(byte, b'#' | b'-' | b'_' | b'.')
            })
        {
            return Err(OmenChatInvitePayloadError::InvalidRoom);
        }
        validate_display(
            &self.room_display_name,
            OMENCHAT_INVITE_ROOM_DISPLAY_MAX_BYTES,
        )?;
        validate_display(
            &self.inviter_display_name,
            OMENCHAT_INVITE_INVITER_DISPLAY_MAX_BYTES,
        )?;
        if self.invite_token.as_ref().is_some_and(|token| {
            token.is_empty()
                || token.len() > OMENCHAT_INVITE_TOKEN_MAX_BYTES
                || token.chars().any(char::is_control)
        }) {
            return Err(OmenChatInvitePayloadError::InvalidToken);
        }
        if self.intro_message.as_ref().is_some_and(|intro| {
            intro.is_empty()
                || intro.len() > OMENCHAT_INVITE_INTRO_MAX_BYTES
                || intro.chars().any(char::is_control)
        }) {
            return Err(OmenChatInvitePayloadError::InvalidIntroduction);
        }
        if self.expires_at_unix.is_some_and(|expires| {
            expires.saturating_add(OMENCHAT_INVITE_EXPIRY_SKEW_SECS) < now_unix
        }) {
            return Err(OmenChatInvitePayloadError::Expired);
        }
        Ok(())
    }

    pub fn replay_policy(&self) -> OmenChatInviteReplayPolicy {
        if self.invite_token.is_some() {
            OmenChatInviteReplayPolicy::RequiresServerTokenConsumption
        } else {
            OmenChatInviteReplayPolicy::BoundedDuplicatePresentation
        }
    }

    pub fn redacted_for_log(&self) -> Self {
        let mut redacted = self.clone();
        if redacted.invite_token.is_some() {
            redacted.invite_token = Some("[redacted]".into());
        }
        if redacted.intro_message.is_some() {
            redacted.intro_message = Some("[redacted]".into());
        }
        redacted
    }
}

fn current_unix_time() -> Result<u64, OmenChatInvitePayloadError> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .map_err(|_| OmenChatInvitePayloadError::Clock)
}

fn canonical_destination(value: &str) -> bool {
    value.len() == OMENCHAT_INVITE_DESTINATION_HEX_BYTES
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn validate_display(value: &str, max_bytes: usize) -> Result<(), OmenChatInvitePayloadError> {
    if value.trim().is_empty() || value.len() > max_bytes || value.chars().any(char::is_control) {
        return Err(OmenChatInvitePayloadError::InvalidDisplay);
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OmenChatResourcePurpose {
    HistoryBatch,
    UserListSnapshot,
    MediaUpload,
    MediaDownload,
    RoomExport,
    ServerNotice,
}

impl OmenChatResourcePurpose {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::HistoryBatch => "history_batch",
            Self::UserListSnapshot => "user_list_snapshot",
            Self::MediaUpload => "media_upload",
            Self::MediaDownload => "media_download",
            Self::RoomExport => "room_export",
            Self::ServerNotice => "server_notice",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "history_batch" => Some(Self::HistoryBatch),
            "user_list_snapshot" => Some(Self::UserListSnapshot),
            "media_upload" => Some(Self::MediaUpload),
            "media_download" => Some(Self::MediaDownload),
            "room_export" => Some(Self::RoomExport),
            "server_notice" => Some(Self::ServerNotice),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct OmenChatResourceMetadata {
    pub purpose: OmenChatResourcePurpose,
    pub transfer_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub room_id: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filename: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size_bytes: Option<u64>,
}

impl OmenChatResourceMetadata {
    pub fn encode(&self) -> Result<Vec<u8>, serde_json::Error> {
        let json = serde_json::to_string(self)?;
        Ok(format!("{OMENCHAT_RESOURCE_METADATA_PREFIX}{json}").into_bytes())
    }

    pub fn decode(bytes: &[u8]) -> Result<Option<Self>, serde_json::Error> {
        let Some(metadata) = std::str::from_utf8(bytes)
            .ok()
            .and_then(|value| value.strip_prefix(OMENCHAT_RESOURCE_METADATA_PREFIX))
        else {
            return Ok(None);
        };
        serde_json::from_str(metadata).map(Some)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_omenchat_invite_roundtrip() {
        let mut invite = OmenChatInvitePayload::new(
            "0123456789abcdef0123456789abcdef",
            "lobby",
            "Lobby",
            "OMENbrowser_dev",
            "4d5b6489674775b5423e8a80d9d95409",
        );
        invite.invite_token = Some("secret-token".into());
        invite.room_password_required = true;
        invite.expires_at_unix = Some(4_000_000_000);
        invite.requested_role = OmenChatInviteRole::Member;
        invite.intro_message = Some("join the room".into());

        let encoded = invite.encode().expect("encode invite");
        let decoded = OmenChatInvitePayload::decode(&encoded).expect("decode invite");

        assert_eq!(decoded, invite);
        assert_eq!(
            decoded.redacted_for_log().invite_token.as_deref(),
            Some("[redacted]")
        );
        assert_eq!(
            decoded.redacted_for_log().intro_message.as_deref(),
            Some("[redacted]")
        );
        assert_eq!(
            decoded.replay_policy(),
            OmenChatInviteReplayPolicy::RequiresServerTokenConsumption
        );
        let debug = format!("{decoded:?}");
        assert!(!debug.contains("secret-token"));
        assert!(!debug.contains("join the room"));
    }

    fn valid_invite() -> OmenChatInvitePayload {
        OmenChatInvitePayload::new(
            "0123456789abcdef0123456789abcdef",
            "lobby",
            "Lobby",
            "Inviter",
            "4d5b6489674775b5423e8a80d9d95409",
        )
    }

    fn invitation_message(invite: &OmenChatInvitePayload, authenticated: bool) -> MessageSummary {
        let mut fields = std::collections::BTreeMap::new();
        if authenticated {
            fields.insert(LXMF_SOURCE_AUTHENTICATED_FIELD.into(), "true".into());
        }
        MessageSummary {
            peer_hash: invite.inviter_destination.clone(),
            peer_label: "inviter".into(),
            title: OMENCHAT_INVITE_PROTOCOL.into(),
            content: String::from_utf8(invite.encode().expect("encode invitation message"))
                .expect("JSON UTF-8"),
            timestamp: 100.0,
            transport_method: crate::messaging::TransportMethod::Direct,
            delivered: false,
            failed: false,
            incoming: true,
            unread: true,
            message_id: Some("message-id".into()),
            fields,
            attachments: Vec::new(),
        }
    }

    #[test]
    fn invite_decode_rejects_outer_overflow_before_json_and_unknown_fields() {
        let oversized = vec![b' '; OMENCHAT_INVITE_MAX_ENCODED_BYTES + 1];
        assert!(matches!(
            OmenChatInvitePayload::decode_at(&oversized, 100),
            Err(OmenChatInvitePayloadError::TooLarge)
        ));

        let mut encoded = serde_json::to_value(valid_invite()).expect("value");
        encoded
            .as_object_mut()
            .expect("object")
            .insert("unexpected".into(), serde_json::json!(true));
        let encoded = serde_json::to_vec(&encoded).expect("encode");
        assert!(matches!(
            OmenChatInvitePayload::decode_at(&encoded, 100),
            Err(OmenChatInvitePayloadError::Malformed(_))
        ));
    }

    #[test]
    fn invite_validation_rejects_protocol_destination_text_and_expiry_boundaries() {
        let mut invite = valid_invite();
        invite.protocol = "other.protocol".into();
        assert!(matches!(
            invite.validate_at(100),
            Err(OmenChatInvitePayloadError::UnsupportedProtocol)
        ));

        let mut invite = valid_invite();
        invite.server_destination = "A".repeat(OMENCHAT_INVITE_DESTINATION_HEX_BYTES);
        assert!(matches!(
            invite.validate_at(100),
            Err(OmenChatInvitePayloadError::InvalidDestination)
        ));

        let mut invite = valid_invite();
        invite.room_id = "r".repeat(OMENCHAT_INVITE_ROOM_ID_MAX_BYTES + 1);
        assert!(matches!(
            invite.validate_at(100),
            Err(OmenChatInvitePayloadError::InvalidRoom)
        ));

        let mut invite = valid_invite();
        invite.intro_message = Some("private\nmessage".into());
        assert!(matches!(
            invite.validate_at(100),
            Err(OmenChatInvitePayloadError::InvalidIntroduction)
        ));

        let mut invite = valid_invite();
        invite.expires_at_unix = Some(100);
        assert!(invite
            .validate_at(100 + OMENCHAT_INVITE_EXPIRY_SKEW_SECS)
            .is_ok());
        assert!(matches!(
            invite.validate_at(101 + OMENCHAT_INVITE_EXPIRY_SKEW_SECS),
            Err(OmenChatInvitePayloadError::Expired)
        ));
    }

    #[test]
    fn invitation_limits_are_exact_and_tokenless_replay_is_never_called_one_time() {
        let mut invite = valid_invite();
        invite.room_id = "r".repeat(OMENCHAT_INVITE_ROOM_ID_MAX_BYTES);
        invite.room_display_name = "r".repeat(OMENCHAT_INVITE_ROOM_DISPLAY_MAX_BYTES);
        invite.inviter_display_name = "i".repeat(OMENCHAT_INVITE_INVITER_DISPLAY_MAX_BYTES);
        invite.invite_token = Some("t".repeat(OMENCHAT_INVITE_TOKEN_MAX_BYTES));
        invite.intro_message = Some("m".repeat(OMENCHAT_INVITE_INTRO_MAX_BYTES));
        assert!(invite.validate_at(100).is_ok());

        invite.invite_token = None;
        assert_eq!(
            invite.replay_policy(),
            OmenChatInviteReplayPolicy::BoundedDuplicatePresentation
        );
    }

    #[test]
    fn lxmf_preview_reports_authenticated_sender_evidence_without_authority() {
        let invite = valid_invite();
        let encoded = invite.encode().expect("encode");
        let mut owner = OmenChatLxmfInvitePreviewOwner::default();

        owner
            .admit_at(&encoded, Some(&invite.inviter_destination), 100)
            .expect("matching sender");
        let matching = owner.pending().expect("matching preview");
        assert_eq!(
            matching.sender_evidence,
            OmenChatInviteSenderEvidence::AuthenticatedMatch
        );
        assert!(matching.allows_confirmation());

        let mut mismatch_invite = invite.clone();
        mismatch_invite.room_id = "other".into();
        let mismatch_encoded = mismatch_invite.encode().expect("encode mismatch");
        owner
            .admit_at(
                &mismatch_encoded,
                Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
                101,
            )
            .expect("mismatching sender is presented as evidence");
        let mismatch = owner.pending().expect("mismatch preview");
        assert_eq!(
            mismatch.sender_evidence,
            OmenChatInviteSenderEvidence::AuthenticatedMismatch
        );
        assert!(!mismatch.allows_confirmation());
        assert!(owner.cancel());
        assert!(owner.pending().is_none());
    }

    #[test]
    fn authenticated_sender_requires_verified_live_inbound_evidence() {
        let mut message = MessageSummary {
            peer_hash: "4d5b6489674775b5423e8a80d9d95409".into(),
            peer_label: "inviter".into(),
            title: OMENCHAT_INVITE_PROTOCOL.into(),
            content: "bounded payload omitted".into(),
            timestamp: 100.0,
            transport_method: crate::messaging::TransportMethod::Direct,
            delivered: false,
            failed: false,
            incoming: true,
            unread: true,
            message_id: Some("message-id".into()),
            fields: std::collections::BTreeMap::new(),
            attachments: Vec::new(),
        };
        assert_eq!(authenticated_lxmf_sender(&message), None);

        message
            .fields
            .insert(LXMF_SOURCE_AUTHENTICATED_FIELD.into(), "true".into());
        assert_eq!(
            authenticated_lxmf_sender(&message),
            Some("4d5b6489674775b5423e8a80d9d95409")
        );

        message.incoming = false;
        assert_eq!(authenticated_lxmf_sender(&message), None);
        message.incoming = true;
        message.peer_hash = "not-a-destination".into();
        assert_eq!(authenticated_lxmf_sender(&message), None);
    }

    #[test]
    fn lxmf_message_extraction_is_explicit_authenticated_and_attachment_free() {
        let invite = valid_invite();
        let mut owner = OmenChatLxmfInvitePreviewOwner::default();
        let mut ordinary = invitation_message(&invite, true);
        ordinary.title = "ordinary message".into();
        assert!(!owner
            .admit_message_at(&ordinary, 100)
            .expect("ordinary message ignored"));
        assert!(owner.pending().is_none());

        let unauthenticated = invitation_message(&invite, false);
        assert!(matches!(
            owner.admit_message_at(&unauthenticated, 100),
            Err(OmenChatLxmfInviteAdmissionError::UnauthenticatedMessage)
        ));
        assert!(owner.pending().is_none());

        let authenticated = invitation_message(&invite, true);
        assert!(owner
            .admit_message_at(&authenticated, 100)
            .expect("authenticated invitation recognized"));
        assert_eq!(
            owner.pending().map(|preview| preview.sender_evidence),
            Some(OmenChatInviteSenderEvidence::AuthenticatedMatch)
        );

        let mut attached = invitation_message(&invite, true);
        attached.content = attached.content.replace("lobby", "other");
        attached
            .attachments
            .push(crate::messaging::AttachmentSummary {
                name: "unexpected.bin".into(),
                size: 1,
                path: None,
            });
        assert!(matches!(
            owner.admit_message_at(&attached, 101),
            Err(OmenChatLxmfInviteAdmissionError::UnexpectedAttachments)
        ));
        assert_eq!(
            owner
                .pending()
                .map(|preview| preview.payload.room_id.as_str()),
            Some("lobby")
        );
    }

    #[test]
    fn lxmf_preview_suppresses_canonical_duplicate_and_preserves_pending_on_error() {
        let mut invite = valid_invite();
        invite.invite_token = Some("private-token".into());
        invite.intro_message = Some("private introduction".into());
        let encoded = invite.encode().expect("encode");
        let duplicate_with_whitespace = serde_json::to_string_pretty(&invite)
            .expect("pretty invite")
            .into_bytes();
        let mut owner = OmenChatLxmfInvitePreviewOwner::default();
        owner
            .admit_at(&encoded, None, 100)
            .expect("first presentation");
        let original = owner.pending().cloned().expect("pending");
        let items = owner.replay_items();
        let bytes = owner.replay_accounted_bytes();

        assert!(matches!(
            owner.admit_at(&duplicate_with_whitespace, None, 101),
            Err(OmenChatLxmfInviteAdmissionError::Duplicate)
        ));
        assert_eq!(owner.pending(), Some(&original));
        assert_eq!(owner.replay_items(), items);
        assert_eq!(owner.replay_accounted_bytes(), bytes);

        let oversized = vec![b'x'; OMENCHAT_INVITE_MAX_ENCODED_BYTES + 1];
        assert!(matches!(
            owner.admit_at(&oversized, None, 102),
            Err(OmenChatLxmfInviteAdmissionError::Payload(
                OmenChatInvitePayloadError::TooLarge
            ))
        ));
        assert_eq!(owner.pending(), Some(&original));
        let debug = format!("{owner:?}");
        assert!(!debug.contains("private-token"));
        assert!(!debug.contains("private introduction"));
    }

    #[test]
    fn lxmf_preview_enforces_sender_rate_and_global_replay_bounds() {
        let sender = "4d5b6489674775b5423e8a80d9d95409";
        let mut owner = OmenChatLxmfInvitePreviewOwner::default();
        for index in 0..OMENCHAT_LXMF_INVITE_MAX_PER_SENDER_WINDOW {
            let mut invite = valid_invite();
            invite.room_id = format!("room-{index}");
            owner
                .admit_at(&invite.encode().expect("encode"), Some(sender), 100)
                .expect("within sender rate");
        }
        let mut over_rate = valid_invite();
        over_rate.room_id = "over-rate".into();
        assert!(matches!(
            owner.admit_at(&over_rate.encode().expect("encode"), Some(sender), 100),
            Err(OmenChatLxmfInviteAdmissionError::RateLimited)
        ));

        let mut bounded = OmenChatLxmfInvitePreviewOwner::default();
        for index in 0..OMENCHAT_LXMF_INVITE_REPLAY_MAX_ITEMS {
            let destination = format!("{index:032x}");
            let mut invite = valid_invite();
            invite.room_id = format!("room-{index}");
            invite.inviter_destination = destination.clone();
            bounded
                .admit_at(
                    &invite.encode().expect("encode bounded"),
                    Some(&destination),
                    1,
                )
                .expect("within global bound");
        }
        assert_eq!(
            bounded.replay_items(),
            OMENCHAT_LXMF_INVITE_REPLAY_MAX_ITEMS
        );
        assert!(
            bounded.replay_accounted_bytes() <= OMENCHAT_LXMF_INVITE_REPLAY_MAX_ACCOUNTED_BYTES
        );

        let mut full = valid_invite();
        full.room_id = "full".into();
        full.inviter_destination = "ffffffffffffffffffffffffffffffff".into();
        assert!(matches!(
            bounded.admit_at(
                &full.encode().expect("encode full"),
                Some(&full.inviter_destination),
                1,
            ),
            Err(OmenChatLxmfInviteAdmissionError::ReplayCapacity)
        ));

        bounded
            .admit_at(
                &full.encode().expect("encode after expiry"),
                Some(&full.inviter_destination),
                OMENCHAT_LXMF_INVITE_REPLAY_MAX_AGE_SECS + 2,
            )
            .expect("incremental expiry pruning creates bounded room");
        assert_eq!(
            bounded.replay_items(),
            OMENCHAT_LXMF_INVITE_REPLAY_MAX_ITEMS - OMENCHAT_LXMF_INVITE_PRUNE_BATCH + 1
        );

        let mut byte_bounded = OmenChatLxmfInvitePreviewOwner::default();
        let mut byte_capacity_rejected = false;
        for index in 0..OMENCHAT_LXMF_INVITE_REPLAY_MAX_ITEMS {
            let destination = format!("{index:032x}");
            let mut invite = valid_invite();
            invite.room_id = format!("large-{index}");
            invite.inviter_destination = destination.clone();
            invite.intro_message = Some("m".repeat(OMENCHAT_INVITE_INTRO_MAX_BYTES));
            match byte_bounded.admit_at(
                &invite.encode().expect("encode byte-bounded"),
                Some(&destination),
                1,
            ) {
                Ok(()) => {}
                Err(OmenChatLxmfInviteAdmissionError::ReplayCapacity) => {
                    byte_capacity_rejected = true;
                    break;
                }
                Err(error) => panic!("unexpected byte-bound admission error: {error}"),
            }
        }
        assert!(byte_capacity_rejected);
        assert!(byte_bounded.replay_items() < OMENCHAT_LXMF_INVITE_REPLAY_MAX_ITEMS);
        assert!(
            byte_bounded.replay_accounted_bytes()
                <= OMENCHAT_LXMF_INVITE_REPLAY_MAX_ACCOUNTED_BYTES
        );
    }

    #[test]
    fn test_omenchat_resource_metadata_roundtrip() {
        let metadata = OmenChatResourceMetadata {
            purpose: OmenChatResourcePurpose::MediaUpload,
            transfer_id: "upload-1".into(),
            room_id: Some(1),
            cursor: None,
            filename: Some("image.gif".into()),
            content_type: Some("image/gif".into()),
            size_bytes: Some(42_000),
        };

        let encoded = metadata.encode().expect("encode metadata");
        let decoded = OmenChatResourceMetadata::decode(&encoded)
            .expect("decode metadata")
            .expect("metadata prefix");

        assert_eq!(decoded, metadata);
        assert_eq!(
            OmenChatResourcePurpose::parse(OmenChatResourcePurpose::RoomExport.as_str()),
            Some(OmenChatResourcePurpose::RoomExport)
        );
        assert_eq!(
            OmenChatResourceMetadata::decode(b"not-omenchat").expect("ignore non-metadata"),
            None
        );
    }
}
