# Iced Crate Admission Record

Reviewed on 2026-07-26 against the locked Rust 1.97 build and the Phase 5
criteria in the approved v0.6.0-1 review. This is a release-input inventory,
not permission to activate a dormant feature in a product alias.

## Product graph decisions

| Crate | Locked version | License / declared MSRV | Product status | Decision and removal path |
|---|---:|---|---|---|
| `iced` | 0.14.0 | MIT / 1.88 | Animated and static-media products | Admitted. It is the desktop UI framework and uses explicit features with defaults disabled. Removing it requires replacing the desktop UI and is outside this release. |
| `iced_gif` | 0.14.0 | MIT / not declared | Animated product only | Admitted only behind `chat-client-gif`. OMENbrowser uses bounded in-memory `Frames::from_bytes`; the unused default `async-fs` feature is disabled. The crate requires an async backend to compile its unused path loader, so its `tokio` feature reuses the product's existing Tokio runtime. `desktop-product-static-media` is the tested removal path. |
| `qrcode` (through Iced) | 0.13.0 | MIT OR Apache-2.0 / not declared | Animated and static-media products | Admitted through Iced's `qr_code` feature for one bounded OMENchat invitation QR. Input is the canonical no-secret URI capped at 2 KiB; one ephemeral owner retains one encoded matrix/cache and has explicit close/session/room cleanup. There is no camera, image decoder, permission, network, or native-library surface. Removal is the `desktop-qr` product edges plus the QR owner/view; text copy remains the fallback. |
| `iced_aw` | 0.14.1 | MIT / 1.88 | Not in either product | Hold as a dormant, default-off candidate. There are no application call sites, so enabling `desktop-widgets` in a product needs a concrete workflow and new measurements. Removal is the feature/dependency edge plus its lock subtree. |
| `iced_drop` | 0.2.37 | MIT / not declared | Not in either product | Hold. There is no attachment/import drop workflow or source call site. Defaults are explicitly disabled; `helpers` is isolated behind `desktop-dnd`. |
| `iced_anim` | 0.3.1 | MIT / not declared | Not in either product | Hold. There is no source call site and reduced motion is implemented without it. Defaults are explicitly disabled and it remains behind `desktop-animations`. |
| `iced_table` | 0.14.0 | MIT / not declared | Not in either product | Hold. No measured large-row table workflow exists. Defaults are explicitly disabled and it remains behind `desktop-tables`. |
| `iced_toaster` | absent | not evaluated for this checkout | Absent | Not admitted. Reconsider only with a bounded/coalesced transient-feedback design, measurements, and persistent error fallback. |
| `iced-code-editor` | absent | not evaluated for this checkout | Absent | Not admitted. The approved plan remains conditional on a real editable source/configuration workflow. |

The locked manifests and upstream project pages were checked directly:

- <https://github.com/iced-rs/iced>
- <https://github.com/iced-rs/iced_aw>
- <https://docs.rs/crate/iced_drop/0.2.37>
- <https://github.com/bradysimon/iced_anim>
- <https://github.com/tarkah/iced_table>
- <https://github.com/tarkah/iced_gif>

All six locked crates declare MIT. `iced` and `iced_aw` declare Rust 1.88;
the other four do not publish `rust-version`, so successful Rust 1.97 checks
are compatibility evidence but not an inferred MSRV claim. Native platform
success remains the Windows/macOS CI gate.

## Machine gates and measurements

`scripts/verify-product-features.sh` now fails when:

- an animated/static product resolves an Iced version other than 0.14.0;
- `iced_aw`, `iced_drop`, `iced_anim`, `iced_table`, `iced_toaster`, or
  `iced-code-editor` enters the canonical animated product;
- `iced_gif`, its required Tokio compatibility feature, or its former default
  `async-fs` feature enters an invalid graph;
- either canonical product loses `desktop-qr`, Iced's QR feature, or the locked
  `qrcode 0.13.0` encoder;
- either canonical product activates unmaintained `rustybuzz` or loses its
  maintained `harfrust` and `skrifa` text-shaping/scaling path;
- either product activates the test/dev-only `iced_beacon` serialization edge
  represented by unmaintained `bincode`;
- `paste` gains any direct desktop parent other than the reviewed
  `rav1e` AVIF encoder path;
- any dormant adjunct profile resolves a second Iced version.

With identical locked product features, disabling unused `iced_gif/async-fs`
reduced sorted unique `cargo tree --prefix none` lines from 604 to 591. It
removed the old `async-fs` 1.6 branch and reused the already-present Tokio
filesystem/I/O features; no dependency version was upgraded.
Runtime behavior, encoded/decoded media limits, startup work, storage, and wire
formats are unchanged. The existing F-018 optimized Linux binary measurement
remains the size baseline; a new clean binary/build-time measurement is not
claimed for this small graph-only removal.

## Security and license gate

The follow-up dependency-security units precisely updated `anyhow` 1.0.103,
`crossbeam-epoch` 0.9.20, lock-only `quinn-proto` 0.11.15, and the yanked
`num-bigint` 0.4.7 entry to 0.4.8. The active desktop `memmap2` edge is patched
from 0.9.10 to 0.9.11. Audit now fails only on `quick-xml` 0.39.2 through Linux
build-time Wayland generation; its parent requires `^0.39` while both fixes
require 0.41. Details and reachability evidence are in
`docs/maintenance/DEPENDENCY_SECURITY.md`. The two findings are accepted only
for the exact machine-checked Wayland proc-macro build path; they still block
admission of any new runtime or untrusted-XML edge.

The checked-in `deny.toml` now gates both independent lockfiles across the
supported native target graphs. It explicitly admits the reviewed license set,
denies wildcard requirements and unknown registry/Git sources, and retains
duplicate versions as visible warnings. It contains no advisory suppressions;
the constrained `quick-xml` findings are filtered only after the release
verifier proves their exact accepted build-time path. The six Iced-adjacent
package licenses above also pass the machine-enforced repository policy.

## Rollback

Restore `iced_gif` defaults only together with a demonstrated async filesystem
call site and new graph/measurement evidence. The graph assertions and this
record can otherwise be removed independently; they do not affect persisted
data or protocol compatibility.
