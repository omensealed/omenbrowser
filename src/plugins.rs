use std::collections::{BTreeMap, BTreeSet};
use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};
use crate::storage::files::atomic_replace;

static PLUGIN_REGISTRY_TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(1);
static PLUGIN_INSTALL_SEQUENCE: AtomicU64 = AtomicU64::new(1);
static PLUGIN_REMOVE_SEQUENCE: AtomicU64 = AtomicU64::new(1);
const PLUGIN_INSTALL_STAGING_PREFIX: &str = ".plugin-install-";
const PLUGIN_REMOVE_QUARANTINE_PREFIX: &str = ".plugin-remove-";

pub const PLUGIN_UNSAFE_INSTALL_WARNING: &str = "Third-party plugins may execute code in a future plugin runtime. Only install plugins you trust.";
pub const BUILTIN_MICRONPLUS_PLUGIN_ID: &str = "micronplus_textui";
pub const BUILTIN_OMENCHAT_PLUGIN_ID: &str = "omenchat_lxmf";
pub const PLUGIN_DISCOVERY_MAX_SCAN_ENTRIES: usize = 4096;
pub const PLUGIN_DISCOVERY_MAX_INSTALLED: usize = 256;
pub const PLUGIN_MANIFEST_MAX_BYTES: u64 = 64 * 1024;
pub const PLUGIN_REGISTRY_MAX_BYTES: u64 = 1024 * 1024;
pub const PLUGIN_INSTALL_MAX_ENTRIES: usize = 1024;
pub const PLUGIN_INSTALL_MAX_TOTAL_BYTES: u64 = 64 * 1024 * 1024;
pub const PLUGIN_INSTALL_MAX_FILE_BYTES: u64 = 16 * 1024 * 1024;
pub const PLUGIN_INSTALL_MAX_DEPTH: usize = 16;

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
        render_micronplus_tree_rows_with_widgets_and_field_cursor, retain_micronplus_control_event,
        try_extract_micronplus_layout, try_parse_micronplus_tree, widget_event_from_control_event,
        MicronPlusControlEvent, MicronPlusLayout, MicronPlusWidgetEvent, MicronPlusWidgetStore,
        MicronPlusWidgetStoreMetrics, MicronPlusWidgetTree,
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
        crate::private_fs::ensure_private_dir(&self.plugins_dir)?;
        let mut registry = self.load_registry()?;
        let mut registry_changed = false;
        let enabled = enabled_plugin_ids
            .iter()
            .map(|id| normalize_plugin_id(id))
            .collect::<BTreeSet<_>>();
        let mut seen = BTreeSet::new();
        let mut report = PluginDiscovery::default();
        report
            .warnings
            .extend(self.recover_pending_plugin_transactions(&registry)?);

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

        let mut entries = Vec::with_capacity(PLUGIN_DISCOVERY_MAX_INSTALLED);
        let mut discovery_truncated = false;
        for (scanned, entry) in std::fs::read_dir(&self.plugins_dir)?.enumerate() {
            if scanned >= PLUGIN_DISCOVERY_MAX_SCAN_ENTRIES {
                discovery_truncated = true;
                break;
            }
            let entry = entry?;
            let path = entry.path();
            if !entry.file_type()?.is_dir()
                || path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.starts_with('.'))
                || !path.join("plugin.json").exists()
            {
                continue;
            }
            if entries.len() >= PLUGIN_DISCOVERY_MAX_INSTALLED {
                discovery_truncated = true;
                break;
            }
            entries.push(entry);
        }
        entries.sort_by_key(|entry| entry.path());
        if discovery_truncated {
            report.warnings.push(format!(
                "plugin discovery stopped at the safety limit ({} directory entries scanned, {} installed plugin candidates retained)",
                PLUGIN_DISCOVERY_MAX_SCAN_ENTRIES, PLUGIN_DISCOVERY_MAX_INSTALLED
            ));
        }
        for entry in entries {
            let path = entry.path();
            let manifest_path = path.join("plugin.json");
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
        match std::fs::symlink_metadata(&self.registry_path) {
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(PluginRegistryFile::default());
            }
            Err(error) => return Err(error.into()),
        }
        let raw = read_bounded_regular_file(
            &self.registry_path,
            PLUGIN_REGISTRY_MAX_BYTES,
            "plugin registry",
        )?;
        crate::private_fs::repair_private_file(&self.registry_path)?;
        serde_json::from_slice(&raw)
            .map_err(|error| AppError::Settings(format!("plugin registry error: {error}")))
    }

    pub fn save_registry(&self, registry: &PluginRegistryFile) -> AppResult<()> {
        crate::private_fs::ensure_private_dir(&self.plugins_dir)?;
        let payload = serde_json::to_string_pretty(registry)
            .map_err(|error| AppError::Settings(format!("plugin registry error: {error}")))?;
        if (payload.len() as u64).saturating_add(1) > PLUGIN_REGISTRY_MAX_BYTES {
            return Err(AppError::Settings(format!(
                "plugin registry exceeds the {PLUGIN_REGISTRY_MAX_BYTES} byte limit"
            )));
        }
        save_registry_payload(&self.registry_path, payload.as_bytes(), atomic_replace)
    }

    pub fn install_from_folder(
        &self,
        source: &Path,
        confirm_unsafe: bool,
    ) -> AppResult<InstalledPlugin> {
        if !confirm_unsafe {
            return Err(AppError::Unsupported(PLUGIN_UNSAFE_INSTALL_WARNING.into()));
        }
        let source_metadata = std::fs::symlink_metadata(source)?;
        if !source_metadata.file_type().is_dir() {
            return Err(AppError::Settings(format!(
                "plugin source must be a regular directory: {}",
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
        ensure_plugin_target_absent(&target, &manifest.plugin_id)?;
        let mut registry = self.load_registry()?;
        crate::private_fs::ensure_private_dir(&self.plugins_dir)?;
        let staging = self
            .plugins_dir
            .join(plugin_install_staging_name(&manifest.plugin_id));
        crate::private_fs::ensure_private_dir(&staging)?;
        let mut budget = PluginInstallBudget::default();
        if let Err(error) = copy_plugin_tree(source, &staging, 0, &mut budget) {
            let _ = std::fs::remove_dir_all(&staging);
            return Err(error);
        }
        if let Err(error) = ensure_plugin_target_absent(&target, &manifest.plugin_id) {
            let _ = std::fs::remove_dir_all(&staging);
            return Err(error);
        }
        if let Err(error) = std::fs::rename(&staging, &target) {
            let _ = std::fs::remove_dir_all(&staging);
            return Err(error.into());
        }
        registry.plugins.insert(
            manifest.plugin_id.clone(),
            PluginRegistryEntry::new(true, true, target.clone(), Some(source.to_path_buf())),
        );
        if let Err(error) = self.save_registry(&registry) {
            let _ = std::fs::remove_dir_all(&target);
            return Err(error);
        }
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
        if matches!(
            plugin_id.as_str(),
            BUILTIN_MICRONPLUS_PLUGIN_ID | BUILTIN_OMENCHAT_PLUGIN_ID
        ) {
            return Err(AppError::Unsupported(
                "built-in plugins cannot be removed".into(),
            ));
        }
        self.remove_installed_with_save(&plugin_id, |registry| self.save_registry(registry))
    }

    fn remove_installed_with_save(
        &self,
        plugin_id: &str,
        save: impl FnOnce(&PluginRegistryFile) -> AppResult<()>,
    ) -> AppResult<bool> {
        let target = self.plugins_dir.join(plugin_id);
        let mut registry = self.load_registry()?;
        let quarantine = match std::fs::symlink_metadata(&target) {
            Ok(metadata) if metadata.file_type().is_dir() => {
                let quarantine = self
                    .plugins_dir
                    .join(plugin_removal_quarantine_name(plugin_id));
                std::fs::rename(&target, &quarantine)?;
                Some(quarantine)
            }
            Ok(_) => {
                return Err(AppError::Settings(format!(
                    "plugin removal target must be a regular directory: {}",
                    target.display()
                )));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(error.into()),
        };
        let had_quarantine = quarantine.is_some();
        let had_registry_entry = registry.plugins.remove(plugin_id).is_some();
        let removed = had_quarantine || had_registry_entry;
        if !removed {
            return Ok(false);
        }
        if let Err(error) = save(&registry) {
            if let Some(quarantine) = quarantine.as_ref() {
                if let Err(rollback_error) = std::fs::rename(quarantine, &target) {
                    return Err(AppError::Settings(format!(
                        "plugin registry removal failed: {error}; plugin restore also failed: {rollback_error}"
                    )));
                }
            }
            return Err(error);
        }
        if let Some(quarantine) = quarantine {
            std::fs::remove_dir_all(quarantine)?;
        }
        Ok(removed)
    }

    fn recover_pending_plugin_transactions(
        &self,
        registry: &PluginRegistryFile,
    ) -> AppResult<Vec<String>> {
        let mut warnings = Vec::new();
        for (scanned, entry) in std::fs::read_dir(&self.plugins_dir)?.enumerate() {
            if scanned >= PLUGIN_DISCOVERY_MAX_SCAN_ENTRIES {
                warnings.push(format!(
                    "plugin transaction recovery stopped at the {PLUGIN_DISCOVERY_MAX_SCAN_ENTRIES} entry scan limit"
                ));
                break;
            }
            let entry = entry?;
            let name = entry.file_name();
            if let Some(plugin_id) = name.to_str().and_then(plugin_id_from_install_staging_name) {
                if entry.file_type()?.is_dir() {
                    std::fs::remove_dir_all(entry.path())?;
                    warnings.push(format!(
                        "removed incomplete plugin installation after interrupted copy: {plugin_id}"
                    ));
                } else {
                    warnings.push(format!(
                        "ignored non-directory plugin installation staging path: {}",
                        entry.path().display()
                    ));
                }
                continue;
            }
            let Some(plugin_id) = name
                .to_str()
                .and_then(plugin_id_from_removal_quarantine_name)
            else {
                continue;
            };
            if !entry.file_type()?.is_dir() {
                warnings.push(format!(
                    "ignored non-directory plugin removal quarantine: {}",
                    entry.path().display()
                ));
                continue;
            }
            let target = self.plugins_dir.join(&plugin_id);
            if registry.plugins.contains_key(&plugin_id) {
                match std::fs::symlink_metadata(&target) {
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                        std::fs::rename(entry.path(), &target)?;
                        warnings.push(format!(
                            "restored plugin after interrupted removal: {plugin_id}"
                        ));
                    }
                    Ok(_) => warnings.push(format!(
                        "plugin removal recovery left quarantine because target already exists: {plugin_id}"
                    )),
                    Err(error) => return Err(error.into()),
                }
            } else {
                std::fs::remove_dir_all(entry.path())?;
                warnings.push(format!(
                    "completed plugin cleanup after interrupted removal: {plugin_id}"
                ));
            }
        }
        Ok(warnings)
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
    let raw = read_bounded_regular_file(path, PLUGIN_MANIFEST_MAX_BYTES, "plugin manifest")?;
    let manifest = serde_json::from_slice(&raw).map_err(|error| {
        crate::error::AppError::Settings(format!("plugin manifest error: {error}"))
    })?;
    Ok(manifest)
}

fn read_bounded_regular_file(path: &Path, max_bytes: u64, label: &str) -> AppResult<Vec<u8>> {
    let metadata = std::fs::symlink_metadata(path)?;
    if !metadata.file_type().is_file() {
        return Err(AppError::Settings(format!(
            "{label} must be a regular file: {}",
            path.display()
        )));
    }
    if metadata.len() > max_bytes {
        return Err(AppError::Settings(format!(
            "{label} exceeds the {max_bytes} byte limit: {}",
            path.display()
        )));
    }
    let file = std::fs::File::open(path)?;
    let mut raw = Vec::with_capacity(metadata.len() as usize);
    file.take(max_bytes.saturating_add(1))
        .read_to_end(&mut raw)?;
    if raw.len() as u64 > max_bytes {
        return Err(AppError::Settings(format!(
            "{label} exceeds the {max_bytes} byte limit: {}",
            path.display()
        )));
    }
    Ok(raw)
}

fn save_registry_payload(
    registry_path: &Path,
    payload: &[u8],
    replace: impl FnOnce(&Path, &Path) -> std::io::Result<()>,
) -> AppResult<()> {
    match std::fs::symlink_metadata(registry_path) {
        Ok(metadata) => {
            if !metadata.file_type().is_file() {
                return Err(AppError::Settings(format!(
                    "plugin registry target must be a regular file: {}",
                    registry_path.display()
                )));
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    let parent = registry_path
        .parent()
        .ok_or_else(|| AppError::Settings("plugin registry path has no parent directory".into()))?;
    let sequence = PLUGIN_REGISTRY_TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let temporary = parent.join(format!(
        ".registry.json.{}.{}.{}.tmp",
        std::process::id(),
        sequence,
        timestamp_nanos()
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
        replace(&temporary, registry_path)?;
        #[cfg(unix)]
        std::fs::File::open(parent)?.sync_all()?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result.map_err(Into::into)
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

fn timestamp_nanos() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default()
}

fn ensure_plugin_target_absent(target: &Path, plugin_id: &str) -> AppResult<()> {
    match std::fs::symlink_metadata(target) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Ok(_) => Err(AppError::Settings(format!(
            "plugin already exists: {plugin_id}"
        ))),
        Err(error) => Err(error.into()),
    }
}

fn encoded_plugin_id(plugin_id: &str) -> String {
    plugin_id
        .as_bytes()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn plugin_id_from_transaction_name(name: &str, prefix: &str) -> Option<String> {
    let mut parts = name.strip_prefix(prefix)?.split('.');
    let encoded = parts.next()?;
    parts.next()?.parse::<u32>().ok()?;
    parts.next()?.parse::<u64>().ok()?;
    parts.next()?.parse::<u128>().ok()?;
    if parts.next().is_some() {
        return None;
    }
    if encoded.is_empty() || encoded.len() % 2 != 0 {
        return None;
    }
    let bytes = encoded
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let text = std::str::from_utf8(pair).ok()?;
            u8::from_str_radix(text, 16).ok()
        })
        .collect::<Option<Vec<_>>>()?;
    let plugin_id = String::from_utf8(bytes).ok()?;
    is_safe_plugin_id(&plugin_id).then_some(plugin_id)
}

fn plugin_install_staging_name(plugin_id: &str) -> String {
    let sequence = PLUGIN_INSTALL_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    format!(
        "{PLUGIN_INSTALL_STAGING_PREFIX}{}.{}.{}.{}",
        encoded_plugin_id(plugin_id),
        std::process::id(),
        sequence,
        timestamp_nanos()
    )
}

fn plugin_id_from_install_staging_name(name: &str) -> Option<String> {
    plugin_id_from_transaction_name(name, PLUGIN_INSTALL_STAGING_PREFIX)
}

fn plugin_removal_quarantine_name(plugin_id: &str) -> String {
    let sequence = PLUGIN_REMOVE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    format!(
        "{PLUGIN_REMOVE_QUARANTINE_PREFIX}{}.{}.{}.{}",
        encoded_plugin_id(plugin_id),
        std::process::id(),
        sequence,
        timestamp_nanos()
    )
}

fn plugin_id_from_removal_quarantine_name(name: &str) -> Option<String> {
    plugin_id_from_transaction_name(name, PLUGIN_REMOVE_QUARANTINE_PREFIX)
}

#[derive(Default)]
struct PluginInstallBudget {
    entries: usize,
    bytes: u64,
}

impl PluginInstallBudget {
    fn admit_file(&mut self, bytes: u64, path: &Path) -> AppResult<()> {
        if bytes > PLUGIN_INSTALL_MAX_FILE_BYTES {
            return Err(AppError::Settings(format!(
                "plugin file exceeds the {PLUGIN_INSTALL_MAX_FILE_BYTES} byte limit: {}",
                path.display()
            )));
        }
        let next_bytes = self
            .bytes
            .checked_add(bytes)
            .ok_or_else(|| AppError::Settings("plugin install byte accounting overflow".into()))?;
        if next_bytes > PLUGIN_INSTALL_MAX_TOTAL_BYTES {
            return Err(AppError::Settings(format!(
                "plugin install exceeds the {PLUGIN_INSTALL_MAX_TOTAL_BYTES} total byte limit"
            )));
        }
        self.bytes = next_bytes;
        Ok(())
    }
}

fn copy_plugin_tree(
    source: &Path,
    target: &Path,
    depth: usize,
    budget: &mut PluginInstallBudget,
) -> AppResult<()> {
    for entry in std::fs::read_dir(source)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        budget.entries = budget.entries.saturating_add(1);
        if budget.entries > PLUGIN_INSTALL_MAX_ENTRIES {
            return Err(AppError::Settings(format!(
                "plugin install exceeds the {PLUGIN_INSTALL_MAX_ENTRIES} entry limit"
            )));
        }
        let destination = target.join(entry.file_name());
        if file_type.is_dir() {
            if depth >= PLUGIN_INSTALL_MAX_DEPTH {
                return Err(AppError::Settings(format!(
                    "plugin install exceeds the {PLUGIN_INSTALL_MAX_DEPTH} directory depth limit"
                )));
            }
            crate::private_fs::ensure_private_dir(&destination)?;
            copy_plugin_tree(&entry.path(), &destination, depth + 1, budget)?;
        } else if file_type.is_file() {
            let metadata = entry.metadata()?;
            budget.admit_file(metadata.len(), &entry.path())?;
            let source_file = std::fs::File::open(entry.path())?;
            let mut destination_file = OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&destination)?;
            let copied = std::io::copy(
                &mut source_file.take(metadata.len().saturating_add(1)),
                &mut destination_file,
            )?;
            if copied != metadata.len() {
                return Err(AppError::Settings(format!(
                    "plugin file changed while being copied: {}",
                    entry.path().display()
                )));
            }
            destination_file.sync_all()?;
        } else {
            return Err(AppError::Settings(format!(
                "plugin install refuses symlink or special entry: {}",
                entry.path().display()
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod install_budget_tests {
    use super::*;

    #[test]
    fn plugin_install_total_byte_budget_accepts_exact_limit_and_rejects_next_byte() {
        let mut budget = PluginInstallBudget::default();
        let fixture = Path::new("plugin-fixture.bin");
        for _ in 0..(PLUGIN_INSTALL_MAX_TOTAL_BYTES / PLUGIN_INSTALL_MAX_FILE_BYTES) {
            budget
                .admit_file(PLUGIN_INSTALL_MAX_FILE_BYTES, fixture)
                .expect("exact total budget");
        }
        assert_eq!(budget.bytes, PLUGIN_INSTALL_MAX_TOTAL_BYTES);
        let error = budget
            .admit_file(1, fixture)
            .expect_err("next byte must be rejected");
        assert!(error.to_string().contains("total byte limit"));
        assert_eq!(budget.bytes, PLUGIN_INSTALL_MAX_TOTAL_BYTES);
    }

    #[test]
    fn plugin_registry_replace_failure_preserves_previous_file_and_cleans_staging() {
        let root = std::env::temp_dir().join(format!(
            "omenbrowser-rs-plugin-registry-replace-fault-{}-{}",
            std::process::id(),
            timestamp_nanos()
        ));
        std::fs::create_dir_all(&root).expect("registry root");
        let registry = root.join("registry.json");
        std::fs::write(&registry, b"previous\n").expect("previous registry");

        let error = save_registry_payload(&registry, b"replacement", |_source, _destination| {
            Err(std::io::Error::other("injected replacement failure"))
        })
        .expect_err("replacement must fail");

        assert!(error.to_string().contains("injected replacement failure"));
        assert_eq!(
            std::fs::read(&registry).expect("previous registry"),
            b"previous\n"
        );
        assert_eq!(
            std::fs::read_dir(&root)
                .expect("registry root")
                .filter_map(Result::ok)
                .filter(|entry| entry.file_name().to_string_lossy().ends_with(".tmp"))
                .count(),
            0
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn plugin_install_recovery_removes_incomplete_directory_but_not_reserved_file() {
        let root = std::env::temp_dir().join(format!(
            "omenbrowser-rs-plugin-install-recovery-{}-{}",
            std::process::id(),
            timestamp_nanos()
        ));
        std::fs::create_dir_all(&root).expect("plugin root");
        let registry = PluginRegistry::new(root.clone());
        let staging = root.join(plugin_install_staging_name("interrupted-install"));
        std::fs::create_dir(&staging).expect("incomplete install stage");
        std::fs::write(staging.join("partial.bin"), b"partial").expect("partial fixture");
        let lookalike = root.join(format!(
            "{PLUGIN_INSTALL_STAGING_PREFIX}{}",
            encoded_plugin_id("not-owned")
        ));
        std::fs::create_dir(&lookalike).expect("non-transaction lookalike");

        let report = registry.discover(&[]).expect("install recovery discovery");

        assert!(!staging.exists());
        assert!(lookalike.is_dir());
        assert!(report
            .warnings
            .iter()
            .any(|line| line.contains("removed incomplete plugin installation")));

        let reserved_file = root.join(plugin_install_staging_name("reserved-file"));
        std::fs::write(&reserved_file, b"do not delete").expect("reserved file fixture");
        let report = registry
            .discover(&[])
            .expect("non-directory recovery discovery");
        assert!(reserved_file.is_file());
        assert!(report
            .warnings
            .iter()
            .any(|line| line.contains("ignored non-directory plugin installation staging")));
        let _ = std::fs::remove_dir_all(root);
    }

    fn removal_test_registry(root: &Path, plugin_id: &str) -> PluginRegistry {
        let registry = PluginRegistry::new(root.to_path_buf());
        let target = root.join(plugin_id);
        std::fs::create_dir_all(&target).expect("plugin target");
        std::fs::write(target.join("plugin.json"), b"{}\n").expect("plugin fixture");
        let mut file = PluginRegistryFile::default();
        file.plugins.insert(
            plugin_id.into(),
            PluginRegistryEntry::new(true, true, target, None),
        );
        registry.save_registry(&file).expect("registry fixture");
        registry
    }

    #[test]
    fn plugin_removal_registry_failure_restores_quarantined_tree() {
        let root = std::env::temp_dir().join(format!(
            "omenbrowser-rs-plugin-remove-registry-fault-{}-{}",
            std::process::id(),
            timestamp_nanos()
        ));
        std::fs::create_dir_all(&root).expect("plugin root");
        let registry = removal_test_registry(&root, "rollback-plugin");

        let error = registry
            .remove_installed_with_save("rollback-plugin", |_registry| {
                Err(AppError::Settings("injected registry failure".into()))
            })
            .expect_err("registry removal must fail");

        assert!(error.to_string().contains("injected registry failure"));
        assert!(root.join("rollback-plugin").is_dir());
        assert!(registry
            .load_registry()
            .expect("preserved registry")
            .plugins
            .contains_key("rollback-plugin"));
        assert_eq!(
            std::fs::read_dir(&root)
                .expect("plugin root")
                .filter_map(Result::ok)
                .filter(|entry| {
                    entry
                        .file_name()
                        .to_string_lossy()
                        .starts_with(PLUGIN_REMOVE_QUARANTINE_PREFIX)
                })
                .count(),
            0
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn plugin_removal_recovery_restores_precommit_and_cleans_postcommit_quarantine() {
        let root = std::env::temp_dir().join(format!(
            "omenbrowser-rs-plugin-remove-recovery-{}-{}",
            std::process::id(),
            timestamp_nanos()
        ));
        std::fs::create_dir_all(&root).expect("plugin root");
        let registry = removal_test_registry(&root, "recovery-plugin");
        let target = root.join("recovery-plugin");
        let precommit = root.join(plugin_removal_quarantine_name("recovery-plugin"));
        std::fs::rename(&target, &precommit).expect("precommit quarantine");

        let report = registry
            .discover(&[])
            .expect("precommit discovery recovery");
        assert!(target.is_dir());
        assert!(!precommit.exists());
        assert!(report
            .warnings
            .iter()
            .any(|line| line.contains("restored plugin")));

        let postcommit = root.join(plugin_removal_quarantine_name("recovery-plugin"));
        std::fs::rename(&target, &postcommit).expect("postcommit quarantine");
        registry
            .save_registry(&PluginRegistryFile::default())
            .expect("committed registry removal");
        let report = registry
            .discover(&[])
            .expect("postcommit discovery recovery");
        assert!(!target.exists());
        assert!(!postcommit.exists());
        assert!(report
            .warnings
            .iter()
            .any(|line| line.contains("completed plugin cleanup")));
        let _ = std::fs::remove_dir_all(root);
    }
}
