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

temporary_root=$(mktemp -d "${TMPDIR:-/tmp}/omen-current-nomadnet.XXXXXX")
cleanup() {
  rm -rf -- "$temporary_root"
}
trap cleanup EXIT INT TERM
trap 'status=$?; echo "current NomadNet page harness failed at line $LINENO (status $status)" >&2' ERR

port=$(python3 -c \
  'import socket; s=socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1]); s.close()')
smoke_output="$temporary_root/smoke-output.txt"
if ! OMENBROWSER_SMOKE_ROOT="$temporary_root/smoke" \
  OMENBROWSER_SMOKE_NOMADNET_TCP="127.0.0.1:$port" \
  OMENBROWSER_SMOKE_NOMADNET_PATH_WAIT=45 \
  timeout 900 bash "$repo_root/scripts/smoke/08_nomadnet_page_fetch.sh" \
  > "$smoke_output" 2>&1; then
  cat "$smoke_output" >&2
  exit 1
fi
grep -qx 'RESULT: PASS' "$smoke_output"

raw_report=$(find "$temporary_root/smoke" -type f \
  -name nomadnet-fetch-report.json -print -quit)
[[ -n "$raw_report" && -f "$raw_report" && ! -L "$raw_report" ]]

browser_bin="${CARGO_TARGET_DIR:-$repo_root/target}/release/omenbrowser_rs"
server_bin="${CARGO_TARGET_DIR:-$repo_root/src/server/target}/release/omenchatd"
browser_version=$("$browser_bin" --version | awk '{print $2}')
server_version=$("$server_bin" --version | awk '{print $2}')
[[ "$browser_version" == "0.10.0-1" ]]
[[ "$server_version" == "0.10.0-1" ]]

summary="$temporary_root/report.json"
python3 - "$raw_report" "$summary" "$browser_version" "$server_version" <<'PY'
import json
import pathlib
import sys

raw = json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))
classification = raw.get("classification", {})
fetch = raw.get("live_fetch", {})
metadata = fetch.get("metadata", {})
verdicts = raw.get("verdicts", {})

expected = {
    "classification": (classification.get("outcome"), "pass"),
    "classification_stage": (classification.get("stage"), "live_fetch"),
    "fetch_ok": (fetch.get("ok"), True),
    "network_source": (fetch.get("source"), "Network"),
    "markup_bytes": (fetch.get("markup_bytes"), 309),
    "markup_lines": (fetch.get("markup_lines"), 17),
    "content_type": (metadata.get("content_type"), "text/x-micron"),
    "request_primitive": (metadata.get("native_request_primitive"), "direct-request"),
    "response_nonempty": (metadata.get("native_response_empty"), False),
    "link_setup": (verdicts.get("link_setup", {}).get("status"), "pass"),
    "request_send": (verdicts.get("request_send", {}).get("status"), "pass"),
}
for name, (actual, wanted) in expected.items():
    if actual != wanted:
        raise SystemExit(f"{name} mismatch: expected {wanted!r}, got {actual!r}")

report = {
    "status": "pass",
    "client_application_version": sys.argv[3],
    "server_application_version": sys.argv[4],
    "content_type": metadata["content_type"],
    "markup_bytes": fetch["markup_bytes"],
    "markup_lines": fetch["markup_lines"],
    "network_source": True,
    "request_primitive": metadata["native_request_primitive"],
    "response_nonempty": True,
    "link_setup_passed": True,
    "request_send_passed": True,
    "isolated_loopback": True,
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
echo "current NomadNet direct page request: pass"
