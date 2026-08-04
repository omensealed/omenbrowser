#!/usr/bin/env bash
set -euo pipefail

out_dir="${1:-target/arm64-dist}"
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

for tool in cross podman; do
  command -v "$tool" >/dev/null 2>&1 || {
    echo "Linux ARM64 headless gate requires $tool" >&2
    exit 1
  }
done

export CROSS_CONTAINER_ENGINE="${CROSS_CONTAINER_ENGINE:-podman}"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-target/aarch64-cross}"
target="aarch64-unknown-linux-gnu"

echo "== ARM64 OMENchat protocol tests =="
cross test --locked \
  --manifest-path src/server/crates/omenchat-protocol/Cargo.toml \
  --target "$target"

echo "== ARM64 headless omenchatd tests =="
# These parent tests directly re-exec the ARM test binary and therefore bypass
# Cross's QEMU runner. Their child fixtures still compile, and the same crash-
# boundary and permissive-umask tests remain mandatory in the native host
# matrix.
cross test --locked --manifest-path src/server/Cargo.toml \
  --target "$target" \
  --no-default-features --features server-headless -- \
  --skip config::tests::init_creates_complete_private_tree_under_permissive_subprocess_umask \
  --skip store::tests::sqlite_main_wal_and_shm_are_private_under_permissive_subprocess_umask \
  --skip store::tests::process_kill_preserves_committed_event_and_rolls_back_in_flight_event \
  --skip upload::tests::process_kill_upload_recovery_is_conservative_at_every_commit_boundary

echo "== ARM64 release package and emulated lifecycle =="
bash scripts/package-linux-arm64-omenchatd.sh "$out_dir" --cross-emulated

echo "Linux ARM64 headless gate: pass"
echo "evidence: cross-compiled and QEMU-executed through Podman/Cross"
echo "hardware-device qualification: not required by this gate"
