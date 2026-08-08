#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
readonly repo_root
readonly pinned_ref=15320e4d2cfabb143c1db20ca887e275fd521585
readonly upstream_url=https://github.com/markqvist/Reticulum.git
readonly pinned_lxmf_ref=727830cefda83d9c6e3982b48675425f3f988f9c
readonly upstream_lxmf_url=https://github.com/markqvist/LXMF.git
readonly msgpack_version=1.2.1

temporary_root=$(mktemp -d "${TMPDIR:-/tmp}/omen-pinned-python.XXXXXX")
cleanup() {
  rm -rf -- "$temporary_root"
}
trap cleanup EXIT

fetch_ref() {
  local destination=$1
  local url=$2
  local ref=$3
  mkdir -p -- "$destination"
  git -C "$destination" init --quiet
  GIT_TERMINAL_PROMPT=0 git -C "$destination" fetch --quiet --depth 1 \
    "$url" "$ref"
  git -C "$destination" checkout --quiet --detach FETCH_HEAD
}

case $# in
  0)
    rns_source="$temporary_root/reticulum"
    lxmf_source="$temporary_root/lxmf"
    fetch_ref "$rns_source" "$upstream_url" "$pinned_ref"
    fetch_ref "$lxmf_source" "$upstream_lxmf_url" "$pinned_lxmf_ref"
    ;;
  2)
    if [[ "$1" != "--rns-source" ]]; then
      echo "usage: $0 [--rns-source /path/to/Reticulum [--lxmf-source /path/to/LXMF]]" >&2
      exit 2
    fi
    rns_source=$2
    lxmf_source="$temporary_root/lxmf"
    fetch_ref "$lxmf_source" "$upstream_lxmf_url" "$pinned_lxmf_ref"
    ;;
  4)
    if [[ "$1" != "--rns-source" || "$3" != "--lxmf-source" ]]; then
      echo "usage: $0 [--rns-source /path/to/Reticulum [--lxmf-source /path/to/LXMF]]" >&2
      exit 2
    fi
    rns_source=$2
    lxmf_source=$4
    ;;
  *)
    echo "usage: $0 [--rns-source /path/to/Reticulum [--lxmf-source /path/to/LXMF]]" >&2
    exit 2
    ;;
esac

rns_source=$(realpath -- "$rns_source")
lxmf_source=$(realpath -- "$lxmf_source")

python3 -m venv "$temporary_root/venv"
"$temporary_root/venv/bin/python" -m pip install \
  --disable-pip-version-check --no-input --quiet \
  "msgpack==$msgpack_version"
export PATH="$temporary_root/venv/bin:$PATH"

lxmf_revision=$(git -C "$lxmf_source" rev-parse HEAD)
if [[ "$lxmf_revision" != "$pinned_lxmf_ref" ]]; then
  echo "Python LXMF source is not the release-blocking pinned revision: expected=$pinned_lxmf_ref actual=$lxmf_revision" >&2
  exit 1
fi
if [[ -n "$(git -C "$lxmf_source" status --porcelain --untracked-files=all)" ]]; then
  echo "Python LXMF source tree has local or untracked changes" >&2
  exit 1
fi

python3 "$repo_root/scripts/verify-ifac-python-vector.py" \
  --rns-source "$rns_source"

cargo test --locked --manifest-path "$repo_root/Cargo.toml" --lib \
  --no-default-features --features desktop-product \
  fixed_identity_preserves_python_compatible_destination_hashes

cargo test --locked --manifest-path "$repo_root/Cargo.toml" --lib \
  --no-default-features --features desktop-product \
  clean_reticulum_stale_receipt_cannot_complete_a_newer_retry

cargo test --locked --manifest-path "$repo_root/Cargo.toml" --lib \
  --no-default-features --features desktop-product \
  clean_timeout_persistence_keeps_late_proof_scoped_to_old_attempt

cargo test --locked --manifest-path "$repo_root/Cargo.toml" --lib \
  --no-default-features --features desktop-product \
  clean_timeout_replacement_survives_abrupt_process_termination \
  -- --test-threads=1

