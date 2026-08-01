#!/usr/bin/env bash
set -euo pipefail

out_dir="${1:-dist}"
mode="${2:-native}"

fail() {
  echo "Linux ARM64 omenchatd packaging failed: $*" >&2
  exit 1
}

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

for tool in awk cargo git gzip readelf rustc sha256sum tar uname; do
  command -v "$tool" >/dev/null 2>&1 || fail "required tool is missing: $tool"
done

host_target="$(rustc -vV | sed -n 's/^host: //p')"
artifact_target="aarch64-unknown-linux-gnu"
case "$mode" in
  native)
    [[ "$host_target" == "aarch64-unknown-linux-gnu" ]] \
      || fail "native aarch64-unknown-linux-gnu host required; found $host_target"
    case "$(uname -m)" in
      aarch64|arm64) ;;
      *) fail "native ARM64 kernel required; found $(uname -m)" ;;
    esac
    build_evidence="native ARM64 Linux host"
    ;;
  --cross-emulated)
    for tool in cross podman; do
      command -v "$tool" >/dev/null 2>&1 \
        || fail "cross-emulated mode requires: $tool"
    done
    export CROSS_CONTAINER_ENGINE="${CROSS_CONTAINER_ENGINE:-podman}"
    export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-target/aarch64-cross}"
    build_evidence="cross-compiled and QEMU-executed through Podman/Cross"
    ;;
  *) fail "unknown mode: $mode (expected native or --cross-emulated)" ;;
esac

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

version="$(read_package_version src/server/Cargo.toml)"
[[ -n "$version" ]] || fail "omenchatd package version is missing"

echo "== Building Linux ARM64 omenchatd ($mode) =="
if [[ "$mode" == "native" ]]; then
  cargo build --release --locked --manifest-path src/server/Cargo.toml \
    --no-default-features --features server-headless --bin omenchatd
  server_binary="$repo_root/src/server/target/release/omenchatd"
else
  cross build --release --locked --manifest-path src/server/Cargo.toml \
    --target aarch64-unknown-linux-gnu \
    --no-default-features --features server-headless --bin omenchatd
  case "$CARGO_TARGET_DIR" in
    /*) server_binary="$CARGO_TARGET_DIR/aarch64-unknown-linux-gnu/release/omenchatd" ;;
    *) server_binary="$repo_root/$CARGO_TARGET_DIR/aarch64-unknown-linux-gnu/release/omenchatd" ;;
  esac
fi

[[ -x "$server_binary" ]] || fail "release binary is missing"
readelf -h "$server_binary" | grep -q 'Machine:.*AArch64' \
  || fail "release binary is not AArch64"

run_server() {
  if [[ "$mode" == "native" ]]; then
    "$server_binary" "$@"
  else
    cross run --quiet --release --locked \
      --manifest-path src/server/Cargo.toml \
      --target aarch64-unknown-linux-gnu \
      --no-default-features --features server-headless --bin omenchatd \
      -- "$@"
  fi
}

if [[ "$mode" == "native" ]]; then
  server_identity="$(run_server --version)"
  for required in \
    "omenchatd $version" \
    "server-headless:on" \
    "server-full:off" \
    "live-reticulum:on"; do
    [[ "$server_identity" == *"$required"* ]] \
      || fail "binary identity is missing: $required"
  done
fi

if [[ "$mode" == "native" ]]; then
  temporary_root="$(mktemp -d "${TMPDIR:-/tmp}/omenchatd-linux-arm64-package.XXXXXX")"
else
  mkdir -p target
  temporary_root="$(mktemp -d target/omenchatd-linux-arm64-package.XXXXXX)"
fi
cleanup() {
  rm -rf "$temporary_root"
}
trap cleanup EXIT INT TERM

echo "== Running isolated ARM64 lifecycle smoke ($mode) =="
selfcheck_home="$temporary_root/selfcheck-home"
run_server init --home "$selfcheck_home" \
  > "$temporary_root/omenchatd-init-selfcheck.txt"
run_server status --home "$selfcheck_home" \
  > "$temporary_root/omenchatd-status-selfcheck.txt"
run_server doctor --home "$selfcheck_home" \
  > "$temporary_root/omenchatd-doctor-selfcheck.txt"
test -f "$selfcheck_home/config.toml"
test -f "$selfcheck_home/identity"
test -f "$selfcheck_home/omenchat.sqlite"
test -d "$selfcheck_home/reticulum"
grep -q 'client uri: omenchat://' "$temporary_root/omenchatd-status-selfcheck.txt"
grep -q 'omenchatd doctor:' "$temporary_root/omenchatd-doctor-selfcheck.txt"
if grep -q '(missing)' "$temporary_root/omenchatd-status-selfcheck.txt"; then
  fail "isolated status reported a missing file"
fi

resolved_out="$(mkdir -p "$out_dir" && cd "$out_dir" && pwd)"
package_name="omenchatd-$version-linux-aarch64"
stage="$temporary_root/$package_name"
mkdir -p "$stage/packaging/systemd"
install -m 0755 "$server_binary" "$stage/omenchatd"
install -m 0644 src/server/README.md "$stage/README.md"
install -m 0644 docs/OMENCHAT_PROTOCOL.md "$stage/OMENCHAT_PROTOCOL.md"
install -m 0644 packaging/systemd/omenchatd.service.in \
  "$stage/packaging/systemd/omenchatd.service.in"
cat > "$stage/PACKAGE-METADATA.txt" <<EOF
version: $version
target: $artifact_target
profile: server-headless
architecture: aarch64
build_evidence: $build_evidence
physical_device_qualified: false
arm64_release_gate: passed
service_install: none
EOF

source_date_epoch="${SOURCE_DATE_EPOCH:-$(git show -s --format=%ct HEAD)}"
[[ "$source_date_epoch" =~ ^[0-9]+$ ]] \
  || fail "SOURCE_DATE_EPOCH must be an integer"
archive="$resolved_out/$package_name.tar.gz"
rm -f "$archive" "$archive.sha256"
tar --sort=name \
  --mtime="@$source_date_epoch" \
  --owner=0 --group=0 --numeric-owner \
  -C "$temporary_root" -cf - "$package_name" \
  | gzip -n > "$archive"
(
  cd "$resolved_out"
  sha256sum "$(basename "$archive")" > "$(basename "$archive").sha256"
  sha256sum --check --strict "$(basename "$archive").sha256"
)

echo "Linux ARM64 omenchatd package:"
echo "  $archive"
echo "target: $artifact_target"
echo "evidence: $build_evidence"
