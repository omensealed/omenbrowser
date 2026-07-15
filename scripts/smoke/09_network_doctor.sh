#!/usr/bin/env bash
set -euo pipefail

source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/_common.sh"
smoke_init "09_network_doctor.sh"
cd "$REPO_ROOT"

browser_root="$SMOKE_RUN_ROOT/browser-root"
browser_root_2="$SMOKE_RUN_ROOT/browser-root-2"
server_home="$SMOKE_RUN_ROOT/server-home"
collector_out="$SMOKE_RUN_ROOT/collector"
mkdir -p "$browser_root/logs" "$browser_root_2/logs" "$server_home/logs"
printf 'smoke browser log with /tmp only\n' > "$browser_root/logs/runtime.log"
printf 'smoke browser 2 log with /tmp only\n' > "$browser_root_2/logs/runtime.log"
printf 'smoke server log with token=<redacted-source>\n' > "$server_home/logs/runtime.log"

smoke_run "build server debug" cargo build --locked --manifest-path src/server/Cargo.toml --no-default-features --features server-headless
smoke_run "omenchatd isolated init" \
  src/server/target/debug/omenchatd init --home "$server_home" --tcp-client 127.0.0.1:4242

smoke_run "release collect isolated" \
  bash scripts/release-collect.sh \
    --browser-root "$browser_root" \
    --browser-root-2 "$browser_root_2" \
    --server-home "$server_home" \
    --out "$collector_out" \
    --tail-lines 20

if grep -R "$HOME/.reticulum\|$HOME/.nomadnetwork\|$HOME/.lxmd" "$collector_out" >/dev/null 2>&1; then
  echo "RESULT: FAIL"
  echo "reason: diagnostic bundle referenced a live user Reticulum/NomadNetwork/LXMD path"
  exit 1
fi

smoke_pass