cargo test --locked --manifest-path "$repo_root/Cargo.toml" --lib \
  --no-default-features --features desktop-product \
  every_replace_fault_boundary_leaves_one_complete_thread_and_cleans_stage

cargo test --locked --manifest-path "$repo_root/Cargo.toml" --lib \
  --no-default-features --features desktop-product \
  process_kill_at_replace_boundaries_preserves_old_or_new_thread \
  -- --test-threads=1

cargo test --locked --manifest-path "$repo_root/Cargo.toml" --lib \
  --no-default-features --features desktop-product \
  abandoned_leased_stage_is_removed_but_unleased_legacy_stage_is_retained

cargo test --locked --manifest-path "$repo_root/Cargo.toml" --lib \
  --no-default-features --features desktop-product \
  recovery_never_removes_a_stage_with_a_live_lease

cargo test --locked --manifest-path "$repo_root/Cargo.toml" --lib \
  --no-default-features --features desktop-product \
  recovery_never_removes_a_same_process_active_publisher

cargo test --locked --manifest-path "$repo_root/Cargo.toml" --lib \
  --no-default-features --features desktop-product \
  recovery_retains_unrecognized_names_and_malformed_leases

cargo test --locked --manifest-path "$repo_root/Cargo.toml" --lib \
  --no-default-features --features desktop-product \
  publication_artifact_inventory_is_bounded

cargo test --locked --manifest-path "$repo_root/Cargo.toml" --lib \
  --no-default-features --features desktop-product \
  exact_publication_artifact_ceiling_recovers_abandoned_and_retains_live_writer \
  -- --test-threads=1

cargo test --locked --manifest-path "$repo_root/Cargo.toml" --lib \
  --no-default-features --features desktop-product \
  repeated_precommit_crashes_recover_in_one_bounded_pass \
  -- --test-threads=1

cargo test --locked --manifest-path "$repo_root/Cargo.toml" --lib \
  --no-default-features --features desktop-product \
  clean_runtime_restart_recovers_only_current_persisted_receipt_correlation

cargo test --locked --manifest-path "$repo_root/Cargo.toml" --lib \
  --no-default-features --features desktop-product \
  clean_runtime_process_restart_recovers_only_current_persisted_correlation \
  -- --test-threads=1

cargo test --locked --manifest-path "$repo_root/src/server/Cargo.toml" \
  -p omen-ifac-tcp ifac_wire_matches_pinned_python_reticulum_vector

env -u OMEN_PYTHON_RNS_SOURCE -u OMEN_PYTHON_RNS_VERSION \
  OMEN_PINNED_RNS_SOURCE="$rns_source" \
  cargo test --locked --manifest-path "$repo_root/src/server/Cargo.toml" \
  -p omen-ifac-tcp --test pinned_python_tcp -- --ignored --nocapture --test-threads=1

env -u OMEN_PYTHON_RNS_SOURCE -u OMEN_PYTHON_RNS_VERSION \
  OMEN_PINNED_RNS_SOURCE="$rns_source" \
  cargo test --locked --manifest-path "$repo_root/src/server/Cargo.toml" \
  -p omen-ifac-tcp --test pinned_python_reticulum -- \
  --ignored --nocapture --test-threads=1

env -u OMEN_PYTHON_RNS_SOURCE -u OMEN_PYTHON_RNS_VERSION \
  OMEN_PINNED_RNS_SOURCE="$rns_source" \
  cargo test --locked --manifest-path "$repo_root/src/server/Cargo.toml" \
  --no-default-features --features server-headless \
  pinned_python_nomadnet_rust_responder_four_quadrants -- \
  --ignored --nocapture --test-threads=1

env -u OMEN_PYTHON_RNS_SOURCE -u OMEN_PYTHON_RNS_VERSION \
  OMEN_PINNED_RNS_SOURCE="$rns_source" \
  OMEN_PINNED_LXMF_SOURCE="$lxmf_source" \
  cargo test --locked --manifest-path "$repo_root/Cargo.toml" --lib \
  --no-default-features --features desktop-product \
  pinned_python_lxmf_propagation_sync_is_received_and_acknowledged -- \
  --ignored --nocapture --test-threads=1

