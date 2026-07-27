use crate::{EventId, FrameBody, FrameValue, RoomId, UserId};

pub const MODERATION_AUDIT_CAPABILITY: &str = "moderation-audit-v1";
pub const MODERATION_AUDIT_REQUEST_BODY_TAG: &str = "moderation-audit-v1";
pub const MODERATION_AUDIT_PAGE_MAX_ENTRIES: usize = 256;
pub const MODERATION_AUDIT_DISPLAY_NAME_MAX_BYTES: usize = 256;
pub const MODERATION_AUDIT_PAGE_MAX_RETAINED_BYTES: usize = 512 * 1024;
pub const MODERATION_AUDIT_ROLE_BITS_MASK: u64 = 0b111;
pub const MODERATION_AUDIT_STATUS_BITS_MASK: u32 = 0b11;

const MODERATION_AUDIT_RETAINED_ROW_OVERHEAD_BYTES: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum ModerationAuditAction {
    Kick = 1,
    Ban = 2,
    Unban = 3,
    Mute = 4,
    Unmute = 5,
    RoleChange = 6,
}

impl TryFrom<u64> for ModerationAuditAction {
    type Error = ModerationAuditError;

    fn try_from(value: u64) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::Kick),
            2 => Ok(Self::Ban),
            3 => Ok(Self::Unban),
            4 => Ok(Self::Mute),
            5 => Ok(Self::Unmute),
            6 => Ok(Self::RoleChange),
            _ => Err(ModerationAuditError::UnknownAction),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ModerationAuditRequest {
    pub before_audit_id: Option<EventId>,
    pub limit: u16,
}

impl ModerationAuditRequest {
    pub fn into_frame_body(self) -> Result<FrameBody, ModerationAuditError> {
        self.validate()?;
        Ok(FrameBody::Fields(vec![
            FrameValue::String(MODERATION_AUDIT_REQUEST_BODY_TAG.into()),
            optional_event_id_value(self.before_audit_id),
            FrameValue::U64(u64::from(self.limit)),
        ]))
    }

    pub fn from_frame_body(body: &FrameBody) -> Result<Self, ModerationAuditError> {
        let FrameBody::Fields(fields) = body else {
            return Err(ModerationAuditError::InvalidRequestShape);
        };
        let [FrameValue::String(tag), before_audit_id, FrameValue::U64(limit)] = fields.as_slice()
        else {
            return Err(ModerationAuditError::InvalidRequestShape);
        };
        if tag != MODERATION_AUDIT_REQUEST_BODY_TAG {
            return Err(ModerationAuditError::InvalidRequestTag);
        }
        let request = Self {
            before_audit_id: parse_optional_event_id(before_audit_id)?,
            limit: u16::try_from(*limit).map_err(|_| ModerationAuditError::InvalidPageLimit)?,
        };
        request.validate()?;
        Ok(request)
    }

