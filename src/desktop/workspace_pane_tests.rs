use super::*;
use iced::widget::scrollable::RelativeOffset;

use crate::app::App;
#[cfg(feature = "chat-client")]
use crate::chat::store::ChatStore;
use crate::desktop::{BrowserMessage, ConversationMessage};
use crate::storage::settings::{
    DesktopWorkspaceLayoutNode, DesktopWorkspacePaneKind, DesktopWorkspacePaneSettings,
    DesktopWorkspaceSplitAxis,
};
use crate::workspace::WorkspaceSection;

const FIXTURE_LXMF_PEER_HASH: &str = "00112233445566778899aabbccddeeff";
#[cfg(feature = "chat-client")]
const FIXTURE_CHAT_SERVER_HASH: &str = "00112233445566778899aabbccddeeff";

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

fn incoming_lxmf_message(
    peer_hash: &str,
    title: &str,
    content: &str,
    message_id: &str,
) -> crate::messaging::MessageSummary {
    crate::messaging::MessageSummary {
        peer_hash: peer_hash.into(),
        peer_label: "Peer".into(),
        title: title.into(),
        content: content.into(),
        timestamp: 1.0,
        transport_method: crate::messaging::TransportMethod::Direct,
        delivered: true,
        failed: false,
        incoming: true,
        unread: true,
        message_id: Some(message_id.into()),
        fields: Default::default(),
        attachments: Vec::new(),
    }
}

#[cfg(feature = "chat-client")]
fn open_test_omenchat_session(desktop: &mut DesktopApp) -> crate::chat::ChatSessionId {
    desktop.open_omenchat_status_session(
        crate::chat::OmenChatDescriptor {
            server_destination: FIXTURE_CHAT_SERVER_HASH.into(),
            display_name: Some("Test OMENchat".into()),
            rooms_hint: vec!["lobby".into()],
            local_display_name: Some("tester".into()),
            ..crate::chat::OmenChatDescriptor::default()
        },
        "connected".into(),
    )
}

#[test]
fn desktop_workspace_starts_with_browser_and_message_panes() {
    let desktop = desktop_with_temp_root("omenbrowser-rs-desktop-panes");

    assert_eq!(desktop.workspace.workspace_panes.len(), 2);
    assert!(desktop
        .workspace
        .workspace_panes
        .iter()
        .any(|(_, pane)| matches!(pane, DesktopPane::Browser(_))));
    assert!(desktop
        .workspace
        .workspace_panes
        .iter()
        .any(|(_, pane)| matches!(pane, DesktopPane::Conversation(_))));
}

#[test]
fn workspace_focus_presets_hide_panes_without_deleting_backing_state() {
    let mut desktop = desktop_with_temp_root("omenbrowser-rs-desktop-focus-presets");
    let active_conversation = desktop.app.workspace.active_conversation;
    desktop.app.workspace.conversations[active_conversation].draft_body = "retained draft".into();
    let browser_id = desktop.app.active_browser_tab().id;
    let conversation_id = desktop.app.active_conversation().id;
    let browser_count = desktop.app.workspace.browser_tabs.len();
    let conversation_count = desktop.app.workspace.conversations.len();

    let _ = desktop.update(Message::WorkspacePane(WorkspacePaneMessage::ApplyPreset(
        DesktopWorkspacePreset::BrowserFocus,
    )));

    assert_eq!(desktop.workspace.workspace_panes.len(), 1);
    assert_eq!(
        desktop
            .workspace
            .workspace_panes
            .get(desktop.workspace.active_workspace_pane),
        Some(&DesktopPane::Browser(browser_id))
    );
    assert_eq!(desktop.app.workspace.browser_tabs.len(), browser_count);
    assert_eq!(
        desktop.app.workspace.conversations.len(),
        conversation_count
    );
    assert!(desktop
        .hidden_conversation_panes()
        .iter()
        .any(|(id, _, _)| *id == conversation_id));

    let _ = desktop.update(Message::WorkspacePane(WorkspacePaneMessage::ApplyPreset(
        DesktopWorkspacePreset::MessagesFocus,
    )));

    assert_eq!(desktop.workspace.workspace_panes.len(), 1);
    assert_eq!(
        desktop
            .workspace
            .workspace_panes
            .get(desktop.workspace.active_workspace_pane),
        Some(&DesktopPane::Conversation(conversation_id))
    );
    assert_eq!(desktop.app.workspace.browser_tabs.len(), browser_count);
    assert_eq!(
        desktop.app.workspace.conversations.len(),
        conversation_count
    );
    assert!(desktop
        .hidden_browser_panes()
        .iter()
        .any(|(id, _)| *id == browser_id));
}

#[test]
fn browser_and_messages_preset_persists_a_bounded_two_pane_layout() {
    let mut desktop = desktop_with_temp_root("omenbrowser-rs-desktop-split-preset");
    let _ = desktop.update(Message::WorkspacePane(WorkspacePaneMessage::ApplyPreset(
        DesktopWorkspacePreset::BrowserFocus,
    )));

    let _ = desktop.update(Message::WorkspacePane(WorkspacePaneMessage::ApplyPreset(
        DesktopWorkspacePreset::BrowserAndMessages,
    )));

    assert_eq!(desktop.workspace.workspace_panes.len(), 2);
    assert_eq!(desktop.app.settings.ui.desktop_workspace_panes.len(), 2);
    let Some(DesktopWorkspaceLayoutNode::Split { axis, ratio, a, b }) =
        desktop.app.settings.ui.desktop_workspace_layout.as_ref()
    else {
        panic!("expected persisted split preset");
    };
    assert_eq!(*axis, DesktopWorkspaceSplitAxis::Vertical);
    assert!((*ratio - 0.5).abs() < f32::EPSILON);
    assert!(matches!(
        a.as_ref(),
        DesktopWorkspaceLayoutNode::Pane {
            pane: DesktopWorkspacePaneSettings {
                kind: DesktopWorkspacePaneKind::Browser,
                ..
            }
        }
    ));
    assert!(matches!(
        b.as_ref(),
        DesktopWorkspaceLayoutNode::Pane {
            pane: DesktopWorkspacePaneSettings {
                kind: DesktopWorkspacePaneKind::Conversation,
                ..
            }
        }
    ));
}

