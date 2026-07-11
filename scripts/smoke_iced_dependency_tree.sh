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

feature_exists() {
  local feature="$1"
  grep -qE "^${feature}[[:space:]]*=" Cargo.toml
}

echo "== duplicate Iced dependency scan =="
cargo tree -d | grep -E 'iced|iced_aw|iced_core|iced_widget|iced_runtime|wgpu|winit' || true

if feature_exists desktop-widgets; then
  echo "== desktop widgets tree =="
  cargo tree --features desktop-ui,desktop-widgets,chat-client | grep -E 'iced|iced_aw|iced_gif|iced_fonts' || true
else
  echo "SKIP: desktop-widgets feature is not declared yet"
fi

echo "== clean network dependency guard =="
if cargo tree --features native-network 2>/dev/null | grep -qE '(^|[[:space:]])rns-net v'; then
  echo "RESULT: FAIL"
  echo "reason: rns-net appears in native-network dependency graph"
  exit 1
fi

echo "RESULT: PASS"
