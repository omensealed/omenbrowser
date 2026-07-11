#!/usr/bin/env bash
set -euo pipefail

browser_root="${OMENBROWSER_RELEASE_ROOT:-/tmp/omenbrowser-rs-test}"
browser_root_2="${OMENBROWSER_RELEASE_ROOT_2:-/tmp/omenbrowser-rs-test-2}"
server_home="${OMENCHATD_RELEASE_HOME:-/tmp/omenchatd-test}"

usage() {
  cat <<'USAGE'
usage: bash scripts/release-root-sanity.sh [options]

Check that public release browser/server test roots are distinct and isolated.
This does not create, delete, or modify the roots.

Options:
  --browser-root DIR    First OMENbrowser_rs app root
  --browser-root-2 DIR  Optional second OMENbrowser_rs app root
  --server-home DIR     omenchatd server home
  -h, --help            Show this help

Environment fallbacks:
  OMENBROWSER_RELEASE_ROOT
  OMENBROWSER_RELEASE_ROOT_2
  OMENCHATD_RELEASE_HOME
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

canonical_path() {
  local path="$1"
  if [[ -z "$path" ]]; then
    echo ""
    return
  fi
  if [[ -e "$path" ]]; then
    realpath -m "$path"
  else
    realpath -m "$(dirname "$path")/$(basename "$path")"
  fi
}

failures=0

check_non_empty() {
  local label="$1"
  local value="$2"
  if [[ -z "$value" ]]; then
    echo "fail: $label is empty"
    failures=$((failures + 1))
  fi
}

check_non_empty "browser root" "$browser_root"
check_non_empty "server home" "$server_home"

browser_root="$(canonical_path "$browser_root")"
browser_root_2="$(canonical_path "$browser_root_2")"
server_home="$(canonical_path "$server_home")"

echo "browser root:        $browser_root"
echo "second browser root: ${browser_root_2:-<not provided>}"
echo "server home:         $server_home"

if [[ -n "$browser_root_2" && "$browser_root" == "$browser_root_2" ]]; then
  echo "fail: browser roots are identical"
  failures=$((failures + 1))
fi

if [[ "$browser_root" == "$server_home" || ( -n "$browser_root_2" && "$browser_root_2" == "$server_home" ) ]]; then
  echo "fail: a browser root matches the omenchatd server home"
  failures=$((failures + 1))
fi

protected_roots=(
  "$HOME/.reticulum"
  "$HOME/.nomadnetwork"
  "$HOME/.lxmd"
)

for path in "$browser_root" "$browser_root_2" "$server_home"; do
  if [[ -z "$path" ]]; then
    continue
  fi
  for protected in "${protected_roots[@]}"; do
    protected="$(canonical_path "$protected")"
    if [[ "$path" == "$protected" || "$path" == "$protected/"* ]]; then
      echo "fail: $path is inside protected shared Reticulum/NomadNet/LXMF storage $protected"
      failures=$((failures + 1))
    fi
  done
done

default_browser="$(canonical_path "$HOME/.config/OMENbrowser_rs")"
default_server="$(canonical_path "$HOME/.omenchatd")"
if [[ "$browser_root" == "$default_browser" || ( -n "$browser_root_2" && "$browser_root_2" == "$default_browser" ) ]]; then
  echo "warn: a browser root is the normal default OMENbrowser_rs config path"
fi
if [[ "$server_home" == "$default_server" ]]; then
  echo "warn: server home is the normal default omenchatd path"
fi

if [[ "$failures" -ne 0 ]]; then
  echo "root sanity: fail ($failures issue(s))"
  exit 1
fi

echo "root sanity: pass"