    fn validate(&self) -> Result<(), ModerationAuditError> {
        if let Some(audit_id) = self.before_audit_id {
            validate_event_id(audit_id)?;
        }
        if self.limit == 0 || usize::from(self.limit) > MODERATION_AUDIT_PAGE_MAX_ENTRIES {
            return Err(ModerationAuditError::InvalidPageLimit);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModerationAuditRecord {
    pub audit_id: EventId,
    pub room_id: RoomId,
    pub actor_user_id: UserId,
    pub actor_display_name_at_action: String,
    pub target_user_id: Option<UserId>,
    pub target_display_name_at_action: Option<String>,
    pub action: ModerationAuditAction,
    pub committed_at_unix: i64,
    pub result_role_bits: Option<u64>,
    pub result_status_bits: Option<u32>,
}

impl ModerationAuditRecord {
    pub fn into_frame_value(self) -> Result<FrameValue, ModerationAuditError> {
        self.validate()?;
        Ok(FrameValue::Array(vec![
            FrameValue::U64(self.audit_id),
            FrameValue::U64(u64::from(self.room_id)),
            FrameValue::U64(u64::from(self.actor_user_id)),
            FrameValue::String(self.actor_display_name_at_action),
            optional_user_id_value(self.target_user_id),
            optional_string_value(self.target_display_name_at_action),
            FrameValue::U64(self.action as u8 as u64),
            timestamp_value(self.committed_at_unix)?,
            self.result_role_bits
                .map(FrameValue::U64)
                .unwrap_or(FrameValue::Nil),
            self.result_status_bits
                .map(|bits| FrameValue::U64(u64::from(bits)))
                .unwrap_or(FrameValue::Nil),
        ]))
    }

    pub fn from_frame_value(value: &FrameValue) -> Result<Self, ModerationAuditError> {
        let FrameValue::Array(fields) = value else {
            return Err(ModerationAuditError::InvalidRecordShape);
        };
        let [FrameValue::U64(audit_id), FrameValue::U64(room_id), FrameValue::U64(actor_user_id), FrameValue::String(actor_display_name), target_user_id, target_display_name, FrameValue::U64(action), committed_at, result_role_bits, result_status_bits] =
            fields.as_slice()
        else {
            return Err(ModerationAuditError::InvalidRecordShape);
        };
        let record = Self {
            audit_id: *audit_id,
            room_id: parse_room_id(*room_id)?,
            actor_user_id: parse_user_id(*actor_user_id)?,
            actor_display_name_at_action: actor_display_name.clone(),
            target_user_id: parse_optional_user_id(target_user_id)?,
            target_display_name_at_action: parse_optional_string(target_display_name)?,
            action: ModerationAuditAction::try_from(*action)?,
            committed_at_unix: parse_timestamp(committed_at)?,
            result_role_bits: parse_optional_role_bits(result_role_bits)?,
            result_status_bits: parse_optional_status_bits(result_status_bits)?,
        };
        record.validate()?;
        Ok(record)
    }

    pub fn retained_bytes(&self) -> usize {
        MODERATION_AUDIT_RETAINED_ROW_OVERHEAD_BYTES
            .saturating_add(self.actor_display_name_at_action.len())
            .saturating_add(
                self.target_display_name_at_action
                    .as_ref()
                    .map_or(0, String::len),
            )
    }

    fn validate(&self) -> Result<(), ModerationAuditError> {
        validate_event_id(self.audit_id)?;
        validate_room_id(self.room_id)?;
        validate_user_id(self.actor_user_id)?;
        validate_display_name(&self.actor_display_name_at_action)?;
        let Some(target_user_id) = self.target_user_id else {
            return Err(ModerationAuditError::MissingTarget);
        };
        validate_user_id(target_user_id)?;
        let Some(target_display_name) = self.target_display_name_at_action.as_deref() else {
            return Err(ModerationAuditError::MissingTarget);
        };
        validate_display_name(target_display_name)?;
        validate_timestamp(self.committed_at_unix)?;
        match self.action {
            ModerationAuditAction::Kick => {
                if self.result_role_bits.is_some() || self.result_status_bits.is_some() {
                    return Err(ModerationAuditError::ActionResultMismatch);
                }
            }
            ModerationAuditAction::Ban
            | ModerationAuditAction::Unban
            | ModerationAuditAction::Mute
            | ModerationAuditAction::Unmute => {
                if self.result_role_bits.is_some() || self.result_status_bits.is_none() {
                    return Err(ModerationAuditError::ActionResultMismatch);
                }
            }
            ModerationAuditAction::RoleChange => {
                if self.result_role_bits.is_none() || self.result_status_bits.is_some() {
                    return Err(ModerationAuditError::ActionResultMismatch);
                }
            }
        }
        if self
            .result_role_bits
            .is_some_and(|bits| bits & !MODERATION_AUDIT_ROLE_BITS_MASK != 0)
        {
            return Err(ModerationAuditError::InvalidRoleBits);
        }
        if self
            .result_status_bits
            .is_some_and(|bits| bits & !MODERATION_AUDIT_STATUS_BITS_MASK != 0)
        {
            return Err(ModerationAuditError::InvalidStatusBits);
        }
        match (self.action, self.result_role_bits, self.result_status_bits) {
            (ModerationAuditAction::Ban, _, Some(bits)) if bits & 0b01 == 0 => {
                return Err(ModerationAuditError::ActionResultMismatch);
            }
            (ModerationAuditAction::Unban, _, Some(bits)) if bits & 0b01 != 0 => {
                return Err(ModerationAuditError::ActionResultMismatch);
            }
            (ModerationAuditAction::Mute, _, Some(bits)) if bits & 0b10 == 0 => {
                return Err(ModerationAuditError::ActionResultMismatch);
            }
            (ModerationAuditAction::Unmute, _, Some(bits)) if bits & 0b10 != 0 => {
                return Err(ModerationAuditError::ActionResultMismatch);
            }
            (ModerationAuditAction::RoleChange, Some(bits), _)
                if !matches!(bits, 0 | 0b001 | 0b011 | 0b111) =>
            {
                return Err(ModerationAuditError::ActionResultMismatch);
            }
            _ => {}
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModerationAuditPage {
    pub records: Vec<ModerationAuditRecord>,
}

impl ModerationAuditPage {
    pub fn into_frame_values(self) -> Result<Vec<FrameValue>, ModerationAuditError> {
        self.validate()?;
        self.records
            .into_iter()
            .map(ModerationAuditRecord::into_frame_value)
            .collect()
    }

    pub fn from_frame_values(values: &[FrameValue]) -> Result<Self, ModerationAuditError> {
        if values.len() > MODERATION_AUDIT_PAGE_MAX_ENTRIES {
            return Err(ModerationAuditError::TooManyPageEntries);
        }
        let page = Self {
            records: values
                .iter()
                .map(ModerationAuditRecord::from_frame_value)
                .collect::<Result<Vec<_>, _>>()?,
        };
        page.validate()?;
        Ok(page)
    }

    pub fn validate_room(&self, room_id: RoomId) -> Result<(), ModerationAuditError> {
        validate_room_id(room_id)?;
        if self.records.iter().any(|record| record.room_id != room_id) {
            return Err(ModerationAuditError::PageRoomMismatch);
        }
        Ok(())
    }

    fn validate(&self) -> Result<(), ModerationAuditError> {
        if self.records.len() > MODERATION_AUDIT_PAGE_MAX_ENTRIES {
            return Err(ModerationAuditError::TooManyPageEntries);
        }
        let mut retained_bytes = 0usize;
        for record in &self.records {
            record.validate()?;
            retained_bytes = retained_bytes.saturating_add(record.retained_bytes());
            if retained_bytes > MODERATION_AUDIT_PAGE_MAX_RETAINED_BYTES {
                return Err(ModerationAuditError::PageBytesExceeded);
            }
        }
        if self
            .records
            .windows(2)
            .any(|pair| pair[0].audit_id <= pair[1].audit_id)
        {
            return Err(ModerationAuditError::NonCanonicalPageOrder);
        }
        Ok(())
    }
}

fn validate_event_id(event_id: EventId) -> Result<(), ModerationAuditError> {
    if event_id == 0 {
        Err(ModerationAuditError::InvalidAuditId)
    } else {
        Ok(())
    }
}

fn validate_room_id(room_id: RoomId) -> Result<(), ModerationAuditError> {
    if room_id == 0 {
        Err(ModerationAuditError::InvalidRoomId)
    } else {
        Ok(())
    }
}

fn validate_user_id(user_id: UserId) -> Result<(), ModerationAuditError> {
    if user_id == 0 {
        Err(ModerationAuditError::InvalidUserId)
    } else {
        Ok(())
    }
}

fn parse_room_id(value: u64) -> Result<RoomId, ModerationAuditError> {
    let room_id = RoomId::try_from(value).map_err(|_| ModerationAuditError::InvalidRoomId)?;
    validate_room_id(room_id)?;
    Ok(room_id)
}

fn parse_user_id(value: u64) -> Result<UserId, ModerationAuditError> {
    let user_id = UserId::try_from(value).map_err(|_| ModerationAuditError::InvalidUserId)?;
    validate_user_id(user_id)?;
    Ok(user_id)
}

fn optional_event_id_value(value: Option<EventId>) -> FrameValue {
    value.map(FrameValue::U64).unwrap_or(FrameValue::Nil)
}

fn optional_user_id_value(value: Option<UserId>) -> FrameValue {
    value
        .map(|user_id| FrameValue::U64(u64::from(user_id)))
        .unwrap_or(FrameValue::Nil)
}

fn optional_string_value(value: Option<String>) -> FrameValue {
    value.map(FrameValue::String).unwrap_or(FrameValue::Nil)
}

fn parse_optional_event_id(value: &FrameValue) -> Result<Option<EventId>, ModerationAuditError> {
    match value {
        FrameValue::Nil => Ok(None),
        FrameValue::U64(event_id) => {
            validate_event_id(*event_id)?;
            Ok(Some(*event_id))
        }
        _ => Err(ModerationAuditError::InvalidRequestShape),
    }
}

fn parse_optional_user_id(value: &FrameValue) -> Result<Option<UserId>, ModerationAuditError> {
    match value {
        FrameValue::Nil => Ok(None),
        FrameValue::U64(user_id) => Ok(Some(parse_user_id(*user_id)?)),
        _ => Err(ModerationAuditError::InvalidRecordShape),
    }
}

fn parse_optional_string(value: &FrameValue) -> Result<Option<String>, ModerationAuditError> {
    match value {
        FrameValue::Nil => Ok(None),
        FrameValue::String(value) => Ok(Some(value.clone())),
        _ => Err(ModerationAuditError::InvalidRecordShape),
    }
}

fn parse_optional_role_bits(value: &FrameValue) -> Result<Option<u64>, ModerationAuditError> {
    match value {
        FrameValue::Nil => Ok(None),
        FrameValue::U64(bits) if bits & !MODERATION_AUDIT_ROLE_BITS_MASK == 0 => Ok(Some(*bits)),
        FrameValue::U64(_) => Err(ModerationAuditError::InvalidRoleBits),
        _ => Err(ModerationAuditError::InvalidRecordShape),
    }
}

fn parse_optional_status_bits(value: &FrameValue) -> Result<Option<u32>, ModerationAuditError> {
    match value {
        FrameValue::Nil => Ok(None),
        FrameValue::U64(bits) => {
            let bits = u32::try_from(*bits).map_err(|_| ModerationAuditError::InvalidStatusBits)?;
            if bits & !MODERATION_AUDIT_STATUS_BITS_MASK != 0 {
                Err(ModerationAuditError::InvalidStatusBits)
            } else {
                Ok(Some(bits))
            }
        }
        _ => Err(ModerationAuditError::InvalidRecordShape),
    }
}

fn validate_display_name(value: &str) -> Result<(), ModerationAuditError> {
    if value.is_empty() || value.len() > MODERATION_AUDIT_DISPLAY_NAME_MAX_BYTES {
        Err(ModerationAuditError::InvalidDisplayName)
    } else {
        Ok(())
    }
}

fn timestamp_value(timestamp: i64) -> Result<FrameValue, ModerationAuditError> {
    validate_timestamp(timestamp)?;
    Ok(FrameValue::U64(timestamp as u64))
}

fn parse_timestamp(value: &FrameValue) -> Result<i64, ModerationAuditError> {
    match value {
        FrameValue::U64(value) => {
            i64::try_from(*value).map_err(|_| ModerationAuditError::InvalidTimestamp)
        }
        FrameValue::I64(value) => {
            validate_timestamp(*value)?;
            Ok(*value)
        }
        _ => Err(ModerationAuditError::InvalidTimestamp),
    }
}

fn validate_timestamp(timestamp: i64) -> Result<(), ModerationAuditError> {
    if timestamp < 0 {
        Err(ModerationAuditError::InvalidTimestamp)
    } else {
        Ok(())
    }
}

#[derive(Clone, Debug, thiserror::Error, PartialEq, Eq)]
pub enum ModerationAuditError {
    #[error("moderation-audit request must use the exact tagged three-field shape")]
    InvalidRequestShape,
    #[error("moderation-audit request has an unknown extension tag")]
    InvalidRequestTag,
    #[error(
        "moderation-audit page limit must be between 1 and {MODERATION_AUDIT_PAGE_MAX_ENTRIES}"
    )]
    InvalidPageLimit,
    #[error("moderation-audit id must be nonzero")]
    InvalidAuditId,
    #[error("moderation-audit room id must be nonzero and fit u32")]
    InvalidRoomId,
    #[error("moderation-audit user id must be nonzero and fit u32")]
    InvalidUserId,
    #[error("moderation-audit action is unknown")]
    UnknownAction,
    #[error("moderation-audit record must use the exact ten-field shape")]
    InvalidRecordShape,
    #[error("moderation-audit target identity and display name are required")]
    MissingTarget,
    #[error(
        "moderation-audit display name must be nonempty and at most {MODERATION_AUDIT_DISPLAY_NAME_MAX_BYTES} bytes"
    )]
    InvalidDisplayName,
    #[error("moderation-audit timestamp must be a nonnegative i64")]
    InvalidTimestamp,
    #[error("moderation-audit action and result fields disagree")]
    ActionResultMismatch,
    #[error("moderation-audit role bits contain an unknown flag")]
    InvalidRoleBits,
    #[error("moderation-audit status bits contain an unknown flag")]
    InvalidStatusBits,
    #[error("moderation-audit page exceeds {MODERATION_AUDIT_PAGE_MAX_ENTRIES} records")]
    TooManyPageEntries,
    #[error(
        "moderation-audit page exceeds {MODERATION_AUDIT_PAGE_MAX_RETAINED_BYTES} retained bytes"
    )]
    PageBytesExceeded,
    #[error("moderation-audit page records must be strictly newest-first by unique audit id")]
    NonCanonicalPageOrder,
    #[error("moderation-audit page contains a record for another room")]
    PageRoomMismatch,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(audit_id: EventId, action: ModerationAuditAction) -> ModerationAuditRecord {
        let (result_role_bits, result_status_bits) = match action {
            ModerationAuditAction::Kick => (None, None),
            ModerationAuditAction::RoleChange => (Some(0b011), None),
            ModerationAuditAction::Ban => (None, Some(0b011)),
            ModerationAuditAction::Unban => (None, Some(0b010)),
            ModerationAuditAction::Mute => (None, Some(0b010)),
            ModerationAuditAction::Unmute => (None, Some(0b001)),
        };
        ModerationAuditRecord {
            audit_id,
            room_id: 7,
            actor_user_id: 2,
            actor_display_name_at_action: "Moderator".into(),
            target_user_id: Some(9),
            target_display_name_at_action: Some("Target".into()),
            action,
            committed_at_unix: 1_700_000_000,
            result_role_bits,
            result_status_bits,
        }
    }

