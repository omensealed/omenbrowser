use crate::{EventId, FrameBody, FrameValue, UserId};

pub const MESSAGE_REVISIONS_CAPABILITY: &str = "message-revisions-v1";
pub const MESSAGE_REVISION_BODY_TAG: &str = "message-revision-v1";
pub const MESSAGE_REVISION_SNAPSHOT_BODY_TAG: &str = "message-revision-snapshot-v1";
pub const MESSAGE_REVISION_MAX_REPLACEMENT_BYTES: usize = 262_144;
pub const MESSAGE_REVISION_MAX_ACTOR_DISPLAY_BYTES: usize = 256;
pub const MESSAGE_REVISION_MAX_NUMBER: u64 = 9;
pub const MESSAGE_REVISION_SNAPSHOT_MAX_TARGETS: usize = 256;
pub const MESSAGE_REVISION_SNAPSHOT_MAX_ENTRIES: usize = 256;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum MessageRevisionAction {
    Correct = 1,
    Tombstone = 2,
}

impl TryFrom<u64> for MessageRevisionAction {
    type Error = MessageRevisionError;

    fn try_from(value: u64) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::Correct),
            2 => Ok(Self::Tombstone),
            _ => Err(MessageRevisionError::UnknownAction),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MessageRevisionRequest {
    pub target_event_id: EventId,
    pub action: MessageRevisionAction,
    pub replacement: Option<String>,
}

impl MessageRevisionRequest {
    pub fn into_frame_body(self) -> Result<FrameBody, MessageRevisionError> {
        self.validate()?;
        Ok(FrameBody::Fields(vec![
            FrameValue::String(MESSAGE_REVISION_BODY_TAG.into()),
            FrameValue::U64(self.target_event_id),
            FrameValue::U64(self.action as u8 as u64),
            optional_string_value(self.replacement),
        ]))
    }

    pub fn from_frame_body(body: &FrameBody) -> Result<Self, MessageRevisionError> {
        let FrameBody::Fields(fields) = body else {
            return Err(MessageRevisionError::InvalidRequestShape);
        };
        let [FrameValue::String(tag), FrameValue::U64(target_event_id), FrameValue::U64(action), replacement] =
            fields.as_slice()
        else {
            return Err(MessageRevisionError::InvalidRequestShape);
        };
        if tag != MESSAGE_REVISION_BODY_TAG {
            return Err(MessageRevisionError::InvalidRequestTag);
        }
        let request = Self {
            target_event_id: *target_event_id,
            action: MessageRevisionAction::try_from(*action)?,
            replacement: parse_optional_string(replacement, MessageRevisionField::Replacement)?,
        };
        request.validate()?;
        Ok(request)
    }

