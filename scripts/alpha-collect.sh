#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root_dir="$(cd "$script_dir/.." && pwd)"

browser_root="${OMENBROWSER_ALPHA_ROOT:-}"
browser_root_2="${OMENBROWSER_ALPHA_ROOT_2:-}"
server_home="${OMENCHATD_ALPHA_HOME:-}"
out_root="${TMPDIR:-/tmp}/omenbrowser-rs-alpha-bundles"
tail_lines=400
unit_name="${OMENCHATD_UNIT:-omenchatd}"

usage() {
  cat <<'USAGE'
usage: bash scripts/alpha-collect.sh [options]

Collect a redacted alpha issue bundle without copying private identity material,
message databases, message JSON, Reticulum storage blobs, or known-destination
caches.

Options:
  --browser-root DIR    OMENbrowser_rs app root to summarize
  --browser-root-2 DIR  Optional second OMENbrowser_rs app root to summarize
  --server-home DIR     omenchatd home to summarize
  --out DIR             Output bundle parent directory
  --tail-lines N        Log tail lines to include per text log (default: 400)
  --unit NAME           omenchatd systemd user unit name (default: omenchatd)
  -h, --help            Show this help

Environment fallbacks:
  OMENBROWSER_ALPHA_ROOT
  OMENBROWSER_ALPHA_ROOT_2
  OMENCHATD_ALPHA_HOME
  OMENCHATD_UNIT
USAGE
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --browser-root)
      browser_root="${2:-}"
      shift 2
      ;;
    --browser-root-2)
      browser_root_2="${2:-}"
      shift 2
      ;;
    --server-home)
      server_home="${2:-}"
      shift 2
      ;;
    --out)
      out_root="${2:-}"
      shift 2
      ;;
    --tail-lines)
      tail_lines="${2:-}"
      shift 2
      ;;
    --unit)
      unit_name="${2:-}"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "unknown option: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

if ! [[ "$tail_lines" =~ ^[0-9]+$ ]] || [[ "$tail_lines" -lt 1 ]]; then
  echo "--tail-lines must be a positive integer" >&2
  exit 2
fi

if [[ ! "$unit_name" =~ ^[A-Za-z0-9_.@-]+$ ]]; then
  echo "--unit must contain only letters, numbers, '.', '_', '@', or '-'" >&2
  exit 2
fi

timestamp="$(date -u +%Y%m%dT%H%M%SZ)"
bundle_dir="${out_root%/}/alpha-${timestamp}"
mkdir -p "$bundle_dir"

redact_stream() {
  sed -E \
    -e 's#(/[^[:space:]"]*/)?(identity|default_identity)(["[:space:]]|$)#<redacted-identity-path>\3#g' \
    -e 's#(/[^[:space:]"]*/)?known_destinations(["[:space:]]|$)#<redacted-known-destinations>\2#g' \
    -e 's#(/[^[:space:]"]*/)?omenchat\.sqlite(["[:space:]]|$)#<redacted-sqlite>\2#g' \
    -e 's#(/[^[:space:]"]*/)?messages/[^[:space:]"]+\.json#<redacted-message-json>#g' \
    -e 's#message body:[^"]*#message body:<redacted>#g'
}

