#!/usr/bin/env bash
set -euo pipefail

source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/_common.sh"
smoke_init "00_build_matrix.sh"
cd "$REPO_ROOT"

feature_exists() {
  local feature="$1"
  grep -qE "^${feature}[[:space:]]*=" Cargo.toml
}

smoke_run "cargo fmt root" cargo fmt --all -- --check
if [[ -f src/server/Cargo.toml ]]; then
  smoke_run "cargo fmt server" cargo fmt --manifest-path src/server/Cargo.toml -- --check
fi

smoke_run "cargo check all targets" cargo check --all-targets
smoke_run "cargo test all targets" cargo test --all-targets

if feature_exists chat-client-reticulum && feature_exists live-network; then
  smoke_run "cargo build release chat-client-reticulum live-network" \
    cargo build --release --features "chat-client-reticulum,live-network"
else
  echo "SKIP: release chat-client-reticulum/live-network build; feature missing"
fi

if feature_exists native-lxmf-sdk; then
  smoke_run "cargo build release native-lxmf-sdk" \
    cargo build --release --features native-lxmf-sdk
else
  echo "SKIP: native-lxmf-sdk build; feature missing"
fi

if [[ -f src/server/Cargo.toml ]]; then
  smoke_run "cargo build omenchatd live-reticulum" \
    cargo build --manifest-path src/server/Cargo.toml --features live-reticulum
fi

if grep -q 'name = "omen-reticulum-gateway"' Cargo.toml && feature_exists native-reticulum; then
  smoke_run "cargo build omen-reticulum-gateway" \
    cargo build --bin omen-reticulum-gateway --features native-reticulum
else
  echo "SKIP: omen-reticulum-gateway build; binary or native-reticulum feature missing"
fi

smoke_pass
