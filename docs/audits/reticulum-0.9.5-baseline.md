# Reticulum/LXMF 0.9.5 baseline audit

Audit date: 2026-07-21  
Reviewed commit: `c6ad96d3e083425a62e6713abe8598c4d494bde0` (`v0.9.5-2`)  
Branch at capture: `main`, equal to `origin/main`  
Host: Linux 7.1.3-2-cachyos, x86_64  
Toolchain: rustc 1.97.0, Cargo 1.97.0, stable; declared MSRV 1.85

This audit records the repository before the v0.9.5-3 improvement work. The
review artifact in `official-sources/` is guidance; the implementation and
locked dependency trees described here are the source of truth. No application
or protocol behavior was changed before these results were recorded.

## Package and workspace structure

- The repository root is the `omenbrowser_rs` package and application build
  root. It is version `0.9.5-2`, has no default features, and declares Rust
  1.85.
- `src/server/` is a separate Cargo workspace and package root for `omenchatd`.
  It has its own manifest, lockfile, target context, configuration, identity,
  database, Reticulum configuration/storage, tests, and release artifact. It is
  also version `0.9.5-2`, has no default features, and declares Rust 1.85.
- `src/server/crates/omen-ifac-tcp/` is the narrow private IFAC-compatible TCP
  support crate. It is version `0.9.5-1` and is used by both roots without
  becoming an independent Reticulum implementation.
- `fuzz/` is an independent fuzzing root. `official-sources/` and
  `vendor/rns-net/` contain reference material and are not production workspace
  members. The legacy `rns-net` tree is not in either production dependency
  graph.

Canonical product profiles are:

- desktop: `--no-default-features --features desktop-product`;
- TUI: `--no-default-features --features tui`;
- omenchatd headless: `--no-default-features --features server-headless`;
- omenchatd TUI: `--no-default-features --features server-full`.

`desktop-product` enables Reticulum-backed OMENchat, bounded GIF/media support,
and portable SQLite. `desktop-dev` adds `mock-runtime`; the release profile does
not. `server-full` adds only the Ratatui UI to `server-headless`.

## Locked Reticulum/LXMF train

Both production roots resolve one registry-sourced 0.9.5 family. Direct pins
are exact to prevent an implicit train change:

| Root | Direct dependency | Version/features |
|---|---|---|
| browser | `reticulum-rs` | `=0.9.5`, defaults off, `core`, `transport` |
| browser | `reticulum-rs-transport` (`rns_transport`) | `=0.9.5` |
| browser | `reticulum-rs-rpc` (`rns_rpc`) | `=0.9.5` |
| browser | `lxmf` | `=0.9.5`, defaults off, `wire` |
| browser | `lxmf-sdk` (`lxmf_sdk`) | `=0.9.5`, defaults off, `std`, `sdk-async`, `rpc-backend` |
| server | `reticulum-rs` | `=0.9.5`, defaults off, `core`, `transport` |
| server | `reticulum-rs-transport` (`rns_transport`) | `=0.9.5` |
| IFAC support | `reticulum-rs-transport` | `=0.9.5` |

The resolved trees also contain `reticulum-rs-core`, `lxmf-wire`, and the
reference crates at 0.9.5. The train verification found no resolved production
0.6 package and no Git/registry source split. No embedded Reticulum crate is a
desktop or server production dependency.

## Runtime and identity ownership

The supported browser mode is managed/integrated Reticulum. OMENbrowser owns
runtime startup, interface configuration, identity attachment, event handling,
and orderly shutdown. The configured `external` value remains readable, but it
fails closed as `external_deferred`; it does not silently start network work or
claim a negotiated shared instance.

`omenchatd` independently owns its headless Reticulum runtime. It does not
import the browser UI or application storage. Browser/client identities and the
server identity remain distinct. Identity parse, permission, symlink, or size
failures do not silently regenerate identities.

Each explicit browser application root owns its settings, managed identities,
Reticulum configuration/storage, messages, caches, plugins, and transient
delivery state. The normal omenchatd root independently owns its `Config.toml`,
identity, SQLite database, uploads, portal page, logs, and Reticulum
configuration/storage. Tests and smoke scripts use temporary explicit roots;
none of the commands below used a maintainer's live data root.

## OMENchat protocol and compatibility

The live protocol is deliberately still version 1 / `omenchat-v0.1`. App
version changes do not alter its destination naming, six-item MessagePack
frame, 32-bit sequence field, database schema version, or storage paths.
Descriptor and session capabilities are separately bounded and negotiated.

Current replay safety is scoped correctly but narrowly:

- omenchatd caches exact room mutation results by live Link and sequence;
- an exact same-Link replay returns the stored result without repeating the
  database mutation;
