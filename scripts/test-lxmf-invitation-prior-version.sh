#!/usr/bin/env bash
set -euo pipefail

readonly release_tag="v0.9.6-5"
readonly expected_commit="2a77a753e80bb8e7db24a6411d923bf14a8e8722"

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

command -v git >/dev/null 2>&1 || {
  printf 'ERROR: git is required for the prior-version source gate\n' >&2
  exit 2
}
command -v python3 >/dev/null 2>&1 || {
  printf 'ERROR: python3 is required for the prior-version source gate\n' >&2
  exit 2
}

actual_commit="$(git rev-parse "${release_tag}^{commit}" 2>/dev/null || true)"
if [[ "$actual_commit" != "$expected_commit" ]]; then
  printf 'ERROR: %s must resolve to reviewed commit %s (found %s)\n' \
    "$release_tag" "$expected_commit" "${actual_commit:-missing}" >&2
  exit 2
fi

evidence_root="$(mktemp -d "${TMPDIR:-/tmp}/omen-lxmf-invite-prior.XXXXXX")"
trap 'rm -rf "$evidence_root"' EXIT
umask 077

git show "${release_tag}:src/app.rs" >"$evidence_root/app.rs"
git show "${release_tag}:src/chat/handoff.rs" >"$evidence_root/handoff.rs"
git show "${release_tag}:src/messaging/message.rs" >"$evidence_root/message.rs"
if git grep -n -E \
  'OmenChatInvitePayload|OMENCHAT_INVITE_PROTOCOL|is_lxmf_omenchat' \
  "$release_tag" -- ':!src/chat/handoff.rs' ':!docs/**' \
  ':!official-sources/**' >"$evidence_root/callers.txt"; then
  :
fi

python3 - "$evidence_root" <<'PY'
from pathlib import Path
import sys

root = Path(sys.argv[1])
app = (root / "app.rs").read_text(encoding="utf-8")
handoff = (root / "handoff.rs").read_text(encoding="utf-8")
message = (root / "message.rs").read_text(encoding="utf-8")
callers = (root / "callers.txt").read_text(encoding="utf-8").strip()

start_marker = "crate::runtime::RuntimeBusEvent::MessageReceived(message) => {"
end_marker = "crate::runtime::RuntimeBusEvent::MessageDeliveryUpdated(status) => {"
reducers = []
cursor = 0
while True:
    start = app.find(start_marker, cursor)
    if start < 0:
        break
    end = app.find(end_marker, start + len(start_marker))
    if end >= 0:
        reducers.append(app[start:end])
    cursor = start + len(start_marker)
matching = [block for block in reducers if "ingest_runtime_message(message)" in block]
if len(matching) != 1:
    raise SystemExit(
        f"expected exactly one reviewed persistence reducer, found {len(matching)}"
    )
reducer = matching[0]

required = (
    "pub struct OmenChatInvitePayload",
    'pub const OMENCHAT_INVITE_PROTOCOL: &str = "omenchat.lxmf.invite"',
)
for marker in required:
    if marker not in handoff:
        raise SystemExit(f"reviewed release invitation declaration changed: {marker}")
if callers:
    raise SystemExit(
        "reviewed release unexpectedly calls the dormant invitation contract:\n" + callers
    )
if "is_lxmf_omenchat_invitation_message" in reducer or "OMENCHAT_INVITE_PROTOCOL" in reducer:
    raise SystemExit("reviewed release unexpectedly recognizes invitation control messages")
if "LXMF_SOURCE_AUTHENTICATED_FIELD" in message:
    raise SystemExit("reviewed release unexpectedly exposes current authenticated-source evidence")

print("PASS: v0.9.6-5 treats omenchat.lxmf.invite as an ordinary persisted inbound message")
print("CLASSIFICATION: current-to-prior outbound invitation is incompatible and remains disabled")
print("NOTE: source-level deterministic evidence is not a live mixed-version transport pass")
PY
