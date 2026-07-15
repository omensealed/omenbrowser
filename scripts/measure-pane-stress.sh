#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

binary="${OMENBROWSER_BINARY:-target/release/omenbrowser_rs}"
cycles="${CYCLES:-3}"
settle_seconds="${SETTLE_SECONDS:-5}"
cpu_sample_seconds="${CPU_SAMPLE_SECONDS:-2}"
output="${1:-/tmp/omenbrowser-pane-stress-results-$(date -u +%Y%m%dT%H%M%SZ)}"

case "$cycles:$settle_seconds:$cpu_sample_seconds" in
  *[!0-9:]*|0:*|*:0:*|*:0) echo "cycle and duration values must be positive integers" >&2; exit 2 ;;
esac
for tool in Xvfb i3 xdpyinfo xdotool xprop jq rg; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    echo "missing pane-stress measurement tool: $tool" >&2
    exit 2
  fi
done
if [[ ! -x "$binary" ]]; then
  echo "product binary is missing or not executable: $binary" >&2
  echo "build with: cargo build --release --locked --no-default-features --features desktop-product --bin omenbrowser_rs" >&2
  exit 2
fi
version="$($binary --version)"
grep -q 'desktop-product:on' <<<"$version"
grep -q 'mock-runtime:off' <<<"$version"

mkdir -p "$output"
session_root="$(mktemp -d "${TMPDIR:-/tmp}/omenbrowser-pane-stress.XXXXXX")"
app_root="$session_root/app"
xvfb_pid=""
wm_pid=""
app_pid=""
window=""

cleanup() {
  if [[ -n "$app_pid" ]] && kill -0 "$app_pid" 2>/dev/null; then
    if [[ -n "$window" ]]; then
      xdotool key --window "$window" alt+F4 2>/dev/null || true
    fi
    for _ in $(seq 1 40); do kill -0 "$app_pid" 2>/dev/null || break; sleep 0.05; done
    kill "$app_pid" 2>/dev/null || true
    wait "$app_pid" 2>/dev/null || true
  fi
  if [[ -n "$wm_pid" ]]; then kill "$wm_pid" 2>/dev/null || true; fi
  if [[ -n "$xvfb_pid" ]]; then kill "$xvfb_pid" 2>/dev/null || true; fi
  rm -rf "$session_root"
}
trap cleanup EXIT INT TERM

echo "== Generate isolated production-format fixture =="
OMENBROWSER_PANE_STRESS_ROOT="$app_root" \
  cargo test --release --locked --no-default-features --features desktop-product \
  --test pane_stress_fixture write_pane_stress_fixture_to_explicit_isolated_root \
  -- --exact --ignored >"$output/fixture-generate.log" 2>&1

jq -e '
  (.browser_tabs | length) == 20 and
  (.conversation_tabs | length) == 20 and
  (.ui.desktop_workspace_panes | length) == 50 and
  ([.ui.desktop_workspace_panes[] | select(.kind == "browser")] | length) == 20 and
  ([.ui.desktop_workspace_panes[] | select(.kind == "conversation")] | length) == 20 and
  ([.ui.desktop_workspace_panes[] | select(.kind == "omen_chat")] | length) == 10 and
  .ui.desktop_workspace_layout != null
' "$app_root/settings.json" >/dev/null

display_number=$((620 + ($$ % 200)))
while [[ -e "/tmp/.X11-unix/X$display_number" ]]; do display_number=$((display_number + 1)); done
export DISPLAY=":$display_number"
unset WAYLAND_DISPLAY
Xvfb "$DISPLAY" -screen 0 1600x1000x24 -nolisten tcp >"$output/xvfb.log" 2>&1 &
xvfb_pid="$!"
for _ in $(seq 1 100); do
  if xdpyinfo >/dev/null 2>&1; then break; fi
  kill -0 "$xvfb_pid" 2>/dev/null || { echo "Xvfb exited during startup" >&2; exit 1; }
  sleep 0.05
done
xdpyinfo >/dev/null 2>&1 || { echo "Xvfb did not become ready" >&2; exit 1; }

I3SOCK="$session_root/i3.sock" i3 -c "$repo_root/scripts/fixtures/i3-native-test.config" \
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

printf 'cycle\tstartup_to_window_ms\tcpu_percent\trss_kib\tprivate_dirty_kib\tfds\tclose_latency_ms\n' \
  >"$output/cycles.tsv"

