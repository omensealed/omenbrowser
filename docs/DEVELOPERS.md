# Developer Notes

## Main Crates

- Root crate: `omenbrowser_rs`.
- Standalone server crate: `src/server`.

`omenchatd` must remain movable and independent. Do not import browser modules
from the server crate.

The protocol-neutral IFAC TCP implementation is owned by
`src/server/crates/omen-ifac-tcp`; both products depend on that one local crate,
so a relocated `src/server` remains complete without duplicating transport
behavior. Run `bash src/server/scripts/verify-standalone.sh check` after changing
server path dependencies, source includes, or fixtures.

## Feature Flags

Common browser build:

```bash
cargo build --locked --no-default-features --features desktop-product
```

Use `desktop-dev` instead when a local development run explicitly needs the
mock runtime. Cargo defaults are empty so development/test support cannot leak
into product artifacts.

The dependency-free root build script records the target triple and source Git
commit in `--version`. It prefers `OMENBROWSER_GIT_COMMIT`, then `GITHUB_SHA`,
then the current checkout. Set the explicit variable when building a release
from an exported source tree without `.git`; the release gate rejects an
unknown product identity.

`desktop-product-static-media` retains live networking and OMENchat but excludes
the optional `iced_gif` decoder/widget. Keep GIF-specific code behind
`chat-client-gif`; the static image fallback must continue to compile and pass
its cache-worker regression.

OMENbrowser desktop icons come from the curated private-use glyph constants in
`src/desktop/icons.rs` and the system Nerd Font detection/fallback in
`src/desktop/fonts.rs`. The application does not use `iced_fonts` icon APIs.
`desktop-widgets` therefore enables `iced_aw` without selecting the unused
Lucide, Nerd, or Codicon bundles. Do not add a complete icon font when a curated
existing glyph or small accessible asset is sufficient.

Iced-adjacent dependency admission is recorded in
`docs/maintenance/ICED_CRATE_ADMISSION.md`. The canonical animated product uses
only `iced` and the bounded in-memory `iced_gif` path; the static-media product
uses only `iced`. Run `bash scripts/verify-product-features.sh` after any UI
feature or dependency change. Dormant widget/drop/animation/table features are
not authorization to add them to a product alias.

`chat-client-reticulum` is the canonical 0.9 migration path. It uses the
clean `reticulum-rs`/`lxmf` 0.9 stack. `chat-client-rns-clean` remains as a
compatibility alias for older local commands. The old `rns-net` compatibility
crates are no longer part of the normal manifests.

Server live RNS build:

```bash
cargo build --locked --manifest-path src/server/Cargo.toml \
  --no-default-features --features server-full
```

## Checks

```bash
cargo fmt --check
cargo check --locked --no-default-features --features native-lxmf
cargo clippy --locked --lib --no-default-features --features native-lxmf -- -D warnings
cargo test --locked --no-default-features --features desktop-product
cargo fmt --manifest-path src/server/Cargo.toml --check
cargo test --locked --manifest-path src/server/Cargo.toml \
  --no-default-features --features server-full
```

Fast pre-share gate:

```bash
bash scripts/release-check.sh quick
```

Optimized cache-index comparison (isolated generated fixtures only):

```bash
bash scripts/measure-cache-indexes.sh
```

Optimized runtime thread-policy comparison (isolated generated files only):

```bash
bash scripts/measure-runtime-threads.sh
bash scripts/measure-runtime-threads.sh --two-core
```

The browser selects one async worker per available CPU, capped at four. This
preserves the measured high-core behavior while avoiding oversubscription on
one- and two-core systems. The eight-thread Tokio blocking ceiling is a safety
backstop, not workload backpressure; file writes, media decoding, and SQLite
work retain their smaller explicit semaphore limits.

## Development profile

The standard Cargo development profile compiles third-party packages at
`opt-level = 1`. This is deliberate: the graphics, image, crypto, and networking
stack remains usable for interactive debugging without the previous blanket
level-3 compile cost. Application code remains at Cargo's normal unoptimized
development setting, preserving useful debugging behavior. Release and
packaging commands continue to use `--release` and are unaffected.

## Source Areas

- `src/msgpack.rs` - allocation-free root MessagePack structural preflight
  shared by browser chat, native requests, and native LXMF. Keep exact/next,
  depth, trailing-data, reserved-marker, and truncation coverage synchronized.
