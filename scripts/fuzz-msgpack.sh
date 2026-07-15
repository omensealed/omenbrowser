#!/usr/bin/env bash
set -euo pipefail

runs="${FUZZ_RUNS:-10000}"
max_len="${FUZZ_MAX_LEN:-4194305}"
artifact_root="${FUZZ_ARTIFACT_ROOT:-/tmp/omenbrowser-rs-fuzz-artifacts}"
toolchain="${FUZZ_TOOLCHAIN:-nightly}"
expected_cargo_fuzz="${FUZZ_EXPECTED_CARGO_FUZZ:-0.13.2}"
expected_nightly_commit="${FUZZ_EXPECTED_NIGHTLY_COMMIT:-14cae681329a63c622a6e1fbe1d30f9374bc51d8}"

case "$runs:$max_len" in
  *[!0-9:]*|0:*|*:0) echo "FUZZ_RUNS and FUZZ_MAX_LEN must be positive integers" >&2; exit 2 ;;
esac
cargo "+$toolchain" fuzz --version >/dev/null 2>&1 || {
  echo "cargo-fuzz and Rust toolchain '$toolchain' are required" >&2
  exit 2
}
actual_cargo_fuzz="$(cargo "+$toolchain" fuzz --version | awk '{print $2}')"
if [[ "$actual_cargo_fuzz" != "$expected_cargo_fuzz" ]]; then
  echo "cargo-fuzz $expected_cargo_fuzz is required; found $actual_cargo_fuzz" >&2
  exit 2
fi
actual_nightly_commit="$(rustc "+$toolchain" -Vv | awk '/^commit-hash:/ {print $2}')"
if [[ "$actual_nightly_commit" != "$expected_nightly_commit" ]]; then
  echo "nightly commit $expected_nightly_commit is required; found $actual_nightly_commit" >&2
  exit 2
fi

mkdir -p "$artifact_root/client" "$artifact_root/server"
mkdir -p fuzz/corpus/client_msgpack fuzz/corpus/server_msgpack
truncate -s "$max_len" fuzz/corpus/client_msgpack/max-length-seed
truncate -s "$max_len" fuzz/corpus/server_msgpack/max-length-seed
cargo "+$toolchain" fuzz run client_msgpack --fuzz-dir fuzz -- \
  -runs="$runs" -max_len="$max_len" -artifact_prefix="$artifact_root/client/"
cargo "+$toolchain" fuzz run server_msgpack --fuzz-dir fuzz -- \
  -runs="$runs" -max_len="$max_len" -artifact_prefix="$artifact_root/server/"
