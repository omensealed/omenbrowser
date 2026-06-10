use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

pub const DEFAULT_PATH: &str = "/page/index.mu";

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
