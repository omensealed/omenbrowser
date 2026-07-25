use crate::app::current_epoch_ms;
use crate::chat::protocol::RoomId;
use crate::chat::{
    chat_message_presentation, ChatEvent, ChatEventKind, ChatReplyPresentation, ChatSessionId,
    ChatSessionView,
};

use super::super::{
    format_epoch_secs, OMENCHAT_LOCAL_ECHO_RESEND_SECS, OMENCHAT_MESSAGE_GROUP_GAP_SECS,
};
use super::{chat_event_actor_label, human_bytes, is_omenchat_local_echo_event};

pub(in crate::desktop) struct ChatTimelineGroup {
    pub(in crate::desktop) actor_key: String,
    pub(in crate::desktop) actor: String,
    pub(in crate::desktop) at_unix: i64,
    pub(in crate::desktop) last_at_unix: i64,
    pub(in crate::desktop) bodies: Vec<ChatTimelineBody>,
}

pub(in crate::desktop) struct ChatTimelineBody {
    pub(in crate::desktop) text: String,
    pub(in crate::desktop) is_action: bool,
    pub(in crate::desktop) pending_acceptance: bool,
    pub(in crate::desktop) mentions_local_user: bool,
    pub(in crate::desktop) reply: Option<ChatTimelineReply>,
    pub(in crate::desktop) upload: Option<ChatTimelineUpload>,
    pub(in crate::desktop) resend: Option<ChatTimelineResend>,
}

pub(in crate::desktop) enum ChatTimelineReply {
    Available {
        session_id: ChatSessionId,
        room_id: RoomId,
        event_id: u64,
        label: String,
    },
    Unavailable {
        event_id: u64,
    },
}

#[derive(Clone)]
pub(in crate::desktop) struct ChatTimelineUpload {
    pub(in crate::desktop) session_id: ChatSessionId,
    pub(in crate::desktop) resource_id: String,
}

pub(in crate::desktop) struct ChatTimelineResend {
    pub(in crate::desktop) session_id: ChatSessionId,
    pub(in crate::desktop) room_id: RoomId,
    pub(in crate::desktop) event_id: u64,
    pub(in crate::desktop) body: String,
    pub(in crate::desktop) action: bool,
}

pub(in crate::desktop) fn chat_event_actor_key(
    session: &ChatSessionView,
    event: &ChatEvent,
) -> String {
    let prefix = match event.kind {
        ChatEventKind::Action { .. } => "action",
        ChatEventKind::Upload { .. } => "upload",
        _ => "message",
    };
    event
        .actor_user_id
        .map(|actor_id| format!("{prefix}:id:{actor_id}"))
        .unwrap_or_else(|| format!("{prefix}:label:{}", chat_event_actor_label(session, event)))
}

pub(in crate::desktop) fn chat_event_body(
    session: &ChatSessionView,
    event: &ChatEvent,
    local_user_id: Option<u32>,
) -> ChatTimelineBody {
    let presentation = chat_message_presentation(&session.events, event, local_user_id);
    let reply = presentation.reply.map(|reply| match reply {
        ChatReplyPresentation::Available {
            event_id,
            actor_display_name,
            preview,
        } => ChatTimelineReply::Available {
            session_id: session.session_id,
            room_id: event.room_id,
            event_id,
            label: actor_display_name
                .map(|actor| format!("↳ {actor}: {preview}"))
                .unwrap_or_else(|| format!("↳ {preview}")),
        },
        ChatReplyPresentation::Unavailable { event_id } => {
            ChatTimelineReply::Unavailable { event_id }
        }
    });
    match &event.kind {
        ChatEventKind::Action { body } => ChatTimelineBody {
            text: format!("* {} {body}", chat_event_actor_label(session, event)),
            is_action: true,
            pending_acceptance: is_omenchat_local_echo_event(event),
            mentions_local_user: presentation.mentions_local_user,
            reply,
            upload: None,
            resend: local_echo_resend(session, event, body, true),
        },
        ChatEventKind::Message { body }
        | ChatEventKind::RichMessage { body, .. }
        | ChatEventKind::Notice { body }
        | ChatEventKind::System { body } => ChatTimelineBody {
            text: body.clone(),
            is_action: false,
            pending_acceptance: is_omenchat_local_echo_event(event),
            mentions_local_user: presentation.mentions_local_user,
            reply,
            upload: None,
            resend: match &event.kind {
                ChatEventKind::Message { body } => local_echo_resend(session, event, body, false),
                _ => None,
            },
        },
        ChatEventKind::Upload {
            resource_id,
            filename,
            bytes,
        } => ChatTimelineBody {
            text: format!("uploaded {} ({})", filename, human_bytes(*bytes)),
            is_action: false,
            pending_acceptance: false,
            mentions_local_user: presentation.mentions_local_user,
            reply,
            upload: Some(ChatTimelineUpload {
                session_id: session.session_id,
                resource_id: resource_id.clone(),
            }),
            resend: None,
        },
    }
}

