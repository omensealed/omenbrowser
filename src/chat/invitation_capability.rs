use std::collections::{BTreeSet, VecDeque};
use std::io::Cursor;

use thiserror::Error;

pub const INVITATION_CAPABILITY_PROTOCOL: &str = "omenbrowser.lxmf.peer-capabilities";
pub const INVITATION_CAPABILITY_VERSION: u16 = 1;
pub const OMENCHAT_LXMF_INVITATIONS_CAPABILITY: &str = "omenchat-lxmf-invitations-v1";
pub const INVITATION_CAPABILITY_DESTINATION_APPLICATION: &str = "omenbrowser";
pub const INVITATION_CAPABILITY_DESTINATION_ASPECTS: &[&str] = &["lxmf", "capabilities"];
pub const INVITATION_CAPABILITY_DESTINATION_ASPECT: &str = "lxmf.capabilities";
pub const INVITATION_CAPABILITY_NONCE_BYTES: usize = 16;
pub const INVITATION_CAPABILITY_REQUEST_MAX_BYTES: usize = 128;
pub const INVITATION_CAPABILITY_RESPONSE_MAX_BYTES: usize = 1024;
pub const INVITATION_CAPABILITY_MAX_ITEMS: usize = 16;
pub const INVITATION_CAPABILITY_NAME_MAX_BYTES: usize = 64;

pub const INVITATION_CAPABILITY_CACHE_MAX_ITEMS: usize = 256;
pub const INVITATION_CAPABILITY_CACHE_MAX_ACCOUNTED_BYTES: usize = 64 * 1024;
pub const INVITATION_CAPABILITY_MAX_IN_FLIGHT: usize = 8;
pub use crate::runtime::network::{
    InvitationCapabilityProbeOutcome,
    LXMF_INVITATION_CAPABILITY_PROBE_DEADLINE_MS as INVITATION_CAPABILITY_PROBE_DEADLINE_MS,
};
pub const INVITATION_CAPABILITY_PROBE_COOLDOWN_MS: u64 = 60_000;
pub const INVITATION_CAPABILITY_SUPPORTED_TTL_MS: u64 = 10 * 60_000;
pub const INVITATION_CAPABILITY_NEGATIVE_TTL_MS: u64 = 60_000;
pub const INVITATION_CAPABILITY_PRUNE_BATCH: usize = 8;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InvitationCapabilityRequest {
    pub nonce: [u8; INVITATION_CAPABILITY_NONCE_BYTES],
}

