use std::collections::BTreeMap;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock, Weak};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::config::ServerConfig;
use crate::error::{ServerError, ServerResult};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UploadPolicy {
    pub cache_root: PathBuf,
    pub quota_bytes: u64,
}

impl UploadPolicy {
    pub fn from_config(config: &ServerConfig) -> Self {
        Self {
            cache_root: config.upload_cache_path(),
            quota_bytes: config.upload_quota_bytes,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UploadQuotaDecision {
    Disabled,
    TooLarge {
        quota_bytes: u64,
        incoming_bytes: u64,
    },
    Accepted(UploadQuotaPlan),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UploadQuotaPlan {
    pub identity_dir: PathBuf,
    pub current_bytes: u64,
    pub incoming_bytes: u64,
    pub quota_bytes: u64,
    pub evict: Vec<PathBuf>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoredUpload {
    pub path: PathBuf,
    pub bytes: u64,
    pub evicted: Vec<PathBuf>,
}

#[derive(Clone, Debug)]
struct UploadCacheEntry {
    path: PathBuf,
    bytes: u64,
    modified: SystemTime,
}

static UPLOAD_TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);
static IDENTITY_UPLOAD_LOCKS: OnceLock<Mutex<BTreeMap<String, Weak<Mutex<()>>>>> = OnceLock::new();

pub fn plan_upload(
    config: &ServerConfig,
    identity_hash: &[u8],
    incoming_bytes: u64,
) -> ServerResult<UploadQuotaDecision> {
    plan_upload_with_policy(
        &UploadPolicy::from_config(config),
        identity_hash,
        incoming_bytes,
    )
}

pub fn plan_upload_with_policy(
    policy: &UploadPolicy,
    identity_hash: &[u8],
    incoming_bytes: u64,
) -> ServerResult<UploadQuotaDecision> {
    let quota_bytes = policy.quota_bytes;
    if quota_bytes == 0 {
        return Ok(UploadQuotaDecision::Disabled);
    }
    if incoming_bytes > quota_bytes {
        return Ok(UploadQuotaDecision::TooLarge {
            quota_bytes,
            incoming_bytes,
        });
    }

    let identity_dir = upload_identity_dir_for_root(&policy.cache_root, identity_hash);
    let entries = upload_cache_entries(&identity_dir)?;
    let current_bytes = entries
        .iter()
        .fold(0u64, |total, entry| total.saturating_add(entry.bytes));
    let mut remaining = current_bytes.saturating_add(incoming_bytes);
    let mut evict = Vec::new();
    for entry in entries {
        if remaining <= quota_bytes {
            break;
        }
        remaining = remaining.saturating_sub(entry.bytes);
        evict.push(entry.path);
    }

    Ok(UploadQuotaDecision::Accepted(UploadQuotaPlan {
        identity_dir,
        current_bytes,
        incoming_bytes,
        quota_bytes,
        evict,
    }))
}

pub fn plan_upload_with_index(
    policy: &UploadPolicy,
    identity_hash: &[u8],
    incoming_bytes: u64,
    indexed: crate::store::UploadLedgerQuotaPlan,
) -> UploadQuotaDecision {
    if policy.quota_bytes == 0 {
        return UploadQuotaDecision::Disabled;
    }
    if incoming_bytes > policy.quota_bytes {
        return UploadQuotaDecision::TooLarge {
            quota_bytes: policy.quota_bytes,
            incoming_bytes,
        };
    }
    UploadQuotaDecision::Accepted(UploadQuotaPlan {
        identity_dir: upload_identity_dir_for_root(&policy.cache_root, identity_hash),
        current_bytes: indexed.current_bytes,
        incoming_bytes,
        quota_bytes: policy.quota_bytes,
        evict: indexed.evict_paths,
    })
}

pub fn store_upload(
    config: &ServerConfig,
    identity_hash: &[u8],
    filename_hint: &str,
    bytes: &[u8],
) -> ServerResult<StoredUpload> {
    store_upload_with_policy(
        &UploadPolicy::from_config(config),
        identity_hash,
        filename_hint,
        bytes,
    )
}

pub fn store_upload_with_policy(
    policy: &UploadPolicy,
    identity_hash: &[u8],
    filename_hint: &str,
    bytes: &[u8],
) -> ServerResult<StoredUpload> {
    store_upload_with_policy_and_commit(policy, identity_hash, filename_hint, bytes, |_| Ok(()))
}

pub fn store_upload_with_policy_and_commit<F>(
    policy: &UploadPolicy,
    identity_hash: &[u8],
    filename_hint: &str,
    bytes: &[u8],
    commit: F,
) -> ServerResult<StoredUpload>
where
    F: FnOnce(&StoredUpload) -> ServerResult<()>,
{
    store_upload_with_policy_and_ops(
        policy,
        identity_hash,
        filename_hint,
        bytes,
        commit,
        &RealUploadFileOps,
    )
}

pub fn store_upload_with_policy_indexed_and_commit<F, P>(
    policy: &UploadPolicy,
    identity_hash: &[u8],
    filename_hint: &str,
    bytes: &[u8],
    planner: P,
    commit: F,
) -> ServerResult<StoredUpload>
where
    F: FnOnce(&StoredUpload) -> ServerResult<()>,
    P: FnOnce(u64) -> ServerResult<UploadQuotaDecision>,
{
    store_upload_with_policy_and_planner_ops(
        policy,
        identity_hash,
        filename_hint,
        bytes,
        planner,
        commit,
        &RealUploadFileOps,
    )
}

pub fn create_channel_upload_stage(
    policy: &UploadPolicy,
    identity_hash: &[u8],
) -> ServerResult<(PathBuf, File)> {
    let identity_dir = upload_identity_dir_for_root(&policy.cache_root, identity_hash);
    ensure_safe_identity_dir(&policy.cache_root, &identity_dir)?;
    for _ in 0..32 {
        let sequence = UPLOAD_TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let path = identity_dir.join(format!(".omen-channel-{sequence:016x}.part"));
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        match options.open(&path) {
            Ok(file) => return Ok((path, file)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.into()),
        }
    }
    Err(ServerError::Message(
        "could not allocate a unique Channel upload staging file".into(),
    ))
}

pub fn commit_staged_upload_with_policy_indexed_and_commit<F, P>(
    policy: &UploadPolicy,
    identity_hash: &[u8],
    filename_hint: &str,
    staged_path: &Path,
    incoming_bytes: u64,
    planner: P,
    commit: F,
) -> ServerResult<StoredUpload>
where
    F: FnOnce(&StoredUpload) -> ServerResult<()>,
    P: FnOnce(u64) -> ServerResult<UploadQuotaDecision>,
{
    let identity_lock = identity_upload_lock(identity_hash);
    let _guard = identity_lock
        .lock()
        .map_err(|_| ServerError::Message("upload identity lock poisoned".into()))?;
    let plan = match planner(incoming_bytes)? {
        UploadQuotaDecision::Accepted(plan) => plan,
        UploadQuotaDecision::Disabled => {
            return Err(ServerError::Message(
                "uploads are disabled by server policy".into(),
            ));
        }
        UploadQuotaDecision::TooLarge {
            quota_bytes,
            incoming_bytes,
        } => {
            return Err(ServerError::Message(format!(
                "upload is too large for quota: {incoming_bytes} > {quota_bytes}"
            )));
        }
    };
    ensure_safe_identity_dir(&policy.cache_root, &plan.identity_dir)?;
    if staged_path.parent() != Some(plan.identity_dir.as_path())
        || std::fs::symlink_metadata(staged_path)?
            .file_type()
            .is_symlink()
        || std::fs::metadata(staged_path)?.len() != incoming_bytes
    {
        let _ = std::fs::remove_file(staged_path);
        return Err(ServerError::Message(
            "Channel upload staging file failed ownership or length validation".into(),
        ));
    }
    let path = next_upload_path(&plan.identity_dir, filename_hint);
    if let Err(error) = std::fs::rename(staged_path, &path) {
        let _ = std::fs::remove_file(staged_path);
        return Err(error.into());
    }
    if let Err(error) = RealUploadFileOps.sync_dir(&plan.identity_dir) {
        let _ = std::fs::remove_file(&path);
        let _ = RealUploadFileOps.sync_dir(&plan.identity_dir);
        return Err(error);
    }
    let pending = StoredUpload {
        path: path.clone(),
        bytes: incoming_bytes,
        evicted: plan.evict.clone(),
    };
    if let Err(error) = commit(&pending) {
        let _ = std::fs::remove_file(&path);
        let _ = RealUploadFileOps.sync_dir(&plan.identity_dir);
        return Err(error);
    }
    let mut evicted = Vec::new();
    for old_path in plan.evict {
        if old_path != path && std::fs::remove_file(&old_path).is_ok() {
            evicted.push(old_path);
        }
    }
    let _ = RealUploadFileOps.sync_dir(&plan.identity_dir);
    Ok(StoredUpload {
        path,
        bytes: incoming_bytes,
        evicted,
    })
}

fn store_upload_with_policy_and_ops<F, O>(
    policy: &UploadPolicy,
    identity_hash: &[u8],
    filename_hint: &str,
    bytes: &[u8],
    commit: F,
    ops: &O,
) -> ServerResult<StoredUpload>
where
    F: FnOnce(&StoredUpload) -> ServerResult<()>,
    O: UploadFileOps,
{
    store_upload_with_policy_and_planner_ops(
        policy,
        identity_hash,
        filename_hint,
        bytes,
        |incoming_bytes| plan_upload_with_policy(policy, identity_hash, incoming_bytes),
        commit,
        ops,
    )
}

fn store_upload_with_policy_and_planner_ops<F, P, O>(
    policy: &UploadPolicy,
    identity_hash: &[u8],
    filename_hint: &str,
    bytes: &[u8],
    planner: P,
    commit: F,
    ops: &O,
) -> ServerResult<StoredUpload>
where
    F: FnOnce(&StoredUpload) -> ServerResult<()>,
    P: FnOnce(u64) -> ServerResult<UploadQuotaDecision>,
    O: UploadFileOps,
{
    let identity_lock = identity_upload_lock(identity_hash);
    let _guard = identity_lock
        .lock()
        .map_err(|_| ServerError::Message("upload identity lock poisoned".into()))?;
    let plan = match planner(bytes.len() as u64)? {
        UploadQuotaDecision::Accepted(plan) => plan,
        UploadQuotaDecision::Disabled => {
            return Err(ServerError::Message(
                "uploads are disabled by server policy".into(),
            ));
        }
        UploadQuotaDecision::TooLarge {
            quota_bytes,
            incoming_bytes,
        } => {
            return Err(ServerError::Message(format!(
                "upload is too large for quota: {incoming_bytes} > {quota_bytes}"
            )));
        }
    };

    ensure_safe_identity_dir(&policy.cache_root, &plan.identity_dir)?;
    let path = next_upload_path(&plan.identity_dir, filename_hint);
    let temp_path = create_temp_path(&plan.identity_dir)?;
    let write_result = (|| {
        let mut file = ops.create_temp(&temp_path)?;
        ops.write_all(&mut file, bytes)?;
        ops.flush(&mut file)?;
        ops.sync_file(&file)?;
        drop(file);
        ops.rename(&temp_path, &path)?;
        ops.sync_dir(&plan.identity_dir)?;
        Ok(())
    })();
    if let Err(error) = write_result {
        let _ = ops.remove_file(&temp_path);
        let _ = ops.remove_file(&path);
        let _ = ops.sync_dir(&plan.identity_dir);
        return Err(error);
    }

    let pending = StoredUpload {
        path: path.clone(),
        bytes: bytes.len() as u64,
        // During the commit callback this is the planned eviction set. The
        // returned StoredUpload below contains only successfully removed files.
        evicted: plan.evict.clone(),
    };
    if let Err(error) = commit(&pending) {
        let _ = ops.remove_file(&path);
        let _ = ops.sync_dir(&plan.identity_dir);
        return Err(error);
    }

    let mut evicted = Vec::new();
    for old_path in plan.evict {
        if old_path == path {
            continue;
        }
        if ops.remove_file(&old_path).is_ok() {
            evicted.push(old_path);
        }
    }
    // The replacement and its database row are already committed. Failure to
    // persist an eviction must leave an extra old file, not turn a valid new
    // upload into an apparent failure that clients may retry.
    let _ = ops.sync_dir(&plan.identity_dir);
    Ok(StoredUpload {
        path,
        bytes: pending.bytes,
        evicted,
    })
}

trait UploadFileOps {
    fn create_temp(&self, path: &Path) -> ServerResult<File>;
    fn write_all(&self, file: &mut File, bytes: &[u8]) -> ServerResult<()>;
    fn flush(&self, file: &mut File) -> ServerResult<()>;
    fn sync_file(&self, file: &File) -> ServerResult<()>;
    fn rename(&self, from: &Path, to: &Path) -> ServerResult<()>;
    fn remove_file(&self, path: &Path) -> ServerResult<()>;
    fn sync_dir(&self, path: &Path) -> ServerResult<()>;
}

struct RealUploadFileOps;

impl UploadFileOps for RealUploadFileOps {
    fn create_temp(&self, path: &Path) -> ServerResult<File> {
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        Ok(options.open(path)?)
    }

    fn write_all(&self, file: &mut File, bytes: &[u8]) -> ServerResult<()> {
        file.write_all(bytes)?;
        Ok(())
    }

    fn flush(&self, file: &mut File) -> ServerResult<()> {
        file.flush()?;
        Ok(())
    }

    fn sync_file(&self, file: &File) -> ServerResult<()> {
        file.sync_all()?;
        Ok(())
    }

    fn rename(&self, from: &Path, to: &Path) -> ServerResult<()> {
        std::fs::rename(from, to)?;
        Ok(())
    }

    fn remove_file(&self, path: &Path) -> ServerResult<()> {
        std::fs::remove_file(path)?;
        Ok(())
    }

    fn sync_dir(&self, path: &Path) -> ServerResult<()> {
        sync_directory(path)
    }
}

fn identity_upload_lock(identity_hash: &[u8]) -> Arc<Mutex<()>> {
    let key = hex_lower(identity_hash);
    let locks = IDENTITY_UPLOAD_LOCKS.get_or_init(|| Mutex::new(BTreeMap::new()));
    let mut locks = locks.lock().unwrap_or_else(|error| error.into_inner());
    locks.retain(|_, lock| lock.strong_count() > 0);
    if let Some(lock) = locks.get(&key).and_then(Weak::upgrade) {
        return lock;
    }
    let lock = Arc::new(Mutex::new(()));
    locks.insert(key, Arc::downgrade(&lock));
    lock
}

fn ensure_safe_identity_dir(cache_root: &Path, identity_dir: &Path) -> ServerResult<()> {
    crate::private_fs::ensure_private_dir(cache_root)?;
    if identity_dir.parent() != Some(cache_root) {
        return Err(ServerError::Message(
            "upload identity directory escapes configured cache root".into(),
        ));
    }
    match std::fs::symlink_metadata(identity_dir) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                return Err(ServerError::Message(
                    "upload identity path must be a real directory".into(),
                ));
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            crate::private_fs::ensure_private_dir(identity_dir)?;
        }
        Err(error) => return Err(error.into()),
    }
    Ok(())
}

fn create_temp_path(identity_dir: &Path) -> ServerResult<PathBuf> {
    for _ in 0..1000 {
        let sequence = UPLOAD_TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        let path = identity_dir.join(format!(
            ".omen-upload-{}-{stamp}-{sequence}.tmp",
            std::process::id()
        ));
        if !path.exists() {
            return Ok(path);
        }
    }
    Err(ServerError::Message(
        "could not allocate unique upload temporary path".into(),
    ))
}

#[cfg(unix)]
fn sync_directory(path: &Path) -> ServerResult<()> {
    File::open(path)?.sync_all()?;
    Ok(())
}

#[cfg(not(unix))]
fn sync_directory(_path: &Path) -> ServerResult<()> {
    Ok(())
}

pub fn upload_identity_dir(config: &ServerConfig, identity_hash: &[u8]) -> PathBuf {
    upload_identity_dir_for_root(&config.upload_cache_path(), identity_hash)
}

pub fn upload_identity_dir_for_root(cache_root: &Path, identity_hash: &[u8]) -> PathBuf {
    cache_root.join(hex_lower(identity_hash))
}

fn upload_cache_entries(identity_dir: &Path) -> ServerResult<Vec<UploadCacheEntry>> {
    let Ok(read_dir) = std::fs::read_dir(identity_dir) else {
        return Ok(Vec::new());
    };
    let mut entries = Vec::new();
    for entry in read_dir {
        let entry = entry?;
        if entry
            .file_name()
            .to_string_lossy()
            .starts_with(".omen-upload-")
        {
            continue;
        }
        let metadata = std::fs::symlink_metadata(entry.path())?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            continue;
        }
        entries.push(UploadCacheEntry {
            path: entry.path(),
            bytes: metadata.len(),
            modified: metadata.modified().unwrap_or(UNIX_EPOCH),
        });
    }
    entries.sort_by(|left, right| {
        left.modified
            .cmp(&right.modified)
            .then_with(|| left.path.cmp(&right.path))
    });
    Ok(entries)
}

fn next_upload_path(identity_dir: &Path, filename_hint: &str) -> PathBuf {
    let filename = sanitize_upload_filename(filename_hint);
    let mut candidate = identity_dir.join(&filename);
    if !candidate.exists() {
        return candidate;
    }
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0);
    for index in 1..=1000 {
        candidate = identity_dir.join(format!("{stamp}-{index}-{filename}"));
        if !candidate.exists() {
            return candidate;
        }
    }
    identity_dir.join(format!("{stamp}-{filename}"))
}