- reuse of the same Link/sequence with different content is rejected;
- closing/replacing the Link removes its replay entries;
- the cache is bounded to 1,024 entries / 4 MiB globally, 64 entries / 256 KiB
  per Link, and 64 KiB per entry.

This is not cross-Link or post-restart idempotency. The current client does not
silently resend an uncertain mutation after Link loss. A durable mutation
extension therefore requires explicit capability negotiation, persistent
operation identity, a canonical request hash, and transactional server replay
storage; it cannot safely reinterpret the existing `seq` field.

## Existing resource and lifecycle bounds

The supplied review's broad concern about unbounded payload work is already
substantially addressed by current code. Confirmed examples include:

- omenchatd transport payload queue: 256 items / 16 MiB, with a separate
  32-item control path and per-Link byte fairness;
- server event payload queue: 512 items / 32 MiB, with a separate 64-item
  control path and per-Link byte fairness;
- active Links: 256; pending handshakes: 32;
- pending outbound Resources: 64 items / 16 MiB, 4 MiB per entry;
- pending uploads: 256 globally and 8 per identity;
- admin database admission: 16 operations, with one bounded blocking database
  operation in flight;
- structured server log queue: 896 items / 768 KiB;
- client sessions: 64; retained session events: 1,024 / 8 MiB;
- client resource cache: 16 items / 16 MiB; an individual inbound OMENchat
  Resource is capped at 8 MiB;
- client transport frames: 64 items / 4 MiB; Resources: 4 items / 16 MiB;
- pending client Resource offers: 32 items / 4 MiB;
- media work: 16 jobs / 16 MiB; media metadata: 256 items / 256 KiB;
- encoded GIF: 8 MiB; decoded GIF: 64 MiB, 128 frames, 4,096-pixel maximum
  dimension, 12 decoded cache items;
- identity-scoped media disk cache: 64 items / 128 MiB;
- reconnect delay is exponential and bounded at five attempts and 30 seconds;
  it resets only after a 30-second stable Link period.

Queue admission exposes item/byte/oldest-age/rejection metrics. Shutdown paths
own cancellation and draining. The baseline searches did not find an unbounded
Tokio payload channel in the browser or server production paths. Standard
library channels found in server tests are synchronization fixtures, not
production payload queues.

## GUI/TUI state sharing and duplication

GUI and TUI share the core browser, messaging, directory, runtime, protocol,
storage, and diagnostics models. omenchatd's TUI uses the standalone server
domain and database APIs. Platform shells still contain separate presentation
reducers, keyboard handling, and view-local caches, which is appropriate.

The largest maintainability risk is `src/app.rs` (about 1.4 MiB / 23,000
lines). It contains cohesive reducers that can be extracted mechanically
without redesigning application state. The first selected slice is diagnostics
task-result reduction (`DiagnosticsTaskResult` and
`App::apply_diagnostics_task_result`), roughly 420 lines. This preserves the
existing project-owned state and avoids a generic event framework.

## Validation results before modification

All commands below used the reviewed commit and lockfiles.

Passed:

```text
cargo fmt --all --check
cargo check --locked --no-default-features --features native-lxmf
cargo check --locked --no-default-features --features desktop-product
cargo check --locked --no-default-features --features tui
cargo test --locked --no-default-features --features desktop-product
cargo clippy --locked --no-default-features --features desktop-product --all-targets -- -D warnings

(cd src/server && cargo fmt --all --check)
cargo check/test --manifest-path src/server/Cargo.toml --locked --no-default-features --features server-headless
cargo check/test --manifest-path src/server/Cargo.toml --locked --no-default-features --features server-full
cargo clippy --manifest-path src/server/Cargo.toml --locked --no-default-features --features server-headless --all-targets -- -D warnings
cargo clippy --manifest-path src/server/Cargo.toml --locked --no-default-features --features server-full --all-targets -- -D warnings

bash scripts/release-check.sh quick
bash scripts/verify-reticulum-train.sh
bash scripts/verify-product-features.sh
bash scripts/verify-accepted-advisories.sh --no-fetch
bash scripts/run-pinned-python-reticulum.sh
cargo deny --manifest-path Cargo.toml check licenses bans sources
cargo deny --manifest-path src/server/Cargo.toml check licenses bans sources
cargo audit --file src/server/Cargo.lock
```

The root desktop test run reported 1,253 library tests plus integration suites
passing. The server headless run reported 196 passed / 7 explicit measurement
tests ignored; full reported 318 passed / the same 7 ignored. Strict Clippy
produced no compiler or Clippy warning in the selected production profiles.

