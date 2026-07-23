#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/../.." && pwd)"
smoke_root="${OMENBROWSER_SMOKE_ROOT:-${TMPDIR:-/tmp}/omenbrowser-rs-smoke}"
run_root="$smoke_root/$(date -u +%Y%m%dT%H%M%SZ)"
mkdir -p "$run_root/logs"

report="${SMOKE_TEST_REPORT:-$repo_root/target/smoke/SMOKE_TEST_REPORT.md}"
mkdir -p "$(dirname "$report")"
commit="$(git -C "$repo_root" rev-parse --short HEAD 2>/dev/null || printf unknown)"
os="$(uname -a)"
rust="$(rustc --version 2>/dev/null || printf 'rustc unavailable')"
cargo_version="$(cargo --version 2>/dev/null || printf 'cargo unavailable')"

scripts=(
  00_build_matrix.sh
  01_feature_inventory.sh
  02_omenchat_server_loopback.sh
  03_omenchat_two_client.sh
  04_omenchat_resource_transfer.sh
  05_lxmf_service_loopback.sh
  06_lxmf_cli_interop.sh
  07_reticulumd_rpc_interop.sh
  08_nomadnet_page_fetch.sh
  09_network_doctor.sh
  10_omenchat_scroll.sh
)

declare -a rows
overall=0

for smoke_script in "${scripts[@]}"; do
  log="$run_root/logs/${smoke_script%.sh}.log"
  echo "== running $smoke_script =="
  set +e
  SMOKE_RUN_ROOT="$run_root/${smoke_script%.sh}" bash "$script_dir/$smoke_script" > "$log" 2>&1
  code=$?
  set -e
  if grep -q '^RESULT: PASS' "$log"; then
    result="PASS"
  elif grep -q '^RESULT: SKIP' "$log"; then
    result="SKIP"
  else
    result="FAIL"
    overall=1
  fi
  reason="$(grep '^reason:' "$log" | tail -n 1 | sed 's/^reason: //' || true)"
  rows+=("| \`$smoke_script\` | $result | \`${log}\` | ${reason:-} |")
  printf '%s %s\n' "$result" "$smoke_script"
done

{
  echo "# Smoke Test Report"
  echo
  echo "## $(date -u +%Y-%m-%dT%H:%M:%SZ)"
  echo
  echo "- git commit: \`$commit\`"
  echo "- repo root: \`$repo_root\`"
  echo "- smoke root: \`$run_root\`"
  echo "- OS: \`$os\`"
  echo "- Rust: \`$rust\`"
  echo "- Cargo: \`$cargo_version\`"
  echo "- feature flags tested: default, chat-client-reticulum, native-network, native-lxmf-sdk, live-reticulum where available"
  echo
  echo "| Script | Result | Log | Notes |"
  echo "|---|---|---|---|"
  printf '%s\n' "${rows[@]}"
  echo
  if [[ "$overall" -eq 0 ]]; then
    echo "Overall result: PASS with documented SKIP entries."
  else
    echo "Overall result: FAIL. Inspect the failing log paths above."
  fi
} > "$report"

cat "$report"
exit "$overall"