    #[test]
    fn request_round_trips_newest_and_exclusive_cursor_shapes() {
        for request in [
            ModerationAuditRequest {
                before_audit_id: None,
                limit: 50,
            },
            ModerationAuditRequest {
                before_audit_id: Some(42),
                limit: MODERATION_AUDIT_PAGE_MAX_ENTRIES as u16,
            },
        ] {
            let body = request.into_frame_body().expect("request");
            assert_eq!(ModerationAuditRequest::from_frame_body(&body), Ok(request));
        }
    }

    #[test]
    fn request_rejects_unknown_tag_cursor_limit_and_trailing_fields() {
        for body in [
            FrameBody::Fields(vec![
                FrameValue::String("other-audit".into()),
                FrameValue::Nil,
                FrameValue::U64(50),
            ]),
            FrameBody::Fields(vec![
                FrameValue::String(MODERATION_AUDIT_REQUEST_BODY_TAG.into()),
                FrameValue::U64(0),
                FrameValue::U64(50),
            ]),
            FrameBody::Fields(vec![
                FrameValue::String(MODERATION_AUDIT_REQUEST_BODY_TAG.into()),
                FrameValue::Nil,
                FrameValue::U64(0),
            ]),
            FrameBody::Fields(vec![
                FrameValue::String(MODERATION_AUDIT_REQUEST_BODY_TAG.into()),
                FrameValue::Nil,
                FrameValue::U64(MODERATION_AUDIT_PAGE_MAX_ENTRIES as u64 + 1),
            ]),
            FrameBody::Fields(vec![
                FrameValue::String(MODERATION_AUDIT_REQUEST_BODY_TAG.into()),
                FrameValue::Nil,
                FrameValue::U64(50),
                FrameValue::Nil,
            ]),
        ] {
            assert!(ModerationAuditRequest::from_frame_body(&body).is_err());
        }
    }

