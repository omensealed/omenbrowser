#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

expected="${OMEN_RELEASE_VERSION:-0.9.6-6}"

manifest_version() {
  awk '
    /^\[package\]$/ { in_package = 1; next }
    /^\[/ { in_package = 0 }
    in_package && /^version = "/ {
      value = $0
      sub(/^version = "/, "", value)
      sub(/".*$/, "", value)
      print value
      exit
    }
  ' "$1"
}

lock_version() {
  awk -v package_name="$2" '
    $0 == "name = \"" package_name "\"" { found = 1; next }
    found && /^version = "/ {
      value = $0
      sub(/^version = "/, "", value)
      sub(/".*$/, "", value)
      print value
      exit
    }
  ' "$1"
}

assert_version() {
  local label="$1"
  local actual="$2"
  if [[ "$actual" != "$expected" ]]; then
    echo "$label version mismatch: expected $expected, found ${actual:-<missing>}" >&2
    exit 1
  fi
  echo "$label=$actual"
}

assert_version root-manifest "$(manifest_version Cargo.toml)"
assert_version root-lock "$(lock_version Cargo.lock omenbrowser_rs)"
assert_version server-manifest "$(manifest_version src/server/Cargo.toml)"
assert_version server-lock "$(lock_version src/server/Cargo.lock omenchatd)"

echo "release version check: pass"
echo "protocol/config/database/cache versions: intentionally independent"
