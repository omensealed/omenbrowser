use std::collections::BTreeMap;
use std::fs::{File, OpenOptions};
use std::io::{ErrorKind, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};
use crate::storage::files::atomic_replace;

pub const BROWSER_FORM_STATE_MAX_BYTES: u64 = 4 * 1024 * 1024;
pub const BROWSER_FORM_STATE_MAX_PAGES: usize = 512;
pub const BROWSER_FORM_STATE_MAX_FIELDS_PER_PAGE: usize = 128;
pub const BROWSER_FORM_STATE_MAX_URL_BYTES: usize = 2 * 1024;
pub const BROWSER_FORM_STATE_MAX_FIELD_NAME_BYTES: usize = 256;
pub const BROWSER_FORM_STATE_MAX_FIELD_VALUE_BYTES: usize = 64 * 1024;
pub const BROWSER_FORM_STATE_CORRUPT_BACKUP_MAX_FILES: usize = 4;
pub const BROWSER_FORM_STATE_CORRUPT_BACKUP_MAX_TOTAL_BYTES: u64 =
    BROWSER_FORM_STATE_CORRUPT_BACKUP_MAX_FILES as u64 * BROWSER_FORM_STATE_MAX_BYTES;
pub const BROWSER_FORM_STATE_BACKUP_MAX_SCAN_ENTRIES: usize = 4096;
static BROWSER_FORM_STATE_TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(1);

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
        let state = match read_bounded_form_state(&path)? {
            Some(raw) => match deserialize_state(&raw) {
                Ok(state) => normalize_state(state),
                Err(_) => {
                    backup_corrupt_form_state(&path, &raw)?;
                    BrowserFormState::default()
                }
            },
            None => BrowserFormState::default(),
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
        validate_page_state(&page_url, &fields)?;
        let previous = self.state.clone();
        if fields.is_empty() {
            self.state.pages.remove(&page_url);
        } else {
            self.state
                .pages
                .insert(page_url, BrowserFormPageState::new(fields, now_epoch_ms));
        }
        self.state = normalize_state(std::mem::take(&mut self.state));
        if let Err(error) = self.save() {
            self.state = previous;
            return Err(error);
        }
        Ok(())
    }

    pub fn prune_expired(&mut self, now_epoch_ms: u64, max_age_secs: u64) -> AppResult<usize> {
        let previous = self.state.clone();
        let max_age_ms = max_age_secs.saturating_mul(1_000);
        let before = self.state.pages.len();
        self.state
            .pages
            .retain(|_, page| now_epoch_ms.saturating_sub(page.updated_epoch_ms) <= max_age_ms);
        let removed = before.saturating_sub(self.state.pages.len());
        if removed > 0 {
            if let Err(error) = self.save() {
                self.state = previous;
                return Err(error);
            }
        }
        Ok(removed)
    }

    pub fn remove_page(&mut self, page_url: &str) -> AppResult<bool> {
        let previous = self.state.clone();
        let removed = self.state.pages.remove(page_url).is_some();
        if removed {
            if let Err(error) = self.save() {
                self.state = previous;
                return Err(error);
            }
        }
        Ok(removed)
    }

    pub fn remove_pages_matching(
        &mut self,
        mut predicate: impl FnMut(&str) -> bool,
    ) -> AppResult<usize> {
        let previous = self.state.clone();
        let before = self.state.pages.len();
        self.state.pages.retain(|url, _| !predicate(url));
        let removed = before.saturating_sub(self.state.pages.len());
        if removed > 0 {
            if let Err(error) = self.save() {
                self.state = previous;
                return Err(error);
            }
        }
        Ok(removed)
    }

    pub fn clear(&mut self) -> AppResult<usize> {
        let previous = self.state.clone();
        let removed = self.state.pages.len();
        if removed > 0 {
            self.state.pages.clear();
            if let Err(error) = self.save() {
                self.state = previous;
                return Err(error);
            }
        }
        Ok(removed)
    }

    pub fn save(&self) -> AppResult<()> {
        let mut payload = serde_json::to_vec_pretty(&self.state)
            .map_err(|error| AppError::Settings(error.to_string()))?;
        payload.push(b'\n');
        if payload.len() as u64 > BROWSER_FORM_STATE_MAX_BYTES {
            return Err(AppError::Settings(format!(
                "browser form state exceeds {BROWSER_FORM_STATE_MAX_BYTES} byte limit"
            )));
        }
        publish_form_state_bytes(&self.path, &payload, PublishMode::Replace)
    }
}