The pinned Python lane passed against RNS commit
`15320e4d2cfabb143c1db20ca887e275fd521585` and LXMF commit
`727830cefda83d9c6e3982b48675425f3f988f9c`. Evidence included deterministic
destination/IFAC vectors, bidirectional IFAC TCP split/coalesced framing and
credential rejection, proof correlation, LXMF propagation, stamp boundaries,
ticket issue/use/expiry/reuse, direct policy discovery, a 65,536-byte stamped
Resource, and restart receipt recovery.

Expected policy failure:

```text
cargo audit --locked
```

The root audit exits 1 for accepted build-time-only `quick-xml 0.39.2`
advisories RUSTSEC-2026-0194 and RUSTSEC-2026-0195 through
`wayland-scanner 0.31.10`. The repository's path-constrained acceptance script
passed and confirmed that omenchatd has no `quick-xml`. Audit also reported the
already-reviewed unmaintained warnings for `bincode 1.3.3`, `paste 1.0.15`,
`rustls-pemfile 2.2.0`, `rustybuzz 0.20.1`, and `ttf-parser 0.25.1`. These are
upstream/dependency maintenance observations, not reasons for an unrelated
broad upgrade in this work.

One measurement setup failure was recorded rather than hidden: the first
desktop idle attempt used an existing release binary last built with the TUI
profile and correctly failed with `desktop UI unavailable`. Rebuilding with
the canonical explicit product command fixed the measurement setup; no source
change was involved.

## Resource baseline

These short local samples establish reproducible evidence, not universal
release thresholds.

Canonical desktop product, isolated Xvfb/i3 root, 10-second warmup and
30-second sample:

- startup to visible window: 1,181 ms;
- normal close: 172 ms;
- CPU median 0.000%, p95 3.934%;
- RSS median 223,844 KiB, p95 223,872 KiB;
- private dirty median 42,760 KiB, p95 42,828 KiB;
- FDs median/p95 60;
- scheduler context-switch proxy: 64.138/minute.

The harness identified recurring application-message count as pending. Software
rendering under Xvfb is not a meaningful physical GPU utilization result.

Fifteen-second omenchatd SQLite saturation fixture:

- 1,500 accepted and committed operations; 10,500 rejected at bounded busy
  admission;
- one operation maximum in flight;
- average worker latency 342 us; maximum 1,279 us;
- heartbeat maximum 1,895 us;
- RSS 9,216,000 -> 9,826,304 bytes; FDs stable at 13;
- SQLite integrity `ok`.

Fifteen-second slow-consumer/backpressure fixture:

- transport peak 256 items / 16,718,820 bytes;
- event peak 513 observed items / 33,554,432 bytes (the figure includes the
  separately bounded control lane);
- control latency maximum 21 ms;
- all final item and byte counters returned to zero;
- peak FDs 11; RSS growth 53,489,664 bytes within the fixture's 112 MiB bound.

Raw ignored evidence is under `target/audit-reticulum-0.9.5/`. Maintainers can
repeat longer runs with:

```text
HEADLESS=1 WARMUP_SECONDS=60 SAMPLE_SECONDS=600 INTERVAL_SECONDS=1 \
  bash scripts/measure-desktop-idle.sh <output-dir>
OMENCHATD_DB_SOAK_SECONDS=60 bash scripts/measure-omenchatd-db.sh <output-dir>
OMENCHATD_QUEUE_SOAK_SECONDS=60 bash scripts/measure-omenchatd-backpressure.sh <output-dir>
bash scripts/measure-omenchatd-links.sh <output-dir>
bash scripts/measure-omenchatd-logging.sh <output-dir>
bash scripts/measure-runtime-threads.sh --two-core
bash scripts/measure-pane-stress.sh <output-dir>
```

## Tests unavailable or intentionally not claimed

- Physical GPU activity and frame submissions: unavailable in the headless
  software-rendered session. Measure on each release GPU/driver family.
- Hardware RNode, Serial/KISS, Meshtastic, BLE, VR-N76, I2P, and public-network
  behavior: no corresponding hardware/peer was present.
- Native Windows and macOS runtime/package behavior: this Linux host cannot
  prove those gates; native CI remains authoritative.
- Long 60-second Link, database, queue, and log soaks and the multi-process
  maximum-Resource cancellation fixtures remain explicit ignored tests. Short
  DB/queue variants were run above; the longer release fixtures remain
  maintainer/CI commands.
- Interactive real-user GUI redraw/GPU observation and live public NomadNet,
  LXMF propagation-node, or third-party OMENchat peer testing were not claimed.
- The current-Python drift lane was not substituted for the pinned release
  lane; it remains a separately versioned informational compatibility run.

## Confirmed risks and review reconciliation

Confirmed correctness risks:

1. OMENchat mutation replay identity ends at the current Link. A commit followed
   by lost acknowledgement remains uncertain across reconnect or restart.
2. `src/app.rs` is oversized and increases regression/review cost even though
   its present tests are strong.
