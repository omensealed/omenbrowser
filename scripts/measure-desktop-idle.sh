#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

binary="${OMENBROWSER_BINARY:-target/release/omenbrowser_rs}"
warmup_seconds="${WARMUP_SECONDS:-60}"
sample_seconds="${SAMPLE_SECONDS:-600}"
interval_seconds="${INTERVAL_SECONDS:-1}"
headless="${HEADLESS:-auto}"
recurring_app_messages_per_minute="${RECURRING_APP_MESSAGES_PER_MINUTE:-pending}"
recurring_app_messages_source="${RECURRING_APP_MESSAGES_SOURCE:-unmeasured}"
perf_record_seconds="${PERF_RECORD_SECONDS:-0}"
measurement_section="${MEASUREMENT_SECTION:-browser}"
measurement_preset="${MEASUREMENT_PRESET:-normal}"
output="${1:-/tmp/omenbrowser-rs-idle-$(date -u +%Y%m%dT%H%M%SZ)}"

case "$warmup_seconds:$sample_seconds:$interval_seconds" in
  *[!0-9:]*|0:*|*:0:*|*:0) echo "durations must be positive integer seconds" >&2; exit 2 ;;
esac
case "$recurring_app_messages_per_minute" in
  pending|''|*[!0-9]*)
    if [[ "$recurring_app_messages_per_minute" != "pending" ]]; then
      echo "RECURRING_APP_MESSAGES_PER_MINUTE must be a non-negative integer or pending" >&2
      exit 2
    fi
    ;;
esac
case "$perf_record_seconds" in
  ''|*[!0-9]*) echo "PERF_RECORD_SECONDS must be a non-negative integer" >&2; exit 2 ;;
esac
case "$measurement_section" in
  browser|monitoring) ;;
  *) echo "MEASUREMENT_SECTION must be browser or monitoring" >&2; exit 2 ;;
esac
case "$measurement_preset" in
  normal|low-power) ;;
  *) echo "MEASUREMENT_PRESET must be normal or low-power" >&2; exit 2 ;;
esac
if [[ ! -x "$binary" ]]; then
  echo "release binary is missing or not executable: $binary" >&2
  echo "build with: cargo build --release --locked --no-default-features --features desktop-product" >&2
  exit 2
fi
if ! command -v xdotool >/dev/null 2>&1; then
  echo "missing idle measurement tool: xdotool" >&2
  exit 2
fi
case "$headless" in
  auto) if [[ -z "${DISPLAY:-}${WAYLAND_DISPLAY:-}" ]]; then headless=1; else headless=0; fi ;;
  0|1) ;;
  *) echo "HEADLESS must be auto, 0, or 1" >&2; exit 2 ;;
esac
if [[ "$headless" == "0" && -z "${DISPLAY:-}${WAYLAND_DISPLAY:-}" ]]; then
  echo "an interactive X11 or Wayland session is required" >&2
  exit 2
fi
if [[ "$headless" == "1" ]]; then
  for tool in Xvfb i3 xdpyinfo xdotool xprop; do
    if ! command -v "$tool" >/dev/null 2>&1; then
      echo "missing headless idle measurement tool: $tool" >&2
      exit 2
    fi
  done
fi

mkdir -p "$output"
session_root="$(mktemp -d "${TMPDIR:-/tmp}/omenbrowser-rs-idle-root.XXXXXX")"
app_root="$session_root/app"
pid=""
perf_pid=""
xvfb_pid=""
wm_pid=""
cleanup() {
  if [[ -n "$perf_pid" ]]; then kill "$perf_pid" 2>/dev/null || true; fi
  if [[ -n "$pid" ]] && kill -0 "$pid" 2>/dev/null; then
    if command -v xdotool >/dev/null 2>&1; then
      window="$(xdotool search --pid "$pid" 2>/dev/null | head -n 1 || true)"
      if [[ -n "$window" ]]; then xdotool windowclose "$window" 2>/dev/null || true; fi
    fi
    for _ in 1 2 3 4 5; do
      kill -0 "$pid" 2>/dev/null || break
      sleep 1
    done
    kill "$pid" 2>/dev/null || true
    wait "$pid" 2>/dev/null || true
  fi
  if [[ -n "$wm_pid" ]]; then kill "$wm_pid" 2>/dev/null || true; fi
  if [[ -n "$xvfb_pid" ]]; then kill "$xvfb_pid" 2>/dev/null || true; fi
  rm -rf "$session_root"
}
trap cleanup EXIT INT TERM

