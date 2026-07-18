#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

duration="${OMENCHATD_LINK_SOAK_SECONDS:-60}"
output="${1:-/tmp/omenchatd-links-$(date -u +%Y%m%dT%H%M%SZ)}"
test_name='live::link_soak_tests::live_link_admission_expires_slow_handshakes_and_recovers_under_reconnect_storm'

if [[ "$(uname -s)" != "Linux" || ! -r /proc/self/status || ! -d /proc/self/fd || ! -d /proc/self/task ]]; then
  echo "the omenchatd link RSS/FD/task soak currently requires Linux /proc" >&2
  exit 2
fi
if ! [[ "$duration" =~ ^[0-9]+$ ]] || (( duration < 1 || duration > 600 )); then
  echo "OMENCHATD_LINK_SOAK_SECONDS must be an integer from 1 through 600" >&2
  exit 2
fi
for tool in cargo rg; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    echo "missing link measurement tool: $tool" >&2
    exit 2
  fi
done

mkdir -p "$output"
raw="$output/soak.log"
echo "== omenchatd live-link admission/reconnect soak (${duration}s) =="
OMENCHATD_LINK_SOAK_SECONDS="$duration" \
  cargo test --release --locked --manifest-path src/server/Cargo.toml \
    --no-default-features --features server-headless "$test_name" \
    -- --exact --ignored --nocapture 2>&1 | tee "$raw"

summary="$(rg '^LINK_SOAK_SUMMARY ' "$raw" | tail -n 1)"
if [[ -z "$summary" ]]; then
  echo "link soak completed without a machine-readable summary" >&2
  exit 1
fi
printf '%s\n' "$summary" > "$output/summary-line.txt"
printf '%s\n' "$summary" | tr ' ' '\n' | tail -n +2 > "$output/summary.txt"

value() {
  local key="$1"
  sed -n "s/^${key}=//p" "$output/summary.txt"
}

[[ "$(value duration_seconds)" == "$duration" ]]
(( $(value cycles) >= duration * 10 ))
(( $(value resident_links) == 224 ))
(( $(value pending_limit) == 32 ))
(( $(value active_limit) == 256 ))
(( $(value peak_active) == $(value active_limit) ))
(( $(value peak_pending) == $(value pending_limit) ))
(( $(value rejected) == $(value cycles) ))
(( $(value expired) == $(value cycles) * $(value pending_limit) ))
(( $(value transport_closes) == $(value links_closed) + $(value rejected) ))
(( $(value max_close_us) <= $(value close_deadline_us) ))
(( $(value rss_delta_bytes) <= $(value allowed_rss_delta_bytes) ))
(( $(value fd_growth) <= $(value allowed_fd_growth) ))
(( $(value task_growth) <= $(value allowed_task_growth) ))
(( $(value final_active) == 0 ))
(( $(value final_pending) == 0 ))

{
  printf 'utc=%s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  printf 'host=%s\n' "$(uname -srvmo)"
  printf 'test=%s\n' "$test_name"
  printf 'resident_identified_links=224\npending_handshake_limit=32\nactive_link_limit=256\n'
  printf 'handshake_timeout_seconds=30\ncycle_pause_ms=10\nclose_deadline_us=250000\n'
  printf 'rss_growth_limit_bytes=67108864\nfd_growth_limit=4\ntask_growth_limit=2\n'
  rustc -Vv | sed 's/^/rustc_/'
  cargo -V | sed 's/^/cargo_/'
} > "$output/metadata.txt"

cat "$output/metadata.txt" "$output/summary.txt"
echo "raw results: $output"
