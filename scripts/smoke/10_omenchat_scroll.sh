#!/usr/bin/env bash
set -euo pipefail

source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/_common.sh"
smoke_init "10_omenchat_scroll.sh"
cd "$REPO_ROOT"

smoke_run "omenchat initial bottom anchor" \
  cargo test --locked --no-default-features --features desktop-product \
  newly_opened_omenchat_pane_treats_bottom_anchor_origin_as_present
smoke_run "omenchat attachment layout follow policy" \
  cargo test --locked --no-default-features --features desktop-product \
  omenchat_media_layout_change

smoke_pass
