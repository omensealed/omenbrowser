#!/usr/bin/env bash
set -euo pipefail

mode="${1:-quick}"
package_archive="${2:-}"
package_smoke_out="${3:-/tmp/omenbrowser-rs-alpha-package-check}"

if [[ -z "$package_archive" ]]; then
  if [[ -f "dist/OMENbrowser_rs-alpha-latest.tar.gz" ]]; then
    package_archive="dist/OMENbrowser_rs-alpha-latest.tar.gz"
  else
    package_archive="/tmp/omenbrowser-rs-alpha-dist/OMENbrowser_rs-alpha-latest.tar.gz"
  fi
fi

case "$mode" in
  quick|full|package)
    ;;
  *)
    echo "usage: $0 [quick|full|package] [package-archive] [package-smoke-out]" >&2
    exit 2
    ;;
esac

echo "== OMENbrowser_rs alpha check: $mode =="

if [[ "$mode" == "package" ]]; then
  require_file() {
    local path="$1"
    if [[ ! -f "$path" ]]; then
      echo "missing required package file: ${path#"$package_dir"/}" >&2
      exit 1
    fi
  }

  require_executable() {
    local path="$1"
    if [[ ! -x "$path" ]]; then
      echo "missing or non-executable package file: ${path#"$package_dir"/}" >&2
      exit 1
    fi
  }

  echo "== Package archive exists =="
  if [[ ! -f "$package_archive" ]]; then
    echo "package archive not found: $package_archive" >&2
    exit 1
  fi

  echo "== Package checksum =="
  if [[ -f "${package_archive}.sha256" ]]; then
    sha256sum -c "${package_archive}.sha256"
  else
    echo "warning: checksum file missing: ${package_archive}.sha256" >&2
  fi

  echo "== Package extraction =="
  extract_root="$(mktemp -d "${TMPDIR:-/tmp}/omenbrowser-alpha-check-package.XXXXXX")"
  tar -C "$extract_root" -xzf "$package_archive"
  package_dir="$(find "$extract_root" -maxdepth 1 -type d -name 'OMENbrowser_rs-alpha-*' | head -n 1)"
  if [[ -z "$package_dir" ]]; then
    echo "package extraction did not produce OMENbrowser_rs-alpha-* directory" >&2
    exit 1
  fi

  echo "== Package required files =="
  require_executable "$package_dir/bin/omenbrowser_rs"
  require_executable "$package_dir/bin/omenchatd"
  require_file "$package_dir/README.md"
  require_file "$package_dir/TESTERS.md"
  require_file "$package_dir/ALPHA-START.txt"
  require_file "$package_dir/SHA256SUMS"
  require_file "$package_dir/PACKAGE-METADATA.txt"
  require_file "$package_dir/docs/27-alpha-test-runbook.md"
  require_file "$package_dir/docs/28-alpha-handoff.md"
  require_file "$package_dir/docs/26-omenchat-protocol-v0.1.md"
  require_file "$package_dir/scripts/alpha-collect.sh"
  require_file "$package_dir/scripts/alpha-omenchat-smoke.sh"
  require_file "$package_dir/scripts/alpha-root-sanity.sh"
  require_file "$package_dir/scripts/install-alpha.sh"
  require_file "$package_dir/scripts/install-omenbrowser-user-launchers.sh"
  require_file "$package_dir/scripts/install-omenchatd-user-service.sh"
  require_file "$package_dir/packaging/systemd/omenchatd.service.in"

  echo "== Package script syntax =="
  bash -n "$package_dir/scripts/alpha-collect.sh"
  bash -n "$package_dir/scripts/alpha-omenchat-smoke.sh"
  bash -n "$package_dir/scripts/alpha-root-sanity.sh"
  bash -n "$package_dir/scripts/install-alpha.sh"
  bash -n "$package_dir/scripts/install-omenbrowser-user-launchers.sh"
  bash -n "$package_dir/scripts/install-omenchatd-user-service.sh"

  echo "== Package root sanity helper =="
  bash "$package_dir/scripts/alpha-root-sanity.sh" \
    --browser-root "$extract_root/browser-a" \
    --browser-root-2 "$extract_root/browser-b" \
    --server-home "$extract_root/server-a" \
    > "$extract_root/root-sanity.txt"
  grep -q 'root sanity: pass' "$extract_root/root-sanity.txt"

  echo "== Package binary help =="
  "$package_dir/bin/omenbrowser_rs" --help > /dev/null
  "$package_dir/bin/omenchatd" --help > /dev/null
  "$package_dir/bin/omenbrowser_rs" --version > "$extract_root/omenbrowser_rs-version.txt"
  "$package_dir/bin/omenchatd" --version > "$extract_root/omenchatd-version.txt"
  grep -q 'OMENbrowser_rs ' "$extract_root/omenbrowser_rs-version.txt"
  grep -q 'features=' "$extract_root/omenbrowser_rs-version.txt"
  grep -q 'omenchatd ' "$extract_root/omenchatd-version.txt"
  grep -q 'features=' "$extract_root/omenchatd-version.txt"

  echo "== Package omenchatd isolated init/status =="
  server_home="$(mktemp -d "${TMPDIR:-/tmp}/omenchatd-alpha-check-package.XXXXXX")"
  "$package_dir/bin/omenchatd" init --home "$server_home" > /dev/null
  "$package_dir/bin/omenchatd" status --home "$server_home" > "$extract_root/omenchatd-status.txt"
  "$package_dir/bin/omenchatd" doctor --home "$server_home" > "$extract_root/omenchatd-doctor.txt"
  test -f "$server_home/config.toml"
  test -f "$server_home/identity"
  test -f "$server_home/omenchat.sqlite"
  test -f "$server_home/reticulum/config"
  grep -q 'client uri: omenchat://' "$extract_root/omenchatd-status.txt"
  grep -q 'portal url: ' "$extract_root/omenchatd-status.txt"
  grep -q 'reticulum/storage/pages/index.mu' "$extract_root/omenchatd-status.txt"
  grep -q 'omenchatd doctor:' "$extract_root/omenchatd-doctor.txt"

  echo "== Package collector =="
  browser_root="$extract_root/browser-root"
  collector_server_home="$extract_root/server-home"
  collector_out="$extract_root/collector-out"
  browser_root_2="$extract_root/browser-root-2"
  mkdir -p "$browser_root/logs" "$browser_root_2/logs" "$collector_server_home/logs"
  printf 'browser alpha check log\n' > "$browser_root/logs/runtime.log"
  printf 'browser alpha check log 2\n' > "$browser_root_2/logs/runtime.log"
  printf 'server alpha check log\n' > "$collector_server_home/logs/runtime.log"
  (
    cd "$package_dir"
    collector_bundle="$(
      bash scripts/alpha-collect.sh \
        --browser-root "$browser_root" \
        --browser-root-2 "$browser_root_2" \
        --server-home "$collector_server_home" \
        --out "$collector_out" \
        --tail-lines 1
    )"
    test -f "$collector_bundle/summary.txt"
    test -f "$collector_bundle/browser-tree.txt"
    test -f "$collector_bundle/browser-2-tree.txt"
    test -f "$collector_bundle/server-tree.txt"
    test -f "$collector_bundle/browser-logs.txt"
    test -f "$collector_bundle/browser-2-logs.txt"
    test -f "$collector_bundle/server-logs.txt"
    test -f "$collector_bundle/root-sanity.txt"
    grep -q 'root sanity: pass' "$collector_bundle/root-sanity.txt"
    test -f "$collector_bundle/package-metadata.txt"
    grep -q 'OMENbrowser_rs ' "$collector_bundle/package-metadata.txt"
    grep -q 'omenchatd ' "$collector_bundle/package-metadata.txt"
    test -f "$collector_bundle/omenchatd-service.txt"
    test -f "$collector_bundle/omenchatd-diagnostics.txt"
  )

  echo "== Package OMENchat smoke =="
  (
    cd "$package_dir"
    bash scripts/alpha-omenchat-smoke.sh \
      --out "$package_smoke_out" \
      --tcp 127.0.0.1:42436 \
      --multi-client \
      --keep-roots
  )

  rm -rf "$server_home" "$extract_root"
  echo "== alpha package check complete =="
  exit 0
