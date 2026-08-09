//! Shared OMENchat protocol v1 wire types and compatibility fixtures.
//!
//! Crate API version `0.2.0` adds typed, explicitly negotiated nickname-colour
//! forms. It does not change [`PROTOCOL_VERSION`], [`PROTOCOL_NAME`], or any
//! legacy frame/user-list shape.
//!
//! This crate deliberately contains no transport, runtime, storage, GUI, TUI,
//! or server policy.

mod durable;
mod message_revisions;
mod moderation_audit;
mod negotiation;
mod nickname_colours;
mod pins;
mod reactions;
mod rich_message;
mod room_policy;

pub use durable::*;
pub use message_revisions::*;
pub use moderation_audit::*;
pub use negotiation::*;
pub use nickname_colours::*;
pub use pins::*;
pub use reactions::*;
pub use rich_message::*;
pub use room_policy::*;

pub type ServerId = String;
pub type RoomId = u32;
pub type UserId = u32;
pub type EventId = u64;
pub type Seq = u32;
pub type Revision = u64;

pub const PROTOCOL_VERSION: u8 = 1;
pub const PROTOCOL_NAME: &str = "omenchat-v0.1";

/// Capabilities requested by every durable-capable current client build.
///
/// Product-feature-gated room policy and moderation capabilities are appended
/// separately. Keeping this list in the shared wire crate lets both workspaces
/// detect capability drift without duplicating string literals.
pub const BASE_DURABLE_SESSION_CAPABILITIES: [&str; 7] = [
    DURABLE_MUTATION_CAPABILITY,
    DURABLE_NOTICE_ACK_CAPABILITY,
    REPLY_MENTIONS_CAPABILITY,
    REACTIONS_CAPABILITY,
    MESSAGE_REVISIONS_CAPABILITY,
    ROOM_PINS_CAPABILITY,
    NICKNAME_COLOURS_CAPABILITY,
];

/// Complete capability vocabulary implemented by the current protocol-v1
/// client/server pair. Negotiation remains explicit; this is not an
/// advertisement and does not activate a capability by itself.
pub const KNOWN_SESSION_CAPABILITIES: [&str; 11] = [
    DURABLE_MUTATION_CAPABILITY,
    DURABLE_NOTICE_ACK_CAPABILITY,
    REPLY_MENTIONS_CAPABILITY,
    REACTIONS_CAPABILITY,
    MESSAGE_REVISIONS_CAPABILITY,
    ROOM_PINS_CAPABILITY,
    ANNOUNCEMENT_ROOMS_CAPABILITY,
    ROOM_SLOW_MODE_CAPABILITY,
    ROOM_MEDIA_POLICY_CAPABILITY,
    MODERATION_AUDIT_CAPABILITY,
    NICKNAME_COLOURS_CAPABILITY,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Compression {
    None = 0,
    Bzip2 = 1,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u16)]
pub enum ChatOp {
    SessionOpen = 1,
    SessionAccept = 2,
    SessionReject = 3,
    JoinRoom = 10,
    JoinAccept = 11,
    PartRoom = 12,
    RoomSubscribe = 13,
    RoomUnsubscribe = 14,
    RoomMessage = 20,
    RoomAction = 21,
    RoomNotice = 22,
    RoomEvent = 23,
    MessageAck = 24,
    RoomReaction = 25,
    ReactionAck = 26,
    ReactionEvent = 27,
    ReactionSnapshotInline = 28,
    ReactionSnapshotResource = 29,
    UserListSnapshotInline = 30,
    UserListSnapshotResource = 31,
    UserDelta = 32,
    RoomDelta = 33,
    RoleDelta = 34,
    RoomMessageRevision = 35,
    MessageRevisionAck = 36,
    MessageRevisionEvent = 37,
    MessageRevisionSnapshotInline = 38,
    MessageRevisionSnapshotResource = 39,
    HistoryBefore = 40,
    HistoryInline = 41,
    HistoryResourceOffer = 42,
    HistoryEnd = 43,
    HistoryRecent = 44,
    HistoryCurrent = 45,
    RoomPin = 46,
    PinAck = 47,
    PinEvent = 48,
    PinSnapshot = 49,
    Command = 50,
    CommandResult = 51,
    ModerationAuditBefore = 52,
    ModerationAuditInline = 53,
    ModerationAuditResource = 54,
    ModerationAuditEnd = 55,
    ContactRequest = 60,
    ContactOffer = 61,
    ContactAccept = 62,
    ContactReject = 63,
    UploadOffer = 70,
    UploadAccept = 71,
    UploadReject = 72,
    UploadComplete = 73,
    UploadFetch = 74,
    UploadResourceOffer = 75,
    UploadInlineChunk = 76,
    NicknameColourSet = 77,
    NicknameColourAck = 78,
    NicknameColourEvent = 79,
    Error = 90,
    Ping = 100,
    Pong = 101,
}

