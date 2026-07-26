use super::*;
use crate::app::App;
use crate::chat::store::ChatStore;
use crate::chat::OmenChatDescriptor;
use crate::desktop::{is_pending_omenchat_destination, DesktopPane, Message, OmenChatMessage};

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
    desktop.reconcile_workspace_panes_after_target_mutation(None, None);
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
    desktop.set_omenchat_connection_state(session_id, crate::chat::ChatConnectionState::Joined);

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
    assert!(!desktop
        .omenchat
        .omenchat_connection_states
        .contains_key(&session_id));
    assert!(!desktop.app.operation_history.records().any(|record| {
        record.id
            == crate::operations::OperationId::numeric(
                crate::operations::OperationDomain::OmenChatConnection,
                session_id,
            )
    }));
}

#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
#[tokio::test]
async fn omenchat_connection_state_is_bounded_by_sessions_and_join_is_event_driven() {
    let (mut desktop, _) = desktop_with_paths("omenbrowser-rs-omenchat-typed-lifecycle");
    let session_id = desktop.open_omenchat_status_session(
        test_descriptor(FIXTURE_CHAT_SERVER_HASH, "Live OMENchat"),
        "waiting".into(),
    );

    assert_eq!(
        desktop.omenchat_connection_state(session_id),
        crate::chat::ChatConnectionState::Disconnected
    );
    desktop.set_omenchat_connection_state(
        session_id.saturating_add(1_000),
        crate::chat::ChatConnectionState::Connecting,
    );
    assert_eq!(desktop.omenchat.omenchat_connection_states.len(), 1);

    desktop.app.runtime_status.connected = true;
    let _ = desktop.request_omenchat_path_task(session_id);
    assert_eq!(
        desktop.omenchat_connection_state(session_id),
        crate::chat::ChatConnectionState::Resolving
    );
    let operation = desktop
        .app
        .operation_history
        .records()
        .find(|record| {
            record.id
                == crate::operations::OperationId::numeric(
                    crate::operations::OperationDomain::OmenChatConnection,
                    session_id,
                )
        })
        .expect("resolving connection operation");
    assert_eq!(operation.state, crate::operations::OperationState::Waiting);

    let _ = desktop.register_omenchat_live_transport(
        session_id,
        crate::desktop::DesktopOmenChatTransport::new([0x73; 16], crate::app::current_epoch_ms()),
    );
    assert_eq!(
        desktop.omenchat_connection_state(session_id),
        crate::chat::ChatConnectionState::Authenticating
    );

    let room = desktop
        .omenchat
        .chat_client
        .session(session_id)
        .expect("session")
        .active_room
        .clone();
    desktop.apply_omenchat_client_events_status(&[crate::chat::ChatClientEvent::RoomJoined {
        session_id,
        room,
        users: Vec::new(),
        latest_events: Vec::new(),
    }]);
    assert_eq!(
        desktop.omenchat_connection_state(session_id),
        crate::chat::ChatConnectionState::Joined
    );
    let operation = desktop
        .app
        .operation_history
        .records()
        .find(|record| {
            record.id
                == crate::operations::OperationId::numeric(
                    crate::operations::OperationDomain::OmenChatConnection,
                    session_id,
                )
        })
        .expect("joined connection operation");
    assert_eq!(operation.state, crate::operations::OperationState::Active);
    assert!(!operation.state.claims_peer_delivery());

    desktop.apply_omenchat_client_events_status(&[crate::chat::ChatClientEvent::Error {
        session_id: Some(session_id),
        message: "room command rejected".into(),
    }]);
    assert_eq!(
        desktop.omenchat_connection_state(session_id),
        crate::chat::ChatConnectionState::Joined
    );

    desktop.set_omenchat_connection_state(
        session_id,
        crate::chat::ChatConnectionState::Authenticating,
    );
    desktop.apply_omenchat_client_events_status(&[crate::chat::ChatClientEvent::Error {
        session_id: Some(session_id),
        message: "session rejected".into(),
    }]);
    assert_eq!(
        desktop.omenchat_connection_state(session_id),
        crate::chat::ChatConnectionState::Failed { retryable: true }
    );
}

