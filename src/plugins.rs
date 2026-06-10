use std::collections::{BTreeMap, BTreeSet};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};

pub const PLUGIN_UNSAFE_INSTALL_WARNING: &str = "Third-party plugins may execute code in a future plugin runtime. Only install plugins you trust.";
pub const BUILTIN_MICRONPLUS_PLUGIN_ID: &str = "micronplus_textui";
pub const BUILTIN_OMENCHAT_PLUGIN_ID: &str = "omenchat_lxmf";

#[allow(unused_imports)]
pub mod micronplus {
    pub use crate::browser::micronplus::{
        apply_micronplus_layout_partial, apply_micronplus_tree_partial, extract_micronplus_layout,
        extract_micronplus_widget_events, has_micronplus_markup, lower_micronplus_markup,
        micronplus_control_binding_for_field, micronplus_event_from_target, parse_micronplus_tree,
        render_column_group_rows_with_widgets,
        render_column_group_rows_with_widgets_and_field_cursor,
        render_column_group_rows_with_widgets_fields_and_cursor,
        render_micronplus_rows_with_widgets, render_micronplus_rows_with_widgets_and_field_cursor,
        render_micronplus_tree_rows_with_widgets,
        render_micronplus_tree_rows_with_widgets_and_field_cursor, widget_event_from_control_event,
        MicronPlusControlEvent, MicronPlusLayout, MicronPlusWidgetEvent, MicronPlusWidgetStore,
        MicronPlusWidgetTree,
    };

    #[cfg(test)]
    pub use crate::browser::micronplus::MicronPlusWidgetItem;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PluginPermission {
    BrowserTransformContent,
    BrowserEnrichRequestData,
    BrowserRenderRows,
    BrowserHandleInteraction,
    RuntimeReadStatus,
    RuntimeRequestPath,
    MessagesCompose,
    FilesystemReadUserSelected,
    FilesystemWritePluginData,
    NetworkExternal,
    Unknown(String),
}

impl PluginPermission {
    fn as_str(&self) -> &str {
        match self {
            Self::BrowserTransformContent => "browser:transform_content",
            Self::BrowserEnrichRequestData => "browser:enrich_request_data",
            Self::BrowserRenderRows => "browser:render_rows",
            Self::BrowserHandleInteraction => "browser:handle_interaction",
            Self::RuntimeReadStatus => "runtime:read_status",
            Self::RuntimeRequestPath => "runtime:request_path",
            Self::MessagesCompose => "messages:compose",
            Self::FilesystemReadUserSelected => "filesystem:read_user_selected",
            Self::FilesystemWritePluginData => "filesystem:write_plugin_data",
            Self::NetworkExternal => "network:external",
            Self::Unknown(value) => value.as_str(),
        }
    }

    fn from_manifest_str(value: &str) -> Self {
        match value {
            "browser:transform_content" | "transform_content" | "transform_document" => {
                Self::BrowserTransformContent
            }
            "browser:enrich_request_data" | "augment_request_data" => {
                Self::BrowserEnrichRequestData
            }
            "browser:render_rows" | "render_browser_rows" => Self::BrowserRenderRows,
            "browser:handle_interaction" | "handle_browser_interactions" | "setup_browser_page" => {
                Self::BrowserHandleInteraction
            }
            "runtime:read_status" => Self::RuntimeReadStatus,
            "runtime:request_path" => Self::RuntimeRequestPath,
            "messages:compose" => Self::MessagesCompose,
            "filesystem:read_user_selected" => Self::FilesystemReadUserSelected,
            "filesystem:write_plugin_data" => Self::FilesystemWritePluginData,
            "network:external" => Self::NetworkExternal,
            other => Self::Unknown(other.into()),
        }
    }
}

impl Serialize for PluginPermission {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for PluginPermission {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Ok(Self::from_manifest_str(&value))
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct PluginManifest {
    #[serde(alias = "id")]
    pub plugin_id: String,
    pub name: String,
    pub version: String,
    pub author: String,
    pub description: String,
    pub entrypoint: String,
    #[serde(default = "default_min_app_version")]
    pub min_app_version: String,
    pub permissions: Vec<PluginPermission>,
}

impl PluginManifest {
    pub fn builtin(
        plugin_id: impl Into<String>,
        name: impl Into<String>,
        description: impl Into<String>,
    ) -> Self {
        Self {
            plugin_id: plugin_id.into(),
            name: name.into(),
            version: "0.1.0".into(),
            author: "OMENbrowser_rs".into(),
            description: description.into(),
            entrypoint: "builtin".into(),
            min_app_version: "0.1.0".into(),
            permissions: Vec::new(),
        }
    }

    pub fn builtin_micronplus() -> Self {
        Self {
            permissions: vec![
                PluginPermission::BrowserTransformContent,
                PluginPermission::BrowserRenderRows,
                PluginPermission::BrowserHandleInteraction,
            ],
            ..Self::builtin(
                BUILTIN_MICRONPLUS_PLUGIN_ID,
                "MicronPlus Text UI",
                "Built-in MicronPlus transform, gated by trusted node status",
            )
        }
    }

