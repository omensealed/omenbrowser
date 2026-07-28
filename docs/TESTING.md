# Testing Guide

## Safety First

Do not share app roots between test clients unless you intentionally want them
to share the same identity and storage.

Recommended local test roots:

```text
/tmp/omenbrowser-rs-test
/tmp/omenbrowser-rs-test-2
/tmp/omenchatd-test
```

Check root isolation:

```bash
bash scripts/release-root-sanity.sh \
  --browser-root /tmp/omenbrowser-rs-test \
  --browser-root-2 /tmp/omenbrowser-rs-test-2 \
  --server-home /tmp/omenchatd-test
```

## Quick Test Gate

```bash
bash scripts/release-check.sh quick
```

This runs the fast repository checks used before sharing a build.

The focused OMENchat viewport smoke verifies initial bottom anchoring and both
sides of attachment/media layout changes: tail-following panes remain at the
newest event, while manual history scrollback is preserved.

```bash
bash scripts/smoke/10_omenchat_scroll.sh
```

For a locally staged archive, the package gate accepts an optional third
argument for retained OMENchat smoke evidence:

```bash
bash scripts/release-check.sh package \
  target/local-package/OMENbrowser_rs-latest.tar.gz \
  target/local-package-smoke
```

A relative smoke-output path is anchored to the directory from which the gate
was invoked. It therefore remains available after the temporary extracted
package is removed. The smoke itself continues to use distinct temporary
browser and server identity/storage roots.

## CLI product identity

The compatibility binary delegates its stable `--version` output to the
library-owned `product_identity` boundary. Packaging and smoke scripts depend
on the exact `OMENbrowser_rs <version> git_commit=<hex> target=<triple>
profile=<name> features=<name>:<state>,...` shape. Exported source trees may
use `OMENBROWSER_GIT_COMMIT`; the release gate rejects an unknown commit or
target. Profile precedence, feature ordering, and forbidden legacy names have
focused library coverage, and the binary parser retains a compatibility
regression:

```bash
cargo test --locked --no-default-features --features tui product_identity --lib
cargo test --locked --no-default-features --features tui \
  cli_parses_version_command --bin omenbrowser_rs
cargo run --quiet --locked --no-default-features --features tui \
  --bin omenbrowser_rs -- --version
cargo run --quiet --locked --no-default-features --features desktop-product \
  --bin omenbrowser_rs -- --version
```

This is the first bounded F-020 entrypoint extraction. It does not rename the
binary, split frontend executables, or change any CLI spelling or package path.

## Bare native LXMF feature closure

The declared `native-lxmf` feature must compile independently of desktop,
OMENchat-client, development, and mock features. The shared MessagePack
preflight tests cover exact/next limits plus depth, trailing data, reserved
markers, and truncation without allocating a decode tree:

```bash
cargo check --locked --no-default-features --features native-lxmf
cargo clippy --locked --lib --no-default-features --features native-lxmf -- -D warnings
cargo test --locked --lib --no-default-features --features native-lxmf msgpack
cargo tree --locked -e features --no-default-features \
  --features native-lxmf -i omenbrowser_rs
```

The inverse feature tree must not report `chat-client`, `desktop-ui`, or
`mock-runtime`. Native Windows and macOS CI run the bare check and strict
library Clippy before their full product matrix.

The next bounded extraction moves the stable help document into `cli_help`
while leaving the binary as its compatibility writer. Check the library shape,
all accepted help aliases, and the actual binary output with:

```bash
cargo test --locked --no-default-features --features tui cli_help --lib
cargo test --locked --no-default-features --features tui \
  cli_parses_help_command_and_alias --bin omenbrowser_rs
cargo run --quiet --locked --no-default-features --features tui \
  --bin omenbrowser_rs -- --help
```

The extraction reference output is 6,816 bytes over 60 lines, ends with exactly
one newline, and has SHA-256
`d6532f8c468e5653efe0ec0e1f51377139b467eac2d5a29b5b2703e6bf5f65f7` both
before and after delegation. The checksum is evidence for this reviewed text,
not a permanent bar against intentional synchronized help changes.

Typed frontend and simple-command recognition is isolated in `cli_frontend`.
The classifier consumes no option values and deliberately declines complex
native-network, app-root, and secret-input arguments. The compatibility parser
still owns ordering, value consumption, and conflict validation:

```bash
cargo test --locked --no-default-features --features tui cli_frontend --lib
cargo test --locked --no-default-features --features tui \
  'cli_parses_' --bin omenbrowser_rs
```

These tests cover the compiled no-argument default, `--desktop`/`--iced`,
`--tui`/`--terminal`, `--help`/`-h`, and
`--version`/`-V`/`version`. They also prove the classifier does not claim bare
`help`, bare frontend names, complex commands, app-root, or passphrase-file
tokens. Frontend selection continues to carry an isolated `--app-root` through
the compatibility parser without touching a real application root.

Browser passphrase-source preprocessing is isolated in `cli_secret` before the
compatibility command parser. These tests use generated temporary roots only;
they never read normal Reticulum or identity configuration:

```bash
cargo test --locked --no-default-features --features tui cli_secret --lib
cargo test --locked --no-default-features --features tui \
  cli_resolves_owner_only_passphrase_file_before_command_parsing \
  --bin omenbrowser_rs
```

Coverage includes the accepted 4,096-byte boundary and rejected 4,097-byte
boundary, trailing-CR/LF-only trimming, empty/NUL/invalid-UTF-8 rejection,
missing values, source exclusivity, argument passthrough, owner-only regular
files, and Unix permissive-mode/symlink rejection. The binary regression proves
the resolved value reaches only the temporary command override; existing debug,
argv, and diagnostic-bundle redaction tests remain separate gates.

This module intentionally does not replace the bundled gateway or standalone
`omenchatd` implementations in the same patch. Their messages and parser return
types differ, so unification requires an explicit compatibility migration.

The browser's typed TCP client override and endpoint validation live in
`cli_network`. Its fields remain private; the compatibility parser uses
explicit setters/accessors and consumes the value through `into_parts` when it
constructs the temporary Reticulum interface:

```bash
cargo test --locked --no-default-features --features tui cli_network --lib
cargo test --locked --no-default-features --features tui \
  cli_resolves_owner_only_passphrase_file_before_command_parsing \
  --bin omenbrowser_rs
```

The library tests preserve IPv4 and existing unbracketed-IPv6 parsing, exact
missing-host/missing-port error text, invalid-port context, and secret-redacted
`Debug`. The isolated binary regression deliberately supplies passphrase and
network name before `--tcp-client`; replacing the endpoint must retain both
credentials without persisting them or exposing the passphrase in debug/report
output. Server-side TCP override types remain standalone package ownership.

Runtime backend and LXMF smoke delivery values are parsed by the library-owned
`cli_values` boundary. Backend compatibility remains deliberately
case-sensitive and retains `native`, `native-reticulum`, and legacy `bridge`;
delivery parsing trims and case-folds input while retaining `propagation` and
`prop`. The exact normalized error messages are part of the compatibility
contract:

```bash
cargo test --locked --no-default-features --features tui cli_values --lib
cargo test --locked --no-default-features --features tui \
  cli_delegates_typed_backend_and_delivery_values --bin omenbrowser_rs
```

This extraction changes neither runtime selection nor LXMF wire behavior. The
binary still owns option-value consumption, missing-value errors, command
conflicts, defaults, and construction of the final command.

The complete command-local override aggregate lives in `cli_overrides`. All
fields are private. Parsing uses explicit setters, runtime application uses
consuming `take_*` methods, and diagnostics use borrowed accessors. Its custom
`Debug` reveals the selected backend and whether protected values exist, but
never emits identity/config/app-root paths or nested TCP credentials:

```bash
cargo test --locked --no-default-features --features tui cli_overrides --lib
cargo test --locked --no-default-features --features tui \
  cli_resolves_owner_only_passphrase_file_before_command_parsing \
  --bin omenbrowser_rs
```

Defaults, option conflicts, bundle schema, runtime settings, and temporary
Reticulum interface construction remain binary-owned compatibility behavior.

Pure diagnostic sanitization lives in `cli_redaction`. Library tests lock the
exact sanitized argv, path-hint object, complete override-snapshot schema,
persisted-log path/passphrase replacement, case-insensitive message-body
suppression, and Unicode-safe 240-character truncation. The existing isolated
bundle integration proves those values still reach `command.json` and
`logs.json` without exposing private paths or IFAC credentials:

```bash
cargo test --locked --no-default-features --features tui cli_redaction --lib
cargo test --locked --no-default-features --features tui \
  report_bundle_writes_expected_redacted_files --bin omenbrowser_rs
```

`cli_redaction` performs no filesystem or environment reads and borrows
protected paths rather than cloning them. `cli_report_logs` owns the persisted
log filesystem boundary. It scans at most 4,096 directory entries, selects at
most eight regular files without following symlinks, reads at most 512 KiB from
each tail and 2 MiB total, and retains only the newest 50 parsed entries. The
bundle's `logs.json` records these limits plus observed scan/read/truncation
counters without file paths. Run its isolated oversize/symlink regressions with:

```bash
cargo test --locked --no-default-features --features tui cli_report_logs --lib
```

Bundle creation and environment capture remain in the compatibility binary.

## Browser structured-log budgets

Startup history and live log display share explicit memory boundaries. The
shared reader loads regular log-file tails only, while `LogBuffer` applies the
4,096-entry/4 MiB live budget and 16 KiB per-message cap. The production sparse
fixture proves startup never reads more than 16 files, 512 KiB per file, or
4 MiB total; separate tests cover newest-entry ordering, corrupt lines,
symlink refusal, Unicode-safe truncation, oversized source-capacity release,
live eviction, and Settings rejection:

```bash
cargo test --locked --no-default-features --features desktop-product \
  structured_log_reader --lib
cargo test --locked --no-default-features --features desktop-product \
  structured_log_memory_is_item_byte_and_message_bounded --lib
cargo test --locked --no-default-features --features desktop-product \
  structured_log_startup_loader_enforces_production_scan_and_byte_budgets --lib
cargo test --locked --no-default-features --features desktop-product \
  log_load_recent_setting_edit_validate_and_persist --lib
```

All fixtures use explicit temporary roots and never inspect the user's normal
application data.

On-disk rotation and retention have a separate focused gate:

```bash
cargo test --locked --no-default-features --features desktop-product \
  structured_log_writer --lib
cargo test --locked --no-default-features --features desktop-product \
  legacy_structured_log_disk_policy_is_normalized_and_reported --lib
cargo test --locked --no-default-features --features desktop-product \
  log_rotation_settings_edit_validate_persist_and_apply --lib
```

These tests use isolated temporary roots. They exercise the production 4 KiB
minimum record/file boundary, repeated rotation with a three-file retention
budget, the 4,096-entry prune scan ceiling, legacy settings above the 8 MiB/16
file maxima, and static active/rotated symlink refusal on Unix.

The same `structured_log_writer` suite drives 1,000 12 KiB records against a
deterministic 2 ms write delay. It requires non-waiting admission below 250 ms,
visible overload, queue state at or below 256 records/2 MiB, nonzero exact
oldest age while saturated, and zero retained items/bytes after flush. Separate
cases prove all admitted records flush in order, explicit shutdown joins the
worker, and refused symlink writes release their byte permits. The release TUI
lifecycle smoke and desktop shutdown tests remain the integration gates for
normal application shutdown.

On Linux, the focused suite also writes one encoded production record to the
kernel `/dev/full` device. It requires the real `ENOSPC` error to increment the
write-failure metric and verifies the associated item/byte permit returns to
zero. This device-only test does not open or alter application log roots. The
Diagnostics and Logs panels read the same in-memory metrics without logging
their observation, so viewing a failure cannot recursively enqueue records.

On the 2026-07-14 Linux debug test build, 1,000 submissions were admitted in
2.862 ms while the worker imposed 2 ms per filesystem operation. The queue
peaked at 167 records/2,083,052 bytes, explicitly dropped 831 overload records,
completed 169, and drained to zero items/bytes. This deterministic delay proves
admission isolation and budgets; it is not storage-device throughput.

## Micron link-action admission

Micron parser regressions cover standard links, shorthand links, and LXMF
autolinks. They cross target, individual-field, item-count, aggregate-field,
and raw-syntax limits; rejected syntax remains visible as a non-link span and
cannot reactivate an embedded autolink-looking substring.

```bash
cargo test --locked --no-default-features --features desktop-product \
  micron::parser::tests --lib
```

### Micron control and browser field-state admission

Control regressions cross raw syntax, name, value, descriptor-part, width,
document-item, and aggregate-string limits. Rejected controls remain literal,
cannot autolink embedded text, and never enter `BrowserSession.field_values`.
Session mutation/restore tests also prove an oversized update preserves the
previous valid value. The aggregate fixture is deterministic in-memory markup
and does not read or write persisted form state.

```bash
cargo test --locked --no-default-features --features desktop-product --lib \
  oversized_or_malformed_controls_remain_non_actionable
cargo test --locked --no-default-features --features desktop-product --lib \
  document_controls_are_item_and_owned_byte_bounded
cargo test --locked --no-default-features --features desktop-product --lib \
  oversized_controls_and_field_updates_do_not_enter_session_state
cargo test --locked --no-default-features --features desktop-product --lib \
  browser_field_state_is_item_and_aggregate_bounded
```

### Desktop browser field-editor admission

Desktop input regressions exercise both event paths into a page field. A
multi-byte UTF-8 insertion that would cross the 64 KiB value ceiling is rejected
as one operation, while a fitting insertion still succeeds. A full-value Iced
draft update is also rejected when it would cross the session's 4 MiB aggregate
form-state budget. In both cases the active editor and session retain the same
previous value. Fixtures are deterministic and use explicit temporary roots.

```bash
cargo test --locked --no-default-features --features desktop-product --lib \
  browser_field_insert_is_utf8_atomic_at_the_value_limit
cargo test --locked --no-default-features --features desktop-product --lib \
  browser_field_draft_preserves_ui_and_session_at_aggregate_limit
cargo test --locked --no-default-features --features desktop-product \
  --test input_model
```

### Micron rendered-action allocation sharing

The renderer regression uses one link with a maximum-sized forwarded field and
one 256-cell field control carrying a maximum-sized value. It proves all 128
link cells point to one immutable `LinkAction` allocation and all control cells
point to one allocation each for name, kind, and the 64 KiB value. It also
round-trips both through owned hit regions, preserving activation behavior.
This is a deterministic allocation-identity measurement, not an allocator- or
hardware-specific RSS benchmark.

```bash
cargo test --locked --no-default-features --features desktop-product --lib \
  rendered_cells_share_payload_bearing_action_metadata
```

### Micron document and rendered-output budgets

Parser regressions cross metadata item/aggregate/style-value ceilings, the
256 KiB source-line ceiling, and the 16,384-row ceiling. They verify rejected
content is not retained, `Document::limits_applied` is set, and a non-actionable
notice remains visible. Renderer regressions pass an absurd requested width,
cell-saturating content, and 65,535 rows; retained output stays within 4,096
columns, 65,535 rows, and 1,048,576 cells with a render-limit notice. Fixtures
are deterministic in-memory data and require no filesystem or network state.

```bash
cargo test --locked --no-default-features --features desktop-product --lib \
  document_metadata_is_item_and_owned_byte_bounded
cargo test --locked --no-default-features --features desktop-product --lib \
  document_source_lines_and_rows_are_bounded_with_visible_notice
cargo test --locked --no-default-features --features desktop-product --lib \
  renderer_bounds_width_rows_and_cells_with_visible_notice
```

### Micron fragment and document-link admission

Inline regressions generate 65,536 styled fragments on one admitted source
line, more than 4 MiB of otherwise valid span text, 4,097 small links, and one
more maximum-sized-target link than the 4 MiB action budget permits. Retained
documents contain at most 65,536 fragments including the fixed notice, 4 MiB
of span text, 4,096 actionable links, and 4 MiB of link target/field strings.
Rejected actions keep their labels as non-actionable spans and set the existing
document limit flag. Fixtures are deterministic in-memory data.

```bash
cargo test --locked --no-default-features --features desktop-product --lib \
  document_fragments_and_span_text_are_aggregate_bounded
cargo test --locked --no-default-features --features desktop-product --lib \
  document_link_actions_are_item_and_owned_byte_bounded
```

### Micron rendered-style allocation sharing

The style regression renders 1,024 identically styled authored cells and one
256-cell control. Each run must point to one immutable `TextStyle` allocation,
and generated default styles must reuse the process-wide default allocation.
Mutating one rendered cell through copy-on-write must leave its neighbor and
the shared original unchanged. Existing MicronPlus title emphasis, default-link
foreground, capture, TUI conversion, and Iced canvas tests cover consumers.

```bash
cargo test --locked --no-default-features --features desktop-product --lib \
  rendered_cells_share_styles_with_copy_on_write_mutation
cargo test --locked --no-default-features --features desktop-product --lib \
  micronplus_rendered_links_use_document_default_foreground
```

### MicronPlus and partial field admission

MicronPlus regressions prove an oversized field attribute creates neither a
typed button nor a lowered link and remains visible with a diagnostic. Partial
tests cross target, field-item, field-count, ID, retained-spec item, and
aggregate-byte limits using only in-memory markup. Invalid descriptors are not
scheduled as partial requests.

```bash
cargo test --locked --no-default-features --features desktop-product \
  oversized_micronplus_fields_remain_non_actionable --lib
cargo test --locked --no-default-features --features desktop-product \
  --test browser_partials
```

### MicronPlus structural admission

The MicronPlus module suite covers the complete structural parser and existing
render/lowering compatibility surface. Focused boundary regressions accept the
declared node, column, attribute, and retained-string ceilings; reject the next
unit, excessive nesting, and oversized lines; and prove repeated partial slots
cannot multiply one valid fragment beyond tree/layout budgets. Rejection leaves
the prior typed structure unchanged. A session regression also requires stale
tree/layout metadata to be removed and a visible fallback plus diagnostic to
remain:

```sh
cargo test --locked --no-default-features --features desktop-product \
  browser::micronplus::tests --lib
cargo test --locked --no-default-features --features desktop-product \
  micronplus_structural_rejection --lib
```

These are deterministic in-memory fixtures. They do not read cache, identity,
message, Reticulum, or server roots.

### MicronPlus widget and event admission

Widget tests fill the 256-widget ceiling, cross the 1,024-item per-widget
ceiling, require newest-edge append retention, reject oversized scalar and
derived-tree payloads atomically, and check store metrics remain within 4,096
items/4 MiB. Extraction crosses both 256 events and 1 MiB while requiring each
rejected event line to remain visible. Control history crosses its item/byte
ceilings and must retain the newest event. The application test requires an
invalid update to preserve prior widget state and emit visible rejection status:

```sh
cargo test --locked --no-default-features --features desktop-product \
  micronplus_widget_store --lib
cargo test --locked --no-default-features --features desktop-product \
  micronplus_widget_event_extraction --lib
cargo test --locked --no-default-features --features desktop-product \
  micronplus_control_event_history --lib
cargo test --locked --no-default-features --features desktop-product \
  micronplus_widget_rejection_is_visible --lib
```

All fixtures are generated in memory and do not deserialize or touch user data.

## OMENchat Smoke

Local single-client smoke:

```bash
bash scripts/release-omenchat-smoke.sh
```

Two-client recent-history smoke:

```bash
bash scripts/release-omenchat-smoke.sh --multi-client
```

Negotiated reaction qualification uses the same isolated harness and forces
the bounded reaction snapshot through the existing Resource transport:

```bash
bash scripts/release-omenchat-smoke.sh \
  --reaction-smoke \
  --multi-client \
  --restart-server
```

The reaction option persists each mutation intent before transmission,
deliberately discards one acknowledgement, replays the exact mutation, verifies
the original result, tests logical no-op and removal, and requires an
authoritative Resource snapshot. It creates isolated browser identities and
omenchatd state; it never uses the maintainer's normal roots.

Server-process restart with the same isolated server and browser roots:

```bash
bash scripts/release-omenchat-smoke.sh --restart-server
```

This runs the client as a fresh process after the server restart. It proves
state-root reopening and a second full exchange, not automatic reconnect by a
desktop process that remained alive.

One continuously running product process across an orderly server restart:

```bash
bash scripts/run-omenchat-continuous-reconnect.sh \
  --report target/omenchat-continuous-reconnect-report.json
```

The first exchange creates a marker only inside the harness's temporary root.
The wrapper then drains and restarts current omenchatd without stopping the
client process. The client must observe the old link close, open a different
link, reconnect the same in-memory session, and receive a second echoed message.
It also negotiates reactions before and after replacement, deliberately loses
and exactly replays an acknowledgement on the replacement Link, and requires
authoritative Resource snapshot, no-op, removal, and clean-intent evidence.
The retained report contains only versions and booleans. This exercises the
headless product smoke path; an interactive Iced-window restart soak remains a
separate presentation/lifecycle check.

Current-product two-client upload/Resource qualification:

```bash
bash scripts/run-omenchat-current-upload.sh \
  --report target/omenchat-current-upload-report.json
```

The sender uploads the existing deterministic 873-byte public OMENchat wire
fixture and fetches it through the server. A second client with a separate
identity/root discovers and fetches the same Resource. The harness requires
typed upload completion and Resource-available events with the exact byte count
for both clients. Reticulum Resource integrity remains enforced; raw payloads,
resource IDs, identities, destinations, paths, and reports are deleted.

Bounded local-history reducer and packaged SQLite capability:

```bash
cargo test --locked --no-default-features --features desktop-product \
  history_search::tests
cargo test --locked --no-default-features --features desktop-product \
  packaged_sqlite_build_exposes_working_fts5
```

The reducer tests use in-memory fixtures only. They enforce query, term, scan,
result, and copied-text ceilings and prove that opaque IDs, arbitrary LXMF
fields, and private attachment paths are not searchable. The FTS5 test creates
only a temporary table on an in-memory bundled-SQLite connection; no user
database or schema is touched.

The opt-in maximum-work measurement is excluded from normal test runs because
its deterministic fixture retains 64 MiB of message text:

```bash
cargo test --release --locked --no-default-features \
  --features desktop-product \
  history_search::tests::measure_maximum_bounded_lxmf_search \
  -- --ignored --exact --nocapture
```

It prints reducer-only full-miss and capped-hit durations. It is a measurement,
not a hardware-specific pass/fail benchmark, and it never opens an application
root or persistent store.

The OMENchat read-only history-store boundary is covered by:

```bash
cargo test --locked --no-default-features --features desktop-product \
  read_only_store
cargo test --locked --no-default-features --features desktop-product \
  history_search_loader
cargo test --locked --no-default-features --features desktop-product \
  read_only_thread_listing
cargo test --locked --no-default-features --features desktop-product \
  persisted_search_combines_bounded_stores
```

These tests use unique temporary paths. They verify read access, query-only
write rejection, non-creation of a missing database and parent directory, and
newest-first item/byte-bounded event loading. The LXMF fixture additionally
proves malformed JSON fails without creating recovery backups.
The combined fixture searches both isolated stores, verifies global ordering,
and ensures opaque routing keys never become presentation or searchable text.

Desktop search ownership and exhaustive routing:

```bash
cargo test --locked --no-default-features --features desktop-product \
  history_search_state::tests
cargo test --locked --no-default-features --features desktop-product \
  history_search_messages_have_one_compile_time_route
```

The owner tests prove one active job, newest-only pending replacement,
stale-completion rejection, and shutdown invalidation. They do not open a
store or start a Tokio task.

Current-product NomadNet page request qualification:

```bash
bash scripts/run-nomadnet-current-page.sh \
  --report target/nomadnet-current-page-report.json
```

The harness starts the standalone server's fixed `nomadnetwork.node` portal and
the canonical browser under separate temporary roots on an ephemeral loopback
interface. It requires link setup, the production direct-request send, and a
non-empty network response that decodes to the deterministic 309-byte,
17-line `text/x-micron` page. The retained report contains only versions,
public page shape, request primitive, and validation booleans. Raw destinations,
URLs, identities, paths, ports, logs, and state are deleted. This is
current-product portal evidence; current-Python application evidence is covered
by the separate drift lane below. Both the wrapper and nested smoke resolve
browser/server release binaries from `CARGO_TARGET_DIR` when supplied; nested
failure output is emitted only after the smoke's existing redaction boundary.

Expected result:

```text
outcome: pass
reason: OMENchat Link opened, room joined, and message echo was observed
```

## Issue Bundles

Collect a redacted report bundle:

```bash
bash scripts/release-collect.sh \
  --browser-root /tmp/omenbrowser-rs-test \
  --browser-root-2 /tmp/omenbrowser-rs-test-2 \
  --server-home /tmp/omenchatd-test
```

Review the created directory before sharing it.

## What To Test

- Start the browser with a fresh identity.
- Add a Reticulum gateway/interface.
- Browse a NomadNet page.
- Open multiple browser panes.
- Send and receive LXMF messages.
- Open an OMENchat server from `omenchat://...`.
- Switch OMENchat rooms, send messages, reconnect, and restart the server.
- Upload small images/GIFs in OMENchat.
- Verify scrollback, load older, and recent-history sync.

### OMENchat client history window

History-window regressions construct only isolated in-memory or temporary
SQLite state. They cross the 1,024-event ceiling, cross the 8 MiB owned-byte
ceiling, verify restore/live append keeps the recent edge even with a skewed
remote timestamp, and verify load-older keeps the older pagination edge. A
desktop persistence regression feeds 1,025 received events through the real
status/persistence boundary: 1,024 remain in the session while all 1,025 are
queryable from SQLite. The mock adapter is compiled and tested separately so it
cannot bypass the shared admission path.

```bash
cargo test --locked --no-default-features --features desktop-product \
  chat::client::tests --lib
cargo test --locked --no-default-features --features desktop-product \
  omenchat_received_history_persists_rows_beyond_the_memory_window --lib
cargo test --locked --no-default-features --features desktop-dev \
  chat::mock::tests --lib
```

### OMENchat client catalog admission

Catalog regressions prove that the 65th session is refused without evicting an
open session or creating a phantom desktop pane, and that the live transport
sends no frame for a refused open. Separate item and owned-byte cases exercise
room and user snapshots. SQLite tests insert more persisted rows than the
resident limits, verify active-room priority, and prove bounded reads leave the
oversized row stored but non-resident.

```bash
cargo test --locked --no-default-features --features desktop-product \
  client_refuses_session_overload --lib
cargo test --locked --no-default-features --features desktop-product \
  client_room_and_user_catalogs --lib
cargo test --locked --no-default-features --features desktop-product \
  live_room_and_user_catalog_snapshots_are_bounded_and_visible --lib
cargo test --locked --no-default-features --features desktop-product \
  live_open_refuses_session_overload_before_sending_frames --lib
cargo test --locked --no-default-features --features desktop-product \
  sqlite_catalog_reads_apply_item_and_byte_admission_limits --lib
cargo test --locked --no-default-features --features desktop-product \
  omenchat_ui_refuses_session_overload_without_creating_a_pane_target --lib
```

### OMENchat live client transfer admission

These regressions keep all state in memory. They saturate outgoing upload item
and owned-byte limits before a frame is sent; cross inline-download item,
declared-byte, per-resource, and fragment limits; reproduce overlapping
out-of-order chunks whose retained sum exceeds the declared file; verify normal
in-order/out-of-order completion releases accounting; and verify session
cancellation releases both transfer directions.

