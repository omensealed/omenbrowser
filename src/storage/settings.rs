use std::collections::BTreeMap;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::browser::Bookmark;
use crate::error::AppResult;
use crate::messaging::DeliveryMode;

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
        if !path.exists() {
            return Ok(Self::default());
        }

        match std::fs::read_to_string(path)
            .ok()
            .and_then(|raw| serde_json::from_str::<Self>(&raw).ok())
        {
            Some(settings) => Ok(settings),
            None => {
                backup_corrupted_settings(path)?;
                Ok(Self::default())
            }
        }
    }

    #[allow(dead_code)]
    pub fn save(&self, path: &Path) -> AppResult<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let temp_path = path.with_file_name(format!(
            "{}.tmp.{}",
            path.file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("settings.json"),
            std::process::id()
        ));
        let payload = serde_json::to_string_pretty(self)
            .map_err(|err| crate::error::AppError::Settings(err.to_string()))?;
        {
            let mut file = OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(&temp_path)?;
            file.write_all(payload.as_bytes())?;
            file.write_all(b"\n")?;
            file.sync_all()?;
        }
        std::fs::rename(temp_path, path)?;
        Ok(())
    }
}

fn backup_corrupted_settings(path: &Path) -> AppResult<()> {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let backup = path.with_file_name(format!(
        "{}.corrupt.{}.bak",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("settings.json"),
        timestamp
    ));
    std::fs::copy(path, backup)?;
    Ok(())
}
