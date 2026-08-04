use std::collections::BTreeMap;
use std::fs::{File, OpenOptions};
use std::io::{ErrorKind, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::browser::page::{BROWSER_PAGE_TITLE_MAX_BYTES, BROWSER_PAGE_URL_MAX_BYTES};
use crate::browser::session::{BROWSER_HISTORY_MAX_ITEMS, BROWSER_HISTORY_MAX_OWNED_BYTES};
use crate::browser::Bookmark;
use crate::error::{AppError, AppResult};
use crate::messaging::DeliveryMode;
use crate::micron::parser::{
    MICRON_CONTROL_NAME_MAX_BYTES, MICRON_LINK_FIELDS_MAX_BYTES, MICRON_LINK_FIELD_MAX_BYTES,
    MICRON_LINK_MAX_FIELDS, MICRON_LINK_TARGET_MAX_BYTES,
};
use crate::storage::files::atomic_replace;

pub const APP_SETTINGS_MAX_BYTES: u64 = 8 * 1024 * 1024;
pub const APP_SETTINGS_CORRUPT_BACKUP_MAX_FILES: usize = 4;
pub const APP_SETTINGS_CORRUPT_BACKUP_MAX_TOTAL_BYTES: u64 = 32 * 1024 * 1024;
pub const APP_SETTINGS_CORRUPT_BACKUP_MAX_SCAN_ENTRIES: usize = 4096;
pub const APP_SETTINGS_MAX_BROWSER_TABS: usize = 128;
pub const APP_SETTINGS_MAX_CONVERSATION_TABS: usize = 128;
pub const APP_SETTINGS_MAX_BOOKMARKS: usize = 4096;
pub const APP_SETTINGS_MAX_DELETED_CONVERSATIONS: usize = 4096;
pub const APP_SETTINGS_MAX_WORKSPACE_PANES: usize = 256;
pub const APP_SETTINGS_MAX_WORKSPACE_LAYOUT_NODES: usize = APP_SETTINGS_MAX_WORKSPACE_PANES * 2 - 1;
pub const APP_SETTINGS_MAX_WORKSPACE_LAYOUT_DEPTH: usize = 32;
pub const APP_SETTINGS_MAX_PLUGIN_IDS: usize = 256;
pub const APP_SETTINGS_MAX_ATTACHMENTS_PER_CONVERSATION: usize = 64;
pub const APP_SETTINGS_MAX_EXTRA_FIELDS: usize = 256;
pub const APP_SETTINGS_MAX_EXTRA_VALUE_DEPTH: usize = 32;
pub const APP_SETTINGS_MAX_EXTRA_VALUE_NODES: usize = 16 * 1024;
pub const APP_SETTINGS_MAX_EXTRA_CONTAINER_ITEMS: usize = 4096;
pub const APP_SETTINGS_JSON_MAX_DEPTH: usize = 48;
pub const APP_SETTINGS_JSON_MAX_TOKENS: usize = 256 * 1024;
pub const APP_SETTINGS_JSON_MAX_CONTAINER_ITEMS: usize = 8192;
pub const APP_SETTINGS_JSON_MAX_RAW_STRING_BYTES: usize = 4 * 1024 * 1024;
static APP_SETTINGS_TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeBackendSetting {
    Auto,
    Mock,
    Reticulum,
    Bridge,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReticulumInstanceMode {
    Managed,
    External,
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BrowserOverlayPreference {
    Hidden,
    Focus,
    #[default]
    Status,
    Both,
    Expanded,
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceSectionPreference {
    #[default]
    Browser,
    Messages,
    Directory,
    Identities,
    Interfaces,
    Monitoring,
    NetworkDoctor,
    Settings,
    Diagnostics,
    Logs,
    Plugins,
    Help,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct UiPreferences {
    pub theme_name: String,
    pub font_size: u16,
    pub reduce_motion: bool,
    pub low_power_mode: bool,
    pub show_help: bool,
    pub browser_overlay_mode: BrowserOverlayPreference,
    pub active_workspace_section: WorkspaceSectionPreference,
    pub active_browser_index: usize,
    pub active_conversation_index: usize,
    pub sidebar_index: usize,
    pub desktop_workspace_panes: Vec<DesktopWorkspacePaneSettings>,
    pub active_desktop_workspace_pane: Option<usize>,
    pub desktop_workspace_layout: Option<DesktopWorkspaceLayoutNode>,
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DesktopWorkspacePaneKind {
    #[default]
    Browser,
    Conversation,
    OmenChat,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct DesktopWorkspacePaneSettings {
    pub kind: DesktopWorkspacePaneKind,
    pub index: usize,
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DesktopWorkspaceSplitAxis {
    Horizontal,
    #[default]
    Vertical,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DesktopWorkspaceLayoutNode {
    Pane {
        pane: DesktopWorkspacePaneSettings,
    },
    Split {
        axis: DesktopWorkspaceSplitAxis,
        ratio: f32,
        a: Box<DesktopWorkspaceLayoutNode>,
        b: Box<DesktopWorkspaceLayoutNode>,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct PluginSettings {
    pub remote_content_enabled: bool,
    pub enabled_plugin_ids: Vec<String>,
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SensitiveFormPersistence {
    #[default]
    Never,
    TrustedNodes,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct BrowserFormStateSettings {
    pub enabled: bool,
    pub max_age_secs: u64,
    pub sensitive_fields: SensitiveFormPersistence,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct LogSettings {
    pub max_file_bytes: u64,
    pub retain_files: usize,
    pub load_recent_entries: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct ClearwebPrivacySettings {
    pub prompt_external_browser: bool,
    pub preferred_external_browser_command: Option<String>,
    pub socks_proxy_enabled: bool,
    pub socks_proxy_host: String,
    pub socks_proxy_port: u16,
    pub remote_media_enabled: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct BrowserTabSettings {
    pub title: String,
    pub address_input: String,
    pub current_url: String,
    pub history: Vec<String>,
    pub history_index: isize,
    pub scroll_offset: usize,
    pub micron_zoom_percent: u16,
    pub focused_control: Option<BrowserFocusedControlSettings>,
    pub focused_link: Option<BrowserFocusedLinkSettings>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct BrowserFocusedControlSettings {
    pub name: String,
    pub index: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct BrowserFocusedLinkSettings {
    pub target: String,
    pub fields: Vec<String>,
    pub region_index: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct ConversationTabSettings {
    pub peer_hash: String,
    pub peer_label: String,
    pub draft_title: String,
    pub draft_body: String,
    pub attachments: Vec<PathBuf>,
    pub delivery_mode: DeliveryMode,
    pub include_ticket: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct DeletedConversationSettings {
    pub peer_hash: String,
    pub deleted_at: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct AppSettings {
    pub ui: UiPreferences,
    pub reticulum_config_path: Option<PathBuf>,
    pub identity_path: Option<PathBuf>,
    pub active_identity_label: Option<String>,
    pub default_start_page: String,
    pub runtime_backend: RuntimeBackendSetting,
    pub reticulum_instance_mode: ReticulumInstanceMode,
    pub announce_on_start: bool,
    pub periodic_lxmf_sync: bool,
    pub auto_sync_after_propagation_accept: bool,
    pub lxmf_sync_interval: u64,
    pub lxmf_sync_limit: u32,
    pub native_lxmf_sdk_rpc_endpoint: Option<String>,
    pub preferred_propagation_node_hash: Option<String>,
    pub diagnostics_target_address: Option<String>,
    pub diagnostics_target_kind: Option<String>,
    pub bookmarks: Vec<Bookmark>,
    pub trusted_plugin_ids: Vec<String>,
    pub browser_tabs: Vec<BrowserTabSettings>,
    pub conversation_tabs: Vec<ConversationTabSettings>,
    pub deleted_conversations: Vec<DeletedConversationSettings>,
    pub browser_form_state: BrowserFormStateSettings,
    pub logs: LogSettings,
    pub clearweb: ClearwebPrivacySettings,
    pub restart_required: bool,
    pub plugins: PluginSettings,
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

impl Default for UiPreferences {
    fn default() -> Self {
        Self {
            theme_name: "default".into(),
            font_size: 16,
            reduce_motion: false,
            low_power_mode: false,
            show_help: false,
            browser_overlay_mode: BrowserOverlayPreference::default(),
            active_workspace_section: WorkspaceSectionPreference::default(),
            active_browser_index: 0,
            active_conversation_index: 0,
            sidebar_index: 0,
            desktop_workspace_panes: Vec::new(),
            active_desktop_workspace_pane: None,
            desktop_workspace_layout: None,
        }
    }
}

impl Default for DesktopWorkspacePaneSettings {
    fn default() -> Self {
        Self {
            kind: DesktopWorkspacePaneKind::Browser,
            index: 0,
        }
    }
}

impl Default for PluginSettings {
    fn default() -> Self {
        Self {
            remote_content_enabled: false,
            enabled_plugin_ids: vec!["micronplus_textui".into()],
        }
    }
}

impl Default for BrowserFormStateSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            max_age_secs: 60 * 60 * 24 * 14,
            sensitive_fields: SensitiveFormPersistence::Never,
        }
    }
}

impl Default for LogSettings {
    fn default() -> Self {
        Self {
            max_file_bytes: 256 * 1024,
            retain_files: 4,
            load_recent_entries: 0,
        }
    }
}

impl Default for ClearwebPrivacySettings {
    fn default() -> Self {
        Self {
            prompt_external_browser: true,
            preferred_external_browser_command: None,
            socks_proxy_enabled: true,
            socks_proxy_host: "127.0.0.1".into(),
            socks_proxy_port: 9050,
            remote_media_enabled: false,
        }
    }
}

impl Default for BrowserTabSettings {
    fn default() -> Self {
        Self {
            title: "Mock Page".into(),
            address_input: "mock.page:/".into(),
            current_url: "mock.page:/".into(),
            history: Vec::new(),
            history_index: -1,
            scroll_offset: 0,
            micron_zoom_percent: 100,
            focused_control: None,
            focused_link: None,
        }
    }
}

impl Default for ConversationTabSettings {
    fn default() -> Self {
        Self {
            peer_hash: String::new(),
            peer_label: "New Conversation".into(),
            draft_title: String::new(),
            draft_body: String::new(),
            attachments: Vec::new(),
            delivery_mode: DeliveryMode::Direct,
            include_ticket: false,
        }
    }
}

impl Default for DeletedConversationSettings {
    fn default() -> Self {
        Self {
            peer_hash: String::new(),
            deleted_at: 0.0,
        }
    }
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            ui: UiPreferences::default(),
            reticulum_config_path: None,
            identity_path: None,
            active_identity_label: None,
            default_start_page: "mock.page:/".into(),
            runtime_backend: RuntimeBackendSetting::Auto,
            reticulum_instance_mode: ReticulumInstanceMode::Managed,
            announce_on_start: true,
            periodic_lxmf_sync: true,
            auto_sync_after_propagation_accept: false,
            lxmf_sync_interval: 360,
            lxmf_sync_limit: 8,
            native_lxmf_sdk_rpc_endpoint: None,
            preferred_propagation_node_hash: None,
            diagnostics_target_address: None,
            diagnostics_target_kind: None,
            bookmarks: Vec::new(),
            trusted_plugin_ids: Vec::new(),
            browser_tabs: Vec::new(),
            conversation_tabs: Vec::new(),
            deleted_conversations: Vec::new(),
            browser_form_state: BrowserFormStateSettings::default(),
            logs: LogSettings::default(),
            clearweb: ClearwebPrivacySettings::default(),
            restart_required: false,
            plugins: PluginSettings::default(),
            extra: BTreeMap::new(),
        }
    }
}

impl AppSettings {
    pub fn load_or_default(path: &Path) -> AppResult<Self> {
        let Some(raw) = read_bounded_settings_file(path)? else {
            return Ok(Self::default());
        };

        let parsed = validate_settings_json_structure(&raw)
            .and_then(|()| serde_json::from_slice::<Self>(&raw).map_err(|_| ()))
            .ok();
        match parsed {
            Some(settings) if settings.validate_retained().is_ok() => Ok(settings),
            Some(_) | None => {
                backup_corrupted_settings(path, &raw)?;
                Ok(Self::default())
            }
        }
    }

    /// Validate collections and recursive values before settings are retained
    /// by application state. Admission is all-or-nothing; callers must not
    /// partially restore a rejected value.
    pub fn validate_retained(&self) -> AppResult<()> {
        validate_count(
            "browser tabs",
            self.browser_tabs.len(),
            APP_SETTINGS_MAX_BROWSER_TABS,
        )?;
        validate_count(
            "conversation tabs",
            self.conversation_tabs.len(),
            APP_SETTINGS_MAX_CONVERSATION_TABS,
        )?;
        validate_count(
            "bookmarks",
            self.bookmarks.len(),
            APP_SETTINGS_MAX_BOOKMARKS,
        )?;
        validate_count(
            "deleted conversations",
            self.deleted_conversations.len(),
            APP_SETTINGS_MAX_DELETED_CONVERSATIONS,
        )?;
        validate_count(
            "workspace panes",
            self.ui.desktop_workspace_panes.len(),
            APP_SETTINGS_MAX_WORKSPACE_PANES,
        )?;
        validate_count(
            "trusted plugin IDs",
            self.trusted_plugin_ids.len(),
            APP_SETTINGS_MAX_PLUGIN_IDS,
        )?;
        validate_count(
            "enabled plugin IDs",
            self.plugins.enabled_plugin_ids.len(),
            APP_SETTINGS_MAX_PLUGIN_IDS,
        )?;
        validate_count(
            "extension fields",
            self.extra.len(),
            APP_SETTINGS_MAX_EXTRA_FIELDS,
        )?;

        for bookmark in &self.bookmarks {
            validate_bytes(
                "bookmark title",
                bookmark.title.len(),
                BROWSER_PAGE_TITLE_MAX_BYTES,
            )?;
            validate_bytes(
                "bookmark URL",
                bookmark.url.len(),
                BROWSER_PAGE_URL_MAX_BYTES,
            )?;
        }
        for tab in &self.browser_tabs {
            validate_bytes(
                "browser tab title",
                tab.title.len(),
                BROWSER_PAGE_TITLE_MAX_BYTES,
            )?;
            validate_bytes(
                "browser address input",
                tab.address_input.len(),
                BROWSER_PAGE_URL_MAX_BYTES,
            )?;
            validate_bytes(
                "browser current URL",
                tab.current_url.len(),
                BROWSER_PAGE_URL_MAX_BYTES,
            )?;
            validate_count(
                "browser history items",
                tab.history.len(),
                BROWSER_HISTORY_MAX_ITEMS,
            )?;
            let history_bytes = tab.history.iter().try_fold(0usize, |total, url| {
                validate_bytes("browser history URL", url.len(), BROWSER_PAGE_URL_MAX_BYTES)?;
                total.checked_add(url.len()).ok_or_else(|| {
                    AppError::Settings("browser history byte count overflowed".into())
                })
            })?;
            validate_bytes(
                "browser history",
                history_bytes,
                BROWSER_HISTORY_MAX_OWNED_BYTES,
            )?;
            if let Some(control) = &tab.focused_control {
                validate_bytes(
                    "focused control name",
                    control.name.len(),
                    MICRON_CONTROL_NAME_MAX_BYTES,
                )?;
            }
            if let Some(link) = &tab.focused_link {
                validate_bytes(
                    "focused link target",
                    link.target.len(),
                    MICRON_LINK_TARGET_MAX_BYTES,
                )?;
                validate_count(
                    "focused link fields",
                    link.fields.len(),
                    MICRON_LINK_MAX_FIELDS,
                )?;
                let field_bytes = link.fields.iter().try_fold(0usize, |total, field| {
                    validate_bytes(
                        "focused link field",
                        field.len(),
                        MICRON_LINK_FIELD_MAX_BYTES,
                    )?;
                    total.checked_add(field.len()).ok_or_else(|| {
                        AppError::Settings("focused link field byte count overflowed".into())
                    })
                })?;
                validate_bytes(
                    "focused link fields",
                    field_bytes,
                    MICRON_LINK_FIELDS_MAX_BYTES,
                )?;
            }
        }
        for conversation in &self.conversation_tabs {
            validate_count(
                "conversation attachments",
                conversation.attachments.len(),
                APP_SETTINGS_MAX_ATTACHMENTS_PER_CONVERSATION,
            )?;
        }
        if let Some(layout) = self.ui.desktop_workspace_layout.as_ref() {
            validate_workspace_layout(layout)?;
        }
        validate_extra_values(&self.extra)
    }

    #[allow(dead_code)]
    pub fn save(&self, path: &Path) -> AppResult<()> {
        self.validate_retained()?;
        let payload = serde_json::to_string_pretty(self)
            .map_err(|err| crate::error::AppError::Settings(err.to_string()))?;
        if payload.len().saturating_add(1) as u64 > APP_SETTINGS_MAX_BYTES {
            return Err(AppError::Settings(format!(
                "settings payload exceeds the {APP_SETTINGS_MAX_BYTES} byte limit"
            )));
        }
        validate_settings_json_structure(payload.as_bytes()).map_err(|()| {
            AppError::Settings("settings payload exceeds structural admission limits".into())
        })?;
        save_settings_payload(path, payload.as_bytes(), atomic_replace)
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SettingsJsonContainerKind {
    Array,
    Object,
}

#[derive(Clone, Copy)]
struct SettingsJsonContainer {
    kind: SettingsJsonContainerKind,
    separators: usize,
    has_content: bool,
}

const EMPTY_SETTINGS_JSON_CONTAINER: SettingsJsonContainer = SettingsJsonContainer {
    kind: SettingsJsonContainerKind::Array,
    separators: 0,
    has_content: false,
};

/// Allocation-free structural admission before Serde constructs owned strings,
/// vectors, maps, or recursive settings nodes. Full JSON grammar validation is
/// intentionally left to Serde after this resource preflight succeeds.
fn validate_settings_json_structure(raw: &[u8]) -> Result<(), ()> {
    let mut stack = [EMPTY_SETTINGS_JSON_CONTAINER; APP_SETTINGS_JSON_MAX_DEPTH];
    let mut depth = 0usize;
    let mut tokens = 0usize;
    let mut cursor = 0usize;

    while cursor < raw.len() {
        match raw[cursor] {
            byte if byte.is_ascii_whitespace() => cursor += 1,
            b'[' | b'{' => {
                count_settings_json_token(&mut tokens)?;
                mark_settings_json_content(&mut stack, depth);
                if depth == APP_SETTINGS_JSON_MAX_DEPTH {
                    return Err(());
                }
                stack[depth] = SettingsJsonContainer {
                    kind: if raw[cursor] == b'[' {
                        SettingsJsonContainerKind::Array
                    } else {
                        SettingsJsonContainerKind::Object
                    },
                    separators: 0,
                    has_content: false,
                };
                depth += 1;
                cursor += 1;
            }
            b']' | b'}' => {
                if depth == 0 {
                    return Err(());
                }
                let expected = if raw[cursor] == b']' {
                    SettingsJsonContainerKind::Array
                } else {
                    SettingsJsonContainerKind::Object
                };
                let frame = stack[depth - 1];
                if frame.kind != expected {
                    return Err(());
                }
                let items = usize::from(frame.has_content).saturating_add(frame.separators);
                if items > APP_SETTINGS_JSON_MAX_CONTAINER_ITEMS {
                    return Err(());
                }
                depth -= 1;
                cursor += 1;
            }
            b',' => {
                if depth == 0 {
                    return Err(());
                }
                let frame = &mut stack[depth - 1];
                frame.separators = frame.separators.saturating_add(1);
                if frame.separators >= APP_SETTINGS_JSON_MAX_CONTAINER_ITEMS {
                    return Err(());
                }
                cursor += 1;
            }
            b':' => cursor += 1,
            b'"' => {
                count_settings_json_token(&mut tokens)?;
                mark_settings_json_content(&mut stack, depth);
                cursor += 1;
                let start = cursor;
                let mut escaped = false;
                loop {
                    let Some(&byte) = raw.get(cursor) else {
                        return Err(());
                    };
                    if !escaped && byte == b'"' {
                        break;
                    }
                    if !escaped && byte < 0x20 {
                        return Err(());
                    }
                    escaped = !escaped && byte == b'\\';
                    if escaped && byte != b'\\' {
                        escaped = false;
                    }
                    cursor += 1;
                    if cursor.saturating_sub(start) > APP_SETTINGS_JSON_MAX_RAW_STRING_BYTES {
                        return Err(());
                    }
                }
                cursor += 1;
            }
            _ => {
                count_settings_json_token(&mut tokens)?;
                mark_settings_json_content(&mut stack, depth);
                cursor += 1;
                while cursor < raw.len()
                    && !raw[cursor].is_ascii_whitespace()
                    && !matches!(raw[cursor], b'[' | b']' | b'{' | b'}' | b',' | b':' | b'"')
                {
                    cursor += 1;
                }
            }
        }
    }

    if depth == 0 {
        Ok(())
    } else {
        Err(())
    }
}

fn count_settings_json_token(tokens: &mut usize) -> Result<(), ()> {
    *tokens = tokens.saturating_add(1);
    if *tokens > APP_SETTINGS_JSON_MAX_TOKENS {
        return Err(());
    }
    Ok(())
}

fn mark_settings_json_content(
    stack: &mut [SettingsJsonContainer; APP_SETTINGS_JSON_MAX_DEPTH],
    depth: usize,
) {
    if depth > 0 {
        stack[depth - 1].has_content = true;
    }
}

fn validate_count(name: &str, actual: usize, limit: usize) -> AppResult<()> {
    if actual > limit {
        return Err(AppError::Settings(format!(
            "settings {name} exceed the {limit} item limit"
        )));
    }
    Ok(())
}

fn validate_bytes(name: &str, actual: usize, limit: usize) -> AppResult<()> {
    if actual > limit {
        return Err(AppError::Settings(format!(
            "settings {name} exceed the {limit} byte limit"
        )));
    }
    Ok(())
}

fn validate_workspace_layout(root: &DesktopWorkspaceLayoutNode) -> AppResult<()> {
    let mut pending = vec![(root, 1usize)];
    let mut nodes = 0usize;
    while let Some((node, depth)) = pending.pop() {
        nodes = nodes.saturating_add(1);
        validate_count(
            "workspace layout nodes",
            nodes,
            APP_SETTINGS_MAX_WORKSPACE_LAYOUT_NODES,
        )?;
        validate_count(
            "workspace layout depth",
            depth,
            APP_SETTINGS_MAX_WORKSPACE_LAYOUT_DEPTH,
        )?;
        if let DesktopWorkspaceLayoutNode::Split { ratio, a, b, .. } = node {
            if !ratio.is_finite() {
                return Err(AppError::Settings(
                    "settings workspace split ratio must be finite".into(),
                ));
            }
            pending.push((b, depth.saturating_add(1)));
            pending.push((a, depth.saturating_add(1)));
        }
    }
    Ok(())
}

fn validate_extra_values(extra: &BTreeMap<String, serde_json::Value>) -> AppResult<()> {
    let mut pending = extra
        .values()
        .map(|value| (value, 1usize))
        .collect::<Vec<_>>();
    let mut nodes = 0usize;
    while let Some((value, depth)) = pending.pop() {
        nodes = nodes.saturating_add(1);
        validate_count(
            "extension value nodes",
            nodes,
            APP_SETTINGS_MAX_EXTRA_VALUE_NODES,
        )?;
        validate_count(
            "extension value depth",
            depth,
            APP_SETTINGS_MAX_EXTRA_VALUE_DEPTH,
        )?;
        match value {
            serde_json::Value::Array(values) => {
                validate_count(
                    "extension array items",
                    values.len(),
                    APP_SETTINGS_MAX_EXTRA_CONTAINER_ITEMS,
                )?;
                pending.extend(values.iter().map(|value| (value, depth.saturating_add(1))));
            }
            serde_json::Value::Object(values) => {
                validate_count(
                    "extension object items",
                    values.len(),
                    APP_SETTINGS_MAX_EXTRA_CONTAINER_ITEMS,
                )?;
                pending.extend(
                    values
                        .values()
                        .map(|value| (value, depth.saturating_add(1))),
                );
            }
            _ => {}
        }
    }
    Ok(())
}

fn save_settings_payload(
    path: &Path,
    payload: &[u8],
    replace: impl FnOnce(&Path, &Path) -> std::io::Result<()>,
) -> AppResult<()> {
    let parent = path
        .parent()
        .ok_or_else(|| AppError::Settings("settings path has no parent directory".into()))?;
    crate::private_fs::ensure_private_parent_dir(parent)?;
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if !metadata.file_type().is_file() => {
            return Err(AppError::Settings(format!(
                "settings target must be a regular file: {}",
                path.display()
            )));
        }
        Ok(_) => {}
        Err(error) if error.kind() == ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }

    let sequence = APP_SETTINGS_TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("settings.json");
    let temporary = parent.join(format!(
        ".{file_name}.{}.{}.{}.tmp",
        std::process::id(),
        sequence,
        timestamp
    ));
    let result = (|| -> std::io::Result<()> {
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temporary)?;
        file.write_all(payload)?;
        file.write_all(b"\n")?;
        file.flush()?;
        file.sync_all()?;
        drop(file);
        replace(&temporary, path)?;
        #[cfg(unix)]
        File::open(parent)?.sync_all()?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result.map_err(Into::into)
}

fn read_bounded_settings_file(path: &Path) -> AppResult<Option<Vec<u8>>> {
    let path_metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    if !path_metadata.file_type().is_file() {
        return Err(AppError::Settings(format!(
            "settings path must be a regular file: {}",
            path.display()
        )));
    }
    if path_metadata.len() > APP_SETTINGS_MAX_BYTES {
        return Err(AppError::Settings(format!(
            "settings file exceeds the {APP_SETTINGS_MAX_BYTES} byte limit: {}",
            path.display()
        )));
    }

    let file = File::open(path)?;
    let opened_metadata = file.metadata()?;
    if !opened_metadata.is_file() {
        return Err(AppError::Settings(format!(
            "settings path must open as a regular file: {}",
            path.display()
        )));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;

        if path_metadata.dev() != opened_metadata.dev()
            || path_metadata.ino() != opened_metadata.ino()
        {
            return Err(AppError::Settings(format!(
                "settings path changed while it was being opened: {}",
                path.display()
            )));
        }
    }

    let mut raw = Vec::with_capacity(path_metadata.len() as usize);
    file.take(APP_SETTINGS_MAX_BYTES.saturating_add(1))
        .read_to_end(&mut raw)?;
    if raw.len() as u64 > APP_SETTINGS_MAX_BYTES {
        return Err(AppError::Settings(format!(
            "settings file exceeds the {APP_SETTINGS_MAX_BYTES} byte limit: {}",
            path.display()
        )));
    }
    Ok(Some(raw))
}

fn backup_corrupted_settings(path: &Path, raw: &[u8]) -> AppResult<PathBuf> {
    prune_corrupt_settings_backups(path)?;
    let backup = backup_corrupted_settings_with_publish(path, raw, |source, destination| {
        publish_new_backup(source, destination)
    })?;
    prune_corrupt_settings_backups(path)?;
    Ok(backup)
}

fn publish_new_backup(source: &Path, destination: &Path) -> std::io::Result<()> {
    std::fs::hard_link(source, destination)?;
    std::fs::remove_file(source)
}

fn backup_corrupted_settings_with_publish(
    path: &Path,
    raw: &[u8],
    publish: impl FnOnce(&Path, &Path) -> std::io::Result<()>,
) -> AppResult<PathBuf> {
    let parent = path
        .parent()
        .ok_or_else(|| AppError::Settings("settings path has no parent directory".into()))?;
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let sequence = APP_SETTINGS_TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("settings.json");
    let backup = parent.join(format!(
        "{file_name}.corrupt.{timestamp}.{}.{}.bak",
        std::process::id(),
        sequence
    ));
    let temporary = parent.join(format!(
        ".{file_name}.corrupt.{timestamp}.{}.{}.tmp",
        std::process::id(),
        sequence
    ));
    let result = (|| -> std::io::Result<()> {
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temporary)?;
        file.write_all(raw)?;
        file.flush()?;
        file.sync_all()?;
        drop(file);
        publish(&temporary, &backup)?;
        #[cfg(unix)]
        File::open(parent)?.sync_all()?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result.map(|()| backup).map_err(Into::into)
}

struct CorruptSettingsBackupCandidate {
    path: PathBuf,
    bytes: u64,
    modified: SystemTime,
}

fn prune_corrupt_settings_backups(path: &Path) -> AppResult<()> {
    let parent = path
        .parent()
        .ok_or_else(|| AppError::Settings("settings path has no parent directory".into()))?;
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("settings.json");
    let mut candidates = Vec::new();
    for (scanned, entry) in std::fs::read_dir(parent)?.enumerate() {
        if scanned == APP_SETTINGS_CORRUPT_BACKUP_MAX_SCAN_ENTRIES {
            return Err(AppError::Settings(format!(
                "settings backup retention exceeds the {} entry scan limit: {}",
                APP_SETTINGS_CORRUPT_BACKUP_MAX_SCAN_ENTRIES,
                parent.display()
            )));
        }
        let entry = entry?;
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        if !is_corrupt_settings_backup_name(&name, file_name) {
            continue;
        }
        let file_type = entry.file_type()?;
        if !file_type.is_file() {
            continue;
        }
        let metadata = std::fs::symlink_metadata(entry.path())?;
        if !metadata.file_type().is_file() {
            continue;
        }
        candidates.push(CorruptSettingsBackupCandidate {
            path: entry.path(),
            bytes: metadata.len(),
            modified: metadata.modified().unwrap_or(UNIX_EPOCH),
        });
    }
    candidates.sort_by(|left, right| {
        right
            .modified
            .cmp(&left.modified)
            .then_with(|| right.path.cmp(&left.path))
    });

    let mut retained_files = 0usize;
    let mut retained_bytes = 0u64;
    #[cfg(unix)]
    let mut removed = false;
    for candidate in candidates {
        let candidate_total = retained_bytes.saturating_add(candidate.bytes);
        if retained_files < APP_SETTINGS_CORRUPT_BACKUP_MAX_FILES
            && candidate_total <= APP_SETTINGS_CORRUPT_BACKUP_MAX_TOTAL_BYTES
        {
            retained_files += 1;
            retained_bytes = candidate_total;
        } else {
            std::fs::remove_file(candidate.path)?;
            #[cfg(unix)]
            {
                removed = true;
            }
        }
    }
    #[cfg(unix)]
    if removed {
        File::open(parent)?.sync_all()?;
    }
    Ok(())
}

fn is_corrupt_settings_backup_name(candidate: &str, file_name: &str) -> bool {
    let prefix = format!("{file_name}.corrupt.");
    let Some(encoded) = candidate
        .strip_prefix(&prefix)
        .and_then(|name| name.strip_suffix(".bak"))
    else {
        return false;
    };
    let parts = encoded.split('.').collect::<Vec<_>>();
    matches!(parts.len(), 1 | 3)
        && parts
            .iter()
            .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn json_null_groups(groups: usize, items_per_group: usize) -> Vec<u8> {
        let group = format!("[{}]", vec!["null"; items_per_group].join(","));
        format!("[{}]", vec![group; groups].join(",")).into_bytes()
    }

    #[test]
    fn settings_json_preflight_accepts_exact_container_depth_and_string_limits() {
        let exact_container = format!(
            "[{}]",
            vec!["null"; APP_SETTINGS_JSON_MAX_CONTAINER_ITEMS].join(",")
        );
        validate_settings_json_structure(exact_container.as_bytes())
            .expect("exact container item limit");

        let exact_depth = format!(
            "{}null{}",
            "[".repeat(APP_SETTINGS_JSON_MAX_DEPTH),
            "]".repeat(APP_SETTINGS_JSON_MAX_DEPTH)
        );
        validate_settings_json_structure(exact_depth.as_bytes()).expect("exact JSON depth limit");

        let mut exact_string = Vec::with_capacity(APP_SETTINGS_JSON_MAX_RAW_STRING_BYTES + 2);
        exact_string.push(b'"');
        exact_string.resize(APP_SETTINGS_JSON_MAX_RAW_STRING_BYTES + 1, b'x');
        exact_string.push(b'"');
        validate_settings_json_structure(&exact_string).expect("exact raw string limit");
    }

    #[test]
    fn settings_json_preflight_rejects_next_container_depth_string_and_token_limits() {
        let oversized_container = format!(
            "[{}]",
            vec!["null"; APP_SETTINGS_JSON_MAX_CONTAINER_ITEMS + 1].join(",")
        );
        assert!(validate_settings_json_structure(oversized_container.as_bytes()).is_err());

        let oversized_depth = format!(
            "{}null{}",
            "[".repeat(APP_SETTINGS_JSON_MAX_DEPTH + 1),
            "]".repeat(APP_SETTINGS_JSON_MAX_DEPTH + 1)
        );
        assert!(validate_settings_json_structure(oversized_depth.as_bytes()).is_err());

        let mut oversized_string = Vec::with_capacity(APP_SETTINGS_JSON_MAX_RAW_STRING_BYTES + 3);
        oversized_string.push(b'"');
        oversized_string.resize(APP_SETTINGS_JSON_MAX_RAW_STRING_BYTES + 2, b'x');
        oversized_string.push(b'"');
        assert!(validate_settings_json_structure(&oversized_string).is_err());

        let below_token_limit = json_null_groups(31, APP_SETTINGS_JSON_MAX_CONTAINER_ITEMS);
        validate_settings_json_structure(&below_token_limit).expect("below total token limit");
        let above_token_limit = json_null_groups(33, APP_SETTINGS_JSON_MAX_CONTAINER_ITEMS);
        assert!(validate_settings_json_structure(&above_token_limit).is_err());
    }

    #[test]
    fn settings_json_preflight_rejects_unclosed_and_mismatched_containers() {
        assert!(validate_settings_json_structure(br#"{"future":[1,2}"#).is_err());
        assert!(validate_settings_json_structure(br#"{"future":[1,2}}"#).is_err());
        assert!(validate_settings_json_structure(br#"{"future":"unterminated}"#).is_err());
    }

    #[test]
    fn settings_replace_failure_preserves_previous_file_and_cleans_staging() {
        let root = std::env::temp_dir().join(format!(
            "omenbrowser-rs-settings-replace-fault-{}-{}",
            std::process::id(),
            APP_SETTINGS_TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("settings root");
        let path = root.join("settings.json");
        let previous = b"{\"default_start_page\":\"mock.node:/previous\"}\n";
        std::fs::write(&path, previous).expect("previous settings");

        let error = save_settings_payload(&path, b"{}", |_source, _destination| {
            Err(std::io::Error::other(
                "injected settings replacement failure",
            ))
        })
        .expect_err("replacement must fail");

        assert!(error
            .to_string()
            .contains("injected settings replacement failure"));
        assert_eq!(std::fs::read(&path).expect("previous settings"), previous);
        assert_eq!(
            std::fs::read_dir(&root)
                .expect("settings root")
                .filter_map(Result::ok)
                .filter(|entry| entry.file_name().to_string_lossy().ends_with(".tmp"))
                .count(),
            0
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn corrupt_backup_publish_failure_uses_admitted_bytes_and_cleans_staging() {
        let root = std::env::temp_dir().join(format!(
            "omenbrowser-rs-settings-backup-fault-{}-{}",
            std::process::id(),
            APP_SETTINGS_TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("settings root");
        let path = root.join("settings.json");
        let admitted = b"{malformed admitted bytes";
        let source_after_read = b"replacement source bytes";
        std::fs::write(&path, source_after_read).expect("replacement source");

        let error =
            backup_corrupted_settings_with_publish(&path, admitted, |source, _destination| {
                assert_eq!(std::fs::read(source).expect("staged backup"), admitted);
                Err(std::io::Error::other("injected backup publish failure"))
            })
            .expect_err("backup publication must fail");

        assert!(error
            .to_string()
            .contains("injected backup publish failure"));
        assert_eq!(
            std::fs::read(&path).expect("replacement source"),
            source_after_read
        );
        assert_eq!(
            std::fs::read_dir(&root)
                .expect("settings root")
                .filter_map(Result::ok)
                .count(),
            1,
            "failed backup publication must leave only the source path"
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn corrupt_backup_publication_never_replaces_a_destination_collision() {
        let root = std::env::temp_dir().join(format!(
            "omenbrowser-rs-settings-backup-collision-{}-{}",
            std::process::id(),
            APP_SETTINGS_TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("settings root");
        let path = root.join("settings.json");
        let sentinel = b"existing operator backup";

        let error =
            backup_corrupted_settings_with_publish(&path, b"{malformed", |source, destination| {
                std::fs::write(destination, sentinel).expect("destination collision");
                publish_new_backup(source, destination)
            })
            .expect_err("existing destination must not be replaced");

        assert!(matches!(
            error,
            AppError::Io(ref io_error) if io_error.kind() == ErrorKind::AlreadyExists
        ));
        let entries = std::fs::read_dir(&root)
            .expect("settings root")
            .filter_map(Result::ok)
            .collect::<Vec<_>>();
        assert_eq!(entries.len(), 1);
        assert_eq!(
            std::fs::read(entries[0].path()).expect("collision"),
            sentinel
        );
        let _ = std::fs::remove_dir_all(root);
    }
}
