# Dependency Security Status

Reviewed on 2026-07-14 with the refreshed RustSec database and both independent
Cargo lockfiles. A nonzero audit is never reported as a pass.

## Inbound LXMF signature admission

Direct LXMF received by the production clean Reticulum bridge now requires an
exact source identity from the bounded authenticated-announce cache. The bridge
derives that identity's `lxmf.delivery` destination and verifies the 0.9.5 wire
signature before creating application state or storing attachment bytes.
Unknown, mismatched, and forged sources are rejected with key-free diagnostics;
verified duplicates remain subject to the bounded replay window. This changes
no dependency, key format, protocol byte, state schema, or configuration.

The deterministic suite covers valid admission, unknown identity, destination
mismatch, forged signature, replay suppression, and pre-write rejection of a
forged attachment. The same policy now applies after recipient-side propagation
envelope decryption; rejected local payloads remain unacknowledged rather than
entering the delivered-transient store. The current-Python informational lane
verifies an LXMF 1.0.1 direct message through the production verifier and one
isolated propagation-node enqueue/sync/ack through the same authenticated
admission boundary. The release-blocking pinned Reticulum 1.2.2/LXMF 0.9.6
source lane now runs that propagation topology and network-facing propagation
stamp acceptance/rejection as well. Both Python lanes also validate Rust ticket
stamp bytes plus issue/use/expiry/reuse lifecycle boundaries without logging
reusable ticket material. The same lanes now complete a live Rust-issued,
Python-ticket-stamped direct reply and verify its signature and stamp before
Rust decoding. The integrated sender now keeps a bounded, atomic, private
issuer cache and implements restart-stable reuse, renewal, and one-day
attempted-inclusion throttling without exposing ticket bytes. The integrated
sender now admits authenticated required direct costs through 8 into a
65,536-attempt, two-job blocking proof boundary with cooperative shutdown
cancellation and ticket precedence. Pinned LXMF 0.9.6 and current LXMF 1.0.1
both accept the stamped cost-1 message and reject an unstamped control. Costs
above 8 and malformed policies fail locally; missing legacy policy does not
start proof work. A missing first-send policy now uses one event-driven,
shutdown-owned announce/path refresh capped at five seconds before encoding;
authenticated empty policy is cached explicitly and over-limit matching policy
fails closed. Both Python lanes accept the resulting integrated stamped send
and reject its unstamped control. There is still no automatic resend after
silence because a transport proof cannot distinguish peer acceptance from an
`LXMRouter` stamp rejection. Propagation-ticket behavior, Resources, live
restart, and mixed-version behavior remain separate evidence.

Deferred unknown senders now trigger structured, exact-destination path
recovery: requests are unique per source and capped at 32 per sync. A malicious
propagation response cannot amplify identical transient payloads into repeated
decrypt, attachment, history, or event work; duplicate candidates and durable
already-delivered IDs are suppressed before publication. No error-string
parsing, unbounded retry set, or acknowledgement of unauthenticated data is used.

## Resolved in this unit

Five compatible patch releases were selected precisely; no other package was
updated:

| Package | Before | After | Product reachability | Reason |
|---|---:|---:|---|---|
| `anyhow` | 1.0.102 | 1.0.103 | Direct root dependency and Iced/image paths in animated and static products | Fixes RUSTSEC-2026-0190 unsound mutable downcast behavior. |
| `crossbeam-epoch` | 0.9.18 | 0.9.20 | Rayon-backed image/EXR paths in animated and static products | Fixes RUSTSEC-2026-0204 invalid pointer dereference. |
| `quinn-proto` | 0.11.14 | 0.11.15 | Lockfile-only optional Reqwest HTTP/3 edge; absent from enabled browser/server graphs | Fixes RUSTSEC-2026-0185 and prevents a future feature from activating the vulnerable lock entry. |
| `num-bigint` | 0.4.7 | 0.4.8 | Iced/image AVIF encoding path in animated and static products; absent from omenchatd | Removes a yanked lock entry while remaining within `num-rational 0.4.2`'s compatible 0.4 requirement. |
| `memmap2` | 0.9.10 | 0.9.11 | Font loading plus Linux window/Wayland paths in animated and static products; absent from TUI-only and omenchatd graphs | Fixes RUSTSEC-2026-0186 range-validation soundness defects while remaining within every parent's 0.9 requirement. |

