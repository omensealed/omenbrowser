#!/usr/bin/env bash
set -euo pipefail
umask 077
script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
validator="$script_dir/validate-lxmf-invitation-capability-report.py"

required=(
  OMEN_LXMF_INVITE_BINARY
  OMEN_LXMF_INVITE_SENDER_ROOT
  OMEN_LXMF_INVITE_RECEIVER_ROOT
  OMEN_LXMF_INVITE_SENDER_IDENTITY
  OMEN_LXMF_INVITE_RECEIVER_IDENTITY
  OMEN_LXMF_INVITE_TCP_ENDPOINT
  OMEN_LXMF_INVITE_SERVER_DESTINATION
  OMEN_LXMF_INVITE_EVIDENCE_ROOT
)
for name in "${required[@]}"; do
  if [[ -z "${!name:-}" ]]; then
    printf 'missing required environment variable: %s\n' "$name" >&2
    exit 2
  fi
done

sender_root="$(realpath -m -- "$OMEN_LXMF_INVITE_SENDER_ROOT")"
receiver_root="$(realpath -m -- "$OMEN_LXMF_INVITE_RECEIVER_ROOT")"
evidence_root="$(realpath -m -- "$OMEN_LXMF_INVITE_EVIDENCE_ROOT")"
home_root="$(realpath -m -- "${HOME:-/nonexistent}")"
if [[ "$sender_root" == "$receiver_root" || "$sender_root" == / ||
      "$receiver_root" == / || "$sender_root" == "$home_root" ||
      "$receiver_root" == "$home_root" ]]; then
  printf 'sender and receiver roots must be distinct isolated roots, not / or HOME\n' >&2
  exit 2
fi
if [[ ! -x "$OMEN_LXMF_INVITE_BINARY" ]]; then
  printf 'binary is not executable: %s\n' "$OMEN_LXMF_INVITE_BINARY" >&2
  exit 2
fi
if [[ ! -f "$validator" ]]; then
  printf 'missing report validator: %s\n' "$validator" >&2
  exit 2
fi
if [[ -n "${OMEN_LXMF_INVITE_PRIOR_BINARY:-}" &&
      ! -x "$OMEN_LXMF_INVITE_PRIOR_BINARY" ]]; then
  printf 'prior binary is not executable: %s\n' "$OMEN_LXMF_INVITE_PRIOR_BINARY" >&2
  exit 2
fi
for identity in "$OMEN_LXMF_INVITE_SENDER_IDENTITY" "$OMEN_LXMF_INVITE_RECEIVER_IDENTITY"; do
  if [[ ! -f "$identity" || -L "$identity" ]]; then
    printf 'identity must be an existing regular non-symlink file: %s\n' "$identity" >&2
    exit 2
  fi
done
if [[ ! "$OMEN_LXMF_INVITE_SERVER_DESTINATION" =~ ^[0-9a-f]{32}$ ]]; then
  printf 'server destination must be exactly 32 lowercase hexadecimal characters\n' >&2
  exit 2
fi

mkdir -p -- "$sender_root" "$receiver_root" "$evidence_root"
common=(--backend reticulum --tcp-client "$OMEN_LXMF_INVITE_TCP_ENDPOINT")
if [[ -n "${OMEN_LXMF_INVITE_NETWORK_NAME:-}" ]]; then
  common+=(--network-name "$OMEN_LXMF_INVITE_NETWORK_NAME")
fi
if [[ -n "${OMEN_LXMF_INVITE_PASSPHRASE_FILE:-}" ]]; then
  common+=(--passphrase-file "$OMEN_LXMF_INVITE_PASSPHRASE_FILE")
fi

discovery_json="$evidence_root/receiver-discovery.json"
"$OMEN_LXMF_INVITE_BINARY" --lxmf-interop --lxmf-wait 1 \
  --app-root "$receiver_root/discovery" --identity "$OMEN_LXMF_INVITE_RECEIVER_IDENTITY" \
  "${common[@]}" --stdout >"$discovery_json"
receiver_hash="$(python3 - "$discovery_json" <<'PY'
import json, sys
with open(sys.argv[1], "r", encoding="utf-8") as handle:
    value = json.load(handle)