impl TryFrom<u64> for ChatOp {
    type Error = ProtocolError;

    fn try_from(value: u64) -> Result<Self, ProtocolError> {
        match value {
            1 => Ok(Self::SessionOpen),
            2 => Ok(Self::SessionAccept),
            3 => Ok(Self::SessionReject),
            10 => Ok(Self::JoinRoom),
            11 => Ok(Self::JoinAccept),
            12 => Ok(Self::PartRoom),
            13 => Ok(Self::RoomSubscribe),
            14 => Ok(Self::RoomUnsubscribe),
            20 => Ok(Self::RoomMessage),
            21 => Ok(Self::RoomAction),
            22 => Ok(Self::RoomNotice),
            23 => Ok(Self::RoomEvent),
            24 => Ok(Self::MessageAck),
            25 => Ok(Self::RoomReaction),
            26 => Ok(Self::ReactionAck),
            27 => Ok(Self::ReactionEvent),
            28 => Ok(Self::ReactionSnapshotInline),
            29 => Ok(Self::ReactionSnapshotResource),
            30 => Ok(Self::UserListSnapshotInline),
            31 => Ok(Self::UserListSnapshotResource),
            32 => Ok(Self::UserDelta),
            33 => Ok(Self::RoomDelta),
            34 => Ok(Self::RoleDelta),
            35 => Ok(Self::RoomMessageRevision),
            36 => Ok(Self::MessageRevisionAck),
            37 => Ok(Self::MessageRevisionEvent),
            38 => Ok(Self::MessageRevisionSnapshotInline),
            39 => Ok(Self::MessageRevisionSnapshotResource),
            40 => Ok(Self::HistoryBefore),
            41 => Ok(Self::HistoryInline),
            42 => Ok(Self::HistoryResourceOffer),
            43 => Ok(Self::HistoryEnd),
            44 => Ok(Self::HistoryRecent),
            45 => Ok(Self::HistoryCurrent),
            46 => Ok(Self::RoomPin),
            47 => Ok(Self::PinAck),
            48 => Ok(Self::PinEvent),
            49 => Ok(Self::PinSnapshot),
            50 => Ok(Self::Command),
            51 => Ok(Self::CommandResult),
            52 => Ok(Self::ModerationAuditBefore),
            53 => Ok(Self::ModerationAuditInline),
            54 => Ok(Self::ModerationAuditResource),
            55 => Ok(Self::ModerationAuditEnd),
            60 => Ok(Self::ContactRequest),
            61 => Ok(Self::ContactOffer),
            62 => Ok(Self::ContactAccept),
            63 => Ok(Self::ContactReject),
            70 => Ok(Self::UploadOffer),
            71 => Ok(Self::UploadAccept),
            72 => Ok(Self::UploadReject),
            73 => Ok(Self::UploadComplete),
            74 => Ok(Self::UploadFetch),
            75 => Ok(Self::UploadResourceOffer),
            76 => Ok(Self::UploadInlineChunk),
            77 => Ok(Self::NicknameColourSet),
            78 => Ok(Self::NicknameColourAck),
            79 => Ok(Self::NicknameColourEvent),
            90 => Ok(Self::Error),
            100 => Ok(Self::Ping),
            101 => Ok(Self::Pong),
            _ => Err(ProtocolError::UnknownOp(value)),
        }
    }
}

impl TryFrom<u64> for Compression {
    type Error = ProtocolError;

    fn try_from(value: u64) -> Result<Self, ProtocolError> {
        match value {
            0 => Ok(Self::None),
            1 => Ok(Self::Bzip2),
            _ => Err(ProtocolError::UnknownCompression(value)),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u16)]
pub enum RoomEventCode {
    UserJoined = 1,
    UserParted = 2,
    UserQuit = 3,
    UserKicked = 4,
    UserBanned = 5,
    UserUnbanned = 6,
    TopicSet = 7,
    ModeChanged = 8,
    RoleChanged = 9,
    MessageEdited = 10,
    MessageDeleted = 11,
    RoomNotice = 12,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u16)]
