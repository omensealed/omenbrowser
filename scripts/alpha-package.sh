#!/usr/bin/env bash
set -euo pipefail

out_root="${1:-dist}"
timestamp="$(date -u +%Y%m%dT%H%M%SZ)"
version="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1)"
target_dir="${out_root%/}/OMENbrowser_rs-alpha-${version:-unknown}-${timestamp}"

echo "== Building OMENbrowser_rs release =="
cargo build --release --features chat-client-rns

echo "== Building omenchatd release =="
cargo build --release --manifest-path src/server/Cargo.toml --features live-rns-net

echo "== Staging alpha package =="
mkdir -p "$target_dir/bin" "$target_dir/docs" "$target_dir/scripts" "$target_dir/src-server" "$target_dir/packaging/systemd"

cp target/release/omenbrowser_rs "$target_dir/bin/"
cp src/server/target/release/omenchatd "$target_dir/bin/"
cp README.md "$target_dir/"
cp TESTERS.md "$target_dir/"
cp docs/README.md "$target_dir/docs/"
cp docs/QUICKSTART.md "$target_dir/docs/"
cp docs/TESTING.md "$target_dir/docs/"
cp docs/OMENCHAT.md "$target_dir/docs/"
cp docs/OMENCHAT_PROTOCOL.md "$target_dir/docs/"
cp docs/CONFIGURATION.md "$target_dir/docs/"
cp docs/TROUBLESHOOTING.md "$target_dir/docs/"
cp scripts/alpha-collect.sh "$target_dir/scripts/"
cp scripts/alpha-omenchat-smoke.sh "$target_dir/scripts/"
cp scripts/alpha-root-sanity.sh "$target_dir/scripts/"
cp scripts/install-alpha.sh "$target_dir/scripts/"
cp scripts/install-omenbrowser-user-launchers.sh "$target_dir/scripts/"
cp scripts/install-omenchatd-user-service.sh "$target_dir/scripts/"
cp src/server/README.md "$target_dir/src-server/README.md"
cp packaging/systemd/omenchatd.service.in "$target_dir/packaging/systemd/"

cat > "$target_dir/ALPHA-START.txt" <<'EOF'
OMENbrowser_rs public alpha bundle

Show build identity:

  ./bin/omenbrowser_rs --version
  ./bin/omenchatd --version

Start an isolated test browser:

  ./bin/omenbrowser_rs --desktop --app-root /tmp/omenbrowser-rs-alpha

For a second local client, use a separate app root:

  ./bin/omenbrowser_rs --desktop --app-root /tmp/omenbrowser-rs-alpha-2

Only start the default browser profile if you intentionally want to use your
normal OMENbrowser_rs identity/storage:

  ./bin/omenbrowser_rs --desktop

Initialize a standalone OMENchat server:

  ./bin/omenchatd init --home /tmp/omenchatd-alpha

Attach that server to a backbone TCP gateway:

  ./bin/omenchatd interfaces tcp-client <gateway-host:port> --home /tmp/omenchatd-alpha

For IFAC-protected gateways:

  ./bin/omenchatd interfaces tcp-client <gateway-host:port> \
    --home /tmp/omenchatd-alpha \
    --network-name <network-name> \
    --passphrase <passphrase>

Start the OMENchat server TUI:

  ./bin/omenchatd tui --home /tmp/omenchatd-alpha

In the TUI, press `g` to start the live server, `c` for Monitoring, `l` for
Logs, Tab/Shift+Tab to change panels, and `q` to quit. If the gateway was not
configured before launch, use the Interfaces panel or press `w` to write a
Connect To Gateway config.

Run the OMENchat server:

  ./bin/omenchatd run --home /tmp/omenchatd-alpha

Optional systemd user service install:

  bash ./scripts/install-omenchatd-user-service.sh \
    --bin "$PWD/bin/omenchatd" \
    --home /tmp/omenchatd-alpha

Optional desktop launcher install:

  bash ./scripts/install-omenbrowser-user-launchers.sh \
    --bin "$PWD/bin/omenbrowser_rs"

Optional combined alpha installer:

  bash ./scripts/install-alpha.sh

Show copyable OMENchat and NomadNet portal addresses:

  ./bin/omenchatd status --home /tmp/omenchatd-alpha

Collect a redacted issue bundle:

  bash ./scripts/alpha-collect.sh \
    --browser-root /tmp/omenbrowser-rs-alpha \
    --browser-root-2 /tmp/omenbrowser-rs-alpha-2 \
    --server-home /tmp/omenchatd-alpha

The collector prints the bundle directory path. Review that directory, then
attach it or archive it when reporting a failure.

Run a local isolated OMENchat server/client smoke:

  bash ./scripts/alpha-omenchat-smoke.sh

Run the stronger two-client smoke with recent-history verification:

  bash ./scripts/alpha-omenchat-smoke.sh --multi-client

