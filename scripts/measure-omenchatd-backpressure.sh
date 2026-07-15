#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

duration="${OMENCHATD_QUEUE_SOAK_SECONDS:-60}"
output="${1:-/tmp/omenchatd-backpressure-$(date -u +%Y%m%dT%H%M%SZ)}"
test_name='reticulum_live::soak_tests::production_queues_bound_slow_resource_consumers_and_keep_control_responsive'

if [[ "$(uname -s)" != "Linux" || ! -r /proc/self/status || ! -d /proc/self/fd ]]; then
  echo "the omenchatd RSS/FD soak currently requires Linux /proc" >&2
  exit 2
fi
if ! [[ "$duration" =~ ^[0-9]+$ ]] || (( duration < 1 || duration > 600 )); then
  echo "OMENCHATD_QUEUE_SOAK_SECONDS must be an integer from 1 through 600" >&2
  exit 2
fi
for tool in cargo rg; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    echo "missing backpressure measurement tool: $tool" >&2
    exit 2
  fi
done

mkdir -p "$output"
raw="$output/soak.log"
echo "== omenchatd production-queue backpressure soak (${duration}s) =="
OMENCHATD_QUEUE_SOAK_SECONDS="$duration" \
  cargo test --release --locked --manifest-path src/server/Cargo.toml \
    --no-default-features --features server-headless "$test_name" \
    -- --exact --ignored --nocapture 2>&1 | tee "$raw"

summary="$(rg '^SOAK_SUMMARY ' "$raw" | tail -n 1)"
if [[ -z "$summary" ]]; then
  echo "soak completed without a machine-readable summary" >&2
  exit 1
fi
printf '%s\n' "$summary" > "$output/summary-line.txt"
printf '%s\n' "$summary" | tr ' ' '\n' | tail -n +2 > "$output/summary.txt"

value() {
  local key="$1"
  sed -n "s/^${key}=//p" "$output/summary.txt"
}

[[ "$(value duration_seconds)" == "$duration" ]]
(( $(value producer_consumer_target) >= 10 ))
(( $(value transport_rejected) > 0 ))
(( $(value event_rejected) > 0 ))
(( $(value transport_controls) > 1 ))
(( $(value event_controls) > 1 ))
(( $(value max_control_latency_ms) <= 250 ))
(( $(value rss_delta_bytes) <= $(value allowed_rss_delta_bytes) ))
(( $(value final_transport_items) == 0 ))
(( $(value final_transport_bytes) == 0 ))
(( $(value final_event_items) == 0 ))
(( $(value final_event_bytes) == 0 ))

{
  printf 'utc=%s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  printf 'host=%s\n' "$(uname -srvmo)"
  printf 'test=%s\n' "$test_name"
  printf 'resource_bytes=65536\nproducer_interval_ms=1\nconsumer_interval_ms=20\n'
  printf 'transport_payload_items=256\ntransport_control_items=32\n'
  printf 'transport_bytes=16777216\ntransport_per_link_bytes=4194304\n'
  printf 'event_payload_items=512\nevent_control_items=64\n'
  printf 'event_bytes=33554432\nevent_per_link_bytes=8388608\n'
  rustc -Vv | sed 's/^/rustc_/'
  cargo -V | sed 's/^/cargo_/'
} > "$output/metadata.txt"

cat "$output/metadata.txt" "$output/summary.txt"
echo "raw results: $output"
