# OMENchat dormant pins qualification

Date: 2026-07-27  
Branch: `release/v0.9.6-4`  
Baseline commit: `fd16a00`  
Host: Linux 7.1.3 x86_64  
Toolchain: rustc 1.97.0, cargo 1.97.0

## Scope and verdict

This audit first qualified the dormant `room-pins-v1` implementation before
production capability activation. A subsequent separately reversible slice
enabled client request and server acceptance together without changing an
operation assignment, schema, retention limit, queue, worker, timer, or retry.

Verdict: deterministic, activation, and isolated current/current live process
gates pass. The live gate uses one continuously running client across an
orderly omenchatd restart and replacement Link; no automatic uncertain
mutation retry is enabled.

## Deterministic evidence

The root pin filter covers:

- the shared byte-exact wire fixture;
- bounded identity-scoped client persistence and restart-stale restoration;
- exact-target authority, delta, inline snapshot, and Resource snapshot
  decoding;
- cached-versus-current presentation;
- durable intent restart without automatic resend;
- capability, role, membership, target, and authority gating;
- persistence before transport admission;
- one pending mutation per target within the existing bounded mutation budget;
- non-optimistic send and exact acknowledgement matching;
- capability and Link loss clearing the pending confirmation state.

The standalone server pin filter covers:

- the independent byte-exact wire fixture;
- durable-dependent explicit capability acceptance and pin-only rejection;
- moderator/administrator, membership, target, tombstone, and room policy;
- atomic state/audit/replay commit, exact restart replay, changed-content
  conflict, and semantic no-op;
- same-room, identity-matched capable-Link fan-out;
- authoritative sorted snapshots and history snapshot scoping;
- schema-8 to schema-9 migration and every injected migration boundary;
- schema-8 downgrade-copy preservation;
- dependency-aware room-history compaction;
- per-room and exact global active-state saturation without partial state;
- age pruning capped at 64 rows while preserving active audit evidence;
- full non-expired per-room and global audit replacement by one eligible row;
- non-reused autoincrement event identifiers;
- transaction rollback;
- maximum 256-target/64-entry snapshot encoding below the 1 MiB frame ceiling.

Adjacent ordinary traffic remains byte-exact against the existing `0.6.0-1`
and `0.9.6-3` fixtures in the independent client and server codecs. The
production session-open regression proves the client requests
`room-pins-v1` only with its persistent durable identity. Client unsolicited
acceptance/downgrade and server pin-only-request regressions remain fail closed.

## Isolated measurement

The ignored measurement uses a unique SQLite database below the operating
system temporary directory, checkpoints it before sizing, and removes the
database/WAL/SHM files afterward. Times are observations from this host, not
release thresholds.

Exact per-room active-pin ceiling, 64 active rows:

```text
active bytes:       2,048
audit bytes:        2,624
snapshot pages:     1
snapshot entries:   64
database bytes:     237,568
mutation p50/p95:   621 / 664 us
semantic no-op p50/p95: 261 / 282 us
snapshot p50/p95:   649 / 649 us
```

The state and audit retained-byte values match their explicit 32-byte and
41-byte row accounting. The measurement adds no timer, worker, queue, cache,
or production path.

## Commands

```bash
# Root, from repository root
cargo test --locked --no-default-features --features desktop-product pin --lib

# Standalone server
cd src/server
cargo test --locked --no-default-features --features server-headless pin --lib
cargo test --locked --no-default-features --features server-headless \
  pin_state_retention_measurement --lib -- --ignored --nocapture

# Isolated live current/current qualification, from repository root
bash scripts/release-omenchat-smoke.sh \
  --browser-bin target/debug/omenbrowser_rs \
  --server-bin src/server/target/debug/omenchatd \
  --tcp 127.0.0.1:<unused-port> \
  --path-wait 45 \
  --out <isolated-output-root> \
  --message "pin reconnect qualification" \
  --pin-smoke \
  --continuous-client-reconnect
```

The complete formatting, check, test, strict-Clippy, protocol, release, and
packaging commands remain documented in `docs/TESTING.md`.

## Live current/current evidence

The 2026-07-27 isolated run passed all of these initial-Link and
replacement-Link stages:

- durable and `room-pins-v1` capability negotiation;
- exact-target authority synchronization;
- moderator-authorized pin;
- deliberately withheld acknowledgement and exact durable replay;
- authoritative pin snapshot;
- semantic no-op pin;
- unpin and authoritative absence snapshot;
- zero nonterminal persistent mutation intents;
- orderly server shutdown, stable server identity, Link closure, replacement
  Link, session restoration, and post-reconnect room traffic.

The harness creates isolated browser/server roots, first registers the browser
identity as an ordinary user, stops omenchatd, assigns that isolated user the
moderator role through omenchatd's local admin console, and restarts it. It
does not bypass production authorization. The retained local report for the
qualification run was:

```text
outcome: pass
continuous_client_reconnect: 1
pin_smoke: 1
continuous_link_closed: 1
continuous_link_reopened: 1
continuous_session_reconnected: 1
continuous_message_echoed: 1
restart_destination_stable: 1
restart_stop: orderly
```

The live run exposed and now guards one real boundary mismatch:
`SessionEngine::pin_snapshot_frame` emitted raw snapshot fields while the
client's `ChatOp::PinSnapshot` transport path expects the same bounded
compressed inline batch used by its existing decoder. The server now emits
that bounded compressed inline body. Focused session/live tests decode the
actual emitted form, and the process test proves the independently built
desktop receives and applies it. Pin snapshots remain deliberately inline;
the forced-Resource threshold applies to history, reaction, and revision
batches, not to the protocol's bounded pin snapshot.

## Remaining evidence and activation boundary

- Client request and server acceptance were enabled together in one separately
  reversible risk class; deterministic negotiation regressions pass.
- Preserve fail-closed unsolicited acceptance, base-only peer, downgrade,
  identity replacement, capability loss, and Link retirement behavior.
- Run adjacent-version ordinary traffic and the normal release resource gates.
- Do not describe an acknowledgement as authoritative pin state; the client
  must continue waiting for the matching delta or explicit snapshot.

Rollback before activation is code-only plus the existing guarded schema-8
copy. After activation, disable request and acceptance first while retaining
schema-9 state and unresolved durable intents for operator reconciliation.
