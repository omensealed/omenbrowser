#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
readonly repo_root
readonly fixture="$repo_root/fixtures/omenchat/v0_6_0_1_wire.rs"
report_path=""
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
    *)
      echo "usage: $0 [--report /path/to/report.json]" >&2
      exit 2
      ;;
  esac
done

[[ -f "$fixture" && ! -L "$fixture" ]]
fixture_bytes=$(wc -c < "$fixture" | tr -d '[:space:]')
[[ "$fixture_bytes" == "873" ]]

temporary_root=$(mktemp -d "${TMPDIR:-/tmp}/omen-current-upload.XXXXXX")
cleanup() {
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
[[ "$browser_version" == "0.9.5-1" ]]
[[ "$server_version" == "0.9.5-1" ]]

port=$(python3 -c \
  'import socket; s=socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1]); s.close()')
run_dir=$(bash "$repo_root/scripts/release-omenchat-smoke.sh" \
  --browser-bin "$browser_bin" \
  --server-bin "$server_bin" \
  --tcp "127.0.0.1:$port" \
  --path-wait 45 \
  --out "$temporary_root/smoke" \
  --message "current product upload smoke" \
  --multi-client \
  --upload-file "$fixture" | tail -n 1)

grep -qx 'outcome: pass' "$run_dir/summary.txt"
grep -qx 'multi_client: 1' "$run_dir/summary.txt"
summary="$temporary_root/report.json"
python3 - "$run_dir/omenchat-smoke.json" "$run_dir/omenchat-smoke-2.json" \
  "$summary" "$browser_version" "$server_version" "$fixture_bytes" <<'PY'
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
first_available = events(first, "upload_resource_available")
second_available = events(second, "upload_resource_available")
if not completed or not first_available or not second_available:
    raise SystemExit("upload completion/fetch evidence was incomplete")
for group in (completed, first_available, second_available):
    if not any(item.get("bytes") == expected_bytes for item in group):
        raise SystemExit("upload byte count did not match the deterministic fixture")

report = {
    "status": "pass",
    "client_application_version": sys.argv[4],
    "server_application_version": sys.argv[5],
    "fixture_bytes": expected_bytes,
    "sender_upload_completed": True,
    "sender_resource_fetch_completed": True,
    "second_client_resource_fetch_completed": True,
    "second_client_isolated": True,
    "isolated_loopback": True,
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
echo "current OMENchat upload Resource: pass"
