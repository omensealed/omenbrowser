#![cfg(feature = "chat-client")]

use std::path::{Path, PathBuf};

use omenbrowser_rs::chat::store::SqliteChatStore;
use omenbrowser_rs::chat::{
    ChatClient, ChatEvent, ChatEventKind, ChatRoomSummary, ChatServerSummary, ChatSessionView,
    ChatUserSummary,
};
use omenbrowser_rs::config::AppPaths;
use omenbrowser_rs::messaging::DeliveryMode;
use omenbrowser_rs::storage::settings::{
    AppSettings, BrowserTabSettings, ConversationTabSettings, DesktopWorkspaceLayoutNode,
    DesktopWorkspacePaneKind, DesktopWorkspacePaneSettings, DesktopWorkspaceSplitAxis,
    ReticulumInstanceMode, UiPreferences,
};

const BROWSER_PANES: usize = 20;
const CONVERSATION_PANES: usize = 20;
const OMENCHAT_PANES: usize = 10;
const EVENTS_PER_OMENCHAT_SESSION: usize = 20;

fn pane_stress_settings() -> AppSettings {
    let browser_tabs = (0..BROWSER_PANES)
        .map(|index| BrowserTabSettings {
            title: format!("Stress Browser {index:02}"),
            address_input: format!("mock.page:/stress/{index:02}.mu"),
            current_url: format!("mock.page:/stress/{index:02}.mu"),
            history: vec![
                "mock.page:/".into(),
                format!("mock.page:/stress/{index:02}.mu"),
            ],
            history_index: 1,
            scroll_offset: index * 3,
            ..Default::default()
        })
        .collect();
    let conversation_tabs = (0..CONVERSATION_PANES)
        .map(|index| ConversationTabSettings {
            peer_hash: format!("{:032x}", 0x1000_u64 + index as u64),
            peer_label: format!("Stress Peer {index:02}"),
            draft_title: format!("Draft {index:02}"),
            draft_body: format!("Deterministic pane-stress conversation body {index:02}"),
            attachments: Vec::new(),
            delivery_mode: DeliveryMode::Direct,
            include_ticket: false,
        })
        .collect();

    let mut panes = Vec::with_capacity(BROWSER_PANES + CONVERSATION_PANES + OMENCHAT_PANES);
    panes.extend(
        (0..BROWSER_PANES).map(|index| DesktopWorkspacePaneSettings {
            kind: DesktopWorkspacePaneKind::Browser,
            index,
        }),
    );
    panes.extend(
        (0..CONVERSATION_PANES).map(|index| DesktopWorkspacePaneSettings {
            kind: DesktopWorkspacePaneKind::Conversation,
            index,
        }),
    );
    panes.extend(
        (0..OMENCHAT_PANES).map(|index| DesktopWorkspacePaneSettings {
            kind: DesktopWorkspacePaneKind::OmenChat,
            index,
        }),
    );
    AppSettings {
        periodic_lxmf_sync: false,
        // This is a UI/persistence fixture, not a network or identity fixture. External mode
        // keeps launch bootstrap from creating an identity and moving storage to a new
        // identity-scoped root between measurement cycles.
        reticulum_instance_mode: ReticulumInstanceMode::External,
        browser_tabs,
        conversation_tabs,
        ui: UiPreferences {
            desktop_workspace_layout: Some(balanced_layout(&panes, 0)),
            desktop_workspace_panes: panes,
            active_desktop_workspace_pane: Some(0),
            ..Default::default()
        },
        ..Default::default()
    }
}

fn balanced_layout(
    panes: &[DesktopWorkspacePaneSettings],
    depth: usize,
) -> DesktopWorkspaceLayoutNode {
    if panes.len() == 1 {
        return DesktopWorkspaceLayoutNode::Pane {
            pane: panes[0].clone(),
        };
    }
    let midpoint = panes.len() / 2;
    DesktopWorkspaceLayoutNode::Split {
        axis: if depth % 2 == 0 {
            DesktopWorkspaceSplitAxis::Vertical
        } else {
            DesktopWorkspaceSplitAxis::Horizontal
        },
        ratio: 0.5,
        a: Box::new(balanced_layout(&panes[..midpoint], depth + 1)),
        b: Box::new(balanced_layout(&panes[midpoint..], depth + 1)),
    }
}

