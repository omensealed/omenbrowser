#!/usr/bin/env bash
set -euo pipefail
umask 077

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
if [[ "$sender_root" == "$receiver_root" || "$sender_root" == / || "$receiver_root" == / ||
      "$sender_root" == "$home_root" || "$receiver_root" == "$home_root" ]]; then
  printf 'sender and receiver roots must be distinct explicit isolated roots, not / or HOME\n' >&2
  exit 2
fi
if [[ ! -x "$OMEN_LXMF_INVITE_BINARY" ]]; then
  printf 'binary is not executable: %s\n' "$OMEN_LXMF_INVITE_BINARY" >&2
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

wait_secs="${OMEN_LXMF_INVITE_WAIT_SECS:-30}"
if [[ ! "$wait_secs" =~ ^[0-9]+$ ]] || (( wait_secs < 1 || wait_secs > 300 )); then
  printf 'OMEN_LXMF_INVITE_WAIT_SECS must be an integer from 1 through 300\n' >&2
  exit 2
fi

mkdir -p -- "$sender_root" "$receiver_root" "$evidence_root"
receiver_json="$evidence_root/receiver.json"
sender_json="$evidence_root/sender.json"
receiver_stderr="$evidence_root/receiver.stderr"
sender_stderr="$evidence_root/sender.stderr"
discovery_json="$evidence_root/receiver-discovery.json"

common=(--backend reticulum --tcp-client "$OMEN_LXMF_INVITE_TCP_ENDPOINT")
if [[ -n "${OMEN_LXMF_INVITE_NETWORK_NAME:-}" ]]; then
  common+=(--network-name "$OMEN_LXMF_INVITE_NETWORK_NAME")
fi
if [[ -n "${OMEN_LXMF_INVITE_PASSPHRASE_FILE:-}" ]]; then
  common+=(--passphrase-file "$OMEN_LXMF_INVITE_PASSPHRASE_FILE")
fi

"$OMEN_LXMF_INVITE_BINARY" --lxmf-interop --lxmf-wait 1 \
  --app-root "$receiver_root" --identity "$OMEN_LXMF_INVITE_RECEIVER_IDENTITY" \
  "${common[@]}" --stdout >"$discovery_json"

receiver_hash="$(python3 - "$discovery_json" <<'PY'
import json, sys
with open(sys.argv[1], "r", encoding="utf-8") as handle:
    value = json.load(handle)
candidate = value.get("local", {}).get("local_lxmf_destination_hash")
if not isinstance(candidate, str) or len(candidate) != 32:
    raise SystemExit("receiver report did not contain a canonical local LXMF destination")
print(candidate)
PY
)"

receiver_pid=""
cleanup() {
  if [[ -n "$receiver_pid" ]] && kill -0 "$receiver_pid" 2>/dev/null; then
    kill "$receiver_pid" 2>/dev/null || true
    wait "$receiver_pid" 2>/dev/null || true
  fi
}
trap cleanup EXIT INT TERM

"$OMEN_LXMF_INVITE_BINARY" \
  --lxmf-invitation-smoke "$OMEN_LXMF_INVITE_SERVER_DESTINATION" \
  --lxmf-wait "$wait_secs" --app-root "$receiver_root" \
  --identity "$OMEN_LXMF_INVITE_RECEIVER_IDENTITY" "${common[@]}" --stdout \
  >"$receiver_json" 2>"$receiver_stderr" &
receiver_pid=$!

"$OMEN_LXMF_INVITE_BINARY" \
  --lxmf-invitation-smoke "$OMEN_LXMF_INVITE_SERVER_DESTINATION" \
  --send-lxmf-smoke "$receiver_hash" --lxmf-wait "$wait_secs" \
  --app-root "$sender_root" --identity "$OMEN_LXMF_INVITE_SENDER_IDENTITY" \
  "${common[@]}" --stdout >"$sender_json" 2>"$sender_stderr"

wait "$receiver_pid"
receiver_pid=""

python3 - "$sender_json" "$receiver_json" <<'PY'
import json, sys
with open(sys.argv[1], "r", encoding="utf-8") as handle:
    sender = json.load(handle)
with open(sys.argv[2], "r", encoding="utf-8") as handle:
    receiver = json.load(handle)
if sender.get("send", {}).get("ok") is not True:
    raise SystemExit("sender did not submit the invitation; inspect sender.json/stderr")
received = receiver.get("receive", {})
if received.get("preview_observed") is not True:
    raise SystemExit("receiver did not observe the invitation preview")
if received.get("authenticated_sender_match") is not True:
    raise SystemExit("receiver preview lacked authenticated sender match")
if received.get("history_persisted") is not False or received.get("connection_action_invoked") is not False:
    raise SystemExit("receiver violated preview-only invariants")
print("PASS: live native LXMF invitation reached an authenticated preview without history or connection action")
PY

printf 'evidence: %s\n' "$evidence_root"
