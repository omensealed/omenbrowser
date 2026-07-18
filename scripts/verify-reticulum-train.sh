#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

if ! command -v jq >/dev/null 2>&1; then
  echo "jq is required to verify Cargo package versions and sources" >&2
  exit 2
fi

expected_version="0.9.5"
expected_source="registry+https://github.com/rust-lang/crates.io-index"
family_regex='^(lxmf|lxmf-sdk|lxmf-wire|lxmf-runtime|lxmf-embedded-mini|reticulum-rs|reticulum-rs-core|reticulum-rs-transport|reticulum-rs-rpc|rns-embedded-core|rns-embedded-runtime|rns-embedded-ffi|rns-embedded-mininode)$'

verify_manifest_pins() {
  local manifest="$1"
  shift
  local dependency
  for dependency in "$@"; do
    if ! grep -Eq "^${dependency}[[:space:]]*=.*version[[:space:]]*=[[:space:]]*\"=${expected_version}\"" "$manifest"; then
      echo "$manifest does not exactly pin $dependency to =$expected_version" >&2
      exit 1
    fi
  done
}

verify_metadata() {
  local label="$1"
  local metadata="$2"
  local family

  if jq -e 'any(.packages[]; .name == "zeromq")' <<<"$metadata" >/dev/null; then
    echo "$label unexpectedly enables the unused ZeroMQ SDK backend" >&2
    exit 1
  fi

  family="$(jq -c --arg regex "$family_regex" '
    [.packages[]
      | select(.name | test($regex))
      | {name, version, source}]
    | unique_by([.name, .version, .source])
    | sort_by(.name)
  ' <<<"$metadata")"

  if [[ "$(jq 'length' <<<"$family")" -eq 0 ]]; then
    echo "$label resolved no Reticulum/LXMF family packages" >&2
    exit 1
  fi

  if jq -e --arg version "$expected_version" \
    'any(.[]; .version != $version)' <<<"$family" >/dev/null; then
    echo "$label contains a Reticulum/LXMF package outside $expected_version" >&2
    jq -r '.[] | "  \(.name) \(.version) \(.source)"' <<<"$family" >&2
    exit 1
  fi

  if jq -e --arg source "$expected_source" \
    'any(.[]; .source != $source)' <<<"$family" >/dev/null; then
    echo "$label contains a non-registry Reticulum/LXMF package source" >&2
    jq -r '.[] | "  \(.name) \(.version) \(.source)"' <<<"$family" >&2
    exit 1
  fi

  if [[ "$(jq '[group_by(.name)[] | select(length != 1)] | length' <<<"$family")" -ne 0 ]]; then
    echo "$label contains duplicate Reticulum/LXMF package identities" >&2
    jq -r '.[] | "  \(.name) \(.version) \(.source)"' <<<"$family" >&2
    exit 1
  fi

  echo "$label Reticulum/LXMF train: pass"
  jq -r '.[] | "  \(.name) \(.version) registry"' <<<"$family"
}

verify_manifest_pins Cargo.toml lxmf lxmf_sdk reticulum-rs rns_transport rns_rpc
verify_manifest_pins src/server/Cargo.toml reticulum-rs rns_transport

root_metadata="$(cargo metadata --locked --format-version 1 \
  --no-default-features --features desktop-product)"
server_metadata="$(cargo metadata --locked --format-version 1 \
  --manifest-path src/server/Cargo.toml \
  --no-default-features --features server-headless)"

verify_metadata "OMENbrowser" "$root_metadata"
verify_metadata "omenchatd" "$server_metadata"
