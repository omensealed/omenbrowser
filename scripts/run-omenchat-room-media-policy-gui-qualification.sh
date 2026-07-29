#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
readonly repo_root
evidence_root="${TMPDIR:-/tmp}/omenchat-room-media-policy-gui-evidence"
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

for tool in Xvfb i3 xdpyinfo xdotool xprop import jq rg python3 truncate find; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    echo "missing room media-policy GUI qualification tool: $tool" >&2
    exit 2
  fi
done

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

cargo build --quiet --locked --manifest-path "$repo_root/Cargo.toml" \
  --no-default-features \
  --features desktop-product,omenchat-room-media-policy-qualification \
  --bin omenbrowser_rs
cargo build --quiet --locked --manifest-path "$repo_root/src/server/Cargo.toml" \
  --no-default-features \
  --features server-headless,omenchat-room-media-policy-qualification \
  --bin omenchatd

browser_bin="${CARGO_TARGET_DIR:-$repo_root/target}/debug/omenbrowser_rs"
server_bin="${CARGO_TARGET_DIR:-$repo_root/src/server/target}/debug/omenchatd"

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

  DISPLAY="$test_display" xdotool key --window "$window" alt+F4
  wait_for_exit "$app_pid" "desktop $label"
  app_pid=""
  kill "$server_pid"
  wait_for_exit "$server_pid" "omenchatd $label"
  server_pid=""

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
