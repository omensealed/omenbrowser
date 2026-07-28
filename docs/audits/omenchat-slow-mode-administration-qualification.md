# OMENchat slow-mode administration qualification

Date: 2026-07-28

Baseline: `release/v0.9.6-4` at `4872374`

Status: stopped-server scalar administration and status complete; capability
negotiation, production enforcement, and client projection remain inactive

## Administration boundary

omenchatd now accepts:

```text
rooms set-slow-mode <room-id> off|<1..=86400> --confirm [--home <path>]
```

The command requires a positive room ID, explicit confirmation, an existing
current-schema database, and exclusive stopped-server access. It uses the
existing bounded single-owner administrative database worker. Missing
confirmation, zero as a numeric interval, values above 86,400, malformed
values, missing rooms, missing databases, schema mismatches, and active writers
fail closed.

`off` stores zero. A changed scalar and `room_revision` commit in one immediate
transaction. A no-op retains the revision. The store returns the value read
inside that transaction with the effective room, allowing the CLI to report:

```text
previous=<off|Ns> configured=<off|Ns> revision=<n> changed=<bool>
enforcement=inactive
```

No secret, identity, database path, or admission row is printed.

## Status and compatibility

`rooms list` adds bounded `slow_mode_config=<off|Ns>` and
`slow_mode_enforcement=inactive` fields. `rooms list --json` adds the numeric
`slow_mode_seconds` and string `slow_mode_enforcement` fields. The independent
room-status schema version remains 1; this does not change the OMENchat wire,
database schema, configuration schema, protocol version, or cache version.

The explicit inactive label is essential: normal `SessionEngine` constructors
still do not enforce the stored scalar, no client requests
`room-slow-mode-v1`, and no server accepts it. Existing four-/five-field room
traffic and error shapes remain unchanged.

This unit adds no dependency, worker, queue, timer, retry loop, cache, identity,
Reticulum interface, destination, disk schema, or recurring status poll. The
existing administrative worker remains bounded to 16 queued jobs and owns its
SQLite connection.

## Validation

Focused checks cover:

- exact parser acceptance for `off` and bounded seconds;
- missing confirmation and invalid numeric/text rejection;
- active-writer refusal followed by success after writer release;
- selected isolated-home ownership;
- atomic prior/configured reporting;
- enable, idempotent no-op, disable, persistence, and revision semantics;
- bounded human/JSON status with explicit inactive enforcement; and
- the existing dormant production-session regression.

Focused commands:

```text
cargo test --locked --manifest-path src/server/Cargo.toml \
  --no-default-features --features server-headless slow_mode -- --nocapture

19 passed; 0 failed

cargo test --locked --manifest-path src/server/Cargo.toml \
  --no-default-features --features server-headless \
  cli_parses_admin_config_and_room_commands -- --nocapture

1 passed; 0 failed

cargo test --locked --manifest-path src/server/Cargo.toml \
  --no-default-features --features server-headless \
  cli_room_mutations_use_the_initialized_administrative_database_path \
  -- --nocapture

1 passed; 0 failed
```

Full standalone headless matrix:

```text
cargo test --locked --manifest-path src/server/Cargo.toml \
  --no-default-features --features server-headless

411 passed; 0 failed; 11 ignored
```

The ignored tests remain the existing explicit soaks, multiprocess/hardware or
upstream Resource cases, cancellation gate, and isolated measurements. None is
claimed as passed here.

Both standalone Clippy profiles passed with warnings denied:

```text
cargo clippy --locked --manifest-path src/server/Cargo.toml \
  --no-default-features --features server-headless --all-targets -- -D warnings
cargo clippy --locked --manifest-path src/server/Cargo.toml \
  --no-default-features --features server-full --all-targets -- -D warnings
```

The repository quick release gate also passed:

```text
bash scripts/release-check.sh quick

release check complete
```

That gate included formatting, deterministic product-feature identity, version
and Reticulum/LXMF train checks, native release CLI identity, TUI lifecycle and
PTY smoke tests, focused browser/OMENchat tests, standalone omenchatd relocation,
and the documented server feature checks. Its advisory check reported only the
two already accepted root Wayland `quick-xml` advisories
(`RUSTSEC-2026-0194` and `RUSTSEC-2026-0195`); the standalone server tree
reported none. This unit did not change either dependency tree.

## Recovery and remaining work

The schema-11 copy export remains the downgrade boundary. Disabling stores zero
without deleting admission rows; bounded maintenance later prunes expired
rows. Re-enabling before a retained deadline expires conservatively restores
that deadline in the test-only admission path.

The next slice is bounded client room-policy projection shared by GUI and TUI,
still without live capability activation. Current/current, restart,
mixed-version, Resource, UI, and measurement gates remain required before
production enforcement can be enabled.
