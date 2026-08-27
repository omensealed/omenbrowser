# v0.10.0-5 release evidence

Status: local release qualification complete; hosted qualification pending

Baseline: `v0.10.0-4`, commit
`33971dbdf20c9d962d4a03fe7d7547e092326d75`, clean `main` checkout.
Source candidate commit: `daabc5293315d715e00227c3bf7f04e756836425`.

Upstream review date: 2026-08-27. Latest official LXMF-rs release remains
`v0.10.0`, commit `5436ee715f94f81e18abb0808cfca52fcd7cc9bc`.
Issues #578 and #581 are open. PRs #579, #580, and #582 are open, unmerged, and
not dependencies. No newer official base was adopted.

## Decisions

- Exact crates.io 0.10.0 train retained; no fork, patch, Git source, or vendor copy.
- Channel/Buffer attachment and managed RNode deferred under their proof gates.
- Passive-table automation unavailable because the private upstream table has
  no public accessor; missing traffic/process-only observation is not a pass.
- Existing project-owned bounds and cleanup were retained; no evidence-backed
  memory/task rewrite or automatic replay/restart was introduced.

## Local results, 2026-08-27

- `scripts/release-check.sh quick`: pass.
- `scripts/release-check.sh full`: pass; root 1660 passed/31 ignored and server
  617 passed/15 ignored, with root/server clippy clean under `-D warnings`.
- Version, exact train, product feature, TUI, private storage, workflow/source,
  documentation, capability, Resource, advisory, audit, and deny gates: pass.
  Installed `cargo-audit` rejects the requested `--locked` option; equivalent
  direct audits of both lockfiles passed with repository-accepted warnings.
- Split-metadata Resource regression: pass. Routed fragment-loss sentinel:
  known-upstream-failure because duplicate Resource data/proof packets are
  suppressed at the forwarding node. Maximum-UDP sentinel:
  known-upstream-failure because 456 bytes cannot hold the 483-byte packet.
- `scripts/smoke/all.sh`: pass with `lxmf-cli` and `reticulumd` optional lanes
  unavailable because the tools are absent.
- Current NomadNet page, continuous OMENchat reconnect, and current upload:
  pass. Reconnect observed old-Link close, replacement Link, same-session
  recovery, post-restart echo, and reaction/revision/pin recovery.
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
