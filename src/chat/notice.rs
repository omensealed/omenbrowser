use std::collections::VecDeque;

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const OMENCHAT_LXMF_NOTICE_PROTOCOL: &str = "omenchat.lxmf.notice";
pub const OMENCHAT_LXMF_NOTICE_VERSION: u16 = 1;
pub const OMENCHAT_LXMF_NOTICES_CAPABILITY: &str = "omenchat-lxmf-notices-v1";
pub const OMENCHAT_LXMF_NOTICE_MAX_ENCODED_BYTES: usize = 1024;
pub const OMENCHAT_LXMF_NOTICE_ID_BYTES: usize = 32;
pub const OMENCHAT_LXMF_NOTICE_DESTINATION_BYTES: usize = 32;
pub const OMENCHAT_LXMF_NOTICE_CLOCK_SKEW_SECS: u64 = 5 * 60;
pub const OMENCHAT_LXMF_NOTICE_MAX_LIFETIME_SECS: u64 = 7 * 24 * 60 * 60;
pub const OMENCHAT_LXMF_NOTICE_MAX_ACTIVITY_COUNT: u16 = 1000;
pub const OMENCHAT_LXMF_NOTICE_RETAINED_MAX_ITEMS: usize = 128;
pub const OMENCHAT_LXMF_NOTICE_RETAINED_MAX_ACCOUNTED_BYTES: usize = 64 * 1024;
pub const OMENCHAT_LXMF_NOTICE_SENDER_WINDOW_SECS: u64 = 10 * 60;
pub const OMENCHAT_LXMF_NOTICE_MAX_PER_SENDER_WINDOW: usize = 8;
pub const OMENCHAT_LXMF_NOTICE_MAX_GLOBAL_PER_WINDOW: usize = 64;
pub const OMENCHAT_LXMF_NOTICE_RATE_RECORD_MAX_ITEMS: usize = 512;
pub const OMENCHAT_LXMF_NOTICE_PRUNE_BATCH: usize = 8;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OmenChatLxmfNoticeKind {
    OfflineMention,
    DirectedModeration,
    PlannedMaintenance,
    FollowedRoomSummary,
}

