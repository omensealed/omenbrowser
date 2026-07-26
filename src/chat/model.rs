pub use super::protocol::ReactionToken;
use super::protocol::{EventId, RoomId, ServerId, UserId};

pub const CHAT_CLIENT_MAX_SESSIONS: usize = 64;
pub const CHAT_SESSION_MAX_ROOMS: usize = 256;
pub const CHAT_SESSION_MAX_ROOM_BYTES: usize = 512 * 1024;
pub const CHAT_SESSION_MAX_USERS: usize = 1_024;
pub const CHAT_SESSION_MAX_USER_BYTES: usize = 1024 * 1024;
pub const CHAT_SERVER_DISPLAY_MAX_BYTES: usize = 256;
pub const CHAT_SERVER_ID_MAX_BYTES: usize = 4 * 1024;
pub const CHAT_SERVER_DESTINATION_MAX_BYTES: usize = 4 * 1024;
pub const CHAT_ROOM_NAME_MAX_BYTES: usize = 64;
pub const CHAT_ROOM_TOPIC_MAX_BYTES: usize = 4 * 1024;
pub const CHAT_USER_DISPLAY_MAX_BYTES: usize = 256;
pub const CHAT_ACTOR_DISPLAY_MAX_BYTES: usize = 256;
pub const CHAT_REPLY_PREVIEW_MAX_BYTES: usize = 160;
pub const CHAT_MOTD_MAX_BYTES: usize = 16 * 1024;
pub const CHAT_STATUS_MAX_BYTES: usize = 4 * 1024;
pub const CHAT_RESOURCE_ID_MAX_BYTES: usize = 4 * 1024;
pub const CHAT_UPLOAD_FILENAME_MAX_BYTES: usize = 4 * 1024;
pub const CHAT_CONTENT_TYPE_MAX_BYTES: usize = 1_024;
pub const CHAT_REACTION_MAX_TOKENS_PER_ACTOR_TARGET: usize = 3;
pub const CHAT_REACTION_MAX_ROWS_PER_TARGET: usize = 128;
pub const CHAT_REACTION_MAX_ROWS_PER_ROOM: usize = 4_096;
pub const CHAT_REACTION_MAX_BYTES_PER_ROOM: usize = 128 * 1024;
pub const CHAT_REACTION_MAX_ROWS_PER_SERVER: usize = 8_192;
pub const CHAT_REACTION_MAX_BYTES_PER_SERVER: usize = 512 * 1024;
pub const CHAT_REACTION_MAX_ROWS: usize = 32_768;
pub const CHAT_REACTION_MAX_BYTES: usize = 2 * 1024 * 1024;

pub fn bounded_chat_text(value: &str, max_bytes: usize) -> String {
    if value.len() <= max_bytes {
        return value.to_owned();
    }
    if max_bytes < '…'.len_utf8() {
        return String::new();
    }
    let mut end = max_bytes - '…'.len_utf8();
    while !value.is_char_boundary(end) {
        end = end.saturating_sub(1);
    }
    let mut bounded = String::with_capacity(max_bytes);
    bounded.push_str(&value[..end]);
    bounded.push('…');
    bounded
}

pub fn chat_text_fits(value: &str, max_bytes: usize) -> bool {
    value.len() <= max_bytes
}

