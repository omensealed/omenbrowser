#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

version="$(sed -n '/^\[package\]/,/^\[/s/^version = "\([^"]*\)"/\1/p' Cargo.toml | head -n 1)"
[[ "$version" == "0.10.0-5" ]] || { echo "unexpected package version: $version" >&2; exit 1; }

mapping="$(bash scripts/package-macos.sh --print-version-mapping "$version")"
[[ "$(sed -n '1p' <<<"$mapping")" == "0.10.0" ]]
[[ "$(sed -n '2p' <<<"$mapping")" == "1000.0.5" ]]

grep -Fq 'return "$($Matches[1])-$($revision - 1)"' scripts/package-windows-installers.ps1
[[ "${version/-/.}" == "0.10.0.5" ]]
prior_revision="${version%-*}-$((10#${version##*-} - 1))"
[[ "$prior_revision" == "0.10.0-4" ]]

echo "package version mappings: pass (macOS 0.10.0/1000.0.5; MSI 0.10.0.5; prior 0.10.0-4)"