3. External/shared Reticulum is configuration-visible but deliberately not
   implemented; diagnostics must continue to report it as deferred rather than
   active.
4. The local IFAC adapter and upstream maximum-Resource limitation remain
   evidence-bound compatibility boundaries and must fail closed.

Confirmed resource risks:

1. The desktop's settled RSS is material on low-resource systems and needs
   comparable post-change measurements; this audit does not attribute it to a
   specific cache or runtime.
2. Server saturation intentionally consumes bounded queue memory. Fairness and
   control latency pass, but longer soak evidence remains important.
3. GUI recurring application-message/redraw and hardware GPU metrics are not
   automated, so regressions require the documented manual/platform procedure.

Already solved or materially narrower than the supplied review suggests:

- production product features are deterministic and mock-free;
- Reticulum/LXMF dependencies already form an exact coherent 0.9.5 train;
- managed runtime ownership and server storage independence are explicit;
- queues, media, Resources, histories, reconnects, logging, SQLite work, and
  replay caches already have item/concurrency and byte/time bounds;
- shutdown/terminal restoration, durable uploads, SQLite recovery, propagation
  status, typed LXMF delivery/events, pinned Python interoperability, package
  profiles, and accepted-advisory boundaries already have focused tests;
- the protocol documentation truthfully distinguishes local admission,
  transport acceptance, receipt evidence, and final delivery.

## Prioritized recommendation

1. Mechanically extract diagnostics task-result reduction from `src/app.rs`,
   preserving behavior and running focused plus product-profile tests.
2. Add explicit model tests for the seven uncertain OMENchat mutation outcomes.
   Do not add automatic mutation retry.
3. Produce a review checkpoint for a capability-negotiated durable mutation
   identity and transactional replay store. Pause before changing the live wire
   contract or SQLite schema.
4. Only after that checkpoint is approved, implement one bounded compatibility
   slice with mixed-version and crash-boundary tests.
5. Re-run the same resource commands and version the application as v0.9.5-3
   only after the behavioral unit and release gates are complete.

## First maintainability slice (post-baseline)

After the preceding baseline was frozen, diagnostics result reduction was moved
from the root `src/app.rs` implementation into the private
`src/app/diagnostics_results.rs` child module. The
`DiagnosticsTaskResult` type, `App::apply_diagnostics_task_result` signature,
state ownership, return values, logs, status messages, and callers are
unchanged. The child module imports the existing parent boundary; it does not
introduce a framework, dependency injection, worker, queue, timer, or crate.

A module-local regression test now proves that a stale diagnostics export
generation cannot clear or replace the current pending generation. The existing
known-destinations reducer regression also passed before and after the move.

Post-extraction validation:

```text
cargo fmt --all --check                                      pass
cargo test --locked --no-default-features --features desktop-product
                                                               pass
cargo clippy --locked --no-default-features --features desktop-product \
  --all-targets -- -D warnings                                pass
cargo check --locked --no-default-features --features tui     pass
```

The full run reported 1,254 library tests passing, including
`app::diagnostics_results::tests::stale_export_result_does_not_replace_the_pending_generation`,
with the existing explicit measurement tests still ignored. There is no
storage, protocol, configuration, runtime, or resource-use impact. Rollback is
the mechanical restoration of the method body to the parent module and removal
of the private child module.

## Retry-safety investigation (post-extraction)

Seven focused standalone omenchatd tests now characterize the protocol v1
mutation boundary without changing it. They prove commit-with-lost-response,
Link-close cleanup, new-Link replay, client restart, server restart, exact
same-Link duplicate, and different-content collision behavior. All seven pass
under `server-headless`. The new-Link/client-restart/server-restart cases
deliberately assert that an externally forced v1 resend produces a second event;
this prevents future code from mistaking `seq` for durable identity and does not
add an automatic resend path.

The proposed capability, wire envelope, schema v3, retention, crash boundaries,
mixed-version behavior, shared protocol-crate boundary, and test matrix are
recorded in `docs/design/OMENCHAT_DURABLE_MUTATION_CHECKPOINT.md`. No protocol,
schema, configuration, runtime, or persistent client state has changed. That
unit paused at the compatibility checkpoint; the checkpoint was subsequently
accepted for staged implementation, with schema/wire changes still gated by
their focused tests.

## Shared protocol boundary (post-checkpoint approval)

The compatibility-only first retry-safety implementation unit created the
private `omenchat-protocol 0.1.0` crate under the standalone server tree. Root
and server manifests/lockfiles now resolve that same local package. Browser and
server retain their existing `chat::protocol` / `protocol` module paths through
re-exports, and both codecs retain their existing allocation limits and
implementations.

