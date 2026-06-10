use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::AppResult;
use crate::storage::settings::AppSettings;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct IdentityProfile {
    pub label: String,
    pub path: PathBuf,
    pub hash_hex: String,
    pub managed: bool,
}

#[derive(Clone, Debug)]
pub struct IdentityManager {
    pub identities_dir: PathBuf,
    pub backups_dir: PathBuf,
}

pub trait IdentityMaterialProvider {
    fn provider_name(&self) -> &'static str;
    fn create_identity_material(&self, label: &str) -> AppResult<Vec<u8>>;
}

#[derive(Clone, Debug, Default)]
pub struct MockIdentityMaterialProvider;

impl IdentityMaterialProvider for MockIdentityMaterialProvider {
    fn provider_name(&self) -> &'static str {
        "mock"
    }

    fn create_identity_material(&self, label: &str) -> AppResult<Vec<u8>> {
        Ok(format!("mock-identity:{}:{}", label, timestamp_nanos()).into_bytes())
    }
}

impl IdentityManager {
    pub fn new(identities_dir: PathBuf, backups_dir: PathBuf) -> Self {
        Self {
            identities_dir,
            backups_dir,
        }
    }

    pub fn create_managed_identity(&self, label: &str) -> AppResult<IdentityProfile> {
        self.create_managed_identity_with_provider(label, &MockIdentityMaterialProvider)
    }

    pub fn create_managed_identity_with_provider(
        &self,
        label: &str,
        provider: &dyn IdentityMaterialProvider,
    ) -> AppResult<IdentityProfile> {
        std::fs::create_dir_all(&self.identities_dir)?;
        let default_path = self.identities_dir.join("default_identity");
        let identity_path = if default_path.exists() {
            self.backup_if_exists(&default_path)?;
            self.identities_dir
                .join(format!("default_identity.{}", timestamp_nanos()))
        } else {
            default_path
        };
        self.backup_if_exists(&identity_path)?;

        let raw = provider.create_identity_material(label)?;
        std::fs::write(&identity_path, &raw)?;

        Ok(IdentityProfile {
            label: label.into(),
            path: identity_path,
            hash_hex: hash_for_bytes(&raw),
            managed: true,
        })
    }

    pub fn attach_existing(
        &self,
        identity_path: PathBuf,
        label: Option<&str>,
    ) -> AppResult<IdentityProfile> {
        let raw = std::fs::read(&identity_path)?;
        Ok(IdentityProfile {
            label: label
                .map(str::to_string)
                .unwrap_or_else(|| path_label(&identity_path)),
            path: identity_path,
            hash_hex: hash_for_bytes(&raw),
            managed: false,
        })
    }

    pub fn import_identity_copy(
        &self,
        source_path: PathBuf,
        label: Option<&str>,
    ) -> AppResult<IdentityProfile> {
        std::fs::create_dir_all(&self.identities_dir)?;
        let target_path = self.identities_dir.join(
            source_path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("imported_identity"),
        );
        self.backup_if_exists(&target_path)?;
        std::fs::copy(&source_path, &target_path)?;
        let raw = std::fs::read(&target_path)?;

        Ok(IdentityProfile {
            label: label
                .map(str::to_string)
                .unwrap_or_else(|| path_label(&source_path)),
            path: target_path,
            hash_hex: hash_for_bytes(&raw),
            managed: true,
        })
    }

    pub fn export_backup(
        &self,
        profile: &IdentityProfile,
        target_dir: Option<PathBuf>,
    ) -> AppResult<PathBuf> {
        let target_dir = target_dir.unwrap_or_else(|| self.backups_dir.clone());
        std::fs::create_dir_all(&target_dir)?;
        let filename = format!(
            "{}.backup.{}",
            profile
                .path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("identity"),
            timestamp_nanos()
        );
        let target_path = target_dir.join(filename);
        std::fs::copy(&profile.path, &target_path)?;
        Ok(target_path)
    }

    pub fn list_managed_identities(&self) -> AppResult<Vec<IdentityProfile>> {
        if !self.identities_dir.exists() {
            return Ok(Vec::new());
        }
        let mut profiles = Vec::new();
        for entry in std::fs::read_dir(&self.identities_dir)? {
            let entry = entry?;
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let raw = std::fs::read(&path)?;
            profiles.push(IdentityProfile {
                label: path_label(&path),
                path,
                hash_hex: hash_for_bytes(&raw),
                managed: true,
            });
        }
        profiles.sort_by(|a, b| a.label.cmp(&b.label));
        Ok(profiles)
    }

    pub fn delete_identity_with_backup(&self, profile: &IdentityProfile) -> AppResult<PathBuf> {
        let backup = self.export_backup(profile, None)?;
        std::fs::remove_file(&profile.path)?;
        Ok(backup)
    }

    pub fn activate_profile(settings: &mut AppSettings, profile: &IdentityProfile) {
        settings.identity_path = Some(profile.path.clone());
        settings.active_identity_label = Some(profile.label.clone());
    }

    fn backup_if_exists(&self, path: &std::path::Path) -> AppResult<Option<PathBuf>> {
        if !path.exists() {
            return Ok(None);
        }
        std::fs::create_dir_all(&self.backups_dir)?;
        let backup_path = self.backups_dir.join(format!(
            "{}.bak.{}",
            path.file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("identity"),
            timestamp_nanos()
        ));
        std::fs::copy(path, &backup_path)?;
        Ok(Some(backup_path))
    }
}

pub fn hash_for_bytes(raw: &[u8]) -> String {
    let digest = Sha256::digest(raw);
    let hex = format!("{digest:x}");
    hex.chars().take(32).collect()
}

fn path_label(path: &std::path::Path) -> String {
    path.file_stem()
        .and_then(|name| name.to_str())
        .unwrap_or("identity")
        .to_string()
}

fn timestamp_nanos() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
}