Check that browser/server test roots are distinct before multi-client testing:

  bash ./scripts/alpha-root-sanity.sh \
    --browser-root /tmp/omenbrowser-rs-alpha \
    --browser-root-2 /tmp/omenbrowser-rs-alpha-2 \
    --server-home /tmp/omenchatd-alpha

Expected smoke result:

  outcome: pass
  reason: OMENchat Link opened, room joined, and message echo was observed

During live testing, use OMENbrowser_rs Monitoring for Runtime Attribution and
OMENchat link health. Use omenchatd TUI Monitoring for active-link rates and
noisy-client flags.

The smoke writes a timestamped report directory under:

  /tmp/omenbrowser-rs-omenchat-smoke/

Read TESTERS.md first.
Read docs/QUICKSTART.md for the fastest build/run path.
Read docs/TESTING.md before testing with real identities.
Read docs/OMENCHAT.md for chat server/client setup.
EOF

cat > "$target_dir/PACKAGE-METADATA.txt" <<EOF
created_utc: $timestamp
version: ${version:-unknown}
host: $(uname -a)
browser_features: chat-client-rns
server_features: live-rns-net
EOF

echo "== Verifying staged binaries =="
"$target_dir/bin/omenbrowser_rs" --help > "$target_dir/omenbrowser_rs-help.txt"
"$target_dir/bin/omenchatd" --help > "$target_dir/omenchatd-help.txt"
"$target_dir/bin/omenbrowser_rs" --version > "$target_dir/omenbrowser_rs-version.txt"
"$target_dir/bin/omenchatd" --version > "$target_dir/omenchatd-version.txt"
grep -q "OMENbrowser_rs ${version:-unknown}" "$target_dir/omenbrowser_rs-version.txt"
grep -q "omenchatd" "$target_dir/omenchatd-version.txt"

echo "== Verifying isolated omenchatd init/status =="
selfcheck_home="$(mktemp -d "${TMPDIR:-/tmp}/omenchatd-alpha-package-selfcheck.XXXXXX")"
"$target_dir/bin/omenchatd" init --home "$selfcheck_home" > "$target_dir/omenchatd-init-selfcheck.txt"
"$target_dir/bin/omenchatd" status --home "$selfcheck_home" > "$target_dir/omenchatd-status-selfcheck.txt"
"$target_dir/bin/omenchatd" doctor --home "$selfcheck_home" > "$target_dir/omenchatd-doctor-selfcheck.txt"
test -f "$selfcheck_home/config.toml"
test -f "$selfcheck_home/identity"
test -f "$selfcheck_home/omenchat.sqlite"
test -d "$selfcheck_home/reticulum"
test -f "$selfcheck_home/reticulum/config"
grep -q 'client uri: omenchat://' "$target_dir/omenchatd-status-selfcheck.txt"
grep -q 'portal url: ' "$target_dir/omenchatd-status-selfcheck.txt"
grep -q 'reticulum/storage/pages/index.mu' "$target_dir/omenchatd-status-selfcheck.txt"
grep -q 'omenchatd doctor:' "$target_dir/omenchatd-doctor-selfcheck.txt"
if grep -q '(missing)' "$target_dir/omenchatd-status-selfcheck.txt"; then
  echo "omenchatd package self-check failed: status reported a missing file" >&2
  exit 1
fi
rm -rf "$selfcheck_home"

(
  cd "$target_dir"
  sha256sum \
    bin/omenbrowser_rs \
    bin/omenchatd \
    README.md \
    TESTERS.md \
    ALPHA-START.txt \
    PACKAGE-METADATA.txt \
    omenbrowser_rs-help.txt \
    omenchatd-help.txt \
    omenbrowser_rs-version.txt \
    omenchatd-version.txt \
    omenchatd-init-selfcheck.txt \
    omenchatd-status-selfcheck.txt \
    omenchatd-doctor-selfcheck.txt \
    docs/README.md \
    docs/QUICKSTART.md \
    docs/TESTING.md \
    docs/OMENCHAT.md \
    docs/OMENCHAT_PROTOCOL.md \
    docs/CONFIGURATION.md \
    docs/TROUBLESHOOTING.md \
    scripts/alpha-collect.sh \
    scripts/alpha-omenchat-smoke.sh \
    scripts/alpha-root-sanity.sh \
    scripts/install-alpha.sh \
    scripts/install-omenbrowser-user-launchers.sh \
    scripts/install-omenchatd-user-service.sh \
    packaging/systemd/omenchatd.service.in \
    src-server/README.md \
    > SHA256SUMS
)

echo "== Creating archive =="
archive_path="${target_dir}.tar.gz"
tar -C "$(dirname "$target_dir")" -czf "$archive_path" "$(basename "$target_dir")"
sha256sum "$archive_path" > "${archive_path}.sha256"
archive_sha="$(sha256sum "$archive_path" | awk '{print $1}')"

