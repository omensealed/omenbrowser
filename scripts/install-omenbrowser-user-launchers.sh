#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
repo_or_package_root="$(cd -- "$script_dir/.." && pwd)"

default_bin="$repo_or_package_root/bin/omenbrowser_rs"
if [[ ! -x "$default_bin" ]]; then
  default_bin="$repo_or_package_root/target/release/omenbrowser_rs"
fi

browser_bin="${OMENBROWSER_BIN:-$default_bin}"
app_root="${OMENBROWSER_ALPHA_ROOT:-/tmp/omenbrowser-rs-alpha}"
app_root_2="${OMENBROWSER_ALPHA_ROOT_2:-/tmp/omenbrowser-rs-alpha-2}"
launcher_dir="${XDG_DATA_HOME:-$HOME/.local/share}/applications"
install_alpha=1
install_second=0
install_default=0
uninstall=0

usage() {
  cat <<'USAGE'
usage: bash scripts/install-omenbrowser-user-launchers.sh [options]

Install or remove user-level desktop launchers for OMENbrowser_rs.

Options:
  --bin PATH          omenbrowser_rs binary path
  --app-root DIR      isolated alpha app root (default: /tmp/omenbrowser-rs-alpha)
  --app-root-2 DIR    second isolated alpha app root (default: /tmp/omenbrowser-rs-alpha-2)
  --default-profile   also install a launcher for the normal default profile
  --second-client     also install a launcher for the second isolated client
  --launcher-dir DIR  applications directory (default: XDG_DATA_HOME/applications)
  --uninstall         remove the installed launchers, preserving app data
  -h, --help          show this help

The default install creates only the isolated alpha launcher:

  OMENbrowser_rs Alpha

It does not create, delete, or modify identities. It only writes .desktop files.
USAGE
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --bin)
      browser_bin="${2:-}"
      shift 2
      ;;
    --app-root)
      app_root="${2:-}"
      shift 2
      ;;
    --app-root-2)
      app_root_2="${2:-}"
      shift 2
      ;;
    --default-profile)
      install_default=1
      shift
      ;;
    --second-client)
      install_second=1
      shift
      ;;
    --launcher-dir)
      launcher_dir="${2:-}"
      shift 2
      ;;
    --uninstall)
      uninstall=1
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

desktop_quote() {
  local value="$1"
  value="${value//\\/\\\\}"
  value="${value//\"/\\\"}"
  printf '"%s"' "$value"
}

launcher_alpha="$launcher_dir/omenbrowser-rs-alpha.desktop"
launcher_second="$launcher_dir/omenbrowser-rs-alpha-second.desktop"
launcher_default="$launcher_dir/omenbrowser-rs.desktop"

if [[ "$uninstall" -eq 1 ]]; then
  removed=0
  for launcher in "$launcher_alpha" "$launcher_second" "$launcher_default"; do
    if [[ -f "$launcher" ]]; then
      rm -f "$launcher"
      removed=$((removed + 1))
    fi
  done
  cat <<EOF
removed launchers: $removed
preserved app roots:
  $app_root
  $app_root_2
preserved default profile: yes
EOF
  exit 0
fi

if [[ -z "$browser_bin" || ! -x "$browser_bin" ]]; then
  echo "omenbrowser_rs binary is not executable: ${browser_bin:-<empty>}" >&2
  exit 2
fi

browser_bin_dir="$(cd -- "$(dirname -- "$browser_bin")" && pwd)"
browser_bin="$browser_bin_dir/$(basename -- "$browser_bin")"
mkdir -p "$launcher_dir"

write_launcher() {
  local path="$1"
  local name="$2"
  local comment="$3"
  local exec_line="$4"

  cat > "$path" <<EOF
[Desktop Entry]
Type=Application
Name=$name
Comment=$comment
Exec=$exec_line
Terminal=false
Categories=Network;Chat;
StartupNotify=true
EOF
  chmod 0644 "$path"
  echo "installed: $path"
}

write_launcher \
  "$launcher_alpha" \
  "OMENbrowser_rs Alpha" \
  "Run OMENbrowser_rs with an isolated alpha app root" \
  "$(desktop_quote "$browser_bin") --desktop --app-root $(desktop_quote "$app_root")"

if [[ "$install_second" -eq 1 ]]; then
  write_launcher \
    "$launcher_second" \
    "OMENbrowser_rs Alpha 2" \
    "Run a second isolated OMENbrowser_rs alpha client" \
    "$(desktop_quote "$browser_bin") --desktop --app-root $(desktop_quote "$app_root_2")"
fi

if [[ "$install_default" -eq 1 ]]; then
  write_launcher \
    "$launcher_default" \
    "OMENbrowser_rs" \
    "Run OMENbrowser_rs with the default profile" \
    "$(desktop_quote "$browser_bin") --desktop"
fi

cat <<EOF

Launcher directory: $launcher_dir
Binary: $browser_bin

Useful commands:
  update-desktop-database "$launcher_dir" 2>/dev/null || true
  bash scripts/install-omenbrowser-user-launchers.sh --uninstall
EOF