```bash
cargo test --locked --no-default-features --features desktop-product \
  'chat::live::tests::live_inline_download' --lib
cargo test --locked --no-default-features --features desktop-product \
  'chat::live::tests::live_pending_upload' --lib
cargo test --locked --no-default-features --features desktop-product \
  live_upload_offer_sends_accepted_resource_payload --lib
cargo test --locked --no-default-features --features desktop-product \
  live_session_cancellation_releases_pending_transfer_state --lib
cargo test --locked --no-default-features --features desktop-product \
  live_reconnect_releases_prior_link_transfer_state --lib
```

### OMENchat client presentation metadata

The metadata tests cross each semantic limit with multibyte UTF-8, prove the
shortener never splits a code point or exceeds its byte ceiling, reject
oversized operational room/user/server identifiers, cap retained MOTD/error/
status/actor text, and filter corrupt oversized SQLite rows before restore.
Mock and live adapters are tested separately.

```bash
cargo test --locked --no-default-features --features desktop-product \
  bounded_chat_text_preserves_utf8_and_exact_byte_ceiling --lib
cargo test --locked --no-default-features --features desktop-product \
  client_session_admission_bounds_presentation_metadata --lib
cargo test --locked --no-default-features --features desktop-product \
  client_session_admission_rejects_oversized_operational_identifiers --lib
cargo test --locked --no-default-features --features desktop-product \
  live_error_and_motd_text_are_utf8_byte_bounded --lib
cargo test --locked --no-default-features --features desktop-product \
  live_room_and_user_parsers_reject_oversized_operational_labels --lib
cargo test --locked --no-default-features --features desktop-product \
  live_outbound_operational_metadata_is_rejected_before_send --lib
cargo test --locked --no-default-features --features desktop-product \
  sqlite_presentation_metadata_reads_reject_oversized_rows --lib
cargo test --locked --no-default-features --features desktop-dev \
  mock_request_handler_bounds_descriptor_and_status_metadata --lib
```

### OMENchat descriptor admission

Descriptor tests prove oversized URI destinations and exact block fields are
rejected, display names shorten at a valid UTF-8 boundary, room/capability
collections obey item and byte limits, invalid Micron link fields do not
partially mutate a descriptor, and oversized blocks remain unlowered.

```bash
cargo test --locked --no-default-features --features desktop-product \
  chat::descriptor::tests --lib
```

## Desktop shutdown

Desktop close requests use an ordered `Running -> ShutdownRequested -> Draining
-> Closed` lifecycle. Shutdown stops subscriptions and rejects newly queued UI
work, flushes pending UI/directory persistence, gives the runtime five seconds
to stop, drains the bounded structured-log worker in parallel, and then asks
Iced to close the window normally. The application must
not use `process::exit` for routine window closure because that bypasses Rust
destructors and buffered guards.

The unit lifecycle test is safe and isolated:

```sh
cargo test --locked --no-default-features --features desktop-product \
  shutdown_phase_allows_only_the_ordered_lifecycle
```

The repository Linux-native qualification launches the canonical product under
an isolated Xvfb/i3 session. It creates a browser tab to leave a 500 ms
workspace-preference write pending, immediately delivers the window manager's
close protocol, and verifies normal process return, bounded close latency,
flushed shutdown tracing, parsed JSON files, the persisted tab, and absence of
temporary persistence files. It also parses the structured JSONL and requires
the queued startup record, proving the worker flush completed before return:

```sh
cargo build --release --locked --no-default-features \
  --features desktop-product --bin omenbrowser_rs
bash scripts/test-desktop-shutdown.sh
```

The harness refuses binaries with mock/development product identity and always
uses a newly generated temporary app root. It requires `Xvfb`, `i3`, `xdotool`,
`xdpyinfo`/`xprop`, `jq`, and `rg`. Debian-family package names are `xvfb`,
`i3-wm`, `xdotool`, `x11-utils`, `jq`, and `ripgrep`. Missing tools are reported
instead of installed. Windows and macOS still require their native close and
final-file qualification before release claims on those platforms.

On 2026-07-14, a freshly rebuilt canonical Linux product opened under this
harness in 1,203 ms and returned normally 137 ms after close. The pending
workspace state and structured startup log record were both durable and every
isolated JSON/JSONL file parsed. This is a local software-rendered Xvfb result,
not a native Windows/macOS timing claim.

## Desktop message routing

The desktop dispatcher exhaustively assigns every production message to one
subsystem before dispatch. The match has no wildcard, so adding a top-level
variant fails compilation until its owner is selected. Domain payloads are
expressed as smaller typed enums; `ThemeMessage`, `ClearwebMessage`,
`ExternalBrowserMessage`, `RuntimeMessage`, `IdentityMessage`, `PluginMessage`,
`DirectoryMessage`, `InterfaceMessage`, `DiagnosticsMessage`, `WorkspacePaneMessage`,
`ShellMessage`, `BrowserMessage`, `ConversationMessage`,
`ConversationCompletionMessage`, `OmenChatMessage`,
`OmenChatTransportCompletionMessage`, and `OmenChatMediaCompletionMessage` are
completed domains. Conversation commands and file-picker completion results use
separate typed envelopes; OMENchat commands remain separate from transport and
media/upload/decode/cache completions. Their focused ownership regressions are:

```sh
cargo test --locked --no-default-features --features desktop-product \
  theme_domain_messages_have_one_compile_time_route
cargo test --locked --no-default-features --features desktop-product \
  clearweb_domain_messages_have_one_compile_time_route
cargo test --locked --no-default-features --features desktop-product \
  external_browser_domain_messages_have_one_compile_time_route
cargo test --locked --no-default-features --features desktop-product \
  runtime_domain_messages_have_one_compile_time_route
cargo test --locked --no-default-features --features desktop-product \
  identity_domain_messages_have_one_compile_time_route
cargo test --locked --no-default-features --features desktop-product \
  plugin_domain_messages_have_one_compile_time_route
cargo test --locked --no-default-features --features desktop-product \
  directory_domain_messages_have_one_compile_time_route
cargo test --locked --no-default-features --features desktop-product \
  interface_domain_messages_have_one_compile_time_route
cargo test --locked --no-default-features --features desktop-product \
  diagnostics_domain_messages_have_one_compile_time_route
cargo test --locked --no-default-features --features desktop-product \
  workspace_pane_domain_messages_have_one_compile_time_route
cargo test --locked --no-default-features --features desktop-product \
  shell_domain_messages_have_one_compile_time_route
cargo test --locked --no-default-features --features desktop-product \
  browser_domain_messages_have_one_compile_time_route
cargo test --locked --no-default-features --features desktop-product \
  conversation_domain_messages_have_one_compile_time_route
cargo test --locked --no-default-features --features desktop-product \
  conversation_completion_messages_have_one_compile_time_route
cargo test --locked --no-default-features --features desktop-product \
  omenchat_domain_messages_have_one_compile_time_route
cargo test --locked --no-default-features --features desktop-product \
  omenchat_transport_completion_messages_have_one_compile_time_route
cargo test --locked --no-default-features --features desktop-product \
  omenchat_media_completion_messages_have_one_compile_time_route
cargo test --locked --no-default-features --features desktop-product \
  shutdown_gate_rejects_non_lifecycle_shell_messages
```

A classifier/handler disagreement must still never disappear in release. The
terminal invariant route emits a tracing error, persists an application error
entry without formatting the message payload, and surfaces `internal UI routing
error; see Logs` in status. Exercise that route with the test-only message:

```sh
cargo test --locked --no-default-features --features desktop-product \
  unhandled_message_is_release_visible_in_status_and_persisted_logs
```

The top-level message-size regression is enabled for every `desktop-ui` build,
including minimal UI, development, animated product, and static-media product
profiles. The closure matrix is:

```sh
for profile in desktop-ui desktop-dev desktop-product desktop-product-static-media; do
  cargo check --locked --no-default-features --features "$profile"
  cargo clippy --locked --no-default-features --features "$profile" --lib -- -D warnings
  cargo test --locked --no-default-features --features "$profile" --lib
done
```

The only unwrapped top-level variant is test-only and exists solely to prove
that a classifier/handler disagreement remains visible in release behavior.

## Workspace pane reconciliation

Pane-target reconciliation runs only after browser tabs, conversations, or
OMENchat sessions are removed. The focused mutation test deliberately creates
one stale target, proves no unrelated work removes it, then proves explicit
reconciliation removes only that pane:

```sh
cargo test --locked --no-default-features --features desktop-product \
  target_mutation_reconciliation_removes_only_the_stale_pane
```

## Event-driven desktop results

Runtime, browser, message, and diagnostics results remain payloads in the
bounded 256-item internal event queue. OMENchat frame/resource/close events also
share a 32 MiB cumulative permit that remains attached until handling. Focused
tests fill that byte budget exactly, require the next payload to be rejected,
and prove all accounting releases after handling. A second test fills all 256
item slots, attempts a payload-bearing event, and proves the failed reservation
does not retain items or bytes. Successful sends also advance a coalescing
Tokio watch generation; the stable Iced subscription emits
`InternalEventsReady` and drains the existing queue immediately. No recurring
desktop tick polls those result queues.

```sh
cargo test --locked --no-default-features --features desktop-product \
  internal_event_bus_wake_is_coalesced_and_drains_existing_bounded_queue
cargo test --locked --no-default-features --features desktop-product \
  omenchat_internal_event_payload
```

The subscription emits an initial drain notification so startup events queued
before watcher activation are not stranded.

Scroll restoration uses its own conditional 250 ms subscription only while
settling or bottom anchoring is active:

```sh
cargo test --locked --no-default-features --features desktop-product \
  scroll_settling_advances_only_from_its_conditional_subscription
```

UI-preference and live-directory persistence use a conditional one-shot
subscription keyed by the application and nearest pending deadline. A
reschedule changes the identity and cancels the obsolete timer; no subscription
exists when nothing is dirty:

```sh
cargo test --locked --no-default-features --features desktop-product \
  persistence_deadline
```

Process usage and runtime-interface statistics use a separate one-second
subscription only while Interfaces, Monitoring, or Network Doctor is active.
The general tick must not sample monitoring state:

```sh
cargo test --locked --no-default-features --features desktop-product \
  monitoring_sampling_runs_only_from_dedicated_section_tick
```

Direct-proof and propagated-transfer reconciliation use the nearest explicit
five-second safety deadline instead of the general one-second tick. Startup
reconciliation is immediate, and each run advances the keyed one-shot:

```sh
cargo test --locked --no-default-features --features desktop-product \
  lxmf_reconcile
```

Browser partial refreshes use the nearest valid tab/spec deadline as a keyed
one-shot subscription. Pending network results continue through the bounded
internal event queue; they do not require a polling timer. Exhausting a
partial's refresh count removes its deadline:

```sh
cargo test --locked --no-default-features --features desktop-product \
  browser_partial_stream_emits_once_at_the_requested_deadline
cargo test --locked --no-default-features --features desktop-product \
  due_partial_refresh_uses_schedule_and_preserves_specs
```

Live OMENchat heartbeat, automatic reconnect, and delayed recent-history work
share a nearest-deadline one-shot. The subscription is absent when there is no
connected transport, eligible reconnect, or pending history sync, and none of
this maintenance runs from the general desktop tick:

```sh
cargo test --locked --no-default-features --features desktop-product \
  omenchat_maintenance_deadline
```

The desktop also projects those owned sets, transports, and protocol events
into a typed per-session connection lifecycle. Focused regressions prove every
state label/retryability decision, fixed session-bounded ownership,
authenticating-to-joined transition on `RoomJoined`, quick reconnect after a
timeout, manual disconnected state after a non-retryable close, retry-limit
failure, and cleanup on session close:

```sh
cargo test --locked --no-default-features --features desktop-product \
  connection_state_labels_and_retryability_are_typed --lib
cargo test --locked --no-default-features --features desktop-product \
  omenchat_connection_state_is_bounded_by_sessions_and_join_is_event_driven --lib
cargo test --locked --no-default-features --features desktop-product \
  omenchat_reconnect_limit_projects_retryable_failed_state --lib
cargo test --locked --no-default-features --features desktop-product \
  omenchat_timeout_close_marks_session_for_quick_reconnect --lib
cargo test --locked --no-default-features --features desktop-product \
  omenchat_non_retryable_close_waits_for_manual_reconnect --lib
```

These tests do not claim live Reticulum handshake, server restart, or
mixed-version interoperability.

There is no unconditional desktop timer. Conversation and OMENchat
follow-bottom state is reconciled at handled-message and internal-event
boundaries, including after live OMENchat frames are drained:

```sh
cargo test --locked --no-default-features --features desktop-product \
  reconcile_follow_bottom_without_a_tick
```

## Desktop idle measurement

Build the canonical release product, then run the isolated native-session
harness. It always creates and removes its own temporary app root; the argument
is only the results directory:

```sh
cargo build --release --locked --no-default-features --features desktop-product
scripts/measure-desktop-idle.sh /tmp/omenbrowser-rs-idle-results
```

Defaults are a 60-second warmup and 600-second sample at one-second intervals.
Raw `/proc` samples, `pidstat`, `perf stat`, startup-to-window time, toolchain,
and median/p95 CPU, RSS, private-dirty memory, and file descriptors are retained.
Set `HEADLESS=1` to run beneath a disposable Xvfb/i3 session instead of using
the maintainer's display. The headless path requires the same host-only tools as
the native shutdown harness and verifies a normal window close after sampling.
Use `PERF_RECORD_SECONDS=30` for a separate post-sampling call-graph capture;
do not enable it during the authoritative CPU interval.

For a reviewed-baseline comparison, export the reviewed commit to a temporary
directory without checking it out over the worktree, build its explicit live
profile with no default features, and run identical durations. Record recurring
application messages only from a verified subscription inventory:

```sh
HEADLESS=1 WARMUP_SECONDS=60 SAMPLE_SECONDS=600 \
  RECURRING_APP_MESSAGES_PER_MINUTE=60 \
  OMENBROWSER_BINARY=/tmp/ce3a964/target/release/omenbrowser_rs \
  scripts/measure-desktop-idle.sh /tmp/omen-idle-baseline
HEADLESS=1 WARMUP_SECONDS=60 SAMPLE_SECONDS=600 \
  RECURRING_APP_MESSAGES_PER_MINUTE=0 \
  scripts/measure-desktop-idle.sh /tmp/omen-idle-current
scripts/compare-desktop-idle.sh \
  /tmp/omen-idle-baseline /tmp/omen-idle-current
```

The comparator refuses mismatched durations. It reports scheduler context
switches as a proxy distinct from application-message counts and incorporates
`perf stat` task-clock when available.
For harness-only smoke validation, shorten the run with `WARMUP_SECONDS` and
`SAMPLE_SECONDS`; do not publish those short values as a performance baseline.
GPU/frame activity requires appropriate vendor tooling and remains a separate
manual measurement.

For deterministic workspace restore and close/reopen stress, run:

```sh
cargo build --release --locked --no-default-features \
  --features desktop-product --bin omenbrowser_rs
scripts/measure-pane-stress.sh /tmp/omenbrowser-rs-pane-stress-results
```

The harness generates production-format settings and OMENchat SQLite state
under a disposable temporary root, restores 20 browser panes, 20 LXMF
conversation panes, and 10 OMENchat panes, and repeats three native Xvfb/i3
launch/normal-close cycles. It asserts the canonical non-mock product identity,
the restored pane/session counts, successful shutdown draining, valid persisted
settings, and post-run OMENchat restoration. The UI-only fixture uses external
Reticulum instance mode so launch bootstrap cannot generate identity material or
change its storage scope. Raw cycle timing, CPU, RSS, private-dirty memory, file
descriptors, and close latency are retained. `Xvfb`, `i3`, `xdotool`,
`xdpyinfo`/`xprop`, `jq`, and `rg` are required and are reported, never installed,
when absent. Page-render and message-to-visible interaction latency remain
separate measurements; window appearance is only the startup-to-window proxy.

For OMENchat media, run the interactive four-phase harness from a graphical
session:

```sh
scripts/measure-omenchat-media.sh /tmp/omenbrowser-rs-media-results
```

It creates a temporary isolated app root and deterministic two-frame 1x1 GIF,
then prompts for visible, maximized-hidden, section-hidden, and closed phases
in the same process. Each phase retains raw CPU, RSS, private-dirty, FD, and
context-switch samples plus median/p95 summaries. It also captures an ignored
release-mode production-decoder latency test and writes exact vendor GPU
observation commands. Missing GPU tooling remains pending, never zero by
assumption. Shortened sampling is only a harness smoke, not a release baseline.

## OMENchat v0.6.0-1 wire compatibility

The browser and independently built server consume the same public fixture
file at `fixtures/omenchat/v0_6_0_1_wire.rs`. Focused checks require both codecs
to emit and accept the tagged v0.6.0-1 session-open, room-message, and
history-resource-offer bytes, and require the legacy context/resource labels to
remain exact:

```sh
cargo test --locked --lib --no-default-features --features desktop-product \
  v0_6_0_1
cargo test --locked --lib --no-default-features --features desktop-product \
  clean_omenchat_accepts_generic_data_and_collapsed_legacy_context_only

(cd src/server && \
  cargo test --locked --no-default-features --features server-headless \
    v0_6_0_1)
(cd src/server && \
  cargo test --locked --no-default-features --features server-headless \
    live_server_routes_known_link_data_and_columba_context_zero_frames)
```

These are deterministic codec and admission checks. They do not claim that
separate v0.6/v0.9 processes completed link establishment, resource transfer,
restart, or reconnect; those isolated live combinations remain release gates.

## omenchatd machine-readable status

The standalone server regressions parse `status --json` and `doctor --json`,
require schema version 1, application/dependency/runtime ownership fields, and
typed doctor checks, then seed adversarial operator/MOTD text and an isolated
private root. Neither JSON document may contain those values, a private path,
credentials, or private identity material.

```bash
cargo test --locked --manifest-path src/server/Cargo.toml \
  --no-default-features --features server-headless machine_readable --lib
```

An isolated CLI smoke should additionally validate both emitted documents with
`jq` when available. These offline reports intentionally set
`runtime.live_metrics_available` to false and do not claim connectivity to a
running omenchatd process.

## OMENchat frame decode budgets

The browser and standalone server maintain identical pre-allocation MessagePack
frame limits and boundary tests:

```sh
cargo test --locked --no-default-features --features desktop-product \
  chat::codec::tests
cargo test --locked --manifest-path src/server/Cargo.toml \
  --no-default-features protocol::codec::tests
```

Coverage includes valid wire round trips and the scalar compatibility boundary,
plus oversized frame/scalar/container, excessive total-value, deep-nesting, and
trailing-data rejection. Compressed batch/resource budgets are tracked
separately and are not proven by these tests.

Compressed batch tests independently cover browser/server round trips,
over-limit compressed input, over-limit advertised output, and a compressed
payload whose actual expansion exceeds its advertised length. The same suite
also covers batch scalar/container/total-value/depth limits and trailing data:

```sh
cargo test --locked --no-default-features --features desktop-product \
  chat::protocol::batch::tests
cargo test --locked --manifest-path src/server/Cargo.toml \
  --no-default-features protocol::batch::tests
```

The standalone NomadNet portal request-resource decoder has a smaller,
request-specific MessagePack budget. Its focused test preserves configured path
matching and rejects oversized, deeply nested, and trailing input:

```sh
cargo test --locked --manifest-path src/server/Cargo.toml \
  --no-default-features --features live-reticulum nomadnet_resource_request
```

Browser-side native NomadNet response-resource decoding retains binary/string
compatibility and rejects oversized, deeply nested, or trailing MessagePack:

```sh
cargo test --locked --no-default-features --features desktop-product \
  native_link_response
```

Native LXMF delivery/propagation announce parsing preserves display-name and
stamp-cost compatibility while rejecting oversized, deep, and trailing data:

```sh
cargo test --locked --no-default-features --features desktop-product \
  lxmf_announce
cargo test --locked --no-default-features --features desktop-product \
  delivery_announce_app_data
```

Larger native LXMF propagation envelopes preserve signed/transient extraction
while rejecting oversized envelopes/entries/containers, deep nesting, and
trailing data:

```sh
cargo test --locked --no-default-features --features desktop-product \
  propagation_envelope
```

The native adapter compatibility decoder applies separate transient-id and
payload-list budgets and rejects wrong-width ids, oversized lists/entries, deep
nesting, and trailing input:

```sh
cargo test --locked --no-default-features --features desktop-product \
  native_lxmf_adapter_propagation_decoders_enforce_shape_budgets
```

Upstream LXMF wire unpack is guarded by local raw/fixed-storage/Python-storage
preflight checks. Focused coverage keeps normal signed wire decoding and rejects
oversized or deeply nested embedded payloads before upstream allocation:

```sh
cargo test --locked --no-default-features --features desktop-product \
  lxmf_wire
cargo test --locked --no-default-features --features desktop-product \
  signed_ticketed_wire_message_decodes_reply_ticket_metadata
```

## MessagePack fuzzing

The non-shipping `fuzz/` package targets browser and standalone-server frame and
batch decoders. The wrapper requires cargo-fuzz 0.13.2 and the pinned nightly
commit, writes crash artifacts only under `/tmp` by default, and never uses app
or server roots:

```sh
FUZZ_RUNS=10000 FUZZ_MAX_LEN=4194305 scripts/fuzz-msgpack.sh
```

The separate [fuzz lockfile](../fuzz/Cargo.lock) pins `libfuzzer-sys` and all
build-only transitive dependencies. Corpus, artifacts, and the sanitizer target
directory are ignored. The wrapper seeds each corpus with an input exactly
`FUZZ_MAX_LEN` bytes so the configured byte boundary is executed even when
libFuzzer's mutation schedule remains short.

On 2026-07-12 both targets completed 10,000 mutation runs with no sanitizer
finding (client 100 MiB reported RSS, server 101 MiB; mutation corpus maxima 14
and 11 bytes). A follow-up seeded run executed a 4,194,305-byte input in each
target with no finding and 101 MiB reported RSS. Generated 3.1 GiB sanitizer
build data and transient corpora were removed after recording the results.

Decoder-only rejection timing/allocation uses a non-shipping counting allocator
binary. It excludes construction of the hostile input from the measured window:

```sh
scripts/measure-msgpack-rejection.sh /tmp/omenbrowser-rs-msgpack-rejection.tsv
```

The report also includes 512 KiB client/server binary-frame and binary-batch
encode cases with simulated legacy payload-clone comparisons. These cases measure encoder peak
live bytes, allocation count, and latency while the wire-equivalence tests
enforce byte-for-byte compatibility.

Across 2026-07-12 Linux x86_64 release runs, valid frames took 296–374 ns
with a 320-byte peak live-allocation delta. Oversized declared scalars rejected
in 58–74 ns with 33 bytes peak; already-materialized 4,194,305-byte inputs
rejected in 47–68 ns with 31 bytes peak. Batch declared-oversize rejection took
216–275 ns with 81 bytes peak. These are workstation reference values, not
cross-platform performance promises.

The same counting-allocator command measures bounded bzip2 rejection. On the
2026-07-12 release build, client/server valid 64 KiB batches decoded in
184–197 µs with a 196,902-byte peak delta. Advertised output above 4 MiB
rejected in 38–40 ns with 37 bytes peak. A highly compressible stream expanding
beyond 4 MiB but advertising one byte rejected in 267–273 µs with only 8,280
bytes peak. The compressed fixtures are constructed before measurement.

## omenchatd backpressure soak

Run the ignored optimized production-queue soak on Linux:

```sh
scripts/measure-omenchatd-backpressure.sh /tmp/omenchatd-backpressure-results
```

The default 60-second run drives both payload-bearing Reticulum server queue
implementations at a 1 ms producer interval while their consumers accept one
resource every 20 ms. Resources are 64 KiB and rotate across eight link IDs.
Every 100 resource attempts also submits and awaits a link-close/reconnect
control event. This uses the production `ReticulumOmenchatTransport`,
`EventQueueSender`, `QueueBudget`, `QueuePermit`, `TransportCommand`, and
`OmenchatLinkEvent` paths, without creating a real server identity or touching
an operator root.

The test requires at least 10x measured producer/consumer pressure, visible
overload rejects, control completion within 250 ms, payload item/byte ceilings,
RSS growth below the combined 48 MiB payload budgets plus a documented 64 MiB
allocator/runtime margin, and zero retained permits after cancellation. Linux
`/proc` supplies RSS and FD counts. Raw one-second samples, the machine-readable
summary, host, limits, and toolchain are retained in the requested output
directory. Shorter `OMENCHATD_QUEUE_SOAK_SECONDS` values are harness smoke only.
This proves queue-level resource/reconnect behavior, not Reticulum/LXMF wire
interoperability.

## omenchatd SQLite policy and event IDs

The standalone store tests use only uniquely named databases under the system
temporary directory. They verify persistent PRAGMAs directly and launch 12
contending connections to prove that per-room event IDs remain unique and
monotonic. The same suite verifies transactional migration from an unversioned
database and confirms that a future-version database is refused without
changing its version or marker data:

```sh
cargo test --locked --manifest-path src/server/Cargo.toml \
  --no-default-features store::tests
```

This test covers connection policy, transactional ID correctness, schema
version safety, and backup collision behavior. It does not claim that
synchronous SQLite work is isolated from async Reticulum tasks; live Reticulum
isolation is covered separately by the worker tests below.

Version-0 migration now uses SQLite's online-backup API before its transaction.
The store suite verifies that the retained backup is readable, contains the
original marker and schema version, is mode `0600` on Unix, and that a
pre-existing backup collision aborts without overwriting either file. Restore
automation is confirmation-gated through `database restore-migration-backup`.
Focused recovery tests validate a generated sibling backup, migrate and
integrity-check a private staging copy, preserve the previous active database,
and prove the selected source remains unchanged. Corrupt/current-schema inputs
and active WAL state are refused. An injected atomic-publication failure leaves
the active and source databases unchanged and removes staging/WAL/SHM files.
Schema-6 tests migrate a representative schema-5 database without losing
reaction state and inject failure at every revision table/index/version/commit
boundary. Recovery tests also prove that a schema-5 export preserves reaction
state while removing every schema-6 revision object, and that a deeper
schema-4 export removes both feature layers.
Schema-7 tests migrate representative schema-6 history without eagerly seeding
or scanning it, lazily initialize the high-water mark, inject rollback at every
sequence table/version/commit boundary, preserve monotonic IDs across
concurrent writers and deletion of newest/all retained rows, and fail closed
at SQLite integer exhaustion. The confirmation-gated schema-6 export removes
only sequence metadata while retaining ordinary history, reactions, and
message revisions.
Schema-8 tests migrate schema 7 without scanning history, inject failure at
each usage-table/version/commit boundary, advance legacy accounting in
256-event batches across restart-safe cursors, account an append during
backfill exactly once, compare stable retained bytes to the bounded source
rows, and roll back event/sequence changes on accounting exhaustion. The
schema-7 copy preserves sequences and history while removing only the usage
ledger.
Focused dormant revision-executor tests cover author/moderator/mute policy,
immutable originals, cross-room/non-message/deleted targets, eight corrections
plus tombstone, reaction cleanup, soft/hard state saturation, bounded audit
pruning, revision-ID non-reuse, transaction and result-codec rollback, exact
restart replay without repeat fan-out, hash conflict, and identical
inline/Resource snapshot decoding. `message-revisions-v1` remains rejected by
normal capability negotiation.
Dormant live-plumbing tests request the capability through the real session
path and prove the resulting production binding stays disabled. Separate
isolated tests inject a capable binding to cover same-room, identity-matched
fan-out; exclusion of base and stale-identity Links; join/history-following
snapshots; exact replay without repeated fan-out; and binding retirement after
identity replacement or Link close. Injection is test-only and does not claim
capability activation.
An injected migration executes one schema statement and then fails on the next.
That test proves the partial schema is transactionally removed, the source
marker and version 0 survive, and the backup stays readable.

