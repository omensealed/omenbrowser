#!/usr/bin/env bash
set -euo pipefail

out_root="${1:-dist-compat}"
image="${OMENBROWSER_COMPAT_IMAGE:-rust:1-bullseye}"
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
uid="$(id -u)"
gid="$(id -g)"

runtime=""
userns_args=()
if command -v podman >/dev/null 2>&1; then
  runtime="podman"
  userns_args=(--userns=keep-id)
elif command -v docker >/dev/null 2>&1; then
  runtime="docker"
else
  cat >&2 <<'EOF'
No compatibility container runtime found.

Install podman or docker, then rerun:

  bash scripts/package-deb-compat.sh dist-compat

This wrapper builds inside a Debian 11 / glibc 2.31 Rust container so the
resulting .deb works on older Debian/Ubuntu/Mint systems instead of inheriting
the host distro's newer GLIBC requirement.
EOF
  exit 127
fi

echo "== Building compatibility .deb in ${image} via ${runtime} =="
"$runtime" run --rm \
  "${userns_args[@]}" \
  -e CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}" \
  -e DEB_ARCH="${DEB_ARCH:-amd64}" \
  -e OMENBROWSER_COMPAT_UID="$uid" \
  -e OMENBROWSER_COMPAT_GID="$gid" \
  -v "$repo_root:/work" \
  -w /work \
  "$image" \
  bash -lc '
    set -euo pipefail
    export PATH="/usr/local/cargo/bin:$PATH"
    export DEBIAN_FRONTEND=noninteractive
    apt-get update
    apt-get install -y \
      binutils \
      build-essential \
      ca-certificates \
      dpkg-dev \
      libxkbcommon-dev \
      libwayland-dev \
      libx11-dev \
      libxi-dev \
      libgl1-mesa-dev \
      pkg-config

    bash scripts/package-deb.sh "'"$out_root"'"
    bash scripts/check-glibc-floor.sh 2.31 \
      target/release/omenbrowser_rs \
      src/server/target/release/omenchatd

    chown -R "$OMENBROWSER_COMPAT_UID:$OMENBROWSER_COMPAT_GID" \
      "'"$out_root"'" target src/server/target
  '

echo "== Compatibility package output =="
ls -lh "${repo_root}/${out_root}"/omenbrowser-rs_*.deb
