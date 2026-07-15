use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::Arc;

use crate::browser::cache::{cache_ttl_for_markup, PageCache};
use crate::browser::partials::{compose_markup_with_partials, extract_partial_specs, PartialSpec};
use crate::browser::{BrowserAddress, BrowserPage, DownloadedFile, PageSource};
#[cfg(feature = "chat-client")]
use crate::chat::descriptor::lower_omenchat_blocks;
use crate::error::{AppError, AppResult};
use crate::micron::parser::{
    Fragment, MICRON_CONTROL_MAX_ITEMS, MICRON_CONTROL_MAX_OWNED_BYTES,
    MICRON_CONTROL_NAME_MAX_BYTES, MICRON_CONTROL_VALUE_MAX_BYTES,
};
use crate::micron::{parse_micron, Document};
use crate::plugins::micronplus::{
    has_micronplus_markup, lower_micronplus_markup, try_extract_micronplus_layout,
    try_parse_micronplus_tree,
};
use crate::runtime::{CancellationToken, NetworkRuntime};

pub const BROWSER_HISTORY_MAX_ITEMS: usize = 512;
pub const BROWSER_HISTORY_MAX_OWNED_BYTES: usize = 1024 * 1024;

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
        session
            .update_page_state(page)
            .expect("static mock home page satisfies browser admission limits");
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

    pub fn history_owned_bytes(&self) -> usize {
        navigation_history_owned_bytes(&self.history)
    }

    pub fn restore_navigation(
        &mut self,
        current_url: impl Into<String>,
        title: impl Into<String>,
        history: Vec<String>,
        history_index: isize,
    ) -> AppResult<()> {
        let current_url = current_url.into();
        let title = title.into();
        let mut page = BrowserPage::mock_home(current_url.clone());
        page.title = title;
        self.restore_page(page, history, history_index)
    }

    pub fn restore_page(
        &mut self,
        page: BrowserPage,
        history: Vec<String>,
        history_index: isize,
    ) -> AppResult<()> {
        let (history, history_index) = admit_restored_navigation_history(history, history_index)?;
        self.update_page_state(page)?;
        self.history = history;
        self.history_index = history_index;
        Ok(())
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
        if resolved.len() > crate::browser::page::BROWSER_PAGE_URL_MAX_BYTES {
            return Err(AppError::Browser(format!(
                "browser address exceeds {} bytes",
                crate::browser::page::BROWSER_PAGE_URL_MAX_BYTES
            )));
        }
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
                let page = self.normalize_page(page)?;
                self.set_page_and_history(page.clone(), add_history)?;
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
        page.validate_retained().map_err(AppError::Browser)?;
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
        let page = self.normalize_page(page)?;
        self.set_page_and_history(page.clone(), add_history)?;
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
        let page = runtime.fetch_page(&resolved, request_data, cancel).await?;
        page.validate_retained().map_err(AppError::Browser)?;
        Ok(page)
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

    pub fn set_field_value(&mut self, name: impl Into<String>, value: impl Into<String>) -> bool {
        let name = name.into();
        let value = value.into();
        if !insert_bounded_field_value(&mut self.field_values, name, value) {
            return false;
        }
        self.sync_document_control_state();
        true
    }

    pub fn can_set_field_value(&self, name: &str, value: &str) -> bool {
        bounded_field_value_is_admitted(&self.field_values, name, value)
    }

    pub fn apply_field_values(&mut self, values: &BTreeMap<String, String>) {
        for (name, value) in values {
            if self.field_values.contains_key(name) {
                insert_bounded_field_value(&mut self.field_values, name.clone(), value.clone());
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
                    if !insert_bounded_field_value(
                        &mut self.field_values,
                        name.into(),
                        value.into(),
                    ) {
                        return false;
                    }
                }
                self.sync_document_control_state();
                true
            }
            "radio" => {
                if !insert_bounded_field_value(&mut self.field_values, name.into(), value.into()) {
                    return false;
                }
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

    pub fn try_apply_page(&mut self, page: BrowserPage, add_history: bool) -> AppResult<()> {
        self.set_page_and_history(page, add_history)
    }

    #[cfg(test)]
    pub fn apply_page(&mut self, page: BrowserPage, add_history: bool) {
        self.try_apply_page(page, add_history)
            .expect("test page satisfies browser admission limits");
    }

    pub fn apply_partial_content(&mut self, slot: &str, content: &str) -> AppResult<BrowserPage> {
        if content.len() > crate::browser::page::BROWSER_PAGE_MARKUP_MAX_BYTES {
            return Err(AppError::Browser(format!(
                "partial content exceeds {} bytes",
                crate::browser::page::BROWSER_PAGE_MARKUP_MAX_BYTES
            )));
        }
        let Some(current_page) = self.current_page.clone() else {
            return Err(AppError::Browser(
                "no page available for partial composition".into(),
            ));
        };
        let existing_micronplus_layout = current_page.metadata.get("micronplus_layout").cloned();
        let existing_micronplus_tree = current_page.metadata.get("micronplus_tree").cloned();
        let mut next_partial_contents = self.partial_contents.clone();
        next_partial_contents.insert(slot.to_string(), content.to_string());
        let base_markup = self
            .partial_base_markup
            .as_deref()
            .unwrap_or(current_page.markup.as_str());
        let markup =
            compose_markup_with_partials(base_markup, &self.partials, &next_partial_contents);
        let mut next_page = BrowserPage {
            markup,
            source: PageSource::Network,
            ..current_page
        };
        next_page.metadata.remove("micronplus_source");
        let page = self.normalize_page(next_page)?;
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
        page.validate_retained().map_err(AppError::Browser)?;
        self.partial_contents = next_partial_contents;
        self.current_document = Some(parse_micron(&page.markup));
        self.current_page = Some(page.clone());
        self.refresh_field_values();
        Ok(page)
    }

    pub fn is_current_generation(&self, generation: u64) -> bool {
        self.generation == generation
    }

    fn set_page_and_history(&mut self, page: BrowserPage, add_history: bool) -> AppResult<()> {
        self.update_page_state(page.clone())?;
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
            trim_live_navigation_history(&mut self.history, &mut self.history_index);
        }
        Ok(())
    }

    fn update_page_state(&mut self, page: BrowserPage) -> AppResult<()> {
        let page = self.normalize_page(page)?;
        self.current_document = Some(parse_micron(&page.markup));
        self.partials = extract_partial_specs(&page.markup);
        self.partial_base_markup = Some(page.markup.clone());
        self.partial_contents.clear();
        self.current_page = Some(page);
        self.generation += 1;
        self.refresh_field_values();
        Ok(())
    }

    fn refresh_field_values(&mut self) {
        let existing = self.field_values.clone();
        let mut field_values = BTreeMap::new();
        let Some(document) = &self.current_document else {
            self.field_values.clear();
            return;
        };
        for row in &document.rows {
            for fragment in &row.fragments {
                let Fragment::Control(control) = fragment else {
                    continue;
                };
                if control.kind == "field" {
                    insert_bounded_field_value(
                        &mut field_values,
                        control.name.clone(),
                        existing
                            .get(&control.name)
                            .cloned()
                            .unwrap_or_else(|| control.value.clone()),
                    );
                } else if matches!(control.kind.as_str(), "checkbox" | "radio")
                    && control.prechecked
                {
                    insert_bounded_field_value(
                        &mut field_values,
                        control.name.clone(),
                        control.value.clone(),
                    );
                }
            }
        }
        self.field_values = field_values;
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

    fn normalize_page(&self, mut page: BrowserPage) -> AppResult<BrowserPage> {
        page.validate_retained().map_err(AppError::Browser)?;
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
            page.metadata.remove("micronplus_layout");
            page.metadata.remove("micronplus_tree");
            let mut structural_diagnostics = Vec::new();
            match try_extract_micronplus_layout(&source_markup) {
                Ok(layout) if !layout.windows.is_empty() => {
                    page.metadata.insert(
                        "micronplus_layout".into(),
                        serde_json::to_value(&layout).expect("MicronPlus layout serializes"),
                    );
                }
                Ok(_) => {}
                Err(error) => structural_diagnostics.push(error),
            }
            match try_parse_micronplus_tree(&source_markup) {
                Ok(tree) if !tree.nodes.is_empty() => {
                    page.metadata.insert(
                        "micronplus_tree".into(),
                        serde_json::to_value(&tree).expect("MicronPlus tree serializes"),
                    );
                }
                Ok(_) => {}
                Err(error) => structural_diagnostics.push(error),
            }
            let lowered = lower_micronplus_markup(&source_markup);
            structural_diagnostics.extend(lowered.diagnostics);
            structural_diagnostics.sort();
            structural_diagnostics.dedup();
            if structural_diagnostics.is_empty() {
                page.metadata.remove("micronplus_diagnostics");
            } else {
                page.metadata.insert(
                    "micronplus_diagnostics".into(),
                    serde_json::to_value(&structural_diagnostics)
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
        page.validate_retained().map_err(AppError::Browser)?;
        Ok(page)
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

fn navigation_history_owned_bytes(history: &[String]) -> usize {
    history
        .iter()
        .fold(0usize, |owned, url| owned.saturating_add(url.len()))
}

fn trim_live_navigation_history(history: &mut Vec<String>, history_index: &mut isize) {
    let mut owned = navigation_history_owned_bytes(history);
    let mut remove = 0usize;
    while history.len().saturating_sub(remove) > BROWSER_HISTORY_MAX_ITEMS
        || owned > BROWSER_HISTORY_MAX_OWNED_BYTES
    {
        owned = owned.saturating_sub(history[remove].len());
        remove += 1;
    }
    if remove > 0 {
        history.drain(..remove);
        *history_index -= remove as isize;
    }
    *history_index = if history.is_empty() {
        -1
    } else {
        (*history_index).clamp(0, history.len().saturating_sub(1) as isize)
    };
}

fn admit_restored_navigation_history(
    mut history: Vec<String>,
    history_index: isize,
) -> AppResult<(Vec<String>, isize)> {
    if history.is_empty() {
        return Ok((Vec::new(), -1));
    }
    let selected = history_index.clamp(0, history.len().saturating_sub(1) as isize) as usize;
    if history[selected].len() > crate::browser::page::BROWSER_PAGE_URL_MAX_BYTES {
        return Err(AppError::Browser(format!(
            "selected browser history URL exceeds {} bytes",
            crate::browser::page::BROWSER_PAGE_URL_MAX_BYTES
        )));
    }

    let mut left = selected;
    let mut right = selected;
    let mut items = 1usize;
    let mut owned = history[selected].len();
    let mut left_open = left > 0;
    let mut right_open = right + 1 < history.len();
    while items < BROWSER_HISTORY_MAX_ITEMS && (left_open || right_open) {
        let mut admitted = false;
        if left_open {
            let candidate = left - 1;
            let bytes = history[candidate].len();
            if bytes <= crate::browser::page::BROWSER_PAGE_URL_MAX_BYTES
                && owned.saturating_add(bytes) <= BROWSER_HISTORY_MAX_OWNED_BYTES
            {
                left = candidate;
                owned += bytes;
                items += 1;
                admitted = true;
                left_open = left > 0;
            } else {
                left_open = false;
            }
        }
        if items >= BROWSER_HISTORY_MAX_ITEMS {
            break;
        }
        if right_open {
            let candidate = right + 1;
            let bytes = history[candidate].len();
            if bytes <= crate::browser::page::BROWSER_PAGE_URL_MAX_BYTES
                && owned.saturating_add(bytes) <= BROWSER_HISTORY_MAX_OWNED_BYTES
            {
                right = candidate;
                owned += bytes;
                items += 1;
                admitted = true;
                right_open = right + 1 < history.len();
            } else {
                right_open = false;
            }
        }
        if !admitted {
            break;
        }
    }

    let retained = history.drain(left..=right).collect::<Vec<_>>();
    Ok((retained, (selected - left) as isize))
}

fn insert_bounded_field_value(
    fields: &mut BTreeMap<String, String>,
    name: String,
    value: String,
) -> bool {
    if !bounded_field_value_is_admitted(fields, &name, &value) {
        return false;
    }
    fields.insert(name, value);
    true
}

fn bounded_field_value_is_admitted(
    fields: &BTreeMap<String, String>,
    name: &str,
    value: &str,
) -> bool {
    if name.is_empty()
        || name.len() > MICRON_CONTROL_NAME_MAX_BYTES
        || value.len() > MICRON_CONTROL_VALUE_MAX_BYTES
        || (!fields.contains_key(name) && fields.len() >= MICRON_CONTROL_MAX_ITEMS)
    {
        return false;
    }
    let retained_bytes = fields
        .iter()
        .fold(0usize, |total, (current_name, current_value)| {
            if current_name == name {
                total
            } else {
                total.saturating_add(current_name.len().saturating_add(current_value.len()))
            }
        });
    if retained_bytes
        .checked_add(name.len())
        .and_then(|total| total.checked_add(value.len()))
        .is_none_or(|total| total > MICRON_CONTROL_MAX_OWNED_BYTES)
    {
        return false;
    }
    true
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
    fn oversized_controls_and_field_updates_do_not_enter_session_state() {
        let oversized_name = "n".repeat(MICRON_CONTROL_NAME_MAX_BYTES + 1);
        let page = BrowserPage {
            url: "mock.node:/bounded-form.mu".into(),
            title: "Bounded Form".into(),
            markup: format!("`<12|nickname`mesh>\n`<{oversized_name}`rejected>"),
            source: PageSource::Network,
            metadata: BTreeMap::new(),
            request_data: None,
        };
        let mut session = BrowserSession::new("mock.node:/");
        session.apply_page(page, true);

        assert_eq!(session.field_values.len(), 1);
        assert_eq!(
            session.field_values.get("nickname").map(String::as_str),
            Some("mesh")
        );
        assert_eq!(session.interactive_controls().len(), 1);
        assert!(
            !session.set_field_value("nickname", "v".repeat(MICRON_CONTROL_VALUE_MAX_BYTES + 1))
        );
        assert_eq!(
            session.field_values.get("nickname").map(String::as_str),
            Some("mesh")
        );
        assert!(!session.set_field_value(oversized_name, "value"));

        session.apply_field_values(&BTreeMap::from([(
            "nickname".into(),
            "v".repeat(MICRON_CONTROL_VALUE_MAX_BYTES + 1),
        )]));
        assert_eq!(
            session.field_values.get("nickname").map(String::as_str),
            Some("mesh")
        );
    }

    #[test]
    fn browser_field_state_is_item_and_aggregate_bounded() {
        let mut session = BrowserSession::new("mock.node:/");
        for index in 0..MICRON_CONTROL_MAX_ITEMS {
            assert!(session.set_field_value(format!("field-{index}"), "value"));
        }
        assert!(!session.set_field_value("one-too-many", "value"));
        assert_eq!(session.field_values.len(), MICRON_CONTROL_MAX_ITEMS);

        let mut session = BrowserSession::new("mock.node:/");
        let value = "v".repeat(MICRON_CONTROL_VALUE_MAX_BYTES);
        let mut admitted = 0usize;
        while session.set_field_value(format!("field-{admitted}"), value.clone()) {
            admitted += 1;
        }
        assert!(admitted > 0 && admitted < MICRON_CONTROL_MAX_ITEMS);
        assert!(
            session
                .field_values
                .iter()
                .map(|(name, value)| name.len() + value.len())
                .sum::<usize>()
                <= MICRON_CONTROL_MAX_OWNED_BYTES
        );
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

    #[test]
    fn micronplus_structural_rejection_is_diagnostic_and_drops_stale_metadata() {
        let depth = crate::browser::micronplus::MICRONPLUS_TREE_MAX_DEPTH;
        let mut markup = "[box]\n".repeat(depth);
        markup.push_str("leaf\n");
        markup.push_str(&"[/box]\n".repeat(depth));
        let page = BrowserPage {
            url: "mock.node:/micronplus-depth.mu".into(),
            title: "Bounded MicronPlus".into(),
            markup,
            source: PageSource::Network,
            metadata: BTreeMap::from([
                ("micronplus_tree".into(), serde_json::json!({"stale": true})),
                (
                    "micronplus_layout".into(),
                    serde_json::json!({"stale": true}),
                ),
            ]),
            request_data: None,
        };
        let mut session = BrowserSession::new("mock.node:/");

        session.apply_page(page, true);

        let page = session
            .current_page()
            .expect("bounded page remains visible");
        assert!(!page.metadata.contains_key("micronplus_tree"));
        assert!(!page.metadata.contains_key("micronplus_layout"));
        assert!(page
            .metadata
            .get("micronplus_diagnostics")
            .and_then(serde_json::Value::as_array)
            .is_some_and(|diagnostics| diagnostics.iter().any(|diagnostic| diagnostic
                .as_str()
                .is_some_and(|diagnostic| diagnostic.contains("exceeds depth")))));
        assert!(page.markup.contains("leaf"));
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
    fn live_navigation_history_retains_newest_item_window() {
        let mut session = BrowserSession::new("mock.node:/");
        for index in 0..=BROWSER_HISTORY_MAX_ITEMS {
            session.apply_page(
                BrowserPage::mock_home(format!("mock.node:/page/{index}.mu")),
                true,
            );
        }

        assert_eq!(session.history_len(), BROWSER_HISTORY_MAX_ITEMS);
        assert_eq!(
            session.history_index,
            BROWSER_HISTORY_MAX_ITEMS as isize - 1
        );
        assert_eq!(
            session.history.first().map(String::as_str),
            Some("mock.node:/page/1.mu")
        );
        assert_eq!(
            session.history.last().map(String::as_str),
            Some("mock.node:/page/512.mu")
        );
        assert!(session.history_owned_bytes() <= BROWSER_HISTORY_MAX_OWNED_BYTES);
    }

    #[test]
    fn live_navigation_history_enforces_aggregate_url_bytes() {
        let mut session = BrowserSession::new("mock.node:/");
        let prefix = "mock.node:/";
        for index in 0..130 {
            let suffix = format!("/{index}");
            let url = format!(
                "{prefix}{}{suffix}",
                "x".repeat(
                    crate::browser::page::BROWSER_PAGE_URL_MAX_BYTES - prefix.len() - suffix.len()
                )
            );
            session.apply_page(BrowserPage::mock_home(url), true);
        }

        assert_eq!(session.history_len(), 128);
        assert_eq!(
            session.history_owned_bytes(),
            BROWSER_HISTORY_MAX_OWNED_BYTES
        );
        assert!(session
            .history
            .first()
            .is_some_and(|url| url.ends_with("/2")));
        assert!(session
            .history
            .last()
            .is_some_and(|url| url.ends_with("/129")));
        assert_eq!(session.history_index, 127);
    }

    #[test]
    fn restored_navigation_retains_contiguous_window_around_pointer() {
        let history = (0..700)
            .map(|index| format!("mock.node:/page/{index}.mu"))
            .collect::<Vec<_>>();
        let selected = 350usize;
        let mut session = BrowserSession::new("mock.node:/");

        session
            .restore_page(
                BrowserPage::mock_home(history[selected].clone()),
                history.clone(),
                selected as isize,
            )
            .expect("bounded history restore");

        assert_eq!(session.history_len(), BROWSER_HISTORY_MAX_ITEMS);
        assert_eq!(session.history, history[94..606]);
        assert_eq!(session.history_index, 256);
        assert_eq!(
            session.history[session.history_index as usize],
            history[selected]
        );
        assert!(session.history_owned_bytes() <= BROWSER_HISTORY_MAX_OWNED_BYTES);
    }

    #[test]
    fn restored_navigation_rejects_oversized_selected_url_atomically() {
        let mut session = BrowserSession::new("mock.node:/");
        session.apply_page(BrowserPage::mock_home("mock.node:/before.mu"), true);
        let before_page = session.current_page().cloned();
        let before_history = session.history.clone();
        let before_generation = session.generation;
        let oversized = "x".repeat(crate::browser::page::BROWSER_PAGE_URL_MAX_BYTES + 1);

        let error = session
            .restore_page(
                BrowserPage::mock_home("mock.node:/after.mu"),
                vec![oversized],
                0,
            )
            .expect_err("oversized selected history URL must fail");

        assert!(error.to_string().contains("selected browser history URL"));
        assert_eq!(session.current_page(), before_page.as_ref());
        assert_eq!(session.history, before_history);
        assert_eq!(session.generation, before_generation);
    }

    #[test]
    fn restored_navigation_does_not_skip_across_invalid_adjacent_edge() {
        let oversized = "x".repeat(crate::browser::page::BROWSER_PAGE_URL_MAX_BYTES + 1);
        let mut session = BrowserSession::new("mock.node:/");

        session
            .restore_page(
                BrowserPage::mock_home("mock.node:/selected.mu"),
                vec![
                    "mock.node:/unreachable-left.mu".into(),
                    oversized,
                    "mock.node:/selected.mu".into(),
                    "mock.node:/right.mu".into(),
                ],
                2,
            )
            .expect("selected and valid adjacent edge remain restorable");

        assert_eq!(
            session.history,
            vec!["mock.node:/selected.mu", "mock.node:/right.mu"]
        );
        assert_eq!(session.history_index, 0);
    }

    #[tokio::test]
    async fn open_rejects_oversized_resolved_url_before_runtime_dispatch() {
        let mut session = runtime_session("oversized-open-url");
        let before_history = session.history.clone();
        let oversized = format!(
            "mock.node:/{}",
            "x".repeat(crate::browser::page::BROWSER_PAGE_URL_MAX_BYTES)
        );

        let error = session
            .open(&oversized, None, true, false, CancellationToken::new())
            .await
            .expect_err("oversized URL must fail before fetch");

        assert!(error.to_string().contains("browser address exceeds"));
        assert_eq!(session.history, before_history);
    }

    #[test]
    fn rejected_page_admission_preserves_current_state_and_history() {
        let mut session = BrowserSession::new("mock.node:/");
        let current_page = session.current_page().cloned();
        let generation = session.generation;
        let history = session.history.clone();
        let mut oversized = BrowserPage::mock_home("mock.node:/oversized.mu");
        oversized.markup = "x".repeat(crate::browser::page::BROWSER_PAGE_MARKUP_MAX_BYTES + 1);

        let error = session
            .try_apply_page(oversized, true)
            .expect_err("oversized page must be rejected");

        assert!(error.to_string().contains("page markup"));
        assert_eq!(session.current_page(), current_page.as_ref());
        assert_eq!(session.generation, generation);
        assert_eq!(session.history, history);
    }

    #[test]
    fn rejected_partial_admission_does_not_retain_candidate_content() {
        let page = BrowserPage {
            url: "mock.node:/partial-admission.mu".into(),
            title: "Partial".into(),
            markup: "before\n`{mock.node:/feed`2`pid=feed}\nafter".into(),
            source: PageSource::Network,
            metadata: BTreeMap::new(),
            request_data: None,
        };
        let mut session = BrowserSession::new("mock.node:/");
        session.apply_page(page, true);
        let slot = session.partials[0].slot.clone();
        let before = session.current_page().cloned();

        assert!(session
            .apply_partial_content(
                &slot,
                &"x".repeat(crate::browser::page::BROWSER_PAGE_MARKUP_MAX_BYTES + 1),
            )
            .is_err());
        assert_eq!(session.current_page(), before.as_ref());

        let page = session
            .apply_partial_content(&slot, "accepted")
            .expect("later bounded partial remains applicable");
        assert!(page.markup.contains("accepted"));
        assert!(!page.markup.contains(&"x".repeat(1024)));
    }

    #[test]
    fn restore_navigation_clamps_history_without_runtime_state() {
        let mut session = BrowserSession::new("mock.node:/");

        session
            .restore_navigation(
                "mock.node:/two.mu",
                "Two",
                vec!["mock.node:/one.mu".into(), "mock.node:/two.mu".into()],
                99,
            )
            .expect("restore navigation");

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

        session
            .restore_page(
                page,
                vec!["mock.node:/".into(), "mock.node:/cached.mu".into()],
                1,
            )
            .expect("restore page");

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
