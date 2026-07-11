use super::*;
use crate::app::App;
use crate::chat::store::ChatStore;
use crate::chat::{ChatEvent, ChatEventKind, OmenChatDescriptor};
use iced::widget::scrollable::RelativeOffset;

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

fn open_mock_session(desktop: &mut DesktopApp) -> ChatSessionId {
    desktop.open_omenchat_status_session(
        OmenChatDescriptor {
            server_destination: "mockchatdestination".into(),
            display_name: Some("Mock OMENchat".into()),
            rooms_hint: vec!["lobby".into()],
            local_display_name: Some("tester".into()),
            ..OmenChatDescriptor::default()
        },
        "connected".into(),
    )
}

fn cached_event() -> ChatEvent {
    ChatEvent {
        server_id: "mockchatdestination".into(),
        room_id: 1,
        event_id: 1,
        actor_user_id: None,
        actor_display_name: Some("Alice".into()),
        at_unix: 1,
        kind: ChatEventKind::Message {
            body: "cached".into(),
        },
    }
}

fn append_cached_event(desktop: &mut DesktopApp) {
    desktop
        .omenchat
        .chat_store
        .as_mut()
        .expect("chat store")
        .append_events(vec![cached_event()])
        .expect("cached event");
}

#[test]
fn cached_omenchat_room_restore_preserves_saved_scroll_offset() {
    let mut desktop = desktop_with_temp_root("omenbrowser-rs-desktop-omenchat-preserve-scroll");
    let session_id = open_mock_session(&mut desktop);
    let saved_offset = RelativeOffset { x: 0.0, y: 0.35 };
    desktop
        .omenchat
        .chat_scroll_offsets
        .insert((session_id, 1), saved_offset);
    append_cached_event(&mut desktop);

    assert_eq!(desktop.restore_cached_omenchat_room_history(session_id), 1);

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
fn cached_omenchat_room_restore_defaults_new_room_to_bottom() {
    let mut desktop = desktop_with_temp_root("omenbrowser-rs-desktop-omenchat-default-scroll");
    let session_id = open_mock_session(&mut desktop);
    desktop
        .omenchat
        .chat_scroll_offsets
        .remove(&(session_id, 1));
    append_cached_event(&mut desktop);

    assert_eq!(desktop.restore_cached_omenchat_room_history(session_id), 1);

    assert_eq!(
        desktop
            .omenchat
            .chat_scroll_offsets
            .get(&(session_id, 1))
            .copied(),
        Some(RelativeOffset { x: 0.0, y: 1.0 })
    );
}

#[test]
fn cached_omenchat_room_restore_schedules_visible_scroll_retry() {
    let mut desktop = desktop_with_temp_root("omenbrowser-rs-desktop-omenchat-scroll-retry");
    let session_id = open_mock_session(&mut desktop);
    desktop.ensure_pane_for_omenchat(session_id);
    desktop.workspace.restore_workspace_scrolls_pending = false;
    desktop.workspace.restore_workspace_scrolls_remaining = 0;
    append_cached_event(&mut desktop);

    assert_eq!(desktop.restore_cached_omenchat_room_history(session_id), 1);

    assert!(desktop.workspace.restore_workspace_scrolls_pending);
    assert!(desktop.workspace.restore_workspace_scrolls_remaining >= 5);
    assert_eq!(
        desktop
            .omenchat
            .chat_scroll_offsets
            .get(&(session_id, 1))
            .copied(),
        Some(RelativeOffset { x: 0.0, y: 1.0 })
    );
    assert!(desktop
        .omenchat
        .chat_scroll_bottom_locks
        .contains(&(session_id, 1)));
}

#[test]
fn omenchat_history_prepended_event_persists_room_history() {
    let root = std::env::temp_dir().join(format!(
        "omenbrowser-rs-desktop-omenchat-history-persist-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    let paths = crate::config::AppPaths::from_root(root);
    paths.ensure().expect("paths");
    let app = App::new(crate::config::AppConfig {
        paths: paths.clone(),
        settings: crate::storage::settings::AppSettings::default(),
    });
    let mut desktop = DesktopApp::new(app);
    let server_id = "00112233445566778899aabbccddeeff";
    let session_id = desktop.open_omenchat_status_session(
        OmenChatDescriptor {
            server_destination: server_id.into(),
            display_name: Some("Test OMENchat".into()),
            rooms_hint: vec!["lobby".into()],
            local_display_name: Some("tester".into()),
            ..OmenChatDescriptor::default()
        },
        "connected".into(),
    );
    let event = ChatEvent {
        server_id: server_id.into(),
        room_id: 1,
        event_id: 42,
        actor_user_id: Some(2),
        actor_display_name: Some("Peer".into()),
        at_unix: 1,
        kind: ChatEventKind::Message {
            body: "persisted history".into(),
        },
    };
    desktop
        .omenchat
        .chat_client
        .prepend_history_events(session_id, vec![event.clone()]);

    desktop.apply_omenchat_client_events_status(&[ChatClientEvent::HistoryPrepended {
        session_id,
        events: vec![event],
    }]);

    let store = desktop.omenchat.chat_store.as_ref().expect("store");
    let events = store
        .latest_events(&server_id.into(), 1, 10)
        .expect("latest events");
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].event_id, 42);
    assert_eq!(events[0].actor_display_name.as_deref(), Some("Peer"));

    let app = App::new(crate::config::AppConfig {
        paths,
        settings: desktop.app.settings.clone(),
    });
    let restored = DesktopApp::new(app);
    let session = restored
        .omenchat
        .chat_client
        .sessions()
        .iter()
        .find(|session| session.server.server_id == server_id)
        .expect("restored session");
    assert!(session.events.iter().any(|event| {
        event.event_id == 42
            && matches!(&event.kind, ChatEventKind::Message { body } if body == "persisted history")
    }));
}