fi

echo "== Browser format =="
cargo fmt --check

echo "== Browser feature check =="
cargo check --features chat-client-rns

echo "== Browser focused OMENchat tests =="
cargo test --features chat-client-rns \
  live_sync_recent_history_requests_latest_active_room_batch
cargo test --features chat-client-rns \
  omenchat_help_documents_alpha_isolation_and_server_storage
cargo test --features chat-client-rns \
  opening_different_omenchat_destinations_creates_separate_sessions

echo "== omenchatd format =="
cargo fmt --manifest-path src/server/Cargo.toml --check

echo "== omenchatd feature check =="
cargo check --manifest-path src/server/Cargo.toml --features live-rns-net

echo "== omenchatd focused server tests =="
cargo test --manifest-path src/server/Cargo.toml --features live-rns-net \
  history_recent_returns_current_when_client_fingerprint_matches
cargo test --manifest-path src/server/Cargo.toml --features live-rns-net \
  history_recent_returns_bounded_backlog_when_client_fingerprint_differs
cargo test --manifest-path src/server/Cargo.toml --features live-rns-net \
  status_reports_live_destination_hash
cargo test --manifest-path src/server/Cargo.toml --features live-rns-net \
  init_creates_editable_baseline_reticulum_config
cargo test --manifest-path src/server/Cargo.toml --features live-rns-net \
  tcp_client_override_writes_isolated_reticulum_config

if [[ "$mode" == "full" ]]; then
  echo "== Browser full feature tests =="
  cargo test --features chat-client-rns

  echo "== Browser clippy =="
  cargo clippy --features chat-client-rns -- -D warnings

  echo "== omenchatd full feature tests =="
  cargo test --manifest-path src/server/Cargo.toml --features live-rns-net

  echo "== omenchatd clippy =="
  cargo clippy --manifest-path src/server/Cargo.toml --features live-rns-net -- -D warnings
fi

echo "== alpha check complete =="
