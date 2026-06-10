# 13 — Native Reticulum-rs Integration

This document guides the replacement of mock Reticulum behavior with a native Rust adapter.

The current project intentionally keeps Reticulum behind a `NetworkRuntime` trait. Keep that boundary. The native adapter is an implementation detail.

## Current external reality

There are multiple Rust Reticulum efforts. Maintainers must inspect the current dependency state before implementation.

Known candidates as of this documentation pass:

- `reticulum-rs` on crates.io / Rust Reticulum workspace line.
- `reticulum` crate lines associated with Rust Reticulum implementations.
- FreeTAKTeam/Reticulum-rs style workspace with `crates/reticulum` and daemon/runtime separation.
- Beechat Reticulum-rs style implementation.

Do not guess APIs from memory. The first live-runtime pass must inspect crate docs and source.

## Cargo feature strategy

Default build must remain mock-only.

Recommended feature flags:

```toml
[features]
default = []
live-reticulum = ["dep:reticulum-rs"]
live-lxmf = ["live-reticulum", "dep:lxmf"]
```

If the chosen crate name is not exactly `reticulum-rs`, use the actual crate package name and keep the public feature name `live-reticulum`.

If the crate API is unstable, isolate it under `src/runtime/native/` so the rest of OMENbrowser_rs does not churn.

## Module layout

Create:

```text
src/runtime/native/
  mod.rs
  config.rs
  error.rs
  identity.rs
  interface.rs
  path.rs
  request.rs
  event.rs
  adapter.rs
```

Suggested responsibilities:

- `mod.rs`: exports feature-gated native runtime types.
- `config.rs`: maps OMENbrowser settings/interface profiles into native config.
- `error.rs`: converts native crate errors into `AppError` or `RuntimeError`.
- `identity.rs`: creates/loads/imports/exports Reticulum identities.
- `interface.rs`: maps managed interface profiles into native transport/interface objects.
- `path.rs`: path discovery, warmup, cache inspection.
- `request.rs`: page fetch/download request logic.
- `event.rs`: converts native events into app-level runtime events.
- `adapter.rs`: owns `NativeNetworkRuntime` implementation.

## Adapter shape

The adapter should look conceptually like this, with exact types adjusted to the real crate API:

```rust
#[cfg(feature = "live-reticulum")]
pub struct NativeNetworkRuntime {
    config: NativeRuntimeConfig,
    // native_reticulum: ...,
    // event_tx: tokio::sync::broadcast::Sender<RuntimeEvent>,
}

#[cfg(feature = "live-reticulum")]
#[async_trait::async_trait]
impl NetworkRuntime for NativeNetworkRuntime {
    async fn status(&self) -> RuntimeResult<NetworkStatus> {
        // Convert native status to OMENbrowser status.
    }

    async fn fetch_page(
        &self,
        address: BrowserAddress,
        request: PageRequest,
        cancel: CancellationToken,
    ) -> RuntimeResult<PageResponse> {
        // Path request -> destination request -> bytes/text -> PageResponse.
    }

    async fn announce(&self, profile: IdentityProfile) -> RuntimeResult<()> {
        // Native announce.
    }
}
```

Do not put crate-specific native types in `BrowserSession`, `MessagingService`, `DirectoryService`, or UI structs.

## Identity integration

The Python app loaded identity material from local storage. The Rust app already has safe path handling and backup behavior. Replace mock identity bytes with real Reticulum identities.

Required behavior:

1. If active managed identity exists, load it.
2. If no identity exists and settings allow creation, create one.
3. If user attaches an external identity path, load from that path without overwriting it.
4. If user imports an identity, copy into managed storage with backup-on-replace.
5. Never log secret key material.
6. Show only display hash/public identifier in diagnostics.

Implementation rule:

`IdentityManager` owns filesystem safety. Native adapter owns conversion from identity file bytes into native identity object.

## Interface integration

The app already persists interface profiles. Native Reticulum integration must consume those profiles.

Profile mapping expectations:

| OMEN profile kind | Native behavior |
|---|---|
| Auto | Use default native config or generated local config |
| TCP client | Connect to configured host/port |
| TCP server | Listen on configured address/port |
| I2P | Use configured I2P/SAM path if supported |
| RNode | Use serial device/frequency/bandwidth/spreading/coding settings if supported |

If native crate does not support a profile kind yet, return a structured unsupported error and show it in diagnostics. Do not silently ignore enabled interfaces.

## Page fetch behavior

The browser service already handles:

- address parsing
- history
- cache
- stale generation checks
- UI task scheduling
- partial composition
- downloads

The native adapter only fetches data.

Required page fetch sequence:

1. Normalize browser address into native destination/path representation.
2. Check cancellation before expensive work.
3. Ensure path is known or request path.
4. Send request with request-data fields.
5. Wait with timeout and cancellation.
6. Convert response into `PageResponse`.
7. Preserve binary data for downloads.
8. Return structured errors for path missing, timeout, denied, invalid response, unsupported request.

## Announce behavior

Implement announce listening as event conversion, not UI polling.

Expected event path:

```text
native reticulum announce -> native adapter event conversion -> RuntimeEvent::Announce -> AppEvent -> DirectoryService::ingest_announce -> UI refresh
```

Do not let the directory panel parse raw native announce data.

## Path discovery behavior

Expose path operations through existing runtime methods:

- request path
- warm path
- inspect destination
- path known/unknown status

Diagnostics should display:

- destination hash
- known path yes/no
- hops if available
- last seen if available
- interface if available

Do not treat hop count as identity/location. It is diagnostics only.

## Error mapping

Create a native error mapping table.

| Native issue | OMENbrowser error |
|---|---|
| identity missing | `RuntimeError::IdentityMissing` |
| invalid identity | `RuntimeError::IdentityInvalid` |
| interface unsupported | `RuntimeError::UnsupportedInterface` |
| path not found | `RuntimeError::PathUnavailable` |
| request timeout | `RuntimeError::Timeout` |
| response parse failure | `RuntimeError::InvalidResponse` |
| cancelled | `RuntimeError::Cancelled` |
| native crate error | `RuntimeError::Native(String)` |

Errors should be visible in logs and diagnostics, but should not crash the terminal UI.

## Tests

Add tests in layers:

### Unit tests

- config mapping
- identity path behavior
- error mapping
- interface profile conversion
- address conversion

### Mock-native tests

Create fake native objects if actual network tests are impossible.

- path known -> fetch succeeds
- path unknown -> returns path error
- timeout -> structured timeout
- cancel before fetch -> cancelled
- cancel while waiting -> cancelled

### Feature compile tests

Run:

```bash
cargo check --features live-reticulum
cargo test --features live-reticulum
```

If the live crate requires unavailable system libraries or network access, document the exact issue in `docs/99-implementation-notes.md`.

## Done when

- Mock runtime still passes all tests.
- `live-reticulum` builds.
- Native identity load/create works.
- Native interface startup path exists.
- Native status, path, announce, and page request paths are implemented or explicitly unsupported by structured errors.
- No UI/service module imports native crate types.
