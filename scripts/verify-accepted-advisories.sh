#!/usr/bin/env bash
set -euo pipefail

root_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root_dir"

fail() {
  echo "accepted advisory verification failed: $*" >&2
  exit 1
}

for tool in cargo jq rg; do
  command -v "$tool" >/dev/null 2>&1 || fail "required tool is missing: $tool"
done
cargo audit --version >/dev/null 2>&1 || fail "cargo-audit is not installed"

case "${1:-}" in
  "") audit_fetch_args=() ;;
  --no-fetch) audit_fetch_args=(--no-fetch) ;;
  *) echo "usage: $0 [--no-fetch]" >&2; exit 2 ;;
esac

evidence_root="$(mktemp -d "${TMPDIR:-/tmp}/omen-accepted-advisories.XXXXXX")"
trap 'rm -rf "$evidence_root"' EXIT
metadata="$evidence_root/root-metadata.json"
server_metadata="$evidence_root/server-metadata.json"
audit_report="$evidence_root/root-audit.json"

cargo metadata --locked --format-version 1 \
  --no-default-features --features desktop-product > "$metadata"
cargo metadata --locked --format-version 1 \
  --manifest-path src/server/Cargo.toml > "$server_metadata"

quick_id="$(
  jq -r '
    [.packages[] | select(
      .name == "quick-xml"
      and .version == "0.39.2"
      and .source == "registry+https://github.com/rust-lang/crates.io-index"
    ) | .id] | if length == 1 then .[0] else empty end
  ' "$metadata"
)"
[[ -n "$quick_id" ]] || fail "expected exactly registry quick-xml 0.39.2"

scanner_id="$(
  jq -r '
    [.packages[] | select(
      .name == "wayland-scanner"
      and .version == "0.31.10"
      and .source == "registry+https://github.com/rust-lang/crates.io-index"
      and any(.targets[].kind[]; . == "proc-macro")
    ) | .id] | if length == 1 then .[0] else empty end
  ' "$metadata"
)"
[[ -n "$scanner_id" ]] || fail "expected exactly registry wayland-scanner 0.31.10 proc-macro"

mapfile -t quick_parents < <(
  jq -r --arg quick "$quick_id" '
    .resolve.nodes[] | select(any(.deps[]; .pkg == $quick)) | .id
  ' "$metadata" | sort -u
)
[[ ${#quick_parents[@]} -eq 1 && "${quick_parents[0]}" == "$scanner_id" ]] \
  || fail "quick-xml 0.39.2 has a dependency parent other than wayland-scanner 0.31.10"

if jq -e '.packages[] | select(.name == "quick-xml")' "$server_metadata" >/dev/null; then
  fail "standalone omenchatd resolved quick-xml"
fi
if rg -n --glob '*.rs' -g '!target/**' -g '!src/server/target/**' \
  '(^|[^[:alnum:]_])quick_xml(::|!)' . >/dev/null; then
  fail "repository Rust source imports quick-xml"
fi
grep -Eq '^ignore[[:space:]]*=[[:space:]]*\[\][[:space:]]*$' deny.toml \
  || fail "deny.toml contains an advisory ignore"

set +e
cargo audit "${audit_fetch_args[@]}" --json > "$audit_report"
audit_status=$?
set -e
[[ $audit_status -eq 1 ]] || fail "raw root audit did not exit with the expected vulnerability status"

mapfile -t vulnerability_ids < <(
  jq -r '.vulnerabilities.list[].advisory.id' "$audit_report" | sort -u
)
expected_ids=(RUSTSEC-2026-0194 RUSTSEC-2026-0195)
[[ "${vulnerability_ids[*]}" == "${expected_ids[*]}" ]] \
  || fail "raw root audit contains an unaccepted vulnerability set: ${vulnerability_ids[*]:-none}"
jq -e '
  .vulnerabilities.count == 2
  and all(.vulnerabilities.list[];
    .package.name == "quick-xml"
    and .package.version == "0.39.2"
    and .package.source == "registry+https://github.com/rust-lang/crates.io-index"
  )
' "$audit_report" >/dev/null \
  || fail "accepted advisories do not map exactly to registry quick-xml 0.39.2"

cargo audit --no-fetch \
  --ignore RUSTSEC-2026-0194 \
  --ignore RUSTSEC-2026-0195 >/dev/null
cargo audit --no-fetch --file src/server/Cargo.lock >/dev/null

echo "accepted build-time advisories: RUSTSEC-2026-0194 RUSTSEC-2026-0195"
echo "accepted path: wayland-scanner 0.31.10 proc-macro -> quick-xml 0.39.2"
echo "standalone omenchatd quick-xml packages: 0"
echo "accepted advisory verification: pass"
