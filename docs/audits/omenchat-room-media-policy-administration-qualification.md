# OMENchat room media-policy administration qualification

Date: 2026-07-28

Baseline: `release/v0.9.6-4` at `9080b54`

Status: stopped-server administration and bounded operator status complete;
desktop projection, capability negotiation, and production enforcement remain
inactive

## Administration boundary

omenchatd now accepts:

```text
rooms set-upload-policy <room-id> inherit|disabled|<1..=10485760> \
  --confirm [--home <path>]
```

The command requires a positive room ID, explicit confirmation, an existing
current-schema database, and exclusive stopped-server access. It uses the
existing bounded single-owner administrative database worker. Missing
confirmation, numeric zero, values above 10 MiB, malformed values, missing
rooms, missing databases, schema mismatches, and active writers fail closed.

`inherit` stores `NULL`, `disabled` stores zero, and a positive byte value
stores that room ceiling. The nullable scalar and `room_revision` commit in one
immediate transaction. A no-op retains the revision. The store returns the
previous value and committed room read inside the transaction, allowing the
CLI to report:

```text
previous=<inherit|disabled|NB> configured=<inherit|disabled|NB>
effective=<disabled|NB> revision=<n> changed=<bool> enforcement=inactive
```

The effective value is the room/global minimum. A disabled room or a disabled
global policy resolves to disabled. No secret, identity, database path, or
upload-ledger row is printed.

## Status, TUI, and compatibility

`rooms list` and `rooms list --json` distinguish the stored policy from the
effective policy and report `upload_policy_enforcement=inactive`. The JSON
projection also exposes the nullable stored/effective numeric values.
Projection retains at most 1,024 rooms and 1 MiB of room names, topics, and
row data, then reports truncation explicitly.

The independently packaged omenchatd TUI retains the scalar in its already
bounded room cache and shows configured upload policy with
`enforcement=inactive`. It adds no policy editor, polling loop, or runtime
activation.

The explicit inactive label is essential: normal `SessionEngine` constructors
still keep room media-policy enforcement disabled, no client requests
`room-media-policy-v1`, and no server accepts it. Existing room wire shapes,
upload offers, Resource publication, global upload policy, database schema 13,
configuration schema, protocol version, and cache versions are unchanged.

This unit adds no dependency, worker, queue, timer, retry loop, cache,
Reticulum interface, destination, identity, persistent schema, or recurring
status poll. It reuses the administrative worker's bounded 16-job channel and
single SQLite owner.

## Validation

Focused checks cover:

- strict parser acceptance for `inherit`, `disabled`, and bounded bytes;
- missing confirmation and invalid numeric/text rejection;
- active-writer refusal followed by success after writer release;
- selected isolated-home ownership;
- atomic prior/configured values and injected pre-commit rollback;
- changed and idempotent revision behavior;
- inherited, disabled, and room/global-minimum effective values;
- missing-room failure;
- bounded human/JSON projections with explicit truncation;
- bounded TUI projection with explicit inactive enforcement; and
- the existing dormant production-session regression.

Focused commands:

```text
cargo test --locked --manifest-path src/server/Cargo.toml \
  --no-default-features --features server-full \
  room_upload_policy --lib -- --nocapture

3 passed; 0 failed

cargo test --locked --manifest-path src/server/Cargo.toml \
  --no-default-features --features server-full \
  upload_policy_cli --lib -- --nocapture

1 passed; 0 failed

cargo test --locked --manifest-path src/server/Cargo.toml \
  --no-default-features --features server-full \
  room_status_projection --lib -- --nocapture

2 passed; 0 failed

cargo test --locked --manifest-path src/server/Cargo.toml \
  --no-default-features --features server-full \
  dashboard_room_projection --lib -- --nocapture

1 passed; 0 failed

cargo test --locked --manifest-path src/server/Cargo.toml \
  --no-default-features --features server-full \
  cli_parses_release_runbook_server_commands --lib -- --nocapture

1 passed; 0 failed
```

The first in-process active-writer test revision attempted to reopen the
database for exclusive maintenance immediately after the short-lived
administrative worker returned and observed SQLite still locked. The
post-command assertion now uses a read-only connection; the actual CLI process
exits after the command, while stopped-server exclusivity remains covered
before mutation. This was a test-lifetime race, not a state or durability
failure.

Full standalone server library matrix:

```text
cargo test --locked --manifest-path src/server/Cargo.toml \
  --no-default-features --features server-full --lib

552 passed; 0 failed; 11 ignored
```

The ignored tests remain explicit soaks, multiprocess/hardware or upstream
Resource cases, cancellation qualification, and isolated measurements. None
is claimed as passed here.

Formatting, the headless compile profile, and both standalone Clippy profiles
also passed:

```text
cargo fmt --manifest-path src/server/Cargo.toml --all --check
git diff --check
cargo check --locked --manifest-path src/server/Cargo.toml \
  --no-default-features --features server-headless
cargo clippy --locked --manifest-path src/server/Cargo.toml \
  --no-default-features --features server-headless --all-targets -- -D warnings
cargo clippy --locked --manifest-path src/server/Cargo.toml \
  --no-default-features --features server-full --all-targets -- -D warnings

all passed
```

## Recovery and remaining work

The schema-12 copy export remains the downgrade boundary. An inherited value
restores global behavior without deleting any upload or ledger entry. The
subsequent production activation and rollback decision is recorded in
`omenchat-room-media-policy-activation-review.md`; this paragraph describes
the earlier administration-only checkpoint.

Hosted native, Python interoperability, package, live Reticulum Resource,
physical-network, and GPU measurements were not run for this local
administration boundary. They remain release-candidate gates, not claimed
results.

The later client, Resource, GUI, measurement, adjacent-version, and activation
gates are complete. Hosted release-candidate evidence remains separate.
