# v0.9.7-6 release qualification checklist

Target: `v0.9.7-6`

Released baseline: `v0.9.7-5` /
`b2e8b21a56b03cad0a772ac74c412f7ed89e4cfa`

## Release scope

- [x] Root and standalone server packages report `0.9.7-6`.
- [x] Both Cargo roots retain one exact official registry 0.9.7 family.
- [x] Checked Resource accounting accepts 1,048,575 and rejects 1,048,576 bytes total.
- [x] Unsafe server Resources fail before the original offer frame and dispatch.
- [x] Default 512 KiB upload behavior remains unchanged.
- [x] Configured upload values remain persistent while runtime admission is capped.
- [x] Desktop and server enforce the exact-train upload ceiling.
- [x] Split inbound NomadNet and OMENchat Resources fail closed without replay.
- [x] Bounded rejection markers suppress later corrupted completion.
- [x] No protocol or persistent-state migration exists.
- [x] Maximum-UDP and split-metadata sentinels remain separately visible.

## Qualification

- [x] Untouched v0.9.7-5 quick baseline and failing dangling-offer reproducer.
- [x] Focused boundary, cleanup, upload-admission, and split-rejection tests.
- [x] Pre-bump full root/server tests and strict all-target Clippy.
- [x] Post-bump full and package qualification.
- [x] Two-client OMENchat upload and direct/Resource NomadNet lanes.
- [x] Pinned/current Python and mixed-release interoperability lanes.
- [x] Linux ARM64 Cross/QEMU gate.
- [ ] Hosted native Windows/macOS gates.

## External boundaries

- [ ] Upstream issue #553 fixed in an official registry release and sentinel passes.
- [ ] Upstream maximum-UDP Resource boundary fixed.
- [ ] Physical interface/radio testing.
- [ ] Reviewed candidate merged to `main`.
- [ ] Annotated tag or published release.

No push, tag, release, or publication is authorized by this checklist.