#[cfg(feature = "chat-client")]
#[test]
fn all_active_panes_preset_restores_browser_messages_and_every_omenchat_session() {
    let mut desktop = desktop_with_temp_root("omenbrowser-rs-desktop-all-panes-preset");
    let first_chat = open_test_omenchat_session(&mut desktop);
    let second_chat = desktop.open_omenchat_status_session(
        crate::chat::OmenChatDescriptor {
            server_destination: "ffeeddccbbaa99887766554433221100".into(),
            display_name: Some("Second Chat".into()),
            rooms_hint: vec!["help".into()],
            local_display_name: Some("tester".into()),
            ..crate::chat::OmenChatDescriptor::default()
        },
        "connected".into(),
    );
    let browser = DesktopPane::Browser(desktop.app.active_browser_tab().id);
    let messages = DesktopPane::Conversation(desktop.app.active_conversation().id);

    let _ = desktop.update(Message::WorkspacePane(WorkspacePaneMessage::ApplyPreset(
        DesktopWorkspacePreset::BrowserFocus,
    )));
    assert_eq!(desktop.workspace.workspace_panes.len(), 1);

    let _ = desktop.update(Message::WorkspacePane(WorkspacePaneMessage::ApplyPreset(
        DesktopWorkspacePreset::AllActivePanes,
    )));

    assert_eq!(desktop.workspace.workspace_panes.len(), 4);
    for expected in [
        browser,
        messages,
        DesktopPane::OmenChat(first_chat),
        DesktopPane::OmenChat(second_chat),
    ] {
        assert!(desktop
            .workspace
            .workspace_panes
            .iter()
            .any(|(_, pane)| pane == &expected));
    }
    assert_eq!(desktop.app.settings.ui.desktop_workspace_panes.len(), 4);
}

#[test]
fn close_pane_does_not_close_backing_browser_tab() {
    let mut desktop = desktop_with_temp_root("omenbrowser-rs-desktop-close-pane");
    let pane = desktop
        .workspace
        .workspace_panes
        .iter()
        .find_map(|(pane, kind)| matches!(kind, DesktopPane::Browser(_)).then_some(*pane))
        .expect("browser pane");
    let initial_tabs = desktop.app.workspace.browser_tabs.len();

    let _ = desktop.update(Message::WorkspacePane(WorkspacePaneMessage::Close(pane)));

    assert_eq!(desktop.app.workspace.browser_tabs.len(), initial_tabs);
    assert!(!desktop
        .workspace
        .workspace_panes
        .iter()
        .any(|(candidate, _)| *candidate == pane));
}

#[test]
fn close_tab_button_closes_backing_browser_tab_and_pane() {
    let mut desktop = desktop_with_temp_root("omenbrowser-rs-desktop-close-tab");
    let _ = desktop.update(Message::Browser(BrowserMessage::NewTab));
    let closing_id = desktop.app.active_browser_tab().id;
    let initial_tabs = desktop.app.workspace.browser_tabs.len();

    let _ = desktop.update(Message::Browser(BrowserMessage::ClosePaneTab(closing_id)));

    assert_eq!(desktop.app.workspace.browser_tabs.len(), initial_tabs - 1);
    assert!(!desktop
        .workspace
        .workspace_panes
        .iter()
        .any(|(_, pane)| *pane == DesktopPane::Browser(closing_id)));
}

#[test]
fn target_mutation_reconciliation_removes_only_the_stale_pane() {
    let mut desktop = desktop_with_temp_root("omenbrowser-rs-desktop-mutation-pane-reconcile");
    let _ = desktop.update(Message::Browser(BrowserMessage::NewTab));
    let stale_id = desktop.app.active_browser_tab().id;
    let stale_pane = desktop
        .find_workspace_pane(&DesktopPane::Browser(stale_id))
        .expect("new browser pane");
    let initial_panes = desktop.workspace.workspace_panes.len();

    desktop.app.close_active_browser_tab();
    assert_eq!(desktop.workspace.workspace_panes.len(), initial_panes);
    assert_eq!(
        desktop.workspace.workspace_panes.get(stale_pane),
        Some(&DesktopPane::Browser(stale_id))
    );

    desktop.reconcile_workspace_panes_after_target_mutation(Some(stale_id), None);

    assert_eq!(desktop.workspace.workspace_panes.len(), initial_panes - 1);
    assert!(desktop
        .workspace
        .workspace_panes
        .iter()
        .all(|(_, pane)| *pane != DesktopPane::Browser(stale_id)));
}

