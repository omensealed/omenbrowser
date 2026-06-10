# 16 — Live Browser Fetch, Cache, Downloads, and Partials

This document finishes the browser side of the Python-to-Rust port.

## Existing foundation

By Phase 14, the Rust browser has:

- `BrowserSession` with current page/document state;
- URL/address history;
- field values;
- partial specs;
- generation counter;
- file-backed page cache;
- async task boundary;
- safe download paths;
- basic link activation;
- stale result protection.

The remaining work is to make it behave like a live NomadNet/OMEN browser through native Reticulum.

## Browser service remains the owner

The browser panel should not fetch pages directly. All browser behavior should go through `BrowserSession` and browser service methods.

Allowed:

```text
UI input -> App browser action -> BrowserSession method -> NetworkRuntime fetch/download
```

Forbidden:

```text
UI input -> native Reticulum request
```

## Address handling completion

Support and test:

- absolute destination/page;
- destination root;
- current-destination relative path;
- page-relative path;
- fragments/partials;
- clearweb URL rejection or explicit external-open prompt;
- malformed destination error;
- empty address behavior;
- download links;
- field-forwarding links.

Keep behavior compatible with Python OMENbrowser/NomadNet where possible.

## Request data

Request data must preserve Python-compatible names:

- user field values use `field_` prefix;
- forwarded variables use `var_` prefix;
- plugin request enrichers may add keys only through approved hooks;
- internal app metadata must not leak unless explicitly required.

Add tests that compare request-data maps for common Micron link/control examples.

## Page response handling

Runtime fetch should return enough metadata for browser service:

```rust
pub struct PageResponse {
    pub address: BrowserAddress,
    pub final_address: Option<BrowserAddress>,
    pub mime_type: Option<String>,
    pub title_hint: Option<String>,
    pub bytes: Vec<u8>,
    pub from_cache_allowed: bool,
    pub cache_ttl: Option<Duration>,
    pub received_at: SystemTime,
}
```

Exact type names may differ. The important point is that browser service, not UI, decides whether content becomes a Micron document, a download, or an error page.

## Cache rules

The cache already parses TTL from `#!c=`. Complete behavior:

- cache key includes destination/path/request-data where appropriate;
- expired cache entries are ignored unless offline fallback is explicitly enabled;
- download cache is separate from page cache if binary payloads are large;
- cache write is atomic;
- corrupt cache entry is ignored and optionally deleted;
- diagnostics show cache count/size/hits/misses.

Do not let cache return stale private field-submitted pages for different request data.

## Download rules

Downloads must:

- never overwrite without explicit user action;
- sanitize filenames;
- preserve extension if safe;
- show result path in UI/log;
- stream or file-write large payloads where possible;
- support cancellation;
- return structured errors.

Future UI should expose a downloads panel, but the browser service should already store enough metadata to add one.

## Partial descriptors

Finish Python-compatible partial parsing.

A partial descriptor should include:

- partial id/key;
- source address/path;
- target row/region if applicable;
- refresh interval;
- cache policy;
- failure behavior;
- last successful generation;
- pending operation id;
- cancellation token.

If exact Python syntax is ambiguous, inspect the archived Python renderer/browser implementation and add fixtures.

## Partial scheduling

Partial refresh is a timer/event-bus problem.

Flow:

```text
rendered page has partial specs
-> BrowserSession stores specs
-> App schedules next refresh per active tab/spec
-> timer emits BrowserEvent::RefreshPartial(tab_id, spec_id, generation)
-> browser task fetches fragment
-> result applies only if tab/spec/generation still match
-> BrowserSession composes fragment into document
-> UI render updates
```

Rules:

- inactive tabs may refresh only if user setting allows it;
- closed tabs cancel refreshes;
- reload invalidates old partial generations;
- failed partial fetch should not destroy last good full page;
- repeated failures should back off;
- partial content must be parsed through Micron subsystem.

## Partial composition

Composition belongs in `browser::partials`, not UI rendering.

The composition result should return:

- updated document;
- list of changed rows/regions if useful;
- updated partial metadata;
- warning/error list.

Unknown or malformed partials should be preserved as placeholders or ignored with warnings, never crash.

## Error pages

Browser errors should render as Micron-like internal pages.

Examples:

- path unavailable;
- timeout;
- cancelled;
- unsupported clearweb URL;
- invalid response encoding;
- download saved;
- permission/plugin error.

Internal error pages should include concise diagnostics but never secret identity material.

## Tests

Add tests for:

- address normalization;
- request-data link activation;
- clearweb rejection;
- cache TTL hit/miss;
- cache key includes request data;
- safe download naming;
- partial descriptor parsing;
- partial composition;
- stale partial result ignored;
- cancelled partial result ignored;
- failed partial leaves previous document intact.

## Done when

- Native runtime can fetch pages through the browser service.
- Cache and downloads behave safely.
- Partials refresh without blocking UI.
- Micron renderer remains the only place that understands Micron syntax.
