#!/usr/bin/env bash
set -euo pipefail

source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/_common.sh"
smoke_init "08_nomadnet_page_fetch.sh"
cd "$REPO_ROOT"

smoke_run "browser cache tests" cargo test --test browser_cache
smoke_run "browser partials tests" cargo test --test browser_partials
smoke_run "browser path retry tests" cargo test --locked --no-default-features --features desktop-product --test browser_path_retry

smoke_run "build browser release" cargo build --release --locked --no-default-features --features desktop-product --bin omenbrowser_rs
smoke_run "build server release" cargo build --release --locked --manifest-path src/server/Cargo.toml --no-default-features --features server-headless

browser_bin="${CARGO_TARGET_DIR:-$REPO_ROOT/target}/release/omenbrowser_rs"
server_bin="${CARGO_TARGET_DIR:-$REPO_ROOT/src/server/target}/release/omenchatd"
tcp_endpoint="${OMENBROWSER_SMOKE_NOMADNET_TCP:-127.0.0.1:42428}"
server_home="$SMOKE_RUN_ROOT/nomadnet-server-home"
browser_root="$SMOKE_RUN_ROOT/nomadnet-browser-root"
mkdir -p "$server_home" "$browser_root"

server_pid=""
cleanup() {
  if [[ -n "$server_pid" ]] && kill -0 "$server_pid" 2>/dev/null; then
    kill "$server_pid" 2>/dev/null || true
    wait "$server_pid" 2>/dev/null || true
  fi
}
trap cleanup EXIT

echo "== Initializing isolated NomadNet portal server =="
"$server_bin" init --home "$server_home" --tcp-server "$tcp_endpoint" \
  > "$SMOKE_LOG_DIR/nomadnet-server-init.log" 2>&1
"$server_bin" config set --home "$server_home" --announce-interval 1 \
  > "$SMOKE_LOG_DIR/nomadnet-server-config.log" 2>&1
"$server_bin" status --home "$server_home" \
  > "$SMOKE_LOG_DIR/nomadnet-server-status.log" 2>&1

portal_url="$(
  sed -n 's/^portal url: //p' "$SMOKE_LOG_DIR/nomadnet-server-status.log" | head -n 1
)"
if [[ -z "$portal_url" ]]; then
  echo "could not parse NomadNet portal URL from omenchatd status" >&2
  cat "$SMOKE_LOG_DIR/nomadnet-server-status.log" >&2
  exit 1
fi

echo "== Starting isolated NomadNet portal server =="
"$server_bin" run --home "$server_home" --tcp-server "$tcp_endpoint" \
  > "$SMOKE_LOG_DIR/nomadnet-server-run.log" 2>&1 &
server_pid="$!"

for _ in {1..80}; do
  if grep -q 'live server ready' "$SMOKE_LOG_DIR/nomadnet-server-run.log" 2>/dev/null; then
    break
  fi
  if ! kill -0 "$server_pid" 2>/dev/null; then
    echo "omenchatd exited before becoming ready" >&2
    tail -n 120 "$SMOKE_LOG_DIR/nomadnet-server-run.log" >&2 || true
    exit 1
  fi
  sleep 0.25
done

if ! grep -q 'live server ready' "$SMOKE_LOG_DIR/nomadnet-server-run.log" 2>/dev/null; then
  echo "omenchatd did not become ready in time" >&2
  tail -n 120 "$SMOKE_LOG_DIR/nomadnet-server-run.log" >&2 || true
  exit 1
fi

echo "== Creating isolated browser identity =="
"$browser_bin" \
  --generate-native-identity "NomadNet Smoke" \
  --app-root "$browser_root" \
  --stdout \
  > "$SMOKE_LOG_DIR/nomadnet-browser-identity.json" \
  2> "$SMOKE_LOG_DIR/nomadnet-browser-identity.stderr"

echo "== Running live NomadNet page fetch smoke =="
"$browser_bin" \
  --native-smoke "$portal_url" \
  --backend reticulum \
  --tcp-client "$tcp_endpoint" \
  --path-wait "${OMENBROWSER_SMOKE_NOMADNET_PATH_WAIT:-75}" \
  --live \
  --fetch-page \
  --app-root "$browser_root" \
  --output "$SMOKE_LOG_DIR/nomadnet-fetch-report.json" \
  > "$SMOKE_LOG_DIR/nomadnet-fetch.stdout" \
  2> "$SMOKE_LOG_DIR/nomadnet-fetch.stderr"

if ! grep -q '"outcome": "pass"' "$SMOKE_LOG_DIR/nomadnet-fetch-report.json"; then
  echo "NomadNet live fetch did not report pass" >&2
  cat "$SMOKE_LOG_DIR/nomadnet-fetch.stderr" >&2
  cat "$SMOKE_LOG_DIR/nomadnet-fetch-report.json" >&2
  exit 1
fi

if ! grep -q '"live_fetch"' "$SMOKE_LOG_DIR/nomadnet-fetch-report.json"; then
  echo "NomadNet report did not include live_fetch section" >&2
  cat "$SMOKE_LOG_DIR/nomadnet-fetch-report.json" >&2
  exit 1
fi

smoke_pass
