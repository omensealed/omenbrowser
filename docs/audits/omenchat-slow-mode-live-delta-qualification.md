# OMENchat slow-mode live room-delta qualification

Date: 2026-07-28

Status: isolated current/current real-Link transition passed; product slow-mode
activation remains off.

## Goal and result

The prior real-Link gate proved an already configured slow-mode interval at
session open, typed rejection after restart, and expiry readmission. It did not
prove that an already connected client learns about a policy change without
reconnecting.

The qualification process now starts with lobby slow mode disabled, negotiates
`durable-mutations-v1` and `room-slow-mode-v1`, joins over a real Reticulum
Link, and records the initial zero-second projection. The live server then:

1. commits a 30-second interval on its existing SQLite owner;
2. creates one authoritative `RoomDelta`;
3. shapes the room value independently for every bounded live session; and
4. disarms the one-shot transition.

The connected client observes the delta change from 0 to 30 seconds before its
first publication. That publication is committed. The existing orderly restart
then proves the next publication is rejected with typed `SlowModeActive`, and
the expiry run proves later readmission.

## Safety boundary

Normal live administration still refuses a second SQLite maintenance writer.
The gate does not weaken `rooms set-slow-mode` or make it safe to run against an
active server.

The transition exists only under `omenchat-slow-mode-qualification`. Its
non-secret environment input is read only by that feature build. The operation
uses the existing serialized live-server worker and existing database
connection; it waits for an authenticated, negotiated, joined client and then
runs once. It adds no production CLI, configuration key, schema, protocol
operation, worker, channel, cache, retry loop, or recurring timer. The bounded
live run loop already owns the readiness check and shutdown.

Canonical product aliases continue to reject the qualification feature.

## Compatibility

The policy update broadcasts one project-owned `RoomDelta`. Existing
per-session shaping remains authoritative:

- slow-mode negotiated sessions receive the six-field shape;
- announcement-policy-only sessions receive five fields; and
- legacy sessions receive four fields.

No wire capability, error number, database schema, identity, state directory,
or production default changed. Rollback is a source revert with no data
migration.

## Validation

The isolated process gate is:

```text
bash scripts/run-omenchat-slow-mode-qualification.sh \
  --report /tmp/omenchat-slow-mode-delta-report.json
```

The generated evidence reports:

```text
status: pass
connected_room_delta: true
initial_commit: true
replacement_link_typed_rejection: true
expiry_readmission: true
server_restart: true
server_destination_stable: true
```

Focused tests cover the readiness guard, six-field delta shaping, persistence,
and idempotent disarming. Root CLI tests cover bounded delta-smoke parsing and
isolation from the rejection and other mutation cases.

Validation completed locally:

- `cargo fmt --all --check`: pass.
- Root and standalone qualification checks and no-run test builds: pass.
- Focused root CLI and server live-delta tests: pass.
- `cargo test --locked --no-default-features --features desktop-product`: pass.
- Standalone `server-headless` tests: 413 passed, 11 explicit soaks ignored.
- Strict product and qualification Clippy profiles for both Cargo roots: pass.
- Strict TUI-only Clippy profile: pass.
- `bash scripts/release-check.sh quick`: initially exposed a TUI-only compile
  dependency on the feature-gated client module. The validator now reads the
  bound from the always-present shared protocol crate; the exact TUI-only
  profile and the complete quick gate then passed.
- `git diff --check`: pass.

No hosted CI, Python interoperability, package build, public-network peer, or
physical-interface result is claimed.

## Remaining activation gates

- Run adjacent released-binary four-/five-field process compatibility.
- Observe the policy change and rejection recovery in the real Iced GUI.
- Record real client/server CPU, RSS, link, and queue measurements.
- Make an explicit product activation and rollback decision.