- `src/protocol_limits.rs` - feature-neutral root wire ceilings shared by chat
  and native transport paths. The standalone server intentionally owns its own
  equivalent limits and scanner; it must not import the browser crate.
- `src/runtime/bootstrap.rs` - the application Tokio runtime policy and builder;
  keep its adaptive worker count, eight-thread blocking backstop, and worker
  name synchronized with measurements and entrypoint tests.
- `src/cli_frontend.rs` - dependency-free typed recognition for frontend,
  help, and version tokens. Complex native-network parsing remains at the
  compatibility boundary until its validation can move intact.
- `src/cli_help.rs` - stable compatibility CLI help consumed by the browser
  binary; keep parser spellings, safe secret-input guidance, and output tests
  synchronized.
- `src/cli_network.rs` - typed command-local TCP client override, endpoint
  validation, credential-preserving option ordering, and redacted debug output.
- `src/cli_overrides.rs` - private command-local runtime/path/network override
  aggregate with explicit borrowed and consuming access; its `Debug` output
  must redact paths and nested credentials.
- `src/cli_redaction.rs` - pure argv, override-snapshot, path-hint, and persisted
  log-message sanitization shared by native reports and diagnostic bundles;
  preserve its compatibility schemas, markers, and truncation order.
- `src/cli_report_logs.rs` - bounded regular-file discovery, tail loading,
  recent-entry selection, and redaction for diagnostic bundles. It delegates
  filesystem policy to the shared reader; preserve the fixed report ceilings
  and path-free collection counters.
- `src/cli_secret.rs` - browser passphrase-source preprocessing with bounded
  input and owner-only regular-file enforcement. Gateway and standalone-server
  CLIs keep their distinct public error contracts until deliberately migrated.
- `src/cli_values.rs` - pure typed parsing for runtime backend and LXMF smoke
  delivery values; preserve compatibility aliases, normalization, and exact
  errors when changing the command parser.
- `src/product_identity.rs` - stable compiled-feature and version identity used
  by the CLI, smoke tests, and packaging gates. Keep its names and ordering
  synchronized with release consumers.
- `src/identity.rs` - identity material admission and managed-profile discovery.
  Every production identity reader must use the shared non-empty, regular,
  non-symlink 64 KiB-capped reader. Import, export, and pre-overwrite backup
  must operate on its admitted byte snapshot instead of reopening or copying a
  mutable source. Discovery must refuse a linked/non-directory root, inspect at
  most 4,096 entries, retain at most 256 regular profiles, and never follow an
  entry link. Identity writes must use private synchronized same-directory
  staging. Creation and backup publication are no-clobber; import replacement
  requires a published backup first. Managed retention recognizes only the
  current application-owned backup namespace, keeps 16 files/1 MiB, scans at
  most 4,096 entries, and leaves legacy or ambiguous material untouched. Keep
  cryptographic format validation in the native provider; this shared boundary
  controls filesystem, durability, and allocation safety.
- `src/storage/settings.rs` - application settings persistence. Load only a
  regular, non-symlink file of at most 8 MiB and cap the actual read at
  limit+1 so metadata races cannot bypass admission. Missing files retain the
  default path; malformed bounded files publish the exact already-read bytes
  through a unique owner-only synchronized sibling backup before returning
  defaults. Publication must remain no-clobber; never reopen or copy the source
  path after parse failure. Retain only the newest four regular backups and
  32 MiB total; prune before and after publication, inspect at most 4,096
  directory entries, and never follow a matching symlink. Unsafe/
  special and oversized paths fail explicitly without being copied or followed.
  Before Serde, retain an allocation-free fixed-stack scan over the admitted
  bytes: depth 48, 262,144 structural tokens, 8,192 items per container, and
  4 MiB per raw string token. The scan is a resource preflight, not a replacement
  JSON parser; Serde still owns grammar and typed decoding. Load and save must
  share the structural limits so the application cannot publish a payload its
  next startup rejects.
  Save must reject serialized output above the same limit
  before creating a staging file. Accepted output uses a unique owner-only
  create-new sibling, flush/file synchronization, shared cross-platform atomic
  replacement, and Unix parent-directory synchronization. Refuse an existing
  symlink or non-regular target before staging; every pre-commit failure must
  remove the sibling and preserve the prior target bytes. After deserialization,
  validate every retained collection and recursive layout/flattened-extension
  value as one unit before constructing application state. Browser history and
  focused-link fields must reuse the live browser/Micron budgets. Semantic
  rejection must publish the exact admitted source bytes and return complete
  defaults; never trim or partially restore settings. Apply the identical
  validator before save so the application cannot publish state that startup
  rejects.
