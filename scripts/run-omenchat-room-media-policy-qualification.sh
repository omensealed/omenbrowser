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
under_file="$temporary_root/under-limit.bin"
over_file="$temporary_root/over-limit.bin"
truncate -s 65536 "$under_file"
truncate -s 300000 "$over_file"

under_port=$(python3 -c \
  'import socket; s=socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1]); s.close()')
under_run_dir=$(bash "$repo_root/scripts/release-omenchat-smoke.sh" \
  --browser-bin "$browser_bin" \
  --server-bin "$server_bin" \
  --tcp "127.0.0.1:$under_port" \
  --path-wait 10 \
  --out "$temporary_root/under-smoke" \
  --message "current/current room media-policy under-limit qualification" \
  --room-media-policy-smoke 262144 \
  --upload-file "$under_file" \
  --restart-server | tail -n 1)

grep -qx 'outcome: pass' "$under_run_dir/summary.txt"
grep -qx 'room_media_policy_smoke_bytes: 262144' "$under_run_dir/summary.txt"
grep -qx 'room_media_policy_upload_rejection_smoke: 0' "$under_run_dir/summary.txt"
grep -qx 'restart_destination_stable: 1' "$under_run_dir/summary.txt"
grep -qx 'restart_stop: orderly' "$under_run_dir/summary.txt"

over_port=$(python3 -c \
  'import socket; s=socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1]); s.close()')
over_run_dir=$(bash "$repo_root/scripts/release-omenchat-smoke.sh" \
  --browser-bin "$browser_bin" \
  --server-bin "$server_bin" \
  --tcp "127.0.0.1:$over_port" \
  --path-wait 10 \
  --out "$temporary_root/over-smoke" \
  --message "current/current room media-policy over-limit qualification" \
  --room-media-policy-smoke 262144 \
  --upload-file "$over_file" | tail -n 1)

grep -qx 'outcome: pass' "$over_run_dir/summary.txt"
grep -qx 'room_media_policy_smoke_bytes: 262144' "$over_run_dir/summary.txt"
grep -qx 'room_media_policy_upload_rejection_smoke: 1' "$over_run_dir/summary.txt"

disabled_port=$(python3 -c \
  'import socket; s=socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1]); s.close()')
disabled_run_dir=$(bash "$repo_root/scripts/release-omenchat-smoke.sh" \
  --browser-bin "$browser_bin" \
  --server-bin "$server_bin" \
  --tcp "127.0.0.1:$disabled_port" \
  --path-wait 10 \
  --out "$temporary_root/disabled-smoke" \
  --message "current/current disabled room upload qualification" \
  --room-media-policy-smoke 0 \
  --upload-file "$under_file" | tail -n 1)

grep -qx 'outcome: pass' "$disabled_run_dir/summary.txt"
grep -qx 'room_media_policy_smoke_bytes: 0' "$disabled_run_dir/summary.txt"
grep -qx 'room_media_policy_upload_rejection_smoke: 1' "$disabled_run_dir/summary.txt"

summary="$temporary_root/report.json"
python3 - "$under_run_dir" "$over_run_dir" "$disabled_run_dir" "$summary" <<'PY'
import json
import pathlib
import sys

under_run_dir = pathlib.Path(sys.argv[1])
over_run_dir = pathlib.Path(sys.argv[2])
disabled_run_dir = pathlib.Path(sys.argv[3])
under = json.loads((under_run_dir / "omenchat-smoke.json").read_text(encoding="utf-8"))
restarted = json.loads(
    (under_run_dir / "omenchat-smoke-restart.json").read_text(encoding="utf-8")
)
over = json.loads((over_run_dir / "omenchat-smoke.json").read_text(encoding="utf-8"))
disabled = json.loads(
    (disabled_run_dir / "omenchat-smoke.json").read_text(encoding="utf-8")
)
under_stages = {
    stage.get("stage"): stage
    for stage in under.get("stages", [])
    if isinstance(stage, dict) and isinstance(stage.get("stage"), str)
}
restart_stages = {
    stage.get("stage"): stage
    for stage in restarted.get("stages", [])
    if isinstance(stage, dict) and isinstance(stage.get("stage"), str)
}
over_stages = {
    stage.get("stage"): stage
    for stage in over.get("stages", [])
    if isinstance(stage, dict) and isinstance(stage.get("stage"), str)
}
disabled_stages = {
    stage.get("stage"): stage
    for stage in disabled.get("stages", [])
    if isinstance(stage, dict) and isinstance(stage.get("stage"), str)
}
capabilities = under_stages.get("capability_observation", {})
restart_capabilities = restart_stages.get("capability_observation", {})
upload_complete = under_stages.get("upload_complete_wait", {})
upload_fetch = under_stages.get("upload_fetch_wait", {})
rejection = over_stages.get("room_media_policy_upload_rejection_wait", {})
disabled_rejection = disabled_stages.get(
    "room_media_policy_upload_rejection_wait", {}
)
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
    "message_round_trip": under["classification"]["outcome"] == "pass",
    "under_limit_resource_committed": upload_complete.get("ok") is True,
    "under_limit_resource_fetched": upload_fetch.get("ok") is True,
    "restart_projection_recovered": (
        restarted["classification"]["outcome"] == "pass"
        and restart_capabilities.get("room_media_policy_negotiated") is True
        and restart_capabilities.get("room_upload_max_file_bytes") == 262144
    ),
    "over_limit_typed_rejection": (
        over["classification"]["outcome"] == "pass"
        and rejection.get("ok") is True
        and rejection.get("policy_upload_rejected") is True
        and rejection.get("upload_accepted") is False
        and rejection.get("upload_completed") is False
        and rejection.get("committed_upload_seen") is False
    ),
    "over_limit_ledger_clean": (
        over_run_dir / "omenchatd-upload-rejection-doctor-room-media-policy.txt"
    ).is_file(),
    "disabled_typed_rejection": (
        disabled["classification"]["outcome"] == "pass"
        and disabled_rejection.get("ok") is True
        and disabled_rejection.get("policy_upload_rejected") is True
        and disabled_rejection.get("upload_accepted") is False
        and disabled_rejection.get("upload_completed") is False
        and disabled_rejection.get("committed_upload_seen") is False
    ),
    "disabled_ledger_clean": (
        disabled_run_dir
        / "omenchatd-upload-rejection-doctor-room-media-policy.txt"
    ).is_file(),
}
if not all((
    report["room_media_policy_negotiated"],
    report["cumulative_capabilities"],
    report["room_upload_max_file_bytes"] == 262144,
    report["message_round_trip"],
    report["under_limit_resource_committed"],
    report["under_limit_resource_fetched"],
    report["restart_projection_recovered"],
    report["over_limit_typed_rejection"],
    report["over_limit_ledger_clean"],
    report["disabled_typed_rejection"],
    report["disabled_ledger_clean"],
)):
    raise SystemExit("room media-policy process evidence is incomplete")
pathlib.Path(sys.argv[4]).write_text(
    json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8"
)
PY

if [[ -n "$report_path" ]]; then
  mkdir -p -- "$(dirname -- "$report_path")"
  cp -- "$summary" "$report_path"
fi
cat "$summary"
