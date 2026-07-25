use crate::{EventId, FrameBody, FrameValue, RoomId, UserId};

pub const REPLY_MENTIONS_CAPABILITY: &str = "reply-mentions-v1";
pub const REPLY_MENTIONS_BODY_TAG: &str = "reply-mentions-v1";
pub const RICH_MESSAGE_BODY_MAX_BYTES: usize = 512 * 1024;
pub const RICH_MESSAGE_MAX_MENTIONS: usize = 16;

const LEGACY_ROOM_EVENT_FIELDS: usize = 6;
const RICH_ROOM_EVENT_FIELDS: usize = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReplyReference {
    pub room_id: RoomId,
    pub event_id: EventId,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RichMessageBody {
    pub body: String,
    pub reply_to: Option<ReplyReference>,
    pub mentioned_user_ids: Vec<UserId>,
}

impl RichMessageBody {
    pub fn into_frame_body(self) -> Result<FrameBody, RichMessageError> {
        validate_body(&self.body)?;
        validate_metadata_present(self.reply_to.is_some(), &self.mentioned_user_ids)?;
        validate_mentions(&self.mentioned_user_ids)?;

        let reply_to = self.reply_to.map_or(FrameValue::Nil, |reference| {
            FrameValue::Array(vec![
                FrameValue::U64(u64::from(reference.room_id)),
                FrameValue::U64(reference.event_id),
            ])
        });
        if matches!(
            self.reply_to,
            Some(ReplyReference { room_id: 0, .. }) | Some(ReplyReference { event_id: 0, .. })
        ) {
            return Err(RichMessageError::InvalidReplyReference);
        }

        Ok(FrameBody::Fields(vec![
            FrameValue::String(self.body),
            FrameValue::String(REPLY_MENTIONS_BODY_TAG.into()),
            reply_to,
            mentions_value(self.mentioned_user_ids),
        ]))
    }

    pub fn from_frame_body(body: &FrameBody) -> Result<Self, RichMessageError> {
        let FrameBody::Fields(fields) = body else {
            return Err(RichMessageError::InvalidRequestShape);
        };
        let [FrameValue::String(message), FrameValue::String(tag), reply, mentions] =
            fields.as_slice()
        else {
            return Err(RichMessageError::InvalidRequestShape);
        };
        if tag != REPLY_MENTIONS_BODY_TAG {
            return Err(RichMessageError::InvalidRequestTag);
        }

        validate_body(message)?;
        let reply_to = parse_reply_reference(reply)?;
        let mentioned_user_ids = parse_mentions(mentions)?;
        validate_metadata_present(reply_to.is_some(), &mentioned_user_ids)?;
        Ok(Self {
            body: message.clone(),
            reply_to,
            mentioned_user_ids,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RichMessageEventMetadata {
    pub reply_to_event_id: Option<EventId>,
    pub mentioned_user_ids: Vec<UserId>,
}

pub fn append_rich_message_event_metadata(
    fields: &mut Vec<FrameValue>,
    metadata: &RichMessageEventMetadata,
) -> Result<(), RichMessageError> {
    if fields.len() != LEGACY_ROOM_EVENT_FIELDS {
        return Err(RichMessageError::InvalidEventShape);
    }
    validate_event_metadata(metadata)?;
    fields.push(
        metadata
            .reply_to_event_id
            .map(FrameValue::U64)
            .unwrap_or(FrameValue::Nil),
    );
    fields.push(mentions_value(metadata.mentioned_user_ids.clone()));
    Ok(())
}

pub fn parse_rich_message_event_metadata(
    fields: &[FrameValue],
) -> Result<Option<RichMessageEventMetadata>, RichMessageError> {
    if fields.len() == LEGACY_ROOM_EVENT_FIELDS {
        return Ok(None);
    }
    if fields.len() != RICH_ROOM_EVENT_FIELDS {
        return Err(RichMessageError::InvalidEventShape);
    }

    let reply_to_event_id = match &fields[LEGACY_ROOM_EVENT_FIELDS] {
        FrameValue::Nil => None,
        FrameValue::U64(0) => return Err(RichMessageError::InvalidReplyReference),
        FrameValue::U64(event_id) => Some(*event_id),
        _ => return Err(RichMessageError::InvalidEventShape),
    };
    let mentioned_user_ids = parse_mentions(&fields[LEGACY_ROOM_EVENT_FIELDS + 1])?;
    let metadata = RichMessageEventMetadata {
        reply_to_event_id,
        mentioned_user_ids,
    };
    validate_event_metadata(&metadata)?;
    Ok(Some(metadata))
}

fn validate_event_metadata(metadata: &RichMessageEventMetadata) -> Result<(), RichMessageError> {
    if metadata.reply_to_event_id == Some(0) {
        return Err(RichMessageError::InvalidReplyReference);
    }
    validate_metadata_present(
        metadata.reply_to_event_id.is_some(),
        &metadata.mentioned_user_ids,
    )?;
    validate_mentions(&metadata.mentioned_user_ids)
}

fn parse_reply_reference(value: &FrameValue) -> Result<Option<ReplyReference>, RichMessageError> {
    match value {
        FrameValue::Nil => Ok(None),
        FrameValue::Array(values) => {
            let [FrameValue::U64(room_id), FrameValue::U64(event_id)] = values.as_slice() else {
                return Err(RichMessageError::InvalidReplyReference);
            };
            let room_id =
                RoomId::try_from(*room_id).map_err(|_| RichMessageError::InvalidReplyReference)?;
            if room_id == 0 || *event_id == 0 {
                return Err(RichMessageError::InvalidReplyReference);
            }
            Ok(Some(ReplyReference {
                room_id,
                event_id: *event_id,
            }))
        }
        _ => Err(RichMessageError::InvalidReplyReference),
    }
}

fn parse_mentions(value: &FrameValue) -> Result<Vec<UserId>, RichMessageError> {
    let FrameValue::Array(values) = value else {
        return Err(RichMessageError::InvalidMentionShape);
    };
    if values.len() > RICH_MESSAGE_MAX_MENTIONS {
        return Err(RichMessageError::TooManyMentions);
    }

    let mut mentions = Vec::with_capacity(values.len());
    for value in values {
        let FrameValue::U64(user_id) = value else {
            return Err(RichMessageError::InvalidMentionShape);
        };
        mentions.push(UserId::try_from(*user_id).map_err(|_| RichMessageError::InvalidMentionId)?);
    }
    validate_mentions(&mentions)?;
    Ok(mentions)
}

fn validate_mentions(mentions: &[UserId]) -> Result<(), RichMessageError> {
    if mentions.len() > RICH_MESSAGE_MAX_MENTIONS {
        return Err(RichMessageError::TooManyMentions);
    }
    if mentions.contains(&0) {
        return Err(RichMessageError::InvalidMentionId);
    }
    if mentions.windows(2).any(|pair| pair[0] >= pair[1]) {
        return Err(RichMessageError::NonCanonicalMentions);
    }
    Ok(())
}

fn validate_body(body: &str) -> Result<(), RichMessageError> {
    if body.trim().is_empty() {
        return Err(RichMessageError::EmptyBody);
    }
    if body.len() > RICH_MESSAGE_BODY_MAX_BYTES {
        return Err(RichMessageError::BodyTooLarge);
    }
    Ok(())
}

fn validate_metadata_present(has_reply: bool, mentions: &[UserId]) -> Result<(), RichMessageError> {
    if !has_reply && mentions.is_empty() {
        return Err(RichMessageError::EmptyMetadata);
    }
    Ok(())
}

fn mentions_value(mentions: Vec<UserId>) -> FrameValue {
    FrameValue::Array(
        mentions
            .into_iter()
            .map(|user_id| FrameValue::U64(u64::from(user_id)))
            .collect(),
    )
}

#[derive(Clone, Debug, thiserror::Error, PartialEq, Eq)]
pub enum RichMessageError {
    #[error("rich message request must use the exact bounded fields shape")]
    InvalidRequestShape,
    #[error("rich message request has an unknown extension tag")]
    InvalidRequestTag,
    #[error("rich message body must not be empty")]
    EmptyBody,
    #[error("rich message body exceeds {RICH_MESSAGE_BODY_MAX_BYTES} bytes")]
    BodyTooLarge,
    #[error("rich message reply reference is invalid")]
    InvalidReplyReference,
    #[error("rich message mention list must be an array of unsigned user ids")]
    InvalidMentionShape,
    #[error("rich message mention user id is invalid")]
    InvalidMentionId,
    #[error("rich message contains more than {RICH_MESSAGE_MAX_MENTIONS} mentions")]
    TooManyMentions,
    #[error("rich message mention ids must be strictly increasing and unique")]
    NonCanonicalMentions,
    #[error("rich message extension must contain a reply or at least one mention")]
    EmptyMetadata,
    #[error("rich message event must have exactly six legacy or eight rich fields")]
    InvalidEventShape,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{canonical_mutation_request_hash, ChatOp};

    fn rich_message() -> RichMessageBody {
        RichMessageBody {
            body: "hello".into(),
            reply_to: Some(ReplyReference {
                room_id: 7,
                event_id: 42,
            }),
            mentioned_user_ids: vec![2, 9],
        }
    }

    fn legacy_event() -> Vec<FrameValue> {
        vec![
            FrameValue::U64(42),
            FrameValue::U64(1),
            FrameValue::U64(2),
            FrameValue::U64(1_700_000_000),
            FrameValue::String("hello".into()),
            FrameValue::String("alice".into()),
        ]
    }

    #[test]
    fn rich_request_round_trips_with_stable_type_vector() {
        let expected = FrameBody::Fields(vec![
            FrameValue::String("hello".into()),
            FrameValue::String(REPLY_MENTIONS_BODY_TAG.into()),
            FrameValue::Array(vec![FrameValue::U64(7), FrameValue::U64(42)]),
            FrameValue::Array(vec![FrameValue::U64(2), FrameValue::U64(9)]),
        ]);
        let encoded = rich_message().into_frame_body().expect("encode rich body");
        assert_eq!(encoded, expected);
        assert_eq!(
            RichMessageBody::from_frame_body(&encoded),
            Ok(rich_message())
        );
    }

    #[test]
    fn legacy_event_has_no_metadata_and_rich_event_round_trips() {
        let mut fields = legacy_event();
        assert_eq!(parse_rich_message_event_metadata(&fields), Ok(None));

        let metadata = RichMessageEventMetadata {
            reply_to_event_id: Some(41),
            mentioned_user_ids: vec![2, 9],
        };
        append_rich_message_event_metadata(&mut fields, &metadata).expect("append rich metadata");
        assert_eq!(fields.len(), RICH_ROOM_EVENT_FIELDS);
        assert_eq!(
            parse_rich_message_event_metadata(&fields),
            Ok(Some(metadata))
        );
    }

    #[test]
    fn rich_request_rejects_empty_or_oversized_bodies() {
        let empty = RichMessageBody {
            body: " \n".into(),
            reply_to: None,
            mentioned_user_ids: vec![1],
        };
        assert_eq!(empty.into_frame_body(), Err(RichMessageError::EmptyBody));

        let oversized = RichMessageBody {
            body: "x".repeat(RICH_MESSAGE_BODY_MAX_BYTES + 1),
            reply_to: None,
            mentioned_user_ids: vec![1],
        };
        assert_eq!(
            oversized.into_frame_body(),
            Err(RichMessageError::BodyTooLarge)
        );

        let exact_limit = RichMessageBody {
            body: "x".repeat(RICH_MESSAGE_BODY_MAX_BYTES),
            reply_to: Some(ReplyReference {
                room_id: 1,
                event_id: 1,
            }),
            mentioned_user_ids: Vec::new(),
        };
        assert!(exact_limit.into_frame_body().is_ok());
    }

    #[test]
    fn rich_request_rejects_noncanonical_and_oversized_mentions() {
        for mentions in [vec![1, 1], vec![2, 1]] {
            let message = RichMessageBody {
                body: "hello".into(),
                reply_to: None,
                mentioned_user_ids: mentions,
            };
            assert_eq!(
                message.into_frame_body(),
                Err(RichMessageError::NonCanonicalMentions)
            );
        }

        let message = RichMessageBody {
            body: "hello".into(),
            reply_to: None,
            mentioned_user_ids: (1..=(RICH_MESSAGE_MAX_MENTIONS as u32 + 1)).collect(),
        };
        assert_eq!(
            message.into_frame_body(),
            Err(RichMessageError::TooManyMentions)
        );

        let exact_limit = RichMessageBody {
            body: "hello".into(),
            reply_to: None,
            mentioned_user_ids: (1..=RICH_MESSAGE_MAX_MENTIONS as u32).collect(),
        };
        assert!(exact_limit.into_frame_body().is_ok());
    }

    #[test]
    fn rich_request_rejects_invalid_references_empty_metadata_and_trailing_fields() {
        let invalid_reference = FrameBody::Fields(vec![
            FrameValue::String("hello".into()),
            FrameValue::String(REPLY_MENTIONS_BODY_TAG.into()),
            FrameValue::Array(vec![FrameValue::U64(0), FrameValue::U64(1)]),
            FrameValue::Array(Vec::new()),
        ]);
        assert_eq!(
            RichMessageBody::from_frame_body(&invalid_reference),
            Err(RichMessageError::InvalidReplyReference)
        );

        let no_metadata = RichMessageBody {
            body: "hello".into(),
            reply_to: None,
            mentioned_user_ids: Vec::new(),
        };
        assert_eq!(
            no_metadata.into_frame_body(),
            Err(RichMessageError::EmptyMetadata)
        );

        let mut trailing = match rich_message().into_frame_body().expect("rich body") {
            FrameBody::Fields(fields) => fields,
            _ => unreachable!(),
        };
        trailing.push(FrameValue::Nil);
        assert_eq!(
            RichMessageBody::from_frame_body(&FrameBody::Fields(trailing)),
            Err(RichMessageError::InvalidRequestShape)
        );
    }

    #[test]
    fn rich_event_rejects_malformed_shape_and_empty_metadata() {
        let mut seven_fields = legacy_event();
        seven_fields.push(FrameValue::Nil);
        assert_eq!(
            parse_rich_message_event_metadata(&seven_fields),
            Err(RichMessageError::InvalidEventShape)
        );

        let mut empty_metadata = legacy_event();
        empty_metadata.push(FrameValue::Nil);
        empty_metadata.push(FrameValue::Array(Vec::new()));
        assert_eq!(
            parse_rich_message_event_metadata(&empty_metadata),
            Err(RichMessageError::EmptyMetadata)
        );
    }

    #[test]
    fn durable_hash_covers_reply_and_mentions() {
        let base = rich_message().into_frame_body().expect("base body");
        let changed_reply = RichMessageBody {
            reply_to: Some(ReplyReference {
                room_id: 7,
                event_id: 43,
            }),
            ..rich_message()
        }
        .into_frame_body()
        .expect("changed reply");
        let changed_mentions = RichMessageBody {
            mentioned_user_ids: vec![2, 10],
            ..rich_message()
        }
        .into_frame_body()
        .expect("changed mentions");

        let hash = canonical_mutation_request_hash(ChatOp::RoomMessage, Some(7), &base)
            .expect("base hash");
        assert_ne!(
            hash,
            canonical_mutation_request_hash(ChatOp::RoomMessage, Some(7), &changed_reply)
                .expect("reply hash")
        );
        assert_ne!(
            hash,
            canonical_mutation_request_hash(ChatOp::RoomMessage, Some(7), &changed_mentions)
                .expect("mention hash")
        );
    }
}