- `src/storage/transient_ids.rs` - native LXMF duplicate-delivery persistence.
  Preserve both the versioned `{ "ids": ... }` JSON and legacy bare-map input,
  the six-month policy, the 65,536-item high-water/90% low-water behavior, and
  the 8 MiB file ceiling. Admit only regular non-symlink files with a capped
  read and stable Unix file identity. Malformed admitted bytes must be backed
  up exactly through a private synchronized no-clobber sibling without moving
  the source; retain four current-namespace backups/32 MiB under a 4,096-entry
  scan ceiling. Saves validate 64-hex-byte IDs and finite timestamps before
  private synchronized atomic replacement. Unsafe or oversized sources fail
  without backup or mutation.
- `src/storage/form_state.rs` - browser form-state persistence. Preserve current
  and legacy page JSON, age pruning, the 512-page/4 MiB store bounds, and the
  existing per-URL/field semantic limits. Load only a stable regular non-symlink
  file through a 4 MiB+1 capped read. Malformed admitted bytes must be backed up
  exactly through a private synchronized no-clobber sibling without moving the
  source; retain four current-namespace backups/16 MiB under a 4,096-entry scan
  ceiling. Saves use private synchronized atomic replacement and reject unsafe
  targets before staging. Every fallible mutation must restore its previous
  in-memory state when persistence fails.
- `src/directory.rs` - saved/discovered directory state and live-announce
  persistence. Preserve numeric trust compatibility, debounce/cooldown,
  transient aging, the 256-item announce stream, and preferred-delivery/
  identify semantics. The store admits a stable regular non-symlink file up to
  8 MiB and at most 4,096 retained entries. Live mutation must enforce the
  1 KiB destination/associated-hash and 16 KiB display-name limits before
  retention. Malformed or semantically excessive admitted bytes use exact
  private synchronized no-clobber backup publication; retain four current
  backups/32 MiB under a 4,096-entry scan ceiling. Saves use private synchronized
  atomic replacement, and every immediately persisted entry/clear mutation
  must restore memory if commit fails.
- `src/interfaces.rs` - browser interface profiles, bundled gateway presets,
  and generated Reticulum configuration. Preserve the accepted JSON/config
  formats and identity-preservation behavior. Profiles admit only a stable
  regular non-symlink file up to 2 MiB, 64 profiles, and 64 peers per profile;
  presets use a 1 MiB/256-entry ceiling; existing generated config is capped at
  1 MiB. Text fields reject CR/LF/NUL configuration injection, use the shared
  semantic byte ceilings, and never expose passphrases through `Debug`.
  Persistence uses unique owner-only same-directory staging, file and Unix
  directory synchronization, and shared atomic replacement. Every immediately
  persisted profile mutation must restore memory on commit failure. Legacy
  preset migration must validate the admitted snapshot and leave its source
  untouched.
- `src/structured_log_reader.rs` - shared bounded regular-file discovery and
  tail parser used by startup and report bundles; do not replace it with
  whole-file reads or follow matching symlinks.
- `src/structured_log_writer.rs` - normalized browser log disk policy plus the
  dedicated 256-record/2 MiB non-waiting writer, bounded rotation/pruning,
  regular-file checks, flush/shutdown controls, and persistence counters shown
  by both Diagnostics and Logs without filesystem polling or recursive logging.