fn read_bounded_form_state(path: &Path) -> AppResult<Option<Vec<u8>>> {
    let path_metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    if !path_metadata.file_type().is_file() {
        return Err(AppError::Settings(format!(
            "browser form-state path must be a regular file: {}",
            path.display()
        )));
    }
    if path_metadata.len() > BROWSER_FORM_STATE_MAX_BYTES {
        return Err(AppError::Settings(format!(
            "browser form-state file exceeds the {BROWSER_FORM_STATE_MAX_BYTES} byte limit: {}",
            path.display()
        )));
    }
    let file = File::open(path)?;
    let opened_metadata = file.metadata()?;
    if !opened_metadata.is_file() {
        return Err(AppError::Settings(format!(
            "browser form-state path must open as a regular file: {}",
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
                "browser form-state path changed while it was being opened: {}",
                path.display()
            )));
        }
    }
    let mut raw = Vec::with_capacity(path_metadata.len() as usize);
    file.take(BROWSER_FORM_STATE_MAX_BYTES.saturating_add(1))
        .read_to_end(&mut raw)?;
    if raw.len() as u64 > BROWSER_FORM_STATE_MAX_BYTES {
        return Err(AppError::Settings(format!(
            "browser form-state file exceeds the {BROWSER_FORM_STATE_MAX_BYTES} byte limit: {}",
            path.display()
        )));
    }
    Ok(Some(raw))
}

#[derive(Clone, Copy)]
enum PublishMode {
    CreateNew,
    Replace,
}

fn publish_form_state_bytes(path: &Path, raw: &[u8], mode: PublishMode) -> AppResult<()> {
    publish_form_state_bytes_with(path, raw, mode, || Ok(()))
}

