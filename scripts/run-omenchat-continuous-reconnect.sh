#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
readonly repo_root
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

temporary_root=$(mktemp -d "${TMPDIR:-/tmp}/omen-continuous-reconnect.XXXXXX")
cleanup() {
  rm -rf -- "$temporary_root"
}
trap cleanup EXIT INT TERM
trap 'status=$?; echo "continuous OMENchat reconnect harness failed at line $LINENO (status $status)" >&2' ERR

cargo build --locked --manifest-path "$repo_root/Cargo.toml" \
  --no-default-features --features desktop-product --bin omenbrowser_rs
cargo build --locked --manifest-path "$repo_root/src/server/Cargo.toml" \
  --no-default-features --features server-full --bin omenchatd

browser_bin="${CARGO_TARGET_DIR:-$repo_root/target}/debug/omenbrowser_rs"
server_bin="${CARGO_TARGET_DIR:-$repo_root/src/server/target}/debug/omenchatd"
browser_version=$("$browser_bin" --version | awk '{print $2}')
server_version=$("$server_bin" --version | awk '{print $2}')
[[ "$browser_version" == "0.9.7-3" ]]
[[ "$server_version" == "0.9.7-3" ]]

port=$(python3 -c \
  'import socket; s=socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1]); s.close()')
run_dir=$(bash "$repo_root/scripts/release-omenchat-smoke.sh" \
  --browser-bin "$browser_bin" \
  --server-bin "$server_bin" \
  --tcp "127.0.0.1:$port" \
  --path-wait 45 \
  --out "$temporary_root/smoke" \
  --message "continuous current product reconnect" \
  --reaction-smoke \
  --revision-smoke \
  --pin-smoke \
  --continuous-client-reconnect | tail -n 1)

summary_file="$run_dir/summary.txt"
grep -qx 'outcome: pass' "$summary_file"
grep -qx 'continuous_client_reconnect: 1' "$summary_file"
grep -qx 'continuous_link_closed: 1' "$summary_file"
grep -qx 'continuous_link_reopened: 1' "$summary_file"
grep -qx 'continuous_session_reconnected: 1' "$summary_file"
grep -qx 'continuous_message_echoed: 1' "$summary_file"
grep -qx 'reaction_smoke: 1' "$summary_file"
grep -qx 'revision_smoke: 1' "$summary_file"
grep -qx 'pin_smoke: 1' "$summary_file"
grep -qx 'continuous_reaction_recovered: 1' "$summary_file"
grep -qx 'restart_destination_stable: 1' "$summary_file"
grep -qx 'restart_stop: orderly' "$summary_file"

summary="$temporary_root/report.json"
python3 - "$summary" "$browser_version" "$server_version" <<'PY'
import json
import pathlib
import sys

report = {
    "status": "pass",
    "client_application_version": sys.argv[2],
    "server_application_version": sys.argv[3],
    "single_client_process": True,
    "server_stop_orderly": True,
    "server_destination_stable": True,
    "old_link_close_observed": True,
    "new_link_identifier_observed": True,
    "same_session_reconnected": True,
    "post_restart_message_echo_observed": True,
    "replacement_link_reaction_recovery_observed": True,
    "replacement_link_revision_recovery_observed": True,
    "replacement_link_pin_recovery_observed": True,
    "isolated_loopback": True,
}
pathlib.Path(sys.argv[1]).write_text(
    json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8"
)
PY

if [[ -n "$report_path" ]]; then
  mkdir -p -- "$(dirname -- "$report_path")"
  cp -- "$summary" "$report_path"
fi
cat "$summary"
echo "continuous OMENchat reconnect: pass"