fn sanitize_upload_filename(filename_hint: &str) -> String {
    let hint = Path::new(filename_hint)
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or(filename_hint);
    let cleaned = hint
        .trim()
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | '_') {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>();
    let cleaned = cleaned.trim_matches('.').trim_matches('_');
    if cleaned.is_empty() {
        "upload.bin".into()
    } else {
        cleaned.chars().take(96).collect()
    }
}

fn hex_lower(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const UPLOAD_CRASH_ROOT_ENV: &str = "OMENCHATD_UPLOAD_CRASH_ROOT";
    const UPLOAD_CRASH_MODE_ENV: &str = "OMENCHATD_UPLOAD_CRASH_MODE";
    const UPLOAD_CRASH_READY_ENV: &str = "OMENCHATD_UPLOAD_CRASH_READY";
    const UPLOAD_CRASH_IDENTITY: &[u8] = b"process-kill-upload-peer";

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum FaultStage {
        Create,
        Write,
        Flush,
        SyncFile,
        Rename,
        SyncDir,
    }

    struct FaultUploadFileOps {
        stage: FaultStage,
    }

    impl FaultUploadFileOps {
        fn fail(&self, stage: FaultStage) -> ServerResult<()> {
            if self.stage == stage {
                Err(ServerError::Io(std::io::Error::other(format!(
                    "injected {stage:?} failure"
                ))))
            } else {
                Ok(())
            }
        }
    }

    impl UploadFileOps for FaultUploadFileOps {
        fn create_temp(&self, path: &Path) -> ServerResult<File> {
            self.fail(FaultStage::Create)?;
            RealUploadFileOps.create_temp(path)
        }

        fn write_all(&self, file: &mut File, bytes: &[u8]) -> ServerResult<()> {
            self.fail(FaultStage::Write)?;
            RealUploadFileOps.write_all(file, bytes)
        }

        fn flush(&self, file: &mut File) -> ServerResult<()> {
            self.fail(FaultStage::Flush)?;
            RealUploadFileOps.flush(file)
        }

        fn sync_file(&self, file: &File) -> ServerResult<()> {
            self.fail(FaultStage::SyncFile)?;
            RealUploadFileOps.sync_file(file)
        }

        fn rename(&self, from: &Path, to: &Path) -> ServerResult<()> {
            self.fail(FaultStage::Rename)?;
            RealUploadFileOps.rename(from, to)
        }

        fn remove_file(&self, path: &Path) -> ServerResult<()> {
            RealUploadFileOps.remove_file(path)
        }

        fn sync_dir(&self, path: &Path) -> ServerResult<()> {
            self.fail(FaultStage::SyncDir)?;
            RealUploadFileOps.sync_dir(path)
        }
    }

    #[cfg(target_os = "linux")]
    struct KernelEnospcUploadFileOps;

    #[cfg(target_os = "linux")]
    impl UploadFileOps for KernelEnospcUploadFileOps {
        fn create_temp(&self, _path: &Path) -> ServerResult<File> {
            Ok(OpenOptions::new().write(true).open("/dev/full")?)
        }

        fn write_all(&self, file: &mut File, bytes: &[u8]) -> ServerResult<()> {
            RealUploadFileOps.write_all(file, bytes)
        }

        fn flush(&self, file: &mut File) -> ServerResult<()> {
            RealUploadFileOps.flush(file)
        }

        fn sync_file(&self, file: &File) -> ServerResult<()> {
            RealUploadFileOps.sync_file(file)
        }

        fn rename(&self, from: &Path, to: &Path) -> ServerResult<()> {
            RealUploadFileOps.rename(from, to)
        }

        fn remove_file(&self, path: &Path) -> ServerResult<()> {
            match RealUploadFileOps.remove_file(path) {
                Ok(()) => Ok(()),
                Err(ServerError::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {
                    Ok(())
                }
                Err(error) => Err(error),
            }
        }

        fn sync_dir(&self, path: &Path) -> ServerResult<()> {
            RealUploadFileOps.sync_dir(path)
        }
    }

    fn temp_root(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!("omenchatd-upload-{label}-{}", std::process::id()))
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum CrashUploadBoundary {
        TempSynced,
        ReplacementRenamed,
    }

    struct CrashBoundaryUploadFileOps {
        boundary: CrashUploadBoundary,
        ready: PathBuf,
    }

    impl CrashBoundaryUploadFileOps {
        fn stop_at(&self, boundary: CrashUploadBoundary) {
            if self.boundary == boundary {
                publish_upload_crash_boundary(&self.ready);
                loop {
                    std::thread::park();
                }
            }
        }
    }

    impl UploadFileOps for CrashBoundaryUploadFileOps {
        fn create_temp(&self, path: &Path) -> ServerResult<File> {
            RealUploadFileOps.create_temp(path)
        }

        fn write_all(&self, file: &mut File, bytes: &[u8]) -> ServerResult<()> {
            RealUploadFileOps.write_all(file, bytes)
        }

        fn flush(&self, file: &mut File) -> ServerResult<()> {
            RealUploadFileOps.flush(file)
        }

        fn sync_file(&self, file: &File) -> ServerResult<()> {
            RealUploadFileOps.sync_file(file)?;
            self.stop_at(CrashUploadBoundary::TempSynced);
            Ok(())
        }

        fn rename(&self, from: &Path, to: &Path) -> ServerResult<()> {
            RealUploadFileOps.rename(from, to)
        }

        fn remove_file(&self, path: &Path) -> ServerResult<()> {
            RealUploadFileOps.remove_file(path)
        }

        fn sync_dir(&self, path: &Path) -> ServerResult<()> {
            RealUploadFileOps.sync_dir(path)?;
            self.stop_at(CrashUploadBoundary::ReplacementRenamed);
            Ok(())
        }
    }

    fn publish_upload_crash_boundary(ready: &Path) {
        let mut marker = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(ready)
            .expect("create upload crash-boundary marker");
        marker
            .write_all(b"ready\n")
            .expect("write upload boundary marker");
        marker.sync_all().expect("sync upload boundary marker");
    }

    fn stop_upload_crash_child(ready: &Path) -> ! {
        publish_upload_crash_boundary(ready);
        loop {
            std::thread::park();
        }
    }

    fn wait_for_upload_boundary_and_kill(root: &Path, ready: &Path, mode: &str) {
        let _ = std::fs::remove_file(ready);
        let mut child = std::process::Command::new(std::env::current_exe().expect("test binary"))
            .args([
                "--exact",
                "upload::tests::process_kill_upload_boundary_child",
                "--nocapture",
                "--test-threads=1",
            ])
            .env(UPLOAD_CRASH_ROOT_ENV, root)
            .env(UPLOAD_CRASH_MODE_ENV, mode)
            .env(UPLOAD_CRASH_READY_ENV, ready)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("spawn isolated upload crash-boundary child");

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        loop {
            if ready.is_file() {
                break;
            }
            if let Some(status) = child.try_wait().expect("poll upload crash child") {
                panic!("upload crash child exited before {mode} marker: {status}");
            }
            if std::time::Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                panic!("upload crash child did not reach {mode} marker");
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        child.kill().expect("kill upload crash child");
        let status = child.wait().expect("reap upload crash child");
        assert!(
            !status.success(),
            "killed upload child unexpectedly exited cleanly"
        );
    }

    fn setup_upload_crash_root(label: &str) -> (PathBuf, PathBuf, PathBuf) {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "omenchatd-upload-process-kill-{label}-{}-{nonce}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).expect("isolated upload crash root");
        let database = root.join("omenchat.sqlite");
        let cache_root = root.join("uploads");
        let identity_dir = upload_identity_dir_for_root(&cache_root, UPLOAD_CRASH_IDENTITY);
        let policy = UploadPolicy {
            cache_root,
            quota_bytes: 5,
        };
        let store = crate::store::OmenchatStore::open(&database).expect("setup upload store");
        let room = store.ensure_room("lobby", None).expect("lobby");
        let user = store
            .ensure_user(UPLOAD_CRASH_IDENTITY, "Crash Upload", None)
            .expect("upload user");
        assert_eq!(room.room_id, 1);
        assert_eq!(user.user_id, 1);
        let old = store_upload_with_policy(&policy, UPLOAD_CRASH_IDENTITY, "old.bin", b"12345")
            .expect("seed old upload");
        store
            .record_upload_file(crate::store::RecordUploadFile {
                resource_id: "a-old",
                room_id: room.room_id,
                actor_user_id: user.user_id,
                filename: "old.bin",
                content_type: None,
                byte_len: old.bytes,
                path: &old.path,
            })
            .expect("seed old upload ledger");
        drop(store);
        (root, database, identity_dir)
    }

    #[test]
    fn process_kill_upload_boundary_child() {
        let Some(root) = std::env::var_os(UPLOAD_CRASH_ROOT_ENV) else {
            return;
        };
        let root = PathBuf::from(root);
        let mode = std::env::var(UPLOAD_CRASH_MODE_ENV).expect("upload crash mode");
        let ready = PathBuf::from(
            std::env::var_os(UPLOAD_CRASH_READY_ENV).expect("upload crash ready marker"),
        );
        let database = root.join("omenchat.sqlite");
        let policy = UploadPolicy {
            cache_root: root.join("uploads"),
            quota_bytes: 5,
        };
        let identity_dir = upload_identity_dir_for_root(&policy.cache_root, UPLOAD_CRASH_IDENTITY);
        let store = crate::store::OmenchatStore::open(&database).expect("open upload crash store");

        let planner = |incoming_bytes| {
            let indexed = store.plan_upload_from_index(1, &identity_dir, incoming_bytes, 5)?;
            Ok(plan_upload_with_index(
                &policy,
                UPLOAD_CRASH_IDENTITY,
                incoming_bytes,
                indexed,
            ))
        };
        let record_replacement = |pending: &StoredUpload| {
            store.record_upload_file(crate::store::RecordUploadFile {
                resource_id: "z-new",
                room_id: 1,
                actor_user_id: 1,
                filename: "new.bin",
                content_type: None,
                byte_len: pending.bytes,
                path: &pending.path,
            })
        };

        match mode.as_str() {
            "temp-synced" | "renamed-before-ledger" => {
                let boundary = if mode == "temp-synced" {
                    CrashUploadBoundary::TempSynced
                } else {
                    CrashUploadBoundary::ReplacementRenamed
                };
                let ops = CrashBoundaryUploadFileOps { boundary, ready };
                store_upload_with_policy_and_planner_ops(
                    &policy,
                    UPLOAD_CRASH_IDENTITY,
                    "new.bin",
                    b"abcde",
                    planner,
                    record_replacement,
                    &ops,
                )
                .expect("boundary must stop before returning");
            }
            "ledger-before-eviction" => {
                store_upload_with_policy_indexed_and_commit(
                    &policy,
                    UPLOAD_CRASH_IDENTITY,
                    "new.bin",
                    b"abcde",
                    planner,
                    |pending| {
                        record_replacement(pending)?;
                        stop_upload_crash_child(&ready);
                    },
                )
                .expect("ledger boundary must stop before returning");
            }
            "eviction-before-cleanup" => {
                let stored = store_upload_with_policy_indexed_and_commit(
                    &policy,
                    UPLOAD_CRASH_IDENTITY,
                    "new.bin",
                    b"abcde",
                    planner,
                    record_replacement,
                )
                .expect("replace before cleanup boundary");
                assert_eq!(stored.evicted.len(), 1);
                stop_upload_crash_child(&ready);
            }
            other => panic!("unknown upload crash mode: {other}"),
        }
    }

    fn assert_sqlite_integrity(database: &Path) {
        let connection = rusqlite::Connection::open(database).expect("open integrity database");
        let integrity: String = connection
            .query_row("PRAGMA integrity_check", [], |row| row.get(0))
            .expect("SQLite integrity check");
        assert_eq!(integrity, "ok");
    }

    #[test]
    fn process_kill_upload_recovery_is_conservative_at_every_commit_boundary() {
        for mode in ["temp-synced", "renamed-before-ledger"] {
            let (root, database, identity_dir) = setup_upload_crash_root(mode);
            let ready = root.join("boundary.ready");
            wait_for_upload_boundary_and_kill(&root, &ready, mode);
            let store = crate::store::OmenchatStore::open(&database).expect("reopen upload store");
            let report = store
                .reconcile_upload_ledger(1, &identity_dir)
                .expect("reconcile pre-ledger kill");
            assert_eq!(report.tracked_files, 1);
            assert_eq!(report.missing_paths.len(), 0);
            assert_eq!(report.orphan_paths.len(), 1);
            assert_eq!(
                std::fs::read(identity_dir.join("old.bin")).expect("old upload"),
                b"12345"
            );
            let error = store
                .plan_upload_from_index(1, &identity_dir, 1, 5)
                .expect_err("orphan must block upload admission")
                .to_string();
            assert!(error.contains("orphan=1"), "unexpected error: {error}");
            drop(store);
            assert_sqlite_integrity(&database);
            std::fs::remove_dir_all(root).expect("remove pre-ledger crash root");
        }

        let (root, database, identity_dir) = setup_upload_crash_root("ledger-before-eviction");
        let ready = root.join("boundary.ready");
        wait_for_upload_boundary_and_kill(&root, &ready, "ledger-before-eviction");
        let store = crate::store::OmenchatStore::open(&database).expect("reopen committed ledger");
        let report = store
            .reconcile_upload_ledger(1, &identity_dir)
            .expect("reconcile committed replacement");
        assert_eq!(report.tracked_files, 2);
        assert_eq!(report.disk_files, 2);
        assert!(report.missing_paths.is_empty());
        assert!(report.orphan_paths.is_empty());
        let plan = store
            .plan_upload_from_index(1, &identity_dir, 0, 5)
            .expect("committed replacement safely over-counts quota");
        assert_eq!(plan.current_bytes, 10);
        assert_eq!(plan.evict_paths, vec![identity_dir.join("old.bin")]);
        drop(store);
        assert_sqlite_integrity(&database);
        std::fs::remove_dir_all(root).expect("remove committed-ledger crash root");

        let (root, database, identity_dir) = setup_upload_crash_root("eviction-before-cleanup");
        let ready = root.join("boundary.ready");
        wait_for_upload_boundary_and_kill(&root, &ready, "eviction-before-cleanup");
        let store = crate::store::OmenchatStore::open(&database).expect("reopen evicted upload");
        let report = store
            .reconcile_upload_ledger(1, &identity_dir)
            .expect("reconcile stale ledger row");
        assert_eq!(report.tracked_files, 2);
        assert_eq!(report.disk_files, 1);
        assert_eq!(report.missing_paths, vec![identity_dir.join("old.bin")]);
        let error = store
            .plan_upload_from_index(1, &identity_dir, 1, 5)
            .expect_err("missing committed file must block admission")
            .to_string();
        assert!(error.contains("missing=1"), "unexpected error: {error}");
        let repair = store
            .repair_upload_ledger_records(1, &identity_dir)
            .expect("remove only stale missing row");
        assert_eq!(repair.removed_missing_records, 1);
        assert_eq!(repair.removed_unsafe_records, 0);
        store.invalidate_upload_ledger(1);
        let clean = store
            .plan_upload_from_index(1, &identity_dir, 0, 5)
            .expect("repaired replacement ledger");
        assert_eq!(clean.current_bytes, 5);
        assert!(clean.evict_paths.is_empty());
        assert_eq!(
            std::fs::read(identity_dir.join("new.bin")).expect("new upload"),
            b"abcde"
        );
        drop(store);
        assert_sqlite_integrity(&database);
        std::fs::remove_dir_all(root).expect("remove eviction crash root");
    }

    #[test]
    fn upload_quota_zero_disables_uploads() {
        let root = temp_root("disabled");
        let _ = std::fs::remove_dir_all(&root);
        let mut config = ServerConfig::for_root(root.clone());
        config.upload_quota_bytes = 0;

        assert_eq!(
            plan_upload(&config, b"peer-a", 1).expect("plan"),
            UploadQuotaDecision::Disabled
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn upload_quota_rejects_single_file_larger_than_quota() {
        let root = temp_root("too-large");
        let _ = std::fs::remove_dir_all(&root);
        let mut config = ServerConfig::for_root(root.clone());
        config.upload_quota_bytes = 10;

        assert_eq!(
            plan_upload(&config, b"peer-a", 11).expect("plan"),
            UploadQuotaDecision::TooLarge {
                quota_bytes: 10,
                incoming_bytes: 11
            }
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn upload_store_evicts_oldest_files_inside_identity_quota() {
        let root = temp_root("evict");
        let _ = std::fs::remove_dir_all(&root);
        let mut config = ServerConfig::for_root(root.clone());
        config.upload_quota_bytes = 10;
        let identity = b"peer-a";

        let first = store_upload(&config, identity, "first.txt", b"12345").expect("first");
        std::thread::sleep(std::time::Duration::from_millis(2));
        let second = store_upload(&config, identity, "second.txt", b"6789").expect("second");
        std::thread::sleep(std::time::Duration::from_millis(2));
        let third = store_upload(&config, identity, "third.txt", b"abcde").expect("third");

        assert!(!first.path.exists());
        assert!(second.path.exists());
        assert!(third.path.exists());
        assert_eq!(third.evicted, vec![first.path]);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn upload_filename_sanitizer_stays_inside_cache_dir() {
        assert_eq!(
            sanitize_upload_filename("../../secret file.png"),
            "secret_file.png"
        );
        assert_eq!(sanitize_upload_filename(""), "upload.bin");
    }

    #[test]
    fn upload_faults_never_evict_last_committed_file() {
        for stage in [
            FaultStage::Create,
            FaultStage::Write,
            FaultStage::Flush,
            FaultStage::SyncFile,
            FaultStage::Rename,
            FaultStage::SyncDir,
        ] {
            let root = temp_root(&format!("fault-{stage:?}"));
            let _ = std::fs::remove_dir_all(&root);
            let policy = UploadPolicy {
                cache_root: root.clone(),
                quota_bytes: 5,
            };
            let identity = b"peer-fault";
            let old = store_upload_with_policy(&policy, identity, "old.bin", b"12345")
                .expect("seed committed upload");

            let result = store_upload_with_policy_and_ops(
                &policy,
                identity,
                "new.bin",
                b"abcde",
                |_| Ok(()),
                &FaultUploadFileOps { stage },
            );
            assert!(result.is_err(), "{stage:?} must fail");
            assert_eq!(std::fs::read(&old.path).expect("old upload"), b"12345");
            assert!(
                upload_cache_entries(old.path.parent().expect("identity dir"))
                    .expect("entries")
                    .iter()
                    .all(|entry| entry.path == old.path)
            );
            let _ = std::fs::remove_dir_all(root);
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn kernel_enospc_preserves_last_committed_upload() {
        assert!(
            Path::new("/dev/full").exists(),
            "Linux /dev/full is required"
        );
        let root = temp_root("kernel-enospc");
        let _ = std::fs::remove_dir_all(&root);
        let policy = UploadPolicy {
            cache_root: root.clone(),
            quota_bytes: 5,
        };
        let identity = b"peer-enospc";
        let old = store_upload_with_policy(&policy, identity, "old.bin", b"12345")
            .expect("seed committed upload");

        let error = store_upload_with_policy_and_planner_ops(
            &policy,
            identity,
            "new.bin",
            b"abcde",
            |incoming_bytes| plan_upload_with_policy(&policy, identity, incoming_bytes),
            |_| panic!("database commit must not run after ENOSPC"),
            &KernelEnospcUploadFileOps,
        )
        .expect_err("/dev/full must reject the replacement write");
        assert!(
            matches!(error, ServerError::Io(ref io) if io.raw_os_error() == Some(28)),
            "expected Linux ENOSPC (28), got {error}"
        );
        assert_eq!(std::fs::read(&old.path).expect("old upload"), b"12345");
        assert_eq!(
            upload_cache_entries(old.path.parent().expect("identity dir"))
                .expect("entries")
                .len(),
            1
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn database_commit_failure_removes_replacement_before_eviction() {
        let root = temp_root("commit-failure");
        let _ = std::fs::remove_dir_all(&root);
        let policy = UploadPolicy {
            cache_root: root.clone(),
            quota_bytes: 5,
        };
        let identity = b"peer-commit";
        let old = store_upload_with_policy(&policy, identity, "old.bin", b"12345")
            .expect("seed committed upload");

        let result =
            store_upload_with_policy_and_commit(&policy, identity, "new.bin", b"abcde", |_| {
                Err(ServerError::Message("injected database failure".into()))
            });
        assert!(result.is_err());
        assert_eq!(std::fs::read(&old.path).expect("old upload"), b"12345");
        assert_eq!(
            upload_cache_entries(old.path.parent().expect("identity dir"))
                .expect("entries")
                .len(),
            1
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn old_upload_is_evicted_only_after_replacement_commit_callback() {
        let root = temp_root("commit-order");
        let _ = std::fs::remove_dir_all(&root);
        let policy = UploadPolicy {
            cache_root: root.clone(),
            quota_bytes: 5,
        };
        let identity = b"peer-order";
        let old = store_upload_with_policy(&policy, identity, "old.bin", b"12345")
            .expect("seed committed upload");

        let replacement = store_upload_with_policy_and_commit(
            &policy,
            identity,
            "new.bin",
            b"abcde",
            |pending| {
                assert!(old.path.exists(), "old upload must exist during DB commit");
                assert_eq!(std::fs::read(&pending.path)?, b"abcde");
                Ok(())
            },
        )
        .expect("replacement");
        assert!(!old.path.exists());
        assert_eq!(replacement.evicted, vec![old.path]);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn concurrent_same_identity_uploads_cannot_exceed_quota() {
        let root = temp_root("concurrent");
        let _ = std::fs::remove_dir_all(&root);
        let policy = UploadPolicy {
            cache_root: root.clone(),
            quota_bytes: 10,
        };
        let barrier = Arc::new(std::sync::Barrier::new(3));
        let mut threads = Vec::new();
        for index in 0..2 {
            let policy = policy.clone();
            let barrier = barrier.clone();
            threads.push(std::thread::spawn(move || {
                barrier.wait();
                store_upload_with_policy(
                    &policy,
                    b"peer-concurrent",
                    &format!("upload-{index}.bin"),
                    b"1234567",
                )
            }));
        }
        barrier.wait();
        for thread in threads {
            thread.join().expect("thread").expect("upload");
        }

        let identity_dir = upload_identity_dir_for_root(&root, b"peer-concurrent");
        let entries = upload_cache_entries(&identity_dir).expect("entries");
        assert!(entries.iter().map(|entry| entry.bytes).sum::<u64>() <= 10);
        assert_eq!(entries.len(), 1);
        let _ = std::fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn upload_identity_symlink_is_rejected() {
        use std::os::unix::fs::symlink;

        let root = temp_root("symlink");
        let outside = temp_root("symlink-outside");
        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_dir_all(&outside);
        std::fs::create_dir_all(&root).expect("root");
        std::fs::create_dir_all(&outside).expect("outside");
        let identity_dir = upload_identity_dir_for_root(&root, b"peer-link");
        symlink(&outside, &identity_dir).expect("symlink");
        let policy = UploadPolicy {
            cache_root: root.clone(),
            quota_bytes: 10,
        };

        assert!(store_upload_with_policy(&policy, b"peer-link", "x.bin", b"x").is_err());
        assert!(std::fs::read_dir(&outside)
            .expect("outside entries")
            .next()
            .is_none());
        let _ = std::fs::remove_dir_all(root);
        let _ = std::fs::remove_dir_all(outside);
    }

    #[cfg(unix)]
    #[test]
    fn committed_upload_permissions_are_owner_only() {
        use std::os::unix::fs::PermissionsExt;

        let root = temp_root("permissions");
        let _ = std::fs::remove_dir_all(&root);
        let policy = UploadPolicy {
            cache_root: root.clone(),
            quota_bytes: 10,
        };
        let stored = store_upload_with_policy(&policy, b"peer-mode", "private.bin", b"secret")
            .expect("upload");
        let mode = std::fs::metadata(&stored.path)
            .expect("metadata")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600);
        assert_eq!(
            std::fs::metadata(&root)
                .expect("upload root metadata")
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        assert_eq!(
            std::fs::metadata(stored.path.parent().expect("identity upload directory"))
                .expect("identity upload metadata")
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        let _ = std::fs::remove_dir_all(root);
    }
}
