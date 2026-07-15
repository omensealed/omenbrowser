#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

if [[ "${1:-}" == "--inside-pty" ]]; then
  : "${OMEN_TUI_PTY_BINARY:?missing TUI binary path}"
  : "${OMEN_TUI_PTY_ROOT:?missing isolated app root}"
  : "${OMEN_TUI_PTY_RESULT:?missing result path}"
  : "${OMEN_TUI_PTY_SIGNAL:?missing shutdown signal}"
  : "${OMEN_TUI_PTY_SIGNAL_COUNT:?missing shutdown signal count}"
  case "$OMEN_TUI_PTY_SIGNAL" in
    INT | TERM) ;;
    *)
      echo "unsupported PTY shutdown signal '$OMEN_TUI_PTY_SIGNAL'" >&2
      exit 2
      ;;
  esac
  case "$OMEN_TUI_PTY_SIGNAL_COUNT" in
    1 | 2) ;;
    *)
      echo "unsupported PTY shutdown signal count '$OMEN_TUI_PTY_SIGNAL_COUNT'" >&2
      exit 2
      ;;
  esac

  before="$(stty -g)"
  before_size="$(stty size)"
  resize_result="${OMEN_TUI_PTY_RESULT}.resize"
  umask 077

  set +e
  "$OMEN_TUI_PTY_BINARY" --tui --app-root "$OMEN_TUI_PTY_ROOT" &
  app_pid=$!
  (
    sleep 0.4
    for dimensions in "0 0" "1 1" "10 40" "30 100"; do
      read -r rows columns <<<"$dimensions"
      stty rows "$rows" cols "$columns" < /dev/tty
      actual="$(stty size < /dev/tty)"
      sleep 0.35
      if ! kill -0 "$app_pid" 2>/dev/null; then
        printf 'app exited during resize to %s\n' "$actual" > "$resize_result"
        exit 1
      fi
      printf '%s\n' "$actual" >> "$resize_result"
    done
    signal_sent_ns="$(date +%s%N)"
    {
      printf 'signal=%s\n' "$OMEN_TUI_PTY_SIGNAL"
      printf 'count=%s\n' "$OMEN_TUI_PTY_SIGNAL_COUNT"
      printf 'sent_ns=%s\n' "$signal_sent_ns"
    } > "${OMEN_TUI_PTY_RESULT}.signal"
    for ((signal_index = 1; signal_index <= OMEN_TUI_PTY_SIGNAL_COUNT; signal_index++)); do
      kill -s "$OMEN_TUI_PTY_SIGNAL" "$app_pid"
      if ((signal_index < OMEN_TUI_PTY_SIGNAL_COUNT)); then
        sleep 0.01
      fi
    done
  ) &
  resize_pid=$!
  wait "$app_pid"
  app_status=$?
  app_exit_ns="$(date +%s%N)"
  wait "$resize_pid"
  resize_status=$?
  read -r before_rows before_columns <<<"$before_size"
  stty rows "$before_rows" cols "$before_columns"
  set -e
  after="$(stty -g)"
  after_size="$(stty size)"

  {
    printf 'app_status=%s\n' "$app_status"
    printf 'app_exit_ns=%s\n' "$app_exit_ns"
    printf 'resize_status=%s\n' "$resize_status"
    printf 'before=%s\n' "$before"
    printf 'after=%s\n' "$after"
    printf 'before_size=%s\n' "$before_size"
    printf 'after_size=%s\n' "$after_size"
  } > "$OMEN_TUI_PTY_RESULT"
  if [[ $app_status -ne 0 ]]; then
    exit "$app_status"
  fi
  exit "$resize_status"
fi

cd "$repo_root"

if [[ "$(uname -s)" != "Linux" ]]; then
  echo "real PTY TUI smoke is Linux-only; use the native lifecycle tests on this host" >&2
  exit 2
fi

require_tool() {
  local tool="$1"
  local package="$2"
  if ! command -v "$tool" >/dev/null 2>&1; then
    echo "real PTY TUI smoke requires '$tool' from package '$package'" >&2
    exit 2
  fi
}

require_tool script util-linux
require_tool stty coreutils
require_tool timeout coreutils
require_tool mktemp coreutils
require_tool date coreutils

cargo build --locked --no-default-features --features tui --bin omenbrowser_rs
binary="$repo_root/target/debug/omenbrowser_rs"
version="$($binary --version)"
if ! grep -q 'tui:on' <<<"$version"; then
  echo "real PTY TUI smoke refuses a binary without the tui feature" >&2
  exit 1
fi

root="$(mktemp -d "${TMPDIR:-/tmp}/omenbrowser-rs-tui-pty.XXXXXX")"
cleanup() {
  rm -rf -- "$root"
}
trap cleanup EXIT

export OMEN_TUI_PTY_BINARY="$binary"
export OMEN_TUI_PTY_SCRIPT="$repo_root/scripts/test-tui-real-pty.sh"
signal_shutdown_limit_ms=3000
signal_summaries=()

