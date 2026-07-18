#!/usr/bin/env bash
set -euo pipefail

root_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root_dir"

fail() {
  echo "workflow security verification failed: $*" >&2
  exit 1
}

mapfile -t workflows < <(find .github/workflows -maxdepth 1 -type f \( -name '*.yml' -o -name '*.yaml' \) -print | sort)
[[ ${#workflows[@]} -gt 0 ]] || fail "no workflow files found"

while IFS= read -r reference; do
  [[ "$reference" =~ ^[0-9a-f]{40}$ ]] || fail "mutable action reference @$reference"
done < <(sed -nE 's/^[[:space:]]*uses:[[:space:]]*[^@[:space:]]+@([^[:space:]#]+).*/\1/p' "${workflows[@]}")

for workflow in "${workflows[@]}"; do
  sed -n '1,15p' "$workflow" | grep -Eq '^permissions:$' \
    || fail "$workflow lacks workflow-level permissions"
  sed -n '1,15p' "$workflow" | grep -Eq '^  contents: read$' \
    || fail "$workflow does not default to contents: read"
done

native_workflow=.github/workflows/native-checks.yml
[[ -f "$native_workflow" ]] || fail "native reusable workflow is missing"
for runner in windows-2025 macos-15-intel macos-15; do
  grep -q "runner: $runner" "$native_workflow" \
    || fail "native workflow lacks runner $runner"
done
for feature in desktop-product tui server-headless server-full; do
  grep -q -- "--features $feature" "$native_workflow" \
    || fail "native workflow lacks feature profile $feature"
  grep -q -- "--features $feature --all-targets -- -D warnings" "$native_workflow" \
    || fail "native workflow lacks all-target strict Clippy for $feature"
done
grep -q '^      - name: Check and test root terminal UI$' "$native_workflow" \
  || fail "native workflow lacks the root terminal UI gate"
grep -q 'bash scripts/verify-tui-dependencies.sh' "$native_workflow" \
  || fail "native workflow lacks TUI dependency verification"
grep -q '^      - name: Run isolated terminal lifecycle smoke$' "$native_workflow" \
  || fail "native workflow lacks the isolated terminal lifecycle smoke"
grep -q 'bash scripts/test-tui-lifecycle.sh' "$native_workflow" \
  || fail "native workflow does not execute the terminal lifecycle harness"
grep -q '^      - name: Run native release CLI identity smoke$' "$native_workflow" \
  || fail "native workflow lacks the release CLI identity smoke"
grep -q 'bash scripts/test-native-cli-identity.sh' "$native_workflow" \
  || fail "native workflow does not execute the release CLI identity smoke"
grep -q 'bash scripts/test-native-cli-identity.sh' scripts/release-check.sh \
  || fail "Linux release checks do not execute the release CLI identity smoke"
grep -q 'bash scripts/test-tui-real-pty.sh' scripts/release-check.sh \
  || fail "Linux release checks do not execute the real PTY TUI smoke"
grep -q 'uses: \./\.github/workflows/native-checks\.yml' .github/workflows/ci.yml \
  || fail "CI does not invoke native checks"
grep -q 'uses: \./\.github/workflows/native-checks\.yml' .github/workflows/package.yml \
  || fail "packaging does not invoke native checks"

package_job="$(sed -n '/^  package:$/,/^  publish:$/p' .github/workflows/package.yml)"
grep -q '^    needs: native$' <<<"$package_job" \
  || fail "package build does not depend on native checks"
grep -q 'contents: write' <<<"$package_job" \
  && fail "package build job has contents: write"

windows_package_job="$(sed -n '/^  windows-portable:$/,/^  publish:$/p' .github/workflows/package.yml)"
grep -q '^    needs: native$' <<<"$windows_package_job" \
  || fail "Windows portable build does not depend on native checks"
grep -q '^    runs-on: windows-2025$' <<<"$windows_package_job" \
  || fail "Windows portable build does not use the native MSVC runner"
grep -q 'scripts/package-windows-portable.ps1' <<<"$windows_package_job" \
  || fail "Windows portable build does not use the reviewed package script"
grep -q 'contents: write' <<<"$windows_package_job" \
  && fail "Windows portable build job has contents: write"

publish_job="$(sed -n '/^  publish:$/,$p' .github/workflows/package.yml)"
grep -q '^      - package$' <<<"$publish_job" \
  || fail "publish job does not depend on Linux package"
grep -q '^      - windows-portable$' <<<"$publish_job" \
  || fail "publish job does not depend on Windows portable package"
grep -q '^    environment: release$' <<<"$publish_job" \
  || fail "publish job lacks the release environment gate"
grep -q '^      contents: write$' <<<"$publish_job" \
  || fail "publish job lacks scoped contents: write"
grep -q 'actions/checkout@' <<<"$publish_job" \
  && fail "privileged publish job checks out repository code"
grep -Eq 'run:.*scripts/|bash[[:space:]]+scripts/' <<<"$publish_job" \
  && fail "privileged publish job executes repository scripts"

grep -q 'releases/download/continuous/' .github/workflows/package.yml \
  && fail "mutable appimagetool continuous URL remains"
grep -q 'releases/download/1\.9\.1/appimagetool-x86_64\.AppImage' .github/workflows/package.yml \
  || fail "pinned appimagetool 1.9.1 URL missing"
grep -q 'ed4ce84f0d9caff66f50bcca6ff6f35aae54ce8135408b3fa33abfc3cb384eb0' .github/workflows/package.yml \
  || fail "reviewed appimagetool checksum missing"
grep -q 'sha256sum --check --strict' .github/workflows/package.yml \
  || fail "appimagetool checksum verification missing"

echo "workflow security verification: pass"