```sh
cargo test --locked --manifest-path src/server/Cargo.toml \
  --no-default-features database_recovery::tests
cargo test --locked --manifest-path src/server/Cargo.toml \
  --no-default-features database_restore
```

The live worker admits exactly one session/database operation to Tokio's
blocking pool. Focused tests hold the worker lock to simulate a stalled database
operation and prove Tokio timers continue, then hold its sole permit and prove
additional work is rejected without entering a waiter queue:

```sh
cargo test --locked --manifest-path src/server/Cargo.toml \
  --no-default-features --features live-reticulum live_worker
```

Production monitoring reports in-flight, completed, rejected, average-latency,
and maximum-latency values on the existing `queues:` line. The focused suite
also holds a real SQLite `BEGIN IMMEDIATE` write lock against an isolated WAL
database. With a test-only 100 ms busy timeout, the worker reports the locked
write in about 100 ms while an independent 10 ms Tokio timer continues to run.
All database/WAL files are removed afterward.

Run the ignored optimized sustained database-worker measurement on Linux:

```sh
scripts/measure-omenchatd-db.sh /tmp/omenchatd-db-results
```

The default 60-second run keeps the production `SessionEngine`,
`LiveServerWorker`, MessagePack frame decoder, and persistent `OmenchatStore`
in the path. Eight isolated peers submit a room message every 10 ms. The
single-admission worker commits accepted operations and rejects concurrent
admission explicitly; it never builds a blocking-task waiter queue. A separate
10 ms Tokio heartbeat measures runtime responsiveness. The harness samples
worker metrics, RSS, file descriptors, and database/WAL/SHM bytes, then closes
and reopens the store, verifies consecutive committed event IDs, and runs
SQLite `integrity_check`. It uses only a generated temporary root and removes
it after a passing run. It does not create a Reticulum identity or claim wire
interoperability.

On 2026-07-13, the 60-second reference run committed 6,000 operations and
explicitly rejected 42,000 busy submissions. Worker average/maximum latency was
355/1,272 us, maximum heartbeat lateness was 1,817 us, RSS grew 794,624 bytes,
and file descriptors stayed at 13. The reopened database contained 6,000
consecutive soak events and passed `integrity_check`. Shorter
`OMENCHATD_DB_SOAK_SECONDS` runs are harness smoke only. Remaining database
work includes native Reticulum load.

Run the deterministic SQLite process-kill boundary regression separately:

```sh
scripts/test-omenchatd-crash-recovery.sh
```

The harness spawns the current Rust test binary against unique temporary WAL
databases, waits for synchronized boundary markers, and terminates each child
with `Child::kill`. Event children stop after a committed room event and with
the next event inside an open `BEGIN IMMEDIATE` transaction. Upload children
stop after the temporary file is synchronized, after the replacement rename is
directory-synchronized, after the new ledger row commits, and after old-file
eviction but before stale-row cleanup. The parent reopens after every kill and
proves committed retention or conservative fail-closed reconciliation,
monotonic event IDs, expected upload ledger/file state, safe stale-row repair,
and SQLite `integrity_check`. No child receives secret material. Every passing
case removes its database, WAL, SHM, upload files, and marker. Controlled backup
restore is covered separately by the recovery tests above.

The first bounded administrative actor slice covers CLI room management:

```sh
cargo test --locked --manifest-path src/server/Cargo.toml \
  --no-default-features admin_db::tests
cargo test --locked --manifest-path src/server/Cargo.toml \
  --no-default-features \
  cli_room_mutations_use_the_initialized_administrative_database_path
```

The actor owns one production `OmenchatStore` connection on a named background
thread. Its fixed 16-item queue uses non-waiting admission, each response has a
six-second deadline, and metrics report queued/in-flight/completed/rejected and
average/maximum latency. The saturation regression holds the worker, fills all
16 queue positions, proves the next request is rejected, then verifies every
slot drains with no retained queue depth. The CLI regression exercises actual
create/topic/archive dispatch against an isolated initialized server home.
The interactive TUI regressions use the same actor and production store. They
verify asynchronous create/topic/archive completion, audit logging, and a
1,024-item/1 MiB room-cache ceiling. A real `BEGIN IMMEDIATE` writer lock holds
SQLite while dashboard topic and moderation mutations return to the event loop within 50 ms;
the operation remains pending and completes after the lock is released. Visible
room-consuming panels request a non-blocking refresh at most every five seconds.
Moderation tests additionally cover typed list/status/role/delete operations,
transactional multi-user pruning, asynchronous completion/audit behavior, and
the 4,096-item/2 MiB user-display cache. The line-console regression reuses one
actor across room and user commands and asserts its queue/completion metrics.
Upload-ledger inspection and confirmed repair use the same bounded actor in
read-only and existing-schema maintenance modes respectively. Controlled backup
restore uses a separate offline staging/preservation path; the process-kill
harness above covers event transactions and upload replacement.

## omenchatd typed configuration

The standalone configuration suite checks current sectioned TOML, compatible
version-0 flat keys, version refusal, fixed-policy validation, malformed and
misspelled security limits, and escaped string round trips:

```sh
cargo test --locked --manifest-path src/server/Cargo.toml \
  --no-default-features config::tests
```

These tests use isolated temporary roots. Save coverage verifies owner-only
target/backup files, last-known-good contents, refusal to replace an invalid
existing document, and an injected final-rename failure that preserves the old
loadable file and leaves no temporary residue.

## omenchatd bounded logging

The dependency-free server log writer has focused tests for its 1 MiB byte
budget, UTF-8-safe 16 KiB record cap, isolated flush visibility, permit release,
and observable file-open failures:

```sh
cargo test --locked --manifest-path src/server/Cargo.toml \
  --no-default-features server_log::tests
```

The rotation regression uses a small deterministic threshold and proves the
active file plus a fixed backup count stay capped while the newest record
remains active. Native slow-disk throughput remains an F-022 measurement.
The priority regression fills every normal record slot, confirms the next
routine message drops, then proves a typed warning is admitted through the
reserved priority lane with no priority drop. A separate regression proves an
`Info` message containing every former warning keyword remains routine while a
neutral-text typed `Error` receives priority. Another writes a typed warning
through the real worker and proves the timestamp/text file line is unchanged.

The slow-consumer regression submits 5,000 routine records while a synthetic
writer consumes one record every 2 ms behind a 16-record queue. Across three
2026-07-12 Linux x86_64 runs, peak retained permits stayed at 17 (16 queued plus
one in flight), 4,975–4,976 records dropped explicitly, median admission was
3.0–3.1 µs, p95 was 3.1–6.0 µs, and maximum admission was 32–41 µs. These
are workstation reference values and a deterministic blocking-isolation test,
not a native-filesystem throughput promise.

Run the optimized repeated-lifecycle filesystem soak on Linux:

```sh
scripts/measure-omenchatd-logging.sh /tmp/omenchatd-log-results
```

The default 60-second run creates three independent production-sized bounded
writers and real isolated log files. Each writer adds a deterministic 2 ms
delay immediately before the real buffered filesystem write, while the
producer drives routine and reserved-priority admission. The test measures
admission latency, queue items/bytes/oldest age, explicit drops, write failures,
RSS, file descriptors, graceful drain/join, rotation, and total retention. It
removes all generated log roots after success; the requested result directory
contains only the captured test output and machine-readable summary. Linux
`/proc` supplies RSS and FD evidence. Shorter
`OMENCHATD_LOG_SOAK_SECONDS` values are harness smoke only.

On 2026-07-13, the 60-second reference run submitted 381,909 records and
explicitly dropped 353,113 routine overload records, with zero priority drops
and zero write failures. Admission median/p95/maximum was
11,587/13,231/182,671 ns. Peak retained queue state was 64 items/777,932 bytes;
RSS grew 4,722,688 bytes and file descriptors stayed at 4. All three lifecycles
rotated and flushed cleanly. Twelve retained files totaled 96,580,273 bytes,
below the three-cycle aggregate of the per-writer 32 MiB production cap. This
qualifies the logger callback boundary under deterministic slow-write pressure;
it is not Reticulum wire interoperability or a claim about a specific storage
device's throughput.

After replacing text scanning with typed severity on the same date, an
identical 60-second run submitted 382,037 records and explicitly dropped
353,185 routine overload records, again with zero priority drops/write
failures. Median admission fell from 11,587 to 565 ns (95.1%) and p95 from
13,231 to 1,778 ns (86.6%). The maximum was a higher scheduler outlier at
223,653 ns versus 182,671 ns. Peak queue state remained 64 items/777,932 bytes,
FDs remained 4, RSS grew 4,898,816 bytes, and all three lifecycles rotated.
Twelve retained files totaled 97,271,092 bytes within the aggregate cap.

## omenchatd upload-ledger reconciliation

The store reconciliation regression creates only an isolated temporary
identity directory and in-memory SQLite store. It verifies tracked/disk totals,
missing rows, retained orphan files, and an unsafe tracked path outside the
identity root without mutating any uncertain state:

```sh
cargo test --locked --manifest-path src/server/Cargo.toml \
  --no-default-features upload_ledger_reconciliation
```

Doctor coverage builds the same isolated discrepancy state and proves warning
classification for missing/orphan files, failure classification for an
out-of-root path, read-only inspection, and no file/row mutation. Explicit
repair coverage requires `--confirm`, removes only missing/out-of-root ledger
rows, preserves every file and orphan, and proves repeated repair is
idempotent. Actor-mode coverage proves inspection cannot mutate through its
read-only connection and confirmed repair uses existing-current-schema mode.
Only the upload-root path crosses the bounded queue; reconciliation path lists
remain owned by the worker. Repair waits for a definitive worker result so a
caller timeout cannot precede a later commit. Indexed-planning coverage proves a dirty or byte-mismatched first
scan blocks admission, a clean scan selects oldest ledger rows within quota,
and subsequent plans use the index without another directory traversal.

Crash-boundary coverage reopens a persistent isolated database after each
replacement boundary: durable rename before row commit is an orphan and
blocks; row commit before eviction safely over-counts and selects the old row;
physical eviction before row cleanup is missing and blocks; completed cleanup
returns to a clean indexed plan. No maintainer data is used.

On Linux, `kernel_enospc_preserves_last_committed_upload` routes the upload
writer to the kernel `/dev/full` device. It requires the real `ENOSPC` errno
(28), proves the database callback is not reached, and verifies the isolated
identity directory retains exactly the previous committed upload. This needs
no mount, privilege, host-package installation, or real user-data path.

## Browser page cache bounds

Page admission tests exercise the shared retained-data boundary independently
of disk caching. They accept exact declared limits, reject the next markup or
request byte, reject excessive/deep JSON metadata, and prove whole-page and
partial failures preserve the previously installed page, generation, history,
and retained partial content:

```sh
cargo test --locked --no-default-features --features desktop-product \
  browser::page::tests --lib
cargo test --locked --no-default-features --features desktop-product \
  rejected_page_admission --lib
cargo test --locked --no-default-features --features desktop-product \
  rejected_partial_admission --lib
```

These tests allocate only process memory and use no user roots. Runtime
responses, cache restores, persisted navigation, partial composition, and
MicronPlus post-normalization all use the same validator.

### Browser navigation-history admission

Navigation-history tests cross the 512-item and 1 MiB URL-string ceilings.
Live navigation must retain the newest contiguous edge and preserve the current
pointer. Persisted restore must retain one contiguous window around its saved
pointer, preserve back/forward ordering, reject an oversized selected URL
without changing the current page, and stop at an invalid adjacent edge rather
than skipping it. Oversized resolved input must fail before runtime dispatch:

```sh
cargo test --locked --no-default-features --features desktop-product \
  browser::session::tests::live_navigation_history --lib
cargo test --locked --no-default-features --features desktop-product \
  browser::session::tests::restored_navigation --lib
cargo test --locked --no-default-features --features desktop-product \
  open_rejects_oversized_resolved_url_before_runtime_dispatch --lib
```

These fixtures are generated in process memory. The runtime-backed rejection
test uses only a generated temporary cache/download root and performs no live
network request.

The page-cache regression suite uses only temporary directories. It verifies
round-trip and TTL behavior, rejects a serialized record above 5 MiB, crosses
the 256-item limit, and proves monotonic oldest-entry eviction even when all
writes share one wall-clock second. A sentinel `.mu` file remains untouched by
normal indexed lookup, while deleting `.page-cache-index.json` explicitly
exercises one-time reconstruction and legacy expiry-filename migration:

```sh
cargo test --locked --test browser_cache
```

The complete cache is also limited to 64 MiB. Native disk/latency measurement
and inventory of other application caches remain Phase 3 work.

GIF media policy tests reject excessive dimensions, frame counts and decoded
byte estimates before `iced_gif` decode, accept a valid one-pixel fixture, and
prove deterministic cache eviction at both the 12-item and 64 MiB ceilings.
The remote path performs a capped read and decode behind a two-permit bounded
blocking gate. Native visible/hidden animation RSS and GPU measurements remain
part of Phase 0 qualification.

Upload media worker coverage proves bounded byte and file sources, rejection
before copying an oversized sparse source, cleanup after malformed GIF decode,
and exact cached bytes for a valid non-image payload. Queue coverage proves the
16-job/16 MiB reservation ceiling and that draining releases the full budget.
Generation tests replace an in-flight key, reject its older completion, accept
only the replacement generation, and prove session cancellation removes queued
reservations plus all in-flight generation authority.
They also prove replacement and session closure signal the corresponding
cooperative worker tokens. Worker coverage proves a pre-cancelled job is
rejected before creating its destination file.
Workspace visibility coverage proves tiled panes are visible, maximization
hides every sibling, and leaving the Browser/Messages workspace hides all
workspace panes. OMENchat uses this predicate to withhold animated GIF frame
handles and construct only the static image fallback for hidden panes.
The reduced-motion accessibility regression exercises the same production
predicate in all visible/hidden and enabled/disabled combinations. Settings
coverage proves the preference defaults off for older files and round-trips
when enabled. Native GPU submission remains a platform measurement gate.
The adversarial GIF corpus runs seven named malformed boundary cases and 512
reproducible xorshift mutations of the valid one-pixel fixture. Every case must
return without unwinding; any accepted mutation must remain within the decoded
byte ceiling. The deterministic seed is embedded in the test for exact replay.
Media-state cache coverage crosses both the 256-entry and 256 KiB metadata
ceilings, verifies deterministic oldest-entry eviction, and rejects a single
entry larger than the complete byte budget.
Disk-media cache coverage crosses its 64-item and 128 MiB limits, protects the
current file, and verifies that a normal indexed prune does not enumerate an
unindexed sentinel. A deliberately path-traversing index is rejected and
rebuilt without modifying the outside fixture.
Crash-boundary coverage adds a committed-but-unindexed file and abandoned
temporary beside a valid index, persists the dirty marker, and verifies the
next prune indexes the committed file, removes the temporary, and clears the
marker only after writing the repaired index.
Clearweb media streaming coverage uses loopback-only HTTP responses and an
isolated temporary root. It accepts an exact cumulative limit, rejects both an
oversized declared length and an oversized streamed body, preserves exact
bytes, and proves the over-limit temporary file is removed. It also proves no
temporary file remains after success and a pre-existing final file cannot be
replaced.
URL-policy coverage permits public domains/IPs and `.onion`, rejects embedded
credentials, non-HTTP schemes, local/mDNS/single-label names and special-use IP
literals, and checks redirect depth, local targets, and HTTPS downgrade. The
SOCKS client cache test crosses its four-entry limit and proves LRU refresh and
eviction without making an external request.
Shared download-writer coverage proves complete bytes are atomically published,
temporary files are removed, and an existing destination is preserved. The
native adapter regression uses its static Reticulum page-transport seam, seeds
the original filename, and verifies the response is committed to the numbered
path without changing the original.
Eight concurrent helper writes exercise the two-permit blocking boundary and
verify every complete result without temporary residue.

LXMF delivered-transient cache tests use isolated files. They verify legacy and
versioned round trips, age pruning, and deterministic oldest-entry pruning above
the 65,536-item ceiling. Files at exactly 8 MiB are accepted while a sparse
next-byte file, directory, and Unix symlink are rejected before reading or
backup and remain untouched. Malformed syntax or invalid transient IDs default
only after an exact owner-only no-clobber backup is synchronized; the source
remains in place and current-namespace retention is four files/32 MiB without
touching legacy names. Save tests prove private atomic replacement, semantic
preflight that preserves the prior file, no staging residue, and an injected
pre-commit fault that preserves exact prior bytes.

Directory-store tests use isolated roots and preserve numeric trust values,
current JSON, saved preferences, announcement debounce, transient aging, and
the 256/1,024 live bounds. Exact 8 MiB input is accepted; a sparse next-byte
file, directory, and Unix symlink are rejected before read or backup and remain
untouched. Malformed syntax and a 4,097-entry semantic overload default only
after an exact owner-only no-clobber backup is synchronized; the source remains
in place and current-namespace retention is four files/32 MiB without touching
legacy names. A next-byte live display name rejects before mutation. Save tests
prove private atomic replacement, no staging residue, injected pre-commit
preservation, and rollback of a trust change when publication fails.

Interface configuration tests use isolated roots and preserve existing profile,
gateway-preset, and generated Reticulum config formats. They cover exact and
next-byte file bounds, profile/preset item limits, configuration-injection
rejection, legacy preset migration without source removal, symlink refusal,
Unix private modes, failed-mutation rollback, identity preservation, and an
injected pre-commit replacement fault that leaves the prior bytes intact and
removes its staging file. Run the focused matrix with:

```sh
cargo test --locked --test interface_config --no-default-features \
  --features mock-runtime
cargo test --locked --test interface_config --no-default-features \
  --features desktop-product
```

Browser form-state tests use isolated roots and cover current/legacy round
trips, age pruning, explicit forget actions, newest-page retention across the
512-page ceiling, field count/value rejection, and preservation after rejected
updates. Exact 4 MiB input is accepted; a sparse next-byte file, directory, and
Unix symlink are rejected before read or backup and remain untouched. Malformed
input defaults only after an exact owner-only no-clobber backup is synchronized;
the source remains in place and current-namespace retention is four files/
16 MiB without touching legacy names. Save coverage proves private atomic
replacement, no temporary residue, injected pre-commit preservation, and
in-memory rollback when a forget action cannot persist.

Application-settings admission tests use isolated roots and accept an exact
8 MiB JSON file, reject a sparse next-byte file before reading or backup, refuse
directory and Unix valid/broken symlink paths, preserve bounded malformed-file
backup/default recovery, and prove an oversized save preserves the prior file
without creating staging. Malformed recovery uses invalid UTF-8, requires an
exact byte-for-byte owner-only backup without reopening or changing the source,
leaves no staging, and preserves an existing legacy-name backup collision. An
injected publish failure inspects the staged admitted bytes, preserves a
different current source, and cleans all recovery output. Recovery coverage
also injects a final-destination collision and requires no-clobber publication
to preserve its exact sentinel bytes. Persistence coverage also leaves the
former predictable temporary filename untouched, refuses directory and
valid/broken symlink targets, verifies Unix owner-only replacement, and injects
replacement failure below the production helper to require exact previous bytes
and zero staging residue:

Retention coverage performs seven changing malformed recoveries and requires
only the newest four byte-distinct backups. Five sparse 8 MiB legacy backups
plus a new recovery must finish within four files/32 MiB. Unix coverage proves
a matching backup symlink and referent are ignored, while 4,097 sibling entries
must fail before publication under the 4,096-entry scan ceiling. The saturated
fixture is removed explicitly after the assertion.

Semantic settings coverage accepts exact collection/container/depth boundaries
and rejects the next browser tab, conversation tab, bookmark, deletion
tombstone, pane, plugin ID, attachment, browser-history item/byte, focused-link
field/item, extension field/container/node, and recursive layout/extension
depth. A syntactically valid over-limit file must produce an exact-byte backup
and complete defaults with no partial field restoration. The matching save
test requires validation before staging and exact preservation of the previous
settings file.

Pre-deserialization coverage drives the production fixed-stack scanner at the
exact and next depth, per-container item, raw-string, and aggregate-token
ceilings, plus mismatched and unterminated structures. A valid JSON file with a
next-byte raw string must be rejected before typed decoding, backed up byte for
byte, and replaced only in memory with complete defaults. The matching save
fault must preserve the prior file and leave no staging output.

```bash
cargo test --locked --no-default-features --features desktop-product \
  --test app_settings
cargo test --locked --no-default-features --features desktop-product \
  storage::settings::tests::settings_replace_failure --lib
cargo test --locked --no-default-features --features desktop-product \
  storage::settings::tests::corrupt_backup_publish_failure --lib
```

No test reads or changes the maintainer's application root. Symlink rejection
also requires the referent bytes and link itself to remain unchanged.

The native LXMF SDK ticket-cache regression preserves validate-to-deliver
single-consumer handoff, crosses the 1,024-entry/256 KiB ceilings, verifies
oldest unmatched validation eviction, and requires a ticket above 256 bytes to
fail validation rather than being silently downgraded.

The integrated issuer regressions use only generated temporary Reticulum roots.
They require one serialized inclusion under concurrent same-peer requests,
case-normalized peer identity, one-day attempted-inclusion throttling, restart
reuse, near-expiry renewal, exact issuer bytes/expiry in the signed LXMF field,
and rejection of expired or wrong-sized overrides. Corrupt and symlinked state
must fail without replacing the file or referent.

```bash
cargo test --locked --no-default-features --features desktop-product \
  'runtime::native_lxmf::tickets::tests' --lib
cargo test --locked --no-default-features --features desktop-product \
  sdk_wire_delivery_uses_validated_issuer_ticket_exactly --lib
```

## Cache index latency

Run the ignored optimized cache measurement with isolated generated fixtures:

```bash
bash scripts/measure-cache-indexes.sh
```

On the 2026-07-12 Linux x86_64 host, 256-entry browser lookup measured 9,581 ns
median/11,616 ns p95 through the index versus 361,047/426,288 ns for the former
directory-scan lookup shape (37.7x median, 36.7x p95). At 64 media files,
indexed maintenance including index persistence measured 65,821/79,957 ns
versus 157,412/182,474 ns for the former two-pass listing/metadata/sort/kept-set
shape (2.39x median, 2.28x p95). These are same-process algorithm-shape
comparisons on an isolated temporary filesystem, not cross-platform storage
throughput claims.

The reusable native workflow has an explicit atomic-cache replacement gate on
Windows MSVC and both macOS architectures. It replaces an existing file and
then exercises repeated browser-page index and form-state publication. A local
`x86_64-pc-windows-gnu` product check proves the Windows implementation and raw
system binding compile, but it is not reported as native execution.
Live OMENchat resource lifecycle coverage proves fetch consumes rather than
clones payloads, rejects a resource above 8 MiB, crosses the 16-item/16 MiB
retained-resource budgets, crosses the 32-item/4 MiB deferred-offer budgets,
evicts the oldest unmatched payload, and releases deferred bytes on match.
The same focused transport suite fills the desktop's 64-item/4 MiB inbound and
outbound frame queues and its four-item/16 MiB outbound resource queue. It
requires explicit overload, exact item/byte ceilings, oversized frame/resource
and resource-ID refusal, byte-accounting release after receive/take, and correct
accounting when a deferred offer is replayed. No fixture opens application data
or a live Reticulum link.
The application channel regression first fills its shared payload permit with
four production-sized resources totaling 32 MiB, rejects the next payload, and
requires channel accounting to reach zero after handling. It separately proves
that an item-full send releases its just-acquired payload permit. The staging
regression then fills the post-channel boundary to 256
frames/16 MiB, four production-sized resources totaling 32 MiB, and 256 close
events carrying 256 KiB of reasons. It requires the next event in each class to be rejected and counted,
then drains each class and requires its retained-byte accounting to return to
zero. Payloads are moved from the runtime event into staging, so this test also
exercises the ownership path whose source review must remain free of the former
full frame/resource clone.
Clean-bridge admission coverage distinguishes `omenchat-frame:` (1 MiB),
`omenchat-resource:` (8 MiB), and unrelated metadata. This protects event-bus
forwarding; transfer-time inbound allocation remains constrained by the pinned
reticulum-rs transport's own 64 MiB/8,192-part global limits because its public
API has no inbound resource cancellation method.
Disk-retention coverage uses isolated temporary roots and sparse fixtures to
cross the 64-file and 128 MiB ceilings without consuming maintainer storage.
It verifies that the current committed file survives and excess regular files
are removed.

The optional-animation split is checked in both directions:

```bash
cargo test --locked --no-default-features --features desktop-product \
  gif_decode_policy_accepts_bounded_one_pixel_gif
cargo test --locked --no-default-features --features desktop-product-static-media \
  media_cache_worker_bounds_sources_and_cleans_failed_decode
bash scripts/verify-product-features.sh
```

The static-media regression proves a GIF remains cached with `animated = false`
and no decoded frame handle. The graph assertion proves `iced_gif` is absent
while live Reticulum/OMENchat product features remain enabled.

The same graph assertion enforces the Phase 5 Iced admission record: canonical
products resolve exactly Iced 0.14.0, dormant adjunct crates cannot leak into
the product, and the in-memory GIF path cannot regain its unused default
`async-fs` feature. The assertion requires the crate's Tokio compatibility
backend, which reuses the product's existing runtime because iced_gif requires
one async path backend even though OMENbrowser decodes from bytes. Each dormant
adjunct feature is also checked independently for a second Iced version.

On the 2026-07-12 Linux x86_64 release build, the canonical animated binary was
51,066,568 bytes and the static-media binary was 50,830,608 bytes: a 235,960-byte
(0.46%) reduction. This is a linkage-size comparison only; native
startup-to-interactive and RSS/GPU comparisons remain pending.

The optional widget/icon audit uses:

```bash
bash scripts/verify-product-features.sh
cargo clippy --locked --no-default-features \
  --features desktop-product,desktop-widgets -- -D warnings
```

The assertion rejects the Lucide, Nerd, and Codicon `iced_fonts` features.
`iced_aw` still owns a transitive `iced_fonts` text-support edge, but OMENbrowser
selects no bundled icon family and has no source reference to its icon APIs.
Linux x86_64 release binaries measured 51,053,712 bytes before removal and
51,053,208 bytes after, a 504-byte reduction. Link-time dead-code elimination
already discarded the unused font data; the main improvement is eliminating
the unused feature/macro/asset compile surface without changing rendered icons.

## Dependency security

The checked-in `deny.toml` is the dependency-admission policy for both the root
application and standalone server. CI pins cargo-deny 0.20.2 and checks every
defined feature across the Linux, Windows, Intel macOS, and Apple Silicon macOS
graphs:

