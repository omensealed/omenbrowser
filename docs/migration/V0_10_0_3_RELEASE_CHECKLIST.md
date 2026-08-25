# v0.10.0-3 release qualification checklist

Target: `v0.10.0-3`

- [x] Exact official registry Reticulum/LXMF 0.10.0 train retained.
- [x] Root/server package revisions and independent lockfiles agree.
- [x] macOS v0.10 bundle mapping remains monotonic.
- [x] macOS packaging path contains no Bash 4-only `mapfile` use.
- [x] Host-independent mapping regression expects `0.10.0` / `1000.0.3`.
- [x] Protocol 1, protocol crate 0.2.0, schema 14, IFAC crate 0.9.5-1,
  identities, storage, and bounds remain unchanged.
- [ ] Native Intel macOS package and lifecycle job passes.
- [ ] Native Apple-Silicon macOS package and lifecycle job passes.
- [ ] Full hosted package workflow passes.
- [ ] GitHub release assets are published.
