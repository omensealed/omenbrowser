use crate::{EventId, FrameBody, FrameValue, UserId};

pub const REACTIONS_CAPABILITY: &str = "reactions-v1";
pub const REACTION_BODY_TAG: &str = "reaction-v1";
pub const REACTION_SNAPSHOT_BODY_TAG: &str = "reaction-snapshot-v1";
pub const REACTION_TOKEN_MAX_BYTES: usize = 16;
pub const REACTION_SNAPSHOT_MAX_TARGETS: usize = 256;
pub const REACTION_SNAPSHOT_MAX_ENTRIES: usize = 1_024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ReactionToken {
    ThumbsUp,
    Heart,
    Laugh,
    Surprised,
    Sad,
    ThumbsDown,
    Celebrate,
    Question,
}

impl ReactionToken {
    pub const ALL: [Self; 8] = [
        Self::ThumbsUp,
        Self::Heart,
        Self::Laugh,
        Self::Surprised,
        Self::Sad,
        Self::ThumbsDown,
        Self::Celebrate,
        Self::Question,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ThumbsUp => "thumbs_up",
            Self::Heart => "heart",
            Self::Laugh => "laugh",
            Self::Surprised => "surprised",
            Self::Sad => "sad",
            Self::ThumbsDown => "thumbs_down",
            Self::Celebrate => "celebrate",
            Self::Question => "question",
        }
    }
}

impl TryFrom<&str> for ReactionToken {
    type Error = ReactionError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "thumbs_up" => Ok(Self::ThumbsUp),
            "heart" => Ok(Self::Heart),
            "laugh" => Ok(Self::Laugh),
            "surprised" => Ok(Self::Surprised),
            "sad" => Ok(Self::Sad),
            "thumbs_down" => Ok(Self::ThumbsDown),
            "celebrate" => Ok(Self::Celebrate),
            "question" => Ok(Self::Question),
            _ => Err(ReactionError::UnknownToken),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum ReactionAction {
    Add = 1,
    Remove = 2,
}

impl TryFrom<u64> for ReactionAction {
    type Error = ReactionError;

    fn try_from(value: u64) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::Add),
            2 => Ok(Self::Remove),
            _ => Err(ReactionError::UnknownAction),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReactionRequest {
    pub target_event_id: EventId,
    pub token: ReactionToken,
    pub action: ReactionAction,
}

impl ReactionRequest {
    pub fn into_frame_body(self) -> Result<FrameBody, ReactionError> {
        validate_event_id(self.target_event_id)?;
        Ok(FrameBody::Fields(vec![
            FrameValue::String(REACTION_BODY_TAG.into()),
            FrameValue::U64(self.target_event_id),
            token_value(self.token),
            FrameValue::U64(self.action as u8 as u64),
        ]))
    }