    #[test]
    fn every_action_round_trips_only_with_its_exact_result_shape() {
        for action in [
            ModerationAuditAction::Kick,
            ModerationAuditAction::Ban,
            ModerationAuditAction::Unban,
            ModerationAuditAction::Mute,
            ModerationAuditAction::Unmute,
            ModerationAuditAction::RoleChange,
        ] {
            let expected = record(action as u8 as u64, action);
            let value = expected.clone().into_frame_value().expect("record");
            assert_eq!(
                ModerationAuditRecord::from_frame_value(&value),
                Ok(expected)
            );
        }
    }

    #[test]
    fn record_rejects_missing_target_unknown_bits_and_action_result_mismatch() {
        assert!(
            ModerationAuditRecord {
                result_role_bits: Some(0),
                ..record(1, ModerationAuditAction::RoleChange)
            }
            .into_frame_value()
            .is_ok(),
            "the existing standard role must remain representable"
        );
        assert_eq!(
            ModerationAuditRecord {
                target_user_id: None,
                target_display_name_at_action: None,
                ..record(1, ModerationAuditAction::Kick)
            }
            .into_frame_value(),
            Err(ModerationAuditError::MissingTarget)
        );
        assert_eq!(
            ModerationAuditRecord {
                result_role_bits: Some(MODERATION_AUDIT_ROLE_BITS_MASK + 1),
                ..record(1, ModerationAuditAction::RoleChange)
            }
            .into_frame_value(),
            Err(ModerationAuditError::InvalidRoleBits)
        );
        assert_eq!(
            ModerationAuditRecord {
                result_status_bits: None,
                ..record(1, ModerationAuditAction::Mute)
            }
            .into_frame_value(),
            Err(ModerationAuditError::ActionResultMismatch)
        );
        assert_eq!(
            ModerationAuditRecord {
                result_status_bits: Some(0),
                ..record(1, ModerationAuditAction::Ban)
            }
            .into_frame_value(),
            Err(ModerationAuditError::ActionResultMismatch)
        );
        assert_eq!(
            ModerationAuditRecord {
                result_role_bits: Some(0b010),
                ..record(1, ModerationAuditAction::RoleChange)
            }
            .into_frame_value(),
            Err(ModerationAuditError::ActionResultMismatch)
        );
    }

