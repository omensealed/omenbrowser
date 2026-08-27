#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
readonly repo_root
readonly fixture="$repo_root/fixtures/omenchat/v0_6_0_1_wire.rs"
report_path=""
routed=0
while (($#)); do
  case "$1" in
    --report)
      if (($# < 2)); then
        echo "--report requires a path" >&2
        exit 2
      fi
      report_path=$2
      shift 2
      ;;
    --routed)
      routed=1
      shift
      ;;
    *)
      echo "usage: $0 [--routed] [--report /path/to/report.json]" >&2
      exit 2
      ;;
  esac
done

[[ -f "$fixture" && ! -L "$fixture" ]]
fixture_bytes=$(wc -c < "$fixture" | tr -d '[:space:]')
[[ "$fixture_bytes" == "873" ]]

temporary_root=$(mktemp -d "${TMPDIR:-/tmp}/omen-current-upload.XXXXXX")
gateway_pid=""
cleanup() {
  if [[ -n "$gateway_pid" ]]; then
    kill "$gateway_pid" 2>/dev/null || true
    wait "$gateway_pid" 2>/dev/null || true
  fi
  rm -rf -- "$temporary_root"
}
trap cleanup EXIT INT TERM
trap 'status=$?; echo "current OMENchat upload harness failed at line $LINENO (status $status)" >&2' ERR

cargo build --locked --manifest-path "$repo_root/Cargo.toml" \
  --no-default-features --features desktop-product --bin omenbrowser_rs
cargo build --locked --manifest-path "$repo_root/src/server/Cargo.toml" \
  --no-default-features --features server-headless --bin omenchatd

browser_bin="${CARGO_TARGET_DIR:-$repo_root/target}/debug/omenbrowser_rs"
server_bin="${CARGO_TARGET_DIR:-$repo_root/src/server/target}/debug/omenchatd"
browser_version=$("$browser_bin" --version | awk '{print $2}')
server_version=$("$server_bin" --version | awk '{print $2}')
[[ "$browser_version" == "0.10.0-5" ]]
[[ "$server_version" == "0.10.0-5" ]]

port=$(python3 -c \
  'import socket; s=socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1]); s.close()')
endpoint="127.0.0.1:$port"
topology_args=(--tcp "$endpoint")
topology="direct"
if [[ "$routed" -eq 1 ]]; then
  gateway_bin="${CARGO_TARGET_DIR:-$repo_root/target}/debug/omen-reticulum-gateway"
  cargo build --locked --manifest-path "$repo_root/Cargo.toml" \
    --no-default-features --features desktop-product --bin omen-reticulum-gateway
  "$gateway_bin" --listen "$endpoint" > "$temporary_root/gateway.log" 2>&1 &
  gateway_pid=$!
  for _ in $(seq 1 100); do
    if grep -q 'gateway listening\|listening on' "$temporary_root/gateway.log" 2>/dev/null; then
      break
    fi
    if ! kill -0 "$gateway_pid" 2>/dev/null; then
      cat "$temporary_root/gateway.log" >&2
      exit 1
    fi
    sleep 0.1
  done
  kill -0 "$gateway_pid"
  topology_args+=(--server-tcp-client "$endpoint")
  topology="three-node-routed"
fi
run_dir=$(bash "$repo_root/scripts/release-omenchat-smoke.sh" \
  --browser-bin "$browser_bin" \
  --server-bin "$server_bin" \
  "${topology_args[@]}" \
  --path-wait 45 \
  --out "$temporary_root/smoke" \
  --message "current product upload smoke" \
  --multi-client \
  --upload-file "$fixture" | tail -n 1)

grep -qx 'outcome: pass' "$run_dir/summary.txt"
grep -qx 'multi_client: 1' "$run_dir/summary.txt"
summary="$temporary_root/report.json"
python3 - "$run_dir/omenchat-smoke.json" "$run_dir/omenchat-smoke-2.json" \
  "$summary" "$browser_version" "$server_version" "$fixture_bytes" "$topology" <<'PY'
import json
import pathlib
import sys

first = json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))
second = json.loads(pathlib.Path(sys.argv[2]).read_text(encoding="utf-8"))
expected_bytes = int(sys.argv[6])

def events(value, name):
    found = []
    if isinstance(value, dict):
        if value.get("event") == name:
            found.append(value)
        for child in value.values():
            found.extend(events(child, name))
    elif isinstance(value, list):
        for child in value:
            found.extend(events(child, name))
    return found

completed = events(first, "upload_completed")
accepted = events(first, "upload_accepted")
first_available = events(first, "upload_resource_available")
second_available = events(second, "upload_resource_available")
if not completed or not accepted or not first_available or not second_available:
    raise SystemExit("upload completion/fetch evidence was incomplete")
if not any(item.get("primitive") == "channel" for item in accepted):
    raise SystemExit("upload did not use the negotiated Channel primitive")
for group in (completed, first_available, second_available):
    if not any(item.get("bytes") == expected_bytes for item in group):
        raise SystemExit("upload byte count did not match the deterministic fixture")

report = {
    "status": "pass",
    "client_application_version": sys.argv[4],
    "server_application_version": sys.argv[5],
    "fixture_bytes": expected_bytes,
    "sender_upload_completed": True,
    "sender_upload_primitive": "channel",
    "sender_resource_fetch_completed": True,
    "second_client_resource_fetch_completed": True,
    "second_client_isolated": True,
    "isolated_loopback": True,
    "topology": sys.argv[7],
}
pathlib.Path(sys.argv[3]).write_text(
    json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8"
)
PY

if [[ -n "$report_path" ]]; then
  mkdir -p -- "$(dirname -- "$report_path")"
  cp -- "$summary" "$report_path"
fi
cat "$summary"
echo "current OMENchat upload Channel (Resource fetch): pass"