```bash
cargo deny --locked --all-features check licenses bans sources
cargo deny --manifest-path src/server/Cargo.toml --locked --all-features \
  check licenses bans sources
```

These commands deny unapproved licenses, wildcard dependency requirements,
unknown registries, and Git dependencies. Duplicate crate versions remain
visible warnings because the current Iced/platform graph legitimately contains
parallel major versions; review any new warning rather than assuming it is
accepted.

Run the machine-checked accepted-advisory boundary plus both independent raw
lockfile inspections:

```bash
bash scripts/verify-accepted-advisories.sh
cargo audit --no-fetch
cargo audit --no-fetch --file src/server/Cargo.lock
cargo deny --locked --all-features check advisories
cargo deny --manifest-path src/server/Cargo.toml --locked --all-features \
  check advisories
```

The verifier requires the current raw root audit to fail only on the two
constrained `quick-xml` 0.39.2 advisories documented in
`docs/maintenance/DEPENDENCY_SECURITY.md`, proves their exact proc-macro-only
dependency boundary, and then applies only those two IDs to the accepted audit.
Any additional vulnerability or dependency-path change is a regression. The
server currently has no vulnerability-class finding or allowed warning.

RUSTSEC-2026-0002 is resolved in both optional TUI profiles by Ratatui 0.30.2
and `lru` 0.18.1. The dependency gate also aligns Crossterm 0.29.0, preserves
the layout cache, and proves that `paste` is absent. Qualify both profiles with:

```bash
bash scripts/verify-tui-dependencies.sh
cargo clippy --locked --no-default-features --features tui -- -D warnings
cargo test --locked --no-default-features --features tui
cargo clippy --locked --manifest-path src/server/Cargo.toml \
  --no-default-features --features server-full -- -D warnings
cargo test --locked --manifest-path src/server/Cargo.toml \
  --no-default-features --features server-full
```

RUSTSEC-2025-0134 is an unmaintained warning for `rustls-pemfile` 2.2.0 in
`lxmf-sdk`'s RPC backend. It is present in both canonical desktop products,
absent from omenchatd, and has no patched release. The RPC backend is active
product behavior, so do not disable it or ignore the advisory merely to obtain
a green audit. Confirm its feature path with:

```bash
cargo tree --locked --no-default-features --features desktop-product \
  -e features -i rustls-pemfile@2.2.0
```

The migration and regression gate is recorded in
`docs/maintenance/DEPENDENCY_SECURITY.md`.

RUSTSEC-2026-0206 and RUSTSEC-2026-0192 cover the unmaintained font crates
`rustybuzz` and `ttf-parser`. Canonical animated and static-media products use
`harfrust` plus `skrifa` for shaping/scaling and must not activate
`rustybuzz`; the latter remains lock-only through Iced's dormant SVG renderer.
`ttf-parser` remains active through `fontdb` and `ab_glyph`, so its warning is
not resolved. Verify the safe product boundary with:

```bash
bash scripts/verify-product-features.sh
cargo tree --locked --no-default-features --features desktop-product \
  -i ttf-parser@0.25.1
```

Do not enable SVG in a release alias or replace the font stack solely to hide
an audit warning. The required upstream migration and malformed-font/render
qualification are documented in `docs/maintenance/DEPENDENCY_SECURITY.md`.

RUSTSEC-2025-0141 marks `bincode` 1.3.3 unmaintained. It is lock-only for both
release products and omenchatd, but Iced's explicit debug/time-travel tooling
activates it inside `iced_beacon`. The product graph must continue excluding
that TCP debug protocol:

```bash
bash scripts/verify-product-features.sh
set +e
OMENBROWSER_BROWSER_FEATURES='desktop-product,iced/debug' \
  bash scripts/verify-product-features.sh
test "$?" -ne 0
set -e
```

The negative run must report that the animated product activates excluded
crate `bincode`. Do not enable Iced debug/time-travel in a product alias or
replace its private encoding as an application wire-format change. The
upstream migration and bounded-frame completion gate are recorded in
`docs/maintenance/DEPENDENCY_SECURITY.md`.

RUSTSEC-2024-0436 marks the compile-time `paste` macro unmaintained. Both
desktop products reach it through `rav1e` for explicitly supported AVIF media;
Apple targets additionally reach it through the native `metal` graphics
backend. Linux and Windows must resolve exactly `rav1e`, while Apple must
resolve exactly `metal` and `rav1e`. Ratatui 0.30.2 removes it from the optional
root TUI and omenchatd `server-full`. The two graph gates fix those boundaries:

```bash
bash scripts/verify-product-features.sh
bash scripts/verify-tui-dependencies.sh
cargo tree --locked --no-default-features --features desktop-product \
  -i paste@1.0.15 --prefix depth
```

Any new depth-1 desktop parent or any TUI occurrence is a release regression.
Removing AVIF merely to clear an informational build-time warning is not
compatible with current media behavior. Native TUI smoke remains required.

The Windows product Clippy gate also enforces the desktop router's strict
sub-128-byte `Message` bound. Payload-bearing OMENchat media completions keep
their typed payload behind one `Box`; removing that boundary can make every
`Result<Task<Message>, Message>` handler cross Clippy's large-error threshold
on Windows even when the Linux layout remains below it.

## Native product dependency identity

The canonical product profile selects bundled SQLite on Linux, Windows, and
macOS. X11, Wayland, and the XDG portal picker are Linux-only dependencies. Run:

```bash
bash scripts/verify-product-features.sh
```

The assertion inspects Cargo's target-specific feature graphs. It does not claim
that Linux graph inspection is a native Windows/macOS compile or runtime test;
those native CI gates remain required.

## Native compile and test gate

`.github/workflows/native-checks.yml` is a reusable, read-only matrix covering
Windows 2025 x86_64, macOS 15 Intel, and macOS 15 Apple Silicon. Each runner
checks and tests the browser's explicit `desktop-product` and root `tui`
profiles plus the standalone server's `server-headless` and `server-full`
profiles, then runs Clippy with warnings denied. The matrix also runs the
product and TUI dependency-identity scripts before compilation. CI invokes the
matrix for normal changes, and the package workflow cannot begin its Linux
artifact job until the same native gate passes.

The workflow is a compile/unit-test prerequisite, not an installer or GUI-launch
claim. Installer lifecycle and interactive file-dialog/window smoke tests remain
separate native release gates.

All four product profiles run strict Clippy across every declared target. This
includes examples and test-only helpers, which caught two cross-target defects
before the hosted matrix: the mixed-version SQLite probe lacked its
`chat-client` feature requirement, and a Linux-only server log soak helper was
compiled as dead test code on Windows. The workflow-security verifier requires
these all-target Clippy commands so later edits cannot silently narrow them.

A Linux host with the GNU Windows target and MinGW can run a useful compile-only
preflight:

```bash
cargo test --locked --target x86_64-pc-windows-gnu \
  --no-default-features --features desktop-product --all-targets --no-run
cargo clippy --locked --target x86_64-pc-windows-gnu \
  --no-default-features --features desktop-product --all-targets -- -D warnings
cargo test --locked --target x86_64-pc-windows-gnu \
  --no-default-features --features tui --all-targets --no-run
cargo clippy --locked --target x86_64-pc-windows-gnu \
  --no-default-features --features tui --all-targets -- -D warnings
cargo test --locked --target x86_64-pc-windows-gnu \
  --manifest-path src/server/Cargo.toml --no-default-features \
  --features server-headless --all-targets --no-run
cargo clippy --locked --target x86_64-pc-windows-gnu \
  --manifest-path src/server/Cargo.toml --no-default-features \
  --features server-headless --all-targets -- -D warnings
cargo test --locked --target x86_64-pc-windows-gnu \
  --manifest-path src/server/Cargo.toml --no-default-features \
  --features server-full --all-targets --no-run
cargo clippy --locked --target x86_64-pc-windows-gnu \
  --manifest-path src/server/Cargo.toml --no-default-features \
  --features server-full --all-targets -- -D warnings
```

This proves Windows conditional compilation and linkage only. It does not
execute Windows binaries, exercise MSVC, or substitute for the hosted Windows
and macOS jobs.

The hosted matrix additionally executes the actual release-profile command-line
entry points without creating application state:

```bash
bash scripts/test-native-cli-identity.sh
```

The smoke runs `--version` for desktop-product, root TUI, headless omenchatd,
and full omenchatd. It requires the native Rust host target, canonical product
feature identity, mock/test exclusion, and the expected server headless/TUI
split. It also executes the browser and server `--help` entry points and checks
that isolated-root and operator-diagnostics options remain present. It does not
launch a GUI/TUI, start Reticulum, create an identity, or read a default user
root.

`scripts/test-tui-lifecycle.sh` runs a deterministic terminal lifecycle harness
under the root `tui` feature. An injected lifecycle records raw-mode,
alternate-screen, and mouse-capture transitions, proving normal drop restoration
and rollback after failure at every enter boundary. A Ratatui test backend then
renders one 100x30 frame from an explicit temporary app root, routes `q`, flushes
pending UI preferences, and removes only that root. It never reads the default
application, identity, Reticulum, message, or cache paths.

The harness is suitable for non-interactive native CI and catches application
lifecycle regressions, including the former partial-enter leak. It does not
exercise a real Windows console or macOS terminal emulator, so interactive raw
mode, key/mouse delivery, platform signal registration, and abrupt-process
restoration remain manual/native-PTY release checks on those systems.

```bash
bash scripts/test-tui-lifecycle.sh
```

Linux additionally runs `scripts/test-tui-real-pty.sh`. It builds the actual
root TUI binary, verifies `tui:on`, and launches three independent Crossterm
processes inside util-linux `script`, each with its own isolated app root and
PTY baseline. The cases receive one `SIGTERM`, one `SIGINT`, and two `SIGTERM`
notifications 10 ms apart. Before each signal sequence, the harness changes
that controlling PTY through 0x0, 1x1, 40x10, and 100x30 and requires the
process to remain alive after every redraw interval. Each process must return
status zero, restore its original PTY dimensions, and match both the exact
`stty -g` state and terminal size from before the application.
Signal-delivery-to-process-exit latency is measured with timestamp ordering
checks and a conservative 3,000 ms gate.

The TUI's single abort-on-drop Tokio task listens for Unix `SIGTERM` and
`SIGINT` for its entire lifetime (and repeatedly awaits Windows console Ctrl-C).
Every notification stores `true` in the same atomic request, so bursts coalesce
without a counter, queue, forced-exit path, or retained permit. The event loop
checks that request within its existing 200 ms interval and routes it through
the same graceful `App::quit` persistence path as `q`. Transcripts are
discarded and all private results plus app data are removed on exit.

The harness detects tools without installing them. `script` is supplied by the
`util-linux` package; `stty`, `timeout`, `mktemp`, and `date` are supplied by
`coreutils`. It deliberately exits as Linux-only instead of treating a
non-interactive test backend as a real native console.

```bash
bash scripts/test-tui-real-pty.sh
```

The first PTY run exposed a zero-height startup panic: `body_rect` used eager
`then_some` evaluation, so its guarded unsigned subtraction still ran. The
layout now rejects header-only and zero-height terminals before subtraction,
with regression coverage across every height at or below the fixed header and
footer allocation. The real PTY resize sequence now exercises that boundary
during a live session as well as at startup.

The focused Ctrl-C and external-request routing regressions are isolated and do
not read normal user state:

```bash
cargo test --locked --no-default-features --features tui \
  control_c_requests_graceful_quit_even_while_editing
cargo test --locked --no-default-features --features tui \
  repeated_external_signal_requests_coalesce_into_graceful_quit
cargo test --locked --no-default-features --features tui \
  repeated_signal_during_synchronous_shutdown_stays_bounded_and_restores_terminal
```

The second regression uses zero-capacity test channels to pause the synchronous
shutdown boundary after its first request has been consumed. A separate thread
issues two more requests before releasing shutdown. The test proves they occupy
only the same boolean state, then runs the real `App::quit` persistence path,
parses the flushed settings from an explicit temporary root, and verifies every
terminal mode is restored on guard drop. This deterministic ownership test does
not inject a sleep into production or claim that an OS signal landed inside a
particular kernel filesystem write.

The optional root `tui` feature explicitly enables Tokio's `signal` capability.
No crate, version, or lockfile entry was added: the TUI graph already contains
the Unix signal registry through Crossterm, and the canonical desktop product
already enables Tokio signal support through its existing live-network graph.
The TUI dependency assertion prevents the root profile from losing its explicit
signal edge.

Before the listener, the same Linux PTY `SIGTERM` scenario returned status 143
and bypassed terminal guard drop. It now returns zero and restores the terminal.
On the 2026-07-14 repeated-signal reference run, delivery-to-exit measurements
were 52 ms for one SIGTERM, 55 ms for one SIGINT, and 55 ms for two SIGTERM
notifications, all below the 3,000 ms gate. These are lifecycle bounds, not
application performance benchmarks. This does not claim restoration after
`SIGKILL`, forced task termination, or native slow-filesystem signal timing
inside a particular persistence syscall.

## Rust 1.97 Clippy baseline

The canonical product, static-media product, and both standalone-server
profiles must pass Clippy without suppressing warnings:

```bash
cargo clippy --locked --no-default-features --features desktop-product -- -D warnings
cargo clippy --locked --no-default-features --features desktop-product-static-media -- -D warnings
cargo clippy --locked --no-default-features --features tui --all-targets -- -D warnings
cargo clippy --locked --manifest-path src/server/Cargo.toml \
  --no-default-features --features server-headless -- -D warnings
cargo clippy --locked --manifest-path src/server/Cargo.toml \
  --no-default-features --features server-full -- -D warnings
```

The July 2026 Rust 1.97 baseline initially exposed 40 warnings. Completion
payloads for live OMENchat open/reconnect and media-cache jobs are now boxed at
the Iced message boundary, and a regression caps `desktop::Message` at 128
bytes. Queue overload still reports explicit full/closed states without
returning the rejected payload inside a large error. Remaining fixes use typed
request/update option structs and mechanical modernizations; no Clippy lint is
allowed or disabled to make the gate pass.

## Development-profile measurement

F-019 uses fresh isolated `CARGO_TARGET_DIR` roots, the canonical
`desktop-product`, two build jobs, Rust 1.97, and Bash's built-in timer. GNU
`/usr/bin/time` was unavailable (the missing host package is `time`), so peak
compiler RSS was not recorded.

On the 2026-07-12/13 Linux x86_64 reference machine:

| Dependency optimization | Clean build | Root-source incremental | No-change build | Target bytes |
|---|---:|---:|---:|---:|
| `opt-level = 3` | 1177.945 s | 9.474 s | 0.624 s | 5,977,759,004 |
| `opt-level = 1` | 922.576 s | 7.700 s | 0.818 s | 5,606,883,942 |

Level 1 saved 255.369 seconds (21.68%) on the clean build, 1.774 seconds
(18.72%) on the root-source incremental build, and 370,875,062 bytes (6.20%).
The sub-second no-change difference is treated as noise, not a regression.

Both isolated debug binaries also completed a five-second warmup and ten-second
X11 idle smoke using temporary application roots. Level 1 opened a window in
245 ms, retained 207,620 KiB median RSS, and remained alive through sampling;
level 3 recorded 124 ms and 207,600 KiB. These single short samples establish
interactive viability, not a statistically meaningful runtime-performance win.
Release-mode Phase 0 measurements remain authoritative for shipped behavior.

## Runtime thread-policy measurement

The compatibility entrypoint delegates Tokio construction to
`runtime::bootstrap`. Focused tests verify the host-aware one-to-four worker
policy, exact eight-thread blocking backstop, `omen-main-async` name, and actual
execution on the named multithread worker pool:

```bash
cargo test --locked --no-default-features --features tui \
  runtime::bootstrap --lib
cargo test --locked --no-default-features --features tui \
  runtime::thread_policy --lib
```

The global blocking ceiling is not admission control. Filesystem, decoding,
compression, stamp work, and SQLite paths must retain their smaller explicit
bounded strategies. This ownership-only extraction does not replace the
optimized low-core measurement or claim a performance change.

Run the ignored optimized comparison with isolated generated files:

```bash
bash scripts/measure-runtime-threads.sh
bash scripts/measure-runtime-threads.sh --two-core
```

The harness compares the desktop's current four-worker/eight-blocking-thread
runtime with Tokio's host-aware default. It reports worker count, queue depth,
median/p95 completion latency for a deterministic yielded-task burst, and the
elapsed time for actual atomic file writes through the shared bounded blocking
gate. Results are diagnostic: repeat them on low-core native targets and pair
them with live page/message latency before changing product defaults.
The `--two-core` case uses Linux `taskset` from `util-linux`; other platforms
must apply their native CPU-affinity mechanism or run on low-core hardware.

## Identity material admission

Identity-manager tests use only generated temporary roots. They admit an exact
64 KiB regular identity, reject a sparse next-byte file and empty material,
and on Unix prove attach never follows a symbolic link or changes its referent.
An oversized import must fail before backing up or replacing the existing
managed target. Provider output is preflighted before creating any identity or
backup file. Discovery accepts exactly 256 regular profiles, rejects the next,
refuses work after 4,096 directory entries, ignores linked entries, and rejects
a linked identity root. The saturated fixture is explicitly removed after its
assertion.

Publication tests inject failures after a private staging file is synchronized
but before commit, proving an existing identity remains byte-identical, a new
identity is not exposed, and staging is removed. Unix coverage checks owner-only
mode and linked-root refusal. Managed-backup coverage crosses the 16-file/1 MiB
retention policy, preserves legacy-name material, and saturates the 4,096-entry
scan: the new durable backup remains, while the prior managed identity is not
replaced. All fixtures use generated temporary roots.

```bash
cargo test --locked --no-default-features --features desktop-product \
  --test identity_manager
```

No test reads, writes, attaches, imports, exports, or deletes the maintainer's
real identity or Reticulum roots.

## Message-store persistence admission

Message-store tests use generated isolated roots. They accept an exact 8 MiB
thread, reject the next byte without copying it, reject 4,097 retained messages,
and prove filesystem-unsafe peer keys map deterministically without escaping the
message directory. Unix coverage also proves an existing single-component
legacy filename remains readable and updates without creating a duplicate.
Discovery accepts 256
threads and rejects the next, caps aggregate retained input at 64 MiB, and
refuses work after 4,096 entries. Unix coverage verifies owner-only publication
and that a thread symlink and its referent remain untouched. Repeated malformed
loads keep four recognized backups while preserving legacy-name material. An
injected failure after staging fsync but before replacement preserves the prior
thread byte-for-byte and leaves no staging file.

```bash
cargo test --locked --no-default-features --features desktop-product \
  --test message_store
cargo test --locked --no-default-features --features desktop-product \
  messaging::store::publication_tests --lib
```

The suite never opens the maintainer's real message or identity roots.

## LXMF outbound TTL and restart reconciliation

Outbound TTL tests use only generated operation identifiers and isolated
message roots. They cover the exact one-second and 24-hour policy boundaries,
absolute-deadline round-trip, malformed/partial retained fields, rejection
before runtime admission, remaining-TTL SDK mapping, dispatch-boundary expiry,
new identity after an expired retry, visible scheduled expiry, and durable
idempotent reconciliation after reopening the message store. The UI assertion
checks fixed TTL/expiry text and does not introduce a countdown subscription.

```bash
cargo test --locked --no-default-features --features desktop-product ttl --lib
cargo test --locked --no-default-features --features desktop-product \
  --test message_store outbound_ttl_reconciliation_is_durable_and_restart_idempotent
cargo test --locked --no-default-features --features desktop-product \
  --test messaging_delivery_status expired_outbound_operation_is_rejected_before_runtime_admission
```

These tests do not start a Reticulum/LXMF daemon. External-daemon enforcement
and late authoritative delivery correction remain live interoperability gates.

## LXMF SDK history ownership and restart recovery

Typed history tests use an in-memory upstream RPC store and explicit temporary
OMEN message roots. They cover request/page bounds, peer-filtered typed mapping,
content-preserving receipt reconciliation, correction of a locally expired row,
missed-inbound import, refusal to invent SDK-only outbound history,
deleted-conversation tombstones, and restart/replay idempotency.

```bash
cargo test --locked --no-default-features --features desktop-product sdk_history
cargo test --locked --no-default-features --features desktop-product lxmf_history
```

These tests do not claim a live external `reticulumd`, Python LXMF, history
beyond the bounded four-page recovery ceiling, or mixed-version history
interoperability.

## Reticulum 0.9 direct NomadNet request candidate

The Phase 4 candidate test uses generated identities, a synthetic interface
hash, and an in-memory pair of Reticulum 0.9 links. It proves that the existing
bounded NomadNet frame can be encrypted as `PacketContext::Request`, that its
direct request ID is the first 16 bytes of the final packet hash, that the peer
observes the same ID and plaintext, and that a correlated `Response` packet is
accepted. A separate assertion rejects inactive links before construction.

```bash
cargo test --locked --no-default-features --features desktop-product \
  reticulum09_direct_request_candidate -- --nocapture
```

Production page fetch uses this direct primitive when the packed request fits
the packet MDU and retains request-resource above that boundary. This isolated
test alone does not claim Python handler dispatch, real interface delivery,
timeout/cancellation, link-close/reuse, or response byte equality.

The oversized request-resource ownership regressions use an isolated in-memory
0.9 transport, synthetic active link, and temporary event channels. They wait
for the outbound advertisement and require browser cancellation and response
timeout to emit an actual initiator-cancel packet plus an outbound NomadNet
resource lifecycle event. Metadata regressions separately prove successful
small pages identify `reticulum-transport/direct-request`; Resource ownership
tests continue to exercise the oversized compatibility primitive.

```bash
cargo test --locked --no-default-features --features desktop-product \
  nomadnet_request_releases_outbound_resource --lib
cargo test --locked --no-default-features --features desktop-product \
  native_request_backend_metadata_is_visible_in_status_and_trace --lib
cargo test --locked --no-default-features --features desktop-product \
  diagnostics_live_fetch_card_extracts_success_metadata --lib
cargo test --locked --no-default-features --features tui \
  live_fetch_summary_names_request_resource_compatibility_primitive --lib
```

Current-Python NomadNet interoperability is explicit and ignored by default:

```bash
bash scripts/run-current-python-drift.sh \
  --report target/current-python-drift-report.json
```

That lane installs exact RNS 1.3.8/NomadNet 1.2.7 packages in a disposable
environment and requires exact bytes for four combinations: empty direct
request/direct response, executable form direct request/direct response,
oversized request Resource/direct response, and direct request/large response
Resource. It also requires typed outbound and inbound Resource completion. A
second fault scenario runs delayed Python handlers: one must reach the exact
two-second response timeout, and one is cancelled only after the production
runtime reports that its request was dispatched. Python must observe exactly
two requests after both delayed handlers drain, proving neither exit silently
replays an executable action. A third scenario fetches the same executable page
twice and requires Python to observe both requests on one link; it records the
first and reused-request latency without imposing a timing threshold. The lane
then runs two warmups plus eight alternated measured samples per request
primitive on one link. In the complete drift lane, direct requests measured
34,339 us median and 39,979 us p95; request Resources measured 80,474 us median
and 87,872 us p95.
The complete lane repeats that exact workload under `cargo test --release`,
sets `OMEN_REQUIRE_OPTIMIZED_NOMADNET_MEASUREMENT=1`, and fails if debug
assertions remain enabled. Its 2026-07-18 release-profile run measured direct
requests at 35,138 us median/40,998 us p95 and request Resources at 78,756 us
median/86,923 us p95. The machine-readable drift report retains only those
aggregates, the `release` profile label, sample count, and same-link boolean.
Finally, a bounded retained-link soak alternates 16 direct/Resource requests on
one link, includes a two-second idle interval, has Python explicitly close that
link, then alternates 16 more requests on exactly one replacement. It requires
32 exact responses, two generations of 16 requests, at most one Python-side
active link, and no replay or third link generation. The 2026-07-18 focused
reference run completed the exchange in 4,411 ms and recovered the second
16-request generation in 1,004 ms; the complete lane passed.
These observations have no timing pass threshold. The lane does not establish a
pinned NomadNet reference.

The NomadNet presentation regressions prove that the existing typed resource
direction survives the application boundary and that an empty native response
is reported as a successful, valid empty page:

```bash
cargo test --locked --no-default-features --features desktop-product \
  nomadnet_resource_status_distinguishes_request_and_response_transfer_direction --lib
cargo test --locked --no-default-features --features desktop-product \
  native_page_response_marks_valid_empty_body_without_treating_it_as_failure --lib
cargo test --locked --no-default-features --features desktop-product \
  browser_page_loaded_status_calls_out_valid_empty_native_response --lib
```

Per-tab correlation is covered separately. The first regression creates two
tab operations, verifies progress changes only the exact tab, replaces one
operation, rejects the stale identifier, and releases only the exact finished
operation. The other checks prove that the native runtime passes the identifier
into the page transport context and that request-resource cancellation/timeout
lifecycle events retain it:

```bash
cargo test --locked --no-default-features --features desktop-product \
  browser_resource_operation_correlation_is_tab_scoped_bounded_and_exactly_released --lib
cargo test --locked --no-default-features --features desktop-product \
  native_fetch_passes_browser_operation_id_to_page_transport_context --lib
cargo test --locked --no-default-features --features desktop-product \
  cancelled_nomadnet_request_releases_outbound_resource_and_reports_direction --lib
cargo test --locked --no-default-features --features desktop-product \
  timed_out_nomadnet_request_releases_outbound_resource --lib
```

The operation map retains at most one entry per browser tab. These tests do not
claim live concurrent Python/NomadNet transfer interoperability.

Reticulum 0.9 link ownership is covered by two local lifecycle regressions.
The first proves the transport returns one active link for repeated destination
lookups, an explicit close emits `LinkClose`, and a later lookup creates a new
pending link. The second proves the fixed 32-stripe page coordinator excludes
same-stripe teardown ownership, permits another stripe, and releases a
cancelled waiter without acquiring the guard:

```bash
cargo test --locked --no-default-features --features desktop-product \
  reticulum09_reuses_active_page_link_and_reconnects_only_after_close --lib
cargo test --locked --no-default-features --features desktop-product \
  nomadnet_page_link_coordinator_serializes_same_stripe_and_is_cancellable --lib
```

Production still closes the page link after each request. These tests do not
claim pinned-Python repeated-request interoperability or establish that
keeping links alive improves latency, link count, CPU, or memory.

## Reticulum 0.9 receipt correlation

The clean transport receipt tests exercise the project boundary immediately
after upstream proof validation. They require exact packet-hash to logical
LXMF-message mapping, one status/evidence emission for a matching receipt,
diagnostic-only duplicate handling, and diagnostic-only stale handling after a
failed or retired attempt. The stale-attempt regression keeps a newer retry
pending and proves that the old receipt cannot complete or otherwise mutate it.
Resource terminal tests separately require completion/failure/cancellation to
release the resource-hash correlation exactly once.

```bash
cargo test --locked --no-default-features --features desktop-product \
  clean_reticulum_receipt_handler
cargo test --locked --no-default-features --features desktop-product \
  clean_reticulum_stale_receipt
cargo test --locked --no-default-features --features desktop-product \
  clean_lxmf_resource
```

