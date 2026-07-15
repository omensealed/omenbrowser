#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

root_tree="$(cargo tree --locked --no-default-features --features tui --prefix none)"
server_tree="$(
  cargo tree --locked --manifest-path src/server/Cargo.toml \
    --no-default-features --features server-full --prefix none
)"

assert_exact_version() {
  local profile="$1"
  local tree="$2"
  local crate="$3"
  local expected="$4"
  local versions
  versions="$(
    sed -n "s/^${crate} v\\([^ ]*\\).*$/\\1/p" <<<"$tree" | sort -u
  )"
  if [[ "$versions" != "$expected" ]]; then
    echo "TUI dependency verification failed: $profile resolves $crate versions '${versions:-none}', expected '$expected'" >&2
    exit 1
  fi
}

assert_excluded() {
  local profile="$1"
  local tree="$2"
  local crate="$3"
  if grep -Eq "^${crate} v" <<<"$tree"; then
    echo "TUI dependency verification failed: $profile includes excluded crate $crate" >&2
    exit 1
  fi
}

for profile in root server; do
  if [[ "$profile" == "root" ]]; then
    tree="$root_tree"
  else
    tree="$server_tree"
  fi
  assert_exact_version "$profile TUI" "$tree" ratatui 0.30.2
  assert_exact_version "$profile TUI" "$tree" crossterm 0.29.0
  assert_exact_version "$profile TUI" "$tree" lru 0.18.1
  assert_excluded "$profile TUI" "$tree" paste
done

root_feature_tree="$(cargo tree --locked --no-default-features --features tui -e features)"
server_feature_tree="$(cargo tree --locked --manifest-path src/server/Cargo.toml \
  --no-default-features --features server-full -e features)"
root_tokio_features="$(cargo tree --locked --no-default-features --features tui \
  -e features -i tokio)"

if ! grep -qF 'tokio feature "signal"' <<<"$root_tokio_features"; then
  echo "TUI dependency verification failed: root TUI lacks Tokio signal handling" >&2
  exit 1
fi

for feature_tree in "$root_feature_tree" "$server_feature_tree"; do
  for feature in crossterm_0_29 layout-cache; do
    if ! grep -qF "ratatui feature \"$feature\"" <<<"$feature_tree"; then
      echo "TUI dependency verification failed: Ratatui lacks required feature '$feature'" >&2
      exit 1
    fi
  done
  for feature in all-widgets macros widget-calendar; do
    if grep -qF "ratatui feature \"$feature\"" <<<"$feature_tree"; then
      echo "TUI dependency verification failed: unused Ratatui feature '$feature' is active" >&2
      exit 1
    fi
  done
done

echo "TUI dependency verification: pass (Ratatui 0.30.2 / Crossterm 0.29.0)"
