use super::*;
use crate::chat::store::{ChatStore, SqliteChatStore};
use crate::desktop::DesktopPane;
use crate::storage::settings::{
    AppSettings, DesktopWorkspaceLayoutNode, DesktopWorkspacePaneKind, DesktopWorkspacePaneSettings,
};

#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
#[test]
fn desktop_startup_persists_and_retains_identity_scoped_client_instance() {
    let root = std::env::temp_dir().join(format!(
        "omenbrowser-rs-desktop-client-instance-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    let paths = crate::config::AppPaths::from_root(root.clone());
    paths.ensure().expect("paths");
    let instance_path = paths
        .identity_storage_root()
        .join("omenchat")
        .join("client-instance-id");

    let first = DesktopApp::new(App::new(crate::config::AppConfig {
        paths: paths.clone(),
        settings: AppSettings::default(),
    }));
    let first_id = first
        .omenchat
        .omenchat_live_state
        .client_instance_id()
        .expect("startup client instance");
    drop(first);
    assert_eq!(
        std::fs::read(&instance_path).expect("persisted client instance"),
        first_id.as_bytes()
    );

    let second = DesktopApp::new(App::new(crate::config::AppConfig {
        paths,
        settings: AppSettings::default(),
    }));
    assert_eq!(
        second.omenchat.omenchat_live_state.client_instance_id(),
        Some(first_id)
    );
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn desktop_omenchat_sessions_restore_from_plugin_store() {
    let root = std::env::temp_dir().join(format!(
        "omenbrowser-rs-desktop-restore-omenchat-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    let paths = crate::config::AppPaths::from_root(root);
    paths.ensure().expect("paths");

    let store_path = paths
        .identity_storage_root()
        .join("plugins")
        .join(crate::plugins::BUILTIN_OMENCHAT_PLUGIN_ID)
        .join("chat.sqlite");
    let mut store = SqliteChatStore::open(&store_path).expect("chat store");
    let mut client = crate::chat::ChatClient::new();
    let session_id = client.reserve_session_id();
    client.push_session(crate::chat::ChatSessionView {
        session_id,
        server: crate::chat::model::ChatServerSummary {
            server_id: "abcd1234abcd1234abcd1234abcd1234".into(),
            destination: "abcd1234abcd1234abcd1234abcd1234".into(),
            display_name: "Restored OMENchat".into(),
        },
        rooms: vec![crate::chat::model::ChatRoomSummary {
            server_id: "abcd1234abcd1234abcd1234abcd1234".into(),
            room_id: 1,
            name: "lobby".into(),
            topic: None,
            unread: 0,
            joined: true,
        }],
        active_room: crate::chat::model::ChatRoomSummary {
            server_id: "abcd1234abcd1234abcd1234abcd1234".into(),
            room_id: 1,
            name: "lobby".into(),
            topic: None,
            unread: 0,
            joined: true,
        },
        users: Vec::new(),
        events: Vec::new(),
        status: "test".into(),
    });
    client
        .persist_session(&mut store, session_id)
        .expect("persist chat session");

    let mut settings = AppSettings::default();
    settings.ui.desktop_workspace_panes = vec![DesktopWorkspacePaneSettings {
        kind: DesktopWorkspacePaneKind::OmenChat,
        index: 0,
    }];
    settings.ui.desktop_workspace_layout = Some(DesktopWorkspaceLayoutNode::Pane {
        pane: DesktopWorkspacePaneSettings {
            kind: DesktopWorkspacePaneKind::OmenChat,
            index: 0,
        },
    });

    let app = App::new(crate::config::AppConfig { paths, settings });
    let desktop = DesktopApp::new(app);

    let restored = desktop
        .omenchat
        .chat_client
        .sessions()
        .iter()
        .find(|session| session.server.display_name == "Restored OMENchat")
        .expect("restored session");
    assert!(desktop
        .workspace
        .workspace_panes
        .iter()
        .any(|(_, pane)| *pane == DesktopPane::OmenChat(restored.session_id)));
}

#[test]
fn desktop_startup_prunes_unrestorable_omenchat_cache_rows() {
    let root = std::env::temp_dir().join(format!(
        "omenbrowser-rs-desktop-prune-omenchat-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    let paths = crate::config::AppPaths::from_root(root);
    paths.ensure().expect("paths");

    let store_path = paths
        .identity_storage_root()
        .join("plugins")
        .join(crate::plugins::BUILTIN_OMENCHAT_PLUGIN_ID)
        .join("chat.sqlite");
    let mut store = SqliteChatStore::open(&store_path).expect("chat store");
    for (server_id, destination) in [
        ("mock-server", "mockchatdestination".to_string()),
        (
            "pending-server",
            format!("{}1", super::super::OMENCHAT_PENDING_DESTINATION_PREFIX),
        ),
    ] {
        store
            .save_server(crate::chat::model::ChatServerSummary {
                server_id: server_id.into(),
                destination,
                display_name: "Old Dev Chat".into(),
            })
            .expect("save server");
        store
            .save_room(crate::chat::model::ChatRoomSummary {
                server_id: server_id.into(),
                room_id: 1,
                name: "lobby".into(),
                topic: None,
                unread: 0,
                joined: true,
            })
            .expect("save room");
    }
    drop(store);

    let app = App::new(crate::config::AppConfig {
        paths: paths.clone(),
        settings: crate::storage::settings::AppSettings::default(),
    });
    let desktop = DesktopApp::new(app);

    assert!(desktop.omenchat.chat_client.sessions().is_empty());
    assert!(desktop
        .omenchat
        .chat_store
        .as_ref()
        .expect("store")
        .saved_servers()
        .expect("servers")
        .is_empty());
}