The shared crate owns only existing protocol v1 enums, operation/error numbers,
frame body/value types, and the v0.6.0-1 compatibility fixture. It has no
Reticulum, Tokio, SQLite, Iced, Ratatui, filesystem, worker, queue, cache, or
policy dependency. At that compatibility-only stage its sole dependency was
`thiserror`, already present in both resolved graphs. No runtime dependency
family or version changed.

Focused browser/server codec tests encode and decode the same fixture bytes,
the shared crate locks public numbers/labels, full product and server tests and
strict Clippy pass, and the copied standalone server compiles/tests offline.
There is no wire, configuration, schema, storage, runtime, or resource-use
change. Rollback restores the two small local protocol definition modules,
removes the path dependencies/workspace member, and removes the shared crate.

## Durable mutation contract types (non-activating unit)

The shared protocol crate now defines the checkpoint's fixed-size client
instance ID, mutation ID, and request hash, plus the proposed durable envelope
shape and canonical request hash. Canonicalization is domain-separated,
streamed into SHA-256, independent of Rust layout/debug output, and bounded by
scalar, container, total-value, nesting-depth, and encoded-byte ceilings. A
fixed digest vector locks operation, room, body-kind, and body-content
semantics. Envelope creation and parsing reject invalid lengths, mismatched
body kinds, and canonical-limit violations.

This unit adds `sha2`, which was already resolved in both application graphs;
it does not introduce a new dependency family. The capability is not requested
or accepted, the live codecs do not recognize the envelope, no mutation is
automatically retried, and no database/configuration/state schema changes.
Accordingly it creates no worker, queue, timer, cache, I/O, or runtime resource
impact. Rollback removes `durable.rs`, its module re-export, and the direct
`sha2` declaration without affecting protocol v1 peers or stored data.

Validation for the shared boundary and non-activating durable contract:

```text
cargo test --locked --manifest-path src/server/Cargo.toml \
  -p omenchat-protocol                                      pass (5)
cargo clippy --locked --manifest-path src/server/Cargo.toml \
  -p omenchat-protocol --all-targets -- -D warnings         pass
cargo test --locked --no-default-features \
  --features desktop-product                               pass (1,226 + 28 ignored)
cargo check --locked --no-default-features --features tui  pass
cargo clippy --locked --no-default-features \
  --features desktop-product --all-targets -- -D warnings  pass
cargo test --locked --manifest-path src/server/Cargo.toml \
  --no-default-features --features server-headless         pass (203 + 7 ignored)
cargo test --locked --manifest-path src/server/Cargo.toml \
  --no-default-features --features server-full             pass (325 + 7 ignored)
cargo clippy --locked --manifest-path src/server/Cargo.toml \
  --no-default-features --features server-headless \
  --all-targets -- -D warnings                             pass
cargo clippy --locked --manifest-path src/server/Cargo.toml \
  --no-default-features --features server-full \
  --all-targets -- -D warnings                             pass
bash src/server/scripts/verify-standalone.sh check          pass
bash scripts/release-check.sh quick                        pass
cargo deny check                                            pass with reviewed warnings
cargo deny --manifest-path src/server/Cargo.toml check     pass with reviewed warnings
git diff --check                                            pass
```

The first standalone `cargo-deny` invocation placed `--manifest-path` after
`check`, which that installed CLI rejects with exit code 2. It was rerun with
the correct option order and passed; this was a command invocation error, not a
dependency-policy failure. Explicit 60-second soak measurements, ignored live
Reticulum/Resource tests, pinned-Python process interoperability, and native
Windows/macOS packaging were not rerun for this type-only unit. They require
the documented opt-in peers, duration, or native CI environments and remain
release gates once the extension becomes live.

## Capability negotiation contract (inactive unit)

Current implementation inspection corrected the checkpoint before wire work:
`SessionOpen` field 2 is already the deployed optional client LXMF destination,
so requested capabilities and client-instance ID are assigned only to trailing
fields 3 and 4. `SessionAccept` accepted capabilities remain trailing field 6.
The shared protocol crate now owns bounded builders/parsers for these optional
fields, rejects duplicate/invalid/oversized capability names and invalid client
IDs, and requires a client-instance ID for `durable-mutations-v1`.

Ten shared contract tests cover legacy absence, exact field preservation,
explicit acceptance, malformed IDs, duplicates, invalid names, and count
bounds. A standalone omenchatd integration test proves that a well-formed
durable request currently receives the unchanged six-field legacy accept with
no implicit capability. Neither browser nor server advertises or accepts the
capability in production, so there is still no durable-envelope transmission,
automatic uncertain retry, persistent client instance, or schema change.

## Persistent client-instance foundation (capability still inactive)

