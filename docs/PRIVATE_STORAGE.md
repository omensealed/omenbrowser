# Private managed storage

OMENbrowser and the independently packaged `omenchatd` treat their own managed
state as private local data. On Unix-like systems, exact product-owned
directories are created and repaired as `0700`, and sensitive product-managed
regular files are created and repaired as `0600`. Creation does not depend on
the caller's umask.

The policy covers the browser managed root and its known identity, Reticulum,
message, attachment, cache, download, plugin, log, and diagnostic directories,
including identity-scoped roots. It also covers the selected `omenchatd` home,
its exact Reticulum storage, generated NomadNet pages, uploads, config and
backup, identity and backup, SQLite database and live WAL/SHM sidecars,
migration backups, and active/rotated logs.

Existing known managed paths receive a metadata-only repair. Repair does not
rewrite file contents, regenerate identities, migrate schemas, or change
destination hashes. Directory and rotated-log inspection uses exact path lists
or existing bounded scans; there is no unbounded startup tree walk.

Custom server identity, database, and Reticulum locations are boundaries rather
than ownership of their entire ancestor tree. OMEN protects the sensitive file
and SQLite sidecars, and privately creates a missing dedicated final directory,
but does not chmod an existing `$HOME`, mount root, shared parent, import tree,
export directory, or attachment source. Unsafe symlink or unexpected
non-regular path types fail closed. Legacy adoption leaves source permissions
unchanged and protects only new managed destinations and the exact marker.

SQLite journal mode, transactions, schema, migration ordering, and backup
behavior are unchanged. The main database is protected before a writable open;
live `-wal` and `-shm` sidecars are checked and repaired when present. Active
and retained rotated logs are owner-only without changing their queue, byte,
rotation, retention, flush, redaction, or shutdown bounds.

The systemd user unit adds `UMask=0077` as defense in depth. The binary still
enforces its policy when run directly, and the installer creates or repairs
only the selected `OMENCHATD_HOME` as `0700` without changing its parent.

Windows and other non-Unix platforms continue using their native filesystem
semantics. This release does not claim POSIX mode enforcement or add a new ACL
framework on those platforms.
