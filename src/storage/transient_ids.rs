use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{ErrorKind, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};

pub const LXMF_LOCAL_DELIVERY_CACHE_MAX_AGE_SECS: f64 = 30.0 * 24.0 * 60.0 * 60.0 * 6.0;
pub const LXMF_LOCAL_DELIVERY_CACHE_MAX_ITEMS: usize = 65_536;
pub const LXMF_LOCAL_DELIVERY_CACHE_MAX_BYTES: u64 = 8 * 1024 * 1024;
pub const LXMF_LOCAL_DELIVERY_CACHE_CORRUPT_BACKUP_MAX_FILES: usize = 4;
pub const LXMF_LOCAL_DELIVERY_CACHE_CORRUPT_BACKUP_MAX_TOTAL_BYTES: u64 =
    LXMF_LOCAL_DELIVERY_CACHE_CORRUPT_BACKUP_MAX_FILES as u64 * LXMF_LOCAL_DELIVERY_CACHE_MAX_BYTES;
pub const LXMF_LOCAL_DELIVERY_CACHE_BACKUP_MAX_SCAN_ENTRIES: usize = 4096;
const LXMF_LOCAL_DELIVERY_CACHE_PRUNE_TO_ITEMS: usize =
    LXMF_LOCAL_DELIVERY_CACHE_MAX_ITEMS * 9 / 10;
const TRANSIENT_ID_HEX_LEN: usize = 64;
static CACHE_FILE_SEQUENCE: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Debug, PartialEq)]
pub struct DeliveredTransientIdStore {
    path: PathBuf,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct DeliveredTransientIds {
    #[serde(default)]
    ids: BTreeMap<String, f64>,
}

impl DeliveredTransientIdStore {
    pub fn for_reticulum_storage(storage_dir: impl AsRef<Path>) -> Self {
        Self {
            path: storage_dir
                .as_ref()
                .join("lxmf")
                .join("local_deliveries_rs.json"),
        }
    }

