# OMENchat moderation-audit paging qualification

Date: 2026-07-27

Baseline: `release/v0.9.6-4` at `9ee6407`, plus this uncommitted unit.

## Scope and activation state

This unit implements the read-only paging boundary designed in
`docs/design/OMENCHAT_PINS_MODERATION_AUDIT_CHECKPOINT.md`. It does not activate
`moderation-audit-v1` in either production product:

- the desktop does not request the capability;
- omenchatd's normal constructors do not accept it;
- test-only state is required on both sides;
- no GUI/TUI control, recurring refresh, worker, timer, or persistent client
  cache was added.

## Invariants qualified

- Capability acceptance is independent of durable mutations but is attached to
  one authenticated Link and identity.
- Identity replacement, duplicate/closed Link retirement, and a new session
  negotiation clear the prior Link binding.
- Every request rechecks current moderator/admin role and current room
  membership; role loss fails before reading a page.
- Requests have an exclusive nonzero cursor and a protocol-bounded limit.
- Pages are newest-first. A short or empty page emits
  `ModerationAuditEnd`; a full page does not claim end.
- The same canonical values are used for compressed inline and Resource
  delivery. Resource purposes must begin with `moderation-audit:`.
- Malformed or unnegotiated operations fail closed.
- The client accepts inline/Resource pages and end markers only after explicit
  test-only negotiation.
- The client projection is memory-only, keyed by server and room, limited to
  1,024 records and 512 KiB, and cleared on capability loss or final session
  removal.
- Client output contains the bounded protocol fields only. It cannot expose
  identity hashes, Link identifiers, Reticulum endpoints, mutation identifiers,
  request hashes, reusable secrets, or operator text-log contents because none
  are present in the page contract.

## Commands and results

Passed:

```text
cargo fmt --all
cargo check --locked --no-default-features --features desktop-product
cargo test --locked --no-default-features --features desktop-product moderation_audit -- --nocapture
(cd src/server && cargo check --locked --no-default-features --features server-headless)
(cd src/server && cargo test --locked --no-default-features --features server-headless moderation_audit -- --nocapture)
```

Focused results:

- desktop: 3 passed, 0 failed;
- omenchatd: 14 passed, 0 failed.

Full local results:

- root tests: all executed tests passed (1,495 library tests passed, 30
  explicitly ignored; all binary/integration tests also passed);
- omenchatd tests: 372 passed, 10 explicitly ignored;
- root strict Clippy: passed with `-D warnings`;
- omenchatd strict Clippy: passed with `-D warnings`;
- omenchatd `server-full` check: passed;
- formatting and diff whitespace checks: passed.

Ignored tests are existing explicit live-Python, hardware/network, 60-second
soak, or release-mode measurement gates. None was converted to a pass.

## Not yet claimed

- a current/current multi-process fetch;
- reconnect or server-restart continuation of an audit paging session;
- adjacent-version live traffic;
- deferred Resource arrival/cancellation for this operation;
- decompression-bomb and maximum-size process measurements;
- user-facing audit presentation or production activation.

Those are Phase 12 qualification work. Until that evidence exists, production
negotiation stays disabled.

## Resource and compatibility impact

No queue, task, timer, retry, database schema, server retention policy, or wire
number changed. A successfully negotiated test client can retain at most one
page per server/room within the shared 1,024-record/512-KiB client budget.
Older peers request no capability and observe unchanged session and room
traffic.

## Rollback

Remove the paging handler, Link binding, client reducer/projection, tests, and
this document. Schema-10 rows remain valid operator-side data and require no
database rollback. Because production negotiation is still disabled, rollback
does not strand live client state or alter mixed-version behavior.