The root lock still contains 701 packages after the update. No manifest version
range, runtime configuration, feature alias, storage format, or wire format
changed. `src/server/Cargo.lock` is independent and none of these versions is
present in omenchatd's enabled profiles.

## Accepted Wayland build-time audit findings

`cargo audit --no-fetch` now fails only on:

- RUSTSEC-2026-0194: quadratic duplicate-attribute checking in `quick-xml`
  0.39.2;
- RUSTSEC-2026-0195: unbounded namespace declaration allocation in
  `quick-xml` 0.39.2 `NsReader`.

The package is a Linux build-time dependency of `wayland-scanner 0.31.10`
through Iced/Wayland and rfd/ashpd. It is absent from the standalone server and
does not parse browser, Reticulum, LXMF, OMENchat, or user-selected XML at
runtime.

Source inspection of the locked scanner shows it uses `quick_xml::Reader` and
checked `Attributes`. Therefore the duplicate-attribute complexity path is
reachable while compiling trusted Wayland protocol XML, while the
namespace-aware `NsReader` allocation path is not called. This reduces runtime
exposure but does not resolve the advisory. The maintainer accepts the two
findings for v0.9.5-1 because neither path parses application or network input,
the scanner is a compile-time proc macro, and the only reachable affected path
processes fixed dependency-owned Wayland protocol XML.

The required fixed version is `quick-xml >=0.41.0`, while
`wayland-scanner 0.31.10` requires `quick-xml ^0.39`. A precise update was
attempted and Cargo rejected it at dependency resolution without changing the
lockfile. Resolving this requires an upstream Wayland/Iced-compatible release
or an explicitly reviewed patch; this program will not vendor or broadly
upgrade Iced as collateral work.

