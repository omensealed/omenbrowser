# v0.9.7-5 release qualification checklist

Target: `v0.9.7-5`

Released baseline: `v0.9.7-4` /
`839d746242c1264e2e2f729a4d36d19d64411ad9`

## Release scope

- [x] Root and standalone server packages report `0.9.7-5`.
- [x] Both Cargo roots retain one exact official registry 0.9.7 family.
- [x] Config symlinks/non-regular objects fail before path-bearing TOML parse.
- [x] Configured identity/database/Reticulum paths reject parent traversal.
- [x] Managed descendant creation rejects intermediate symlinks.
- [x] Clean managed and operator-controlled custom paths remain supported.
- [x] Sensitive reads/appends use validated handles where required.
- [x] Production file-backed SQLite source opens add `NOFOLLOW` centrally.
- [x] External ancestor modes/content remain outside product ownership.
- [x] OMENchat protocol and persistent compatibility domains remain unchanged.
- [x] Send/request retry, fallback, cancellation, and replay behavior remains unchanged.
- [x] The maximum-UDP Resource failure remains visible and unchanged.

## Qualification

- [x] Untouched v0.9.7-4 quick baseline and focused failing reproducers.
- [x] Focused traversal, config-symlink, managed-symlink, stable-file, and SQLite tests.
- [x] Pre-bump full root/server tests and strict all-target Clippy.
- [x] Post-bump full and package qualification.
- [x] Isolated two-client OMENchat, reconnect, upload/Resource, and NomadNet lanes.
- [x] Pinned/current Python and mixed-release interoperability lanes.
- [x] Linux ARM64 Cross/QEMU test, package, and lifecycle gate.
- [ ] Hosted native Windows/macOS gates.

## External boundaries

- [ ] Physical interface/radio testing.
- [ ] Reviewed candidate merged to `main`.
- [ ] Annotated tag or published release.

No push, tag, release, or publication is authorized by this checklist.
