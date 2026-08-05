#!/usr/bin/env bash
set -euo pipefail

grep -q '^UMask=0077$' packaging/systemd/omenchatd.service.in
grep -q 'chmod 0700 "$omenchatd_home"' scripts/install-omenchatd-user-service.sh
grep -q 'ensure_private_dir' src/config.rs
grep -q 'create_private_new' src/server/src/config.rs
grep -q 'repair_database_sidecars' src/server/src/store.rs
grep -q 'open_private_append' src/server/src/server_log.rs
grep -q 'open_private_append' src/structured_log_writer.rs
grep -q 'PRIVATE_DIRECTORY_MODE: u32 = 0o700' src/private_fs.rs
grep -q 'PRIVATE_FILE_MODE: u32 = 0o600' src/private_fs.rs
grep -q 'PRIVATE_DIRECTORY_MODE: u32 = 0o700' src/server/src/private_fs.rs
grep -q 'PRIVATE_FILE_MODE: u32 = 0o600' src/server/src/private_fs.rs
grep -q 'SQLITE_OPEN_NOFOLLOW' src/server/src/sqlite.rs
grep -q 'read_private_bounded' src/server/src/config.rs

if grep -q 'parent\.starts_with(managed_root)' src/server/src/config.rs; then
  echo 'unsafe lexical managed-path authorization returned' >&2
  exit 1
fi

for source in src/server/src/config.rs src/server/src/store.rs src/server/src/database_recovery.rs; do
  if sed '/^#\[cfg(test)\]/,$d' "$source" | grep -Eq 'rusqlite::Connection::open(_with_flags)?\('; then
    echo "unreviewed direct file-backed SQLite open in $source" >&2
    exit 1
  fi
done

echo "private storage policy verification: pass"
