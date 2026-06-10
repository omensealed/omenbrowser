#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root_dir="$(cd "$script_dir/.." && pwd)"

browser_bin="${OMENBROWSER_BIN:-}"
server_bin="${OMENCHATD_BIN:-}"
tcp_endpoint="${OMENCHAT_TCP_ENDPOINT:-127.0.0.1:42420}"
path_wait="${OMENCHAT_PATH_WAIT:-75}"
out_root="${TMPDIR:-/tmp}/omenbrowser-rs-omenchat-smoke"
message="OMENchat alpha smoke from packaged script"
keep_roots=0
multi_client=0

usage() {
  cat <<'USAGE'
usage: bash scripts/alpha-omenchat-smoke.sh [options]

Start an isolated local omenchatd, then run OMENbrowser_rs's OMENchat Link
smoke against it. This is intended for public alpha validation from either
the repo tree or an unpacked alpha bundle.

Options:
  --browser-bin FILE   OMENbrowser_rs binary to run
  --server-bin FILE    omenchatd binary to run
  --tcp HOST:PORT      Local TCPServerInterface endpoint (default: 127.0.0.1:42420)
  --path-wait SECS     OMENchat path wait seconds (default: 75)
  --out DIR            Output parent directory
  --message TEXT       Smoke message body
  --multi-client       Run a second isolated browser root and verify it receives the first message
  --keep-roots         Leave generated browser/server roots in place
  -h, --help           Show this help

Environment fallbacks:
  OMENBROWSER_BIN
  OMENCHATD_BIN
  OMENCHAT_TCP_ENDPOINT
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
    --multi-client)
      multi_client=1
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

timestamp="$(date -u +%Y%m%dT%H%M%SZ)"
run_dir="${out_root%/}/omenchat-smoke-${timestamp}"
server_home="$run_dir/server-home"
browser_root="$run_dir/browser-root"
browser_root_2="$run_dir/browser-root-2"
mkdir -p "$server_home" "$browser_root"

server_pid=""
cleanup() {
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
"$server_bin" init --home "$server_home" --tcp-server "$tcp_endpoint" \
  > "$run_dir/omenchatd-init.txt"
"$server_bin" config set --home "$server_home" --announce-interval 1 \
  > "$run_dir/omenchatd-config.txt"

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
"$server_bin" run --home "$server_home" --tcp-server "$tcp_endpoint" \
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
  --generate-native-identity "OMENchat Alpha Smoke" \
  --app-root "$browser_root" \
  --stdout \
  > "$run_dir/browser-identity.json" \
  2> "$run_dir/browser-identity.stderr"

echo "== Running OMENchat client smoke =="
"$browser_bin" \
  --omenchat-smoke "$destination" \
  --tcp-client "$tcp_endpoint" \
  --path-wait "$path_wait" \
  --app-root "$browser_root" \
  --omenchat-message "$message" \
  --stdout \
  > "$run_dir/omenchat-smoke.json" \
  2> "$run_dir/omenchat-smoke.stderr"

if ! grep -q '"outcome": "pass"' "$run_dir/omenchat-smoke.json"; then
  echo "OMENchat smoke did not report pass" >&2
  cat "$run_dir/omenchat-smoke.stderr" >&2
  exit 1
fi

if [[ "$multi_client" -eq 1 ]]; then
  echo "== Creating second isolated browser identity =="
  mkdir -p "$browser_root_2"
  "$browser_bin" \
    --generate-native-identity "OMENchat Alpha Smoke 2" \
    --app-root "$browser_root_2" \
    --stdout \
    > "$run_dir/browser-identity-2.json" \
    2> "$run_dir/browser-identity-2.stderr"

  echo "== Running second OMENchat client smoke =="
  second_message="${message} (second client)"
  "$browser_bin" \
    --omenchat-smoke "$destination" \
    --tcp-client "$tcp_endpoint" \
    --path-wait "$path_wait" \
    --app-root "$browser_root_2" \
    --omenchat-message "$second_message" \
    --stdout \
    > "$run_dir/omenchat-smoke-2.json" \
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
fi

cat > "$run_dir/summary.txt" <<EOF
created_utc: $timestamp
outcome: pass
destination: $destination
tcp_endpoint: $tcp_endpoint
browser_bin: $browser_bin
server_bin: $server_bin
browser_root: $browser_root
browser_root_2: $([[ "$multi_client" -eq 1 ]] && printf '%s' "$browser_root_2" || printf 'not-run')
server_home: $server_home
multi_client: $multi_client
EOF

echo "$run_dir"
