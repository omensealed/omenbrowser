#!/usr/bin/env bash
set -euo pipefail

source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/_common.sh"
smoke_init "07_reticulumd_rpc_interop.sh"
cd "$REPO_ROOT"

if ! command -v reticulumd >/dev/null 2>&1; then
  smoke_skip "reticulumd not found in PATH; install reticulumd for opt-in RPC interop"
fi

smoke_run "native rpc feature build" cargo check --features native-rpc
smoke_skip "reticulumd detected, but managed isolated daemon startup is not wired yet"
