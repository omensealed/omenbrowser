use std::path::PathBuf;

use omenbrowser_rs::messaging::DeliveryMode;
use omenbrowser_rs::storage::settings::{
    AppSettings, BrowserFocusedControlSettings, BrowserFocusedLinkSettings,
    BrowserFormStateSettings, BrowserOverlayPreference, BrowserTabSettings,
    ConversationTabSettings, DesktopWorkspaceLayoutNode, DesktopWorkspacePaneKind,
    DesktopWorkspacePaneSettings, DesktopWorkspaceSplitAxis, RuntimeBackendSetting,
    SensitiveFormPersistence, WorkspaceSectionPreference,
};

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "omenbrowser-rs-settings-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

#[test]
fn default_settings_match_phase_zero_expectations() {
    let settings = AppSettings::default();

    assert_eq!(settings.ui.theme_name, "default");
    assert_eq!(
        settings.ui.browser_overlay_mode,
        BrowserOverlayPreference::Status
    );
    assert_eq!(
        settings.ui.active_workspace_section,
        WorkspaceSectionPreference::Browser
    );
    assert_eq!(settings.ui.active_browser_index, 0);
    assert_eq!(settings.ui.active_conversation_index, 0);
    assert!(settings.browser_tabs.is_empty());
    assert!(settings.conversation_tabs.is_empty());
    assert!(settings.browser_form_state.enabled);
    assert_eq!(settings.browser_form_state.max_age_secs, 60 * 60 * 24 * 14);
    assert_eq!(
        settings.browser_form_state.sensitive_fields,
        SensitiveFormPersistence::Never
    );
    assert_eq!(settings.default_start_page, "mock.page:/");
    assert_eq!(settings.runtime_backend, RuntimeBackendSetting::Auto);
    assert!(settings.periodic_lxmf_sync);
    assert!(!settings.auto_sync_after_propagation_accept);
    assert_eq!(settings.lxmf_sync_interval, 360);
    assert_eq!(settings.lxmf_sync_limit, 8);
    assert!(!settings.plugins.remote_content_enabled);
    assert!(settings
        .plugins
        .enabled_plugin_ids
        .contains(&"micronplus_textui".into()));
}

#[test]
fn missing_settings_loads_defaults() {
    let path = temp_dir("missing").join("settings.json");

    let settings = AppSettings::load_or_default(&path).expect("load missing settings");

    assert_eq!(settings, AppSettings::default());
}

