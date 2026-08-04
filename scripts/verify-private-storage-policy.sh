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

echo "private storage policy verification: pass"
