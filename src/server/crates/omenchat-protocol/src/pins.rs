use crate::{EventId, FrameBody, FrameValue, UserId};

pub const ROOM_PINS_CAPABILITY: &str = "room-pins-v1";
pub const ROOM_PIN_BODY_TAG: &str = "room-pin-v1";
pub const ROOM_PIN_SNAPSHOT_BODY_TAG: &str = "room-pin-snapshot-v1";
pub const ROOM_PIN_SNAPSHOT_MAX_TARGETS: usize = 256;
pub const ROOM_PIN_SNAPSHOT_MAX_ENTRIES: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum PinAction {
    Pin = 1,
    Unpin = 2,
}

impl TryFrom<u64> for PinAction {
    type Error = PinError;

    fn try_from(value: u64) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::Pin),
            2 => Ok(Self::Unpin),
            _ => Err(PinError::UnknownAction),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PinRequest {
    pub target_event_id: EventId,
    pub action: PinAction,
}

impl PinRequest {
    pub fn into_frame_body(self) -> Result<FrameBody, PinError> {
        validate_event_id(self.target_event_id)?;
        Ok(FrameBody::Fields(vec![
            FrameValue::String(ROOM_PIN_BODY_TAG.into()),
            FrameValue::U64(self.target_event_id),
            FrameValue::U64(self.action as u8 as u64),
        ]))
    }