#[test]
fn settings_load_merges_missing_fields_with_defaults() {
    let path = temp_dir("partial").join("settings.json");
    std::fs::write(&path, r#"{"default_start_page":"mock.node:/custom"}"#)
        .expect("write partial settings");

    let settings = AppSettings::load_or_default(&path).expect("load partial settings");

    assert_eq!(settings.default_start_page, "mock.node:/custom");
    assert_eq!(settings.ui.theme_name, "default");
    assert_eq!(
        settings.ui.browser_overlay_mode,
        BrowserOverlayPreference::Status
    );
    assert_eq!(
        settings.ui.active_workspace_section,
        WorkspaceSectionPreference::Browser
    );
    assert_eq!(settings.lxmf_sync_limit, 8);
    assert!(settings.browser_tabs.is_empty());
    assert!(settings.conversation_tabs.is_empty());
    assert_eq!(
        settings.browser_form_state,
        BrowserFormStateSettings::default()
    );
}

#[test]
fn browser_form_state_settings_load_with_nested_defaults() {
    let path = temp_dir("form-state-settings").join("settings.json");
    std::fs::write(
        &path,
        r#"{"browser_form_state":{"sensitive_fields":"trusted_nodes"}}"#,
    )
    .expect("write form state settings");

    let settings = AppSettings::load_or_default(&path).expect("load settings");

    assert!(settings.browser_form_state.enabled);
    assert_eq!(settings.browser_form_state.max_age_secs, 60 * 60 * 24 * 14);
    assert_eq!(
        settings.browser_form_state.sensitive_fields,
        SensitiveFormPersistence::TrustedNodes
    );
}

#[test]
fn nested_ui_settings_load_merges_missing_fields_with_defaults() {
    let path = temp_dir("partial-ui").join("settings.json");
    std::fs::write(&path, r#"{"ui":{"theme_name":"amber","show_help":true}}"#)
        .expect("write partial ui settings");

    let settings = AppSettings::load_or_default(&path).expect("load partial ui settings");

    assert_eq!(settings.ui.theme_name, "amber");
    assert!(settings.ui.show_help);
    assert_eq!(
        settings.ui.browser_overlay_mode,
        BrowserOverlayPreference::Status
    );
    assert_eq!(
        settings.ui.active_workspace_section,
        WorkspaceSectionPreference::Browser
    );
    assert_eq!(settings.ui.active_browser_index, 0);
    assert_eq!(settings.ui.active_conversation_index, 0);
    assert_eq!(settings.ui.sidebar_index, 0);
    assert!(settings.ui.desktop_workspace_panes.is_empty());
    assert_eq!(settings.ui.active_desktop_workspace_pane, None);
    assert_eq!(settings.ui.desktop_workspace_layout, None);
}

#[test]
fn workspace_display_preferences_serialize_as_stable_strings() {
    let dir = temp_dir("overlay-serialization");
    let path = dir.join("settings.json");
    let mut settings = AppSettings::default();
    settings.ui.browser_overlay_mode = BrowserOverlayPreference::Expanded;
    settings.ui.active_workspace_section = WorkspaceSectionPreference::Diagnostics;
    settings.ui.active_browser_index = 3;
    settings.ui.active_conversation_index = 2;
    settings.ui.sidebar_index = 5;
    settings.ui.desktop_workspace_panes = vec![
        DesktopWorkspacePaneSettings {
            kind: DesktopWorkspacePaneKind::Browser,
            index: 1,
        },
        DesktopWorkspacePaneSettings {
            kind: DesktopWorkspacePaneKind::Conversation,
            index: 0,
        },
    ];
    settings.ui.active_desktop_workspace_pane = Some(1);
    settings.ui.desktop_workspace_layout = Some(DesktopWorkspaceLayoutNode::Split {
        axis: DesktopWorkspaceSplitAxis::Horizontal,
        ratio: 0.33,
        a: Box::new(DesktopWorkspaceLayoutNode::Pane {
            pane: DesktopWorkspacePaneSettings {
                kind: DesktopWorkspacePaneKind::Browser,
                index: 1,
            },
        }),
        b: Box::new(DesktopWorkspaceLayoutNode::Pane {
            pane: DesktopWorkspacePaneSettings {
                kind: DesktopWorkspacePaneKind::Conversation,
                index: 0,
            },
        }),
    });

    settings.save(&path).expect("save settings");
    let raw = std::fs::read_to_string(&path).expect("read settings");
    let loaded = AppSettings::load_or_default(&path).expect("load settings");

    assert!(raw.contains(r#""browser_overlay_mode": "expanded""#));
    assert!(raw.contains(r#""active_workspace_section": "diagnostics""#));
    assert!(raw.contains(r#""kind": "conversation""#));
    assert!(raw.contains(r#""axis": "horizontal""#));
    assert!(raw.contains(r#""ratio": 0.33"#));
    assert_eq!(
        loaded.ui.browser_overlay_mode,
        BrowserOverlayPreference::Expanded
    );
    assert_eq!(
        loaded.ui.active_workspace_section,
        WorkspaceSectionPreference::Diagnostics
    );
    assert_eq!(loaded.ui.active_browser_index, 3);
    assert_eq!(loaded.ui.active_conversation_index, 2);
    assert_eq!(loaded.ui.sidebar_index, 5);
    assert_eq!(
        loaded.ui.desktop_workspace_panes,
        settings.ui.desktop_workspace_panes
    );
    assert_eq!(loaded.ui.active_desktop_workspace_pane, Some(1));
    assert_eq!(
        loaded.ui.desktop_workspace_layout,
        settings.ui.desktop_workspace_layout
    );
}

#[test]
fn session_restore_descriptors_serialize_without_runtime_state() {
    let dir = temp_dir("session-descriptors");
    let path = dir.join("settings.json");
    let settings = AppSettings {
        browser_tabs: vec![BrowserTabSettings {
            title: "Docs".into(),
            address_input: "mock.node:/docs.mu".into(),
            current_url: "mock.node:/docs.mu".into(),
            history: vec!["mock.node:/".into(), "mock.node:/docs.mu".into()],
            history_index: 1,
            scroll_offset: 7,
            micron_zoom_percent: 125,
            focused_control: Some(BrowserFocusedControlSettings {
                name: "query".into(),
                index: 2,
            }),
            focused_link: Some(BrowserFocusedLinkSettings {
                target: "mock.node:/next.mu".into(),
                fields: vec!["query".into()],
                region_index: 3,
            }),
        }],
        conversation_tabs: vec![ConversationTabSettings {
            peer_hash: "lxmf@abc".into(),
            peer_label: "Peer ABC".into(),
            draft_title: "subject".into(),
            draft_body: "body".into(),
            attachments: vec![PathBuf::from("/tmp/example.txt")],
            delivery_mode: DeliveryMode::Propagated,
            include_ticket: true,
        }],
        ..AppSettings::default()
    };

    settings.save(&path).expect("save settings");
    let raw = std::fs::read_to_string(&path).expect("read settings");
    let loaded = AppSettings::load_or_default(&path).expect("load settings");

    assert!(raw.contains(r#""browser_tabs""#));
    assert!(raw.contains(r#""conversation_tabs""#));
    assert!(!raw.contains("pending_send"));
    assert!(!raw.contains("current_page"));
    assert!(raw.contains(r#""scroll_offset": 7"#));
    assert!(raw.contains(r#""focused_control""#));
    assert!(raw.contains(r#""focused_link""#));
    assert_eq!(loaded.browser_tabs, settings.browser_tabs);
    assert_eq!(loaded.conversation_tabs, settings.conversation_tabs);
}

#[test]
fn clearweb_privacy_defaults_keep_remote_media_off_and_tor_socks_hint_on() {
    let settings = AppSettings::default();

    assert!(settings.clearweb.prompt_external_browser);
    assert!(settings.clearweb.socks_proxy_enabled);
    assert_eq!(settings.clearweb.socks_proxy_host, "127.0.0.1");
    assert_eq!(settings.clearweb.socks_proxy_port, 9050);
    // Runtime detection also checks Tor Browser Bundle's common 9150 port.
    assert!(!settings.clearweb.remote_media_enabled);
    assert!(settings
        .clearweb
        .preferred_external_browser_command
        .is_none());
}

#[test]
fn corrupted_settings_are_backed_up_and_defaults_returned() {
    let dir = temp_dir("corrupt");
    let path = dir.join("settings.json");
    std::fs::write(&path, b"{not json").expect("write corrupt settings");

    let settings = AppSettings::load_or_default(&path).expect("load corrupt settings");
    let backups = std::fs::read_dir(&dir)
        .expect("read temp dir")
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with("settings.json.corrupt.")
        })
        .count();

    assert_eq!(settings, AppSettings::default());
    assert_eq!(backups, 1);
}

#[test]
fn save_preserves_unknown_future_fields() {
    let dir = temp_dir("extra");
    let path = dir.join("settings.json");
    std::fs::write(
        &path,
        r#"{"default_start_page":"mock.node:/x","future_setting":{"enabled":true}}"#,
    )
    .expect("write settings");
    let mut settings = AppSettings::load_or_default(&path).expect("load settings");
    settings.default_start_page = "mock.node:/y".into();

    settings.save(&path).expect("save settings");
    let raw = std::fs::read_to_string(&path).expect("read saved settings");

    assert!(raw.contains("future_setting"));
    assert!(raw.contains("mock.node:/y"));
}