env -u OMEN_PYTHON_RNS_SOURCE -u OMEN_PYTHON_RNS_VERSION \
  OMEN_PINNED_RNS_SOURCE="$rns_source" \
  OMEN_PINNED_LXMF_SOURCE="$lxmf_source" \
  cargo test --locked --manifest-path "$repo_root/Cargo.toml" --lib \
  --no-default-features --features desktop-product \
  pinned_python_lxmf_propagation_stamp_boundaries_match_rust -- \
  --ignored --nocapture --test-threads=1

env -u OMEN_PYTHON_RNS_SOURCE -u OMEN_PYTHON_RNS_VERSION \
  OMEN_PINNED_RNS_SOURCE="$rns_source" \
  OMEN_PINNED_LXMF_SOURCE="$lxmf_source" \
  cargo test --locked --manifest-path "$repo_root/Cargo.toml" --lib \
  --no-default-features --features desktop-product \
  pinned_python_lxmf_network_propagation_accepts_and_rejects_rust_stamps -- \
  --ignored --nocapture --test-threads=1

env -u OMEN_PYTHON_RNS_SOURCE -u OMEN_PYTHON_RNS_VERSION \
  OMEN_PINNED_RNS_SOURCE="$rns_source" \
  OMEN_PINNED_LXMF_SOURCE="$lxmf_source" \
  cargo test --locked --manifest-path "$repo_root/Cargo.toml" --lib \
  --no-default-features --features desktop-product \
  pinned_python_lxmf_ticket_issue_use_expiry_and_reuse_match_rust -- \
  --ignored --nocapture --test-threads=1

env -u OMEN_PYTHON_RNS_SOURCE -u OMEN_PYTHON_RNS_VERSION \
  OMEN_PINNED_RNS_SOURCE="$rns_source" \
  OMEN_PINNED_LXMF_SOURCE="$lxmf_source" \
  cargo test --locked --manifest-path "$repo_root/Cargo.toml" --lib \
  --no-default-features --features desktop-product \
  pinned_python_lxmf_first_direct_send_discovers_stamp_policy_before_encoding -- \
  --ignored --nocapture --test-threads=1

env -u OMEN_PYTHON_RNS_SOURCE -u OMEN_PYTHON_RNS_VERSION \
  OMEN_PINNED_RNS_SOURCE="$rns_source" \
  OMEN_PINNED_LXMF_SOURCE="$lxmf_source" \
  cargo test --locked --manifest-path "$repo_root/Cargo.toml" --lib \
  --no-default-features --features desktop-product \
  pinned_python_lxmf_stamped_direct_resource_preserves_bytes_and_reports_progress -- \
  --ignored --nocapture --test-threads=1

env -u OMEN_PYTHON_RNS_SOURCE -u OMEN_PYTHON_RNS_VERSION \
  OMEN_PINNED_RNS_SOURCE="$rns_source" \
  OMEN_PINNED_LXMF_SOURCE="$lxmf_source" \
  cargo test --locked --manifest-path "$repo_root/Cargo.toml" --lib \
  --no-default-features --features desktop-product \
  pinned_python_lxmf_live_direct_stamp_accepts_stamped_and_rejects_unstamped -- \
  --ignored --nocapture --test-threads=1

env -u OMEN_PYTHON_RNS_SOURCE -u OMEN_PYTHON_RNS_VERSION \
  OMEN_PINNED_RNS_SOURCE="$rns_source" \
  OMEN_PINNED_LXMF_SOURCE="$lxmf_source" \
  cargo test --locked --manifest-path "$repo_root/Cargo.toml" --lib \
  --no-default-features --features desktop-product \
  pinned_python_lxmf_live_ticket_roundtrip_uses_rust_issued_ticket -- \
  --ignored --nocapture --test-threads=1

echo "pinned Python Reticulum/LXMF vector/TCP/link/proof/NomadNet-response-matrix/propagation/stamp/direct-policy/direct-resource/direct-stamp/ticket/live-ticket/restart-recovery interoperability: pass (RNS $pinned_ref / LXMF $pinned_lxmf_ref)"
