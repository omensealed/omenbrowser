#!/usr/bin/env bash
set -euo pipefail

out_root="${1:-dist}"
version="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1)"
version="${version:-0.1.0}"
arch="${DEB_ARCH:-$(dpkg --print-architecture 2>/dev/null || uname -m)}"
pkg_dir="${out_root%/}/deb/omenbrowser-rs_${version}_${arch}"

if ! command -v dpkg-deb >/dev/null 2>&1; then
  echo "dpkg-deb not found. Install dpkg-dev/dpkg tooling before building .deb packages." >&2
  exit 127
fi

echo "== Building release binaries =="
cargo build --release --features chat-client-rns
cargo build --release --manifest-path src/server/Cargo.toml --features live-rns-net

echo "== Staging Debian package =="
rm -rf "$pkg_dir"
mkdir -p \
  "$pkg_dir/DEBIAN" \
  "$pkg_dir/usr/bin" \
  "$pkg_dir/usr/share/applications" \
  "$pkg_dir/usr/share/icons/hicolor/scalable/apps" \
  "$pkg_dir/usr/share/metainfo" \
  "$pkg_dir/usr/share/doc/omenbrowser-rs" \
  "$pkg_dir/usr/lib/systemd/user"

install -m 0755 target/release/omenbrowser_rs "$pkg_dir/usr/bin/omenbrowser_rs"
install -m 0755 src/server/target/release/omenchatd "$pkg_dir/usr/bin/omenchatd"
install -m 0644 README.md "$pkg_dir/usr/share/doc/omenbrowser-rs/README.md"
install -m 0644 TESTERS.md "$pkg_dir/usr/share/doc/omenbrowser-rs/TESTERS.md"
install -m 0644 docs/QUICKSTART.md "$pkg_dir/usr/share/doc/omenbrowser-rs/QUICKSTART.md"
install -m 0644 docs/TESTING.md "$pkg_dir/usr/share/doc/omenbrowser-rs/TESTING.md"
install -m 0644 docs/OMENCHAT.md "$pkg_dir/usr/share/doc/omenbrowser-rs/OMENCHAT.md"
install -m 0644 packaging/systemd/omenchatd.service.in "$pkg_dir/usr/lib/systemd/user/omenchatd.service"
install -m 0644 packaging/linux/omenbrowser-rs.desktop "$pkg_dir/usr/share/applications/omenbrowser-rs.desktop"
install -m 0644 packaging/linux/omenbrowser-rs.svg "$pkg_dir/usr/share/icons/hicolor/scalable/apps/omenbrowser-rs.svg"
install -m 0644 packaging/linux/omenbrowser-rs.appdata.xml "$pkg_dir/usr/share/metainfo/io.github.omensealed.omenbrowser.metainfo.xml"

installed_size="$(du -sk "$pkg_dir/usr" | awk '{print $1}')"
cat > "$pkg_dir/DEBIAN/control" <<EOF
Package: omenbrowser-rs
Version: $version
Section: net
Priority: optional
Architecture: $arch
Maintainer: OMENbrowser_rs maintainers
Installed-Size: $installed_size
Depends: libc6
Description: Reticulum/NomadNet/LXMF browser and OMENchat client/server
 OMENbrowser_rs is a native Rust browser/client for Reticulum, NomadNet,
 LXMF messaging, and OMENchat. The package includes the desktop browser and
 the standalone omenchatd server.
EOF

echo "== Building .deb =="
mkdir -p "${out_root%/}"
deb_path="${out_root%/}/omenbrowser-rs_${version}_${arch}.deb"
dpkg-deb --root-owner-group --build "$pkg_dir" "$deb_path"
sha256sum "$deb_path" > "${deb_path}.sha256"
echo "$deb_path"
