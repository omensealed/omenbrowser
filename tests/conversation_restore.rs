use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};

use omenbrowser_rs::app::App;
use omenbrowser_rs::config::{AppConfig, AppPaths};
use omenbrowser_rs::messaging::{DeliveryMode, MessageStore, MessageSummary, TransportMethod};
use omenbrowser_rs::storage::settings::{
    AppSettings, ConversationTabSettings, DeletedConversationSettings, DesktopWorkspaceLayoutNode,
    DesktopWorkspacePaneKind, DesktopWorkspacePaneSettings, RuntimeBackendSetting,
};

const FIXTURE_PEER_HASH: &str = "00112233445566778899aabbccddeeff";

fn test_config(name: &str) -> AppConfig {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "omenbrowser-rs-conversation-restore-{name}-{}-{nonce}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    let settings = AppSettings {
        runtime_backend: RuntimeBackendSetting::Mock,
        ..AppSettings::default()
    };
    AppConfig {
        paths: AppPaths::from_root(root),
        settings,
    }
}

fn stored_inbound_message(
    peer_hash: &str,
    peer_label: &str,
    title: &str,
    content: &str,
    timestamp: f64,
    message_id: &str,
) -> MessageSummary {
    MessageSummary {
        peer_hash: peer_hash.into(),
        peer_label: peer_label.into(),
        title: title.into(),
        content: content.into(),
        timestamp,
        transport_method: TransportMethod::Direct,
        delivered: true,
        failed: false,
        incoming: true,
        unread: true,
        message_id: Some(message_id.into()),
        fields: BTreeMap::new(),
        attachments: Vec::new(),
    }
}

fn conversation_tab(peer_hash: &str, peer_label: &str) -> ConversationTabSettings {
    ConversationTabSettings {
        peer_hash: peer_hash.into(),
        peer_label: peer_label.into(),
        draft_title: String::new(),
        draft_body: String::new(),
        attachments: Vec::new(),
        delivery_mode: DeliveryMode::Direct,
        include_ticket: false,
    }
}

#[test]
fn delete_conversation_removes_stored_thread_and_replaces_last_tab_with_blank() {
    let config = test_config("delete-conversation-thread");
    let store = MessageStore::new(config.paths.messages_dir.clone()).expect("message store");
    store
        .append(stored_inbound_message(
            FIXTURE_PEER_HASH,
            "Peer",
            "Old title",
            "Old body",
            1.0,
            "old-message",
        ))
        .expect("append thread");
    let mut app = App::new(config);

    app.delete_active_conversation();

    assert_eq!(app.workspace.conversations.len(), 1);
    assert_eq!(app.active_conversation().peer_hash, "");
    assert_eq!(app.active_conversation().peer_label, "New Conversation");
    assert!(app
        .message_store
        .get_thread(FIXTURE_PEER_HASH)
        .expect("default missing thread")
        .messages
        .is_empty());
}

#[test]
fn delete_conversation_removes_thread_json_even_when_filename_drifted() {
    let config = test_config("delete-conversation-drifted-thread-file");
    let messages_dir = config.paths.messages_dir.clone();
    let store = MessageStore::new(messages_dir.clone()).expect("message store");
    store
        .append(stored_inbound_message(
            FIXTURE_PEER_HASH,
            "Peer",
            "Old title",
            "Old body",
            1.0,
            "old-message",
        ))
        .expect("append thread");
    std::fs::rename(
        messages_dir.join(format!("{FIXTURE_PEER_HASH}.json")),
        messages_dir.join("peer-label.json"),
    )
    .expect("rename drifted thread");

    let mut app = App::new(config);
    app.delete_active_conversation();

    assert!(!messages_dir.join("peer-label.json").exists());
    assert!(app
        .workspace
        .conversations
        .iter()
        .all(|conversation| !conversation
            .peer_hash
            .eq_ignore_ascii_case(FIXTURE_PEER_HASH)));
}

#[test]
fn delete_conversation_removes_duplicate_tabs_for_same_peer() {
    let mut config = test_config("delete-conversation-duplicates");
    config.settings.conversation_tabs = vec![
        conversation_tab(FIXTURE_PEER_HASH, "Peer"),
        conversation_tab(FIXTURE_PEER_HASH, "Peer duplicate"),
    ];
    config.settings.ui.active_conversation_index = 0;
    let store = MessageStore::new(config.paths.messages_dir.clone()).expect("message store");
    store
        .append(stored_inbound_message(
            FIXTURE_PEER_HASH,
            "Peer",
            "Old title",
            "Old body",
            1.0,
            "old-message",
        ))
        .expect("append thread");
    let mut app = App::new(config);

    app.delete_active_conversation();

    assert!(app
        .workspace
        .conversations
        .iter()
        .all(|conversation| conversation.peer_hash != FIXTURE_PEER_HASH));
    assert_eq!(app.active_conversation().peer_hash, "");
    assert!(app
        .settings
        .conversation_tabs
        .iter()
        .all(|tab| tab.peer_hash != FIXTURE_PEER_HASH));
}

