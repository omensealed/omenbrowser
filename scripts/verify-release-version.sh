#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

expected="${OMEN_RELEASE_VERSION:-0.10.0-5}"
expected_protocol="${OMENCHAT_PROTOCOL_CRATE_VERSION:-0.2.0}"
expected_schema="${OMENCHATD_SCHEMA_VERSION:-14}"

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

assert_exact() {
  local label="$1"
  local expected_value="$2"
  local actual="$3"
  if [[ "$actual" != "$expected_value" ]]; then
    echo "$label mismatch: expected $expected_value, found ${actual:-<missing>}" >&2
    exit 1
  fi
  echo "$label=$actual"
}

assert_exact protocol-manifest "$expected_protocol" \
  "$(manifest_version src/server/crates/omenchat-protocol/Cargo.toml)"
assert_exact protocol-root-lock "$expected_protocol" \
  "$(lock_version Cargo.lock omenchat-protocol)"
assert_exact protocol-server-lock "$expected_protocol" \
  "$(lock_version src/server/Cargo.lock omenchat-protocol)"

schema_version="$(sed -n 's/^pub(crate) const SCHEMA_VERSION: i64 = \([0-9][0-9]*\);$/\1/p' src/server/src/store.rs)"
assert_exact omenchatd-schema "$expected_schema" "$schema_version"

echo "release version check: pass"
echo "wire protocol/config/cache versions: intentionally independent"
