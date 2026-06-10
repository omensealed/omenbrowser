# 10 — Testing

## Testing goal

The Rust port must be test-driven around renderer and service behavior. Live Reticulum testing is important, but the app must be useful and testable without a live network.

## Test categories

1. Core model serialization tests
2. Settings/path/identity safety tests
3. Micron parser tests
4. Micron renderer snapshot tests
5. Browser session tests
6. Cache tests
7. Partial refresh tests
8. Message store tests
9. Directory service tests
10. Interface config render/parse tests
11. Mock runtime tests
12. UI state transition tests
13. Bridge/runtime integration tests
14. Packaging smoke tests

## Test organization

Prefer integration tests under `tests/` for user-visible behavior and service
contracts. Inline `#[cfg(test)]` module tests are still acceptable for small
private parser helpers, codec edge cases, and focused invariants that cannot be
expressed cleanly through the public API.

Large UI/runtime regressions should move out of production source files over
time. Keep production modules readable by migrating broad scenario tests into
named integration suites such as:

```text
tests/browser_session.rs
tests/micron_rendering.rs
tests/micronplus_live_widgets.rs
tests/messaging_delivery_status.rs
tests/runtime_native_lxmf.rs
tests/desktop_workspace.rs
```

## Test Suite Hygiene

The suite should protect release behavior, not permanently preserve every
temporary debugging route used while building the port. Before adding a new
regression test, check whether an existing scenario test already covers the
same public behavior. Prefer improving that broader test over adding another
narrow bug-specific test.

Keep a bug-fix regression test when it protects one of these risks:

- identity loss, overwrite, or cross-identity storage bleed;
- message loss, incorrect delivery status, or deleted data restoring;
- browser path/request retry behavior that changes network traffic;
- Micron/MicronPlus rendering, form input, focus, or live partial behavior;
- app freezes, blocking UI paths, or stale async results mutating active state;
- protocol compatibility with Python Reticulum/LXMF/NomadNet behavior;
- moderation/security behavior in OMENchatd.

Consolidate or remove a bug-fix regression test when all of these are true:

- it only asserts an internal helper implementation detail;
- a broader service/UI behavior test would fail if the bug returned;
- the bug was caused by a temporary implementation branch that no longer exists;
- the test uses highly specific local node/chat data instead of generic fixtures;
- it makes production modules harder to read without protecting user-visible
  behavior.

Current cleanup hotspots by inline test count after the latest helper extraction
and TUI polish passes:

```text
src/app.rs                         237
src/desktop/mod.rs                 130
src/server/src/tui.rs               74
src/runtime/native/adapter.rs       39
src/browser/micronplus.rs           37
src/chat/live.rs                    35
src/main.rs                         31
src/runtime/native/rns_net.rs       26
src/server/src/session.rs           23
src/server/src/config.rs            21
src/micron/render.rs                18
src/micron/parser.rs                17
src/server/src/live.rs              16
src/runtime/native_lxmf/codec.rs    16
src/desktop/message_status.rs       15
```

The latest cleanup found no remaining OMENnode-specific MicronPlus fixtures,
`seal_mark`/`omen-mark` captures, live node labels, or private hashes in source
or tests. The raw example destination hash that remains in `src/app.rs` is used
for user-facing command/help examples and the shared fixture constant.

