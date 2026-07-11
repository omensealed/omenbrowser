#!/usr/bin/env bash
set -euo pipefail

source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/_common.sh"
smoke_init "06_lxmf_cli_interop.sh"
cd "$REPO_ROOT"

if ! command -v lxmf-cli >/dev/null 2>&1; then
  smoke_skip "lxmf-cli not found in PATH; install lxmf-cli and rerun for opt-in CLI interop"
fi

smoke_skip "lxmf-cli detected, but isolated config flags have not been validated for this repository yet"
