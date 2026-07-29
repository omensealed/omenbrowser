use crate::{FrameValue, Revision, RoomId};

pub const ANNOUNCEMENT_ROOMS_CAPABILITY: &str = "announcement-rooms-v1";
pub const ROOM_SLOW_MODE_CAPABILITY: &str = "room-slow-mode-v1";
pub const ROOM_MEDIA_POLICY_CAPABILITY: &str = "room-media-policy-v1";
pub const ROOM_POLICY_ANNOUNCEMENT: u64 = 0x01;
pub const ROOM_POLICY_KNOWN_MASK: u64 = ROOM_POLICY_ANNOUNCEMENT;
pub const ROOM_SLOW_MODE_MAX_SECONDS: u32 = 86_400;
pub const ROOM_UPLOAD_MAX_FILE_BYTES: u64 = 10 * 1024 * 1024;
pub const ROOM_NAME_MAX_BYTES: usize = 64;
pub const ROOM_TOPIC_MAX_BYTES: usize = 4 * 1024;

const LEGACY_ROOM_VALUE_FIELDS: usize = 4;
const POLICY_ROOM_VALUE_FIELDS: usize = 5;
const SLOW_MODE_ROOM_VALUE_FIELDS: usize = 6;
const MEDIA_POLICY_ROOM_VALUE_FIELDS: usize = 7;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RoomCatalogShape {
    #[default]
    Legacy,
    PolicyBits,
    SlowMode,
    MediaPolicy,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RoomPolicyProjection {
    policy_bits: u64,
    slow_mode_seconds: u32,
    upload_policy: Option<RoomUploadPolicyProjection>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RoomUploadPolicyProjection {
    Inherit,
    Disabled,
    MaximumFileBytes(u64),
}

impl RoomPolicyProjection {
    pub fn new(policy_bits: u64, slow_mode_seconds: u32) -> Result<Self, RoomPolicyError> {
        Self::new_with_upload_policy(policy_bits, slow_mode_seconds, None)
    }

    pub fn new_with_upload_policy(
        policy_bits: u64,
        slow_mode_seconds: u32,
        upload_max_file_bytes: Option<Option<u64>>,
    ) -> Result<Self, RoomPolicyError> {
        if policy_bits & !ROOM_POLICY_KNOWN_MASK != 0 {
            return Err(RoomPolicyError::UnknownPolicyBits(policy_bits));
        }
        if slow_mode_seconds > ROOM_SLOW_MODE_MAX_SECONDS {
            return Err(RoomPolicyError::InvalidSlowMode);
        }
        let upload_policy = upload_max_file_bytes
            .map(RoomUploadPolicyProjection::from_configured_max_file_bytes)
            .transpose()?;
        Ok(Self {
            policy_bits,
            slow_mode_seconds,
            upload_policy,
        })
    }

    pub fn policy_bits(self) -> u64 {
        self.policy_bits
    }

    pub fn slow_mode_seconds(self) -> u32 {
        self.slow_mode_seconds
    }

    pub fn announcement_only(self) -> bool {
        self.policy_bits & ROOM_POLICY_ANNOUNCEMENT != 0
    }

    pub fn slow_mode_enabled(self) -> bool {
        self.slow_mode_seconds != 0
    }

    pub fn upload_policy(self) -> Option<RoomUploadPolicyProjection> {
        self.upload_policy
    }
}

impl RoomUploadPolicyProjection {
    fn from_configured_max_file_bytes(configured: Option<u64>) -> Result<Self, RoomPolicyError> {
        match configured {
            None => Ok(Self::Inherit),
            Some(0) => Ok(Self::Disabled),
            Some(bytes) if bytes <= ROOM_UPLOAD_MAX_FILE_BYTES => Ok(Self::MaximumFileBytes(bytes)),
            Some(_) => Err(RoomPolicyError::InvalidUploadMaxFileBytes),
        }
    }

    pub fn configured_max_file_bytes(self) -> Option<u64> {
        match self {
            Self::Inherit => None,
            Self::Disabled => Some(0),
            Self::MaximumFileBytes(bytes) => Some(bytes),
        }
    }

    pub fn effective_max_file_bytes(self, global_max_file_bytes: u64) -> Option<u64> {
        if global_max_file_bytes == 0 {
            return None;
        }
        match self {
            Self::Inherit => Some(global_max_file_bytes),
            Self::Disabled => None,
            Self::MaximumFileBytes(bytes) => Some(bytes.min(global_max_file_bytes)),
        }
    }

    pub fn uploads_disabled(self) -> bool {
        matches!(self, Self::Disabled)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RoomCatalogEntry {
    pub room_id: RoomId,
    pub name: String,
    pub topic: Option<String>,
    pub room_revision: Revision,
    pub policy_bits: u64,
    pub slow_mode_seconds: u32,
    pub upload_max_file_bytes: Option<u64>,
}

impl RoomCatalogEntry {
    pub fn announcement_only(&self) -> bool {
        self.policy_bits & ROOM_POLICY_ANNOUNCEMENT != 0
    }

    pub fn policy_projection(&self) -> Result<RoomPolicyProjection, RoomPolicyError> {
        RoomPolicyProjection::new(self.policy_bits, self.slow_mode_seconds)
    }

    pub fn policy_projection_for_shape(
        &self,
        shape: RoomCatalogShape,
    ) -> Result<Option<RoomPolicyProjection>, RoomPolicyError> {
        match shape {
            RoomCatalogShape::Legacy => Ok(None),
            RoomCatalogShape::PolicyBits | RoomCatalogShape::SlowMode => {
                self.policy_projection().map(Some)
            }
            RoomCatalogShape::MediaPolicy => RoomPolicyProjection::new_with_upload_policy(
                self.policy_bits,
                self.slow_mode_seconds,
                Some(self.upload_max_file_bytes),
            )
            .map(Some),
        }
    }

    pub fn into_frame_value(self, policy_negotiated: bool) -> Result<FrameValue, RoomPolicyError> {
        self.into_frame_value_for_shape(if policy_negotiated {
            RoomCatalogShape::PolicyBits
        } else {
            RoomCatalogShape::Legacy
        })
    }

    pub fn into_frame_value_for_shape(
        self,
        shape: RoomCatalogShape,
    ) -> Result<FrameValue, RoomPolicyError> {
        self.validate()?;
        let mut fields = Vec::with_capacity(match shape {
            RoomCatalogShape::Legacy => LEGACY_ROOM_VALUE_FIELDS,
            RoomCatalogShape::PolicyBits => POLICY_ROOM_VALUE_FIELDS,
            RoomCatalogShape::SlowMode => SLOW_MODE_ROOM_VALUE_FIELDS,
            RoomCatalogShape::MediaPolicy => MEDIA_POLICY_ROOM_VALUE_FIELDS,
        });
        fields.push(FrameValue::U64(u64::from(self.room_id)));
        fields.push(FrameValue::String(self.name));
        fields.push(
            self.topic
                .map(FrameValue::String)
                .unwrap_or(FrameValue::Nil),
        );
        fields.push(FrameValue::U64(self.room_revision));
        if shape != RoomCatalogShape::Legacy {
            fields.push(FrameValue::U64(self.policy_bits));
        }
        if matches!(
            shape,
            RoomCatalogShape::SlowMode | RoomCatalogShape::MediaPolicy
        ) {
            fields.push(FrameValue::U64(u64::from(self.slow_mode_seconds)));
        }
        if shape == RoomCatalogShape::MediaPolicy {
            fields.push(
                self.upload_max_file_bytes
                    .map(FrameValue::U64)
                    .unwrap_or(FrameValue::Nil),
            );
        }
        Ok(FrameValue::Array(fields))
    }

    pub fn from_frame_value(
        value: &FrameValue,
        policy_negotiated: bool,
    ) -> Result<Self, RoomPolicyError> {
        Self::from_frame_value_for_shape(
            value,
            if policy_negotiated {
                RoomCatalogShape::PolicyBits
            } else {
                RoomCatalogShape::Legacy
            },
        )
    }

    pub fn from_frame_value_for_shape(
        value: &FrameValue,
        shape: RoomCatalogShape,
    ) -> Result<Self, RoomPolicyError> {
        let FrameValue::Array(fields) = value else {
            return Err(RoomPolicyError::InvalidShape);
        };
        let expected_fields = match shape {
            RoomCatalogShape::Legacy => LEGACY_ROOM_VALUE_FIELDS,
            RoomCatalogShape::PolicyBits => POLICY_ROOM_VALUE_FIELDS,
            RoomCatalogShape::SlowMode => SLOW_MODE_ROOM_VALUE_FIELDS,
            RoomCatalogShape::MediaPolicy => MEDIA_POLICY_ROOM_VALUE_FIELDS,
        };
        if fields.len() != expected_fields {
            return Err(RoomPolicyError::InvalidShape);
        }
        let [FrameValue::U64(room_id), FrameValue::String(name), topic, FrameValue::U64(room_revision), rest @ ..] =
            fields.as_slice()
        else {
            return Err(RoomPolicyError::InvalidShape);
        };
        let (policy_bits, slow_mode_seconds, upload_max_file_bytes) = match (shape, rest) {
            (RoomCatalogShape::Legacy, []) => (0, 0, None),
            (RoomCatalogShape::PolicyBits, [FrameValue::U64(policy_bits)]) => {
                (*policy_bits, 0, None)
            }
            (
                RoomCatalogShape::SlowMode,
                [FrameValue::U64(policy_bits), FrameValue::U64(slow_mode_seconds)],
            ) => (
                *policy_bits,
                u32::try_from(*slow_mode_seconds).map_err(|_| RoomPolicyError::InvalidSlowMode)?,
                None,
            ),
            (
                RoomCatalogShape::MediaPolicy,
                [FrameValue::U64(policy_bits), FrameValue::U64(slow_mode_seconds), upload_max_file_bytes],
            ) => (
                *policy_bits,
                u32::try_from(*slow_mode_seconds).map_err(|_| RoomPolicyError::InvalidSlowMode)?,
                match upload_max_file_bytes {
                    FrameValue::Nil => None,
                    FrameValue::U64(bytes) => Some(*bytes),
                    _ => return Err(RoomPolicyError::InvalidUploadMaxFileBytes),
                },
            ),
            _ => return Err(RoomPolicyError::InvalidShape),
        };
        let entry = Self {
            room_id: u32::try_from(*room_id).map_err(|_| RoomPolicyError::InvalidRoomId)?,
            name: name.clone(),
            topic: match topic {
                FrameValue::Nil => None,
                FrameValue::String(topic) => Some(topic.clone()),
                _ => return Err(RoomPolicyError::InvalidShape),
            },
            room_revision: *room_revision,
            policy_bits,
            slow_mode_seconds,
            upload_max_file_bytes,
        };
        entry.validate()?;
        Ok(entry)
    }

    fn validate(&self) -> Result<(), RoomPolicyError> {
        if self.room_id == 0 {
            return Err(RoomPolicyError::InvalidRoomId);
        }
        if self.name.trim().is_empty() || self.name.len() > ROOM_NAME_MAX_BYTES {
            return Err(RoomPolicyError::InvalidName);
        }
        if self
            .topic
            .as_ref()
            .is_some_and(|topic| topic.trim().is_empty() || topic.len() > ROOM_TOPIC_MAX_BYTES)
        {
            return Err(RoomPolicyError::InvalidTopic);
        }
        if self.policy_bits & !ROOM_POLICY_KNOWN_MASK != 0 {
            return Err(RoomPolicyError::UnknownPolicyBits(self.policy_bits));
        }
        if self.slow_mode_seconds > ROOM_SLOW_MODE_MAX_SECONDS {
            return Err(RoomPolicyError::InvalidSlowMode);
        }
        if self
            .upload_max_file_bytes
            .is_some_and(|bytes| bytes > ROOM_UPLOAD_MAX_FILE_BYTES)
        {
            return Err(RoomPolicyError::InvalidUploadMaxFileBytes);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum RoomMediaUploadRejectCode {
    UploadsDisabled = 1,
    FileSizeExceeded = 2,
}

impl TryFrom<u64> for RoomMediaUploadRejectCode {
    type Error = RoomPolicyError;

    fn try_from(value: u64) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::UploadsDisabled),
            2 => Ok(Self::FileSizeExceeded),
            _ => Err(RoomPolicyError::UnknownUploadRejectCode(value)),
        }
    }
}

#[derive(Clone, Debug, thiserror::Error, PartialEq, Eq)]
pub enum RoomPolicyError {
    #[error("room catalog value has an invalid shape")]
    InvalidShape,
    #[error("room catalog id must be a nonzero u32")]
    InvalidRoomId,
    #[error("room catalog name is empty or exceeds {ROOM_NAME_MAX_BYTES} bytes")]
    InvalidName,
    #[error("room catalog topic is empty or exceeds {ROOM_TOPIC_MAX_BYTES} bytes")]
    InvalidTopic,
    #[error("room catalog policy contains unknown bits 0x{0:x}")]
    UnknownPolicyBits(u64),
    #[error("room catalog slow mode exceeds {ROOM_SLOW_MODE_MAX_SECONDS} seconds")]
    InvalidSlowMode,
    #[error("room upload maximum must be nil or no more than {ROOM_UPLOAD_MAX_FILE_BYTES} bytes")]
    InvalidUploadMaxFileBytes,
    #[error("unknown room media upload rejection code {0}")]
    UnknownUploadRejectCode(u64),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn announcement_room() -> RoomCatalogEntry {
        RoomCatalogEntry {
            room_id: 7,
            name: "announcements".into(),
            topic: Some("Operator updates".into()),
            room_revision: 3,
            policy_bits: ROOM_POLICY_ANNOUNCEMENT,
            slow_mode_seconds: 0,
            upload_max_file_bytes: None,
        }
    }

    #[test]
    fn negotiated_policy_value_round_trips_and_legacy_value_stays_four_fields() {
        let room = announcement_room();
        let negotiated = room
            .clone()
            .into_frame_value(true)
            .expect("negotiated value");
        assert_eq!(
            RoomCatalogEntry::from_frame_value(&negotiated, true),
            Ok(room.clone())
        );
        let legacy = room.clone().into_frame_value(false).expect("legacy value");
        let FrameValue::Array(fields) = &legacy else {
            panic!("room value must be an array");
        };
        assert_eq!(fields.len(), LEGACY_ROOM_VALUE_FIELDS);
        assert_eq!(
            RoomCatalogEntry::from_frame_value(&legacy, false),
            Ok(RoomCatalogEntry {
                policy_bits: 0,
                slow_mode_seconds: 0,
                upload_max_file_bytes: None,
                ..room
            })
        );
    }

    #[test]
    fn slow_mode_shape_is_explicit_bounded_and_round_trips() {
        let room = RoomCatalogEntry {
            slow_mode_seconds: 30,
            ..announcement_room()
        };
        let value = room
            .clone()
            .into_frame_value_for_shape(RoomCatalogShape::SlowMode)
            .expect("slow-mode value");
        assert_eq!(
            RoomCatalogEntry::from_frame_value_for_shape(&value, RoomCatalogShape::SlowMode),
            Ok(room.clone())
        );
        assert_eq!(
            RoomCatalogEntry::from_frame_value_for_shape(&value, RoomCatalogShape::PolicyBits),
            Err(RoomPolicyError::InvalidShape)
        );

        let FrameValue::Array(mut fields) = value else {
            panic!("room value must be an array");
        };
        fields[5] = FrameValue::U64(u64::from(ROOM_SLOW_MODE_MAX_SECONDS) + 1);
        assert_eq!(
            RoomCatalogEntry::from_frame_value_for_shape(
                &FrameValue::Array(fields),
                RoomCatalogShape::SlowMode
            ),
            Err(RoomPolicyError::InvalidSlowMode)
        );
    }

    #[test]
    fn media_policy_shape_is_explicit_bounded_and_round_trips() {
        for upload_max_file_bytes in [None, Some(0), Some(256 * 1024)] {
            let room = RoomCatalogEntry {
                slow_mode_seconds: 30,
                upload_max_file_bytes,
                ..announcement_room()
            };
            let value = room
                .clone()
                .into_frame_value_for_shape(RoomCatalogShape::MediaPolicy)
                .expect("media-policy value");
            assert_eq!(
                RoomCatalogEntry::from_frame_value_for_shape(&value, RoomCatalogShape::MediaPolicy),
                Ok(room)
            );
            assert_eq!(
                RoomCatalogEntry::from_frame_value_for_shape(&value, RoomCatalogShape::SlowMode),
                Err(RoomPolicyError::InvalidShape)
            );
        }

        let value = RoomCatalogEntry {
            upload_max_file_bytes: Some(ROOM_UPLOAD_MAX_FILE_BYTES),
            ..announcement_room()
        }
        .into_frame_value_for_shape(RoomCatalogShape::MediaPolicy)
        .expect("maximum media-policy value");
        let FrameValue::Array(mut fields) = value else {
            panic!("room value must be an array");
        };
        fields[6] = FrameValue::U64(ROOM_UPLOAD_MAX_FILE_BYTES + 1);
        assert_eq!(
            RoomCatalogEntry::from_frame_value_for_shape(
                &FrameValue::Array(fields),
                RoomCatalogShape::MediaPolicy
            ),
            Err(RoomPolicyError::InvalidUploadMaxFileBytes)
        );

        let value = RoomCatalogEntry {
            upload_max_file_bytes: Some(1),
            ..announcement_room()
        }
        .into_frame_value_for_shape(RoomCatalogShape::MediaPolicy)
        .expect("typed media-policy value");
        let FrameValue::Array(mut fields) = value else {
            panic!("room value must be an array");
        };
        fields[6] = FrameValue::Bool(false);
        assert_eq!(
            RoomCatalogEntry::from_frame_value_for_shape(
                &FrameValue::Array(fields),
                RoomCatalogShape::MediaPolicy
            ),
            Err(RoomPolicyError::InvalidUploadMaxFileBytes)
        );
    }

    #[test]
    fn media_policy_rejection_codes_are_stable_and_fail_closed() {
        assert_eq!(RoomMediaUploadRejectCode::UploadsDisabled as u8, 1);
        assert_eq!(RoomMediaUploadRejectCode::FileSizeExceeded as u8, 2);
        assert_eq!(
            RoomMediaUploadRejectCode::try_from(1),
            Ok(RoomMediaUploadRejectCode::UploadsDisabled)
        );
        assert_eq!(
            RoomMediaUploadRejectCode::try_from(2),
            Ok(RoomMediaUploadRejectCode::FileSizeExceeded)
        );
        assert_eq!(
            RoomMediaUploadRejectCode::try_from(3),
            Err(RoomPolicyError::UnknownUploadRejectCode(3))
        );
    }

    #[test]
    fn room_policy_projection_is_typed_bounded_and_value_only() {
        let ordinary = RoomPolicyProjection::default();
        assert_eq!(ordinary.policy_bits(), 0);
        assert_eq!(ordinary.slow_mode_seconds(), 0);
        assert_eq!(ordinary.upload_policy(), None);
        assert!(!ordinary.announcement_only());
        assert!(!ordinary.slow_mode_enabled());

        let projected =
            RoomPolicyProjection::new(ROOM_POLICY_ANNOUNCEMENT, 30).expect("bounded room policy");
        assert_eq!(projected.policy_bits(), ROOM_POLICY_ANNOUNCEMENT);
        assert_eq!(projected.slow_mode_seconds(), 30);
        assert_eq!(projected.upload_policy(), None);
        assert!(projected.announcement_only());
        assert!(projected.slow_mode_enabled());
        assert_eq!(
            RoomCatalogEntry {
                slow_mode_seconds: 30,
                ..announcement_room()
            }
            .policy_projection(),
            Ok(projected)
        );

        assert_eq!(
            RoomPolicyProjection::new(ROOM_POLICY_KNOWN_MASK << 1, 0),
            Err(RoomPolicyError::UnknownPolicyBits(
                ROOM_POLICY_KNOWN_MASK << 1
            ))
        );
        assert_eq!(
            RoomPolicyProjection::new(0, ROOM_SLOW_MODE_MAX_SECONDS + 1),
            Err(RoomPolicyError::InvalidSlowMode)
        );
    }

    #[test]
    fn room_upload_policy_projection_distinguishes_unavailable_inherit_disabled_and_maximum() {
        let unavailable = RoomPolicyProjection::new(0, 0).expect("legacy policy projection");
        assert_eq!(unavailable.upload_policy(), None);

        for (configured, expected, effective) in [
            (None, RoomUploadPolicyProjection::Inherit, Some(512 * 1024)),
            (Some(0), RoomUploadPolicyProjection::Disabled, None),
            (
                Some(256 * 1024),
                RoomUploadPolicyProjection::MaximumFileBytes(256 * 1024),
                Some(256 * 1024),
            ),
        ] {
            let projection = RoomPolicyProjection::new_with_upload_policy(0, 0, Some(configured))
                .expect("bounded upload policy projection");
            let policy = projection.upload_policy().expect("available policy");
            assert_eq!(policy, expected);
            assert_eq!(policy.configured_max_file_bytes(), configured);
            assert_eq!(policy.effective_max_file_bytes(512 * 1024), effective);
            assert_eq!(policy.uploads_disabled(), configured == Some(0));
        }

        assert_eq!(
            RoomPolicyProjection::new_with_upload_policy(
                0,
                0,
                Some(Some(ROOM_UPLOAD_MAX_FILE_BYTES + 1))
            ),
            Err(RoomPolicyError::InvalidUploadMaxFileBytes)
        );
        assert_eq!(
            RoomUploadPolicyProjection::Inherit.effective_max_file_bytes(0),
            None
        );

        let media_entry = RoomCatalogEntry {
            upload_max_file_bytes: None,
            ..announcement_room()
        };
        assert_eq!(
            media_entry.policy_projection_for_shape(RoomCatalogShape::MediaPolicy),
            RoomPolicyProjection::new_with_upload_policy(ROOM_POLICY_ANNOUNCEMENT, 0, Some(None))
                .map(Some)
        );
        assert_eq!(
            media_entry.policy_projection_for_shape(RoomCatalogShape::SlowMode),
            media_entry.policy_projection().map(Some)
        );
        assert_eq!(
            media_entry.policy_projection_for_shape(RoomCatalogShape::Legacy),
            Ok(None)
        );
    }

    #[test]
    fn negotiation_shape_is_explicit_and_unknown_policy_fails_closed() {
        let negotiated = announcement_room()
            .into_frame_value(true)
            .expect("negotiated value");
        assert_eq!(
            RoomCatalogEntry::from_frame_value(&negotiated, false),
            Err(RoomPolicyError::InvalidShape)
        );
        let FrameValue::Array(mut fields) = negotiated else {
            panic!("room value must be an array");
        };
        fields[4] = FrameValue::U64(ROOM_POLICY_KNOWN_MASK << 1);
        assert!(matches!(
            RoomCatalogEntry::from_frame_value(&FrameValue::Array(fields), true),
            Err(RoomPolicyError::UnknownPolicyBits(_))
        ));
    }

    #[test]
    fn room_value_bounds_reject_before_projection() {
        for invalid in [
            RoomCatalogEntry {
                room_id: 0,
                ..announcement_room()
            },
            RoomCatalogEntry {
                name: "n".repeat(ROOM_NAME_MAX_BYTES + 1),
                ..announcement_room()
            },
            RoomCatalogEntry {
                topic: Some("t".repeat(ROOM_TOPIC_MAX_BYTES + 1)),
                ..announcement_room()
            },
            RoomCatalogEntry {
                slow_mode_seconds: ROOM_SLOW_MODE_MAX_SECONDS + 1,
                ..announcement_room()
            },
            RoomCatalogEntry {
                upload_max_file_bytes: Some(ROOM_UPLOAD_MAX_FILE_BYTES + 1),
                ..announcement_room()
            },
        ] {
            assert!(invalid
                .into_frame_value_for_shape(RoomCatalogShape::MediaPolicy)
                .is_err());
        }
    }
}