impl OmenChatLxmfNoticeKind {
    const fn index(self) -> usize {
        match self {
            Self::OfflineMention => 0,
            Self::DirectedModeration => 1,
            Self::PlannedMaintenance => 2,
            Self::FollowedRoomSummary => 3,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct OmenChatLxmfNotice {
    pub protocol: String,
    pub version: u16,
    pub notice_id: String,
    pub kind: OmenChatLxmfNoticeKind,
    pub server_destination: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub room_id: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub event_id: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub activity_count: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub maintenance_at_unix: Option<u64>,
    pub created_at_unix: u64,
    pub expires_at_unix: u64,
}

impl OmenChatLxmfNotice {
    pub fn encode_at(&self, now_unix: u64) -> Result<Vec<u8>, OmenChatLxmfNoticeError> {
        self.validate_at(now_unix)?;
        let encoded = serde_json::to_vec(self).map_err(|_| OmenChatLxmfNoticeError::Malformed)?;
        if encoded.len() > OMENCHAT_LXMF_NOTICE_MAX_ENCODED_BYTES {
            return Err(OmenChatLxmfNoticeError::TooLarge);
        }
        Ok(encoded)
    }

    pub fn decode_at(encoded: &[u8], now_unix: u64) -> Result<Self, OmenChatLxmfNoticeError> {
        if encoded.len() > OMENCHAT_LXMF_NOTICE_MAX_ENCODED_BYTES {
            return Err(OmenChatLxmfNoticeError::TooLarge);
        }
        let notice = serde_json::from_slice::<Self>(encoded)
            .map_err(|_| OmenChatLxmfNoticeError::Malformed)?;
        notice.validate_at(now_unix)?;
        Ok(notice)
    }

    pub fn validate_at(&self, now_unix: u64) -> Result<(), OmenChatLxmfNoticeError> {
        if self.protocol != OMENCHAT_LXMF_NOTICE_PROTOCOL
            || self.version != OMENCHAT_LXMF_NOTICE_VERSION
        {
            return Err(OmenChatLxmfNoticeError::UnsupportedProtocol);
        }
        if !canonical_hex(self.notice_id.as_str(), OMENCHAT_LXMF_NOTICE_ID_BYTES) {
            return Err(OmenChatLxmfNoticeError::InvalidNoticeId);
        }
        if !canonical_hex(
            self.server_destination.as_str(),
            OMENCHAT_LXMF_NOTICE_DESTINATION_BYTES,
        ) {
            return Err(OmenChatLxmfNoticeError::InvalidDestination);
        }
        if self.created_at_unix > now_unix.saturating_add(OMENCHAT_LXMF_NOTICE_CLOCK_SKEW_SECS) {
            return Err(OmenChatLxmfNoticeError::CreatedInFuture);
        }
        if self.expires_at_unix <= self.created_at_unix
            || self.expires_at_unix.saturating_sub(self.created_at_unix)
                > OMENCHAT_LXMF_NOTICE_MAX_LIFETIME_SECS
        {
            return Err(OmenChatLxmfNoticeError::InvalidExpiry);
        }
        if self
            .expires_at_unix
            .saturating_add(OMENCHAT_LXMF_NOTICE_CLOCK_SKEW_SECS)
            < now_unix
        {
            return Err(OmenChatLxmfNoticeError::Expired);
        }
        self.validate_kind_shape()
    }

    pub fn deduplication_key(&self) -> &str {
        self.notice_id.as_str()
    }

    fn validate_kind_shape(&self) -> Result<(), OmenChatLxmfNoticeError> {
        let room_and_event = self.room_id.is_some_and(|value| value > 0)
            && self.event_id.is_some_and(|value| value > 0);
        let valid = match self.kind {
            OmenChatLxmfNoticeKind::OfflineMention | OmenChatLxmfNoticeKind::DirectedModeration => {
                room_and_event
                    && self.activity_count.is_none()
                    && self.maintenance_at_unix.is_none()
            }
            OmenChatLxmfNoticeKind::PlannedMaintenance => {
                self.room_id.is_none()
                    && self.event_id.is_none()
                    && self.activity_count.is_none()
                    && self.maintenance_at_unix.is_some_and(|value| {
                        value >= self.created_at_unix && value <= self.expires_at_unix
                    })
            }
            OmenChatLxmfNoticeKind::FollowedRoomSummary => {
                room_and_event
                    && self.activity_count.is_some_and(|value| {
                        (1..=OMENCHAT_LXMF_NOTICE_MAX_ACTIVITY_COUNT).contains(&value)
                    })
                    && self.maintenance_at_unix.is_none()
            }
        };
        if valid {
            Ok(())
        } else {
            Err(OmenChatLxmfNoticeError::InvalidKindShape)
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OmenChatLxmfNoticeAdmissionOutcome {
    Added,
    CoalescedRoomSummary,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OmenChatLxmfRetainedNotice {
    notice: OmenChatLxmfNotice,
    authenticated_sender: String,
    received_at_unix: u64,
    accounted_bytes: usize,
}

impl OmenChatLxmfRetainedNotice {
    pub fn notice(&self) -> &OmenChatLxmfNotice {
        &self.notice
    }

    pub fn authenticated_sender(&self) -> &str {
        self.authenticated_sender.as_str()
    }

    pub fn received_at_unix(&self) -> u64 {
        self.received_at_unix
    }

    pub const fn allows_automatic_action(&self) -> bool {
        false
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct OmenChatLxmfNoticeRateRecord {
    authenticated_sender: String,
    admitted_at_unix: u64,
}

#[derive(Clone, Debug, Default)]
pub struct OmenChatLxmfNoticeAdmissionOwner {
    enabled_kinds: [bool; 4],
    retained: VecDeque<OmenChatLxmfRetainedNotice>,
    retained_accounted_bytes: usize,
    rate_records: VecDeque<OmenChatLxmfNoticeRateRecord>,
}

impl OmenChatLxmfNoticeAdmissionOwner {
    pub fn set_enabled(&mut self, kind: OmenChatLxmfNoticeKind, enabled: bool) {
        self.enabled_kinds[kind.index()] = enabled;
    }

    pub fn is_enabled(&self, kind: OmenChatLxmfNoticeKind) -> bool {
        self.enabled_kinds[kind.index()]
    }

    pub fn retained(&self) -> impl Iterator<Item = &OmenChatLxmfRetainedNotice> {
        self.retained.iter()
    }

    pub fn retained_accounted_bytes(&self) -> usize {
        self.retained_accounted_bytes
    }

    pub fn rate_record_count(&self) -> usize {
        self.rate_records.len()
    }

    pub fn admit_at(
        &mut self,
        encoded: &[u8],
        authenticated_sender: Option<&str>,
        received_at_unix: u64,
    ) -> Result<OmenChatLxmfNoticeAdmissionOutcome, OmenChatLxmfNoticeAdmissionError> {
        self.prune_at(received_at_unix);
        let authenticated_sender = authenticated_sender
            .filter(|value| canonical_hex(value, OMENCHAT_LXMF_NOTICE_DESTINATION_BYTES))
            .ok_or(OmenChatLxmfNoticeAdmissionError::UnauthenticatedSender)?;
        let notice = OmenChatLxmfNotice::decode_at(encoded, received_at_unix)?;
        if notice.expires_at_unix <= received_at_unix {
            return Err(OmenChatLxmfNoticeError::Expired.into());
        }
        if !self.is_enabled(notice.kind) {
            return Err(OmenChatLxmfNoticeAdmissionError::KindDisabled);
        }
        if self.retained.iter().any(|retained| {
            retained.authenticated_sender == authenticated_sender
                && retained.notice.notice_id == notice.notice_id
        }) {
            return Err(OmenChatLxmfNoticeAdmissionError::Duplicate);
        }
        let window_start = received_at_unix.saturating_sub(OMENCHAT_LXMF_NOTICE_SENDER_WINDOW_SECS);
        if self
            .rate_records
            .iter()
            .filter(|record| {
                record.authenticated_sender == authenticated_sender
                    && record.admitted_at_unix >= window_start
            })
            .count()
            >= OMENCHAT_LXMF_NOTICE_MAX_PER_SENDER_WINDOW
        {
            return Err(OmenChatLxmfNoticeAdmissionError::RateLimited);
        }
        if self
            .rate_records
            .iter()
            .filter(|record| record.admitted_at_unix >= window_start)
            .count()
            >= OMENCHAT_LXMF_NOTICE_MAX_GLOBAL_PER_WINDOW
        {
            return Err(OmenChatLxmfNoticeAdmissionError::GlobalRateLimited);
        }
        if self.rate_records.len() >= OMENCHAT_LXMF_NOTICE_RATE_RECORD_MAX_ITEMS {
            return Err(OmenChatLxmfNoticeAdmissionError::RateCapacity);
        }

        let coalesce_index = (notice.kind == OmenChatLxmfNoticeKind::FollowedRoomSummary)
            .then(|| {
                self.retained.iter().position(|retained| {
                    retained.authenticated_sender == authenticated_sender
                        && retained.notice.kind == OmenChatLxmfNoticeKind::FollowedRoomSummary
                        && retained.notice.server_destination == notice.server_destination
                        && retained.notice.room_id == notice.room_id
                })
            })
            .flatten();
        if let Some(index) = coalesce_index {
            let prior_event = self.retained[index].notice.event_id.unwrap_or(0);
            if notice.event_id.unwrap_or(0) <= prior_event {
                return Err(OmenChatLxmfNoticeAdmissionError::StaleRoomSummary);
            }
        }

        let accounted_bytes = encoded.len().saturating_add(authenticated_sender.len());
        let replaced_bytes = coalesce_index.map_or(0, |index| self.retained[index].accounted_bytes);
        let projected_items = self.retained.len() + usize::from(coalesce_index.is_none());
        let projected_bytes = self
            .retained_accounted_bytes
            .saturating_sub(replaced_bytes)
            .saturating_add(accounted_bytes);
        if projected_items > OMENCHAT_LXMF_NOTICE_RETAINED_MAX_ITEMS
            || projected_bytes > OMENCHAT_LXMF_NOTICE_RETAINED_MAX_ACCOUNTED_BYTES
        {
            return Err(OmenChatLxmfNoticeAdmissionError::RetentionCapacity);
        }

        let retained = OmenChatLxmfRetainedNotice {
            notice,
            authenticated_sender: authenticated_sender.into(),
            received_at_unix,
            accounted_bytes,
        };
        let outcome = if let Some(index) = coalesce_index {
            self.retained[index] = retained;
            OmenChatLxmfNoticeAdmissionOutcome::CoalescedRoomSummary
        } else {
            self.retained.push_back(retained);
            OmenChatLxmfNoticeAdmissionOutcome::Added
        };
        self.retained_accounted_bytes = projected_bytes;
        self.rate_records.push_back(OmenChatLxmfNoticeRateRecord {
            authenticated_sender: authenticated_sender.into(),
            admitted_at_unix: received_at_unix,
        });
        Ok(outcome)
    }

    pub fn prune_at(&mut self, now_unix: u64) -> usize {
        let mut pruned = 0usize;
        let mut index = 0usize;
        while index < self.retained.len() && pruned < OMENCHAT_LXMF_NOTICE_PRUNE_BATCH {
            if self.retained[index].notice.expires_at_unix <= now_unix {
                if let Some(removed) = self.retained.remove(index) {
                    self.retained_accounted_bytes = self
                        .retained_accounted_bytes
                        .saturating_sub(removed.accounted_bytes);
                }
                pruned += 1;
            } else {
                index += 1;
            }
        }
        let rate_cutoff = now_unix.saturating_sub(OMENCHAT_LXMF_NOTICE_SENDER_WINDOW_SECS);
        let mut index = 0usize;
        while index < self.rate_records.len() && pruned < OMENCHAT_LXMF_NOTICE_PRUNE_BATCH {
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
        self.retained.clear();
        self.retained_accounted_bytes = 0;
        self.rate_records.clear();
    }
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum OmenChatLxmfNoticeError {
    #[error("OMENchat LXMF notice exceeds its encoded byte limit")]
    TooLarge,
    #[error("OMENchat LXMF notice is malformed")]
    Malformed,
    #[error("OMENchat LXMF notice protocol or version is unsupported")]
    UnsupportedProtocol,
    #[error("OMENchat LXMF notice identifier is invalid")]
    InvalidNoticeId,
    #[error("OMENchat LXMF notice server destination is invalid")]
    InvalidDestination,
    #[error("OMENchat LXMF notice creation time is too far in the future")]
    CreatedInFuture,
    #[error("OMENchat LXMF notice expiry is invalid")]
    InvalidExpiry,
    #[error("OMENchat LXMF notice has expired")]
    Expired,
    #[error("OMENchat LXMF notice fields do not match its kind")]
    InvalidKindShape,
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum OmenChatLxmfNoticeAdmissionError {
    #[error(transparent)]
    Notice(#[from] OmenChatLxmfNoticeError),
    #[error("OMENchat LXMF notice sender is not authenticated")]
    UnauthenticatedSender,
    #[error("OMENchat LXMF notice kind is disabled")]
    KindDisabled,
    #[error("OMENchat LXMF notice is a duplicate")]
    Duplicate,
    #[error("OMENchat LXMF notice sender rate limit was reached")]
    RateLimited,
    #[error("OMENchat LXMF notice global rate limit was reached")]
    GlobalRateLimited,
    #[error("OMENchat LXMF notice rate accounting is full")]
    RateCapacity,
    #[error("OMENchat LXMF notice retention capacity is full")]
    RetentionCapacity,
    #[error("OMENchat LXMF room summary is stale")]
    StaleRoomSummary,
}

fn canonical_hex(value: &str, bytes: usize) -> bool {
    value.len() == bytes
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: u64 = 2_000_000_000;
    const DESTINATION: &str = "0123456789abcdef0123456789abcdef";
    const NOTICE_ID: &str = "fedcba9876543210fedcba9876543210";

    fn mention() -> OmenChatLxmfNotice {
        OmenChatLxmfNotice {
            protocol: OMENCHAT_LXMF_NOTICE_PROTOCOL.into(),
            version: OMENCHAT_LXMF_NOTICE_VERSION,
            notice_id: NOTICE_ID.into(),
            kind: OmenChatLxmfNoticeKind::OfflineMention,
            server_destination: DESTINATION.into(),
            room_id: Some(7),
            event_id: Some(42),
            activity_count: None,
            maintenance_at_unix: None,
            created_at_unix: NOW,
            expires_at_unix: NOW + 3600,
        }
    }

    fn notice_id(value: u128) -> String {
        format!("{value:032x}")
    }

    fn sender(value: u128) -> String {
        format!("{value:032x}")
    }

    fn encoded_mention(id: u128, created_at_unix: u64) -> Vec<u8> {
        let mut notice = mention();
        notice.notice_id = notice_id(id);
        notice.created_at_unix = created_at_unix;
        notice.expires_at_unix = created_at_unix + OMENCHAT_LXMF_NOTICE_MAX_LIFETIME_SECS;
        notice
            .encode_at(created_at_unix)
            .expect("encode admission fixture")
    }

    #[test]
    fn bounded_notice_round_trip_contains_no_room_history_or_free_text() {
        let notice = mention();
        let encoded = notice.encode_at(NOW).expect("encode notice");
        assert!(encoded.len() <= OMENCHAT_LXMF_NOTICE_MAX_ENCODED_BYTES);
        assert_eq!(
            OmenChatLxmfNotice::decode_at(&encoded, NOW).expect("decode notice"),
            notice
        );
        let value: serde_json::Value = serde_json::from_slice(&encoded).expect("notice JSON");
        for forbidden in [
            "body",
            "content",
            "message",
            "history",
            "attachment",
            "token",
            "role",
            "display_name",
        ] {
            assert!(
                value.get(forbidden).is_none(),
                "forbidden field {forbidden}"
            );
        }
        assert_eq!(notice.deduplication_key(), NOTICE_ID);
    }

    #[test]
    fn notice_kinds_require_only_their_minimal_pointer_fields() {
        let mut moderation = mention();
        moderation.kind = OmenChatLxmfNoticeKind::DirectedModeration;
        assert!(moderation.validate_at(NOW).is_ok());

        let mut maintenance = mention();
        maintenance.kind = OmenChatLxmfNoticeKind::PlannedMaintenance;
        maintenance.room_id = None;
        maintenance.event_id = None;
        maintenance.maintenance_at_unix = Some(NOW + 1800);
        assert!(maintenance.validate_at(NOW).is_ok());

        let mut summary = mention();
        summary.kind = OmenChatLxmfNoticeKind::FollowedRoomSummary;
        summary.activity_count = Some(10);
        assert!(summary.validate_at(NOW).is_ok());

        summary.activity_count = Some(0);
        assert_eq!(
            summary.validate_at(NOW),
            Err(OmenChatLxmfNoticeError::InvalidKindShape)
        );
        summary.activity_count = Some(OMENCHAT_LXMF_NOTICE_MAX_ACTIVITY_COUNT + 1);
        assert_eq!(
            summary.validate_at(NOW),
            Err(OmenChatLxmfNoticeError::InvalidKindShape)
        );
    }

    #[test]
    fn notice_rejects_unknown_fields_noncanonical_ids_and_time_abuse() {
        let encoded = mention().encode_at(NOW).expect("encode fixture");
        let mut value: serde_json::Value = serde_json::from_slice(&encoded).expect("fixture JSON");
        value["unexpected"] = serde_json::json!(true);
        assert_eq!(
            OmenChatLxmfNotice::decode_at(
                &serde_json::to_vec(&value).expect("unknown field fixture"),
                NOW,
            ),
            Err(OmenChatLxmfNoticeError::Malformed)
        );

        let mut invalid = mention();
        invalid.notice_id = NOTICE_ID.to_ascii_uppercase();
        assert_eq!(
            invalid.validate_at(NOW),
            Err(OmenChatLxmfNoticeError::InvalidNoticeId)
        );
        invalid = mention();
        invalid.created_at_unix = NOW + OMENCHAT_LXMF_NOTICE_CLOCK_SKEW_SECS + 1;
        assert_eq!(
            invalid.validate_at(NOW),
            Err(OmenChatLxmfNoticeError::CreatedInFuture)
        );
        invalid = mention();
        invalid.expires_at_unix = NOW + OMENCHAT_LXMF_NOTICE_MAX_LIFETIME_SECS + 1;
        assert_eq!(
            invalid.validate_at(NOW),
            Err(OmenChatLxmfNoticeError::InvalidExpiry)
        );
        invalid = mention();
        invalid.expires_at_unix = NOW - OMENCHAT_LXMF_NOTICE_CLOCK_SKEW_SECS - 1;
        invalid.created_at_unix = invalid.expires_at_unix - 1;
        assert_eq!(
            invalid.validate_at(NOW),
            Err(OmenChatLxmfNoticeError::Expired)
        );
    }

    #[test]
    fn notice_total_byte_cap_is_checked_before_decode() {
        assert_eq!(
            OmenChatLxmfNotice::decode_at(
                &vec![b'x'; OMENCHAT_LXMF_NOTICE_MAX_ENCODED_BYTES + 1],
                NOW,
            ),
            Err(OmenChatLxmfNoticeError::TooLarge)
        );
    }

    #[test]
    fn admission_requires_authenticated_sender_and_explicit_kind_opt_in() {
        let encoded = mention().encode_at(NOW).expect("encode fixture");
        let mut owner = OmenChatLxmfNoticeAdmissionOwner::default();

        assert_eq!(
            owner.admit_at(&encoded, None, NOW),
            Err(OmenChatLxmfNoticeAdmissionError::UnauthenticatedSender)
        );
        assert_eq!(
            owner.admit_at(&encoded, Some(DESTINATION), NOW),
            Err(OmenChatLxmfNoticeAdmissionError::KindDisabled)
        );

        owner.set_enabled(OmenChatLxmfNoticeKind::OfflineMention, true);
        assert_eq!(
            owner.admit_at(&encoded, Some(DESTINATION), NOW),
            Ok(OmenChatLxmfNoticeAdmissionOutcome::Added)
        );
        let retained = owner.retained().next().expect("retained notice");
        assert_eq!(retained.authenticated_sender(), DESTINATION);
        assert_eq!(retained.received_at_unix(), NOW);
        assert!(!retained.allows_automatic_action());
    }

    #[test]
    fn duplicate_identity_is_scoped_to_the_authenticated_sender() {
        let encoded = mention().encode_at(NOW).expect("encode fixture");
        let mut owner = OmenChatLxmfNoticeAdmissionOwner::default();
        owner.set_enabled(OmenChatLxmfNoticeKind::OfflineMention, true);

        assert_eq!(
            owner.admit_at(&encoded, Some(&sender(1)), NOW),
            Ok(OmenChatLxmfNoticeAdmissionOutcome::Added)
        );
        assert_eq!(
            owner.admit_at(&encoded, Some(&sender(1)), NOW),
            Err(OmenChatLxmfNoticeAdmissionError::Duplicate)
        );
        assert_eq!(
            owner.admit_at(&encoded, Some(&sender(2)), NOW),
            Ok(OmenChatLxmfNoticeAdmissionOutcome::Added)
        );
        assert_eq!(owner.retained().count(), 2);
    }

    #[test]
    fn admission_enforces_sender_and_global_window_limits() {
        let mut owner = OmenChatLxmfNoticeAdmissionOwner::default();
        owner.set_enabled(OmenChatLxmfNoticeKind::OfflineMention, true);
        for id in 1..=OMENCHAT_LXMF_NOTICE_MAX_PER_SENDER_WINDOW {
            assert!(owner
                .admit_at(&encoded_mention(id as u128, NOW), Some(&sender(1)), NOW)
                .is_ok());
        }
        assert_eq!(
            owner.admit_at(&encoded_mention(100, NOW), Some(&sender(1)), NOW),
            Err(OmenChatLxmfNoticeAdmissionError::RateLimited)
        );

        let mut global = OmenChatLxmfNoticeAdmissionOwner::default();
        global.set_enabled(OmenChatLxmfNoticeKind::OfflineMention, true);
        for index in 0..OMENCHAT_LXMF_NOTICE_MAX_GLOBAL_PER_WINDOW {
            let sender_index = index / OMENCHAT_LXMF_NOTICE_MAX_PER_SENDER_WINDOW + 1;
            assert!(global
                .admit_at(
                    &encoded_mention(index as u128 + 1, NOW),
                    Some(&sender(sender_index as u128)),
                    NOW,
                )
                .is_ok());
        }
        assert_eq!(
            global.admit_at(&encoded_mention(1000, NOW), Some(&sender(99)), NOW),
            Err(OmenChatLxmfNoticeAdmissionError::GlobalRateLimited)
        );
    }

    #[test]
    fn newer_room_summary_coalesces_without_inventing_activity() {
        let mut first = mention();
        first.kind = OmenChatLxmfNoticeKind::FollowedRoomSummary;
        first.activity_count = Some(5);
        let mut newer = first.clone();
        newer.notice_id = notice_id(2);
        newer.event_id = Some(43);
        newer.activity_count = Some(2);
        let mut stale = newer.clone();
        stale.notice_id = notice_id(3);
        stale.event_id = Some(42);
        let mut owner = OmenChatLxmfNoticeAdmissionOwner::default();
        owner.set_enabled(OmenChatLxmfNoticeKind::FollowedRoomSummary, true);

        assert_eq!(
            owner.admit_at(&first.encode_at(NOW).unwrap(), Some(DESTINATION), NOW),
            Ok(OmenChatLxmfNoticeAdmissionOutcome::Added)
        );
        assert_eq!(
            owner.admit_at(&newer.encode_at(NOW).unwrap(), Some(DESTINATION), NOW),
            Ok(OmenChatLxmfNoticeAdmissionOutcome::CoalescedRoomSummary)
        );
        let retained = owner.retained().next().expect("coalesced summary");
        assert_eq!(owner.retained().count(), 1);
        assert_eq!(retained.notice().event_id, Some(43));
        assert_eq!(retained.notice().activity_count, Some(2));
        assert_eq!(
            owner.admit_at(&stale.encode_at(NOW).unwrap(), Some(DESTINATION), NOW),
            Err(OmenChatLxmfNoticeAdmissionError::StaleRoomSummary)
        );
    }

    #[test]
    fn retention_is_bounded_by_items_and_accounted_bytes() {
        let mut item_owner = OmenChatLxmfNoticeAdmissionOwner::default();
        item_owner.set_enabled(OmenChatLxmfNoticeKind::OfflineMention, true);
        for index in 0..OMENCHAT_LXMF_NOTICE_RETAINED_MAX_ITEMS {
            let window = index / OMENCHAT_LXMF_NOTICE_MAX_GLOBAL_PER_WINDOW;
            let admitted_at = NOW + (window as u64 * (OMENCHAT_LXMF_NOTICE_SENDER_WINDOW_SECS + 1));
            let sender_index = index / OMENCHAT_LXMF_NOTICE_MAX_PER_SENDER_WINDOW + 1;
            assert!(item_owner
                .admit_at(
                    &encoded_mention(index as u128 + 1, admitted_at),
                    Some(&sender(sender_index as u128)),
                    admitted_at,
                )
                .is_ok());
        }
        let next_at = NOW + 2 * (OMENCHAT_LXMF_NOTICE_SENDER_WINDOW_SECS + 1);
        assert_eq!(
            item_owner.admit_at(
                &encoded_mention(1000, next_at),
                Some(&sender(1000)),
                next_at,
            ),
            Err(OmenChatLxmfNoticeAdmissionError::RetentionCapacity)
        );

        let mut byte_owner = OmenChatLxmfNoticeAdmissionOwner::default();
        byte_owner.set_enabled(OmenChatLxmfNoticeKind::OfflineMention, true);
        let mut rejected = false;
        for index in 0..OMENCHAT_LXMF_NOTICE_RETAINED_MAX_ITEMS {
            let mut encoded = encoded_mention(index as u128 + 1, NOW);
            encoded.resize(OMENCHAT_LXMF_NOTICE_MAX_ENCODED_BYTES, b' ');
            let result = byte_owner.admit_at(&encoded, Some(&sender(index as u128 + 1)), NOW);
            if result == Err(OmenChatLxmfNoticeAdmissionError::RetentionCapacity) {
                rejected = true;
                break;
            }
        }
        assert!(
            rejected,
            "accounted-byte limit must reject before item limit"
        );
        assert!(
            byte_owner.retained_accounted_bytes()
                <= OMENCHAT_LXMF_NOTICE_RETAINED_MAX_ACCOUNTED_BYTES
        );
    }

    #[test]
    fn pruning_is_incremental_and_shutdown_clear_preserves_preferences() {
        let mut owner = OmenChatLxmfNoticeAdmissionOwner::default();
        owner.set_enabled(OmenChatLxmfNoticeKind::OfflineMention, true);
        for index in 0..12 {
            let sender_index = index / OMENCHAT_LXMF_NOTICE_MAX_PER_SENDER_WINDOW + 1;
            assert!(owner
                .admit_at(
                    &encoded_mention(index as u128 + 1, NOW),
                    Some(&sender(sender_index as u128)),
                    NOW,
                )
                .is_ok());
        }
        let prune_at = NOW + OMENCHAT_LXMF_NOTICE_MAX_LIFETIME_SECS + 1;
        let first = owner.prune_at(prune_at);
        assert_eq!(first, OMENCHAT_LXMF_NOTICE_PRUNE_BATCH);
        assert!(owner.retained().count() + owner.rate_record_count() > 0);
        while owner.prune_at(prune_at) > 0 {}
        assert_eq!(owner.retained().count(), 0);
        assert_eq!(owner.retained_accounted_bytes(), 0);
        assert_eq!(owner.rate_record_count(), 0);

        owner.clear_ephemeral();
        assert!(owner.is_enabled(OmenChatLxmfNoticeKind::OfflineMention));
    }
}
