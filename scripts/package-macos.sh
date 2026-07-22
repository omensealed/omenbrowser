#!/usr/bin/env bash
set -euo pipefail

out_dir="${1:-dist}"
lifecycle_smoke="${2:-}"

fail() {
  echo "macOS packaging failed: $*" >&2
  exit 1
}

[[ "$(uname -s)" == "Darwin" ]] || fail "this script must run on macOS"
for tool in cargo codesign hdiutil lipo plutil shasum tar; do
  command -v "$tool" >/dev/null 2>&1 || fail "required tool is missing: $tool"
done
if [[ -n "$lifecycle_smoke" && "$lifecycle_smoke" != "--run-lifecycle-smoke" ]]; then
  fail "unknown option: $lifecycle_smoke"
fi

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

read_package_version() {
  awk '
    /^\[package\]$/ { in_package = 1; next }
    in_package && /^\[/ { exit }
    in_package && /^version[[:space:]]*=/ {
      value = $0
      sub(/^[^=]*=[[:space:]]*"/, "", value)
      sub(/".*/, "", value)
      print value
      exit
    }
  ' "$1"
}

version="$(read_package_version Cargo.toml)"
server_version="$(read_package_version src/server/Cargo.toml)"
[[ -n "$version" ]] || fail "root package version is missing"
[[ "$server_version" == "$version" ]] \
  || fail "package version mismatch: browser=$version server=$server_version"

if [[ "$version" =~ ^([0-9]+)\.([0-9]+)\.([0-9]+)-([0-9]+)$ ]]; then
  version_major="${BASH_REMATCH[1]}"
  version_minor="${BASH_REMATCH[2]}"
  version_patch="${BASH_REMATCH[3]}"
  version_revision="${BASH_REMATCH[4]}"
  (( version_major <= 9 && version_minor <= 9 && version_patch <= 9 \
      && version_revision <= 99 )) \
    || fail "release version exceeds the documented macOS numeric mapping: $version"
  bundle_short_version="$version_major.$version_minor.$version_patch"
  bundle_build_version="$((version_major * 1000 + version_minor * 100 + version_patch)).$version_revision"
else
  fail "release version is not numeric revision SemVer: $version"
fi

host_target="$(rustc -vV | sed -n 's/^host: //p')"
case "$host_target" in
  x86_64-apple-darwin)
    artifact_arch="x86_64"
    macho_arch="x86_64"
    ;;
  aarch64-apple-darwin)
    artifact_arch="aarch64"
    macho_arch="arm64"
    ;;
  *)
    fail "unsupported macOS package host: $host_target"
    ;;
esac

echo "== Building macOS desktop product =="
cargo build --release --locked --no-default-features \
  --features desktop-product --bin omenbrowser_rs

echo "== Building standalone macOS omenchatd =="
cargo build --release --locked --manifest-path src/server/Cargo.toml \
  --no-default-features --features server-full --bin omenchatd

browser_binary="$repo_root/target/release/omenbrowser_rs"
server_binary="$repo_root/src/server/target/release/omenchatd"
[[ -x "$browser_binary" ]] || fail "browser release binary is missing"
[[ -x "$server_binary" ]] || fail "omenchatd release binary is missing"

browser_identity="$($browser_binary --version)"
server_identity="$($server_binary --version)"
for required in \
  "OMENbrowser_rs $version" \
  "target=$host_target" \
  "desktop-product:on" \
  "native-network:on" \
  "mock-runtime:off"; do
  [[ "$browser_identity" == *"$required"* ]] \
    || fail "browser identity is missing: $required"
done
for required in \
  "omenchatd $version" \
  "server-full:on" \
  "live-reticulum:on"; do
  [[ "$server_identity" == *"$required"* ]] \
    || fail "omenchatd identity is missing: $required"
done

[[ "$(lipo -archs "$browser_binary")" == "$macho_arch" ]] \
  || fail "browser binary architecture does not match $macho_arch"
[[ "$(lipo -archs "$server_binary")" == "$macho_arch" ]] \
  || fail "omenchatd binary architecture does not match $macho_arch"

resolved_out="$(mkdir -p "$out_dir" && cd "$out_dir" && pwd)"
temporary_root="$(mktemp -d "${TMPDIR:-/tmp}/omenbrowser-macos-package.XXXXXX")"
mount_point="$temporary_root/mount"
mounted=0
cleanup() {
  if [[ "$mounted" == 1 ]]; then
    hdiutil detach "$mount_point" -quiet -force >/dev/null 2>&1 || true
  fi
  rm -rf "$temporary_root"
}
trap cleanup EXIT INT TERM

app_stage="$temporary_root/app-stage"
app_bundle="$app_stage/OMENbrowser_rs.app"
contents="$app_bundle/Contents"
mkdir -p "$contents/MacOS" "$contents/Resources"
install -m 0755 "$browser_binary" "$contents/MacOS/omenbrowser_rs"
install -m 0644 README.md "$contents/Resources/README.md"
install -m 0644 TESTERS.md "$contents/Resources/TESTERS.md"
install -m 0644 docs/QUICKSTART.md "$contents/Resources/QUICKSTART.md"
install -m 0644 assets/fonts/adwaita/OFL.txt "$contents/Resources/ADWAITA_MONO_OFL.txt"

cat > "$contents/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleDevelopmentRegion</key><string>en</string>
  <key>CFBundleDisplayName</key><string>OMENbrowser_rs</string>
  <key>CFBundleExecutable</key><string>omenbrowser_rs</string>
  <key>CFBundleIdentifier</key><string>org.omensealed.omenbrowser</string>
  <key>CFBundleInfoDictionaryVersion</key><string>6.0</string>
  <key>CFBundleName</key><string>OMENbrowser_rs</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>$bundle_short_version</string>
  <key>CFBundleVersion</key><string>$bundle_build_version</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>NSHighResolutionCapable</key><true/>
