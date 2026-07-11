#!/usr/bin/env bash
set -euo pipefail

source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/_common.sh"
smoke_init "05_lxmf_service_loopback.sh"
cd "$REPO_ROOT"

smoke_run "lxmf state mapping tests" \
  cargo test --features chat-client-reticulum test_lxmf_delivery_state_mapping
smoke_run "native lxmf smoke report tests" \
  cargo test --features chat-client-reticulum native_lxmf_smoke_send_report_skips_send_when_not_ready

smoke_pass
