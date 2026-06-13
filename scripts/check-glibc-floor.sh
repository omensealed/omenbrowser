#!/usr/bin/env bash
set -euo pipefail

max_glibc="${1:?usage: scripts/check-glibc-floor.sh <max-glibc-version> <binary> [binary...]}"
shift

if [[ "$#" -eq 0 ]]; then
  echo "usage: scripts/check-glibc-floor.sh <max-glibc-version> <binary> [binary...]" >&2
  exit 2
fi

if ! command -v objdump >/dev/null 2>&1; then
  echo "objdump not found; install binutils to check glibc symbol requirements" >&2
  exit 127
fi

failed=0
for binary in "$@"; do
  if [[ ! -f "$binary" ]]; then
    echo "glibc floor check failed: missing binary: $binary" >&2
    failed=1
    continue
  fi

  required="$(
    objdump -T "$binary" \
      | sed -nE 's/.*\(GLIBC_([0-9.]+)\).*/\1/p' \
      | sort -Vu \
      | tail -n 1
  )"

  if [[ -z "$required" ]]; then
    echo "$binary: no GLIBC symbol requirements found"
    continue
  fi

  highest="$(printf '%s\n%s\n' "$required" "$max_glibc" | sort -Vu | tail -n 1)"
  if [[ "$highest" != "$max_glibc" ]]; then
    echo "$binary: requires GLIBC_$required, exceeds GLIBC_$max_glibc" >&2
    failed=1
  else
    echo "$binary: requires GLIBC_$required <= GLIBC_$max_glibc"
  fi
done

exit "$failed"
