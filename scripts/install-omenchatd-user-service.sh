#!/usr/bin/env bash
set -euo pipefail
umask 077

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
repo_or_package_root="$(cd -- "$script_dir/.." && pwd)"

default_bin="$repo_or_package_root/bin/omenchatd"
if [[ ! -x "$default_bin" ]]; then
  default_bin="$repo_or_package_root/src/server/target/release/omenchatd"
fi

omenchatd_bin="${OMENCHATD_BIN:-$default_bin}"
omenchatd_home="${OMENCHATD_HOME:-$HOME/.omenchatd}"
unit_name="omenchatd"
enable_unit=0
start_unit=0
uninstall_unit=0

usage() {
  cat <<'USAGE'
usage: bash scripts/install-omenchatd-user-service.sh [options]

Install or remove a systemd user service for the standalone omenchatd server.

Options:
  --bin PATH       omenchatd binary path
  --home DIR       omenchatd server home (default: ~/.omenchatd)
  --unit NAME      systemd user unit name without .service (default: omenchatd)
  --enable         run systemctl --user enable after writing the unit
  --start          run systemctl --user start after writing the unit
  --uninstall      stop/disable/remove the user unit, preserving server data
  -h, --help       show this help

The service runs:

  omenchatd run --home <home>

It does not touch ~/.reticulum, ~/.nomadnetwork, or ~/.lxmd. Configure the
server-owned Reticulum gateway before starting the service, for example:

  omenchatd interfaces tcp-client <gateway-host:port> --home <home>

Uninstall removes only the systemd user unit. It preserves <home>, identity,
database, Reticulum config/storage, logs, and portal pages.
USAGE
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --bin)
      omenchatd_bin="${2:-}"
      shift 2
      ;;
    --home)
      omenchatd_home="${2:-}"
      shift 2
      ;;
    --unit)
      unit_name="${2:-}"
      shift 2
      ;;
    --enable)
      enable_unit=1
      shift
      ;;
    --start)
      start_unit=1
      shift
      ;;
    --uninstall)
      uninstall_unit=1
      shift
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

if [[ ! "$unit_name" =~ ^[A-Za-z0-9_.@-]+$ ]]; then
  echo "--unit must contain only letters, numbers, '.', '_', '@', or '-'" >&2
  exit 2
fi

quote_systemd_arg() {
  local value="$1"
  value="${value//\\/\\\\}"
  value="${value//\"/\\\"}"
  value="${value//%/%%}"
  printf '"%s"' "$value"
}

unit_dir="${XDG_CONFIG_HOME:-$HOME/.config}/systemd/user"
unit_path="$unit_dir/${unit_name%.service}.service"
unit_file="$(basename -- "$unit_path")"

reload_user_manager() {
  if command -v systemctl >/dev/null 2>&1; then
    if ! systemctl --user daemon-reload; then
      echo "warning: systemctl --user daemon-reload failed" >&2
    fi
  else
    echo "warning: systemctl not found" >&2
  fi
}

if [[ "$uninstall_unit" -eq 1 ]]; then
  if command -v systemctl >/dev/null 2>&1; then
    systemctl --user stop "$unit_file" 2>/dev/null || true
    systemctl --user disable "$unit_file" 2>/dev/null || true
  else
    echo "warning: systemctl not found; removing unit file only" >&2
  fi
  if [[ -f "$unit_path" ]]; then
    rm -f "$unit_path"
    removed="yes"
  else
    removed="no; unit file was not present"
  fi
  reload_user_manager
  cat <<EOF
uninstalled: $unit_path
removed: $removed
preserved home: $omenchatd_home
EOF
  exit 0
fi

if [[ -z "$omenchatd_bin" || ! -x "$omenchatd_bin" ]]; then
  echo "omenchatd binary is not executable: ${omenchatd_bin:-<empty>}" >&2
  exit 2
fi

omenchatd_bin_dir="$(cd -- "$(dirname -- "$omenchatd_bin")" && pwd)"
omenchatd_bin="$omenchatd_bin_dir/$(basename -- "$omenchatd_bin")"
if [[ -L "$omenchatd_home" ]]; then
  echo "omenchatd home must not be a symbolic link" >&2
  exit 2
fi
mkdir -p "$omenchatd_home"
if [[ ! -d "$omenchatd_home" || -L "$omenchatd_home" ]]; then
  echo "omenchatd home must be a real directory" >&2
  exit 2
fi
chmod 0700 "$omenchatd_home"
omenchatd_home="$(cd -- "$omenchatd_home" && pwd)"

working_dir="$(dirname -- "$omenchatd_bin")"

mkdir -p "$unit_dir"

template="$repo_or_package_root/packaging/systemd/omenchatd.service.in"
if [[ ! -f "$template" ]]; then
  echo "missing service template: $template" >&2
  exit 2
fi

bin_arg="$(quote_systemd_arg "$omenchatd_bin")"
home_arg="$(quote_systemd_arg "$omenchatd_home")"
work_arg="$(quote_systemd_arg "$working_dir")"

sed \
  -e "s#__OMENCHATD_BIN__#$bin_arg#g" \
  -e "s#__OMENCHATD_HOME__#$home_arg#g" \
  -e "s#__WORKING_DIRECTORY__#$work_arg#g" \
  "$template" > "$unit_path"

reload_user_manager

if [[ "$enable_unit" -eq 1 ]]; then
  systemctl --user enable "$unit_file"
fi

if [[ "$start_unit" -eq 1 ]]; then
  systemctl --user start "$unit_file"
fi

cat <<EOF
installed: $unit_path
binary: $omenchatd_bin
home: $omenchatd_home

Useful commands:
  systemctl --user status $unit_file
  systemctl --user start $unit_file
  systemctl --user stop $unit_file
  journalctl --user -u $unit_file -f
  bash scripts/install-omenchatd-user-service.sh --unit ${unit_name%.service} --uninstall
EOF
