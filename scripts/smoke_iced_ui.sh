#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
smoke_root="${OMENBROWSER_ICED_SMOKE_ROOT:-${TMPDIR:-/tmp}/omenbrowser-rs-iced-smoke}"

export OMENBROWSER_CONFIG_DIR="$smoke_root/config"
export OMENBROWSER_DATA_DIR="$smoke_root/data"
export OMENBROWSER_CACHE_DIR="$smoke_root/cache"

case "$smoke_root" in
  "$HOME/.reticulum"*|"$HOME/.nomadnetwork"*|"$HOME/.lxmd"*|"$HOME/.config/OMENbrowser"*|"$HOME/.config/OMENbrowser_rs"*)
    echo "refusing unsafe smoke root: $smoke_root" >&2
    exit 2
    ;;
esac

mkdir -p "$OMENBROWSER_CONFIG_DIR" "$OMENBROWSER_DATA_DIR" "$OMENBROWSER_CACHE_DIR"

cd "$repo_root"

echo "repo_root: $repo_root"
echo "smoke_root: $smoke_root"
rustc --version
cargo --version

run() {
  echo "== $* =="
  "$@"
}

run cargo check --no-default-features --features desktop-ui
run cargo check --no-default-features --features chat-client
run cargo check --no-default-features --features chat-client-rns-clean
run cargo test --no-default-features --features desktop-ui

echo "RESULT: PASS"