fn write_fixture(root: &Path) {
    let paths = AppPaths::from_root(root.to_path_buf());
    paths.ensure().expect("create isolated fixture paths");
    assert!(
        !paths.settings_file.exists(),
        "pane-stress fixture refuses to replace existing settings"
    );
    pane_stress_settings()
        .save(&paths.settings_file)
        .expect("save pane-stress settings");

    let chat_path = paths
        .identity_storage_root()
        .join("plugins")
        .join(omenbrowser_rs::plugins::BUILTIN_OMENCHAT_PLUGIN_ID)
        .join("chat.sqlite");
    let mut store = SqliteChatStore::open(chat_path).expect("open isolated OMENchat store");
    let mut client = ChatClient::new();
    for index in 0..OMENCHAT_PANES {
        let session_id = client.reserve_session_id();
        let server_id = format!("stress-server-{index:02}");
        let room = ChatRoomSummary {
            server_id: server_id.clone(),
            room_id: 1,
            name: "lobby".into(),
            topic: Some(format!("Deterministic stress room {index:02}")),
            unread: index as u32,
            joined: true,
        };
        client.push_session(ChatSessionView {
            session_id,
            server: ChatServerSummary {
                server_id: server_id.clone(),
                destination: format!("{:032x}", 0x2000_u64 + index as u64),
                display_name: format!("Stress Server {index:02}"),
            },
            rooms: vec![room.clone()],
            active_room: room,
            users: (0..5)
                .map(|user| ChatUserSummary {
                    server_id: server_id.clone(),
                    user_id: user + 1,
                    display_name: format!("User {index:02}-{user:02}"),
                    role_bits: 0,
                    status_bits: 0,
                    lxmf_available: user % 2 == 0,
                })
                .collect(),
            events: (0..EVENTS_PER_OMENCHAT_SESSION)
                .map(|event| ChatEvent {
                    server_id: server_id.clone(),
                    room_id: 1,
                    event_id: event as u64 + 1,
                    actor_user_id: Some(event as u32 % 5 + 1),
                    actor_display_name: Some(format!("User {index:02}-{:02}", event % 5)),
                    at_unix: 1_700_000_000 + event as i64,
                    kind: ChatEventKind::Message {
                        body: format!("Deterministic stress event {index:02}-{event:02}"),
                    },
                })
                .collect(),
            status: "fixture restored".into(),
        });
        client
            .persist_session(&mut store, session_id)
            .expect("persist pane-stress OMENchat session");
    }
}

fn assert_fixture_shape(root: &Path) {
    let paths = AppPaths::from_root(root.to_path_buf());
    let settings = AppSettings::load_or_default(&paths.settings_file).expect("load fixture");
    assert_eq!(settings.browser_tabs.len(), BROWSER_PANES);
    assert_eq!(settings.conversation_tabs.len(), CONVERSATION_PANES);
    assert_eq!(settings.ui.desktop_workspace_panes.len(), 50);
    assert!(settings.ui.desktop_workspace_layout.is_some());

    let chat_path = paths
        .identity_storage_root()
        .join("plugins")
        .join(omenbrowser_rs::plugins::BUILTIN_OMENCHAT_PLUGIN_ID)
        .join("chat.sqlite");
    let store = SqliteChatStore::open(chat_path).expect("open fixture store");
    let mut client = ChatClient::new();
    assert_eq!(
        client
            .restore_from_store(&store, 100)
            .expect("restore fixture sessions"),
        OMENCHAT_PANES
    );
}

fn explicit_isolated_root() -> PathBuf {
    let root = PathBuf::from(
        std::env::var_os("OMENBROWSER_PANE_STRESS_ROOT")
            .expect("OMENBROWSER_PANE_STRESS_ROOT is required"),
    );
    let temp = std::env::temp_dir();
    assert!(
        root.starts_with(&temp),
        "fixture root must be beneath the OS temp directory"
    );
    assert!(
        root.ancestors().any(|ancestor| {
            ancestor
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("omenbrowser-pane-stress."))
        }),
        "fixture root must be inside an omenbrowser-pane-stress.* directory"
    );
    root
}

#[test]
fn pane_stress_fixture_shape_is_deterministic() {
    let root = std::env::temp_dir().join(format!(
        "omenbrowser-pane-stress.unit.{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    write_fixture(&root);
    assert_fixture_shape(&root);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
#[ignore = "writes only to OMENBROWSER_PANE_STRESS_ROOT for the native measurement harness"]
fn write_pane_stress_fixture_to_explicit_isolated_root() {
    let root = explicit_isolated_root();
    write_fixture(&root);
    assert_fixture_shape(&root);
}

#[test]
#[ignore = "reads only from OMENBROWSER_PANE_STRESS_ROOT for the native measurement harness"]
fn verify_pane_stress_fixture_at_explicit_isolated_root() {
    let root = explicit_isolated_root();
    assert_fixture_shape(&root);
}
