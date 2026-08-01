use std::collections::VecDeque;

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const NOMADNET_UPDATE_POINTER_PROTOCOL: &str = "omenbrowser.nomadnet.update-pointer";
pub const NOMADNET_UPDATE_POINTER_VERSION: u16 = 1;
pub const NOMADNET_UPDATE_POINTER_MAX_ENCODED_BYTES: usize = 2 * 1024;
pub const NOMADNET_UPDATE_POINTER_DESTINATION_BYTES: usize = 32;
pub const NOMADNET_UPDATE_POINTER_PATH_MAX_BYTES: usize = 512;
pub const NOMADNET_UPDATE_POINTER_REVISION_MAX_BYTES: usize = 128;
pub const NOMADNET_UPDATE_POINTER_TITLE_MAX_BYTES: usize = 256;
pub const NOMADNET_UPDATE_POINTER_CLOCK_SKEW_SECS: u64 = 5 * 60;
pub const NOMADNET_UPDATE_POINTER_MAX_LIFETIME_SECS: u64 = 30 * 24 * 60 * 60;
pub const NOMADNET_UPDATE_FOLLOW_MAX_ITEMS: usize = 64;
pub const NOMADNET_UPDATE_FOLLOW_MAX_ACCOUNTED_BYTES: usize = 64 * 1024;
pub const NOMADNET_UPDATE_NOTICE_MAX_ITEMS: usize = 128;
pub const NOMADNET_UPDATE_NOTICE_MAX_ACCOUNTED_BYTES: usize = 64 * 1024;
pub const NOMADNET_UPDATE_PUBLISHER_WINDOW_SECS: u64 = 10 * 60;
pub const NOMADNET_UPDATE_MAX_PER_PUBLISHER_WINDOW: usize = 8;
pub const NOMADNET_UPDATE_RATE_RECORD_MAX_ITEMS: usize = 512;
pub const NOMADNET_UPDATE_PRUNE_BATCH: usize = 8;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct NomadNetUpdatePointer {
    pub protocol: String,
    pub version: u16,
    pub destination: String,
    pub page_path: String,
    pub revision_or_content_hash: String,
    pub title: String,
    pub published_at_unix: u64,
    pub expires_at_unix: u64,
}

impl NomadNetUpdatePointer {
    pub fn encode_at(&self, now_unix: u64) -> Result<Vec<u8>, NomadNetUpdatePointerError> {
        self.validate_at(now_unix)?;
        let encoded =
            serde_json::to_vec(self).map_err(|_| NomadNetUpdatePointerError::Malformed)?;
        if encoded.len() > NOMADNET_UPDATE_POINTER_MAX_ENCODED_BYTES {
            return Err(NomadNetUpdatePointerError::TooLarge);
        }
        Ok(encoded)
    }

    pub fn decode_at(encoded: &[u8], now_unix: u64) -> Result<Self, NomadNetUpdatePointerError> {
        if encoded.len() > NOMADNET_UPDATE_POINTER_MAX_ENCODED_BYTES {
            return Err(NomadNetUpdatePointerError::TooLarge);
        }
        let pointer = serde_json::from_slice::<Self>(encoded)
            .map_err(|_| NomadNetUpdatePointerError::Malformed)?;
        pointer.validate_at(now_unix)?;
        Ok(pointer)
    }

