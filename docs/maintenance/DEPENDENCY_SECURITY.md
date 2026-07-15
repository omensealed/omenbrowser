# Dependency Security Status

Reviewed on 2026-07-14 with the refreshed RustSec database and both independent
Cargo lockfiles. A nonzero audit is never reported as a pass.

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

## Remaining audit failure

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
exposure but does not resolve the advisory.

The required fixed version is `quick-xml >=0.41.0`, while
`wayland-scanner 0.31.10` requires `quick-xml ^0.39`. A precise update was
attempted and Cargo rejected it at dependency resolution without changing the
lockfile. Resolving this requires an upstream Wayland/Iced-compatible release
or an explicitly reviewed patch; this program will not vendor or broadly
upgrade Iced as collateral work.

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
- `lxmf 0.6.0`'s `sdk` feature enables `lxmf-sdk 0.6.0` with its default
  features;
- those defaults include `rpc-backend`, which requires `rustls-pemfile` and
  uses it to read configured RPC mTLS certificate chains and private keys;
- root minimal/TUI-only profiles and every standalone omenchatd profile do
  not contain this dependency.

The RPC backend is not dead product surface: OMENbrowser constructs
`lxmf::sdk::RpcBackendClient` for configured endpoints and exposes an explicit
RPC probe. Disabling the backend to make the warning disappear would remove
supported transport behavior. Cargo feature unification also cannot disable a
transitive default selected by `lxmf`'s dependency declaration.

There is no compatible lockfile update. Version 2.2.0 is the final
`rustls-pemfile` release, and the current `lxmf-sdk` 0.9.0 manifest still
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

The two active parent boundaries are:

- both canonical desktop products use `paste` through
  `image 0.25.10 -> ravif 0.13.0 -> rav1e 0.8.1`; this is the explicitly
  supported AVIF media path, and `rav1e` uses the macro to generate CPU/ASM
  dispatch symbols at compile time;
- root `tui` and omenchatd `server-full` no longer use it after the Ratatui
  0.30.2 migration; minimal/headless server profiles also exclude it.

The macro executes while compiling trusted dependency source; it does not
parse browser media, Reticulum/LXMF frames, OMENchat payloads, configuration,
or stored user data at runtime. Its parent libraries remain runtime-reachable,
so the warning cannot be described as lock-only.

The product graph gate requires `rav1e` to be the sole direct `paste` parent in
both desktop products. The TUI dependency gate separately proves `paste` is
absent from root `tui` and omenchatd `server-full`. AVIF remains enabled:
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

## Reproduction

```sh
cargo audit --no-fetch
cargo audit --no-fetch --file src/server/Cargo.lock

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
bash scripts/verify-tui-dependencies.sh
cargo tree --locked --manifest-path src/server/Cargo.toml \
  --no-default-features --features server-full -i lru@0.18.1
cargo tree --locked --target all --no-default-features \
  --features desktop-product -i quinn-proto@0.11.15
```

The last command is expected to print no active dependency path. Audit scans
the complete lockfile rather than only enabled target/features.

The repository policy in `deny.toml` has no advisory exceptions. CI installs
the deliberate cargo-deny 0.20.2 tool version and applies its license, source,
and version-requirement gates to both independent lockfiles. Advisory checks
remain separate so the documented `quick-xml` findings stay visible instead of
being ignored in policy.

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
