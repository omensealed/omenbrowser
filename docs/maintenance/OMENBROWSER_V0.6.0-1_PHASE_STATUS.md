# OMENbrowser v0.6.0-1 implementation ledger

Updated: 2026-07-15. Reviewed baseline and current starting HEAD:
`v0.6.0-1` / `ce3a964ce93dd207d02b28ac3c2a0b2c42faf291` on `main`, with no
divergence and a clean starting worktree. This ledger summarizes, rather than
replaces, the immutable review in `official-sources/`.

Status terms are: `confirmed`, `partially addressed`, `already fixed`, `not
reproducible`, `superseded`, and `not yet inspected`.

## Findings

| ID | Status | Evidence and symbol | Proposed unit; tests/measurement; affected surface; rollback and gate |
|---|---|---|---|
| F-001 | already fixed | `Cargo.toml` has empty defaults plus explicit `desktop-{product,dev,test}` aliases; `build.rs` and `product_identity.rs` emit commit/target/profile/feature identity; release/package/native-CI commands use locked, no-default product builds; `verify-product-features.sh` rejects mock, test/debug, and legacy features | Positive graph checks and deliberate mock/Iced-debug injections prove the machine gate; compiled `--version` proves the canonical profile, exact checkout commit, target, and mock-off state. Release checks reject unknown commit/target identity. No runtime dependency, wire, storage, or config change. Roll back aliases, identity build script, assertions, commands, and docs together. Completion gate met: product identity is deterministic and contains no mock/test/legacy path. |
| F-002 | already fixed | `src/server/src/reticulum_live.rs`: bounded payload/control channels, exact-oldest `QueueBudget`/`QueuePermit` accounting and `queue_metrics`; `reticulum_live_soak_tests.rs`; no `UnboundedSender` remains in the live server | Item/global-byte/per-link-byte limits, priority control lanes, explicit outbound overload, and observable depth/bytes/actual-oldest/rejects are enforced. The 60 s release soak made 60,000 resource attempts per lane against 2,851/2,852 consumptions (over 21x), rotated eight links, completed 601 reconnect controls per lane at 21 ms maximum, held transport/events at <=16/32 MiB, recorded 56,900/56,642 rejects, kept FDs at 11, and limited peak RSS delta to 55,377,920 bytes versus the 117,440,512-byte budget-plus-margin gate. Cancellation returned all queue items/bytes to zero. No wire/config/dependency change. Roll back queue wrapper/accounting/tests/harness/docs together. Completion gate met for production queue backpressure; wire interoperability remains separately qualified. |
| F-003 | already fixed | `server/upload.rs` uses keyed identity locks, same-directory create-new temp, flush/fsync/rename, new-row commit, physical eviction, then successful-row cleanup. Old rows remain until their files are removed | Injected boundary failures, concurrent quota, ordering, permission, symlink and persistent restart tests pass. Linux `/dev/full` supplies real kernel ENOSPC (28) through the write path; the database callback is not reached and the prior committed upload remains the only identity file. No wire/config change; schema v2 belongs to F-023. Roll back upload/session/store changes together only with storage-safety review. Completion gate met: every tested interruption preserves or conservatively over-counts the last committed state. |
| F-004 | partially addressed | `.github/workflows/{ci,package}.yml`: actions pinned to reviewed full SHAs; workflow/build authority defaults to `contents: read`; dependent `release` environment publish job alone receives `contents: write`; AppImageTool 1.9.1 is immutable and SHA-256 verified; `scripts/verify-workflow-security.sh` enforces these invariants | Static assertion, `actionlint`, fresh upstream asset digest, and local release quick gate pass. Repository administrators must configure required reviewers on the `release` environment, then a real tag run must prove artifact handoff/publication before `already fixed`. CI/config only; no runtime dependency. Roll back workflow/security-script/docs unit together. Gate: reviewed environment policy plus successful unprivileged build and approved publish run. |
| F-005 | already fixed | The unconditional desktop `Tick`, its one-second subscription, and debug counters are removed. `InternalEventSender` supplies immediate bounded-queue delivery; scroll and monitoring subscriptions exist only while active; persistence, LXMF reconciliation, browser partials, and OMENchat maintenance use keyed nearest-deadline one-shots. Conversation/OMENchat follow-bottom checks run at handled mutation and internal-event boundaries | Identical isolated 60 s warmup/600 s software-rendered Linux runs compared archived `ce3a964` (`--no-default-features --features chat-client-reticulum`) with the current canonical product. Recurring idle application messages fell 60->0/min (100%); median CPU fell 1.963%->0.000%; `perf stat` task-clock fell 14,301.54->4,600.45 ms (67.83%); median RSS fell 216,740->179,754 KiB. P95 CPU fell 3.908%->2.940%, while the explicitly non-equivalent scheduler context-switch proxy fell only 9.99%. Deadline/event regressions and normal-close checks pass. No config/wire/dependency change. Roll back subscription/message/deadline changes together. Completion gate met: application updates fall >=80%, median CPU is <=1%, and actual events retain immediate bounded delivery. |
| F-006 | already fixed | No recurring `update_tick` exists. `workspace_persistence.rs::reconcile_workspace_panes_after_target_mutation` is reachable only from browser-tab, conversation, and OMENchat-session removal boundaries | The focused stale-target mutation regression proves exact removal. A separate 30 s Linux `perf record -g` idle capture retained 350 current-product samples with zero losses; neither `update_tick` nor target-validation/reconciliation appears, matching the static call-site inventory. The profiler run is separate from authoritative CPU sampling. No config/wire/dependency change. Roll back mutation helper/call sites/tests/docs together. Completion gate met: idle control flow contains no validation scan and mutation tests cover cleanup. |
| F-007 | partially addressed | Remote GIF reads plus inbound-resource writes/local-source copies decode behind two bounded blocking permits and typed Iced completions. GIF admission and decoded cache are byte/item bounded; pending cache work is 16 jobs/16 MiB. Per-key generations reject stale completions; replacement/session close signals cooperative cancellation through worker read, policy, decoder boundary, prune, and publication stages. UI media status is bounded to 256 items/256 KiB metadata. Identity-scoped disk media is bounded to 64 files/128 MiB. Exact workspace/maximized-pane visibility withholds GIF frame handles from hidden panes. A persisted, default-off reduced-motion preference now applies at the same boundary, withholding frame handles from visible panes and retaining their static fallback. Seven named adversarial cases plus 512 deterministic mutations exercise the production decoder boundary | Policy/cache eviction, valid fixture, panic containment, queue saturation/release, sparse oversize rejection, failed-decode cleanup, worker-write, cancellation token, metadata/disk eviction, visibility/reduced-motion, and deterministic corpus tests pass without unwind. The monolithic third-party decoder cannot be preempted inside its call. Animated product library tests pass 899/901 with two documented measurement ignores; static media passes 896/897 with one. The native four-phase/GPU capture remains pending; its isolated harness and release decoder micro-measurement are reproducible. The new JSON field defaults compatibly; no wire/new crate change. Roll back the preference/message/Settings control/predicate/tests/docs together without affecting the existing visibility boundary. Gate: native measurement confirms hidden and reduced-motion panes submit no animation work and bounded RSS. |
| F-008 | already fixed | Native NomadNet response parsing rejects MessagePack above 4 MiB before its second value tree. SOCKS media streams under 8 MiB with bounded client/redirect policy. Live OMENchat completed resources are single-consumer, capped at 8 MiB each and 16 items/16 MiB retained per link; deferred offers are 32 items/4 MiB. Desktop per-link transport queues cap inbound and outbound frames at 64 items/4 MiB each and outbound resources at four items/16 MiB. The 256-item internal application channel gives frame/resource/close payloads a shared 32 MiB permit carried through async wait and deferral until handling. Post-channel staging moves payloads and caps frames at 256/16 MiB, resources at 16/32 MiB, and closes at 256 items/256 KiB. Monitoring exposes current items/bytes and rejects. The clean bridge rejects explicit frame/resource completions above 1/8 MiB before application-event forwarding | Native response, HTTP policy, every owned desktop transport/channel/staging item/byte/oversize/replay/release boundary, retained-resource/deferred-offer budgets, and clean metadata limit-selection tests pass. Channel tests prove exact 32 MiB saturation/release and permit cleanup on item-full rejection. Pinned transport source proves a 64 MiB/8,192-part global inbound cap and progress totals, but its public cancel method is outbound-only; earlier receiver cancellation requires upstream API work. No new crate/version/wire/config change. Roll back the channel envelope/budget/monitoring/tests/docs together; other boundaries remain independent. Completion gate met: every hop/resource/cumulative byte cap is tested before allocation where the dependency API permits. |
| F-009 | already fixed | Every local raw `rmpv` decode has an allocation-free pre-scan; pinned `lxmf-wire` 0.6.0 unbounded paths are guarded. The separately locked fuzz package uses cargo-fuzz 0.13.2/pinned nightly with explicit max-length seeds; a non-shipping counting allocator measures decoder-only rejection | Client/server each completed 10,000 mutation runs without sanitizer findings plus explicit 4,194,305-byte seeds. Release ranges: valid frames 296–374 ns/320-byte peak; declared oversize 58–74 ns/33 bytes; actual over-limit buffer 47–68 ns/31 bytes; batch oversize 216–275 ns/81 bytes. Accepted encoding/runtime dependencies are unchanged. Roll back scanners/preflight/fuzz/measurement/docs together only with security review. Completion gate met: hostile declared lengths do not control allocation, rejection is faster/smaller than valid decode, and sustained sanitizer runs pass. |
| F-010 | already fixed | Client and standalone-server batches enforce identical 4 MiB compressed/uncompressed limits. Advertised output rejects before decode; bzip2 reads through `take(expected + 1)` into validated capacity and requires an exact match; envelope/value MessagePack is pre-scanned | All fault/round-trip tests pass. Counting-allocator release ranges: valid 64 KiB batch 184–197 µs/196,902-byte peak; advertised oversize 38–40 ns/37 bytes; stream expanding beyond 4 MiB while claiming one byte 267–273 µs/8,280 bytes. No config or accepted wire change. Roll back both batch implementations/tests/measurement/docs together only with security review. Completion gate met: output allocation is independent of compressed expansion and bomb rejection time is bounded. |
| F-011 | partially addressed | Store PRAGMAs, transactional IDs, schema versioning, SQLite-consistent migration backup, and confirmation-gated restore are implemented. Restore accepts only a regular generated sibling with matching older `user_version`, refuses active WAL/SHM and corrupt/current inputs, migrates/checkpoints/integrity-checks a private stage, atomically publishes it, and retains the prior active database. `LiveServerWorker` gives live traffic single-admission bounded blocking isolation and metrics. `AdminDatabase` owns CLI room operations, line-console room/user administration, interactive-dashboard room/moderation operations, and upload-ledger inspection/repair on one named thread per process; its 16-item queue rejects overload without waiting and exposes queue/in-flight/completion/rejection/latency metrics. Interactive responses have six-second deadlines; confirmed offline repair instead waits for a definitive result so it cannot time out before a later commit. Doctor uses read-only mode and repair uses existing-current-schema mode. Dashboard room metadata is cached at 1,024 items/1 MiB and user metadata at 4,096 items/2 MiB, refreshed asynchronously only for visible consumers. Stale-user batches delete membership/user rows in one transaction | PRAGMA, 12-writer, migration/future-version/restore/refusal/publication-failure, live-worker saturation/lock/load, administrative actor saturation/drain and typed user lifecycle, persistent line-console mixed-command ownership, CLI room lifecycle, asynchronous TUI room/moderation lifecycle/audit, both cache bounds, read-only/repair actor-mode upload tests, real writer-lock responsiveness, and actual child-process termination pass. Event kills cover committed/open transactions. Upload kills cover synchronized temp, durable rename, committed-ledger/pre-eviction, and physical-eviction/pre-cleanup boundaries; restart either retains a clean committed state or blocks admission with explicit orphan/missing evidence, and confirmed repair removes only the stale missing row. Every reopened database passes `integrity_check`. Under `BEGIN IMMEDIATE`, room and moderation mutation admission return within the 50 ms test gate and complete after release. The 60 s live soak remains 6,000 commits/42,000 explicit rejects with 1,272 us worker and 1,817 us heartbeat maxima. Actual Reticulum wire load remains. Schema/config/wire/dependencies are unchanged. Roll back restore command/module/tests/docs independently; actor/console/TUI/maintenance integrations, process-kill tests, and live worker also remain independently removable. Gate remains native/live evidence. |
| F-012 | partially addressed | Typed versioned TOML rejects unknown/malformed/future/unsupported values while preserving supported version-0 flat keys. `save_rendered_with_rename` reparses before disk I/O, atomically writes private synchronized same-directory temp/backup/target files, retains `config.toml.bak`, refuses invalid existing data, cleans pre-commit failures, and syncs the directory on Unix | Strict parsing, special-string round-trip, owner-only target/backup, invalid-existing refusal, and injected final-rename preservation tests pass. Acceptance gate is met on Linux. Native Windows replace-existing behavior and post-rename power-loss testing remain before `already fixed`. Config metadata is version 1; values/defaults/wire unchanged. `serde` derive and `toml` 0.8 remain the admitted parser dependencies. Roll back reader/writer/version/docs together while retaining version-0 compatibility and any operator backup. Gate: native platforms prove failed replacement preserves the previous valid config. |
| F-013 | partially addressed | Browser passphrase preprocessing lives in `cli_secret`; private-field `cli_network::TcpClientOverride` owns endpoint parsing/credentials, private-field `cli_overrides::SmokeOverrides` owns the complete command-local aggregate, `cli_redaction` owns pure argv/override/path/log-message sanitization, and `cli_report_logs` owns bounded regular-file discovery/tail parsing. Override types implement redacted `Debug`. Bundled gateway and standalone server retain distinct resolver/override contracts. All three provide owner-only files, bounded stdin, hidden prompt, and source conflicts. Browser UI redaction and atomic `0600` repairs remain | Tests cover exact byte/encoding/value/file boundaries, endpoint/error/ordering behavior, consuming ownership, exact sanitized argv/JSON schemas, all protected log paths, active passphrases, message-body suppression, Unicode-safe truncation, nested credential/path `Debug` redaction, and isolated bundle output. Persisted bundle logs scan <=4,096 directory entries, select <=8 regular non-symlink files, read <=512 KiB/file and <=2 MiB total, retain 50 entries, and emit path-free counters. Normal documented paths avoid secrets on argv; legacy `--passphrase` warns. Native Windows/macOS prompt/ACL behavior, external collectors, in-memory lifetime, and deliberate three-CLI unification remain. No dependency/config/wire change. Roll back each CLI module delegation/test/doc slice independently. Gate: native OS tests prove no secret in logs/reports/process arguments and platform ACL expectations. |
| F-014 | partially addressed | `desktop/ui_state.rs::ShutdownPhase` models `Running -> ShutdownRequested -> Draining -> Closed`; `desktop/update.rs` rejects new work after request; subscriptions stop; `shell_update.rs` flushes UI/directory persistence, drains the dedicated structured-log worker from one shutdown-only bounded blocking task in parallel with the five-second runtime stop, logs outcomes, and returns Iced `window::close` instead of calling `process::exit`. TUI shutdown also requests a bounded log flush. `scripts/test-desktop-shutdown.sh` exercises the canonical product through an isolated Xvfb/i3 window-manager close while runtime startup is active and a 500 ms workspace-preference save is pending | Ordered-state, worker flush/join/permit-release, and product/release checks pass. A freshly rebuilt canonical Linux product opened in 1,203 ms and returned normally 137 ms after close; the pending three-pane workspace and queued structured startup record were durable, shutdown tracing flushed, every isolated JSON/JSONL file parsed, and no temporary persistence file remained. Three pre-worker warm runs closed in 135–139 ms, so this run shows no material close regression but is not a controlled startup comparison. The harness rejects mock/dev binaries and statically refuses routine desktop `process::exit`. Windows/macOS native close, platform file replacement, and debugger/destructor probes remain before `already fixed`. No config/wire/dependency change; host-only tools are detected, never installed. Roll back the log worker/lifecycle/tests/docs together; roll back the existing harness/config/docs independently. Gate: each release OS proves normal close, guard/destructor execution, structured-log flush, and valid final files within its timeout. |
| F-015 | already fixed | `desktop/update.rs::Message::route` exhaustively assigns every feature-gated production variant to exactly one `MessageRoute`; `DesktopApp::update` invokes only that owner instead of probing subsystems. The match has no wildcard, so a new production variant fails compilation until routed. `desktop/message.rs` contains typed envelopes for theme, clearweb, external-browser, runtime, identity, plugin, directory, interface, diagnostics, workspace-pane, shell, browser, conversation command/completion, and OMENchat command/transport/media-completion ownership. The sole unwrapped top-level variant is test-only and proves a classifier/handler disagreement logs, persists a payload-free `Error/App` diagnostic, and surfaces status | Focused ownership tests cover every production envelope. The message-size bound runs whenever `desktop-ui` is enabled. Minimal UI, development, animated product, and static-media product profiles all pass locked check, warning-clean library Clippy, and full library tests (582/903/897/894 passed respectively; only the profiles' documented ignored tests remain). The post-request shutdown allowlist still admits only close/begin/complete lifecycle messages. Behavior, protocol, storage, configuration, and dependencies are unchanged. Roll back any domain extraction with its producers and handler while retaining exhaustive routing and the release-visible invariant diagnostic. Completion gate met: no mixed production top-level family remains, every supported desktop feature profile compiles, and the router size remains bounded. |
| F-016 | partially addressed | root `Cargo.toml`: Iced X11/Wayland and rfd XDG portal/Tokio features are Linux-target-specific; `portable-sqlite` makes bundled SQLite explicit in `desktop-product`. `scripts/verify-product-features.sh` asserts Linux-only backend isolation and bundled SQLite in Linux/Windows/macOS target graphs. The native CI matrix exercises the product on Windows x86_64 plus macOS Intel/Apple Silicon | Linux product check and three-target Cargo graph assertion pass without dependency version changes; the new native jobs have not yet executed on GitHub. Native launch/file-dialog tests and binary-size/startup comparison remain; graph inspection is not treated as a native test. No config/wire change. Roll back the target dependency tables, alias, assertion, workflow, and docs together. Gate: native Linux/Windows/macOS checks and measurements pass. |
| F-017 | partially addressed | `src/server/Cargo.toml`: `tui` owns optional Crossterm/Ratatui dependencies; `server-headless` enables live transport without terminal crates; `server-full` adds the TUI. Root `tui` explicitly owns Tokio signal handling; modules and execution are feature-gated; packaged artifacts explicitly preserve the full products | Headless/full checks, headless rejection test, compiled feature identity, dependency-tree exclusion/signal assertion, warning-clean Clippy, docs, smoke and packaging aliases pass on Linux. Release binary measurement: headless 7,061,784 bytes versus full 7,906,104 bytes, an 844,320-byte reduction. Independent root TUI real-PTY runs survive live 0x0-to-100x30 resize, then gracefully handle one SIGTERM in 52 ms, one SIGINT in 55 ms, and two SIGTERM notifications 10 ms apart in 55 ms, all with status zero and exact terminal restoration below the 3,000 ms gate. Repeated notifications coalesce through one lifetime listener and atomic flag. A zero-capacity-channel fault blocks synchronous shutdown, delivers two concurrent requests, then proves real isolated-root settings flush plus complete guard restoration without a production delay hook. Native workflow root/server tests and lifecycle faults are configured but have not executed. Tokio's feature reuses locked prerequisites already present through Crossterm and adds no crate/version/lock entry. Roll back aliases/module gates/signal task/script/docs together. Gate: native headless graphs exclude terminal crates, packaged full server retains TUI, and Windows/macOS interactive terminal restoration passes. |
| F-018 | already fixed | `chat-client-gif` owns `iced_gif`; canonical `desktop-product` preserves animation while `desktop-product-static-media` retains live Reticulum/OMENchat with static GIF fallback and no decoder/frame-widget graph. `desktop-widgets` no longer selects unused Lucide/Nerd/Codicon bundles; icons remain the curated system Nerd Font glyph set in `desktop/icons.rs`/`fonts.rs` | Focused regressions prove static fallback and bounded animated decode; graph assertions prove static-media excludes `iced_gif` and widget builds exclude all three font features. Linux release binaries: animated 51,066,568 versus static 50,830,608 bytes (235,960/0.46% smaller); three-font widget 51,053,712 versus curated widget 51,053,208 bytes (504 bytes smaller because the linker already discarded unused assets). All affected Rust 1.97 Clippy profiles pass. No wire/config/dependency-version change. Roll back aliases, conditional frame boundary, direct font edge, assertions, tests, and docs together. Completion gate met: minimal product excludes decoder/fonts and both feature deltas are recorded. |
| F-019 | already fixed | root `[profile.dev.package."*"]` now uses `opt-level=1`; application code retains normal debug optimization and release profiles are unchanged | Identical isolated two-job canonical builds measured clean 1177.945 -> 922.576 s (21.68% faster), root-source incremental 9.474 -> 7.700 s (18.72% faster), and target bytes 5,977,759,004 -> 5,606,883,942 (6.20% smaller). No-change timings were sub-second noise. A short isolated X11 smoke opened the level-1 debug UI in 245 ms and held 207,620 KiB median RSS without exit; level 3 recorded 124 ms/207,600 KiB. Full product tests and Clippy pass. No wire/config/dependency/release change. One-line rollback to level 3. Completion gate met: material build improvement with viable interactive debug UI. |
| F-020 | partially addressed | Root has a library and gateway bin, but `src/main.rs` remains a large combined CLI/desktop entry. Stable identity/help/simple recognition, browser secret preprocessing, typed TCP/complete override ownership, pure value/diagnostic redaction, shared bounded structured-log reading/report collection, bounded structured-log writing/retention, and Tokio construction live in library modules. `main` retains option consumption, command conflicts/defaults, bundle creation/environment capture, complex execution, reviewed runtime error context, and `block_on` | Exact identity/help/CLI/bootstrap/secret/network/override/value/redaction/report-log boundaries and isolated integration tests pass. Report and application startup log loading share one fixed-budget regular-file tail reader; browser log disk ownership is now a separate fixed-policy module. Diagnostic JSON/argv/log schemas remain compatible while protected paths/credentials stay redacted. Backend/delivery aliases and errors remain unchanged. Server/gateway contracts are not normalized. Release consumers and package paths remain unchanged. Continue one boundary at a time before separate frontend binaries; measure compile/link effects only after material extraction. No dependency/wire/default change; Settings rejects startup history and disk-policy values above their hard caps. Roll back each module export/delegation/test/doc slice independently. Gate: compatibility CLI plus native platform entry points and package paths work. |
| F-021 | already fixed | Server resources, native LXMF attachments, propagation fallback/stamp work, and media decoder input move owned buffers. Client/server frame and batch encoders borrow strings/binaries. The embedded clean SDK submitter now borrows `NativeLxmfSdkWireDelivery`; direct sends view the existing signed wire allocation while propagated sends own only their newly constructed envelope. Remaining payload copies are classified: one bounded upload-event-to-job ownership copy, decode-tree-to-owned-result transfers, explicit retained-state/event fan-out, test fixtures, or release-excluded legacy code | Pointer regressions cover 1 MiB server resource, propagation fallback/stamp worker, and direct signed-wire submission. Frame/batch wire equivalence and attachment/media round trips pass. For 512 KiB binary frames/batches, borrowed encoding removes one allocation and about 512 KiB peak live memory, with roughly half the measured encode latency. No dependency, wire, config, or public API change. Roll back each ownership boundary independently. Completion gate met: profiled large copies are removed at current product boundaries and accepted wire bytes are identical. |
| F-022 | already fixed | `server_log.rs` replaces callback-path writes with one buffered writer. Call sites explicitly submit typed `Info`, `Warning`, or `Error`; only typed warning/error records use the first-drained 128-record/256 KiB lane, while routine admission retains 896 records/768 KiB. Text never selects priority. The overall 1,024-record/1 MiB bound, 16 KiB record cap, metrics, flush/TUI stop flush, 8 MiB rotation, and three backups (~32 MiB) remain; severity is admission metadata and timestamp/text lines are byte-format compatible. The ignored release soak repeats three production-sized writer lifecycles against real isolated rotating files with a deterministic 2 ms write-boundary delay | Budget/release, UTF-8 cap, flush/failure, rollover/retention, typed-lane/text-independence/file-format, 2 ms slow-consumer, and 60 s filesystem-soak gates pass. The typed soak submitted 382,037 records, explicitly dropped 353,185 routine overload records, lost zero priority records, recorded zero write failures, held 64 items/777,932 bytes peak, measured 565/1,778/223,653 ns median/p95/max admission, grew RSS 4,898,816 bytes, held FDs at 4, rotated all three lifecycles, and retained 97,271,092 bytes across their separate caps. Against the identical text-classifier run, median/p95 fell 95.1%/86.6%; the maximum scheduler outlier rose from 182,671 ns. No crate/config/wire change. Roll back delay harness/script/docs independently; typed severity, call-site classifications, and lane tests remain one production unit. Completion gate met: deterministic slow writes do not stall admission, logs remain bounded/flushed, and priority is structured without changing file format. |
| F-023 | already fixed | Upload admission performs one lazy per-identity reconciliation, blocks missing/mismatched/orphan/unsafe state, then uses the schema-v2 actor/time index. Replacement commits the new row, physically evicts, then removes only successfully evicted rows | Clean/dirty/size-mismatch, indexed order, no-repeat-scan, explicit repair, preservation, idempotence, persistent restart boundaries, and real Linux kernel ENOSPC pass. Schema advances from 1 to 2 with a retained SQLite-consistent backup; no config/wire/dependency change. Roll back planner/session/index migration together and restore schema-v1 compatibility only with an explicit downgrade plan. Completion gate met: normal admission has no repeated directory scan and every tested failure blocks or conservatively over-counts physical storage. |
| F-024 | partially addressed | Browser pages are indexed/capped at 256 records/64 MiB/5 MiB each; OMENchat disk media uses a persistent 64-item/128 MiB index with dirty-marker recovery; SOCKS and buffered Reticulum/mock downloads atomically publish with bounded blocking. Delivered LXMF transient IDs, browser form state, saved/discovered directory state, interface profiles/gateway presets/generated config, the optional SDK ticket bridge, and client structured logs have explicit item/byte/string limits. Delivered-ID, browser form-state, directory, and interface/config stores now have bounded regular-file admission and private synchronized atomic replacement; delivered IDs validate ID/timestamp semantics, malformed delivered/form/directory inputs retain bounded exact backups, and failed form/directory/interface mutations roll back memory. Structured-log startup reads <=4,096 directory entries, <=16 regular non-symlink files, <=512 KiB/file and <=4 MiB total; the live buffer holds <=4,096 entries/4 MiB with 16 KiB messages. Browser persistence uses one dedicated non-waiting 256-record/2 MiB worker with exact queue/oldest-age/drop/failure metrics and bounded graceful flush. Diagnostics and Logs expose those in-memory counters without polling, filesystem reads, or recursive logging. Disk logs accept 4 KiB..8 MiB files and 1..16 rotations, truncate encoded records to the selected file cap, rotate before overflow, bound prune scans to 4,096 entries, and refuse static symlink/non-regular targets. Shared atomic replacement uses rename on Unix and `MoveFileExW(REPLACE_EXISTING|WRITE_THROUGH)` on Windows without a new crate; page/form/media/transient-cache/directory/interface files no longer remove the prior object before replacement | Fault/saturation/recovery and production-sized sparse/log-rotation tests pass; 1,000 12 KiB submissions against deterministic 2 ms writes stay within 256 items/2 MiB, explicitly drop overload, admit below 250 ms, flush in order, and release every permit. Linux `/dev/full` qualification observes kernel `ENOSPC`, increments the write-failure counter, and releases the item/byte permit. Linux optimized page/media improvements are 37.7x/2.39x median. Linux replacement, transient-cache, form-state, directory, and interface/config persistence tests pass and the Windows product graph compiles for `x86_64-pc-windows-gnu`; this is not native execution. The reusable Windows MSVC/macOS workflow now explicitly runs replacement plus page/form repeated-publication tests. A successful native workflow run remains before `already fixed`. Older excessive structured-log settings are clamped in memory with a warning and persist on the next settings save. No wire/crate/default change; the worker and disk-policy bounds are additive safety behavior. Roll back the shared reader/writer/LogBuffer budgets/lifecycle/tests/docs together; rollback each persistence call site with its admission and fault tests. Gate: native Windows executes bounded atomic behavior and graceful log flush without normal directory scans. |
| F-025 | partially addressed | `runtime::thread_policy` selects one desktop async worker per available CPU, clamped to one through four; `runtime::bootstrap` owns the typed effective policy and sole application Tokio builder. The exact eight-thread ceiling remains only a backstop, while file writes, GIF decoding, propagation-stamp CPU work, and server SQLite access retain explicit two/two/two/one-permit bounds. The ignored optimized harness compares legacy, adaptive, and Tokio-default policies | Bootstrap tests prove policy constants and actual execution on an `omen-main-async` worker. Under two-core Linux affinity, adaptive versus legacy fixed-four results remain 11.938/881.120 us median, 40.609/1,402.347 us p95, queue depth 2/308, and bounded-write time 2.999/4.707 ms. The 28-core policy remains four workers. This ownership extraction makes no new performance claim and changes no config/dependency/default. Native low-core and live Reticulum page/message latency under media/database/stamp load remain. Roll back bootstrap delegation independently; roll back policy/harness only together if changing behavior. Gate: native matrix and live Reticulum latency show no regression. |
| F-026 | partially addressed | `.github/workflows/native-checks.yml` defines a reusable least-privilege matrix for Windows 2025 x86_64, macOS 15 Intel, and macOS 15 Apple Silicon. Each checks, tests, and runs Clippy for root `desktop-product`/`tui` and standalone `server-headless`/`server-full`, after machine-checking both product and TUI dependency identities. It also runs an isolated test-backend render/quit smoke and injected terminal-guard enter/rollback/drop faults; Linux release checks additionally run the real Crossterm binary inside a PTY | Static workflow/security assertions, `actionlint`, focused lifecycle/signal faults, isolated-root render/quit, full root TUI tests, strict Clippy, and three independent Linux real-PTY launch/live 0x0-to-100x30 resize/external-signal/exact-restoration runs pass. Single SIGTERM/SIGINT and double SIGTERM delivery-to-zero-exit measured 52/55/55 ms against a 3,000 ms gate. The PTY gate exposed and fixed eager zero-height subtraction before first draw; the live resize gate proves the application remains running across each boundary. Ctrl-C and repeated external signals coalesce through the bounded atomic flag, normal persistence, and guard drop. A deterministic blocked-shutdown fault proves concurrent requests remain bounded while actual isolated-root settings persistence and terminal restoration complete; it does not claim kernel-write timing. Pre-fix external SIGTERM returned 143. No hosted native job has executed, so Windows/macOS interactive success is not claimed. Launch/file-dialog, native terminal input/signal/restoration, slow-native-filesystem syscall timing, and installer lifecycle tests remain. The optional TUI Tokio feature changes, with no new crate/version/lock entry; no config/wire change. Roll back lifecycle/layout/key/signal routing, tests/harnesses, feature edge, workflow, and docs together. Gate: all matrix jobs and real native smoke are green before packaging. |

## Phase status

| Phase | Status | Completion gate |
|---|---|---|
| 0 Baseline/build identity/measurements | partially addressed | Checkout/toolchain/feature graphs and validation captured below; repeatable runtime measurement scenarios exist and reference results are recorded or explicitly pending. |
| 1 Release correctness/security/backpressure/durability | partially addressed | F-001/F-002/F-003 completion gates are met; F-004 awaits repository environment policy and a real tag run; F-013 awaits native platform validation. No S0 is closed without its complete fault/load gate. |
| 2 Event-driven desktop/shutdown | partially addressed | F-005/F-006 event-driven and mutation-only gates pass with full Linux before/after evidence. F-014 hard exit is replaced and Linux native cleanup/persistence passes; Windows/macOS native cleanup remains. Hardware-specific static GPU submission evidence remains pending. |
| 3 Media/network/cache budgets | partially addressed | Browser page, OMENchat media, structured logs, protocol, and network bounds are implemented. OMENchat retains at most 64 client sessions; per-session history is capped at 1,024 events/8 MiB, room catalogs at 256 items/512 KiB, and active-room user catalogs at 1,024 items/1 MiB. Live client upload offers are capped at four items/16 MiB; inline assemblies at 16 items/16 MiB, 8 MiB each, and 1,024 pending fragments each, with visible metrics and close/reconnect release. Retained presentation fields now have 64-byte through 16 KiB semantic limits rather than inheriting the 512 KiB codec scalar allowance; operational identifiers reject instead of truncating and SQLite filters byte lengths before materialization. History uses direction-aware pagination with SQLite preservation, while catalog restore performs bounded active/joined-first reads without deleting non-resident rows. Plugin startup now scans at most 4,096 directory entries, retains at most 256 installed candidates, caps manifests at 64 KiB and the registry at 1 MiB, refuses symlink/non-regular metadata files, reports overload without executing plugins, persists the registry through private synchronized atomic replacement, and recovers interrupted quarantined removals according to committed registry ownership. Every remaining untrusted-byte/cache path still needs inventory, tests, and metrics. |
| 4 Server DB/config/logging | partially addressed | Live database work plus CLI, line-console, dashboard room/moderation, and upload-ledger maintenance have bounded worker ownership; sustained contention/restart/integrity, actor saturation, cache-bound, read-only/repair/restore modes, locked-database TUI responsiveness, event/upload process-kill, typed logging, and repeated slow-writer logging gates pass. Actual Reticulum load remains. |
| 5 Iced QOL/crate admission | partially addressed | A persisted, default-off reduced-motion preference with an explicit focusable Settings control now withholds animated GIF frame handles at the existing visibility boundary. The locked Iced-adjacent inventory and machine graph gate admit only `iced` plus bounded in-memory `iced_gif` to the animated product and only `iced` to static media; dormant adjuncts remain product-excluded and no crate was added. The same gate requires maintained `harfrust`/`skrifa` shaping in both products and rejects `rustybuzz`; an intentional `desktop-svg` product override proves the negative gate. It also rejects Iced debug beacon's test/dev-only `bincode` edge; an intentional `iced/debug` product override proves that negative gate. Active build-time `paste` is constrained to exactly the reviewed `rav1e` AVIF parent on Linux/Windows and `metal` plus `rav1e` on Apple targets, while the separate TUI gate rejects it from root/server terminal profiles. Removing unused `iced_gif/async-fs` reduced unique product tree lines 604->591 without a version change. Accessibility/settings/route regressions, both product Clippy/library matrices, graph assertion, and quick release pass. Native GPU and platform verification plus remaining keyboard/focus/high-contrast review remain. Precise security patches resolve `anyhow`, `crossbeam-epoch`, and lock-only `quinn-proto`. A checked-in cargo-deny policy now gates licenses, sources, and wildcard requirements for both lockfiles without advisory exceptions; the two constrained Linux build-time `quick-xml` advisories still block a new runtime crate. |
| 6 Native packaging | partially addressed | Read-only native compile/test jobs are defined and gate packaging; they must execute successfully before installer work, then native build/install/launch/upgrade/uninstall gates must pass. |
| 7 Release qualification | partially addressed | Root/server lockfiles are audited and advisory reachability is recorded. Compatible updates remove the previously recorded runtime/lock soundness findings and yanked image-stack entry. Root audit still fails on two build-time `quick-xml` advisories constrained by `wayland-scanner ^0.39`; server audit is clean. Ratatui 0.30.2 and Crossterm 0.29.0 move both TUI profiles to fixed `lru` 0.18.1, remove their `paste` edge, retain explicit layout caching, and pass Linux strict Clippy/full tests plus a machine graph gate; native TUI smoke remains. The unmaintained `rustls-pemfile` warning is runtime-reachable only through the desktop LXMF SDK RPC backend; that backend is used, no patched crate exists, and current `lxmf-sdk 0.9.5` still depends on it, so resolution requires an approved upstream SDK migration rather than feature removal, a local fork, or an advisory ignore. Font-stack triage proves canonical products already shape through maintained `harfrust`/`skrifa` and now rejects activation of lock-only `rustybuzz`; active `ttf-parser` remains constrained by `fontdb` and `ab_glyph`, including their current releases, and requires an upstream Iced/font-stack migration plus malformed-font and native rendering qualification. Unmaintained `bincode` is absent from release, normal development, TUI, and server graphs; explicit Iced debug/time-travel enables it in an unbounded local debug TCP protocol, so a machine gate rejects release activation while resolution awaits an upstream bounded protocol migration. Unmaintained compile-time `paste` remains through desktop AVIF encoding on every target and the reviewed `metal` graphics backend on Apple targets. Cargo-deny 0.20.2 policy and CI gates cover approved licenses, native target sources, and wildcard requirements for both independent graphs with no advisory ignores. No S0; S1 fixed or approved; fuzz/soak/interoperability/provenance complete. |

## Supplemental hardening units

- Hosted Python-interoperability environment correction: checked-out pinned
  Reticulum/LXMF source paths are canonicalized before Cargo changes the test
  process working directory, so every fixture verifies the intended immutable
  commits instead of resolving a repository-relative path beneath its crate.
  The explicitly versioned current-Python environment now installs
  `msgpack==1.2.1`, which the propagation fixture imports directly rather than
  relying on an incidental transitive package. Neither change alters product
  dependencies, wire behavior, configuration, or state. Completion gate: the
  release-blocking pinned lane passes, the informational drift lane emits its
  report, and mixed 0.6/0.9 application interoperability completes.
- Windows portable package boundary: after the native matrix passes, a read-only
  Windows 2025 MSVC job builds and identity-checks separate unsigned desktop and
  standalone omenchatd ZIPs with SHA-256 files. The browser package neither
  bundles nor starts the server, and the server package installs no service.
  Publication depends on both native Linux and Windows artifacts without
  checking out repository code in the privileged job. NSIS/MSI and native
  install/upgrade/uninstall/GUI-launch evidence remain release gates.
- Bounded quick-runner build storage: after dependency policy passed, the
  pull-request quick job exhausted the GitHub-hosted filesystem while the
  Actions runner wrote its own diagnostic log. The quick job now caches Cargo
  registries/tools without either workspace target tree and disables
  incremental artifacts on the ephemeral runner. Commands and test scope are
  unchanged. The exact pinned action source confirms the input; local workflow
  syntax/security and quick gates pass before the hosted rerun. Roll back both
  storage controls together, with recurrence of the observed disk failure as
  the risk.
- Exact local-crate dependency identity: the pull-request all-feature policy
  gate exposed that path-only declarations count as wildcard requirements.
  Root and standalone-server `omen-ifac-tcp` dependencies now retain their
  existing relative paths while requiring the crate's exact `=0.9.5-1`
  package version. Both pinned cargo-deny commands pass. Resolution, lockfiles,
  production behavior, wire, schema, and configuration are unchanged.
- Native release CLI identity smoke: the hosted matrix now executes the actual
  browser desktop/TUI and standalone server headless/full command-line entry
  points after compilation. State-free `--version` assertions require the
  native Rust host target, deterministic product identity, mock/test exclusion,
  and the expected server feature split; `--help` assertions preserve isolated
  root and operator diagnostic controls. The workflow verifier preserves the
  step. No GUI/TUI, Reticulum, identity, configuration, or user root is opened.
  Local Linux execution and the quick release gate pass; hosted Windows/macOS
  execution and interactive/installer lifecycle remain completion gates.
- Native all-target preflight: strict Clippy in the reusable native workflow now
  covers all declared targets for `desktop-product`, root `tui`,
  `server-headless`, and `server-full`, and the workflow-security verifier
  preserves that scope. Windows-GNU cross compilation/test construction and
  strict all-target Clippy pass for bare native LXMF and all four product
  profiles. This exposed and fixed a missing `chat-client` requirement on the
  mixed SQLite probe example and a Linux-only server-log soak helper that was
  otherwise dead on Windows. The strengthened Linux all-target test/Clippy
  matrix passes as well. No production behavior, dependency, wire, schema, or
  configuration changed. This is portability evidence, not native execution;
  hosted Windows MSVC, both macOS jobs, interactive native smoke, and installer
  lifecycle remain completion gates.
- First hosted-native CI correction: the initial Windows/macOS matrix exposed
  two target assumptions hidden by Linux. The product graph gate now checks
  exact target-specific `paste` parents: `rav1e` on Linux/Windows and `metal`
  plus `rav1e` on Apple. Windows backup retention no longer declares Unix-only
  directory-sync state, and payload-bearing OMENchat media completions are
  boxed so the exhaustive router `Message` remains strictly below 128 bytes on
  Windows as well as Linux. Target-aware graph checks, Windows-GNU strict
  product library Clippy, native Linux strict Clippy, focused routing/scroll/
  size tests, and settings-retention tests are the local gate. No dependency,
  wire, configuration, storage-format, or accepted behavior change. Roll back
  the target graph expectations/docs independently; roll back the cfg cleanup
  and boxed internal envelope only with their native Clippy gates. The first
  rerun passed each corrected graph gate, then Apple Silicon's all-target
  Clippy exposed 13 Rust 1.97 style failures in settings test fixtures; those
  fixtures now use equivalent direct initializers, with all 31 settings tests
  and the complete desktop-product Clippy target passing locally. The same
  rerun's later Windows test gate showed that a Unix absolute-path literal was
  relative under Windows path rules; redaction fixtures now construct an
  absolute private identity path for the target OS and assert that its complete
  rendered form is absent. All six focused redaction tests and Windows-GNU
  strict product library Clippy pass locally. Completion gate: rerun hosted
  Windows/MSVC and both macOS jobs successfully. The next Windows standalone-
  server gate exposed a durability portability defect: Windows rejects
  `sync_all` on a read-only file handle even though Unix admits it. Migration
  backups and staged database restores now reopen completed SQLite files with
  read/write access before flushing, without changing create/replace policy.
  The complete server-full library suite passes locally (289 passed, 3
  explicitly ignored), as does Windows-GNU strict server-full Clippy; native
  Windows execution confirmed the repaired SQLite paths in the headless and
  full suites. The subsequent full-profile TUI run exposed two test-only Unix
  separator assumptions; both path-visibility assertions now derive the exact
  native display strings from `ServerConfig`. Focused tests and Windows-GNU
  strict server-full Clippy pass. Completion still requires one green final
  native matrix.
- Bare native-LXMF feature closure: root native request/LXMF code now uses the
  feature-neutral allocation-free MessagePack preflight and shared OMENchat
  wire ceilings rather than importing the `chat-client` module. The declared
  `--no-default-features --features native-lxmf` profile therefore compiles
  without desktop, chat-client, development, or mock leakage. Exact/next,
  depth, trailing-data, reserved-marker, truncation, native request, announce,
  propagation, and LXMF wire tests cover the shared boundary. Root smoke,
  quick release, and native Windows/macOS jobs machine-check this profile; the
  standalone server retains its independent implementation. No dependency,
  configuration, storage, public API, or accepted wire-byte change. Roll back
  the shared modules, call-site delegation, and gates together. Completion
  gate: bare strict Clippy/full library tests pass and hosted native jobs are
  green.
- Browser page retained-data admission: every network response, cache restore,
  partial composition, direct application, and persisted-navigation restore
  crosses `BrowserPage::validate_retained` before parsing or state mutation.
  Pages admit at most 8 KiB URL, 16 KiB title, 4 MiB markup, 64 top-level
  metadata entries/16 MiB metadata strings and keys, and 128 request entries/
  4 MiB request strings, with scalar, container, value-count, and depth limits.
  MicronPlus-derived metadata is checked again after normalization. Invalid
  pages fail atomically; startup falls back to the safe mock page, and rejected
  partial content does not enter the retained partial map. Exact-limit,
  next-byte, deep/excessive metadata, page-state preservation, and partial
  rollback tests pass. No wire, dependency, configuration, schema, cache-file,
  or accepted below-limit behavior changes. Roll back page admission,
  fallible session boundaries, tests, and docs together. Completion gate:
  parsing, caching, and session state never receive a page outside the declared
  retained-data policy.
- MicronPlus structural admission: source is preflighted at 4 MiB, 16,384
  lines, and 256 KiB per line before line-reference or attribute collection.
  Attribute parsing admits 64 entries, 256-byte keys, 64 KiB values, and
  128 KiB aggregate strings. Fallible tree construction admits depth 32,
  8,192 nodes, 512 columns, and 8 MiB retained strings; layout construction
  admits 64 windows, 256 groups, 512 columns, and 8 MiB retained strings.
  Session normalization removes stale tree/layout metadata on rejection and
  retains visible lowered fallback plus a deduplicated diagnostic. Tree/layout
  partial updates preflight repeated-slot multiplication, borrow fragments
  during traversal, mutate a bounded candidate, and publish only after both
  structural and whole-page admission. Exact/next node, depth, line,
  attribute, column, retained-byte, multiplication, compatibility, and stale
  metadata tests pass. No wire, dependency, configuration, schema, or valid
  below-limit syntax changes. Roll back MicronPlus source/tree/layout/partial
  admission, session diagnostics, app candidate publication, tests, and docs
  together. Completion gate: no MicronPlus parser or partial path can retain a
  structure above the declared budgets or recurse beyond depth 32.
- MicronPlus widget/event admission: per-tab widget state admits 256 widgets,
  4,096 items, and 4 MiB total; each widget admits 1,024 items/1 MiB. Widget
  IDs, text, style, and markup are capped at 256 bytes, 16 KiB, 64 bytes, and
  256 KiB respectively. Markup items also share the tree's 8,192-node/
  512-column derived-structure budget. Append events transactionally retain
  the newest per-widget edge without evicting another widget; set/status
  updates reject atomically. Extraction admits 256 events/1 MiB and leaves
  overload lines visible as markup. Control-event history retains the newest
  256 events/2 MiB after semantic field validation. Store metrics expose
  widgets/items/bytes/rejections, and application rejection writes a warning
  plus visible task status. Exact widget/item/byte, recent-edge, invalid
  scalar, derived-structure, extraction, history, visibility, compatibility,
  Clippy, and product/development matrix tests pass. No wire, dependency,
  configuration, or persistence-schema change. Roll back widget-store
  admission, control-history retention, app reporting, tests, and docs
  together. Completion gate: widget events and histories cannot grow without
  item and byte ceilings, and rejected mutation preserves prior state.
- Browser navigation-history admission: sessions retain at most 512 URLs and
  1 MiB of URL strings under the existing 8 KiB per-page URL ceiling. Live
  navigation truncates the forward branch, then removes one oldest prefix so
  the current/newest edge and pointer remain coherent. Persisted restore keeps
  one contiguous bounded window around the clamped saved pointer; an oversized
  selected URL rejects before page mutation, while an invalid adjacent edge
  closes only that direction rather than creating a navigable gap. Resolved
  input above 8 KiB rejects before cache or runtime dispatch. Item, aggregate
  byte, centered restore, atomic rejection, invalid-edge, and pre-dispatch
  regressions pass with memory fixtures and generated temporary roots. No wire,
  dependency, configuration, persistence schema, or below-limit history
  behavior changes. Roll back the history constants/admission helpers, URL
  preflight, tests, and docs together. Completion gate: every session-created
  or restored navigation history has explicit item/byte bounds and a valid
  pointer without skipping across discarded entries.
- Application-settings file admission: `AppSettings` loads only a regular,
  non-symlink file up to 8 MiB and reads through an 8 MiB+1 hard cap before
  Serde allocation. Missing settings still default; malformed bounded regular
  files retain corruption-backup/default recovery. Oversized, directory, and
  valid or broken symlink paths fail explicitly without reading, copying, or
  following them. Save rejects a payload above the same limit before staging,
  preserving the prior file. Exact-limit JSON, sparse next-byte, special-path,
  referent-preservation, existing corruption recovery, and oversized-save
  regressions pass under generated roots. No dependency, wire, schema, setting
  default, or valid below-limit behavior changes. Roll back the shared limit,
  bounded reader/save preflight, tests, and docs together. Completion gate:
  settings filesystem reads and published serialized payloads share one hard
  limit, and unsafe paths never enter JSON parsing or backup copying.
- Application-settings atomic persistence: accepted JSON is written to a
  unique same-directory `create_new` sibling, owner-only on Unix, then flushed,
  synchronized, and published through the shared Unix/Windows atomic-replace
  primitive; Unix synchronizes the parent directory after commit. Existing
  directory and valid/broken symlink targets reject before staging. Former-temp
  collision, normal round trip, Unix mode, both symlink forms, directory target,
  oversized preflight, and injected replacement-failure tests pass in isolated
  roots. The injected pre-commit fault preserves exact previous bytes and
  removes every new sibling. No dependency, wire, settings schema/default, or
  accepted JSON change. Roll back the settings staging helper, shared replace
  call, fault seam/tests, and docs together. Completion gate: no tested
  pre-commit fault damages the last valid settings file, no metadata target is
  followed, and a successful save is synchronized before and after publication
  where the platform exposes directory synchronization.
- Malformed-settings backup publication: recovery writes the exact bounded byte
  buffer already admitted by the loader instead of reopening the source path.
  A unique owner-only same-directory stage is flushed/synchronized, renamed to
  a unique `.corrupt.*.bak` through no-clobber hard-link publication, and
  followed by Unix parent synchronization before
  defaults return. Invalid-UTF-8 exact-byte/source-preservation, Unix mode,
  legacy-name collision, zero-residue success, and injected publication-failure
  tests pass in isolated roots. The fault inspects the staged admitted bytes,
  leaves a deliberately different current source untouched, and removes all
  backup/staging output. No dependency, wire, settings schema/default, or
  bounded corruption-fallback change. Roll back the admitted-byte handoff,
  backup staging/publish helper, tests, and docs together. Completion gate: a
  parse failure cannot race a second source-path read, overwrite an existing
  operator backup, expose group/world-readable recovery bytes on Unix, or leave
  a tested partial backup after a pre-publication failure.
- Malformed-settings backup retention: recovery recognizes only strictly
  encoded current or legacy regular sibling backups, retains the newest four
  and 32 MiB total, and refuses work after 4,096 directory entries. Matching
  symlinks and special files are ignored. Retention runs before publication to
  repair a prior interrupted over-cap state and afterward to include the new
  backup; the only crash window is bounded to one additional admitted backup
  and is repaired before the next publication. Seven distinct recoveries,
  five sparse 8 MiB legacy files plus a new backup, Unix symlink preservation,
  and a 4,097-entry saturation fixture pass in isolated roots. No dependency,
  wire, settings schema/default, or valid recovery naming change. Roll back the
  recognition/pruning helpers, before/after hooks, constants, tests, and docs
  together. Completion gate: completed recovery owns at most four regular
  backups/32 MiB, prior over-cap state cannot grow again, and retention work is
  directory-scan bounded without following or deleting a link.
- Application-settings retained-state admission: parsed settings are accepted
  atomically only within explicit collection and recursion budgets: 128 browser
  tabs, 128 conversation tabs, 4,096 bookmarks/tombstones, 256 panes/plugin
  IDs/extension fields, 64 attachments per draft, 511 layout nodes, 4,096
  extension container items, 16,384 extension nodes, and depth 32. Browser
  history, URL/title, focused-control, and focused-link admission reuses the
  live browser/Micron limits. Iterative layout/extension validation avoids a
  second recursive walk and rejects non-finite split ratios. Syntactically
  valid over-limit input publishes the exact admitted bytes through the existing
  bounded recovery path and returns complete defaults; save rejects through the
  same validator before serialization/staging. Exact/next boundary, all
  persisted collection, recursion, exact-backup/no-partial-restore, and
  prior-file/no-staging regressions pass in isolated roots. No wire,
  dependency, schema, or default change; older settings above a new semantic
  ceiling recover as a preserved backup plus defaults. Roll back the semantic
  constants/validator, tests, and docs together. Completion gate: no admitted
  settings collection or recursive extension/layout value can create
  unbounded retained application state, and rejected state is never partially
  restored or published.
- Application-settings pre-deserialization admission: the bounded raw settings
  buffer crosses an allocation-free fixed-stack structural scanner before
  Serde can allocate typed strings, vectors, maps, or recursive nodes. It caps
  nesting at 48, structural tokens at 262,144, each container at 8,192 items,
  and each raw string token at 4 MiB. Container/depth/token limits exceed their
  retained-state counterparts; the string cap independently bounds any one
  decoded setting. Full grammar validation remains with Serde.
  Structurally excessive valid input follows the existing exact-byte
  backup/default path, and save runs the same scanner before staging. Exact and
  next depth/container/string/token tests, malformed-structure tests,
  exact-backup/default recovery, and prior-file/no-staging save rejection pass
  in isolated roots. No dependency, wire, schema, or default change; a legacy
  settings string above 4 MiB now recovers as a preserved backup plus defaults.
  Roll back the scanner/constants/tests/docs together. Completion gate: an
  admitted 8 MiB file cannot control unbounded settings container/value
  allocation before semantic validation, and the application cannot publish a
  structurally inadmissible settings payload.
- Identity-material and discovery admission: attach, import, export,
  pre-overwrite backup, identity-scoped storage hashing, and all active native
  Reticulum identity loaders now share one non-empty regular non-symlink reader
  capped at 64 KiB and at limit+1 actual bytes. Unix inode/device comparison
  rejects a source-path swap before reading. Import/export/backup use the one
  admitted snapshot rather than reopening or copying a mutable source.
  Managed discovery refuses a linked/non-directory root, scans at most 4,096
  entries, retains at most 256 regular profiles, and never follows an entry
  link. Exact/next/empty material, oversized-import prior-target preservation,
  provider overflow, exact/next profile count, scan saturation, entry/root
  symlink, and referent-preservation regressions pass in isolated roots. No
  dependency, wire, identity format, config, or default change. Existing
  material above 64 KiB or through a symlink is now rejected. Roll back the
  shared reader/call sites/discovery limits/tests/docs together. Completion
  gate: no production identity read or managed-profile enumeration can allocate
  from unbounded file/directory input or follow a statically linked metadata
  path.
- Identity publication and backup retention: create, import, export, and
  pre-replacement backup now publish the already-admitted byte snapshot through
  a unique owner-only same-directory stage after flush/fsync. New identity and
  backup paths are no-clobber; import atomically replaces a regular target only
  after a synchronized backup is visible. Unix directory sync covers
  publication, retention, replacement, and deletion. Managed retention scans
  at most 4,096 entries and keeps at most 16 recognized current-namespace
  backups/1 MiB. It runs only after the new backup is durable; failure preserves
  the extra backup and aborts replacement/deletion. Legacy, custom-export,
  symlink, and ambiguous entries are not pruned. Injected precommit failures,
  prior-file/new-file cleanup, Unix private-mode/linked-root, replacement,
  retention, legacy preservation, and scan-saturation regressions pass in
  isolated roots. No dependency, wire, identity-format, configuration, or
  default change. Backup filenames move to the reserved
  `omen-identity.backup.*.bak` namespace; legacy backups remain readable and
  untouched. Roll back writer/retention/tests/docs together while retaining the
  prior shared reader. Completion gate: every successful identity publication
  is synchronized/private, every tested precommit fault preserves the prior
  identity, and managed application backup growth is bounded without deleting
  uncertain material.
- Message-store persistence admission: identity-scoped LXMF thread files now
  use one regular non-symlink reader capped at 8 MiB/limit+1 actual bytes with
  Unix inode/device comparison. Retained thread state admits at most 4,096
  messages plus bounded scalar, field, attachment, transport, and ticket
  metadata. Discovery/import scan at most 4,096 entries and admit at most 256
  portable-filename threads/64 MiB total. Peer-derived traversal and
  cross-platform-special names map to a deterministic contained filename while
  preserving their original JSON value; an existing safe single-component
  legacy filename remains readable/updatable without duplication. Save and
  no-clobber import publish an owner-only same-directory stage only after
  flush/fsync, then atomically replace or link and sync the directory on Unix.
  Malformed bounded input backs up the exact admitted snapshot; retention keeps
  four strictly recognized current backups/32 MiB and never prunes legacy,
  symlink, or ambiguous entries. Exact/next file bytes, next message/thread,
  aggregate bytes, scan saturation, path traversal, Unix mode/symlink,
  corruption retention, existing behavior, and injected precommit preservation
  pass in isolated roots. No dependency, wire, JSON schema, configuration, or
  default change. Existing over-limit/unsafe stores now fail explicitly rather
  than allocate, follow, or partially load. Roll back store admission/writer,
  exports, tests, and docs together. Completion gate: no message-store read or
  enumeration is input-unbounded, failed publication preserves the prior
  thread, and automatic backups cannot grow without bound or delete uncertain
  material.
- Micron control/form-state admission: control syntax is preflighted at 72 KiB;
  accepted descriptors have at most four parts, 32-byte flags, a 256-byte exact
  name, 64 KiB value/label, and width 1..=256. Parser state retains at most 128
  controls/4 MiB of kind/name/value/label/style strings. Rejected controls stay
  literal and explicitly non-actionable, including embedded autolink text.
  Browser session set/restore/toggle paths enforce matching name/value/item/
  aggregate limits and preserve the previous valid value on rejection. No wire,
  dependency, configuration, schema, or accepted control syntax below the new
  ceilings changes. Roll back parser/session admission, tests, and docs
  together. Completion gate: no parsed control or browser field-state entry
  exceeds these semantic budgets.
- Desktop browser field-editor admission: keyboard text, paste text, and Iced
  full-value updates are preflighted against the active session's 64 KiB value
  and 4 MiB aggregate budgets before mutating `InputState`. Rejection is atomic
  at UTF-8 boundaries, retains the previous editor/session value, and gives a
  visible status message. Generic address, message, and settings editors are
  unchanged. No wire, dependency, configuration, persistence-schema, or valid
  field behavior changes. Roll back the browser-session predicate, app/input
  admission, desktop atomic insertion, tests, and docs together. Completion
  gate: every live desktop field-edit path retains only a session-admissible
  value and reports rejection without partial insertion.
- Micron rendered-action allocation sharing: rendered link cells retain one
  shared immutable action per span instead of cloning its target/forwarded
  fields into every cell. Control cells retain per-cell offset/length state but
  share their name, kind, and payload-bearing value strings. Hit-region and
  canvas activation still produce the existing owned `HitAction` API. The
  deterministic gate proves 128 link cells share one maximum-field action and
  256 control cells share one 64 KiB value allocation while both actions remain
  activatable. No rendering, wire, dependency, configuration, persistence, or
  input behavior changes. Roll back the render reference representation,
  consumer conversions, regression, and docs together. Completion gate: cell
  wrapping/copying cannot multiply payload-bearing action string allocations.
- Micron document/render-output budgets: core parsing retains at most 16,384
  rows, rejects source lines above 256 KiB, and retains at most 64 metadata
  entries/64 KiB (256-byte keys, 4 KiB values, 16-byte style values). Any parser
  admission loss sets `Document::limits_applied` and appends a visible,
  non-actionable notice. Core and top-level MicronPlus rendering clamp width to
  4,096 cells and stream into a 65,535-row/1,048,576-cell budget with reserved
  notice capacity; hit-region scans use matching coordinate bounds. Valid
  content below the ceilings is unchanged. No wire, dependency, configuration,
  or persistence migration is introduced; serialized `Document` gains a
  defaulted limit flag. Roll back parser/renderer budgets, the app top-level
  bounding call, tests, and docs together. Completion gate: retained parsed and
  rendered structures cannot exceed these ceilings and every truncation path is
  observable.
- Micron fragment/document-link admission: authored content retains at most
  65,535 inline fragments plus the fixed visible limit notice and no more than
  4 MiB total span text including that notice. At most 4,096 link actions/4 MiB
  of target and forwarded-field strings remain actionable across a document.
  Over-budget actions are demoted in place to visible non-link spans without a
  second autolink pass; other over-budget inline content sets the existing
  machine flag and visible notice. No wire, dependency, configuration, schema,
  or valid below-ceiling behavior changes. Roll back parser-state admission,
  tests, and docs together. Completion gate: many tiny style/link elements
  cannot exceed document fragment or action-storage budgets.
- Micron rendered-style allocation sharing: cells in an authored span or
  control run share one immutable `TextStyle`; generated plain/padding cells
  reuse a process-wide default style. The two post-render mutation paths use
  copy-on-write, preserving MicronPlus title emphasis and document-default link
  coloring without leaking changes to neighboring cells. The deterministic
  gate proves one style allocation serves 1,024 text cells and one serves a
  256-cell control, then proves single-cell mutation isolation. No rendering,
  wire, dependency, configuration, schema, or persistence behavior changes.
  The internal public `Cell.style` representation changes from owned
  `TextStyle` to `Arc<TextStyle>`. Roll back renderer/consumer conversions,
  tests, and docs together. Completion gate: rendered cell count cannot
  multiply style-string allocations and every mutation is copy-on-write.
- MicronPlus/partial field admission: live, input, and button field attributes
  use the shared Micron 128-item/4 KiB-item/64 KiB-total collector before
  retaining vectors. Invalid widgets remain literal/non-actionable and emit a
  lowering diagnostic. Browser partials add fallible 96 KiB raw, 8 KiB target,
  shared field, and 256-byte ID admission; extraction skips invalid descriptors
  and retains at most 256 specs/1 MiB. The concrete compatibility parser remains
  available, but scheduling uses only fallible results. No wire, dependency,
  configuration, schema, or valid-syntax change. Roll back shared collection,
  MicronPlus/partial admission, tests, and docs together. Completion gate: no
  MicronPlus control or scheduled partial owns an over-budget field vector.
- Micron link-action admission: standard link descriptors are preflighted at
  96 KiB before their raw character slice is collected. Retained labels are
  limited to 16 KiB, exact targets to 8 KiB, and forwarded fields to 128 items,
  4 KiB each, and 64 KiB aggregate. Shorthand and LXMF autolinks share the
  target ceiling. Rejected syntax stays visible in an explicitly non-actionable
  span so embedded autolink-looking text cannot bypass rejection. No wire,
  dependency, configuration, schema, or accepted-link syntax change. Roll back
  parser constants/admission/tests/docs together. Completion gate: no parsed
  `LinkAction` owns a target or field collection above these budgets.
- OMENchat descriptor admission: declarative blocks are preflighted at 64 KiB,
  128 lines, and 32 KiB per line before recognized values are retained. Room
  hints and capabilities each admit at most 64 entries, with 64-byte room names
  and 128-byte/8 KiB-total capabilities. Exact destination/path/theme/signature
  metadata rejects above its limit; display names shorten at a UTF-8 boundary.
  Lowering does not join an over-budget block. Micron links use atomic
  32-field/16 KiB admission, so a later invalid field cannot partially alter a
  descriptor. No wire, dependency, configuration, schema, or server-validation
  change. Roll back descriptor constants/parser/link admission/tests/docs
  together. Completion gate: no descriptor-owned collection or scalar inherits
  an unbounded source-page size before session admission.
- OMENchat client presentation metadata: semantic admission is separate from
  the 512 KiB codec scalar ceiling. Server/user/actor display names are capped
  at 256 UTF-8 bytes, operational room names at 64 bytes, topics/status at
  4 KiB, MOTDs at 16 KiB, exact resource IDs at 4 KiB, filenames at 4 KiB, and
  content types at 1 KiB. Display-only values shorten at a valid UTF-8 boundary;
  oversized operational labels/identifiers are rejected rather than rewritten.
  SQLite applies BLOB byte-length predicates before materialization. UTF-8
  edges, live/mock events, session admission, pre-send command rejection, and
  corrupt-store rows pass in isolated state. No dependency, configuration,
  schema, server validation, or wire change. Roll back constants/helpers,
  adapter/store admission, tests, and docs together. Completion gate: no
  retained client presentation scalar can inherit the 512 KiB general codec
  allowance, and command identifiers remain exact.
- OMENchat live client transfer admission: outgoing offers retain at most four
  payloads/16 MiB and 8 MiB per payload. Inline assembly reserves at most 16
  resources/16 MiB, 8 MiB per resource, and 1,024 pending fragments per
  resource; stable metadata and retained-payload-within-declared-length checks
  prevent overlapping fragments from multiplying memory. Item/byte/resource/
  fragment saturation, normal in-order/out-of-order completion, accept release,
  and session cancellation pass in isolated memory. Monitoring exposes items,
  outgoing/declared/retained bytes, fragments, and rejection totals. There is no
  dependency, configuration, server quota, or wire change. Roll back constants,
  ownership fields/admission, metrics/UI, cancellation hooks, tests, and docs
  together. Completion gate: peer-controlled frames and repeated local offers
  cannot grow retained transfer payload beyond any declared dimension, and
  close/reconnect releases session-owned transfer state.
- OMENchat client catalog admission: the client retains at most 64 sessions;
  each session retains at most 256 room summaries/512 KiB and 1,024 active-room
  user summaries/1 MiB of estimated owned storage. The 65th mock, live, or
  desktop open is refused visibly without eviction, phantom pane state, or an
  outbound live frame. Live snapshots are deterministically reduced and status
  reports overload. SQLite applies item and cumulative-byte admission while
  iterating, orders the active and joined rooms first, and does not delete rows
  outside the resident view. Item/byte saturation, live visibility, active-room
  priority, mock behavior, and desktop pane ownership pass in isolated state.
  There is no dependency, configuration, schema, quota, or wire change. Roll
  back the catalog constants/admission helpers, open-path handling, bounded
  store queries, tests, and docs together. Completion gate: repeated snapshot
  and startup restore cannot exceed any resident catalog dimension, and refused
  session opens cannot mutate an established client view.
- OMENchat client history window: `ChatSessionView.events` retains at most
  1,024 events and 8 MiB of estimated owned event/string storage. Restore and
  live append keep the recent edge; cache/server load-older keeps the older
  edge. The newly appended key is protected from remote timestamp skew. Every
  received history row is appended idempotently to SQLite before the bounded
  session is persisted, so overflow changes memory residency rather than
  durable history. Item/byte saturation, both eviction directions, timestamp
  skew, overflow persistence, existing pagination, live behavior, and mock
  behavior pass in isolated state. No dependency, configuration, schema, or
  wire change. Roll back the history constants/admission helpers, live/mock
  routing, persistence boundary, tests, and docs together. Completion gate:
  repeated history loading cannot exceed either per-session memory dimension,
  accepted overflow remains queryable from SQLite, and old/new clients and
  servers remain wire-compatible.
- Plugin discovery admission: `src/plugins.rs` bounds startup work to 4,096
  directory entries and 256 installed candidates, reads only regular manifests
  up to 64 KiB and a regular registry up to 1 MiB, and refuses oversized
  registry output. Saturation, sparse oversize, and Unix symlink regressions use
  isolated roots. No entrypoint is executed and there is no dependency, wire,
  configuration, or registry-schema change. Roll back constants, bounded reader,
  discovery admission, tests, and docs together. Completion gate: discovery
  remains available with explicit warnings under candidate/manifest overload and
  its owned startup memory is independent of directory or file size beyond the
  declared caps.
- Plugin installation admission: confirmed folder installs accept at most 1,024
  entries, 64 MiB total, 16 MiB per regular file, and 16 directory levels.
  Symlink/special entries and source-root symlinks are refused. Files are copied
  and synchronized in a hidden same-filesystem directory before one final
  rename; every tested pre-publication failure removes staging and leaves no
  target. No-follow destination checks also refuse and preserve broken links.
  Bounded startup recovery deletes only safely encoded reserved staging
  directories; non-directory collisions are retained with a warning. A
  published tree missing registry metadata is rediscovered disabled and
  untrusted. Exact-total/next-byte accounting, sparse oversize, entry saturation,
  depth, source/destination symlink, interrupted copy, normal publication, and
  registry compatibility tests pass in isolated roots. No dependency, wire,
  configuration, or registry-schema change. Roll back install
  constants/budget/copy/publish/recovery/tests/docs together.
  Completion gate: no partial plugin tree is visible after admission failure,
  installed source work is bounded by the declared dimensions, and normal
  confirmed installation remains compatible.
- Plugin registry persistence: `save_registry` refuses symlink/non-regular
  targets, writes a unique owner-only create-new sibling, flushes/synchronizes
  it, atomically replaces the target through the shared cross-platform helper,
  and synchronizes the parent directory on Unix. Loading no longer treats a
  broken symlink as an absent registry. Existing-file replacement, former-temp
  collision, directory/symlink refusal, Unix mode, and injected replacement
  failure tests pass; the fault preserves exact prior bytes and removes the
  unique stage. No dependency, schema, configuration, wire, or plugin execution
  change. Roll back registry target validation/staging/replacement/tests/docs
  together. Completion gate: pre-commit faults retain the previous valid file,
  no metadata path follows a link, and normal registry round trips remain
  compatible.
- Plugin removal transaction: both built-ins and symlink/non-directory targets
  are refused. An installed directory is atomically renamed to a uniquely named
  hidden quarantine, registry removal is durably committed, and physical
  deletion follows. Registry-save failure renames the tree back before
  returning. Bounded startup discovery recognizes only safely hex-encoded
  reserved quarantine names: registry ownership restores a pre-commit tree;
  absent ownership completes post-commit deletion. Normal removal, both
  built-ins, referent preservation, injected save rollback, and both discovery
  crash states pass in isolated roots. No dependency, schema, configuration,
  wire, or plugin execution change. Roll back quarantine naming, removal/recovery
  helpers, discovery hook, tests, and docs together. Completion gate: no tested
  interruption leaves an ambiguously visible plugin, and recovery never follows
  a link or deletes a tree still owned by the registry.

- LXMF delivered-transient cache persistence: the existing six-month policy,
  65,536-item high-water/90% low-water pruning, 8 MiB ceiling, versioned JSON,
  and legacy bare-map JSON remain compatible. Loading now admits only stable
  regular non-symlink files through a limit+1 capped reader. Oversized and
  special paths fail without read, backup, or mutation. Malformed syntax and
  semantically invalid 64-hex-ID/finite-timestamp maps default only after the
  exact admitted bytes are published to a private synchronized no-clobber
  sibling; the source remains unchanged. Current-namespace backup retention is
  four files/32 MiB under a 4,096-entry scan ceiling and ignores legacy names.
  Saves validate before staging and use private synchronized same-directory
  atomic replacement. Exact/next-byte, current/legacy round-trip, semantic,
  directory/symlink, source/backup preservation, retention, Unix mode,
  replacement, and injected pre-commit fault tests pass in isolated roots. No
  dependency, wire, configuration default, or accepted cache schema change.
  Roll back the cache admission/publication/backup helpers, tests, and
  documentation updates together. Completion gate: native Windows/macOS run
  the focused cache suite and successful live Reticulum qualification preserves
  duplicate suppression across restart.
- Browser form-state persistence: current and legacy JSON, age pruning,
  512-page/4 MiB store limits, and existing URL/field normalization remain
  compatible. Loading now admits only a stable regular non-symlink file through
  a limit+1 capped reader. Oversized and special paths fail without read,
  backup, or mutation. Malformed admitted bytes default only after exact private
  synchronized no-clobber backup publication; the source remains unchanged.
  Current-namespace retention is four files/16 MiB under a 4,096-entry scan
  ceiling and ignores legacy names. Saves reject unsafe targets before private
  synchronized same-directory atomic replacement. Prune/remove/matching-remove/
  clear now restore their prior in-memory state on persistence failure.
  Exact/next-byte, current/legacy, directory/symlink, source/backup preservation,
  retention, Unix mode, item/field normalization, mutation rollback, and
  injected pre-commit fault tests pass in isolated roots. No dependency, wire,
  configuration default, or accepted schema change. Roll back form-state
  admission/publication/backup/transaction helpers, tests, and documentation
  together. Completion gate: native Windows/macOS execute the focused suite.
- Directory-store persistence and admission: numeric trust encoding, current
  JSON, announcement debounce/cooldown, six-hour transient aging, 256-item
  announce stream, 1,024 transient entries, and saved/preferred-delivery/
  identify behavior remain compatible. The store now admits only a stable
  regular non-symlink file through an 8 MiB+1 capped reader and retains at most
  4,096 entries. Live inputs reject destination/associated hashes above 1 KiB
  and display names above 16 KiB before mutation. Oversized and special paths
  fail without read, backup, or mutation. Malformed or semantically excessive
  admitted bytes default only after exact private synchronized no-clobber
  backup publication; the source remains unchanged. Current-namespace retention
  is four files/32 MiB under a 4,096-entry scan ceiling and ignores legacy
  names. Saves use private synchronized same-directory atomic replacement;
  immediate saved/trust/delivery/identify/clear mutations restore memory on
  commit failure. Exact/next-byte, syntax/semantic, directory/symlink,
  source/backup preservation, retention, Unix mode, live-string rejection,
  mutation rollback, compatibility, and injected pre-commit fault tests pass.
  No dependency, wire, configuration default, or accepted JSON schema change.
  Roll back directory admission/publication/backup/transaction helpers, tests,
  and documentation together. Completion gate: native Windows/macOS execute
  the focused suite and live Reticulum announce qualification stays within the
  retained limits.

- Interface-profile and generated-config persistence: the accepted profile and
  gateway JSON plus rendered Reticulum configuration remain compatible.
  Profiles now admit only a stable regular non-symlink file through a 2 MiB+1
  capped reader, retain at most 64 profiles and 64 peers per profile, and reject
  CR/LF/NUL injection in bounded text fields. Gateway presets use the same
  admission with a 1 MiB/256-item ceiling; legacy migration validates a single
  admitted snapshot, publishes it privately, and leaves the source untouched.
  Existing generated config is admitted through a 1 MiB ceiling before its
  instance/network identity is preserved. All three targets use unique
  owner-only synchronized same-directory staging and shared atomic replacement;
  profile create/update/toggle/delete and gateway enable/create restore memory
  on commit failure. Exact/next-byte, item, injection, directory/symlink,
  migration/source preservation, Unix mode, identity, rollback, compatibility,
  and injected pre-commit fault tests pass. No dependency, wire, configuration
  default, or accepted schema change. Roll back interface admission,
  validation, publication/transaction helpers, tests, and documentation
  together. Completion gate: native Windows/macOS execute the focused suite and
  a live isolated Reticulum qualification preserves the generated identity.

- Native LXMF attachment admission/publication: the Python-compatible field key,
  `[name, binary]` entry layout, signed wire representation, and missing-path
  skip behavior remain compatible. Outbound sources now admit at most 64 stable
  regular non-symlink files through an 8 MiB+1 capped reader and reject more
  than 16 MiB aggregate before field construction completes. Inbound extraction
  applies the same item/per-file/aggregate ceilings plus a 4 KiB filename limit.
  Accepted files use deterministic message/index/name paths, owner-only
  synchronized same-directory staging, shared atomic replacement, and Unix
  directory synchronization; replay replaces the same path rather than
  proliferating suffixes. Unsafe roots and linked/non-regular destinations fail
  without touching referents. Exact/next-byte, item, aggregate, missing-source,
  symlink, replay, Unix mode, compatibility, and injected pre-commit fault tests
  pass. No dependency, wire, configuration default, or retained message-schema
  change. Active clean direct-link, full-wire, resource, and propagation-sync
  paths now move decode plus attachment I/O through one two-job blocking gate.
  The closure owns its permit, so cancellation cannot leak capacity or admit a
  third job; deterministic eight-job saturation and aborted-waiter tests pass.
  Roll back attachment admission/publication and blocking-boundary helpers,
  call sites, tests, and documentation together. Completion gate: native
  Windows/macOS execute the focused suite and live direct/propagated LXMF
  interop confirms attachment compatibility and acceptable receive latency.

## Baseline validation record

- Host/time: 2026-07-11T19:40:26-04:00, Linux x86_64
  `7.1.3-2-cachyos`.
- Rust: `rustc 1.97.0 (2d8144b78 2026-07-07)`, Cargo 1.97.0,
  `stable-x86_64-unknown-linux-gnu`.
- Lockfile: 7,353 lines, SHA-256
  `4a0fdedb87b0f64a399a8be752bbdb76891bf6ed0d0c3a2776a21d153273ba41`.
- Repository manifests: root package and standalone `src/server`; vendored and
  immutable reference-source manifests are not workspace members. No
  `Config.toml` exists in the checkout; runtime `config.toml` files are generated
  beneath explicit browser/server roots and were not read.
- Installed: rustfmt 1.9.0, Clippy 0.1.97, nextest 0.9.140, cargo-audit
  0.22.2, cargo-deny 0.20.2, cargo-llvm-cov 0.8.7, cargo-bloat 0.12.1,
  perf 7.1.3. Missing: cargo-packager, cargo-llvm-lines, samply, Podman/Docker.
  A continuous-build appimagetool is installed, but is not an approved pinned
  release input.
- Baseline format, root default tests (637 library tests plus integration),
  isolated mock tests (407 library tests plus integration), live product tests
  (824 library tests plus integration), server no-default tests (202), and
  server live tests (207) passed. `scripts/release-check.sh quick` passed.
- `cargo tree` proves the old documented additive live command enabled
  `mock-runtime`; the same graph with `--no-default-features` did not.
- `cargo audit --locked` was not runnable because cargo-audit 0.22.2 rejects
  that flag. `cargo deny check` could not fetch missing crate sources because
  network access was unavailable; neither result is reported as a pass.
- Post-change strict Clippy is not green on Rust 1.97: the root product reports
  39 pre-existing library warnings (48 with test targets), led by large error
  variants and new-version style lints; standalone `omenchatd` reports two
  pre-existing warnings (`if_same_then_else`, `needless_return`). This Phase 1A
  unit does not suppress or collateral-fix them.

## Phase 0 measurement harness

All automated runs must create roots with `mktemp -d` and export browser config,
data and cache directories or pass `--app-root`; server commands must pass
`--home`. Refuse paths under normal Reticulum/NomadNet/LXMF/browser/server homes.

1. Build the canonical release binaries, record `/usr/bin/time -v`, binary
   sizes, `cargo tree -d`, and the product feature assertion.
2. Desktop idle: launch the release binary on an isolated root, warm for 60 s,
   sample for 10 min with `pidstat -rud -p PID 1`, `/proc/PID/status`,
   `/proc/PID/smaps_rollup`, `/proc/PID/fd`, and `perf stat -p PID`; repeat with
   Monitoring open. Record median/p95 CPU, RSS/private dirty, context switches,
   messages/wakeups where instrumented, FDs, and startup-to-interactive.
3. Pane fixture: restore a deterministic workspace containing 20 browser, 20
   LXMF, and 10 OMENchat panes; time restore, representative page render and
   message-to-visible latency, then repeat close/reopen. The fixture generator
   must write only beneath its isolated root.
4. Media fixture: bounded valid and adversarial image/GIF corpus; record decode
   latency and peak RSS with panes visible, hidden and closed.
5. Server: run the packaged loopback/two-client/resource smoke against isolated
   roots; sample RSS, FDs and database latency. Once F-002 lands, record queue
   items/bytes/oldest age while producing 10x consumer rate and during reconnect
   storms. Once upload fault hooks exist, exercise every replacement boundary.
6. GPU/frame submissions are hardware/session specific. On Linux use
   `intel_gpu_top`, `nvtop`, or vendor tooling when available and capture the
   compositor/backend; otherwise mark pending. Never invent a zero value.

`scripts/measure-desktop-idle.sh` automates release launch, isolated-root and
optional Xvfb/i3 session creation, startup/normal-close timing, interval CPU
from `/proc` tick deltas, RSS/private-dirty/FD/context-switch samples, raw
`pidstat`/`perf stat`, and optional separate `perf record` capture.
`scripts/compare-desktop-idle.sh` rejects mismatched durations and keeps the OS
scheduler proxy distinct from verified application-message counts. On
2026-07-13, an archived `ce3a964` tree and the current product each completed a
60-second warmup plus 600 one-second samples. Baseline/current median CPU was
1.963%/0.000%, p95 CPU 3.908%/2.940%, task-clock 14,301.54/4,600.45 ms, median
RSS 216,740/179,754 KiB, private dirty 66,928/6,404 KiB, and verified recurring
application messages 60/0 per minute. Separate 30-second call-graph captures
had zero lost samples. Raw evidence remains in the reported temporary result
directories.

`scripts/measure-pane-stress.sh` now supplies the deterministic pane scenario.
It creates production-format state only beneath a disposable temporary root,
uses external Reticulum instance mode to prevent identity creation/storage-scope
changes, restores 20 browser, 20 LXMF conversation, and 10 OMENchat panes, and
verifies three normal close/reopen cycles plus final settings/SQLite restoration.
On 2026-07-13 the canonical non-mock product measured startup-to-window at
354 ms median/406 ms p95, CPU at 2.014%/2.513%, RSS at 233,040/233,148 KiB,
private dirty at 54,592/54,604 KiB, 60/60 file descriptors, and normal-close
latency at 218/221 ms. These are same-host reproducibility values, not a
cross-platform performance claim. Representative page-render and
message-to-visible latency, interactive media phases, server scenarios, and
vendor-specific GPU measurement remain pending.

`scripts/measure-omenchatd-db.sh` supplies the sustained standalone database
worker scenario. It drives the production session engine, frame decoder,
single-admission live worker, and persistent SQLite store with eight isolated
peers, samples worker/RSS/FD/database metrics and a separate 10 ms Tokio
heartbeat, then reopens and integrity-checks the database. On 2026-07-13 the
60-second release run committed 6,000 consecutive events, rejected 42,000 busy
submissions, measured 355/1,272 us average/maximum worker latency and 1,817 us
maximum heartbeat lateness, grew RSS by 794,624 bytes, and held FDs at 13.
This closes the live-worker sustained-load measurement, not Reticulum wire.
`scripts/test-omenchatd-crash-recovery.sh` separately kills
child processes at committed/open event-transaction boundaries and all four
upload replacement boundaries, then verifies committed retention or explicit
fail-closed reconciliation, safe repair, consecutive IDs, and SQLite integrity.

`scripts/measure-omenchat-media.sh` supplies the reproducible media procedure.
It generates a deterministic two-frame 1x1 GIF under a temporary isolated root,
keeps one release process alive across visible, maximized-hidden,
section-hidden, and closed phases, records raw and median/p95
CPU/RSS/private-dirty/FD/context-switch evidence, runs an ignored release-mode
production-decoder latency measurement, and emits vendor GPU capture commands.
A decoder-harness smoke on 2026-07-12 completed 200 samples at 1,857 ns median
and 3,742 ns p95, with reported process RSS 6,840 KiB before and 6,980 KiB
after. The tiny fixture makes this reproducibility evidence, not the native
visible/hidden release gate. Interactive phase and GPU captures remain pending.

`scripts/measure-runtime-threads.sh` runs an ignored optimized comparison of
the legacy fixed-four, adaptive maximum-four, and Tokio host-default policies.
It records available parallelism, actual runtime workers,
global queue depth after a 5,000-task burst, median/p95 queue-to-completion
latency, and elapsed time for 32 real 256 KiB atomic writes through the shared
two-permit blocking gate. All files live under a generated temporary root.
On the 28-CPU Linux reference host, the 2026-07-12 release run measured fixed
versus default median latency of 6.268 versus 11.651 us, p95 of 19.708 versus
38.321 us, and bounded-write elapsed time of 2.535 versus 2.254 ms. This is a
reproducible synthetic baseline, not evidence to tune the product from one
high-core machine. Under a two-core Linux affinity, the adaptive/legacy
fixed-four policies measured 11.938/881.120 us median, 40.609/1,402.347 us p95,
queue depth 2/308, and bounded-write elapsed time 2.999/4.707 ms. This supports
the one-per-available-CPU, maximum-four policy. Native low-core and live
Reticulum latency runs remain.