    pub fn validate_at(&self, now_unix: u64) -> Result<(), NomadNetUpdatePointerError> {
        if self.protocol != NOMADNET_UPDATE_POINTER_PROTOCOL
            || self.version != NOMADNET_UPDATE_POINTER_VERSION
        {
            return Err(NomadNetUpdatePointerError::UnsupportedProtocol);
        }
        if !is_canonical_destination(&self.destination) {
            return Err(NomadNetUpdatePointerError::InvalidDestination);
        }
        if !is_canonical_page_path(&self.page_path) {
            return Err(NomadNetUpdatePointerError::InvalidPagePath);
        }
        if !is_valid_revision(&self.revision_or_content_hash) {
            return Err(NomadNetUpdatePointerError::InvalidRevision);
        }
        if self.title.is_empty()
            || self.title.len() > NOMADNET_UPDATE_POINTER_TITLE_MAX_BYTES
            || self.title.chars().any(char::is_control)
        {
            return Err(NomadNetUpdatePointerError::InvalidTitle);
        }
        if self.published_at_unix > now_unix.saturating_add(NOMADNET_UPDATE_POINTER_CLOCK_SKEW_SECS)
        {
            return Err(NomadNetUpdatePointerError::PublishedInFuture);
        }
        if self.expires_at_unix <= self.published_at_unix
            || self.expires_at_unix.saturating_sub(self.published_at_unix)
                > NOMADNET_UPDATE_POINTER_MAX_LIFETIME_SECS
        {
            return Err(NomadNetUpdatePointerError::InvalidExpiry);
        }
        if self
            .expires_at_unix
            .saturating_add(NOMADNET_UPDATE_POINTER_CLOCK_SKEW_SECS)
            < now_unix
        {
            return Err(NomadNetUpdatePointerError::Expired);
        }
        Ok(())
    }

