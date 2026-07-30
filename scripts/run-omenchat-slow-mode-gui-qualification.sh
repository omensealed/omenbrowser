#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
readonly repo_root
evidence_root="${TMPDIR:-/tmp}/omenchat-slow-mode-gui-evidence"
sample_seconds="${OMENCHAT_SLOW_MODE_SAMPLE_SECONDS:-0}"
warmup_seconds="${OMENCHAT_SLOW_MODE_WARMUP_SECONDS:-10}"
while (($#)); do
  case "$1" in
    --evidence)
      if (($# < 2)); then
        echo "--evidence requires a directory" >&2
        exit 2
      fi
      evidence_root=$2
      shift 2
      ;;
    *)
      echo "usage: $0 [--evidence /path/to/directory]" >&2
      exit 2
      ;;
  esac
done

if [[ ! "$sample_seconds" =~ ^[0-9]+$ ]] ||
    ((sample_seconds > 300)); then
  echo "OMENCHAT_SLOW_MODE_SAMPLE_SECONDS must be an integer from 0 through 300" >&2
  exit 2
fi
if [[ ! "$warmup_seconds" =~ ^[0-9]+$ ]] ||
    ((warmup_seconds > 300)); then
  echo "OMENCHAT_SLOW_MODE_WARMUP_SECONDS must be an integer from 0 through 300" >&2
  exit 2
fi
if ((sample_seconds > 0)) &&
    [[ "$(uname -s)" != "Linux" || ! -r /proc/self/status ]]; then
  echo "slow-mode process measurement requires Linux /proc" >&2
  exit 2
fi

for tool in Xvfb i3 xdpyinfo xdotool xprop xclip import jq rg python3; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    echo "missing slow-mode GUI qualification tool: $tool" >&2
    exit 2
  fi
done
if ((sample_seconds > 0)); then
  for tool in awk find getconf; do
    if ! command -v "$tool" >/dev/null 2>&1; then
      echo "missing slow-mode process measurement tool: $tool" >&2
      exit 2
    fi
  done
fi

session_root=$(mktemp -d "${TMPDIR:-/tmp}/omenchat-slow-mode-gui.XXXXXX")
browser_root="$session_root/browser"
server_root="$session_root/server"
xvfb_pid=""
wm_pid=""
app_pid=""
server_pid=""

cleanup() {
  for pid in "$app_pid" "$server_pid" "$wm_pid" "$xvfb_pid"; do
    if [[ -n "$pid" ]] && kill -0 "$pid" 2>/dev/null; then
      kill "$pid" 2>/dev/null || true
      wait "$pid" 2>/dev/null || true
    fi
  done
  rm -rf -- "$session_root"
}
report_error() {
  local status=$1
  local line=$2
  mkdir -p -- "$evidence_root"
  for name in \
    server-init.log server.log server-status.txt \
    browser-identity.stderr browser.stdout browser.stderr \
    browser-log.jsonl \
    xvfb.log i3.log; do
    if [[ -f "$session_root/$name" ]]; then
      cp -- "$session_root/$name" "$evidence_root/failed-$name"
    fi
  done
  echo "slow-mode GUI qualification failed at line $line (status $status)" >&2
  echo "failure evidence: $evidence_root" >&2
}
trap cleanup EXIT INT TERM
trap 'report_error "$?" "$LINENO"' ERR

mkdir -p -- "$evidence_root"

build_profile="debug"
build_profile_args=()
if ((sample_seconds > 0)); then
  build_profile="release"
  build_profile_args=(--release)
fi
cargo build --quiet --locked --manifest-path "$repo_root/Cargo.toml" \
  "${build_profile_args[@]}" \
  --no-default-features \
  --features desktop-product,omenchat-slow-mode-qualification \
  --bin omenbrowser_rs
cargo build --quiet --locked --manifest-path "$repo_root/src/server/Cargo.toml" \
  "${build_profile_args[@]}" \
  --no-default-features \
  --features server-headless,omenchat-slow-mode-qualification \
  --bin omenchatd

browser_bin="${CARGO_TARGET_DIR:-$repo_root/target}/$build_profile/omenbrowser_rs"
server_bin="${CARGO_TARGET_DIR:-$repo_root/src/server/target}/$build_profile/omenchatd"
port=$(python3 -c \
  'import socket; s=socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1]); s.close()')

"$server_bin" init --home "$server_root" --tcp-server "127.0.0.1:$port" \
  >"$session_root/server-init.log"
env OMENCHATD_QUALIFICATION_SLOW_MODE_TRANSITION=30 \
  "$server_bin" run --home "$server_root" --tcp-server "127.0.0.1:$port" \
  >"$session_root/server.log" 2>&1 &
server_pid=$!
for _ in {1..120}; do
  if rg -q 'live server ready' "$session_root/server.log"; then
    break
  fi
  if ! kill -0 "$server_pid" 2>/dev/null; then
    echo "qualification omenchatd exited before becoming ready" >&2
    exit 1
  fi
  sleep 0.1
done
rg -q 'live server ready' "$session_root/server.log"

"$server_bin" status --home "$server_root" >"$session_root/server-status.txt"
destination=$(
  sed -n 's/^client uri: omenchat:\/\/\([0-9a-fA-F]\+\)$/\1/p' \
    "$session_root/server-status.txt" |
    head -n 1
)
if [[ ! "$destination" =~ ^[0-9a-fA-F]{32}$ ]]; then
  echo "could not read isolated OMENchat destination" >&2
  exit 1
fi

"$browser_bin" \
  --generate-native-identity "Slow Mode GUI Qualification" \
  --app-root "$browser_root" \
  --stdout >"$session_root/browser-identity.json" \
  2>"$session_root/browser-identity.stderr"
identity_hash=$(jq -r '.identity.hash_hex' "$session_root/browser-identity.json")
identity_storage="$browser_root/identity_storage/default_identity-${identity_hash:0:16}"
mkdir -p -- "$identity_storage"
jq -n --argjson port "$port" '{
  profiles: [{
    profile_id: "qualification-loopback",
    name: "Qualification Loopback",
    kind: "tcp_client",
    enabled: true,
    target_host: "127.0.0.1",
    target_port: $port,
    network_name: "",
    passphrase: "",
    connectable: false,
    peers: [],
    device_port: "",
    frequency: 867200000,
    bandwidth: 125000,
    tx_power: 7,
    spreading_factor: 8,
    coding_rate: 5
  }]
}' >"$browser_root/interfaces.json"
chmod 600 "$browser_root/interfaces.json"

display_number=$((420 + ($$ % 150)))
while [[ -e "/tmp/.X11-unix/X$display_number" ]]; do
  display_number=$((display_number + 1))
done
test_display=":$display_number"
Xvfb "$test_display" -screen 0 1400x900x24 -nolisten tcp \
  >"$session_root/xvfb.log" 2>&1 &
xvfb_pid=$!
for _ in {1..100}; do
  if DISPLAY="$test_display" xdpyinfo >/dev/null 2>&1; then
    break
  fi
  sleep 0.05
done
DISPLAY="$test_display" xdpyinfo >/dev/null

DISPLAY="$test_display" I3SOCK="$session_root/i3.sock" \
  i3 -c "$repo_root/scripts/fixtures/i3-native-test.config" \
  >"$session_root/i3.log" 2>&1 &
wm_pid=$!
for _ in {1..100}; do
  if DISPLAY="$test_display" xprop -root _NET_SUPPORTING_WM_CHECK 2>/dev/null |
      rg -q 'window id'; then
    break
  fi
  sleep 0.05
done
DISPLAY="$test_display" xprop -root _NET_SUPPORTING_WM_CHECK 2>/dev/null |
  rg -q 'window id'

DISPLAY="$test_display" LIBGL_ALWAYS_SOFTWARE=1 \
OMENBROWSER_QUALIFICATION_OMENCHAT_TARGET="omenchat://$destination" \
RUST_LOG=omenbrowser_rs=info \
  "$browser_bin" --desktop --app-root "$browser_root" \
  >"$session_root/browser.stdout" 2>"$session_root/browser.stderr" &
app_pid=$!

window=""
for _ in {1..240}; do
  window=$(
    DISPLAY="$test_display" xdotool search --onlyvisible --pid "$app_pid" \
      --name '^OMENbrowser_rs$' 2>/dev/null |
      head -n 1 || true
  )
  if [[ -n "$window" ]]; then
    break
  fi
  if ! kill -0 "$app_pid" 2>/dev/null; then
    echo "desktop exited before opening a window" >&2
    exit 1
  fi
  sleep 0.05
done
if [[ -z "$window" ]]; then
  echo "desktop window did not become visible" >&2
  exit 1
fi

for _ in {1..300}; do
  if [[ -f "$browser_root/logs/omenbrowser_rs.jsonl" ]]; then
    cp -- "$browser_root/logs/omenbrowser_rs.jsonl" \
      "$session_root/browser-log.jsonl"
  fi
  if rg -q 'qualification slow-mode transition committed' "$session_root/server.log"; then
    break
  fi
  if ! kill -0 "$app_pid" 2>/dev/null; then
    echo "desktop exited before slow-mode projection" >&2
    exit 1
  fi
  sleep 0.1
done
rg -q 'qualification slow-mode transition committed: room=1 seconds=30' \
  "$session_root/server.log"
sleep 1

DISPLAY="$test_display" import -window "$window" "$evidence_root/connected.png"
if ((sample_seconds > 0)); then
  sleep "$warmup_seconds"
  printf 'epoch_ms\tbrowser_cpu_percent\tserver_cpu_percent\tbrowser_rss_kib\tserver_rss_kib\tbrowser_private_dirty_kib\tserver_private_dirty_kib\tbrowser_threads\tserver_threads\tbrowser_fds\tserver_fds\n' \
    >"$evidence_root/process-samples.tsv"
  browser_ticks=$(awk '{print $14+$15}' "/proc/$app_pid/stat")
  server_ticks=$(awk '{print $14+$15}' "/proc/$server_pid/stat")
  total_ticks=$(awk 'NR==1 {for(i=2;i<=NF;i++) total+=$i; print total}' /proc/stat)
  cpu_count=$(getconf _NPROCESSORS_ONLN)
  for ((sample = 0; sample < sample_seconds; sample++)); do
    sleep 1
    kill -0 "$app_pid" 2>/dev/null ||
      { echo "desktop exited during process measurement" >&2; exit 1; }
    kill -0 "$server_pid" 2>/dev/null ||
      { echo "omenchatd exited during process measurement" >&2; exit 1; }
    next_browser_ticks=$(awk '{print $14+$15}' "/proc/$app_pid/stat")
    next_server_ticks=$(awk '{print $14+$15}' "/proc/$server_pid/stat")
    next_total_ticks=$(
      awk 'NR==1 {for(i=2;i<=NF;i++) total+=$i; print total}' /proc/stat
    )
    browser_cpu=$(
      awk -v p="$next_browser_ticks" -v pp="$browser_ticks" \
        -v t="$next_total_ticks" -v pt="$total_ticks" -v n="$cpu_count" \
        'BEGIN {if(t>pt) printf "%.3f", 100*(p-pp)*n/(t-pt); else print "0.000"}'
    )
    server_cpu=$(
      awk -v p="$next_server_ticks" -v pp="$server_ticks" \
        -v t="$next_total_ticks" -v pt="$total_ticks" -v n="$cpu_count" \
        'BEGIN {if(t>pt) printf "%.3f", 100*(p-pp)*n/(t-pt); else print "0.000"}'
    )
    browser_ticks=$next_browser_ticks
    server_ticks=$next_server_ticks
    total_ticks=$next_total_ticks
    browser_rss=$(awk '/^VmRSS:/ {print $2}' "/proc/$app_pid/status")
    server_rss=$(awk '/^VmRSS:/ {print $2}' "/proc/$server_pid/status")
    browser_dirty=$(
      awk '/^Private_Dirty:/ {sum += $2} END {print sum + 0}' \
        "/proc/$app_pid/smaps_rollup"
    )
    server_dirty=$(
      awk '/^Private_Dirty:/ {sum += $2} END {print sum + 0}' \
        "/proc/$server_pid/smaps_rollup"
    )
    browser_threads=$(awk '/^Threads:/ {print $2}' "/proc/$app_pid/status")
    server_threads=$(awk '/^Threads:/ {print $2}' "/proc/$server_pid/status")
    browser_fds=$(
      find "/proc/$app_pid/fd" -mindepth 1 -maxdepth 1 2>/dev/null | wc -l
    )
    server_fds=$(
      find "/proc/$server_pid/fd" -mindepth 1 -maxdepth 1 2>/dev/null | wc -l
    )
    printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
      "$(( $(date +%s%N) / 1000000 ))" \
      "$browser_cpu" "$server_cpu" "$browser_rss" "$server_rss" \
      "$browser_dirty" "$server_dirty" "$browser_threads" "$server_threads" \
      "$browser_fds" "$server_fds" \
      >>"$evidence_root/process-samples.tsv"
  done
  awk -F '\t' '
    NR > 1 {
      browser_cpu[++n]=$2; server_cpu[n]=$3
      browser_rss[n]=$4; server_rss[n]=$5
      browser_dirty[n]=$6; server_dirty[n]=$7
      browser_threads[n]=$8; server_threads[n]=$9
      browser_fds[n]=$10; server_fds[n]=$11
    }
    function sort(values, count, i, j, temporary) {
      for (i=1; i<=count; i++) for (j=i+1; j<=count; j++)
        if (values[j] < values[i]) {
          temporary=values[i]; values[i]=values[j]; values[j]=temporary
        }
    }
    function median(values, count) {
      sort(values,count)
      return count%2 ? values[(count+1)/2] :
        (values[count/2]+values[count/2+1])/2
    }
    function p95(values, count, percentile_index) {
      sort(values,count); percentile_index=int(count*.95+.999)
      if (percentile_index < 1) percentile_index=1
      return values[percentile_index]
    }
    END {
      browser_rss_initial=browser_rss[1]
      browser_rss_final=browser_rss[n]
      server_rss_initial=server_rss[1]
      server_rss_final=server_rss[n]
      printf "samples=%d\n", n
      printf "browser_cpu_percent_median=%.3f\n", median(browser_cpu,n)
      printf "browser_cpu_percent_p95=%.3f\n", p95(browser_cpu,n)
      printf "server_cpu_percent_median=%.3f\n", median(server_cpu,n)
      printf "server_cpu_percent_p95=%.3f\n", p95(server_cpu,n)
      printf "browser_rss_kib_median=%.0f\n", median(browser_rss,n)
      printf "browser_rss_kib_p95=%.0f\n", p95(browser_rss,n)
      printf "server_rss_kib_median=%.0f\n", median(server_rss,n)
      printf "server_rss_kib_p95=%.0f\n", p95(server_rss,n)
      printf "browser_private_dirty_kib_p95=%.0f\n", p95(browser_dirty,n)
      printf "server_private_dirty_kib_p95=%.0f\n", p95(server_dirty,n)
      printf "browser_threads_p95=%.0f\n", p95(browser_threads,n)
      printf "server_threads_p95=%.0f\n", p95(server_threads,n)
      printf "browser_fds_p95=%.0f\n", p95(browser_fds,n)
      printf "server_fds_p95=%.0f\n", p95(server_fds,n)
      printf "browser_rss_growth_kib=%d\n", browser_rss_final-browser_rss_initial
      printf "server_rss_growth_kib=%d\n", server_rss_final-server_rss_initial
    }
  ' "$evidence_root/process-samples.tsv" >"$evidence_root/process-summary.txt"
fi

DISPLAY="$test_display" xdotool windowactivate --sync "$window"
DISPLAY="$test_display" xdotool mousemove --window "$window" 690 766 click 1
DISPLAY="$test_display" xdotool type --window "$window" --delay 1 \
  'qualification-first-message'
DISPLAY="$test_display" xdotool key --window "$window" Return
sleep 2
DISPLAY="$test_display" import -window "$window" "$evidence_root/first-admitted.png"

DISPLAY="$test_display" xdotool mousemove --window "$window" 690 766 click 1
DISPLAY="$test_display" xdotool type --window "$window" --delay 1 \
  'qualification-second-message'
DISPLAY="$test_display" xdotool key --window "$window" Return
sleep 2
DISPLAY="$test_display" import -window "$window" "$evidence_root/second-rejected.png"
DISPLAY="$test_display" xdotool mousemove --window "$window" 690 766 click 1
DISPLAY="$test_display" xdotool key --window "$window" ctrl+a ctrl+c
DISPLAY="$test_display" xclip -selection clipboard -o \
  >"$evidence_root/rejected-draft.txt"
if [[ $(<"$evidence_root/rejected-draft.txt") != "qualification-second-message" ]]; then
  echo "slow-mode rejection did not preserve the exact second draft" >&2
  exit 1
fi

python3 - "$server_root/omenchat.sqlite" >"$evidence_root/database-observation.json" <<'PY'
import json
import sqlite3
import sys

connection = sqlite3.connect(f"file:{sys.argv[1]}?mode=ro", uri=True)
rows = connection.execute(
    "SELECT event_id, CAST(payload AS TEXT) "
    "FROM room_events WHERE room_id = 1 AND event_kind = 1 ORDER BY event_id"
).fetchall()
print(json.dumps({"room_message_count": len(rows), "messages": rows}))
PY
jq -e \
  '.room_message_count == 1 and .messages[0][1] == "qualification-first-message"' \
  "$evidence_root/database-observation.json" >/dev/null

desktop_shutdown_start_ns=$(date +%s%N)
DISPLAY="$test_display" xdotool key --window "$window" alt+F4
for _ in {1..160}; do
  if ! kill -0 "$app_pid" 2>/dev/null; then
    break
  fi
  sleep 0.05
done
if kill -0 "$app_pid" 2>/dev/null; then
  echo "desktop did not close after connected-state capture" >&2
  exit 1
fi
wait "$app_pid"
app_pid=""
desktop_shutdown_end_ns=$(date +%s%N)

server_shutdown_start_ns=$(date +%s%N)
kill "$server_pid"
for _ in {1..160}; do
  if ! kill -0 "$server_pid" 2>/dev/null; then
    break
  fi
  sleep 0.05
done
if kill -0 "$server_pid" 2>/dev/null; then
  echo "omenchatd did not drain within 8 seconds" >&2
  exit 1
fi
wait "$server_pid"
server_pid=""
server_shutdown_end_ns=$(date +%s%N)
rg -q 'desktop shutdown drained successfully' "$session_root/browser.stderr"

cp -- "$session_root/server.log" "$evidence_root/server.log"
cp -- "$session_root/browser.stderr" "$evidence_root/browser.stderr"
cp -- "$server_root/omenchatd.log" "$evidence_root/omenchatd.log"
if [[ -f "$browser_root/logs/omenbrowser_rs.jsonl" ]]; then
  cp -- "$browser_root/logs/omenbrowser_rs.jsonl" \
    "$evidence_root/browser-log.jsonl"
fi
{
  printf 'utc=%s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  printf 'build_profile=%s\n' "$build_profile"
  printf 'warmup_seconds=%s\n' "$warmup_seconds"
  printf 'sample_seconds=%s\n' "$sample_seconds"
  printf 'desktop_shutdown_ms=%s\n' \
    "$(( (desktop_shutdown_end_ns - desktop_shutdown_start_ns) / 1000000 ))"
  printf 'server_shutdown_ms=%s\n' \
    "$(( (server_shutdown_end_ns - server_shutdown_start_ns) / 1000000 ))"
  printf 'browser_version=%s\n' "$("$browser_bin" --version)"
  printf 'server_version=%s\n' "$("$server_bin" --version)"
  rustc -Vv | sed 's/^/rustc_/'
} >"$evidence_root/measurement-metadata.txt"
if ((sample_seconds >= 30)); then
  rg -q '^stats: active_links=1 ' "$evidence_root/server.log"
  rg -q '^queues: transport=items:0 bytes:0 .*events=items:0 bytes:0 ' \
    "$evidence_root/server.log"
  rg -q \
    'reticulum-rs live server drained active_links=1 worker_join_timeouts=0 worker_join_failures=0 queues: transport=items:0 bytes:0 .*events=items:0 bytes:0 ' \
    "$evidence_root/omenchatd.log"
fi
if ((sample_seconds > 0)); then
  {
    cat "$evidence_root/process-summary.txt"
    printf 'desktop_shutdown_ms=%s\n' \
      "$(( (desktop_shutdown_end_ns - desktop_shutdown_start_ns) / 1000000 ))"
    printf 'server_shutdown_ms=%s\n' \
      "$(( (server_shutdown_end_ns - server_shutdown_start_ns) / 1000000 ))"
    if ((sample_seconds >= 30)); then
      rg '^stats: active_links=' "$evidence_root/server.log" |
        tail -n 1 |
        sed 's/^/server_/'
      rg '^queues:' "$evidence_root/server.log" |
        tail -n 1 |
        sed 's/^/server_/'
      rg 'reticulum-rs live server drained' "$evidence_root/omenchatd.log" |
        tail -n 1 |
        sed 's/^/server_/'
    fi
  } >"$evidence_root/resource-summary.txt"
fi
printf '%s\n' "$evidence_root"
