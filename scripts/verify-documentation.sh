#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

fail() {
  echo "documentation verification failed: $*" >&2
  exit 1
}

version="$(
  sed -n '/^\[package\]/,/^\[/s/^version = "\([^"]*\)"/\1/p' Cargo.toml |
    head -n 1
)"
[[ -n "$version" ]] || fail "could not read root package version"
version_token="${version//./_}"
version_token="${version_token//-/_}"
notes="docs/RELEASE_NOTES_V${version_token}.md"

required_docs=(
  docs/README.md
  docs/CURRENT_STATUS.md
  docs/HISTORY.md
  docs/QUICKSTART.md
  docs/GETTING_ONLINE.md
  docs/TESTING.md
  docs/OMENCHAT.md
  docs/OMENCHAT_PROTOCOL.md
  docs/CONFIGURATION.md
  docs/NETWORK_BACKENDS.md
  docs/PRIVATE_STORAGE.md
  docs/PACKAGING.md
  docs/DEVELOPERS.md
  docs/RETICULUM_TRANSPORT_API_GAP.md
  "$notes"
)

for path in "${required_docs[@]}"; do
  [[ -f "$path" ]] || fail "missing current document: $path"
done

grep -Fq "released \`v${version}\`" docs/CURRENT_STATUS.md ||
  fail "CURRENT_STATUS.md does not identify v${version}"
grep -Fq "RELEASE_NOTES_V${version_token}.md" docs/README.md ||
  fail "documentation index does not link current release notes"
grep -Fq "Reticulum/LXMF Rust train | exact official crates.io" \
  docs/CURRENT_STATUS.md ||
  fail "current dependency-train statement is missing"

for obsolete in docs/audits docs/design docs/reviews; do
  [[ -z "$(find "$obsolete" -type f -print -quit 2>/dev/null)" ]] ||
    fail "superseded phase artifacts remain below: $obsolete"
done

if rg -n 'Managed\*\* for v0\.9\.' docs/QUICKSTART.md >/dev/null; then
  fail "quickstart binds managed mode to an obsolete release"
fi
if rg -n 'accepted.*quick-xml 0\.39\.2|quick-xml 0\.39\.2.*accepted' \
  README.md docs/README.md docs/CURRENT_STATUS.md docs/maintenance >/dev/null; then
  fail "active docs retain the retired quick-xml advisory exception"
fi

echo "documentation verification: pass (${version})"
