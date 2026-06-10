#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
repo_or_package_root="$(cd -- "$script_dir/.." && pwd)"

browser_bin="${OMENBROWSER_BIN:-$repo_or_package_root/bin/omenbrowser_rs}"
if [[ ! -x "$browser_bin" ]]; then
  browser_bin="$repo_or_package_root/target/release/omenbrowser_rs"
fi

omenchatd_bin="${OMENCHATD_BIN:-$repo_or_package_root/bin/omenchatd}"
if [[ ! -x "$omenchatd_bin" ]]; then
  omenchatd_bin="$repo_or_package_root/src/server/target/release/omenchatd"
fi

browser_root="${OMENBROWSER_ALPHA_ROOT:-/tmp/omenbrowser-rs-alpha}"
browser_root_2="${OMENBROWSER_ALPHA_ROOT_2:-/tmp/omenbrowser-rs-alpha-2}"
server_home="${OMENCHATD_ALPHA_HOME:-/tmp/omenchatd-alpha}"
launcher_dir="${XDG_DATA_HOME:-$HOME/.local/share}/applications"
unit_name="omenchatd"

install_launchers=1
install_second_launcher=0
install_default_launcher=0
install_server_service=0
enable_server_service=0
start_server_service=0
uninstall=0
dry_run=0

usage() {
  cat <<'USAGE'
usage: bash scripts/install-alpha.sh [options]

Install or remove the optional public-alpha user integrations.

Default install:
  - installs the isolated OMENbrowser_rs Alpha desktop launcher only

Optional install flags:
  --second-client-launcher   also install a second isolated-client launcher
  --default-profile-launcher also install a launcher for the normal default profile
  --server-service           install the omenchatd systemd user service
  --enable-server-service    also enable the omenchatd user service
  --start-server-service     also start the omenchatd user service

Path options:
  --browser-bin PATH         omenbrowser_rs binary path
  --omenchatd-bin PATH       omenchatd binary path
  --browser-root DIR         isolated alpha browser app root
  --browser-root-2 DIR       second isolated alpha browser app root
  --server-home DIR          isolated omenchatd server home
  --launcher-dir DIR         user applications directory
  --unit NAME                systemd user unit name without .service

Removal:
  --uninstall                remove installed launchers and service unit only

Other:
  --dry-run                  print planned commands without running them
  -h, --help                 show this help

This wrapper never deletes app roots, identities, databases, Reticulum storage,
messages, portal pages, or the normal OMENbrowser_rs profile.
USAGE
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --browser-bin)
      browser_bin="${2:-}"
      shift 2
      ;;
    --omenchatd-bin)
      omenchatd_bin="${2:-}"
      shift 2
      ;;
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
    --launcher-dir)
      launcher_dir="${2:-}"
      shift 2
      ;;
    --unit)
      unit_name="${2:-}"
      shift 2
      ;;
    --second-client-launcher)
      install_second_launcher=1
      shift
      ;;
    --default-profile-launcher)
      install_default_launcher=1
      shift
      ;;
    --server-service)
      install_server_service=1
      shift
      ;;
    --enable-server-service)
      install_server_service=1
      enable_server_service=1
      shift
      ;;
    --start-server-service)
      install_server_service=1
      start_server_service=1
      shift
      ;;
    --uninstall)
      uninstall=1
      shift
      ;;
    --dry-run)
      dry_run=1
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

run_cmd() {
  if [[ "$dry_run" -eq 1 ]]; then
    printf 'dry-run:'
    printf ' %q' "$@"
    printf '\n'
  else
    "$@"
  fi
}

launcher_script="$script_dir/install-omenbrowser-user-launchers.sh"
service_script="$script_dir/install-omenchatd-user-service.sh"

if [[ ! -f "$launcher_script" ]]; then
  echo "missing launcher installer: $launcher_script" >&2
  exit 2
fi

if [[ ! -f "$service_script" ]]; then
  echo "missing service installer: $service_script" >&2
  exit 2
fi

if [[ "$uninstall" -eq 1 ]]; then
  run_cmd bash "$launcher_script" \
    --bin "$browser_bin" \
    --app-root "$browser_root" \
    --app-root-2 "$browser_root_2" \
    --launcher-dir "$launcher_dir" \
    --uninstall

  run_cmd bash "$service_script" \
    --bin "$omenchatd_bin" \
    --home "$server_home" \
    --unit "$unit_name" \
    --uninstall

  cat <<EOF
uninstall complete
preserved browser roots:
  $browser_root
  $browser_root_2
preserved server home:
  $server_home
EOF
  exit 0
fi

launcher_args=(
  "$launcher_script"
  --bin "$browser_bin"
  --app-root "$browser_root"
  --app-root-2 "$browser_root_2"
  --launcher-dir "$launcher_dir"
)

if [[ "$install_second_launcher" -eq 1 ]]; then
  launcher_args+=(--second-client)
fi

if [[ "$install_default_launcher" -eq 1 ]]; then
  launcher_args+=(--default-profile)
fi

run_cmd bash "${launcher_args[@]}"

if [[ "$install_server_service" -eq 1 ]]; then
  service_args=(
    "$service_script"
    --bin "$omenchatd_bin"
    --home "$server_home"
    --unit "$unit_name"
  )
  if [[ "$enable_server_service" -eq 1 ]]; then
    service_args+=(--enable)
  fi
  if [[ "$start_server_service" -eq 1 ]]; then
    service_args+=(--start)
  fi
  run_cmd bash "${service_args[@]}"
else
  cat <<EOF

omenchatd service not installed. To add it later:
  bash scripts/install-alpha.sh --server-service
EOF
fi

cat <<EOF

alpha install complete
browser root: $browser_root
second browser root: $browser_root_2
server home: $server_home
launcher directory: $launcher_dir
EOF
