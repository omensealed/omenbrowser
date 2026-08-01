#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

duration="${OMENCHATD_IDLE_SECONDS:-15}"
output="${1:-/tmp/omenchatd-idle-$(date -u +%Y%m%dT%H%M%SZ)}"
home="$output/isolated-home"
binary="${OMENCHATD_IDLE_BINARY:-$repo_root/src/server/target/release/omenchatd}"
pid=""

cleanup() {
  if [[ -n "$pid" ]] && kill -0 "$pid" 2>/dev/null; then
    kill -TERM "$pid" 2>/dev/null || true
    wait "$pid" 2>/dev/null || true
  fi
  rm -rf "$home"
}
trap cleanup EXIT

if [[ "$(uname -s)" != "Linux" || ! -r /proc/self/stat ]]; then
  echo "omenchatd idle evidence currently requires Linux /proc" >&2
  exit 2
fi
if ! [[ "$duration" =~ ^[0-9]+$ ]] || (( duration < 5 || duration > 600 )); then
  echo "OMENCHATD_IDLE_SECONDS must be an integer from 5 through 600" >&2
  exit 2
fi
for tool in awk cargo getconf rg sha256sum; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    echo "missing omenchatd idle measurement tool: $tool" >&2
    exit 2
  fi
done

if [[ -z "${OMENCHATD_IDLE_BINARY:-}" ]]; then
  cargo build --release --locked --manifest-path src/server/Cargo.toml \
    --no-default-features --features server-headless
fi
if [[ ! -x "$binary" ]]; then
  echo "omenchatd measurement binary is unavailable: $binary" >&2
  exit 2
fi

mkdir -p "$output"
rm -rf "$home"
"$binary" init --home "$home" >"$output/init.log" 2>&1
"$binary" run --home "$home" >"$output/server.log" 2>&1 &
pid=$!

for _ in $(seq 1 100); do
  if rg -q '^omenchatd reticulum-rs live server ready$' "$output/server.log"; then
    break
  fi
  if ! kill -0 "$pid" 2>/dev/null; then
    echo "omenchatd stopped before readiness" >&2
    tail -n 20 "$output/server.log" >&2
    exit 1
  fi
  sleep 0.1
done
if ! rg -q '^omenchatd reticulum-rs live server ready$' "$output/server.log"; then
  echo "omenchatd did not report readiness within 10 seconds" >&2
  exit 1
fi

read_cpu_ticks() {
  awk '{print $14 + $15}' "/proc/$pid/stat"
}

ticks_per_second="$(getconf CLK_TCK)"
start_ticks="$(read_cpu_ticks)"
start_epoch="$(date +%s)"
sleep "$duration"
end_ticks="$(read_cpu_ticks)"
end_epoch="$(date +%s)"
elapsed=$((end_epoch - start_epoch))
cpu_ticks=$((end_ticks - start_ticks))
rss_kib="$(awk '/^VmRSS:/ {print $2}' "/proc/$pid/status")"
threads="$(find "/proc/$pid/task" -mindepth 1 -maxdepth 1 -type d | wc -l)"
fds="$(find "/proc/$pid/fd" -mindepth 1 -maxdepth 1 | wc -l)"
readiness="$(rg '^readiness: ' "$output/server.log" | tail -n 1 | cut -d' ' -f2-)"

kill -TERM "$pid"
wait "$pid"
pid=""

{
  printf 'utc=%s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  printf 'host=%s\n' "$(uname -srvmo)"
  printf 'binary=%s\n' "$binary"
  printf 'binary_sha256=%s\n' "$(sha256sum "$binary" | awk '{print $1}')"
  printf 'feature_identity=server-headless\n'
  printf 'sample_seconds=%s\n' "$elapsed"
  printf 'cpu_ticks=%s\n' "$cpu_ticks"
  printf 'clock_ticks_per_second=%s\n' "$ticks_per_second"
  printf 'rss_kib=%s\n' "$rss_kib"
  printf 'threads=%s\n' "$threads"
  printf 'file_descriptors=%s\n' "$fds"
  printf 'readiness=%s\n' "$readiness"
  printf 'announce_cadence=configuration_minutes\n'
  printf 'handshake_sweep_seconds=1\n'
  printf 'statistics_deadline_seconds=30\n'
  rustc -Vv | sed 's/^/rustc_/'
  cargo -V | sed 's/^/cargo_/'
} >"$output/summary.txt"

cat "$output/summary.txt"
echo "isolated server home removed; evidence: $output"
