#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root_dir="$(cd "$script_dir/.." && pwd)"

browser_bin="${OMENBROWSER_BIN:-}"
server_bin="${OMENCHATD_BIN:-}"
tcp_endpoint="${OMENCHAT_TCP_ENDPOINT:-127.0.0.1:42420}"
server_tcp_client="${OMENCHAT_SERVER_TCP_CLIENT:-}"
network_name="${OMENCHAT_NETWORK_NAME:-}"
passphrase="${OMENCHAT_PASSPHRASE:-}"
passphrase_file="${OMENCHAT_PASSPHRASE_FILE:-}"
path_wait="${OMENCHAT_PATH_WAIT:-75}"
out_root="${TMPDIR:-/tmp}/omenbrowser-rs-omenchat-smoke"
message="OMENchat release smoke from packaged script"
upload_file=""
server_upload_max_file_bytes=""
server_large_batch_threshold_bytes=""
keep_roots=0
multi_client=0
restart_server=0
continuous_client_reconnect=0
reaction_smoke=0
revision_smoke=0

usage() {
  cat <<'USAGE'
usage: bash scripts/release-omenchat-smoke.sh [options]

Start an isolated local omenchatd, then run OMENbrowser_rs's OMENchat Link
smoke against it. This is intended for public release validation from either
the repo tree or an unpacked release bundle.

Options:
  --browser-bin FILE   OMENbrowser_rs binary to run
  --server-bin FILE    omenchatd binary to run
  --tcp HOST:PORT      Local TCPServerInterface endpoint (default: 127.0.0.1:42420)
  --server-tcp-client HOST:PORT
                       Run omenchatd as a TCP client to an existing gateway.
                       Browser clients also connect to this endpoint.
  --network-name NAME  Optional IFAC network name for server and browser clients
  --passphrase-file FILE
                       Owner-only IFAC passphrase file for server and clients
  --passphrase TEXT    Deprecated: exposes the secret in argv
  --path-wait SECS     OMENchat path wait seconds (default: 75)
  --out DIR            Output parent directory
  --message TEXT       Smoke message body
  --upload-file FILE   Upload this file during the OMENchat smoke and fetch it back
  --server-upload-max-file-bytes BYTES
                       Raise isolated omenchatd per-file upload limit for this run
  --server-large-batch-threshold-bytes BYTES
                       Set the isolated server history/resource threshold for this run
  --multi-client       Run a second isolated browser root and verify it receives the first message
  --restart-server     Gracefully restart omenchatd, reuse the first browser root, and rerun smoke
  --continuous-client-reconnect
                       Keep one browser smoke process alive while omenchatd restarts
  --reaction-smoke     Exercise negotiated durable reactions and authoritative snapshot recovery
  --revision-smoke     Exercise negotiated durable corrections, tombstones, replay, and Resource recovery
  --keep-roots         Leave generated browser/server roots in place
  -h, --help           Show this help

Environment fallbacks:
  OMENBROWSER_BIN
  OMENCHATD_BIN
  OMENCHAT_TCP_ENDPOINT
  OMENCHAT_SERVER_TCP_CLIENT
  OMENCHAT_NETWORK_NAME
  OMENCHAT_PASSPHRASE
  OMENCHAT_PASSPHRASE_FILE
  OMENCHAT_PATH_WAIT
USAGE
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --browser-bin)
      browser_bin="${2:-}"
      shift 2
      ;;
    --server-bin)
      server_bin="${2:-}"
      shift 2
      ;;
    --tcp)
      tcp_endpoint="${2:-}"
      shift 2
      ;;
    --server-tcp-client)
      server_tcp_client="${2:-}"
      tcp_endpoint="${2:-}"
      shift 2
      ;;
    --network-name)
      network_name="${2:-}"
      shift 2
      ;;
    --passphrase)
      passphrase="${2:-}"
      echo "warning: --passphrase exposes the secret in argv; use --passphrase-file" >&2
      shift 2
      ;;
    --passphrase-file)
      passphrase_file="${2:-}"
      shift 2
      ;;
    --path-wait)
      path_wait="${2:-}"
      shift 2
      ;;
    --out)
      out_root="${2:-}"
      shift 2
      ;;
    --message)
      message="${2:-}"
      shift 2
      ;;
    --upload-file)
      upload_file="${2:-}"
      shift 2
      ;;
    --server-upload-max-file-bytes)
      server_upload_max_file_bytes="${2:-}"
      shift 2
      ;;
    --server-large-batch-threshold-bytes)
      server_large_batch_threshold_bytes="${2:-}"
      shift 2
      ;;
    --multi-client)
      multi_client=1
      shift
      ;;
    --restart-server)
      restart_server=1
      shift
      ;;
    --continuous-client-reconnect)
      continuous_client_reconnect=1
      shift
      ;;
    --reaction-smoke)
      reaction_smoke=1
      shift
      ;;
    --revision-smoke)
      revision_smoke=1
      shift
      ;;
    --keep-roots)
      keep_roots=1
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "unknown option: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

