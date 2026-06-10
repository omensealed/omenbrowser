# 06 — Browser Session

## Browser goal

The browser service owns navigation behavior independent of the UI. In Rust, each browser tab has its own `BrowserSession`.

## Python source

Reference files:

```text
src/omenbrowser/services/browser.py
src/omenbrowser/services/browser_partials.py
src/omenbrowser/services/cache.py
```

## Address model

The Python `BrowserAddress` contains:

- destination;
- path;
- computed URL.

Rust should support:

```text
mock.node:/
<destination_hash>:/
<destination_hash>:/page
<destination_hash>:page
/page-relative
relative-page
```

If a relative URL is opened, it resolves against the current destination and current path context.

## BrowserSession state

Each browser tab owns one session:

```rust
pub struct BrowserSession {
    pub current_page: Option<BrowserPage>,
    pub history: Vec<BrowserPage>,
    pub forward_stack: Vec<BrowserPage>,
    pub field_values: BTreeMap<String, String>,
    pub render_state: BrowserRenderState,
    pub cache: Arc<PageCache>,
    pub runtime: Arc<RuntimeService>,
}
```

Do not share history between tabs.

## Required methods

Port behavior equivalent to:

- `current_destination()`
- `update_render_state(markup)`
- `resolve_url(url)`
- `is_clearweb_url(url)`
- `is_download_url(url)`
- `open(url, request_data, add_history, cancel)`
- `fetch_fragment(url, request_data, cancel)`
- `back()`
- `forward()`
- `reload()`
- `available_links()`
- `set_field_value(name, value)`
- `build_request_data(fields)`
- `open_link(link)`
- `download(url)`

## Open page flow

1. Resolve URL.
2. Build default request data for the URL.
3. Determine cache key.
4. Try cache if allowed.
5. Fetch through runtime adapter if needed.
6. Store cache if TTL permits.
7. Update current page and render state.
8. Push old current page into history if `add_history` is true.
9. Clear forward stack on new navigation.
10. Return page to UI.

## Cache behavior

Cache should be file-backed and TTL-based.

Recommended cache item:

```rust
pub struct CacheRecord {
    pub key: String,
    pub title: String,
    pub markup: String,
    pub metadata: BTreeMap<String, serde_json::Value>,
    pub created_at: OffsetDateTime,
    pub expires_at: OffsetDateTime,
}
```

Cache file names should be hash-derived, not raw URLs.

## Cache TTL

The Python browser detects TTL from markup. Preserve equivalent behavior. If no TTL is present, use a conservative default. A no-cache directive should prevent storage.

Document any exact Micron cache directive syntax discovered during implementation.

## Request fields

Micron links and controls can forward fields. The browser session stores field values by name. When activating a link with field list, build request data from current field state.

Rules:

- unknown field names become empty strings unless Python behavior says otherwise;
- field state is tab-local;
- field state updates when the user edits controls;
- request data should be visible to plugin request enrichers;
- do not leak fields across browser tabs.

## Downloads

Download behavior:

- resolve URL;
- call runtime download;
- write into downloads directory using safe filename;
- avoid overwriting by creating `name-1.ext`, `name-2.ext`, etc.;
- return `DownloadedFile` for status display.

## Clearweb URLs

If a Micron page links to an HTTP/HTTPS URL, the browser should not fetch it through Reticulum. UI may offer to open externally using the system browser. This action must be explicit.

## Partials

Port `browser_partials.py` behavior.

Partials are descriptors embedded in markup that tell the app to refresh a slot from another target. The Rust model should track:

```rust
pub struct PartialSpec {
    pub slot: String,
    pub target: String,
    pub fields: Vec<String>,
    pub interval: Option<Duration>,
    pub remaining: Option<u32>,
}
```

Partial refresh must:

- belong to one browser tab;
- use the tab's field state when needed;
- ignore stale results after navigation;
- not block input;
- update rendered page after composition;
- expose errors subtly, not crash page rendering.

