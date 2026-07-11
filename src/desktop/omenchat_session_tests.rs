use super::*;
use crate::app::App;
use crate::chat::store::ChatStore;
use crate::chat::OmenChatDescriptor;
use crate::desktop::{is_pending_omenchat_destination, DesktopPane, Message};

const FIXTURE_CHAT_SERVER_HASH: &str = "00112233445566778899aabbccddeeff";
const FIXTURE_OMENCHAT_HASH: &str = "ffeeddccbbaa99887766554433221100";

fn desktop_with_paths(name: &str) -> (DesktopApp, crate::config::AppPaths) {
    let root = std::env::temp_dir().join(format!("{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let paths = crate::config::AppPaths::from_root(root);
    paths.ensure().expect("paths");
    let app = App::new(crate::config::AppConfig {
        paths: paths.clone(),
        settings: crate::storage::settings::AppSettings::default(),
    });
    (DesktopApp::new(app), paths)
}

fn test_descriptor(destination: &str, display_name: &str) -> OmenChatDescriptor {
    OmenChatDescriptor {
        server_destination: destination.into(),
        display_name: Some(display_name.into()),
        rooms_hint: vec!["lobby".into()],
        local_display_name: Some("tester".into()),
        ..OmenChatDescriptor::default()
    }
}

#[test]
fn close_omenchat_session_deletes_plugin_store_rows() {
    let (mut desktop, paths) = desktop_with_paths("omenbrowser-rs-desktop-delete-omenchat-store");
    let session_id = desktop.open_omenchat_status_session(
        test_descriptor("mockchatdestination", "Mock OMENchat"),
        "connected".into(),
    );
    let server_id = desktop
        .omenchat
        .chat_client
        .session(session_id)
        .expect("session")
        .server
        .server_id
        .clone();
    assert!(desktop
        .omenchat
        .chat_store
        .as_ref()
        .expect("store")
        .saved_servers()
        .expect("servers")
        .iter()
        .any(|server| server.server_id == server_id));

    desktop.close_omenchat_session(session_id);
    desktop.remove_workspace_panes_for_missing_targets(None, None);
    desktop.persist_workspace_panes("workspace panes");

    assert!(desktop
        .omenchat
        .chat_store
        .as_ref()
        .expect("store")
        .saved_servers()
        .expect("servers")
        .is_empty());

    let app = App::new(crate::config::AppConfig {
        paths,
        settings: desktop.app.settings.clone(),
    });
    let restored = DesktopApp::new(app);
    assert!(restored.omenchat.chat_client.sessions().is_empty());
    assert!(!restored
        .workspace
        .workspace_panes
        .iter()
        .any(|(_, pane)| matches!(pane, DesktopPane::OmenChat(_))));
}

#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
#[tokio::test]
async fn close_omenchat_session_clears_live_transport_and_retry_state() {
    let (mut desktop, _) = desktop_with_paths("omenbrowser-rs-desktop-close-omenchat-live");
    let session_id = desktop.open_omenchat_status_session(
        test_descriptor("00112233445566778899aabbccddeeff", "Live OMENchat"),
        "connected".into(),
    );
    let link_id = [0x42; 16];
    desktop.omenchat.omenchat_live_opening.insert(session_id);
    desktop
        .omenchat
        .omenchat_live_retry_after
        .insert(session_id, 123);
    desktop
        .omenchat
        .omenchat_live_retry_count
        .insert(session_id, 2);
    desktop
        .omenchat
        .omenchat_live_reconnect_generation
        .insert(session_id, 4);
    desktop
        .omenchat
        .omenchat_link_sessions
        .insert(link_id, session_id);
    desktop.omenchat.omenchat_live_transports.insert(
        session_id,
        crate::desktop::omenchat_runtime::DesktopOmenChatTransport::new(
            link_id,
            crate::app::current_epoch_ms(),
        ),
    );

    desktop.close_omenchat_session(session_id);

    assert!(desktop.omenchat.chat_client.session(session_id).is_none());
    assert!(!desktop.omenchat.omenchat_live_opening.contains(&session_id));
    assert!(!desktop
        .omenchat
        .omenchat_live_retry_after
        .contains_key(&session_id));
    assert!(!desktop
        .omenchat
        .omenchat_live_retry_count
        .contains_key(&session_id));
    assert!(!desktop
        .omenchat
        .omenchat_live_reconnect_generation
        .contains_key(&session_id));
    assert!(!desktop
        .omenchat
        .omenchat_live_transports
        .contains_key(&session_id));
    assert!(!desktop
        .omenchat
        .omenchat_link_sessions
        .values()
        .any(|stored_session_id| *stored_session_id == session_id));
}

#[test]
fn close_omenchat_session_message_updates_session_store() {
    let (mut desktop, _) = desktop_with_paths("omenbrowser-rs-desktop-close-message");
    let session_id = desktop.open_omenchat_status_session(
        test_descriptor("mockchatdestination", "Mock OMENchat"),
        "connected".into(),
    );

    let _ = desktop.update(Message::CloseOmenChatSession(session_id));

    assert!(desktop.omenchat.chat_client.session(session_id).is_none());
    assert_eq!(desktop.app.status.task, "closed OMENchat session");
}

#[test]
fn new_chat_creates_blank_session_instead_of_restoring_existing_chat() {
    let (mut desktop, _) = desktop_with_paths("omenbrowser-rs-desktop-new-chat-blank");
    let descriptor = test_descriptor(FIXTURE_CHAT_SERVER_HASH, "Existing Server");
    let existing_id = desktop.open_omenchat_status_session(descriptor, "connected".into());
    desktop.ensure_pane_for_omenchat(existing_id);
    let existing_pane = desktop
        .find_workspace_pane(&DesktopPane::OmenChat(existing_id))
        .expect("existing pane");
    desktop.close_workspace_pane(existing_pane);

    let _ = desktop.update(Message::NewOmenChatPane);

    let sessions = desktop.omenchat.chat_client.sessions();
    assert_eq!(sessions.len(), 2);
    assert!(sessions
        .iter()
        .any(|session| session.session_id == existing_id
            && session.server.destination == FIXTURE_CHAT_SERVER_HASH));
    let blank = sessions
        .iter()
        .find(|session| session.session_id != existing_id)
        .expect("blank session");
    assert!(is_pending_omenchat_destination(&blank.server.destination));
    assert_eq!(blank.server.display_name, "New Chat");
    assert!(desktop
        .find_workspace_pane(&DesktopPane::OmenChat(blank.session_id))
        .is_some());
    assert!(desktop
        .find_workspace_pane(&DesktopPane::OmenChat(existing_id))
        .is_none());
}

#[test]
fn opening_existing_omenchat_destination_restores_without_duplicate_session() {
    let (mut desktop, _) = desktop_with_paths("omenbrowser-rs-desktop-open-existing-chat");
    let descriptor = test_descriptor(FIXTURE_CHAT_SERVER_HASH, "Existing Server");
    let existing_id = desktop.open_omenchat_status_session(descriptor, "connected".into());
    desktop.ensure_pane_for_omenchat(existing_id);
    let existing_pane = desktop
        .find_workspace_pane(&DesktopPane::OmenChat(existing_id))
        .expect("existing pane");
    desktop.close_workspace_pane(existing_pane);

    desktop.omenchat.omenchat_server_entry = format!("omenchat://{FIXTURE_CHAT_SERVER_HASH}");
    let _ = desktop.update(Message::OpenOmenChatServerEntry);

    assert_eq!(desktop.omenchat.chat_client.sessions().len(), 1);
    assert!(desktop
        .find_workspace_pane(&DesktopPane::OmenChat(existing_id))
        .is_some());
    assert!(desktop
        .app
        .status
        .task
        .contains("restored existing OMENchat session"));
}

#[cfg(feature = "mock-runtime")]
#[test]
fn opening_destination_from_blank_chat_replaces_blank_pane() {
    let (mut desktop, _) = desktop_with_paths("omenbrowser-rs-desktop-open-from-blank-chat");
    desktop.app.runtime_status.connected = false;

    let _ = desktop.update(Message::NewOmenChatPane);
    let blank_id = desktop.omenchat.chat_client.sessions()[0].session_id;
    assert!(is_pending_omenchat_destination(
        &desktop.omenchat.chat_client.sessions()[0]
            .server
            .destination
    ));

    desktop.omenchat.omenchat_server_entry = FIXTURE_CHAT_SERVER_HASH.into();
    let _ = desktop.update(Message::OpenOmenChatServerEntry);

    let sessions = desktop.omenchat.chat_client.sessions();
    assert_eq!(sessions.len(), 1);
    assert_ne!(sessions[0].session_id, blank_id);
    assert_eq!(sessions[0].server.destination, FIXTURE_CHAT_SERVER_HASH);
    assert!(!is_pending_omenchat_destination(
        &sessions[0].server.destination
    ));
    assert!(desktop
        .workspace
        .workspace_panes
        .iter()
        .any(|(_, pane)| *pane == DesktopPane::OmenChat(sessions[0].session_id)));
    assert!(desktop
        .workspace
        .workspace_panes
        .iter()
        .all(|(_, pane)| *pane != DesktopPane::OmenChat(blank_id)));
}

#[cfg(feature = "mock-runtime")]
#[test]
fn opening_different_omenchat_destinations_creates_separate_sessions() {
    let (mut desktop, _) = desktop_with_paths("omenbrowser-rs-desktop-open-multiple-chats");
    desktop.app.runtime_status.connected = false;

    desktop.omenchat.omenchat_server_entry = FIXTURE_CHAT_SERVER_HASH.into();
    let _ = desktop.update(Message::OpenOmenChatServerEntry);
    desktop.omenchat.omenchat_server_entry = FIXTURE_OMENCHAT_HASH.into();
    let _ = desktop.update(Message::OpenOmenChatServerEntry);

    let sessions = desktop.omenchat.chat_client.sessions();
    assert_eq!(sessions.len(), 2);
    let first = sessions
        .iter()
        .find(|session| session.server.destination == FIXTURE_CHAT_SERVER_HASH)
        .expect("first server session");
    let second = sessions
        .iter()
        .find(|session| session.server.destination == FIXTURE_OMENCHAT_HASH)
        .expect("second server session");
    assert_ne!(first.session_id, second.session_id);
    assert!(desktop
        .workspace
        .workspace_panes
        .iter()
        .any(|(_, pane)| *pane == DesktopPane::OmenChat(first.session_id)));
    assert!(desktop
        .workspace
        .workspace_panes
        .iter()
        .any(|(_, pane)| *pane == DesktopPane::OmenChat(second.session_id)));

    desktop.omenchat.omenchat_server_entry = format!("omenchat://{FIXTURE_CHAT_SERVER_HASH}");
    let _ = desktop.update(Message::OpenOmenChatServerEntry);

    assert_eq!(desktop.omenchat.chat_client.sessions().len(), 2);
    assert!(desktop
        .app
        .status
        .task
        .contains("restored existing OMENchat session"));
}