pub(in crate::desktop) fn chat_timeline_body_text(body: &ChatTimelineBody) -> String {
    let mut text = body.text.clone();
    if body.mentions_local_user {
        text.push_str("  [mentioned you]");
    }
    if body.pending_acceptance {
        text.push_str("  [queued · awaiting server acceptance]");
    }
    text
}

pub(in crate::desktop) fn local_echo_resend(
    session: &ChatSessionView,
    event: &ChatEvent,
    body: &str,
    action: bool,
) -> Option<ChatTimelineResend> {
    if !is_omenchat_local_echo_event(event) {
        return None;
    }
    let now = current_epoch_ms() / 1_000;
    if event.at_unix > 0
        && (now as i64).saturating_sub(event.at_unix) < OMENCHAT_LOCAL_ECHO_RESEND_SECS
    {
        return None;
    }
    Some(ChatTimelineResend {
        session_id: session.session_id,
        room_id: event.room_id,
        event_id: event.event_id,
        body: body.to_owned(),
        action,
    })
}

pub(in crate::desktop) fn chat_event_time_label(at_unix: i64) -> String {
    if at_unix <= 0 {
        String::new()
    } else {
        format_epoch_secs(at_unix as f64)
    }
}

pub(in crate::desktop) fn chat_timeline_groups(
    session: &ChatSessionView,
) -> Vec<ChatTimelineGroup> {
    chat_timeline_groups_for_local_user(session, None)
}

pub(in crate::desktop) fn chat_timeline_groups_for_local_user(
    session: &ChatSessionView,
    local_user_id: Option<u32>,
) -> Vec<ChatTimelineGroup> {
    let mut groups: Vec<ChatTimelineGroup> = Vec::new();
    for event in session
        .events
        .iter()
        .filter(|event| event.room_id == session.active_room.room_id)
    {
        let actor_key = chat_event_actor_key(session, event);
        let body = chat_event_body(session, event, local_user_id);
        if let Some(last) = groups.last_mut() {
            if last.actor_key == actor_key
                && chat_events_fit_same_group(last.last_at_unix, event.at_unix)
            {
                last.bodies.push(body);
                last.last_at_unix = event.at_unix;
                continue;
            }
        }
        groups.push(ChatTimelineGroup {
            actor_key,
            actor: chat_event_actor_label(session, event),
            at_unix: event.at_unix,
            last_at_unix: event.at_unix,
            bodies: vec![body],
        });
    }
    groups
}

pub(in crate::desktop) fn chat_events_fit_same_group(
    previous_at_unix: i64,
    next_at_unix: i64,
) -> bool {
    if previous_at_unix <= 0 || next_at_unix <= 0 {
        return true;
    }
    next_at_unix.saturating_sub(previous_at_unix) <= OMENCHAT_MESSAGE_GROUP_GAP_SECS
}

#[cfg(all(test, feature = "chat-client"))]
mod tests {
    use super::*;
    use crate::chat::{ChatMessageMetadata, ChatRoomSummary, ChatServerSummary, ChatUserSummary};

    fn timeline_session(active_room_id: RoomId, events: Vec<ChatEvent>) -> ChatSessionView {
        ChatSessionView {
            session_id: 1,
            server: ChatServerSummary {
                server_id: "server-a".into(),
                destination: "abcd".into(),
                display_name: "Server A".into(),
            },
            rooms: Vec::new(),
            active_room: ChatRoomSummary {
                server_id: "server-a".into(),
                room_id: active_room_id,
                name: if active_room_id == 1 { "lobby" } else { "help" }.into(),
                topic: None,
                unread: 0,
                joined: true,
            },
            users: vec![ChatUserSummary {
                server_id: "server-a".into(),
                user_id: 7,
                display_name: "Alice".into(),
                role_bits: 0,
                status_bits: 0,
                lxmf_available: true,
            }],
            events,
            status: String::new(),
        }
    }

    fn message(room_id: RoomId, event_id: u64, at_unix: i64, body: &str) -> ChatEvent {
        ChatEvent {
            server_id: "server-a".into(),
            room_id,
            event_id,
            actor_user_id: Some(7),
            actor_display_name: None,
            at_unix,
            kind: ChatEventKind::Message { body: body.into() },
        }
    }

