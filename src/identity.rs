use std::fs::File;
use std::io::{ErrorKind, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::{AppError, AppResult};
use crate::storage::settings::AppSettings;

pub const IDENTITY_MATERIAL_MAX_BYTES: u64 = 64 * 1024;
pub const IDENTITY_DISCOVERY_MAX_SCAN_ENTRIES: usize = 4096;
pub const IDENTITY_DISCOVERY_MAX_PROFILES: usize = 256;
pub const IDENTITY_BACKUP_MAX_FILES: usize = 16;
pub const IDENTITY_BACKUP_MAX_TOTAL_BYTES: u64 =
    IDENTITY_BACKUP_MAX_FILES as u64 * IDENTITY_MATERIAL_MAX_BYTES;
pub const IDENTITY_BACKUP_MAX_SCAN_ENTRIES: usize = 4096;

const IDENTITY_BACKUP_PREFIX: &str = "omen-identity.backup.";
const IDENTITY_BACKUP_SUFFIX: &str = ".bak";
static IDENTITY_FILE_SEQUENCE: AtomicU64 = AtomicU64::new(1);

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
        let raw = provider.create_identity_material(label)?;
        validate_identity_material(&raw)?;
        crate::private_fs::ensure_private_dir(&self.identities_dir)?;
        let default_path = self.identities_dir.join("default_identity");
        let identity_path = if default_path.exists() {
            self.backup_if_exists(&default_path)?;
            self.identities_dir
                .join(format!("default_identity.{}", timestamp_nanos()))
        } else {
            default_path
        };
        self.backup_if_exists(&identity_path)?;
        publish_identity_material(&identity_path, &raw, PublishMode::CreateNew)?;

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
        let raw = read_identity_material(&identity_path)?;
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
        let raw = read_identity_material(&source_path)?;
        crate::private_fs::ensure_private_dir(&self.identities_dir)?;
        let target_path = self.identities_dir.join(
            source_path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("imported_identity"),
        );
        let publish_mode = if self.backup_if_exists(&target_path)?.is_some() {
            PublishMode::Replace
        } else {
            PublishMode::CreateNew
        };
        publish_identity_material(&target_path, &raw, publish_mode)?;

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
        let raw = read_identity_material(&profile.path)?;
        let managed_backup_dir = target_dir.is_none();
        let target_dir = target_dir.unwrap_or_else(|| self.backups_dir.clone());
        if managed_backup_dir {
            crate::private_fs::ensure_private_dir(&target_dir)?;
        } else {
            ensure_real_directory(&target_dir)?;
        }
        let target_path = unique_backup_path(&target_dir);
        publish_identity_material(&target_path, &raw, PublishMode::CreateNew)?;
        if managed_backup_dir {
            prune_managed_backups(&target_dir)?;
        }
        Ok(target_path)
    }

    pub fn list_managed_identities(&self) -> AppResult<Vec<IdentityProfile>> {
        match std::fs::symlink_metadata(&self.identities_dir) {
            Ok(metadata) if metadata.file_type().is_dir() => {}
            Ok(_) => {
                return Err(AppError::Runtime(
                    "managed identity root must be a directory and not a symbolic link".into(),
                ));
            }
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(error.into()),
        }
        let mut profiles = Vec::new();
        for (scanned, entry) in std::fs::read_dir(&self.identities_dir)?.enumerate() {
            if scanned == IDENTITY_DISCOVERY_MAX_SCAN_ENTRIES {
                return Err(AppError::Runtime(format!(
                    "managed identity discovery exceeds the {IDENTITY_DISCOVERY_MAX_SCAN_ENTRIES} entry scan limit"
                )));
            }
            let entry = entry?;
            let path = entry.path();
            if !entry.file_type()?.is_file() {
                continue;
            }
            if profiles.len() == IDENTITY_DISCOVERY_MAX_PROFILES {
                return Err(AppError::Runtime(format!(
                    "managed identity discovery exceeds the {IDENTITY_DISCOVERY_MAX_PROFILES} profile limit"
                )));
            }
            crate::private_fs::repair_private_file(&path)?;
            let raw = read_identity_material(&path)?;
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
        if let Some(parent) = profile.path.parent() {
            sync_directory(parent)?;
        }
        Ok(backup)
    }

    pub fn activate_profile(settings: &mut AppSettings, profile: &IdentityProfile) {
        settings.identity_path = Some(profile.path.clone());
        settings.active_identity_label = Some(profile.label.clone());
    }

    fn backup_if_exists(&self, path: &std::path::Path) -> AppResult<Option<PathBuf>> {
        let raw = match std::fs::symlink_metadata(path) {
            Ok(metadata) if metadata.file_type().is_file() => read_identity_material(path)?,
            Ok(_) => {
                return Err(AppError::Runtime(
                    "identity backup source must be a regular file and not a symbolic link".into(),
                ));
            }
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        crate::private_fs::ensure_private_dir(&self.backups_dir)?;
        let backup_path = unique_backup_path(&self.backups_dir);
        publish_identity_material(&backup_path, &raw, PublishMode::CreateNew)?;
        prune_managed_backups(&self.backups_dir)?;
        Ok(Some(backup_path))
    }
}

#[derive(Clone, Copy)]
enum PublishMode {
    CreateNew,
    Replace,
}

fn ensure_real_directory(path: &Path) -> AppResult<()> {
    std::fs::create_dir_all(path)?;
    let metadata = std::fs::symlink_metadata(path)?;
    if !metadata.file_type().is_dir() {
        return Err(AppError::Runtime(format!(
            "identity storage root must be a directory and not a symbolic link: {}",
            path.display()
        )));
    }
    Ok(())
}

fn publish_identity_material(path: &Path, raw: &[u8], mode: PublishMode) -> AppResult<()> {
    publish_identity_material_with(path, raw, mode, || Ok(()))
}

fn publish_identity_material_with(
    path: &Path,
    raw: &[u8],
    mode: PublishMode,
    before_commit: impl FnOnce() -> std::io::Result<()>,
) -> AppResult<()> {
    validate_identity_material(raw)?;
    let parent = path.parent().ok_or_else(|| {
        std::io::Error::new(
            ErrorKind::InvalidInput,
            "identity destination has no parent directory",
        )
    })?;
    ensure_real_directory(parent)?;

    match (mode, std::fs::symlink_metadata(path)) {
        (PublishMode::CreateNew, Ok(_)) => {
            return Err(std::io::Error::new(
                ErrorKind::AlreadyExists,
                "identity destination already exists",
            )
            .into());
        }
        (PublishMode::Replace, Ok(metadata)) if !metadata.file_type().is_file() => {
            return Err(AppError::Runtime(
                "identity replacement destination must be a regular file and not a symbolic link"
                    .into(),
            ));
        }
        (_, Err(error)) if error.kind() != ErrorKind::NotFound => return Err(error.into()),
        _ => {}
    }

    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| {
            std::io::Error::new(
                ErrorKind::InvalidInput,
                "identity destination has no safe file name",
            )
        })?;
    let sequence = IDENTITY_FILE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let temporary = parent.join(format!(
        ".{file_name}.{}.{}.identity.tmp",
        std::process::id(),
        sequence
    ));

    let result = (|| -> std::io::Result<()> {
        let mut options = std::fs::OpenOptions::new();
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
        before_commit()?;

        match mode {
            PublishMode::CreateNew => {
                std::fs::hard_link(&temporary, path)?;
                sync_directory(parent)?;
                std::fs::remove_file(&temporary)?;
            }
            PublishMode::Replace => crate::storage::files::atomic_replace(&temporary, path)?,
        }
        sync_directory(parent)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result.map_err(Into::into)
}

fn sync_directory(path: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        std::fs::File::open(path)?.sync_all()?;
    }
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

fn unique_backup_path(directory: &Path) -> PathBuf {
    let sequence = IDENTITY_FILE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    directory.join(format!(
        "{IDENTITY_BACKUP_PREFIX}{}.{}.{}{IDENTITY_BACKUP_SUFFIX}",
        timestamp_nanos(),
        std::process::id(),
        sequence
    ))
}

fn is_managed_backup_name(name: &str) -> bool {
    let Some(body) = name
        .strip_prefix(IDENTITY_BACKUP_PREFIX)
        .and_then(|name| name.strip_suffix(IDENTITY_BACKUP_SUFFIX))
    else {
        return false;
    };
    let mut parts = body.split('.');
    matches!(
        (parts.next(), parts.next(), parts.next(), parts.next()),
        (Some(timestamp), Some(process), Some(sequence), None)
            if !timestamp.is_empty()
                && timestamp.bytes().all(|byte| byte.is_ascii_digit())
                && !process.is_empty()
                && process.bytes().all(|byte| byte.is_ascii_digit())
                && !sequence.is_empty()
                && sequence.bytes().all(|byte| byte.is_ascii_digit())
    )
}

fn prune_managed_backups(directory: &Path) -> AppResult<()> {
    let mut backups = Vec::new();
    let mut total_bytes = 0_u64;
    for (scanned, entry) in std::fs::read_dir(directory)?.enumerate() {
        if scanned == IDENTITY_BACKUP_MAX_SCAN_ENTRIES {
            return Err(AppError::Runtime(format!(
                "identity backup discovery exceeds the {IDENTITY_BACKUP_MAX_SCAN_ENTRIES} entry scan limit"
            )));
        }
        let entry = entry?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        if !is_managed_backup_name(name) || !entry.file_type()?.is_file() {
            continue;
        }
        crate::private_fs::repair_private_file(&entry.path())?;
        let bytes = entry.metadata()?.len();
        total_bytes = total_bytes.saturating_add(bytes);
        backups.push((name.to_owned(), entry.path(), bytes));
    }
    backups.sort_by(|left, right| left.0.cmp(&right.0));

    let mut removed_any = false;
    let mut retained = backups.len();
    for (_, path, bytes) in backups {
        if retained <= IDENTITY_BACKUP_MAX_FILES && total_bytes <= IDENTITY_BACKUP_MAX_TOTAL_BYTES {
            break;
        }
        std::fs::remove_file(path)?;
        retained = retained.saturating_sub(1);
        total_bytes = total_bytes.saturating_sub(bytes);
        removed_any = true;
    }
    if removed_any {
        sync_directory(directory)?;
    }
    Ok(())
}

pub fn read_identity_material(path: &Path) -> AppResult<Vec<u8>> {
    let path_metadata = std::fs::symlink_metadata(path)?;
    if !path_metadata.file_type().is_file() {
        return Err(AppError::Runtime(
            "identity material must be a regular file and not a symbolic link".into(),
        ));
    }
    if path_metadata.len() > IDENTITY_MATERIAL_MAX_BYTES {
        return Err(AppError::Runtime(format!(
            "identity material exceeds the {IDENTITY_MATERIAL_MAX_BYTES} byte limit"
        )));
    }

    let file = File::open(path)?;
    let opened_metadata = file.metadata()?;
    if !opened_metadata.is_file() {
        return Err(AppError::Runtime(
            "identity material must open as a regular file".into(),
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;

        if path_metadata.dev() != opened_metadata.dev()
            || path_metadata.ino() != opened_metadata.ino()
        {
            return Err(AppError::Runtime(
                "identity material changed while it was being opened".into(),
            ));
        }
    }

    let mut raw = Vec::with_capacity(path_metadata.len() as usize);
    file.take(IDENTITY_MATERIAL_MAX_BYTES.saturating_add(1))
        .read_to_end(&mut raw)?;
    validate_identity_material(&raw)?;
    Ok(raw)
}

fn validate_identity_material(raw: &[u8]) -> AppResult<()> {
    if raw.is_empty() {
        return Err(AppError::Runtime(
            "identity material must not be empty".into(),
        ));
    }
    if raw.len() as u64 > IDENTITY_MATERIAL_MAX_BYTES {
        return Err(AppError::Runtime(format!(
            "identity material exceeds the {IDENTITY_MATERIAL_MAX_BYTES} byte limit"
        )));
    }
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::{publish_identity_material_with, PublishMode};

    fn fixture(name: &str) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!(
            "omenbrowser-identity-publication-{name}-{}-{}",
            std::process::id(),
            super::IDENTITY_FILE_SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("publication fixture");
        root
    }

    #[test]
    fn precommit_failure_preserves_existing_identity_and_removes_stage() {
        let root = fixture("replace-fault");
        let target = root.join("identity");
        std::fs::write(&target, b"previous identity").expect("previous identity");

        publish_identity_material_with(&target, b"replacement", PublishMode::Replace, || {
            Err(std::io::Error::other("injected precommit failure"))
        })
        .expect_err("publication must fail");

        assert_eq!(
            std::fs::read(&target).expect("preserved identity"),
            b"previous identity"
        );
        assert_eq!(
            std::fs::read_dir(&root)
                .expect("fixture entries")
                .filter_map(Result::ok)
                .count(),
            1
        );
    }

    #[test]
    fn precommit_failure_does_not_publish_new_identity() {
        let root = fixture("create-fault");
        let target = root.join("identity");

        publish_identity_material_with(&target, b"new identity", PublishMode::CreateNew, || {
            Err(std::io::Error::other("injected precommit failure"))
        })
        .expect_err("publication must fail");

        assert!(!target.exists());
        assert_eq!(
            std::fs::read_dir(&root)
                .expect("fixture entries")
                .filter_map(Result::ok)
                .count(),
            0
        );
    }
}