run_signal_case() {
  local signal="$1"
  local signal_count="$2"
  local result="$root/pty-result-${signal}-${signal_count}.txt"
  local pty_status app_status resize_status before after before_size after_size
  local actual_resizes delivered_signal delivered_count signal_sent_ns app_exit_ns latency_ns latency_ms

  export OMEN_TUI_PTY_ROOT="$root/app-${signal}-${signal_count}"
  export OMEN_TUI_PTY_RESULT="$result"
  export OMEN_TUI_PTY_SIGNAL="$signal"
  export OMEN_TUI_PTY_SIGNAL_COUNT="$signal_count"

  set +e
  # The environment variable must expand inside the child shell attached to the PTY.
  # shellcheck disable=SC2016
  timeout --signal=TERM --kill-after=2s 20s script --quiet --return --command \
    'bash "$OMEN_TUI_PTY_SCRIPT" --inside-pty' /dev/null >/dev/null
  pty_status=$?
  set -e

  if [[ $pty_status -eq 124 || $pty_status -eq 137 ]]; then
    echo "real PTY TUI smoke timed out waiting for SIG${signal}x${signal_count} shutdown" >&2
    exit 1
  fi
  if [[ $pty_status -ne 0 ]]; then
    echo "real PTY TUI smoke SIG${signal}x${signal_count} application exited with status $pty_status" >&2
    exit 1
  fi
  if [[ ! -f "$result" ]]; then
    echo "real PTY TUI smoke SIG${signal}x${signal_count} produced no terminal restoration result" >&2
    exit 1
  fi

  app_status="$(sed -n 's/^app_status=//p' "$result")"
  app_exit_ns="$(sed -n 's/^app_exit_ns=//p' "$result")"
  resize_status="$(sed -n 's/^resize_status=//p' "$result")"
  before="$(sed -n 's/^before=//p' "$result")"
  after="$(sed -n 's/^after=//p' "$result")"
  before_size="$(sed -n 's/^before_size=//p' "$result")"
  after_size="$(sed -n 's/^after_size=//p' "$result")"
  if [[ "$app_status" != "0" ]]; then
    echo "real PTY TUI smoke SIG$signal recorded application status '${app_status:-missing}'" >&2
    exit 1
  fi
  if [[ "$resize_status" != "0" ]]; then
    echo "real PTY TUI smoke SIG$signal recorded resize status '${resize_status:-missing}'" >&2
    exit 1
  fi
  if [[ -z "$before" || -z "$after" || "$before" != "$after" ]]; then
    echo "real PTY TUI smoke SIG$signal detected terminal flag drift" >&2
    exit 1
  fi
  if [[ -z "$before_size" || "$before_size" != "$after_size" ]]; then
    echo "real PTY TUI smoke SIG$signal detected terminal size drift" >&2
    exit 1
  fi

  expected_resizes=$'0 0\n1 1\n10 40\n30 100'
  actual_resizes="$(cat "${result}.resize" 2>/dev/null || true)"
  if [[ "$actual_resizes" != "$expected_resizes" ]]; then
    echo "real PTY TUI smoke SIG$signal did not survive the expected resize sequence" >&2
    exit 1
  fi

  delivered_signal="$(sed -n 's/^signal=//p' "${result}.signal" 2>/dev/null || true)"
  delivered_count="$(sed -n 's/^count=//p' "${result}.signal" 2>/dev/null || true)"
  signal_sent_ns="$(sed -n 's/^sent_ns=//p' "${result}.signal" 2>/dev/null || true)"
  if [[ "$delivered_signal" != "$signal" || "$delivered_count" != "$signal_count" ]]; then
    echo "real PTY TUI smoke did not deliver expected SIG${signal}x${signal_count}" >&2
    exit 1
  fi
  if [[ ! "$signal_sent_ns" =~ ^[0-9]+$ || ! "$app_exit_ns" =~ ^[0-9]+$ || $app_exit_ns -lt $signal_sent_ns ]]; then
    echo "real PTY TUI smoke SIG$signal produced invalid shutdown timestamps" >&2
    exit 1
  fi
  latency_ns=$((app_exit_ns - signal_sent_ns))
  latency_ms=$(((latency_ns + 999999) / 1000000))
  if ((latency_ms > signal_shutdown_limit_ms)); then
    echo "real PTY TUI smoke SIG$signal shutdown took ${latency_ms} ms (limit ${signal_shutdown_limit_ms} ms)" >&2
    exit 1
  fi
  signal_summaries+=("SIG${signal}x${signal_count}=${latency_ms}ms")
}

run_signal_case TERM 1
run_signal_case INT 1
run_signal_case TERM 2

echo "real PTY TUI smoke: pass (isolated roots, live resize, signal quit, terminal restored; ${signal_summaries[*]})"