These are deterministic application-boundary tests. They do not synthesize a
cryptographic proof packet or claim pinned-Python receipt equality, live retry
timing, restart delivery, or authoritative peer-level LXMF delivery.

## LXMF peer stamp-cost policy

The Phase 4 policy tests compare every admitted direct cost boundary used by
OMEN with the published `lxmf-wire` 0.9 parser. They separately require typed
decisions for missing/legacy data, an explicit nil cost, a required cost, a
valid reply-ticket override, and malformed or out-of-range costs. Existing
tests cover directory retention, 0.9 SDK/RPC field mapping, low-cost direct and
propagation stamp generation/validation, and ticket-stamp precedence.

```bash
cargo test --locked --no-default-features --features desktop-product \
  delivery_stamp_policy
cargo test --locked --no-default-features --features desktop-product \
  delivery_stamp_cost_parser
cargo test --locked --no-default-features --features desktop-product \
  direct_stamp_generation
cargo test --locked --no-default-features --features desktop-product \
  ticket_stamp
```

The integrated direct-stamp policy additionally tests ticket precedence, the
cost-8 safety ceiling, a 65,536-attempt work limit, a two-job blocking gate,
permit release, and cooperative cancellation:

```bash
cargo test --locked --no-default-features --features desktop-product \
  clean_direct_stamp
```

Live direct-stamp admission is a separate ignored interoperability test:

```bash
cargo test --locked --no-default-features --features desktop-product \
  pinned_python_lxmf_live_direct_stamp_accepts_stamped_and_rejects_unstamped -- \
  --ignored --nocapture --test-threads=1
```

The Python router advertises cost 1 and enforces stamps. It must invoke exactly
one delivery callback for the production-signed stamped message and none for a
second valid but unstamped control. The same case is selected by the
`current_python_lxmf` filter. Missing/legacy policy remains a compatibility
send; malformed policy and required costs above 8 fail locally. This does not
measure high-cost proof work or retry after a stale-policy rejection.

First-send policy discovery has an additional application-boundary case:

```bash
cargo test --locked --no-default-features --features desktop-product \
  clean_direct_policy
cargo test --locked --no-default-features --features desktop-product \
  pinned_python_lxmf_first_direct_send_discovers_stamp_policy_before_encoding -- \
  --ignored --nocapture --test-threads=1
```

The deterministic case distinguishes authenticated empty policy from absence,
ignores unrelated announce events, fails closed when matching app data was not
admitted, and proves timeout/shutdown ownership. The live case starts the real
integrated runtime, removes cached policy, and requires policy discovery before
wire construction. Python must accept that stamped first send and reject the
unstamped control. This does not claim an automatic retry after remote
rejection; no authoritative rejection event exists on the integrated path.

Ticket wire/lifecycle compatibility is exercised separately in both Python
lanes:

```bash
cargo test --locked --no-default-features --features desktop-product \
  pinned_python_lxmf_ticket_issue_use_expiry_and_reuse_match_rust -- \
  --ignored --nocapture --test-threads=1
```

The matrix passes reusable ticket material only through files under a unique
temporary root and reports booleans, versions, and byte counts—not ticket bytes.
It requires Rust ticket-stamp acceptance/wrong-ticket rejection plus Python
issue, reuse, renewal, delivery-throttle, remembered-use, expiry, and cleanup
behavior. High-cost latency and user-policy UX remain future work.

The live ticket exchange is a distinct ignored interoperability test:

```bash
cargo test --locked --no-default-features --features desktop-product \
  pinned_python_lxmf_live_ticket_roundtrip_uses_rust_issued_ticket -- \
  --ignored --nocapture --test-threads=1
```

Rust sends a production-signed direct message containing a generated ticket.
Python must authenticate and remember it, use it for a real direct reply, and
receive the reply proof. Rust independently verifies the reply signature,
message ID, and exact ticket-derived stamp before production decoding. The same
case runs in the current-package filter.

## Native LXMF attachment admission

Native LXMF attachment tests use generated isolated roots. Outbound coverage
accepts exactly two 8 MiB files at the 16 MiB aggregate ceiling, rejects a
sparse next-byte file before reading it, rejects the 65th path before file
access, preserves missing-path compatibility, and on Unix refuses a symlink
without reading its referent. Inbound coverage rejects the 65th entry and the
aggregate next byte, stores accepted files with `0600` mode below a `0700`
message directory on Unix, proves long path components are bounded and
collision-resistant, and proves replay uses the same deterministic path.
A linked destination is rejected without changing its referent. An injected
failure after staging synchronization but before replacement preserves the
previous file and removes the staging file.

The runtime blocking-boundary regressions submit eight deterministic jobs and
require peak blocking concurrency to equal the two-job policy. A second test
aborts the async waiter after its closure starts, proves the permit remains held
while blocking work finishes, and requires its release within one second. This
models Tokio cancellation honestly: dispatched filesystem/decode work is not
forcefully preempted, but cancellation cannot leak capacity or admit work above
the bound.

```bash
cargo test --locked --lib --no-default-features --features desktop-product \
  attachment
cargo test --locked --lib --no-default-features --features desktop-product \
  native_lxmf_blocking_gate
cargo test --locked --lib --no-default-features --features desktop-product \
  cancelled_lxmf_waiter
```

The tests do not use the maintainer's real attachment, identity, message, or
Reticulum roots.

The quick LXMF smoke runs the signed Python-compatible attachment encode,
bounded private-file store, and idempotent replay path:

```bash
bash scripts/smoke/05_lxmf_service_loopback.sh
```

## Pinned Python IFAC wire fixture

The project-local IFAC TCP client has a deterministic public fixture generated
through the Python Reticulum transmit path pinned for the 0.9.5 parity lane.
The release-blocking deterministic lane fetches that exact revision into a
temporary isolated directory, verifies its Git identity and clean state, and
compares Python and Rust identity, destination-name, destination-address, and
IFAC bytes:

```bash
bash scripts/run-pinned-python-reticulum.sh
```

An already checked-out source tree can be supplied for an offline rerun:

```bash
bash scripts/run-pinned-python-reticulum.sh \
  --rns-source /path/to/Reticulum-at-15320e4d2cfabb143c1db20ca887e275fd521585 \
  --lxmf-source /path/to/LXMF-at-727830cefda83d9c6e3982b48675425f3f988f9c
```

The Python oracle refuses a different Reticulum or LXMF commit or any
tracked/untracked source change. It imports directly from the verified trees
rather than installing or floating a Python package. Its identity, network
name, and passphrase are fixed
public test fixtures and must never be replaced with live credentials. The
temporary checkout is removed on exit. The deterministic portion does not
start a Reticulum runtime and proves identity/destination derivation and
wire-byte equality. The bounded live portions below cover the supported client
direction; broader role and platform coverage remains separate.

The same runner also executes an ignored-by-default real-socket test against a
bounded Python peer imported from the verified source tree. It proves a Rust
IFAC TCP client packet is authenticated by Python, Python replies are
authenticated by Rust, an HDLC frame split across writes is reconstructed, two
frames coalesced into one write are delivered separately, a closed socket is
reconnected after the production delay, and mismatched credentials are
rejected in both directions. The peer binds only an ephemeral IPv4 loopback
port, caps each frame at 4 KiB, uses eight-second I/O deadlines, accepts at most
two connections, and never initializes Python Reticulum storage or interfaces.

Role reversal is not claimed: the retained OMEN compatibility implementation
is a TCP client. omenchatd deliberately rejects an IFAC-configured stock TCP
server because upstream 0.9.5 does not apply the Python IFAC transform there.
Adding a new server implementation is outside this compatibility unit. Native
IPv6, multiple simultaneous clients, and long-running reconnect/resource
measurements remain pending.

The runner also starts a complete Python Reticulum instance with one inbound
`omeninterop.link` destination and a real IFAC `TCPServerInterface`. Its config,
storage, fixed identity, and listener are confined to a unique temporary root
and ephemeral IPv4 loopback port. The production Rust `IfacTcpClient` and
registry 0.9.5 `Transport` must send a path request, receive and validate the
Python announce, recall the exact identity, establish a link, send encrypted
link data, receive the Python echo, and validate Python's packet proof against
the exact finalized encrypted packet hash. The receipt handler uses a bounded
four-item metadata channel and rejects a duplicate callback. Startup and
exchange reads, Python's wait, Rust transport waits, interface shutdown, child
shutdown, and the outer script are all bounded. The test remains ignored
outside the explicit lane:

```bash
OMEN_PINNED_RNS_SOURCE=/path/to/pinned/Reticulum \
  cargo test --locked --manifest-path src/server/Cargo.toml \
  -p omen-ifac-tcp --test pinned_python_reticulum -- \
  --ignored --nocapture --test-threads=1
```

This proves the supported Python-server/Rust-client path, announce, identity,
link, small link-data, and cryptographically validated transport-proof
sequence. Rust first sends an old attempt; Python retains it without returning
a proof, and Rust requires a bounded 250 ms no-proof interval before sending a
replacement attempt on the same active link. Python then sends a modified-hash
invalid-signature proof, the correctly signed old packet proof, and the
replacement proof. Rust must reject the forgery and expose the old/current
hashes in order. The runner separately executes the production clean LXMF
correlation regression requiring that a removed old attempt emits only a
diagnostic and cannot advance the newer retry. It also uses an isolated message
root to persist old and current correlations, lets the first runtime recover
both, durably deletes the obsolete thread, then reopens the store and a fresh
runtime. Only the surviving hash may recover and emit delivery state; a receipt
for the deleted hash must remain diagnostic-only. A second regression repeats
that contract across an operating-system process boundary: the parent drops its
runtime/store ownership and launches the current unit-test executable with the
isolated root supplied only through a test environment variable. The child
reopens the persisted bytes, rebuilds correlation ownership, and runs both
receipts through the production handler. Child execution is bounded to ten
seconds, output is retained on failure, and the parent removes the temporary
root. The lane also verifies the scheduled durable timeout accepts the active
clean-transport pair (`submitted_to_clean_reticulum` plus
`waiting_for_transport_receipt`). It persists that transition, adds a replacement
attempt with the same logical operation identity and a different packet hash,
and proves a late old proof remains scoped to the old message before restart.
After restart, only the replacement correlation is recovered. A separate
post-commit crash regression performs the timeout and replacement in a child
process, publishes a synced readiness marker only after both store operations
return, and parks. The parent terminates and reaps that child, then requires the
same old/current recovery behavior from the committed bytes. This covers abrupt
process loss after the atomic store operations, not interruption during a file
write or replacement. Message publication has a second boundary harness. It
injects returned errors after staging creation, write, sync, destination commit,
and directory sync; every case must leave parseable complete old or new JSON and
remove its stage. It also kills a child after stage sync and after destination
commit. The former preserves the old thread and initially leaves one non-JSON
stage plus its locked lease; the latter preserves the new thread and initially
leaves only the lease. Process death releases the OS lock. On reopen,
`MessageStore` acquires each abandoned lease nonblockingly, removes only the
associated abandoned artifacts, syncs the directory, and loads the correct
ownership. A cross-process live-lock regression proves a second store cannot
delete a stage whose child writer still holds the lease. A separate concurrent
publisher regression proves the process-local active-lease registry protects a
writer thread even where same-process operating-system lock semantics differ.
An unleased legacy stage is retained rather than guessed dead, and
publication-artifact discovery rejects more than 4,096 entries. The lane
does not exercise Reticulum Resources, request/response, automatic retry
dispatch, Python-as-client role reversal, IPv6, lock behavior on network
filesystems, physical power loss, authoritative LXMF delivery, NomadNet, or
OMENchat semantics.

The release-blocking lane also imports Python Reticulum commit
`15320e4d2cfabb143c1db20ca887e275fd521585` (module version 1.2.2) and Python
LXMF commit `727830cefda83d9c6e3982b48675425f3f988f9c` (module version 0.9.6)
from separate verified source roots. It runs the same isolated propagation-node
topology as the current drift lane: Python learns the Rust receiver announce,
queues a signed recipient-encrypted transient in its real router store, and
serves the production Rust `/get` list/get/ack sync. Rust must authenticate the
announced sender and publish the exact message; Python must retain the store
entry until acknowledgement and then remove it. This proves one pinned
software topology. The same pinned lane separately generates a cost-2
propagation stamp with Rust under a 4,096-attempt ceiling and invokes the exact
Python `LXStamper.validate_pn_stamps` primitive. Python must calculate the same
achieved value, preserve the transient/stamp, accept at that value, and reject
the same bytes at value+1. The deterministic value+1 case avoids probabilistic
corruption tests. It proves stamp-algorithm and validator-boundary
compatibility. The pinned lane additionally sends two messages through the
production Rust clean propagated-send path and Python's real network-facing
propagation handler. Python accepts and locally delivers the signed first
message at its minimum advertised cost 13. The fixture then raises its live
admission floor to 255 without issuing a new announce; Python must reject the
second stale-policy envelope, leave its accepted-client counter at one, and
avoid a second delivery callback. The pinned lane also runs the isolated ticket
issue/use/expiry/reuse matrix and live Rust-issue/Python-reply exchange described
above. Resources, restart during sync,
multiple recipients, policy-refresh recovery, and peer delivery beyond node
acknowledgement remain outside this case.

## Current Python drift lane

The current-Python lane is deliberately separate from the immutable pinned
reference above. As of 2026-07-21 it installs exactly RNS 1.4.0, LXMF 1.1.0,
and NomadNet 1.2.7 into a disposable virtual environment, records the resolved
Python and pip versions, and verifies that all three packages import. It then
reuses the bounded compatibility-vector, IFAC TCP, and link/proof tests against
the installed RNS package and runs reciprocal Rust/Python LXMF direct-delivery
cases:

```bash
bash scripts/run-current-python-drift.sh \
  --report target/current-python-drift-report.json
```

The scheduled workflow pins Python 3.12.11 and uploads the JSON report for 14
days. The job is informational (`continue-on-error`) and cannot replace or
weaken the pinned-reference release gate. Its top-level Python stack versions
are exact, while transitive Python dependencies are resolved afresh rather
than treated as a reproducible release input; their resolved names and versions
are captured in the report. A failure therefore reports drift for investigation
instead of silently moving the release baseline.

This lane proves current-RNS identity/destination/IFAC vectors, the
supported Python-server/Rust-client IFAC direction, reconnect and wrong-key
rejection, announce/path/link data, forged-proof rejection, and stale/current
proof ordering. For LXMF it also requires Rust to announce the exact local
`lxmf.delivery` identity, waits for Python to learn that identity, sends a
production-encoded signed direct message over an activated link, and requires
Python LXMF to report the exact source, destination, title, content, direct
method, and a valid signature. Rust separately requires the Python packet proof
to match the sent packet hash. In the reverse direction, Python announces its
source, learns the Rust delivery destination from an authenticated announce,
sends through `LXMRouter`, and requires the Rust packet proof. Rust requires the
production verifier to preserve exact source/destination/title/content/message
ID and validate the wire signature against the identity learned from the
authenticated Python `lxmf.delivery` announce. Deterministic admission tests
also reject an unknown source, a forged signature, and a cached identity that
does not derive the claimed source destination; they require authentication
before attachment storage and suppress a verified replay by message ID. These
are two small link packets. The same informational lane now starts an isolated
Python LXMF 1.0.1 propagation node, queues one Python-signed recipient-encrypted
transient through its router, and drives the production Rust `/get` list/get/ack
sync. Rust must authenticate the announced Python sender before publication,
preserve the exact title/content/source and propagated method, and the Python
node must observe removal only after the Rust acknowledgement. Live Python
LXMF also accepts a bounded Rust propagation stamp at its exact achieved value
and rejects the identical bytes at value+1 through its upstream validator. Its
real network handler accepts and delivers one production Rust cost-13 envelope,
then rejects an under-cost second envelope after a simulated stale-policy
change without incrementing accepted-message or delivery counts. The drift lane
also applies its installed Python stack to the ticket matrix and records
`ticket_issue_use_expiry_reuse` and `live_ticket_roundtrip` in the JSON report.
Live Python Resources/attachments, node restart, automatic propagation-policy
refresh, NomadNet behavior, and mixed-version behavior remain unclaimed.
The strict production admission boundary also covers decrypted clean-transport
propagation payloads: deterministic tests require a matching authenticated
sender identity, reject unknown/mismatched/forged senders before attachment
storage, and prove a rejected local payload is left unacknowledged for retry.
An unknown authenticated source exposes its exact 16-byte destination to the
sync coordinator, which requests each missing sender path once and caps a sync
at 32 such requests. Duplicate transient bytes in one response and already
delivered transients are suppressed before decryption/publication; only the
first successfully verified copy can be acknowledged.
The recovery and replay bounds are deterministic Rust evidence; the single
current-Python enqueue/sync/ack case is live interoperability evidence for that
narrow topology. All Python configuration and Reticulum/LXMF storage used by
live tests remain under unique temporary roots and are removed on exit.

Run the repeated-crash publication recovery measurement with:

```sh
bash scripts/measure-message-publication-recovery.sh
```

The harness uses the current unit-test executable and a unique temporary
message root. Sixteen child processes each sync a complete replacement stage,
publish a synced readiness marker, and park before rename. The parent kills and
reaps every child, removes only the markers, then performs one recovery pass.
Its machine-readable summary reports crash count, artifact count, retained
artifact bytes, recovery microseconds, post-recovery artifacts/bytes, and total
elapsed milliseconds. The test requires the original thread to remain
byte-exact throughout, all 32 stage/lease artifacts to be removed in one pass,
and the reopened store to parse the old ownership. It does not set a
hardware-independent latency threshold or write outside the isolated root.
The same script then fills the exact 4,096-artifact ceiling with 2,047
abandoned pairs and one child-held live pair. Its first recovery pass must
remove every abandoned pair while retaining exactly the live stage and lease.
After the parent terminates and reaps that child, a second pass must remove the
last pair. The summary reports initial/retained/final items and bytes plus both
recovery latencies. The existing overload regression separately verifies that
artifact 4,097 is rejected before cleanup work becomes unbounded.

## Bounded plugin discovery

Plugin discovery uses only isolated temporary roots in tests. The regression
suite crosses the 256-installed-candidate ceiling, requires an explicit
overload warning, rejects a sparse manifest above 64 KiB before reading it,
rejects a registry above 1 MiB, and on Unix proves discovery does not follow a
manifest symlink. Existing compatibility, install, enable, and removal tests
remain in the same suite; no test executes a third-party entrypoint.

Registry persistence coverage replaces an existing valid registry, leaves the
former predictable temporary filename untouched, refuses directory and symlink
targets, verifies owner-only mode on Unix, and requires no unique staging file
to remain. An injected replacement fault runs below the production helper and
requires the previous bytes to remain exact while the create-new staging file
is removed. These isolated filesystem tests contain no identity or runtime
state.

Folder-install regressions additionally cross the 1,024-entry and 16-level
limits, reject a sparse file above 16 MiB, and on Unix reject a symlink inside
the source tree or at the final destination (including a broken destination
link). Each failure requires both the final plugin path and hidden staging path
to be absent, except that a pre-existing destination link must remain exact. A
separate allocation-free accounting test accepts
exactly 64 MiB across production-sized files, rejects the next byte, and proves
the rejected byte does not change accounting. The normal install regression
requires the synchronized staging tree to be atomically published and registry
metadata to remain compatible. Startup recovery removes safely encoded reserved
install-stage directories and warns, but preserves a reserved non-directory
path. The published-tree crash boundary is covered by discovery: a complete
tree without metadata is persisted disabled and untrusted rather than deleted
or trusted.

Removal coverage refuses both built-ins and a symlinked installed target,
requires normal removal to leave neither a visible tree nor hidden quarantine,
and injects registry-save failure after quarantine. The failure must restore the
tree and preserve registry ownership. Startup discovery crash-boundary tests
then create the two possible quarantine states: registry ownership restores a
pre-commit tree, while absent ownership completes post-commit deletion. Both
paths emit explicit recovery warnings and remain bounded by the discovery scan
limit.

```bash
cargo test --locked --no-default-features --features desktop-product \
  --test plugin_registry
cargo test --locked --no-default-features --features desktop-product \
  plugin_install_total_byte_budget --lib
```

## Runtime lifecycle and capability diagnostics

The diagnostics snapshot regression uses an explicitly isolated temporary root
and the mock runtime. It verifies that lifecycle and typed capability records
are projected into the snapshot, that supported and unsupported capabilities
remain distinct, and that runtime failure technical detail is redacted from
the exported JSON. The application export regression additionally verifies the
compact lifecycle/capability UI summaries and the serialized field names.

```bash
cargo test --locked --no-default-features --features desktop-product \
  diagnostics --lib
```

This is deterministic projection/redaction evidence. It does not prove live
Reticulum interface state, shared-instance ownership, next-hop selection, or a
remote SDK/RPC capability negotiation.

The path/interface projection adds a separate fail-closed regression. A
snapshot with placeholder zero/false values and unavailable evidence flags must
render aggregate path table, request-failure metrics, and shared-instance status
as unavailable. A fixture that explicitly marks those metrics available must
continue to render its exact counts.

```bash
cargo test --locked --no-default-features --features desktop-product \
  network_doctor --lib
```

This test does not turn configured managed/external mode into proof of a live
shared Reticulum instance.

## Local LXMF announce rate limiting

The isolated mock-runtime regression proves that a second request is coalesced
while the first local announce is pending and that a completed attempt cannot
be repeated inside the 30-second monotonic cooldown. The native pre-send
regression proves a rate-limited required announce does not leave a deferred
send action that could resume after an unrelated future announce.

```bash
cargo test --locked --no-default-features --features desktop-product \
  local_lxmf_announce --lib
cargo test --locked --no-default-features --features desktop-product \
  rate_limited_pre_send_announce --lib
```

The tests use isolated application roots and do not transmit a live announce.
They do not prove targeted announce support or network propagation behavior.

## External/shared runtime ownership gate

Two isolated regressions enforce the same fail-closed invariant at independent
boundaries: application startup must not create an identity or queue startup
when External is configured, and direct use of the native adapter must not
construct an integrated transport. A separate projection regression verifies
configured external state remains distinct from uncollected/negotiated shared
capability.

```bash
cargo test --locked --no-default-features --features desktop-product \
  external_mode --lib
cargo test --locked --no-default-features --features desktop-product \
  runtime_ownership_line --lib
```

These tests do not prove an external backend works. They prove the deferred mode
cannot silently start a conflicting integrated instance.

## OMENchat reconnect link ownership

The clean Reticulum regressions prove explicit opens are cancellation-aware and
that reconnect retirement closes only the prior link for the matching
destination. Desktop regressions prove a newer reconnect cancels its prior task,
while a stale completion cannot remove the current generation's owner or count
as a failed current attempt.

```bash
cargo test --locked --no-default-features --features desktop-product \
  clean_omenchat_link_coordinator --lib
cargo test --locked --no-default-features --features desktop-product \
  clean_omenchat_reconnect_retires --lib
cargo test --locked --no-default-features --features desktop-product \
  clean_omenchat_cancelled_pending --lib
cargo test --locked --no-default-features --features desktop-product \
  omenchat_stale_reconnect_result --lib
cargo test --locked --no-default-features --features desktop-product \
  newer_omenchat_reconnect_cancels --lib
```

The tests use in-memory Reticulum transports and isolated application roots.
They establish local ownership and cleanup semantics, not live server restart,
mixed-version, Python, radio/interface, latency, CPU, or link-count evidence.

## omenchatd same-link mutation replay

The standalone server regressions send the same room-message, part, and kick
frames twice on one authenticated link. They require retained origin responses,
one durable SQLite event, one applicable peer/user-list fan-out, one moderation
disconnect, one rate-limit charge, and a replay hit. Reusing the message
sequence with different content must produce an error without another event. A
rate-limited kick must leave its target connected. Classification coverage
requires all mutating commands to be guarded and the read-only `rooms` command
to remain uncached. A separate cache regression crosses the per-link item
limit, rejects an oversized entry, and proves close releases all item/byte
accounting.

```bash
(
  cd src/server
  cargo test --locked --no-default-features --features server-headless replay
)
```

These tests use isolated SQLite paths and an in-memory captured transport. They
exercise the legacy same-Link cache and do not by themselves prove cross-Link,
server-restart, mixed-version, Python, or live Reticulum retry idempotency. The
separate negotiated durable-mutation tests below cover deterministic cross-Link
and restart behavior.

## Shared Operations/Transfers model

The frontend-neutral operation-model fixtures distinguish
queue/transport/receipt state from authoritative peer delivery, reject delivery
without authoritative evidence, require bounded Resource totals for progress,
coalesce repeated updates by stable operation ID, evict only terminal history,
reject saturation consisting only of unresolved work, incrementally expire
completed records, and reject excessive text, evidence, or action retention.
They also prove atomic per-domain snapshot replacement, rejection of mixed or
duplicate snapshot identities, preservation of other domains under replacement
or saturation, and exact byte release on removal.

```bash
cargo test --locked --no-default-features --features desktop-product \
  operations::tests --lib
```

`App` owns the bounded history and Network Doctor provides a compact read-only
desktop consumer. A dedicated desktop workspace, the TUI surface, and general
runtime event adapters do not exist yet. The owner creates no worker, timer,
subscription, persistence file, network peer, or user-state access.

The OMENchat recovery adapter adds deterministic fixtures for prepared,
uncertain, past-expiry, retry-blocked, terminal, and redaction cases:

```bash
cargo test --locked --no-default-features --features desktop-product \
  operations::omenchat::tests --lib

cargo test --locked --no-default-features --features desktop-product \
  desktop::views::omenchat::accessibility_tests --lib
```

These tests do not transmit a mutation or access the maintainer's identity,
Reticulum, OMENchat server, or persistent application root. They verify only
the read-only projection and the existing recovery card. Server/client restart
and replay behavior remains covered by the isolated durable-mutation tests.

The isolated desktop restart-recovery fixture additionally proves that all
current-identity recovered intents atomically populate the shared owner,
other-identity intents do not, past-expiry records remain nonterminal, no
transmission action is stored in the conservative snapshot, and explicit
resolution removes the exact records:

```bash
cargo test --locked --no-default-features --features desktop-product \
  desktop::omenchat_mutations::tests::restart_recovery_is_identity_scoped_visible_and_never_transmits \
  --lib
```

The shared presentation-model fixtures verify deterministic attention-first
sorting, row limits and omission counts, active/attention/completed/domain
filter semantics, bounded ASCII-insensitive search, opaque-ID exclusion,
control-character sanitization, UTF-8-safe target/evidence truncation, exact
authoritative progress, valid-action preservation, and stable shared labels:

```bash
cargo test --locked --no-default-features --features desktop-product \
  operations::presentation::tests --lib
```

This is a pure read-only projection over in-memory bounded history. It does not
render Iced or Ratatui widgets and does not touch identities, Reticulum,
OMENchat peers, storage, or network state.

The first desktop consumer is the passive Network Doctor card. Its model tests
cover explicit empty state, the fixed eight-row limit and omitted count,
attention-first ordering, shared transport/authority terminology, opaque-ID
omission, and authoritative-only byte progress:

```bash
cargo test --locked --no-default-features --features desktop-product \
  desktop::views::operations::tests --lib
```

These fixtures are entirely in memory and exercise no Iced action, worker,
timer, subscription, persistent root, identity, Reticulum peer, or OMENchat
server.

