#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

features="${OMENBROWSER_BROWSER_FEATURES:-desktop-product}"
tree="$(cargo tree --locked -e features --no-default-features --features "$features" -i omenbrowser_rs)"

required=(desktop-product desktop-qr portable-sqlite chat-client-gif chat-client-reticulum native-network omenchat-announcement-rooms omenchat-slow-mode)
for feature in "${required[@]}"; do
  if ! grep -q "omenbrowser_rs feature \"$feature\"" <<<"$tree"; then
    echo "product feature verification failed: required feature '$feature' is absent" >&2
    exit 1
  fi
done

forbidden=(mock-runtime desktop-dev desktop-test desktop-ui-test native-rns-net experimental-rns-net-stack legacy-live-rns-net chat-client-rns-legacy omenchat-slow-mode-qualification omenchat-moderation-audit-qualification)
for feature in "${forbidden[@]}"; do
  if grep -q "omenbrowser_rs feature \"$feature\"" <<<"$tree"; then
    echo "product feature verification failed: forbidden feature '$feature' is active" >&2
    exit 1
  fi
done

target_tree() {
  cargo tree --locked --target "$1" -e features --no-default-features \
    --features "$features" -p omenbrowser_rs
}

linux_tree="$(target_tree x86_64-unknown-linux-gnu)"
windows_tree="$(target_tree x86_64-pc-windows-msvc)"
macos_tree="$(target_tree aarch64-apple-darwin)"

for feature in 'iced feature "x11"' 'iced feature "wayland"' \
  'rfd feature "xdg-portal"' 'rfd feature "tokio"'; do
  if ! grep -qF "$feature" <<<"$linux_tree"; then
    echo "product feature verification failed: Linux target lacks $feature" >&2
    exit 1
  fi
  if grep -qF "$feature" <<<"$windows_tree$macos_tree"; then
    echo "product feature verification failed: Linux-only $feature leaked to a native target" >&2
    exit 1
  fi
done

for entry in "$linux_tree" "$windows_tree" "$macos_tree"; do
  if ! grep -qF 'rusqlite feature "bundled"' <<<"$entry"; then
    echo "product feature verification failed: portable SQLite is absent from a native target" >&2
    exit 1
  fi
done

static_media_dependencies="$(cargo tree --locked -e features --no-default-features --features desktop-product-static-media)"
static_media_features="$(cargo tree --locked -e features --no-default-features --features desktop-product-static-media -i omenbrowser_rs)"
if grep -Eq '(^|[[:space:]])iced_gif v' <<<"$static_media_dependencies"; then
  echo "product feature verification failed: static-media product includes iced_gif" >&2
  exit 1
fi
for feature in desktop-product-static-media desktop-qr chat-client-reticulum native-network omenchat-announcement-rooms omenchat-slow-mode; do
  if ! grep -q "omenbrowser_rs feature \"$feature\"" <<<"$static_media_features"; then
    echo "product feature verification failed: static-media product lacks '$feature'" >&2
    exit 1
  fi
done

for profile_spec in \
  "animated product|$features" \
  "static-media product|desktop-product-static-media"; do
  profile="${profile_spec%%|*}"
  profile_features="${profile_spec#*|}"
  qr_tree="$(
    cargo tree --locked -e features --no-default-features \
      --features "$profile_features" -i qrcode
  )"
  qr_dependencies="$(
    cargo tree --locked --no-default-features --features "$profile_features" --prefix none
  )"
  if ! grep -qF 'iced_widget feature "qr_code"' <<<"$qr_tree"; then
    echo "product feature verification failed: $profile lacks reviewed Iced QR support" >&2
    exit 1
  fi
  if ! grep -q '^qrcode v0\.13\.0$' <<<"$qr_dependencies"; then
    echo "product feature verification failed: $profile lacks locked qrcode 0.13.0" >&2
    exit 1
  fi
done

product_dependencies="$(cargo tree --locked --no-default-features --features "$features" --prefix none)"
for dormant_crate in iced_aw iced_drop iced_anim iced_table iced_toaster iced-code-editor; do
  if grep -Eq "^${dormant_crate} v" <<<"$product_dependencies"; then
    echo "product feature verification failed: dormant Iced adjunct '$dormant_crate' entered the product" >&2
    exit 1
  fi
done

assert_maintained_product_shaper() {
  local profile="$1"
  local profile_features="$2"
  local dependencies
  dependencies="$(
    cargo tree --locked --no-default-features --features "$profile_features" --prefix none
  )"
  if grep -Eq '^rustybuzz v' <<<"$dependencies"; then
    echo "product feature verification failed: $profile activates unmaintained rustybuzz" >&2
    exit 1
  fi
  for maintained_crate in harfrust skrifa; do
    if ! grep -Eq "^${maintained_crate} v" <<<"$dependencies"; then
      echo "product feature verification failed: $profile lacks maintained $maintained_crate text support" >&2
      exit 1
    fi
  done
}