fn publish_form_state_bytes_with(
    path: &Path,
    raw: &[u8],
    mode: PublishMode,
    before_commit: impl FnOnce() -> std::io::Result<()>,
) -> AppResult<()> {
    if raw.len() as u64 > BROWSER_FORM_STATE_MAX_BYTES {
        return Err(AppError::Settings(format!(
            "browser form state exceeds {BROWSER_FORM_STATE_MAX_BYTES} byte limit"
        )));
    }
    let parent = path.parent().ok_or_else(|| {
        std::io::Error::new(ErrorKind::InvalidInput, "form-state path has no parent")
    })?;
    crate::private_fs::ensure_private_parent_dir(parent)?;
    if !std::fs::symlink_metadata(parent)?.file_type().is_dir() {
        return Err(AppError::Settings(format!(
            "browser form-state parent must be a directory: {}",
            parent.display()
        )));
    }
    match (mode, std::fs::symlink_metadata(path)) {
        (PublishMode::CreateNew, Ok(_)) => {
            return Err(std::io::Error::new(
                ErrorKind::AlreadyExists,
                "form-state destination already exists",
            )
            .into());
        }
        (PublishMode::Replace, Ok(metadata)) if !metadata.file_type().is_file() => {
            return Err(AppError::Settings(format!(
                "browser form-state target must be a regular file: {}",
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
                "form-state path has no safe filename",
            )
        })?;
    let sequence = BROWSER_FORM_STATE_TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let temporary = parent.join(format!(
        ".{file_name}.{}.{}.form-state.tmp",
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
                std::fs::hard_link(&temporary, path)?;
                sync_directory(parent)?;
                std::fs::remove_file(&temporary)?;
            }
            PublishMode::Replace => atomic_replace(&temporary, path)?,
        }
        sync_directory(parent)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result.map_err(Into::into)
}

fn backup_corrupt_form_state(path: &Path, raw: &[u8]) -> AppResult<()> {
    let parent = path.parent().ok_or_else(|| {
        std::io::Error::new(ErrorKind::InvalidInput, "form-state path has no parent")
    })?;
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| {
            std::io::Error::new(
                ErrorKind::InvalidInput,
                "form-state path has no safe filename",
            )
        })?;
    let sequence = BROWSER_FORM_STATE_TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let backup = parent.join(format!(
        "{file_name}.corrupt.{}.{}.{}.bak",
        current_epoch_nanos(),
        std::process::id(),
        sequence
    ));
    publish_form_state_bytes(&backup, raw, PublishMode::CreateNew)?;
    prune_corrupt_form_state_backups(path)
}

fn prune_corrupt_form_state_backups(path: &Path) -> AppResult<()> {
    let parent = path.parent().ok_or_else(|| {
        std::io::Error::new(ErrorKind::InvalidInput, "form-state path has no parent")
    })?;
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| {
            std::io::Error::new(
                ErrorKind::InvalidInput,
                "form-state path has no safe filename",
            )
        })?;
    let prefix = format!("{file_name}.corrupt.");
    let mut backups = Vec::new();
    let mut total_bytes = 0_u64;
    for (scanned, entry) in std::fs::read_dir(parent)?.enumerate() {
        if scanned == BROWSER_FORM_STATE_BACKUP_MAX_SCAN_ENTRIES {
            return Err(AppError::Settings(format!(
                "browser form-state backup discovery exceeds the {} entry scan limit",
                BROWSER_FORM_STATE_BACKUP_MAX_SCAN_ENTRIES
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
        if retained <= BROWSER_FORM_STATE_CORRUPT_BACKUP_MAX_FILES
            && total_bytes <= BROWSER_FORM_STATE_CORRUPT_BACKUP_MAX_TOTAL_BYTES
        {
            break;
        }
        std::fs::remove_file(backup)?;
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

fn current_epoch_nanos() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
}

fn validate_page_state(page_url: &str, fields: &BTreeMap<String, String>) -> AppResult<()> {
    if page_url.len() > BROWSER_FORM_STATE_MAX_URL_BYTES {
        return Err(AppError::Settings(
            "browser form-state URL is too long".into(),
        ));
    }
    if fields.len() > BROWSER_FORM_STATE_MAX_FIELDS_PER_PAGE {
        return Err(AppError::Settings(format!(
            "browser form state exceeds {BROWSER_FORM_STATE_MAX_FIELDS_PER_PAGE} fields per page"
        )));
    }
    if fields.iter().any(|(name, value)| {
        name.len() > BROWSER_FORM_STATE_MAX_FIELD_NAME_BYTES
            || value.len() > BROWSER_FORM_STATE_MAX_FIELD_VALUE_BYTES
    }) {
        return Err(AppError::Settings(
            "browser form-state field name or value is too long".into(),
        ));
    }
    Ok(())
}

fn normalize_state(mut state: BrowserFormState) -> BrowserFormState {
    state.pages.retain(|url, page| {
        validate_page_state(url, &page.fields).is_ok() && !page.fields.is_empty()
    });
    if state.pages.len() > BROWSER_FORM_STATE_MAX_PAGES {
        let mut newest = state
            .pages
            .iter()
            .map(|(url, page)| (page.updated_epoch_ms, url.clone()))
            .collect::<Vec<_>>();
        newest.sort_by(|left, right| right.0.cmp(&left.0).then_with(|| left.1.cmp(&right.1)));
        newest.truncate(BROWSER_FORM_STATE_MAX_PAGES);
        let keep = newest
            .into_iter()
            .map(|(_, url)| url)
            .collect::<std::collections::BTreeSet<_>>();
        state.pages.retain(|url, _| keep.contains(url));
    }
    state
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

fn deserialize_state(raw: &[u8]) -> Result<BrowserFormState, serde_json::Error> {
    let compat = serde_json::from_slice::<BrowserFormStateCompat>(raw)?;
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

#[cfg(test)]
mod tests {
    use super::{publish_form_state_bytes_with, PublishMode};

    #[test]
    fn failed_replace_preserves_prior_form_state_and_removes_stage() {
        let root = std::env::temp_dir().join(format!(
            "omenbrowser-rs-form-state-replace-fault-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("create fixture");
        let target = root.join("browser_form_state.json");
        std::fs::write(&target, b"previous").expect("seed state");

        let result =
            publish_form_state_bytes_with(&target, b"replacement", PublishMode::Replace, || {
                Err(std::io::Error::other("injected pre-commit failure"))
            });

        assert!(result.is_err());
        assert_eq!(
            std::fs::read(&target).expect("read prior state"),
            b"previous"
        );
        assert_eq!(std::fs::read_dir(&root).expect("list fixture").count(), 1);
        let _ = std::fs::remove_dir_all(root);
    }
}