candidate = value.get("local", {}).get("local_lxmf_destination_hash")
if not isinstance(candidate, str) or len(candidate) != 32:
    raise SystemExit("receiver report lacked a canonical local LXMF destination")
print(candidate)
PY
)"
rm -f -- "$discovery_json"

receiver_pid=""
cleanup() {
  if [[ -n "$receiver_pid" ]] && kill -0 "$receiver_pid" 2>/dev/null; then
    kill "$receiver_pid" 2>/dev/null || true
    wait "$receiver_pid" 2>/dev/null || true
  fi
}
trap cleanup EXIT INT TERM

run_case() {
  local label="$1"
  local receiver_binary="$2"
  local expect_supported="$3"
  local probe_json="$evidence_root/${label}-probe.json"
  local receiver_stderr="$evidence_root/${label}-receiver.stderr"
  local probe_stderr="$evidence_root/${label}-probe.stderr"
  local case_receiver_root="$receiver_root/$label"
  local case_sender_root="$sender_root/$label"
  mkdir -p -- "$case_receiver_root" "$case_sender_root"

  "$receiver_binary" --lxmf-invitation-smoke "$OMEN_LXMF_INVITE_SERVER_DESTINATION" \
    --lxmf-wait 30 --app-root "$case_receiver_root" \
    --identity "$OMEN_LXMF_INVITE_RECEIVER_IDENTITY" "${common[@]}" --stdout \
    >/dev/null 2>"$receiver_stderr" &
  receiver_pid=$!
  # Process-start coordination only; the probe itself performs one bounded path
  # request and never retries.
  sleep 1

  "$OMEN_LXMF_INVITE_BINARY" \
    --lxmf-invitation-capability-probe "$receiver_hash" \
    --app-root "$case_sender_root" --identity "$OMEN_LXMF_INVITE_SENDER_IDENTITY" \
    "${common[@]}" --stdout >"$probe_json" \
    2>"$probe_stderr"

  kill "$receiver_pid" 2>/dev/null || true
  wait "$receiver_pid" 2>/dev/null || true
  receiver_pid=""
  python3 - "$receiver_hash" "$receiver_stderr" "$probe_stderr" <<'PY'
import pathlib, sys
secret = sys.argv[1]
for name in sys.argv[2:]:
    path = pathlib.Path(name)
    text = path.read_text(encoding="utf-8", errors="replace")
    path.write_text(text.replace(secret, "<peer-destination>"), encoding="utf-8")
PY
  python3 "$validator" "$probe_json" --expect "$expect_supported"
}

run_cancelled_case() {
  local probe_json="$evidence_root/cancelled-probe.json"
  local probe_stderr="$evidence_root/cancelled-probe.stderr"
  local case_sender_root="$sender_root/cancelled"
  mkdir -p -- "$case_sender_root"

  "$OMEN_LXMF_INVITE_BINARY" \
    --lxmf-invitation-capability-probe "$receiver_hash" \
    --lxmf-invitation-capability-cancel-after-ms 0 \
    --app-root "$case_sender_root" --identity "$OMEN_LXMF_INVITE_SENDER_IDENTITY" \
    "${common[@]}" --stdout >"$probe_json" 2>"$probe_stderr"
  python3 - "$receiver_hash" "$probe_stderr" <<'PY'
import pathlib, sys
secret = sys.argv[1]
path = pathlib.Path(sys.argv[2])
text = path.read_text(encoding="utf-8", errors="replace")
path.write_text(text.replace(secret, "<peer-destination>"), encoding="utf-8")
PY
  python3 "$validator" "$probe_json" --expect cancelled
}

run_cancelled_case
run_case current "$OMEN_LXMF_INVITE_BINARY" supported
if [[ -n "${OMEN_LXMF_INVITE_PRIOR_BINARY:-}" ]]; then
  run_case prior "$OMEN_LXMF_INVITE_PRIOR_BINARY" unsupported
fi

if grep -R -F -q -- "$receiver_hash" "$evidence_root"; then
  printf 'evidence redaction failed\n' >&2
  exit 1
fi

printf 'PASS: capability probe evidence contains no invitation, retry, or peer hash\n'
printf 'evidence: %s\n' "$evidence_root"