assert_maintained_product_shaper "animated product" "$features"
assert_maintained_product_shaper "static-media product" "desktop-product-static-media"

assert_product_crate_excluded() {
  local profile="$1"
  local profile_features="$2"
  local crate="$3"
  if cargo tree --locked --no-default-features --features "$profile_features" \
    --prefix none | grep -Eq "^${crate} v"; then
    echo "product feature verification failed: $profile activates excluded crate $crate" >&2
    exit 1
  fi
}

assert_product_crate_excluded "animated product" "$features" bincode
assert_product_crate_excluded "static-media product" \
  "desktop-product-static-media" bincode

assert_inverse_direct_parents() {
  local profile="$1"
  local profile_features="$2"
  local crate="$3"
  local target="$4"
  local expected_parents="$5"
  local parents
  parents="$(
    cargo tree --locked --target "$target" --no-default-features \
      --features "$profile_features" \
      -i "$crate" --prefix depth \
      | sed -n 's/^1\([^ ]*\) v.*$/\1/p' \
      | sort -u \
      | paste -sd, -
  )"
  if [[ "$parents" != "$expected_parents" ]]; then
    echo "product feature verification failed: $profile $target $crate direct parents are '${parents:-none}', expected '$expected_parents'" >&2
    exit 1
  fi
}

for profile_spec in \
  "animated product|$features" \
  "static-media product|desktop-product-static-media"; do
  profile="${profile_spec%%|*}"
  profile_features="${profile_spec#*|}"
  assert_inverse_direct_parents "$profile" "$profile_features" \
    paste@1.0.15 x86_64-unknown-linux-gnu rav1e
  assert_inverse_direct_parents "$profile" "$profile_features" \
    paste@1.0.15 x86_64-pc-windows-msvc rav1e
  assert_inverse_direct_parents "$profile" "$profile_features" \
    paste@1.0.15 aarch64-apple-darwin metal,rav1e
done

if ! grep -q '^iced_gif v0\.14\.0$' <<<"$product_dependencies"; then
  echo "product feature verification failed: animated product lacks the reviewed iced_gif 0.14.0" >&2
  exit 1
fi
if grep -Eq '^async-fs v|iced_gif feature "(default|async-fs)"' <<<"$(
  cargo tree --locked -e features --no-default-features --features "$features"
)"; then
  echo "product feature verification failed: unused iced_gif async filesystem support is active" >&2
  exit 1
fi
if ! grep -q 'iced_gif feature "tokio"' <<<"$(
  cargo tree --locked -e features --no-default-features --features "$features"
)"; then
  echo "product feature verification failed: iced_gif lacks its reviewed Tokio backend" >&2
  exit 1
fi

assert_single_iced_version() {
  local profile="$1"
  local profile_features="$2"
  local versions
  versions="$(
    cargo tree --locked --no-default-features --features "$profile_features" --prefix none \
      | sed -n 's/^iced v\([^ ]*\).*$/\1/p' \
      | sort -u
  )"
  if [[ "$versions" != "0.14.0" ]]; then
    echo "product feature verification failed: $profile resolves Iced versions '${versions:-none}'" >&2
    exit 1
  fi
}

assert_single_iced_version "animated product" "desktop-product"
assert_single_iced_version "static-media product" "desktop-product-static-media"
assert_single_iced_version "optional widgets" "desktop-widgets"
assert_single_iced_version "optional drag/drop" "desktop-dnd"
assert_single_iced_version "optional animations" "desktop-animations"
assert_single_iced_version "optional tables" "desktop-tables"

widget_tree="$(cargo tree --locked -e features --no-default-features --features desktop-widgets)"
for font_feature in lucide nerd codicon; do
  if grep -qF "iced_fonts feature \"$font_feature\"" <<<"$widget_tree"; then
    echo "product feature verification failed: desktop-widgets includes unused iced_fonts/$font_feature" >&2
    exit 1
  fi
done

for server_profile in server-headless server-full; do
  server_features="$(
    cargo tree --locked --manifest-path src/server/Cargo.toml -e features \
      --no-default-features --features "$server_profile" -i omenchatd
  )"
  if ! grep -q 'omenchatd feature "omenchat-announcement-rooms"' \
    <<<"$server_features"; then
    echo "product feature verification failed: $server_profile lacks announcement-room support" >&2
    exit 1
  fi
  if ! grep -q 'omenchatd feature "omenchat-slow-mode"' \
    <<<"$server_features"; then
    echo "product feature verification failed: $server_profile lacks slow-mode support" >&2
    exit 1
  fi
  if grep -q 'omenchatd feature "omenchat-slow-mode-qualification"' \
    <<<"$server_features"; then
    echo "product feature verification failed: $server_profile activates dormant slow-mode qualification" >&2
    exit 1
  fi
  if grep -q 'omenchatd feature "omenchat-moderation-audit-qualification"' \
    <<<"$server_features"; then
    echo "product feature verification failed: $server_profile activates moderation-audit qualification" >&2
    exit 1
  fi
done

echo "product feature verification: pass ($features)"
