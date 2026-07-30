# v0.9.6-5 release qualification checklist

Target: `v0.9.6-5`

## Scope and compatibility

- [x] Patch is limited to OMENchat live reaction fan-out, message action
  presentation, moderation-audit visibility, user-list styling, and the
  corresponding smoke evidence.
- [x] OMENchat wire protocol, capabilities, operation numbers, and codecs are
  unchanged.
- [x] Browser and omenchatd database schemas and configuration formats are
  unchanged.
- [x] Reticulum/LXMF direct dependencies remain on the exact `0.9.6` train.

## Local evidence

- [x] Root canonical product compiles.
- [x] Standalone server headless product compiles.
- [x] Root and server Clippy pass with warnings denied.
- [x] Focused desktop hover and moderation-close regressions pass.
- [x] Root reaction tests pass.
- [x] Standalone server reaction tests pass.
- [x] Isolated two-client live reaction and Resource-snapshot smoke passes.
- [x] Full local root/server test and quick-release matrix rerun after the
  version lockfiles are refreshed.

## Hosted and packaging gates

- [ ] Pull-request CI passes once for the complete candidate.
- [ ] Python interoperability passes once for the complete candidate.
- [ ] Native Linux, Windows, Intel macOS, and Apple Silicon macOS packaging
  passes once for the complete candidate.
- [ ] Artifact versions, checksums, and machine-readable manifest report
  `0.9.6-5`.
- [ ] Reviewed candidate commit is merged to `main`.
- [ ] Annotated `v0.9.6-5` tag resolves to the reviewed `main` commit.
- [ ] Published release contains the reviewed native artifacts and release
  notes.