#[test]
fn monitoring_sampling_runs_only_from_dedicated_section_tick() {
    let mut desktop = desktop_with_temp_root("omenbrowser-rs-desktop-monitoring-tick");
    desktop.app.workspace.active_section = crate::workspace::WorkspaceSection::Monitoring;
    desktop.monitoring.sample_epoch_ms = 0;

    assert_eq!(desktop.monitoring.sample_epoch_ms, 0);

    let _ = desktop.update_monitoring_tick();
    assert!(desktop.monitoring.sample_epoch_ms > 0);
}

#[test]
fn desktop_workspace_panes_restore_from_settings_order() {
    let root = std::env::temp_dir().join(format!(
        "omenbrowser-rs-desktop-restore-panes-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    let paths = crate::config::AppPaths::from_root(root);
    paths.ensure().expect("paths");
    let mut settings = crate::storage::settings::AppSettings::default();
    settings.browser_tabs = vec![
        crate::storage::settings::BrowserTabSettings {
            title: "One".into(),
            address_input: "mock.page:/one.mu".into(),
            current_url: "mock.page:/one.mu".into(),
            ..Default::default()
        },
        crate::storage::settings::BrowserTabSettings {
            title: "Two".into(),
            address_input: "mock.page:/two.mu".into(),
            current_url: "mock.page:/two.mu".into(),
            ..Default::default()
        },
    ];
    settings.ui.desktop_workspace_panes = vec![
        DesktopWorkspacePaneSettings {
            kind: DesktopWorkspacePaneKind::Conversation,
            index: 0,
        },
        DesktopWorkspacePaneSettings {
            kind: DesktopWorkspacePaneKind::Browser,
            index: 1,
        },
    ];
    settings.ui.active_desktop_workspace_pane = Some(1);
    let app = App::new(crate::config::AppConfig { paths, settings });
    let desktop = DesktopApp::new(app);
    let second_tab_id = desktop.app.workspace.browser_tabs[1].id;

    assert!(desktop
        .workspace
        .workspace_panes
        .iter()
        .any(|(_, pane)| *pane == DesktopPane::Browser(second_tab_id)));
    assert_eq!(
        desktop
            .workspace
            .workspace_panes
            .get(desktop.workspace.active_workspace_pane),
        Some(&DesktopPane::Browser(second_tab_id))
    );
}

#[test]
fn desktop_workspace_layout_uses_stable_generated_split() {
    let root = std::env::temp_dir().join(format!(
        "omenbrowser-rs-desktop-restore-layout-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    let paths = crate::config::AppPaths::from_root(root);
    paths.ensure().expect("paths");
    let mut settings = crate::storage::settings::AppSettings::default();
    settings.ui.desktop_workspace_layout = Some(DesktopWorkspaceLayoutNode::Split {
        axis: DesktopWorkspaceSplitAxis::Horizontal,
        ratio: 0.37,
        a: Box::new(DesktopWorkspaceLayoutNode::Pane {
            pane: DesktopWorkspacePaneSettings {
                kind: DesktopWorkspacePaneKind::Browser,
                index: 99,
            },
        }),
        b: Box::new(DesktopWorkspaceLayoutNode::Pane {
            pane: DesktopWorkspacePaneSettings {
                kind: DesktopWorkspacePaneKind::Conversation,
                index: 99,
            },
        }),
    });
    let app = App::new(crate::config::AppConfig { paths, settings });
    let desktop = DesktopApp::new(app);

    match desktop.workspace.workspace_panes.layout() {
        pane_grid::Node::Split { axis, ratio, .. } => {
            assert_eq!(*axis, pane_grid::Axis::Vertical);
            assert!((*ratio - 0.5).abs() < f32::EPSILON);
        }
        pane_grid::Node::Pane(_) => panic!("expected generated split layout"),
    }
}

#[test]
fn desktop_workspace_layout_restores_multi_pane_startup_layout() {
    let root = std::env::temp_dir().join(format!(
        "omenbrowser-rs-desktop-restore-heavy-layout-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    let paths = crate::config::AppPaths::from_root(root);
    paths.ensure().expect("paths");
    let mut settings = crate::storage::settings::AppSettings::default();
    settings.browser_tabs = (0..4)
        .map(|index| crate::storage::settings::BrowserTabSettings {
            title: format!("Browser {index}"),
            address_input: format!("mock.page:/tab-{index}.mu"),
            current_url: format!("mock.page:/tab-{index}.mu"),
            ..Default::default()
        })
        .collect();
    settings.ui.desktop_workspace_layout = Some(DesktopWorkspaceLayoutNode::Split {
        axis: DesktopWorkspaceSplitAxis::Vertical,
        ratio: 0.33,
        a: Box::new(DesktopWorkspaceLayoutNode::Pane {
            pane: DesktopWorkspacePaneSettings {
                kind: DesktopWorkspacePaneKind::Browser,
                index: 0,
            },
        }),
        b: Box::new(DesktopWorkspaceLayoutNode::Split {
            axis: DesktopWorkspaceSplitAxis::Horizontal,
            ratio: 0.5,
            a: Box::new(DesktopWorkspaceLayoutNode::Pane {
                pane: DesktopWorkspacePaneSettings {
                    kind: DesktopWorkspacePaneKind::Browser,
                    index: 1,
                },
            }),
            b: Box::new(DesktopWorkspaceLayoutNode::Split {
                axis: DesktopWorkspaceSplitAxis::Vertical,
                ratio: 0.5,
                a: Box::new(DesktopWorkspaceLayoutNode::Pane {
                    pane: DesktopWorkspacePaneSettings {
                        kind: DesktopWorkspacePaneKind::Browser,
                        index: 2,
                    },
                }),
                b: Box::new(DesktopWorkspaceLayoutNode::Pane {
                    pane: DesktopWorkspacePaneSettings {
                        kind: DesktopWorkspacePaneKind::Browser,
                        index: 3,
                    },
                }),
            }),
        }),
    });
    settings.ui.desktop_workspace_panes = (0..4)
        .map(|index| DesktopWorkspacePaneSettings {
            kind: DesktopWorkspacePaneKind::Browser,
            index,
        })
        .collect();
    let app = App::new(crate::config::AppConfig { paths, settings });
    let desktop = DesktopApp::new(app);

    assert_eq!(desktop.workspace.workspace_panes.len(), 4);
    for tab in &desktop.app.workspace.browser_tabs {
        assert!(desktop
            .workspace
            .workspace_panes
            .iter()
            .any(|(_, pane)| *pane == DesktopPane::Browser(tab.id)));
    }
}

#[test]
fn desktop_workspace_layout_persists_current_split_ratio() {
    let mut desktop = desktop_with_temp_root("omenbrowser-rs-desktop-persist-layout");
    let split = *desktop
        .workspace
        .workspace_panes
        .layout()
        .splits()
        .next()
        .expect("initial split");

    let _ = desktop.update(Message::WorkspacePane(WorkspacePaneMessage::Resized(
        pane_grid::ResizeEvent { split, ratio: 0.64 },
    )));

    let Some(DesktopWorkspaceLayoutNode::Split { axis, ratio, .. }) =
        desktop.app.settings.ui.desktop_workspace_layout.as_ref()
    else {
        panic!("expected persisted split layout");
    };
    assert_eq!(*axis, DesktopWorkspaceSplitAxis::Vertical);
    assert!((*ratio - 0.64).abs() < f32::EPSILON);
}

#[test]
fn new_browser_tab_adds_workspace_pane() {
    let mut desktop = desktop_with_temp_root("omenbrowser-rs-desktop-new-pane");
    let initial_panes = desktop.workspace.workspace_panes.len();

    let _ = desktop.update(Message::Browser(BrowserMessage::NewTab));

    assert_eq!(desktop.workspace.workspace_panes.len(), initial_panes + 1);
    let active_id = desktop.app.active_browser_tab().id;
    assert!(desktop
        .workspace
        .workspace_panes
        .iter()
        .any(|(_, pane)| *pane == DesktopPane::Browser(active_id)));
}

#[test]
fn hidden_browser_pane_can_be_restored_to_tiled_layout() {
    let mut desktop = desktop_with_temp_root("omenbrowser-rs-desktop-hidden-browser-pane");
    desktop.app.new_browser_tab();
    let hidden_id = desktop.app.active_browser_tab().id;
    desktop.ensure_pane_for_active_browser();
    let pane = desktop
        .find_workspace_pane(&DesktopPane::Browser(hidden_id))
        .expect("browser pane");

    desktop.close_workspace_pane(pane);
    assert!(desktop
        .hidden_browser_panes()
        .iter()
        .any(|(tab_id, _)| *tab_id == hidden_id));

    let _ = desktop.restore_desktop_pane(DesktopPane::Browser(hidden_id));
    assert!(desktop
        .find_workspace_pane(&DesktopPane::Browser(hidden_id))
        .is_some());
    assert!(!desktop
        .hidden_browser_panes()
        .iter()
        .any(|(tab_id, _)| *tab_id == hidden_id));
}

#[cfg(feature = "chat-client")]
#[test]
fn omenchat_pane_subtitle_does_not_duplicate_room_topic() {
    let mut desktop = desktop_with_temp_root("omenbrowser-rs-desktop-omenchat-subtitle-topic");
    let session_id = open_test_omenchat_session(&mut desktop);
    if let Some(session) = desktop.omenchat.chat_client.session_mut(session_id) {
        session.active_room.topic = Some("Welcome to OMENchat".into());
    }

    let subtitle = desktop
        .workspace_pane_subtitle(&DesktopPane::OmenChat(session_id))
        .expect("subtitle");

    assert!(subtitle.contains("room: #lobby"));
    assert!(!subtitle.contains("Welcome to OMENchat"));
}

#[cfg(feature = "chat-client")]
#[test]
fn omenchat_pane_and_restore_label_prefer_directory_name_and_keep_server_stats() {
    let mut desktop = desktop_with_temp_root("omenbrowser-rs-desktop-omenchat-directory-title");
    let session_id = open_test_omenchat_session(&mut desktop);
    desktop
        .app
        .directory_service
        .ingest_announce(
            FIXTURE_CHAT_SERVER_HASH,
            "NEMO",
            crate::directory::DirectoryKind::OmenChat,
            None,
            None,
        )
        .expect("directory announce");
    if let Some(session) = desktop.omenchat.chat_client.session_mut(session_id) {
        for (user_id, name) in [(7, "Alice"), (8, "Bob")] {
            session.users.push(crate::chat::ChatUserSummary {
                server_id: FIXTURE_CHAT_SERVER_HASH.into(),
                user_id,
                display_name: name.into(),
                role_bits: 0,
                status_bits: 0,
                lxmf_available: false,
                profile_revision: 0,
                nickname_colour_rgb: None,
            });
        }
    }

    assert_eq!(
        desktop.workspace_pane_title(&DesktopPane::OmenChat(session_id)),
        "NEMO - OMENchat"
    );

    let _ = desktop.restore_desktop_pane(DesktopPane::OmenChat(session_id));
    let pane = desktop
        .find_workspace_pane(&DesktopPane::OmenChat(session_id))
        .expect("omenchat pane");
    desktop.close_workspace_pane(pane);
    assert!(desktop
        .hidden_omenchat_panes()
        .iter()
        .any(|(hidden_id, label, _)| {
            *hidden_id == session_id && label == "NEMO - OMENchat · disconnected · #lobby · 2 users"
        }));
}

#[cfg(feature = "chat-client")]
#[test]
fn hidden_omenchat_panes_report_unread_state_for_restore_tabs() {
    let mut desktop = desktop_with_temp_root("omenbrowser-rs-desktop-hidden-omenchat-unread");
    let session_id = open_test_omenchat_session(&mut desktop);
    let _ = desktop.restore_desktop_pane(DesktopPane::OmenChat(session_id));
    let pane = desktop
        .find_workspace_pane(&DesktopPane::OmenChat(session_id))
        .expect("omenchat pane");

    desktop.close_workspace_pane(pane);
    if let Some(session) = desktop.omenchat.chat_client.session_mut(session_id) {
        session.active_room.unread = 2;
        if let Some(room) = session.rooms.first_mut() {
            room.unread = 2;
        }
    }

    assert!(desktop
        .hidden_omenchat_panes()
        .iter()
        .any(|(hidden_id, label, unread)| {
            *hidden_id == session_id
                && label == "Test OMENchat - OMENchat · disconnected · #lobby · 0 users"
                && *unread
        }));
}

#[cfg(feature = "chat-client")]
#[test]
fn hidden_omenchat_event_marks_restore_tab_unread_until_restored() {
    let mut desktop = desktop_with_temp_root("omenbrowser-rs-desktop-hidden-omenchat-event-unread");
    let session_id = open_test_omenchat_session(&mut desktop);
    let _ = desktop.restore_desktop_pane(DesktopPane::OmenChat(session_id));
    let pane = desktop
        .find_workspace_pane(&DesktopPane::OmenChat(session_id))
        .expect("omenchat pane");
    desktop.close_workspace_pane(pane);

    desktop.apply_omenchat_client_events_status(&[crate::chat::ChatClientEvent::EventAppended {
        session_id,
        event: crate::chat::ChatEvent {
            server_id: FIXTURE_CHAT_SERVER_HASH.into(),
            room_id: 1,
            event_id: 2,
            actor_user_id: Some(2),
            actor_display_name: Some("Peer".into()),
            at_unix: 1,
            kind: crate::chat::ChatEventKind::Message {
                body: "hello".into(),
            },
        },
    }]);

    assert!(desktop
        .hidden_omenchat_panes()
        .iter()
        .any(|(hidden_id, _, unread)| *hidden_id == session_id && *unread));

    let _ = desktop.restore_desktop_pane(DesktopPane::OmenChat(session_id));
    assert!(!desktop
        .hidden_omenchat_panes()
        .iter()
        .any(|(hidden_id, _, unread)| *hidden_id == session_id && *unread));
    let session = desktop
        .omenchat
        .chat_client
        .session(session_id)
        .expect("session");
    assert_eq!(session.active_room.unread, 0);
    assert_eq!(session.rooms[0].unread, 0);
}

#[cfg(feature = "chat-client")]
#[test]
fn hidden_omenchat_muted_room_counts_only_authoritative_mentions_as_unread() {
    let mut desktop =
        desktop_with_temp_root("omenbrowser-rs-desktop-hidden-omenchat-mentions-only");
    let session_id = open_test_omenchat_session(&mut desktop);
    assert!(desktop
        .omenchat
        .chat_client
        .bind_local_user_id(session_id, 7));
    desktop.persist_omenchat_session(session_id);
    let _ = desktop.update(Message::OmenChat(
        crate::desktop::OmenChatMessage::ToggleMuteExceptMentions {
            session_id,
            room_id: 1,
        },
    ));
    assert!(desktop
        .omenchat
        .chat_client
        .room_mute_except_mentions(session_id, 1));
    assert!(desktop
        .omenchat
        .chat_store
        .as_ref()
        .expect("chat store")
        .room_mute_except_mentions(&FIXTURE_CHAT_SERVER_HASH.to_owned(), 1)
        .expect("stored policy"));
    let _ = desktop.restore_desktop_pane(DesktopPane::OmenChat(session_id));
    let pane = desktop
        .find_workspace_pane(&DesktopPane::OmenChat(session_id))
        .expect("omenchat pane");
    desktop.close_workspace_pane(pane);

    let event = |event_id, mentioned_user_ids| crate::chat::ChatClientEvent::EventAppended {
        session_id,
        event: crate::chat::ChatEvent {
            server_id: FIXTURE_CHAT_SERVER_HASH.into(),
            room_id: 1,
            event_id,
            actor_user_id: Some(2),
            actor_display_name: Some("Peer".into()),
            at_unix: event_id as i64,
            kind: crate::chat::ChatEventKind::RichMessage {
                body: "message".into(),
                metadata: crate::chat::model::ChatMessageMetadata {
                    reply_to_event_id: None,
                    mentioned_user_ids,
                },
            },
        },
    };
    desktop.apply_omenchat_client_events_status(&[event(2, Vec::new())]);
    assert_eq!(
        desktop
            .omenchat
            .chat_client
            .session(session_id)
            .map(|session| session.active_room.unread),
        Some(0)
    );

    desktop.apply_omenchat_client_events_status(&[event(3, vec![7])]);
    let session = desktop
        .omenchat
        .chat_client
        .session(session_id)
        .expect("session");
    assert_eq!(session.active_room.unread, 1);
    assert_eq!(session.rooms[0].unread, 1);
}

#[cfg(feature = "chat-client")]
#[test]
fn hidden_omenchat_inactive_room_event_does_not_double_count_unread() {
    let mut desktop =
        desktop_with_temp_root("omenbrowser-rs-desktop-hidden-omenchat-inactive-unread");
    let session_id = open_test_omenchat_session(&mut desktop);
    if let Some(session) = desktop.omenchat.chat_client.session_mut(session_id) {
        session.rooms.push(crate::chat::model::ChatRoomSummary {
            server_id: FIXTURE_CHAT_SERVER_HASH.into(),
            room_id: 2,
            name: "help".into(),
            topic: None,
            unread: 1,
            joined: true,
        });
    }
    let _ = desktop.restore_desktop_pane(DesktopPane::OmenChat(session_id));
    let pane = desktop
        .find_workspace_pane(&DesktopPane::OmenChat(session_id))
        .expect("omenchat pane");
    desktop.close_workspace_pane(pane);

    desktop.apply_omenchat_client_events_status(&[crate::chat::ChatClientEvent::EventAppended {
        session_id,
        event: crate::chat::ChatEvent {
            server_id: FIXTURE_CHAT_SERVER_HASH.into(),
            room_id: 2,
            event_id: 3,
            actor_user_id: Some(2),
            actor_display_name: Some("Peer".into()),
            at_unix: 1,
            kind: crate::chat::ChatEventKind::Message {
                body: "inactive room hello".into(),
            },
        },
    }]);

    let session = desktop
        .omenchat
        .chat_client
        .session(session_id)
        .expect("session");
    assert_eq!(session.active_room.unread, 0);
    assert_eq!(
        session
            .rooms
            .iter()
            .find(|room| room.room_id == 2)
            .map(|room| room.unread),
        Some(1)
    );
    assert!(desktop
        .hidden_omenchat_panes()
        .iter()
        .any(|(hidden_id, _, unread)| *hidden_id == session_id && *unread));
}

#[test]
fn browser_pane_address_edit_targets_backing_tab() {
    let mut desktop = desktop_with_temp_root("omenbrowser-rs-desktop-target-tab");
    let first_id = desktop.app.active_browser_tab().id;
    let _ = desktop.update(Message::Browser(BrowserMessage::NewTab));
    let second_id = desktop.app.active_browser_tab().id;

    let _ = desktop.update(Message::Browser(BrowserMessage::PaneAddressChanged {
        tab_id: first_id,
        value: "mock.page:/first.mu".into(),
    }));

    let first = desktop
        .app
        .workspace
        .browser_tabs
        .iter()
        .find(|tab| tab.id == first_id)
        .expect("first tab");
    let second = desktop
        .app
        .workspace
        .browser_tabs
        .iter()
        .find(|tab| tab.id == second_id)
        .expect("second tab");
    assert_eq!(first.address_input, "mock.page:/first.mu");
    assert_ne!(second.address_input, "mock.page:/first.mu");
    assert_eq!(desktop.app.active_browser_tab().id, first_id);
}

#[test]
fn conversation_pane_composer_targets_backing_conversation() {
    let root = std::env::temp_dir().join(format!(
        "omenbrowser-rs-desktop-target-conversation-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    let paths = crate::config::AppPaths::from_root(root);
    paths.ensure().expect("paths");
    let mut app = App::new(crate::config::AppConfig {
        paths,
        settings: crate::storage::settings::AppSettings::default(),
    });
    let first_id = app.active_conversation().id;
    app.new_conversation();
    let second_id = app.active_conversation().id;
    let mut desktop = DesktopApp::new(app);

    let _ = desktop.update(Message::Conversation(
        ConversationMessage::PaneBodyChanged {
            conversation_id: first_id,
            value: "first body".into(),
        },
    ));

    let first = desktop
        .app
        .workspace
        .conversations
        .iter()
        .find(|conversation| conversation.id == first_id)
        .expect("first conversation");
    let second = desktop
        .app
        .workspace
        .conversations
        .iter()
        .find(|conversation| conversation.id == second_id)
        .expect("second conversation");
    assert_eq!(first.draft_body, "first body");
    assert_ne!(second.draft_body, "first body");
    assert_eq!(desktop.app.active_conversation().id, first_id);
}

#[test]
fn desktop_lxmf_micron_link_restores_conversation_pane() {
    let mut desktop = desktop_with_temp_root("omenbrowser-rs-desktop-lxmf-link");

    assert!(desktop.activate_lxmf_link(crate::micron::LinkAction {
        target: format!("lxmf@{FIXTURE_LXMF_PEER_HASH}"),
        fields: Vec::new(),
    }));

    let conversation_id = desktop.app.active_conversation().id;
    assert_eq!(
        desktop.app.active_conversation().peer_hash,
        FIXTURE_LXMF_PEER_HASH
    );
    assert!(desktop
        .workspace
        .workspace_panes
        .iter()
        .any(|(_, pane)| *pane == DesktopPane::Conversation(conversation_id)));
}

#[test]
fn new_conversation_button_adds_tiled_conversation_pane() {
    let mut desktop = desktop_with_temp_root("omenbrowser-rs-desktop-new-conversation-pane");
    let initial_conversations = desktop.app.workspace.conversations.len();
    let initial_panes = desktop.workspace.workspace_panes.len();

    let _ = desktop.update(Message::WorkspacePane(
        WorkspacePaneMessage::NewConversation,
    ));

    assert_eq!(
        desktop.app.workspace.conversations.len(),
        initial_conversations + 1
    );
    assert_eq!(desktop.workspace.workspace_panes.len(), initial_panes + 1);
    let active_id = desktop.app.active_conversation().id;
    assert!(desktop
        .workspace
        .workspace_panes
        .iter()
        .any(|(_, pane)| *pane == DesktopPane::Conversation(active_id)));
}

#[test]
fn adding_workspace_pane_preserves_existing_chat_scrollback_position() {
    let mut desktop = desktop_with_temp_root("omenbrowser-rs-desktop-add-pane-scroll-lock");
    let conversation_id = desktop.app.active_conversation().id;
    desktop.ensure_pane_for_active_conversation();
    desktop
        .conversation
        .scroll_offsets
        .insert(conversation_id, RelativeOffset { x: 0.0, y: 0.61 });
    desktop.workspace.restore_workspace_scrolls_pending = false;
    desktop.workspace.restore_workspace_scrolls_remaining = 0;
    desktop
        .workspace
        .restore_workspace_scroll_locks_release_pending = false;
    desktop.conversation.scroll_restore_locks.clear();

    let _ = desktop.update(Message::WorkspacePane(
        WorkspacePaneMessage::NewConversation,
    ));
    let _ = desktop.update(Message::Conversation(ConversationMessage::Scrolled {
        conversation_id,
        offset: RelativeOffset { x: 0.0, y: 0.0 },
    }));

    assert_eq!(
        desktop.conversation.scroll_offsets.get(&conversation_id),
        Some(&RelativeOffset { x: 0.0, y: 0.61 })
    );
}

#[test]
fn closing_workspace_pane_preserves_existing_chat_scrollback_position() {
    let mut desktop = desktop_with_temp_root("omenbrowser-rs-desktop-close-pane-scroll-lock");
    let conversation_id = desktop.app.active_conversation().id;
    desktop.ensure_pane_for_active_conversation();
    let browser_pane = desktop
        .workspace
        .workspace_panes
        .iter()
        .find_map(|(pane, kind)| matches!(kind, DesktopPane::Browser(_)).then_some(*pane))
        .expect("browser pane");
    desktop
        .conversation
        .scroll_offsets
        .insert(conversation_id, RelativeOffset { x: 0.0, y: 0.74 });
    desktop.workspace.restore_workspace_scrolls_pending = false;
    desktop.workspace.restore_workspace_scrolls_remaining = 0;
    desktop
        .workspace
        .restore_workspace_scroll_locks_release_pending = false;
    desktop.conversation.scroll_restore_locks.clear();

    desktop.close_workspace_pane(browser_pane);
    let _ = desktop.update(Message::Conversation(ConversationMessage::Scrolled {
        conversation_id,
        offset: RelativeOffset { x: 0.0, y: 0.0 },
    }));

    assert_eq!(
        desktop.conversation.scroll_offsets.get(&conversation_id),
        Some(&RelativeOffset { x: 0.0, y: 0.74 })
    );
}

#[test]
fn hidden_conversation_panes_report_unread_state_for_restore_tabs() {
    let mut desktop = desktop_with_temp_root("omenbrowser-rs-desktop-hidden-conversation-unread");
    desktop.app.workspace.conversations[0].peer_hash = FIXTURE_LXMF_PEER_HASH.into();
    desktop.app.workspace.conversations[0].peer_label = "Peer".into();
    desktop.app.workspace.conversations[0].push_message(incoming_lxmf_message(
        FIXTURE_LXMF_PEER_HASH,
        "hello",
        "body",
        "incoming-1",
    ));
    let conversation_id = desktop.app.workspace.conversations[0].id;
    let pane = desktop
        .find_workspace_pane(&DesktopPane::Conversation(conversation_id))
        .expect("conversation pane");
    desktop.close_workspace_pane(pane);

    assert!(desktop
        .hidden_conversation_panes()
        .iter()
        .any(|(hidden_id, label, unread)| {
            *hidden_id == conversation_id && label == "Peer" && *unread
        }));
}

#[test]
fn hidden_active_conversation_runtime_message_updates_unread_status() {
    let mut desktop =
        desktop_with_temp_root("omenbrowser-rs-desktop-hidden-active-conversation-runtime-unread");
    desktop.app.workspace.active_section = WorkspaceSection::Messages;
    desktop.app.workspace.conversations[0].peer_hash = FIXTURE_LXMF_PEER_HASH.into();
    desktop.app.workspace.conversations[0].peer_label = "Peer".into();
    desktop.app.workspace.conversations[0].thread.peer_hash = FIXTURE_LXMF_PEER_HASH.into();
    desktop.app.workspace.conversations[0].thread.peer_label = "Peer".into();
    let conversation_id = desktop.app.workspace.conversations[0].id;
    let pane = desktop
        .find_workspace_pane(&DesktopPane::Conversation(conversation_id))
        .expect("conversation pane");
    desktop.close_workspace_pane(pane);
    assert!(!desktop.active_conversation_pane_is_visible());

    assert!(desktop
        .app
        .enqueue_runtime_event(crate::runtime::RuntimeBusEvent::MessageReceived(
            incoming_lxmf_message(
                FIXTURE_LXMF_PEER_HASH,
                "hidden hello",
                "message while minimized",
                "hidden-active-inbound-1",
            ),
        )));
    let active_conversation_readable = desktop.active_conversation_pane_is_visible();
    assert_eq!(
        desktop
            .app
            .drain_internal_events_with_active_conversation_readable(active_conversation_readable,),
        1
    );

    let conversation = desktop
        .app
        .workspace
        .conversations
        .iter()
        .find(|conversation| conversation.id == conversation_id)
        .expect("conversation");
    assert_eq!(conversation.thread.unread_count, 1);
    assert_eq!(desktop.footer_lxmf_unread_counts(), (0, 1));
    assert!(desktop
        .hidden_conversation_panes()
        .iter()
        .any(|(hidden_id, label, unread)| {
            *hidden_id == conversation_id && label == "Peer" && *unread
        }));
}

#[test]
fn workspace_visibility_excludes_nonmaximized_and_inactive_section_panes() {
    let mut desktop = desktop_with_temp_root("omenbrowser-rs-desktop-pane-visibility");
    desktop.app.workspace.active_section = WorkspaceSection::Messages;
    let panes = desktop
        .workspace
        .workspace_panes
        .iter()
        .map(|(pane, kind)| (*pane, kind.clone()))
        .collect::<Vec<_>>();
    assert!(panes.len() >= 2);
    assert!(panes
        .iter()
        .all(|(_, kind)| desktop.workspace_pane_is_visible(kind)));

    desktop.workspace.workspace_panes.maximize(panes[0].0);
    assert!(desktop.workspace_pane_is_visible(&panes[0].1));
    assert!(!desktop.workspace_pane_is_visible(&panes[1].1));

    desktop.app.workspace.active_section = WorkspaceSection::Settings;
    assert!(!desktop.workspace_pane_is_visible(&panes[0].1));
    desktop.workspace.workspace_panes.restore();
    assert!(!desktop.workspace_pane_is_visible(&panes[1].1));
}

#[test]
fn history_search_context_requires_a_visible_lxmf_conversation_pane() {
    let mut desktop = desktop_with_temp_root("omenbrowser-rs-history-search-pane-visibility");
    desktop.app.workspace.active_section = WorkspaceSection::Messages;
    assert!(desktop.has_visible_lxmf_conversation_pane());

    let browser_pane = desktop
        .workspace
        .workspace_panes
        .iter()
        .find_map(|(pane, kind)| matches!(kind, DesktopPane::Browser(_)).then_some(*pane))
        .expect("browser pane");
    desktop.workspace.workspace_panes.maximize(browser_pane);
    assert!(!desktop.has_visible_lxmf_conversation_pane());

    desktop.workspace.workspace_panes.restore();
    assert!(desktop.has_visible_lxmf_conversation_pane());
}

#[test]
fn red_x_delete_conversation_removes_pane_instead_of_retargeting_to_next_thread() {
    let root = std::env::temp_dir().join(format!(
        "omenbrowser-rs-desktop-delete-conversation-pane-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    let paths = crate::config::AppPaths::from_root(root);
    paths.ensure().expect("paths");
    let mut app = App::new(crate::config::AppConfig {
        paths,
        settings: crate::storage::settings::AppSettings::default(),
    });
    app.workspace.conversations[0].peer_hash = "peer-one".into();
    app.workspace.conversations[0].peer_label = "Peer One".into();
    let first_id = app.workspace.conversations[0].id;
    app.new_conversation();
    app.workspace.conversations[1].peer_hash = "peer-two".into();
    app.workspace.conversations[1].peer_label = "Peer Two".into();
    let second_id = app.workspace.conversations[1].id;
    let mut desktop = DesktopApp::new(app);

    assert!(desktop
        .workspace
        .workspace_panes
        .iter()
        .any(|(_, pane)| *pane == DesktopPane::Conversation(second_id)));

    let _ = desktop.update(Message::WorkspacePane(
        WorkspacePaneMessage::CloseConversationTab(second_id),
    ));

    assert!(desktop
        .app
        .workspace
        .conversations
        .iter()
        .any(|conversation| conversation.id == first_id));
    assert!(desktop
        .app
        .workspace
        .conversations
        .iter()
        .all(|conversation| conversation.id != second_id));
    assert!(desktop
        .workspace
        .workspace_panes
        .iter()
        .all(|(_, pane)| !matches!(pane, DesktopPane::Conversation(_))));
    assert!(desktop
        .workspace
        .workspace_panes
        .iter()
        .any(|(_, pane)| matches!(pane, DesktopPane::Browser(_))));
}

#[test]
fn red_x_delete_last_blank_conversation_does_not_leave_restore_tab() {
    let root = std::env::temp_dir().join(format!(
        "omenbrowser-rs-desktop-delete-blank-conversation-pane-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    let paths = crate::config::AppPaths::from_root(root);
    paths.ensure().expect("paths");
    let settings_file = paths.settings_file.clone();
    let app = App::new(crate::config::AppConfig {
        paths,
        settings: crate::storage::settings::AppSettings::default(),
    });
    let mut desktop = DesktopApp::new(app);
    let conversation_id = desktop.app.active_conversation().id;

    let _ = desktop.update(Message::WorkspacePane(
        WorkspacePaneMessage::CloseConversationTab(conversation_id),
    ));

    assert!(desktop.hidden_conversation_panes().is_empty());
    assert!(desktop
        .workspace
        .workspace_panes
        .iter()
        .all(|(_, pane)| !matches!(pane, DesktopPane::Conversation(_))));
    let saved = crate::storage::settings::AppSettings::load_or_default(&settings_file)
        .expect("saved settings");
    assert!(saved.conversation_tabs.is_empty());
}