The first integration migration created `tests/native_smoke_reports.rs` for
direct native smoke-report helper coverage that can run through public app
APIs. Continue moving public report/service tests there when possible, while
leaving UI-preview/status/log mutation tests inline until there is a desktop
test harness that can assert the same behavior. Conversation deletion/restore
coverage now lives in `tests/conversation_restore.rs`, including drifted JSON
filenames, duplicate tabs, stale settings metadata, tombstone suppression, empty
thread suppression, and legacy-root cleanup for scoped identities.
`tests/browser_path_retry.rs` now owns public browser path/retry regressions for
deferred opens, path timeout previews, ready retry state, native timeout warning
details, retry guard behavior, retry completion through mock page load,
non-native error handling, and stale live-warning cleanup. Do not
collapse native LXMF evidence/status or OMENchat history/upload tests just
because they originated as bug regressions: those now protect live-tested
release behavior.
Pure desktop LXMF status/retry label helpers now live in
`src/desktop/message_status.rs` with their focused tests, reducing
`src/desktop/mod.rs` without creating a public test-only API.
Pure omenchatd TUI human-readable formatting and width-fitting helpers now live
in `src/server/src/tui_format.rs` with focused tests. Keep server admin
behavior, moderation, room management, and sqlite-backed regressions in
`src/server/src/tui.rs` until they can move behind stable public command/service
APIs.
Pure omenchatd TUI tab/action layout helpers now live in
`src/server/src/tui_layout.rs` with focused tests for wrapped tabs, compact
labels, visible row mapping, and action hitboxes. The remaining TUI tests in
`src/server/src/tui.rs` should continue to protect app-click routing and
admin/server behavior, not layout arithmetic.
Pure omenchatd TUI static help/line-console text now lives in
`src/server/src/tui_text.rs` with focused tests for command coverage and the
admin help launch checklist. Interface summary text and IFAC redaction checks
also live there because they are pure text parsing/formatting. Server
limit/upload-policy text belongs there too because it is deterministic config
formatting with no terminal or database behavior. Room detail/action-guide text
also belongs there when it only formats selected room state; action lists and
click routing stay in `src/server/src/tui.rs`. Moderation explanatory text can
move there via small text snapshots, while known-user loading, role mutation,
delete confirmations, stale-age calculation, and active-link lifecycle behavior
stay in `src/server/src/tui.rs`.
Keep behavior tests in `src/server/src/tui.rs`; do not move sqlite-backed admin
flows into text/layout helpers.
Pure monitoring display helpers such as interface health labels and upload
transfer summaries can also live in `tui_text.rs`. Pure closed-link reason
classification belongs there too when it only interprets a reason string; live
runtime polling, snapshot timing, and link lifecycle behavior stay in
`src/server/src/tui.rs`. Pure active-link rate/activity labels can live in
`tui_text.rs`; active/closed monitoring row formatting can live there too behind
small text snapshots after `tui.rs` has computed room labels, ages, compact
identities, byte labels, and activity labels from live structs. Monitoring
traffic-delta text can also live in `tui_text.rs` when it only formats
already-collected before/after counter snapshots. Server-health label
classification can live there too when it consumes only stats counters and close
reason strings; the closed-link structs and lifecycle collection stay in
`tui.rs`. Monitoring operator-summary text can also live in `tui_text.rs` once
`tui.rs` has already collected live stats, interface lines, recent samples, and
close reasons.
Pure setup checklist advice/action labels can live in `tui_text.rs`; checklist
construction, config/file checks, and Reticulum summary reads stay in
`src/server/src/tui.rs`. Setup launch-status text can also live there behind a
readiness snapshot; config/file readiness calculation stays in `tui.rs`.
Overview operator-summary text can also live in `tui_text.rs` once `tui.rs`
has computed the checklist readiness, room count, interface summary, and upload
labels.
Portal panel wording can also live in `tui_text.rs` once `tui.rs` has computed
the destination text, portal checklist, page state, and MOTD display value.
Identity panel wording can also live in `tui_text.rs` once `tui.rs` has
computed identity/storage path strings, destination text, and the identity
safety checklist.
Setup address display text can also live in `tui_text.rs` once `tui.rs` has
computed public address text and the portal page file path.
Setup next-step body text can also live in `tui_text.rs` once `tui.rs` has
computed launch status, checklist readiness, missing labels, storage root,
Reticulum summary, and upload policy.
Setup checklist line text can also live in `tui_text.rs`; Ratatui color/style
application can stay in `tui.rs`.
Room-list label text can also live in `tui_text.rs`; room loading, selected-row
state, and click handling stay in `tui.rs`.
Line-console user row display can also live in `tui_text.rs` once `tui.rs` has
loaded users and computed role/status labels, timestamp strings, LXMF display,
and stale-delete text.
Line-console setup block assembly can also live in `tui_text.rs` once
`tui.rs` has computed checklist lines, address text, and next-step text.
Line-console room row display can also live in `tui_text.rs`; room loading and
config mutation behavior stay in `tui.rs`.
Line-console command result display can also live in `tui_text.rs` when it only
formats already-mutated values; command parsing, validation, persistence, and
admin audit logging stay in `tui.rs`.

