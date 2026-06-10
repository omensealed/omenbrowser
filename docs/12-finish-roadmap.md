# 12 — Finish Roadmap

This document starts after the repository has completed the mock-first Rust scaffold through Phase 14.

Current state assumed by this roadmap:

- Rust TUI shell exists.
- Browser tabs and conversation tabs exist.
- Shared input buffer exists.
- Mock `NetworkRuntime` exists.
- Browser service exists and is non-blocking from the UI.
- Messaging service exists and can send/sync through the mock runtime.
- Directory, settings, identity, interfaces, diagnostics, and storage boundaries exist.
- Micron parser and renderer foundation exists.
- Mouse routing exists for primary workspace areas.
- Live native Reticulum/LXMF integration is still deferred.

The rest of the port is not a rewrite. It is a staged replacement of mock behavior with live native Rust networking while keeping the UI, storage, browser, messaging, and service boundaries stable.

## Completion phases

### Phase 15 — Dependency/API validation

Goal: confirm the exact current Rust APIs for `reticulum-rs` and `lxmf-rs` before coding live integration.

Tasks:

1. Inspect `Cargo.toml`.
2. Inspect `docs/05-reticulum-lxmf-runtime.md`.
3. Inspect all current `runtime::*` traits/types.
4. Add dependency candidates behind feature flags:
   - `live-reticulum`
   - `live-lxmf`
5. Run `cargo search`, `cargo info`, and/or inspect docs for current crate names.
6. Do not assume old crate APIs.
7. Write findings to `docs/99-implementation-notes.md`.

Expected result:

- A clear implementation decision for which crates and versions will be used.
- Mock mode remains the default build path.
- Live mode can be compiled behind feature flags when APIs are ready.

### Phase 16 — Native Reticulum adapter skeleton

Goal: create `NativeReticulumRuntime` without exposing crate-specific types above `runtime/`.

Tasks:

1. Add `src/runtime/native/`.
2. Implement adapter-owned config structs.
3. Implement identity loading/creation placeholders using native APIs where possible.
4. Implement runtime lifecycle:
   - initialize
   - start
   - stop
   - status snapshot
   - interface snapshot
   - path request
   - announce
5. Add feature-gated compile tests.

Expected result:

- OMENbrowser_rs can be compiled with mock runtime by default.
- When feature-enabled, the live adapter compiles far enough to initialize native Reticulum objects.

### Phase 17 — Native page fetch/browser runtime

Goal: replace mock page fetching with native Reticulum request behavior.

Tasks:

1. Map OMEN/NomadNet addresses into destination/path request objects.
2. Implement request-data forwarding.
3. Implement cancellation and timeout boundaries.
4. Preserve cache behavior from the browser service.
5. Preserve generation-based stale result handling.
6. Add integration tests with a fake native adapter where real network is unavailable.

Expected result:

- Browser service calls do not know whether mock or native runtime is used.
- Address normalization, history, cache, partials, downloads, and UI state still behave the same.

### Phase 18 — Native LXMF adapter

Goal: implement native LXMF send/receive behind `MessagingService`.

Tasks:

1. Map `MessageEnvelope`/`MessageSummary` to/from native LXMF types.
2. Implement direct delivery.
3. Implement propagated delivery.
4. Implement propagation node set/get/sync.
5. Implement tickets where supported.
6. Implement delivery status events.
7. Reconcile pending outbound messages.
8. Preserve JSON message store as the local durable user-visible state.

Expected result:

- User can send and receive LXMF messages through the Rust app without Python.
- Message store remains stable and readable.

### Phase 19 — Runtime event stream

Goal: create one event bus for Reticulum, LXMF, browser partials, messages, diagnostics, logs, and UI notifications.

Tasks:

1. Define `AppEvent` if it does not already exist.
2. Bridge runtime events into UI event channel.
3. Merge crossterm input, timer ticks, runtime events, browser results, and message results.
4. Add debouncing/throttling where needed.
5. Ensure no event path blocks UI rendering.

Expected result:

- Incoming announces/messages/status updates appear without manual refresh.
- Browser partial refresh can run from timers.
- Long network operations do not freeze the UI.

### Phase 20 — Browser partial refresh completion

Goal: finish NomadNet/Micron partial behavior.

Tasks:

1. Parse all Python-compatible partial descriptors.
2. Schedule refresh timers per active tab.
3. Fetch partial fragments through the runtime.
4. Compose partials into the current document.
5. Respect cancellation, tab close, tab switch, generation changes, cache policy, and errors.
6. Add regression fixtures from Python examples.

Expected result:

- Dynamic NomadNet-style pages update correctly in Rust.

### Phase 21 — MicronPlus and controls

Goal: support interactive Micron/MicronPlus constructs without polluting browser service/UI state.

