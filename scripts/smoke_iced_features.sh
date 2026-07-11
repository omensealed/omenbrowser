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

run_features() {
  local label="$1"
  local features="$2"
  shift 2
  IFS=',' read -ra parts <<< "$features"
  for feature in "${parts[@]}"; do
    if ! feature_exists "$feature"; then
      echo "SKIP: $label; missing feature $feature"
      return 0
    fi
  done
  echo "== $label =="
  cargo check --no-default-features --features "$features" "$@"
}

run_features "desktop-ui + desktop-widgets" "desktop-ui,desktop-widgets"
run_features "desktop-ui + desktop-widgets + chat-client" "desktop-ui,desktop-widgets,chat-client"
run_features "desktop-ui + desktop-widgets + desktop-dnd" "desktop-ui,desktop-widgets,desktop-dnd"
run_features "desktop-ui + desktop-widgets + desktop-animations" "desktop-ui,desktop-widgets,desktop-animations"
run_features "desktop-ui + markdown/qr/svg" "desktop-ui,desktop-markdown,desktop-qr,desktop-svg"
run_features "desktop optional full compile set" "desktop-ui,desktop-widgets,desktop-dnd,desktop-animations,desktop-markdown,desktop-qr,desktop-svg,chat-client"

if feature_exists desktop-ui-test; then
  echo "SKIP: desktop-ui-test tests"
  echo "reason: iced/tester currently pulls rfd/ashpd with async-std while desktop-ui uses rfd/ashpd with tokio"
else
  echo "SKIP: desktop-ui-test tests; missing feature desktop-ui-test"
fi

echo "RESULT: PASS"