The TUI Network Doctor model and route fixtures cover the same empty state,
fixed row limit, omissions, attention ordering, terminology, opaque-ID
omission, and authoritative-progress behavior. The route smoke uses a generated
temporary application root and proves Network Doctor renders the Operations
view rather than its previous placeholder:

```bash
cargo test --locked --no-default-features --features tui \
  ui::operations::tests --lib
```

The temporary root is removed after rendering. No runtime backend, identity,
Reticulum peer, OMENchat server, timer, worker, or input action is started.
Interactive filter/action controls remain a later gate.

The typed path-observation adapter fixtures prove destination normalization,
stable opaque correlation, control/size rejection, known versus unknown
semantics, hop evidence, coalescing, route-loss reopening, stale-observation
rejection, unrelated-event omission, no peer-delivery claim, and saturation
that preserves unresolved work:

```bash
cargo test --locked --no-default-features --features tui \
  operations::path::tests --lib

cargo test --locked --no-default-features --features desktop-product \
  operations::path::tests --lib

cargo test --locked --no-default-features --features desktop-product \
  runtime_handler_projects_path_observations --lib
```

These are deterministic in-memory or isolated-root tests. They do not request a
path, warm a destination, start Reticulum, contact a peer, or touch the
maintainer's identity/configuration. The adapter intentionally has no fixture
for typed request failure because `PathUpdated` does not expose request
identity, timeout, failure, or reason; live path request behavior remains a
separate smoke/interoperability gate.

The OMENchat connection projection fixtures prove all typed connection-state
mappings, stable session correlation, normalized and bounded public targets,
transition coalescing, stale-state rejection, session-close removal, no
transport/receipt/delivery claim, and saturation behavior:

```bash
cargo test --locked --no-default-features --features desktop-product \
  operations::connection::tests --lib

cargo test --locked --no-default-features --features desktop-product \
  omenchat_connection_state_is_bounded_by_sessions_and_join_is_event_driven --lib

cargo test --locked --no-default-features --features desktop-product \
  close_omenchat_session_clears_live_transport_and_retry_state --lib
```

The tests use isolated application roots and the existing typed desktop state
reducer. They do not start Reticulum, establish a Link, authenticate to an
OMENchat server, reconnect over the network, or inspect private identity
material. Live open/close/reconnect behavior remains covered by the documented
OMENchat smoke and interoperability gates.

The typed LXMF SDK Operations fixtures prove every v0.9.6 delivery-state
mapping, backend-dependent terminal-sent behavior, opaque message correlation,
peer-target retention, bounded reason handling, exact attempts/sequence,
transition and evidence coalescing, duplicate/stale rejection, terminal
regression protection, inconsistent terminal-flag rejection, no private event
metadata retention, exact native-evidence correlation, RNS-proof and
propagation-acceptance boundaries, uncertain peer activity/no-receipt
semantics, raw-detail omission, timestamp fallback, terminal precedence, and
legacy status compatibility, contradictory-flag rejection, stronger-evidence
precedence, and saturation behavior:

```bash
cargo test --locked --no-default-features --features desktop-product \
  operations::lxmf::tests --lib

cargo test --locked --no-default-features --features tui \
  operations::lxmf::tests --lib

cargo test --locked --no-default-features --features desktop-product \
  runtime_handler_projects_typed_sdk_delivery --lib

cargo test --locked --no-default-features --features desktop-product \
  runtime_handler_correlates_native_lxmf_evidence --lib

cargo test --locked --no-default-features --features desktop-product \
  runtime_handler_projects_legacy_lxmf_status --lib
```

These tests construct typed runtime events in memory and use an isolated
application root. They do not send LXMF, start Reticulum, contact a peer,
observe a live receipt, synchronize propagation, or establish Python
interoperability. Live peer-delivery proof remains a separate smoke and
interoperability gate.

The runtime event-stream Operations fixtures prove independent source
correlation, gap/recovery state, cursor ordering, duplicate rejection,
successful completion, incomplete recovery, reopening after a later gap,
bounded evidence, and omission of raw upstream cursors and recovery error
text:

```bash
cargo test --locked --no-default-features --features desktop-product \
  operations::event_stream::tests --lib

cargo test --locked --no-default-features --features tui \
  operations::event_stream::tests --lib

cargo test --locked --no-default-features --features desktop-product \
  runtime_handler_projects_event_gap --lib
```

These tests use typed in-memory events and an isolated application root. They
do not force broadcast lag, connect to an SDK/RPC daemon, request a snapshot,
or change the existing event worker. The worker's bounded lag and recovery
tests remain the behavioral recovery gate.

The propagation-sync Operations fixtures prove app-generation correlation,
queue/start/progress/intermediate/final state boundaries, blocked and failed
outcomes, task-result finalization, stable destination normalization,
unrelated-event rejection, exclusion of ambiguous `Complete/Progress`, raw
detail/count omission, stale and duplicate rejection, repeated-progress
coalescing, terminal precedence, bounded evidence, late-target omission, and
unresolved-history saturation:

```bash
cargo test --locked --no-default-features --features desktop-product \
  operations::propagation::tests --lib

cargo test --locked --no-default-features --features tui \
  operations::propagation::tests --lib

cargo test --locked --no-default-features --features desktop-product \
  propagation_sync_operations_require_app_correlation --lib
```

These are deterministic in-memory and isolated-root tests. They do not select a
live propagation node, establish a Link, synchronize LXMF, contact a peer, or
prove peer delivery. Existing propagation smoke and Python interoperability
remain the live gates.

The typed Resource adapter fixtures prove stable opaque correlation, transfer
identifier and browser-operation redaction, offer/progress coalescing, retained
authoritative totals, regression and malformed-total rejection, completion
without a peer-delivery claim, distinct failure/cancellation, terminal
precedence over late progress, and saturation that preserves unresolved work:

```bash
cargo test --locked --no-default-features --features tui \
  operations::resource::tests --lib

cargo test --locked --no-default-features --features desktop-product \
  operations::resource::tests --lib
```

The existing application Resource handler tests additionally verify that typed
events populate both Network Doctor and the shared owner without changing
status or browser-correlation behavior:

```bash
cargo test --locked --no-default-features --features desktop-product \
  network_doctor_runtime_handler_records --lib
```

All fixtures are generated in memory or under the existing isolated test
roots. They do not start Reticulum, transfer a Resource, contact an OMENchat
server, or touch the maintainer's application data. Live Resource
interoperability remains a separate smoke/release gate.

## OMENchat negotiated durable room and user mutations

Negotiated `/me` sends must persist a `RoomAction` intent before transport,
transition it to uncertain, emit the canonical durable envelope, and correlate
the matching `MessageAck`. Negotiated `/notice` additionally requires
`durable-room-notice-ack-v1`, follows the same intent boundary, and uses notice
kind `3` in the acknowledgement. Older, ordinary, and downgraded protocol-v1
notices retain their `RoomEvent` response and legacy send path. Recovery exposes
uncertain actions and notices after client restart but never automatically
transmits them. Negotiated `/part` persists an empty-body PartRoom intent and
must leave local membership unchanged until an exact correlated
`CommandResult`; restart recovery exposes the uncertain leave without sending
it. Negotiated `/topic` persists the normalized command and retains the prior
local room metadata until an exact correlated result. Negotiated `/create`
persists a roomless command, adds no optimistic room, and accepts only the
server-normalized requested room in an otherwise exact result. Server tests require
exact replay after Link replacement and server restart to retain the original
result without another event, rate charge, metadata revision, or fan-out;
mutation-ID reuse with different content must conflict.
Negotiated `/role` and `/unban` persist canonical commands and require the
returned catalog-known numeric ID or display name plus role/ban state to match. Hex
identity-prefix-only targets retain the legacy path because the result has no
identity hash. Their replacement-Link and restart replays must not repeat user
mutation, audit event, rate admission, or fan-out.
Negotiated `/kick`, `/ban`, `/mute`, and `/unmute` use the same persistent
intent and exact user/result-state boundary. Client regressions reject a wrong
user or wrong ban/mute state before acknowledging. Server regressions execute
all four once, replay without another mutation/event/rate admission, and prove
that a lost kick response closes only the originally selected Link rather than
a replacement Link.
The recovered-intent panel classifies those operations without displaying
their request bodies, targets, mutation identifiers, or hashes. Pure label
coverage checks redaction, relative expiry, and bounded destination shortening.
The existing restart fixture proves that unavailable retry never creates a
confirmation or transport frame, while stop-tracking and expiry remain
explicit confirmed storage-only actions.

```bash
cargo test --locked --no-default-features --features desktop-product \
  negotiated_room_text_persists_before_transport_and_acknowledges --lib
cargo test --locked --no-default-features --features desktop-product \
  durable_room_action_sends_canonical_envelope_and_correlates_acknowledgement --lib
cargo test --locked --no-default-features --features desktop-product \
  durable_room_notice_sends_canonical_envelope_and_correlates_acknowledgement --lib
cargo test --locked --no-default-features --features desktop-product \
  durable_part_waits_for_matching_result_before_leaving_and_acknowledging --lib
cargo test --locked --no-default-features --features desktop-product \
  durable_topic_waits_for_matching_result_before_updating_and_acknowledging --lib
cargo test --locked --no-default-features --features desktop-product \
  durable_create_waits_for_matching_normalized_room_before_acknowledging --lib
cargo test --locked --no-default-features --features desktop-product \
  durable_role_and_unban_require_matching_user_and_result_state --lib
cargo test --locked --no-default-features --features desktop-product \
  durable_active_peer_moderation_requires_exact_user_and_status_result --lib
cargo test --locked --no-default-features --features desktop-product \
  restart_recovery_is_identity_scoped_visible_and_never_transmits --lib
cargo test --locked --no-default-features --features desktop-product \
  recovered_mutation_labels_are_redacted_and_semantic --lib
(
  cd src/server
  cargo test --locked --no-default-features --features server-headless \
    durable_create
  cargo test --locked --no-default-features --features server-headless \
    durable_role
  cargo test --locked --no-default-features --features server-headless \
    durable_unban
  cargo test --locked --no-default-features --features server-headless \
    durable_active_peer_moderation_executes_once_for_each_action
  cargo test --locked --no-default-features --features server-headless \
    durable_kick_commit_survives_lost_response_without_disconnecting_replacement
)
```

All fixtures use isolated temporary SQLite roots and captured in-memory
transports. They do not establish live Reticulum, mixed-version, Python,
packaged-platform, or physical-interface interoperability.

## omenchatd duplicate peer-link retirement

The standalone server replacement regressions open two links with the same
identified peer. They require the older physical Reticulum link to receive one
close request, the replacement to remain the sole active link, the closure to
be counted and summarized, and all old per-link room, response-context,
replay-cache, timing, and traffic ownership to be released.

```bash
(
  cd src/server
  cargo test --locked --no-default-features --features server-headless \
    duplicate_peer
  cargo test --locked --no-default-features --features server-headless \
    live_server_reports_active_identity_link_counts_after_duplicate_replacement
)
```

These are deterministic lifecycle tests using the captured transport and an
in-memory SQLite store. They do not establish a server-federation feature (none
is currently defined), live close delivery, reconnect-storm fairness,
cross-link idempotency, mixed-version interoperability, or task/RSS stability.

## omenchatd pending Resource retention

The standalone server Resource regressions require fixed item, total-byte, and
per-entry budgets; exact-boundary acceptance; explicit overflow rejection;
replacement accounting; and capacity restoration after removal. Transport
tests require successful response batches and injected send failures to release
all generated payloads. A separate two-peer test requires a resource-backed
user list to reach both joined links with identical bytes before its retained
copy is released.

```bash
(
  cd src/server
  cargo test --locked --no-default-features --features server-headless \
    pending_resource
  cargo test --locked --no-default-features --features server-headless \
    link_bridge_
  cargo test --locked --no-default-features --features server-headless \
    live_server_retains_resource_payload_through_userlist_fanout_then_releases_it
)
```

These deterministic tests use in-memory SQLite and captured/rejecting
transports. They do not prove live Reticulum resource completion/cancellation,
slow-recipient fairness, task/RSS stability, or mixed-version interoperability.
Unauthenticated link lifetime remains a separate Phase 7.5 admission boundary.

## omenchatd pending upload-offer ownership

The pending-offer regressions require a global 256-item ceiling, an eight-item
per-identity ceiling, independent admission for another identity, same-owner
replacement without double counting, six-hour expiry, and capacity restoration
after removal. Engine tests require oversized filename metadata and the ninth
same-identity offer to return typed upload rejection frames. An identity
mismatch must leave the true owner's offer usable, and live link closure must
release every offer owned by that identity while updating status counters.

```bash
(
  cd src/server
  cargo test --locked --no-default-features --features server-headless \
    pending_upload
  cargo test --locked --no-default-features --features server-headless \
    upload_offer_rejects_metadata_and_pending_identity_overload
  cargo test --locked --no-default-features --features server-headless \
    accepted_upload_resource_is_stored_and_announced_to_room
  cargo test --locked --no-default-features --features server-headless \
    live_server_link_close_releases_owned_pending_upload_offers
)
```

These tests use deterministic store timestamps, in-memory SQLite, captured
transport, and an isolated upload root. They do not prove live low-bandwidth
Resource completion before expiry, Reticulum cancellation, reconnect transfer
resumption, mixed-version interoperability, or process RSS stability.
Unauthenticated link lifetime and total active-link admission remain separate
Phase 7.5 boundaries.

## omenchatd Resource terminal ownership

The standalone live-server tests require Reticulum Resource terminal events to
remain typed at the project boundary. An inbound failure must release the
identified peer's pending upload offers without closing a healthy link.
Outbound completion, failure, and cancellation must remain observable even
after link cleanup and must not be misclassified as unknown-link traffic.

```bash
(
  cd src/server
  cargo test --locked --no-default-features --features server-headless \
    resource_terminal
  cargo test --locked --no-default-features --features server-headless \
    reticulum_resource_terminals_cross_production_bridge_and_shutdown_cleanly
  cargo test --locked --no-default-features --features server-headless \
    inbound_resource_failure_releases_peer_upload_offers_without_closing_link
  cargo test --locked --no-default-features --features server-headless \
    live_server_link_close_releases_owned_pending_upload_offers
  cargo test --locked --no-default-features --features server-headless \
    accepted_upload_resource_is_stored_and_announced_to_room
)
```

These tests use isolated/in-memory storage and a captured transport. The bridge
test constructs the public Reticulum 0.9 `ResourceEvent` variants and drives
the production receiver, bounded control queue, typed project projection, and
owned shutdown path; it does not inject project-owned substitute terminals.
Reticulum 0.9 inbound-failure events contain a link and transfer hash but no
Resource metadata, so cleanup is deliberately per identified peer rather than
falsely correlated to one upload offer. A physical initiator-cancel packet,
live cancellation timing, resumable transfers, mixed 0.6/0.9, and Python
interop remain separate gates.

Run the explicit local Rust-to-Rust initiator-cancellation interoperability
harness on a host that permits loopback UDP sockets:

```bash
(
  cd src/server
  cargo test --locked --no-default-features --features server-headless \
    reticulum_loopback_resource_cancel_crosses_wire_and_production_bridge \
    -- --ignored --nocapture
)
```

The harness creates two in-memory Reticulum 0.9 transports with ephemeral
identities and two point-to-point UDP interfaces on dynamically reserved
loopback ports. It establishes a real announce/path/link, observes the Resource
advertisement and `ResourceInitiatorCancel` packets at the receiving interface,
requires the production bridge to emit the typed outbound-cancel terminal,
checks both link ends remain active, drains the bounded control queue, detaches
both interfaces, joins the bridge, and removes its isolated temporary roots.
It is ignored in the fast suite because it binds sockets. A complete Resource
transfer after cancellation was not established by this single-process harness
and remains a separate multi-process/pinned-Python gate.

## omenchatd two-process Resource completion gate

The explicit two-process gate re-executes only its own ignored test in separate
receiver and sender processes. It uses deterministic test-only identities,
dynamically reserved point-to-point UDP ports, bounded child lifetimes, and an
isolated coordination root. The intended sequence is baseline completion,
active-transfer initiator cancellation, and completion on the reused link:

```bash
(
  cd src/server
  cargo test --locked --no-default-features --features server-headless \
    reticulum_multiprocess_resource_complete_cancel_reuse \
    -- --ignored --nocapture
)
```

As of 2026-07-16 this is an intentionally failing release gate. The first
4 KiB baseline Resource does not complete, so cancellation/reuse is not
reached. Upstream diagnostics prove the sender accepts each request, finds the
outbound Resource, and builds four requested parts. The UDP worker then drops
those parts because its `size_of::<Packet>() * 3` transmit buffer is 456 bytes
on this target, while a maximum type-one Resource wire packet is 483 bytes;
serialization failure is silently ignored. The same implementation is present
in upstream v0.9.0, v0.9.1, v0.9.5, and `main` as checked on 2026-07-16.

The capacity invariant has its own quick, deliberately red regression:

```bash
(
  cd src/server
  cargo test --locked --no-default-features --features server-headless \
    reticulum_udp_tx_buffer_covers_max_resource_wire_packet \
    -- --ignored --nocapture
)
```

Do not mark the Resource, UDP, attachment, history, or post-cancel-reuse gates
complete until the upstream implementation uses a serialized-size-derived
buffer, reports serialization errors, and both explicit commands pass. Do not
work around this by lowering protocol bounds or silently fragmenting the
OMENchat wire protocol.

## omenchatd link admission and handshake lifetime

The live server admits at most 256 links and 32 incomplete handshakes. A
handshake remains pending until both the Reticulum `PeerIdentified` event and a
valid OMENchat `SessionOpen` have been observed. A narrow one-second deadline
sweep closes incomplete links at 30 seconds and releases their owned live
state. The limits are fixed safety ceilings and require no configuration or
wire-version negotiation.

```bash
(
  cd src/server
  cargo test --locked --no-default-features --features server-headless \
    pending_handshake
  cargo test --locked --no-default-features --features server-headless \
    incomplete_handshake_expires_at_deadline_but_complete_link_survives
  cargo test --locked --no-default-features --features server-headless \
    total_active_link_admission_is_bounded
)
```

These deterministic tests prove exact pending/total saturation, physical
rejection, capacity recovery after complete authentication/session
negotiation, the exact timeout boundary, completed-link survival, cleanup, and
counter projection.

Run the ignored optimized reconnect/slow-handshake measurement on Linux:

```bash
scripts/measure-omenchatd-links.sh /tmp/omenchatd-link-results
```

The 60-second harness keeps 224 authenticated sessions resident, repeatedly
saturates the remaining 32 pending slots, rejects an excess link, expires all
slow handshakes, replaces an authenticated identity, and finally drains to
zero. It records RSS, file descriptors, tasks, peak link counts, close-command
latency, rejection/expiry accounting, and final ownership. Override its duration
only for a harness smoke with `OMENCHATD_LINK_SOAK_SECONDS`; a shortened run is
not release evidence. The harness uses in-memory SQLite and a discard/count
transport, never the maintainer's state roots. Live Reticulum slow-handshake
and reconnect behavior, mixed 0.6/0.9, pinned/current Python, and native
Windows/macOS remain separate gates.

The 2026-07-16 optimized 60-second reference run completed 4,587 cycles,
rejected 4,587 excess links, expired 146,784 slow handshakes, reached exactly
256 active/32 pending links, and drained to zero. Maximum measured close-path
latency was 691 us; RSS grew 176,128 bytes; FD and task counts remained at four
and two respectively. Raw local evidence was written outside the repository at
`/tmp/omenchatd-link-60s`.

## OMENchat history resource integrity

The shared client decoder tests both immediate and delayed history-resource
delivery. Valid resources must preserve existing behavior. Invalid purpose and
oversized advertised lengths must fail before an offer enters pending retention;
after arrival, mismatched compression, uncompressed length, or compressed
payload length must fail before batch values reach the chat client. Boundary
coverage accepts the exact 4 MiB compressed/uncompressed offer limits and
rejects the next byte.

```bash
cargo test --locked --no-default-features --features desktop-product \
  chat::rns::tests::resource
cargo test --locked --no-default-features --features desktop-product \
  chat::protocol::batch::tests::resource_offer_lengths
```

These are deterministic in-memory transport tests. Live Reticulum resource
cancellation/progress, link loss during transfer, Python peers, and mixed
0.6/0.9 peers remain separate interoperability evidence.

## OMENchat resource terminal cleanup

The desktop lifecycle regressions prove that failed/cancelled inbound resource
events cross an item/byte-bounded staging queue, release retained pending-offer
bytes, and do not close an otherwise healthy link. Outbound terminals are not
misrouted into inbound cleanup.

```bash
cargo test --locked --no-default-features --features desktop-product \
  omenchat_event_staging_is_item_byte_and_release_bounded --lib
cargo test --locked --no-default-features --features desktop-product \
  omenchat_transport_consumes_and_bounds_resource_payloads_and_offers --lib
cargo test --locked --no-default-features --features desktop-product \
  omenchat_inbound_resource_failure_releases_pending_offers_but_keeps_link --lib
cargo test --locked --no-default-features --features desktop-product \
  omenchat_inbound_resource_cancellation_releases_pending_offer_but_keeps_link --lib
```

These tests inject project-owned lifecycle events and use isolated temporary
application roots. They do not prove an actual Reticulum initiator-cancel
exchange, per-resource cancellation, Python interoperability, or link loss
during a physical transfer.

## OMENchat announce identity attribution

Discovery tests require authenticated announce identity metadata to survive the
runtime candidate boundary, Directory ingestion, persistence/reload, filtering,
and selected-server presentation. Malformed hashes and identity mutation for an
existing destination must fail before state changes. Old records with no
identity field remain valid.

```bash
cargo test --locked --no-default-features --features desktop-product \
  omenchat_announce_identity
cargo test --locked --no-default-features --features desktop-product \
  omenchat_announce_rejects_malformed_identity_hash_before_mutation
cargo test --locked --no-default-features --features desktop-product \
  directory_selected_state_lines_are_kind_specific --lib
cargo test --locked --no-default-features --features desktop-product \
  directory_view_filter_matches_verified_identity_hash --lib
cargo test --locked --no-default-features --features desktop-product \
  announce_state_deduplicates_and_counts_payloads --lib
```

These deterministic tests do not replace a live Rust/Python announce signature,
path, reconnect, or server-impersonation interoperability test.

## OMENchat manual reconnect action validity

The typed connection-state regression distinguishes automatic retryability from
whether a user may start a new reconnect. A manual reconnect is available only
when disconnected or after a retryable failure; it is unavailable while
connection work is in flight, while joined or draining, and after a terminal
failure. Every tiled, compact, and maximized OMENchat pane toolbar consumes this
same predicate.

```bash
cargo test --locked --no-default-features --features desktop-product \
  connection_state_labels_and_retryability_are_typed --lib
```

This is a deterministic UI-policy/model test. It does not replace live manual
reconnect, automatic backoff, server restart, or Rust/Python interoperability
evidence.

## OMENchat outbound acceptance indicator

Timeline presentation tests distinguish a locally queued room message from a
server-accepted event using the existing temporary local-echo identifier. The
pending row must show `queued · awaiting server acceptance`; an ordinary event
must not show that marker. Existing live-client tests separately prove a
correlated `MessageAck` replaces the temporary identifier and that absence of
an acknowledgement retains the local echo for bounded delayed resend.

```bash
cargo test --locked --no-default-features --features desktop-product \
  omenchat_timeline_marks_only_unacknowledged_local_echoes_pending --lib
cargo test --locked --no-default-features --features desktop-product \
  live_send_message_local_echo_is_confirmed_by_message_ack --lib
cargo test --locked --no-default-features --features desktop-product \
  live_send_message_without_ack_keeps_pending_local_echo --lib
```

These deterministic tests do not claim live omenchatd acceptance latency,
disconnect/reconnect delivery, mixed-version behavior, or Reticulum transport
interoperability.

## OMENchat pending local-echo correlation bounds

The client retains only fixed-size correlation metadata for unacknowledged room
messages and actions: at most 64 entries per session and 256 globally. The
saturation regressions require rejection before transport send, exact pending
and rejected metrics, per-session fairness, capacity restoration after a valid
`MessageAck`, and complete entry release on session cleanup.

```bash
cargo test --locked --no-default-features --features desktop-product \
  live_pending_local_echoes --lib
```

The overload error uses the existing desktop failure path, which preserves the
composer draft. These deterministic in-memory tests do not replace a slow or
unresponsive live omenchatd saturation test, reconnect storm, mixed-version
test, or long-running memory measurement.

## Redacted OMENchat session diagnostics

The per-session diagnostics regression requires a valid JSON report no larger
than 8 KiB and verifies that public connection/transport metrics remain useful.
Its adversarial fixture places a private path, token-shaped text, a message
body, composer draft, room name, and free-form disconnect reason in live UI
state and requires all of them to be absent. A closed session must fail safely
without placing stale data on the clipboard.

```bash
cargo test --locked --no-default-features --features desktop-product \
  omenchat_session_diagnostics --lib
cargo test --locked --no-default-features --features desktop-product \
  copy_omenchat_session_diagnostics --lib
```

These are deterministic serialization, redaction, and routing tests. They do
not prove platform clipboard integration, live Reticulum counters, or that a
human support workflow cannot disclose information copied separately by the
user.

## OMENchat pane resource progress

The pane progress regressions populate the same bounded active-resource model
used by Network Doctor. They require source=`omenchat`, inbound direction, and
the exact active link identity before projecting progress into a session. When
multiple matching transfers are active, the newest is displayed and the
remaining count is reported. Terminal and other-link records are ignored, an
unknown total remains useful without a percentage, and maximum `u64` values do
not overflow percentage arithmetic.

```bash
cargo test --locked --no-default-features --features desktop-product \
  omenchat_session_resource_progress --lib
```

This is deterministic attribution and presentation evidence. It does not prove
live Reticulum progress cadence or identify whether an opaque OMENchat Resource
contains history, a user list, or media; that distinction remains unavailable
at the public transfer boundary.

## OMENchat link sequence ownership

The sequence regressions drive the client allocator at `u32::MAX`. They require
the final session-open/join pair to be reserved atomically, reject exhaustion
before transport send or local echo, keep other sessions independent, and
reset to `1,2` only when reconnect retires the prior link state. Equal numeric
sequences on independent sessions must coexist in pending message/upload maps,
and cancelling one session's transfers must neither remove the other's entries
nor reset the active link allocator.

```bash
cargo test --locked --no-default-features --features desktop-product \
  live_sequence --lib
cargo test --locked --no-default-features --features desktop-product \
  live_pending_correlations_allow_equal_sequences --lib
cargo test --locked --no-default-features --features desktop-product \
  live_reconnect_releases_prior_link_transfer_state --lib
```

These deterministic boundary tests preserve the existing `u32` wire field and
same-link omenchatd replay policy. They do not simulate four billion live
operations or replace mixed-version reconnect/replay interoperability tests.

## OMENchat reconnect backoff and stable reset

The reconnect-policy regressions require deterministic per-session jitter,
strictly increasing exponential delays, a 30-second cap, and exactly five
scheduled automatic attempts. A replacement link retains its attempt count
until its explicit stability deadline; merely registering a link or clearing
stale pending work cannot reset the budget. Deadline maintenance then clears
the budget without polling or closing the healthy transport.