echo "== Verifying archive extraction =="
extract_root="$(mktemp -d "${TMPDIR:-/tmp}/omenbrowser-alpha-package-extract.XXXXXX")"
tar -C "$extract_root" -xzf "$archive_path"
extracted_dir="$extract_root/$(basename "$target_dir")"
test -x "$extracted_dir/bin/omenbrowser_rs"
test -x "$extracted_dir/bin/omenchatd"
"$extracted_dir/bin/omenbrowser_rs" --help > /dev/null
"$extracted_dir/bin/omenchatd" --help > /dev/null
"$extracted_dir/bin/omenbrowser_rs" --version > "$extract_root/omenbrowser_rs-version.txt"
"$extracted_dir/bin/omenchatd" --version > "$extract_root/omenchatd-version.txt"
grep -q "OMENbrowser_rs ${version:-unknown}" "$extract_root/omenbrowser_rs-version.txt"
grep -q "omenchatd" "$extract_root/omenchatd-version.txt"
test -f "$extracted_dir/scripts/alpha-collect.sh"
test -f "$extracted_dir/scripts/alpha-omenchat-smoke.sh"
test -f "$extracted_dir/scripts/alpha-root-sanity.sh"
test -f "$extracted_dir/scripts/install-alpha.sh"
test -f "$extracted_dir/scripts/install-omenbrowser-user-launchers.sh"
test -f "$extracted_dir/scripts/install-omenchatd-user-service.sh"
test -f "$extracted_dir/packaging/systemd/omenchatd.service.in"
bash -n "$extracted_dir/scripts/alpha-omenchat-smoke.sh"
bash -n "$extracted_dir/scripts/alpha-root-sanity.sh"
bash -n "$extracted_dir/scripts/install-alpha.sh"
bash -n "$extracted_dir/scripts/install-omenbrowser-user-launchers.sh"
bash -n "$extracted_dir/scripts/install-omenchatd-user-service.sh"
bash "$extracted_dir/scripts/alpha-root-sanity.sh" \
  --browser-root "$extract_root/browser-a" \
  --browser-root-2 "$extract_root/browser-b" \
  --server-home "$extract_root/server-a" \
  > "$extract_root/root-sanity.txt"
grep -q 'root sanity: pass' "$extract_root/root-sanity.txt"
collector_browser_root="$extract_root/browser-root"
collector_browser_root_2="$extract_root/browser-root-2"
collector_server_home="$extract_root/server-home"
collector_out="$extract_root/collector-out"
mkdir -p "$collector_browser_root/logs" "$collector_browser_root_2/logs" "$collector_server_home/logs"
printf 'browser alpha smoke log\n' > "$collector_browser_root/logs/runtime.log"
printf 'browser alpha smoke log 2\n' > "$collector_browser_root_2/logs/runtime.log"
printf 'server alpha smoke log\n' > "$collector_server_home/logs/runtime.log"
collector_bundle="$(
  cd "$extracted_dir"
  bash "scripts/alpha-collect.sh" \
    --browser-root "$collector_browser_root" \
    --browser-root-2 "$collector_browser_root_2" \
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
extracted_home="$(mktemp -d "${TMPDIR:-/tmp}/omenchatd-alpha-package-extracted.XXXXXX")"
"$extracted_dir/bin/omenchatd" init --home "$extracted_home" > /dev/null
"$extracted_dir/bin/omenchatd" status --home "$extracted_home" > "$extract_root/omenchatd-status.txt"
"$extracted_dir/bin/omenchatd" doctor --home "$extracted_home" > "$extract_root/omenchatd-doctor.txt"
test -f "$extracted_home/reticulum/config"
grep -q 'client uri: omenchat://' "$extract_root/omenchatd-status.txt"
grep -q 'portal url: ' "$extract_root/omenchatd-status.txt"
grep -q 'reticulum/storage/pages/index.mu' "$extract_root/omenchatd-status.txt"
grep -q 'omenchatd doctor:' "$extract_root/omenchatd-doctor.txt"
if grep -q '(missing)' "$extract_root/omenchatd-status.txt"; then
  echo "omenchatd extracted package self-check failed: status reported a missing file" >&2
  exit 1
fi
rm -rf "$extracted_home" "$extract_root"

echo "== Updating latest alpha package copy =="
latest_archive="${out_root%/}/OMENbrowser_rs-alpha-latest.tar.gz"
latest_manifest="${out_root%/}/OMENbrowser_rs-alpha-latest.txt"
cp -f "$archive_path" "$latest_archive"
sha256sum "$latest_archive" > "${latest_archive}.sha256"
cat > "$latest_manifest" <<EOF
created_utc: $timestamp
version: ${version:-unknown}
package_dir: $target_dir
archive: $archive_path
archive_sha256: $archive_sha
latest_archive: $latest_archive
latest_archive_sha256_file: ${latest_archive}.sha256
EOF

echo "$target_dir"
echo "$archive_path"
echo "$latest_archive"
echo "$latest_manifest"