    #[test]
    fn page_is_newest_first_item_byte_and_room_bounded() {
        let page = ModerationAuditPage {
            records: vec![
                record(3, ModerationAuditAction::Kick),
                record(2, ModerationAuditAction::Mute),
                record(1, ModerationAuditAction::RoleChange),
            ],
        };
        let values = page.clone().into_frame_values().expect("page");
        let decoded = ModerationAuditPage::from_frame_values(&values).expect("decoded");
        assert_eq!(decoded, page);
        assert_eq!(decoded.validate_room(7), Ok(()));
        assert_eq!(
            decoded.validate_room(8),
            Err(ModerationAuditError::PageRoomMismatch)
        );

        let reversed = ModerationAuditPage {
            records: vec![
                record(1, ModerationAuditAction::Kick),
                record(2, ModerationAuditAction::Kick),
            ],
        };
        assert_eq!(
            reversed.into_frame_values(),
            Err(ModerationAuditError::NonCanonicalPageOrder)
        );

        let maximum_retained = MODERATION_AUDIT_PAGE_MAX_ENTRIES
            * (MODERATION_AUDIT_RETAINED_ROW_OVERHEAD_BYTES
                + 2 * MODERATION_AUDIT_DISPLAY_NAME_MAX_BYTES);
        assert!(maximum_retained <= MODERATION_AUDIT_PAGE_MAX_RETAINED_BYTES);
    }
}