Desktop startup now owns one random 16-byte OMENchat client-instance ID per
active identity-scoped application root at `omenchat/client-instance-id`. The
store publishes from a same-directory owner-only staging file using atomic
create-without-replacement, synchronizes the file and directory, and makes
concurrent first starts converge on the winning value. Existing wrong-size,
symlinked, special, or permissively readable state fails closed and is not
rewritten or regenerated. Startup retains a successfully loaded value in the
owned live-client state; failure is warning-reported without logging its bytes.

Six isolated storage tests cover creation/reuse and Unix modes, corrupt-state
preservation, pre-commit failure cleanup, concurrent first creation, symlinked
file/parent refusal, and permissive-file refusal. A desktop integration test
proves restart reuse from the same identity root. This adds one 16-byte durable
file and no queue, cache, timer, retry, worker, schema, or network traffic. The
client still does not add negotiation fields to `SessionOpen`, and omenchatd
still cannot accept the capability.

## Dormant omenchatd schema-v3 replay boundary

The next bounded server unit advances SQLite `user_version` from 2 to 3 and
adds only the checkpointed `durable_mutation_results` table and deterministic
creation-order index. No production request path reads or writes the table, no
capability is accepted, and no mutation is retried. Fixed-size SQL checks cover
the 16-byte client/mutation identifiers and 32-byte request hash; live storage
code must still enforce the 64 KiB result and retention ceilings before use.

A version-2 fixture proves rooms and upload-ledger rows survive migration, the
generated pre-v3 backup remains at version 2 without the new table, and the
existing recovery suite proves a selected older backup migrates through a
private staging database before atomic restoration. Existing injected-failure,
backup-collision, future-schema, and integrity tests continue to cover the
common migration machinery. Rollback requires stopping omenchatd and using the
documented guarded restore command with the generated
`omenchat.sqlite.pre-v3-from-v2.bak`; an older binary cannot directly open the
v3 database.

Focused validation passed formatting, the v2-to-v3 preservation test, and all
three database-recovery tests. The broader quick release gate was also rerun;
its format, feature/version/dependency-train, accepted-advisory, native CLI,
and isolated TUI checks passed, but the Linux real-PTY smoke timed out waiting
for single-SIGTERM shutdown. That gate is recorded as failed and is not hidden;
the schema/client-instance units do not touch TUI lifecycle code.

## Dormant durable replay operations

The schema-v3 table now has an isolated store boundary that performs lookup,
SQLite-only mutation work, result validation, retention, and replay insertion
inside one immediate transaction. An exact key/hash returns the original frame
without invoking the mutation callback; hash reuse with different content
returns conflict; malformed or larger-than-64-KiB results and retention
exhaustion roll the callback work back. Production ceilings remain 30 days,
100,000 items/64 MiB globally, 10,000 items/8 MiB per authenticated identity,
and at most 128 deterministic deletions for one admission.

Focused tests cover exact replay, conflict, callback execution once, invalid-
result rollback, oversized-result rejection, incremental pruning, and capacity
rollback. A two-connection race additionally proves SQLite serialization lets
exactly one callback execute while the other connection receives the stored
frame. The boundary creates no worker, queue, timer, or network traffic and
is not called by production session handling. Live activation remains blocked
on persisted outbound intents and a deterministic expired/pruned-key response;
without that rule, a sufficiently old retry could otherwise look new.

## Inactive outbound mutation-intent store

The desktop side now provides an identity-scoped, owner-only SQLite intent
store under `omenchat/mutation-intents.sqlite`, while startup and live send
paths remain disconnected. Preparing through the isolated API persists the
server destination, authenticated peer binding, client/mutation IDs, canonical
hash and request, expiry, prepared state, and local correlation before
returning. Admission is bounded to 4,096 intents/16 MiB and 64 KiB per intent;
it fails visibly without evicting existing pending or uncertain work.

Five isolated tests cover owner-only restart persistence, canonical recovery,
capacity preservation, corrupt-hash refusal without rewriting, allocation-safe
oversized-row preflight, and symlink/permissive-file refusal. Strict desktop-
product Clippy passes. The boundary adds no production file, worker, queue,
timer, retry, or network traffic until a bounded storage owner exists and
negotiated activation is safe.

The next inactive slice adds a single-owner storage thread with a
32-request/2-MiB bounded channel, pre-admission payload validation, immediate
overload rejection, queue-item/byte/rejection/completion metrics, and joined
draining shutdown. It performs monotonic compare-and-set
state transitions, bounded prepared/uncertain recovery, and at most 128 old
terminal deletions per maintenance request. Nine focused tests now cover the
store plus worker lifecycle, restart recovery, terminal-regression refusal,
terminal-only pruning, deterministic item saturation, oversized-payload
rejection before admission, recovery after release, and joined shutdown. The
application still does not start this worker.

## Deterministic dormant expired-key boundary

