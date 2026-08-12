# v0.9.9-1 release qualification checklist

Target: `v0.9.9-1`

Released baseline: `v0.9.8-5` / `5de9b897f79fbf2309549c680e05946da0fc9f6c`

## Release scope

- [x] Root and standalone server resolve one exact official crates.io
      Reticulum/LXMF 0.9.9 train.
- [x] No Git source, fork, vendoring, private registry, or patch override.
- [x] Public Request/Response, Link, Resource, cancellation, and exactly-one-
      dispatch behavior remains unchanged.
- [x] External SDK/RPC operations requiring dropped guarantees remain rejected
      before dispatch.
- [x] OMENchat wire protocol 1, protocol crate 0.2.0, schema 14, product bounds,
      and persistent formats remain unchanged.
- [x] Split-metadata exact-byte coverage passes.
- [x] Routed retransmission and maximum-UDP limitations remain separately
      visible with 0.9.9 upstream evidence.

## Qualification

- [x] Root desktop-product and standalone server-full tests.
- [x] Root/server formatting and strict all-target Clippy.
- [x] Version, dependency-train, source, feature, Resource, documentation, and
      zero-accepted-advisory policy checks.
- [x] Current Python drift interoperability (RNS 1.4.2, LXMF 1.1.1,
      NomadNet 1.2.8); pinned reference lane remains pending below.
- [x] Pinned Python interoperability at immutable RNS/LXMF source revisions.
- [x] Consolidated quick release check, including isolated standalone
      relocation, TUI lifecycle/real PTY, native CLI identities, security, and
      focused root/server gates.
- [x] Local smoke matrix, including two-client OMENchat, Resource transfer,
      integrated LXMF loopback, NomadNet fetch, diagnostics, and scroll.
- [x] Consolidated full release check.
- [ ] Linux package candidate and isolated package smoke.
- [x] Linux ARM64 Cross/QEMU headless tests and package lifecycle through
      Podman.
- [ ] Native Windows MSVC, Intel macOS, and Apple Silicon hosted lanes.
- [ ] Reviewed candidate merged, tagged, packaged, and published.

Unchecked hosted/publication items are not local passing claims.

## Rollback

After orderly shutdown, preserve both application/server roots and install the
v0.9.8-5 binaries. No database, configuration, cache, identity, destination,
message, ticket, upload-content, or Reticulum-storage rollback is required.