- `src/micron` - Micron parser/rendering. Parsed link actions admit at most
  96 KiB of raw syntax, a 16 KiB label, an 8 KiB exact target, and 128 forwarded
  fields with 4 KiB per field/64 KiB aggregate. Standard, shorthand, and LXMF
  autolinks share the target policy. Rejected syntax remains visible but
  explicitly non-actionable, including embedded autolink-looking text. Field
  controls admit at most 72 KiB raw syntax, four descriptor parts, a 256-byte
  name, a 64 KiB value/label, and width 1..=256. A document retains at most 128
  controls/4 MiB of control strings. Browser session mutation applies the same
  name/value/item/aggregate policy before form state or request forwarding.
  Desktop field editing preflights keyboard/paste and full-value updates against
  that session policy before changing `InputState`; rejected edits are atomic,
  preserve the previous draft/session value, and report the field limit.
  Rendered cells share immutable link actions and the payload-bearing
  name/kind/value strings of control references. Keep hit actions owned at the
  activation boundary; do not restore per-cell payload clones when changing
  canvas, wrapping, hit-region, or capture code.
  Core documents retain at most 16,384 rows, reject individual source lines
  above 256 KiB, and retain at most 64 metadata entries/64 KiB of metadata
  strings (256-byte keys, 4 KiB values, and 16-byte style values). A document
  with any dropped content sets `limits_applied` and ends with a visible,
  non-actionable notice. Core and top-level MicronPlus rendering clamp requested
  width to 4,096 cells and retain at most 65,535 rows/1,048,576 cells, reserving
  capacity for a visible render-limit notice. Preserve streaming collection;
  do not collect an unbounded intermediate row vector before applying budgets.
  Authored inline content retains at most 65,535 fragments plus the fixed limit
  notice and 4 MiB total span text including that notice. At most 4,096 link
  actions/4 MiB of target and forwarded-field strings remain actionable.
  Over-budget actions are demoted in place to visible non-link spans; never hide
  their labels or reactivate them through a later autolink pass.
  Rendered cells store styles as shared immutable allocations: one style per
  authored span/control run and one process-wide default for generated plain
  cells. Post-render emphasis and document-default link coloring must use
  `Arc::make_mut` so a changed cell receives copy-on-write isolation instead of
  leaking style changes across the run.
- `src/browser` - browsing, cache, partials, MicronPlus helpers.
  `BrowserPage` is admitted before parsing, caching, or session installation:
  URL/title/markup are capped at 8 KiB/16 KiB/4 MiB; metadata has 64 top-level
  entries, 16 MiB aggregate key/string storage, 4 MiB scalars, and explicit
  container/value/depth limits; request data has 128 entries/4 MiB with
  256-byte keys and 64 KiB values. Normalization validates both the received
  page and MicronPlus-derived metadata. Restore and direct-application APIs are
  fallible, and partial composition commits its content only after the complete
  candidate page passes admission. Do not add a page entry path that bypasses
  `BrowserPage::validate_retained`.
  Navigation history retains at most 512 URLs and 1 MiB of URL strings, with
  the existing 8 KiB per-URL page limit. Live navigation truncates the forward
  branch and evicts the oldest prefix so the newest edge remains available.
  Restore retains one contiguous bounded window around the saved pointer;
  invalid adjacent entries terminate that side instead of allowing navigation
  to skip across an unretained edge. Keep restore admission ahead of page-state
  mutation, and reject an oversized resolved URL before runtime dispatch.
  MicronPlus parsing preflights source at 4 MiB/16,384 lines/256 KiB per line
  and attributes at 64 items/128 KiB. Widget trees retain at most depth 32,
  8,192 nodes, 512 columns, and 8 MiB strings; typed layouts retain at most 64
  windows, 256 groups, 512 columns, and 8 MiB strings. Use the fallible
  `try_parse_micronplus_tree` and `try_extract_micronplus_layout` at retained
  boundaries. Partial tree/layout changes must remain transactional and
  preflight repeated-slot multiplication before cloning fragment content.
  Per-tab widget stores retain at most 256 widgets, 4,096 items, and 4 MiB;
  each widget retains at most 1,024 items/1 MiB. Append events keep the newest
  edge within the target widget, while set/status changes reject atomically.
  Item markup must also fit the aggregate 8,192-node/512-column render
  augmentation budget. Widget-event extraction retains at most 256 events/
  1 MiB and leaves rejected event lines visible. Control-event history keeps
  the newest 256 events/2 MiB. Preserve `MicronPlusWidgetStore::metrics` and do
  not bypass `apply_event` by exposing mutable widget state.
  MicronPlus live/input/button field attributes use the Micron link field
  ceilings before constructing retained vectors; rejected controls remain
  literal and non-actionable. Partial descriptors use fallible admission with
  the same 96 KiB raw, 8 KiB target, and 128-item/64 KiB field policy, plus a
  256-byte partial ID. Extraction retains at most 256 specs/1 MiB and skips
  invalid descriptors rather than scheduling a modified request.
