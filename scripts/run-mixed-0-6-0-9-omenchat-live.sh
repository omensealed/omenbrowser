#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
readonly repo_root
readonly old_commit=${OMEN_MIXED_OLD_COMMIT:-5ba6683055fb6c59111919fbad1ac37f56a4c203}
readonly old_expected_version=${OMEN_MIXED_OLD_VERSION:-0.6.0-1}
readonly old_server_stop_mode=${OMEN_MIXED_OLD_SERVER_STOP_MODE:-sigterm}
readonly current_expected_version=0.9.6-7
readonly current_client_features=${OMEN_MIXED_CURRENT_CLIENT_FEATURES:-desktop-product}
readonly current_server_features=${OMEN_MIXED_CURRENT_SERVER_FEATURES:-server-headless}

case "$old_server_stop_mode" in
  orderly|sigterm) ;;
  *)
    echo "OMEN_MIXED_OLD_SERVER_STOP_MODE must be orderly or sigterm" >&2
    exit 2
    ;;
esac

report_path=""
reverse=0
restart=0
history_resource=0
while (($#)); do
  case "$1" in
    --reverse)
      reverse=1
      shift
      ;;
    --restart)
      restart=1
      shift
      ;;
    --history-resource)
      history_resource=1
      shift
      ;;
    --report)
      if (($# < 2)); then
        echo "--report requires a path" >&2
        exit 2
      fi
      report_path=$2
      shift 2
      ;;
    *)
      echo "usage: $0 [--reverse] [--restart] [--history-resource] [--report /path/to/report.json]" >&2
      exit 2
      ;;
  esac
done

if ((restart && history_resource)); then
  echo "--restart and --history-resource are separate compatibility cases" >&2
  exit 2
fi

temporary_root=$(mktemp -d "${TMPDIR:-/tmp}/omen-mixed-omenchat.XXXXXX")
cleanup() {
  rm -rf -- "$temporary_root"
}
report_error() {
  local status=$1
  local line=$2
  echo "mixed OMENchat live harness failed at line $line (status $status)" >&2
}
trap cleanup EXIT INT TERM
trap 'report_error "$?" "$LINENO"' ERR

old_source="$temporary_root/old-source"
old_target=${OMEN_MIXED_OLD_TARGET_DIR:-$temporary_root/old-target}
mkdir -p -- "$old_source" "$old_target"
git -C "$repo_root" cat-file -e "$old_commit^{commit}"
git -C "$repo_root" archive "$old_commit" | tar -x -C "$old_source"

if ((reverse)); then
  CARGO_TARGET_DIR="$old_target" cargo build --locked \
    --manifest-path "$old_source/Cargo.toml" \
    --no-default-features --features desktop-product --bin omenbrowser_rs
  cargo build --locked --manifest-path "$repo_root/src/server/Cargo.toml" \
    --no-default-features --features "$current_server_features" --bin omenchatd
  browser_bin="$old_target/debug/omenbrowser_rs"
  server_bin="${CARGO_TARGET_DIR:-$repo_root/src/server/target}/debug/omenchatd"
  expected_client_version="$old_expected_version"
  expected_server_version="$current_expected_version"
  direction="${old_expected_version}_client_to_${current_expected_version}_server"
  message="mixed old client to current server"
else
  CARGO_TARGET_DIR="$old_target" cargo build --locked \
    --manifest-path "$old_source/src/server/Cargo.toml" \
    --no-default-features --features server-headless --bin omenchatd
  cargo build --locked --manifest-path "$repo_root/Cargo.toml" \
    --no-default-features --features "$current_client_features" --bin omenbrowser_rs
  browser_bin="${CARGO_TARGET_DIR:-$repo_root/target}/debug/omenbrowser_rs"
  server_bin="$old_target/debug/omenchatd"
  expected_client_version="$current_expected_version"
  expected_server_version="$old_expected_version"
  direction="${current_expected_version}_client_to_${old_expected_version}_server"
  message="mixed current client to hardened old server"
fi

client_version=$("$browser_bin" --version | awk '{print $2}')
server_version=$("$server_bin" --version | awk '{print $2}')
[[ "$client_version" == "$expected_client_version" ]]
[[ "$server_version" == "$expected_server_version" ]]

port=$(python3 -c \
  'import socket; s=socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1]); s.close()')
smoke_parent="$temporary_root/smoke"
mkdir -p -- "$smoke_parent"
smoke_args=()
if ((restart)); then
  smoke_args+=(--restart-server)
fi
if ((history_resource)); then
  smoke_args+=(--multi-client --server-large-batch-threshold-bytes 1)
fi
run_dir=$(bash "$repo_root/scripts/release-omenchat-smoke.sh" \
  --browser-bin "$browser_bin" \
  --server-bin "$server_bin" \
  --tcp "127.0.0.1:$port" \
  --path-wait 45 \
  --out "$smoke_parent" \
  --message "$message" \
  "${smoke_args[@]}" | tail -n 1)

smoke_report="$run_dir/omenchat-smoke.json"
restart_report=""
restart_stop="not-run"
if ((restart)); then
  restart_report="$run_dir/omenchat-smoke-restart.json"
  grep -qx 'restart_server: 1' "$run_dir/summary.txt"
  grep -qx 'restart_destination_stable: 1' "$run_dir/summary.txt"
  restart_stop=$(sed -n 's/^restart_stop: //p' "$run_dir/summary.txt")
  [[ "$restart_stop" == "orderly" || "$restart_stop" == "sigterm" ]]
  if ((reverse)); then
    [[ "$restart_stop" == "orderly" ]]
  else
    [[ "$restart_stop" == "$old_server_stop_mode" ]]
  fi
fi
history_report=""
if ((history_resource)); then
  history_report="$run_dir/omenchat-smoke-2.json"
  grep -qx 'multi_client: 1' "$run_dir/summary.txt"
  grep -qx 'server_large_batch_threshold_bytes: 1' "$run_dir/summary.txt"
fi
summary="$temporary_root/summary.json"
python3 - "$smoke_report" "$summary" "$old_commit" "$client_version" \
  "$server_version" "$direction" "$restart" "$restart_report" \
  "$restart_stop" "$history_resource" "$history_report" "$((1 - reverse))" <<'PY'
import json
import pathlib
import sys

report = json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))
classification = report.get("classification", {})
if classification.get("outcome") != "pass" or classification.get("stage") != "complete":
    raise RuntimeError("mixed OMENchat live smoke did not complete")