if [[ "$measurement_section" != "browser" || "$measurement_preset" != "normal" ]]; then
  command -v jq >/dev/null 2>&1 || {
    echo "missing idle measurement fixture tool: jq" >&2
    exit 2
  }
  mkdir -p "$app_root"
  low_power=false
  if [[ "$measurement_preset" == "low-power" ]]; then low_power=true; fi
  printf '{"reticulum_instance_mode":"external","periodic_lxmf_sync":false,"ui":{"low_power_mode":%s,"active_workspace_section":"%s"}}\n' \
    "$low_power" "$measurement_section" >"$app_root/settings.json"
  jq -e \
    --argjson low_power "$low_power" \
    --arg section "$measurement_section" \
    '.reticulum_instance_mode == "external" and
     .periodic_lxmf_sync == false and
     .ui.low_power_mode == $low_power and
     .ui.active_workspace_section == $section' \
    "$app_root/settings.json" >/dev/null
fi

if [[ "$headless" == "1" ]]; then
  display_number=$((420 + ($$ % 200)))
  while [[ -e "/tmp/.X11-unix/X$display_number" ]]; do
    display_number=$((display_number + 1))
  done
  export DISPLAY=":$display_number"
  unset WAYLAND_DISPLAY
  Xvfb "$DISPLAY" -screen 0 1280x800x24 -nolisten tcp \
    >"$output/xvfb.log" 2>&1 &
  xvfb_pid="$!"
  for _ in $(seq 1 100); do
    if xdpyinfo >/dev/null 2>&1; then break; fi
    kill -0 "$xvfb_pid" 2>/dev/null || { echo "Xvfb exited during startup" >&2; exit 1; }
    sleep 0.05
  done
  xdpyinfo >/dev/null 2>&1 || { echo "Xvfb did not become ready" >&2; exit 1; }

  I3SOCK="$session_root/i3.sock" \
    i3 -c "$repo_root/scripts/fixtures/i3-native-test.config" \
    >"$output/i3.log" 2>&1 &
  wm_pid="$!"
  for _ in $(seq 1 100); do
    if xprop -root _NET_SUPPORTING_WM_CHECK 2>/dev/null | grep -q 'window id'; then break; fi
    kill -0 "$wm_pid" 2>/dev/null || { echo "i3 exited during startup" >&2; exit 1; }
    sleep 0.05
  done
  xprop -root _NET_SUPPORTING_WM_CHECK 2>/dev/null | grep -q 'window id' || {
    echo "test window manager did not become ready" >&2
    exit 1
  }
fi

start_ns="$(date +%s%N)"
LIBGL_ALWAYS_SOFTWARE="${LIBGL_ALWAYS_SOFTWARE:-1}" \
  "$binary" --desktop --app-root "$app_root" \
  >"$output/stdout.log" 2>"$output/stderr.log" &
pid="$!"
window_ns=""
window=""
if command -v xdotool >/dev/null 2>&1; then
  for _ in $(seq 1 300); do
    window="$(xdotool search --onlyvisible --pid "$pid" --name '^OMENbrowser_rs$' 2>/dev/null | head -n 1 || true)"
    if [[ -n "$window" ]]; then window_ns="$(date +%s%N)"; break; fi
    kill -0 "$pid" 2>/dev/null || break
    sleep 0.1
  done
fi
kill -0 "$pid" 2>/dev/null || { echo "desktop exited during startup" >&2; exit 1; }

sleep "$warmup_seconds"
count=$((sample_seconds / interval_seconds))
(( count > 0 )) || count=1
printf 'epoch_ms\tcpu_percent\trss_kib\tprivate_dirty_kib\tfds\tvoluntary_ctxt\tnonvoluntary_ctxt\n' >"$output/samples.tsv"

if command -v pidstat >/dev/null 2>&1; then
  pidstat -rud -p "$pid" "$interval_seconds" "$count" >"$output/pidstat.txt" 2>&1 &
