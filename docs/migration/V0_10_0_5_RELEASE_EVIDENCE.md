# v0.10.0-5 release evidence

Status: local and hosted release qualification complete; publication pending

Baseline: `v0.10.0-4`, commit
`33971dbdf20c9d962d4a03fe7d7547e092326d75`, clean `main` checkout.
Channel implementation commit: `8eb1153aeb45096ba54219a538fbacbb5d3af1bc`.
Exact hosted qualification candidate:
`8660c6f42fd08206639a929007b692ca61f75afd`. The final evidence-only
descendant is reported in the maintainer handoff because a tracked evidence
file cannot contain its own commit hash.

Upstream review date: 2026-08-27. Latest official LXMF-rs release remains
`v0.10.0`, commit `5436ee715f94f81e18abb0808cfca52fcd7cc9bc`.
Issues #578 and #581 are open. PRs #579, #580, and #582 are open, unmerged, and
not dependencies. No newer official base was adopted.

## Decisions

- Exact crates.io 0.10.0 train retained; no fork, patch, Git source, or vendor copy.
- The managed RNode remains deferred. The negotiated Channel attachment path is
  implemented with protocol crate 0.3.0; wire protocol 1 and schema 14 remain.
- Passive-table automation unavailable because the private upstream table has
  no public accessor; missing traffic/process-only observation is not a pass.
- Existing project-owned bounds and cleanup were retained; no evidence-backed
  memory/task rewrite or automatic replay/restart was introduced.
- Duplicate-Link retirement now preserves identity pending work while a
  replacement Link is active. Channel stages remain exact-Link owned.

## Channel attachment candidate results, 2026-08-27

- Shared frame, negotiation, downgrade, no-fallback, offset/digest rejection,
  exact-Link cleanup, and atomic publication tests: pass.
- `scripts/run-omenchat-current-upload.sh`: pass; negotiated Channel dispatch,
  873-byte durable commit, sender fetch, and isolated second-client fetch.
- `scripts/run-omenchat-current-upload.sh --routed`: pass with separate browser,
  gateway, omenchatd, and second-browser connections; report topology is
  `three-node-routed` and primitive is `channel`.
- Official Reticulum 0.10.0 Channel tests cover out-of-order buffering,
  duplicate/window rejection, retransmission, bounded windows, and retry
  exhaustion. `scripts/run-omenchat-current-upload.sh --impaired` also passed
  through a bounded production-TCP HDLC proxy: three connections, 203 frames,
  two dropped frames, two reordered frames, 128 KiB committed, and both-client
  retrieval. Missing impairment counts are a hard failure, never a pass.
- Adjacent v0.10.0-4 binary upload/download lanes pass in both directions
  against immutable commit `33971dbdf20c9d962d4a03fe7d7547e092326d75`.
  Non-negotiating peers selected the unchanged Resource path; both isolated
  clients fetched the upload and no retry or fallback occurred.

## Local results, 2026-08-27

- `scripts/release-check.sh quick`: pass.
- `scripts/release-check.sh full`: pass; root 1692 passed and server-full 621
  passed/15 ignored, with root/server clippy clean under `-D warnings`.
- Version, exact train, product feature, TUI, private storage, workflow/source,
  documentation, capability, Resource, advisory, audit, and deny gates: pass.
  Installed `cargo-audit` rejects the requested `--locked` option; equivalent
  direct audits of both lockfiles passed with repository-accepted warnings.
- Split-metadata Resource regression: pass. Routed fragment-loss sentinel:
  known-upstream-failure because duplicate Resource data/proof packets are
  suppressed at the forwarding node. Maximum-UDP sentinel:
  known-upstream-failure because 456 bytes cannot hold the 483-byte packet.
- `scripts/smoke/all.sh` application lanes passed with `lxmf-cli` and
  `reticulumd` unavailable because the tools are absent. Its first build-matrix
  run exposed a missing `PathBuf` import in the bare `native-reticulum` profile;
  the import was fixed and the exact `00_build_matrix.sh` rerun passed.
- Current NomadNet page, continuous OMENchat reconnect, and current Channel
  upload in direct and three-node routed topologies: pass. Reconnect observed
  old-Link close, replacement Link, same-session recovery, post-restart echo,
  and reaction/revision/pin recovery.
