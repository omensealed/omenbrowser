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
grep -q 'cargo install cargo-packager --version 0\.11\.8 --locked' <<<"$windows_package_job" \
  || fail "Windows installer builder does not pin cargo-packager 0.11.8"
grep -q 'scripts/package-windows-installers.ps1 -OutDir dist -RunLifecycleSmoke' <<<"$windows_package_job" \
  || fail "Windows installer build does not run the reviewed lifecycle gate"
for installer_pattern in '-setup-unsigned.exe' '-unsigned.msi'; do
  grep -q -- "$installer_pattern" <<<"$windows_package_job" \
    || fail "Windows artifact upload lacks $installer_pattern"
done
grep -q 'contents: write' <<<"$windows_package_job" \
  && fail "Windows portable build job has contents: write"

installer_script=scripts/package-windows-installers.ps1
[[ -f "$installer_script" ]] || fail "Windows installer script is missing"
for tool_hash in \
  f5dc52eef1f3884230520199bac6f36b82d643d86b003ce51bd24b05c6ba7c91 \
  1c2772b0edfb0f96a7524734d6c8fac1fc011f26221faf88f3ed2c950f0c06c0 \
  0eed48313a7f904d7cc1977b70000ab3f11f18cadc8e6a69b807d288ca71f9db \
  2c1888d5d1dba377fc7fa14444cf556963747ff9a0a289a3599cf09da03b9e2e; do
  grep -q "$tool_hash" "$installer_script" \
    || fail "Windows installer tool hash is missing: $tool_hash"
done
grep -q 'installMode = "currentUser"' "$installer_script" \
  || fail "NSIS installer is not explicitly current-user scoped"
grep -q 'allowDowngrades = \$false' "$installer_script" \
  || fail "Windows installers do not reject downgrades"
grep -q 'name = "omenbrowser-installer"' "$installer_script" \
  || fail "Windows installer config lacks an explicit package identity"
grep -q 'Get-Content -LiteralPath \$logPath -Tail 80' "$installer_script" \
  || fail "Windows installed-GUI failure output is not bounded"

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
grep -Fq "github.event_name != 'workflow_dispatch' || inputs.run_package_smoke == 'true'" \
  .github/workflows/package.yml \
  || fail "tag packaging does not require the package smoke gate"

echo "workflow security verification: pass"
