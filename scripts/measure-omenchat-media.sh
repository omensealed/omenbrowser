#!/usr/bin/env bash
set -euo pipefail

binary="${OMENBROWSER_BINARY:-target/release/omenbrowser_rs}"
sample_seconds="${SAMPLE_SECONDS:-120}"
interval_seconds="${INTERVAL_SECONDS:-1}"
output="${1:-/tmp/omenbrowser-rs-media-$(date -u +%Y%m%dT%H%M%SZ)}"

case "$sample_seconds:$interval_seconds" in
  *[!0-9:]*|0:*|*:0) echo "durations must be positive integer seconds" >&2; exit 2 ;;
esac
if [[ ! -x "$binary" ]]; then
  echo "release binary is missing or not executable: $binary" >&2
  echo "build with: cargo build --release --locked --no-default-features --features desktop-product" >&2
  exit 2
fi
if [[ -z "${DISPLAY:-}${WAYLAND_DISPLAY:-}" ]]; then
  echo "an interactive X11 or Wayland session is required" >&2
  exit 2
fi
if [[ ! -t 0 ]]; then
  echo "this harness requires an operator on stdin to establish each visible/hidden phase" >&2
  exit 2
fi

mkdir -p "$output"
app_root="$(mktemp -d "${TMPDIR:-/tmp}/omenbrowser-rs-media-root.XXXXXX")"
fixture_dir="$app_root/measurement-fixtures"
mkdir -p "$fixture_dir"
fixture="$fixture_dir/animated-two-frame-1x1.gif"
printf '%s' 'R0lGODlhAQABAIAAAAAAAP///ywAAAAAAQABAAACAUwALAAAAAABAAEAAAIBTAA7' | base64 -d >"$fixture"

pid=""
cleanup() {
  if [[ -n "$pid" ]] && kill -0 "$pid" 2>/dev/null; then
    kill "$pid" 2>/dev/null || true
    wait "$pid" 2>/dev/null || true
  fi
  rm -rf "$app_root"
}
trap cleanup EXIT INT TERM

"$binary" --desktop --app-root "$app_root" >"$output/stdout.log" 2>"$output/stderr.log" &
pid="$!"
for _ in $(seq 1 300); do
  kill -0 "$pid" 2>/dev/null || { echo "desktop exited during startup" >&2; exit 1; }
  if command -v xdotool >/dev/null 2>&1 && xdotool search --pid "$pid" >/dev/null 2>&1; then break; fi
  sleep 0.1
done

