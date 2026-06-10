use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::error::AppResult;

pub const LXMF_LOCAL_DELIVERY_CACHE_MAX_AGE_SECS: f64 = 30.0 * 24.0 * 60.0 * 60.0 * 6.0;

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
        if !self.path.exists() {
            return Ok(BTreeMap::new());
        }

        let data = fs::read(&self.path)?;
        match serde_json::from_slice::<DeliveredTransientIds>(&data)
            .map(|cache| cache.ids)
            .or_else(|_| serde_json::from_slice::<BTreeMap<String, f64>>(&data))
        {
            Ok(ids) => Ok(ids),
            Err(_) => {
                self.backup_corrupt_file()?;
                Ok(BTreeMap::new())
            }
        }
    }

    pub fn save(&self, ids: &BTreeMap<String, f64>) -> AppResult<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let cache = DeliveredTransientIds { ids: ids.clone() };
        let data = serde_json::to_vec_pretty(&cache)
            .map_err(|error| crate::error::AppError::Settings(error.to_string()))?;
        fs::write(&self.path, data)?;
        Ok(())
    }

    pub fn mark_delivered(ids: &mut BTreeMap<String, f64>, transient_id: &[u8; 32], now: f64) {
        ids.insert(hex_encode(transient_id), now);
    }

    pub fn has_delivered(ids: &BTreeMap<String, f64>, transient_id: &[u8; 32]) -> bool {
        ids.contains_key(&hex_encode(transient_id))
    }

    pub fn prune_expired(ids: &mut BTreeMap<String, f64>, now: f64, max_age_secs: f64) -> usize {
        let before = ids.len();
        ids.retain(|_, timestamp| now <= *timestamp + max_age_secs);
        before.saturating_sub(ids.len())
    }

    fn backup_corrupt_file(&self) -> AppResult<()> {
        let backup_path = self.path.with_extension(format!(
            "corrupt-{}-{}",
            std::process::id(),
            unix_timestamp_secs() as u64
        ));
        fs::rename(&self.path, backup_path)?;
        Ok(())
    }
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
