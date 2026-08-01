# OMENbrowser v0.9.6-5 Phase 5 unit 6 report

Date: 2026-07-31 UTC  
Branch: `hardening/v0.9.6-6-phase-plan`  
Baseline SHA: `2a77a753e80bb8e7db24a6411d923bf14a8e8722`

## Outcome

Added the first dormant asynchronous OMENchat LXMF notice envelope. It is a
bounded numeric pointer, not a chat-message transport, and has no production
caller.

## Design

The proposed capability `omenchat-lxmf-notices-v1` is deliberately separate
from invitation support. The version-1 payload is capped at 1 KiB and covers
offline mentions, directed moderation, planned maintenance, and followed-room
summaries. It contains no free text, room history, attachment, token, role,
display name, URL, command, or arbitrary extension map.

Notice IDs are canonical 128-bit random values represented by 32 lowercase hex
characters. Server destinations are canonical lowercase hashes. Kind-specific
room/event/count/timestamp fields, five-minute clock skew, and a seven-day
maximum lifetime are validated before encoding and after decoding.

## Files changed

- `src/chat/mod.rs`
- `src/chat/notice.rs`
- `docs/design/OMENCHAT_LXMF_NOTICES_CHECKPOINT.md`
- `docs/TESTING.md`
- `docs/reviews/OMENBROWSER_V0965_PHASE5_UNIT6.md`

## Compatibility, storage, and resources

No network, capability endpoint, message title recognition, persistence,
database, configuration, UI, task, timer, queue, retry, download, or automatic
action was added. Existing and older peers receive nothing. The codec performs
one bounded JSON encode/decode and retains only the caller-owned value.

There is no migration. Rollback removes the module/export and documentation.

## Validation

Passed focused tests:

```text
cargo test --locked --no-default-features --features desktop-product \
  chat::notice --lib
cargo clippy --locked --no-default-features --features desktop-product \
  --all-targets -- -D warnings
cargo fmt --all --check
git diff --check
```

Four tests cover round trip and forbidden fields, exact kind shapes, temporal
and canonical bounds, unknown fields, and pre-decode byte rejection.

Live LXMF, capability negotiation, mixed versions, external daemon, Python
interoperability, packaging, non-Linux platforms, and hardware were not run and
are not claimed.

## Next gate

The next safe unit is a caller-inert admission owner with authoritative sender
evidence, per-kind opt-in, item/byte/rate bounds, deduplication, coalescing, and
incremental pruning. It must still have no runtime or UI caller until
capability/mixed-version evidence exists.
