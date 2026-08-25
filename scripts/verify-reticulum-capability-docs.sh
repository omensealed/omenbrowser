#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
gap_doc="${OMEN_RETICULUM_GAP_DOC:-$repo_root/docs/RETICULUM_TRANSPORT_API_GAP.md}"
ledger="${OMEN_RETICULUM_CAPABILITY_LEDGER:-$repo_root/docs/upstream/RETICULUM_0_10_0_OMEN_CAPABILITY_LEDGER.md}"

fail() {
  echo "Reticulum capability documentation verification failed: $*" >&2
  exit 1
}

[[ -f "$gap_doc" ]] || fail "missing transport-gap document"
[[ -f "$ledger" ]] || fail "missing 0.10.0 capability ledger"

require_marker() {
  local capability="$1"
  local expected="$2"
  local marker="<!-- omen-capability:${capability}=${expected} -->"
  [[ "$(grep -Fxc "$marker" "$ledger")" == "1" ]] ||
    fail "${capability} must have exactly one ${expected} marker"
}

require_marker nomadnet-direct-request-response supported
require_marker nomadnet-request-resource supported
require_marker nomadnet-response-resource supported
require_marker resource-split-metadata supported
require_marker resource-direct-local supported
require_marker resource-routed-fragment-loss unsupported
require_marker resource-maximum-udp unsupported
require_marker managed-integrated-runtime supported
require_marker external-rpc-durable-send unsupported
require_marker external-shared-runtime unknown

if grep -Eqi 'oversized Python (request )?path is not yet qualified' "$gap_doc"; then
  fail "stale oversized current-Python Request Resource claim returned"
fi

grep -Fq 'current-Python four-quadrant matrix' "$gap_doc" ||
  fail "transport-gap document does not retain current-Python quadrant evidence"
grep -Fqi 'retransmission after fragment loss' "$gap_doc" ||
  fail "transport-gap document does not distinguish routed fragment-loss evidence"
grep -Fq 'maximum-UDP' "$gap_doc" ||
  fail "transport-gap document does not retain the independent UDP boundary"

echo "Reticulum 0.10.0 capability documentation: pass"