fi
if command -v perf >/dev/null 2>&1; then
  perf stat -p "$pid" -e task-clock,context-switches,cpu-migrations,page-faults \
    -- sleep "$sample_seconds" 2>"$output/perf-stat.txt" &
  perf_pid="$!"
fi

prev_proc_ticks="$(awk '{print $14+$15}' "/proc/$pid/stat")"
prev_total_ticks="$(awk 'NR==1 {for(i=2;i<=NF;i++) total+=$i; print total}' /proc/stat)"
cpu_count="$(getconf _NPROCESSORS_ONLN)"
for _ in $(seq 1 "$count"); do
  sleep "$interval_seconds"
  kill -0 "$pid" 2>/dev/null || { echo "desktop exited during sampling" >&2; exit 1; }
  proc_ticks="$(awk '{print $14+$15}' "/proc/$pid/stat")"
  total_ticks="$(awk 'NR==1 {for(i=2;i<=NF;i++) total+=$i; print total}' /proc/stat)"
  cpu="$(awk -v p="$proc_ticks" -v pp="$prev_proc_ticks" -v t="$total_ticks" -v pt="$prev_total_ticks" -v n="$cpu_count" 'BEGIN {if(t>pt) printf "%.3f", 100*(p-pp)*n/(t-pt); else print "0.000"}')"
  prev_proc_ticks="$proc_ticks"
  prev_total_ticks="$total_ticks"
  rss="$(awk '/^VmRSS:/ {print $2}' "/proc/$pid/status")"
  private_dirty="$(awk '/^Private_Dirty:/ {sum += $2} END {print sum + 0}' "/proc/$pid/smaps_rollup")"
  fds="$(find "/proc/$pid/fd" -mindepth 1 -maxdepth 1 2>/dev/null | wc -l)"
  voluntary="$(awk '/^voluntary_ctxt_switches:/ {print $2}' "/proc/$pid/status")"
  nonvoluntary="$(awk '/^nonvoluntary_ctxt_switches:/ {print $2}' "/proc/$pid/status")"
  printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$(( $(date +%s%N) / 1000000 ))" "$cpu" "$rss" "$private_dirty" "$fds" "$voluntary" "$nonvoluntary" >>"$output/samples.tsv"
done
if [[ -n "$perf_pid" ]]; then wait "$perf_pid" || true; perf_pid=""; fi
perf_task_clock_ms=""
if [[ -f "$output/perf-stat.txt" ]]; then
  perf_task_clock_ms="$(awk '$2 == "msec" && $3 ~ /^task-clock/ {print $1; exit}' "$output/perf-stat.txt")"
fi

perf_record_status="not_requested"
if [[ "$perf_record_seconds" -gt 0 ]]; then
  if command -v perf >/dev/null 2>&1; then
    if perf record -q -g -p "$pid" -o "$output/perf.data" \
        -- sleep "$perf_record_seconds" 2>"$output/perf-record.log"; then
      perf report --stdio -i "$output/perf.data" --percent-limit 0.1 \
        >"$output/perf-report.txt" 2>"$output/perf-report.log" || true
      perf_record_status="captured"
    else
      perf_record_status="unavailable"
    fi
  else
    perf_record_status="tool_missing"
  fi
fi

awk -F '\t' -v interval="$interval_seconds" '
  NR > 1 {cpu[++n]=$2; rss[n]=$3; dirty[n]=$4; fd[n]=$5; voluntary[n]=$6; nonvoluntary[n]=$7}
  function sort(a, n, i, j, t) {for(i=1;i<=n;i++) for(j=i+1;j<=n;j++) if(a[j]<a[i]) {t=a[i];a[i]=a[j];a[j]=t}}
  function median(a,n) {sort(a,n); return n%2 ? a[(n+1)/2] : (a[n/2]+a[n/2+1])/2}
  function p95(a,n, i) {sort(a,n); i=int(n*0.95+0.999); if(i<1)i=1; return a[i]}
  END {
    elapsed=(n>1 ? (n-1)*interval : 1)
    printf "samples=%d\n", n
    printf "cpu_percent_median=%.3f\n", median(cpu,n)
    printf "cpu_percent_p95=%.3f\n", p95(cpu,n)
    printf "rss_kib_median=%.0f\n", median(rss,n)
    printf "rss_kib_p95=%.0f\n", p95(rss,n)
    printf "private_dirty_kib_median=%.0f\n", median(dirty,n)
    printf "private_dirty_kib_p95=%.0f\n", p95(dirty,n)
    printf "fds_median=%.0f\n", median(fd,n)
    printf "fds_p95=%.0f\n", p95(fd,n)
    printf "voluntary_context_switches_per_second=%.3f\n", (voluntary[n]-voluntary[1])/elapsed
    printf "nonvoluntary_context_switches_per_second=%.3f\n", (nonvoluntary[n]-nonvoluntary[1])/elapsed
    printf "scheduler_context_switch_proxy_per_minute=%.3f\n", ((voluntary[n]-voluntary[1])+(nonvoluntary[n]-nonvoluntary[1]))*60/elapsed
  }
