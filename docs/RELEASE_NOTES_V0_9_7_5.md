# OMENbrowser_rs and omenchatd v0.9.7-5 release notes

Reticulum/LXMF crate train: exact official registry `0.9.7`

Status: final

## Standalone-server private-path containment

- `omenchatd` validates an existing config as a stable bounded regular file
  before parsing path-bearing TOML. Final config symlinks and non-regular
  objects fail before downstream state initialization.
- Configured identity, database, and Reticulum paths reject parent (`..`)
  components. Managed descendants are walked from the canonical selected root
  one component at a time and cannot cross an intermediate symlink.
- Relative configured paths retain their prior current-working-directory
  meaning while receiving deterministic containment classification.
- Clean custom paths remain supported. Existing external ancestors are not
  chmodded or recursively created and must remain operator-controlled.
- Sensitive server reads/appends use stable validated handles where applicable.
  Existing regular-file permissions are repaired through the opened handle.
- Every production file-backed SQLite source open is routed through a central
  wrapper that adds SQLite `NOFOLLOW` while preserving existing open modes,
  WAL/SHM behavior, timeouts, transactions, migrations, backups, and exports.
  On Unix, the already validated parent is resolved to its stable filesystem
  spelling before the final database name is opened. This supports system-owned
  ancestor aliases such as macOS `/var` while leaving the final component
  protected by `NOFOLLOW`.

## Compatibility boundaries

- There is no OMENchat wire, operation, capability, destination-aspect, database
  schema, configuration schema, cache, identity, destination, message, ticket,
  upload-content, or Reticulum-storage migration.
- Send/request retry, replay, fallback, dispatch, cancellation, and Resource
  bounds are unchanged.
- Both Cargo roots retain the exact official registry Reticulum/LXMF `0.9.7`
  train; no Git source, private fork, vendoring, or patch override is used.
- Windows and other non-Unix platforms retain native filesystem semantics;
  this revision adds no POSIX-mode or ACL claim there.
- The known maximum-UDP Resource boundary remains visible and its deliberately
  ignored sentinel is unchanged.

## Rollback

The revision changes path validation and open-time containment, not persistent
content. A clean v0.9.7-4 tree can be reopened after rollback without state
conversion. Configurations containing `..`, symlinked managed descendants, or a
symlinked config file must be corrected to a clean operator-controlled layout
rather than being accepted after rollback.