</dict>
</plist>
EOF
plutil -lint "$contents/Info.plist" >/dev/null

cat > "$contents/Resources/PACKAGE-METADATA.txt" <<EOF
version: $version
target: $host_target
profile: desktop-product
architecture: $macho_arch
developer_id_signed: false
notarized: false
omenchatd_bundled: false
EOF

dmg_stage="$temporary_root/dmg-stage"
mkdir -p "$dmg_stage"
cp -R "$app_bundle" "$dmg_stage/OMENbrowser_rs.app"
ln -s /Applications "$dmg_stage/Applications"

dmg_path="$resolved_out/OMENbrowser_rs-$version-macos-$artifact_arch-unsigned.dmg"
rm -f "$dmg_path" "$dmg_path.sha256"
hdiutil create \
  -volname "OMENbrowser_rs $version" \
  -srcfolder "$dmg_stage" \
  -format UDZO \
  -ov \
  "$dmg_path" >/dev/null

server_name="omenchatd-$version-macos-$artifact_arch"
server_stage="$temporary_root/$server_name"
mkdir -p "$server_stage"
install -m 0755 "$server_binary" "$server_stage/omenchatd"
install -m 0644 src/server/README.md "$server_stage/README.md"
install -m 0644 docs/OMENCHAT_PROTOCOL.md "$server_stage/OMENCHAT_PROTOCOL.md"
cat > "$server_stage/PACKAGE-METADATA.txt" <<EOF
version: $version
target: $host_target
profile: server-full
architecture: $macho_arch
developer_id_signed: false
notarized: false
service_install: none
EOF
server_archive="$resolved_out/$server_name.tar.gz"
rm -f "$server_archive" "$server_archive.sha256"
tar -C "$temporary_root" -czf "$server_archive" "$server_name"

write_sha256() {
  local path="$1"
  local directory name digest
  directory="$(dirname "$path")"
  name="$(basename "$path")"
  digest="$(shasum -a 256 "$path" | awk '{print $1}')"
  printf '%s  %s\n' "$digest" "$name" > "$directory/$name.sha256"
  (cd "$directory" && shasum -a 256 -c "$name.sha256") >/dev/null
}
write_sha256 "$dmg_path"
write_sha256 "$server_archive"

mkdir -p "$mount_point"
hdiutil attach "$dmg_path" -readonly -nobrowse -mountpoint "$mount_point" >/dev/null
mounted=1
mounted_app="$mount_point/OMENbrowser_rs.app"
mounted_binary="$mounted_app/Contents/MacOS/omenbrowser_rs"
[[ -x "$mounted_binary" ]] || fail "mounted DMG lacks the browser executable"
[[ "$($mounted_binary --version)" == *"OMENbrowser_rs $version"* ]] \
  || fail "mounted application version mismatch"
[[ "$(plutil -extract CFBundleIdentifier raw "$mounted_app/Contents/Info.plist")" \
  == "org.omensealed.omenbrowser" ]] || fail "mounted bundle identifier mismatch"
[[ "$(plutil -extract CFBundleVersion raw "$mounted_app/Contents/Info.plist")" \
  == "$bundle_build_version" ]] || fail "mounted bundle version mismatch"
[[ "$(lipo -archs "$mounted_binary")" == "$macho_arch" ]] \
  || fail "mounted application architecture mismatch"

signature_report="$temporary_root/signature.txt"
if codesign -dvv "$mounted_app" >/dev/null 2>"$signature_report"; then
  grep -q '^Authority=' "$signature_report" \
    && fail "application unexpectedly carries a signing authority"
  if grep -q '^TeamIdentifier=' "$signature_report" \
      && ! grep -q '^TeamIdentifier=not set$' "$signature_report"; then
    fail "application unexpectedly carries a signing team identifier"
  fi
fi

if [[ "$lifecycle_smoke" == "--run-lifecycle-smoke" ]]; then
  for tool in open osascript pgrep seq; do
    command -v "$tool" >/dev/null 2>&1 \
      || fail "$tool is required for lifecycle smoke"
  done
  app_root="$temporary_root/isolated-app-root"
  mkdir -p "$app_root"
  sentinel="$app_root/preserve-after-dmg-smoke.txt"
  printf '%s\n' "DMG lifecycle smoke must preserve isolated user data" > "$sentinel"
  open -n "$mounted_app" --args --desktop --app-root "$app_root"

  app_pid=""
  for _ in $(seq 1 40); do
    app_pid="$(pgrep -f "$mounted_binary" | head -n 1 || true)"
    [[ -n "$app_pid" ]] && break
    sleep 0.25
  done
  [[ -n "$app_pid" ]] || fail "mounted application did not launch"
  sleep 4
  kill -0 "$app_pid" 2>/dev/null || fail "mounted application exited during launch smoke"
  osascript -e 'tell application id "org.omensealed.omenbrowser" to quit'
  for _ in $(seq 1 60); do
    kill -0 "$app_pid" 2>/dev/null || break
    sleep 0.25
  done
  if kill -0 "$app_pid" 2>/dev/null; then
    kill -TERM "$app_pid" 2>/dev/null || true
    fail "mounted application did not perform a normal quit within 15 seconds"
  fi
  [[ -f "$sentinel" ]] || fail "application lifecycle removed isolated user data"
fi

hdiutil detach "$mount_point" -quiet
mounted=0

echo "macOS packages:"
echo "  $dmg_path"
echo "  $server_archive"
echo "target: $host_target"
echo "lifecycle smoke: ${lifecycle_smoke:---not-requested}"