    fn validate(&self) -> Result<(), MessageRevisionError> {
        validate_event_id(self.target_event_id)?;
        validate_replacement(self.action, self.replacement.as_deref())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MessageRevisionAck {
    pub target_event_id: EventId,
    pub action: MessageRevisionAction,
    pub actor_user_id: UserId,
    pub changed: bool,
    pub revision_event_id: Option<EventId>,
    pub revision_number: u64,
}

impl MessageRevisionAck {
    pub fn into_frame_body(self) -> Result<FrameBody, MessageRevisionError> {
        self.validate()?;
        Ok(FrameBody::Fields(vec![
            FrameValue::U64(self.target_event_id),
            FrameValue::U64(self.action as u8 as u64),
            FrameValue::U64(u64::from(self.actor_user_id)),
            FrameValue::Bool(self.changed),
            self.revision_event_id
                .map(FrameValue::U64)
                .unwrap_or(FrameValue::Nil),
            FrameValue::U64(self.revision_number),
        ]))
    }

    pub fn from_frame_body(body: &FrameBody) -> Result<Self, MessageRevisionError> {
        let FrameBody::Fields(fields) = body else {
            return Err(MessageRevisionError::InvalidAckShape);
        };
        let [FrameValue::U64(target_event_id), FrameValue::U64(action), FrameValue::U64(actor_user_id), FrameValue::Bool(changed), revision_event_id, FrameValue::U64(revision_number)] =
            fields.as_slice()
        else {
            return Err(MessageRevisionError::InvalidAckShape);
        };
        let ack = Self {
            target_event_id: *target_event_id,
            action: MessageRevisionAction::try_from(*action)?,
            actor_user_id: parse_user_id(*actor_user_id)?,
            changed: *changed,
            revision_event_id: parse_optional_event_id(revision_event_id)?,
            revision_number: *revision_number,
        };
        ack.validate()?;
        Ok(ack)
    }

    fn validate(&self) -> Result<(), MessageRevisionError> {
        validate_event_id(self.target_event_id)?;
        validate_user_id(self.actor_user_id)?;
        validate_revision_number(self.revision_number)?;
        if self.changed != self.revision_event_id.is_some() {
            return Err(MessageRevisionError::InvalidAckResult);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MessageRevisionEvent {
    pub revision_event_id: EventId,
    pub target_event_id: EventId,
    pub action: MessageRevisionAction,
    pub actor_user_id: UserId,
    pub at_unix: i64,
    pub replacement: Option<String>,
    pub revision_number: u64,
    pub actor_display_name: Option<String>,
}

impl MessageRevisionEvent {
    pub fn into_frame_body(self) -> Result<FrameBody, MessageRevisionError> {
        self.validate()?;
        Ok(FrameBody::Fields(vec![
            FrameValue::U64(self.revision_event_id),
            FrameValue::U64(self.target_event_id),
            FrameValue::U64(self.action as u8 as u64),
            FrameValue::U64(u64::from(self.actor_user_id)),
            timestamp_value(self.at_unix)?,
            optional_string_value(self.replacement),
            FrameValue::U64(self.revision_number),
            optional_string_value(self.actor_display_name),
        ]))
    }

    pub fn from_frame_body(body: &FrameBody) -> Result<Self, MessageRevisionError> {
        let FrameBody::Fields(fields) = body else {
            return Err(MessageRevisionError::InvalidEventShape);
        };
        let [FrameValue::U64(revision_event_id), FrameValue::U64(target_event_id), FrameValue::U64(action), FrameValue::U64(actor_user_id), at_unix, replacement, FrameValue::U64(revision_number), actor_display_name] =
            fields.as_slice()
        else {
            return Err(MessageRevisionError::InvalidEventShape);
        };
        let event = Self {
            revision_event_id: *revision_event_id,
            target_event_id: *target_event_id,
            action: MessageRevisionAction::try_from(*action)?,
            actor_user_id: parse_user_id(*actor_user_id)?,
            at_unix: parse_timestamp(at_unix)?,
            replacement: parse_optional_string(replacement, MessageRevisionField::Replacement)?,
            revision_number: *revision_number,
            actor_display_name: parse_optional_string(
                actor_display_name,
                MessageRevisionField::ActorDisplay,
            )?,
        };
        event.validate()?;
        Ok(event)
    }

    fn validate(&self) -> Result<(), MessageRevisionError> {
        validate_event_id(self.revision_event_id)?;
        validate_event_id(self.target_event_id)?;
        validate_user_id(self.actor_user_id)?;
        validate_timestamp(self.at_unix)?;
        validate_revision_number(self.revision_number)?;
        validate_replacement(self.action, self.replacement.as_deref())?;
        validate_actor_display_name(self.actor_display_name.as_deref())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MessageRevisionSnapshotEntry {
    pub target_event_id: EventId,
    pub latest_revision_event_id: EventId,
    pub action: MessageRevisionAction,
    pub actor_user_id: UserId,
    pub at_unix: i64,
    pub replacement: Option<String>,
    pub revision_number: u64,
}

impl MessageRevisionSnapshotEntry {
    fn into_value(self) -> Result<FrameValue, MessageRevisionError> {
        self.validate()?;
        Ok(FrameValue::Array(vec![
            FrameValue::U64(self.target_event_id),
            FrameValue::U64(self.latest_revision_event_id),
            FrameValue::U64(self.action as u8 as u64),
            FrameValue::U64(u64::from(self.actor_user_id)),
            timestamp_value(self.at_unix)?,
            optional_string_value(self.replacement),
            FrameValue::U64(self.revision_number),
        ]))
    }

    fn from_value(value: &FrameValue) -> Result<Self, MessageRevisionError> {
        let FrameValue::Array(fields) = value else {
            return Err(MessageRevisionError::InvalidSnapshotShape);
        };
        let [FrameValue::U64(target_event_id), FrameValue::U64(latest_revision_event_id), FrameValue::U64(action), FrameValue::U64(actor_user_id), at_unix, replacement, FrameValue::U64(revision_number)] =
            fields.as_slice()
        else {
            return Err(MessageRevisionError::InvalidSnapshotShape);
        };
        let entry = Self {
            target_event_id: *target_event_id,
            latest_revision_event_id: *latest_revision_event_id,
            action: MessageRevisionAction::try_from(*action)?,
            actor_user_id: parse_user_id(*actor_user_id)?,
            at_unix: parse_timestamp(at_unix)?,
            replacement: parse_optional_string(replacement, MessageRevisionField::Replacement)?,
            revision_number: *revision_number,
        };
        entry.validate()?;
        Ok(entry)
    }

    fn validate(&self) -> Result<(), MessageRevisionError> {
        validate_event_id(self.target_event_id)?;
        validate_event_id(self.latest_revision_event_id)?;
        validate_user_id(self.actor_user_id)?;
        validate_timestamp(self.at_unix)?;
        validate_revision_number(self.revision_number)?;
        validate_replacement(self.action, self.replacement.as_deref())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MessageRevisionSnapshot {
    pub target_event_ids: Vec<EventId>,
    pub entries: Vec<MessageRevisionSnapshotEntry>,
}

impl MessageRevisionSnapshot {
    pub fn into_frame_body(self) -> Result<FrameBody, MessageRevisionError> {
        self.validate()?;
        Ok(FrameBody::Fields(vec![
            FrameValue::String(MESSAGE_REVISION_SNAPSHOT_BODY_TAG.into()),
            FrameValue::Array(
                self.target_event_ids
                    .into_iter()
                    .map(FrameValue::U64)
                    .collect(),
            ),
            FrameValue::Array(
                self.entries
                    .into_iter()
                    .map(MessageRevisionSnapshotEntry::into_value)
                    .collect::<Result<Vec<_>, _>>()?,
            ),
        ]))
    }

    pub fn from_frame_body(body: &FrameBody) -> Result<Self, MessageRevisionError> {
        let FrameBody::Fields(fields) = body else {
            return Err(MessageRevisionError::InvalidSnapshotShape);
        };
        let [FrameValue::String(tag), FrameValue::Array(targets), FrameValue::Array(entries)] =
            fields.as_slice()
        else {
            return Err(MessageRevisionError::InvalidSnapshotShape);
        };
        if tag != MESSAGE_REVISION_SNAPSHOT_BODY_TAG {
            return Err(MessageRevisionError::InvalidSnapshotTag);
        }
        if targets.len() > MESSAGE_REVISION_SNAPSHOT_MAX_TARGETS {
            return Err(MessageRevisionError::TooManySnapshotTargets);
        }
        if entries.len() > MESSAGE_REVISION_SNAPSHOT_MAX_ENTRIES {
            return Err(MessageRevisionError::TooManySnapshotEntries);
        }
        let snapshot = Self {
            target_event_ids: targets
                .iter()
                .map(|value| match value {
                    FrameValue::U64(event_id) => {
                        validate_event_id(*event_id)?;
                        Ok(*event_id)
                    }
                    _ => Err(MessageRevisionError::InvalidSnapshotShape),
                })
                .collect::<Result<Vec<_>, _>>()?,
            entries: entries
                .iter()
                .map(MessageRevisionSnapshotEntry::from_value)
                .collect::<Result<Vec<_>, _>>()?,
        };
        snapshot.validate()?;
        Ok(snapshot)
    }

    fn validate(&self) -> Result<(), MessageRevisionError> {
        if self.target_event_ids.len() > MESSAGE_REVISION_SNAPSHOT_MAX_TARGETS {
            return Err(MessageRevisionError::TooManySnapshotTargets);
        }
        if self.entries.len() > MESSAGE_REVISION_SNAPSHOT_MAX_ENTRIES {
            return Err(MessageRevisionError::TooManySnapshotEntries);
        }
        if self
            .target_event_ids
            .iter()
            .any(|event_id| validate_event_id(*event_id).is_err())
        {
            return Err(MessageRevisionError::InvalidEventId);
        }
        if self
            .target_event_ids
            .windows(2)
            .any(|pair| pair[0] >= pair[1])
        {
            return Err(MessageRevisionError::NonCanonicalSnapshotTargets);
        }
        for entry in &self.entries {
            entry.validate()?;
            if self
                .target_event_ids
                .binary_search(&entry.target_event_id)
                .is_err()
            {
                return Err(MessageRevisionError::SnapshotEntryOutsideTargets);
            }
        }
        if self
            .entries
            .windows(2)
            .any(|pair| pair[0].target_event_id >= pair[1].target_event_id)
        {
            return Err(MessageRevisionError::NonCanonicalSnapshotEntries);
        }
        Ok(())
    }
}

#[derive(Clone, Copy)]
enum MessageRevisionField {
    Replacement,
    ActorDisplay,
}

fn optional_string_value(value: Option<String>) -> FrameValue {
    value.map(FrameValue::String).unwrap_or(FrameValue::Nil)
}

fn parse_optional_string(
    value: &FrameValue,
    field: MessageRevisionField,
) -> Result<Option<String>, MessageRevisionError> {
    match value {
        FrameValue::Nil => Ok(None),
        FrameValue::String(value) => {
            if matches!(field, MessageRevisionField::Replacement)
                && value.len() > MESSAGE_REVISION_MAX_REPLACEMENT_BYTES
            {
                return Err(MessageRevisionError::ReplacementTooLong);
            }
            if matches!(field, MessageRevisionField::ActorDisplay)
                && value.len() > MESSAGE_REVISION_MAX_ACTOR_DISPLAY_BYTES
            {
                return Err(MessageRevisionError::ActorDisplayTooLong);
            }
            Ok(Some(value.clone()))
        }
        _ => Err(MessageRevisionError::InvalidOptionalString),
    }
}

fn validate_replacement(
    action: MessageRevisionAction,
    replacement: Option<&str>,
) -> Result<(), MessageRevisionError> {
    match (action, replacement) {
        (MessageRevisionAction::Correct, Some("")) => Err(MessageRevisionError::EmptyReplacement),
        (MessageRevisionAction::Correct, Some(value))
            if value.len() > MESSAGE_REVISION_MAX_REPLACEMENT_BYTES =>
        {
            Err(MessageRevisionError::ReplacementTooLong)
        }
        (MessageRevisionAction::Correct, Some(_)) | (MessageRevisionAction::Tombstone, None) => {
            Ok(())
        }
        _ => Err(MessageRevisionError::ActionReplacementMismatch),
    }
}

fn validate_actor_display_name(value: Option<&str>) -> Result<(), MessageRevisionError> {
    if value.is_some_and(|value| value.len() > MESSAGE_REVISION_MAX_ACTOR_DISPLAY_BYTES) {
        Err(MessageRevisionError::ActorDisplayTooLong)
    } else {
        Ok(())
    }
}

fn validate_event_id(event_id: EventId) -> Result<(), MessageRevisionError> {
    if event_id == 0 {
        Err(MessageRevisionError::InvalidEventId)
    } else {
        Ok(())
    }
}

fn validate_user_id(user_id: UserId) -> Result<(), MessageRevisionError> {
    if user_id == 0 {
        Err(MessageRevisionError::InvalidUserId)
    } else {
        Ok(())
    }
}

fn parse_user_id(user_id: u64) -> Result<UserId, MessageRevisionError> {
    let user_id = UserId::try_from(user_id).map_err(|_| MessageRevisionError::InvalidUserId)?;
    validate_user_id(user_id)?;
    Ok(user_id)
}

fn parse_optional_event_id(value: &FrameValue) -> Result<Option<EventId>, MessageRevisionError> {
    match value {
        FrameValue::Nil => Ok(None),
        FrameValue::U64(event_id) => {
            validate_event_id(*event_id)?;
            Ok(Some(*event_id))
        }
        _ => Err(MessageRevisionError::InvalidAckShape),
    }
}

fn validate_revision_number(revision_number: u64) -> Result<(), MessageRevisionError> {
    if (1..=MESSAGE_REVISION_MAX_NUMBER).contains(&revision_number) {
        Ok(())
    } else {
        Err(MessageRevisionError::InvalidRevisionNumber)
    }
}

fn timestamp_value(timestamp: i64) -> Result<FrameValue, MessageRevisionError> {
    validate_timestamp(timestamp)?;
    Ok(FrameValue::U64(timestamp as u64))
}

fn parse_timestamp(value: &FrameValue) -> Result<i64, MessageRevisionError> {
    match value {
        FrameValue::U64(value) => {
            i64::try_from(*value).map_err(|_| MessageRevisionError::InvalidTimestamp)
        }
        FrameValue::I64(value) => {
            validate_timestamp(*value)?;
            Ok(*value)
        }
        _ => Err(MessageRevisionError::InvalidTimestamp),
    }
}

fn validate_timestamp(timestamp: i64) -> Result<(), MessageRevisionError> {
    if timestamp < 0 {
        Err(MessageRevisionError::InvalidTimestamp)
    } else {
        Ok(())
    }
}

#[derive(Clone, Debug, thiserror::Error, PartialEq, Eq)]
pub enum MessageRevisionError {
    #[error("message revision request must use the exact four-field shape")]
    InvalidRequestShape,
    #[error("message revision request has an unknown extension tag")]
    InvalidRequestTag,
    #[error("message revision event id must be nonzero")]
    InvalidEventId,
    #[error("message revision actor user id must be nonzero and fit u32")]
    InvalidUserId,
    #[error("message revision action must be correct (1) or tombstone (2)")]
    UnknownAction,
    #[error("message correction replacement must not be empty")]
    EmptyReplacement,
    #[error(
        "message correction replacement exceeds {MESSAGE_REVISION_MAX_REPLACEMENT_BYTES} bytes"
    )]
    ReplacementTooLong,
    #[error("message revision action and replacement do not agree")]
    ActionReplacementMismatch,
    #[error("message revision optional field must be a string or nil")]
    InvalidOptionalString,
    #[error(
        "message revision actor display exceeds {MESSAGE_REVISION_MAX_ACTOR_DISPLAY_BYTES} bytes"
    )]
    ActorDisplayTooLong,
    #[error("message revision acknowledgement must use the exact six-field shape")]
    InvalidAckShape,
    #[error("message revision acknowledgement changed state and event id disagree")]
    InvalidAckResult,
    #[error("message revision event must use the exact eight-field shape")]
    InvalidEventShape,
    #[error("message revision timestamp must be a nonnegative i64")]
    InvalidTimestamp,
    #[error("message revision number must be between 1 and {MESSAGE_REVISION_MAX_NUMBER}")]
    InvalidRevisionNumber,
    #[error("message revision snapshot must use the exact tagged target-and-entry shape")]
    InvalidSnapshotShape,
    #[error("message revision snapshot has an unknown extension tag")]
    InvalidSnapshotTag,
    #[error("message revision snapshot exceeds {MESSAGE_REVISION_SNAPSHOT_MAX_TARGETS} targets")]
    TooManySnapshotTargets,
    #[error("message revision snapshot exceeds {MESSAGE_REVISION_SNAPSHOT_MAX_ENTRIES} entries")]
    TooManySnapshotEntries,
    #[error("message revision snapshot targets must be strictly increasing and unique")]
    NonCanonicalSnapshotTargets,
    #[error("message revision snapshot entries must be strictly sorted and unique")]
    NonCanonicalSnapshotEntries,
    #[error("message revision snapshot entry is outside its explicit target set")]
    SnapshotEntryOutsideTargets,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{canonical_mutation_request_hash, ChatOp};

    fn correction() -> MessageRevisionRequest {
        MessageRevisionRequest {
            target_event_id: 42,
            action: MessageRevisionAction::Correct,
            replacement: Some("edited".into()),
        }
    }

    fn snapshot_entry(target_event_id: EventId) -> MessageRevisionSnapshotEntry {
        MessageRevisionSnapshotEntry {
            target_event_id,
            latest_revision_event_id: target_event_id + 100,
            action: MessageRevisionAction::Correct,
            actor_user_id: 7,
            at_unix: 1_700_000_000,
            replacement: Some(format!("edited {target_event_id}")),
            revision_number: 1,
        }
    }

    #[test]
    fn request_round_trips_with_exact_shape() {
        let body = correction().into_frame_body().expect("request");
        assert_eq!(
            body,
            FrameBody::Fields(vec![
                FrameValue::String(MESSAGE_REVISION_BODY_TAG.into()),
                FrameValue::U64(42),
                FrameValue::U64(1),
                FrameValue::String("edited".into()),
            ])
        );
        assert_eq!(
            MessageRevisionRequest::from_frame_body(&body),
            Ok(correction())
        );
        let tombstone = MessageRevisionRequest {
            target_event_id: 42,
            action: MessageRevisionAction::Tombstone,
            replacement: None,
        };
        assert_eq!(
            MessageRevisionRequest::from_frame_body(
                &tombstone.clone().into_frame_body().expect("tombstone")
            ),
            Ok(tombstone)
        );
    }

    #[test]
    fn request_rejects_malformed_action_replacement_and_bounds() {
        for request in [
            MessageRevisionRequest {
                target_event_id: 0,
                ..correction()
            },
            MessageRevisionRequest {
                replacement: Some(String::new()),
                ..correction()
            },
            MessageRevisionRequest {
                replacement: None,
                ..correction()
            },
            MessageRevisionRequest {
                action: MessageRevisionAction::Tombstone,
                ..correction()
            },
        ] {
            assert!(request.into_frame_body().is_err());
        }
        assert_eq!(
            MessageRevisionRequest {
                replacement: Some("x".repeat(MESSAGE_REVISION_MAX_REPLACEMENT_BYTES + 1)),
                ..correction()
            }
            .into_frame_body(),
            Err(MessageRevisionError::ReplacementTooLong)
        );
        let mut trailing = match correction().into_frame_body().expect("request") {
            FrameBody::Fields(fields) => fields,
            _ => unreachable!(),
        };
        trailing.push(FrameValue::Nil);
        assert_eq!(
            MessageRevisionRequest::from_frame_body(&FrameBody::Fields(trailing)),
            Err(MessageRevisionError::InvalidRequestShape)
        );
        let unknown_action = FrameBody::Fields(vec![
            FrameValue::String(MESSAGE_REVISION_BODY_TAG.into()),
            FrameValue::U64(42),
            FrameValue::U64(3),
            FrameValue::Nil,
        ]);
        assert_eq!(
            MessageRevisionRequest::from_frame_body(&unknown_action),
            Err(MessageRevisionError::UnknownAction)
        );
    }

    #[test]
    fn acknowledgement_event_and_bounds_round_trip() {
        let ack = MessageRevisionAck {
            target_event_id: 42,
            action: MessageRevisionAction::Correct,
            actor_user_id: 7,
            changed: true,
            revision_event_id: Some(100),
            revision_number: 1,
        };
        assert_eq!(
            MessageRevisionAck::from_frame_body(&ack.into_frame_body().expect("acknowledgement")),
            Ok(ack)
        );
        assert_eq!(
            MessageRevisionAck {
                changed: false,
                ..ack
            }
            .into_frame_body(),
            Err(MessageRevisionError::InvalidAckResult)
        );

        let event = MessageRevisionEvent {
            revision_event_id: 100,
            target_event_id: 42,
            action: MessageRevisionAction::Correct,
            actor_user_id: 7,
            at_unix: 1_700_000_000,
            replacement: Some("edited".into()),
            revision_number: 1,
            actor_display_name: Some("Alice".into()),
        };
        assert_eq!(
            MessageRevisionEvent::from_frame_body(&event.clone().into_frame_body().expect("event")),
            Ok(event.clone())
        );
        assert_eq!(
            MessageRevisionEvent {
                revision_number: MESSAGE_REVISION_MAX_NUMBER + 1,
                ..event
            }
            .into_frame_body(),
            Err(MessageRevisionError::InvalidRevisionNumber)
        );
    }

    #[test]
    fn snapshot_is_explicit_bounded_and_canonical() {
        let snapshot = MessageRevisionSnapshot {
            target_event_ids: vec![10, 20],
            entries: vec![snapshot_entry(10), snapshot_entry(20)],
        };
        let body = snapshot.clone().into_frame_body().expect("snapshot");
        assert_eq!(
            MessageRevisionSnapshot::from_frame_body(&body),
            Ok(snapshot)
        );
        assert_eq!(
            MessageRevisionSnapshot {
                target_event_ids: vec![20, 10],
                entries: Vec::new(),
            }
            .into_frame_body(),
            Err(MessageRevisionError::NonCanonicalSnapshotTargets)
        );
        assert_eq!(
            MessageRevisionSnapshot {
                target_event_ids: vec![10],
                entries: vec![snapshot_entry(20)],
            }
            .into_frame_body(),
            Err(MessageRevisionError::SnapshotEntryOutsideTargets)
        );
        assert_eq!(
            MessageRevisionSnapshot {
                target_event_ids: (1..=MESSAGE_REVISION_SNAPSHOT_MAX_TARGETS as u64 + 1).collect(),
                entries: Vec::new(),
            }
            .into_frame_body(),
            Err(MessageRevisionError::TooManySnapshotTargets)
        );
        assert_eq!(
            MessageRevisionSnapshot::from_frame_body(&FrameBody::Fields(vec![
                FrameValue::String(MESSAGE_REVISION_SNAPSHOT_BODY_TAG.into()),
                FrameValue::Array(Vec::new()),
                FrameValue::Array(vec![
                    FrameValue::Array(Vec::new());
                    MESSAGE_REVISION_SNAPSHOT_MAX_ENTRIES + 1
                ]),
            ])),
            Err(MessageRevisionError::TooManySnapshotEntries)
        );
    }

    #[test]
    fn durable_hash_is_stable_and_scoped_to_revision_content() {
        let body = correction().into_frame_body().expect("request");
        let hash = canonical_mutation_request_hash(ChatOp::RoomMessageRevision, Some(7), &body)
            .expect("hash");
        let hex = hash
            .as_bytes()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        assert_eq!(
            hex,
            "ff7ae5d094570eb5af301ed0842bf1d2d025b283980540f3c27c3ee567dda558"
        );
        let different_room =
            canonical_mutation_request_hash(ChatOp::RoomMessageRevision, Some(8), &body)
                .expect("different room");
        let tombstone = MessageRevisionRequest {
            target_event_id: 42,
            action: MessageRevisionAction::Tombstone,
            replacement: None,
        }
        .into_frame_body()
        .expect("tombstone");
        let different_action =
            canonical_mutation_request_hash(ChatOp::RoomMessageRevision, Some(7), &tombstone)
                .expect("different action");
        assert_ne!(hash, different_room);
        assert_ne!(hash, different_action);
    }
}
