# Release Versioning

OMENbrowser_rs and the standalone omenchatd use the upstream Reticulum/LXMF
base version followed by a numeric OMEN release revision:

```text
Cargo package version: 0.9.6-4
Git tag when released: v0.9.6-4
```

The leading `v` belongs only in Git tags. Cargo interprets the hyphen suffix as
a SemVer prerelease identifier; numeric identifiers retain the intended
ordering. When the upstream base changes, the OMEN revision resets, for example
from `0.9.5-2` to `0.9.6-1`. The previously planned `0.9.5-3` release was
superseded when the reviewed upstream 0.9.6 train became available; the
0.9.5 improvement commits remain the rollback baseline.

Both application packages normally carry the same release version, but their
Cargo roots and lockfiles remain independent. Packaging scripts derive the
version from the root package manifest. Runtime and diagnostic displays use
`env!("CARGO_PKG_VERSION")` rather than another hard-coded copy.

The private, protocol-neutral `omen-ifac-tcp` support crate has its own package
identity and remains at `0.9.5-1`; its Reticulum transport dependency follows
the coherent 0.9.6 train, while changing the application release revision does
not change the adapter's API or IFAC wire behavior.

Application release versions do not change these independent compatibility
domains:

- the OMENchat frame/wire protocol version;
- SQLite `PRAGMA user_version` and migration numbers;
- persisted configuration schema versions;
- Reticulum destination names and aspects;
- RPC contract versions;
- cache format versions.

Those values change only with their own versioned migration, compatibility
tests, and rollback documentation. `scripts/verify-release-version.sh` checks
the two package manifests and lockfiles without treating protocol or storage
schema numbers as application versions.

No tag is created by migration work itself. A `v0.9.6-4` tag is permitted only
after the dependency/API, native functional, interoperability, security,
performance, documentation, and packaging gates pass and the maintainer
explicitly requests the release operation.
