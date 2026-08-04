# OMENbrowser_rs and omenchatd v0.9.7-4 release notes

Reticulum/LXMF crate train: exact official registry `0.9.7`

Status: final

## Unix private-storage hardening

- Exact product-owned private directories are created and repaired as `0700`.
- Sensitive managed files are created and repaired as `0600`, independent of
  the caller's umask.
- Existing known managed paths receive metadata-only repair. File contents,
  identity material, destination hashes, SQLite rows, and schema versions are
  unchanged.
- Server SQLite main, live WAL, and live SHM files are covered and tested while
  a WAL connection remains active under a permissive subprocess umask.
- Browser and server active/rotated logs are owner-only while retaining their
  existing bounded queue, rotation, retention, redaction, and shutdown policy.
- The systemd user unit adds `UMask=0077`; the installer creates or repairs only
  the selected `OMENCHATD_HOME` as `0700` and preserves it on uninstall.

## Boundaries and compatibility

- Custom identity/database/Reticulum files are protected without recursively
  chmodding unrelated ancestors. User-selected imports, exports, attachment
  sources, and legacy source trees are not recursively modified.
- Unsafe symlink and unexpected non-regular private paths continue to fail
  closed. Permission repair uses exact paths and bounded retained-log scans.
- Windows and other non-Unix systems retain their current native filesystem
  behavior; no POSIX-mode or ACL claim is added there.
- There is no OMENchat wire/capability change and no database schema, config
  schema, cache, identity, destination, ticket, upload-content, message, or
  Reticulum-storage migration.
- Send/request retry, replay, fallback, cancellation, and dispatch behavior are
  unchanged.
- The known maximum-UDP Resource boundary remains visible and its sentinel is
  unchanged.

## Rollback

This release changes Unix permission metadata only. Rolling back requires no
state conversion; owner-only modes may remain in place and are compatible with
the prior release.