The next maintenance pass should target another pure helper cluster, likely
more line-console command result/status display or another formatting-only
cluster.
Keep the scroll-position, hidden-pane unread, red-X deletion, multi-server
OMENchat, media upload/rendering, and omenchatd moderation regressions unless
they have been moved into integration suites with the same behavioral
assertions.
When adding new regressions, prefer one durable behavior test per user-visible
contract over preserving every temporary bugfix probe. Merge or remove tests
that only assert intermediate implementation details once broader behavior
coverage protects the same failure mode.

## Contained Local Test Runs

Default developer tests should not touch a user's normal browser profile,
Reticulum storage, NomadNet storage, LXMF storage, or default `omenchatd` home.
Use `/tmp` roots for smoke tests and second-client checks unless you are
intentionally validating a real profile.

Recommended baseline:

```bash
cargo test --offline
cargo test --manifest-path src/server/Cargo.toml --offline
cargo fmt --check
```

Recommended contained browser/client roots:

```bash
export OMENBROWSER_TEST_ROOT_1=/tmp/omenbrowser-rs-alpha
export OMENBROWSER_TEST_ROOT_2=/tmp/omenbrowser-rs-alpha-2
export OMENCHATD_TEST_HOME=/tmp/omenchatd-alpha

bash scripts/alpha-root-sanity.sh \
  --browser-root "$OMENBROWSER_TEST_ROOT_1" \
  --browser-root-2 "$OMENBROWSER_TEST_ROOT_2" \
  --server-home "$OMENCHATD_TEST_HOME"
```

Launch isolated browser clients with explicit app roots:

```bash
./target/release/omenbrowser_rs --desktop --app-root "$OMENBROWSER_TEST_ROOT_1"
./target/release/omenbrowser_rs --desktop --app-root "$OMENBROWSER_TEST_ROOT_2"
```

Initialize and run an isolated `omenchatd` server:

```bash
./src/server/target/release/omenchatd init --home "$OMENCHATD_TEST_HOME"
./src/server/target/release/omenchatd interfaces tcp-client <gateway-host:port> \
  --home "$OMENCHATD_TEST_HOME"
./src/server/target/release/omenchatd tui --home "$OMENCHATD_TEST_HOME"
```

For automated OMENchat smoke, prefer the packaged helper because it creates
isolated temporary roots and validates recent-history sync:

```bash
bash scripts/alpha-omenchat-smoke.sh --multi-client
```

Do not run two live desktop clients against the same app root unless the test is
specifically about shared-state corruption. Reusing one root intentionally
shares identity material, Reticulum config, message history, plugin SQLite
state, cached media, and pane layout.

## Fixture safety

Never commit live private keys, identity files, passphrases, personal message
bodies, real access tokens, or non-redacted diagnostic bundles. Captured live
Micron pages are allowed only after sanitizing:

- destination hashes that identify a private node;
- nicknames and operator names;
- message bodies;
- session tokens or form secrets;
- private paths and identity filenames.

Use deterministic placeholder hashes such as
`00112233445566778899aabbccddeeff` and generic labels such as `ExampleNode`,
`ExamplePeer`, and `ExampleChat`. A test should prove behavior, not preserve a
real conversation transcript.

