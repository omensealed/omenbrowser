#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

tests=(
  'store::tests::process_kill_preserves_committed_event_and_rolls_back_in_flight_event'
  'upload::tests::process_kill_upload_recovery_is_conservative_at_every_commit_boundary'
)

if ! command -v cargo >/dev/null 2>&1; then
  echo "missing crash-recovery test tool: cargo" >&2
  exit 2
fi

echo "== omenchatd SQLite process-kill recovery =="
for test_name in "${tests[@]}"; do
  cargo test --locked --manifest-path src/server/Cargo.toml \
    --no-default-features "$test_name" -- --exact --nocapture
done
