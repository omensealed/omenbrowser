# v0.9.6-7 release qualification checklist

Target: `v0.9.6-7`

Released baseline: `v0.9.6-6` /
`e04dc8f93e0121774fecc62c8f25f95f0fce6f71`

## Accepted maintenance scope

- [x] Reticulum/LXMF direct dependencies remain exact registry `0.9.6`.
- [x] OMENchat remains protocol version 1 with no mandatory database migration.
- [x] Live interface health uses actual status and worker liveness.
- [x] Ordinary TCP reconnect never creates a competing full runtime.
- [x] Terminal recovery is thresholded, bounded, deduplicated, and cancellable.
- [x] No production TUI recovery path uses `std::thread::sleep`.
- [x] TUI input, Stop, quit, and redraw remain available during recovery backoff.
- [x] Direct upstream stdout damage is repaired by an owned full redraw.
- [x] Headless queue/timer work is event-driven with control priority and bounded
  draining; the 25 ms idle poll is removed.
- [x] Exact Resource hashes cross the internal bridge.
- [x] Outbound Resource IDs have bounded exact hash correlation and cleanup.
- [x] Ambiguous inbound failures remove no upload offer; unique identity/size
  matches remove exactly one.
- [x] Headless and TUI Tokio worker/blocking policies are explicit and bounded.
- [x] Readiness distinguishes configured/progress/operational/degraded/terminal
  states without removing the existing parser-compatible ready line.
- [x] Public package qualification rejects draft current release notes.
- [x] Root and standalone package versions advanced together only after the
  implementation/full local gates passed.

## Local deterministic and live gates

- [x] Root/server formatting and `git diff --check`.
- [x] Canonical desktop tests and all-target strict Clippy.
- [x] Static-media desktop tests.
- [x] Root TUI tests and all-target strict Clippy.
- [x] Standalone `server-full` tests and all-target strict Clippy.
- [x] `bash scripts/release-check.sh full`.
- [x] Isolated root sanity.
- [x] Real PTY TUI lifecycle and terminal restoration.
- [x] Multi-client OMENchat plus server restart smoke.
- [x] Continuous reconnect smoke.
- [x] Current upload/Resource smoke with a second client.
- [x] Current NomadNet direct page request smoke.
- [x] Linux ARM64 headless Podman/Cross/QEMU test and package lifecycle.
- [x] Exact locked-0.9.6 UDP maximum-Resource gate rerun and truthful failure
  retained as an upstream limitation.
- [x] Version, dependency-train, product-feature, workflow-security, and release
  finalization verification after version advancement.
- [x] Release package and package smoke against the exact final working tree.

## External/publication gates not claimed locally

- [ ] Hosted pull-request CI.
- [ ] Pinned Python interoperability workflow.
- [ ] Current-Python drift workflow/report.
- [ ] Native Windows packaging and smoke.
- [ ] Native Intel and Apple Silicon macOS packaging/DMGs and smoke.
- [ ] Physical interface/radio testing.
- [ ] Physical Raspberry Pi testing (not required for the maintained ARM64
  headless gate).
- [ ] Reviewed candidate commit merged to `main`.
- [ ] Annotated `v0.9.6-7` tag and published release.

## Release blockers

Any deterministic gate failure, root/server version mismatch, draft active
release notes at package publication, automatic uncertain replay, unbounded
work, storage-root crossing, dependency-train drift, or feature-profile leakage
blocks publication. The documented pinned-0.9.6 maximum UDP Resource limitation
does not authorize a local fork or incompatible workaround and remains a
truthful upstream boundary.
