#!/usr/bin/env bash
set -euo pipefail

manifest="${1:-Cargo.toml}"
notes="${2:-}"

version="$(sed -n 's/^version = "\([^"]*\)"/\1/p' "$manifest" | head -n 1)"
if [[ -z "$version" ]]; then
  echo "release finalization check: package version is unavailable" >&2
  exit 1
fi
server_version="$(sed -n 's/^version = "\([^"]*\)"/\1/p' src/server/Cargo.toml | head -n 1)"
if [[ "$server_version" != "$version" ]]; then
  echo "release finalization check: root/server version mismatch: $version / ${server_version:-<missing>}" >&2
  exit 1
fi
if [[ -z "$notes" ]]; then
  notes_version="${version//./_}"
  notes_version="${notes_version//-/_}"
  notes="docs/RELEASE_NOTES_V${notes_version}.md"
fi
if [[ ! -f "$notes" ]]; then
  echo "release finalization check: missing active release notes: $notes" >&2
  exit 1
fi
if grep -Eiq '^Status:[[:space:]].*(draft|candidate)|release-candidate draft' "$notes"; then
  echo "release finalization check: active release notes remain draft: $notes" >&2
  exit 1
fi
if ! grep -Eiq '^Status:[[:space:]]*(final|released)([[:space:]]|$)' "$notes"; then
  echo "release finalization check: active release notes need 'Status: final' or 'Status: released': $notes" >&2
  exit 1
fi

notes_version="${version//./_}"
notes_version="${notes_version//-/_}"
checklist="docs/migration/V${notes_version}_RELEASE_CHECKLIST.md"
if [[ ! -f "$checklist" ]] || ! grep -Fqx "Target: \`v$version\`" "$checklist"; then
  echo "release finalization check: active checklist does not target v$version: $checklist" >&2
  exit 1
fi

echo "release finalization check: pass ($version; $notes; $checklist)"