- `src/messaging` - LXMF conversations and store. Persisted threads admit only
  regular non-symlink files of at most 8 MiB and 4,096 messages. Discovery is
  bounded to 4,096 entries, 256 threads, and 64 MiB; filesystem-unsafe peer
  keys must use the deterministic contained filename mapping. Preserve private synchronized atomic
  publication, exact admitted-byte corruption backups, and four-file/32 MiB
  recognized-backup retention. Never reopen a malformed source or prune legacy
  or ambiguous material. Import must remain no-clobber and capacity checked.
- `src/runtime` - runtime abstraction and native networking. Native LXMF file
  attachments preserve the Python-compatible field layout while enforcing 64
  items, 8 MiB per item, 16 MiB aggregate, and 4 KiB names. Outbound sources
  must use the capped stable regular non-symlink reader. Inbound publication
  uses deterministic per-message paths, private synchronized same-directory
  staging, and must refuse linked/non-directory roots and linked/non-regular
  destinations. Do not restore whole-file reads, replay suffix proliferation,
  or direct truncating writes. Active clean direct, resource, and propagation
  receive paths move decode plus attachment I/O through one two-job blocking
  gate before awaiting the result. Cancellation while waiting retains no
  permit; cancellation after dispatch lets the blocking closure finish while
  retaining its permit, so no third job is admitted. Keep synchronous codec
  entry points for compatibility tests and explicitly non-async callers only.
- `src/chat` - OMENchat client plugin. Each session history view is bounded to
  1,024 events and 8 MiB of estimated owned storage. Cache restore/live append
  retains the recent edge; load-older retains the older edge. SQLite remains
  authoritative for evicted rows. The client retains at most 64 sessions; each
  room catalog is bounded to 256 items/512 KiB and each active-room user catalog
  to 1,024 items/1 MiB. Live, mock, desktop, and SQLite restore all pass through
  the same admission policy. Live outgoing upload offers retain at most four
  items/16 MiB; inline download assembly retains at most 16 declared
  resources/16 MiB, 8 MiB per resource, and 1,024 pending fragments per
  resource. Session close/reconnect releases transfer ownership. Presentation
  metadata is admitted separately from the 512 KiB codec scalar ceiling:
  operational labels are rejected above their semantic limits, display-only
  text is UTF-8-safely shortened, and SQLite filters invalid byte lengths before
  materialization. Declarative descriptors are preflighted at 64 KiB/128 lines,
  their room/capability lists are item- and byte-bounded, and Micron link fields
  use an atomic 32-item/16 KiB admission before session creation.
- `src/plugins.rs` - typed plugin manifests and bounded discovery. Startup scans
  at most 4,096 directory entries, retains at most 256 installed candidates,
  reads manifests through a 64 KiB regular-file cap, and reads/writes the
  registry through a 1 MiB cap. Overload is reported in discovery warnings;
  manifest and registry symlinks are not followed. Registry saves use unique
  owner-only create-new staging, file synchronization, atomic replacement, and
  Unix parent-directory synchronization; pre-commit failure preserves the
  previous valid file and removes staging. Confirmed folder installs
  accept at most 1,024 entries, 64 MiB total, 16 MiB per file, and 16 directory
  levels. They refuse symlink/special entries, synchronize copied files in a
  hidden same-filesystem staging directory, atomically rename the complete tree
  into place, and remove staging after any pre-publication failure. Destination
  checks use no-follow metadata, so an existing broken link is never replaced.
  Bounded startup discovery removes only safely encoded reserved install-stage
  directories left by an interrupted copy; a complete published tree missing
  registry metadata is instead registered disabled and untrusted. Removal
  refuses links/non-directories, atomically moves the tree into a reserved
  hidden quarantine, commits registry removal, and only then deletes files.
  Discovery restores pre-commit crash leftovers when registry ownership remains
  and finishes post-commit cleanup when it does not.
- `src/server` - standalone `omenchatd`.
- `src/desktop` and `src/ui` - Iced UI shell and widgets.

## Dependency Policy

Prefer published crates. Do not vendor or patch networking crates in this
repository unless there is an explicit release-blocking reason and a follow-up
plan to upstream or remove the patch.