is_sensitive_path() {
  local path="$1"
  case "$path" in
    */identity|*/default_identity|*/identities/*|*/messages/*.json|*/omenchat.sqlite|*/known_destinations|*/reticulum/storage/*)
      return 0
      ;;
    */target/*|*/.git/*)
      return 0
      ;;
    *)
      return 1
      ;;
  esac
}

write_tree_summary() {
  local label="$1"
  local root="$2"
  local output="$bundle_dir/${label}-tree.txt"

  {
    echo "$label root: $root"
    if [[ -z "$root" || ! -d "$root" ]]; then
      echo "not found"
      return
    fi
    find "$root" -maxdepth 4 -mindepth 1 -print | sort | while IFS= read -r path; do
      if is_sensitive_path "$path"; then
        continue
      fi
      if [[ -d "$path" ]]; then
        printf 'dir  %s\n' "${path#"$root"/}"
      elif [[ -f "$path" ]]; then
        size="$(wc -c < "$path" 2>/dev/null || echo 0)"
        printf 'file %s %s bytes\n' "${path#"$root"/}" "$size"
      fi
    done
  } | redact_stream > "$output"
}

copy_log_tails() {
  local label="$1"
  local root="$2"
  local output="$bundle_dir/${label}-logs.txt"

  {
    echo "$label logs from: $root"
    if [[ -z "$root" || ! -d "$root" ]]; then
      echo "not found"
      return
    fi
    find "$root" -maxdepth 5 -type f \( -name '*.log' -o -name 'runtime*.txt' -o -name 'debug.output' \) -print | sort | while IFS= read -r log; do
      if is_sensitive_path "$log"; then
        continue
      fi
      echo
      echo "== ${log#"$root"/} =="
      tail -n "$tail_lines" "$log" 2>/dev/null || true
    done
  } | redact_stream > "$output"
}

write_package_metadata() {
  local output="$bundle_dir/package-metadata.txt"
  {
    echo "collector_cwd: $(pwd)"
    echo
    echo "== binary versions =="
    if [[ -x "bin/omenbrowser_rs" ]]; then
      "bin/omenbrowser_rs" --version 2>&1 || true
    else
      echo "bin/omenbrowser_rs: missing"
    fi
    if [[ -x "bin/omenchatd" ]]; then
      "bin/omenchatd" --version 2>&1 || true
    else
      echo "bin/omenchatd: missing"
    fi
    echo
    for file in \
      "PACKAGE-METADATA.txt" \
      "OMENbrowser_rs-alpha-latest.txt" \
      "ALPHA-START.txt"; do
      if [[ -f "$file" ]]; then
        echo "== $file =="
        sed -n '1,120p' "$file"
        echo
      fi
    done
    if [[ -f "SHA256SUMS" ]]; then
      echo "== SHA256SUMS entries =="
      sed -n '1,80p' "SHA256SUMS"
    fi
  } | redact_stream > "$output"
}

write_service_status() {
  local output="$bundle_dir/omenchatd-service.txt"
  local unit_file="${unit_name%.service}.service"
  local unit_path="${XDG_CONFIG_HOME:-$HOME/.config}/systemd/user/$unit_file"

  {
    echo "unit: $unit_file"
    echo "unit_path: $unit_path"
    if [[ -f "$unit_path" ]]; then
      echo "unit_file: present"
      echo
      echo "== unit file =="
      sed -n '1,120p' "$unit_path"
    else
      echo "unit_file: missing"
    fi
    echo
    if command -v systemctl >/dev/null 2>&1; then
      echo "== systemctl --user status =="
      systemctl --user status "$unit_file" --no-pager 2>&1 || true
    else
      echo "systemctl: not found"
    fi
  } | redact_stream > "$output"
}

find_omenchatd_bin() {
  if [[ -x "$root_dir/bin/omenchatd" ]]; then
    printf '%s\n' "$root_dir/bin/omenchatd"
  elif [[ -x "$root_dir/src/server/target/release/omenchatd" ]]; then
    printf '%s\n' "$root_dir/src/server/target/release/omenchatd"
  elif [[ -x "$root_dir/src/server/target/debug/omenchatd" ]]; then
    printf '%s\n' "$root_dir/src/server/target/debug/omenchatd"
  elif command -v omenchatd >/dev/null 2>&1; then
    command -v omenchatd
  fi
}

write_server_diagnostics() {
  local output="$bundle_dir/omenchatd-diagnostics.txt"
  local bin
  bin="$(find_omenchatd_bin || true)"

  {
    echo "server_home: ${server_home:-<not provided>}"
    if [[ -z "$server_home" ]]; then
      echo "omenchatd diagnostics: skipped; --server-home was not provided"
      return
    fi
    if [[ ! -d "$server_home" ]]; then
      echo "omenchatd diagnostics: skipped; server home not found"
      return
    fi
    if [[ -z "$bin" ]]; then
      echo "omenchatd diagnostics: skipped; omenchatd binary not found"
      return
    fi

    echo "omenchatd_bin: $bin"
    echo
    echo "== omenchatd status =="
    "$bin" status --home "$server_home" 2>&1 || true
    echo
    echo "== omenchatd doctor =="
    "$bin" doctor --home "$server_home" 2>&1 || true
  } | redact_stream > "$output"
}

write_root_sanity() {
  local output="$bundle_dir/root-sanity.txt"
  local helper="scripts/alpha-root-sanity.sh"

  {
    echo "root sanity helper: $helper"
    if [[ ! -f "$helper" ]]; then
      echo "root sanity: unavailable"
      return
    fi
    if ! bash "$helper" \
      --browser-root "${browser_root:-}" \
      --browser-root-2 "${browser_root_2:-}" \
      --server-home "${server_home:-}"; then
      echo "root sanity helper exited non-zero; inspect failures above"
    fi
  } | redact_stream > "$output"
}

{
  echo "created_utc: $timestamp"
  echo "repo: $(pwd)"
  echo "uname: $(uname -a)"
  echo "browser_root: ${browser_root:-<not provided>}"
  echo "browser_root_2: ${browser_root_2:-<not provided>}"
  echo "server_home: ${server_home:-<not provided>}"
  echo "tail_lines: $tail_lines"
  echo
  echo "This bundle intentionally excludes identity files, message stores,"
  echo "SQLite databases, known-destination caches, and Reticulum storage blobs."
} | redact_stream > "$bundle_dir/summary.txt"

write_tree_summary "browser" "$browser_root"
write_tree_summary "browser-2" "$browser_root_2"
write_tree_summary "server" "$server_home"
copy_log_tails "browser" "$browser_root"
copy_log_tails "browser-2" "$browser_root_2"
copy_log_tails "server" "$server_home"
write_root_sanity
write_package_metadata
write_service_status
write_server_diagnostics

cat > "$bundle_dir/README.txt" <<'README'
Attach this directory when reporting alpha test failures.

Before sharing publicly, skim the files and remove anything you consider
sensitive. This script avoids copying known private stores, but logs may still
contain destination hashes, hostnames, interface names, or user-entered labels.
README

echo "$bundle_dir"
