#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

binary="${OMENBROWSER_BINARY:-target/release/omenbrowser_rs}"
warmup_seconds="${WARMUP_SECONDS:-60}"
sample_seconds="${SAMPLE_SECONDS:-600}"
interval_seconds="${INTERVAL_SECONDS:-1}"
headless="${HEADLESS:-auto}"
case_order="${CASE_ORDER:-normal-first}"
output="${1:-/tmp/omenbrowser-low-power-$(date -u +%Y%m%dT%H%M%SZ)}"

case "$case_order" in
  normal-first|low-power-first) ;;
  *) echo "CASE_ORDER must be normal-first or low-power-first" >&2; exit 2 ;;
esac
if [[ -e "$output" ]]; then
  echo "low-power measurement output already exists: $output" >&2
  exit 2
fi
if [[ ! -x "$binary" ]]; then
  echo "release binary is missing or not executable: $binary" >&2
  exit 2
fi

mkdir -p "$output"
binary="$(realpath "$binary")"
binary_hash="$(sha256sum "$binary" | awk '{print $1}')"

run_case() {
  local preset="$1" messages="$2" case_output="$3"
  OMENBROWSER_BINARY="$binary" \
    WARMUP_SECONDS="$warmup_seconds" \
    SAMPLE_SECONDS="$sample_seconds" \
    INTERVAL_SECONDS="$interval_seconds" \
    HEADLESS="$headless" \
    MEASUREMENT_SECTION=monitoring \
    MEASUREMENT_PRESET="$preset" \
    RECURRING_APP_MESSAGES_PER_MINUTE="$messages" \
    RECURRING_APP_MESSAGES_SOURCE=configured_subscription_cadence \
    bash scripts/measure-desktop-idle.sh "$case_output"
}

if [[ "$case_order" == "normal-first" ]]; then
  run_case normal 60 "$output/normal"
  run_case low-power 12 "$output/low-power"
else
  run_case low-power 12 "$output/low-power"
  run_case normal 60 "$output/normal"
fi

value() {
  local file="$1" key="$2"
  awk -F= -v key="$key" '$1 == key {sub(/^[^=]*=/, ""); print; exit}' "$file"
}

for case_name in normal low-power; do
  metadata="$output/$case_name/metadata.txt"
  [[ "$(value "$metadata" binary_sha256)" == "$binary_hash" ]] || {
    echo "binary changed during low-power measurement" >&2
    exit 1
  }
  [[ "$(value "$metadata" measurement_section)" == "monitoring" ]] || {
    echo "measurement did not keep Monitoring visible" >&2
    exit 1
  }
  [[ "$(value "$metadata" measurement_preset)" == "$case_name" ]] || {
    echo "measurement preset mismatch for $case_name" >&2
    exit 1
  }
done

bash scripts/compare-desktop-idle.sh \
  "$output/normal" "$output/low-power" | tee "$output/comparison.txt"

{
  printf 'utc=%s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  printf 'binary=%s\nbinary_sha256=%s\n' "$binary" "$binary_hash"
  printf 'warmup_seconds=%s\nsample_seconds=%s\ninterval_seconds=%s\n' \
    "$warmup_seconds" "$sample_seconds" "$interval_seconds"
  printf 'case_order=%s\n' "$case_order"
  printf 'measurement_section=monitoring\n'
  printf 'normal_configured_samples_per_minute=60\n'
  printf 'low_power_configured_samples_per_minute=12\n'
  printf 'configured_sample_reduction_percent=80\n'
  printf 'gpu_measurement=not_collected_by_software_rendered_harness\n'
} >"$output/metadata.txt"

cat "$output/metadata.txt" "$output/comparison.txt"
echo "raw results: $output"