stages = {
    stage.get("stage"): stage
    for stage in report.get("stages", [])
    if isinstance(stage, dict) and isinstance(stage.get("stage"), str)
}
required = (
    "runtime_start",
    "link_open",
    "session_open_frames",
    "join_wait",
    "message_send_frame",
    "message_echo_wait",
)
if any(stages.get(stage, {}).get("ok") is not True for stage in required):
    raise RuntimeError("mixed OMENchat live stage evidence was incomplete")
session = report.get("session", {})
room = session.get("room", {})
if room.get("joined") is not True or session.get("event_count", 0) < 1:
    raise RuntimeError("mixed OMENchat live session state was incomplete")

current_client = sys.argv[12] == "1"
if current_client:
    capabilities = stages.get("capability_observation", {})
    if capabilities.get("announcement_rooms_negotiated") is not False:
        raise RuntimeError("adjacent server unexpectedly negotiated announcement rooms")
    if capabilities.get("announcement_policy_bits") is not None:
        raise RuntimeError("adjacent server projected policy without capability negotiation")
    if capabilities.get("moderation_audit_negotiated") is not False:
        raise RuntimeError("adjacent server unexpectedly negotiated moderation audit")
    if capabilities.get("room_media_policy_negotiated") is not False:
        raise RuntimeError("adjacent server unexpectedly negotiated room media policy")
    if capabilities.get("room_upload_max_file_bytes") is not None:
        raise RuntimeError("adjacent server projected a room upload policy")

