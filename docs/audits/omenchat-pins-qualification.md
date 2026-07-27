# OMENchat dormant pins qualification

Date: 2026-07-27  
Branch: `release/v0.9.6-4`  
Baseline commit: `fd16a00`  
Host: Linux 7.1.3 x86_64  
Toolchain: rustc 1.97.0, cargo 1.97.0

## Scope and verdict

This audit qualifies the dormant `room-pins-v1` implementation before any
production capability activation. It changes no operation assignment, schema,
retention limit, production client request, or server acceptance default.

Verdict: deterministic gates are ready for a separate activation review.
Release qualification is not complete. Production still does not negotiate the
capability, so a real current/current two-client pin/unpin/restart smoke is not
yet possible and was not claimed.

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
- dormant capability rejection;
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
production session-open regression proves the client omits
`room-pins-v1`, and the standalone session regression proves the server refuses
it.

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
```

The complete formatting, check, test, strict-Clippy, protocol, release, and
packaging commands remain documented in `docs/TESTING.md`.

## Remaining evidence and activation boundary

- Review client request and server acceptance activation as one separately
  reversible risk class.
- Preserve fail-closed unsolicited acceptance, base-only peer, downgrade,
  identity replacement, capability loss, and Link retirement behavior.
- After activation, run an isolated current/current two-client smoke covering
  pin, semantic no-op, unpin, deliberately lost acknowledgement, exact replay,
  forced snapshot reconciliation, graceful omenchatd restart, replacement
  Link, and clean durable-intent completion.
- Run adjacent-version ordinary traffic and the normal release resource gates.
- Do not describe an acknowledgement as authoritative pin state; the client
  must continue waiting for the matching delta or explicit snapshot.

Rollback before activation is code-only plus the existing guarded schema-8
copy. After activation, disable request and acceptance first while retaining
schema-9 state and unresolved durable intents for operator reconciliation.
