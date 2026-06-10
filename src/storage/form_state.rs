use std::collections::BTreeMap;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct BrowserFormState {
    pub pages: BTreeMap<String, BrowserFormPageState>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct BrowserFormPageState {
    pub updated_epoch_ms: u64,
    pub fields: BTreeMap<String, String>,
}

impl Default for BrowserFormPageState {
    fn default() -> Self {
        Self {
            updated_epoch_ms: current_epoch_ms(),
            fields: BTreeMap::new(),
        }
    }
}

impl BrowserFormPageState {
    pub fn new(fields: BTreeMap<String, String>, updated_epoch_ms: u64) -> Self {
        Self {
            updated_epoch_ms,
            fields,
        }
    }
}

#[derive(Clone, Debug)]
pub struct BrowserFormStateStore {
    path: PathBuf,
    state: BrowserFormState,
}

impl BrowserFormStateStore {
    pub fn load_or_default(path: PathBuf) -> AppResult<Self> {
        let state = if path.exists() {
            std::fs::read_to_string(&path)
                .ok()
                .and_then(|raw| deserialize_state(&raw).ok())
                .unwrap_or_default()
        } else {
            BrowserFormState::default()
        };
        Ok(Self { path, state })
    }

    pub fn fields_for(&self, page_url: &str) -> Option<&BTreeMap<String, String>> {
        self.state.pages.get(page_url).map(|page| &page.fields)
    }

    pub fn page_count(&self) -> usize {
        self.state.pages.len()
    }

    pub fn set_fields(
        &mut self,
        page_url: impl Into<String>,
        fields: BTreeMap<String, String>,
    ) -> AppResult<()> {
        self.set_fields_at(page_url, fields, current_epoch_ms())
    }

    pub fn set_fields_at(
        &mut self,
        page_url: impl Into<String>,
        fields: BTreeMap<String, String>,
        now_epoch_ms: u64,
    ) -> AppResult<()> {
        let page_url = page_url.into();
        if fields.is_empty() {
            self.state.pages.remove(&page_url);
        } else {
            self.state
                .pages
                .insert(page_url, BrowserFormPageState::new(fields, now_epoch_ms));
        }
        self.save()
    }

    pub fn prune_expired(&mut self, now_epoch_ms: u64, max_age_secs: u64) -> AppResult<usize> {
        let max_age_ms = max_age_secs.saturating_mul(1_000);
        let before = self.state.pages.len();
        self.state
            .pages
            .retain(|_, page| now_epoch_ms.saturating_sub(page.updated_epoch_ms) <= max_age_ms);
        let removed = before.saturating_sub(self.state.pages.len());
        if removed > 0 {
            self.save()?;
        }
        Ok(removed)
    }

    pub fn remove_page(&mut self, page_url: &str) -> AppResult<bool> {
        let removed = self.state.pages.remove(page_url).is_some();
        if removed {
            self.save()?;
        }
        Ok(removed)
    }

    pub fn remove_pages_matching(
        &mut self,
        mut predicate: impl FnMut(&str) -> bool,
    ) -> AppResult<usize> {
        let before = self.state.pages.len();
        self.state.pages.retain(|url, _| !predicate(url));
        let removed = before.saturating_sub(self.state.pages.len());
        if removed > 0 {
            self.save()?;
        }
        Ok(removed)
    }

    pub fn clear(&mut self) -> AppResult<usize> {
        let removed = self.state.pages.len();
        if removed > 0 {
            self.state.pages.clear();
            self.save()?;
        }
        Ok(removed)
    }

    pub fn save(&self) -> AppResult<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let temp_path = self.path.with_file_name(format!(
            "{}.tmp.{}",
            self.path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("browser_form_state.json"),
            std::process::id()
        ));
        let payload = serde_json::to_string_pretty(&self.state)
            .map_err(|error| AppError::Settings(error.to_string()))?;
        {
            let mut file = OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(&temp_path)?;
            file.write_all(payload.as_bytes())?;
            file.write_all(b"\n")?;
            file.sync_all()?;
        }
        std::fs::rename(temp_path, &self.path)?;
        Ok(())
    }
}

#[derive(Deserialize)]
#[serde(untagged)]
enum BrowserFormPageStateCompat {
    Legacy(BTreeMap<String, String>),
    Current(BrowserFormPageState),
}

#[derive(Default, Deserialize)]
#[serde(default)]
struct BrowserFormStateCompat {
    pages: BTreeMap<String, BrowserFormPageStateCompat>,
}

fn deserialize_state(raw: &str) -> Result<BrowserFormState, serde_json::Error> {
    let compat = serde_json::from_str::<BrowserFormStateCompat>(raw)?;
    Ok(BrowserFormState {
        pages: compat
            .pages
            .into_iter()
            .filter_map(|(url, page)| {
                let page = match page {
                    BrowserFormPageStateCompat::Current(page) => page,
                    BrowserFormPageStateCompat::Legacy(fields) => {
                        BrowserFormPageState::new(fields, current_epoch_ms())
                    }
                };
                (!page.fields.is_empty()).then_some((url, page))
            })
            .collect(),
    })
}

fn current_epoch_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u128::from(u64::MAX)) as u64
}