    pub fn from_path(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn load_or_default(&self) -> AppResult<BTreeMap<String, f64>> {
        let Some(data) = read_bounded_cache(&self.path)? else {
            return Ok(BTreeMap::new());
        };
        // Try the scalar-valued legacy map first. A versioned wrapper cannot be
        // mistaken for it because `ids` is an object, while retaining the
        // wrapper's existing tolerance for future unknown fields.
        let parsed = serde_json::from_slice::<BTreeMap<String, f64>>(&data).or_else(|_| {
            serde_json::from_slice::<DeliveredTransientIds>(&data).map(|cache| cache.ids)
        });
        match parsed {
            Ok(mut ids) if validate_ids(&ids).is_ok() => {
                Self::prune_to_limit(&mut ids);
                Ok(ids)
            }
            _ => {
                backup_corrupt_cache(&self.path, &data)?;
                Ok(BTreeMap::new())
            }
        }
    }

    pub fn save(&self, ids: &BTreeMap<String, f64>) -> AppResult<()> {
        validate_ids(ids).map_err(AppError::Settings)?;
        let mut cache = DeliveredTransientIds { ids: ids.clone() };
        Self::prune_to_limit(&mut cache.ids);
        let mut data = serde_json::to_vec_pretty(&cache)
            .map_err(|error| AppError::Settings(error.to_string()))?;
        data.push(b'\n');
        if data.len() as u64 > LXMF_LOCAL_DELIVERY_CACHE_MAX_BYTES {
            return Err(AppError::Settings(format!(
                "LXMF local delivery cache exceeds {LXMF_LOCAL_DELIVERY_CACHE_MAX_BYTES} byte limit"
            )));
        }
        publish_cache_bytes(&self.path, &data, PublishMode::Replace, || Ok(()))
    }

    pub fn mark_delivered(ids: &mut BTreeMap<String, f64>, transient_id: &[u8; 32], now: f64) {
        ids.insert(hex_encode(transient_id), now);
        Self::prune_to_limit(ids);
    }

    pub fn has_delivered(ids: &BTreeMap<String, f64>, transient_id: &[u8; 32]) -> bool {
        ids.contains_key(&hex_encode(transient_id))
    }

    pub fn prune_expired(ids: &mut BTreeMap<String, f64>, now: f64, max_age_secs: f64) -> usize {
        let before = ids.len();
        ids.retain(|_, timestamp| now <= *timestamp + max_age_secs);
        before.saturating_sub(ids.len())
    }

    pub fn prune_to_limit(ids: &mut BTreeMap<String, f64>) -> usize {
        if ids.len() <= LXMF_LOCAL_DELIVERY_CACHE_MAX_ITEMS {
            return 0;
        }
        let remove_count = ids
            .len()
            .saturating_sub(LXMF_LOCAL_DELIVERY_CACHE_PRUNE_TO_ITEMS);
        let mut oldest = ids
            .iter()
            .map(|(key, timestamp)| (key.clone(), *timestamp))
            .collect::<Vec<_>>();
        oldest.sort_by(|left, right| {
            left.1
                .total_cmp(&right.1)
                .then_with(|| left.0.cmp(&right.0))
        });
        for (key, _) in oldest.into_iter().take(remove_count) {
            ids.remove(&key);
        }
        remove_count
    }
}

fn validate_ids(ids: &BTreeMap<String, f64>) -> Result<(), String> {
    for (transient_id, timestamp) in ids {
        if transient_id.len() != TRANSIENT_ID_HEX_LEN
            || !transient_id.bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            return Err("LXMF local delivery cache contains an invalid transient id".into());
        }
        if !timestamp.is_finite() {
            return Err("LXMF local delivery cache contains a non-finite timestamp".into());
        }
    }
    Ok(())
}

fn read_bounded_cache(path: &Path) -> AppResult<Option<Vec<u8>>> {
    let path_metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    if !path_metadata.file_type().is_file() {
        return Err(AppError::Settings(format!(
            "LXMF local delivery cache must be a regular file: {}",
            path.display()
        )));
    }
    if path_metadata.len() > LXMF_LOCAL_DELIVERY_CACHE_MAX_BYTES {
        return Err(AppError::Settings(format!(
            "LXMF local delivery cache exceeds the {LXMF_LOCAL_DELIVERY_CACHE_MAX_BYTES} byte limit: {}",
            path.display()
        )));
    }
    let file = File::open(path)?;
    let opened_metadata = file.metadata()?;
    if !opened_metadata.is_file() {
        return Err(AppError::Settings(format!(
            "LXMF local delivery cache must open as a regular file: {}",
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
                "LXMF local delivery cache changed while it was being opened: {}",
                path.display()
            )));
        }
    }
    let mut data = Vec::with_capacity(path_metadata.len() as usize);
    file.take(LXMF_LOCAL_DELIVERY_CACHE_MAX_BYTES.saturating_add(1))
        .read_to_end(&mut data)?;
    if data.len() as u64 > LXMF_LOCAL_DELIVERY_CACHE_MAX_BYTES {
        return Err(AppError::Settings(format!(
            "LXMF local delivery cache exceeds the {LXMF_LOCAL_DELIVERY_CACHE_MAX_BYTES} byte limit: {}",
            path.display()
        )));
    }
    Ok(Some(data))
}

#[derive(Clone, Copy)]
enum PublishMode {
    CreateNew,
    Replace,
}

fn publish_cache_bytes(
    path: &Path,
    raw: &[u8],
    mode: PublishMode,
    before_commit: impl FnOnce() -> std::io::Result<()>,
) -> AppResult<()> {
    if raw.len() as u64 > LXMF_LOCAL_DELIVERY_CACHE_MAX_BYTES {
        return Err(AppError::Settings(format!(
            "LXMF local delivery cache exceeds the {LXMF_LOCAL_DELIVERY_CACHE_MAX_BYTES} byte limit"
        )));
    }
    let parent = path.parent().ok_or_else(|| {
        std::io::Error::new(ErrorKind::InvalidInput, "cache destination has no parent")
    })?;
    fs::create_dir_all(parent)?;
    if !fs::symlink_metadata(parent)?.file_type().is_dir() {
        return Err(AppError::Settings(format!(
            "LXMF local delivery cache parent must be a directory: {}",
            parent.display()
        )));
    }
    match (mode, fs::symlink_metadata(path)) {
        (PublishMode::CreateNew, Ok(_)) => {
            return Err(std::io::Error::new(
                ErrorKind::AlreadyExists,
                "cache destination already exists",
            )
            .into());
        }
        (PublishMode::Replace, Ok(metadata)) if !metadata.file_type().is_file() => {
            return Err(AppError::Settings(format!(
                "LXMF local delivery cache target must be a regular file: {}",
                path.display()
            )));
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
                "cache destination has no safe filename",
            )
        })?;
    let sequence = CACHE_FILE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let temporary = parent.join(format!(
        ".{file_name}.{}.{}.cache.tmp",
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
        before_commit()?;
        match mode {
            PublishMode::CreateNew => {
                fs::hard_link(&temporary, path)?;
                sync_directory(parent)?;
                fs::remove_file(&temporary)?;
            }
            PublishMode::Replace => crate::storage::files::atomic_replace(&temporary, path)?,
        }
        sync_directory(parent)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result.map_err(Into::into)
}

fn backup_corrupt_cache(path: &Path, raw: &[u8]) -> AppResult<()> {
    let parent = path.parent().ok_or_else(|| {
        std::io::Error::new(ErrorKind::InvalidInput, "cache destination has no parent")
    })?;
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| {
            std::io::Error::new(
                ErrorKind::InvalidInput,
                "cache destination has no safe filename",
            )
        })?;
    let sequence = CACHE_FILE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let backup = parent.join(format!(
        "{file_name}.corrupt.{}.{}.{}.bak",
        timestamp_nanos(),
        std::process::id(),
        sequence
    ));
    publish_cache_bytes(&backup, raw, PublishMode::CreateNew, || Ok(()))?;
    prune_corrupt_backups(path)
}

