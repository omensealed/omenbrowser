#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
tmp="$(mktemp -d "${TMPDIR:-/tmp}/omen-capability-docs.XXXXXX")"
trap 'rm -rf "$tmp"' EXIT

cp "$repo_root/docs/RETICULUM_TRANSPORT_API_GAP.md" "$tmp/gap.md"
cp "$repo_root/docs/upstream/RETICULUM_0_10_0_OMEN_CAPABILITY_LEDGER.md" "$tmp/ledger.md"
cp "$repo_root/docs/upstream/reticulum-rs-0.10.0-passive-announce-retention.md" "$tmp/passive.md"
cp "$repo_root/docs/upstream/reticulum-rs-0.10.0-announce-broadcast-ladder.md" "$tmp/broadcast.md"

run_verifier() {
  OMEN_RETICULUM_GAP_DOC="$tmp/gap.md" \
  OMEN_RETICULUM_CAPABILITY_LEDGER="$tmp/ledger.md" \
  OMEN_RETICULUM_PASSIVE_ANNOUNCE_DOC="$tmp/passive.md" \
  OMEN_RETICULUM_ANNOUNCE_BROADCAST_DOC="$tmp/broadcast.md" \
    bash "$repo_root/scripts/verify-reticulum-capability-docs.sh" >/dev/null 2>&1
}

expect_failure() {
  local label="$1"
  if run_verifier; then
    echo "capability documentation fixture unexpectedly passed: $label" >&2
    exit 1
  fi
}

run_verifier

printf '\noversized Python path is not yet qualified\n' >> "$tmp/gap.md"
expect_failure "stale Request Resource claim"
cp "$repo_root/docs/RETICULUM_TRANSPORT_API_GAP.md" "$tmp/gap.md"

sed -i 's/resource-routed-fragment-loss=unsupported/resource-routed-fragment-loss=supported/' "$tmp/ledger.md"
expect_failure "routed limitation promoted"
cp "$repo_root/docs/upstream/RETICULUM_0_10_0_OMEN_CAPABILITY_LEDGER.md" "$tmp/ledger.md"

sed -i 's/resource-maximum-udp=unsupported/resource-maximum-udp=supported/' "$tmp/ledger.md"
expect_failure "UDP limitation promoted"
cp "$repo_root/docs/upstream/RETICULUM_0_10_0_OMEN_CAPABILITY_LEDGER.md" "$tmp/ledger.md"

sed -i 's/transport-passive-announce-retention=unsupported/transport-passive-announce-retention=supported/' "$tmp/ledger.md"
expect_failure "passive announce limitation promoted"
cp "$repo_root/docs/upstream/RETICULUM_0_10_0_OMEN_CAPABILITY_LEDGER.md" "$tmp/ledger.md"

sed -i '/transport-announce-broadcast-ladder=unsupported/d' "$tmp/ledger.md"
expect_failure "announce broadcast marker removed"
cp "$repo_root/docs/upstream/RETICULUM_0_10_0_OMEN_CAPABILITY_LEDGER.md" "$tmp/ledger.md"

sed -i 's/reticulum-rs-0.10.0-passive-announce-retention.md/missing-passive-record.md/' "$tmp/ledger.md"
expect_failure "passive announce marker detached"
cp "$repo_root/docs/upstream/RETICULUM_0_10_0_OMEN_CAPABILITY_LEDGER.md" "$tmp/ledger.md"

sed -i '/OMEN carries no local patch/d' "$tmp/broadcast.md"
expect_failure "announce broadcast no-patch condition removed"
cp "$repo_root/docs/upstream/reticulum-rs-0.10.0-announce-broadcast-ladder.md" "$tmp/broadcast.md"

sed -i '/Removal condition: select an official published fixed release/d' "$tmp/passive.md"
expect_failure "passive announce removal condition removed"

echo "Reticulum capability documentation verifier fixtures: pass"