```bash
cargo test --locked --no-default-features --features desktop-product \
  omenchat_reconnect_backoff --lib
cargo test --locked --no-default-features --features desktop-product \
  omenchat_retry_ --lib
cargo test --locked --no-default-features --features desktop-product \
  omenchat_delayed_reconnect --lib
cargo test --locked --no-default-features --features desktop-product \
  omenchat_reconnect_limit --lib
```

These deterministic scheduler tests do not replace a live server-restart soak,
real path variability, mixed-version peers, or CPU/RSS/link-count measurements.

## Stamped direct LXMF Resource interoperability

The direct-Resource regression sends a deterministic 64 KiB stamped body and a
2 KiB binary file attachment over the integrated Reticulum runtime. It requires
automatic `link-resource` selection, a bounded `0 / total` outbound offer,
terminal completion correlated to the same message and Resource hash, and
Python verification of body and attachment names, sizes, SHA-256 values, source
signature, and stamp admission.

```bash
cargo test --locked --no-default-features --features desktop-product \
  clean_lxmf_resource_offer_reports_bounded_total_and_operation --lib

OMEN_PINNED_RNS_SOURCE=/path/to/pinned/Reticulum \
OMEN_PINNED_LXMF_SOURCE=/path/to/pinned/LXMF \
cargo test --locked --no-default-features --features desktop-product \
  pinned_python_lxmf_stamped_direct_resource_preserves_bytes_and_reports_progress \
  -- --ignored --nocapture --test-threads=1
```

`scripts/run-current-python-drift.sh` selects the current-Python counterpart.
The test uses temporary config/storage roots and prints only bounded metadata.
Outbound incremental percentages are not claimed because the 0.9.5 transport
currently exposes receiver progress and sender terminal events.

## Mixed OMENbrowser 0.6.0-1 and 0.9.6-2 LXMF/OMENchat

This Linux-only multi-process harness tests actual application binaries rather
than importing 0.6 types into the current build:

```bash
bash scripts/run-mixed-0-6-0-9-lxmf.sh \
  --report target/mixed-0-6-0-9-lxmf-report.json

bash scripts/run-mixed-0-6-0-9-lxmf.sh --resource \
  --report target/mixed-0-6-0-9-lxmf-resource-report.json

bash scripts/run-mixed-0-6-0-9-lxmf.sh --restart \
  --report target/mixed-0-6-0-9-lxmf-restart-report.json

bash scripts/run-mixed-0-6-0-9-propagation.sh \
  --report target/mixed-0-6-0-9-lxmf-propagation-report.json

bash scripts/run-mixed-0-6-0-9-propagation.sh --reverse \
  --report target/mixed-0-6-0-9-lxmf-propagation-reverse-report.json

bash scripts/run-mixed-0-6-0-9-propagation.sh --node-restart \
  --report target/mixed-0-6-0-9-lxmf-propagation-node-restart-report.json

bash scripts/run-mixed-0-6-0-9-propagation.sh --node-crash \
  --report target/mixed-0-6-0-9-lxmf-propagation-node-crash-report.json

bash scripts/run-mixed-0-6-0-9-propagation.sh --stamp-ticket \
  --report target/mixed-0-6-0-9-lxmf-propagation-stamp-ticket-report.json

bash scripts/run-mixed-0-6-0-9-omenchat-history.sh \
  --report target/mixed-0-6-0-9-omenchat-history-report.json

bash scripts/run-mixed-0-6-0-9-omenchat-live.sh \
  --report target/mixed-0-6-0-9-omenchat-live-current-client-report.json

bash scripts/run-mixed-0-6-0-9-omenchat-live.sh --reverse \
  --report target/mixed-0-6-0-9-omenchat-live-old-client-report.json

bash scripts/run-mixed-0-6-0-9-omenchat-live.sh --restart \
  --report target/mixed-0-6-0-9-omenchat-live-restart-current-client-report.json

bash scripts/run-mixed-0-6-0-9-omenchat-live.sh --reverse --restart \
  --report target/mixed-0-6-0-9-omenchat-live-restart-old-client-report.json

bash scripts/run-mixed-0-6-0-9-omenchat-live.sh --history-resource \
  --report target/mixed-0-6-0-9-omenchat-live-history-resource-current-client-report.json

bash scripts/run-mixed-0-6-0-9-omenchat-live.sh --reverse --history-resource \
  --report target/mixed-0-6-0-9-omenchat-live-history-resource-old-client-report.json
```

The old binary is built with `--locked` from immutable hardened commit
`5ba6683055fb6c59111919fbad1ac37f56a4c203` in a disposable Git archive. The
current binary uses the canonical native-network layers. Separate temporary
application/identity/Reticulum roots connect through the exact Python RNS
version selected by the harness on
ephemeral loopback TCP with fixed public IFAC fixture credentials supplied by
an owner-only file. The harness requires both direct sends, one reciprocal
peer-bound message in each process, and exact 32-byte-title/102-byte-content
shape metadata. It retains no raw application report containing paths or
identity material.

For adjacent-release qualification, the direct, propagation, and OMENchat
harnesses also
accept an explicit immutable old commit and expected version without changing
their legacy defaults. The published `v0.9.5-2` cases use:

```bash
export OMEN_MIXED_OLD_COMMIT=c6ad96d3e083425a62e6713abe8598c4d494bde0
export OMEN_MIXED_OLD_VERSION=0.9.5-2
export OMEN_MIXED_OLD_TARGET_DIR="$PWD/target/mixed-v0.9.5-2"
export OMEN_MIXED_OLD_SERVER_STOP_MODE=orderly

bash scripts/run-mixed-0-6-0-9-lxmf.sh
bash scripts/run-mixed-0-6-0-9-lxmf.sh --resource
bash scripts/run-mixed-0-6-0-9-lxmf.sh --restart
bash scripts/run-mixed-0-6-0-9-propagation.sh --reverse
OMEN_MIXED_RECOVER_UNKNOWN_SENDER=true \
  bash scripts/run-mixed-0-6-0-9-propagation.sh
OMEN_MIXED_RECOVER_UNKNOWN_SENDER=true \
  bash scripts/run-mixed-0-6-0-9-propagation.sh --node-restart
OMEN_MIXED_RECOVER_UNKNOWN_SENDER=true \
  bash scripts/run-mixed-0-6-0-9-propagation.sh --node-crash
OMEN_MIXED_RECOVER_UNKNOWN_SENDER=true \
  bash scripts/run-mixed-0-6-0-9-propagation.sh --stamp-ticket
bash scripts/run-mixed-0-6-0-9-omenchat-history.sh
bash scripts/run-mixed-0-6-0-9-omenchat-live.sh
bash scripts/run-mixed-0-6-0-9-omenchat-live.sh --reverse
bash scripts/run-mixed-0-6-0-9-omenchat-live.sh --restart
bash scripts/run-mixed-0-6-0-9-omenchat-live.sh --reverse --restart
bash scripts/run-mixed-0-6-0-9-omenchat-live.sh --history-resource
bash scripts/run-mixed-0-6-0-9-omenchat-live.sh --reverse --history-resource
```

The stop-mode override records a real release difference: the long-range
`0.6.0-1` server default still expects SIGTERM, while `0.9.5-2` participates in
the harness's orderly server shutdown. It changes only the assertion for the
selected immutable old peer.

The recovery option does not resend the logical message. It requires an initial
sync to retain exactly one transient and request the unknown authenticated
sender path, learns a fresh sender announce, and then syncs the retained
transient. It is used where the older recipient does not persist enough sender
evidence across the receive/sync process boundary.

For propagation-node restart and crash cases, both peers reconnect to the new
ephemeral listener before the recovery announce. The assertion also requires a
stable propagation identity and exactly one restored queued transient. The
stamp/ticket case applies the same retained-transient recovery while preserving
its independent stamp-policy and ticket-wire assertions.

Each direct and Resource case runs one direction at a time with its receive-only
peer online before the sender opens a link. Each logical message is attempted
exactly once; this avoids the simultaneous reciprocal link activation already
identified by the restart case. With `--resource`, the harness copies both
source trees to disposable roots and applies
`fixtures/lxmf/mixed_application_resource_driver.patch`. That fixture-only
patch changes the diagnostic message body to 65,536 ASCII bytes; it does not
alter either manifest, runtime adapter, protocol, state, or normal CLI in the
working tree. Its old-version Cargo target is separate from the normal direct,
restart, propagation, and OMENchat target so the fixture binary cannot be
reused by a later case. Both versions select Resource above their shared 431-byte
Link-packet MDU. The harness then requires one exactly 65,536-byte, peer-bound
decoded message in each application.

With `--restart`, after the initial two-direction exchange, both processes exit,
reopen the same
application/identity/configuration/Reticulum roots, and repeat. The report
requires stable local destinations, new outbound and inbound message IDs,
exact peer-send/inbound-ID correlation, and exactly one inbound event per
direction. Compared IDs and destinations are discarded rather than retained.

The propagation command tests both store-and-forward directions separately.
The sender submits one propagated message to the exact Python RNS/LXMF versions
selected by the harness;
the recipient then reconnects with the same isolated identity and syncs it. In
the reverse case, the current recipient must initially defer the unknown old
sender without acknowledgement, request sender-path recovery, learn a fresh
authenticated announce without a message retransmission, and then decode the
same retained transient. A link-activation failure before payload admission is
retryable once; no other state is. Both reports require source correlation,
the expected message shape, and a zero-entry node after acknowledgement.

With `--node-restart`, the current sender still submits exactly once. The
Python node exits only after reporting one queued transient, then reopens the
same LXMF storage with the same deterministic identity on a new ephemeral
loopback port. The harness requires the restarted router to report one restored
entry and the same propagation destination before the old recipient syncs and
acknowledges it. This proves orderly restart persistence, not power-loss or
filesystem durability.

With `--node-crash`, the fixture takes a baseline of its isolated LXMF storage,
then requires the storage to change, remain stable across bounded samples, and
contain nonzero bytes after the single queue admission. Only then does the
harness send `SIGKILL` to that fixture PID. A new process must restore the same
node identity and one transient before exactly-once recipient sync and
acknowledgement. This is process-crash recovery after observed settled storage;
it is not physical power-loss, filesystem, or storage-device durability proof.

With `--stamp-ticket`, Python enables propagation-stamp enforcement and
advertises its effective positive cost. The current application must report a
bounded stamp at that exact cost and include a fresh reply ticket. Queue
admission proves Python accepted the stamp; the old application must decode
the same message and recover a correctly shaped ticket before acknowledgement
removes the transient. Ticket bytes and stamp material are checked only inside
the temporary root and are never retained.

The OMENchat history command is deliberately network-free. It builds a small
probe against each version's canonical `desktop-product` graph and uses only
the public `SqliteChatStore` API. The old application seeds server, room,
active-room, and event state; current reopens and appends; old reopens that
current write and appends; current performs the final reopen. All four stages
must preserve exact event ordering/content and metadata. The database lives
under one explicit temporary root and is deleted; the retained report contains
only versions, counts, and validation booleans.

The live OMENchat command builds the selected client/server pair from the
immutable hardened `0.6.0-1` source and current `0.9.6-2` source. The default
case is current client to old server; `--reverse` is old client to current
server. Each starts both binaries with separate isolated roots over an
ephemeral loopback TCP interface, then requires the client to start its
runtime, open a link and session, join a room, send one message, and observe
the echoed room event. All identity, server, message, and Reticulum state is
deleted. These cases prove reciprocal single-session message compatibility;
history Resource transfer remains a separate gate.

With `--restart`, the selected client first completes its exchange with the
opposite-version server. The harness stops that server within a bounded
deadline, reopens the same server home on the same interface, requires an
unchanged destination, and runs the client again with its original application
root. The second process must repeat link/session/join/message/echo
successfully. Hardened `0.6.0-1` predates the owned SIGTERM drain path and
therefore exits with the expected signal status; current `0.9.6-2` must report
an orderly stop. Neither test claims that a continuously running desktop
automatically reconnected.

With `--history-resource`, the current client connects to the hardened old
server and the isolated server configuration sets its large-batch threshold to
one byte. A normal small first-client message therefore makes the production
history path choose an OMENchat Resource without changing the committed server
default or using an oversized room message. A second client with a separate
identity/root must receive `resource_data`, decode `history_prepended` from
inside that Resource event, and observe the first client's exact message
content. This proves current-client consumption of old-server history
Resources. The reciprocal `--reverse --history-resource` case applies the same
isolated threshold to current omenchatd and requires the hardened old client to
decode and validate its history Resource. Together they prove both application
directions without changing either production default.

The scheduled Python-interoperability workflow runs all fifteen cases as
release-blocking mixed-application gates and retains only their redacted
summaries for 14 days. The Resource result proves complete application decode
in both directions, not peer delivery from a sender-side Resource completion
event. The stamp/ticket case does not prove ticket use on a propagated reply.
The SQLite case proves store-format reopening, not history-resource transfer or
crash durability. The live cases prove both client/server directions for one
session/message exchange. The restart cases prove both client-state roots
reopen after the opposite-version server process restarts, not live automatic
reconnect. The history-Resource cases prove both mixed application directions.
The same scheduled workflow also runs the current-product continuous reconnect
case and the current-product two-client upload/Resource case, retaining only
their redacted reports. Physical power-loss durability remains separate
evidence.

## OMENchat routed-path admission

The native adapter regression
`clean_omenchat_accepts_known_routed_paths_without_app_hop_cutoff` verifies
that OMENbrowser delegates hop limits to Reticulum 0.9.6. Known 1-, 3-, 13-,
and 127-hop paths are usable; an unknown path still requires discovery. Run it
with:

```bash
cargo test --locked --no-default-features --features desktop-product \
  clean_omenchat_accepts_known_routed_paths_without_app_hop_cutoff
```

This is a deterministic admission-policy test. It does not prove the
maintainer's public-gateway/private-gateway deployment, which still requires a
live announce, path, link, and OMENchat exchange smoke test from an isolated
client root.

## omenchatd multiple TCP clients

The standalone server config tests add two TCP clients, retain an unrelated
TCP listener, list only redacted endpoint/IFAC state, delete one endpoint, and
verify the owner-only recovery backup. The live Reticulum test parses the
generated configuration, starts two independent loopback TCP client workers,
and requires bounded shutdown to join both workers:

```bash
cargo test --locked --manifest-path src/server/Cargo.toml \
  --no-default-features --features server-full tcp_client
cargo test --locked --manifest-path src/server/Cargo.toml \
  --no-default-features --features server-full \
  live_runtime_owns_two_configured_tcp_client_workers
```

These tests use isolated temporary server homes. The loopback workers prove
multi-interface ownership and shutdown, not connectivity through the
maintainer's WNS/private-gateway topology.

## OMENchat mentions-only unread policy

The client-store, model, live reducer, and hidden-pane regressions verify that
the per-room preference defaults off, survives an isolated SQLite reopen,
rejects invalid stored booleans, and accepts only an exact bound numeric
mention while enabled:

```bash
cargo test --locked --no-default-features --features desktop-dev mute_except
cargo test --locked --no-default-features --features desktop-dev \
  hidden_omenchat_muted_room_counts_only_authoritative_mentions_as_unread
```

These tests do not activate `reply-mentions-v1`, send a live notification, or
claim server-authoritative read receipts. They prove local bounded unread
presentation and persistence only.

## OMENchat negotiated reaction client state

These isolated regressions exercise the shared GUI/TUI model, identity-scoped
SQLite cache, negotiated live parser, and bounded inline/Resource snapshot
transport. They prove duplicate deltas are idempotent, explicit-target
snapshots are authoritative only for their page, overload rolls back prior
state, and restart restores eligible retained targets. Presentation tests prove
actor deduplication, fixed token ordering, identity/room/target scoping, exact
counts, and local-user highlighting in the non-interactive Iced timeline model.
They also prove restart/reconnect clears non-persistent snapshot evidence while
retaining bounded cache rows and that the next snapshot prunes targets no
longer present in resident history. Dormant action tests require both
capabilities, prohibit optimistic state, match acknowledgement identity and
request fields, preserve canonical intents over restart, and block recovered
retry after capability loss. Production requests and accepts `reactions-v1`
only as an explicit extension of a valid durable-mutation negotiation.

```bash
cargo test --locked --no-default-features --features desktop-dev reaction --lib
cargo test --locked --no-default-features --features desktop-dev \
  client_reactions_are_authoritative_bounded_and_restart_safe --lib
cargo test --locked --no-default-features --features desktop-dev \
  reaction_delta_and_snapshot_parsers_are_negotiated_bounded_and_authoritative --lib
cargo test --locked --no-default-features --features desktop-dev \
  client_transport_decodes_reaction_inline_and_resource_snapshots --lib
cargo test --locked --no-default-features --features desktop-dev \
  reaction_snapshot_overload_rolls_back_prior_page_state --lib
cargo test --locked --no-default-features --features desktop-dev \
  reaction_summaries_are_deduplicated_ordered_and_identity_scoped --lib
cargo test --locked --no-default-features --features desktop-dev \
  omenchat_timeline_uses_shared_read_only_reaction_presentation --lib
cargo test --locked --no-default-features --features desktop-dev \
  reaction_snapshot_evidence_and_rows_follow_retained_history --lib
cargo test --locked --no-default-features --features desktop-dev \
  durable_reaction --lib
cargo test --locked --no-default-features --features desktop-dev \
  reaction_intent_survives_restart --lib
```

Standalone server qualification and explicit isolated measurements:

```bash
(
  cd src/server
  cargo test --locked --no-default-features --features server-headless \
    reaction --lib
  cargo test --locked --no-default-features --features server-headless \
    reaction_state_retention_measurement --lib -- --ignored --nocapture
  OMEN_REACTION_MEASUREMENT_ITEMS=4096 \
    cargo test --locked --no-default-features --features server-headless \
    reaction_state_retention_measurement --lib -- --ignored --nocapture
  cargo test --locked --no-default-features --features server-headless \
    durable_replay_retention_measurement --lib -- --ignored --nocapture
)
cargo test --locked --no-default-features --features desktop-dev \
  durable_intent_retention_measurement --lib -- --ignored --nocapture
```

The measurement tests use unique temporary SQLite roots, remove their files,
and print observations rather than enforcing hardware-dependent latency
thresholds. The 2026-07-26 qualification results and remaining live smoke are
recorded in `docs/audits/omenchat-reactions-qualification.md`.

The Ratatui Messages workspace currently covers LXMF conversations and has no
OMENchat session/timeline model. The omenchatd TUI is server administration and
has no client-local user identity. These commands therefore do not claim a TUI
reaction rendering path.

## OMENchat message-revision contract

The shared correction/tombstone contract uses reserved operations 35–39 and
activates them only through explicit `message-revisions-v1` negotiation:

```bash
cargo test --locked -p omenchat-protocol
cargo test --locked --no-default-features --features desktop-product \
  message_revision --lib
cargo test --locked --no-default-features --features desktop-product \
  live_open_requests_supported_durable_extensions_with_persistent_client_identity --lib
(
  cd src/server
  cargo test --locked --no-default-features --features server-headless \
    message_revision --lib
  cargo test --locked --no-default-features --features server-full \
    message_revision -- --nocapture
)
```

The shared tests cover exact request/action/replacement shapes, bounds,
acknowledgement/event agreement, canonical explicit-target snapshots,
capability dependency, and a stable durable hash. Both independent frame
codecs preserve the same correction bytes. Client and server tests prove
explicit request/accept dependency, unsolicited acceptance rejection,
downgrade clearing, and base-only peer isolation. The server-full focus also
covers persistence/execution, Link-scoped fan-out, history snapshots, replay
suppression, and retirement.
The desktop-dev focus covers the separate immutable-event revision projection,
stable item/byte bounds, additive cache and restart recovery, transactional
capacity rollback, ordered/idempotent deltas, authoritative snapshots,
fail-closed snapshot evidence, durable-intent recovery, and inline/Resource
decoding:

```bash
cargo test --locked --no-default-features --features desktop-dev \
  message_revision --lib -- --nocapture
```

Production requests revisions only beside durable mutations and a persistent
client instance identifier; unsolicited acceptance cannot activate the
reducer. Unit tests do not claim native package or interactive GUI support.

The deterministic pre-activation qualification additionally runs the bounded
retention cleanup/fault and database-recovery filters documented in
`docs/audits/omenchat-message-revisions-qualification.md`. Capability-loss
regressions prove that action targets disappear and a late acknowledgement
cannot resolve pending intent outside the negotiated session. Mixed-version
evidence means unchanged ordinary protocol-v1 traffic and no optional revision
operation; it does not claim that an adjacent peer implements this capability.
The current/current process gate is:

```bash
bash scripts/release-omenchat-smoke.sh \
  --browser-bin target/debug/omenbrowser_rs \
  --server-bin src/server/target/debug/omenchatd \
  --tcp 127.0.0.1:<unused-port> \
  --revision-smoke --multi-client
bash scripts/run-omenchat-continuous-reconnect.sh \
  --report target/omenchat-continuous-reconnect-revision-report.json
```

It covers deliberately lost acknowledgement, exact replay, correction and
tombstone Resource snapshots, two isolated client roots, and one continuous
client across an orderly server restart and replacement Link.

The binary-only implementation for these OMENchat process gates lives in
`src/omenchat_smoke.rs`. The root CLI module retains argument parsing and shared
runtime-configuration helpers; the child module owns the bounded live-smoke
transport, waits, reconnect flow, mutation qualification, and report
formatting. This is an ownership boundary only: CLI flags and report fields are
unchanged. The extraction record and rollback are documented in
`docs/audits/omenchat-smoke-module-extraction.md`.

## Bounded local-history desktop search

These focused desktop-product tests exercise explicit-submit state, the
one-active/one-replaceable-pending owner, exhaustive message routing, truthful
limit presentation, and fail-closed validation of persisted LXMF and OMENchat
jump targets:

```bash
cargo test --locked --no-default-features --features desktop-product \
  desktop::history_search::tests
cargo test --locked --no-default-features --features desktop-product \
  desktop::history_search_state::tests
cargo test --locked --no-default-features --features desktop-product \
  presentation_labels_sources_and_every_limit_truthfully
cargo test --locked --no-default-features --features desktop-product \
  history_search_messages_have_one_compile_time_route
```

All storage tests use explicit temporary roots. An interactive packaged-app
smoke must still verify text-input focus, result density, and jump scroll
restoration; unit tests do not claim display-server behavior.

## OMENchat safe invitation URI

The first dormant invitation slice is a pure bounded parser/serializer. It has
no production connection, trust, persistence, or QR caller:

```bash
cargo test --locked --no-default-features --features desktop-product \
  chat::invitation::tests --lib
```

The tests cover the exact legacy plain URI, enhanced canonical ordering,
outer/field boundaries, hexadecimal normalization, room overflow, malformed
percent/UTF-8 data, unsupported or duplicate fields, authority tricks, and
secret-field omission. They also cover all Directory identity-evidence classes,
conflicting duplicate precedence, one-item preview replacement, invalid-input
preservation, explicit cancellation, and conflict-blocked confirmation.
Desktop preview confirmation and deferred room admission use the development
profile because their deterministic session tests exercise the mock runtime:

```bash
cargo test --locked --no-default-features --features desktop-dev \
  --lib invitation_room
cargo test --locked --no-default-features --features desktop-dev \
  --lib cancelling_or_replacing_an_invitation
```

These tests prove exact destination/session binding, exact numeric catalog
match, cross-session isolation, mismatch clearing, cancellation, and
replacement. Native live smoke must still confirm a real authenticated catalog
and join; QR presentation requires separate tests before activation.

Canonical clipboard generation is covered in the production profile:

```bash
cargo test --locked --no-default-features --features desktop-product \
  --lib omenchat_invitations
```

The tests verify joined-room and bounded-label serialization, omission of an
unjoined room, fail-closed omission of conflicting identity evidence, missing
session handling, and no session-state mutation. A packaged display smoke must
still confirm the share icon writes the canonical URI to the native clipboard.
QR rendering/import remains outside this test claim.

Enhanced Micron link routing uses the same invitation reducer:

```bash
cargo test --locked --no-default-features --features desktop-product \
  --lib enhanced_micron_link
cargo test --locked --no-default-features --features desktop-dev \
  --lib plain_micron_omenchat_link
```

The production tests prove valid and malformed enhanced links cannot bypass
confirmation. The development-profile test proves the compatible plain link
still reaches the deterministic mock open path. A packaged interaction smoke
must still cover both keyboard-focused and pointer link activation. QR
rendering/import remains outside this claim.

Canonical product QR generation and ownership:

```bash
cargo test --locked --no-default-features --features desktop-product \
  --lib qr_owner
bash scripts/verify-product-features.sh
cargo check --locked --no-default-features \
  --features desktop-product-static-media
```

The tests prove one-item replacement, toggle/close and session cleanup,
canonical 2 KiB input, missing-session failure, and byte-identical clipboard
text after the visible QR is created. The graph gate requires Iced QR and
locked `qrcode 0.13.0` in both canonical products. Packaged native smoke must
still verify QR contrast/scanning, layout at supported UI scales, clipboard
behavior, and absence of camera/image-import permissions. Camera and image-file
QR decoding are not implemented.

The desktop quick-open activation adds focused tests proving parse-only input
does not create a session or mutate Directory state, a conflicting fingerprint
cannot be confirmed, cancellation is explicit, and confirmation consumes the
preview before returning the existing asynchronous open task. These tests do
not execute the returned Iced task or claim live Link establishment:

```bash
cargo test --locked --no-default-features --features desktop-product \
  invitation --lib
cargo test --locked --no-default-features --features desktop-product \
  omenchat_domain_messages_have_one_compile_time_route --lib
```

The dormant omenchatd history-compaction primitive has a focused isolated
SQLite gate:

```bash
(cd src/server && cargo test --locked --no-default-features \
  --features server-full history_retention -- --nocapture)
```

It covers the 64-original transaction ceiling, exact usage-ledger accounting,
projection-aware batch reduction, excessive single-event fan-out refusal,
surviving versus selected replies, upload/durable-replay preservation,
monotonic event IDs, and rollback at every cleanup/ledger/commit boundary.
The explicit primitive remains inert under the default configuration. The live
production store invokes it only when `[history_retention].enabled = true`.

Typed policy and redacted maintenance-status coverage uses:

```bash
(cd src/server && cargo test --locked --no-default-features \
  --features server-full history_retention_ -- --nocapture)
(cd src/server && cargo test --locked --no-default-features \
  --features server-full \
  machine_readable_status_and_doctor_are_valid_and_redacted -- --nocapture)
```

The tests prove disabled defaults, exact round-trip, enabled zero-limit
rejection, hard-maximum clamping, a 256-room read ceiling, truncation evidence,
complete/incomplete/missing ledger classification, read-only behavior, and
machine-output redaction.

The same focused `history_retention` gate covers admission integration:
disabled compatibility; independent age, item, and byte triggers; retention of
one oversized newest event; ordinary insert compaction; incomplete-ledger
refusal; and rollback of the insertion, sequence, dependency cleanup, and usage
ledger when restoring a ceiling would require more than the 64-event batch.
The full server suite additionally exercises durable writers through the shared
store boundary. Live mixed-version and restart/Resource smoke remain separate
release gates.

## Dormant OMENchat pin contract

