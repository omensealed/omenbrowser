use crate::{FrameBody, FrameValue, MutationId, Revision, UserId};

pub const NICKNAME_COLOURS_CAPABILITY: &str = "nickname-colours-v1";
pub const RGB24_MAX: u32 = 0x00ff_ffff;

/// A strict, transport-independent 24-bit sRGB preference.
///
/// `None` on the wire means the deterministic automatic colour. This type has
/// no theme, storage, UI, or runtime policy; those remain product concerns.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Rgb24(u32);

impl Rgb24 {
    pub const fn new(value: u32) -> Result<Self, NicknameColourError> {
        if value <= RGB24_MAX {
            Ok(Self(value))
        } else {
            Err(NicknameColourError::RgbOverflow)
        }
    }

    pub const fn get(self) -> u32 {
        self.0
    }
}

impl TryFrom<u64> for Rgb24 {
    type Error = NicknameColourError;

    fn try_from(value: u64) -> Result<Self, Self::Error> {
        let value = u32::try_from(value).map_err(|_| NicknameColourError::RgbOverflow)?;
        Self::new(value)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NicknameColourSet {
    pub colour: Option<Rgb24>,
}

impl NicknameColourSet {
    pub fn into_frame_body(self) -> FrameBody {
        FrameBody::Fields(vec![colour_value(self.colour)])
    }

    pub fn from_frame_body(body: &FrameBody) -> Result<Self, NicknameColourError> {
        let FrameBody::Fields(fields) = body else {
            return Err(NicknameColourError::InvalidSetShape);
        };
        let [colour] = fields.as_slice() else {
            return Err(NicknameColourError::InvalidSetShape);
        };
        Ok(Self {
            colour: parse_colour(colour)?,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NicknameColourAck {
    pub mutation_id: MutationId,
    pub profile_revision: Revision,
    pub colour: Option<Rgb24>,
}

impl NicknameColourAck {
    pub fn into_frame_body(self) -> FrameBody {
        FrameBody::Fields(vec![
            FrameValue::Bytes(self.mutation_id.into_bytes().to_vec()),
            FrameValue::U64(self.profile_revision),
            colour_value(self.colour),
        ])
    }

    pub fn from_frame_body(body: &FrameBody) -> Result<Self, NicknameColourError> {
        let FrameBody::Fields(fields) = body else {
            return Err(NicknameColourError::InvalidAckShape);
        };
        let [FrameValue::Bytes(mutation_id), FrameValue::U64(profile_revision), colour] =
            fields.as_slice()
        else {
            return Err(NicknameColourError::InvalidAckShape);
        };
        Ok(Self {
            mutation_id: MutationId::try_from(mutation_id.as_slice())?,
            profile_revision: *profile_revision,
            colour: parse_colour(colour)?,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NicknameColourEvent {
    pub user_id: UserId,
    pub profile_revision: Revision,
    pub colour: Option<Rgb24>,
}

impl NicknameColourEvent {
    pub fn into_frame_body(self) -> Result<FrameBody, NicknameColourError> {
        if self.user_id == 0 {
            return Err(NicknameColourError::InvalidUserId);
        }
        Ok(FrameBody::Fields(vec![
            FrameValue::U64(u64::from(self.user_id)),
            FrameValue::U64(self.profile_revision),
            colour_value(self.colour),
        ]))
    }

    pub fn from_frame_body(body: &FrameBody) -> Result<Self, NicknameColourError> {
        let FrameBody::Fields(fields) = body else {
            return Err(NicknameColourError::InvalidEventShape);
        };
        let [FrameValue::U64(user_id), FrameValue::U64(profile_revision), colour] =
            fields.as_slice()
        else {
            return Err(NicknameColourError::InvalidEventShape);
        };
        let user_id = u32::try_from(*user_id).map_err(|_| NicknameColourError::InvalidUserId)?;
        if user_id == 0 {
            return Err(NicknameColourError::InvalidUserId);
        }
        Ok(Self {
            user_id,
            profile_revision: *profile_revision,
            colour: parse_colour(colour)?,
        })
    }
}

fn colour_value(colour: Option<Rgb24>) -> FrameValue {
    colour
        .map(|colour| FrameValue::U64(u64::from(colour.get())))
        .unwrap_or(FrameValue::Nil)
}

fn parse_colour(value: &FrameValue) -> Result<Option<Rgb24>, NicknameColourError> {
    match value {
        FrameValue::Nil => Ok(None),
        FrameValue::U64(value) => Ok(Some(Rgb24::try_from(*value)?)),
        _ => Err(NicknameColourError::InvalidColourType),
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum NicknameColourError {
    #[error("nickname colour must be nil or an integer from 0x000000 through 0xFFFFFF")]
    InvalidColourType,
    #[error("nickname colour exceeds 0xFFFFFF")]
    RgbOverflow,
    #[error("nickname colour set must contain exactly one field")]
    InvalidSetShape,
    #[error("nickname colour acknowledgement must contain exactly three fields")]
    InvalidAckShape,
    #[error("nickname colour event must contain exactly three fields")]
    InvalidEventShape,
    #[error("nickname colour event user id is invalid")]
    InvalidUserId,
    #[error(transparent)]
    Durable(#[from] crate::DurableMutationError),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rgb24_accepts_exact_endpoints_and_rejects_overflow() {
        assert_eq!(Rgb24::new(0).unwrap().get(), 0);
        assert_eq!(Rgb24::new(RGB24_MAX).unwrap().get(), RGB24_MAX);
        assert_eq!(
            Rgb24::new(RGB24_MAX + 1),
            Err(NicknameColourError::RgbOverflow)
        );
        assert_eq!(
            Rgb24::try_from(u64::MAX),
            Err(NicknameColourError::RgbOverflow)
        );
    }

    #[test]
    fn set_supports_explicit_and_automatic_forms_strictly() {
        let explicit = NicknameColourSet {
            colour: Some(Rgb24::new(0x12_34_56).unwrap()),
        };
        assert_eq!(
            NicknameColourSet::from_frame_body(&explicit.into_frame_body()),
            Ok(explicit)
        );
        let automatic = NicknameColourSet { colour: None };
        assert_eq!(
            NicknameColourSet::from_frame_body(&automatic.into_frame_body()),
            Ok(automatic)
        );
        assert_eq!(
            NicknameColourSet::from_frame_body(&FrameBody::Fields(vec![FrameValue::I64(1)])),
            Err(NicknameColourError::InvalidColourType)
        );
        assert_eq!(
            NicknameColourSet::from_frame_body(&FrameBody::Fields(vec![
                FrameValue::Nil,
                FrameValue::Nil
            ])),
            Err(NicknameColourError::InvalidSetShape)
        );
    }

    #[test]
    fn ack_and_event_round_trip_exact_fields() {
        let ack = NicknameColourAck {
            mutation_id: MutationId::new([7; 16]),
            profile_revision: 9,
            colour: Some(Rgb24::new(RGB24_MAX).unwrap()),
        };
        assert_eq!(
            NicknameColourAck::from_frame_body(&ack.into_frame_body()),
            Ok(ack)
        );
        let event = NicknameColourEvent {
            user_id: 4,
            profile_revision: 10,
            colour: None,
        };
        assert_eq!(
            NicknameColourEvent::from_frame_body(&event.into_frame_body().unwrap()),
            Ok(event)
        );
    }

    #[test]
    fn malformed_ack_event_and_overflow_are_rejected() {
        assert_eq!(
            NicknameColourAck::from_frame_body(&FrameBody::Fields(Vec::new())),
            Err(NicknameColourError::InvalidAckShape)
        );
        assert_eq!(
            NicknameColourEvent::from_frame_body(&FrameBody::Fields(vec![
                FrameValue::U64(1),
                FrameValue::U64(1),
                FrameValue::U64(u64::from(RGB24_MAX) + 1),
            ])),
            Err(NicknameColourError::RgbOverflow)
        );
    }
}
