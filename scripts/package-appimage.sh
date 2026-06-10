#!/usr/bin/env bash
set -euo pipefail

out_root="${1:-dist}"
version="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1)"
version="${version:-0.1.0}"
arch="$(uname -m)"
appdir="${out_root%/}/AppDir"
appimagetool="${APPIMAGETOOL:-appimagetool}"

if ! command -v "$appimagetool" >/dev/null 2>&1; then
  echo "appimagetool not found. Install it or set APPIMAGETOOL=/path/to/appimagetool." >&2
  exit 127
fi

echo "== Building release binaries =="
cargo build --release --features chat-client-rns
cargo build --release --manifest-path src/server/Cargo.toml --features live-rns-net

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
install -m 0644 docs/27-alpha-test-runbook.md "$appdir/usr/share/doc/omenbrowser-rs/alpha-test-runbook.md"
install -m 0644 docs/28-alpha-handoff.md "$appdir/usr/share/doc/omenbrowser-rs/alpha-handoff.md"

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
mkdir -p "${out_root%/}"
appimage_path="${out_root%/}/OMENbrowser_rs-${version}-${arch}.AppImage"
"$appimagetool" "$appdir" "$appimage_path"
sha256sum "$appimage_path" > "${appimage_path}.sha256"
echo "$appimage_path"
