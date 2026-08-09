pub use super::protocol::Rgb24;
use super::protocol::{
    EventId, ModerationAuditAction, ModerationAuditPage, ModerationAuditRecord, RoomId, ServerId,
    UserId,
};
pub use super::protocol::{MessageRevisionAction, ReactionToken};

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
pub const CHAT_MESSAGE_REVISION_MAX_ROWS_PER_ROOM: usize = 1_024;
pub const CHAT_MESSAGE_REVISION_MAX_BYTES_PER_ROOM: usize = 8 * 1024 * 1024;
pub const CHAT_MESSAGE_REVISION_MAX_ROWS_PER_SERVER: usize = 8_192;
pub const CHAT_MESSAGE_REVISION_MAX_BYTES_PER_SERVER: usize = 32 * 1024 * 1024;
pub const CHAT_MESSAGE_REVISION_MAX_ROWS: usize = 32_768;
pub const CHAT_MESSAGE_REVISION_MAX_BYTES: usize = 64 * 1024 * 1024;
pub const CHAT_PIN_MAX_ROWS_PER_SERVER: usize = 1_024;
pub const CHAT_PIN_MAX_ROWS: usize = 4_096;
pub const CHAT_PIN_MAX_BYTES: usize = 1024 * 1024;
const CHAT_MESSAGE_REVISION_FIXED_RETAINED_BYTES: usize = 96;
const CHAT_PIN_FIXED_RETAINED_BYTES: usize = 64;

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
    pub profile_revision: u64,
    pub nickname_colour_rgb: Option<Rgb24>,
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

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum ChatModerationAuditRequestState {
    #[default]
    Idle,
    Receiving,
    Complete {
        has_more: bool,
    },
    Failed(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChatModerationAuditView<'a> {
    Unavailable,
    Unauthorized,
    Ready,
    Receiving {
        previous_records: &'a [ModerationAuditRecord],
    },
    Failed {
        message: &'a str,
        previous_records: &'a [ModerationAuditRecord],
    },
    Empty,
    Loaded {
        records: &'a [ModerationAuditRecord],
        has_more: bool,
    },
}

pub fn chat_moderation_audit_view<'a>(
    authorized: bool,
    negotiated: bool,
    state: &'a ChatModerationAuditRequestState,
    page: Option<&'a ModerationAuditPage>,
) -> ChatModerationAuditView<'a> {
    if !authorized {
        return ChatModerationAuditView::Unauthorized;
    }
    if !negotiated {
        return ChatModerationAuditView::Unavailable;
    }
    let records = page.map_or(&[][..], |page| page.records.as_slice());
    match state {
        ChatModerationAuditRequestState::Idle => ChatModerationAuditView::Ready,
        ChatModerationAuditRequestState::Receiving => ChatModerationAuditView::Receiving {
            previous_records: records,
        },
        ChatModerationAuditRequestState::Complete { .. } if records.is_empty() => {
            ChatModerationAuditView::Empty
        }
        ChatModerationAuditRequestState::Complete { has_more } => ChatModerationAuditView::Loaded {
            records,
            has_more: *has_more,
        },
        ChatModerationAuditRequestState::Failed(message) => ChatModerationAuditView::Failed {
            message,
            previous_records: records,
        },
    }
}

pub fn moderation_audit_action_label(action: ModerationAuditAction) -> &'static str {
    match action {
        ModerationAuditAction::Kick => "kicked",
        ModerationAuditAction::Ban => "banned",
        ModerationAuditAction::Unban => "unbanned",
        ModerationAuditAction::Mute => "muted",
        ModerationAuditAction::Unmute => "unmuted",
        ModerationAuditAction::RoleChange => "changed role for",
    }
}