impl InvitationCapabilityRequest {
    pub fn encode(&self) -> Result<Vec<u8>, InvitationCapabilityCodecError> {
        encode_bounded(
            rmpv::Value::Array(vec![
                rmpv::Value::from(INVITATION_CAPABILITY_PROTOCOL),
                rmpv::Value::from(INVITATION_CAPABILITY_VERSION),
                rmpv::Value::Binary(self.nonce.to_vec()),
            ]),
            INVITATION_CAPABILITY_REQUEST_MAX_BYTES,
        )
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, InvitationCapabilityCodecError> {
        let value = decode_bounded(bytes, INVITATION_CAPABILITY_REQUEST_MAX_BYTES)?;
        let rmpv::Value::Array(fields) = value else {
            return Err(InvitationCapabilityCodecError::InvalidShape);
        };
        if fields.len() != 3 {
            return Err(InvitationCapabilityCodecError::InvalidShape);
        }
        validate_header(&fields[0], &fields[1])?;
        Ok(Self {
            nonce: decode_nonce(&fields[2])?,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InvitationCapabilityResponse {
    pub nonce: [u8; INVITATION_CAPABILITY_NONCE_BYTES],
    pub capabilities: Vec<String>,
}

impl InvitationCapabilityResponse {
    pub fn new(
        nonce: [u8; INVITATION_CAPABILITY_NONCE_BYTES],
        mut capabilities: Vec<String>,
    ) -> Result<Self, InvitationCapabilityCodecError> {
        validate_capabilities(&capabilities, false)?;
        capabilities.sort_unstable();
        Ok(Self {
            nonce,
            capabilities,
        })
    }

    pub fn encode(&self) -> Result<Vec<u8>, InvitationCapabilityCodecError> {
        validate_capabilities(&self.capabilities, true)?;
        encode_bounded(
            rmpv::Value::Array(vec![
                rmpv::Value::from(INVITATION_CAPABILITY_PROTOCOL),
                rmpv::Value::from(INVITATION_CAPABILITY_VERSION),
                rmpv::Value::Binary(self.nonce.to_vec()),
                rmpv::Value::Array(
                    self.capabilities
                        .iter()
                        .map(|capability| rmpv::Value::from(capability.as_str()))
                        .collect(),
                ),
            ]),
            INVITATION_CAPABILITY_RESPONSE_MAX_BYTES,
        )
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, InvitationCapabilityCodecError> {
        let value = decode_bounded(bytes, INVITATION_CAPABILITY_RESPONSE_MAX_BYTES)?;
        let rmpv::Value::Array(fields) = value else {
            return Err(InvitationCapabilityCodecError::InvalidShape);
        };
        if fields.len() != 4 {
            return Err(InvitationCapabilityCodecError::InvalidShape);
        }
        validate_header(&fields[0], &fields[1])?;
        let nonce = decode_nonce(&fields[2])?;
        let rmpv::Value::Array(values) = &fields[3] else {
            return Err(InvitationCapabilityCodecError::InvalidShape);
        };
        if values.len() > INVITATION_CAPABILITY_MAX_ITEMS {
            return Err(InvitationCapabilityCodecError::TooManyCapabilities);
        }
        let mut capabilities = Vec::with_capacity(values.len());
        for value in values {
            let Some(capability) = value.as_str() else {
                return Err(InvitationCapabilityCodecError::InvalidCapability);
            };
            capabilities.push(capability.to_owned());
        }
        validate_capabilities(&capabilities, true)?;
        Ok(Self {
            nonce,
            capabilities,
        })
    }

    pub fn supports_for(
        &self,
        request: &InvitationCapabilityRequest,
        capability: &str,
    ) -> Result<bool, InvitationCapabilityCodecError> {
        if self.nonce != request.nonce {
            return Err(InvitationCapabilityCodecError::NonceMismatch);
        }
        Ok(self
            .capabilities
            .binary_search_by(|known| known.as_str().cmp(capability))
            .is_ok())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InvitationCapabilityState {
    Supported,
    Unsupported,
    Unknown,
    Stale,
    Checking,
    Conflict,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct InvitationCapabilityEvidence {
    peer_destination: String,
    state: InvitationCapabilityState,
    last_probe_started_ms: u64,
    expires_at_ms: u64,
    accounted_bytes: usize,
}

#[derive(Clone, Debug, Default)]
pub struct InvitationCapabilityEvidenceOwner {
    records: VecDeque<InvitationCapabilityEvidence>,
    accounted_bytes: usize,
}

impl InvitationCapabilityEvidenceOwner {
    pub fn len(&self) -> usize {
        self.records.len()
    }

    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    pub fn accounted_bytes(&self) -> usize {
        self.accounted_bytes
    }

    pub fn active_probes(&self, now_ms: u64) -> usize {
        self.records
            .iter()
            .filter(|record| {
                record.state == InvitationCapabilityState::Checking && now_ms < record.expires_at_ms
            })
            .count()
    }

    pub fn state(&self, peer_destination: &str, now_ms: u64) -> InvitationCapabilityState {
        self.records
            .iter()
            .find(|record| record.peer_destination == peer_destination)
            .map_or(InvitationCapabilityState::Unknown, |record| {
                if now_ms >= record.expires_at_ms {
                    InvitationCapabilityState::Stale
                } else {
                    record.state
                }
            })
    }

    pub fn begin_probe(
        &mut self,
        peer_destination: &str,
        now_ms: u64,
    ) -> Result<(), InvitationCapabilityEvidenceError> {
        validate_peer_destination(peer_destination)?;
        if let Some(record) = self
            .records
            .iter()
            .find(|record| record.peer_destination == peer_destination)
        {
            if record.state == InvitationCapabilityState::Checking && now_ms < record.expires_at_ms
            {
                return Err(InvitationCapabilityEvidenceError::AlreadyChecking);
            }
            if now_ms
                < record
                    .last_probe_started_ms
                    .saturating_add(INVITATION_CAPABILITY_PROBE_COOLDOWN_MS)
            {
                return Err(InvitationCapabilityEvidenceError::Cooldown);
            }
        }
        self.prune_expired(now_ms);
        if self.active_probes(now_ms) >= INVITATION_CAPABILITY_MAX_IN_FLIGHT {
            return Err(InvitationCapabilityEvidenceError::InFlightCapacity);
        }

        self.remove(peer_destination);
        let accounted_bytes = evidence_accounted_bytes(peer_destination);
        if self.records.len() >= INVITATION_CAPABILITY_CACHE_MAX_ITEMS
            || self.accounted_bytes.saturating_add(accounted_bytes)
                > INVITATION_CAPABILITY_CACHE_MAX_ACCOUNTED_BYTES
        {
            return Err(InvitationCapabilityEvidenceError::CacheCapacity);
        }
        self.records.push_back(InvitationCapabilityEvidence {
            peer_destination: peer_destination.to_owned(),
            state: InvitationCapabilityState::Checking,
            last_probe_started_ms: now_ms,
            expires_at_ms: now_ms.saturating_add(INVITATION_CAPABILITY_PROBE_DEADLINE_MS),
            accounted_bytes,
        });
        self.accounted_bytes = self.accounted_bytes.saturating_add(accounted_bytes);
        Ok(())
    }

    pub fn complete_probe(
        &mut self,
        peer_destination: &str,
        outcome: InvitationCapabilityProbeOutcome,
        now_ms: u64,
    ) -> Result<(), InvitationCapabilityEvidenceError> {
        let Some(record) = self
            .records
            .iter_mut()
            .find(|record| record.peer_destination == peer_destination)
        else {
            return Err(InvitationCapabilityEvidenceError::NotChecking);
        };
        if record.state != InvitationCapabilityState::Checking || now_ms >= record.expires_at_ms {
            return Err(InvitationCapabilityEvidenceError::NotChecking);
        }
        record.state = match outcome {
            InvitationCapabilityProbeOutcome::Supported => InvitationCapabilityState::Supported,
            InvitationCapabilityProbeOutcome::Unsupported => InvitationCapabilityState::Unsupported,
            InvitationCapabilityProbeOutcome::Unknown => InvitationCapabilityState::Unknown,
            InvitationCapabilityProbeOutcome::Conflict => InvitationCapabilityState::Conflict,
        };
        let ttl = if outcome == InvitationCapabilityProbeOutcome::Supported {
            INVITATION_CAPABILITY_SUPPORTED_TTL_MS
        } else {
            INVITATION_CAPABILITY_NEGATIVE_TTL_MS
        };
        record.expires_at_ms = now_ms.saturating_add(ttl);
        Ok(())
    }

    pub fn consume_supported(&mut self, peer_destination: &str, now_ms: u64) -> bool {
        let Some(record) = self
            .records
            .iter_mut()
            .find(|record| record.peer_destination == peer_destination)
        else {
            return false;
        };
        if record.state != InvitationCapabilityState::Supported || now_ms >= record.expires_at_ms {
            return false;
        }
        record.state = InvitationCapabilityState::Stale;
        record.expires_at_ms = now_ms;
        true
    }

    pub fn clear(&mut self) {
        self.records.clear();
        self.accounted_bytes = 0;
    }

    fn prune_expired(&mut self, now_ms: u64) {
        let mut removed = 0usize;
        let mut index = 0usize;
        while index < self.records.len() && removed < INVITATION_CAPABILITY_PRUNE_BATCH {
            if now_ms >= self.records[index].expires_at_ms {
                if let Some(record) = self.records.remove(index) {
                    self.accounted_bytes =
                        self.accounted_bytes.saturating_sub(record.accounted_bytes);
                }
                removed += 1;
            } else {
                index += 1;
            }
        }
    }

    fn remove(&mut self, peer_destination: &str) {
        if let Some(index) = self
            .records
            .iter()
            .position(|record| record.peer_destination == peer_destination)
        {
            if let Some(record) = self.records.remove(index) {
                self.accounted_bytes = self.accounted_bytes.saturating_sub(record.accounted_bytes);
            }
        }
    }
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum InvitationCapabilityCodecError {
    #[error("invitation capability value exceeds its byte limit")]
    TooLarge,
    #[error("invitation capability value is malformed")]
    Malformed,
    #[error("invitation capability value has an invalid shape")]
    InvalidShape,
    #[error("invitation capability protocol or version is unsupported")]
    UnsupportedProtocol,
    #[error("invitation capability nonce is invalid")]
    InvalidNonce,
    #[error("invitation capability response nonce does not match the request")]
    NonceMismatch,
    #[error("invitation capability response has too many capability names")]
    TooManyCapabilities,
    #[error("invitation capability name is invalid")]
    InvalidCapability,
    #[error("invitation capability names are duplicated or not canonical")]
    NonCanonicalCapabilities,
    #[error("invitation capability value contains trailing data")]
    TrailingData,
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum InvitationCapabilityEvidenceError {
    #[error("peer destination must be canonical lowercase 32-character hexadecimal")]
    InvalidPeer,
    #[error("an invitation capability probe is already active for this peer")]
    AlreadyChecking,
    #[error("invitation capability probe is in cooldown")]
    Cooldown,
    #[error("invitation capability probe concurrency is full")]
    InFlightCapacity,
    #[error("invitation capability evidence cache is full")]
    CacheCapacity,
    #[error("no active invitation capability probe exists for this peer")]
    NotChecking,
}

fn encode_bounded(
    value: rmpv::Value,
    max_bytes: usize,
) -> Result<Vec<u8>, InvitationCapabilityCodecError> {
    let mut encoded = Vec::new();
    rmpv::encode::write_value(&mut encoded, &value)
        .map_err(|_| InvitationCapabilityCodecError::Malformed)?;
    if encoded.len() > max_bytes {
        return Err(InvitationCapabilityCodecError::TooLarge);
    }
    Ok(encoded)
}

fn decode_bounded(
    bytes: &[u8],
    max_bytes: usize,
) -> Result<rmpv::Value, InvitationCapabilityCodecError> {
    if bytes.len() > max_bytes {
        return Err(InvitationCapabilityCodecError::TooLarge);
    }
    let mut cursor = Cursor::new(bytes);
    let value = rmpv::decode::read_value(&mut cursor)
        .map_err(|_| InvitationCapabilityCodecError::Malformed)?;
    if cursor.position() != bytes.len() as u64 {
        return Err(InvitationCapabilityCodecError::TrailingData);
    }
    Ok(value)
}

fn validate_header(
    protocol: &rmpv::Value,
    version: &rmpv::Value,
) -> Result<(), InvitationCapabilityCodecError> {
    if protocol.as_str() != Some(INVITATION_CAPABILITY_PROTOCOL)
        || version.as_u64() != Some(u64::from(INVITATION_CAPABILITY_VERSION))
    {
        return Err(InvitationCapabilityCodecError::UnsupportedProtocol);
    }
    Ok(())
}

fn decode_nonce(
    value: &rmpv::Value,
) -> Result<[u8; INVITATION_CAPABILITY_NONCE_BYTES], InvitationCapabilityCodecError> {
    let rmpv::Value::Binary(bytes) = value else {
        return Err(InvitationCapabilityCodecError::InvalidNonce);
    };
    bytes
        .as_slice()
        .try_into()
        .map_err(|_| InvitationCapabilityCodecError::InvalidNonce)
}

fn validate_capabilities(
    capabilities: &[String],
    require_canonical_order: bool,
) -> Result<(), InvitationCapabilityCodecError> {
    if capabilities.len() > INVITATION_CAPABILITY_MAX_ITEMS {
        return Err(InvitationCapabilityCodecError::TooManyCapabilities);
    }
    let mut unique = BTreeSet::new();
    for capability in capabilities {
        if capability.is_empty()
            || capability.len() > INVITATION_CAPABILITY_NAME_MAX_BYTES
            || !capability
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        {
            return Err(InvitationCapabilityCodecError::InvalidCapability);
        }
        if !unique.insert(capability.as_str()) {
            return Err(InvitationCapabilityCodecError::NonCanonicalCapabilities);
        }
    }
    if require_canonical_order && capabilities.windows(2).any(|window| window[0] >= window[1]) {
        return Err(InvitationCapabilityCodecError::NonCanonicalCapabilities);
    }
    Ok(())
}

fn validate_peer_destination(
    peer_destination: &str,
) -> Result<(), InvitationCapabilityEvidenceError> {
    if peer_destination.len() != 32
        || !peer_destination
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(InvitationCapabilityEvidenceError::InvalidPeer);
    }
    Ok(())
}

fn evidence_accounted_bytes(peer_destination: &str) -> usize {
    peer_destination.len().saturating_add(64)
}

#[cfg(test)]
mod tests {
    use super::*;

    const NONCE: [u8; INVITATION_CAPABILITY_NONCE_BYTES] = [0x5a; 16];
    const PEER: &str = "0123456789abcdef0123456789abcdef";

    fn encode_value(value: rmpv::Value) -> Vec<u8> {
        let mut encoded = Vec::new();
        rmpv::encode::write_value(&mut encoded, &value).expect("encode fixture");
        encoded
    }

    #[test]
    fn request_and_sorted_response_round_trip_exactly() {
        let request = InvitationCapabilityRequest { nonce: NONCE };
        assert_eq!(
            InvitationCapabilityRequest::decode(&request.encode().expect("request encode")),
            Ok(request.clone())
        );
        let response = InvitationCapabilityResponse::new(
            NONCE,
            vec![
                "z-capability".into(),
                OMENCHAT_LXMF_INVITATIONS_CAPABILITY.into(),
            ],
        )
        .expect("response");
        assert_eq!(
            response.capabilities,
            vec![OMENCHAT_LXMF_INVITATIONS_CAPABILITY, "z-capability"]
        );
        assert_eq!(
            InvitationCapabilityResponse::decode(&response.encode().expect("response encode")),
            Ok(response.clone())
        );
        assert_eq!(
            response.supports_for(&request, OMENCHAT_LXMF_INVITATIONS_CAPABILITY),
            Ok(true)
        );
    }

    #[test]
    fn codec_rejects_wrong_shapes_versions_nonces_order_and_trailing_data() {
        let wrong_version = encode_value(rmpv::Value::Array(vec![
            rmpv::Value::from(INVITATION_CAPABILITY_PROTOCOL),
            rmpv::Value::from(2),
            rmpv::Value::Binary(NONCE.to_vec()),
        ]));
        assert_eq!(
            InvitationCapabilityRequest::decode(&wrong_version),
            Err(InvitationCapabilityCodecError::UnsupportedProtocol)
        );
        let short_nonce = encode_value(rmpv::Value::Array(vec![
            rmpv::Value::from(INVITATION_CAPABILITY_PROTOCOL),
            rmpv::Value::from(INVITATION_CAPABILITY_VERSION),
            rmpv::Value::Binary(vec![0; 15]),
        ]));
        assert_eq!(
            InvitationCapabilityRequest::decode(&short_nonce),
            Err(InvitationCapabilityCodecError::InvalidNonce)
        );
        let noncanonical = encode_value(rmpv::Value::Array(vec![
            rmpv::Value::from(INVITATION_CAPABILITY_PROTOCOL),
            rmpv::Value::from(INVITATION_CAPABILITY_VERSION),
            rmpv::Value::Binary(NONCE.to_vec()),
            rmpv::Value::Array(vec![rmpv::Value::from("z"), rmpv::Value::from("a")]),
        ]));
        assert_eq!(
            InvitationCapabilityResponse::decode(&noncanonical),
            Err(InvitationCapabilityCodecError::NonCanonicalCapabilities)
        );
        let mut trailing = InvitationCapabilityRequest { nonce: NONCE }
            .encode()
            .expect("request");
        trailing.push(0xc0);
        assert_eq!(
            InvitationCapabilityRequest::decode(&trailing),
            Err(InvitationCapabilityCodecError::TrailingData)
        );
        let mismatch = InvitationCapabilityResponse::new([1; 16], Vec::new()).expect("response");
        assert_eq!(
            mismatch.supports_for(
                &InvitationCapabilityRequest { nonce: NONCE },
                OMENCHAT_LXMF_INVITATIONS_CAPABILITY
            ),
            Err(InvitationCapabilityCodecError::NonceMismatch)
        );
    }

    #[test]
    fn capability_item_name_and_total_byte_bounds_fail_closed() {
        let too_many = (0..=INVITATION_CAPABILITY_MAX_ITEMS)
            .map(|index| format!("capability-{index:02}"))
            .collect();
        assert_eq!(
            InvitationCapabilityResponse::new(NONCE, too_many),
            Err(InvitationCapabilityCodecError::TooManyCapabilities)
        );
        assert!(InvitationCapabilityResponse::new(NONCE, vec!["a".repeat(64)]).is_ok());
        assert_eq!(
            InvitationCapabilityResponse::new(NONCE, vec!["a".repeat(65)]),
            Err(InvitationCapabilityCodecError::InvalidCapability)
        );
        assert_eq!(
            InvitationCapabilityResponse::new(NONCE, vec!["duplicate".into(); 2]),
            Err(InvitationCapabilityCodecError::NonCanonicalCapabilities)
        );
        let excessive = vec![0; INVITATION_CAPABILITY_RESPONSE_MAX_BYTES + 1];
        assert_eq!(
            InvitationCapabilityResponse::decode(&excessive),
            Err(InvitationCapabilityCodecError::TooLarge)
        );
    }

    #[test]
    fn evidence_requires_fresh_one_use_support_and_enforces_cooldown() {
        let mut owner = InvitationCapabilityEvidenceOwner::default();
        assert_eq!(owner.state(PEER, 0), InvitationCapabilityState::Unknown);
        owner.begin_probe(PEER, 1_000).expect("begin");
        assert_eq!(
            owner.state(PEER, 1_001),
            InvitationCapabilityState::Checking
        );
        assert_eq!(
            owner.begin_probe(PEER, 1_001),
            Err(InvitationCapabilityEvidenceError::AlreadyChecking)
        );
        owner
            .complete_probe(PEER, InvitationCapabilityProbeOutcome::Supported, 2_000)
            .expect("complete");
        assert_eq!(
            owner.state(PEER, 2_001),
            InvitationCapabilityState::Supported
        );
        assert!(owner.consume_supported(PEER, 2_001));
        assert!(!owner.consume_supported(PEER, 2_001));
        assert_eq!(owner.state(PEER, 2_001), InvitationCapabilityState::Stale);
        assert_eq!(
            owner.begin_probe(PEER, 2_001),
            Err(InvitationCapabilityEvidenceError::Cooldown)
        );
        owner.begin_probe(PEER, 61_000).expect("after cooldown");
    }

    #[test]
    fn evidence_bounds_concurrency_cache_bytes_and_shutdown_clear() {
        let mut owner = InvitationCapabilityEvidenceOwner::default();
        for index in 0..INVITATION_CAPABILITY_MAX_IN_FLIGHT {
            owner
                .begin_probe(&format!("{index:032x}"), 1_000)
                .expect("bounded probe");
        }
        assert_eq!(
            owner.active_probes(1_001),
            INVITATION_CAPABILITY_MAX_IN_FLIGHT
        );
        assert_eq!(
            owner.begin_probe("000000000000000000000000000000ff", 1_001),
            Err(InvitationCapabilityEvidenceError::InFlightCapacity)
        );
        for index in 0..INVITATION_CAPABILITY_MAX_IN_FLIGHT {
            owner
                .complete_probe(
                    &format!("{index:032x}"),
                    InvitationCapabilityProbeOutcome::Unsupported,
                    2_000,
                )
                .expect("complete");
        }
        for index in INVITATION_CAPABILITY_MAX_IN_FLIGHT..INVITATION_CAPABILITY_CACHE_MAX_ITEMS {
            let peer = format!("{index:032x}");
            owner.begin_probe(&peer, 2_001).expect("cache probe");
            owner
                .complete_probe(&peer, InvitationCapabilityProbeOutcome::Unknown, 2_002)
                .expect("cache completion");
        }
        assert_eq!(owner.len(), INVITATION_CAPABILITY_CACHE_MAX_ITEMS);
        assert!(owner.accounted_bytes() <= INVITATION_CAPABILITY_CACHE_MAX_ACCOUNTED_BYTES);
        assert_eq!(
            owner.begin_probe("ffffffffffffffffffffffffffffffff", 2_003),
            Err(InvitationCapabilityEvidenceError::CacheCapacity)
        );
        owner.clear();
        assert!(owner.is_empty());
        assert_eq!(owner.accounted_bytes(), 0);
    }

    #[test]
    fn invalid_peer_and_late_completion_are_rejected() {
        let mut owner = InvitationCapabilityEvidenceOwner::default();
        assert_eq!(
            owner.begin_probe("ABCDEF", 0),
            Err(InvitationCapabilityEvidenceError::InvalidPeer)
        );
        owner.begin_probe(PEER, 1_000).expect("begin");
        assert_eq!(
            owner.complete_probe(
                PEER,
                InvitationCapabilityProbeOutcome::Supported,
                1_000 + INVITATION_CAPABILITY_PROBE_DEADLINE_MS,
            ),
            Err(InvitationCapabilityEvidenceError::NotChecking)
        );
        assert_eq!(owner.state(PEER, 16_000), InvitationCapabilityState::Stale);
    }
}
