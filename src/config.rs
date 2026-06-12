use std::path::{Path, PathBuf};

use directories::ProjectDirs;

use crate::error::{AppError, AppResult};
use crate::identity::{hash_for_bytes, IdentityProfile};
use crate::storage::settings::AppSettings;

#[derive(Clone, Debug)]
pub struct AppPaths {
    pub root: PathBuf,
    pub settings_file: PathBuf,
    pub identities_dir: PathBuf,
    pub identity_backups_dir: PathBuf,
    pub identity_storage_dir: PathBuf,
    pub reticulum_config_dir: PathBuf,
    pub reticulum_storage_dir: PathBuf,
    pub messages_dir: PathBuf,
    pub attachments_dir: PathBuf,
    pub directory_file: PathBuf,
    pub cache_dir: PathBuf,
    pub downloads_dir: PathBuf,
    pub plugins_dir: PathBuf,
    pub logs_dir: PathBuf,
    pub diagnostics_dir: PathBuf,
    pub interfaces_file: PathBuf,
    pub gateways_file: PathBuf,
    pub browser_form_state_file: PathBuf,
    pub legacy_python_conversations_dir: Option<PathBuf>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct IdentityStorageMigration {
    pub copied_files: usize,
    pub created_dirs: usize,
    pub skipped_existing: usize,
}

impl AppPaths {
    pub fn discover() -> AppResult<Self> {
        let mut paths = Self::from_root(default_managed_root()?);
        paths.legacy_python_conversations_dir = default_python_conversations_dir();
        Ok(paths)
    }

    pub fn from_root(root: PathBuf) -> Self {
        Self {
            settings_file: root.join("settings.json"),
            identities_dir: root.join("identities"),
            identity_backups_dir: root.join("identities").join("backups"),
            identity_storage_dir: root.join("identity_storage"),
            reticulum_config_dir: root.join("reticulum"),
            reticulum_storage_dir: root.join("reticulum").join("storage"),
            messages_dir: root.join("messages"),
            attachments_dir: root.join("attachments"),
            directory_file: root.join("directory.json"),
            cache_dir: root.join("cache"),
            downloads_dir: root.join("downloads"),
            plugins_dir: root.join("plugins"),
            logs_dir: root.join("logs"),
            diagnostics_dir: root.join("diagnostics"),
            interfaces_file: root.join("interfaces.json"),
            gateways_file: root.join("interface_gateways.json"),
            browser_form_state_file: root.join("browser_form_state.json"),
            legacy_python_conversations_dir: None,
            root,
        }
    }

    pub fn ensure(&self) -> AppResult<()> {
        for dir in [
            &self.root,
            &self.identities_dir,
            &self.identity_backups_dir,
            &self.identity_storage_dir,
            &self.reticulum_config_dir,
            &self.reticulum_storage_dir,
            &self.messages_dir,
            &self.attachments_dir,
            &self.cache_dir,
            &self.downloads_dir,
            &self.plugins_dir,
            &self.logs_dir,
            &self.diagnostics_dir,
        ] {
            std::fs::create_dir_all(dir)?;
        }
        Ok(())
    }

    pub fn scoped_for_active_identity(&self, settings: &AppSettings) -> Self {
        let Some(identity_path) = settings.identity_path.as_ref() else {
            return self.clone();
        };
        self.scoped_to_identity_path(identity_path)
    }

    pub fn scoped_to_identity_path(&self, identity_path: &std::path::Path) -> Self {
        let storage_root = self.storage_root_for_identity_path(identity_path);
        self.with_identity_storage_root(storage_root)
    }

    pub fn storage_root_for_identity_profile(&self, profile: &IdentityProfile) -> PathBuf {
        self.identity_storage_dir
            .join(identity_storage_key(&profile.path, Some(&profile.hash_hex)))
    }

    pub fn storage_root_for_identity_path(&self, identity_path: &std::path::Path) -> PathBuf {
        let hash = std::fs::read(identity_path)
            .ok()
            .map(|raw| hash_for_bytes(&raw));
        self.identity_storage_dir
            .join(identity_storage_key(identity_path, hash.as_deref()))
    }

    pub fn with_identity_storage_root(&self, storage_root: PathBuf) -> Self {
        let mut paths = self.clone();
        paths.reticulum_config_dir = storage_root.join("reticulum");
        paths.reticulum_storage_dir = paths.reticulum_config_dir.join("storage");
        paths.messages_dir = storage_root.join("messages");
        paths.attachments_dir = storage_root.join("attachments");
        paths.directory_file = storage_root.join("directory.json");
        paths.cache_dir = storage_root.join("cache");
        paths.browser_form_state_file = storage_root.join("browser_form_state.json");
        paths
    }

