#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

test_file="${OMEN_RETICULUM_RESOURCE_TEST_FILE:-src/server/src/reticulum_live_multiprocess_tests.rs}"
routed_doc="${OMEN_RETICULUM_ROUTED_DOC:-docs/upstream/reticulum-rs-0.9.9-routed-resource-retransmission.md}"
udp_doc="${OMEN_RETICULUM_UDP_DOC:-docs/upstream/reticulum-rs-0.9.9-udp-max-wire-buffer.md}"

fail() {
  echo "Reticulum Resource compatibility verification failed: $*" >&2
  exit 1
}

test_block() {
  local name="$1"
  sed -n "/fn ${name}(/,/^}/p" "$test_file"
}

attribute_block() {
  local name="$1"
  grep -B3 -F "fn ${name}(" "$test_file"
}

if ! grep -Eq '^rns_transport[[:space:]]*=.*version[[:space:]]*=[[:space:]]*"=0\.9\.9"' Cargo.toml \
  || ! grep -Eq '^rns_transport[[:space:]]*=.*version[[:space:]]*=[[:space:]]*"=0\.9\.9"' src/server/Cargo.toml; then
  echo "Resource compatibility rebaseline requires the exact Reticulum 0.9.9 train" >&2
  exit 1
fi

[[ -f "$test_file" ]] || fail "missing Resource sentinel source"
[[ -f "$routed_doc" ]] || fail "missing routed retransmission evidence document"
[[ -f "$udp_doc" ]] || fail "missing maximum-UDP evidence document"

if [[ -e src/resource_compat.rs || -e src/server/src/resource_compat.rs ]]; then
  fail "obsolete Reticulum 0.9.7 split-Resource guard module remains"
fi

if rg -n 'RETICULUM_0_9_7|metadata_bearing_resource_is_unsplit_safe|exact_train_upload_payload_max' \
  src --glob '*.rs'; then
  fail "obsolete exact-0.9.7 split-Resource safeguard remains"
fi

split='reticulum_split_metadata_assembly_preserves_segment_two_payload'
udp='reticulum_udp_tx_buffer_covers_max_resource_wire_packet'
routed='reticulum_routed_resource_retransmission_survives_fragment_loss'

for name in "$split" "$udp" "$routed"; do
  [[ "$(grep -Fc "fn ${name}(" "$test_file")" == "1" ]] ||
    fail "${name} must exist exactly once"
done

if attribute_block "$split" | grep -Fq '#[ignore'; then
  fail "the fixed split-metadata regression must remain a normal test"
fi
test_block "$split" | grep -Fq 'upstream split-metadata regression sentinel failed on official 0.9.9' ||
  fail "split-metadata exact-train assertion is missing"

attribute_block "$udp" | grep -Fq '#[ignore = "known upstream Reticulum 0.9.9 UDP maximum-Resource serialization regression"]' ||
  fail "the independent maximum-UDP limitation must retain its exact ignored reason"
test_block "$udp" | grep -Fq 'upstream_udp_buffer >= max_type_one_wire_packet' ||
  fail "maximum-UDP sentinel no longer asserts buffer sufficiency"
test_block "$udp" | grep -Fq 'PACKET_MDU' ||
  fail "maximum-UDP sentinel no longer uses the authoritative packet bound"

attribute_block "$routed" | grep -Fq '#[ignore = "known upstream Reticulum 0.9.9 routed Resource retransmission regression; requires the documented three-node fragment-loss topology"]' ||
  fail "the routed retransmission limitation must retain its exact ignored reason"
test_block "$routed" | grep -Fq 'panic!' ||
  fail "routed sentinel was converted into an unconditional pass"
test_block "$routed" | grep -Fq 'suppresses requested duplicate Resource data/proof packets' ||
  fail "routed sentinel no longer records the exact upstream failure"

grep -Fq '`reticulum-rs-transport 0.9.9`' "$routed_doc" ||
  fail "routed evidence does not name the exact affected train"
grep -Fq 'three-node' "$routed_doc" ||
  fail "routed evidence does not retain the required topology"
grep -Fq 'does not carry a local patch' "$routed_doc" ||
  fail "routed evidence does not retain the no-patch boundary"
grep -Fq '`reticulum-rs-transport 0.9.9`' "$udp_doc" ||
  fail "UDP evidence does not name the exact affected train"
grep -Fq '456 bytes' "$udp_doc" || fail "UDP evidence does not retain the observed bound"
grep -Fq '483 bytes' "$udp_doc" || fail "UDP evidence does not retain the required wire size"

echo "Reticulum 0.9.9 Resource compatibility: split regression normal; routed and maximum-UDP sentinels protected"
