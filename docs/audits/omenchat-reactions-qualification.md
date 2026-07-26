# OMENchat dormant reactions qualification

Date: 2026-07-26  
Branch: `release/v0.9.6-4`  
Baseline commit: `1864fca`  
Host: Linux 7.1.3 x86_64  
Toolchain: rustc 1.97.0, cargo 1.97.0

## Scope and verdict

This audit qualifies the dormant `reactions-v1` implementation before any
production capability activation. It changes no wire assignment, schema,
retention limit, production client request, or server acceptance default.

Verdict: deterministic gates are ready for a separate activation commit.
Release qualification is not complete. Production still does not negotiate the
capability, and the required isolated two-client add/remove/restart/Resource
smoke remains pending until activation is deliberately staged.

## Deterministic evidence

The root reaction filter covers the shared wire fixture, bounded client cache,
strict delta/snapshot parsing, inline and Resource decoding, overload rollback,
restart behavior, presentation, durable intent restart, capability loss,
non-optimistic send, and exact acknowledgement matching.

The standalone server reaction filter covers:

- byte-exact shared wire fixtures;
- dormant capability rejection;
- membership, target, ban, and mute policy;
- atomic commit, exact replay, changed-content conflict, and restart;
- semantic no-op without another audit event or fan-out;
- inline and Resource snapshot equality;
- same-room, capability-bound fan-out;
- schema 4 to 5 migration and every injected reaction migration boundary;
- schema-4 downgrade-copy preservation;
- actor/target limits and recovery by removal;
- exact server-global active-row rejection without partial state;
- age-pruning and full non-expired room-audit replacement;
- authoritative sorted snapshots including explicit empty targets.

Adjacent ordinary traffic remains byte-exact against both the `0.6.0-1` and
`0.9.6-3` fixtures in the independent client and server codecs. The production
session-open test still proves that the client requests durable mutations,
notice acknowledgement, and replies/mentions, but not reactions. The server
constant `REACTIONS_SERVER_ENABLED` remains `false`.

## Isolated measurements

Measurements use unique files below the operating-system temporary directory,
checkpoint SQLite before sizing, remove database/WAL/SHM files afterward, and
are ignored during normal test runs. Times are observations from this host, not
release thresholds.

Default reaction-state run, 1,024 active rows:

```text
active bytes:       29,696
audit bytes:        38,912
snapshot pages:     4
snapshot entries:   1,024
database bytes:     344,064
mutation p50/p95:   1,543 / 2,425 us
semantic no-op p50: 165 us
snapshot p50/p95:   1,750 / 1,900 us
```

Exact room item ceiling, 4,096 active rows:

```text
active bytes:       118,784 of 131,072
audit bytes:        155,648 of 524,288
snapshot pages:     16
snapshot entries:   4,096
database bytes:     880,640
mutation p50/p95:   4,509 / 8,121 us
semantic no-op p50: 170 us
snapshot p50/p95:   1,728 / 1,864 us
```

The existing 1,024-item client durable-intent measurement recovered all 1,024
rows, pruned all terminal rows in eight bounded calls, and produced a
364,544-byte checkpointed database. The existing server durable-replay
measurement retained the configured 512 of 1,024 results, retired the other
512 client instances, and produced a 462,848-byte checkpointed database.

## Commands

```bash
cargo test --locked --no-default-features --features desktop-dev reaction --lib

cd src/server
cargo test --locked --no-default-features --features server-headless reaction --lib
cargo test --locked --no-default-features --features server-headless \
  reaction_state_retention_measurement --lib -- --ignored --nocapture
OMEN_REACTION_MEASUREMENT_ITEMS=4096 \
  cargo test --locked --no-default-features --features server-headless \
  reaction_state_retention_measurement --lib -- --ignored --nocapture
```

The durable-intent, durable-replay, adjacent-version fixture, formatting,
Clippy, and full root/server commands are recorded in `docs/TESTING.md`.

## Remaining evidence

- Client request and server acceptance were subsequently enabled together in
  one separately reversible activation unit. Explicit-request, unsolicited
  acceptance, base-only peer, downgrade, and live Link-binding regressions
  pass.
- Deterministic root and standalone-server matrices were re-run after
  activation.
- The isolated current/current smoke passed add, remove, semantic no-op, a
  deliberately lost acknowledgement, exact replay, two independent clients,
  graceful server restart, and forced Resource snapshots on 2026-07-26. Local
  evidence was retained under
  `/tmp/omenchat-reactions-v0964-full-fixed/omenchat-smoke-20260726T132850Z`.
- Capability-absent behavior remains covered by the explicit link-scoped
  negotiation regression; ordinary traffic is unchanged.
- Continuous same-process Link replacement remains covered by the existing
  reconnect smoke and deterministic replacement-Link replay tests; it was not
  combined with the mutually exclusive graceful-restart mode in this run.
