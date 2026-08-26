# v0.10.0-4 release qualification checklist

Target: `v0.10.0-4`

- [x] Exact official registry Reticulum/LXMF 0.10.0 train retained.
- [x] Protocol 1, protocol crate 0.2.0, schema 14, IFAC crate 0.9.5-1,
  identities, storage, and bounds remain unchanged.
- [x] Duplicate same-identity callback preserves negotiated durable authority.
- [x] Changed identity still clears link-scoped capability authority.
- [x] Link close still clears link-scoped capability authority.
- [x] No rejected or uncertain message is automatically replayed.
- [x] Expired durable client instances rotate only after terminal persistence
  and quiescence, then renegotiate on a replacement Link.
- [x] macOS packager remains compatible with system Bash 3.2.
- [x] Focused server regression passes.
- [x] Focused client expired-instance rotation regression passes.
- [x] Persistent-WAL unmanaged-reader regression passes.
- [x] Sender-side live reaction authority regression passes.
- [x] Live two-client message commit and reaction projection pass.
- [x] Quick, full, and package gates pass locally.
- [ ] Hosted native/package workflows pass.
- [ ] GitHub release assets are published.
