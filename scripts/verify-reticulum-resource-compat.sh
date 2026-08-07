#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

if ! grep -Eq '^rns_transport[[:space:]]*=.*version[[:space:]]*=[[:space:]]*"=0\.9\.8"' Cargo.toml \
  || ! grep -Eq '^rns_transport[[:space:]]*=.*version[[:space:]]*=[[:space:]]*"=0\.9\.8"' src/server/Cargo.toml; then
  echo "Resource compatibility rebaseline requires the exact Reticulum 0.9.8 train" >&2
  exit 1
fi

if [[ -e src/resource_compat.rs || -e src/server/src/resource_compat.rs ]]; then
  echo "obsolete Reticulum 0.9.7 split-Resource guard module remains" >&2
  exit 1
fi

if rg -n 'RETICULUM_0_9_7|metadata_bearing_resource_is_unsplit_safe|exact_train_upload_payload_max' \
  src --glob '*.rs'; then
  echo "obsolete exact-0.9.7 split-Resource safeguard remains" >&2
  exit 1
fi

grep -Fq 'reticulum_split_metadata_assembly_preserves_segment_two_payload' \
  src/server/src/reticulum_live_multiprocess_tests.rs
grep -Fq 'reticulum_udp_tx_buffer_covers_max_resource_wire_packet' \
  src/server/src/reticulum_live_multiprocess_tests.rs
if grep -B2 -F 'fn reticulum_split_metadata_assembly_preserves_segment_two_payload' \
  src/server/src/reticulum_live_multiprocess_tests.rs | grep -Fq '#[ignore'; then
  echo "the fixed split-metadata sentinel must be a normal 0.9.8 regression test" >&2
  exit 1
fi
if ! grep -B2 -F 'fn reticulum_udp_tx_buffer_covers_max_resource_wire_packet' \
  src/server/src/reticulum_live_multiprocess_tests.rs | grep -Fq '#[ignore'; then
  echo "the independent maximum-UDP limitation sentinel must remain explicit" >&2
  exit 1
fi

echo "Reticulum 0.9.8 Resource rebaseline: split guard retired; maximum-UDP sentinel retained"
