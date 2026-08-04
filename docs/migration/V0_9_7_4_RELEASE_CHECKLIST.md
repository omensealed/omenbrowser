# v0.9.7-4 release qualification checklist

Target: `v0.9.7-4`

Released baseline: `v0.9.7-3` /
`67ab1b910e0d51d14d2f63c3e764e86bbccfe2cc`

## Release scope

- [x] Root and standalone server packages report `0.9.7-4`.
- [x] Both Cargo roots retain one exact official registry 0.9.7 family.
- [x] Exact product-owned Unix directories are created/repaired as `0700`.
- [x] Sensitive product-managed Unix files are created/repaired as `0600`.
- [x] SQLite main/WAL/SHM paths pass under an isolated permissive umask.
- [x] Existing known modes are repaired without content, identity, or schema changes.
- [x] Active/rotated browser and server logs remain bounded and are owner-only.
- [x] The systemd unit contains `UMask=0077` and installer boundaries are tested.
- [x] Custom parents, legacy sources, and user export/import targets are not recursively chmodded.
- [x] OMENchat protocol and persistent compatibility domains remain unchanged.
- [x] Send/request retry, fallback, cancellation, and replay behavior remains unchanged.
- [x] The maximum-UDP Resource failure remains visible and unchanged.

## Qualification

- [x] Unmodified `v0.9.7-3` quick baseline and permissive-umask reproducer.
- [x] Focused path, repair, SQLite sidecar, log rotation, upload, page, and installer tests.
- [x] Pre-bump full root/server tests and strict all-target Clippy.
- [x] Audit, deny, dependency train, architecture/product, and standalone relocation gates.
- [x] Post-bump full/package qualification.
- [x] Isolated two-client OMENchat, reconnect, upload/Resource, and NomadNet lanes.
- [x] Pinned/current Python interoperability lanes.
- [x] Mixed-release compatibility lane.
- [x] Linux ARM64 Cross/QEMU gate.

## External boundaries

- [x] Hosted Linux/native Windows/Intel macOS/Apple Silicon macOS CI.
- [ ] Native installer/DMG lifecycle checks.
- [ ] Physical interface/radio testing.
- [ ] Reviewed candidate merged to `main`.
- [ ] Annotated tag or published release.

No push, tag, release, or publication is authorized by this checklist.
