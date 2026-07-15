#!/usr/bin/env bash
set -euo pipefail

source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/_common.sh"
smoke_init "01_feature_inventory.sh"
cd "$REPO_ROOT"

smoke_run "cargo metadata" cargo metadata --format-version 1
smoke_run "cargo tree features default" cargo tree -e features
smoke_run "verify product features" bash scripts/verify-product-features.sh

if grep -qE '^native-network[[:space:]]*=' Cargo.toml; then
  smoke_run "cargo tree features native-network" cargo tree -e features --features native-network
  if cargo tree --features native-network 2>/dev/null | grep -qE '(^|[[:space:]])rns-net v'; then
    echo "RESULT: FAIL"
    echo "reason: rns-net appears in native-network dependency graph"
    exit 1
  fi
fi

if [[ -f src/server/Cargo.toml ]]; then
  smoke_run "server cargo tree features live-reticulum" \
    cargo tree --manifest-path src/server/Cargo.toml -e features --no-default-features --features server-headless
  if cargo tree --manifest-path src/server/Cargo.toml --no-default-features --features server-headless 2>/dev/null | grep -qE '(^|[[:space:]])rns-net v'; then
    echo "RESULT: FAIL"
    echo "reason: rns-net appears in server live-reticulum dependency graph"
    exit 1
  fi
fi

smoke_pass