fn prune_corrupt_backups(path: &Path) -> AppResult<()> {
    let parent = path.parent().ok_or_else(|| {
        std::io::Error::new(ErrorKind::InvalidInput, "cache destination has no parent")
    })?;
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| {
            std::io::Error::new(
                ErrorKind::InvalidInput,
                "cache destination has no safe filename",
            )
        })?;
    let prefix = format!("{file_name}.corrupt.");
    let mut backups = Vec::new();
    let mut total_bytes = 0_u64;
    for (scanned, entry) in fs::read_dir(parent)?.enumerate() {
        if scanned == LXMF_LOCAL_DELIVERY_CACHE_BACKUP_MAX_SCAN_ENTRIES {
            return Err(AppError::Settings(format!(
                "LXMF local delivery cache backup discovery exceeds the {} entry scan limit",
                LXMF_LOCAL_DELIVERY_CACHE_BACKUP_MAX_SCAN_ENTRIES
            )));
        }
        let entry = entry?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        let Some(body) = name
            .strip_prefix(&prefix)
            .and_then(|name| name.strip_suffix(".bak"))
        else {
            continue;
        };
        if body.split('.').count() != 3
            || !body
                .split('.')
                .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
            || !entry.file_type()?.is_file()
        {
            continue;
        }
        let bytes = entry.metadata()?.len();
        total_bytes = total_bytes.saturating_add(bytes);
        backups.push((name.to_owned(), entry.path(), bytes));
    }
    backups.sort_by(|left, right| left.0.cmp(&right.0));
    let mut retained = backups.len();
    let mut removed = false;
    for (_, backup, bytes) in backups {
        if retained <= LXMF_LOCAL_DELIVERY_CACHE_CORRUPT_BACKUP_MAX_FILES
            && total_bytes <= LXMF_LOCAL_DELIVERY_CACHE_CORRUPT_BACKUP_MAX_TOTAL_BYTES
        {
            break;
        }
        fs::remove_file(backup)?;
        retained = retained.saturating_sub(1);
        total_bytes = total_bytes.saturating_sub(bytes);
        removed = true;
    }
    if removed {
        sync_directory(parent)?;
    }
    Ok(())
}

fn sync_directory(path: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    File::open(path)?.sync_all()?;
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

fn timestamp_nanos() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
}

pub fn unix_timestamp_secs() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs_f64())
        .unwrap_or_default()
}

pub fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::{publish_cache_bytes, PublishMode};

    #[test]
    fn failed_replace_preserves_prior_cache_and_removes_stage() {
        let root = std::env::temp_dir().join(format!(
            "omenbrowser-rs-transient-replace-fault-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("create fixture");
        let target = root.join("ids.json");
        std::fs::write(&target, b"previous").expect("seed cache");

        let result = publish_cache_bytes(&target, b"replacement", PublishMode::Replace, || {
            Err(std::io::Error::other("injected pre-commit failure"))
        });

        assert!(result.is_err());
        assert_eq!(
            std::fs::read(&target).expect("read prior cache"),
            b"previous"
        );
        assert_eq!(std::fs::read_dir(&root).expect("list fixture").count(), 1);
        let _ = std::fs::remove_dir_all(root);
    }
}
