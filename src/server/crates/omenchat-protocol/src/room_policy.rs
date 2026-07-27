use crate::{FrameValue, Revision, RoomId};

pub const ANNOUNCEMENT_ROOMS_CAPABILITY: &str = "announcement-rooms-v1";
pub const ROOM_POLICY_ANNOUNCEMENT: u64 = 0x01;
pub const ROOM_POLICY_KNOWN_MASK: u64 = ROOM_POLICY_ANNOUNCEMENT;
pub const ROOM_NAME_MAX_BYTES: usize = 64;
pub const ROOM_TOPIC_MAX_BYTES: usize = 4 * 1024;

const LEGACY_ROOM_VALUE_FIELDS: usize = 4;
const POLICY_ROOM_VALUE_FIELDS: usize = 5;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RoomCatalogEntry {
    pub room_id: RoomId,
    pub name: String,
    pub topic: Option<String>,
    pub room_revision: Revision,
    pub policy_bits: u64,
}

impl RoomCatalogEntry {
    pub fn announcement_only(&self) -> bool {
        self.policy_bits & ROOM_POLICY_ANNOUNCEMENT != 0
    }

    pub fn into_frame_value(self, policy_negotiated: bool) -> Result<FrameValue, RoomPolicyError> {
        self.validate()?;
        let mut fields = Vec::with_capacity(if policy_negotiated {
            POLICY_ROOM_VALUE_FIELDS
        } else {
            LEGACY_ROOM_VALUE_FIELDS
        });
        fields.push(FrameValue::U64(u64::from(self.room_id)));
        fields.push(FrameValue::String(self.name));
        fields.push(
            self.topic
                .map(FrameValue::String)
                .unwrap_or(FrameValue::Nil),
        );
        fields.push(FrameValue::U64(self.room_revision));
        if policy_negotiated {
            fields.push(FrameValue::U64(self.policy_bits));
        }
        Ok(FrameValue::Array(fields))
    }

    pub fn from_frame_value(
        value: &FrameValue,
        policy_negotiated: bool,
    ) -> Result<Self, RoomPolicyError> {
        let FrameValue::Array(fields) = value else {
            return Err(RoomPolicyError::InvalidShape);
        };
        let expected_fields = if policy_negotiated {
            POLICY_ROOM_VALUE_FIELDS
        } else {
            LEGACY_ROOM_VALUE_FIELDS
        };
        if fields.len() != expected_fields {
            return Err(RoomPolicyError::InvalidShape);
        }
        let [FrameValue::U64(room_id), FrameValue::String(name), topic, FrameValue::U64(room_revision), rest @ ..] =
            fields.as_slice()
        else {
            return Err(RoomPolicyError::InvalidShape);
        };
        let policy_bits = if policy_negotiated {
            match rest {
                [FrameValue::U64(policy_bits)] => *policy_bits,
                _ => return Err(RoomPolicyError::InvalidShape),
            }
        } else {
            if !rest.is_empty() {
                return Err(RoomPolicyError::InvalidShape);
            }
            0
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
        Ok(())
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
                ..room
            })
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
        ] {
            assert!(invalid.into_frame_value(true).is_err());
        }
    }
}