pub fn moderation_audit_result_label(record: &ModerationAuditRecord) -> String {
    match record.action {
        ModerationAuditAction::Kick => "removed from room".into(),
        ModerationAuditAction::Ban => "status: banned".into(),
        ModerationAuditAction::Unban => "status: active".into(),
        ModerationAuditAction::Mute => "status: muted".into(),
        ModerationAuditAction::Unmute => "status: unmuted".into(),
        ModerationAuditAction::RoleChange => match record.result_role_bits.unwrap_or_default() {
            bits if bits & CHAT_ROLE_ADMIN != 0 => "role: admin".into(),
            bits if bits & CHAT_ROLE_MODERATOR != 0 => "role: moderator".into(),
            bits if bits & CHAT_ROLE_TRUSTED != 0 => "role: trusted".into(),
            _ => "role: member".into(),
        },
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChatMessageRevision {
    pub server_id: ServerId,
    pub room_id: RoomId,
    pub target_event_id: EventId,
    pub latest_revision_event_id: EventId,
    pub action: MessageRevisionAction,
    pub actor_user_id: UserId,
    pub replacement_body: Option<String>,
    pub at_unix: i64,
    pub revision_number: u64,
}

impl ChatMessageRevision {
    pub fn retained_bytes(&self) -> usize {
        CHAT_MESSAGE_REVISION_FIXED_RETAINED_BYTES
            .saturating_add(self.server_id.len())
            .saturating_add(self.replacement_body.as_ref().map_or(0, String::len))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChatPin {
    pub server_id: ServerId,
    pub room_id: RoomId,
    pub target_event_id: EventId,
    pub pin_event_id: EventId,
    pub actor_user_id: UserId,
    pub pinned_at_unix: i64,
}

impl ChatPin {
    pub fn retained_bytes(&self) -> usize {
        CHAT_PIN_FIXED_RETAINED_BYTES.saturating_add(self.server_id.len())
    }
}

pub fn chat_pins_fit_bounds<'a>(pins: impl IntoIterator<Item = &'a ChatPin>) -> bool {
    let mut total_rows = 0_usize;
    let mut total_bytes = 0_usize;
    let mut server_rows = std::collections::BTreeMap::<&str, usize>::new();
    for pin in pins {
        total_rows = total_rows.saturating_add(1);
        total_bytes = total_bytes.saturating_add(pin.retained_bytes());
        *server_rows.entry(pin.server_id.as_str()).or_default() += 1;
    }
    total_rows <= CHAT_PIN_MAX_ROWS
        && total_bytes <= CHAT_PIN_MAX_BYTES
        && server_rows
            .values()
            .all(|rows| *rows <= CHAT_PIN_MAX_ROWS_PER_SERVER)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChatMessageRevisionPresentation<'a> {
    Original(&'a str),
    Edited { body: &'a str, revision_number: u64 },
    Deleted { revision_number: u64 },
}

pub fn chat_message_revision_presentation<'a>(
    event: &'a ChatEvent,
    revision: Option<&'a ChatMessageRevision>,
) -> Option<ChatMessageRevisionPresentation<'a>> {
    let original = match &event.kind {
        ChatEventKind::Message { body } | ChatEventKind::RichMessage { body, .. } => body.as_str(),
        _ => return None,
    };
    let Some(revision) = revision.filter(|revision| {
        revision.server_id == event.server_id
            && revision.room_id == event.room_id
            && revision.target_event_id == event.event_id
    }) else {
        return Some(ChatMessageRevisionPresentation::Original(original));
    };
    match revision.action {
        MessageRevisionAction::Correct => revision.replacement_body.as_deref().map(|body| {
            ChatMessageRevisionPresentation::Edited {
                body,
                revision_number: revision.revision_number,
            }
        }),
        MessageRevisionAction::Tombstone => Some(ChatMessageRevisionPresentation::Deleted {
            revision_number: revision.revision_number,
        }),
    }
}

pub fn chat_message_revisions_fit_bounds<'a>(
    revisions: impl IntoIterator<Item = &'a ChatMessageRevision>,
) -> bool {
    let mut total_rows = 0_usize;
    let mut total_bytes = 0_usize;
    let mut server_usage = std::collections::BTreeMap::<&str, (usize, usize)>::new();
    let mut room_usage = std::collections::BTreeMap::<(&str, RoomId), (usize, usize)>::new();
    for revision in revisions {
        let bytes = revision.retained_bytes();
        total_rows = total_rows.saturating_add(1);
        total_bytes = total_bytes.saturating_add(bytes);
        let server = server_usage.entry(revision.server_id.as_str()).or_default();
        server.0 = server.0.saturating_add(1);
        server.1 = server.1.saturating_add(bytes);
        let room = room_usage
            .entry((revision.server_id.as_str(), revision.room_id))
            .or_default();
        room.0 = room.0.saturating_add(1);
        room.1 = room.1.saturating_add(bytes);
    }
    total_rows <= CHAT_MESSAGE_REVISION_MAX_ROWS
        && total_bytes <= CHAT_MESSAGE_REVISION_MAX_BYTES
        && server_usage.values().all(|(rows, bytes)| {
            *rows <= CHAT_MESSAGE_REVISION_MAX_ROWS_PER_SERVER
                && *bytes <= CHAT_MESSAGE_REVISION_MAX_BYTES_PER_SERVER
        })
        && room_usage.values().all(|(rows, bytes)| {
            *rows <= CHAT_MESSAGE_REVISION_MAX_ROWS_PER_ROOM
                && *bytes <= CHAT_MESSAGE_REVISION_MAX_BYTES_PER_ROOM
        })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChatReactionSummary {
    pub token: ReactionToken,
    pub actor_count: u32,
    pub reacted_by_local_user: bool,
}

impl ChatReactionSummary {
    pub fn label(&self) -> String {
        let token = match self.token {
            ReactionToken::ThumbsUp => "+1",
            ReactionToken::Heart => "heart",
            ReactionToken::Laugh => "laugh",
            ReactionToken::Surprised => "wow",
            ReactionToken::Sad => "sad",
            ReactionToken::ThumbsDown => "-1",
            ReactionToken::Celebrate => "celebrate",
            ReactionToken::Question => "?",
        };
        if self.reacted_by_local_user {
            format!("{token} {} · you", self.actor_count)
        } else {
            format!("{token} {}", self.actor_count)
        }
    }
}

pub fn chat_reaction_summaries<'a>(
    reactions: impl IntoIterator<Item = &'a ChatReaction>,
    event: &ChatEvent,
    local_user_id: Option<UserId>,
) -> Vec<ChatReactionSummary> {
    if !chat_event_supports_reactions(event) {
        return Vec::new();
    }
    let mut actors_by_token =
        std::collections::BTreeMap::<ReactionToken, std::collections::BTreeSet<UserId>>::new();
    for reaction in reactions {
        if reaction.server_id == event.server_id
            && reaction.room_id == event.room_id
            && reaction.target_event_id == event.event_id
        {
            actors_by_token
                .entry(reaction.token)
                .or_default()
                .insert(reaction.actor_user_id);
        }
    }
    ReactionToken::ALL
        .into_iter()
        .filter_map(|token| {
            let actors = actors_by_token.get(&token)?;
            Some(ChatReactionSummary {
                token,
                actor_count: u32::try_from(actors.len()).unwrap_or(u32::MAX),
                reacted_by_local_user: local_user_id
                    .is_some_and(|user_id| actors.contains(&user_id)),
            })
        })
        .collect()
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

pub fn chat_event_supports_pins(event: &ChatEvent) -> bool {
    chat_event_supports_reactions(event)
}

pub fn chat_event_supports_message_revisions(event: &ChatEvent) -> bool {
    matches!(
        event.kind,
        ChatEventKind::Message { .. } | ChatEventKind::RichMessage { .. }
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
            profile_revision: 0,
            nickname_colour_rgb: None,
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

    #[test]
    fn reaction_summaries_are_deduplicated_ordered_and_identity_scoped() {
        let event = message_event(10, "hello");
        let reaction = |server_id: &str,
                        room_id: RoomId,
                        target_event_id: EventId,
                        actor_user_id: UserId,
                        token: ReactionToken| ChatReaction {
            server_id: server_id.into(),
            room_id,
            target_event_id,
            actor_user_id,
            token,
            created_at_unix: 1,
        };
        let heart = reaction("server", 1, 10, 7, ReactionToken::Heart);
        let reactions = vec![
            reaction("server", 1, 10, 8, ReactionToken::ThumbsUp),
            heart.clone(),
            heart,
            reaction("server", 1, 10, 9, ReactionToken::Heart),
            reaction("other", 1, 10, 7, ReactionToken::Sad),
            reaction("server", 2, 10, 7, ReactionToken::Sad),
            reaction("server", 1, 11, 7, ReactionToken::Sad),
        ];

        let summaries = chat_reaction_summaries(&reactions, &event, Some(7));
        assert_eq!(
            summaries,
            vec![
                ChatReactionSummary {
                    token: ReactionToken::ThumbsUp,
                    actor_count: 1,
                    reacted_by_local_user: false,
                },
                ChatReactionSummary {
                    token: ReactionToken::Heart,
                    actor_count: 2,
                    reacted_by_local_user: true,
                },
            ]
        );
        assert_eq!(summaries[0].label(), "+1 1");
        assert_eq!(summaries[1].label(), "heart 2 · you");

        let system = ChatEvent {
            kind: ChatEventKind::System {
                body: "status".into(),
            },
            ..event
        };
        assert!(chat_reaction_summaries(&reactions, &system, Some(7)).is_empty());
    }

    #[test]
    fn message_revision_presentation_preserves_original_and_derives_effective_state() {
        let event = message_event(10, "original");
        assert_eq!(
            chat_message_revision_presentation(&event, None),
            Some(ChatMessageRevisionPresentation::Original("original"))
        );
        let corrected = ChatMessageRevision {
            server_id: event.server_id.clone(),
            room_id: event.room_id,
            target_event_id: event.event_id,
            latest_revision_event_id: 20,
            action: MessageRevisionAction::Correct,
            actor_user_id: 7,
            replacement_body: Some("corrected".into()),
            at_unix: 2,
            revision_number: 1,
        };
        assert_eq!(
            chat_message_revision_presentation(&event, Some(&corrected)),
            Some(ChatMessageRevisionPresentation::Edited {
                body: "corrected",
                revision_number: 1,
            })
        );
        let tombstone = ChatMessageRevision {
            action: MessageRevisionAction::Tombstone,
            replacement_body: None,
            latest_revision_event_id: 21,
            revision_number: 2,
            ..corrected
        };
        assert_eq!(
            chat_message_revision_presentation(&event, Some(&tombstone)),
            Some(ChatMessageRevisionPresentation::Deleted { revision_number: 2 })
        );
        assert!(matches!(
            event.kind,
            ChatEventKind::Message { ref body } if body == "original"
        ));
    }

    #[test]
    fn message_revision_projection_has_stable_owned_byte_and_scope_bounds() {
        let revision = |target_event_id, replacement_body: Option<String>| ChatMessageRevision {
            server_id: "server".into(),
            room_id: 1,
            target_event_id,
            latest_revision_event_id: target_event_id.saturating_add(10_000),
            action: if replacement_body.is_some() {
                MessageRevisionAction::Correct
            } else {
                MessageRevisionAction::Tombstone
            },
            actor_user_id: 7,
            replacement_body,
            at_unix: 1,
            revision_number: 1,
        };
        assert_eq!(
            revision(1, Some("edited".into())).retained_bytes(),
            CHAT_MESSAGE_REVISION_FIXED_RETAINED_BYTES + "server".len() + "edited".len()
        );
        let at_room_limit = (1..=CHAT_MESSAGE_REVISION_MAX_ROWS_PER_ROOM as u64)
            .map(|target| revision(target, None))
            .collect::<Vec<_>>();
        assert!(chat_message_revisions_fit_bounds(&at_room_limit));
        let mut over_room_limit = at_room_limit;
        over_room_limit.push(revision(
            CHAT_MESSAGE_REVISION_MAX_ROWS_PER_ROOM as u64 + 1,
            None,
        ));
        assert!(!chat_message_revisions_fit_bounds(&over_room_limit));
        let oversized = [revision(
            1,
            Some("x".repeat(CHAT_MESSAGE_REVISION_MAX_BYTES_PER_ROOM)),
        )];
        assert!(!chat_message_revisions_fit_bounds(&oversized));
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

    #[test]
    fn moderation_audit_view_distinguishes_authority_progress_empty_and_failure() {
        let record = ModerationAuditRecord {
            audit_id: 2,
            room_id: 1,
            actor_user_id: 7,
            actor_display_name_at_action: "Moderator".into(),
            target_user_id: Some(8),
            target_display_name_at_action: Some("Member".into()),
            action: ModerationAuditAction::RoleChange,
            committed_at_unix: 1_700_000_000,
            result_role_bits: Some(CHAT_ROLE_TRUSTED),
            result_status_bits: None,
        };
        let page = ModerationAuditPage {
            records: vec![record.clone()],
        };
        assert_eq!(
            chat_moderation_audit_view(
                false,
                true,
                &ChatModerationAuditRequestState::Idle,
                Some(&page),
            ),
            ChatModerationAuditView::Unauthorized
        );
        assert_eq!(
            chat_moderation_audit_view(
                true,
                false,
                &ChatModerationAuditRequestState::Idle,
                Some(&page),
            ),
            ChatModerationAuditView::Unavailable
        );
        assert_eq!(
            chat_moderation_audit_view(
                true,
                true,
                &ChatModerationAuditRequestState::Idle,
                Some(&page),
            ),
            ChatModerationAuditView::Ready
        );
        assert!(matches!(
            chat_moderation_audit_view(
                true,
                true,
                &ChatModerationAuditRequestState::Receiving,
                Some(&page),
            ),
            ChatModerationAuditView::Receiving {
                previous_records: [retained]
            } if retained == &record
        ));
        assert!(matches!(
            chat_moderation_audit_view(
                true,
                true,
                &ChatModerationAuditRequestState::Complete { has_more: true },
                Some(&page),
            ),
            ChatModerationAuditView::Loaded {
                records: [retained],
                has_more: true,
            } if retained == &record
        ));
        assert_eq!(
            chat_moderation_audit_view(
                true,
                true,
                &ChatModerationAuditRequestState::Complete { has_more: false },
                Some(&ModerationAuditPage { records: vec![] }),
            ),
            ChatModerationAuditView::Empty
        );
        assert!(matches!(
            chat_moderation_audit_view(
                true,
                true,
                &ChatModerationAuditRequestState::Failed("request failed".into()),
                Some(&page),
            ),
            ChatModerationAuditView::Failed {
                message: "request failed",
                previous_records: [retained],
            } if retained == &record
        ));
        assert_eq!(
            moderation_audit_action_label(record.action),
            "changed role for"
        );
        assert_eq!(moderation_audit_result_label(&record), "role: trusted");
    }
}
