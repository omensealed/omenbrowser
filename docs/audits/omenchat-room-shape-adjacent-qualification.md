# OMENchat adjacent room-shape qualification

Date: 2026-07-28

Status at qualification: adjacent four-field and current negotiated five-field
process matrix passed. Slow-mode production activation was completed later in
`omenchat-slow-mode-activation.md`; this historical process result remains the
adjacent compatibility evidence.

## Goal

OMENchat protocol version 1 has three deliberately distinct room shapes:

- legacy peers receive exactly four fields;
- peers negotiating `announcement-rooms-v1` receive exactly five fields; and
- the still-qualified `room-slow-mode-v1` extension uses six fields only after
  its separate capability negotiation.

The activation gate required real adjacent-release traffic without claiming
that `v0.9.6-3` implements a capability it never shipped.

## Evidence design

The matrix pins the peeled commit of the annotated `v0.9.6-3` tag:

```text
414d8eafd1a845a986032bad993ac9c09cc378e4
```

It runs four complementary cases:

1. The current strict client connects to the immutable adjacent server. The
   capability is not accepted, no policy bits are projected, and the current
   exact-shape decoder successfully consumes the legacy four-field catalog.
2. The immutable adjacent client connects to the current server and completes
   Link open, session open, join, publication, and echo. Its parser tolerates
   extra room fields, so this is ordinary compatibility evidence only.
3. The current server's captured-transport regression requires a negotiated
   Link to receive five-field JoinAccept and RoomDelta values while a
   simultaneous legacy Link receives four fields. This is the exact reverse-
   direction shape evidence that the permissive adjacent parser cannot supply.
4. Current product binaries negotiate announcement policy over real loopback
   Reticulum Links. Both the initial Link and an orderly server restart with a
   replacement Link must project authoritative five-field policy.

This separation prevents a successful permissive decode from being mislabeled
as direct wire-shape evidence.

## Reproducible command and result

```bash
bash scripts/run-omenchat-room-shape-compatibility.sh \
  --report /tmp/omenchat-room-shape-compatibility.json
```

The local result was:

```text
status: pass
adjacent_release: v0.9.6-3
current_client_adjacent_server_legacy_four_field: true
adjacent_client_current_server_ordinary_traffic: true
current_server_four_and_five_field_shaping_regression: true
current_current_initial_five_field: true
current_current_replacement_link_five_field: true
capability_fabricated_for_adjacent_peer: false
moderation_audit_fabricated_for_adjacent_peer: false
```

The matrix was rerun on 2026-07-28 after moderation-audit Resource process
qualification. The strict current client explicitly reported
`moderation_audit_negotiated: false` against the immutable adjacent server,
while ordinary open, join, room publication, and echo still passed in both
directions. This is the correct adjacent evidence: `v0.9.6-3` never shipped
`moderation-audit-v1`, so the current product must not request, fabricate, or
send that extension to it.

All process roots, identities, Reticulum configuration, SQLite state, and TCP
ports are temporary and isolated. The reusable adjacent Cargo target contains
only build output. Reports retain public versions, the immutable source commit,
and booleans; they do not retain identities, credentials, raw frames, messages,
or private paths.

The first attempt stopped before building because the harness default used the
annotated tag object's hash rather than its peeled commit. The pin now compares
the explicit peeled commit to `v0.9.6-3^{commit}` and fails closed on any
mismatch.

Additional final-source validation:

- the complete matrix above: pass twice, including after the diagnostic-trap
  cleanup;
- `shellcheck` for both touched harnesses: pass;
- `bash -n` for both touched harnesses: pass;
- `cargo fmt --all --check`: pass;
- `bash scripts/verify-product-features.sh`: pass;
- `git diff --check`: pass.

The complete desktop/server unit and Clippy suites were not repeated because
this unit changes only shell orchestration, report assertions, and
documentation. The matrix itself compiles both current product binaries and
the immutable adjacent products, runs the affected exact server regression,
and performs the three relevant process cases. No hosted CI, package build,
Python peer, public-network peer, Windows/macOS runtime, or physical interface
result is claimed.

## Compatibility and resource impact

No protocol, capability, database schema, configuration, identity, production
feature, queue, worker, timer, cache, retry, or release artifact changed. The
existing mixed-version harness now records the current client's negative
announcement-room and moderation-audit capability evidence, but its ordinary
and restart behavior is unchanged.

The new wrapper is intentionally a manual release-candidate gate. It is not
added to quick CI because it compiles an immutable desktop release and performs
three bounded multi-process network runs. The adjacent target is reusable to
avoid recompiling that release on subsequent local runs.

Rollback removes the wrapper and report-only mixed-harness fields. No persisted
user or server data requires migration.

## Subsequent slow-mode gates

The later live-delta, Iced draft-recovery, resource-measurement, and explicit
product-activation gates passed. See
`omenchat-slow-mode-gui-qualification.md`,
`omenchat-slow-mode-resource-qualification.md`, and
`omenchat-slow-mode-activation.md`.