The schema-v3 replay store now retains a bounded client-instance registry in
addition to replay results. Whenever age/item/byte retention removes any result,
the store first permanently retires that complete authenticated
identity/client-instance pair. Later requests under that instance return the
typed internal `Expired` outcome before a SQLite mutation callback can run.
This avoids clock-dependent retention floors and prevents a pruned retry from
being accepted as a new operation after server restart.

The conservative tradeoff is coarse invalidation: clients must eventually
rotate the persistent instance only after all pending intents are resolved or
explicitly abandoned. Registry admission is bounded to 100,000 instances
globally and 1,024 per authenticated identity and fails closed before mutation
execution. Replay pruning remains limited to 128 rows per admission. Focused
tests cover restart-persistent expiry, callback non-execution, incremental
retirement, fail-closed client capacity, and preservation of both schema-v3
tables across the guarded v2 migration path. The tables remain disconnected
from live sessions.

## Inactive client-instance rotation and durable errors

The client intent store now owns a crash-safe rotation boundary. It acquires an
immediate SQLite transaction, rejects rotation while any prepared or uncertain
intent exists, and only then atomically replaces and synchronizes the
owner-only client-instance file. Terminal records retain their original ID.
Focused tests cover prepared and uncertain refusal, explicit abandonment,
restart persistence, stale expected IDs, injected pre-commit failure, and a
second SQLite connection being boundedly excluded during rotation.

The shared protocol crate reserves stable error numbers 1011–1015 for durable
not-negotiated, malformed, conflict, result-expired, and store-busy outcomes;
the client has display labels for forward-compatible diagnostics. Neither side
uses the codes in production yet. No worker, timer, retry, capability
advertisement, or network traffic was activated.

## Dormant durable-retention measurement

`scripts/measure-durable-mutation-retention.sh` now runs ignored, isolated
release-mode fixtures for the server replay store and client intent store. It
hard-checks exact retained/retired/recovered/pruned counts, 128-row incremental
client pruning, and bounded database sizes. Broad latency ceilings act as
regression triggers rather than hardware-independent performance claims.

The 2026-07-21 run at 1,024 items passed. Server replay storage retained 512
results and recorded 1,024 client instances with 512 retired; the checkpointed
database used 434,176 bytes. Server commit p50/p95/max was 439/536/692 µs and
exact replay was 30/39/58 µs. The client recovered and pruned all 1,024 intents
in eight bounded calls; its database used 364,544 bytes, prepare p50/p95/max was
156/199/400 µs, and full recovery took 41,839 µs. The detailed procedure and
activation thresholds are recorded in
`docs/maintenance/OMENCHAT_DURABLE_RETENTION_MEASUREMENT.md`.

The maximum 4,096-item run also passed: 2,048 replay results remained, 2,048
client instances were retired, and all client intents were recovered and
pruned in 32 bounded calls. Server/client databases were
1,282,048/1,388,544 bytes; server commit p95 was 1,390 µs, replay p95 was
40 µs, client prepare p95 was 360 µs, and client recovery was 139,768 µs.

## Fail-closed live negotiation parsing

The server session engine now validates optional trailing capability fields
before accepting a session. Valid durable and unknown capabilities remain
unsupported and receive the six-field legacy `SessionAccept`; malformed fields
receive durable error 1012. The live server now records handshake completion
only when the engine actually produced `SessionAccept`, rather than for every
inbound `SessionOpen` opcode. Focused tests prove malformed negotiation remains
pending and a corrected legacy request can recover on the same Link. No client
instance is retained and no durable envelope, retry, or mutation path is live.

## Transactional durable room-event boundary (inactive)

The server store now has a narrowly scoped persistence boundary that appends
one room event and retains its exact encoded origin response in the same
`BEGIN IMMEDIATE` SQLite transaction. The existing ordinary append path uses
the same internal event allocator, so event IDs, timestamps, actor names, and
payload encoding retain one implementation. A first execution returns the
stored event for future one-time fan-out; an exact replay returns only the
retained response bytes, making accidental rebroadcast structurally harder.
Hash conflicts and retired client instances execute no event work, and an
invalid or oversized origin response rolls back both the event and replay row.

This boundary remains dormant. It does not perform authorization, membership,
rate accounting, broadcasting, capability acceptance, or automatic retry.
Before live activation, rate admission must be arranged so an exact replay is
not charged twice and a rolled-back database mutation does not permanently
consume an in-memory rate slot. The live Link must also bind the negotiated
client instance to its authenticated identity and broadcast only the event
returned by a `Stored` result.

Local validation used the isolated standalone server root: the two focused
durable room-event tests passed; the headless suite passed 218 tests with eight
explicit hardware/soak/interoperability tests ignored; headless all-target
Clippy passed with `-D warnings`; the full server/TUI profile passed 340 tests
with the same eight explicit ignores; formatting passed; and the copied
standalone relocation check, compile-only test build, four IFAC fixture tests,
and doc tests passed. No remote CI was dispatched for this server-only inactive
unit; it is being batched with the next checkpoint to avoid a low-value
15-minute workflow.

