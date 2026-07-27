# OMENchat announcement-room client projection qualification

Date: 2026-07-27

Baseline: `release/v0.9.6-4` at `e7ffce8`, plus this client projection unit

Verdict: bounded client projection and desktop controls are implemented
locally; production capability request/acceptance remains disabled

## Scope

This unit adds a session-owned, memory-only room-policy projection to the
OMENchat client. It deliberately does not change the persisted room summary,
OMENchat database schema, application configuration, or protocol version.

The projection is populated only when:

- the client explicitly requested `announcement-rooms-v1`;
- the server explicitly accepted it; and
- each room value passes the shared exact five-field codec and known-bit
  validation.

The production session-open capability vector still does not request this
capability. The test-only request hook exists solely to qualify the dormant
path before process and adjacent-version testing.

## Client invariant

- Legacy four-field catalogs carry no authoritative policy evidence.
- Negotiated five-field catalogs and room deltas use the shared bounded codec.
- A new session acceptance clears previous policy evidence before evaluating
  the new capability result.
- Capability rejection/loss restores legacy ordinary-room presentation.
- Policy is keyed by client session, not only server identity, so one
  connection cannot clear or overwrite another connection's evidence.
- Retention is bounded by the existing per-session room catalog ceiling.
- Unknown bits and malformed negotiated room values are rejected rather than
  projected.
- Server error `1016` remains the authorization source of truth; client state
  is only an early control and presentation aid.

## Desktop behavior

For an authoritatively identified announcement room:

- standard/trusted members cannot submit the composer with Enter or Send;
- the attachment picker is disabled before file metadata/read work;
- reaction and message-revision mutation preparation is refused;
- the live request boundary rechecks messages, actions, notices, and uploads;
- uncertain durable message, reaction, and revision transmission rechecks the
  current policy;
- moderators and administrators retain publication controls;
- a compact read-only explanation is shown above the composer.

Ordinary rooms and legacy servers preserve their previous behavior. Draft text
is retained when controls are disabled.

The standalone omenchatd TUI has no member message composer. Its existing room
administration remains stopped-server CLI-only during the dormant phase; a
live policy editor or policy fanout is intentionally not added here.

## Resource and storage impact

The projection retains at most one `u64` plus map key per retained room in each
bounded client session. It adds no persistence, migration, worker, channel,
timer, retry loop, scan, network request, or recurring UI subscription.

The early upload check avoids filesystem metadata and file reads for a
read-only member. The normal server authorization remains mandatory.

## Focused validation

Passed locally:

```text
cargo check --locked --no-default-features --features desktop-product
cargo test --locked --no-default-features --features desktop-product \
  dormant_announcement_policy_projects_only_when_requested_and_clears_on_loss \
  -- --nocapture
cargo test --locked --no-default-features --features desktop-product \
  announcement_room --lib -- --nocapture
```

The projection/rejection regression proves requested-and-accepted projection,
standard-member send rejection before transport, moderator permission, and
capability-loss clearing. The shared codec regression preserves byte-exact
legacy and negotiated room values.

Full local validation also passed:

- root `cargo fmt --all --check`;
- root default/mock `cargo check --locked`;
- root desktop-product tests: 1,498 passed, 31 ignored, plus every integration
  target passed with its existing explicit ignores;
- root desktop-product all-target strict Clippy with `-D warnings`;
- `git diff --check`.

No hosted CI, Python interoperability, live Reticulum peer, package build, or
hardware run is warranted for this dormant client-only slice.

## Compatibility and rollback

There is no wire activation, persistent state, identity, destination, or server
behavior change. Reverting this unit removes the in-memory projection and
desktop controls. The already-qualified server authorization and schema-11
policy storage remain independently safe.

## Remaining activation gates

- current/current process traffic with ordinary and announcement rooms;
- room delta and replacement-Link behavior in a live process;
- adjacent-version old-client/current-server and current-client/old-server
  traffic;
- native GUI observation of member and moderator controls;
- live server policy reload/fanout design, or a documented restart-only
  activation contract;
- joint review before enabling the production capability vectors.
