use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::Arc;

use crate::browser::cache::{cache_ttl_for_markup, PageCache};
use crate::browser::partials::{compose_markup_with_partials, extract_partial_specs, PartialSpec};
use crate::browser::{BrowserAddress, BrowserPage, DownloadedFile, PageSource};
#[cfg(feature = "chat-client")]
use crate::chat::descriptor::lower_omenchat_blocks;
use crate::error::{AppError, AppResult};
use crate::micron::parser::Fragment;
use crate::micron::{parse_micron, Document};
use crate::plugins::micronplus::{
    extract_micronplus_layout, has_micronplus_markup, lower_micronplus_markup,
    parse_micronplus_tree,
};
use crate::runtime::{CancellationToken, NetworkRuntime};

#[derive(Clone)]
pub struct BrowserSession {
    pub current_page: Option<BrowserPage>,
    pub current_document: Option<Document>,
    pub history: Vec<String>,
    pub history_index: isize,
    pub field_values: BTreeMap<String, String>,
    pub partials: Vec<PartialSpec>,
    pub generation: u64,
    partial_base_markup: Option<String>,
    partial_contents: BTreeMap<String, String>,
    micronplus_enabled: bool,
    trusted_micronplus_nodes: Option<BTreeSet<String>>,
    cache: Option<Arc<PageCache>>,
    runtime: Option<Arc<dyn NetworkRuntime>>,
    downloads_dir: Option<PathBuf>,
}

impl std::fmt::Debug for BrowserSession {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("BrowserSession")
            .field("current_page", &self.current_page)
            .field("history", &self.history)
            .field("history_index", &self.history_index)
            .field("field_values", &self.field_values)
            .field("partials", &self.partials)
            .field("generation", &self.generation)
            .field("partial_contents", &self.partial_contents.keys())
            .finish_non_exhaustive()
    }
}

impl BrowserSession {
    pub fn new(start_url: impl Into<String>) -> Self {
        let page = BrowserPage::mock_home(start_url.into());
        let mut session = Self {
            current_page: None,
            current_document: None,
            history: Vec::new(),
            history_index: -1,
            field_values: BTreeMap::new(),
            partials: Vec::new(),
            generation: 0,
            partial_base_markup: None,
            partial_contents: BTreeMap::new(),
            micronplus_enabled: true,
            trusted_micronplus_nodes: None,
            cache: None,
            runtime: None,
            downloads_dir: None,
        };
        session.update_page_state(page);
        session
    }

    pub fn with_runtime(
        start_url: impl Into<String>,
        runtime: Arc<dyn NetworkRuntime>,
        cache: Arc<PageCache>,
        downloads_dir: PathBuf,
    ) -> Self {
        let mut session = Self::new(start_url);
        session.runtime = Some(runtime);
        session.cache = Some(cache);
        session.downloads_dir = Some(downloads_dir);
        session
    }

    pub fn current_page(&self) -> Option<&BrowserPage> {
        self.current_page.as_ref()
    }

    pub fn current_url(&self) -> Option<&str> {
        self.current_page.as_ref().map(|page| page.url.as_str())
    }

    pub fn replace_runtime(&mut self, runtime: Arc<dyn NetworkRuntime>) {
        self.runtime = Some(runtime);
    }

    pub fn current_destination(&self) -> Option<String> {
        BrowserAddress::parse(self.current_url()?).map(|address| address.destination)
    }

    pub fn set_micronplus_policy(
        &mut self,
        enabled: bool,
        trusted_nodes: Option<BTreeSet<String>>,
    ) {
        self.micronplus_enabled = enabled;
        self.trusted_micronplus_nodes = trusted_nodes;
    }

    pub fn micronplus_allowed_for_current_page(&self) -> bool {
        self.current_page
            .as_ref()
            .is_some_and(|page| self.micronplus_allowed_for_page(page))
    }

    pub fn history_len(&self) -> usize {
        self.history.len()
    }

    pub fn restore_navigation(
        &mut self,
        current_url: impl Into<String>,
        title: impl Into<String>,
        history: Vec<String>,
        history_index: isize,
    ) {
        let current_url = current_url.into();
        let title = title.into();
        let mut page = BrowserPage::mock_home(current_url.clone());
        page.title = title;
        self.restore_page(page, history, history_index);
    }

    pub fn restore_page(&mut self, page: BrowserPage, history: Vec<String>, history_index: isize) {
        self.update_page_state(page);
        self.history = history;
        self.history_index = if self.history.is_empty() {
            -1
        } else {
            history_index.clamp(0, self.history.len().saturating_sub(1) as isize)
        };
    }