## Authenticated Link/client-instance ownership boundary

Live session metadata is now staged as a candidate and committed to the Link's
peer record only after the engine returns `SessionAccept`. Malformed capability
negotiation therefore cannot change the retained display name or LXMF
destination. A corrected request on the same Link can still be accepted and
apply its metadata, preserving recovery behavior.

The live server also derives a durable client-instance binding only when a
valid `SessionOpen` requests `durable-mutations-v1` and the corresponding
`SessionAccept` explicitly lists that exact capability. It installs such a
binding only for an authenticated Link and scopes the value to the authenticated
identity. Identity replacement, capability downgrade, Link close, duplicate
replacement, administrative disconnect, and handshake retirement remove the
binding. Storage is bounded by the existing 256-active-Link ceiling. The
current server still returns a legacy accept, so a well-formed durable request
creates no binding and the capability remains inactive.

Focused malformed/recovery, unaccepted capability, explicit two-sided
acceptance, identity-change, and Link-close tests passed. The complete headless
profile passed 223 tests and the full server/TUI profile passed 345 tests, each
with eight explicit long-running, hardware, upstream-regression, or
interoperability tests ignored. Strict headless Clippy and formatting passed.

## Reversible rate admission (inactive durable handoff)

The existing per-identity message/command limiter now represents an admitted
slot with an owned reservation. Legacy handlers immediately commit that
reservation, preserving their prior behavior. Dropping an uncommitted
reservation restores the count only in the same active time window, so an
aborted database operation does not leak capacity or decrement a newer window.
Disabled limits allocate no reservation.

The dormant durable room-event transaction now returns an opaque admission
guard supplied by its completion callback. That callback runs only for a new
mutation after the replay lookup and while the SQLite transaction is active.
Exact replay and conflict never reserve another rate slot; invalid response or
transaction failure drops the guard and rolls back the event; successful first
execution returns the guard for explicit commit before any network fan-out.
No live handler calls this composition yet.

Focused reservation, legacy rate-limit, durable replay, and rollback tests
passed. The complete headless profile passed 220 tests and the full server/TUI
profile passed 342 tests, each with eight explicit long-running, hardware,
upstream-regression, or interoperability tests ignored. Strict headless Clippy
and formatting passed. This remains part of the local CI batch.

## Inactive durable room-message executor

The session engine now exposes a non-live executor for already-negotiated
durable `RoomMessage` and `RoomAction` envelopes. It verifies the canonical
operation/room/body hash and body bounds before storage. A replay miss evaluates
room existence, user policy, rate admission, membership, event insertion,
acknowledgement encoding, and replay publication inside one immediate SQLite
transaction. Permission and rate rejections are retained as the terminal
original result. SQLite busy/locked maps to error 1015 without a mutation;
malformed hash/body uses 1012; key conflict and retired instance use 1013 and
1014.

First execution returns the exact origin `MessageAck`, a one-use `RoomEvent`
for future fan-out, and the bounded prune count. Exact replay returns only the
stored origin bytes without repeating policy, rate accounting, membership,
event insertion, or broadcast. Tests prove one event/one fan-out, exact replay,
rate isolation, hash conflict, malformed hash, stable permission rejection
after policy change, and server/store restart replay. The executor is not
called by live frame dispatch and the server still does not advertise the
capability.

The complete headless profile passed 227 tests and the full server/TUI profile
passed 349 tests, each with eight explicit long-running, hardware,
upstream-regression, or interoperability tests ignored. Strict headless Clippy
and formatting passed.

## Authenticated durable live-routing gate

Live dispatch now recognizes the exact durable-envelope tag before the legacy
same-Link sequence replay cache. A structurally invalid tagged envelope returns
1012. A valid envelope without an authenticated, identity-matched durable Link
binding returns 1011. Only a matching binding reaches the transactional room
executor; its origin result is sent once per request, while its room event is
broadcast only for the first committed execution. An exact duplicate under a
new sequence number therefore returns the originally retained acknowledgement
and cannot repeat room fan-out.

Focused live tests cover authenticated routing, replay fan-out suppression,
malformed and unnegotiated failures, and continued legacy room messaging. The
production server still emits no durable capability acceptance, so this route
cannot be activated by a current peer. Client advertisement, outbound intent
use, uncertain-mutation retry, and automatic resend remain disabled.

The complete standalone headless profile passed 229 tests and the full
server/TUI profile passed 351 tests, with eight explicit soak, hardware,
upstream-regression, or interoperability tests ignored in each profile. Strict
Clippy passed for both profiles and formatting passed.
