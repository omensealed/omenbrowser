#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

duration="${OMENCHATD_DB_SOAK_SECONDS:-60}"
output="${1:-/tmp/omenchatd-db-soak-$(date -u +%Y%m%dT%H%M%SZ)}"
test_name='reticulum_live::db_soak_tests::persistent_sqlite_worker_stays_responsive_and_commits_monotonic_events_under_load'

if [[ "$(uname -s)" != "Linux" || ! -r /proc/self/status || ! -d /proc/self/fd ]]; then
  echo "the omenchatd database RSS/FD soak currently requires Linux /proc" >&2
  exit 2
fi
if ! [[ "$duration" =~ ^[0-9]+$ ]] || (( duration < 1 || duration > 600 )); then
  echo "OMENCHATD_DB_SOAK_SECONDS must be an integer from 1 through 600" >&2
  exit 2
fi
for tool in cargo rg; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    echo "missing database measurement tool: $tool" >&2
    exit 2
  fi
done

mkdir -p "$output"
raw="$output/soak.log"
echo "== omenchatd persistent SQLite/live-worker soak (${duration}s) =="
OMENCHATD_DB_SOAK_SECONDS="$duration" \
  cargo test --release --locked --manifest-path src/server/Cargo.toml \
    --no-default-features --features server-headless "$test_name" \
    -- --exact --ignored --nocapture 2>&1 | tee "$raw"

summary="$(rg '^DB_SOAK_SUMMARY ' "$raw" | tail -n 1)"
if [[ -z "$summary" ]]; then
  echo "database soak completed without a machine-readable summary" >&2
  exit 1
fi
printf '%s\n' "$summary" > "$output/summary-line.txt"
printf '%s\n' "$summary" | tr ' ' '\n' | tail -n +2 > "$output/summary.txt"

value() {
  local key="$1"
  sed -n "s/^${key}=//p" "$output/summary.txt"
}

[[ "$(value duration_seconds)" == "$duration" ]]
[[ "$(value integrity)" == "ok" ]]
(( $(value accepted) >= duration * 10 ))
(( $(value busy_rejected) > 0 ))
(( $(value worker_completed) == $(value setup_completed) + $(value accepted) ))
(( $(value worker_rejected) == $(value busy_rejected) ))
(( $(value max_in_flight) <= 1 ))
(( $(value heartbeat_max_us) <= 250000 ))
(( $(value rss_delta_bytes) <= $(value allowed_rss_delta_bytes) ))
(( $(value committed_soak_events) == $(value accepted) ))

{
  printf 'utc=%s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  printf 'host=%s\n' "$(uname -srvmo)"
  printf 'test=%s\n' "$test_name"
  printf 'producers=8\nsubmit_interval_ms=10\nheartbeat_interval_ms=10\n'
  printf 'worker_admission=1\nheartbeat_limit_us=250000\nrss_growth_limit_bytes=67108864\n'
  rustc -Vv | sed 's/^/rustc_/'
  cargo -V | sed 's/^/cargo_/'
} > "$output/metadata.txt"

cat "$output/metadata.txt" "$output/summary.txt"
echo "raw results: $output"
