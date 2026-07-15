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

Run both independent lockfile advisory checks without treating advisory
warnings or a nonzero exit as success:

```bash
cargo audit --no-fetch
cargo audit --no-fetch --file src/server/Cargo.lock
cargo deny --locked --all-features check advisories
cargo deny --manifest-path src/server/Cargo.toml --locked --all-features \
  check advisories
```

The current root audit is expected to fail only on the two constrained
`quick-xml` 0.39.2 advisories documented in
`docs/maintenance/DEPENDENCY_SECURITY.md`. Any additional vulnerability is a
regression. The server currently has no vulnerability-class finding or allowed
warning.

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