    pub fn resolve_url(&self, input: &str) -> Option<String> {
        if Self::is_clearweb_url(input) {
            return Some(input.into());
        }
        if let Some(address) = BrowserAddress::parse(input) {
            return Some(address.url());
        }
        let current_destination = self.current_destination()?;
        if input.starts_with(':') {
            return BrowserAddress::parse_with_current(input, Some(&current_destination))
                .map(|address| address.url());
        }
        if input.starts_with('/') {
            return Some(format!("{current_destination}:{input}"));
        }
        let current = BrowserAddress::parse(self.current_url()?)?;
        let parent = current
            .path
            .rsplit_once('/')
            .map(|(prefix, _)| if prefix.is_empty() { "/" } else { prefix })
            .unwrap_or("/");
        let separator = if parent.ends_with('/') { "" } else { "/" };
        Some(format!("{current_destination}:{parent}{separator}{input}"))
    }

    pub fn is_clearweb_url(url: &str) -> bool {
        let lowered = url.trim().to_ascii_lowercase();
        lowered.starts_with("http://") || lowered.starts_with("https://")
    }

    pub fn is_download_url(&self, url: &str) -> bool {
        if Self::is_clearweb_url(url) {
            return false;
        }
        self.resolve_url(url)
            .and_then(|resolved| BrowserAddress::parse(&resolved))
            .is_some_and(|address| {
                let path = address.path.trim();
                !path.is_empty()
                    && path != "/"
                    && !path.ends_with('/')
                    && !path.to_ascii_lowercase().ends_with(".mu")
            })
    }

    pub async fn open(
        &mut self,
        url: &str,
        request_data: Option<BTreeMap<String, String>>,
        add_history: bool,
        use_cache: bool,
        cancel: CancellationToken,
    ) -> AppResult<BrowserPage> {
        let resolved = self
            .resolve_url(url)
            .ok_or_else(|| AppError::Browser(format!("invalid address: {url}")))?;
        if Self::is_clearweb_url(&resolved) {
            return Err(AppError::Browser(format!(
                "clearweb URLs require explicit external open: {resolved}"
            )));
        }
        if resolved.ends_with("/file") || resolved.contains("/file/") {
            return Err(AppError::Browser("use download() for file URLs".into()));
        }

        let explicit_request_data = request_data;
        let has_explicit_request_data = explicit_request_data.is_some();
        let default_request_data = self.default_request_data(&resolved);
        let cache_key = self.cache_key(&resolved, Some(&default_request_data));
        if explicit_request_data.is_none() && use_cache {
            if let Some(record) = self
                .cache
                .as_ref()
                .and_then(|cache| cache.load(&cache_key).transpose())
                .transpose()?
            {
                let page = BrowserPage {
                    url: resolved,
                    markup: record.markup,
                    title: record.title,
                    source: PageSource::Cache,
                    metadata: record.metadata,
                    request_data: None,
                };
                let page = self.normalize_page(page);
                self.set_page_and_history(page.clone(), add_history);
                return Ok(page);
            }
        }

        let runtime = self
            .runtime
            .as_ref()
            .ok_or_else(|| AppError::Browser("browser session has no runtime".into()))?
            .clone();
        let mut merged_request_data = default_request_data;
        if let Some(request_data) = explicit_request_data {
            merged_request_data.extend(request_data);
        }
        let request_data = (!merged_request_data.is_empty()).then_some(merged_request_data);
        let page = runtime
            .fetch_page(&resolved, request_data.clone(), cancel)
            .await?;
        if !has_explicit_request_data {
            let ttl = cache_ttl_for_markup(&page.markup);
            if let Some(cache) = &self.cache {
                cache.store(
                    &cache_key,
                    &page.markup,
                    ttl,
                    &page.title,
                    page.metadata.clone(),
                )?;
            }
        }
        let page = self.normalize_page(page);
        self.set_page_and_history(page.clone(), add_history);
        Ok(page)
    }

    pub async fn fetch_fragment(
        &self,
        url: &str,
        request_data: Option<BTreeMap<String, String>>,
        cancel: CancellationToken,
    ) -> AppResult<BrowserPage> {
        let resolved = self
            .resolve_url(url)
            .ok_or_else(|| AppError::Browser(format!("invalid fragment address: {url}")))?;
        let mut merged_request_data = self.default_request_data(&resolved);
        if let Some(request_data) = request_data {
            merged_request_data.extend(request_data);
        }
        let request_data = (!merged_request_data.is_empty()).then_some(merged_request_data);
        let runtime = self
            .runtime
            .as_ref()
            .ok_or_else(|| AppError::Browser("browser session has no runtime".into()))?;
        runtime.fetch_page(&resolved, request_data, cancel).await
    }