## Renderer tests

Renderer tests are mandatory before UI work grows.

Fixtures should include:

```text
fixtures/micron/plain.mu
fixtures/micron/colors.mu
fixtures/micron/links.mu
fixtures/micron/forms.mu
fixtures/micron/alignment.mu
fixtures/micron/wrapping.mu
fixtures/micron/half_block_40.mu
fixtures/micron/half_block_60.mu
fixtures/micron/half_block_71.mu
fixtures/micron/micronplus_window.mu
fixtures/micron/micronplus_columns.mu
fixtures/micron/captures/*.mu
```

For each fixture, test:

- parsed row count;
- rendered text at width;
- important style spans;
- links and controls metadata;
- no panic on malformed markup.

Captured live pages can be promoted into the fixture corpus by copying the
exported `page.mu` from a Browser `Capture Render` diagnostics export into
`fixtures/micron/captures/` with a descriptive filename. The Rust fixture
harness renders all `.mu` files under `fixtures/micron/` recursively at 40, 60,
71, and 80 columns so live renderer bugs become permanent regressions.

## Browser tests

Test one `BrowserSession` independent of UI.

Required tests:

- parse absolute destination path;
- resolve relative path;
- open mock page;
- history push;
- back/forward;
- reload;
- field value updates;
- request data from fields;
- open link with field forwarding;
- download safe filename;
- cache store/load/expire;
- partial descriptor extraction;
- stale partial ignored after navigation.

## Message tests

Required tests:

- append incoming message;
- append outgoing pending message;
- list threads sorted by recent activity;
- mark read;
- update delivery success;
- update delivery failure;
- reconcile pending timeout;
- update peer label;
- attachment summaries.

## Directory tests

Required tests:

- load missing directory as empty;
- load corrupted directory with backup/default behavior if implemented;
- ingest node announce;
- ingest peer announce;
- ingest propagation announce;
- save entry;
- remove saved entry;
- trust level changes;
- preferred delivery changes;
- identify-on-connect changes;
- filter views;
- placeholder names do not overwrite better names;
- persistent saved entries survive transient clear.

## Interface config tests

Required tests:

- render TCP client profile;
- render TCP server profile;
- render I2P profile;
- render RNode/LoRa profile;
- parse existing managed config if parser implemented;
- toggle enabled;
- toggle connectable;
- apply config writes expected file.

## Runtime tests

Mock adapter tests:

- status;
- attach identity;
- fetch mock page;
- download mock file;
- send direct;
- send propagated;
- emit delivery event;
- request path;
- warm paths;
- interface stats;
- network snapshot.

Bridge tests:

- JSON command serialization;
- JSON event deserialization;
- timeout handling;
- helper process crash handling;
- cancellation handling;
- redaction of secrets in logs.

## UI tests

TUI tests should focus on state transitions, not pixel perfection at first.

Required state tests:

- new browser tab creates independent session;
- closing current tab chooses next valid tab;
- page result applies only to matching generation;
- stale page result ignored;
- opening directory node creates/updates browser tab;
- opening directory peer creates conversation tab;
- sending message updates conversation tab state;
- received message updates thread list and open tab;
- top-level section switching preserves nested tab state.

## Commands

Expected developer commands:

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo run
```

Do not mark a task done if these fail unless the failure is documented and intentionally deferred.

## Manual live test checklist

After mock parity:

1. Launch app in mock mode.
2. Open multiple browser tabs.
3. Open mock pages in different tabs.
4. Verify independent history.
5. Open multiple conversation tabs.
6. Send mock direct and propagated messages.
7. Verify delivery statuses.
8. Switch sections and confirm state persists.
9. Enable live runtime/bridge.
10. Attach identity.
11. Announce identity.
12. Browse known NomadNet node.
13. Send direct LXMF to known peer.
14. Select propagation node and send propagated LXMF.
15. Export diagnostics with redaction.