    #[test]
    fn omenchat_timeline_renders_actions_as_separate_action_lines() {
        let session = timeline_session(
            1,
            vec![
                message(1, 1, 0, "hello"),
                ChatEvent {
                    server_id: "server-a".into(),
                    room_id: 1,
                    event_id: 2,
                    actor_user_id: Some(7),
                    actor_display_name: None,
                    at_unix: 1,
                    kind: ChatEventKind::Action {
                        body: "waves".into(),
                    },
                },
            ],
        );

        let groups = chat_timeline_groups(&session);
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].bodies[0].text, "hello");
        assert!(!groups[0].bodies[0].is_action);
        assert_eq!(groups[1].bodies[0].text, "* Alice waves");
        assert!(groups[1].bodies[0].is_action);
    }

    #[test]
    fn omenchat_timeline_marks_only_unacknowledged_local_echoes_pending() {
        let mut pending = message(1, u64::MAX - 1, 1_700_000_000, "awaiting ack");
        pending.actor_user_id = None;
        pending.actor_display_name = Some("You".into());
        let session = timeline_session(1, vec![pending, message(1, 42, 1_700_000_001, "accepted")]);

        let groups = chat_timeline_groups(&session);
        let bodies = groups
            .iter()
            .flat_map(|group| group.bodies.iter())
            .collect::<Vec<_>>();
        assert_eq!(bodies.len(), 2);
        assert!(bodies[0].pending_acceptance);
        assert!(!bodies[1].pending_acceptance);
        assert_eq!(
            chat_timeline_body_text(bodies[0]),
            "awaiting ack  [queued · awaiting server acceptance]"
        );
        assert_eq!(chat_timeline_body_text(bodies[1]), "accepted");
    }

    #[test]
    fn omenchat_timeline_group_preserves_first_event_timestamp() {
        let session = timeline_session(
            1,
            vec![
                message(1, 1, 1_700_000_000, "first"),
                message(1, 2, 1_700_000_060, "second"),
            ],
        );

        let groups = chat_timeline_groups(&session);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].at_unix, 1_700_000_000);
        assert_eq!(
            chat_event_time_label(groups[0].at_unix),
            "2023-11-14 22:13:20 UTC"
        );
        assert_eq!(groups[0].bodies.len(), 2);
    }

    #[test]
    fn omenchat_timeline_splits_same_actor_after_group_gap() {
        let session = timeline_session(
            1,
            vec![
                message(1, 1, 1_700_000_000, "first"),
                message(1, 2, 1_700_000_240, "same stack"),
                message(1, 3, 1_700_000_601, "new stack"),
            ],
        );

        let groups = chat_timeline_groups(&session);
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].bodies.len(), 2);
        assert_eq!(groups[1].bodies.len(), 1);
    }

    #[test]
    fn omenchat_timeline_only_renders_active_room_events() {
        let session = timeline_session(
            2,
            vec![
                message(1, 1, 1, "lobby only"),
                message(2, 2, 2, "help visible"),
            ],
        );

        let groups = chat_timeline_groups(&session);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].bodies[0].text, "help visible");
    }

    #[test]
    fn rich_timeline_uses_retained_reply_and_authoritative_mention() {
        let original = message(1, 10, 1, "original");
        let rich = ChatEvent {
            server_id: "server-a".into(),
            room_id: 1,
            event_id: 11,
            actor_user_id: Some(8),
            actor_display_name: Some("Bob".into()),
            at_unix: 2,
            kind: ChatEventKind::RichMessage {
                body: "reply".into(),
                metadata: ChatMessageMetadata {
                    reply_to_event_id: Some(10),
                    mentioned_user_ids: vec![7],
                },
            },
        };
        let session = timeline_session(1, vec![original, rich]);
        let groups = chat_timeline_groups_for_local_user(&session, Some(7));
        let body = &groups[1].bodies[0];
        assert!(body.mentions_local_user);
        assert_eq!(chat_timeline_body_text(body), "reply  [mentioned you]");
        assert!(matches!(
            body.reply,
            Some(ChatTimelineReply::Available {
                event_id: 10,
                ref label,
                ..
            }) if label == "↳ original"
        ));
    }

    #[test]
    fn rich_timeline_marks_pruned_original_without_network_work() {
        let rich = ChatEvent {
            server_id: "server-a".into(),
            room_id: 1,
            event_id: 11,
            actor_user_id: Some(8),
            actor_display_name: Some("Bob".into()),
            at_unix: 2,
            kind: ChatEventKind::RichMessage {
                body: "reply".into(),
                metadata: ChatMessageMetadata {
                    reply_to_event_id: Some(10),
                    mentioned_user_ids: Vec::new(),
                },
            },
        };
        let session = timeline_session(1, vec![rich]);
        let groups = chat_timeline_groups(&session);
        assert!(matches!(
            groups[0].bodies[0].reply,
            Some(ChatTimelineReply::Unavailable { event_id: 10 })
        ));
    }
}
