#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

if ! grep -Eq '^rns_transport[[:space:]]*=.*version[[:space:]]*=[[:space:]]*"=0\.9\.8"' Cargo.toml \
  || ! grep -Eq '^rns_transport[[:space:]]*=.*version[[:space:]]*=[[:space:]]*"=0\.9\.8"' src/server/Cargo.toml; then
  echo "Resource compatibility rebaseline requires the exact Reticulum 0.9.8 train" >&2
  exit 1
fi

for source in src/resource_compat.rs src/server/src/resource_compat.rs; do
  grep -Fq 'RETICULUM_0_9_7_MAX_EFFICIENT_RESOURCE_BYTES: usize = 1_048_575' "$source"
  grep -Fq 'RETICULUM_RESOURCE_METADATA_LENGTH_PREFIX_BYTES: usize = 3' "$source"
  grep -Fq 'metadata_bearing_resource_is_unsplit_safe' "$source"
done

grep -Fq 'reticulum_split_metadata_assembly_preserves_segment_two_payload' \
  src/server/src/reticulum_live_multiprocess_tests.rs
grep -Fq 'reticulum_udp_tx_buffer_covers_max_resource_wire_packet' \
  src/server/src/reticulum_live_multiprocess_tests.rs

echo "Reticulum 0.9.8 Resource rebaseline: legacy guard retained pending unchanged sentinels"
