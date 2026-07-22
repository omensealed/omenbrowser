#!/usr/bin/env bash
set -euo pipefail

mode="${1:-metadata}"
case "$mode" in
  metadata|check)
    ;;
  *)
    echo "usage: $0 [metadata|check]" >&2
    exit 2
    ;;
esac

server_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
copy_root="$(mktemp -d "${TMPDIR:-/tmp}/omenchatd-standalone.XXXXXX")"
target_parent="${CARGO_TARGET_DIR:-$server_root/target}"
mkdir -p -- "$target_parent"
target_dir="$(mktemp -d "$target_parent/omenchatd-standalone-target.XXXXXX")"
cleanup() {
  rm -rf -- "$copy_root" "$target_dir"
}
trap cleanup EXIT

tar -C "$server_root" --exclude=target -cf - . | tar -C "$copy_root" -xf -

cargo metadata --offline --locked \
  --manifest-path "$copy_root/Cargo.toml" \
  --format-version 1 --no-deps >/dev/null

if [[ "$mode" == "check" ]]; then
  CARGO_TARGET_DIR="$target_dir" cargo check --offline --locked \
    --manifest-path "$copy_root/Cargo.toml" \
    --no-default-features --features server-headless
  CARGO_TARGET_DIR="$target_dir" cargo test --offline --locked \
    --manifest-path "$copy_root/Cargo.toml" \
    --no-default-features --features server-headless --no-run
  CARGO_TARGET_DIR="$target_dir" cargo test --offline --locked \
    --manifest-path "$copy_root/Cargo.toml" -p omen-ifac-tcp
fi

echo "omenchatd standalone relocation check: pass ($mode)"
