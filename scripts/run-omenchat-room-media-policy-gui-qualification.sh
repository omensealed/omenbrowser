#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
readonly repo_root
evidence_root="${TMPDIR:-/tmp}/omenchat-room-media-policy-gui-evidence"
sample_seconds="${OMENCHAT_ROOM_MEDIA_POLICY_SAMPLE_SECONDS:-0}"
warmup_seconds="${OMENCHAT_ROOM_MEDIA_POLICY_WARMUP_SECONDS:-10}"
while (($#)); do
  case "$1" in
    --evidence)
      [[ $# -ge 2 ]] || {
        echo "--evidence requires a directory" >&2
        exit 2
      }
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
  echo "OMENCHAT_ROOM_MEDIA_POLICY_SAMPLE_SECONDS must be an integer from 0 through 300" >&2
  exit 2
fi
if [[ ! "$warmup_seconds" =~ ^[0-9]+$ ]] ||
    ((warmup_seconds > 300)); then
  echo "OMENCHAT_ROOM_MEDIA_POLICY_WARMUP_SECONDS must be an integer from 0 through 300" >&2
  exit 2
fi
if ((sample_seconds > 0)) &&
    [[ "$(uname -s)" != "Linux" || ! -r /proc/self/status ]]; then
  echo "room media-policy process measurement requires Linux /proc" >&2
  exit 2
fi

for tool in Xvfb i3 xdpyinfo xdotool xprop import jq rg python3 truncate find; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    echo "missing room media-policy GUI qualification tool: $tool" >&2
    exit 2
  fi
done
if ((sample_seconds > 0)); then
  for tool in awk getconf; do
    if ! command -v "$tool" >/dev/null 2>&1; then
      echo "missing room media-policy process measurement tool: $tool" >&2
      exit 2
    fi
  done
fi

session_root=$(mktemp -d "${TMPDIR:-/tmp}/omenchat-room-media-policy-gui.XXXXXX")
xvfb_pid=""
wm_pid=""
app_pid=""
server_pid=""

stop_pid() {
  local pid=$1
  if [[ -n "$pid" ]] && kill -0 "$pid" 2>/dev/null; then
    kill "$pid" 2>/dev/null || true
    wait "$pid" 2>/dev/null || true
  fi
}

cleanup() {
  stop_pid "$app_pid"
  stop_pid "$server_pid"
  stop_pid "$wm_pid"
  stop_pid "$xvfb_pid"
  rm -rf -- "$session_root"
}

report_error() {
  local status=$1
  local line=$2
  mkdir -p -- "$evidence_root"
  if [[ -d "$session_root" ]]; then
    find "$session_root" -maxdepth 3 -type f \
      \( -name '*.log' -o -name '*.stderr' -o -name '*.stdout' \) \
      -exec cp --parents -- '{}' "$evidence_root" \; 2>/dev/null || true
  fi
  echo "room media-policy GUI qualification failed at line $line (status $status)" >&2
  echo "failure evidence: $evidence_root" >&2
}

trap cleanup EXIT INT TERM
trap 'report_error "$?" "$LINENO"' ERR

rm -rf -- "$evidence_root"
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
  --features desktop-product,omenchat-room-media-policy-qualification \
  --bin omenbrowser_rs
cargo build --quiet --locked --manifest-path "$repo_root/src/server/Cargo.toml" \
  "${build_profile_args[@]}" \
  --no-default-features \
  --features server-headless,omenchat-room-media-policy-qualification \
  --bin omenchatd

browser_bin="${CARGO_TARGET_DIR:-$repo_root/target}/$build_profile/omenbrowser_rs"
server_bin="${CARGO_TARGET_DIR:-$repo_root/src/server/target}/$build_profile/omenchatd"

under_file="$session_root/under-limit.bin"
over_file="$session_root/over-limit.bin"
truncate -s 65536 "$under_file"
truncate -s 300000 "$over_file"

display_number=$((570 + ($$ % 120)))
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

wait_for_server() {
  local log=$1
  for _ in {1..200}; do
    if rg -q 'live server ready' "$log"; then
      return
    fi
    if ! kill -0 "$server_pid" 2>/dev/null; then
      echo "qualification omenchatd exited before becoming ready" >&2
      return 1
    fi
    sleep 0.1
  done
  echo "qualification omenchatd did not become ready" >&2
  return 1
}

wait_for_exit() {
  local pid=$1
  local label=$2
  for _ in {1..160}; do
    if ! kill -0 "$pid" 2>/dev/null; then
      wait "$pid"
      return
    fi
    sleep 0.05
  done
  echo "$label did not stop within 8 seconds" >&2
  return 1
}

database_upload_count() {
  local database=$1
  python3 - "$database" <<'PY'
import sqlite3
import sys

connection = sqlite3.connect(f"file:{sys.argv[1]}?mode=ro", uri=True)
print(connection.execute("SELECT COUNT(*) FROM upload_files").fetchone()[0])
PY
}

database_member_count() {
  local database=$1
  python3 - "$database" <<'PY'
import sqlite3
import sys

connection = sqlite3.connect(f"file:{sys.argv[1]}?mode=ro", uri=True)
print(connection.execute("SELECT COUNT(*) FROM room_members").fetchone()[0])
PY
}

write_database_observation() {
  local database=$1
  local destination=$2
  python3 - "$database" "$destination" <<'PY'
import json
import pathlib
import sqlite3
import sys

connection = sqlite3.connect(f"file:{sys.argv[1]}?mode=ro", uri=True)
rows = connection.execute(
    "SELECT resource_id, byte_len, path FROM upload_files ORDER BY resource_id"
).fetchall()
print(json.dumps({
    "destination": sys.argv[2],
    "upload_count": len(rows),
    "upload_bytes": sum(row[1] for row in rows),
    "uploads": [{
        "resource_id": row[0],
        "byte_len": row[1],
        "path_exists": pathlib.Path(row[2]).is_file(),
    } for row in rows],
}, sort_keys=True))
PY
}

sample_processes() {
  local phase=$1
  local seconds=$2
  local destination=$3
  local browser_ticks
  local server_ticks
  local total_ticks
  local cpu_count
  local sample
  local next_browser_ticks
  local next_server_ticks
  local next_total_ticks
  local browser_cpu
  local server_cpu
  local browser_rss
  local server_rss
  local browser_dirty
  local server_dirty
  local browser_threads
  local server_threads
  local browser_fds
  local server_fds

  browser_ticks=$(awk '{print $14+$15}' "/proc/$app_pid/stat")
  server_ticks=$(awk '{print $14+$15}' "/proc/$server_pid/stat")
  total_ticks=$(awk 'NR==1 {for(i=2;i<=NF;i++) total+=$i; print total}' /proc/stat)
  cpu_count=$(getconf _NPROCESSORS_ONLN)
  for ((sample = 0; sample < seconds; sample++)); do
    sleep 1
    kill -0 "$app_pid" 2>/dev/null ||
      { echo "desktop exited during process measurement" >&2; return 1; }
    kill -0 "$server_pid" 2>/dev/null ||
      { echo "omenchatd exited during process measurement" >&2; return 1; }
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
    printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
      "$phase" "$(( $(date +%s%N) / 1000000 ))" \
      "$browser_cpu" "$server_cpu" "$browser_rss" "$server_rss" \
      "$browser_dirty" "$server_dirty" "$browser_threads" "$server_threads" \
      "$browser_fds" "$server_fds" >>"$destination"
  done
}

write_process_summary() {
  local samples=$1
  local destination=$2
  python3 - "$samples" "$destination" <<'PY'
import math
import pathlib
import statistics
import sys

source = pathlib.Path(sys.argv[1])
destination = pathlib.Path(sys.argv[2])
columns = (
    "browser_cpu_percent",
    "server_cpu_percent",
    "browser_rss_kib",
    "server_rss_kib",
    "browser_private_dirty_kib",
    "server_private_dirty_kib",
    "browser_threads",
    "server_threads",
    "browser_fds",
    "server_fds",
)
rows = []
with source.open(encoding="utf-8") as stream:
    header = stream.readline().rstrip("\n").split("\t")
    for line in stream:
        values = line.rstrip("\n").split("\t")
        rows.append(dict(zip(header, values, strict=True)))

def percentile(values, fraction):
    ordered = sorted(values)
    return ordered[max(0, math.ceil(len(ordered) * fraction) - 1)]

lines = []
for phase in ("before_upload", "after_upload"):
    selected = [row for row in rows if row["phase"] == phase]
    lines.append(f"{phase}_samples={len(selected)}")
    for column in columns:
        values = [float(row[column]) for row in selected]
        lines.append(f"{phase}_{column}_median={statistics.median(values):.3f}")
        lines.append(f"{phase}_{column}_p95={percentile(values, 0.95):.3f}")
before = [row for row in rows if row["phase"] == "before_upload"]
after = [row for row in rows if row["phase"] == "after_upload"]
for process in ("browser", "server"):
    initial = float(before[-1][f"{process}_rss_kib"])
    final = float(after[-1][f"{process}_rss_kib"])
    lines.append(f"{process}_rss_kib_post_minus_pre={final - initial:.0f}")
destination.write_text("\n".join(lines) + "\n", encoding="utf-8")
PY
}

run_case() {
  local label=$1
  local policy=$2
  local upload_path=$3
  local expected=$4
  local case_root="$session_root/$label"
  local case_evidence="$evidence_root/$label"
  local browser_root="$case_root/browser"
  local server_root="$case_root/server"
  local port
  local destination
  local identity_hash
  local identity_storage
  local window=""
  local upload_count
  local desktop_shutdown_start_ns=0
  local desktop_shutdown_end_ns=0
  local server_shutdown_start_ns=0
  local server_shutdown_end_ns=0
  local process_samples="$case_evidence/process-samples.tsv"

  mkdir -p -- "$case_root" "$case_evidence"
  port=$(python3 -c \
    'import socket; s=socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1]); s.close()')

  "$server_bin" init --home "$server_root" --tcp-server "127.0.0.1:$port" \
    >"$case_root/server-init.log"
  "$server_bin" run --home "$server_root" --tcp-server "127.0.0.1:$port" \
    >"$case_root/server-bootstrap.log" 2>&1 &
  server_pid=$!
  wait_for_server "$case_root/server-bootstrap.log"
  kill "$server_pid"
  wait_for_exit "$server_pid" "bootstrap omenchatd"
  server_pid=""

  "$server_bin" rooms set-upload-policy 1 "$policy" \
    --confirm --home "$server_root" >"$case_evidence/policy.txt"

  "$server_bin" run --home "$server_root" --tcp-server "127.0.0.1:$port" \
    >"$case_root/server.log" 2>&1 &
  server_pid=$!
  wait_for_server "$case_root/server.log"
  "$server_bin" status --home "$server_root" >"$case_evidence/server-status.txt"
  destination=$(
    sed -n 's/^client uri: omenchat:\/\/\([0-9a-fA-F]\+\)$/\1/p' \
      "$case_evidence/server-status.txt" |
      head -n 1
  )
  if [[ ! "$destination" =~ ^[0-9a-fA-F]{32}$ ]]; then
    echo "could not read isolated OMENchat destination for $label" >&2
    return 1
  fi

  "$browser_bin" \
    --generate-native-identity "Room Media GUI $label" \
    --app-root "$browser_root" \
    --stdout >"$case_evidence/browser-identity.json" \
    2>"$case_root/browser-identity.stderr"
  identity_hash=$(jq -r '.identity.hash_hex' "$case_evidence/browser-identity.json")
  identity_storage="$browser_root/identity_storage/default_identity-${identity_hash:0:16}"
  mkdir -p -- "$identity_storage"
  jq -n --argjson port "$port" '{
    profiles: [{
      profile_id: "room-media-gui-loopback",
      name: "Room Media GUI Loopback",
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

  DISPLAY="$test_display" LIBGL_ALWAYS_SOFTWARE=1 \
  OMENBROWSER_QUALIFICATION_OMENCHAT_TARGET="omenchat://$destination" \
  OMENBROWSER_QUALIFICATION_OMENCHAT_UPLOAD_PATH="$upload_path" \
  RUST_LOG=omenbrowser_rs=info \
    "$browser_bin" --desktop --app-root "$browser_root" \
    >"$case_root/browser.stdout" 2>"$case_root/browser.stderr" &
  app_pid=$!

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
      echo "desktop exited before opening $label window" >&2
      return 1
    fi
    sleep 0.05
  done
  if [[ -z "$window" ]]; then
    echo "desktop $label window did not become visible" >&2
    return 1
  fi

  for _ in {1..450}; do
    if [[ -f "$server_root/omenchat.sqlite" ]] &&
        [[ $(database_member_count "$server_root/omenchat.sqlite" 2>/dev/null || printf '0') -ge 1 ]]; then
      break
    fi
    if ! kill -0 "$app_pid" 2>/dev/null; then
      echo "desktop exited before joining $label room" >&2
      return 1
    fi
    sleep 0.1
  done
  if [[ $(database_member_count "$server_root/omenchat.sqlite") -lt 1 ]]; then
    echo "desktop did not join the $label room" >&2
    return 1
  fi
  sleep 1

  DISPLAY="$test_display" import -window "$window" "$case_evidence/before-attach.png"
  if [[ "$label" == "under-limit" ]] && ((sample_seconds > 0)); then
    sleep "$warmup_seconds"
    printf 'phase\tepoch_ms\tbrowser_cpu_percent\tserver_cpu_percent\tbrowser_rss_kib\tserver_rss_kib\tbrowser_private_dirty_kib\tserver_private_dirty_kib\tbrowser_threads\tserver_threads\tbrowser_fds\tserver_fds\n' \
      >"$process_samples"
    sample_processes before_upload 5 "$process_samples"
  fi
  DISPLAY="$test_display" xdotool windowactivate --sync "$window"
  DISPLAY="$test_display" xdotool mousemove --window "$window" 572 765 click 1

  if [[ "$expected" == "accepted" ]]; then
    for _ in {1..200}; do
      upload_count=$(database_upload_count "$server_root/omenchat.sqlite")
      if [[ "$upload_count" -eq 1 ]]; then
        break
      fi
      sleep 0.1
    done
    if [[ $(database_upload_count "$server_root/omenchat.sqlite") -ne 1 ]]; then
      echo "accepted GUI upload did not reach durable storage" >&2
      return 1
    fi
    if [[ "$label" == "under-limit" ]] && ((sample_seconds > 0)); then
      sample_processes after_upload "$sample_seconds" "$process_samples"
      write_process_summary "$process_samples" "$case_evidence/process-summary.txt"
    fi
  else
    sleep 2
    if [[ $(database_upload_count "$server_root/omenchat.sqlite") -ne 0 ]]; then
      echo "rejected GUI upload reached durable storage" >&2
      return 1
    fi
  fi
  DISPLAY="$test_display" import -window "$window" "$case_evidence/after-attach.png"

  write_database_observation "$server_root/omenchat.sqlite" "$destination" \
    >"$case_evidence/database-observation.json"
  if [[ "$expected" == "accepted" ]]; then
    jq -e \
      '.upload_count == 1 and .upload_bytes == 65536 and .uploads[0].path_exists == true' \
      "$case_evidence/database-observation.json" >/dev/null
  else
    jq -e '.upload_count == 0 and .upload_bytes == 0 and .uploads == []' \
      "$case_evidence/database-observation.json" >/dev/null
    if find "$server_root/uploads" -type f -print -quit 2>/dev/null | rg -q .; then
      echo "rejected GUI upload created a server file" >&2
      return 1
    fi
  fi

  desktop_shutdown_start_ns=$(date +%s%N)
  DISPLAY="$test_display" xdotool key --window "$window" alt+F4
  wait_for_exit "$app_pid" "desktop $label"
  app_pid=""
  desktop_shutdown_end_ns=$(date +%s%N)
  server_shutdown_start_ns=$(date +%s%N)
  kill "$server_pid"
  wait_for_exit "$server_pid" "omenchatd $label"
  server_pid=""
  server_shutdown_end_ns=$(date +%s%N)

  rg -q 'desktop shutdown drained successfully' "$case_root/browser.stderr"
  cp -- "$case_root/browser.stderr" "$case_evidence/browser.stderr"
  cp -- "$case_root/server.log" "$case_evidence/server.log"
  if [[ -f "$browser_root/logs/omenbrowser_rs.jsonl" ]]; then
    cp -- "$browser_root/logs/omenbrowser_rs.jsonl" \
      "$case_evidence/browser-log.jsonl"
  fi
  if [[ -f "$server_root/omenchatd.log" ]]; then
    cp -- "$server_root/omenchatd.log" "$case_evidence/omenchatd.log"
  fi
  if [[ "$label" == "under-limit" ]] && ((sample_seconds > 0)); then
    {
      printf 'utc=%s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
      printf 'build_profile=%s\n' "$build_profile"
      printf 'warmup_seconds=%s\n' "$warmup_seconds"
      printf 'before_upload_sample_seconds=5\n'
      printf 'after_upload_sample_seconds=%s\n' "$sample_seconds"
      printf 'desktop_shutdown_ms=%s\n' \
        "$(( (desktop_shutdown_end_ns - desktop_shutdown_start_ns) / 1000000 ))"
      printf 'server_shutdown_ms=%s\n' \
        "$(( (server_shutdown_end_ns - server_shutdown_start_ns) / 1000000 ))"
      printf 'browser_version=%s\n' "$("$browser_bin" --version)"
      printf 'server_version=%s\n' "$("$server_bin" --version)"
      rustc -Vv | sed 's/^/rustc_/'
    } >"$case_evidence/measurement-metadata.txt"
    if ((sample_seconds >= 30)); then
      rg -q '^stats: active_links=1 ' "$case_evidence/server.log"
      rg -q '^queues: transport=items:0 bytes:0 .*events=items:0 bytes:0 ' \
        "$case_evidence/server.log"
      rg -q \
        'reticulum-rs live server drained .*worker_join_timeouts=0 worker_join_failures=0 queues: transport=items:0 bytes:0 .*events=items:0 bytes:0 ' \
        "$case_evidence/omenchatd.log"
    fi
    {
      cat "$case_evidence/process-summary.txt"
      printf 'desktop_shutdown_ms=%s\n' \
        "$(( (desktop_shutdown_end_ns - desktop_shutdown_start_ns) / 1000000 ))"
      printf 'server_shutdown_ms=%s\n' \
        "$(( (server_shutdown_end_ns - server_shutdown_start_ns) / 1000000 ))"
      if ((sample_seconds >= 30)); then
        rg '^stats: active_links=' "$case_evidence/server.log" |
          tail -n 1 |
          sed 's/^/server_/'
        rg '^queues:' "$case_evidence/server.log" |
          tail -n 1 |
          sed 's/^/server_/'
        rg 'reticulum-rs live server drained' "$case_evidence/omenchatd.log" |
          tail -n 1 |
          sed 's/^/server_/'
      fi
    } >"$case_evidence/resource-summary.txt"
  fi
}

run_case under-limit 262144 "$under_file" accepted
run_case over-limit 262144 "$over_file" rejected
run_case disabled disabled "$under_file" rejected

python3 - "$evidence_root" <<'PY'
import json
import pathlib
import sys

root = pathlib.Path(sys.argv[1])
under = json.loads((root / "under-limit" / "database-observation.json").read_text())
over = json.loads((root / "over-limit" / "database-observation.json").read_text())
disabled = json.loads((root / "disabled" / "database-observation.json").read_text())
report = {
    "status": "pass",
    "isolated_loopback": True,
    "native_linux_iced": True,
    "software_rendering": True,
    "qualification_feature_only": True,
    "accepted_upload_count": under["upload_count"],
    "accepted_upload_bytes": under["upload_bytes"],
    "accepted_upload_file_exists": under["uploads"][0]["path_exists"],
    "over_limit_upload_count": over["upload_count"],
    "disabled_upload_count": disabled["upload_count"],
    "screenshots": {
        label: all((root / label / name).is_file() for name in (
            "before-attach.png", "after-attach.png"
        ))
        for label in ("under-limit", "over-limit", "disabled")
    },
}
if not (
    report["accepted_upload_count"] == 1
    and report["accepted_upload_bytes"] == 65536
    and report["accepted_upload_file_exists"] is True
    and report["over_limit_upload_count"] == 0
    and report["disabled_upload_count"] == 0
    and all(report["screenshots"].values())
):
    raise SystemExit("room media-policy GUI evidence is incomplete")
(root / "report.json").write_text(
    json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8"
)
PY

cat "$evidence_root/report.json"
printf '%s\n' "$evidence_root"
