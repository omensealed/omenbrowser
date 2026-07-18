#!/usr/bin/env bash
set -euo pipefail

root_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root_dir"

fail() {
  echo "native CLI identity smoke failed: $*" >&2
  exit 1
}

require_contains() {
  local value="$1"
  local expected="$2"
  local label="$3"
  [[ "$value" == *"$expected"* ]] \
    || fail "$label lacks expected identity '$expected'"
}

require_absent() {
  local value="$1"
  local forbidden="$2"
  local label="$3"
  [[ "$value" != *"$forbidden"* ]] \
    || fail "$label unexpectedly contains '$forbidden'"
}

expected_target="$(rustc -vV | sed -n 's/^host: //p')"
[[ -n "$expected_target" ]] || fail "rustc did not report a host target"

desktop_version="$(cargo run --quiet --locked --no-default-features \
  --features desktop-product --bin omenbrowser_rs -- --version)"
require_contains "$desktop_version" "target=$expected_target" "desktop product"
require_contains "$desktop_version" "profile=desktop-product" "desktop product"
require_contains "$desktop_version" "desktop-product:on" "desktop product"
require_contains "$desktop_version" "mock-runtime:off" "desktop product"
require_contains "$desktop_version" "desktop-test:off" "desktop product"
require_contains "$desktop_version" "native-network:on" "desktop product"
require_absent "$desktop_version" "mock-runtime:on" "desktop product"

tui_version="$(cargo run --quiet --locked --no-default-features \
  --features tui --bin omenbrowser_rs -- --version)"
require_contains "$tui_version" "target=$expected_target" "root TUI"
require_contains "$tui_version" "desktop-product:off" "root TUI"
require_contains "$tui_version" "desktop-ui:off" "root TUI"
require_contains "$tui_version" "tui:on" "root TUI"
require_contains "$tui_version" "mock-runtime:off" "root TUI"
require_absent "$tui_version" "mock-runtime:on" "root TUI"

headless_version="$(cargo run --quiet --locked \
  --manifest-path src/server/Cargo.toml --no-default-features \
  --features server-headless -- --version)"
require_contains "$headless_version" "server-headless:on" "omenchatd headless"
require_contains "$headless_version" "server-full:off" "omenchatd headless"
require_contains "$headless_version" "live-reticulum:on" "omenchatd headless"
require_contains "$headless_version" "tui:off" "omenchatd headless"

full_version="$(cargo run --quiet --locked \
  --manifest-path src/server/Cargo.toml --no-default-features \
  --features server-full -- --version)"
require_contains "$full_version" "server-headless:on" "omenchatd full"
require_contains "$full_version" "server-full:on" "omenchatd full"
require_contains "$full_version" "live-reticulum:on" "omenchatd full"
require_contains "$full_version" "tui:on" "omenchatd full"

desktop_help="$(cargo run --quiet --locked --no-default-features \
  --features desktop-product --bin omenbrowser_rs -- --help)"
require_contains "$desktop_help" "--desktop" "desktop product help"
require_contains "$desktop_help" "--app-root <dir>" "desktop product help"
require_contains "$desktop_help" "--native-startup" "desktop product help"

server_help="$(cargo run --quiet --locked \
  --manifest-path src/server/Cargo.toml --no-default-features \
  --features server-headless -- --help)"
require_contains "$server_help" "doctor [--home <path>] [--json]" "omenchatd help"
require_contains "$server_help" "status [--home <path>] [--json]" "omenchatd help"

printf '%s\n' "native CLI identity smoke: pass ($expected_target; desktop, TUI, omenchatd headless/full)"