sample_phase() {
  local phase="$1" count prev_proc_ticks prev_total_ticks cpu_count
  count=$((sample_seconds / interval_seconds)); (( count > 0 )) || count=1
  printf 'epoch_ms\tcpu_percent\trss_kib\tprivate_dirty_kib\tfds\tvoluntary_ctxt\tnonvoluntary_ctxt\n' >"$output/$phase.tsv"
  prev_proc_ticks="$(awk '{print $14+$15}' "/proc/$pid/stat")"
  prev_total_ticks="$(awk 'NR==1 {for(i=2;i<=NF;i++) total+=$i; print total}' /proc/stat)"
  cpu_count="$(getconf _NPROCESSORS_ONLN)"
  for _ in $(seq 1 "$count"); do
    sleep "$interval_seconds"
    kill -0 "$pid" 2>/dev/null || { echo "desktop exited during $phase" >&2; exit 1; }
    local proc_ticks total_ticks cpu rss dirty fds voluntary nonvoluntary
    proc_ticks="$(awk '{print $14+$15}' "/proc/$pid/stat")"
    total_ticks="$(awk 'NR==1 {for(i=2;i<=NF;i++) total+=$i; print total}' /proc/stat)"
    cpu="$(awk -v p="$proc_ticks" -v pp="$prev_proc_ticks" -v t="$total_ticks" -v pt="$prev_total_ticks" -v n="$cpu_count" 'BEGIN {if(t>pt) printf "%.3f",100*(p-pp)*n/(t-pt); else print "0.000"}')"
    prev_proc_ticks="$proc_ticks"; prev_total_ticks="$total_ticks"
    rss="$(awk '/^VmRSS:/ {print $2}' "/proc/$pid/status")"
    dirty="$(awk '/^Private_Dirty:/ {sum += $2} END {print sum + 0}' "/proc/$pid/smaps_rollup")"
    fds="$(find "/proc/$pid/fd" -mindepth 1 -maxdepth 1 2>/dev/null | wc -l)"
    voluntary="$(awk '/^voluntary_ctxt_switches:/ {print $2}' "/proc/$pid/status")"
    nonvoluntary="$(awk '/^nonvoluntary_ctxt_switches:/ {print $2}' "/proc/$pid/status")"
    printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$(( $(date +%s%N) / 1000000 ))" "$cpu" "$rss" "$dirty" "$fds" "$voluntary" "$nonvoluntary" >>"$output/$phase.tsv"
  done
  awk -F '\t' -v phase="$phase" '
    NR>1 {cpu[++n]=$2; rss[n]=$3; dirty[n]=$4; fd[n]=$5; v[n]=$6; nv[n]=$7}
    function sort(a,n,i,j,t){for(i=1;i<=n;i++)for(j=i+1;j<=n;j++)if(a[j]<a[i]){t=a[i];a[i]=a[j];a[j]=t}}
    function median(a,n){sort(a,n);return n%2?a[(n+1)/2]:(a[n/2]+a[n/2+1])/2}
    function p95(a,n,i){sort(a,n);i=int(n*.95+.999);if(i<1)i=1;return a[i]}
    END {printf "phase=%s samples=%d cpu_median=%.3f cpu_p95=%.3f rss_kib_median=%.0f rss_kib_p95=%.0f private_dirty_kib_p95=%.0f fds_p95=%.0f voluntary_delta=%d nonvoluntary_delta=%d\n",phase,n,median(cpu,n),p95(cpu,n),median(rss,n),p95(rss,n),p95(dirty,n),p95(fd,n),v[n]-v[1],nv[n]-nv[1]}
  ' "$output/$phase.tsv" | tee -a "$output/summary.txt"
}

cat >"$output/gpu-observation.txt" <<EOF
GPU capture is hardware/session specific; run one applicable command beside each phase:
Intel: sudo-free when permitted: intel_gpu_top -J -s 1000 -o <phase>-intel-gpu.json
NVIDIA: nvidia-smi dmon -s u -d 1 -o DT > <phase>-nvidia-dmon.txt
AMD: radeontop -d <phase>-radeontop.txt -l $sample_seconds
If none is installed or access is denied, record tool/access as pending; never record zero by assumption.
EOF

printf 'Fixture: %s\n' "$fixture"
printf 'Open OMENchat in mock/isolated mode, upload or link this fixture, and leave its animation visible. Press Enter when stable.\n'
read -r
sample_phase visible
printf 'Maximize a sibling pane so the GIF pane is hidden. Press Enter when stable.\n'; read -r
sample_phase maximized_hidden
printf 'Switch to Settings or another top-level section. Press Enter when stable.\n'; read -r
sample_phase section_hidden
printf 'Close the media/OMENchat pane and wait for cleanup. Press Enter when stable.\n'; read -r
sample_phase closed

{
  printf 'utc=%s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  printf 'binary=%s\nbinary_bytes=%s\n' "$(realpath "$binary")" "$(stat -c %s "$binary")"
  printf 'fixture_sha256=%s\nsample_seconds=%s\ninterval_seconds=%s\n' "$(sha256sum "$fixture" | awk '{print $1}')" "$sample_seconds" "$interval_seconds"
  printf 'display=%s\nwayland_display=%s\n' "${DISPLAY:-}" "${WAYLAND_DISPLAY:-}"
  rustc -Vv | sed 's/^/rustc_/'
} >"$output/metadata.txt"

cat "$output/metadata.txt" "$output/summary.txt"
echo "running isolated release-mode decoder latency measurement"
cargo test --release --locked --no-default-features --features desktop-product \
  desktop::omenchat_media_tasks::tests::measure_omenchat_gif_decode_latency \
  -- --exact --ignored --nocapture 2>&1 | tee "$output/decode-latency.txt"
echo "GPU procedure: $output/gpu-observation.txt"
echo "raw results: $output"
