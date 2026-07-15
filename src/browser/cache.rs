use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::{AppError, AppResult};
use crate::storage::files::atomic_replace;

pub const DEFAULT_CACHE_SECONDS: u64 = 12 * 60 * 60;
pub const PAGE_CACHE_MAX_ITEMS: usize = 256;
pub const PAGE_CACHE_MAX_BYTES: u64 = 64 * 1024 * 1024;
pub const PAGE_CACHE_MAX_RECORD_BYTES: u64 = 5 * 1024 * 1024;
const CACHE_INDEX_NAME: &str = ".page-cache-index.json";

#[derive(Clone, Debug)]
pub struct PageCache {
    cache_dir: PathBuf,
    index: Arc<Mutex<CacheIndex>>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct CacheRecord {
    pub key: String,
    pub title: String,
    pub markup: String,
    pub metadata: BTreeMap<String, serde_json::Value>,
    pub created_at: u64,
    pub expires_at: u64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct CacheIndex {
    entries: BTreeMap<String, CacheIndexEntry>,
    total_bytes: u64,
    #[serde(default)]
    next_sequence: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct CacheIndexEntry {
    key: String,
    created_at: u64,
    expires_at: u64,
    bytes: u64,
    #[serde(default)]
    sequence: u64,
}

impl PageCache {
    pub fn new(cache_dir: PathBuf) -> AppResult<Self> {
        std::fs::create_dir_all(&cache_dir)?;
        let index = load_or_rebuild_index(&cache_dir)?;
        let cache = Self {
            cache_dir,
            index: Arc::new(Mutex::new(index)),
        };
        cache.clean()?;
        Ok(cache)
    }

    pub fn cache_dir(&self) -> &Path {
        &self.cache_dir
    }

    pub fn entry_count(&self) -> AppResult<usize> {
        Ok(self.lock_index()?.entries.len())
    }

    pub fn total_bytes(&self) -> AppResult<u64> {
        Ok(self.lock_index()?.total_bytes)
    }

    pub fn store(
        &self,
        key: &str,
        markup: &str,
        ttl_seconds: u64,
        title: &str,
        metadata: BTreeMap<String, serde_json::Value>,
    ) -> AppResult<Option<PathBuf>> {
        if ttl_seconds == 0 {
            self.delete(key)?;
            return Ok(None);
        }
        let now = unix_timestamp();
        let record = CacheRecord {
            key: key.into(),
            title: title.into(),
            markup: markup.into(),
            metadata,
            created_at: now,
            expires_at: now.saturating_add(ttl_seconds),
        };
        let raw = serde_json::to_vec(&record)
            .map_err(|error| AppError::Browser(format!("cache record encode failed: {error}")))?;
        if raw.len() as u64 > PAGE_CACHE_MAX_RECORD_BYTES {
            return Err(AppError::Browser(format!(
                "cache record exceeds {PAGE_CACHE_MAX_RECORD_BYTES} byte limit"
            )));
        }
        let hash = cache_hash(key);
        let path = self.path_for_hash(&hash);
        let mut index = self.lock_index()?;
        index.next_sequence = index.next_sequence.saturating_add(1);
        let sequence = index.next_sequence;
        let temporary = self
            .cache_dir
            .join(format!(".{hash}.{}.{}.tmp", std::process::id(), now));
        std::fs::write(&temporary, &raw)?;
        if let Err(error) = atomic_replace(&temporary, &path) {
            let _ = std::fs::remove_file(&temporary);
            return Err(error.into());
        }

        remove_index_entry_file(&self.cache_dir, &mut index, &hash, false);
        index.total_bytes = index.total_bytes.saturating_add(raw.len() as u64);
        index.entries.insert(
            hash.clone(),
            CacheIndexEntry {
                key: key.into(),
                created_at: now,
                expires_at: record.expires_at,
                bytes: raw.len() as u64,
                sequence,
            },
        );
        enforce_budget(&self.cache_dir, &mut index, Some(&hash));
        save_index(&self.cache_dir, &index)?;
        Ok(Some(path))
    }

    pub fn load(&self, key: &str) -> AppResult<Option<CacheRecord>> {
        let hash = cache_hash(key);
        let mut index = self.lock_index()?;
        let Some(entry) = index.entries.get(&hash).cloned() else {
            return Ok(None);
        };
        if entry.key != key || unix_timestamp() > entry.expires_at {
            remove_index_entry_file(&self.cache_dir, &mut index, &hash, true);
            save_index(&self.cache_dir, &index)?;
            return Ok(None);
        }
        let path = self.path_for_hash(&hash);
        let metadata = match std::fs::metadata(&path) {
            Ok(metadata) if metadata.is_file() && metadata.len() <= PAGE_CACHE_MAX_RECORD_BYTES => {
                metadata
            }
            _ => {
                remove_index_entry_file(&self.cache_dir, &mut index, &hash, true);
                save_index(&self.cache_dir, &index)?;
                return Ok(None);
            }
        };
        if metadata.len() != entry.bytes {
            remove_index_entry_file(&self.cache_dir, &mut index, &hash, true);
            save_index(&self.cache_dir, &index)?;
            return Ok(None);
        }
        let raw = std::fs::read(&path)?;
        match serde_json::from_slice::<CacheRecord>(&raw) {
            Ok(record)
                if record.key == key
                    && record.expires_at == entry.expires_at
                    && unix_timestamp() <= record.expires_at =>
            {
                Ok(Some(record))
            }
            _ => {
                remove_index_entry_file(&self.cache_dir, &mut index, &hash, true);
                save_index(&self.cache_dir, &index)?;
                Ok(None)
            }
        }
    }

    pub fn delete(&self, key: &str) -> AppResult<()> {
        let hash = cache_hash(key);
        let mut index = self.lock_index()?;
        remove_index_entry_file(&self.cache_dir, &mut index, &hash, true);
        save_index(&self.cache_dir, &index)
    }

    pub fn clean(&self) -> AppResult<()> {
        let now = unix_timestamp();
        let mut index = self.lock_index()?;
        let stale = index
            .entries
            .iter()
            .filter_map(|(hash, entry)| {
                (now > entry.expires_at || !self.path_for_hash(hash).is_file())
                    .then_some(hash.clone())
            })
            .collect::<Vec<_>>();
        for hash in stale {
            remove_index_entry_file(&self.cache_dir, &mut index, &hash, true);
        }
        enforce_budget(&self.cache_dir, &mut index, None);
        save_index(&self.cache_dir, &index)
    }

    fn path_for_hash(&self, hash: &str) -> PathBuf {
        self.cache_dir.join(format!("{hash}.mu"))
    }

    fn lock_index(&self) -> AppResult<std::sync::MutexGuard<'_, CacheIndex>> {
        self.index
            .lock()
            .map_err(|_| AppError::Browser("page cache index lock poisoned".into()))
    }
}

fn load_or_rebuild_index(cache_dir: &Path) -> AppResult<CacheIndex> {
    let index_path = cache_dir.join(CACHE_INDEX_NAME);
    if let Ok(raw) = std::fs::read(&index_path) {
        if let Ok(index) = serde_json::from_slice::<CacheIndex>(&raw) {
            return Ok(index);
        }
    }
    let mut index = CacheIndex::default();
    for entry in std::fs::read_dir(cache_dir)? {
        let path = entry?.path();
        if path.extension().and_then(|value| value.to_str()) != Some("mu") {
            continue;
        }
        let Ok(metadata) = std::fs::metadata(&path) else {
            continue;
        };
        if !metadata.is_file() || metadata.len() > PAGE_CACHE_MAX_RECORD_BYTES {
            let _ = std::fs::remove_file(path);
            continue;
        }
        let Ok(raw) = std::fs::read(&path) else {
            continue;
        };
        let Ok(record) = serde_json::from_slice::<CacheRecord>(&raw) else {
            let _ = std::fs::remove_file(path);
            continue;
        };
        if unix_timestamp() > record.expires_at {
            let _ = std::fs::remove_file(path);
            continue;
        }
        let hash = cache_hash(&record.key);
        let canonical = cache_dir.join(format!("{hash}.mu"));
        if path != canonical {
            if canonical.exists() {
                let _ = std::fs::remove_file(&path);
                continue;
            }
            if std::fs::rename(&path, &canonical).is_err() {
                continue;
            }
        }
        index.total_bytes = index.total_bytes.saturating_add(raw.len() as u64);
        index.next_sequence = index.next_sequence.saturating_add(1);
        let sequence = index.next_sequence;
        index.entries.insert(
            hash,
            CacheIndexEntry {
                key: record.key,
                created_at: record.created_at,
                expires_at: record.expires_at,
                bytes: raw.len() as u64,
                sequence,
            },
        );
    }
    enforce_budget(cache_dir, &mut index, None);
    save_index(cache_dir, &index)?;
    Ok(index)
}

fn enforce_budget(cache_dir: &Path, index: &mut CacheIndex, protected: Option<&str>) {
    while index.entries.len() > PAGE_CACHE_MAX_ITEMS || index.total_bytes > PAGE_CACHE_MAX_BYTES {
        let oldest = index
            .entries
            .iter()
            .filter(|(hash, _)| protected != Some(hash.as_str()))
            .min_by_key(|(hash, entry)| (entry.sequence, entry.created_at, *hash))
            .map(|(hash, _)| hash.clone());
        let Some(hash) = oldest else { break };
        remove_index_entry_file(cache_dir, index, &hash, true);
    }
}

fn remove_index_entry_file(
    cache_dir: &Path,
    index: &mut CacheIndex,
    hash: &str,
    remove_file: bool,
) {
    if let Some(entry) = index.entries.remove(hash) {
        index.total_bytes = index.total_bytes.saturating_sub(entry.bytes);
    }
    if remove_file {
        let _ = std::fs::remove_file(cache_dir.join(format!("{hash}.mu")));
    }
}

fn save_index(cache_dir: &Path, index: &CacheIndex) -> AppResult<()> {
    let path = cache_dir.join(CACHE_INDEX_NAME);
    let temporary = cache_dir.join(format!("{CACHE_INDEX_NAME}.{}.tmp", std::process::id()));
    let raw = serde_json::to_vec(index)
        .map_err(|error| AppError::Browser(format!("cache index encode failed: {error}")))?;
    std::fs::write(&temporary, raw)?;
    if let Err(error) = atomic_replace(&temporary, &path) {
        let _ = std::fs::remove_file(&temporary);
        return Err(error.into());
    }
    Ok(())
}

pub fn cache_ttl_for_markup(markup: &str) -> u64 {
    for line in markup.lines() {
        let trimmed = line.trim();
        let Some(directive) = trimmed
            .strip_prefix("#!")
            .or_else(|| trimmed.strip_prefix('#'))
        else {
            if trimmed.is_empty() {
                continue;
            }
            return DEFAULT_CACHE_SECONDS;
        };
        let Some((key, value)) = directive.split_once('=') else {
            continue;
        };
        if key.trim() == "c" {
            return value.trim().parse().unwrap_or(DEFAULT_CACHE_SECONDS);
        }
    }
    DEFAULT_CACHE_SECONDS
}

fn cache_hash(key: &str) -> String {
    let digest = Sha256::digest(key.as_bytes());
    format!("{digest:x}")
}

fn unix_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