if [[ -z "$browser_bin" ]]; then
  if [[ -x "$root_dir/bin/omenbrowser_rs" ]]; then
    browser_bin="$root_dir/bin/omenbrowser_rs"
  else
    browser_bin="$root_dir/target/release/omenbrowser_rs"
  fi
fi

if [[ -z "$server_bin" ]]; then
  if [[ -x "$root_dir/bin/omenchatd" ]]; then
    server_bin="$root_dir/bin/omenchatd"
  else
    server_bin="$root_dir/src/server/target/release/omenchatd"
  fi
fi

if [[ ! -x "$browser_bin" ]]; then
  echo "browser binary is not executable: $browser_bin" >&2
  exit 1
fi

if [[ ! -x "$server_bin" ]]; then
  echo "omenchatd binary is not executable: $server_bin" >&2
  exit 1
fi

if ! [[ "$path_wait" =~ ^[0-9]+$ ]] || [[ "$path_wait" -lt 1 ]]; then
  echo "--path-wait must be a positive integer" >&2
  exit 2
fi

if [[ -n "$upload_file" && ! -f "$upload_file" ]]; then
  echo "--upload-file does not exist: $upload_file" >&2
  exit 2
fi

if [[ -n "$server_upload_max_file_bytes" ]] && ! [[ "$server_upload_max_file_bytes" =~ ^[0-9]+$ ]]; then
  echo "--server-upload-max-file-bytes must be an integer byte count" >&2
  exit 2
fi
if [[ -n "$server_large_batch_threshold_bytes" ]] \
  && { ! [[ "$server_large_batch_threshold_bytes" =~ ^[0-9]+$ ]] \
    || [[ "$server_large_batch_threshold_bytes" -lt 1 ]]; }; then
  echo "--server-large-batch-threshold-bytes must be a positive integer" >&2
  exit 2
fi
if [[ "$restart_server" -eq 1 && "$continuous_client_reconnect" -eq 1 ]]; then
  echo "--restart-server and --continuous-client-reconnect are separate cases" >&2
  exit 2
fi
if [[ ( "$reaction_smoke" -eq 1 || "$revision_smoke" -eq 1 ) \
  && -z "$server_large_batch_threshold_bytes" ]]; then
  server_large_batch_threshold_bytes=1
fi

timestamp="$(date -u +%Y%m%dT%H%M%SZ)"
run_dir="${out_root%/}/omenchat-smoke-${timestamp}"
server_home="$run_dir/server-home"
browser_root="$run_dir/browser-root"
browser_root_2="$run_dir/browser-root-2"
mkdir -p "$server_home" "$browser_root"

if [[ -n "$passphrase" && -n "$passphrase_file" ]]; then
  echo "choose only one passphrase source" >&2
  exit 2
fi
if [[ -n "$passphrase" ]]; then
  passphrase_file="$run_dir/ifac-passphrase"
  (umask 077; printf '%s\n' "$passphrase" > "$passphrase_file")
  unset passphrase
fi
if [[ -n "$passphrase_file" && ! -f "$passphrase_file" ]]; then
  echo "--passphrase-file does not exist: $passphrase_file" >&2
  exit 2
fi

server_interface_args=()
client_interface_args=(--tcp-client "$tcp_endpoint")
if [[ -n "$server_tcp_client" ]]; then
  server_interface_args=(--tcp-client "$server_tcp_client")
else
  server_interface_args=(--tcp-server "$tcp_endpoint")
fi
if [[ -n "$network_name" ]]; then
  server_interface_args+=(--network-name "$network_name")
  client_interface_args+=(--network-name "$network_name")
fi
if [[ -n "$passphrase_file" ]]; then
  server_interface_args+=(--passphrase-file "$passphrase_file")
  client_interface_args+=(--passphrase-file "$passphrase_file")