for cycle in $(seq 1 "$cycles"); do
  echo "== Pane-stress cycle $cycle/$cycles =="
  cycle_log="$output/cycle-$cycle.stderr.log"
  start_ns="$(date +%s%N)"
  LIBGL_ALWAYS_SOFTWARE=1 RUST_LOG=omenbrowser_rs=info \
    "$binary" --desktop --app-root "$app_root" \
    >"$output/cycle-$cycle.stdout.log" 2>"$cycle_log" &
  app_pid="$!"
  window=""
  for _ in $(seq 1 300); do
    window="$(xdotool search --onlyvisible --pid "$app_pid" --name '^OMENbrowser_rs$' 2>/dev/null | head -n 1 || true)"
    if [[ -n "$window" ]]; then break; fi
    kill -0 "$app_pid" 2>/dev/null || {
      echo "desktop exited during pane-stress startup cycle $cycle" >&2
      sed -n '1,160p' "$cycle_log" >&2
      exit 1
    }
    sleep 0.05
  done
  [[ -n "$window" ]] || { echo "desktop window did not appear in cycle $cycle" >&2; exit 1; }
  window_ns="$(date +%s%N)"

  for field in 'workspace_panes=50' 'browser_tabs=20' 'conversations=20' 'omenchat_sessions=10'; do
    grep -q "$field" "$cycle_log" || {
      echo "restored workspace trace missing $field in cycle $cycle" >&2
      sed -n '1,160p' "$cycle_log" >&2
      exit 1
    }
  done

  sleep "$settle_seconds"
  proc_ticks_before="$(awk '{print $14+$15}' "/proc/$app_pid/stat")"
  total_ticks_before="$(awk 'NR==1 {for(i=2;i<=NF;i++) total+=$i; print total}' /proc/stat)"
  sleep "$cpu_sample_seconds"
  proc_ticks_after="$(awk '{print $14+$15}' "/proc/$app_pid/stat")"
  total_ticks_after="$(awk 'NR==1 {for(i=2;i<=NF;i++) total+=$i; print total}' /proc/stat)"
  cpu_count="$(getconf _NPROCESSORS_ONLN)"
  cpu="$(awk -v p="$proc_ticks_after" -v pp="$proc_ticks_before" -v t="$total_ticks_after" -v pt="$total_ticks_before" -v n="$cpu_count" 'BEGIN {if(t>pt) printf "%.3f",100*(p-pp)*n/(t-pt); else print "0.000"}')"
  rss="$(awk '/^VmRSS:/ {print $2}' "/proc/$app_pid/status")"
  dirty="$(awk '/^Private_Dirty:/ {sum += $2} END {print sum + 0}' "/proc/$app_pid/smaps_rollup")"
  fds="$(find "/proc/$app_pid/fd" -mindepth 1 -maxdepth 1 2>/dev/null | wc -l)"

  close_ns="$(date +%s%N)"
  xdotool windowactivate --sync "$window" 2>/dev/null || true
  xdotool key --window "$window" alt+F4
  for _ in $(seq 1 160); do kill -0 "$app_pid" 2>/dev/null || break; sleep 0.05; done
  if kill -0 "$app_pid" 2>/dev/null; then
    echo "desktop did not close normally in cycle $cycle" >&2
    exit 1
  fi
  set +e
  wait "$app_pid"
  status="$?"
  set -e
  app_pid=""
  window=""
  end_ns="$(date +%s%N)"
  [[ "$status" -eq 0 ]] || { echo "desktop cycle $cycle returned $status" >&2; exit 1; }
  grep -q 'desktop shutdown drained successfully' "$cycle_log" || {
    echo "shutdown drain trace missing in cycle $cycle" >&2
    exit 1
  }
  jq -e '(.ui.desktop_workspace_panes | length) == 50' "$app_root/settings.json" >/dev/null
  printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
    "$cycle" "$(( (window_ns-start_ns)/1000000 ))" "$cpu" "$rss" "$dirty" "$fds" \
    "$(( (end_ns-close_ns)/1000000 ))" >>"$output/cycles.tsv"
done

OMENBROWSER_PANE_STRESS_ROOT="$app_root" \
  cargo test --release --locked --no-default-features --features desktop-product \
  --test pane_stress_fixture verify_pane_stress_fixture_at_explicit_isolated_root \
  -- --exact --ignored >"$output/fixture-verify.log" 2>&1

awk -F '\t' '
  NR > 1 {startup[++n]=$2; cpu[n]=$3; rss[n]=$4; dirty[n]=$5; fds[n]=$6; shutdown[n]=$7}
  function sort(a,n,i,j,t){for(i=1;i<=n;i++)for(j=i+1;j<=n;j++)if(a[j]<a[i]){t=a[i];a[i]=a[j];a[j]=t}}
  function median(a,n){sort(a,n);return n%2?a[(n+1)/2]:(a[n/2]+a[n/2+1])/2}
  function p95(a,n,i){sort(a,n);i=int(n*.95+.999);if(i<1)i=1;return a[i]}
  END {
    printf "cycles=%d\n",n
    printf "startup_to_window_ms_median=%.0f\nstartup_to_window_ms_p95=%.0f\n",median(startup,n),p95(startup,n)
    printf "cpu_percent_median=%.3f\ncpu_percent_p95=%.3f\n",median(cpu,n),p95(cpu,n)
    printf "rss_kib_median=%.0f\nrss_kib_p95=%.0f\n",median(rss,n),p95(rss,n)
    printf "private_dirty_kib_median=%.0f\nprivate_dirty_kib_p95=%.0f\n",median(dirty,n),p95(dirty,n)
    printf "fds_median=%.0f\nfds_p95=%.0f\n",median(fds,n),p95(fds,n)
    printf "close_latency_ms_median=%.0f\nclose_latency_ms_p95=%.0f\n",median(shutdown,n),p95(shutdown,n)
  }
' "$output/cycles.tsv" >"$output/summary.txt"

{
  printf 'utc=%s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  printf 'binary=%s\nbinary_bytes=%s\n' "$(realpath "$binary")" "$(stat -c %s "$binary")"
  printf 'version=%s\ncycles=%s\nsettle_seconds=%s\ncpu_sample_seconds=%s\n' \
    "$version" "$cycles" "$settle_seconds" "$cpu_sample_seconds"
  printf 'fixture_browser_panes=20\nfixture_conversation_panes=20\nfixture_omenchat_panes=10\nfixture_total_panes=50\n'
  rustc -Vv | sed 's/^/rustc_/'
} >"$output/metadata.txt"

cat "$output/metadata.txt" "$output/summary.txt"
echo "raw results: $output"