- Adjacent direct OMENchat: pass in both directions against immutable
  `v0.10.0-4` commit `33971dbdf20c9d962d4a03fe7d7547e092326d75`.
- Adjacent orderly server restart: pass in both directions with stable server
  destination, reused client state root, replacement Link/session, room rejoin,
  and post-restart echo.
- Adjacent history Resource: old client to current server passed. Current client
  to old server initially failed to decode the Resource event, then passed on an
  isolated rerun with exact history content. This timing-sensitive harness
  result was repeated in hosted qualification. Two runs on `0d1f539` exposed
  that asynchronous Resource completion and typed `history_prepended` decoding
  can be drained in adjacent runtime-event envelopes. The assertion was fixed
  to require both Resource receipt and typed history decoding in the same
  isolated report without requiring one callback envelope. The unchanged
  transport lane then passed on `8660c6f`.
- Adjacent SQLite history reopening: pass in both read/write directions with
  room/server metadata, event order, and content preserved.
- Passive announce observation: unavailable. Official 0.10.0 has no public
  accessor for the private table, and no controlled announce-heavy public
  topology was supplied; this is not counted as pass.

## Package

Exact-candidate hosted evidence on 2026-08-28:

- CI run `33138391587`: pass.
- Python interoperability run `33138399631`: pass. Pinned vectors,
  proof/propagation/stamp/recovery, current-product OMENchat/NomadNet, current
  Python drift reporting, maintained `v0.9.9-2`, and adjacent `v0.10.0-4`
  directions completed.
- Package run `33175917126`: pass. Native prerequisites passed on Windows
  x86_64, macOS x86_64, and macOS aarch64. Linux release artifacts, Windows
  portable/unsigned installer artifacts with the `0.10.0-4` MSI upgrade lane,
  and both macOS package jobs passed. Publication was skipped as required.
- Linux ARM64 headless run `33175916892`: pass for standalone omenchatd.

Hosted artifact checksums from package run `33175917126`:

```text
0958a91b30161b02a7727398b503cb7fa210f743c7279050da190030941d03cf  OMENbrowser_rs-latest.tar.gz
1d2111e9a5488d99c17bc89069a0990755edb3bf1b182bb8d1ca656daec455ba  omenbrowser-rs_0.10.0-5_amd64.deb
e4863282b62215eddb0be0629c24630d19fd8d6a47d8bffc2799e0b2ca471455  OMENbrowser_rs-0.10.0-5-x86_64.AppImage
2da7702774280f09f80f1620926bee70ec5113ca3dad2950c3026ea2becbf191  OMENbrowser_rs-0.10.0-5-windows-x86_64-portable.zip
98984fe963ffa56a0298cd79f72e599d203abcb7b3e28d72d3068c4096a79632  OMENbrowser_rs-0.10.0-5-windows-x86_64-setup-unsigned.exe
d4afa92b0d9fef396a075a70a674bf834f1aae7734d7cb8ba11e5cda702f85fb  OMENbrowser_rs-0.10.0-5-windows-x86_64-unsigned.msi
f7afe90e98efa548b1490911e29d7d774651dbffc4288210e6154256810c33c8  omenchatd-0.10.0-5-windows-x86_64.zip
62a05cb02fbbbd8d77bb7c3f220c348a368ec4cde8780b89546d3400b484b1f5  OMENbrowser_rs-0.10.0-5-macos-x86_64-unsigned.dmg
d33330ecd5ab01182c3c81fb2bd7b1ca9cb84a16a7766067b02cf1737d010df4  omenchatd-0.10.0-5-macos-x86_64.tar.gz
a55703f024c04678e69bdd84e558bd78a96950fd9c78fd88b14838fae222e983  OMENbrowser_rs-0.10.0-5-macos-aarch64-unsigned.dmg
235b19cc11bdbd94a3b37f2804de9fbeab006beb43fe232ed5927a92fa7ddb6b  omenchatd-0.10.0-5-macos-aarch64.tar.gz
```

The final archive is generated after this embedded evidence is finalized. Its
new checksum belongs in the external `dist` manifest and maintainer handoff,
avoiding a self-referential checksum inside the archive.

Signing, notarization, a graphical display, physical RNode hardware, and
controlled public-network announce-heavy observation remain unavailable and
are not counted as passes. The local PTY lane passed. No tag, publication, or
GitHub release was performed.