fi

server_pid=""
client_pid=""
cleanup() {
  if [[ -n "$client_pid" ]] && kill -0 "$client_pid" 2>/dev/null; then
    kill "$client_pid" 2>/dev/null || true
    wait "$client_pid" 2>/dev/null || true
  fi
  if [[ -n "$server_pid" ]] && kill -0 "$server_pid" 2>/dev/null; then
    kill "$server_pid" 2>/dev/null || true
    wait "$server_pid" 2>/dev/null || true
  fi
  if [[ "$keep_roots" -eq 0 ]]; then
    rm -rf "$server_home" "$browser_root" "$browser_root_2"
  fi
}
trap cleanup EXIT

echo "== Initializing isolated omenchatd =="
"$server_bin" init --home "$server_home" "${server_interface_args[@]}" \
  > "$run_dir/omenchatd-init.txt"
"$server_bin" config set --home "$server_home" --announce-interval 1 \
  > "$run_dir/omenchatd-config.txt"
if [[ -n "$server_upload_max_file_bytes" ]]; then
  "$server_bin" config set --home "$server_home" \
    --upload-max-file-bytes "$server_upload_max_file_bytes" \
    >> "$run_dir/omenchatd-config.txt"
fi
if [[ -n "$server_large_batch_threshold_bytes" ]]; then
  "$server_bin" config set --home "$server_home" \
    --large-batch-threshold-bytes "$server_large_batch_threshold_bytes" \
    >> "$run_dir/omenchatd-config.txt"
fi

"$server_bin" status --home "$server_home" > "$run_dir/omenchatd-status.txt"
destination="$(
  sed -n 's/^client uri: omenchat:\/\/\([0-9a-fA-F]\+\)$/\1/p' \
    "$run_dir/omenchatd-status.txt" | head -n 1
)"

if [[ -z "$destination" ]]; then
  echo "could not parse OMENchat destination from omenchatd status" >&2
  exit 1
fi

echo "== Starting isolated omenchatd =="
"$server_bin" run --home "$server_home" "${server_interface_args[@]}" \
  > "$run_dir/omenchatd-run.log" 2>&1 &
server_pid="$!"

for _ in {1..80}; do
  if grep -q 'live server ready' "$run_dir/omenchatd-run.log" 2>/dev/null; then
    break
  fi
  if ! kill -0 "$server_pid" 2>/dev/null; then
    echo "omenchatd exited before becoming ready" >&2
    tail -n 80 "$run_dir/omenchatd-run.log" >&2 || true
    exit 1
  fi
  sleep 0.25
done

if ! grep -q 'live server ready' "$run_dir/omenchatd-run.log" 2>/dev/null; then
  echo "omenchatd did not become ready in time" >&2
  tail -n 80 "$run_dir/omenchatd-run.log" >&2 || true
  exit 1
fi

echo "== Creating isolated browser identity =="
"$browser_bin" \
  --generate-native-identity "OMENchat Release Smoke" \
  --app-root "$browser_root" \
  --stdout \
  > "$run_dir/browser-identity.json" \
  2> "$run_dir/browser-identity.stderr"

echo "== Running OMENchat client smoke =="
restart_destination_stable=0
restart_stop="not-run"
upload_args=()
if [[ -n "$upload_file" ]]; then
  upload_args=(--omenchat-upload-file "$upload_file")
fi
continuous_args=()
if [[ "$continuous_client_reconnect" -eq 1 ]]; then
  continuous_args=(
    --omenchat-reconnect-ready-file "$run_dir/continuous-client-ready"
    --omenchat-reconnect-wait 75
  )
fi
reaction_args=()
if [[ "$reaction_smoke" -eq 1 ]]; then
  reaction_args=(--omenchat-reaction-smoke --omenchat-response-wait 30)
fi
revision_args=()
if [[ "$revision_smoke" -eq 1 ]]; then
  revision_args=(--omenchat-revision-smoke --omenchat-response-wait 30)
fi
"$browser_bin" \
  --omenchat-smoke "$destination" \
  "${client_interface_args[@]}" \
  --path-wait "$path_wait" \
  --app-root "$browser_root" \
  --omenchat-message "$message" \
  "${reaction_args[@]}" \
  "${revision_args[@]}" \
  "${upload_args[@]}" \
  "${continuous_args[@]}" \
  --output "$run_dir/omenchat-smoke.json" \
  > "$run_dir/omenchat-smoke.stdout" \
  2> "$run_dir/omenchat-smoke.stderr" &
