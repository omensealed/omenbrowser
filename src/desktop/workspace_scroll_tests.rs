use super::*;
use crate::app::App;
use crate::chat::ChatSessionId;
use crate::desktop::{
    omenchat_scroll_id, ConversationMessage, OmenChatMessage, ShellMessage, WorkspacePaneMessage,
};
use iced::widget::pane_grid;

fn desktop_with_temp_root(name: &str) -> DesktopApp {
    let root = std::env::temp_dir().join(format!("{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let paths = crate::config::AppPaths::from_root(root);
    paths.ensure().expect("paths");
    DesktopApp::new(App::new(crate::config::AppConfig {
        paths,
        settings: crate::storage::settings::AppSettings::default(),
    }))
}

fn incoming_message(content: &str) -> crate::messaging::MessageSummary {
    crate::messaging::MessageSummary {
        peer_hash: "peer".into(),
        peer_label: "Peer".into(),
        title: "hello".into(),
        content: content.into(),
        timestamp: 1.0,
        transport_method: crate::messaging::TransportMethod::Direct,
        delivered: true,
        failed: false,
        incoming: true,
        unread: true,
        message_id: Some("message-1".into()),
        fields: Default::default(),
        attachments: Vec::new(),
    }
}

fn push_conversation_message(desktop: &mut DesktopApp, conversation_id: u64, content: &str) {
    desktop
        .app
        .workspace
        .conversations
        .iter_mut()
        .find(|conversation| conversation.id == conversation_id)
        .expect("conversation")
        .push_message(incoming_message(content));
}

fn open_test_omenchat_session(desktop: &mut DesktopApp) -> ChatSessionId {
    desktop.open_omenchat_status_session(
        crate::chat::OmenChatDescriptor {
            server_destination: "00112233445566778899aabbccddeeff".into(),
            display_name: Some("Test OMENchat".into()),
            rooms_hint: vec!["lobby".into()],
            local_display_name: Some("tester".into()),
            ..crate::chat::OmenChatDescriptor::default()
        },
        "connected".into(),
    )
}

fn push_omenchat_message(desktop: &mut DesktopApp, session_id: ChatSessionId, body: &str) {
    if let Some(session) = desktop.omenchat.chat_client.session_mut(session_id) {
        session.events.push(crate::chat::ChatEvent {
            server_id: "00112233445566778899aabbccddeeff".into(),
            room_id: 1,
            event_id: 1,
            actor_user_id: Some(2),
            actor_display_name: Some("Peer".into()),
            at_unix: 1,
            kind: crate::chat::ChatEventKind::Message { body: body.into() },
        });
    }
}

#[test]
fn omenchat_scroll_ids_are_room_specific() {
    assert_ne!(omenchat_scroll_id(7, 1), omenchat_scroll_id(7, 2));
    assert_ne!(omenchat_scroll_id(7, 1), omenchat_scroll_id(8, 1));
}

#[test]
fn resizing_workspace_pane_preserves_visible_chat_scrollback_position() {
    let mut desktop = desktop_with_temp_root("omenbrowser-rs-desktop-resize-scroll-bottom");
    let conversation_id = desktop.app.active_conversation().id;
    desktop.ensure_pane_for_active_conversation();
    desktop
        .conversation
        .scroll_offsets
        .insert(conversation_id, RelativeOffset { x: 0.0, y: 0.38 });
    desktop.workspace.restore_workspace_scrolls_pending = false;
    desktop.workspace.restore_workspace_scrolls_remaining = 0;
    desktop
        .workspace
        .restore_workspace_scroll_locks_release_pending = false;
    desktop.conversation.scroll_restore_locks.clear();
    let split = *desktop
        .workspace
        .workspace_panes
        .layout()
        .splits()
        .next()
        .expect("conversation split");

    let _ = desktop.update(Message::WorkspacePane(WorkspacePaneMessage::Resized(
        pane_grid::ResizeEvent { split, ratio: 0.42 },
    )));

    assert_eq!(
        desktop.conversation.scroll_offsets.get(&conversation_id),
        Some(&RelativeOffset { x: 0.0, y: 0.38 })
    );
}

#[test]
fn resizing_workspace_pane_keeps_bottom_anchored_chat_at_bottom() {
    let mut desktop = desktop_with_temp_root("omenbrowser-rs-desktop-resize-scroll-bottom-anchor");
    let conversation_id = desktop.app.active_conversation().id;
    desktop.ensure_pane_for_active_conversation();
    desktop
        .conversation
        .scroll_offsets
        .insert(conversation_id, RelativeOffset { x: 0.0, y: 1.0 });
    desktop.workspace.restore_workspace_scrolls_pending = false;
    desktop.workspace.restore_workspace_scrolls_remaining = 0;
    desktop
        .workspace
        .restore_workspace_scroll_locks_release_pending = false;
    desktop.conversation.scroll_restore_locks.clear();
    let split = *desktop
        .workspace
        .workspace_panes
        .layout()
        .splits()
        .next()
        .expect("conversation split");

    let _ = desktop.update(Message::WorkspacePane(WorkspacePaneMessage::Resized(
        pane_grid::ResizeEvent { split, ratio: 0.42 },
    )));
    let _ = desktop.update(Message::Conversation(ConversationMessage::Scrolled {
        conversation_id,
        offset: RelativeOffset { x: 0.0, y: 0.0 },
    }));

    assert_eq!(
        desktop.conversation.scroll_offsets.get(&conversation_id),
        Some(&RelativeOffset { x: 0.0, y: 1.0 })
    );
}

#[test]
fn new_conversation_messages_do_not_force_scroll_when_reading_history() {
    let mut desktop = desktop_with_temp_root("omenbrowser-rs-desktop-conversation-follow-mode");
    let conversation_id = desktop.app.active_conversation().id;
    desktop.ensure_pane_for_active_conversation();
    desktop
        .conversation
        .message_counts
        .insert(conversation_id, 0);
    desktop
        .conversation
        .scroll_offsets
        .insert(conversation_id, RelativeOffset { x: 0.0, y: 0.42 });

    push_conversation_message(&mut desktop, conversation_id, "while reading history");
    let _ = desktop.snap_conversations_with_new_messages_to_bottom();

    assert_eq!(
        desktop.conversation.scroll_offsets.get(&conversation_id),
        Some(&RelativeOffset { x: 0.0, y: 0.42 })
    );
    assert!(desktop.conversation_is_viewing_history(conversation_id));
}

#[test]
fn new_conversation_messages_follow_bottom_when_already_at_present() {
    let mut desktop = desktop_with_temp_root("omenbrowser-rs-desktop-conversation-follow-bottom");
    let conversation_id = desktop.app.active_conversation().id;
    desktop.ensure_pane_for_active_conversation();
    desktop
        .conversation
        .message_counts
        .insert(conversation_id, 0);
    desktop
        .conversation
        .scroll_offsets
        .insert(conversation_id, RelativeOffset { x: 0.0, y: 1.0 });

    push_conversation_message(&mut desktop, conversation_id, "at present");
    let _ = desktop.snap_conversations_with_new_messages_to_bottom();

    assert_eq!(
        desktop.conversation.scroll_offsets.get(&conversation_id),
        Some(&RelativeOffset { x: 0.0, y: 1.0 })
    );
    assert!(!desktop.conversation_is_viewing_history(conversation_id));
}

#[test]
fn handled_conversation_messages_reconcile_follow_bottom_without_a_tick() {
    let mut desktop = desktop_with_temp_root("omenbrowser-rs-desktop-conversation-event-snap");
    let conversation_id = desktop.app.active_conversation().id;
    desktop.ensure_pane_for_active_conversation();
    desktop
        .conversation
        .message_counts
        .insert(conversation_id, 0);
    push_conversation_message(&mut desktop, conversation_id, "event boundary");

    let _ = desktop.update(Message::Conversation(ConversationMessage::BodyChanged(
        String::new(),
    )));

    assert_eq!(
        desktop.conversation.message_counts.get(&conversation_id),
        Some(&1)
    );
}

#[test]
fn conversation_history_notice_requires_meaningful_scrollback() {
    let mut desktop = desktop_with_temp_root("omenbrowser-rs-desktop-conversation-history-notice");
    let conversation_id = desktop.app.active_conversation().id;

    desktop
        .conversation
        .scroll_offsets
        .insert(conversation_id, RelativeOffset { x: 0.0, y: 0.93 });
    assert!(!desktop.conversation_is_viewing_history(conversation_id));

    desktop
        .conversation
        .scroll_offsets
        .insert(conversation_id, RelativeOffset { x: 0.0, y: 0.50 });
    assert!(desktop.conversation_is_viewing_history(conversation_id));
}

#[test]
fn restored_conversation_pane_starts_at_bottom() {
    let mut desktop = desktop_with_temp_root("omenbrowser-rs-desktop-restore-conversation-bottom");
    let conversation_id = desktop.app.active_conversation().id;
    desktop
        .conversation
        .scroll_offsets
        .insert(conversation_id, RelativeOffset { x: 0.0, y: 0.25 });

    let _ = desktop.restore_desktop_pane(DesktopPane::Conversation(conversation_id));

    assert_eq!(
        desktop.conversation.scroll_offsets.get(&conversation_id),
        Some(&RelativeOffset { x: 0.0, y: 1.0 })
    );
}

#[test]
fn programmatic_conversation_scroll_restore_does_not_persist_top_callback() {
    let mut desktop = desktop_with_temp_root("omenbrowser-rs-desktop-conversation-scroll-lock");
    let conversation_id = desktop.app.active_conversation().id;
    let saved_offset = RelativeOffset { x: 0.0, y: 0.72 };
    desktop
        .conversation
        .scroll_offsets
        .insert(conversation_id, saved_offset);

    desktop.schedule_visible_workspace_scroll_restore(2);
    let _ = desktop.update(Message::Conversation(ConversationMessage::Scrolled {
        conversation_id,
        offset: RelativeOffset { x: 0.0, y: 0.0 },
    }));

    assert_eq!(
        desktop.conversation.scroll_offsets.get(&conversation_id),
        Some(&saved_offset)
    );
}

#[test]
fn scroll_settling_advances_only_from_its_conditional_subscription() {
    let mut desktop = desktop_with_temp_root("omenbrowser-rs-desktop-scroll-deadline");
    desktop.schedule_visible_workspace_bottom_anchor(2);
    let initial_restore_ticks = desktop.workspace.restore_workspace_scrolls_remaining;

    let _ = desktop.update_workspace_scroll_tick();
    assert_eq!(
        desktop.workspace.restore_workspace_scrolls_remaining,
        initial_restore_ticks - 1
    );
    assert_eq!(desktop.workspace.pending_workspace_bottom_anchor_ticks, 1);

    let _ = desktop.update_workspace_scroll_tick();
    assert_eq!(desktop.workspace.pending_workspace_bottom_anchor_ticks, 0);
}

#[test]
fn hidden_workspace_conversation_scroll_callback_does_not_persist_top_offset() {
    let mut desktop = desktop_with_temp_root("omenbrowser-rs-desktop-conversation-hidden-scroll");
    let conversation_id = desktop.app.active_conversation().id;
    desktop.ensure_pane_for_active_conversation();
    let saved_offset = RelativeOffset { x: 0.0, y: 0.82 };
    desktop
        .conversation
        .scroll_offsets
        .insert(conversation_id, saved_offset);

    let _ = desktop.update(Message::Shell(ShellMessage::SwitchSection(
        WorkspaceSection::Logs,
    )));
    let _ = desktop.update(Message::Conversation(ConversationMessage::Scrolled {
        conversation_id,
        offset: RelativeOffset { x: 0.0, y: 0.0 },
    }));

    assert_eq!(
        desktop.conversation.scroll_offsets.get(&conversation_id),
        Some(&saved_offset)
    );
}

#[test]
fn restored_omenchat_pane_starts_at_bottom() {
    let mut desktop = desktop_with_temp_root("omenbrowser-rs-desktop-restore-omenchat-bottom");
    let session_id = open_test_omenchat_session(&mut desktop);
    desktop
        .omenchat
        .chat_scroll_offsets
        .insert((session_id, 1), RelativeOffset { x: 0.0, y: 0.25 });

    let _ = desktop.restore_desktop_pane(DesktopPane::OmenChat(session_id));

    assert_eq!(
        desktop.omenchat.chat_scroll_offsets.get(&(session_id, 1)),
        Some(&RelativeOffset { x: 0.0, y: 1.0 })
    );
}

#[test]
fn newly_opened_omenchat_pane_rejects_initial_top_scroll_callback() {
    let mut desktop = desktop_with_temp_root("omenbrowser-rs-desktop-new-omenchat-bottom-lock");
    let session_id = open_test_omenchat_session(&mut desktop);
    desktop.workspace.restore_workspace_scrolls_pending = false;
    desktop.workspace.restore_workspace_scrolls_remaining = 0;
    desktop
        .workspace
        .restore_workspace_scroll_locks_release_pending = false;
    desktop.omenchat.chat_scroll_bottom_locks.clear();

    desktop.ensure_pane_for_omenchat(session_id);
    let _ = desktop.update(Message::OmenChat(OmenChatMessage::Scrolled {
        session_id,
        room_id: 1,
        offset: RelativeOffset { x: 0.0, y: 0.0 },
    }));

    assert_eq!(
        desktop.omenchat.chat_scroll_offsets.get(&(session_id, 1)),
        Some(&RelativeOffset { x: 0.0, y: 1.0 })
    );
    assert!(desktop
        .omenchat
        .chat_scroll_bottom_locks
        .contains(&(session_id, 1)));
    assert!(desktop.workspace.restore_workspace_scrolls_remaining >= 3);
}

#[test]
fn omenchat_media_layout_change_preserves_follow_tail() {
    let mut desktop = desktop_with_temp_root("omenbrowser-rs-desktop-media-follow-tail");
    let session_id = open_test_omenchat_session(&mut desktop);
    desktop.ensure_pane_for_omenchat(session_id);
    desktop.workspace.restore_workspace_scrolls_pending = false;
    desktop.workspace.restore_workspace_scrolls_remaining = 0;
    desktop
        .workspace
        .restore_workspace_scroll_locks_release_pending = false;
    desktop.workspace.pending_workspace_bottom_anchor_ticks = 0;
    desktop.omenchat.chat_scroll_bottom_locks.clear();
    desktop
        .omenchat
        .chat_scroll_offsets
        .insert((session_id, 1), RelativeOffset { x: 0.0, y: 1.0 });

    let _ = desktop.update_omenchat_media_loaded(
        "https://example.invalid/attachment.png".into(),
        Err("isolated smoke failure".into()),
    );

    assert_eq!(
        desktop.omenchat.chat_scroll_offsets.get(&(session_id, 1)),
        Some(&RelativeOffset { x: 0.0, y: 1.0 })
    );
    assert!(desktop
        .omenchat
        .chat_scroll_bottom_locks
        .contains(&(session_id, 1)));
    assert!(desktop.workspace.pending_workspace_bottom_anchor_ticks >= 3);
}

#[test]
fn omenchat_media_layout_change_does_not_interrupt_history_reading() {
    let mut desktop = desktop_with_temp_root("omenbrowser-rs-desktop-media-history-scroll");
    let session_id = open_test_omenchat_session(&mut desktop);
    desktop.ensure_pane_for_omenchat(session_id);
    desktop.workspace.restore_workspace_scrolls_pending = false;
    desktop.workspace.restore_workspace_scrolls_remaining = 0;
    desktop
        .workspace
        .restore_workspace_scroll_locks_release_pending = false;
    desktop.workspace.pending_workspace_bottom_anchor_ticks = 0;
    desktop.omenchat.chat_scroll_bottom_locks.clear();
    let history_offset = RelativeOffset { x: 0.0, y: 0.40 };
    desktop
        .omenchat
        .chat_scroll_offsets
        .insert((session_id, 1), history_offset);

    let _ = desktop.update_omenchat_media_loaded(
        "https://example.invalid/attachment.png".into(),
        Err("isolated smoke failure".into()),
    );

    assert_eq!(
        desktop.omenchat.chat_scroll_offsets.get(&(session_id, 1)),
        Some(&history_offset)
    );
    assert!(!desktop
        .omenchat
        .chat_scroll_bottom_locks
        .contains(&(session_id, 1)));
    assert_eq!(desktop.workspace.pending_workspace_bottom_anchor_ticks, 0);
}

#[test]
fn omenchat_upload_picker_cancel_does_not_touch_scroll_restore() {
    let mut desktop = desktop_with_temp_root("omenbrowser-rs-desktop-omenchat-upload-no-scroll");
    let session_id = open_test_omenchat_session(&mut desktop);
    desktop.ensure_pane_for_omenchat(session_id);
    let saved_offset = RelativeOffset { x: 0.0, y: 0.42 };
    desktop
        .omenchat
        .chat_scroll_offsets
        .insert((session_id, 1), saved_offset);
    desktop.workspace.restore_workspace_scrolls_pending = false;
    desktop
        .workspace
        .restore_workspace_scroll_locks_release_pending = false;
    desktop.omenchat.chat_scroll_bottom_locks.clear();

    let _ = desktop.update(Message::OmenChatMediaCompletion(Box::new(
        crate::desktop::OmenChatMediaCompletionMessage::UploadPicked {
            session_id,
            result: Ok(None),
        },
    )));

    assert!(!desktop.workspace.restore_workspace_scrolls_pending);
    assert!(!desktop
        .omenchat
        .chat_scroll_bottom_locks
        .contains(&(session_id, 1)));
    assert_eq!(
        desktop
            .omenchat
            .chat_scroll_offsets
            .get(&(session_id, 1))
            .copied(),
        Some(saved_offset)
    );
}

#[test]
fn new_omenchat_events_do_not_force_scroll_when_reading_history() {
    let mut desktop = desktop_with_temp_root("omenbrowser-rs-desktop-omenchat-follow-mode");
    let session_id = open_test_omenchat_session(&mut desktop);
    let _ = desktop.restore_desktop_pane(DesktopPane::OmenChat(session_id));
    desktop
        .omenchat
        .chat_event_counts
        .insert((session_id, 1), 0);
    desktop
        .omenchat
        .chat_scroll_offsets
        .insert((session_id, 1), RelativeOffset { x: 0.0, y: 0.36 });

    push_omenchat_message(&mut desktop, session_id, "while reading history");
    let _ = desktop.snap_omenchat_with_new_events_to_bottom();

    assert_eq!(
        desktop.omenchat.chat_scroll_offsets.get(&(session_id, 1)),
        Some(&RelativeOffset { x: 0.0, y: 0.36 })
    );
    assert!(desktop.omenchat_is_viewing_history(session_id, 1));
}

#[test]
fn new_omenchat_events_follow_bottom_when_already_at_present() {
    let mut desktop = desktop_with_temp_root("omenbrowser-rs-desktop-omenchat-follow-bottom");
    let session_id = open_test_omenchat_session(&mut desktop);
    let _ = desktop.restore_desktop_pane(DesktopPane::OmenChat(session_id));
    desktop
        .omenchat
        .chat_event_counts
        .insert((session_id, 1), 0);
    desktop
        .omenchat
        .chat_scroll_offsets
        .insert((session_id, 1), RelativeOffset { x: 0.0, y: 1.0 });

    push_omenchat_message(&mut desktop, session_id, "at present");
    let _ = desktop.snap_omenchat_with_new_events_to_bottom();

    assert_eq!(
        desktop.omenchat.chat_scroll_offsets.get(&(session_id, 1)),
        Some(&RelativeOffset { x: 0.0, y: 1.0 })
    );
    assert!(!desktop.omenchat_is_viewing_history(session_id, 1));
}

#[test]
fn handled_omenchat_messages_reconcile_follow_bottom_without_a_tick() {
    let mut desktop = desktop_with_temp_root("omenbrowser-rs-desktop-omenchat-event-snap");
    let session_id = open_test_omenchat_session(&mut desktop);
    let _ = desktop.restore_desktop_pane(DesktopPane::OmenChat(session_id));
    desktop
        .omenchat
        .chat_event_counts
        .insert((session_id, 1), 0);
    push_omenchat_message(&mut desktop, session_id, "event boundary");

    let _ = desktop.update(Message::OmenChat(OmenChatMessage::DraftChanged {
        session_id,
        value: String::new(),
    }));

    assert_eq!(
        desktop.omenchat.chat_event_counts.get(&(session_id, 1)),
        Some(&1)
    );
}

#[test]
fn omenchat_history_notice_requires_meaningful_scrollback() {
    let mut desktop = desktop_with_temp_root("omenbrowser-rs-desktop-omenchat-history-notice");
    let session_id = open_test_omenchat_session(&mut desktop);

    desktop
        .omenchat
        .chat_scroll_offsets
        .insert((session_id, 1), RelativeOffset { x: 0.0, y: 0.93 });
    assert!(!desktop.omenchat_is_viewing_history(session_id, 1));

    desktop
        .omenchat
        .chat_scroll_offsets
        .insert((session_id, 1), RelativeOffset { x: 0.0, y: 0.50 });
    assert!(desktop.omenchat_is_viewing_history(session_id, 1));
}

#[test]
fn programmatic_omenchat_scroll_restore_does_not_persist_top_callback() {
    let mut desktop = desktop_with_temp_root("omenbrowser-rs-desktop-omenchat-scroll-lock");
    let session_id = open_test_omenchat_session(&mut desktop);
    desktop.ensure_pane_for_omenchat(session_id);
    let saved_offset = RelativeOffset { x: 0.0, y: 0.64 };
    desktop
        .omenchat
        .chat_scroll_offsets
        .insert((session_id, 1), saved_offset);

    desktop.schedule_visible_workspace_scroll_restore(2);
    let _ = desktop.update(Message::OmenChat(OmenChatMessage::Scrolled {
        session_id,
        room_id: 1,
        offset: RelativeOffset { x: 0.0, y: 0.0 },
    }));

    assert_eq!(
        desktop
            .omenchat
            .chat_scroll_offsets
            .get(&(session_id, 1))
            .copied(),
        Some(saved_offset)
    );
}

#[test]
fn hidden_workspace_omenchat_scroll_callback_does_not_persist_top_offset() {
    let mut desktop = desktop_with_temp_root("omenbrowser-rs-desktop-omenchat-hidden-scroll");
    let session_id = open_test_omenchat_session(&mut desktop);
    desktop.ensure_pane_for_omenchat(session_id);
    let saved_offset = RelativeOffset { x: 0.0, y: 0.58 };
    desktop
        .omenchat
        .chat_scroll_offsets
        .insert((session_id, 1), saved_offset);

    let _ = desktop.update(Message::Shell(ShellMessage::SwitchSection(
        WorkspaceSection::Logs,
    )));
    let _ = desktop.update(Message::OmenChat(OmenChatMessage::Scrolled {
        session_id,
        room_id: 1,
        offset: RelativeOffset { x: 0.0, y: 0.0 },
    }));

    assert_eq!(
        desktop
            .omenchat
            .chat_scroll_offsets
            .get(&(session_id, 1))
            .copied(),
        Some(saved_offset)
    );
}
