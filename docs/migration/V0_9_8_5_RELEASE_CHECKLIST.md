# v0.9.8-5 release qualification checklist

Target: `v0.9.8-5`

Released baseline: `v0.9.8-4` / `8db8ec5a6e3c9b287fa7fdf7aa1919b52202b49b`

## Release scope

- [x] OMENchat live actions, nickname-colour editing, titles, scrolling, and
      identity-scoped workspace restoration are covered by focused tests.
- [x] Local-history search is compact, closable, and visibility-aware.
- [x] LXMF receipt-window expiry is presented as uncertain rather than an
      authoritative delivery failure.
- [x] OMENchat wire protocol 1, protocol crate 0.2.0, and schema 14 are
      unchanged.
- [x] Exact registry Reticulum/LXMF 0.9.8 remains unchanged.
- [x] No persistent-state migration or automatic retry/replay was introduced.

## Qualification

- [x] Focused regression tests, formatting, strict desktop Clippy, and local
      release-profile build.
- [x] Consolidated local quick release check, including standalone relocation,
      native CLI identities, TUI lifecycle/real-PTY, security, feature, and
      focused root/server gates.
- [ ] Full local release check and Linux package candidate/package smoke.
- [ ] Hosted CI and Python interoperability.
- [ ] Native Windows MSVC, Intel macOS, Apple Silicon, and Linux ARM64 lanes.
- [ ] Reviewed candidate merged, tagged, packaged, and published.

## Rollback

Install the v0.9.8-4 binaries and restart the application/server. No database,
configuration, cache, identity, destination, ticket, upload-content, message,
or Reticulum-storage rollback is required.
