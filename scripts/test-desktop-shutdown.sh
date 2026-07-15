#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

binary="${OMENBROWSER_BINARY:-target/release/omenbrowser_rs}"
close_timeout_seconds="${CLOSE_TIMEOUT_SECONDS:-8}"

case "$close_timeout_seconds" in
  ''|*[!0-9]*|0) echo "CLOSE_TIMEOUT_SECONDS must be a positive integer" >&2; exit 2 ;;
esac

for tool in Xvfb i3 xdpyinfo xdotool xprop jq rg; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    echo "missing native shutdown test tool: $tool" >&2
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
grep -q 'chat-client-reticulum:on' <<<"$version"

if rg -n '(^|[^[:alnum:]_])(std::)?process::exit' src/desktop >/dev/null; then
  echo "routine desktop code must not call process::exit" >&2
  exit 1
fi

work_root="$(mktemp -d "${TMPDIR:-/tmp}/omenbrowser-native-shutdown.XXXXXX")"
app_root="$work_root/app"
display_number=$((220 + ($$ % 200)))
while [[ -e "/tmp/.X11-unix/X$display_number" ]]; do
  display_number=$((display_number + 1))
done
test_display=":$display_number"
xvfb_pid=""
wm_pid=""
app_pid=""

cleanup() {
  if [[ -n "$app_pid" ]] && kill -0 "$app_pid" 2>/dev/null; then
    kill "$app_pid" 2>/dev/null || true
    wait "$app_pid" 2>/dev/null || true
  fi
  if [[ -n "$wm_pid" ]]; then kill "$wm_pid" 2>/dev/null || true; fi
  if [[ -n "$xvfb_pid" ]]; then kill "$xvfb_pid" 2>/dev/null || true; fi
  rm -rf "$work_root"
}
trap cleanup EXIT INT TERM

Xvfb "$test_display" -screen 0 1280x800x24 -nolisten tcp \
  >"$work_root/xvfb.log" 2>&1 &
xvfb_pid="$!"
for _ in $(seq 1 100); do
  if DISPLAY="$test_display" xdpyinfo >/dev/null 2>&1; then break; fi
  kill -0 "$xvfb_pid" 2>/dev/null || {
    echo "Xvfb exited during startup" >&2
    exit 1
  }
  sleep 0.05
done
DISPLAY="$test_display" xdpyinfo >/dev/null 2>&1 || {
  echo "Xvfb did not become ready" >&2
  exit 1
}

DISPLAY="$test_display" I3SOCK="$work_root/i3.sock" \
  i3 -c "$repo_root/scripts/fixtures/i3-native-test.config" \
  >"$work_root/i3.log" 2>&1 &
wm_pid="$!"
for _ in $(seq 1 100); do
  if DISPLAY="$test_display" xprop -root _NET_SUPPORTING_WM_CHECK 2>/dev/null \
      | grep -q 'window id'; then
    break
  fi
  kill -0 "$wm_pid" 2>/dev/null || {
    echo "i3 exited during startup" >&2
    exit 1
  }
  sleep 0.05
done
DISPLAY="$test_display" xprop -root _NET_SUPPORTING_WM_CHECK 2>/dev/null \
  | grep -q 'window id' || {
    echo "test window manager did not become ready" >&2
    exit 1
  }

start_ms="$(date +%s%3N)"
DISPLAY="$test_display" LIBGL_ALWAYS_SOFTWARE=1 RUST_LOG=omenbrowser_rs=info \
  "$binary" --desktop --app-root "$app_root" \
  >"$work_root/stdout.log" 2>"$work_root/stderr.log" &
app_pid="$!"

window=""
for _ in $(seq 1 200); do
  window="$(DISPLAY="$test_display" xdotool search --onlyvisible --pid "$app_pid" \
    --name '^OMENbrowser_rs$' 2>/dev/null | head -n 1 || true)"
  if [[ -n "$window" ]]; then break; fi
  kill -0 "$app_pid" 2>/dev/null || {
    echo "desktop exited before opening a window" >&2
    sed -n '1,120p' "$work_root/stderr.log" >&2
    exit 1
  }
  sleep 0.05
done
if [[ -z "$window" ]]; then
  echo "desktop window did not become visible" >&2
  exit 1
fi
window_ms="$(date +%s%3N)"

DISPLAY="$test_display" xdotool windowactivate --sync "$window"
# Creating a browser tab schedules UI-preference persistence for 500 ms. Close
# well before that deadline so the shutdown flush, rather than the timer, must
# commit the updated workspace.
DISPLAY="$test_display" xdotool key --window "$window" ctrl+t
sleep 0.10
close_ms="$(date +%s%3N)"
DISPLAY="$test_display" xdotool key --window "$window" alt+F4

iterations=$((close_timeout_seconds * 20))
for _ in $(seq 1 "$iterations"); do
  if ! kill -0 "$app_pid" 2>/dev/null; then break; fi
  sleep 0.05
done
if kill -0 "$app_pid" 2>/dev/null; then
  echo "desktop did not return within ${close_timeout_seconds}s after close" >&2
  sed -n '1,160p' "$work_root/stderr.log" >&2
  sed -n '1,120p' "$work_root/i3.log" >&2
  exit 1
fi
set +e
wait "$app_pid"
app_status="$?"
set -e
app_pid=""
end_ms="$(date +%s%3N)"
if [[ "$app_status" -ne 0 ]]; then
  echo "desktop returned non-zero status $app_status" >&2
  sed -n '1,160p' "$work_root/stderr.log" >&2
  exit 1
fi

grep -q 'desktop shutdown drained successfully' "$work_root/stderr.log" || {
  echo "successful shutdown-drain trace was not flushed" >&2
  sed -n '1,160p' "$work_root/stderr.log" >&2
  exit 1
}
if [[ ! -f "$app_root/settings.json" ]]; then
  echo "shutdown did not persist settings.json" >&2
  exit 1
fi
structured_log="$app_root/logs/omenbrowser_rs.jsonl"
if [[ ! -s "$structured_log" ]]; then
  echo "shutdown did not flush the browser structured log" >&2
  exit 1
fi
jq -e . "$structured_log" >/dev/null || {
  echo "shutdown left malformed structured JSONL" >&2
  exit 1
}
jq -e 'select(.message == "OMENbrowser_rs mock shell initialized")' \
  "$structured_log" >/dev/null || {
  echo "shutdown structured log is missing the startup record" >&2
  exit 1
}
while IFS= read -r json_file; do
  jq empty "$json_file"
done < <(rg --files "$app_root" -g '*.json')
jq -e '.ui.desktop_workspace_panes | length >= 2' "$app_root/settings.json" >/dev/null || {
  echo "pending workspace preference was not committed during shutdown" >&2
  exit 1
}
if find "$app_root" -type f \( -name '*.tmp' -o -name '*.partial' \) -print -quit \
    | grep -q .; then
  echo "shutdown left a temporary persistence file behind" >&2
  exit 1
fi

printf 'native desktop shutdown: pass\n'
printf 'binary=%s\n' "$(realpath "$binary")"
printf 'startup_to_window_ms=%s\n' "$((window_ms - start_ms))"
printf 'close_latency_ms=%s\n' "$((end_ms - close_ms))"
printf 'persisted_workspace_panes=%s\n' \
  "$(jq -r '.ui.desktop_workspace_panes | length' "$app_root/settings.json")"
printf 'normal_process_return=yes\nshutdown_trace_flushed=yes\nstructured_log_flushed=yes\njson_files_parse=yes\n'
