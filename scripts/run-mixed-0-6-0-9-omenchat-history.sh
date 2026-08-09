#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
readonly repo_root
readonly old_commit=${OMEN_MIXED_OLD_COMMIT:-5ba6683055fb6c59111919fbad1ac37f56a4c203}
readonly old_expected_version=${OMEN_MIXED_OLD_VERSION:-0.6.0-1}
readonly current_expected_version=0.9.8-4

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

temporary_root=$(mktemp -d "${TMPDIR:-/tmp}/omen-mixed-history.XXXXXX")
cleanup() {
  rm -rf -- "$temporary_root"
}
trap cleanup EXIT INT TERM
trap 'status=$?; echo "mixed OMENchat history harness failed at line $LINENO (status $status)" >&2' ERR

old_source="$temporary_root/old-source"
old_target=${OMEN_MIXED_OLD_TARGET_DIR:-$temporary_root/old-target}
database_root="$temporary_root/database"
mkdir -p -- "$old_source" "$old_target" "$database_root"

git -C "$repo_root" cat-file -e "$old_commit^{commit}"
git -C "$repo_root" archive "$old_commit" | tar -x -C "$old_source"
mkdir -p -- "$old_source/examples"
cp -- "$repo_root/examples/mixed_sqlite_history_probe.rs" \
  "$old_source/examples/mixed_sqlite_history_probe.rs"

CARGO_TARGET_DIR="$old_target" cargo build --locked \
  --manifest-path "$old_source/Cargo.toml" \
  --no-default-features --features desktop-product \
  --example mixed_sqlite_history_probe
cargo build --locked --manifest-path "$repo_root/Cargo.toml" \
  --no-default-features --features desktop-product \
  --example mixed_sqlite_history_probe

old_probe="$old_target/debug/examples/mixed_sqlite_history_probe"
current_probe="${CARGO_TARGET_DIR:-$repo_root/target}/debug/examples/mixed_sqlite_history_probe"
old_version=$(python3 -c \
  'import sys,tomllib; print(tomllib.load(open(sys.argv[1], "rb"))["package"]["version"])' \
  "$old_source/Cargo.toml")
current_version=$(python3 -c \
  'import sys,tomllib; print(tomllib.load(open(sys.argv[1], "rb"))["package"]["version"])' \
  "$repo_root/Cargo.toml")
[[ "$old_version" == "$old_expected_version" ]]
[[ "$current_version" == "$current_expected_version" ]]

"$old_probe" seed-old "$database_root" >"$temporary_root/seed-old.json"
"$current_probe" reopen-current "$database_root" >"$temporary_root/reopen-current.json"
"$old_probe" reopen-old "$database_root" >"$temporary_root/reopen-old.json"
"$current_probe" final-current "$database_root" >"$temporary_root/final-current.json"

summary="$temporary_root/summary.json"
python3 - "$temporary_root" "$summary" "$old_commit" "$old_version" \
  "$current_version" <<'PY'
import json
import pathlib
import sys

root = pathlib.Path(sys.argv[1])
expected = (
    ("seed-old", 1),
    ("reopen-current", 2),
    ("reopen-old", 3),
    ("final-current", 3),
)
for stage, count in expected:
    report = json.loads((root / f"{stage}.json").read_text(encoding="utf-8"))
    if (
        report.get("status") != "pass"
        or report.get("stage") != stage
        or report.get("events") != count
        or report.get("metadata_verified") is not True
    ):
        raise RuntimeError(f"mixed OMENchat history stage failed: {stage}")

database = root / "database" / "chat.sqlite"
if not database.is_file() or database.stat().st_size <= 0:
    raise RuntimeError("mixed OMENchat history database was not durably materialized")

summary = {
    "status": "pass",
    "old_source_commit": sys.argv[3],
    "old_application_version": sys.argv[4],
    "current_application_version": sys.argv[5],
    "store": "omenchat-sqlite",
    "old_seed_events": 1,
    "current_reopen_events": 1,
    "old_reopen_current_events": 1,
    "final_events": 3,
    "server_metadata_preserved": True,
    "room_metadata_preserved": True,
    "active_room_preserved": True,
    "event_order_preserved": True,
    "event_content_preserved": True,
    "old_reads_current_writes": True,
    "current_reads_old_writes": True,
    "database_nonempty": True,
}
pathlib.Path(sys.argv[2]).write_text(
    json.dumps(summary, indent=2, sort_keys=True) + "\n", encoding="utf-8"
)
PY

if [[ -n "$report_path" ]]; then
  mkdir -p -- "$(dirname -- "$report_path")"
  cp -- "$summary" "$report_path"
fi
cat "$summary"
echo "mixed OMENchat $old_version/$current_version SQLite history reopening: pass"
