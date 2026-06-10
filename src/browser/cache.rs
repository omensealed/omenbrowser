use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::AppResult;

pub const DEFAULT_CACHE_SECONDS: u64 = 12 * 60 * 60;

#[derive(Clone, Debug)]
pub struct PageCache {
    cache_dir: PathBuf,
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

impl PageCache {
    pub fn new(cache_dir: PathBuf) -> AppResult<Self> {
        std::fs::create_dir_all(&cache_dir)?;
        Ok(Self { cache_dir })
    }

    pub fn cache_dir(&self) -> &Path {
        &self.cache_dir
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
        self.delete(key)?;
        let now = unix_timestamp();
        let record = CacheRecord {
            key: key.into(),
            title: title.into(),
            markup: markup.into(),
            metadata,
            created_at: now,
            expires_at: now + ttl_seconds,
        };
        let path = self.path_for(key, record.expires_at);
        std::fs::write(
            &path,
            serde_json::to_vec(&record).expect("cache record serializes"),
        )?;
        Ok(Some(path))
    }

    pub fn load(&self, key: &str) -> AppResult<Option<CacheRecord>> {
        let now = unix_timestamp();
        for path in self.paths_for_key(key)? {
            let Some(expires_at) = expires_from_path(&path) else {
                let _ = std::fs::remove_file(&path);
                continue;
            };
            if now > expires_at {
                let _ = std::fs::remove_file(&path);
                continue;
            }
            let raw = std::fs::read(&path)?;
            match serde_json::from_slice::<CacheRecord>(&raw) {
                Ok(record) => return Ok(Some(record)),
                Err(_) => {
                    let _ = std::fs::remove_file(&path);
                }
            }
        }
        Ok(None)
    }

    pub fn delete(&self, key: &str) -> AppResult<()> {
        for path in self.paths_for_key(key)? {
            let _ = std::fs::remove_file(path);
        }
        Ok(())
    }

    pub fn clean(&self) -> AppResult<()> {
        let now = unix_timestamp();
        for entry in std::fs::read_dir(&self.cache_dir)? {
            let path = entry?.path();
            match expires_from_path(&path) {
                Some(expires_at) if now <= expires_at => {}
                _ => {
                    let _ = std::fs::remove_file(path);
                }
            }
        }
        Ok(())
    }

    fn path_for(&self, key: &str, expires_at: u64) -> PathBuf {
        self.cache_dir
            .join(format!("{}_{}.mu", cache_hash(key), expires_at))
    }

    fn paths_for_key(&self, key: &str) -> AppResult<Vec<PathBuf>> {
        let prefix = cache_hash(key);
        let mut paths = Vec::new();
        for entry in std::fs::read_dir(&self.cache_dir)? {
            let path = entry?.path();
            if path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with(&prefix) && name.ends_with(".mu"))
            {
                paths.push(path);
            }
        }
        Ok(paths)
    }
}

pub fn cache_ttl_for_markup(markup: &str) -> u64 {
    for line in markup.lines() {
        let trimmed = line.trim();
        let Some(directive) = trimmed
            .strip_prefix("#!")
            .or_else(|| trimmed.strip_prefix("#"))
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

fn expires_from_path(path: &Path) -> Option<u64> {
    path.file_stem()
        .and_then(|stem| stem.to_str())
        .and_then(|stem| stem.rsplit_once('_'))
        .and_then(|(_, expires)| expires.parse().ok())
}

fn unix_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