    pub fn from_frame_body(body: &FrameBody) -> Result<Self, ReactionError> {
        let FrameBody::Fields(fields) = body else {
            return Err(ReactionError::InvalidRequestShape);
        };
        let [FrameValue::String(tag), FrameValue::U64(target_event_id), FrameValue::String(token), FrameValue::U64(action)] =
            fields.as_slice()
        else {
            return Err(ReactionError::InvalidRequestShape);
        };
        if tag != REACTION_BODY_TAG {
            return Err(ReactionError::InvalidRequestTag);
        }
        validate_event_id(*target_event_id)?;
        Ok(Self {
            target_event_id: *target_event_id,
            token: parse_token(token)?,
            action: ReactionAction::try_from(*action)?,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReactionAck {
    pub target_event_id: EventId,
    pub actor_user_id: UserId,
    pub token: ReactionToken,
    pub action: ReactionAction,
    pub changed: bool,
    pub reaction_event_id: Option<EventId>,
}

impl ReactionAck {
    pub fn into_frame_body(self) -> Result<FrameBody, ReactionError> {
        self.validate()?;
        Ok(FrameBody::Fields(vec![
            FrameValue::U64(self.target_event_id),
            FrameValue::U64(u64::from(self.actor_user_id)),
            token_value(self.token),
            FrameValue::U64(self.action as u8 as u64),
            FrameValue::Bool(self.changed),
            self.reaction_event_id
                .map(FrameValue::U64)
                .unwrap_or(FrameValue::Nil),
        ]))
    }

    pub fn from_frame_body(body: &FrameBody) -> Result<Self, ReactionError> {
        let FrameBody::Fields(fields) = body else {
            return Err(ReactionError::InvalidAckShape);
        };
        let [FrameValue::U64(target_event_id), FrameValue::U64(actor_user_id), FrameValue::String(token), FrameValue::U64(action), FrameValue::Bool(changed), reaction_event_id] =
            fields.as_slice()
        else {
            return Err(ReactionError::InvalidAckShape);
        };
        let ack = Self {
            target_event_id: *target_event_id,
            actor_user_id: parse_user_id(*actor_user_id)?,
            token: parse_token(token)?,
            action: ReactionAction::try_from(*action)?,
            changed: *changed,
            reaction_event_id: parse_optional_event_id(reaction_event_id)?,
        };
        ack.validate()?;
        Ok(ack)
    }

    fn validate(&self) -> Result<(), ReactionError> {
        validate_event_id(self.target_event_id)?;
        validate_user_id(self.actor_user_id)?;
        if self.changed != self.reaction_event_id.is_some() {
            return Err(ReactionError::InvalidAckResult);
        }
        if let Some(event_id) = self.reaction_event_id {
            validate_event_id(event_id)?;
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReactionEvent {
    pub reaction_event_id: EventId,
    pub target_event_id: EventId,
    pub actor_user_id: UserId,
    pub token: ReactionToken,
    pub action: ReactionAction,
    pub at_unix: i64,
}

impl ReactionEvent {
    pub fn into_frame_body(self) -> Result<FrameBody, ReactionError> {
        self.validate()?;
        Ok(FrameBody::Fields(vec![
            FrameValue::U64(self.reaction_event_id),
            FrameValue::U64(self.target_event_id),
            FrameValue::U64(u64::from(self.actor_user_id)),
            token_value(self.token),
            FrameValue::U64(self.action as u8 as u64),
            timestamp_value(self.at_unix)?,
        ]))
    }

    pub fn from_frame_body(body: &FrameBody) -> Result<Self, ReactionError> {
        let FrameBody::Fields(fields) = body else {
            return Err(ReactionError::InvalidEventShape);
        };
        let [FrameValue::U64(reaction_event_id), FrameValue::U64(target_event_id), FrameValue::U64(actor_user_id), FrameValue::String(token), FrameValue::U64(action), at_unix] =
            fields.as_slice()
        else {
            return Err(ReactionError::InvalidEventShape);
        };
        let event = Self {
            reaction_event_id: *reaction_event_id,
            target_event_id: *target_event_id,
            actor_user_id: parse_user_id(*actor_user_id)?,
            token: parse_token(token)?,
            action: ReactionAction::try_from(*action)?,
            at_unix: parse_timestamp(at_unix)?,
        };
        event.validate()?;
        Ok(event)
    }

    fn validate(&self) -> Result<(), ReactionError> {
        validate_event_id(self.reaction_event_id)?;
        validate_event_id(self.target_event_id)?;
        validate_user_id(self.actor_user_id)?;
        validate_timestamp(self.at_unix)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReactionSnapshotEntry {
    pub target_event_id: EventId,
    pub actor_user_id: UserId,
    pub token: ReactionToken,
    pub created_at_unix: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReactionSnapshot {
    pub target_event_ids: Vec<EventId>,
    pub entries: Vec<ReactionSnapshotEntry>,
}

impl ReactionSnapshot {
    pub fn into_frame_body(self) -> Result<FrameBody, ReactionError> {
        self.validate()?;
        Ok(FrameBody::Fields(vec![
            FrameValue::String(REACTION_SNAPSHOT_BODY_TAG.into()),
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
                            FrameValue::U64(u64::from(entry.actor_user_id)),
                            token_value(entry.token),
                            timestamp_value(entry.created_at_unix)?,
                        ]))
                    })
                    .collect::<Result<Vec<_>, ReactionError>>()?,
            ),
        ]))
    }

    pub fn from_frame_body(body: &FrameBody) -> Result<Self, ReactionError> {
        let FrameBody::Fields(fields) = body else {
            return Err(ReactionError::InvalidSnapshotShape);
        };
        let [FrameValue::String(tag), FrameValue::Array(target_values), FrameValue::Array(entry_values)] =
            fields.as_slice()
        else {
            return Err(ReactionError::InvalidSnapshotShape);
        };
        if tag != REACTION_SNAPSHOT_BODY_TAG {
            return Err(ReactionError::InvalidSnapshotTag);
        }
        if target_values.len() > REACTION_SNAPSHOT_MAX_TARGETS {
            return Err(ReactionError::TooManySnapshotTargets);
        }
        if entry_values.len() > REACTION_SNAPSHOT_MAX_ENTRIES {
            return Err(ReactionError::TooManySnapshotEntries);
        }

        let target_event_ids = target_values
            .iter()
            .map(|value| match value {
                FrameValue::U64(event_id) => {
                    validate_event_id(*event_id)?;
                    Ok(*event_id)
                }
                _ => Err(ReactionError::InvalidSnapshotShape),
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

    fn validate(&self) -> Result<(), ReactionError> {
        if self.target_event_ids.len() > REACTION_SNAPSHOT_MAX_TARGETS {
            return Err(ReactionError::TooManySnapshotTargets);
        }
        if self.entries.len() > REACTION_SNAPSHOT_MAX_ENTRIES {
            return Err(ReactionError::TooManySnapshotEntries);
        }
        if self
            .target_event_ids
            .iter()
            .any(|event_id| validate_event_id(*event_id).is_err())
        {
            return Err(ReactionError::InvalidEventId);
        }
        if self
            .target_event_ids
            .windows(2)
            .any(|pair| pair[0] >= pair[1])
        {
            return Err(ReactionError::NonCanonicalSnapshotTargets);
        }
        for entry in &self.entries {
            validate_event_id(entry.target_event_id)?;
            validate_user_id(entry.actor_user_id)?;
            validate_timestamp(entry.created_at_unix)?;
            if self
                .target_event_ids
                .binary_search(&entry.target_event_id)
                .is_err()
            {
                return Err(ReactionError::SnapshotEntryOutsideTargets);
            }
        }
        if self
            .entries
            .windows(2)
            .any(|pair| snapshot_sort_key(&pair[0]) >= snapshot_sort_key(&pair[1]))
        {
            return Err(ReactionError::NonCanonicalSnapshotEntries);
        }
        Ok(())
    }
}

fn parse_snapshot_entry(value: &FrameValue) -> Result<ReactionSnapshotEntry, ReactionError> {
    let FrameValue::Array(fields) = value else {
        return Err(ReactionError::InvalidSnapshotShape);
    };
    let [FrameValue::U64(target_event_id), FrameValue::U64(actor_user_id), FrameValue::String(token), created_at_unix] =
        fields.as_slice()
    else {
        return Err(ReactionError::InvalidSnapshotShape);
    };
    Ok(ReactionSnapshotEntry {
        target_event_id: *target_event_id,
        actor_user_id: parse_user_id(*actor_user_id)?,
        token: parse_token(token)?,
        created_at_unix: parse_timestamp(created_at_unix)?,
    })
}

fn snapshot_sort_key(entry: &ReactionSnapshotEntry) -> (EventId, &'static str, UserId) {
    (
        entry.target_event_id,
        entry.token.as_str(),
        entry.actor_user_id,
    )
}

fn token_value(token: ReactionToken) -> FrameValue {
    FrameValue::String(token.as_str().into())
}

fn parse_token(token: &str) -> Result<ReactionToken, ReactionError> {
    if token.len() > REACTION_TOKEN_MAX_BYTES {
        return Err(ReactionError::TokenTooLong);
    }
    ReactionToken::try_from(token)
}

fn validate_event_id(event_id: EventId) -> Result<(), ReactionError> {
    if event_id == 0 {
        Err(ReactionError::InvalidEventId)
    } else {
        Ok(())
    }
}

fn validate_user_id(user_id: UserId) -> Result<(), ReactionError> {
    if user_id == 0 {
        Err(ReactionError::InvalidUserId)
    } else {
        Ok(())
    }
}

fn parse_user_id(user_id: u64) -> Result<UserId, ReactionError> {
    let user_id = UserId::try_from(user_id).map_err(|_| ReactionError::InvalidUserId)?;
    validate_user_id(user_id)?;
    Ok(user_id)
}

fn parse_optional_event_id(value: &FrameValue) -> Result<Option<EventId>, ReactionError> {
    match value {
        FrameValue::Nil => Ok(None),
        FrameValue::U64(event_id) => {
            validate_event_id(*event_id)?;
            Ok(Some(*event_id))
        }
        _ => Err(ReactionError::InvalidAckShape),
    }
}

fn timestamp_value(timestamp: i64) -> Result<FrameValue, ReactionError> {
    validate_timestamp(timestamp)?;
    Ok(FrameValue::U64(timestamp as u64))
}

fn parse_timestamp(value: &FrameValue) -> Result<i64, ReactionError> {
    match value {
        FrameValue::U64(value) => {
            i64::try_from(*value).map_err(|_| ReactionError::InvalidTimestamp)
        }
        FrameValue::I64(value) => {
            validate_timestamp(*value)?;
            Ok(*value)
        }
        _ => Err(ReactionError::InvalidTimestamp),
    }
}

fn validate_timestamp(timestamp: i64) -> Result<(), ReactionError> {
    if timestamp < 0 {
        Err(ReactionError::InvalidTimestamp)
    } else {
        Ok(())
    }
}

#[derive(Clone, Debug, thiserror::Error, PartialEq, Eq)]
pub enum ReactionError {
    #[error("reaction request must use the exact four-field shape")]
    InvalidRequestShape,
    #[error("reaction request has an unknown extension tag")]
    InvalidRequestTag,
    #[error("reaction event id must be nonzero")]
    InvalidEventId,
    #[error("reaction actor user id must be nonzero and fit u32")]
    InvalidUserId,
    #[error("reaction token exceeds {REACTION_TOKEN_MAX_BYTES} bytes")]
    TokenTooLong,
    #[error("reaction token is not in the fixed v1 catalog")]
    UnknownToken,
    #[error("reaction action must be add (1) or remove (2)")]
    UnknownAction,
    #[error("reaction acknowledgement must use the exact six-field shape")]
    InvalidAckShape,
    #[error("reaction acknowledgement changed state and event id disagree")]
    InvalidAckResult,
    #[error("reaction event must use the exact six-field shape")]
    InvalidEventShape,
    #[error("reaction timestamp must be a nonnegative i64")]
    InvalidTimestamp,
    #[error("reaction snapshot must use the exact tagged target-and-entry shape")]
    InvalidSnapshotShape,
    #[error("reaction snapshot has an unknown extension tag")]
    InvalidSnapshotTag,
    #[error("reaction snapshot exceeds {REACTION_SNAPSHOT_MAX_TARGETS} targets")]
    TooManySnapshotTargets,
    #[error("reaction snapshot exceeds {REACTION_SNAPSHOT_MAX_ENTRIES} entries")]
    TooManySnapshotEntries,
    #[error("reaction snapshot targets must be strictly increasing and unique")]
    NonCanonicalSnapshotTargets,
    #[error("reaction snapshot entries must be strictly sorted and unique")]
    NonCanonicalSnapshotEntries,
    #[error("reaction snapshot entry is outside its explicit target set")]
    SnapshotEntryOutsideTargets,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{canonical_mutation_request_hash, ChatOp};

    fn request() -> ReactionRequest {
        ReactionRequest {
            target_event_id: 42,
            token: ReactionToken::Heart,
            action: ReactionAction::Add,
        }
    }

    #[test]
    fn token_catalog_is_fixed_ascii_and_bounded() {
        for token in ReactionToken::ALL {
            assert!(token.as_str().is_ascii());
            assert!(token.as_str().len() <= REACTION_TOKEN_MAX_BYTES);
            assert_eq!(ReactionToken::try_from(token.as_str()), Ok(token));
        }
        assert_eq!(
            ReactionToken::try_from("HEART"),
            Err(ReactionError::UnknownToken)
        );
        assert_eq!(
            ReactionToken::try_from("arbitrary"),
            Err(ReactionError::UnknownToken)
        );
    }

    #[test]
    fn request_round_trips_with_exact_type_vector() {
        let body = request().into_frame_body().expect("request body");
        assert_eq!(
            body,
            FrameBody::Fields(vec![
                FrameValue::String(REACTION_BODY_TAG.into()),
                FrameValue::U64(42),
                FrameValue::String("heart".into()),
                FrameValue::U64(1),
            ])
        );
        assert_eq!(ReactionRequest::from_frame_body(&body), Ok(request()));
    }

    #[test]
    fn request_rejects_zero_unknown_and_trailing_values() {
        let zero = FrameBody::Fields(vec![
            FrameValue::String(REACTION_BODY_TAG.into()),
            FrameValue::U64(0),
            FrameValue::String("heart".into()),
            FrameValue::U64(1),
        ]);
        assert_eq!(
            ReactionRequest::from_frame_body(&zero),
            Err(ReactionError::InvalidEventId)
        );

        let unknown = FrameBody::Fields(vec![
            FrameValue::String(REACTION_BODY_TAG.into()),
            FrameValue::U64(1),
            FrameValue::String("sparkles".into()),
            FrameValue::U64(1),
        ]);
        assert_eq!(
            ReactionRequest::from_frame_body(&unknown),
            Err(ReactionError::UnknownToken)
        );

        let mut trailing = match request().into_frame_body().expect("request") {
            FrameBody::Fields(fields) => fields,
            _ => unreachable!(),
        };
        trailing.push(FrameValue::Nil);
        assert_eq!(
            ReactionRequest::from_frame_body(&FrameBody::Fields(trailing)),
            Err(ReactionError::InvalidRequestShape)
        );
    }

    #[test]
    fn acknowledgement_requires_changed_event_id_agreement() {
        let changed = ReactionAck {
            target_event_id: 42,
            actor_user_id: 7,
            token: ReactionToken::Heart,
            action: ReactionAction::Add,
            changed: true,
            reaction_event_id: Some(100),
        };
        let body = changed.into_frame_body().expect("changed ack");
        assert_eq!(ReactionAck::from_frame_body(&body), Ok(changed));

        assert_eq!(
            ReactionAck {
                changed: false,
                ..changed
            }
            .into_frame_body(),
            Err(ReactionError::InvalidAckResult)
        );
        assert_eq!(
            ReactionAck {
                changed: true,
                reaction_event_id: None,
                ..changed
            }
            .into_frame_body(),
            Err(ReactionError::InvalidAckResult)
        );
    }

    #[test]
    fn event_round_trips_and_rejects_negative_timestamp() {
        let event = ReactionEvent {
            reaction_event_id: 100,
            target_event_id: 42,
            actor_user_id: 7,
            token: ReactionToken::Heart,
            action: ReactionAction::Add,
            at_unix: 1_700_000_000,
        };
        let body = event.into_frame_body().expect("event body");
        assert_eq!(ReactionEvent::from_frame_body(&body), Ok(event));
        assert_eq!(
            ReactionEvent {
                at_unix: -1,
                ..event
            }
            .into_frame_body(),
            Err(ReactionError::InvalidTimestamp)
        );
    }

    #[test]
    fn empty_snapshot_explicitly_names_the_targets_it_replaces() {
        let snapshot = ReactionSnapshot {
            target_event_ids: vec![41, 42],
            entries: Vec::new(),
        };
        let body = snapshot.clone().into_frame_body().expect("empty snapshot");
        assert_eq!(
            body,
            FrameBody::Fields(vec![
                FrameValue::String(REACTION_SNAPSHOT_BODY_TAG.into()),
                FrameValue::Array(vec![FrameValue::U64(41), FrameValue::U64(42)]),
                FrameValue::Array(Vec::new()),
            ])
        );
        assert_eq!(
            ReactionSnapshot::from_frame_body(&body),
            Ok(snapshot.clone())
        );
    }

    #[test]
    fn snapshot_is_bounded_canonical_and_target_scoped() {
        let snapshot = ReactionSnapshot {
            target_event_ids: vec![41, 42],
            entries: vec![
                ReactionSnapshotEntry {
                    target_event_id: 42,
                    actor_user_id: 7,
                    token: ReactionToken::Heart,
                    created_at_unix: 1_700_000_000,
                },
                ReactionSnapshotEntry {
                    target_event_id: 42,
                    actor_user_id: 9,
                    token: ReactionToken::Heart,
                    created_at_unix: 1_700_000_001,
                },
            ],
        };
        let body = snapshot.clone().into_frame_body().expect("snapshot");
        assert_eq!(
            ReactionSnapshot::from_frame_body(&body),
            Ok(snapshot.clone())
        );

        assert_eq!(
            ReactionSnapshot {
                target_event_ids: vec![42, 41],
                entries: Vec::new(),
            }
            .into_frame_body(),
            Err(ReactionError::NonCanonicalSnapshotTargets)
        );
        assert_eq!(
            ReactionSnapshot {
                target_event_ids: vec![41],
                entries: snapshot.entries,
            }
            .into_frame_body(),
            Err(ReactionError::SnapshotEntryOutsideTargets)
        );
    }

    #[test]
    fn snapshot_rejects_target_and_entry_count_overload_before_use() {
        assert_eq!(
            ReactionSnapshot {
                target_event_ids: (1..=(REACTION_SNAPSHOT_MAX_TARGETS as u64 + 1)).collect(),
                entries: Vec::new(),
            }
            .into_frame_body(),
            Err(ReactionError::TooManySnapshotTargets)
        );

        let entry = ReactionSnapshotEntry {
            target_event_id: 1,
            actor_user_id: 1,
            token: ReactionToken::Heart,
            created_at_unix: 1,
        };
        assert_eq!(
            ReactionSnapshot {
                target_event_ids: vec![1],
                entries: vec![entry; REACTION_SNAPSHOT_MAX_ENTRIES + 1],
            }
            .into_frame_body(),
            Err(ReactionError::TooManySnapshotEntries)
        );
    }

    #[test]
    fn durable_hash_covers_target_token_action_and_room() {
        let body = request().into_frame_body().expect("request");
        let base = canonical_mutation_request_hash(ChatOp::RoomReaction, Some(7), &body)
            .expect("base hash");
        assert_eq!(
            base.as_bytes(),
            &[
                193, 195, 219, 115, 90, 204, 2, 9, 138, 166, 55, 102, 235, 155, 183, 185, 171, 58,
                28, 99, 180, 210, 152, 8, 98, 70, 76, 4, 163, 210, 136, 180,
            ]
        );
        let changed_target = ReactionRequest {
            target_event_id: 43,
            ..request()
        }
        .into_frame_body()
        .expect("target");
        let changed_token = ReactionRequest {
            token: ReactionToken::Laugh,
            ..request()
        }
        .into_frame_body()
        .expect("token");
        let changed_action = ReactionRequest {
            action: ReactionAction::Remove,
            ..request()
        }
        .into_frame_body()
        .expect("action");

        assert_ne!(
            base,
            canonical_mutation_request_hash(ChatOp::RoomReaction, Some(7), &changed_target)
                .expect("target hash")
        );
        assert_ne!(
            base,
            canonical_mutation_request_hash(ChatOp::RoomReaction, Some(7), &changed_token)
                .expect("token hash")
        );
        assert_ne!(
            base,
            canonical_mutation_request_hash(ChatOp::RoomReaction, Some(7), &changed_action)
                .expect("action hash")
        );
        assert_ne!(
            base,
            canonical_mutation_request_hash(ChatOp::RoomReaction, Some(8), &body)
                .expect("room hash")
        );
    }
}
