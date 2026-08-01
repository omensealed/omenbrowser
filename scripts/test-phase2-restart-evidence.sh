#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

run_root_test() {
  local test_name="$1"
  cargo test --locked --no-default-features --features desktop-product \
    --lib "$test_name" -- --exact
}

run_server_test() {
  local test_name="$1"
  cargo test --locked --manifest-path src/server/Cargo.toml \
    --no-default-features --features server-full \
    --lib "$test_name" -- --exact
}

echo "== client uncertainty and backend-boundary evidence =="
run_root_test \
  "desktop::omenchat_mutations::tests::restart_recovery_is_identity_scoped_visible_and_never_transmits"
run_root_test \
  "chat::mutation_intents::tests::reaction_intent_survives_restart_without_changing_its_canonical_request"
run_root_test \
  "runtime::native_lxmf::client::tests::external_rpc_096_send_capture_proves_preserved_and_dropped_fields"
run_root_test \
  "runtime::native_lxmf::client::tests::embedded_sdk_sender_covers_direct_and_propagated_ticket_matrix"
run_root_test \
  "runtime::native::request::tests::pre_cancelled_nomadnet_request_dispatches_neither_packet_nor_resource"

echo "== server commit, restart, replacement-Link, and Resource evidence =="
run_server_test \
  "store::tests::process_kill_preserves_committed_event_and_rolls_back_in_flight_event"
run_server_test \
  "upload::tests::process_kill_upload_recovery_is_conservative_at_every_commit_boundary"
run_server_test \
  "session::tests::durable_room_text_replays_after_server_restart_without_new_event"
run_server_test \
  "live::tests::durable_room_action_replay_on_replacement_link_uses_new_sequence_without_refanout"
run_server_test \
  "session::tests::durable_reaction_commit_replay_conflict_and_snapshots_survive_restart"
run_server_test \
  "session::tests::dormant_message_revision_executor_replays_across_restart_without_refanout"
run_server_test \
  "live::tests::live_server_link_close_releases_owned_pending_upload_offers"
run_server_test \
  "live::tests::inbound_resource_failure_releases_peer_upload_offers_without_closing_link"

echo "Phase 2 deterministic restart/failure evidence: pass"