Upstream Smithay merged its `quick-xml 0.41` scanner update in [pull request
938](https://github.com/Smithay/wayland-rs/pull/938) on 2026-07-08, but crates.io
still reports `wayland-scanner 0.31.10` as the latest release and its published
manifest still requires `quick-xml ^0.39`. The merged source is useful
removal-path evidence, but an unpublished branch is not an approved production
dependency. Re-evaluate the next immutable registry release with a precise
lockfile update, full native product matrix, `cargo audit`, and `cargo deny`;
do not add a Git dependency or advisory exception to bridge the publication
gap.

`scripts/verify-accepted-advisories.sh` is the release boundary. It requires
the raw audit to contain exactly RUSTSEC-2026-0194 and RUSTSEC-2026-0195, maps
both to registry `quick-xml 0.39.2`, requires its only parent to be registry
`wayland-scanner 0.31.10` with a proc-macro target, rejects any repository Rust
import, and requires omenchatd to resolve no `quick-xml`. It then runs the
filtered audit and the unfiltered standalone-server audit. `deny.toml` retains
an empty advisory-ignore list. A new vulnerability, version, parent, runtime
import, or server edge fails closed; a fixed scanner also fails until this
temporary verifier and acceptance are deliberately removed.

The audit also emits five allowed warning categories covering unmaintained and
unsound transitive packages. They remain triage work and must not be
silently converted into an audit pass. The standalone-server audit now reports
no vulnerabilities or warnings.

## `lru` soundness resolution

RUSTSEC-2026-0002 affects `lru` 0.9.0 through 0.16.2. Both independent TUI
graphs previously selected 0.12.5 through Ratatui 0.26.3. They now use Ratatui
0.30.2 with its `lru` 0.18.1 layout cache, above the affected range. Crossterm
is aligned at 0.29.0, and Ratatui defaults are disabled in favor of the explicit
`crossterm_0_29` and `layout-cache` features.

The migration retains the cache because Ratatui documents a performance cost
when layout caching is disabled. Neither application imports `lru` directly.
`scripts/verify-tui-dependencies.sh` requires the exact reviewed Ratatui,
Crossterm, and `lru` versions, rejects `paste`, and rejects unused macro and
calendar features in both root `tui` and omenchatd `server-full`. Headless
profiles remain TUI-free.

The application API migration is limited to `Frame::area()` and converting the
new `Terminal::size()` `Size` result into `Rect`. Strict Clippy and complete
tests pass for both TUI profiles on Linux. Native Windows/macOS compile and
terminal input/restoration smoke remain release gates. Roll back both manifests,
both lockfiles, these API adaptations, the graph gate, and documentation as one
unit.

## `rustls-pemfile` maintenance triage

RUSTSEC-2025-0134 marks `rustls-pemfile` 2.2.0 unmaintained and recommends
using the PEM support in `rustls-pki-types` directly. This is informational,
not a reported memory-safety or input-validation vulnerability, but it is
runtime-reachable in both desktop product profiles:

- `desktop-product` and `desktop-product-static-media` select
  `native-lxmf-sdk` through the clean Reticulum/OMENchat product path;
- OMEN declares `lxmf-sdk 0.9.5` directly with upstream defaults disabled and
  explicitly enables only `std`, `sdk-async`, and `rpc-backend`;
- `rpc-backend` requires `rustls-pemfile` and
  uses it to read configured RPC mTLS certificate chains and private keys;
- root minimal/TUI-only profiles and every standalone omenchatd profile do
  not contain this dependency.

The RPC backend is not dead product surface: OMENbrowser constructs
`lxmf_sdk::RpcBackendClient` for configured endpoints and exposes an explicit
RPC probe. Disabling the backend to make the warning disappear would remove
supported transport behavior. The separate direct declaration is intentional:
it prevents 0.9.5's unrelated default ZeroMQ backend from entering the product
while retaining the reviewed RPC surface.

There is no compatible lockfile update. Version 2.2.0 is the final
`rustls-pemfile` release, and the current `lxmf-sdk` 0.9.5 manifest still
declares the same dependency for `rpc-backend`. Resolving the warning therefore
requires an upstream LXMF SDK migration to `rustls-pki-types::PemObject`, or a
maintainer-approved, wire/API-reviewed ecosystem update or patch. A local fork,
feature removal, and advisory ignore are intentionally not introduced by this
triage unit.

Completion gate: an approved reticulum-rs/LXMF-compatible SDK release removes
`rustls-pemfile` from the canonical product graph, and RPC mTLS certificate,
private-key, invalid-PEM, probe, send, and interoperability tests pass. Roll
back this documentation-only triage by removing this section; runtime and
lockfiles are unchanged.

## Font-stack maintenance triage

RUSTSEC-2026-0206 and RUSTSEC-2026-0192 mark `rustybuzz` 0.20.1 and
`ttf-parser` 0.25.1 unmaintained, with `harfrust` and `skrifa` named as their
respective maintained alternatives. Neither advisory identifies a patched
release.

The current product paths differ:

- `cosmic-text 0.15.0` already uses `harfrust` and `skrifa` for text shaping
  and scaling in both canonical desktop products;
- `rustybuzz` remains in `Cargo.lock` through `usvg 0.45.1` and Iced's optional
  SVG renderer, but neither `desktop-product` nor
  `desktop-product-static-media` enables that graph;
- `ttf-parser` is runtime-reachable in both products through `fontdb 0.23.0`
  and through the Wayland decoration path's `ab_glyph` /
  `owned_ttf_parser`; it parses the bundled viewport font and selected system
  fonts;
- root non-desktop profiles and every standalone omenchatd profile exclude
  both crates.

The graph gate now rejects `rustybuzz` in either canonical product and requires
their maintained `harfrust` and `skrifa` path to remain present. This prevents
a future SVG/renderer feature change from turning the lock-only warning into
release runtime surface without review.

There is no compatible precise update. Current `usvg 0.47.0` still declares
both `rustybuzz 0.20.1` and `ttf-parser 0.25.1`; current `cosmic-text 0.19.0`
still depends on `fontdb 0.23`, which uses `ttf-parser`; and `ab_glyph 0.2.32`
still uses `owned_ttf_parser 0.25.1`. Replacing those internals locally would
fork the Iced font/rendering stack and is not a patch-level dependency update.
No runtime crate, advisory ignore, SVG feature, or broad Iced upgrade is added.

Completion gate: approved upstream Iced renderer, `fontdb`, and `ab_glyph`
releases remove both legacy crates from the lock and enabled native product
graphs. Qualification must cover malformed/truncated fonts without panic,
bundled Micron monospace metrics, system font fallback, emoji and Nerd Font
selection, complex-script shaping, variable fonts, Wayland decorations,
startup/RSS/render latency, and native Windows/macOS/Linux rendering. Roll back
this unit by removing the product-graph assertions and this documentation
together; runtime, wire, configuration, and stored data are unchanged.

## `bincode` maintenance triage

RUSTSEC-2025-0141 marks `bincode` 1.3.3 permanently unmaintained. It names no
patched release; the former maintainers describe 1.3.3 as complete while
suggesting maintained formats for new designs.

Reachability is isolated but the upstream consumer deserves explicit review:

- both canonical desktop products, normal `desktop-dev`, root TUI, and every
  standalone omenchatd profile exclude `bincode`;
- `desktop-ui-test` / `desktop-test` activate it only through Iced 0.14's
  debug/time-travel chain: `iced_debug -> iced_beacon -> bincode`;
- OMENbrowser has no direct `bincode` call and no persisted or network format
  depends on it;
- `iced_beacon` uses the encoding for its local debug TCP protocol, whose
  address can be overridden by `ICED_BEACON_SERVER_ADDRESS`;
- that dependency reads a peer-supplied `u64` frame length, resizes a reusable
  buffer to that value, and only then reads/deserializes the frame. It has no
  item or byte ceiling, so it is unsuitable for release activation regardless
  of the informational advisory classification.

The product graph gate now rejects `bincode` in both canonical profiles. An
intentional `desktop-product,iced/debug` run proves the assertion fails before
such a build can be packaged. This does not claim the optional debug beacon is
hardened, and the existing tester profile's unrelated ashpd async-runtime
conflict is not changed in this unit.

There is no compatible lockfile update because 1.3.3 is the final release and
Iced 0.14 directly selects it. Replacing the private protocol requires an Iced
upstream change or a separately reviewed Iced upgrade; substituting a codec at
the application level would create an unmaintained fork. No runtime crate,
advisory ignore, serialization change, or product feature is added.

Completion gate: an approved Iced release removes `bincode` from the lockfile
and debug graph, bounds inbound frame bytes before allocation, binds locally by
default, rejects malformed/truncated/trailing data, and passes debug reconnect,
slow-peer, oversized-frame, cancellation, and native-platform tests. Roll back
this unit by removing the two graph assertions and this documentation; release
runtime, wire compatibility, configuration, storage, and metrics are unchanged.

## `paste` maintenance triage

RUSTSEC-2024-0436 marks the `paste` 1.0.15 proc macro unmaintained and names
`pastey` as a maintained drop-in successor. There is no patched `paste`
release. OMENbrowser and omenchatd do not invoke the macro directly.

The active target-specific parent boundaries are:

- both canonical desktop products use `paste` through
  `image 0.25.10 -> ravif 0.13.0 -> rav1e 0.8.1`; this is the explicitly
  supported AVIF media path, and `rav1e` uses the macro to generate CPU/ASM
  dispatch symbols at compile time;
- Apple desktop targets also use `paste` through `wgpu-hal -> metal 0.32.0`
  for the native graphics backend; Linux and Windows product targets do not
  activate that parent;
- root `tui` and omenchatd `server-full` no longer use it after the Ratatui
  0.30.2 migration; minimal/headless server profiles also exclude it.

The macro executes while compiling trusted dependency source; it does not
parse browser media, Reticulum/LXMF frames, OMENchat payloads, configuration,
or stored user data at runtime. Its parent libraries remain runtime-reachable,
so the warning cannot be described as lock-only.

The product graph gate requires `rav1e` to be the sole direct `paste` parent on
Linux and Windows, and exactly `metal` plus `rav1e` on Apple targets, in both
desktop products. The TUI dependency gate separately proves `paste` is absent
from root `tui` and omenchatd `server-full`. AVIF remains enabled:
OMENbrowser recognizes `.avif` and `image/avif`, and removing `image`'s AVIF
feature would be an observable compatibility regression.

There is no compatible desktop parent update. `rav1e 0.8.1` is current and
still declares `paste = "1.0"`. Patching it locally or substituting a proc macro
through Cargo would create a fork and is not a lockfile-only maintenance update.

Completion gates are independent. The desktop gate requires an approved
`rav1e`/`ravif`/`image` release using a maintained macro, plus AVIF decode,
malformed/oversized input, CPU-feature, native-platform, render-latency, and
RSS tests. The completed Linux TUI gate includes full root/server tests and
proof that both `paste` and affected `lru` are absent. Native terminal
restoration and input/mouse/layout smoke remain pending. Roll back the desktop
parent assertions and AVIF documentation together; runtime behavior and all
formats remain unchanged.

## `fs4` admission for message publication leases

The root application pins `fs4 =1.1.0` with only its synchronous feature. This
is a deliberate runtime dependency: an age or PID heuristic cannot prove that a
message staging file has no live writer, while a nonblocking operating-system
file lock can. Each publisher holds a separate zero-byte lease until its atomic
replacement and directory sync finish. Recovery removes an artifact only after
acquiring that lease; it retains locked, malformed, and legacy unleased stages.

The crate is maintained, supports Unix and Windows through `rustix` and
`windows-sys`, declares Rust 1.75 (below OMENbrowser's Rust 1.85 policy), and is
dual MIT/Apache-2.0 licensed. The product enables no async runtime integration,
and its `rustix` version was already present in the resolved graph. Tests cover
live-lock exclusion, abandoned recovery, process termination, and the bounded
4,096-artifact inventory. The removal path is Rust 1.89's equivalent
`std::fs::File::{lock,try_lock}` API after the project deliberately raises its
MSRV; no stored-data or wire migration is involved.

## Reproduction

```sh
cargo audit --no-fetch
cargo audit --no-fetch --file src/server/Cargo.lock
bash scripts/verify-accepted-advisories.sh --no-fetch

cargo deny --locked --all-features check licenses bans sources
cargo deny --manifest-path src/server/Cargo.toml --locked --all-features \
  check licenses bans sources

cargo tree --locked --no-default-features --features desktop-product \
  -i anyhow@1.0.103
cargo tree --locked --no-default-features --features desktop-product \
  -i crossbeam-epoch@0.9.20
cargo tree --locked --no-default-features --features desktop-product \
  -i quick-xml@0.39.2
cargo tree --locked --no-default-features --features desktop-product \
  -i num-bigint@0.4.8
cargo tree --locked --no-default-features --features desktop-product \
  -i memmap2@0.9.11
cargo tree --locked --no-default-features --features tui -i lru@0.18.1
cargo tree --locked --no-default-features --features desktop-product \
  -e features -i rustls-pemfile@2.2.0
cargo tree --locked --no-default-features --features desktop-product \
  -i ttf-parser@0.25.1
cargo tree --locked --all-features --target all -i rustybuzz@0.20.1
cargo tree --locked --no-default-features --features desktop-test \
  -i bincode@1.3.3
cargo tree --locked --no-default-features --features desktop-product \
  -i paste@1.0.15 --prefix depth
cargo tree --locked --no-default-features --features desktop-product \
  -i fs4@1.1.0
bash scripts/verify-tui-dependencies.sh
cargo tree --locked --manifest-path src/server/Cargo.toml \
  --no-default-features --features server-full -i lru@0.18.1
cargo tree --locked --target all --no-default-features \
  --features desktop-product -i quinn-proto@0.11.15
```

The last command is expected to print no active dependency path. Audit scans
the complete lockfile rather than only enabled target/features.

The repository policy in `deny.toml` has no persistent advisory exceptions. CI
installs deliberate cargo-deny 0.20.2 and cargo-audit 0.22.2 tool versions.
Cargo-deny applies its license, source, and version-requirement gates to both
independent lockfiles. The advisory verifier records the raw failure before
filtering only the exact accepted IDs, so the `quick-xml` findings remain
visible and a broader vulnerability set cannot become green.

The stamped direct-Resource interoperability unit adds no dependency or feature.
It uses the already pinned `reticulum-rs-transport` 0.9.5 Resource selection and
the existing 16 MiB LXMF wire, 8 MiB scalar, and 4,096-correlation limits. The
Python fixture uses temporary roots and reports only size/digest booleans and
public validation state.

The mixed-application harness also changes no manifest or lockfile. Its 0.6
crate train exists only inside a disposable archive/target built from immutable
commit `5ba6683`; production dependency verification therefore continues to
require a single 0.9.5 train. Its gateway installs exact Python RNS 1.3.8 into a
temporary virtual environment, binds ephemeral IPv4 loopback, uses public test
credentials through an owner-only file, and deletes all raw reports and identity
state on exit. The retained summary contains no paths, hashes, payloads, or
credentials.

The optional mixed Resource case copies the current source to a second
disposable root and applies the reviewed one-line fixture patch to both source
copies. It changes only the live-interop diagnostic body length to 65,536 bytes,
which remains below the existing LXMF decode/allocation limits and above both
adapters' 431-byte Link-packet MDU. Neither patched binary nor its source tree is
retained. The report records byte counts and the public threshold only, never
payload contents.

The restart case uses the same temporary roots across two completed application
lifecycles and then deletes them. Destination and message identifiers are used
only for in-process equality checks; the retained summary contains booleans,
counts, timings, and public versions. Each direction sends once with its
receive-only peer already online, preventing a retry from duplicating a message
whose acknowledgement was merely delayed.

The mixed propagation case likewise introduces no production dependency. Its
temporary virtual environment pins Python RNS 1.3.8 and LXMF 1.0.1; its TCP
transport/propagation node binds only ephemeral loopback and uses public fixture
IFAC credentials. Separate old/current identities and application roots are
deleted on exit. The node must observe exactly one queued transient and zero
after the recipient's authenticated sync acknowledgement. In the reverse case,
the current recipient must prove one initial unauthenticated deferral and one
bounded sender-path request before authenticated recovery; the node must retain
the transient throughout. The retained reports contain only versions, counts,
and validation booleans.

The orderly node-restart mode uses the same isolated Python storage and a
deterministic fixture identity across two processes, but changes to a fresh
ephemeral loopback port. The first process exits only after observing one
queued transient. The second must report that exact entry count and stable
public propagation destination before serving the recipient. All storage and
raw identifiers are deleted afterward; no crash- or power-loss claim is made.

The abrupt-process mode first requires a changed, stable, nonempty snapshot of
the isolated LXMF storage, then sends `SIGKILL` only to the recorded Python
fixture PID. Recovery must retain the same public node identity and exactly one
transient. This does not execute against a host service or maintainer root and
does not claim filesystem flush, power-loss, or storage-device durability.

The stamp/ticket mode enables Python propagation-stamp enforcement at the
fixture's effective positive cost. It admits only a matching stamped transient
and requires the old recipient to recover a correctly shaped embedded ticket.
The report retains booleans and public version/cost evidence only; the ticket,
stamp, identities, message IDs, payload, and temporary storage are deleted.

The mixed OMENchat history probe adds no dependency and opens no network
listener. Both application versions use their public SQLite store API against
one explicit temporary root. The database is deleted on every exit path, and
the retained report contains no database path, event body, destination, user
identity, or SQLite bytes.

The live mixed OMENchat harness likewise adds no dependency. It archives the
immutable old source and builds the selected old/current desktop-client and
standalone-server pair for each direction. Their Reticulum/application state
uses separate explicit temporary roots, and communication is limited to an
ephemeral loopback TCP port. The roots and raw smoke report are deleted on
every exit path; retained evidence contains only public versions and
stage-validation booleans, never identities, destinations, payloads,
credentials, ports, or private paths.

Restart mode reuses only those temporary roots. It requires the public server
destination to remain stable and retains that fact as a boolean; the actual
destination is discarded with the raw reports. The old server's expected
bounded SIGTERM status is classified without retaining PIDs, ports, paths, or
process logs. The reciprocal case requires current omenchatd to report an
orderly stop; a signal-exit classification fails that direction.

History-Resource mode changes only the selected isolated server configuration by
setting its large-batch threshold to one byte. It uses a normal small message
and a second, independently rooted client to force and consume the production
Resource history path without weakening a production limit or default. The raw
Resource, messages, identities, destination, paths, logs, and server state are
deleted; the retained report contains only public versions and validation
booleans. Separate current-client/old-server and old-client/current-server
cases cover both directions.

The continuous reconnect harness also adds no dependency. It creates browser
and server state under one temporary root, binds only an ephemeral loopback
port, and uses a marker inside that root to coordinate the orderly server
restart. Raw reports, identities, destinations, messages, ports, paths, logs,
and state are deleted. The retained report contains only public versions and
booleans for process continuity, old-link closure, new-link admission,
same-session reconnect, stable server destination, and the post-restart echo.

The current upload harness reuses the checked-in public 873-byte OMENchat wire
fixture as deterministic payload data. Both clients and omenchatd use separate
temporary roots on ephemeral loopback. The sender and a separately rooted
second client must each fetch the server-held Resource at the exact byte count.
The raw payload, Resource identifiers, identities, destination, port, paths,
logs, server storage, and client reports are deleted; only versions, byte count,
and validation booleans are retained.

The current NomadNet page harness also adds no dependency. It places the
browser identity and standalone portal state in separate temporary roots and
binds only an ephemeral loopback port. It validates the deterministic page
shape before deleting the raw report, destination, URL, identities, paths,
port, logs, and state. Its retained report contains only public versions,
content type, page byte/line counts, the request primitive, and booleans.

The current-Python NomadNet fixture is part of the existing informational drift
lane and adds no runtime dependency. The lane installs exact RNS 1.3.8 and
NomadNet 1.2.7 packages into a disposable virtual environment. Python and Rust
use separate temporary identity/config/storage roots and an ephemeral loopback
IFAC connection with checked-in public fixture credentials. The executable page
accepts only deterministic `field_*`/`var_*` values; its oversized value is
reduced to a byte count rather than echoed. The large response is deterministic
public text and remains within the production four-MiB response cap. Fault
pages execute only a fixed three-second delay and emit fixed public text; they
accept no request-derived command or output. The timeout/cancellation scenario
retains only its public check name and exact request count. The repeated-request
page stores its temporary link identifier only inside the isolated fixture root
and reports solely the request count and same-link boolean; neither identifier
nor hash is printed or retained. The comparative page uses fixed one-byte and
2,048-byte public payload shapes, emits only request number and byte count, and
applies the same temporary-only link tracking. The report retains package
versions and public check names, never identities, destinations, form values,
ports, paths, credentials, logs, or state; all temporary material is deleted.

## Rollback

Each precise lockfile package can be reverted independently, but doing so
reintroduces its named advisory. Do not revert any package below its patched
version without a maintainer-approved security exception. The remaining
`quick-xml` entry must be removed through its parent dependency boundary, not by
weakening audit or parser validation.

The policy/CI unit can be rolled back independently by removing `deny.toml` and
the two cargo-deny CI steps together, then reverting the corresponding testing
and maintenance documentation. That rollback changes no binary or stored data,
but it reopens dependency admission without an automated license/source gate.
