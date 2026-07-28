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

temporary_root=$(mktemp -d "${TMPDIR:-/tmp}/omen-slow-mode-qualification.XXXXXX")
cleanup() {
  rm -rf -- "$temporary_root"
}
trap cleanup EXIT INT TERM
trap 'status=$?; echo "slow-mode qualification failed at line $LINENO (status $status)" >&2' ERR

cargo build --locked --manifest-path "$repo_root/Cargo.toml" \
  --no-default-features \
  --features desktop-product,omenchat-slow-mode-qualification \
  --bin omenbrowser_rs
cargo build --locked --manifest-path "$repo_root/src/server/Cargo.toml" \
  --no-default-features \
  --features server-headless,omenchat-slow-mode-qualification \
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
  --message "current/current slow-mode qualification" \
  --slow-mode-rejection-smoke | tail -n 1)

grep -qx 'outcome: pass' "$run_dir/summary.txt"
grep -qx 'slow_mode_rejection_smoke: 1' "$run_dir/summary.txt"
grep -qx 'slow_mode_seconds: 30' "$run_dir/summary.txt"
grep -qx 'restart_destination_stable: 1' "$run_dir/summary.txt"
grep -qx 'restart_stop: orderly' "$run_dir/summary.txt"

summary="$temporary_root/report.json"
python3 - "$run_dir" "$summary" <<'PY'
import json
import pathlib
import sys

run_dir = pathlib.Path(sys.argv[1])
initial = json.loads((run_dir / "omenchat-smoke.json").read_text(encoding="utf-8"))
rejected = json.loads((run_dir / "omenchat-smoke-restart.json").read_text(encoding="utf-8"))
expired = json.loads((run_dir / "omenchat-smoke-expiry.json").read_text(encoding="utf-8"))
report = {
    "status": "pass",
    "isolated_loopback": True,
    "qualification_feature_only": True,
    "slow_mode_seconds": 30,
    "initial_commit": initial["classification"]["outcome"] == "pass",
    "replacement_link_typed_rejection": rejected["classification"]["outcome"] == "pass",
    "expiry_readmission": expired["classification"]["outcome"] == "pass",
    "server_restart": True,
    "server_destination_stable": True,
}
pathlib.Path(sys.argv[2]).write_text(
    json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8"
)
PY

if [[ -n "$report_path" ]]; then
  mkdir -p -- "$(dirname -- "$report_path")"
  cp -- "$summary" "$report_path"
fi
cat "$summary"