    pub fn builtin_omenchat() -> Self {
        Self {
            permissions: vec![
                PluginPermission::RuntimeReadStatus,
                PluginPermission::RuntimeRequestPath,
                PluginPermission::MessagesCompose,
                PluginPermission::FilesystemWritePluginData,
            ],
            ..Self::builtin(
                BUILTIN_OMENCHAT_PLUGIN_ID,
                "OMENchat LXMF",
                "Built-in LXMF chat client plugin scaffold for room chat, rich media, and upload-aware server integration",
            )
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstalledPlugin {
    pub manifest: PluginManifest,
    pub path: Option<PathBuf>,
    pub builtin: bool,
    pub enabled: bool,
    pub trusted: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PluginDiscovery {
    pub plugins: Vec<InstalledPlugin>,
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct PluginRegistryEntry {
    pub enabled: bool,
    pub trusted: bool,
    pub installed_path: String,
    pub source_path: Option<String>,
    pub installed_at_epoch_secs: u64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct PluginRegistryFile {
    pub plugins: BTreeMap<String, PluginRegistryEntry>,
}

#[derive(Clone, Debug)]
pub struct PluginRegistry {
    plugins_dir: PathBuf,
    registry_path: PathBuf,
}

impl PluginRegistry {
    pub fn new(plugins_dir: PathBuf) -> Self {
        let registry_path = plugins_dir.join("registry.json");
        Self {
            plugins_dir,
            registry_path,
        }
    }

    pub fn discover(&self, enabled_plugin_ids: &[String]) -> AppResult<PluginDiscovery> {
        self.discover_with_builtins(
            vec![
                PluginManifest::builtin_micronplus(),
                PluginManifest::builtin_omenchat(),
            ],
            enabled_plugin_ids,
        )
    }

    pub fn discover_with_builtins(
        &self,
        builtins: Vec<PluginManifest>,
        enabled_plugin_ids: &[String],
    ) -> AppResult<PluginDiscovery> {
        std::fs::create_dir_all(&self.plugins_dir)?;
        let mut registry = self.load_registry()?;
        let mut registry_changed = false;
        let enabled = enabled_plugin_ids
            .iter()
            .map(|id| normalize_plugin_id(id))
            .collect::<BTreeSet<_>>();
        let mut seen = BTreeSet::new();
        let mut report = PluginDiscovery::default();

        for mut manifest in builtins {
            manifest.plugin_id = normalize_plugin_id(&manifest.plugin_id);
            let plugin_id = manifest.plugin_id.clone();
            seen.insert(plugin_id.clone());
            report.plugins.push(InstalledPlugin {
                manifest,
                path: None,
                builtin: true,
                enabled: enabled.contains(&plugin_id),
                trusted: true,
            });
        }

        let mut entries = std::fs::read_dir(&self.plugins_dir)?.collect::<Result<Vec<_>, _>>()?;
        entries.sort_by_key(|entry| entry.path());
        for entry in entries {
            let path = entry.path();
            if !path.is_dir()
                || path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.starts_with('.'))
            {
                continue;
            }
            let manifest_path = path.join("plugin.json");
            if !manifest_path.exists() {
                continue;
            }
            match load_manifest(&manifest_path) {
                Ok(mut manifest) => {
                    manifest.plugin_id = normalize_plugin_id(&manifest.plugin_id);
                    if !is_safe_plugin_id(&manifest.plugin_id) {
                        report
                            .warnings
                            .push(format!("ignored unsafe plugin id: {}", manifest.plugin_id));
                        continue;
                    }
                    if !seen.insert(manifest.plugin_id.clone()) {
                        report.warnings.push(format!(
                            "ignored duplicate plugin manifest: {}",
                            manifest.plugin_id
                        ));
                        continue;
                    }
                    let enabled = enabled.contains(&manifest.plugin_id);
                    let metadata = registry
                        .plugins
                        .entry(manifest.plugin_id.clone())
                        .or_insert_with(|| {
                            registry_changed = true;
                            PluginRegistryEntry::new(false, false, path.clone(), None)
                        });
                    report.plugins.push(InstalledPlugin {
                        manifest,
                        path: Some(path.clone()),
                        builtin: false,
                        enabled,
                        trusted: metadata.trusted,
                    });
                }
                Err(error) => report.warnings.push(format!(
                    "ignored invalid plugin manifest {}: {error}",
                    manifest_path.display()
                )),
            }
        }
        if registry_changed {
            self.save_registry(&registry)?;
        }

        Ok(report)
    }

    pub fn load_registry(&self) -> AppResult<PluginRegistryFile> {
        if !self.registry_path.exists() {
            return Ok(PluginRegistryFile::default());
        }
        let text = std::fs::read_to_string(&self.registry_path)?;
        serde_json::from_str(&text)
            .map_err(|error| AppError::Settings(format!("plugin registry error: {error}")))
    }

    pub fn save_registry(&self, registry: &PluginRegistryFile) -> AppResult<()> {
        std::fs::create_dir_all(&self.plugins_dir)?;
        let temp_path = self
            .registry_path
            .with_file_name(format!("registry.json.tmp.{}", std::process::id()));
        let payload = serde_json::to_string_pretty(registry)
            .map_err(|error| AppError::Settings(format!("plugin registry error: {error}")))?;
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
        std::fs::rename(temp_path, &self.registry_path)?;
        Ok(())
    }

    pub fn install_from_folder(
        &self,
        source: &Path,
        confirm_unsafe: bool,
    ) -> AppResult<InstalledPlugin> {
        if !confirm_unsafe {
            return Err(AppError::Unsupported(PLUGIN_UNSAFE_INSTALL_WARNING.into()));
        }
        if !source.is_dir() {
            return Err(AppError::Settings(format!(
                "plugin source is not a directory: {}",
                source.display()
            )));
        }
        let mut manifest = load_manifest(&source.join("plugin.json"))?;
        manifest.plugin_id = normalize_plugin_id(&manifest.plugin_id);
        if !is_safe_plugin_id(&manifest.plugin_id) {
            return Err(AppError::Settings(format!(
                "unsafe plugin id: {}",
                manifest.plugin_id
            )));
        }
        let target = self.plugins_dir.join(&manifest.plugin_id);
        if target.exists() {
            return Err(AppError::Settings(format!(
                "plugin already exists: {}",
                manifest.plugin_id
            )));
        }
        copy_dir_all(source, &target)?;
        let mut registry = self.load_registry()?;
        registry.plugins.insert(
            manifest.plugin_id.clone(),
            PluginRegistryEntry::new(true, true, target.clone(), Some(source.to_path_buf())),
        );
        self.save_registry(&registry)?;
        Ok(InstalledPlugin {
            manifest,
            path: Some(target),
            builtin: false,
            enabled: true,
            trusted: true,
        })
    }

    pub fn remove_installed(&self, plugin_id: &str) -> AppResult<bool> {
        let plugin_id = normalize_plugin_id(plugin_id);
        if !is_safe_plugin_id(&plugin_id) {
            return Err(AppError::Settings(format!("unsafe plugin id: {plugin_id}")));
        }
        if plugin_id == BUILTIN_MICRONPLUS_PLUGIN_ID {
            return Err(AppError::Unsupported(
                "built-in plugins cannot be removed".into(),
            ));
        }
        let mut removed = false;
        let target = self.plugins_dir.join(&plugin_id);
        if target.exists() {
            std::fs::remove_dir_all(&target)?;
            removed = true;
        }
        let mut registry = self.load_registry()?;
        removed |= registry.plugins.remove(&plugin_id).is_some();
        self.save_registry(&registry)?;
        Ok(removed)
    }

    pub fn sync_enabled_metadata(&self, enabled_plugin_ids: &[String]) -> AppResult<()> {
        let enabled = enabled_plugin_ids
            .iter()
            .map(|id| normalize_plugin_id(id))
            .collect::<BTreeSet<_>>();
        let mut registry = self.load_registry()?;
        let mut changed = false;
        for (plugin_id, entry) in &mut registry.plugins {
            let next_enabled = enabled.contains(plugin_id);
            if entry.enabled != next_enabled {
                entry.enabled = next_enabled;
                changed = true;
            }
        }
        if changed {
            self.save_registry(&registry)?;
        }
        Ok(())
    }
}

impl PluginRegistryEntry {
    fn new(
        enabled: bool,
        trusted: bool,
        installed_path: PathBuf,
        source_path: Option<PathBuf>,
    ) -> Self {
        Self {
            enabled,
            trusted,
            installed_path: installed_path.display().to_string(),
            source_path: source_path.map(|path| path.display().to_string()),
            installed_at_epoch_secs: current_epoch_secs(),
        }
    }
}

fn load_manifest(path: &Path) -> AppResult<PluginManifest> {
    let text = std::fs::read_to_string(path)?;
    let manifest = serde_json::from_str(&text).map_err(|error| {
        crate::error::AppError::Settings(format!("plugin manifest error: {error}"))
    })?;
    Ok(manifest)
}

fn normalize_plugin_id(plugin_id: &str) -> String {
    match plugin_id {
        "micronplus-textui" => BUILTIN_MICRONPLUS_PLUGIN_ID.into(),
        other => other.into(),
    }
}

fn is_safe_plugin_id(plugin_id: &str) -> bool {
    !plugin_id.is_empty()
        && plugin_id
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.'))
        && plugin_id != "."
        && plugin_id != ".."
}

fn default_min_app_version() -> String {
    "0.1.0".into()
}

fn current_epoch_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}

fn copy_dir_all(source: &Path, target: &Path) -> AppResult<()> {
    std::fs::create_dir_all(target)?;
    for entry in std::fs::read_dir(source)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let destination = target.join(entry.file_name());
        if file_type.is_dir() {
            copy_dir_all(&entry.path(), &destination)?;
        } else if file_type.is_file() {
            std::fs::copy(entry.path(), destination)?;
        }
    }
    Ok(())
}
