#!/usr/bin/env bash
set -euo pipefail

mode="${1:-quick}"
package_archive="${2:-}"
package_smoke_out="${3:-/tmp/omenbrowser-rs-test-package-check}"
browser_features="${OMENBROWSER_BROWSER_FEATURES:-desktop-product}"

case "$package_smoke_out" in
  /*) ;;
  *) package_smoke_out="$PWD/$package_smoke_out" ;;
esac

if [[ -z "$package_archive" ]]; then
  if [[ -f "dist/OMENbrowser_rs-latest.tar.gz" ]]; then
    package_archive="dist/OMENbrowser_rs-latest.tar.gz"
  else
    package_archive="/tmp/omenbrowser-rs-test-dist/OMENbrowser_rs-latest.tar.gz"
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

echo "== OMENbrowser_rs release check: $mode =="
echo "browser features: $browser_features"

require_clean_browser_version() {
  local version_file="$1"
  grep -Eq 'git_commit=[[:xdigit:]]{7,64}([[:space:]]|$)' "$version_file"
  grep -Eq 'target=[^[:space:]]+' "$version_file"
  grep -q 'profile=desktop-product' "$version_file"
  grep -q 'chat-client-reticulum:on' "$version_file"
  grep -q 'native-network:on' "$version_file"
  grep -q 'desktop-product:on' "$version_file"
  grep -q 'mock-runtime:off' "$version_file"
}

require_clean_server_version() {
  local version_file="$1"
  grep -q 'live-reticulum:on' "$version_file"
}

require_headless_server_version() {
  local version_file="$1"
  require_clean_server_version "$version_file"
  grep -q 'server-headless:on' "$version_file"
  grep -q 'server-full:off' "$version_file"
  grep -q 'tui:off' "$version_file"
}

require_full_server_version() {
  local version_file="$1"
  require_clean_server_version "$version_file"
  grep -q 'server-headless:on' "$version_file"
  grep -q 'server-full:on' "$version_file"
  grep -q 'tui:on' "$version_file"
}

if [[ "$mode" == "package" ]]; then
  echo "== Release notes finalized =="
  bash scripts/verify-release-finalization.sh

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
    (
      cd "$(dirname "$package_archive")"
      sha256sum -c "$(basename "$package_archive").sha256"
    )
  else
    echo "warning: checksum file missing: ${package_archive}.sha256" >&2
  fi

  echo "== Package extraction =="
  extract_root="$(mktemp -d "${TMPDIR:-/tmp}/omenbrowser-release-check-package.XXXXXX")"
  tar -C "$extract_root" -xzf "$package_archive"
  package_dir="$(find "$extract_root" -maxdepth 1 -type d -name 'OMENbrowser_rs-*' | head -n 1)"
  if [[ -z "$package_dir" ]]; then
    echo "package extraction did not produce OMENbrowser_rs-* directory" >&2
    exit 1
  fi

  echo "== Package required files =="
  require_executable "$package_dir/bin/omenbrowser_rs"
  require_executable "$package_dir/bin/omenchatd"
  require_file "$package_dir/README.md"
  require_file "$package_dir/TESTERS.md"
  require_file "$package_dir/START.txt"
  require_file "$package_dir/SHA256SUMS"
  require_file "$package_dir/PACKAGE-METADATA.txt"
  require_file "$package_dir/docs/CURRENT_STATUS.md"
  require_file "$package_dir/docs/QUICKSTART.md"
  require_file "$package_dir/docs/TESTING.md"
  require_file "$package_dir/docs/OMENCHAT.md"
  require_file "$package_dir/docs/OMENCHAT_PROTOCOL.md"
  require_file "$package_dir/scripts/release-collect.sh"
  require_file "$package_dir/scripts/release-omenchat-smoke.sh"
  require_file "$package_dir/scripts/release-root-sanity.sh"
  require_file "$package_dir/scripts/install-release.sh"
  require_file "$package_dir/scripts/install-omenbrowser-user-launchers.sh"
  require_file "$package_dir/scripts/install-omenchatd-user-service.sh"
  require_file "$package_dir/packaging/systemd/omenchatd.service.in"

  echo "== Package script syntax =="
  bash -n "$package_dir/scripts/release-collect.sh"
  bash -n "$package_dir/scripts/release-omenchat-smoke.sh"
  bash -n "$package_dir/scripts/release-root-sanity.sh"
  bash -n "$package_dir/scripts/install-release.sh"
  bash -n "$package_dir/scripts/install-omenbrowser-user-launchers.sh"
  bash -n "$package_dir/scripts/install-omenchatd-user-service.sh"

  echo "== Package root sanity helper =="
  bash "$package_dir/scripts/release-root-sanity.sh" \
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
  require_clean_browser_version "$extract_root/omenbrowser_rs-version.txt"
  grep -q 'omenchatd ' "$extract_root/omenchatd-version.txt"
  grep -q 'features=' "$extract_root/omenchatd-version.txt"
  require_full_server_version "$extract_root/omenchatd-version.txt"

  echo "== Package omenchatd isolated init/status =="
  server_home="$(mktemp -d "${TMPDIR:-/tmp}/omenchatd-release-check-package.XXXXXX")"
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
  printf 'passphrase = collector-secret-value\n' > "$browser_root/logs/runtime.log"
  printf 'browser release check log 2\n' > "$browser_root_2/logs/runtime.log"
  printf 'server release check log\n' > "$collector_server_home/logs/runtime.log"
  (
    cd "$package_dir"
    collector_bundle="$(
      bash scripts/release-collect.sh \
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
    grep -q 'passphrase = <redacted-secret>' "$collector_bundle/browser-logs.txt"
    ! grep -R -q 'collector-secret-value' "$collector_bundle"
    if [[ "$(uname -s)" != "Darwin" ]]; then
      test "$(stat -c '%a' "$collector_bundle")" = "700"
      test "$(stat -c '%a' "$collector_bundle/browser-logs.txt")" = "600"
    fi
  )

  echo "== Package OMENchat smoke =="
  (
    cd "$package_dir"
    bash scripts/release-omenchat-smoke.sh \
      --out "$package_smoke_out" \
      --tcp 127.0.0.1:42436 \
      --multi-client \
      --keep-roots
  )

  rm -rf "$server_home" "$extract_root"
  echo "== release package check complete =="
  exit 0
fi

echo "== Browser format =="
cargo fmt --check
bash -n scripts/test-desktop-shutdown.sh
bash -n scripts/measure-desktop-idle.sh
bash -n scripts/compare-desktop-idle.sh
bash -n scripts/measure-low-power-desktop.sh
bash -n scripts/measure-pane-stress.sh
bash -n scripts/measure-durable-mutation-retention.sh
bash -n scripts/measure-omenchatd-backpressure.sh
bash -n scripts/measure-omenchatd-db.sh
bash -n scripts/measure-omenchatd-idle.sh
bash -n scripts/verify-release-version.sh
bash -n scripts/verify-reticulum-train.sh
bash -n scripts/verify-accepted-advisories.sh
bash -n src/server/scripts/verify-standalone.sh
bash -n scripts/verify-tui-dependencies.sh
bash -n scripts/test-tui-lifecycle.sh
bash -n scripts/test-tui-real-pty.sh
bash -n scripts/test-native-cli-identity.sh
bash -n scripts/package-macos.sh
macos_version_mapping="$(
  bash scripts/package-macos.sh --print-version-mapping 0.10.0-4
)"
[[ "$(printf '%s\n' "$macos_version_mapping" | sed -n '1p')" == "0.10.0" ]]
[[ "$(printf '%s\n' "$macos_version_mapping" | sed -n '2p')" == "1000.0.4" ]]
bash -n scripts/package-linux-arm64-omenchatd.sh
bash -n scripts/test-linux-arm64-headless.sh
bash -n scripts/test-omenchatd-private-service.sh
bash -n scripts/verify-private-storage-policy.sh
bash -n scripts/verify-reticulum-resource-compat.sh
bash -n scripts/test-reticulum-resource-compat-verifier.sh
bash -n scripts/verify-reticulum-capability-docs.sh
bash -n scripts/test-reticulum-capability-docs-verifier.sh
bash -n scripts/verify-documentation.sh

echo "== Current documentation =="
bash scripts/verify-documentation.sh

echo "== Private storage policy =="
bash scripts/verify-private-storage-policy.sh

if [[ "$(uname -s)" == "Linux" ]]; then
  echo "== omenchatd private service installer =="
  bash scripts/test-omenchatd-private-service.sh
fi

echo "== TUI dependency check =="
bash scripts/verify-tui-dependencies.sh

echo "== Release version consistency =="
bash scripts/verify-release-version.sh

echo "== Reticulum/LXMF dependency train =="
bash scripts/verify-reticulum-train.sh

echo "== Reticulum 0.10.0 Resource compatibility =="
bash scripts/verify-reticulum-resource-compat.sh

echo "== Reticulum capability documentation =="
bash scripts/verify-reticulum-capability-docs.sh

if [[ "$mode" == "full" ]]; then
  echo "== Reticulum verifier fixture tests =="
  bash scripts/test-reticulum-resource-compat-verifier.sh
  bash scripts/test-reticulum-capability-docs-verifier.sh
fi

echo "== Accepted build-time advisory boundary =="
bash scripts/verify-accepted-advisories.sh --no-fetch

echo "== Native release CLI identity smoke =="
bash scripts/test-native-cli-identity.sh

echo "== TUI lifecycle smoke =="
bash scripts/test-tui-lifecycle.sh
if [[ "$(uname -s)" == "Linux" ]]; then
  echo "== Linux real PTY TUI smoke =="
  bash scripts/test-tui-real-pty.sh
fi

echo "== Browser feature check =="
bash scripts/verify-product-features.sh
cargo check --locked --no-default-features --features native-lxmf
cargo check --locked --no-default-features --features "$browser_features"
cargo run --locked --no-default-features --features "$browser_features" --bin omenbrowser_rs -- --version > /tmp/omenbrowser-rs-test-check-version.txt
require_clean_browser_version /tmp/omenbrowser-rs-test-check-version.txt

echo "== Browser focused OMENchat tests =="
cargo test --locked --no-default-features --features "$browser_features" \
  live_sync_recent_history_requests_latest_active_room_batch
cargo test --locked --no-default-features --features "$browser_features" \
  omenchat_help_documents_release_isolation_and_server_storage
cargo test --locked --no-default-features --features "$browser_features" \
  opening_different_omenchat_destinations_creates_separate_sessions

echo "== omenchatd format =="
cargo fmt --manifest-path src/server/Cargo.toml --check

echo "== omenchatd standalone relocation =="
cmp fixtures/omenchat/v0_6_0_1_wire.rs \
  src/server/fixtures/omenchat/v0_6_0_1_wire.rs
bash src/server/scripts/verify-standalone.sh check

echo "== omenchatd feature check =="
cargo check --locked --manifest-path src/server/Cargo.toml --no-default-features --features server-headless
cargo run --locked --manifest-path src/server/Cargo.toml --no-default-features --features server-headless -- --version > /tmp/omenchatd-test-check-version.txt
require_headless_server_version /tmp/omenchatd-test-check-version.txt
if cargo tree --locked --manifest-path src/server/Cargo.toml --no-default-features --features server-headless | grep -Eq '(^|[[:space:]])(ratatui|crossterm) v'; then
  echo "headless omenchatd dependency graph includes TUI crates" >&2
  exit 1
fi
cargo check --locked --manifest-path src/server/Cargo.toml --no-default-features --features server-full

echo "== omenchatd focused server tests =="
cargo test --locked --manifest-path src/server/Cargo.toml --no-default-features --features server-headless \
  history_recent_returns_current_when_client_fingerprint_matches
cargo test --locked --manifest-path src/server/Cargo.toml --no-default-features --features server-headless \
  history_recent_returns_bounded_backlog_when_client_fingerprint_differs
cargo test --locked --manifest-path src/server/Cargo.toml --no-default-features --features server-headless \
  status_reports_live_destination_hash
cargo test --locked --manifest-path src/server/Cargo.toml --no-default-features --features server-headless \
  init_creates_editable_baseline_reticulum_config
cargo test --locked --manifest-path src/server/Cargo.toml --no-default-features --features server-headless \
  tcp_client_override_writes_isolated_reticulum_config

if [[ "$mode" == "full" ]]; then
  echo "== Browser full feature tests =="
  cargo test --locked --no-default-features --features "$browser_features"

  echo "== Browser clippy =="
  cargo clippy --locked --no-default-features --features "$browser_features" -- -D warnings

  echo "== omenchatd full feature tests =="
  cargo test --locked --manifest-path src/server/Cargo.toml --no-default-features --features server-full

  echo "== omenchatd clippy =="
  cargo clippy --locked --manifest-path src/server/Cargo.toml --no-default-features --features server-full -- -D warnings
fi

echo "== release check complete =="