#[test]
fn delete_conversation_removes_settings_only_restore_metadata() {
    let mut config = test_config("delete-conversation-settings-only");
    let peer = FIXTURE_PEER_HASH;
    config.settings.conversation_tabs = vec![ConversationTabSettings {
        peer_hash: peer.into(),
        peer_label: "Settings Peer".into(),
        draft_title: "stale subject".into(),
        draft_body: "stale body".into(),
        attachments: Vec::new(),
        delivery_mode: DeliveryMode::Direct,
        include_ticket: false,
    }];
    config.settings.ui.desktop_workspace_panes = vec![DesktopWorkspacePaneSettings {
        kind: DesktopWorkspacePaneKind::Conversation,
        index: 0,
    }];
    config.settings.ui.desktop_workspace_layout = Some(DesktopWorkspaceLayoutNode::Pane {
        pane: DesktopWorkspacePaneSettings {
            kind: DesktopWorkspacePaneKind::Conversation,
            index: 0,
        },
    });
    config
        .settings
        .save(&config.paths.settings_file)
        .expect("save settings");

    let mut app = App::new(config.clone());
    assert_eq!(app.active_conversation().peer_hash, peer);

    app.delete_active_conversation();

    let saved_settings =
        AppSettings::load_or_default(&app.paths.settings_file).expect("saved settings");
    assert!(saved_settings
        .conversation_tabs
        .iter()
        .all(|tab| !tab.peer_hash.eq_ignore_ascii_case(peer)));
    assert!(saved_settings
        .deleted_conversations
        .iter()
        .any(|deleted| deleted.peer_hash.eq_ignore_ascii_case(peer)));
    assert!(saved_settings
        .ui
        .desktop_workspace_panes
        .iter()
        .all(|pane| pane.kind != DesktopWorkspacePaneKind::Conversation));
    let reloaded = App::new(AppConfig {
        paths: app.paths.clone(),
        settings: saved_settings,
    });
    assert!(reloaded
        .workspace
        .conversations
        .iter()
        .all(|conversation| !conversation.peer_hash.eq_ignore_ascii_case(peer)));
}

#[test]
fn deleted_conversation_marker_blocks_stale_stored_thread_restore() {
    let mut config = test_config("delete-conversation-tombstone-store");
    let peer = FIXTURE_PEER_HASH;
    let store = MessageStore::new(config.paths.messages_dir.clone()).expect("message store");
    store
        .append(stored_inbound_message(
            peer,
            "Stale Peer",
            "stale",
            "old message",
            10.0,
            "old-message",
        ))
        .expect("append stale message");
    config.settings.deleted_conversations = vec![DeletedConversationSettings {
        peer_hash: peer.into(),
        deleted_at: 20.0,
    }];

    let app = App::new(config);

    assert!(app
        .workspace
        .conversations
        .iter()
        .all(|conversation| !conversation.peer_hash.eq_ignore_ascii_case(peer)));
    assert_eq!(app.active_conversation().peer_hash, "");
}

#[test]
fn empty_message_threads_do_not_restore_as_conversations() {
    let config = test_config("empty-message-thread-ignored");
    let peer = FIXTURE_PEER_HASH;
    let store = MessageStore::new(config.paths.messages_dir.clone()).expect("message store");
    store
        .ensure_thread(peer, Some("Empty Peer"))
        .expect("empty thread");

    let app = App::new(config);

    assert!(app
        .workspace
        .conversations
        .iter()
        .all(|conversation| !conversation.peer_hash.eq_ignore_ascii_case(peer)));
    assert_eq!(app.active_conversation().peer_hash, "");
}

#[test]
fn delete_conversation_removes_legacy_root_thread_for_scoped_identity() {
    let mut config = test_config("delete-conversation-legacy-root-thread");
    let identity_path = config.paths.identities_dir.join("default_identity");
    std::fs::create_dir_all(config.paths.identities_dir.clone()).expect("identities dir");
    std::fs::write(&identity_path, b"test identity material").expect("identity material");
    config.settings.identity_path = Some(identity_path);

    let peer = FIXTURE_PEER_HASH;
    let legacy_messages_dir = config.paths.messages_dir.clone();
    let legacy_store = MessageStore::new(legacy_messages_dir.clone()).expect("legacy store");
    legacy_store
        .append(stored_inbound_message(
            peer,
            "Peer",
            "Old title",
            "Old body",
            1.0,
            "old-message",
        ))
        .expect("append legacy thread");

    let mut app = App::new(config);
    let scoped_messages_dir = app.paths.messages_dir.clone();
    assert_ne!(scoped_messages_dir, legacy_messages_dir);
    assert!(legacy_messages_dir.join(format!("{peer}.json")).exists());
    assert!(scoped_messages_dir.join(format!("{peer}.json")).exists());

    app.delete_active_conversation();

    assert!(!legacy_messages_dir.join(format!("{peer}.json")).exists());
    assert!(!scoped_messages_dir.join(format!("{peer}.json")).exists());
    assert!(app
        .settings
        .conversation_tabs
        .iter()
        .all(|tab| !tab.peer_hash.eq_ignore_ascii_case(peer)));

    let saved_settings =
        AppSettings::load_or_default(&app.paths.settings_file).expect("saved settings");
    assert!(saved_settings
        .conversation_tabs
        .iter()
        .all(|tab| !tab.peer_hash.eq_ignore_ascii_case(peer)));
}