restart = sys.argv[7] == "1"
if restart:
    restart_report = json.loads(
        pathlib.Path(sys.argv[8]).read_text(encoding="utf-8")
    )
    restart_classification = restart_report.get("classification", {})
    if (
        restart_classification.get("outcome") != "pass"
        or restart_classification.get("stage") != "complete"
    ):
        raise RuntimeError("mixed OMENchat post-restart smoke did not complete")
    restart_stages = {
        stage.get("stage"): stage
        for stage in restart_report.get("stages", [])
        if isinstance(stage, dict) and isinstance(stage.get("stage"), str)
    }
    if any(restart_stages.get(stage, {}).get("ok") is not True for stage in required):
        raise RuntimeError("mixed OMENchat post-restart stage evidence was incomplete")
    restart_session = restart_report.get("session", {})
    restart_room = restart_session.get("room", {})
    if restart_room.get("joined") is not True or restart_session.get("event_count", 0) < 1:
        raise RuntimeError("mixed OMENchat post-restart session state was incomplete")

history_resource = sys.argv[10] == "1"
if history_resource:
    history_report = json.loads(
        pathlib.Path(sys.argv[11]).read_text(encoding="utf-8")
    )
    history_classification = history_report.get("classification", {})
    if (
        history_classification.get("outcome") != "pass"
        or history_classification.get("stage") != "complete"
    ):
        raise RuntimeError("mixed OMENchat history Resource smoke did not complete")

    def contains_event(value, expected):
        if isinstance(value, dict):
            return value.get("event") == expected or any(
                contains_event(child, expected) for child in value.values()
            )
        if isinstance(value, list):
            return any(contains_event(child, expected) for child in value)
        return False

    resource_events = []
    def collect_resource_events(value):
        if isinstance(value, dict):
            if value.get("event") == "resource_data":
                resource_events.append(value)
            for child in value.values():
                collect_resource_events(child)
        elif isinstance(value, list):
            for child in value:
                collect_resource_events(child)

    collect_resource_events(history_report)
    if not any(contains_event(event, "history_prepended") for event in resource_events):
        raise RuntimeError("history was not decoded from an OMENchat Resource event")
    if history_report.get("session", {}).get("event_count", 0) < 2:
        raise RuntimeError("mixed OMENchat history Resource session was incomplete")

summary = {
    "status": "pass",
    "direction": sys.argv[6],
    "old_source_commit": sys.argv[3],
    "client_application_version": sys.argv[4],
    "server_application_version": sys.argv[5],
    "runtime_started": True,
    "link_opened": True,
    "session_opened": True,
    "room_joined": True,
    "message_sent": True,
    "message_echo_observed": True,
    "session_event_count_positive": True,
    "isolated_loopback": True,
}
if current_client:
    summary.update(
        {
            "current_client_legacy_room_projection": True,
            "announcement_rooms_negotiated": False,
            "announcement_policy_bits": None,
            "moderation_audit_negotiated": False,
            "room_media_policy_negotiated": False,
            "room_upload_max_file_bytes": None,
        }
    )
else:
    summary["adjacent_client_completed_against_current_server"] = True
if restart:
    summary.update(
        {
            "server_restarted": True,
            "server_stop_mode": sys.argv[9],
            "server_destination_stable": True,
            "client_state_root_reused": True,
            "post_restart_link_opened": True,
            "post_restart_session_opened": True,
            "post_restart_room_joined": True,
            "post_restart_message_echo_observed": True,
        }
    )
if history_resource:
    summary.update(
        {
            "history_resource_received": True,
            "history_event_decoded_from_resource": True,
            "history_content_observed": True,
            "second_client_isolated": True,
            "isolated_large_batch_threshold_bytes": 1,
        }
    )
pathlib.Path(sys.argv[2]).write_text(
    json.dumps(summary, indent=2, sort_keys=True) + "\n", encoding="utf-8"
)
PY

if [[ -n "$report_path" ]]; then
  mkdir -p -- "$(dirname -- "$report_path")"
  cp -- "$summary" "$report_path"
fi
cat "$summary"
echo "mixed OMENchat $direction: pass"
