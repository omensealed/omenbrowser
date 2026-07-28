#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
readonly repo_root
readonly adjacent_commit=${OMEN_ROOM_SHAPE_ADJACENT_COMMIT:-414d8eafd1a845a986032bad993ac9c09cc378e4}
readonly adjacent_version=${OMEN_ROOM_SHAPE_ADJACENT_VERSION:-0.9.6-3}
readonly adjacent_target=${OMEN_ROOM_SHAPE_ADJACENT_TARGET_DIR:-$repo_root/target/mixed-v0.9.6-3-room-shape}

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

temporary_root=$(mktemp -d "${TMPDIR:-/tmp}/omenchat-room-shape.XXXXXX")
cleanup() {
  rm -rf -- "$temporary_root"
}
report_error() {
  local status=$1
  local line=$2
  echo "OMENchat room-shape compatibility failed at line $line (status $status)" >&2
}
trap cleanup EXIT INT TERM
trap 'report_error "$?" "$LINENO"' ERR

git -C "$repo_root" cat-file -e "$adjacent_commit^{commit}"
tag_commit=$(git -C "$repo_root" rev-parse "v${adjacent_version}^{commit}")
if [[ "$tag_commit" != "$adjacent_commit" ]]; then
  echo "adjacent commit does not match immutable v${adjacent_version}" >&2
  exit 1
fi

current_to_adjacent="$temporary_root/current-client-adjacent-server.json"
adjacent_to_current="$temporary_root/adjacent-client-current-server.json"

echo "== Current strict client -> adjacent immutable server (legacy four-field projection) =="
OMEN_MIXED_OLD_COMMIT="$adjacent_commit" \
OMEN_MIXED_OLD_VERSION="$adjacent_version" \
OMEN_MIXED_OLD_TARGET_DIR="$adjacent_target" \
OMEN_MIXED_OLD_SERVER_STOP_MODE=orderly \
  bash "$repo_root/scripts/run-mixed-0-6-0-9-omenchat-live.sh" \
    --report "$current_to_adjacent"

echo "== Adjacent immutable client -> current server (ordinary compatibility) =="
OMEN_MIXED_OLD_COMMIT="$adjacent_commit" \
OMEN_MIXED_OLD_VERSION="$adjacent_version" \
OMEN_MIXED_OLD_TARGET_DIR="$adjacent_target" \
OMEN_MIXED_OLD_SERVER_STOP_MODE=orderly \
  bash "$repo_root/scripts/run-mixed-0-6-0-9-omenchat-live.sh" --reverse \
    --report "$adjacent_to_current"

echo "== Current per-Link four-/five-field shaping regression =="
cargo test --quiet --locked --manifest-path "$repo_root/src/server/Cargo.toml" \
  --no-default-features --features server-headless \
  live::tests::test_enabled_announcement_rooms_shape_join_and_delta_per_authenticated_link \
  --lib -- --exact

browser_bin="${CARGO_TARGET_DIR:-$repo_root/target}/debug/omenbrowser_rs"
server_bin="${CARGO_TARGET_DIR:-$repo_root/src/server/target}/debug/omenchatd"
if [[ ! -x "$browser_bin" || ! -x "$server_bin" ]]; then
  echo "mixed-version builds did not produce the current product binaries" >&2
  exit 1
fi

port=$(python3 -c \
  'import socket; s=socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1]); s.close()')
echo "== Current negotiated client -> current server (five-field projection and replacement Link) =="
current_run_dir=$(
  bash "$repo_root/scripts/release-omenchat-smoke.sh" \
    --browser-bin "$browser_bin" \
    --server-bin "$server_bin" \
    --tcp "127.0.0.1:$port" \
    --path-wait 45 \
    --out "$temporary_root/current-current" \
    --message "room shape compatibility qualification" \
    --announcement-negotiation-smoke \
    --restart-server |
    tail -n 1
)

current_initial="$current_run_dir/omenchat-smoke.json"
current_restart="$current_run_dir/omenchat-smoke-restart.json"
summary="$temporary_root/summary.json"
python3 - "$current_to_adjacent" "$adjacent_to_current" "$current_initial" \
  "$current_restart" "$summary" "$adjacent_commit" "$adjacent_version" <<'PY'
import json
import pathlib
import sys

def load(path):
    return json.loads(pathlib.Path(path).read_text(encoding="utf-8"))

current_to_adjacent = load(sys.argv[1])
adjacent_to_current = load(sys.argv[2])
current_initial = load(sys.argv[3])
current_restart = load(sys.argv[4])

if current_to_adjacent.get("status") != "pass":
    raise SystemExit("current-client/adjacent-server compatibility did not pass")
if current_to_adjacent.get("current_client_legacy_room_projection") is not True:
    raise SystemExit("strict current client did not record the legacy room projection")
if current_to_adjacent.get("announcement_rooms_negotiated") is not False:
    raise SystemExit("adjacent server unexpectedly negotiated announcement rooms")
if current_to_adjacent.get("announcement_policy_bits") is not None:
    raise SystemExit("adjacent server projected announcement policy bits")
if current_to_adjacent.get("moderation_audit_negotiated") is not False:
    raise SystemExit("adjacent server unexpectedly negotiated moderation audit")

if adjacent_to_current.get("status") != "pass":
    raise SystemExit("adjacent-client/current-server compatibility did not pass")
if adjacent_to_current.get("adjacent_client_completed_against_current_server") is not True:
    raise SystemExit("adjacent client did not complete ordinary current-server traffic")

def negotiated_five_field(report):
    if report.get("classification", {}).get("outcome") != "pass":
        return False
    stages = {
        stage.get("stage"): stage
        for stage in report.get("stages", [])
        if isinstance(stage, dict) and isinstance(stage.get("stage"), str)
    }
    capabilities = stages.get("capability_observation", {})
    return (
        capabilities.get("announcement_rooms_negotiated") is True
        and capabilities.get("announcement_policy_observed") is True
        and capabilities.get("announcement_policy_bits") == 1
    )

if not negotiated_five_field(current_initial):
    raise SystemExit("current initial Link did not project negotiated five-field policy")
if not negotiated_five_field(current_restart):
    raise SystemExit("current replacement Link did not project negotiated five-field policy")

summary = {
    "status": "pass",
    "adjacent_release": f"v{sys.argv[7]}",
    "adjacent_commit": sys.argv[6],
    "isolated_loopback": True,
    "current_client_adjacent_server_legacy_four_field": True,
    "adjacent_client_current_server_ordinary_traffic": True,
    "current_server_four_and_five_field_shaping_regression": True,
    "current_current_initial_five_field": True,
    "current_current_replacement_link_five_field": True,
    "capability_fabricated_for_adjacent_peer": False,
    "moderation_audit_fabricated_for_adjacent_peer": False,
}
pathlib.Path(sys.argv[5]).write_text(
    json.dumps(summary, indent=2, sort_keys=True) + "\n",
    encoding="utf-8",
)
PY

if [[ -n "$report_path" ]]; then
  mkdir -p -- "$(dirname -- "$report_path")"
  cp -- "$summary" "$report_path"
fi
cat "$summary"
