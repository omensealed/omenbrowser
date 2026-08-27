# v0.10.0-5 release evidence

Status: local release qualification complete; hosted qualification pending

Baseline: `v0.10.0-4`, commit
`33971dbdf20c9d962d4a03fe7d7547e092326d75`, clean `main` checkout.
Channel implementation candidate commit:
`8eb1153aeb45096ba54219a538fbacbb5d3af1bc`. The final evidence-only
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
  result remains a hosted-repeat item, not a hidden pass.
- Adjacent SQLite history reopening: pass in both read/write directions with
  room/server metadata, event order, and content preserved.
- Passive announce observation: unavailable. Official 0.10.0 has no public
  accessor for the private table, and no controlled announce-heavy public
  topology was supplied; this is not counted as pass.

## Package

The final archive is generated only after this embedded evidence is finalized.
Its timestamped filename and SHA-256 are recorded in the external `dist`
checksum/manifest files and the maintainer handoff report, avoiding a
self-referential checksum inside the archive itself.

Hosted native Windows/macOS/ARM64, signing, notarization, physical RNode,
public-network announce traffic, and adjacent upload/download/mutation and
installer-upgrade breadth remain `unavailable` until an exact-candidate hosted
run is linked. The local PTY
lane passed; a graphical display lane was not claimed. No tag, push,
publication, or GitHub release was performed.
