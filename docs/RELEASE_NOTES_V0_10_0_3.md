# OMENbrowser_rs and omenchatd v0.10.0-3 release notes

Status: unpublished; superseded by v0.10.0-4 after live duplicate-identification evidence exposed lost link-scoped capability state.

Reticulum/LXMF crate train: exact official crates.io `0.10.0`.

## Packaging correction

This revision replaces Bash 4-only `mapfile` usage in the macOS packager and
its release regression with Bash 3.2-compatible command substitution and
`sed` line extraction. It retains the v0.10 bundle mapping introduced in
v0.10.0-2:

- `CFBundleShortVersionString`: `0.10.0`
- `CFBundleVersion`: `1000.0.3`

No runtime, protocol, schema, storage, identity, dependency, upload, reconnect,
or capability behavior changes from v0.10.0-1.

## Compatibility

OMENchat wire protocol remains 1, `omenchat-protocol` remains 0.2.0, the
omenchatd SQLite schema remains 14, and `omen-ifac-tcp` remains 0.9.5-1.

The routed Resource retransmission and maximum UDP Resource wire-packet
sentinels remain unchanged upstream limitations. Typed transport telemetry
remains bounded diagnostic evidence, not delivery or durable-commit evidence.

See `migration/V0_10_0_3_RELEASE_EVIDENCE.md` for corrective gate results.