' "$output/samples.tsv" >"$output/summary.txt"
if [[ -n "$perf_task_clock_ms" ]]; then
  printf 'perf_task_clock_ms=%s\n' "$perf_task_clock_ms" >>"$output/summary.txt"
fi

close_start_ns="$(date +%s%N)"
if [[ -n "$window" ]] && command -v xdotool >/dev/null 2>&1; then
  xdotool windowactivate --sync "$window" 2>/dev/null || true
  xdotool key --window "$window" alt+F4 2>/dev/null \
    || xdotool windowclose "$window" 2>/dev/null \
    || true
fi
for _ in $(seq 1 160); do
  if ! kill -0 "$pid" 2>/dev/null; then break; fi
  sleep 0.05
done
if kill -0 "$pid" 2>/dev/null; then
  echo "desktop did not return normally within 8 seconds after measurement" >&2
  exit 1
fi
set +e
wait "$pid"
app_status="$?"
set -e
pid=""
close_end_ns="$(date +%s%N)"
if [[ "$app_status" -ne 0 ]]; then
  echo "desktop returned non-zero status $app_status after measurement" >&2
  exit 1
fi

if [[ "$measurement_section" != "browser" || "$measurement_preset" != "normal" ]]; then
  expected_low_power=false
  if [[ "$measurement_preset" == "low-power" ]]; then expected_low_power=true; fi
  jq -e \
    --argjson low_power "$expected_low_power" \
    --arg section "$measurement_section" \
    '.reticulum_instance_mode == "external" and
     .ui.low_power_mode == $low_power and
     .ui.active_workspace_section == $section' \
    "$app_root/settings.json" >/dev/null || {
      echo "desktop did not preserve the isolated measurement policy" >&2
      exit 1
    }
fi

{
  printf 'utc=%s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  printf 'binary=%s\n' "$(realpath "$binary")"
  printf 'binary_bytes=%s\n' "$(stat -c %s "$binary")"
  printf 'binary_sha256=%s\n' "$(sha256sum "$binary" | awk '{print $1}')"
  printf 'warmup_seconds=%s\nsample_seconds=%s\ninterval_seconds=%s\n' "$warmup_seconds" "$sample_seconds" "$interval_seconds"
  printf 'headless=%s\n' "$headless"
  printf 'measurement_section=%s\nmeasurement_preset=%s\n' "$measurement_section" "$measurement_preset"
  printf 'recurring_app_messages_per_minute=%s\n' "$recurring_app_messages_per_minute"
  printf 'recurring_app_messages_source=%s\n' "$recurring_app_messages_source"
  printf 'perf_record_seconds=%s\nperf_record_status=%s\n' "$perf_record_seconds" "$perf_record_status"
  printf 'display=%s\nwayland_display=%s\n' "${DISPLAY:-}" "${WAYLAND_DISPLAY:-}"
  if [[ -n "$window_ns" ]]; then printf 'startup_to_window_ms=%s\n' "$(( (window_ns - start_ns) / 1000000 ))"; else printf 'startup_to_window_ms=pending\n'; fi
  printf 'close_latency_ms=%s\nnormal_process_return=yes\n' "$(( (close_end_ns - close_start_ns) / 1000000 ))"
  printf 'version=%s\n' "$($binary --version)"
  rustc -Vv | sed 's/^/rustc_/'
} >"$output/metadata.txt"

cat "$output/metadata.txt" "$output/summary.txt"
echo "raw results: $output"
