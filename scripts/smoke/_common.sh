#!/usr/bin/env bash

smoke_repo_root() {
  local script_dir
  script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
  (cd "$script_dir/../.." && pwd)
}

smoke_default_root() {
  printf '%s\n' "${OMENBROWSER_SMOKE_ROOT:-${TMPDIR:-/tmp}/omenbrowser-rs-smoke}"
}

smoke_timestamp() {
  date -u +%Y%m%dT%H%M%SZ
}

smoke_init() {
  local name="$1"
  REPO_ROOT="$(smoke_repo_root)"
  SMOKE_ROOT="$(smoke_default_root)"
  SMOKE_RUN_ROOT="${SMOKE_RUN_ROOT:-$SMOKE_ROOT/$(smoke_timestamp)}"
  SMOKE_LOG_DIR="$SMOKE_RUN_ROOT/logs"
  mkdir -p "$SMOKE_LOG_DIR"
  echo "repo_root: $REPO_ROOT"
  echo "smoke_root: $SMOKE_RUN_ROOT"
  echo "script: $name"
  rustc --version
  cargo --version
  if [[ "$SMOKE_RUN_ROOT" == "$HOME/.reticulum"* || "$SMOKE_RUN_ROOT" == "$HOME/.nomadnetwork"* || "$SMOKE_RUN_ROOT" == "$HOME/.lxmd"* ]]; then
    echo "refusing unsafe smoke root: $SMOKE_RUN_ROOT" >&2
    exit 2
  fi
}

smoke_run() {
  local label="$1"
  shift
  local safe_label
  safe_label="$(printf '%s' "$label" | tr -c 'A-Za-z0-9_.-' '_')"
  local log="$SMOKE_LOG_DIR/${safe_label}.log"
  echo "== $label =="
  echo "+ $*" | tee "$log"
  "$@" >> "$log" 2>&1
}

smoke_skip() {
  local reason="$1"
  echo "RESULT: SKIP"
  echo "reason: $reason"
  exit 0
}

smoke_pass() {
  echo "RESULT: PASS"
  exit 0
}