pub const CHAT_STATUS_BANNED: u32 = 1;
pub const CHAT_STATUS_MUTED: u32 = 1 << 1;
pub const CHAT_ROLE_TRUSTED: u64 = 1;
pub const CHAT_ROLE_MODERATOR: u64 = 1 << 1;
pub const CHAT_ROLE_ADMIN: u64 = 1 << 2;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChatServerSummary {
    pub server_id: ServerId,
    pub destination: String,
    pub display_name: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChatRoomSummary {
    pub server_id: ServerId,
    pub room_id: RoomId,
    pub name: String,
    pub topic: Option<String>,
    pub unread: u32,
    pub joined: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChatUserSummary {
    pub server_id: ServerId,
    pub user_id: UserId,
    pub display_name: String,
    pub role_bits: u64,
    pub status_bits: u32,
    pub lxmf_available: bool,
}

impl ChatUserSummary {
    pub fn role_label(&self) -> &'static str {
        if self.role_bits & CHAT_ROLE_ADMIN != 0 {
            "admin"
        } else if self.role_bits & CHAT_ROLE_MODERATOR != 0 {
            "mod"
        } else if self.role_bits & CHAT_ROLE_TRUSTED != 0 {
            "trusted"
        } else {
            "member"
        }
    }

    pub fn status_label(&self) -> Option<&'static str> {
        if self.status_bits & CHAT_STATUS_BANNED != 0 {
            Some("banned")
        } else if self.status_bits & CHAT_STATUS_MUTED != 0 {
            Some("muted")
        } else {
            None
        }
    }

    pub fn display_label(&self) -> String {
        let mut label = self.display_name.clone();
        let role = self.role_label();
        if role != "member" {
            label.push_str(" [");
            label.push_str(role);
            label.push(']');
        }
        if let Some(status) = self.status_label() {
            label.push_str(" (");
            label.push_str(status);
            label.push(')');
        }
        label
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChatEvent {
    pub server_id: ServerId,
    pub room_id: RoomId,
    pub event_id: EventId,
    pub actor_user_id: Option<UserId>,
    pub actor_display_name: Option<String>,
    pub at_unix: i64,
    pub kind: ChatEventKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChatEventKind {
    Message {
        body: String,
    },
    RichMessage {
        body: String,
        metadata: ChatMessageMetadata,
    },
    Action {
        body: String,
    },
    Notice {
        body: String,
    },
    System {
        body: String,
    },
    Upload {
        resource_id: String,
        filename: String,
        bytes: u64,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChatMessageMetadata {
    pub reply_to_event_id: Option<EventId>,
    pub mentioned_user_ids: Vec<UserId>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ChatReaction {
    pub server_id: ServerId,
    pub room_id: RoomId,
    pub target_event_id: EventId,
    pub actor_user_id: UserId,
    pub token: ReactionToken,
    pub created_at_unix: i64,
}

impl ChatReaction {
    pub fn retained_bytes(&self) -> usize {
        std::mem::size_of::<Self>()
            .saturating_add(self.server_id.len())
            .saturating_add(self.token.as_str().len())
    }
}

pub fn chat_reactions_fit_bounds<'a>(
    reactions: impl IntoIterator<Item = &'a ChatReaction>,
) -> bool {
    use std::collections::BTreeMap;

    let mut total_rows = 0_usize;
    let mut total_bytes = 0_usize;
    let mut server_usage = BTreeMap::<&str, (usize, usize)>::new();
    let mut room_usage = BTreeMap::<(&str, RoomId), (usize, usize)>::new();
    let mut target_rows = BTreeMap::<(&str, RoomId, EventId), usize>::new();
    let mut actor_target_rows = BTreeMap::<(&str, RoomId, EventId, UserId), usize>::new();
    for reaction in reactions {
        let bytes = reaction.retained_bytes();
        total_rows = total_rows.saturating_add(1);
        total_bytes = total_bytes.saturating_add(bytes);
        let server = server_usage.entry(reaction.server_id.as_str()).or_default();
        server.0 = server.0.saturating_add(1);
        server.1 = server.1.saturating_add(bytes);
        let room = room_usage
            .entry((reaction.server_id.as_str(), reaction.room_id))
            .or_default();
        room.0 = room.0.saturating_add(1);
        room.1 = room.1.saturating_add(bytes);
        *target_rows
            .entry((
                reaction.server_id.as_str(),
                reaction.room_id,
                reaction.target_event_id,
            ))
            .or_default() += 1;
        *actor_target_rows
            .entry((
                reaction.server_id.as_str(),
                reaction.room_id,
                reaction.target_event_id,
                reaction.actor_user_id,
            ))
            .or_default() += 1;
    }
    total_rows <= CHAT_REACTION_MAX_ROWS
        && total_bytes <= CHAT_REACTION_MAX_BYTES
        && server_usage.values().all(|(rows, bytes)| {
            *rows <= CHAT_REACTION_MAX_ROWS_PER_SERVER
                && *bytes <= CHAT_REACTION_MAX_BYTES_PER_SERVER
        })
        && room_usage.values().all(|(rows, bytes)| {
            *rows <= CHAT_REACTION_MAX_ROWS_PER_ROOM && *bytes <= CHAT_REACTION_MAX_BYTES_PER_ROOM
        })
        && target_rows
            .values()
            .all(|rows| *rows <= CHAT_REACTION_MAX_ROWS_PER_TARGET)
        && actor_target_rows
            .values()
            .all(|rows| *rows <= CHAT_REACTION_MAX_TOKENS_PER_ACTOR_TARGET)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChatMessagePresentation {
    pub reply: Option<ChatReplyPresentation>,
    pub mentions_local_user: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChatReplyPresentation {
    Available {
        event_id: EventId,
        actor_display_name: Option<String>,
        preview: String,
    },
    Unavailable {
        event_id: EventId,
    },
}

impl ChatEvent {
    pub fn message_metadata(&self) -> Option<&ChatMessageMetadata> {
        match &self.kind {
            ChatEventKind::RichMessage { metadata, .. } => Some(metadata),
            _ => None,
        }
    }
}

pub fn chat_event_supports_reactions(event: &ChatEvent) -> bool {
    matches!(
        event.kind,
        ChatEventKind::Message { .. }
            | ChatEventKind::RichMessage { .. }
            | ChatEventKind::Action { .. }
            | ChatEventKind::Notice { .. }
            | ChatEventKind::Upload { .. }
    )
}

pub fn chat_message_presentation(
    events: &[ChatEvent],
    event: &ChatEvent,
    local_user_id: Option<UserId>,
) -> ChatMessagePresentation {
    let Some(metadata) = event.message_metadata() else {
        return ChatMessagePresentation {
            reply: None,
            mentions_local_user: false,
        };
    };
    let reply = metadata.reply_to_event_id.map(|event_id| {
        events
            .iter()
            .find(|candidate| {
                candidate.server_id == event.server_id
                    && candidate.room_id == event.room_id
                    && candidate.event_id == event_id
            })
            .map(|original| ChatReplyPresentation::Available {
                event_id,
                actor_display_name: original.actor_display_name.clone(),
                preview: bounded_chat_text(
                    chat_event_preview_text(original),
                    CHAT_REPLY_PREVIEW_MAX_BYTES,
                ),
            })
            .unwrap_or(ChatReplyPresentation::Unavailable { event_id })
    });
    ChatMessagePresentation {
        reply,
        mentions_local_user: local_user_id
            .is_some_and(|user_id| metadata.mentioned_user_ids.binary_search(&user_id).is_ok()),
    }
}

pub fn retained_local_mention_count(
    events: &[ChatEvent],
    server_id: &ServerId,
    room_id: RoomId,
    local_user_id: Option<UserId>,
) -> u32 {
    let Some(local_user_id) = local_user_id else {
        return 0;
    };
    events
        .iter()
        .filter(|event| event.server_id == *server_id && event.room_id == room_id)
        .filter_map(ChatEvent::message_metadata)
        .filter(|metadata| {
            metadata
                .mentioned_user_ids
                .binary_search(&local_user_id)
                .is_ok()
        })
        .fold(0_u32, |count, _| count.saturating_add(1))
}

fn chat_event_preview_text(event: &ChatEvent) -> &str {
    match &event.kind {
        ChatEventKind::Message { body }
        | ChatEventKind::RichMessage { body, .. }
        | ChatEventKind::Action { body }
        | ChatEventKind::Notice { body }
        | ChatEventKind::System { body } => body,
        ChatEventKind::Upload { filename, .. } => filename,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn user(role_bits: u64, status_bits: u32) -> ChatUserSummary {
        ChatUserSummary {
            server_id: "server".into(),
            user_id: 1,
            display_name: "Alice".into(),
            role_bits,
            status_bits,
            lxmf_available: false,
        }
    }

    #[test]
    fn bounded_chat_text_preserves_utf8_and_exact_byte_ceiling() {
        assert_eq!(bounded_chat_text("hello", 5), "hello");
        let bounded = bounded_chat_text(&"☃".repeat(10), 10);
        assert!(bounded.is_char_boundary(bounded.len()));
        assert!(bounded.len() <= 10);
        assert!(bounded.ends_with('…'));
        assert_eq!(bounded_chat_text("hello", 2), "");
    }

    #[test]
    fn user_display_labels_include_roles_and_moderation_status() {
        assert_eq!(user(0, 0).display_label(), "Alice");
        assert_eq!(
            user(CHAT_ROLE_TRUSTED, 0).display_label(),
            "Alice [trusted]"
        );
        assert_eq!(
            user(CHAT_ROLE_TRUSTED | CHAT_ROLE_MODERATOR, CHAT_STATUS_MUTED).display_label(),
            "Alice [mod] (muted)"
        );
        assert_eq!(
            user(
                CHAT_ROLE_TRUSTED | CHAT_ROLE_MODERATOR | CHAT_ROLE_ADMIN,
                CHAT_STATUS_BANNED
            )
            .display_label(),
            "Alice [admin] (banned)"
        );
    }

    fn message_event(event_id: EventId, body: &str) -> ChatEvent {
        ChatEvent {
            server_id: "server".into(),
            room_id: 1,
            event_id,
            actor_user_id: Some(1),
            actor_display_name: Some("Alice".into()),
            at_unix: 1,
            kind: ChatEventKind::Message { body: body.into() },
        }
    }

    #[test]
    fn rich_message_presentation_uses_only_retained_same_room_evidence() {
        let original = message_event(10, &"x".repeat(CHAT_REPLY_PREVIEW_MAX_BYTES + 20));
        let rich = ChatEvent {
            event_id: 11,
            actor_user_id: Some(2),
            actor_display_name: Some("Bob".into()),
            kind: ChatEventKind::RichMessage {
                body: "reply".into(),
                metadata: ChatMessageMetadata {
                    reply_to_event_id: Some(10),
                    mentioned_user_ids: vec![1, 9],
                },
            },
            ..message_event(11, "reply")
        };
        let presentation =
            chat_message_presentation(&[original.clone(), rich.clone()], &rich, Some(1));
        assert!(presentation.mentions_local_user);
        assert!(matches!(
            presentation.reply,
            Some(ChatReplyPresentation::Available {
                event_id: 10,
                actor_display_name: Some(ref actor),
                ref preview,
            }) if actor == "Alice"
                && preview.len() <= CHAT_REPLY_PREVIEW_MAX_BYTES
                && preview.ends_with('…')
        ));

        let unavailable = chat_message_presentation(std::slice::from_ref(&rich), &rich, Some(1));
        assert_eq!(
            unavailable.reply,
            Some(ChatReplyPresentation::Unavailable { event_id: 10 })
        );
        let wrong_room = ChatEvent {
            room_id: 2,
            ..original
        };
        assert_eq!(
            chat_message_presentation(&[wrong_room, rich.clone()], &rich, Some(1)).reply,
            Some(ChatReplyPresentation::Unavailable { event_id: 10 })
        );
        assert!(
            !chat_message_presentation(std::slice::from_ref(&rich), &rich, Some(8))
                .mentions_local_user
        );
        assert!(
            !chat_message_presentation(std::slice::from_ref(&rich), &rich, None)
                .mentions_local_user
        );
    }

    #[test]
    fn retained_mention_count_is_server_room_and_numeric_identity_scoped() {
        let rich = |server: &str, room_id, event_id, mentions: Vec<u32>| ChatEvent {
            server_id: server.into(),
            room_id,
            event_id,
            actor_user_id: Some(2),
            actor_display_name: Some("Bob".into()),
            at_unix: 1,
            kind: ChatEventKind::RichMessage {
                body: "@display text is not evidence".into(),
                metadata: ChatMessageMetadata {
                    reply_to_event_id: None,
                    mentioned_user_ids: mentions,
                },
            },
        };
        let events = vec![
            rich("server", 1, 1, vec![7]),
            rich("server", 1, 2, vec![3, 7]),
            rich("server", 2, 3, vec![7]),
            rich("other", 1, 4, vec![7]),
            message_event(5, "@Alice"),
        ];
        assert_eq!(
            retained_local_mention_count(&events, &"server".into(), 1, Some(7)),
            2
        );
        assert_eq!(
            retained_local_mention_count(&events, &"server".into(), 1, Some(8)),
            0
        );
        assert_eq!(
            retained_local_mention_count(&events, &"server".into(), 1, None),
            0
        );
    }
}
