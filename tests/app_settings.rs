use std::path::PathBuf;

use omenbrowser_rs::browser::session::{
    BROWSER_HISTORY_MAX_ITEMS, BROWSER_HISTORY_MAX_OWNED_BYTES,
};
use omenbrowser_rs::browser::Bookmark;
use omenbrowser_rs::messaging::DeliveryMode;
use omenbrowser_rs::micron::parser::{MICRON_LINK_FIELD_MAX_BYTES, MICRON_LINK_MAX_FIELDS};
use omenbrowser_rs::storage::settings::{
    AppSettings, BrowserFocusedControlSettings, BrowserFocusedLinkSettings,
    BrowserFormStateSettings, BrowserOverlayPreference, BrowserTabSettings,
    ConversationTabSettings, DesktopWorkspaceLayoutNode, DesktopWorkspacePaneKind,
    DesktopWorkspacePaneSettings, DesktopWorkspaceSplitAxis, RuntimeBackendSetting,
    SensitiveFormPersistence, WorkspaceSectionPreference, APP_SETTINGS_CORRUPT_BACKUP_MAX_FILES,
    APP_SETTINGS_CORRUPT_BACKUP_MAX_SCAN_ENTRIES, APP_SETTINGS_CORRUPT_BACKUP_MAX_TOTAL_BYTES,
    APP_SETTINGS_JSON_MAX_RAW_STRING_BYTES, APP_SETTINGS_MAX_ATTACHMENTS_PER_CONVERSATION,
    APP_SETTINGS_MAX_BOOKMARKS, APP_SETTINGS_MAX_BROWSER_TABS, APP_SETTINGS_MAX_BYTES,
    APP_SETTINGS_MAX_CONVERSATION_TABS, APP_SETTINGS_MAX_DELETED_CONVERSATIONS,
    APP_SETTINGS_MAX_EXTRA_CONTAINER_ITEMS, APP_SETTINGS_MAX_EXTRA_FIELDS,
    APP_SETTINGS_MAX_EXTRA_VALUE_DEPTH, APP_SETTINGS_MAX_PLUGIN_IDS,
    APP_SETTINGS_MAX_WORKSPACE_LAYOUT_DEPTH, APP_SETTINGS_MAX_WORKSPACE_PANES,
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

fn regular_corrupt_settings_backups(root: &std::path::Path) -> Vec<std::fs::DirEntry> {
    std::fs::read_dir(root)
        .expect("read settings root")
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with("settings.json.corrupt.")
                && entry.file_type().is_ok_and(|file_type| file_type.is_file())
        })
        .collect()
}