    pub fn deduplication_key(&self) -> (&str, &str, &str) {
        (
            &self.destination,
            &self.page_path,
            &self.revision_or_content_hash,
        )
    }
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum NomadNetUpdatePointerError {
    #[error("NomadNet update pointer exceeds its encoded byte limit")]
    TooLarge,
    #[error("NomadNet update pointer is malformed")]
    Malformed,
    #[error("NomadNet update pointer protocol or version is unsupported")]
    UnsupportedProtocol,
    #[error("NomadNet update pointer destination is not canonical")]
    InvalidDestination,
    #[error("NomadNet update pointer page path is invalid")]
    InvalidPagePath,
    #[error("NomadNet update pointer revision is invalid")]
    InvalidRevision,
    #[error("NomadNet update pointer title is invalid")]
    InvalidTitle,
    #[error("NomadNet update pointer publication time is too far in the future")]
    PublishedInFuture,
    #[error("NomadNet update pointer expiry is invalid")]
    InvalidExpiry,
    #[error("NomadNet update pointer has expired")]
    Expired,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NomadNetUpdateFollowTarget {
    destination: String,
    page_path: Option<String>,
}

impl NomadNetUpdateFollowTarget {
    pub fn new(
        destination: impl Into<String>,
        page_path: Option<String>,
    ) -> Result<Self, NomadNetUpdateAdmissionError> {
        let target = Self {
            destination: destination.into(),
            page_path,
        };
        if !is_canonical_destination(&target.destination)
            || target
                .page_path
                .as_deref()
                .is_some_and(|path| !is_canonical_page_path(path))
        {
            return Err(NomadNetUpdateAdmissionError::InvalidFollowTarget);
        }
        Ok(target)
    }

    pub fn destination(&self) -> &str {
        &self.destination
    }

    pub fn page_path(&self) -> Option<&str> {
        self.page_path.as_deref()
    }

    fn matches(&self, pointer: &NomadNetUpdatePointer) -> bool {
        self.destination == pointer.destination
            && self
                .page_path
                .as_deref()
                .is_none_or(|path| path == pointer.page_path)
    }

    fn accounted_bytes(&self) -> usize {
        self.destination
            .len()
            .saturating_add(self.page_path.as_ref().map_or(0, String::len))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NomadNetUpdatePublisherEvidence {
    Authenticated,
    Unverified,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NomadNetUpdateNotice {
    pointer: NomadNetUpdatePointer,
    publisher_destination: String,
    publisher_evidence: NomadNetUpdatePublisherEvidence,
    received_at_unix: u64,
    accounted_bytes: usize,
}

impl NomadNetUpdateNotice {
    pub fn pointer(&self) -> &NomadNetUpdatePointer {
        &self.pointer
    }

    pub fn publisher_destination(&self) -> &str {
        &self.publisher_destination
    }

    pub fn publisher_evidence(&self) -> NomadNetUpdatePublisherEvidence {
        self.publisher_evidence
    }

    pub fn received_at_unix(&self) -> u64 {
        self.received_at_unix
    }

    pub const fn allows_automatic_fetch(&self) -> bool {
        false
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct NomadNetPublisherRateRecord {
    publisher_destination: String,
    admitted_at_unix: u64,
}

#[derive(Clone, Debug, Default)]
pub struct NomadNetUpdateAdmissionOwner {
    followed: Vec<NomadNetUpdateFollowTarget>,
    followed_accounted_bytes: usize,
    notices: VecDeque<NomadNetUpdateNotice>,
    notice_accounted_bytes: usize,
    rate_records: VecDeque<NomadNetPublisherRateRecord>,
}

impl NomadNetUpdateAdmissionOwner {
    pub fn followed(&self) -> impl Iterator<Item = &NomadNetUpdateFollowTarget> {
        self.followed.iter()
    }

    pub fn notices(&self) -> impl Iterator<Item = &NomadNetUpdateNotice> {
        self.notices.iter()
    }

    pub fn followed_accounted_bytes(&self) -> usize {
        self.followed_accounted_bytes
    }

    pub fn notice_accounted_bytes(&self) -> usize {
        self.notice_accounted_bytes
    }

    pub fn rate_record_count(&self) -> usize {
        self.rate_records.len()
    }

    pub fn follow(
        &mut self,
        target: NomadNetUpdateFollowTarget,
    ) -> Result<bool, NomadNetUpdateAdmissionError> {
        if self.followed.contains(&target) {
            return Ok(false);
        }
        let accounted = target.accounted_bytes();
        if self.followed.len() >= NOMADNET_UPDATE_FOLLOW_MAX_ITEMS
            || self.followed_accounted_bytes.saturating_add(accounted)
                > NOMADNET_UPDATE_FOLLOW_MAX_ACCOUNTED_BYTES
        {
            return Err(NomadNetUpdateAdmissionError::FollowCapacity);
        }
        self.followed_accounted_bytes = self.followed_accounted_bytes.saturating_add(accounted);
        self.followed.push(target);
        Ok(true)
    }

    pub fn unfollow(&mut self, target: &NomadNetUpdateFollowTarget) -> bool {
        let Some(index) = self.followed.iter().position(|known| known == target) else {
            return false;
        };
        let removed = self.followed.remove(index);
        self.followed_accounted_bytes = self
            .followed_accounted_bytes
            .saturating_sub(removed.accounted_bytes());

        let mut retained = VecDeque::with_capacity(self.notices.len());
        while let Some(notice) = self.notices.pop_front() {
            if target.matches(&notice.pointer) {
                self.notice_accounted_bytes = self
                    .notice_accounted_bytes
                    .saturating_sub(notice.accounted_bytes);
            } else {
                retained.push_back(notice);
            }
        }
        self.notices = retained;
        true
    }

    pub fn admit(
        &mut self,
        pointer: NomadNetUpdatePointer,
        publisher_destination: &str,
        publisher_evidence: NomadNetUpdatePublisherEvidence,
        received_at_unix: u64,
    ) -> Result<(), NomadNetUpdateAdmissionError> {
        self.prune_at(received_at_unix);
        pointer.validate_at(received_at_unix)?;
        if pointer.expires_at_unix <= received_at_unix {
            return Err(NomadNetUpdatePointerError::Expired.into());
        }
        if !is_canonical_destination(publisher_destination) {
            return Err(NomadNetUpdateAdmissionError::InvalidPublisher);
        }
        if !self.followed.iter().any(|target| target.matches(&pointer)) {
            return Err(NomadNetUpdateAdmissionError::NotFollowed);
        }
        if self
            .notices
            .iter()
            .any(|notice| notice.pointer.deduplication_key() == pointer.deduplication_key())
        {
            return Err(NomadNetUpdateAdmissionError::Duplicate);
        }
        let window_start = received_at_unix.saturating_sub(NOMADNET_UPDATE_PUBLISHER_WINDOW_SECS);
        if self
            .rate_records
            .iter()
            .filter(|record| {
                record.publisher_destination == publisher_destination
                    && record.admitted_at_unix >= window_start
            })
            .count()
            >= NOMADNET_UPDATE_MAX_PER_PUBLISHER_WINDOW
        {
            return Err(NomadNetUpdateAdmissionError::RateLimited);
        }
        if self.rate_records.len() >= NOMADNET_UPDATE_RATE_RECORD_MAX_ITEMS {
            return Err(NomadNetUpdateAdmissionError::RateCapacity);
        }
        let encoded_bytes = pointer.encode_at(received_at_unix)?.len();
        let accounted_bytes = encoded_bytes.saturating_add(publisher_destination.len());
        if self.notices.len() >= NOMADNET_UPDATE_NOTICE_MAX_ITEMS
            || self.notice_accounted_bytes.saturating_add(accounted_bytes)
                > NOMADNET_UPDATE_NOTICE_MAX_ACCOUNTED_BYTES
        {
            return Err(NomadNetUpdateAdmissionError::NoticeCapacity);
        }
        self.notice_accounted_bytes = self.notice_accounted_bytes.saturating_add(accounted_bytes);
        self.notices.push_back(NomadNetUpdateNotice {
            pointer,
            publisher_destination: publisher_destination.into(),
            publisher_evidence,
            received_at_unix,
            accounted_bytes,
        });
        self.rate_records.push_back(NomadNetPublisherRateRecord {
            publisher_destination: publisher_destination.into(),
            admitted_at_unix: received_at_unix,
        });
        Ok(())
    }

    pub fn prune_at(&mut self, now_unix: u64) -> usize {
        let mut pruned = 0usize;
        let mut index = 0usize;
        while index < self.notices.len() && pruned < NOMADNET_UPDATE_PRUNE_BATCH {
            if self.notices[index].pointer.expires_at_unix <= now_unix {
                if let Some(notice) = self.notices.remove(index) {
                    self.notice_accounted_bytes = self
                        .notice_accounted_bytes
                        .saturating_sub(notice.accounted_bytes);
                }
                pruned += 1;
            } else {
                index += 1;
            }
        }

        let rate_cutoff = now_unix.saturating_sub(NOMADNET_UPDATE_PUBLISHER_WINDOW_SECS);
        let mut index = 0usize;
        while index < self.rate_records.len() && pruned < NOMADNET_UPDATE_PRUNE_BATCH {
            if self.rate_records[index].admitted_at_unix < rate_cutoff {
                self.rate_records.remove(index);
                pruned += 1;
            } else {
                index += 1;
            }
        }
        pruned
    }

    pub fn clear(&mut self) {
        self.followed.clear();
        self.followed_accounted_bytes = 0;
        self.notices.clear();
        self.notice_accounted_bytes = 0;
        self.rate_records.clear();
    }
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum NomadNetUpdateAdmissionError {
    #[error(transparent)]
    Pointer(#[from] NomadNetUpdatePointerError),
    #[error("NomadNet update follow target is invalid")]
    InvalidFollowTarget,
    #[error("NomadNet update follow capacity is full")]
    FollowCapacity,
    #[error("NomadNet update publisher destination is invalid")]
    InvalidPublisher,
    #[error("NomadNet update pointer does not match a followed target")]
    NotFollowed,
    #[error("NomadNet update pointer is a duplicate")]
    Duplicate,
    #[error("NomadNet update publisher rate limit was reached")]
    RateLimited,
    #[error("NomadNet update publisher-rate accounting is full")]
    RateCapacity,
    #[error("NomadNet update notice capacity is full")]
    NoticeCapacity,
}

fn is_canonical_destination(value: &str) -> bool {
    value.len() == NOMADNET_UPDATE_POINTER_DESTINATION_BYTES
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn is_canonical_page_path(value: &str) -> bool {
    if value.is_empty()
        || value.len() > NOMADNET_UPDATE_POINTER_PATH_MAX_BYTES
        || !value.starts_with('/')
        || value.contains(['?', '#', '\\'])
        || value.contains("//")
        || value.chars().any(char::is_control)
    {
        return false;
    }
    value
        .split('/')
        .skip(1)
        .all(|segment| !matches!(segment, "." | ".."))
}

fn is_valid_revision(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= NOMADNET_UPDATE_POINTER_REVISION_MAX_BYTES
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: u64 = 1_800_000_000;

    fn fixture() -> NomadNetUpdatePointer {
        NomadNetUpdatePointer {
            protocol: NOMADNET_UPDATE_POINTER_PROTOCOL.into(),
            version: NOMADNET_UPDATE_POINTER_VERSION,
            destination: "0123456789abcdef0123456789abcdef".into(),
            page_path: "/page/index.mu".into(),
            revision_or_content_hash: "sha256:0123456789abcdef".into(),
            title: "Updated page".into(),
            published_at_unix: NOW,
            expires_at_unix: NOW + 3600,
        }
    }

    #[test]
    fn bounded_pointer_round_trips_and_exposes_stable_deduplication_key() {
        let pointer = fixture();
        let encoded = pointer.encode_at(NOW).expect("encode");
        assert!(encoded.len() <= NOMADNET_UPDATE_POINTER_MAX_ENCODED_BYTES);
        let decoded = NomadNetUpdatePointer::decode_at(&encoded, NOW).expect("decode");
        assert_eq!(decoded, pointer);
        assert_eq!(
            decoded.deduplication_key(),
            (
                "0123456789abcdef0123456789abcdef",
                "/page/index.mu",
                "sha256:0123456789abcdef"
            )
        );
    }

    #[test]
    fn pointer_rejects_unbounded_unknown_and_noncanonical_input_before_use() {
        let mut pointer = fixture();
        pointer.title = "x".repeat(NOMADNET_UPDATE_POINTER_TITLE_MAX_BYTES + 1);
        assert_eq!(
            pointer.encode_at(NOW),
            Err(NomadNetUpdatePointerError::InvalidTitle)
        );

        let oversized = vec![b' '; NOMADNET_UPDATE_POINTER_MAX_ENCODED_BYTES + 1];
        assert_eq!(
            NomadNetUpdatePointer::decode_at(&oversized, NOW),
            Err(NomadNetUpdatePointerError::TooLarge)
        );

        let mut value = serde_json::to_value(fixture()).expect("value");
        value["page_body"] = serde_json::Value::String("not allowed".into());
        let unknown = serde_json::to_vec(&value).expect("unknown encode");
        assert_eq!(
            NomadNetUpdatePointer::decode_at(&unknown, NOW),
            Err(NomadNetUpdatePointerError::Malformed)
        );

        for invalid in ["ABCDEF0123456789abcdef0123456789", "0123456789abcdef"] {
            let mut pointer = fixture();
            pointer.destination = invalid.into();
            assert_eq!(
                pointer.validate_at(NOW),
                Err(NomadNetUpdatePointerError::InvalidDestination)
            );
        }
        for invalid in [
            "page/index.mu",
            "/../secret",
            "/page//index.mu",
            "/page?q=x",
        ] {
            let mut pointer = fixture();
            pointer.page_path = invalid.into();
            assert_eq!(
                pointer.validate_at(NOW),
                Err(NomadNetUpdatePointerError::InvalidPagePath)
            );
        }
    }

    #[test]
    fn pointer_enforces_revision_and_time_boundaries_without_fetching() {
        let mut pointer = fixture();
        pointer.revision_or_content_hash = "bad revision".into();
        assert_eq!(
            pointer.validate_at(NOW),
            Err(NomadNetUpdatePointerError::InvalidRevision)
        );

        let mut pointer = fixture();
        pointer.published_at_unix = NOW + NOMADNET_UPDATE_POINTER_CLOCK_SKEW_SECS + 1;
        assert_eq!(
            pointer.validate_at(NOW),
            Err(NomadNetUpdatePointerError::PublishedInFuture)
        );

        let mut pointer = fixture();
        pointer.expires_at_unix = pointer.published_at_unix;
        assert_eq!(
            pointer.validate_at(NOW),
            Err(NomadNetUpdatePointerError::InvalidExpiry)
        );

        let mut pointer = fixture();
        pointer.expires_at_unix =
            pointer.published_at_unix + NOMADNET_UPDATE_POINTER_MAX_LIFETIME_SECS + 1;
        assert_eq!(
            pointer.validate_at(NOW),
            Err(NomadNetUpdatePointerError::InvalidExpiry)
        );

        let mut pointer = fixture();
        pointer.expires_at_unix = NOW - NOMADNET_UPDATE_POINTER_CLOCK_SKEW_SECS - 1;
        pointer.published_at_unix = pointer.expires_at_unix - 60;
        assert_eq!(
            pointer.validate_at(NOW),
            Err(NomadNetUpdatePointerError::Expired)
        );
    }

    #[test]
    fn admission_requires_follow_and_preserves_publisher_authority_without_fetch() {
        let pointer = fixture();
        let target = NomadNetUpdateFollowTarget::new(pointer.destination.clone(), None)
            .expect("follow target");
        let mut owner = NomadNetUpdateAdmissionOwner::default();
        assert_eq!(
            owner.admit(
                pointer.clone(),
                "11111111111111111111111111111111",
                NomadNetUpdatePublisherEvidence::Authenticated,
                NOW,
            ),
            Err(NomadNetUpdateAdmissionError::NotFollowed)
        );
        assert!(owner.follow(target.clone()).expect("follow"));
        assert!(!owner.follow(target.clone()).expect("duplicate follow"));
        owner
            .admit(
                pointer,
                "11111111111111111111111111111111",
                NomadNetUpdatePublisherEvidence::Authenticated,
                NOW,
            )
            .expect("admit");
        let notice = owner.notices().next().expect("notice");
        assert_eq!(
            notice.publisher_evidence(),
            NomadNetUpdatePublisherEvidence::Authenticated
        );
        assert!(!notice.allows_automatic_fetch());
        assert!(owner.unfollow(&target));
        assert_eq!(owner.notices().count(), 0);
        assert_eq!(owner.notice_accounted_bytes(), 0);
    }

    #[test]
    fn admission_deduplicates_and_rate_limits_each_canonical_publisher() {
        let mut owner = NomadNetUpdateAdmissionOwner::default();
        owner
            .follow(
                NomadNetUpdateFollowTarget::new(fixture().destination, None)
                    .expect("follow target"),
            )
            .expect("follow");
        let publisher = "22222222222222222222222222222222";
        for index in 0..NOMADNET_UPDATE_MAX_PER_PUBLISHER_WINDOW {
            let mut pointer = fixture();
            pointer.revision_or_content_hash = format!("revision-{index}");
            owner
                .admit(
                    pointer.clone(),
                    publisher,
                    NomadNetUpdatePublisherEvidence::Unverified,
                    NOW + u64::try_from(index).expect("index"),
                )
                .expect("bounded admission");
            assert_eq!(
                owner.admit(
                    pointer,
                    publisher,
                    NomadNetUpdatePublisherEvidence::Unverified,
                    NOW + u64::try_from(index).expect("index"),
                ),
                Err(NomadNetUpdateAdmissionError::Duplicate)
            );
        }
        let mut next = fixture();
        next.revision_or_content_hash = "rate-limited".into();
        assert_eq!(
            owner.admit(
                next,
                publisher,
                NomadNetUpdatePublisherEvidence::Unverified,
                NOW + 10,
            ),
            Err(NomadNetUpdateAdmissionError::RateLimited)
        );
        assert_eq!(
            owner.rate_record_count(),
            NOMADNET_UPDATE_MAX_PER_PUBLISHER_WINDOW
        );
    }

    #[test]
    fn owner_caps_follows_and_prunes_expired_state_incrementally() {
        let mut full = NomadNetUpdateAdmissionOwner::default();
        for index in 0..NOMADNET_UPDATE_FOLLOW_MAX_ITEMS {
            let target = NomadNetUpdateFollowTarget::new(format!("{index:032x}"), None)
                .expect("canonical target");
            assert!(full.follow(target).expect("bounded follow"));
        }
        assert_eq!(
            full.follow(
                NomadNetUpdateFollowTarget::new(
                    format!("{:032x}", NOMADNET_UPDATE_FOLLOW_MAX_ITEMS),
                    None,
                )
                .expect("overflow target"),
            ),
            Err(NomadNetUpdateAdmissionError::FollowCapacity)
        );

        let mut owner = NomadNetUpdateAdmissionOwner::default();
        owner
            .follow(
                NomadNetUpdateFollowTarget::new(fixture().destination, None)
                    .expect("follow target"),
            )
            .expect("follow");
        for index in 0..(NOMADNET_UPDATE_PRUNE_BATCH + 4) {
            let mut pointer = fixture();
            pointer.revision_or_content_hash = format!("expiring-{index}");
            pointer.expires_at_unix = NOW + 1;
            owner
                .admit(
                    pointer,
                    &format!("{:032x}", index + 100),
                    NomadNetUpdatePublisherEvidence::Unverified,
                    NOW,
                )
                .expect("admit expiring");
        }
        assert_eq!(owner.prune_at(NOW + 2), NOMADNET_UPDATE_PRUNE_BATCH);
        assert_eq!(owner.notices().count(), 4);
        assert_eq!(owner.prune_at(NOW + 2), 4);
        assert_eq!(owner.notices().count(), 0);
        assert_eq!(owner.notice_accounted_bytes(), 0);
        owner.clear();
        assert_eq!(owner.followed().count(), 0);
        assert_eq!(owner.rate_record_count(), 0);
    }

    #[test]
    fn notice_retention_rejects_at_the_item_or_owned_byte_ceiling() {
        let mut owner = NomadNetUpdateAdmissionOwner::default();
        owner
            .follow(
                NomadNetUpdateFollowTarget::new(fixture().destination, None)
                    .expect("follow target"),
            )
            .expect("follow");
        let mut rejected = false;
        for index in 0..=NOMADNET_UPDATE_NOTICE_MAX_ITEMS {
            let mut pointer = fixture();
            pointer.page_path = format!("/{}", "p".repeat(511));
            pointer.title = "t".repeat(NOMADNET_UPDATE_POINTER_TITLE_MAX_BYTES);
            pointer.revision_or_content_hash = format!("{index:03}-{}", "r".repeat(124));
            let result = owner.admit(
                pointer,
                &format!("{:032x}", index + 1_000),
                NomadNetUpdatePublisherEvidence::Unverified,
                NOW,
            );
            if result == Err(NomadNetUpdateAdmissionError::NoticeCapacity) {
                rejected = true;
                break;
            }
            result.expect("admit below retention ceiling");
        }
        assert!(rejected);
        assert!(owner.notices().count() <= NOMADNET_UPDATE_NOTICE_MAX_ITEMS);
        assert!(owner.notice_accounted_bytes() <= NOMADNET_UPDATE_NOTICE_MAX_ACCOUNTED_BYTES);
    }
}