pub enum ChatErrorCode {
    PermissionDenied = 1001,
    NotJoined = 1002,
    RoomNotFound = 1003,
    UserNotFound = 1004,
    RateLimited = 1005,
    HistoryUnavailable = 1006,
    MalformedFrame = 1007,
    UnsupportedProtocolVersion = 1008,
    CompressionUnsupported = 1009,
    ResourceUnavailable = 1010,
    DurableMutationNotNegotiated = 1011,
    DurableMutationMalformed = 1012,
    DurableMutationConflict = 1013,
    DurableMutationResultExpired = 1014,
    DurableMutationStoreBusy = 1015,
    RoomPolicyRestricted = 1016,
    SlowModeActive = 1017,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Frame {
    pub version: u8,
    pub op: ChatOp,
    pub flags: u16,
    pub seq: Seq,
    pub room_id: Option<RoomId>,
    pub body: FrameBody,
}

impl Frame {
    pub fn new(op: ChatOp, seq: Seq, room_id: Option<RoomId>, body: FrameBody) -> Self {
        Self {
            version: PROTOCOL_VERSION,
            op,
            flags: 0,
            seq,
            room_id,
            body,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum FrameBody {
    Empty,
    Text(String),
    Fields(Vec<FrameValue>),
}

#[derive(Clone, Debug, PartialEq)]
pub enum FrameValue {
    Nil,
    Bool(bool),
    U64(u64),
    I64(i64),
    String(String),
    Bytes(Vec<u8>),
    Array(Vec<FrameValue>),
}

#[derive(Clone, Debug, thiserror::Error, PartialEq, Eq)]
pub enum ProtocolError {
    #[error("unknown OMENchat op code {0}")]
    UnknownOp(u64),
    #[error("unknown OMENchat compression code {0}")]
    UnknownCompression(u64),
    #[error("malformed frame: {0}")]
    MalformedFrame(&'static str),
}

pub mod fixtures {
    pub mod reactions_v1 {
        pub const ROOM_REACTION_ADD: &[u8] =
            b"\x96\x01\x19\x00\x08\x07\x94\xabreaction-v1\x2a\xa5heart\x01";
    }

    pub mod message_revisions_v1 {
        pub const ROOM_MESSAGE_CORRECTION: &[u8] =
            b"\x96\x01\x23\x00\x09\x07\x94\xb3message-revision-v1\x2a\x01\xa6edited";
    }

    pub mod pins_v1 {
        pub const ROOM_PIN_ADD: &[u8] = b"\x96\x01\x2e\x00\x0a\x07\x93\xabroom-pin-v1\x2a\x01";
    }

    pub mod moderation_audit_v1 {
        pub const AUDIT_BEFORE: &[u8] =
            b"\x96\x01\x34\x00\x0b\x07\x93\xb3moderation-audit-v1\x2a\x32";
    }

    pub mod announcement_rooms_v1 {
        pub const LEGACY_ROOM_DELTA: &[u8] =
            b"\x96\x01\x21\x00\x0c\xc0\x91\x94\x07\xadannouncements\xb0Operator updates\x03";
        pub const POLICY_ROOM_DELTA: &[u8] =
            b"\x96\x01\x21\x00\x0c\xc0\x91\x95\x07\xadannouncements\xb0Operator updates\x03\x01";
    }

    pub mod room_slow_mode_v1 {
        pub const ROOM_DELTA: &[u8] =
            b"\x96\x01\x21\x00\x0c\xc0\x91\x96\x07\xadannouncements\xb0Operator updates\x03\x01\x1e";
    }

    pub mod room_media_policy_v1 {
        pub const ROOM_DELTA: &[u8] =
            b"\x96\x01\x21\x00\x0c\xc0\x91\x97\x07\xadannouncements\xb0Operator updates\x03\x01\x1e\xce\x00\x04\x00\x00";
        pub const UPLOAD_REJECT: &[u8] =
            b"\x96\x01\x48\x00\x0d\x07\x94\xd9#upload exceeds room file size limit\xce\x00\x04\x00\x00\xce\x00\x08\x00\x00\x02";
    }

    pub mod reply_mentions_v1 {
        pub const ROOM_MESSAGE: &[u8] =
            b"\x96\x01\x14\x00\x07\x07\x94\xa5hello\xb1reply-mentions-v1\x92\x07\x2a\x92\x02\x09";
    }

    pub mod v0_6_0_1 {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../fixtures/omenchat/v0_6_0_1_wire.rs"
        ));
    }

    pub mod v0_9_6_3 {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../fixtures/omenchat/v0_9_6_3_wire.rs"
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protocol_v1_public_numbers_and_fixture_labels_are_stable() {
        assert_eq!(PROTOCOL_VERSION, 1);
        assert_eq!(PROTOCOL_NAME, "omenchat-v0.1");
        assert_eq!(ChatOp::SessionOpen as u16, 1);
        assert_eq!(ChatOp::RoomMessage as u16, 20);
        assert_eq!(ChatOp::RoomReaction as u16, 25);
        assert_eq!(ChatOp::ReactionSnapshotResource as u16, 29);
        assert_eq!(ChatOp::RoomMessageRevision as u16, 35);
        assert_eq!(ChatOp::MessageRevisionSnapshotResource as u16, 39);
        assert_eq!(ChatOp::HistoryResourceOffer as u16, 42);
        assert_eq!(ChatOp::RoomPin as u16, 46);
        assert_eq!(ChatOp::PinSnapshot as u16, 49);
        assert_eq!(ChatOp::ModerationAuditBefore as u16, 52);
        assert_eq!(ChatOp::ModerationAuditEnd as u16, 55);
        assert_eq!(ChatOp::NicknameColourSet as u16, 77);
        assert_eq!(ChatOp::NicknameColourAck as u16, 78);
        assert_eq!(ChatOp::NicknameColourEvent as u16, 79);
        assert_eq!(ChatErrorCode::MalformedFrame as u16, 1007);
        assert_eq!(ChatErrorCode::DurableMutationNotNegotiated as u16, 1011);
        assert_eq!(DURABLE_MUTATION_CAPABILITY, "durable-mutations-v1");
        assert_eq!(REACTIONS_CAPABILITY, "reactions-v1");
        assert_eq!(MESSAGE_REVISIONS_CAPABILITY, "message-revisions-v1");
        assert_eq!(ROOM_PINS_CAPABILITY, "room-pins-v1");
        assert_eq!(MODERATION_AUDIT_CAPABILITY, "moderation-audit-v1");
        assert_eq!(ANNOUNCEMENT_ROOMS_CAPABILITY, "announcement-rooms-v1");
        assert_eq!(ROOM_SLOW_MODE_CAPABILITY, "room-slow-mode-v1");
        assert_eq!(ROOM_MEDIA_POLICY_CAPABILITY, "room-media-policy-v1");
        assert_eq!(NICKNAME_COLOURS_CAPABILITY, "nickname-colours-v1");
        assert_eq!(ROOM_UPLOAD_MAX_FILE_BYTES, 10 * 1024 * 1024);
        assert_eq!(DURABLE_NOTICE_ACK_CAPABILITY, "durable-room-notice-ack-v1");
        assert_eq!(ChatErrorCode::DurableMutationMalformed as u16, 1012);
        assert_eq!(ChatErrorCode::DurableMutationConflict as u16, 1013);
        assert_eq!(ChatErrorCode::DurableMutationResultExpired as u16, 1014);
        assert_eq!(ChatErrorCode::DurableMutationStoreBusy as u16, 1015);
        assert_eq!(ChatErrorCode::RoomPolicyRestricted as u16, 1016);
        assert_eq!(ChatErrorCode::SlowModeActive as u16, 1017);
        assert_eq!(fixtures::v0_6_0_1::PROTOCOL_VERSION, PROTOCOL_VERSION);
        assert_eq!(fixtures::v0_6_0_1::PROTOCOL_NAME, PROTOCOL_NAME);
        assert_eq!(fixtures::v0_6_0_1::LINK_CONTEXT, 0x4f);
        assert_eq!(
            fixtures::v0_6_0_1::RESOURCE_METADATA_PREFIX,
            b"omenchat-resource:"
        );
        assert!(!fixtures::v0_6_0_1::SESSION_OPEN.is_empty());
        assert!(!fixtures::v0_6_0_1::ROOM_MESSAGE.is_empty());
        assert!(!fixtures::v0_6_0_1::HISTORY_RESOURCE_OFFER.is_empty());
        assert_eq!(fixtures::v0_9_6_3::APPLICATION_VERSION, "0.9.6-3");
        assert_eq!(fixtures::v0_9_6_3::PROTOCOL_VERSION, PROTOCOL_VERSION);
        assert_eq!(fixtures::v0_9_6_3::PROTOCOL_NAME, PROTOCOL_NAME);
        assert!(!fixtures::v0_9_6_3::ORDINARY_ROOM_MESSAGE.is_empty());
    }

    #[test]
    fn authoritative_capability_vocabulary_is_unique_and_within_wire_bounds() {
        let unique = KNOWN_SESSION_CAPABILITIES
            .iter()
            .copied()
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(unique.len(), KNOWN_SESSION_CAPABILITIES.len());
        assert!(KNOWN_SESSION_CAPABILITIES.len() <= SESSION_CAPABILITY_MAX_ITEMS);
        assert!(KNOWN_SESSION_CAPABILITIES.iter().all(|capability| {
            capability.is_ascii()
                && !capability.is_empty()
                && capability.len() <= SESSION_CAPABILITY_MAX_BYTES
        }));
        assert!(BASE_DURABLE_SESSION_CAPABILITIES
            .iter()
            .all(|capability| unique.contains(capability)));
    }
}