    pub fn adopt_legacy_app_storage_once(
        &self,
        legacy: &AppPaths,
    ) -> AppResult<Option<IdentityStorageMigration>> {
        let marker = legacy
            .identity_storage_dir
            .join(".app_level_storage_adopted");
        if marker.exists() || self.identity_storage_root() == legacy.identity_storage_root() {
            return Ok(None);
        }

        let mut migration = IdentityStorageMigration::default();
        copy_dir_missing(&legacy.messages_dir, &self.messages_dir, &mut migration)?;
        copy_dir_missing(
            &legacy.attachments_dir,
            &self.attachments_dir,
            &mut migration,
        )?;
        copy_dir_missing(&legacy.cache_dir, &self.cache_dir, &mut migration)?;
        copy_dir_missing(
            &legacy.reticulum_config_dir,
            &self.reticulum_config_dir,
            &mut migration,
        )?;
        copy_file_missing(&legacy.directory_file, &self.directory_file, &mut migration)?;
        copy_file_missing(
            &legacy.browser_form_state_file,
            &self.browser_form_state_file,
            &mut migration,
        )?;

        std::fs::create_dir_all(&legacy.identity_storage_dir)?;
        std::fs::write(
            marker,
            format!(
                "app-level storage adopted into {}\n",
                self.identity_storage_root().display()
            ),
        )?;
        Ok(Some(migration))
    }

    pub fn identity_storage_root(&self) -> PathBuf {
        self.messages_dir
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| self.root.clone())
    }
}

fn copy_dir_missing(
    source: &Path,
    target: &Path,
    migration: &mut IdentityStorageMigration,
) -> AppResult<()> {
    if !source.exists() {
        return Ok(());
    }
    if !source.is_dir() || source == target {
        return Ok(());
    }
    if !target.exists() {
        std::fs::create_dir_all(target)?;
        migration.created_dirs += 1;
    }
    for entry in std::fs::read_dir(source)? {
        let entry = entry?;
        let source_path = entry.path();
        let target_path = target.join(entry.file_name());
        if source_path.is_dir() {
            copy_dir_missing(&source_path, &target_path, migration)?;
        } else if source_path.is_file() {
            copy_file_missing(&source_path, &target_path, migration)?;
        }
    }
    Ok(())
}

fn copy_file_missing(
    source: &Path,
    target: &Path,
    migration: &mut IdentityStorageMigration,
) -> AppResult<()> {
    if !source.exists() || !source.is_file() || source == target {
        return Ok(());
    }
    if target.exists() {
        migration.skipped_existing += 1;
        return Ok(());
    }
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::copy(source, target)?;
    migration.copied_files += 1;
    Ok(())
}

fn identity_storage_key(identity_path: &std::path::Path, hash_hex: Option<&str>) -> String {
    let stem = identity_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("identity");
    let stem = safe_storage_component(stem);
    let hash = hash_hex
        .filter(|hash| !hash.is_empty())
        .map(|hash| hash.chars().take(16).collect::<String>())
        .unwrap_or_else(|| "unhashed".into());
    format!("{stem}-{hash}")
}

fn safe_storage_component(input: &str) -> String {
    let mut safe = input
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>();
    while safe.starts_with('.') {
        safe.remove(0);
    }
    if safe.is_empty() {
        "identity".into()
    } else {
        safe
    }
}

fn default_managed_root() -> AppResult<PathBuf> {
    if let Some(home) = std::env::var_os("HOME") {
        return Ok(PathBuf::from(home).join(".config").join("OMENbrowser_rs"));
    }
    let project_dirs = ProjectDirs::from("net", "OMEN", "OMENbrowser_rs")
        .ok_or_else(|| AppError::Settings("could not resolve project data directory".into()))?;
    Ok(project_dirs.config_dir().to_path_buf())
}

fn default_python_conversations_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from).map(|home| {
        home.join(".local")
            .join("share")
            .join("OMENbrowser")
            .join("conversations")
    })
}

#[derive(Clone, Debug)]
pub struct AppConfig {
    pub paths: AppPaths,
    pub settings: AppSettings,
}

impl AppConfig {
    pub fn load() -> AppResult<Self> {
        let paths = AppPaths::discover()?;
        paths.ensure()?;
        let settings = AppSettings::load_or_default(&paths.settings_file)?;
        Ok(Self { paths, settings })
    }
}
