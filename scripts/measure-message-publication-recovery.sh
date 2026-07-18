#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

echo "message publication recovery measurements: repeated crashes and exact artifact ceiling"
echo "host=$(uname -srm)"
echo "rustc=$(rustc -V)"

echo "== 16 isolated pre-rename process crashes =="
CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}" \
  cargo test --locked --release --lib --no-default-features \
  --features desktop-product \
  messaging::store::publication_tests::repeated_precommit_crashes_recover_in_one_bounded_pass \
  -- --exact --nocapture --test-threads=1

echo "== exact 4096-artifact ceiling with one live writer =="
CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}" \
  cargo test --locked --release --lib --no-default-features \
  --features desktop-product \
  messaging::store::publication_tests::exact_publication_artifact_ceiling_recovers_abandoned_and_retains_live_writer \
  -- --exact --nocapture --test-threads=1