    pub fn from_frame_body(body: &FrameBody) -> Result<Self, PinError> {
        let FrameBody::Fields(fields) = body else {
            return Err(PinError::InvalidRequestShape);
        };
        let [FrameValue::String(tag), FrameValue::U64(target_event_id), FrameValue::U64(action)] =
            fields.as_slice()
        else {
            return Err(PinError::InvalidRequestShape);
        };
        if tag != ROOM_PIN_BODY_TAG {
            return Err(PinError::InvalidRequestTag);
        }
        validate_event_id(*target_event_id)?;
        Ok(Self {
            target_event_id: *target_event_id,
            action: PinAction::try_from(*action)?,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PinAck {
    pub target_event_id: EventId,
    pub action: PinAction,
    pub actor_user_id: UserId,
    pub changed: bool,
    pub pin_event_id: Option<EventId>,
}

impl PinAck {
    pub fn into_frame_body(self) -> Result<FrameBody, PinError> {
        self.validate()?;
        Ok(FrameBody::Fields(vec![
            FrameValue::U64(self.target_event_id),
            FrameValue::U64(self.action as u8 as u64),
            FrameValue::U64(u64::from(self.actor_user_id)),
            FrameValue::Bool(self.changed),
            self.pin_event_id
                .map(FrameValue::U64)
                .unwrap_or(FrameValue::Nil),
        ]))
    }

    pub fn from_frame_body(body: &FrameBody) -> Result<Self, PinError> {
        let FrameBody::Fields(fields) = body else {
            return Err(PinError::InvalidAckShape);
        };
        let [FrameValue::U64(target_event_id), FrameValue::U64(action), FrameValue::U64(actor_user_id), FrameValue::Bool(changed), pin_event_id] =
            fields.as_slice()
        else {
            return Err(PinError::InvalidAckShape);
        };
        let ack = Self {
            target_event_id: *target_event_id,
            action: PinAction::try_from(*action)?,
            actor_user_id: parse_user_id(*actor_user_id)?,
            changed: *changed,
            pin_event_id: parse_optional_event_id(pin_event_id)?,
        };
        ack.validate()?;
        Ok(ack)
    }

    fn validate(&self) -> Result<(), PinError> {
        validate_event_id(self.target_event_id)?;
        validate_user_id(self.actor_user_id)?;
        if self.changed != self.pin_event_id.is_some() {
            return Err(PinError::InvalidAckResult);
        }
        if let Some(event_id) = self.pin_event_id {
            validate_event_id(event_id)?;
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PinEvent {
    pub pin_event_id: EventId,
    pub target_event_id: EventId,
    pub action: PinAction,
    pub actor_user_id: UserId,
    pub at_unix: i64,
}

impl PinEvent {
    pub fn into_frame_body(self) -> Result<FrameBody, PinError> {
        self.validate()?;
        Ok(FrameBody::Fields(vec![
            FrameValue::U64(self.pin_event_id),
            FrameValue::U64(self.target_event_id),
            FrameValue::U64(self.action as u8 as u64),
            FrameValue::U64(u64::from(self.actor_user_id)),
            timestamp_value(self.at_unix)?,
        ]))
    }

    pub fn from_frame_body(body: &FrameBody) -> Result<Self, PinError> {
        let FrameBody::Fields(fields) = body else {
            return Err(PinError::InvalidEventShape);
        };
        let [FrameValue::U64(pin_event_id), FrameValue::U64(target_event_id), FrameValue::U64(action), FrameValue::U64(actor_user_id), at_unix] =
            fields.as_slice()
        else {
            return Err(PinError::InvalidEventShape);
        };
        let event = Self {
            pin_event_id: *pin_event_id,
            target_event_id: *target_event_id,
            action: PinAction::try_from(*action)?,
            actor_user_id: parse_user_id(*actor_user_id)?,
            at_unix: parse_timestamp(at_unix)?,
        };
        event.validate()?;
        Ok(event)
    }

    fn validate(&self) -> Result<(), PinError> {
        validate_event_id(self.pin_event_id)?;
        validate_event_id(self.target_event_id)?;
        validate_user_id(self.actor_user_id)?;
        validate_timestamp(self.at_unix)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PinSnapshotEntry {
    pub target_event_id: EventId,
    pub pin_event_id: EventId,
    pub actor_user_id: UserId,
    pub pinned_at_unix: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PinSnapshot {
    pub target_event_ids: Vec<EventId>,
    pub entries: Vec<PinSnapshotEntry>,
}

impl PinSnapshot {
    pub fn into_frame_body(self) -> Result<FrameBody, PinError> {
        self.validate()?;
        Ok(FrameBody::Fields(vec![
            FrameValue::String(ROOM_PIN_SNAPSHOT_BODY_TAG.into()),
            FrameValue::Array(
                self.target_event_ids
                    .into_iter()
                    .map(FrameValue::U64)
                    .collect(),
            ),
            FrameValue::Array(
                self.entries
                    .into_iter()
                    .map(|entry| {
                        Ok(FrameValue::Array(vec![
                            FrameValue::U64(entry.target_event_id),
                            FrameValue::U64(entry.pin_event_id),
                            FrameValue::U64(u64::from(entry.actor_user_id)),
                            timestamp_value(entry.pinned_at_unix)?,
                        ]))
                    })
                    .collect::<Result<Vec<_>, PinError>>()?,
            ),
        ]))
    }

    pub fn from_frame_body(body: &FrameBody) -> Result<Self, PinError> {
        let FrameBody::Fields(fields) = body else {
            return Err(PinError::InvalidSnapshotShape);
        };
        let [FrameValue::String(tag), FrameValue::Array(target_values), FrameValue::Array(entry_values)] =
            fields.as_slice()
        else {
            return Err(PinError::InvalidSnapshotShape);
        };
        if tag != ROOM_PIN_SNAPSHOT_BODY_TAG {
            return Err(PinError::InvalidSnapshotTag);
        }
        if target_values.len() > ROOM_PIN_SNAPSHOT_MAX_TARGETS {
            return Err(PinError::TooManySnapshotTargets);
        }
        if entry_values.len() > ROOM_PIN_SNAPSHOT_MAX_ENTRIES {
            return Err(PinError::TooManySnapshotEntries);
        }

        let target_event_ids = target_values
            .iter()
            .map(|value| match value {
                FrameValue::U64(event_id) => {
                    validate_event_id(*event_id)?;
                    Ok(*event_id)
                }
                _ => Err(PinError::InvalidSnapshotShape),
            })
            .collect::<Result<Vec<_>, _>>()?;
        let entries = entry_values
            .iter()
            .map(parse_snapshot_entry)
            .collect::<Result<Vec<_>, _>>()?;
        let snapshot = Self {
            target_event_ids,
            entries,
        };
        snapshot.validate()?;
        Ok(snapshot)
    }

    fn validate(&self) -> Result<(), PinError> {
        if self.target_event_ids.len() > ROOM_PIN_SNAPSHOT_MAX_TARGETS {
            return Err(PinError::TooManySnapshotTargets);
        }
        if self.entries.len() > ROOM_PIN_SNAPSHOT_MAX_ENTRIES {
            return Err(PinError::TooManySnapshotEntries);
        }
        if self
            .target_event_ids
            .iter()
            .any(|event_id| validate_event_id(*event_id).is_err())
        {
            return Err(PinError::InvalidEventId);
        }
        if self
            .target_event_ids
            .windows(2)
            .any(|pair| pair[0] >= pair[1])
        {
            return Err(PinError::NonCanonicalSnapshotTargets);
        }
        for entry in &self.entries {
            validate_event_id(entry.target_event_id)?;
            validate_event_id(entry.pin_event_id)?;
            validate_user_id(entry.actor_user_id)?;
            validate_timestamp(entry.pinned_at_unix)?;
            if self
                .target_event_ids
                .binary_search(&entry.target_event_id)
                .is_err()
            {
                return Err(PinError::SnapshotEntryOutsideTargets);
            }
        }
        if self
            .entries
            .windows(2)
            .any(|pair| pair[0].target_event_id >= pair[1].target_event_id)
        {
            return Err(PinError::NonCanonicalSnapshotEntries);
        }
        Ok(())
    }
}

fn parse_snapshot_entry(value: &FrameValue) -> Result<PinSnapshotEntry, PinError> {
    let FrameValue::Array(fields) = value else {
        return Err(PinError::InvalidSnapshotShape);
    };
    let [FrameValue::U64(target_event_id), FrameValue::U64(pin_event_id), FrameValue::U64(actor_user_id), pinned_at_unix] =
        fields.as_slice()
    else {
        return Err(PinError::InvalidSnapshotShape);
    };
    Ok(PinSnapshotEntry {
        target_event_id: *target_event_id,
        pin_event_id: *pin_event_id,
        actor_user_id: parse_user_id(*actor_user_id)?,
        pinned_at_unix: parse_timestamp(pinned_at_unix)?,
    })
}

fn validate_event_id(event_id: EventId) -> Result<(), PinError> {
    if event_id == 0 {
        Err(PinError::InvalidEventId)
    } else {
        Ok(())
    }
}

fn validate_user_id(user_id: UserId) -> Result<(), PinError> {
    if user_id == 0 {
        Err(PinError::InvalidUserId)
    } else {
        Ok(())
    }
}

fn parse_user_id(user_id: u64) -> Result<UserId, PinError> {
    let user_id = UserId::try_from(user_id).map_err(|_| PinError::InvalidUserId)?;
    validate_user_id(user_id)?;
    Ok(user_id)
}

fn parse_optional_event_id(value: &FrameValue) -> Result<Option<EventId>, PinError> {
    match value {
        FrameValue::Nil => Ok(None),
        FrameValue::U64(event_id) => {
            validate_event_id(*event_id)?;
            Ok(Some(*event_id))
        }
        _ => Err(PinError::InvalidAckShape),
    }
}

fn timestamp_value(timestamp: i64) -> Result<FrameValue, PinError> {
    validate_timestamp(timestamp)?;
    Ok(FrameValue::U64(timestamp as u64))
}

fn parse_timestamp(value: &FrameValue) -> Result<i64, PinError> {
    match value {
        FrameValue::U64(value) => i64::try_from(*value).map_err(|_| PinError::InvalidTimestamp),
        FrameValue::I64(value) => {
            validate_timestamp(*value)?;
            Ok(*value)
        }
        _ => Err(PinError::InvalidTimestamp),
    }
}

fn validate_timestamp(timestamp: i64) -> Result<(), PinError> {
    if timestamp < 0 {
        Err(PinError::InvalidTimestamp)
    } else {
        Ok(())
    }
}

#[derive(Clone, Debug, thiserror::Error, PartialEq, Eq)]
pub enum PinError {
    #[error("pin request must use the exact three-field shape")]
    InvalidRequestShape,
    #[error("pin request has an unknown extension tag")]
    InvalidRequestTag,
    #[error("pin event id must be nonzero")]
    InvalidEventId,
    #[error("pin actor user id must be nonzero and fit u32")]
    InvalidUserId,
    #[error("pin action must be pin (1) or unpin (2)")]
    UnknownAction,
    #[error("pin acknowledgement must use the exact five-field shape")]
    InvalidAckShape,
    #[error("pin acknowledgement changed state and event id disagree")]
    InvalidAckResult,
    #[error("pin event must use the exact five-field shape")]
    InvalidEventShape,
    #[error("pin timestamp must be a nonnegative i64")]
    InvalidTimestamp,
    #[error("pin snapshot must use the exact tagged target-and-entry shape")]
    InvalidSnapshotShape,
    #[error("pin snapshot has an unknown extension tag")]
    InvalidSnapshotTag,
    #[error("pin snapshot exceeds {ROOM_PIN_SNAPSHOT_MAX_TARGETS} targets")]
    TooManySnapshotTargets,
    #[error("pin snapshot exceeds {ROOM_PIN_SNAPSHOT_MAX_ENTRIES} entries")]
    TooManySnapshotEntries,
    #[error("pin snapshot targets must be strictly increasing and unique")]
    NonCanonicalSnapshotTargets,
    #[error("pin snapshot entries must be strictly ordered by unique target")]
    NonCanonicalSnapshotEntries,
    #[error("pin snapshot entry is outside its explicit target set")]
    SnapshotEntryOutsideTargets,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{canonical_mutation_request_hash, ChatOp};

    fn request() -> PinRequest {
        PinRequest {
            target_event_id: 42,
            action: PinAction::Pin,
        }
    }

    fn snapshot_entry(target_event_id: EventId) -> PinSnapshotEntry {
        PinSnapshotEntry {
            target_event_id,
            pin_event_id: target_event_id + 100,
            actor_user_id: 7,
            pinned_at_unix: 1_700_000_000,
        }
    }

    #[test]
    fn request_round_trips_with_exact_shape() {
        let body = request().into_frame_body().expect("request");
        assert_eq!(
            body,
            FrameBody::Fields(vec![
                FrameValue::String(ROOM_PIN_BODY_TAG.into()),
                FrameValue::U64(42),
                FrameValue::U64(1),
            ])
        );
        assert_eq!(PinRequest::from_frame_body(&body), Ok(request()));
    }

    #[test]
    fn request_rejects_zero_unknown_tag_action_and_trailing_values() {
        let zero = FrameBody::Fields(vec![
            FrameValue::String(ROOM_PIN_BODY_TAG.into()),
            FrameValue::U64(0),
            FrameValue::U64(1),
        ]);
        assert_eq!(
            PinRequest::from_frame_body(&zero),
            Err(PinError::InvalidEventId)
        );
        let unknown_tag = FrameBody::Fields(vec![
            FrameValue::String("other-pin".into()),
            FrameValue::U64(42),
            FrameValue::U64(1),
        ]);
        assert_eq!(
            PinRequest::from_frame_body(&unknown_tag),
            Err(PinError::InvalidRequestTag)
        );
        let unknown_action = FrameBody::Fields(vec![
            FrameValue::String(ROOM_PIN_BODY_TAG.into()),
            FrameValue::U64(42),
            FrameValue::U64(3),
        ]);
        assert_eq!(
            PinRequest::from_frame_body(&unknown_action),
            Err(PinError::UnknownAction)
        );
        let mut trailing = match request().into_frame_body().expect("request") {
            FrameBody::Fields(fields) => fields,
            _ => unreachable!(),
        };
        trailing.push(FrameValue::Nil);
        assert_eq!(
            PinRequest::from_frame_body(&FrameBody::Fields(trailing)),
            Err(PinError::InvalidRequestShape)
        );
    }

    #[test]
    fn acknowledgement_round_trips_and_requires_changed_event_id_agreement() {
        let changed = PinAck {
            target_event_id: 42,
            action: PinAction::Pin,
            actor_user_id: 7,
            changed: true,
            pin_event_id: Some(100),
        };
        let body = changed.into_frame_body().expect("changed ack");
        assert_eq!(PinAck::from_frame_body(&body), Ok(changed));
        assert_eq!(
            PinAck {
                changed: false,
                ..changed
            }
            .into_frame_body(),
            Err(PinError::InvalidAckResult)
        );
        assert_eq!(
            PinAck {
                changed: true,
                pin_event_id: None,
                ..changed
            }
            .into_frame_body(),
            Err(PinError::InvalidAckResult)
        );
    }

    #[test]
    fn event_round_trips_and_rejects_invalid_identifiers_and_timestamp() {
        let event = PinEvent {
            pin_event_id: 100,
            target_event_id: 42,
            action: PinAction::Pin,
            actor_user_id: 7,
            at_unix: 1_700_000_000,
        };
        let body = event.into_frame_body().expect("event");
        assert_eq!(PinEvent::from_frame_body(&body), Ok(event));
        assert_eq!(
            PinEvent {
                actor_user_id: 0,
                ..event
            }
            .into_frame_body(),
            Err(PinError::InvalidUserId)
        );
        assert_eq!(
            PinEvent {
                at_unix: -1,
                ..event
            }
            .into_frame_body(),
            Err(PinError::InvalidTimestamp)
        );
    }

    #[test]
    fn empty_snapshot_explicitly_names_the_targets_it_replaces() {
        let snapshot = PinSnapshot {
            target_event_ids: vec![41, 42],
            entries: Vec::new(),
        };
        let body = snapshot.clone().into_frame_body().expect("snapshot");
        assert_eq!(
            body,
            FrameBody::Fields(vec![
                FrameValue::String(ROOM_PIN_SNAPSHOT_BODY_TAG.into()),
                FrameValue::Array(vec![FrameValue::U64(41), FrameValue::U64(42)]),
                FrameValue::Array(Vec::new()),
            ])
        );
        assert_eq!(PinSnapshot::from_frame_body(&body), Ok(snapshot));
    }

    #[test]
    fn snapshot_is_bounded_canonical_unique_and_target_scoped() {
        let snapshot = PinSnapshot {
            target_event_ids: vec![41, 42],
            entries: vec![snapshot_entry(41), snapshot_entry(42)],
        };
        let body = snapshot.clone().into_frame_body().expect("snapshot");
        assert_eq!(PinSnapshot::from_frame_body(&body), Ok(snapshot.clone()));

        assert_eq!(
            PinSnapshot {
                target_event_ids: vec![42, 41],
                entries: Vec::new(),
            }
            .into_frame_body(),
            Err(PinError::NonCanonicalSnapshotTargets)
        );
        assert_eq!(
            PinSnapshot {
                target_event_ids: vec![41, 42],
                entries: vec![snapshot_entry(42), snapshot_entry(41)],
            }
            .into_frame_body(),
            Err(PinError::NonCanonicalSnapshotEntries)
        );
        assert_eq!(
            PinSnapshot {
                target_event_ids: vec![41],
                entries: vec![snapshot_entry(42)],
            }
            .into_frame_body(),
            Err(PinError::SnapshotEntryOutsideTargets)
        );
    }

    #[test]
    fn snapshot_rejects_target_and_entry_count_overload() {
        assert_eq!(
            PinSnapshot {
                target_event_ids: (1..=(ROOM_PIN_SNAPSHOT_MAX_TARGETS as u64 + 1)).collect(),
                entries: Vec::new(),
            }
            .into_frame_body(),
            Err(PinError::TooManySnapshotTargets)
        );
        assert_eq!(
            PinSnapshot {
                target_event_ids: (1..=(ROOM_PIN_SNAPSHOT_MAX_ENTRIES as u64 + 1)).collect(),
                entries: (1..=(ROOM_PIN_SNAPSHOT_MAX_ENTRIES as u64 + 1))
                    .map(snapshot_entry)
                    .collect(),
            }
            .into_frame_body(),
            Err(PinError::TooManySnapshotEntries)
        );
    }

    #[test]
    fn durable_hash_covers_op_room_target_and_action() {
        let body = request().into_frame_body().expect("request");
        let base =
            canonical_mutation_request_hash(ChatOp::RoomPin, Some(7), &body).expect("base hash");
        assert_eq!(
            base.as_bytes(),
            &[
                251, 202, 147, 20, 222, 172, 191, 94, 108, 68, 174, 174, 235, 104, 68, 27, 182,
                198, 200, 113, 195, 40, 251, 176, 211, 70, 124, 237, 46, 205, 105, 164,
            ]
        );
        let changed_target = PinRequest {
            target_event_id: 43,
            ..request()
        }
        .into_frame_body()
        .expect("target");
        let changed_action = PinRequest {
            action: PinAction::Unpin,
            ..request()
        }
        .into_frame_body()
        .expect("action");

        assert_ne!(
            base,
            canonical_mutation_request_hash(ChatOp::RoomReaction, Some(7), &body).expect("op hash")
        );
        assert_ne!(
            base,
            canonical_mutation_request_hash(ChatOp::RoomPin, Some(8), &body).expect("room hash")
        );
        assert_ne!(
            base,
            canonical_mutation_request_hash(ChatOp::RoomPin, Some(7), &changed_target)
                .expect("target hash")
        );
        assert_ne!(
            base,
            canonical_mutation_request_hash(ChatOp::RoomPin, Some(7), &changed_action)
                .expect("action hash")
        );
    }
}
