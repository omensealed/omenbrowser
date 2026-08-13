#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source_file="$repo_root/src/server/src/reticulum_live_multiprocess_tests.rs"
tmp="$(mktemp -d "${TMPDIR:-/tmp}/omen-resource-verifier.XXXXXX")"
trap 'rm -rf "$tmp"' EXIT

run_verifier() {
  OMEN_RETICULUM_RESOURCE_TEST_FILE="$tmp/tests.rs" \
    bash "$repo_root/scripts/verify-reticulum-resource-compat.sh" >/dev/null 2>&1
}

reset_fixture() {
  cp "$source_file" "$tmp/tests.rs"
}

expect_failure() {
  local label="$1"
  if run_verifier; then
    echo "Resource verifier fixture unexpectedly passed: $label" >&2
    exit 1
  fi
}

reset_fixture
run_verifier

sed -i 's/reticulum_routed_resource_retransmission_survives_fragment_loss/renamed_routed_sentinel/' "$tmp/tests.rs"
expect_failure "routed sentinel renamed"

reset_fixture
sed -i '/known upstream Reticulum 0.9.9 routed Resource retransmission regression/d' "$tmp/tests.rs"
expect_failure "routed sentinel unignored"

reset_fixture
sed -i '/panic!(/,/^    );/d' "$tmp/tests.rs"
expect_failure "routed sentinel made unconditional pass"

reset_fixture
sed -i 's/upstream_udp_buffer >= max_type_one_wire_packet/upstream_udp_buffer <= max_type_one_wire_packet/' "$tmp/tests.rs"
expect_failure "maximum-UDP assertion weakened"

reset_fixture
sed -i '/async fn reticulum_split_metadata_assembly_preserves_segment_two_payload/i #[ignore = "incorrectly ignored"]' "$tmp/tests.rs"
expect_failure "split regression ignored"

echo "Reticulum Resource compatibility verifier fixtures: pass"
