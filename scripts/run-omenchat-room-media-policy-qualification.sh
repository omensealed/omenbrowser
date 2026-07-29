#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
readonly repo_root
report_path=""
while (($#)); do
  case "$1" in
    --report)
      [[ $# -ge 2 ]] || {
        echo "--report requires a path" >&2
        exit 2
      }
      report_path=$2
      shift 2
      ;;
    *)
      echo "usage: $0 [--report /path/to/report.json]" >&2
      exit 2
      ;;
  esac
done

temporary_root=$(mktemp -d "${TMPDIR:-/tmp}/omen-room-media-policy.XXXXXX")
cleanup() {
  rm -rf -- "$temporary_root"
}
trap cleanup EXIT INT TERM
trap 'status=$?; echo "room media-policy qualification failed at line $LINENO (status $status)" >&2' ERR

cargo build --locked --manifest-path "$repo_root/Cargo.toml" \
  --no-default-features \
  --features desktop-product,omenchat-room-media-policy-qualification \
  --bin omenbrowser_rs
cargo build --locked --manifest-path "$repo_root/src/server/Cargo.toml" \
  --no-default-features \
  --features server-headless,omenchat-room-media-policy-qualification \
  --bin omenchatd

browser_bin="${CARGO_TARGET_DIR:-$repo_root/target}/debug/omenbrowser_rs"
server_bin="${CARGO_TARGET_DIR:-$repo_root/src/server/target}/debug/omenchatd"
port=$(python3 -c \
  'import socket; s=socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1]); s.close()')
run_dir=$(bash "$repo_root/scripts/release-omenchat-smoke.sh" \
  --browser-bin "$browser_bin" \
  --server-bin "$server_bin" \
  --tcp "127.0.0.1:$port" \
  --path-wait 10 \
  --out "$temporary_root/smoke" \
  --message "current/current room media-policy qualification" \
  --room-media-policy-smoke 262144 | tail -n 1)

grep -qx 'outcome: pass' "$run_dir/summary.txt"
grep -qx 'room_media_policy_smoke_bytes: 262144' "$run_dir/summary.txt"

summary="$temporary_root/report.json"
python3 - "$run_dir" "$summary" <<'PY'
import json
import pathlib
import sys

run_dir = pathlib.Path(sys.argv[1])
smoke = json.loads((run_dir / "omenchat-smoke.json").read_text(encoding="utf-8"))
stages = {
    stage.get("stage"): stage
    for stage in smoke.get("stages", [])
    if isinstance(stage, dict) and isinstance(stage.get("stage"), str)
}
capabilities = stages.get("capability_observation", {})
report = {
    "status": "pass",
    "isolated_loopback": True,
    "qualification_feature_only": True,
    "real_link": True,
    "room_media_policy_negotiated": capabilities.get("room_media_policy_negotiated") is True,
    "cumulative_capabilities": (
        capabilities.get("durable_mutations_negotiated") is True
        and capabilities.get("announcement_rooms_negotiated") is True
        and capabilities.get("slow_mode_negotiated") is True
    ),
    "room_upload_max_file_bytes": capabilities.get("room_upload_max_file_bytes"),
    "message_round_trip": smoke["classification"]["outcome"] == "pass",
}
if not all((
    report["room_media_policy_negotiated"],
    report["cumulative_capabilities"],
    report["room_upload_max_file_bytes"] == 262144,
    report["message_round_trip"],
)):
    raise SystemExit("room media-policy process evidence is incomplete")
pathlib.Path(sys.argv[2]).write_text(
    json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8"
)
PY

if [[ -n "$report_path" ]]; then
  mkdir -p -- "$(dirname -- "$report_path")"
  cp -- "$summary" "$report_path"
fi
cat "$summary"
