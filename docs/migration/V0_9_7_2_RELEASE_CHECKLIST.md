# v0.9.7-2 release qualification checklist

Target: `v0.9.7-2`

Released baseline: `v0.9.7-1` /
`2bf21d4cc7abfed7afda5424a76ad2e7135b71e9`

## Release scope

- [x] Root and standalone server packages report `0.9.7-2`.
- [x] Both roots retain one exact official registry 0.9.7 family.
- [x] External RPC lossy guarantees fail before connection or dispatch.
- [x] Managed/integrated sending and no-automatic-replay behavior remain.
- [x] IFAC constant-time comparison, poison handling, and buffer bounds pass.
- [x] Pinned-Python IFAC/Reticulum/LXMF interoperability passes after changes.
- [x] The two `quick-xml` advisories are resolved by the precise fixed registry
  scanner path; no audit exception remains.
- [x] Node-24 action releases are full-SHA pinned without permission changes.
- [x] OMENchat protocol and persistent compatibility domains remain unchanged.
- [x] The maximum-UDP Resource failure is rerun and remains visible.

## Local gates

- [x] Pre-bump full release check, complete tests, and strict Clippy.
- [x] Focused external RPC and upstream capture tests.
- [x] IFAC vectors, tamper/truncation, framing, bounds, and terminal-state tests.
- [x] Raw root/server audit and dependency/workflow verification.
- [x] Post-change pinned-Python interoperability.
- [x] Post-bump full release check and package candidate smoke.
- [x] Current-Python informational lane.
- [x] Current OMENchat upload, continuous reconnect, and NomadNet page lanes.
- [x] Mixed 0.6.0-1/0.9.7-2 direct LXMF and bidirectional OMENchat lanes.
- [x] Linux ARM64 headless Cross/Podman/QEMU test and package lifecycle gate.

## External gates not claimed locally

- [ ] Hosted Linux/native Windows/Intel macOS/Apple Silicon macOS CI.
- [ ] Native Windows installers and macOS DMG lifecycle smoke.
- [ ] Physical interface/radio testing.
- [ ] Reviewed candidate merged to `main`.
- [ ] Annotated tag and published release.

No push, tag, release, or publication is authorized by this checklist.