    pub async fn back(&mut self, cancel: CancellationToken) -> AppResult<Option<BrowserPage>> {
        if self.history_index <= 0 {
            return Ok(None);
        }
        self.history_index -= 1;
        let url = self.history[self.history_index as usize].clone();
        self.open(&url, None, false, true, cancel).await.map(Some)
    }

    pub async fn forward(&mut self, cancel: CancellationToken) -> AppResult<Option<BrowserPage>> {
        if self.history_index + 1 >= self.history.len() as isize {
            return Ok(None);
        }
        self.history_index += 1;
        let url = self.history[self.history_index as usize].clone();
        self.open(&url, None, false, true, cancel).await.map(Some)
    }

    pub async fn reload(&mut self, cancel: CancellationToken) -> AppResult<Option<BrowserPage>> {
        if self.history_index < 0 {
            return Ok(None);
        }
        let url = self.history[self.history_index as usize].clone();
        if let Some(cache) = &self.cache {
            cache.delete(&self.cache_key(&url, None))?;
        }
        self.open(&url, None, false, false, cancel).await.map(Some)
    }

    pub fn available_links(&self) -> Vec<crate::micron::LinkAction> {
        self.current_document
            .as_ref()
            .map(|document| {
                document
                    .rows
                    .iter()
                    .flat_map(|row| row.fragments.iter())
                    .filter_map(|fragment| match fragment {
                        Fragment::Span(span) => span.link.clone(),
                        Fragment::Control(_) => None,
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn set_field_value(&mut self, name: impl Into<String>, value: impl Into<String>) {
        self.field_values.insert(name.into(), value.into());
        self.sync_document_control_state();
    }

    pub fn apply_field_values(&mut self, values: &BTreeMap<String, String>) {
        for (name, value) in values {
            if self.field_values.contains_key(name) {
                self.field_values.insert(name.clone(), value.clone());
            }
        }
        self.sync_document_control_state();
    }

    pub fn interactive_controls(&self) -> Vec<crate::micron::FieldControl> {
        self.current_document
            .as_ref()
            .map(|document| {
                document
                    .rows
                    .iter()
                    .flat_map(|row| row.fragments.iter())
                    .filter_map(|fragment| match fragment {
                        Fragment::Control(control) => Some(control.clone()),
                        Fragment::Span(_) => None,
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn toggle_control(&mut self, name: &str, value: &str) -> bool {
        let Some(control) = self
            .interactive_controls()
            .into_iter()
            .find(|control| control.name == name && control.value == value)
        else {
            return false;
        };
        match control.kind.as_str() {
            "checkbox" => {
                if self
                    .field_values
                    .get(name)
                    .is_some_and(|current| current == value)
                {
                    self.field_values.remove(name);
                } else {
                    self.field_values.insert(name.into(), value.into());
                }
                self.sync_document_control_state();
                true
            }
            "radio" => {
                self.field_values.insert(name.into(), value.into());
                self.sync_document_control_state();
                true
            }
            _ => false,
        }
    }

    pub fn build_request_data(&self, fields: &[String]) -> BTreeMap<String, String> {
        let include_all = fields.iter().any(|field| field == "*");
        let mut request_data = BTreeMap::new();
        let mut named_fields = Vec::new();
        for field in fields {
            if let Some((key, value)) = field.split_once('=') {
                request_data.insert(format!("var_{key}"), value.to_string());
            } else if field != "*" {
                named_fields.push(field.clone());
            }
        }
        if include_all {
            for (name, value) in &self.field_values {
                request_data.insert(format!("field_{name}"), value.clone());
            }
        }
        for field in named_fields {
            request_data.insert(
                format!("field_{field}"),
                self.field_values.get(&field).cloned().unwrap_or_default(),
            );
        }
        request_data
    }

    pub async fn open_link(
        &mut self,
        link: &crate::micron::LinkAction,
        cancel: CancellationToken,
    ) -> AppResult<Option<BrowserPage>> {
        if link.target.starts_with("lxmf@")
            || link.target.starts_with("lxmf.delivery@")
            || link.target.starts_with("p:")
        {
            return Ok(None);
        }
        let request_data = (!link.fields.is_empty()).then(|| self.build_request_data(&link.fields));
        let use_cache = request_data.is_none();
        self.open(&link.target, request_data, true, use_cache, cancel)
            .await
            .map(Some)
    }

    pub async fn download(
        &self,
        url: &str,
        cancel: CancellationToken,
    ) -> AppResult<DownloadedFile> {
        let resolved = self
            .resolve_url(url)
            .ok_or_else(|| AppError::Browser(format!("invalid download address: {url}")))?;
        let runtime = self
            .runtime
            .as_ref()
            .ok_or_else(|| AppError::Browser("browser session has no runtime".into()))?;
        let downloads_dir = self.downloads_dir.as_ref().ok_or_else(|| {
            AppError::Browser("browser session has no downloads directory".into())
        })?;
        runtime
            .download_file(&resolved, downloads_dir, cancel)
            .await
    }

    pub fn apply_page(&mut self, page: BrowserPage, add_history: bool) {
        self.set_page_and_history(page, add_history);
    }

    pub fn apply_partial_content(&mut self, slot: &str, content: &str) -> AppResult<BrowserPage> {
        let Some(current_page) = self.current_page.clone() else {
            return Err(AppError::Browser(
                "no page available for partial composition".into(),
            ));
        };
        let existing_micronplus_layout = current_page.metadata.get("micronplus_layout").cloned();
        let existing_micronplus_tree = current_page.metadata.get("micronplus_tree").cloned();
        self.partial_contents
            .insert(slot.to_string(), content.to_string());
        let base_markup = self
            .partial_base_markup
            .as_deref()
            .unwrap_or(current_page.markup.as_str());
        let markup =
            compose_markup_with_partials(base_markup, &self.partials, &self.partial_contents);
        let mut next_page = BrowserPage {
            markup,
            source: PageSource::Network,
            ..current_page
        };
        next_page.metadata.remove("micronplus_source");
        let page = self.normalize_page(next_page);
        let page = if existing_micronplus_layout.is_some() || existing_micronplus_tree.is_some() {
            let mut page = page;
            if let Some(layout) = existing_micronplus_layout {
                page.metadata.insert("micronplus_layout".into(), layout);
            } else {
                page.metadata.remove("micronplus_layout");
            }
            if let Some(tree) = existing_micronplus_tree {
                page.metadata.insert("micronplus_tree".into(), tree);
            } else {
                page.metadata.remove("micronplus_tree");
            }
            page
        } else {
            let mut page = page;
            page.metadata.remove("micronplus_layout");
            page.metadata.remove("micronplus_tree");
            page
        };
        self.current_document = Some(parse_micron(&page.markup));
        self.current_page = Some(page.clone());
        self.refresh_field_values();
        Ok(page)
    }

    pub fn is_current_generation(&self, generation: u64) -> bool {
        self.generation == generation
    }

    fn set_page_and_history(&mut self, page: BrowserPage, add_history: bool) {
        self.update_page_state(page.clone());
        if add_history
            && (self.history_index < 0
                || self
                    .history
                    .get(self.history_index as usize)
                    .is_none_or(|url| url != &page.url))
        {
            self.history
                .truncate((self.history_index + 1).max(0) as usize);
            self.history.push(page.url);
            self.history_index += 1;
        }
    }

    fn update_page_state(&mut self, page: BrowserPage) {
        let page = self.normalize_page(page);
        self.current_document = Some(parse_micron(&page.markup));
        self.partials = extract_partial_specs(&page.markup);
        self.partial_base_markup = Some(page.markup.clone());
        self.partial_contents.clear();
        self.current_page = Some(page);
        self.generation += 1;
        self.refresh_field_values();
    }

    fn refresh_field_values(&mut self) {
        let existing = self.field_values.clone();
        self.field_values.clear();
        let Some(document) = &self.current_document else {
            return;
        };
        for row in &document.rows {
            for fragment in &row.fragments {
                let Fragment::Control(control) = fragment else {
                    continue;
                };
                if control.kind == "field" {
                    self.field_values.insert(
                        control.name.clone(),
                        existing
                            .get(&control.name)
                            .cloned()
                            .unwrap_or_else(|| control.value.clone()),
                    );
                } else if matches!(control.kind.as_str(), "checkbox" | "radio")
                    && control.prechecked
                {
                    self.field_values
                        .insert(control.name.clone(), control.value.clone());
                }
            }
        }
        self.sync_document_control_state();
    }

    fn sync_document_control_state(&mut self) {
        let Some(document) = &mut self.current_document else {
            return;
        };
        for row in &mut document.rows {
            for fragment in &mut row.fragments {
                let Fragment::Control(control) = fragment else {
                    continue;
                };
                match control.kind.as_str() {
                    "field" => {
                        if let Some(value) = self.field_values.get(&control.name) {
                            control.value = value.clone();
                        }
                    }
                    "checkbox" | "radio" => {
                        control.prechecked = self
                            .field_values
                            .get(&control.name)
                            .is_some_and(|value| value == &control.value);
                    }
                    _ => {}
                }
            }
        }
    }

    fn cache_key(&self, url: &str, request_data: Option<&BTreeMap<String, String>>) -> String {
        if let Some(data) = request_data.filter(|data| !data.is_empty()) {
            format!(
                "{url}::request::{}",
                serde_json::to_string(data).expect("request data serializes")
            )
        } else {
            url.into()
        }
    }

    fn default_request_data(&self, resolved_url: &str) -> BTreeMap<String, String> {
        if self.micronplus_allowed_for_url(resolved_url) {
            BTreeMap::from([
                ("var_micronplus_plugin_enabled".into(), "1".into()),
                ("var_micronplus_version".into(), "1".into()),
                ("var_client".into(), "omenbrowser".into()),
            ])
        } else {
            BTreeMap::new()
        }
    }

    fn normalize_page(&self, mut page: BrowserPage) -> BrowserPage {
        let micronplus_source = if self.micronplus_allowed_for_page(&page) {
            page.metadata
                .get("micronplus_source")
                .and_then(|value| value.as_str())
                .filter(|source| has_micronplus_markup(source))
                .map(str::to_string)
                .or_else(|| has_micronplus_markup(&page.markup).then(|| page.markup.clone()))
        } else {
            None
        };

        if let Some(source_markup) = micronplus_source {
            page.metadata.insert(
                "micronplus_source".into(),
                serde_json::Value::String(source_markup.clone()),
            );
            let layout = extract_micronplus_layout(&source_markup);
            if !layout.windows.is_empty() {
                page.metadata.insert(
                    "micronplus_layout".into(),
                    serde_json::to_value(&layout).expect("MicronPlus layout serializes"),
                );
            }
            let tree = parse_micronplus_tree(&source_markup);
            if !tree.nodes.is_empty() {
                page.metadata.insert(
                    "micronplus_tree".into(),
                    serde_json::to_value(&tree).expect("MicronPlus tree serializes"),
                );
            }
            let lowered = lower_micronplus_markup(&source_markup);
            if lowered.diagnostics.is_empty() {
                page.metadata.remove("micronplus_diagnostics");
            } else {
                page.metadata.insert(
                    "micronplus_diagnostics".into(),
                    serde_json::to_value(&lowered.diagnostics)
                        .expect("MicronPlus diagnostics serialize"),
                );
            }
            page.markup = lowered.markup;
        } else {
            page.metadata.remove("micronplus_source");
            page.metadata.remove("micronplus_layout");
            page.metadata.remove("micronplus_tree");
            page.metadata.remove("micronplus_diagnostics");
        }
        #[cfg(feature = "chat-client")]
        {
            page.markup = lower_omenchat_blocks(&page.markup);
        }
        page
    }

    fn micronplus_allowed_for_page(&self, page: &BrowserPage) -> bool {
        self.micronplus_allowed_for_url(&page.url)
    }

    fn micronplus_allowed_for_url(&self, url: &str) -> bool {
        if !self.micronplus_enabled {
            return false;
        }
        let Some(trusted_nodes) = &self.trusted_micronplus_nodes else {
            return true;
        };
        BrowserAddress::parse(url)
            .map(|address| trusted_nodes.contains(&address.destination))
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::browser::cache::PageCache;
    use crate::runtime::MockNetworkRuntime;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "omenbrowser-rs-browser-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    fn runtime_session(name: &str) -> BrowserSession {
        let root = temp_dir(name);
        BrowserSession::with_runtime(
            "mock.node:/",
            Arc::new(MockNetworkRuntime::default()),
            Arc::new(PageCache::new(root.join("cache")).expect("cache")),
            root.join("downloads"),
        )
    }

    #[test]
    fn resolves_relative_urls_against_current_destination() {
        let session = BrowserSession::new("mock.node:/folder/page");

        assert_eq!(
            session.resolve_url("child").as_deref(),
            Some("mock.node:/folder/child")
        );
        assert_eq!(
            session.resolve_url("/root").as_deref(),
            Some("mock.node:/root")
        );
        assert_eq!(
            session.resolve_url(":/page/index.mu").as_deref(),
            Some("mock.node:/page/index.mu")
        );
    }

    #[tokio::test]
    async fn open_mock_page_updates_history_and_document() {
        let mut session = runtime_session("open");

        let page = session
            .open(
                "mock.node:/page/gallery.mu",
                None,
                true,
                true,
                CancellationToken::new(),
            )
            .await
            .expect("open page");

        assert_eq!(page.title, "Micron Gallery");
        assert_eq!(session.history_len(), 1);
        assert!(session.current_document.is_some());
        assert!(session
            .available_links()
            .iter()
            .any(|link| link.target.contains("index")));
    }

    #[tokio::test]
    async fn back_forward_and_reload_use_history() {
        let mut session = runtime_session("history");
        session
            .open("mock.node:/", None, true, true, CancellationToken::new())
            .await
            .expect("open first");
        session
            .open(
                "mock.node:/page/gallery.mu",
                None,
                true,
                true,
                CancellationToken::new(),
            )
            .await
            .expect("open second");

        let back = session
            .back(CancellationToken::new())
            .await
            .expect("back")
            .expect("page");
        assert_eq!(back.url, "mock.node:/");
        let forward = session
            .forward(CancellationToken::new())
            .await
            .expect("forward")
            .expect("page");
        assert!(forward.url.contains("gallery"));
        assert!(session
            .reload(CancellationToken::new())
            .await
            .expect("reload")
            .is_some());
    }

    #[test]
    fn request_data_uses_field_prefix_and_empty_string_for_unknown_fields() {
        let mut session = BrowserSession::new("mock.node:/");
        session.set_field_value("known", "value");

        let data = session.build_request_data(&["known".into(), "missing".into(), "x=1".into()]);

        assert_eq!(data.get("field_known").map(String::as_str), Some("value"));
        assert_eq!(data.get("field_missing").map(String::as_str), Some(""));
        assert_eq!(data.get("var_x").map(String::as_str), Some("1"));
    }

    #[tokio::test]
    async fn open_link_forwards_fields() {
        let mut session = runtime_session("link");
        session
            .open(
                "mock.node:/page/gallery.mu",
                None,
                true,
                false,
                CancellationToken::new(),
            )
            .await
            .expect("open gallery");
        session.set_field_value("nickname", "mesh");
        let link = session
            .available_links()
            .into_iter()
            .find(|link| link.fields == vec!["nickname"])
            .expect("submit link");

        let page = session
            .open_link(&link, CancellationToken::new())
            .await
            .expect("open link")
            .expect("page");

        assert_eq!(
            page.request_data
                .as_ref()
                .and_then(|data| data.get("field_nickname"))
                .map(String::as_str),
            Some("mesh")
        );
    }

    #[tokio::test]
    async fn open_adds_micronplus_detection_flag_for_trusted_nodes() {
        let mut session = runtime_session("micronplus-detect");
        session.set_micronplus_policy(true, Some(BTreeSet::from(["mock.node".to_string()])));

        let page = session
            .open(
                "mock.node:/page/micronplus.mu",
                None,
                true,
                false,
                CancellationToken::new(),
            )
            .await
            .expect("open micronplus page");

        assert_eq!(
            page.request_data
                .as_ref()
                .and_then(|data| data.get("var_micronplus_plugin_enabled"))
                .map(String::as_str),
            Some("1")
        );
        assert_eq!(
            page.request_data
                .as_ref()
                .and_then(|data| data.get("var_micronplus_version"))
                .map(String::as_str),
            Some("1")
        );
        assert_eq!(
            page.request_data
                .as_ref()
                .and_then(|data| data.get("var_client"))
                .map(String::as_str),
            Some("omenbrowser")
        );
        assert!(page.markup.contains("`{:/page/micronplus-feed.mu"));
        let layout = page
            .metadata
            .get("micronplus_layout")
            .cloned()
            .and_then(|value| {
                serde_json::from_value::<crate::plugins::micronplus::MicronPlusLayout>(value).ok()
            })
            .expect("trusted MicronPlus page carries layout metadata");
        assert_eq!(layout.windows.len(), 1);
        assert_eq!(layout.windows[0].title.as_deref(), Some("MicronPlus Demo"));
        assert_eq!(layout.windows[0].column_groups[0].columns.len(), 2);
    }

    #[tokio::test]
    async fn open_omits_micronplus_detection_flag_for_untrusted_nodes() {
        let mut session = runtime_session("micronplus-untrusted");
        session.set_micronplus_policy(true, Some(BTreeSet::new()));

        let page = session
            .open(
                "mock.node:/page/micronplus.mu",
                None,
                true,
                false,
                CancellationToken::new(),
            )
            .await
            .expect("open micronplus fallback");

        assert!(page
            .request_data
            .as_ref()
            .is_none_or(|data| !data.contains_key("var_micronplus_plugin_enabled")));
        assert!(page
            .request_data
            .as_ref()
            .is_none_or(|data| !data.contains_key("var_micronplus_version")));
        assert!(page
            .request_data
            .as_ref()
            .is_none_or(|data| !data.contains_key("var_client")));
        assert!(!page.metadata.contains_key("micronplus_layout"));
        assert!(page.markup.contains("falls back"));
    }

    #[test]
    fn toggles_checkbox_and_radio_field_state() {
        let page = BrowserPage {
            url: "mock.node:/form.mu".into(),
            title: "Form".into(),
            markup: "`<10|name`guest>\n`<?|subscribe|yes`Subscribe>\n`<^|mode|fast`Fast>".into(),
            source: PageSource::Network,
            metadata: BTreeMap::new(),
            request_data: None,
        };
        let mut session = BrowserSession::new("mock.node:/");
        session.apply_page(page, true);

        assert_eq!(
            session.field_values.get("name").map(String::as_str),
            Some("guest")
        );
        assert!(session.toggle_control("subscribe", "yes"));
        assert_eq!(
            session.field_values.get("subscribe").map(String::as_str),
            Some("yes")
        );
        assert!(session.toggle_control("subscribe", "yes"));
        assert!(!session.field_values.contains_key("subscribe"));
        assert!(session.toggle_control("mode", "fast"));
        assert_eq!(
            session.field_values.get("mode").map(String::as_str),
            Some("fast")
        );
        let controls = session.interactive_controls();
        assert!(controls
            .iter()
            .any(|control| control.name == "subscribe" && !control.prechecked));
        assert!(controls
            .iter()
            .any(|control| control.name == "mode" && control.prechecked));
    }

    #[test]
    fn partial_content_composes_without_advancing_generation() {
        let page = BrowserPage {
            url: "mock.node:/partial.mu".into(),
            title: "Partial".into(),
            markup: "before\n`{mock.node:/feed`2`pid=feed}\nafter".into(),
            source: PageSource::Network,
            metadata: BTreeMap::new(),
            request_data: None,
        };
        let mut session = BrowserSession::new("mock.node:/");
        session.apply_page(page, true);
        let generation = session.generation;
        let slot = session.partials[0].slot.clone();

        let page = session
            .apply_partial_content(&slot, "loaded")
            .expect("compose partial");

        assert_eq!(page.markup, "before\nloaded\nafter");
        assert_eq!(session.generation, generation);
    }

    #[test]
    fn micronplus_partial_columns_do_not_replace_parent_layout_metadata() {
        let page = BrowserPage {
            url: "mock.node:/page/sample.mu".into(),
            title: "Sample".into(),
            markup: r#"[window title="Parent"]
[columns]
[column]
before
[live id="sample_badge" src=":/page/status-card.mu" refresh=1 loop=7 fields="started_at=1|seed=1"]
after
[/column]
[column]
side
[/column]
[/columns]
[/window]"#
                .into(),
            source: PageSource::Network,
            metadata: BTreeMap::new(),
            request_data: None,
        };
        let mut session = BrowserSession::new("mock.node:/");
        session.apply_page(page, true);
        let generation = session.generation;
        let slot = session.partials[0].slot.clone();

        let page = session
            .apply_partial_content(
                &slot,
                r#"[columns]
[column weight=1]
[/column]
[column width=14]
`F8ff`!SAMPLE/READY`!`f
[/column]
[column weight=1]
[/column]
[/columns]"#,
            )
            .expect("compose partial columns");
        let layout = page
            .metadata
            .get("micronplus_layout")
            .cloned()
            .and_then(|value| {
                serde_json::from_value::<crate::plugins::micronplus::MicronPlusLayout>(value).ok()
            })
            .expect("parent layout");

        assert!(layout
            .windows
            .iter()
            .any(|window| window.title.as_deref() == Some("Parent")));
        assert!(layout
            .windows
            .iter()
            .flat_map(|window| window.column_groups.iter())
            .flat_map(|group| group.columns.iter())
            .any(|column| column.raw_markup.contains("[live id=\"sample_badge\"")));
        assert!(page.markup.contains("SAMPLE/READY"));
        assert_eq!(session.generation, generation);
    }

    #[test]
    fn micronplus_live_input_and_button_lower_into_session_model() {
        let page = BrowserPage {
            url: "mock.node:/micronplus.mu".into(),
            title: "MicronPlus".into(),
            markup: r#"[window title="MicronPlus Demo"]
[columns]
[column weight=3]
[status text="MicronPlus detected" style="success"]
[live id="feed" src=":/feed.mu" refresh=2 loop=2 fields="message"]
[input name="message" submit=enter action="p:feed:log" fields="message"]
[button label="Refresh" action="p:feed:log" fields="message"]
[/column]
[column weight=2]
Side content
[/column]
[/columns]
[/window]"#
                .into(),
            source: PageSource::Network,
            metadata: BTreeMap::new(),
            request_data: None,
        };
        let mut session = BrowserSession::new("mock.node:/");
        session.apply_page(page, true);

        let page = session.current_page().expect("page");
        assert!(page
            .markup
            .contains("`{:/feed.mu`2`message|pid=feed|loop=2}"));
        assert!(page.markup.contains("`<24|message`>"));
        assert!(page.markup.contains("`[Refresh`p:feed:log`message]"));
        assert!(page.markup.contains(">MicronPlus Demo"));
        assert!(page.markup.contains("Side content"));
        assert!(!page.markup.contains("[columns]"));
        assert!(page.metadata.contains_key("micronplus_layout"));
        assert_eq!(session.partials.len(), 1);
        assert_eq!(session.partials[0].remaining, Some(2));
        assert_eq!(
            session.field_values.get("message").map(String::as_str),
            Some("")
        );
        assert!(session
            .available_links()
            .iter()
            .any(|link| link.fields == vec!["message"]));
    }

    #[tokio::test]
    async fn download_uses_runtime_and_safe_download_path() {
        let session = runtime_session("download");

        let downloaded = session
            .download("mock.node:/file/blob", CancellationToken::new())
            .await
            .expect("download");

        assert!(downloaded.path.exists());
        assert_eq!(downloaded.content_type, "text/plain");
    }

    #[test]
    fn clearweb_url_detection_trims_user_input() {
        assert!(BrowserSession::is_clearweb_url(" https://example.com "));
        assert!(BrowserSession::is_clearweb_url("http://example.com"));
        assert!(!BrowserSession::is_clearweb_url(
            "example.com:/page/index.mu"
        ));
    }

    #[test]
    fn download_url_detection_keeps_roots_browsable() {
        let session = runtime_session("download-detection");

        assert!(!session.is_download_url("mock.node:/"));
        assert!(!session.is_download_url("mock.node:/page/"));
        assert!(!session.is_download_url("mock.node:/page/index.mu"));
        assert!(session.is_download_url("mock.node:/files/readme.txt"));
    }

    #[tokio::test]
    async fn cache_serves_second_open_without_network_source() {
        let mut session = runtime_session("cache");
        session
            .open("mock.node:/", None, true, true, CancellationToken::new())
            .await
            .expect("open first");
        let page = session
            .open("mock.node:/", None, true, true, CancellationToken::new())
            .await
            .expect("open cached");

        assert_eq!(page.source, PageSource::Cache);
    }

    #[test]
    fn generation_check_detects_stale_results() {
        let mut session = BrowserSession::new("mock.node:/");
        let generation = session.generation;
        session.apply_page(BrowserPage::mock_home("mock.node:/next"), true);

        assert!(!session.is_current_generation(generation));
    }

    #[test]
    fn restore_navigation_clamps_history_without_runtime_state() {
        let mut session = BrowserSession::new("mock.node:/");

        session.restore_navigation(
            "mock.node:/two.mu",
            "Two",
            vec!["mock.node:/one.mu".into(), "mock.node:/two.mu".into()],
            99,
        );

        assert_eq!(session.current_url(), Some("mock.node:/two.mu"));
        assert_eq!(
            session.current_page().map(|page| page.title.as_str()),
            Some("Two")
        );
        assert_eq!(session.history_index, 1);
        assert_eq!(session.history_len(), 2);
    }

    #[test]
    fn restore_page_hydrates_document_and_preserves_history_pointer() {
        let mut session = BrowserSession::new("mock.node:/");
        let page = BrowserPage {
            url: "mock.node:/cached.mu".into(),
            title: "Cached".into(),
            markup: "`[cached`mock.node:/next.mu]".into(),
            source: PageSource::Cache,
            metadata: BTreeMap::new(),
            request_data: None,
        };

        session.restore_page(
            page,
            vec!["mock.node:/".into(), "mock.node:/cached.mu".into()],
            1,
        );

        assert_eq!(session.current_url(), Some("mock.node:/cached.mu"));
        assert_eq!(
            session.current_page().map(|page| &page.source),
            Some(&PageSource::Cache)
        );
        assert_eq!(session.available_links().len(), 1);
        assert_eq!(session.history_index, 1);
    }

    #[test]
    fn apply_field_values_only_restores_known_controls() {
        let page = BrowserPage {
            url: "mock.node:/form.mu".into(),
            title: "Form".into(),
            markup: "`<12|nickname`>".into(),
            source: PageSource::Network,
            metadata: BTreeMap::new(),
            request_data: None,
        };
        let mut session = BrowserSession::new("mock.node:/");
        session.apply_page(page, true);

        session.apply_field_values(&BTreeMap::from([
            ("nickname".into(), "mesh".into()),
            ("unknown".into(), "ignored".into()),
        ]));

        assert_eq!(
            session.field_values.get("nickname").map(String::as_str),
            Some("mesh")
        );
        assert!(!session.field_values.contains_key("unknown"));
        assert!(session
            .interactive_controls()
            .iter()
            .any(|control| control.name == "nickname" && control.value == "mesh"));
    }
}
