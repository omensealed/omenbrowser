#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
readonly repo_root
evidence_root="${TMPDIR:-/tmp}/omenchat-slow-mode-gui-evidence"
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

for tool in Xvfb i3 xdpyinfo xdotool xprop xclip import jq rg python3; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    echo "missing slow-mode GUI qualification tool: $tool" >&2
    exit 2
  fi
done

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

cargo build --quiet --locked --manifest-path "$repo_root/Cargo.toml" \
  --no-default-features \
  --features desktop-product,omenchat-slow-mode-qualification \
  --bin omenbrowser_rs
cargo build --quiet --locked --manifest-path "$repo_root/src/server/Cargo.toml" \
  --no-default-features \
  --features server-headless,omenchat-slow-mode-qualification \
  --bin omenchatd

browser_bin="${CARGO_TARGET_DIR:-$repo_root/target}/debug/omenbrowser_rs"
server_bin="${CARGO_TARGET_DIR:-$repo_root/src/server/target}/debug/omenchatd"
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

kill "$server_pid"
wait "$server_pid"
server_pid=""
rg -q 'desktop shutdown drained successfully' "$session_root/browser.stderr"

cp -- "$session_root/server.log" "$evidence_root/server.log"
cp -- "$session_root/browser.stderr" "$evidence_root/browser.stderr"
if [[ -f "$browser_root/logs/omenbrowser_rs.jsonl" ]]; then
  cp -- "$browser_root/logs/omenbrowser_rs.jsonl" \
    "$evidence_root/browser-log.jsonl"
fi
printf '%s\n' "$evidence_root"