The first pin slice reserves the shared operations and bounded wire shapes
without advertising or accepting `room-pins-v1`:

```bash
cargo test --locked -p omenchat-protocol
cargo test --locked --no-default-features --features desktop-product \
  --lib pins_v1_fixture_is_bidirectionally_exact_and_dormant
(cd src/server && cargo test --locked --no-default-features \
  --features server-headless pins_v1_fixture_is_bidirectionally_exact_and_dormant)
(cd src/server && cargo test --locked --no-default-features \
  --features server-headless pin_capability_requires_explicit_durable_request)
```

These tests cover exact request/ack/event/snapshot shapes, malformed and
trailing values, identifier and timestamp validation, snapshot count and
canonical-order bounds, target scoping, canonical durable hash inputs, and
byte-exact agreement between the independent desktop and server codecs. They
do not claim storage, authorization, replay, fan-out, client projection, UI,
mixed-version live behavior, or capability activation.

Schema-9 dormant pin storage, migration faults, bounded admission, compaction,
and the guarded schema-8 copy use isolated SQLite roots:

```bash
(cd src/server && cargo test --locked --no-default-features \
  --features server-headless store::pins::tests -- --nocapture)
(cd src/server && cargo test --locked --no-default-features \
  --features server-headless pin_schema -- --nocapture)
(cd src/server && cargo test --locked --no-default-features \
  --features server-headless compaction_ -- --nocapture)
(cd src/server && cargo test --locked --no-default-features \
  --features server-headless schema_eight_export -- --nocapture)
```

The store tests do not advertise `room-pins-v1`. Dormant transactional server
execution, restart replay, conflict handling, role authorization, Link-scoped
snapshots, and capable-room-only fan-out use:

```bash
(cd src/server && cargo test --locked --no-default-features \
  --features server-headless dormant_pin -- --nocapture)
(cd src/server && cargo test --locked --no-default-features \
  --features server-headless pin_events_fan_out_only -- --nocapture)
```

These tests bind the internal Link capability state explicitly. After the
separate activation slice, production negotiation tests prove the desktop
requests `room-pins-v1` only with durable identity and omenchatd refuses a
pin-only request.

The dormant desktop projection, identity-scoped SQLite cache, restart-stale
authority, inline snapshot decoding, and read-only timeline labels use:

```bash
cargo test --locked --no-default-features --features desktop-product --lib \
  client_pins_are_bounded_authoritative_and_restart_stale
cargo test --locked --no-default-features --features desktop-product --lib \
  pin_store_is_restart_safe_ordered_and_snapshot_authoritative
cargo test --locked --no-default-features --features desktop-product --lib \
  pin_delta_and_snapshot_reducers_remain_dormant_and_authoritative
cargo test --locked --no-default-features --features desktop-product --lib \
  omenchat_timeline_distinguishes_authoritative_and_cached_pins
cargo test --locked --no-default-features --features desktop-product --lib \
  client_transport_decodes_dormant_pin_snapshot_inline
```

Restart preserves bounded cached pin rows but deliberately clears authority.
The timeline renders authoritative state as `📌 pinned` and unreconciled
restart state as `📌 pinned · cached`. No pin action is rendered in a
production session. Production negotiation remains unchanged; mixed-version
live qualification and capability activation remain separate slices.

The next dormant slice exercises pin/unpin controls only through test-bound
negotiation state:

```bash
cargo test --locked --no-default-features --features desktop-product --lib \
  durable_pin_
cargo test --locked --no-default-features --features desktop-product --lib \
  pin_controls_require_test_negotiation_role_authority_and_current_state
cargo test --locked --no-default-features --features desktop-product --lib \
  pin_prepare_persists_before_send_and_preserves_ordinary_draft
cargo test --locked --no-default-features --features desktop-product --lib \
  dormant_pin_intent_kind_is_restart_safe_without_capability_activation
cargo test --locked --no-default-features --features desktop-product --lib \
  live_open_requests_supported_durable_extensions_with_persistent_client_identity
```

These tests require current moderator/administrator and retained-target
evidence, persist intent before transmission, permit only one pending pin
mutation per target, reject mismatched ACKs, preserve uncertainty across
restart, and keep accepted mutation evidence separate until an authoritative
delta or exact-target snapshot arrives. Post-ACK confirmation slots share the
existing 256-global/64-per-session mutation budget and clear on capability or
Link loss. The final test now proves the production desktop requests
`room-pins-v1` only inside the persistent durable negotiation envelope. The
session activation test proves unsolicited acceptance and downgrade remain
fail closed; the standalone server test proves a pin-only request is refused.

The dormant deterministic qualification filter and explicit isolated
measurement are:

```bash
cargo test --locked --no-default-features --features desktop-product pin --lib
(cd src/server && cargo test --locked --no-default-features \
  --features server-headless pin --lib)
(cd src/server && cargo test --locked --no-default-features \
  --features server-headless pin_state_retention_measurement \
  --lib -- --ignored --nocapture)
```

The server filter includes exact global active saturation, per-room/global
audit replacement, and the maximum 256-target/64-entry inline frame. The
measurement uses one isolated temporary database, checkpoints it before sizing,
removes it afterward, and reports observations rather than release thresholds.
Exact deterministic, activation, and live-process evidence is in
`docs/audits/omenchat-pins-qualification.md`.

Activated current/current pins are qualified with isolated roots and a
moderator role assigned through omenchatd's confirmation-gated headless admin
CLI:

```bash
bash scripts/release-omenchat-smoke.sh \
  --browser-bin target/debug/omenbrowser_rs \
  --server-bin src/server/target/debug/omenchatd \
  --tcp 127.0.0.1:<unused-port> \
  --path-wait 45 \
  --out <isolated-output-root> \
  --message "pin reconnect qualification" \
  --pin-smoke \
  --continuous-client-reconnect
```

The gate covers pin capability and authority, deliberately withheld
acknowledgement, exact durable replay, authoritative bounded-inline snapshot,
semantic no-op, unpin, persistent-intent cleanup, graceful server restart, and
replacement-Link recovery. It does not weaken role checks or automatically
resend uncertain mutations. The server's small large-batch threshold still
forces Resource transport for eligible history/reaction/revision batches;
`PinSnapshot` is intentionally a separately bounded compressed inline frame.

## OMENchat announcement-room contract

The announcement-room wire boundary is covered by:

```bash
cargo test --locked -p omenchat-protocol room_policy -- --nocapture
cargo test --locked -p omenchat-protocol announcement_room -- --nocapture
cargo test --locked --no-default-features --features desktop-product \
  announcement_room_values_are_byte_exact_and_negotiation_scoped --lib
(
  cd src/server
  cargo test --locked --no-default-features --features server-headless \
    announcement_room --lib -- --nocapture
)
```

These tests require exact legacy four-field and negotiated five-field room
values, independent desktop/server MessagePack agreement, fixed known policy
bits, bounds, explicit negotiation shape, and production capability acceptance.
They do not alone claim schema 11, authorization, presentation, or process
traffic. Evidence is in
`docs/audits/omenchat-announcement-rooms-wire-qualification.md`.

The slow-mode scalar and production wire extension are covered independently:

```bash
cargo test --locked -p omenchat-protocol -- --nocapture
cargo test --locked --no-default-features --features desktop-product \
  slow_mode_room_value_is_byte_exact_and_shape_scoped --lib -- --nocapture
(
  cd src/server
  cargo test --locked --no-default-features --features server-headless \
    slow_mode_room_value_is_byte_exact_and_shape_scoped --lib -- --nocapture
)
```

These gates require exact four-/five-/six-field shape isolation, a bounded
`slow_mode_seconds` scalar, identical desktop/server MessagePack bytes, typed
error number 1017, and the `durable-mutations-v1` capability dependency.
Canonical products negotiate and enforce the extension; shape and codec
qualification remains independently covered below. Evidence is in
`docs/audits/omenchat-slow-mode-wire-qualification.md`.

Schema-12 slow-mode storage and guarded schema-11 rollback are covered by:

```bash
(
  cd src/server
  cargo test --locked --no-default-features --features server-headless \
    slow_mode --lib -- --nocapture
  cargo test --locked --no-default-features --features server-headless \
    schema_eleven --lib -- --nocapture
  cargo test --locked --no-default-features --features server-headless \
    every_slow_mode_schema_fault_boundary --lib -- --nocapture
)
```

These tests require disabled-by-default migration without history scans,
transactional rollback at every schema-12 boundary, a readable pre-v12
schema-11 backup, scalar/revision atomicity, fixed item/logical-byte bounds,
64-row incremental expired pruning, fail-closed saturation, restart
persistence, and publication-failure cleanup. The operator rollback command is:

```bash
omenchatd database export-schema11-copy \
  --to <new-database-path> --confirm --home <server-home>
```

The staged copy removes only `slow_mode_seconds`,
`room_slow_mode_admissions`, and its expiry index. Evidence is in
`docs/audits/omenchat-slow-mode-storage-qualification.md`.

The admission matrix retains `SessionEngine::with_test_slow_mode` for isolated
boundary control; canonical constructors enable capability negotiation and
enforcement through `omenchat-slow-mode`:

```bash
cargo test --locked --manifest-path src/server/Cargo.toml \
  --no-default-features --features server-headless slow_mode -- --nocapture
```

This matrix proves that exact durable replay bypasses cooldown work, a new
event, replay result, and persisted deadline commit or roll back together,
leave/rejoin does not clear admission, and restart remains conservatively
protected. Announcement-policy, malformed-body, and role rejections consume no
cooldown. The in-process owner has no worker or timer and is separately tested
for bounded pruning, fail-closed capacity, competing reservation
serialization, rollback-on-drop, and backward monotonic observations. A
dormancy regression omits the production feature, sets a nonzero scalar, and
proves an explicitly feature-disabled build retains prior behavior without
creating admission state. Evidence is in
`docs/audits/omenchat-slow-mode-admission-qualification.md`.

The stopped-server scalar administration/status slice is covered by:

```bash
cargo test --locked --manifest-path src/server/Cargo.toml \
  --no-default-features --features server-headless slow_mode -- --nocapture
cargo test --locked --manifest-path src/server/Cargo.toml \
  --no-default-features --features server-headless \
  cli_parses_admin_config_and_room_commands -- --nocapture
cargo test --locked --manifest-path src/server/Cargo.toml \
  --no-default-features --features server-headless \
  cli_room_mutations_use_the_initialized_administrative_database_path \
  -- --nocapture
```

The command is:

```bash
omenchatd rooms set-slow-mode <room-id> off|<1..=86400> \
  --confirm --home <server-home>
```

Tests require missing confirmation/invalid bounds to fail, an active writer to
block the command, and a stopped-server update to report and persist its prior
and configured values. A no-op keeps the room revision; enable/disable changes
increment it once. Human and JSON room status expose the scalar and report
enforcement from the selected build identity (`active` in canonical server
profiles). Evidence for the pre-activation administration slice is in
`docs/audits/omenchat-slow-mode-administration-qualification.md`.

The bounded shared policy/client presentation slice is covered by:

```bash
cargo test --locked -p omenchat-protocol room_policy -- --nocapture
cargo test --locked --no-default-features --features desktop-product \
  slow_mode_projection -- --nocapture
cargo test --locked --no-default-features --features desktop-product \
  slow_mode_indicator -- --nocapture
cargo test --locked --no-default-features --features desktop-product \
  room_policy_projection_is_catalog_bounded -- --nocapture
cargo test --locked --manifest-path src/server/Cargo.toml \
  --no-default-features --features server-full dashboard_room -- --nocapture
```

The shared DTO rejects unknown policy bits and values above 86,400 seconds.
Client projection is keyed by session/room, capped by the existing 256-room
session ceiling, cleared on session removal/capability loss, and retains no
strings or payloads. The six-field value is accepted only for a negotiated
slow-mode session; legacy and announcement-only parsers still reject that
shape. Iced renders only a static label and omenchatd TUI reports the selected
build's enforcement status. Activation adds no worker, timer, polling
subscription, retry, queue, cache, or dependency. Pre-activation projection
evidence is in
`docs/audits/omenchat-slow-mode-client-projection-qualification.md`.

Native Linux Iced projection, admission, typed rejection, and exact draft
recovery are covered by the isolated Xvfb/i3 harness:

```bash
bash scripts/run-omenchat-slow-mode-gui-qualification.sh \
  --evidence /tmp/omenchat-slow-mode-gui-evidence
```

The harness requires Xvfb, i3, `xdotool`, `xprop`, `xclip`, ImageMagick
`import`, `jq`, `rg`, and Python 3. It creates fresh browser/server roots and
identities, uses only loopback Reticulum TCP, and leaves screenshots,
structured logs, the exact copied rejected draft, and a read-only SQLite
observation in the selected evidence directory. It requires one admitted
message and no second committed message. See
`docs/audits/omenchat-slow-mode-gui-qualification.md`.

The same isolated gate records optimized process and server-runtime
measurements when given a bounded nonzero sample duration:

```bash
OMENCHAT_SLOW_MODE_WARMUP_SECONDS=10 \
OMENCHAT_SLOW_MODE_SAMPLE_SECONDS=30 \
  bash scripts/run-omenchat-slow-mode-gui-qualification.sh \
  --evidence /tmp/omenchat-slow-mode-measurement
```

The measurement path currently requires Linux `/proc`. Durations of at least
30 seconds additionally assert one active Link, empty transport/event queues,
and clean server worker/queue drain. It emits raw per-process samples,
process/runtime summaries, both structured logs, shutdown latency, screenshots,
the SQLite observation, and the exact rejected draft. See
`docs/audits/omenchat-slow-mode-resource-qualification.md`.

Canonical product activation and the feature-disabled rollback boundary are
checked by:

```bash
bash scripts/verify-product-features.sh
cargo test --locked --no-default-features --features desktop-product \
  slow_mode_projection_is_bounded_and_follows_product_capability
(
  cd src/server
  cargo test --locked --no-default-features --features server-headless \
    slow_mode_product_feature_requires_durable_mutations_and_encodes_exact_shape
  cargo test --locked --no-default-features \
    dormant_slow_mode_setting_does_not_change_production_session_behavior
)
```

The verifier requires `omenchat-slow-mode` in all four canonical products and
rejects `omenchat-slow-mode-qualification`. The latter depends on the product
feature but owns only deterministic process-test hooks. See
`docs/audits/omenchat-slow-mode-activation.md`.

Schema-11 announcement-room storage and guarded rollback are covered by:

```bash
(
  cd src/server
  cargo test --locked --no-default-features --features server-headless \
    version_ten_database_adds_constrained_room_policy_and_slow_mode_storage \
    --lib -- --nocapture
  cargo test --locked --no-default-features --features server-headless \
    room_policy_schema --lib -- --nocapture
  cargo test --locked --no-default-features --features server-headless \
    schema_ten_export --lib -- --nocapture
)
```

These tests prove the ordinary default and SQLite constraint, transactional
rollback at every schema-11 fault boundary, readable pre-v11 backup,
confirmation-gated parsing, preservation of schema-10 moderation audit, removal
of only `policy_bits`, and cleanup after injected publication failure. The
operator command is:

```bash
omenchatd database export-schema10-copy \
  --to <new-database-path> --confirm --home <server-home>
```

omenchatd must be stopped. The destination must not exist. This does not
activate room policy, authorization, negotiation, or presentation. Evidence is
in `docs/audits/omenchat-announcement-rooms-storage-qualification.md`.

Announcement-room server authorization and stopped-server administration are
covered by:

```bash
(
  cd src/server
  cargo test --locked --no-default-features --features server-headless \
    announcement_room --lib -- --nocapture
  cargo test --locked --no-default-features --features server-headless \
    room_content_policy --lib -- --nocapture
  cargo test --locked --no-default-features --features server-headless \
    room_policy_update --lib -- --nocapture
  cargo test --locked --no-default-features --features server-headless \
    room_policy_maintenance_refuses_an_active_writer --lib -- --nocapture
  cargo test --locked --no-default-features --features server-headless \
    cli_parses_admin_config_and_room_commands --lib -- --nocapture
  cargo test --locked --no-default-features --features server-headless \
    cli_room_mutations_use_the_initialized_administrative_database_path \
    --lib -- --nocapture
)
```

These tests cover standard/trusted rejection, moderator publication, legacy
messages/actions/notices, durable message replay after a role change,
reactions, revisions, upload offer/publication boundaries, absence of event,
rate, replay-effect, pending-upload, file, and ledger side effects, atomic
policy/revision rollback, idempotency, restart, strict CLI vocabulary, and
dormant negotiation. Effective policy is available through:

```bash
omenchatd rooms list --json --home <server-home>
```

This does not claim negotiated room-policy evidence, GUI/TUI controls, process
traffic, or adjacent-version compatibility. Evidence is in
`docs/audits/omenchat-announcement-rooms-authorization-qualification.md`.

Current/current real-Link member authorization and restart persistence are
covered by:

```bash
bash scripts/release-omenchat-smoke.sh \
  --browser-bin target/debug/omenbrowser_rs \
  --server-bin src/server/target/debug/omenchatd \
  --tcp 127.0.0.1:<unused-port> \
  --path-wait 20 \
  --out <isolated-output-root> \
  --message "announcement policy qualification" \
  --announcement-rejection-smoke \
  --restart-server
```

The member-rejection mode performs one normal server start before stopped-server policy
maintenance. While that server is live, the harness first requires the policy
command to fail with the exclusive-maintenance refusal. It then stops the
server, applies policy, restarts, and requires typed policy error `1016` and no
committed message both before and after an orderly restart. The browser reuses its isolated
identity root and opens a new Link; each attempted message is explicit and no
uncertain mutation is resent automatically. Production capability negotiation
remains dormant, so this gate does not claim five-field catalog/delta traffic,
same-process capability recovery, moderator/resource process traffic, native
GUI observation, or adjacent-binary support for a capability older peers
cannot advertise. Evidence is in
`docs/audits/omenchat-announcement-rooms-process-qualification.md`.

Authorized moderator message and Resource publication, including role/policy
persistence across restart, are covered by:

```bash
bash scripts/release-omenchat-smoke.sh \
  --browser-bin target/debug/omenbrowser_rs \
  --server-bin src/server/target/debug/omenchatd \
  --tcp 127.0.0.1:<unused-port> \
  --path-wait 20 \
  --out <isolated-output-root> \
  --message "announcement moderator qualification" \
  --announcement-moderator-smoke \
  --upload-file fixtures/omenchat/v0_6_0_1_wire.rs \
  --restart-server
```

The harness registers exactly one isolated standard user while the room is
ordinary and proves live policy maintenance is refused, stops omenchatd,
identifies that user through the redacted headless
JSON command, applies confirmation-gated moderator role plus announcement
policy, and restarts. The unchanged client must observe its message echo,
upload completion, and fetched Resource, then publish another message after an
orderly restart. This still does not activate or claim negotiated room-policy
projection.

The expected-rejection wait short-circuits only for the typed announcement
restriction. It must continue collecting after unrelated operation errors; the
normal headless `--pin-smoke` process case is the regression gate for that
shared wait behavior.

Replacement-Link policy ownership is covered through the production lifecycle:

```bash
cargo test --locked --no-default-features --features desktop-product \
  announcement_policy_clears_on_replacement_link_and_requires_renegotiation \
  --lib -- --nocapture
```

This drives the production reconnect/retirement function with a captured
transport. It proves stale policy is cleared before reopening, is not inherited
from a replacement that does not accept the capability, and is restored only
after a fresh explicit request/accept. Real-Link replacement evidence is
recorded separately in the process qualification audit.

Server acceptance and the initial-catalog encoder boundary are covered
independently:

```bash
cargo test --locked --manifest-path src/server/Cargo.toml \
  --no-default-features --features server-headless \
  announcement_rooms --lib -- --nocapture
```

The production engine accepts only an explicit request and encodes
authoritative policy through the five-field shared room value. The test helper
keeps the same boundary independently controllable.

Per-Link mixed-format shaping is covered separately:

```bash
cargo test --locked --manifest-path src/server/Cargo.toml \
  --no-default-features --features server-headless \
  test_enabled_announcement_rooms_shape_join_and_delta_per_authenticated_link \
  --lib -- --nocapture
```

One authenticated Link negotiates policy while a simultaneous legacy Link
does not. The test requires five-field JoinAccept/RoomDelta values only on the
negotiated Link, four-field values on the legacy Link, and binding removal on
identity replacement.

The adjacent process matrix combines that exact shaping regression with both
directions of immutable `v0.9.6-3` live traffic and negotiated current/current
replacement-Link traffic:

```bash
bash scripts/run-omenchat-room-shape-compatibility.sh \
  --report target/omenchat-room-shape-compatibility.json
```

The strict current parser requires the adjacent server's legacy four-field
shape and records that no announcement capability or policy field was
projected. The adjacent parser is permissive, so its successful current-server
run is treated only as ordinary compatibility; the exact current-server
four-field claim comes from the per-Link shaping regression above. The
current/current process cases require negotiated five-field policy on both the
initial and replacement Links. The harness never fabricates capability support
for the adjacent peer. This is a long local/release-candidate gate and is not
part of the quick release check.

Production announcement-room feature identities are checked with:

```bash
cargo test --locked --no-default-features \
  --features desktop-product \
  live_open_requests_supported_durable_extensions_with_persistent_client_identity \
  --lib -- --nocapture

cargo test --locked --manifest-path src/server/Cargo.toml \
  --no-default-features \
  --features server-headless \
  announcement_room --lib -- --nocapture

bash scripts/verify-product-features.sh
```

The first two commands require request/accept behavior in canonical builds.
The product assertion requires canonical animated, static-media, headless, and
full server graphs to include `omenchat-announcement-rooms`.

Real-Link negotiation and replacement-Link qualification uses those canonical
binaries:

```bash
bash scripts/release-omenchat-smoke.sh \
  --browser-bin target/debug/omenbrowser_rs \
  --server-bin src/server/target/debug/omenchatd \
  --tcp 127.0.0.1:<unused-port> \
  --path-wait 20 \
  --out <isolated-output-root> \
  --message "negotiated announcement qualification" \
  --announcement-negotiation-smoke \
  --restart-server
```

The mode requires negotiated capability and authoritative room-policy
evidence, the exact local preflight error, no queued publication frame, and no
committed message in both reports. It does not call local queue admission a
server rejection. Run the ordinary `--announcement-rejection-smoke` against
canonical binaries separately to preserve typed server-error `1016`
enforcement evidence.

Native Linux GUI presentation was additionally observed with those
qualification binaries against an isolated loopback server under Xvfb/i3.
The connected Iced pane must show the announcement-room banner, retain a local
draft, and leave member Send/Attach actions inert. After an attempted Send,
the isolated server's shutdown counters must still report `chat:0`. Exact
setup, evidence boundaries, and limitations are recorded in
`docs/audits/omenchat-announcement-rooms-gui-qualification.md`.

Standard-member upload rejection before server acceptance or allocation is
covered by:

```bash
bash scripts/release-omenchat-smoke.sh \
  --browser-bin target/debug/omenbrowser_rs \
  --server-bin src/server/target/debug/omenchatd \
  --tcp 127.0.0.1:<unused-port> \
  --path-wait 20 \
  --out <isolated-output-root> \
  --message "announcement upload rejection qualification" \
  --announcement-upload-rejection-smoke \
  --upload-file fixtures/omenchat/v0_6_0_1_wire.rs \
  --restart-server
```

Both client reports must contain typed policy rejection with no upload
acceptance, completion, or committed upload event. Before and after the orderly
restart, the isolated server doctor must report zero tracked/disk upload files
and bytes, and the upload root must contain no regular file. The machine
doctor remains redacted; this local isolated harness uses the human detail
line rather than weakening that boundary.

The first moderation-audit slice reserves a read-only operation range and
bounded shared types without requesting or accepting
`moderation-audit-v1`:

```bash
cargo test --locked -p omenchat-protocol moderation_audit -- --nocapture
cargo test --locked --no-default-features --features desktop-product \
  moderation_audit --lib
(cd src/server && cargo test --locked --no-default-features \
  --features server-headless moderation_audit --lib)
```

These gates cover exact request/cursor/limit shape, the fixed action/result
vocabulary, forbidden extra fields, identifier/timestamp/display-name bounds,
known role/status bits, newest-first unique paging, item/owned-byte ceilings,
room scoping, independent byte-exact codecs, authorized inline/Resource page
equality, exclusive cursors and end markers, immediate role-loss denial,
authenticated Link-scoped capability invalidation, the bounded ephemeral
desktop projection, file-backed server restart, stable duplicate reads,
delayed Resource replay, oversized/invalid Resource rejection before pending
retention, invalid-page projection clearing, Resource-purpose validation, and
explicit production refusal to accept the dormant capability. Schema-10
storage, durable transaction coupling,
migration faults, and guarded schema-9/schema-8 exports are covered separately
in `docs/audits/omenchat-moderation-audit-storage-qualification.md`. These
commands do not claim a current/current process restart, adjacent-binary live
interoperability, active Reticulum Resource cancellation, UI presentation,
resource measurements, or activation. Paging
evidence is in
`docs/audits/omenchat-moderation-audit-paging-qualification.md`; wire evidence
is in `docs/audits/omenchat-moderation-audit-qualification.md`.

The explicit isolated moderation-audit measurements are:

```bash
cargo test --locked --no-default-features --features desktop-product \
  moderation_audit_projection_measurement --lib -- --ignored --nocapture
(
  cd src/server
  cargo test --locked --no-default-features --features server-headless \
    moderation_audit_retention_measurement --lib -- --ignored --nocapture
)
```

They exercise the configured client and per-room server ceilings using
temporary isolated state, print host observations, and remove their files.
They are not hardware-independent latency thresholds.

The moderation-audit cancellation assessment reuses the real loopback
Reticulum sender-cancel gate:

```bash
(
  cd src/server
  cargo test --locked --no-default-features --features server-headless \
    reticulum_loopback_resource_cancel_crosses_wire_and_production_bridge \
    -- --ignored --nocapture
)
```

It proves outbound initiator cancellation and production bridge cleanup, not
receiver-side cancellation of an inbound audit page. The locked
`reticulum-rs-transport 0.9.6` API has no public receiver-cancel operation.
Keep that distinction in test and release claims.

The non-product current/current moderation-audit process gate is:

```bash
bash scripts/run-omenchat-moderation-audit-qualification.sh \
  --report /tmp/omen-moderation-audit-qualification-report.json
```

It builds both roots with
`omenchat-moderation-audit-qualification`, promotes an isolated registered
identity to moderator, keeps a second identified target Link active, persists
and sends one durable mute, and requires the matching non-empty inline audit
row plus explicit `ModerationAuditEnd`. It orderly-restarts omenchatd and reads
the persisted row again with a stable server destination. It does not prove a
Resource page or inbound Resource cancellation. Canonical product profiles
reject this qualification feature.

Focused schema/storage gates:

```bash
(cd src/server && cargo test --locked --no-default-features \
  --features server-headless moderation_audit --lib)
(cd src/server && cargo test --locked --no-default-features \
  --features server-headless durable_active_peer_moderation_executes_once_for_each_action --lib)
(cd src/server && cargo test --locked --no-default-features \
  --features server-headless schema_eight_export --lib)
(cd src/server && cargo test --locked --no-default-features \
  --features server-headless schema_nine_export --lib)
```