client_pid="$!"

if [[ "$continuous_client_reconnect" -eq 1 ]]; then
  for _ in {1..480}; do
    if [[ -f "$run_dir/continuous-client-ready" ]]; then
      break
    fi
    if ! kill -0 "$client_pid" 2>/dev/null; then
      echo "OMENchat smoke exited before reaching the reconnect boundary" >&2
      cat "$run_dir/omenchat-smoke.stderr" >&2 || true
      exit 1
    fi
    sleep 0.25
  done
  if [[ ! -f "$run_dir/continuous-client-ready" ]]; then
    echo "OMENchat smoke did not reach the reconnect boundary in time" >&2
    exit 1
  fi

  echo "== Restarting omenchatd while the client remains alive =="
  kill "$server_pid"
  for _ in {1..80}; do
    if ! kill -0 "$server_pid" 2>/dev/null; then
      break
    fi
    sleep 0.25
  done
  if kill -0 "$server_pid" 2>/dev/null; then
    echo "omenchatd did not stop within the continuous reconnect deadline" >&2
    exit 1
  fi
  set +e
  wait "$server_pid"
  server_stop_status=$?
  set -e
  case "$server_stop_status" in
    0) restart_stop="orderly" ;;
    143) restart_stop="sigterm" ;;
    *)
      echo "omenchatd returned unexpected continuous restart status $server_stop_status" >&2
      exit 1
      ;;
  esac
  server_pid=""

  "$server_bin" run --home "$server_home" "${server_interface_args[@]}" \
    > "$run_dir/omenchatd-run-continuous-restart.log" 2>&1 &
  server_pid="$!"
  for _ in {1..80}; do
    if grep -q 'live server ready' "$run_dir/omenchatd-run-continuous-restart.log" 2>/dev/null; then
      break
    fi
    if ! kill -0 "$server_pid" 2>/dev/null; then
      echo "restarted omenchatd exited before becoming ready" >&2
      exit 1
    fi
    sleep 0.25
  done
  if ! grep -q 'live server ready' "$run_dir/omenchatd-run-continuous-restart.log" 2>/dev/null; then
    echo "restarted omenchatd did not become ready in time" >&2
    exit 1
  fi
  "$server_bin" status --home "$server_home" > "$run_dir/omenchatd-status-continuous-restart.txt"
  continuous_destination="$(
    sed -n 's/^client uri: omenchat:\/\/\([0-9a-fA-F]\+\)$/\1/p' \
      "$run_dir/omenchatd-status-continuous-restart.txt" | head -n 1
  )"
  if [[ "$continuous_destination" != "$destination" ]]; then
    echo "omenchatd destination changed during continuous reconnect" >&2
    exit 1
  fi
  restart_destination_stable=1
fi

for _ in {1..480}; do
  if ! kill -0 "$client_pid" 2>/dev/null; then
    break
  fi
  sleep 0.25
done
if kill -0 "$client_pid" 2>/dev/null; then
  echo "OMENchat smoke did not finish within its bounded deadline" >&2
  exit 1
fi
wait "$client_pid"
client_pid=""

if ! grep -q '"outcome": "pass"' "$run_dir/omenchat-smoke.json"; then
  echo "OMENchat smoke did not report pass" >&2
  cat "$run_dir/omenchat-smoke.stderr" >&2
  exit 1
fi
continuous_link_closed=0
continuous_link_reopened=0
continuous_session_reconnected=0
continuous_message_echoed=0
continuous_reaction_recovered=0
if [[ "$continuous_client_reconnect" -eq 1 ]]; then
  python3 - "$run_dir/omenchat-smoke.json" "$reaction_smoke" "$revision_smoke" <<'PY'
import json
import pathlib
import sys

report = json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))
reaction_smoke = sys.argv[2] == "1"
revision_smoke = sys.argv[3] == "1"
stages = {
    stage.get("stage"): stage
    for stage in report.get("stages", [])
    if isinstance(stage, dict) and isinstance(stage.get("stage"), str)
}
required = (
    "continuous_link_close_wait",
    "continuous_link_reopen",
    "continuous_session_reconnect",
    "continuous_message_send",
    "continuous_message_echo_wait",
)
if any(stages.get(name, {}).get("ok") is not True for name in required):
    raise SystemExit("continuous reconnect stage evidence was incomplete")
