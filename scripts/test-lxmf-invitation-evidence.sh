#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

features="desktop-product"

run_test() {
  local filter="$1"
  cargo test --locked --no-default-features --features "$features" --lib \
    "$filter" -- --nocapture
}

run_test signed_native_invitation_wire_enters_preview_without_history_or_action
run_test verified_signed_wire_message_rejects_forged_signature_and_identity_mismatch
run_test runtime_invitation_activation_requires_per_message_authenticated_source_evidence
run_test lxmf_invitation_preview_and_dismiss_never_open_join_or_trust

printf '%s\n' \
  "PASS: deterministic signed-wire -> verified source -> bounded preview evidence" \
  "NOTE: this does not claim live transport, external RPC provenance, prior-binary rendering, or peer support for outbound invitations."