Tasks:

1. Formalize parsed controls:
   - text input
   - checkbox
   - radio
   - button/link
   - forwarded fields
2. Give every control a stable ID.
3. Track control state inside `BrowserSession.field_values` or a dedicated form state.
4. Map keyboard/mouse activation to control actions.
5. Submit field state as request data.
6. Add fallback rendering for unsupported MicronPlus features.

Expected result:

- Rust browser can interact with OMEN/NomadNet pages, not only display them.

### Phase 22 — Directory/announce live integration

Goal: use native Reticulum announces to populate directory and diagnostics.

Tasks:

1. Subscribe to announce events.
2. Normalize announce app-data into `DirectoryEntry`.
3. Preserve saved/trusted entries across live updates.
4. De-duplicate transient entries.
5. Add known-node and propagation-node workflows.
6. Add directory UI actions for save/trust/message/open.

Expected result:

- Directory becomes the live address book and network discovery panel.

### Phase 23 — Interface management live integration

Goal: connect managed interface profiles to real Reticulum startup.

Tasks:

1. Confirm native crate expectations for interface configuration.
2. Render or build equivalent runtime interface objects.
3. Support TCP client/server, I2P, RNode, and auto profiles where available.
4. Provide clear unsupported-state messages when crate support is not ready.
5. Add diagnostics showing actual live interface state.

Expected result:

- User can manage Reticulum connectivity from OMENbrowser_rs.

### Phase 24 — Identity/key material completion

Goal: replace mock identity files with real Reticulum identity material.

Tasks:

1. Implement native identity create/import/export.
2. Preserve existing backup/safety behavior.
3. Show display hashes without exposing secret material.
4. Add migration path from Python OMENbrowser identity files if compatible.
5. Add destructive-action confirmation boundaries.

Expected result:

- OMENbrowser_rs is self-contained and does not rely on Python for identity.

### Phase 25 — UI completion

Goal: make the shell usable as the daily client.

Tasks:

1. Finish scroll behavior for browser, sidebar, messages, directory, logs.
2. Add link/control hit testing.
3. Add visible keybinding/help overlay for every panel.
4. Add status badges:
   - network state
   - active identity
   - unread messages
   - pending sends
   - partial refresh state
   - cache hits/misses
5. Add error/toast panel.
6. Add command palette if useful.

Expected result:

- OMENbrowser_rs feels coherent, responsive, and tied together.

### Phase 26 — Plugin execution

Goal: implement the plugin model safely.

Tasks:

1. Manifest discovery.
2. Permissions/capabilities enforcement.
3. Request-data enrichers.
4. Page post-processors.
5. Message hooks.
6. Sandboxed process execution or constrained WASM path.
7. Plugin diagnostics and disable-on-error.

Expected result:

- Plugins extend browser/messaging behavior without taking over the app.

### Phase 27 — Interop tests

Goal: prove Rust behavior against Python OMENbrowser/NomadNet expectations.

Tasks:

1. Build fixture corpus from archived Python examples.
2. Test Micron rendering at 40, 60, 71, and 80 columns.
3. Test request-data encoding.
4. Test LXMF message shape and delivery modes.
5. Test identity hash display compatibility.
6. Test Reticulum address/path behavior as far as native crates expose it.

Expected result:

- The Rust app is not just functional; it is compatible enough to replace the Python app.

### Phase 28 — Packaging/release

Goal: ship the app cleanly.

Tasks:

1. CLI args.
2. Config paths.
3. Linux packaging.
4. Optional AppImage/deb/rpm/tarball.
5. Logging setup.
6. Panic recovery/terminal restore.
7. Release checklist.

Expected result:

- User can install, run, debug, and recover OMENbrowser_rs.

## Non-negotiable architectural rule

The UI must never call native Reticulum/LXMF crate APIs directly.

Allowed flow:

```text
TUI -> App/service methods -> BrowserService/MessagingService/DirectoryService -> NetworkRuntime trait -> MockRuntime or NativeRuntime
```

Forbidden flow:

```text
TUI -> reticulum-rs/lxmf-rs objects
BrowserSession -> native crate internals
Messaging panel -> native crate internals
Directory panel -> raw native event parsing
```

## Definition of finished

OMENbrowser_rs is considered functionally ported when:

- It starts with a real or managed Reticulum identity.
- It can connect to configured Reticulum interfaces.
- It can fetch and render Micron/NomadNet pages.
- It can handle links, fields, forms, downloads, cache, and partial refreshes.
- It can send and receive LXMF messages direct and propagated where supported.
- It can discover announces and maintain a usable directory.
- It can manage settings, identity, interfaces, diagnostics, and logs.
- It has tests for all non-trivial model/service behavior.
- It preserves mock mode for offline tests.
- It does not rely on the original Python runtime.