if stages["continuous_link_reopen"].get("link_changed") is not True:
    raise SystemExit("continuous reconnect reused the closed link identifier")
if reaction_smoke:
    reaction_required = (
        "continuous_reaction_capability",
        "continuous_reaction_lost_ack",
        "continuous_reaction_exact_replay",
        "continuous_reaction_resource_snapshot",
        "continuous_reaction_noop_add",
        "continuous_reaction_remove",
        "continuous_reaction_remove_snapshot",
        "continuous_reaction_intent_persistence",
    )
    if any(stages.get(name, {}).get("ok") is not True for name in reaction_required):
        raise SystemExit("replacement-link reaction evidence was incomplete")
if revision_smoke:
    revision_required = (
        "continuous_revision_capability",
        "continuous_revision_lost_ack",
        "continuous_revision_exact_replay",
        "continuous_revision_correction_resource_snapshot",
        "continuous_revision_tombstone",
        "continuous_revision_tombstone_resource_snapshot",
        "continuous_revision_intent_persistence",
    )
    if any(stages.get(name, {}).get("ok") is not True for name in revision_required):
        raise SystemExit("replacement-link revision evidence was incomplete")
PY
  continuous_link_closed=1
  continuous_link_reopened=1
  continuous_session_reconnected=1
  continuous_message_echoed=1
  if [[ "$reaction_smoke" -eq 1 ]]; then
    continuous_reaction_recovered=1
  fi
fi

if [[ "$restart_server" -eq 1 ]]; then
  echo "== Restarting isolated omenchatd =="
  kill "$server_pid"
  for _ in {1..80}; do
    if ! kill -0 "$server_pid" 2>/dev/null; then
      break
    fi
    sleep 0.25
  done
  if kill -0 "$server_pid" 2>/dev/null; then
    echo "omenchatd did not stop within the restart deadline" >&2
    exit 1
  fi
  set +e
  wait "$server_pid"
  server_stop_status=$?
  set -e
  case "$server_stop_status" in
    0)
      restart_stop="orderly"
      ;;
    143)
      # The hardened 0.6 server predates the owned SIGTERM drain path. It still
      # stops within the deadline; mixed-version restart evidence records that
      # narrower signal-stop boundary instead of claiming orderly shutdown.
      restart_stop="sigterm"
      ;;
    *)
      echo "omenchatd returned unexpected restart status $server_stop_status" >&2
      exit 1
      ;;
  esac
  server_pid=""

  "$server_bin" run --home "$server_home" "${server_interface_args[@]}" \
    > "$run_dir/omenchatd-run-restart.log" 2>&1 &
  server_pid="$!"
  for _ in {1..80}; do
    if grep -q 'live server ready' "$run_dir/omenchatd-run-restart.log" 2>/dev/null; then
      break
    fi
    if ! kill -0 "$server_pid" 2>/dev/null; then
      echo "restarted omenchatd exited before becoming ready" >&2
      tail -n 80 "$run_dir/omenchatd-run-restart.log" >&2 || true
      exit 1
    fi
    sleep 0.25
  done
  if ! grep -q 'live server ready' "$run_dir/omenchatd-run-restart.log" 2>/dev/null; then
    echo "restarted omenchatd did not become ready in time" >&2
    tail -n 80 "$run_dir/omenchatd-run-restart.log" >&2 || true
    exit 1
  fi

  "$server_bin" status --home "$server_home" > "$run_dir/omenchatd-status-restart.txt"
  restart_destination="$(
    sed -n 's/^client uri: omenchat:\/\/\([0-9a-fA-F]\+\)$/\1/p' \
      "$run_dir/omenchatd-status-restart.txt" | head -n 1
  )"
  if [[ -z "$restart_destination" || "$restart_destination" != "$destination" ]]; then
    echo "omenchatd destination changed across restart" >&2
    exit 1
  fi
  restart_destination_stable=1

  echo "== Reopening OMENchat client state after server restart =="
  "$browser_bin" \
    --omenchat-smoke "$restart_destination" \
    "${client_interface_args[@]}" \
    --path-wait "$path_wait" \
    --app-root "$browser_root" \
    --omenchat-message "${message} (after server restart)" \
    "${reaction_args[@]}" \
    "${revision_args[@]}" \
    --output "$run_dir/omenchat-smoke-restart.json" \
    > "$run_dir/omenchat-smoke-restart.stdout" \
    2> "$run_dir/omenchat-smoke-restart.stderr"

  if ! grep -q '"outcome": "pass"' "$run_dir/omenchat-smoke-restart.json"; then
    echo "post-restart OMENchat smoke did not report pass" >&2
    cat "$run_dir/omenchat-smoke-restart.stderr" >&2
    exit 1
  fi
