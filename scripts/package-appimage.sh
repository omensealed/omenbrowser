#!/usr/bin/env bash
set -euo pipefail

out_root="${1:-dist}"
version="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1)"
version="${version:-0.6.0}"
arch="$(uname -m)"
appdir="${out_root%/}/AppDir"
appimagetool="${APPIMAGETOOL:-appimagetool}"
appimagetool_args=()
if [[ "${APPIMAGE_VALIDATE_APPSTREAM:-0}" != "1" ]]; then
  appimagetool_args+=(--no-appstream)
fi

if ! command -v "$appimagetool" >/dev/null 2>&1; then
  echo "appimagetool not found. Install it or set APPIMAGETOOL=/path/to/appimagetool." >&2
  exit 127
fi

echo "== Building release binaries =="
browser_features="${OMENBROWSER_BROWSER_FEATURES:-chat-client-rns-clean}"
cargo build --release --features "$browser_features"
cargo build --release --manifest-path src/server/Cargo.toml --features live-reticulum
mkdir -p "${out_root%/}"
target/release/omenbrowser_rs --version > "${out_root%/}/omenbrowser_rs-appimage-version.txt"
grep -q "chat-client-rns-clean:on" "${out_root%/}/omenbrowser_rs-appimage-version.txt"
grep -q "native-network:on" "${out_root%/}/omenbrowser_rs-appimage-version.txt"
src/server/target/release/omenchatd --version > "${out_root%/}/omenchatd-appimage-version.txt"
grep -q "live-reticulum:on" "${out_root%/}/omenchatd-appimage-version.txt"

echo "== Staging AppDir =="
rm -rf "$appdir"
mkdir -p \
  "$appdir/usr/bin" \
  "$appdir/usr/share/applications" \
  "$appdir/usr/share/icons/hicolor/scalable/apps" \
  "$appdir/usr/share/metainfo" \
  "$appdir/usr/share/doc/omenbrowser-rs"

install -m 0755 target/release/omenbrowser_rs "$appdir/usr/bin/omenbrowser_rs"
install -m 0755 src/server/target/release/omenchatd "$appdir/usr/bin/omenchatd"
install -m 0644 README.md "$appdir/usr/share/doc/omenbrowser-rs/README.md"
install -m 0644 TESTERS.md "$appdir/usr/share/doc/omenbrowser-rs/TESTERS.md"
install -m 0644 docs/QUICKSTART.md "$appdir/usr/share/doc/omenbrowser-rs/QUICKSTART.md"
install -m 0644 docs/TESTING.md "$appdir/usr/share/doc/omenbrowser-rs/TESTING.md"
install -m 0644 docs/OMENCHAT.md "$appdir/usr/share/doc/omenbrowser-rs/OMENCHAT.md"
install -m 0644 assets/fonts/adwaita/OFL.txt "$appdir/usr/share/doc/omenbrowser-rs/ADWAITA_MONO_OFL.txt"

install -m 0644 packaging/linux/omenbrowser-rs.desktop "$appdir/omenbrowser-rs.desktop"
install -m 0644 packaging/linux/omenbrowser-rs.desktop "$appdir/usr/share/applications/omenbrowser-rs.desktop"
install -m 0644 packaging/linux/omenbrowser-rs.svg "$appdir/omenbrowser-rs.svg"
install -m 0644 packaging/linux/omenbrowser-rs.svg "$appdir/usr/share/icons/hicolor/scalable/apps/omenbrowser-rs.svg"
install -m 0644 packaging/linux/omenbrowser-rs.appdata.xml "$appdir/usr/share/metainfo/io.github.omensealed.omenbrowser.metainfo.xml"

cat > "$appdir/AppRun" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
here="$(dirname "$(readlink -f "$0")")"
exec "$here/usr/bin/omenbrowser_rs" --desktop "$@"
EOF
chmod 0755 "$appdir/AppRun"

echo "== Building AppImage =="
appimage_path="${out_root%/}/OMENbrowser_rs-${version}-${arch}.AppImage"
"$appimagetool" "${appimagetool_args[@]}" "$appdir" "$appimage_path"
(
  cd "$(dirname "$appimage_path")"
  sha256sum "$(basename "$appimage_path")" > "$(basename "$appimage_path").sha256"
)
echo "$appimage_path"
