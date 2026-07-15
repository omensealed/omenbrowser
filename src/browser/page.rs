use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

pub const DEFAULT_PATH: &str = "/page/index.mu";
pub const BROWSER_PAGE_URL_MAX_BYTES: usize = 8 * 1024;
pub const BROWSER_PAGE_TITLE_MAX_BYTES: usize = 16 * 1024;
pub const BROWSER_PAGE_MARKUP_MAX_BYTES: usize = 4 * 1024 * 1024;
pub const BROWSER_PAGE_METADATA_MAX_ITEMS: usize = 64;
pub const BROWSER_PAGE_METADATA_KEY_MAX_BYTES: usize = 256;
pub const BROWSER_PAGE_METADATA_SCALAR_MAX_BYTES: usize = 4 * 1024 * 1024;
pub const BROWSER_PAGE_METADATA_MAX_OWNED_BYTES: usize = 16 * 1024 * 1024;
pub const BROWSER_PAGE_METADATA_MAX_VALUES: usize = 128 * 1024;
pub const BROWSER_PAGE_METADATA_MAX_CONTAINER_ITEMS: usize = 64 * 1024;
pub const BROWSER_PAGE_METADATA_MAX_DEPTH: usize = 32;
pub const BROWSER_PAGE_REQUEST_MAX_ITEMS: usize = 128;
pub const BROWSER_PAGE_REQUEST_KEY_MAX_BYTES: usize = 256;
pub const BROWSER_PAGE_REQUEST_VALUE_MAX_BYTES: usize = 64 * 1024;
pub const BROWSER_PAGE_REQUEST_MAX_OWNED_BYTES: usize = 4 * 1024 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Bookmark {
    pub title: String,
    pub url: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum PageSource {
    Cache,
    Network,
    Mock,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct BrowserPage {
    pub url: String,
    pub markup: String,
    pub title: String,
    pub source: PageSource,
    pub metadata: BTreeMap<String, serde_json::Value>,
    pub request_data: Option<BTreeMap<String, String>>,
}

impl BrowserPage {
    pub fn mock_home(url: impl Into<String>) -> Self {
        let url = url.into();
        Self {
            title: "Mock Node".into(),
            markup: ">OMENbrowser_rs\nMock runtime is active.\n`[Messages`lxmf@0011223344556677]"
                .into(),
            url,
            source: PageSource::Mock,
            metadata: BTreeMap::new(),
            request_data: None,
        }
    }

    /// Validate every allocation retained by a page before parsing, caching, or
    /// installing it into browser state. Operational strings are rejected
    /// atomically rather than truncated.
    pub fn validate_retained(&self) -> Result<(), String> {
        validate_len("page URL", self.url.len(), BROWSER_PAGE_URL_MAX_BYTES)?;
        validate_len("page title", self.title.len(), BROWSER_PAGE_TITLE_MAX_BYTES)?;
        validate_len(
            "page markup",
            self.markup.len(),
            BROWSER_PAGE_MARKUP_MAX_BYTES,
        )?;
        if self.metadata.len() > BROWSER_PAGE_METADATA_MAX_ITEMS {
            return Err(format!(
                "page metadata exceeds {} entries",
                BROWSER_PAGE_METADATA_MAX_ITEMS
            ));
        }

        let mut metadata_owned = 0usize;
        let mut values = 0usize;
        let mut pending = Vec::with_capacity(self.metadata.len());
        for (key, value) in &self.metadata {
            validate_len(
                "page metadata key",
                key.len(),
                BROWSER_PAGE_METADATA_KEY_MAX_BYTES,
            )?;
            add_bounded(
                &mut metadata_owned,
                key.len(),
                BROWSER_PAGE_METADATA_MAX_OWNED_BYTES,
                "page metadata",
            )?;
            pending.push((value, 1usize));
        }
        while let Some((value, depth)) = pending.pop() {
            if depth > BROWSER_PAGE_METADATA_MAX_DEPTH {
                return Err(format!(
                    "page metadata exceeds depth {}",
                    BROWSER_PAGE_METADATA_MAX_DEPTH
                ));
            }
            values = values.saturating_add(1);
            if values > BROWSER_PAGE_METADATA_MAX_VALUES {
                return Err(format!(
                    "page metadata exceeds {} values",
                    BROWSER_PAGE_METADATA_MAX_VALUES
                ));
            }
            match value {
                serde_json::Value::String(text) => {
                    validate_len(
                        "page metadata scalar",
                        text.len(),
                        BROWSER_PAGE_METADATA_SCALAR_MAX_BYTES,
                    )?;
                    add_bounded(
                        &mut metadata_owned,
                        text.len(),
                        BROWSER_PAGE_METADATA_MAX_OWNED_BYTES,
                        "page metadata",
                    )?;
                }
                serde_json::Value::Array(items) => {
                    if items.len() > BROWSER_PAGE_METADATA_MAX_CONTAINER_ITEMS {
                        return Err(format!(
                            "page metadata array exceeds {} items",
                            BROWSER_PAGE_METADATA_MAX_CONTAINER_ITEMS
                        ));
                    }
                    pending.extend(items.iter().map(|item| (item, depth + 1)));
                }
                serde_json::Value::Object(items) => {
                    if items.len() > BROWSER_PAGE_METADATA_MAX_CONTAINER_ITEMS {
                        return Err(format!(
                            "page metadata object exceeds {} items",
                            BROWSER_PAGE_METADATA_MAX_CONTAINER_ITEMS
                        ));
                    }
                    for (key, item) in items {
                        validate_len(
                            "page metadata object key",
                            key.len(),
                            BROWSER_PAGE_METADATA_KEY_MAX_BYTES,
                        )?;
                        add_bounded(
                            &mut metadata_owned,
                            key.len(),
                            BROWSER_PAGE_METADATA_MAX_OWNED_BYTES,
                            "page metadata",
                        )?;
                        pending.push((item, depth + 1));
                    }
                }
                serde_json::Value::Null
                | serde_json::Value::Bool(_)
                | serde_json::Value::Number(_) => {}
            }
        }

        if let Some(request_data) = &self.request_data {
            if request_data.len() > BROWSER_PAGE_REQUEST_MAX_ITEMS {
                return Err(format!(
                    "page request data exceeds {} entries",
                    BROWSER_PAGE_REQUEST_MAX_ITEMS
                ));
            }
            let mut request_owned = 0usize;
            for (key, value) in request_data {
                validate_len(
                    "page request key",
                    key.len(),
                    BROWSER_PAGE_REQUEST_KEY_MAX_BYTES,
                )?;
                validate_len(
                    "page request value",
                    value.len(),
                    BROWSER_PAGE_REQUEST_VALUE_MAX_BYTES,
                )?;
                add_bounded(
                    &mut request_owned,
                    key.len().saturating_add(value.len()),
                    BROWSER_PAGE_REQUEST_MAX_OWNED_BYTES,
                    "page request data",
                )?;
            }
        }
        Ok(())
    }
}

fn validate_len(label: &str, actual: usize, maximum: usize) -> Result<(), String> {
    if actual > maximum {
        Err(format!("{label} exceeds {maximum} bytes"))
    } else {
        Ok(())
    }
}

fn add_bounded(total: &mut usize, added: usize, maximum: usize, label: &str) -> Result<(), String> {
    *total = total
        .checked_add(added)
        .ok_or_else(|| format!("{label} byte accounting overflow"))?;
    if *total > maximum {
        Err(format!("{label} exceeds {maximum} owned bytes"))
    } else {
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct DownloadedFile {
    pub url: String,
    pub path: PathBuf,
    pub content_type: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct BrowserAddress {
    pub destination: String,
    pub path: String,
}

impl BrowserAddress {
    pub fn parse(input: &str) -> Option<Self> {
        Self::parse_with_current(input, None)
    }

    pub fn parse_with_current(input: &str, current_destination: Option<&str>) -> Option<Self> {
        let trimmed = input.trim();
        let (destination, path) = trimmed.split_once(':')?;
        let destination = if destination.is_empty() {
            current_destination?
        } else {
            destination
        };
        if destination.is_empty() {
            return None;
        }
        let path = if path.is_empty() { DEFAULT_PATH } else { path };
        Some(Self {
            destination: destination.into(),
            path: normalize_path(path),
        })
    }

    pub fn url(&self) -> String {
        format!("{}:{}", self.destination, self.path)
    }
}

fn normalize_path(path: &str) -> String {
    if path.starts_with('/') {
        path.into()
    } else {
        format!("/{path}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_page() -> BrowserPage {
        BrowserPage {
            url: "mock.node:/page/index.mu".into(),
            markup: "hello".into(),
            title: "Home".into(),
            source: PageSource::Network,
            metadata: BTreeMap::new(),
            request_data: None,
        }
    }

    #[test]
    fn browser_page_admits_declared_boundaries() {
        let mut page = fixture_page();
        page.markup = "m".repeat(BROWSER_PAGE_MARKUP_MAX_BYTES);
        page.request_data = Some(BTreeMap::from([(
            "key".into(),
            "v".repeat(BROWSER_PAGE_REQUEST_VALUE_MAX_BYTES),
        )]));
        page.metadata.insert(
            "source".into(),
            serde_json::Value::String("s".repeat(BROWSER_PAGE_METADATA_SCALAR_MAX_BYTES)),
        );

        assert_eq!(page.validate_retained(), Ok(()));
    }

    #[test]
    fn browser_page_rejects_oversized_primary_and_request_allocations() {
        let mut page = fixture_page();
        page.markup = "m".repeat(BROWSER_PAGE_MARKUP_MAX_BYTES + 1);
        assert!(page
            .validate_retained()
            .is_err_and(|error| error.contains("page markup")));

        let mut page = fixture_page();
        page.request_data = Some(BTreeMap::from([(
            "key".into(),
            "v".repeat(BROWSER_PAGE_REQUEST_VALUE_MAX_BYTES + 1),
        )]));
        assert!(page
            .validate_retained()
            .is_err_and(|error| error.contains("page request value")));
    }

    #[test]
    fn browser_page_rejects_deep_or_excessive_metadata() {
        let mut nested = serde_json::Value::Null;
        for _ in 0..=BROWSER_PAGE_METADATA_MAX_DEPTH {
            nested = serde_json::Value::Array(vec![nested]);
        }
        let mut page = fixture_page();
        page.metadata.insert("nested".into(), nested);
        assert!(page
            .validate_retained()
            .is_err_and(|error| error.contains("metadata exceeds depth")));

        let mut page = fixture_page();
        for index in 0..=BROWSER_PAGE_METADATA_MAX_ITEMS {
            page.metadata
                .insert(format!("key-{index}"), serde_json::Value::Null);
        }
        assert!(page
            .validate_retained()
            .is_err_and(|error| error.contains("metadata exceeds")));
    }
}
