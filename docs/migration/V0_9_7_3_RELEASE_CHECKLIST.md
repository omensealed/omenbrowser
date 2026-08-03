# v0.9.7-3 release qualification checklist

Target: `v0.9.7-3`

Released baseline: `v0.9.7-2` /
`7deaafa6a1827588fec3a444b8707ff93fa1c93d`

## Release scope

- [x] Root and standalone server packages report `0.9.7-3`.
- [x] Both roots retain one exact official registry 0.9.7 family.
- [x] Live-server lock poison produces typed redacted errors rather than panics.
- [x] Headless statistics poison enters the fatal/drain path.
- [x] TUI monitoring and moderation do not present stale/false success.
- [x] Poisoned shutdown still cancels and joins owned workers and is idempotent.
- [x] Request lag evidence preserves exactly one dispatch and no replay.
- [x] Active security documentation expects zero accepted vulnerabilities.
- [x] OMENchat protocol and persistent compatibility domains remain unchanged.
- [x] The maximum-UDP Resource failure is rerun and remains visible.

## Qualification

- [x] Unmodified `v0.9.7-2` quick baseline.
- [x] Focused poison, TUI, shutdown, request-lag, and timeout/no-replay tests.
- [x] Pre-bump full root/server tests and strict Clippy.
- [x] Audit, deny, dependency train, and standalone relocation gates.
- [x] Isolated OMENchat, reconnect, upload/Resource, and NomadNet lanes.
- [x] Pinned/current Python lanes.
- [x] Linux ARM64 Cross/QEMU gate.
- [x] Mixed-release SQLite history reopening lane.
- [x] Release package and isolated package-smoke gates.
- [ ] Post-bump full/package qualification.

## External boundaries

- [ ] Hosted Linux/native Windows/Intel macOS/Apple Silicon macOS CI.
- [ ] Native installer/DMG lifecycle checks.
- [ ] Physical interface/radio testing.
- [ ] Reviewed candidate merged to `main`.
- [ ] Annotated tag or published release.

No push, tag, release, or publication is authorized by this checklist.