#[test]
fn close_omenchat_session_message_updates_session_store() {
    let (mut desktop, _) = desktop_with_paths("omenbrowser-rs-desktop-close-message");
    let session_id = desktop.open_omenchat_status_session(
        test_descriptor("mockchatdestination", "Mock OMENchat"),
        "connected".into(),
    );

    let _ = desktop.update(Message::OmenChat(OmenChatMessage::CloseSession(session_id)));

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

    let _ = desktop.update(Message::OmenChat(OmenChatMessage::NewPane));

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
    let _ = desktop.update(Message::OmenChat(OmenChatMessage::OpenServerEntry));

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

#[test]
fn enhanced_invitation_requires_confirmation_without_connecting_or_persisting_trust() {
    let (mut desktop, _) = desktop_with_paths("omenbrowser-rs-desktop-invitation-preview");
    let sessions_before = desktop.omenchat.chat_client.sessions().len();
    let directory_before = desktop.app.directory_state.entries.clone();
    desktop.omenchat.omenchat_server_entry =
        format!("omenchat://{FIXTURE_CHAT_SERVER_HASH}?invite=1&room=7&label=Field%20Ops");

    let _ = desktop.update(Message::OmenChat(OmenChatMessage::OpenServerEntry));

    let preview = desktop
        .omenchat
        .omenchat_invitation_preview
        .pending()
        .expect("invitation preview");
    assert_eq!(preview.invitation.room_id, Some(7));
    assert_eq!(
        preview.invitation.display_label.as_deref(),
        Some("Field Ops")
    );
    assert_eq!(
        desktop.omenchat.chat_client.sessions().len(),
        sessions_before,
        "parsing an invitation must not open a session"
    );
    assert_eq!(desktop.app.directory_state.entries, directory_before);
    assert!(desktop
        .app
        .status
        .task
        .contains("no connection has been opened"));

    let _ = desktop.update(Message::OmenChat(OmenChatMessage::CancelInvitation));
    assert!(desktop
        .omenchat
        .omenchat_invitation_preview
        .pending()
        .is_none());
    assert_eq!(
        desktop.omenchat.chat_client.sessions().len(),
        sessions_before
    );
}

#[test]
fn conflicting_invitation_identity_blocks_desktop_confirmation() {
    let (mut desktop, _) = desktop_with_paths("omenbrowser-rs-desktop-invitation-conflict");
    let mut entry = crate::directory::DirectoryEntry::new(
        FIXTURE_CHAT_SERVER_HASH,
        "Known Server",
        crate::directory::DirectoryKind::OmenChat,
    );
    entry.identity_hash = Some("11111111111111111111111111111111".into());
    desktop.app.directory_state.entries.push(entry);
    desktop.omenchat.omenchat_server_entry = format!(
        "omenchat://{FIXTURE_CHAT_SERVER_HASH}?invite=1&identity=22222222222222222222222222222222"
    );

    let _ = desktop.update(Message::OmenChat(OmenChatMessage::OpenServerEntry));
    let sessions_before = desktop.omenchat.chat_client.sessions().len();
    let _ = desktop.update(Message::OmenChat(OmenChatMessage::ConfirmInvitation));

    assert_eq!(
        desktop.omenchat.chat_client.sessions().len(),
        sessions_before
    );
    assert!(desktop
        .omenchat
        .omenchat_invitation_preview
        .pending()
        .is_some());
    assert!(desktop.app.status.task.contains("cannot be confirmed"));
}

#[test]
fn explicit_invitation_confirmation_consumes_preview_before_returning_open_task() {
    let (mut desktop, _) = desktop_with_paths("omenbrowser-rs-desktop-invitation-confirm");
    desktop.omenchat.omenchat_server_entry =
        format!("omenchat://{FIXTURE_CHAT_SERVER_HASH}?invite=1&label=Field%20Ops");
    let _ = desktop.update(Message::OmenChat(OmenChatMessage::OpenServerEntry));
    assert!(desktop
        .omenchat
        .omenchat_invitation_preview
        .pending()
        .is_some());

    let _ = desktop.update(Message::OmenChat(OmenChatMessage::ConfirmInvitation));

    assert!(desktop
        .omenchat
        .omenchat_invitation_preview
        .pending()
        .is_none());
    assert!(desktop.omenchat.omenchat_server_entry.is_empty());
}

#[cfg(feature = "mock-runtime")]
#[test]
fn opening_destination_from_blank_chat_replaces_blank_pane() {
    let (mut desktop, _) = desktop_with_paths("omenbrowser-rs-desktop-open-from-blank-chat");
    desktop.app.runtime_status.connected = false;

    let _ = desktop.update(Message::OmenChat(OmenChatMessage::NewPane));
    let blank_id = desktop.omenchat.chat_client.sessions()[0].session_id;
    assert!(is_pending_omenchat_destination(
        &desktop.omenchat.chat_client.sessions()[0]
            .server
            .destination
    ));

    desktop.omenchat.omenchat_server_entry = FIXTURE_CHAT_SERVER_HASH.into();
    let _ = desktop.update(Message::OmenChat(OmenChatMessage::OpenServerEntry));

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
    let _ = desktop.update(Message::OmenChat(OmenChatMessage::OpenServerEntry));
    desktop.omenchat.omenchat_server_entry = FIXTURE_OMENCHAT_HASH.into();
    let _ = desktop.update(Message::OmenChat(OmenChatMessage::OpenServerEntry));

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
    let _ = desktop.update(Message::OmenChat(OmenChatMessage::OpenServerEntry));

    assert_eq!(desktop.omenchat.chat_client.sessions().len(), 2);
    assert!(desktop
        .app
        .status
        .task
        .contains("restored existing OMENchat session"));
}
