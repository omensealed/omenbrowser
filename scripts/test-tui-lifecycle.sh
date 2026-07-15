#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

cargo test --locked --no-default-features --features tui --lib \
  ui::tests::terminal_guard
cargo test --locked --no-default-features --features tui --lib \
  ui::tests::isolated_tui_render_and_quit_smoke_preserves_root_boundary

echo "isolated TUI lifecycle smoke: pass"
