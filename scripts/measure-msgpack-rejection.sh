#!/usr/bin/env bash
set -euo pipefail

output="${1:-/tmp/omenbrowser-rs-msgpack-rejection.tsv}"
CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-target}" \
  cargo run --locked --release --manifest-path fuzz/Cargo.toml \
  --bin measure_msgpack >"$output"
cat "$output"