#[test]
fn default_settings_match_phase_zero_expectations() {
    let settings = AppSettings::default();

    assert_eq!(settings.ui.theme_name, "default");
    assert!(!settings.ui.reduce_motion);
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

fn nested_layout(depth: usize) -> DesktopWorkspaceLayoutNode {
    let pane = || DesktopWorkspaceLayoutNode::Pane {
        pane: DesktopWorkspacePaneSettings::default(),
    };
    let mut node = pane();
    for _ in 1..depth {
        node = DesktopWorkspaceLayoutNode::Split {
            axis: DesktopWorkspaceSplitAxis::Vertical,
            ratio: 0.5,
            a: Box::new(node),
            b: Box::new(pane()),
        };
    }
    node
}

fn nested_extra(depth: usize) -> serde_json::Value {
    let mut value = serde_json::Value::Null;
    for _ in 1..depth {
        value = serde_json::Value::Array(vec![value]);
    }
    value
}

#[test]
fn semantic_settings_accept_collection_and_recursion_boundaries() {
    let mut settings = AppSettings {
        browser_tabs: vec![BrowserTabSettings::default(); APP_SETTINGS_MAX_BROWSER_TABS],
        conversation_tabs: vec![
            ConversationTabSettings::default();
            APP_SETTINGS_MAX_CONVERSATION_TABS
        ],
        bookmarks: vec![
            Bookmark {
                title: "saved".into(),
                url: "mock.node:/".into(),
            };
            APP_SETTINGS_MAX_BOOKMARKS
        ],
        deleted_conversations: vec![Default::default(); APP_SETTINGS_MAX_DELETED_CONVERSATIONS],
        ..AppSettings::default()
    };
    settings.ui.desktop_workspace_panes =
        vec![Default::default(); APP_SETTINGS_MAX_WORKSPACE_PANES];
    settings.trusted_plugin_ids = vec!["trusted".into(); APP_SETTINGS_MAX_PLUGIN_IDS];
    settings.plugins.enabled_plugin_ids = vec!["enabled".into(); APP_SETTINGS_MAX_PLUGIN_IDS];
    settings.ui.desktop_workspace_layout =
        Some(nested_layout(APP_SETTINGS_MAX_WORKSPACE_LAYOUT_DEPTH));
    settings.extra.insert(
        "future".into(),
        serde_json::Value::Array(vec![
            serde_json::Value::Null;
            APP_SETTINGS_MAX_EXTRA_CONTAINER_ITEMS
        ]),
    );

    settings
        .validate_retained()
        .expect("exact semantic boundaries must be admitted");
}

#[test]
fn semantic_settings_reject_each_unbounded_collection_and_recursive_value() {
    let mut cases = Vec::new();

    let settings = AppSettings {
        browser_tabs: vec![BrowserTabSettings::default(); APP_SETTINGS_MAX_BROWSER_TABS + 1],
        ..AppSettings::default()
    };
    cases.push(settings);

    let settings = AppSettings {
        conversation_tabs: vec![
            ConversationTabSettings::default();
            APP_SETTINGS_MAX_CONVERSATION_TABS + 1
        ],
        ..AppSettings::default()
    };
    cases.push(settings);

    let settings = AppSettings {
        bookmarks: vec![
            Bookmark {
                title: "saved".into(),
                url: "mock.node:/".into(),
            };
            APP_SETTINGS_MAX_BOOKMARKS + 1
        ],
        ..AppSettings::default()
    };
    cases.push(settings);

    let settings = AppSettings {
        deleted_conversations: vec![Default::default(); APP_SETTINGS_MAX_DELETED_CONVERSATIONS + 1],
        ..AppSettings::default()
    };
    cases.push(settings);

    let mut settings = AppSettings::default();
    settings.ui.desktop_workspace_panes =
        vec![Default::default(); APP_SETTINGS_MAX_WORKSPACE_PANES + 1];
    cases.push(settings);

    let settings = AppSettings {
        trusted_plugin_ids: vec!["trusted".into(); APP_SETTINGS_MAX_PLUGIN_IDS + 1],
        ..AppSettings::default()
    };
    cases.push(settings);

    let mut settings = AppSettings::default();
    settings.plugins.enabled_plugin_ids = vec!["enabled".into(); APP_SETTINGS_MAX_PLUGIN_IDS + 1];
    cases.push(settings);

    let settings = AppSettings {
        conversation_tabs: vec![ConversationTabSettings {
            attachments: vec![
                PathBuf::from("attachment");
                APP_SETTINGS_MAX_ATTACHMENTS_PER_CONVERSATION + 1
            ],
            ..ConversationTabSettings::default()
        }],
        ..AppSettings::default()
    };
    cases.push(settings);

    let settings = AppSettings {
        browser_tabs: vec![BrowserTabSettings {
            history: vec!["mock.node:/".into(); BROWSER_HISTORY_MAX_ITEMS + 1],
            ..BrowserTabSettings::default()
        }],
        ..AppSettings::default()
    };
    cases.push(settings);

    let settings = AppSettings {
        browser_tabs: vec![BrowserTabSettings {
            history: vec![
                "x".repeat(
                    BROWSER_HISTORY_MAX_OWNED_BYTES / BROWSER_HISTORY_MAX_ITEMS + 1
                );
                BROWSER_HISTORY_MAX_ITEMS
            ],
            ..BrowserTabSettings::default()
        }],
        ..AppSettings::default()
    };
    cases.push(settings);

    let settings = AppSettings {
        browser_tabs: vec![BrowserTabSettings {
            focused_link: Some(BrowserFocusedLinkSettings {
                target: "mock.node:/".into(),
                fields: vec!["field".into(); MICRON_LINK_MAX_FIELDS + 1],
                region_index: 0,
            }),
            ..BrowserTabSettings::default()
        }],
        ..AppSettings::default()
    };
    cases.push(settings);

    let settings = AppSettings {
        browser_tabs: vec![BrowserTabSettings {
            focused_link: Some(BrowserFocusedLinkSettings {
                target: "mock.node:/".into(),
                fields: vec!["x".repeat(MICRON_LINK_FIELD_MAX_BYTES + 1)],
                region_index: 0,
            }),
            ..BrowserTabSettings::default()
        }],
        ..AppSettings::default()
    };
    cases.push(settings);

    let mut settings = AppSettings::default();
    for index in 0..=APP_SETTINGS_MAX_EXTRA_FIELDS {
        settings
            .extra
            .insert(format!("future_{index}"), serde_json::Value::Null);
    }
    cases.push(settings);

    let mut settings = AppSettings::default();
    settings.ui.desktop_workspace_layout =
        Some(nested_layout(APP_SETTINGS_MAX_WORKSPACE_LAYOUT_DEPTH + 1));
    cases.push(settings);

    let mut settings = AppSettings::default();
    settings.extra.insert(
        "future".into(),
        nested_extra(APP_SETTINGS_MAX_EXTRA_VALUE_DEPTH + 1),
    );
    cases.push(settings);

    let mut settings = AppSettings::default();
    settings.extra.insert(
        "future".into(),
        serde_json::Value::Array(
            (0..5)
                .map(|_| {
                    serde_json::Value::Array(vec![
                        serde_json::Value::Null;
                        APP_SETTINGS_MAX_EXTRA_CONTAINER_ITEMS
                    ])
                })
                .collect(),
        ),
    );
    cases.push(settings);

    for (index, settings) in cases.into_iter().enumerate() {
        assert!(
            settings.validate_retained().is_err(),
            "over-limit semantic case {index} must be rejected"
        );
    }
}

#[test]
fn syntactically_valid_over_limit_settings_are_backed_up_and_defaulted_atomically() {
    let dir = temp_dir("semantic-over-limit-load");
    let path = dir.join("settings.json");
    let mut rejected = AppSettings {
        default_start_page: "mock.node:/must-not-partially-restore".into(),
        ..AppSettings::default()
    };
    rejected.browser_tabs = vec![BrowserTabSettings::default(); APP_SETTINGS_MAX_BROWSER_TABS + 1];
    let raw = serde_json::to_vec(&rejected).expect("serialize over-limit fixture");
    std::fs::write(&path, &raw).expect("write over-limit settings");

    let loaded = AppSettings::load_or_default(&path).expect("recover semantic settings");
    let backups = regular_corrupt_settings_backups(&dir);

    assert_eq!(loaded, AppSettings::default());
    assert_eq!(backups.len(), 1);
    assert_eq!(std::fs::read(backups[0].path()).expect("backup"), raw);
    assert_eq!(std::fs::read(&path).expect("source remains"), raw);
}

#[test]
fn semantic_save_rejection_preserves_previous_file_without_staging() {
    let dir = temp_dir("semantic-over-limit-save");
    let path = dir.join("settings.json");
    let previous = br#"{"default_start_page":"mock.node:/previous"}"#;
    std::fs::write(&path, previous).expect("previous settings");
    let settings = AppSettings {
        conversation_tabs: vec![ConversationTabSettings {
            attachments: vec![
                PathBuf::from("attachment");
                APP_SETTINGS_MAX_ATTACHMENTS_PER_CONVERSATION + 1
            ],
            ..ConversationTabSettings::default()
        }],
        ..AppSettings::default()
    };

    let error = settings
        .save(&path)
        .expect_err("semantic rejection must precede staging");

    assert!(error.to_string().contains("conversation attachments"));
    assert_eq!(std::fs::read(&path).expect("previous settings"), previous);
    assert_eq!(
        std::fs::read_dir(&dir)
            .expect("settings root")
            .filter_map(Result::ok)
            .count(),
        1
    );
}

#[test]
fn structural_save_rejection_preserves_previous_file_without_staging() {
    let dir = temp_dir("structural-over-limit-save");
    let path = dir.join("settings.json");
    let previous = br#"{"default_start_page":"mock.node:/previous"}"#;
    std::fs::write(&path, previous).expect("previous settings");
    let mut settings = AppSettings::default();
    settings.extra.insert(
        "future".into(),
        serde_json::Value::String("x".repeat(APP_SETTINGS_JSON_MAX_RAW_STRING_BYTES + 1)),
    );

    let error = settings
        .save(&path)
        .expect_err("structural rejection must precede staging");

    assert!(error.to_string().contains("structural admission limits"));
    assert_eq!(std::fs::read(&path).expect("previous settings"), previous);
    assert_eq!(
        std::fs::read_dir(&dir)
            .expect("settings root")
            .filter_map(Result::ok)
            .count(),
        1
    );
}

#[test]
fn structurally_excessive_valid_json_is_backed_up_before_default_recovery() {
    let dir = temp_dir("structural-over-limit-load");
    let path = dir.join("settings.json");
    let mut raw = Vec::with_capacity(APP_SETTINGS_JSON_MAX_RAW_STRING_BYTES + 32);
    raw.extend_from_slice(br#"{"future":""#);
    raw.resize(raw.len() + APP_SETTINGS_JSON_MAX_RAW_STRING_BYTES + 1, b'x');
    raw.extend_from_slice(br#""}"#);
    std::fs::write(&path, &raw).expect("structurally excessive valid JSON");

    let loaded = AppSettings::load_or_default(&path).expect("recover structural settings");
    let backups = regular_corrupt_settings_backups(&dir);

    assert_eq!(loaded, AppSettings::default());
    assert_eq!(backups.len(), 1);
    assert_eq!(std::fs::read(backups[0].path()).expect("backup"), raw);
    assert_eq!(std::fs::read(&path).expect("source remains"), raw);
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
    assert!(!settings.ui.reduce_motion);
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
fn reduced_motion_preference_round_trips_without_changing_legacy_defaults() {
    let dir = temp_dir("reduced-motion");
    let path = dir.join("settings.json");
    let mut settings = AppSettings::default();
    settings.ui.reduce_motion = true;

    settings.save(&path).expect("save reduced-motion setting");
    let loaded = AppSettings::load_or_default(&path).expect("load reduced-motion setting");

    assert!(loaded.ui.reduce_motion);
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
    let corrupt = b"{not json\xff";
    std::fs::write(&path, corrupt).expect("write corrupt settings");

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
        .collect::<Vec<_>>();

    assert_eq!(settings, AppSettings::default());
    assert_eq!(backups.len(), 1);
    assert_eq!(std::fs::read(&path).expect("source remains"), corrupt);
    assert_eq!(
        std::fs::read(backups[0].path()).expect("corrupt backup"),
        corrupt,
        "backup must contain the exact admitted bytes"
    );
    assert_eq!(
        std::fs::read_dir(&dir)
            .expect("read temp dir")
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().ends_with(".tmp"))
            .count(),
        0
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        assert_eq!(
            backups[0]
                .metadata()
                .expect("backup metadata")
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
}

#[test]
fn corrupt_settings_backup_does_not_overwrite_legacy_name_collision() {
    let dir = temp_dir("corrupt-collision");
    let path = dir.join("settings.json");
    let collision = dir.join("settings.json.corrupt.0.bak");
    let sentinel = b"existing operator backup";
    std::fs::write(&collision, sentinel).expect("legacy backup collision");
    std::fs::write(&path, b"{malformed").expect("corrupt settings");

    let settings = AppSettings::load_or_default(&path).expect("recover corrupt settings");

    assert_eq!(settings, AppSettings::default());
    assert_eq!(
        std::fs::read(&collision).expect("legacy collision"),
        sentinel
    );
    assert_eq!(
        std::fs::read_dir(&dir)
            .expect("settings root")
            .filter_map(Result::ok)
            .filter(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with("settings.json.corrupt.")
            })
            .count(),
        2
    );
}

#[test]
fn corrupt_settings_backup_retention_keeps_the_newest_bounded_set() {
    let dir = temp_dir("corrupt-retention-count");
    let path = dir.join("settings.json");
    for index in 0..7 {
        std::fs::write(&path, format!("{{malformed-{index}"))
            .expect("write changing malformed settings");
        AppSettings::load_or_default(&path).expect("recover malformed settings");
    }

    let backups = regular_corrupt_settings_backups(&dir);
    let contents = backups
        .iter()
        .map(|entry| std::fs::read_to_string(entry.path()).expect("backup contents"))
        .collect::<std::collections::BTreeSet<_>>();

    assert_eq!(backups.len(), APP_SETTINGS_CORRUPT_BACKUP_MAX_FILES);
    assert_eq!(
        contents,
        (3..7)
            .map(|index| format!("{{malformed-{index}"))
            .collect::<std::collections::BTreeSet<_>>()
    );
}

#[test]
fn corrupt_settings_backup_retention_enforces_aggregate_sparse_bytes() {
    let dir = temp_dir("corrupt-retention-bytes");
    let path = dir.join("settings.json");
    for index in 0..=APP_SETTINGS_CORRUPT_BACKUP_MAX_FILES {
        let backup = dir.join(format!("settings.json.corrupt.{index}.bak"));
        let file = std::fs::File::create(backup).expect("sparse legacy backup");
        file.set_len(APP_SETTINGS_MAX_BYTES)
            .expect("extend sparse legacy backup");
    }
    std::fs::write(&path, b"{malformed-current").expect("malformed settings");

    AppSettings::load_or_default(&path).expect("recover and prune backups");

    let backups = regular_corrupt_settings_backups(&dir);
    let total_bytes = backups.iter().fold(0u64, |total, entry| {
        total.saturating_add(entry.metadata().expect("backup metadata").len())
    });
    assert!(backups.len() <= APP_SETTINGS_CORRUPT_BACKUP_MAX_FILES);
    assert!(total_bytes <= APP_SETTINGS_CORRUPT_BACKUP_MAX_TOTAL_BYTES);
    assert!(backups.iter().any(|entry| {
        entry
            .metadata()
            .is_ok_and(|metadata| metadata.len() == b"{malformed-current".len() as u64)
    }));
}

#[test]
fn corrupt_settings_backup_refuses_directory_scan_saturation_before_publication() {
    let dir = temp_dir("corrupt-retention-scan");
    let path = dir.join("settings.json");
    std::fs::write(&path, b"{malformed").expect("malformed settings");
    for index in 0..APP_SETTINGS_CORRUPT_BACKUP_MAX_SCAN_ENTRIES {
        std::fs::write(dir.join(format!("unrelated-{index:04}")), b"").expect("unrelated entry");
    }

    let error = AppSettings::load_or_default(&path)
        .expect_err("saturated directory scan must fail before backup publication");

    assert!(error.to_string().contains("entry scan limit"));
    assert!(regular_corrupt_settings_backups(&dir).is_empty());
    assert_eq!(
        std::fs::read(&path).expect("source settings"),
        b"{malformed"
    );
    std::fs::remove_dir_all(dir).expect("remove saturated fixture");
}

#[cfg(unix)]
#[test]
fn corrupt_settings_backup_retention_does_not_follow_matching_symlink() {
    use std::os::unix::fs::symlink;

    let dir = temp_dir("corrupt-retention-symlink");
    let path = dir.join("settings.json");
    let outside = dir.join("outside.bin");
    let outside_bytes = b"operator data outside retention";
    std::fs::write(&outside, outside_bytes).expect("outside data");
    let linked = dir.join("settings.json.corrupt.1.bak");
    symlink(&outside, &linked).expect("matching backup symlink");
    for index in 0..6 {
        std::fs::write(&path, format!("{{malformed-symlink-{index}")).expect("malformed settings");
        AppSettings::load_or_default(&path).expect("recover settings");
    }

    assert!(linked
        .symlink_metadata()
        .expect("linked metadata")
        .file_type()
        .is_symlink());
    assert_eq!(
        std::fs::read(&outside).expect("outside data"),
        outside_bytes
    );
    assert_eq!(
        regular_corrupt_settings_backups(&dir).len(),
        APP_SETTINGS_CORRUPT_BACKUP_MAX_FILES
    );
}

#[test]
fn settings_loader_accepts_the_exact_file_byte_limit() {
    let dir = temp_dir("exact-byte-limit");
    let path = dir.join("settings.json");
    let json = br#"{"default_start_page":"mock.node:/exact"}"#;
    let mut payload = vec![b' '; APP_SETTINGS_MAX_BYTES as usize];
    payload[..json.len()].copy_from_slice(json);
    std::fs::write(&path, payload).expect("write exact-limit settings");

    let settings = AppSettings::load_or_default(&path).expect("load exact-limit settings");

    assert_eq!(settings.default_start_page, "mock.node:/exact");
}

#[test]
fn oversized_sparse_settings_are_rejected_before_read_or_backup() {
    let dir = temp_dir("oversized-sparse");
    let path = dir.join("settings.json");
    let file = std::fs::File::create(&path).expect("create sparse settings");
    file.set_len(APP_SETTINGS_MAX_BYTES + 1)
        .expect("extend sparse settings");
    drop(file);

    let error = AppSettings::load_or_default(&path)
        .expect_err("oversized settings must fail before JSON parsing");

    assert!(error.to_string().contains("settings file exceeds"));
    assert_eq!(
        std::fs::symlink_metadata(&path)
            .expect("settings metadata")
            .len(),
        APP_SETTINGS_MAX_BYTES + 1
    );
    assert_eq!(
        std::fs::read_dir(&dir)
            .expect("read settings root")
            .filter_map(Result::ok)
            .count(),
        1,
        "an oversized file must not be copied into an equally oversized backup"
    );
}

#[test]
fn settings_loader_refuses_non_regular_paths() {
    let dir = temp_dir("non-regular");
    let path = dir.join("settings.json");
    std::fs::create_dir(&path).expect("create settings directory");

    let error = AppSettings::load_or_default(&path)
        .expect_err("settings directory must not be treated as corrupt JSON");

    assert!(error.to_string().contains("must be a regular file"));
    assert!(path.is_dir());
}

#[cfg(unix)]
#[test]
fn settings_loader_does_not_follow_valid_or_broken_symlinks() {
    use std::os::unix::fs::symlink;

    let dir = temp_dir("symlink");
    let outside = dir.join("outside.json");
    let outside_bytes = br#"{"default_start_page":"mock.node:/outside"}"#;
    std::fs::write(&outside, outside_bytes).expect("write outside settings");
    let linked = dir.join("settings.json");
    symlink(&outside, &linked).expect("create settings symlink");

    let error =
        AppSettings::load_or_default(&linked).expect_err("settings symlink must not be followed");
    assert!(error.to_string().contains("must be a regular file"));
    assert_eq!(
        std::fs::read(&outside).expect("read outside settings"),
        outside_bytes
    );
    assert!(linked
        .symlink_metadata()
        .expect("link metadata")
        .file_type()
        .is_symlink());

    let broken = dir.join("broken-settings.json");
    symlink(dir.join("missing.json"), &broken).expect("create broken settings symlink");
    let error = AppSettings::load_or_default(&broken)
        .expect_err("broken settings symlink must not be treated as missing");
    assert!(error.to_string().contains("must be a regular file"));
}

#[test]
fn oversized_settings_save_preserves_existing_file_without_staging() {
    let dir = temp_dir("oversized-save");
    let path = dir.join("settings.json");
    let existing = br#"{"default_start_page":"mock.node:/existing"}"#;
    std::fs::write(&path, existing).expect("write existing settings");
    let mut settings = AppSettings::default();
    settings.extra.insert(
        "oversized".into(),
        serde_json::Value::String("x".repeat(APP_SETTINGS_MAX_BYTES as usize)),
    );

    let error = settings
        .save(&path)
        .expect_err("save must reject a payload the loader cannot admit");

    assert!(error.to_string().contains("settings payload exceeds"));
    assert_eq!(std::fs::read(&path).expect("preserved settings"), existing);
    assert_eq!(
        std::fs::read_dir(&dir)
            .expect("read settings root")
            .filter_map(Result::ok)
            .count(),
        1,
        "payload rejection must happen before a temporary file is created"
    );
}

#[test]
fn settings_save_uses_unique_staging_and_leaves_former_temp_collision_untouched() {
    let dir = temp_dir("unique-save-staging");
    let path = dir.join("settings.json");
    let former_temp = dir.join(format!("settings.json.tmp.{}", std::process::id()));
    let sentinel = b"former predictable staging path";
    std::fs::write(&former_temp, sentinel).expect("former staging collision");
    let settings = AppSettings {
        default_start_page: "mock.node:/unique-stage".into(),
        ..AppSettings::default()
    };

    settings.save(&path).expect("save through unique staging");

    assert_eq!(std::fs::read(&former_temp).expect("sentinel"), sentinel);
    assert_eq!(
        AppSettings::load_or_default(&path)
            .expect("load saved settings")
            .default_start_page,
        "mock.node:/unique-stage"
    );
    assert_eq!(
        std::fs::read_dir(&dir)
            .expect("settings root")
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().ends_with(".tmp"))
            .count(),
        0
    );
}

#[test]
fn settings_save_refuses_directory_target() {
    let dir = temp_dir("save-directory-target");
    let path = dir.join("settings.json");
    std::fs::create_dir(&path).expect("settings target directory");

    let error = AppSettings::default()
        .save(&path)
        .expect_err("directory target must be refused");

    assert!(error.to_string().contains("target must be a regular file"));
    assert!(path.is_dir());
}

#[cfg(unix)]
#[test]
fn settings_save_refuses_valid_and_broken_symlink_targets() {
    use std::os::unix::fs::symlink;

    let dir = temp_dir("save-symlink-target");
    let outside = dir.join("outside.json");
    let previous = b"outside settings";
    std::fs::write(&outside, previous).expect("outside settings");
    let linked = dir.join("settings.json");
    symlink(&outside, &linked).expect("settings target symlink");

    let error = AppSettings::default()
        .save(&linked)
        .expect_err("settings target symlink must be refused");
    assert!(error.to_string().contains("target must be a regular file"));
    assert_eq!(std::fs::read(&outside).expect("outside settings"), previous);
    assert!(linked
        .symlink_metadata()
        .expect("linked metadata")
        .file_type()
        .is_symlink());

    let broken = dir.join("broken-settings.json");
    symlink(dir.join("missing.json"), &broken).expect("broken settings target symlink");
    let error = AppSettings::default()
        .save(&broken)
        .expect_err("broken settings target symlink must be refused");
    assert!(error.to_string().contains("target must be a regular file"));
    assert!(broken
        .symlink_metadata()
        .expect("broken metadata")
        .file_type()
        .is_symlink());
}

#[cfg(unix)]
#[test]
fn settings_save_replacement_is_owner_only() {
    use std::os::unix::fs::PermissionsExt;

    let dir = temp_dir("save-owner-only");
    let path = dir.join("settings.json");
    std::fs::write(&path, b"{}").expect("existing settings");
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o666))
        .expect("relax existing mode");

    AppSettings::default()
        .save(&path)
        .expect("replace settings privately");

    assert_eq!(
        std::fs::symlink_metadata(&path)
            .expect("settings metadata")
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
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
