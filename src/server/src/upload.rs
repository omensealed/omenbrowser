use std::path::{Path, PathBuf};
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
    let plan = match plan_upload_with_policy(policy, identity_hash, bytes.len() as u64)? {
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

    std::fs::create_dir_all(&plan.identity_dir)?;
    for path in &plan.evict {
        let _ = std::fs::remove_file(path);
    }

    let path = next_upload_path(&plan.identity_dir, filename_hint);
    std::fs::write(&path, bytes)?;
    Ok(StoredUpload {
        path,
        bytes: bytes.len() as u64,
        evicted: plan.evict,
    })
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
        let metadata = entry.metadata()?;
        if !metadata.is_file() {
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

    fn temp_root(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!("omenchatd-upload-{label}-{}", std::process::id()))
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
}
