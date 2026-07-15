#!/usr/bin/env bash
set -euo pipefail

source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/_common.sh"
smoke_init "03_omenchat_two_client.sh"
cd "$REPO_ROOT"

if [[ ! -x scripts/release-omenchat-smoke.sh ]]; then
  smoke_skip "scripts/release-omenchat-smoke.sh is missing or not executable"
fi

smoke_run "build browser release" cargo build --release --locked --no-default-features --features desktop-product --bin omenbrowser_rs
smoke_run "build server release" cargo build --release --locked --manifest-path src/server/Cargo.toml --no-default-features --features server-headless

smoke_run "omenchat two client smoke" \
  bash scripts/release-omenchat-smoke.sh \
    --out "$SMOKE_RUN_ROOT/omenchat-two-client" \
    --tcp 127.0.0.1:42421 \
    --multi-client \
    --keep-roots

smoke_pass
