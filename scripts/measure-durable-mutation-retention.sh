#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

items="${OMEN_DURABLE_MEASUREMENT_ITEMS:-1024}"
output="${1:-/tmp/omen-durable-retention-$(date -u +%Y%m%dT%H%M%SZ)}"
server_test='store::durable_replay::tests::durable_replay_retention_measurement'
client_test='chat::mutation_intents::tests::durable_intent_retention_measurement'

if ! [[ "$items" =~ ^[0-9]+$ ]] || (( items < 256 || items > 4096 )); then
  echo "OMEN_DURABLE_MEASUREMENT_ITEMS must be an integer from 256 through 4096" >&2
  exit 2
fi
for tool in cargo git rg rustc uname; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    echo "missing durable-retention measurement tool: $tool" >&2
    exit 2
  fi
done

mkdir -p "$output"
server_log="$output/server-replay.log"
client_log="$output/client-intents.log"

echo "== omenchatd durable replay retention ($items items) =="
OMEN_DURABLE_MEASUREMENT_ITEMS="$items" \
  cargo test --release --locked --manifest-path src/server/Cargo.toml \
    --no-default-features --lib "$server_test" -- --exact --ignored --nocapture \
    2>&1 | tee "$server_log"

echo "== OMENbrowser durable intent retention ($items items) =="
OMEN_DURABLE_MEASUREMENT_ITEMS="$items" \
  cargo test --release --locked --no-default-features --features desktop-product \
    --lib "$client_test" -- --exact --ignored --nocapture \
    2>&1 | tee "$client_log"

server_summary="$(rg '^DURABLE_REPLAY_MEASUREMENT ' "$server_log" | tail -n 1)"
client_summary="$(rg '^MUTATION_INTENT_MEASUREMENT ' "$client_log" | tail -n 1)"
if [[ -z "$server_summary" || -z "$client_summary" ]]; then
  echo "durable-retention measurement completed without both machine-readable summaries" >&2
  exit 1
fi

value() {
  local line="$1" key="$2"
  tr ' ' '\n' <<<"$line" | awk -F= -v key="$key" '$1 == key {print $2; exit}'
}
require_equal() {
  local actual="$1" expected="$2" label="$3"
  if [[ "$actual" != "$expected" ]]; then
    echo "$label: expected $expected, got $actual" >&2
    exit 1
  fi
}
require_at_most() {
  local actual="$1" ceiling="$2" label="$3"
  if [[ -z "$actual" ]] || (( actual > ceiling )); then
    echo "$label exceeded activation threshold: $actual > $ceiling" >&2
    exit 1
  fi
}

retained=$((items / 2))
require_equal "$(value "$server_summary" items)" "$items" "server fixture items"
require_equal "$(value "$server_summary" result_rows)" "$retained" "server retained results"
require_equal "$(value "$server_summary" client_rows)" "$items" "server client registry"
require_equal "$(value "$server_summary" retired_rows)" "$((items - retained))" "server retired clients"
require_equal "$(value "$client_summary" items)" "$items" "client fixture items"
require_equal "$(value "$client_summary" recovered)" "$items" "client recovered intents"
require_equal "$(value "$client_summary" pruned)" "$items" "client pruned intents"
require_equal "$(value "$client_summary" prune_calls)" "$(((items + 127) / 128))" "client bounded prune calls"

# These release-mode thresholds are deliberately broad regression triggers,
# not claims about all hardware. Structural bounds above remain mandatory.
require_at_most "$(value "$server_summary" database_bytes)" $((16 * 1024 * 1024)) "server fixture bytes"
require_at_most "$(value "$client_summary" database_bytes)" $((32 * 1024 * 1024)) "client fixture bytes"
require_at_most "$(value "$server_summary" commit_p95_us)" 50000 "server commit p95"
require_at_most "$(value "$server_summary" replay_p95_us)" 10000 "server replay p95"
require_at_most "$(value "$server_summary" commit_max_us)" 250000 "server commit max"
require_at_most "$(value "$client_summary" prepare_p95_us)" 50000 "client prepare p95"
require_at_most "$(value "$client_summary" prepare_max_us)" 250000 "client prepare max"
require_at_most "$(value "$client_summary" recovery_us)" 2000000 "client recovery"

{
  printf 'utc=%s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  printf 'head=%s\nitems=%s\n' "$(git rev-parse HEAD)" "$items"
  printf 'host=%s\n' "$(uname -a)"
  rustc -Vv | sed 's/^/rustc_/'
} >"$output/metadata.txt"
printf '%s\n%s\n' "$server_summary" "$client_summary" >"$output/summary-lines.txt"
{
  printf 'status=pass\n'
  tr ' ' '\n' <<<"$server_summary" | tail -n +2 | sed 's/^/server_/'
  tr ' ' '\n' <<<"$client_summary" | tail -n +2 | sed 's/^/client_/'
} >"$output/summary.txt"

cat "$output/metadata.txt" "$output/summary.txt"
echo "durable-retention results: $output"