fi

if [[ "$multi_client" -eq 1 ]]; then
  echo "== Creating second isolated browser identity =="
  mkdir -p "$browser_root_2"
  "$browser_bin" \
    --generate-native-identity "OMENchat Release Smoke 2" \
    --app-root "$browser_root_2" \
    --stdout \
    > "$run_dir/browser-identity-2.json" \
    2> "$run_dir/browser-identity-2.stderr"

  echo "== Running second OMENchat client smoke =="
  second_message="${message} (second client)"
  second_upload_args=()
  if [[ -n "$upload_file" ]]; then
    upload_name="$(basename "$upload_file")"
    if stat -c %s "$upload_file" >/dev/null 2>&1; then
      upload_bytes="$(stat -c %s "$upload_file")"
    else
      upload_bytes="$(wc -c < "$upload_file" | tr -d '[:space:]')"
    fi
    second_upload_args=(
      --omenchat-fetch-upload "$upload_name"
      --omenchat-fetch-upload-bytes "$upload_bytes"
    )
  fi
  "$browser_bin" \
    --omenchat-smoke "$destination" \
    "${client_interface_args[@]}" \
    --path-wait "$path_wait" \
    --app-root "$browser_root_2" \
    --omenchat-message "$second_message" \
    "${reaction_args[@]}" \
    "${revision_args[@]}" \
    "${second_upload_args[@]}" \
    --output "$run_dir/omenchat-smoke-2.json" \
    > "$run_dir/omenchat-smoke-2.stdout" \
    2> "$run_dir/omenchat-smoke-2.stderr"

  if ! grep -q '"outcome": "pass"' "$run_dir/omenchat-smoke-2.json"; then
    echo "second OMENchat smoke did not report pass" >&2
    cat "$run_dir/omenchat-smoke-2.stderr" >&2
    exit 1
  fi

  if ! grep -Fq "$message" "$run_dir/omenchat-smoke-2.json"; then
    echo "second OMENchat smoke did not observe first client's message in room history" >&2
    cat "$run_dir/omenchat-smoke-2.json" >&2
    exit 1
  fi

  if [[ -n "$upload_file" ]] && ! grep -q '"event": "upload_resource_available"' "$run_dir/omenchat-smoke-2.json"; then
    echo "second OMENchat smoke did not fetch first client's upload resource" >&2
    cat "$run_dir/omenchat-smoke-2.json" >&2
    exit 1
  fi
fi

cat > "$run_dir/summary.txt" <<EOF
created_utc: $timestamp
outcome: pass
destination: $destination
tcp_endpoint: $tcp_endpoint
server_mode: $([[ -n "$server_tcp_client" ]] && printf 'tcp-client' || printf 'tcp-server')
ifac: $([[ -n "$network_name$passphrase" ]] && printf 'configured' || printf 'none')
browser_bin: $browser_bin
server_bin: $server_bin
browser_root: $browser_root
browser_root_2: $([[ "$multi_client" -eq 1 ]] && printf '%s' "$browser_root_2" || printf 'not-run')
server_home: $server_home
multi_client: $multi_client
restart_server: $restart_server
continuous_client_reconnect: $continuous_client_reconnect
reaction_smoke: $reaction_smoke
revision_smoke: $revision_smoke
continuous_link_closed: $continuous_link_closed
continuous_link_reopened: $continuous_link_reopened
continuous_session_reconnected: $continuous_session_reconnected
continuous_message_echoed: $continuous_message_echoed
continuous_reaction_recovered: $continuous_reaction_recovered
restart_destination_stable: $restart_destination_stable
restart_stop: $restart_stop
server_large_batch_threshold_bytes: $([[ -n "$server_large_batch_threshold_bytes" ]] && printf '%s' "$server_large_batch_threshold_bytes" || printf 'default')
upload_file: $([[ -n "$upload_file" ]] && printf '%s' "$upload_file" || printf 'not-run')
EOF

echo "$run_dir"
