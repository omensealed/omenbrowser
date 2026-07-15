#!/usr/bin/env bash
set -euo pipefail

source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/_common.sh"
smoke_init "04_omenchat_resource_transfer.sh"
cd "$REPO_ROOT"

smoke_run "omenchat resource metadata tests" \
  cargo test --locked --no-default-features --features desktop-product test_omenchat_resource_metadata_roundtrip

smoke_run "build OMENbrowser_rs clean OMENchat smoke binary" \
  cargo build --release --locked --no-default-features --features desktop-product

smoke_run "build omenchatd clean Reticulum server" \
  cargo build --locked --manifest-path src/server/Cargo.toml --release --no-default-features --features server-headless

payload="$SMOKE_RUN_ROOT/omenchat-resource-payload.bin"
dd if=/dev/zero of="$payload" bs=1024 count=640 status=none

smoke_run "omenchat resource upload/fetch smoke" \
  bash scripts/release-omenchat-smoke.sh \
    --browser-bin "$REPO_ROOT/target/release/omenbrowser_rs" \
    --server-bin "$REPO_ROOT/src/server/target/release/omenchatd" \
    --tcp "127.0.0.1:42424" \
    --path-wait 75 \
    --out "$SMOKE_RUN_ROOT" \
    --message "OMENchat resource transfer smoke" \
    --upload-file "$payload" \
    --server-upload-max-file-bytes 1048576 \
    --multi-client \
    --keep-roots

latest_run="$(find "$SMOKE_RUN_ROOT" -maxdepth 1 -type d -name 'omenchat-smoke-*' | sort | tail -n 1)"
if [[ -z "$latest_run" || ! -f "$latest_run/omenchat-smoke.json" ]]; then
  echo "RESULT: FAIL"
  echo "reason: OMENchat resource smoke did not produce an omenchat-smoke.json report"
  exit 1
fi

if ! grep -q '"stage": "upload_fetch_wait"' "$latest_run/omenchat-smoke.json"; then
  echo "RESULT: FAIL"
  echo "reason: OMENchat resource smoke report did not include upload_fetch_wait"
  exit 1
fi

if ! grep -q '"event": "upload_resource_available"' "$latest_run/omenchat-smoke.json"; then
  echo "RESULT: FAIL"
  echo "reason: OMENchat resource smoke did not observe upload_resource_available"
  exit 1
fi

if [[ ! -f "$latest_run/omenchat-smoke-2.json" ]]; then
  echo "RESULT: FAIL"
  echo "reason: OMENchat resource smoke did not produce a second-client report"
  exit 1
fi

if ! grep -q '"stage": "existing_upload_lookup"' "$latest_run/omenchat-smoke-2.json"; then
  echo "RESULT: FAIL"
  echo "reason: second OMENchat resource smoke did not look up the first client's upload"
  exit 1
fi

if ! grep -q '"event": "upload_resource_available"' "$latest_run/omenchat-smoke-2.json"; then
  echo "RESULT: FAIL"
  echo "reason: second OMENchat resource smoke did not fetch the first client's upload"
  exit 1
fi

smoke_pass
