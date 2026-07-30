# OMENchat room media-policy enforcement qualification

Date: 2026-07-28

Branch: `release/v0.9.6-4`

Baseline commit: `678a1a3`

## Outcome

The third staged room media-policy slice is complete. One store-owned typed
resolver now defines effective room upload behavior, and a test-only session
constructor qualifies enforcement at offer and Resource-publication
boundaries. Production constructors explicitly leave enforcement disabled.
No client or server negotiates `room-media-policy-v1`.

## Effective policy

Given the already validated global file ceiling, the resolver returns:

- room `NULL`: the global file ceiling;
- room zero: disabled;
- positive room value: the lesser of room and global ceilings;
- zero global ceiling with inherited room value: disabled;
- missing room: no policy result.

The resolver rejects a global value above the shared 10-MiB protocol maximum.
The schema already prevents invalid stored room values. A qualification-only
transactional setter validates against the same shared constant, increments
the room revision only when the scalar changes, and exists only in test builds.

## Offer boundary

The qualification path validates authorization and bounded metadata, resolves
room policy, and rejects disabled or excessive offers before command-rate or
quota admission. It then uses the existing rollback-capable command-rate
reservation and commits that reservation only after the existing bounded
pending-offer owner accepts the entry.

Consequently room-policy, global file limit, cache availability, quota, and
pending-store overload failures retain no rate admission. This replaces the
older eager rate-slot commit; it adds no refund table, retry, timer, or worker.
Non-negotiating rejection bodies remain exactly three fields.

## Publication boundary

The exact identity-bound pending offer is removed first. Exact Resource length,
current ban/mute/membership state, announcement authorization, and effective
room policy are then rechecked before entering upload serialization. A rejected
offer leaves no pending permit, file, upload-ledger row, or room event.

Accepted publication continues through the existing per-identity serializer,
same-filesystem temporary file, synchronized atomic replacement, transactional
ledger callback, post-commit eviction, and room-event path. No SQLite
transaction or policy-state lock is held across filesystem I/O; the existing
per-identity upload serializer deliberately owns the file operation.

## Compatibility and resource impact

Canonical `SessionEngine` constructors set room media-policy enforcement to
false. A stored disabled or limited value therefore does not alter legacy
upload admission yet. Capability vectors, wire shapes, typed rejection codes,
configuration, administration, UI/TUI presentation, protocol version,
identities, and storage paths are unchanged.

The resolver performs one existing indexed room lookup at each qualified offer
and publication boundary. It adds no queue, cache, task, timer, retry loop,
polling subscription, table, index, or retained history.

## Commands and results

Passed:

```text
cargo fmt --manifest-path src/server/Cargo.toml --all --check
cargo test --locked --manifest-path src/server/Cargo.toml --no-default-features --features server-headless --lib effective_room_upload_policy_is_store_owned_bounded_and_persistent -- --nocapture
cargo test --locked --manifest-path src/server/Cargo.toml --no-default-features --features server-headless --lib room_media_policy -- --nocapture
cargo test --locked --manifest-path src/server/Cargo.toml --no-default-features --features server-headless --lib upload_publication_rechecks_membership -- --nocapture
cargo test --locked --manifest-path src/server/Cargo.toml --no-default-features --features server-headless --lib
cargo clippy --locked --manifest-path src/server/Cargo.toml --no-default-features --features server-headless --all-targets -- -D warnings
cargo check --locked --manifest-path src/server/Cargo.toml --no-default-features --features server-full
```

The full standalone result was 426 passed, 0 failed, and 11 explicitly ignored
live/soak/hardware/interoperability tests.

During implementation, the first focused compile used a rusqlite convenience
method unavailable in the locked version; it was replaced with the repository's
existing explicit immediate-transaction pattern. The first persistent-engine
fixture also assumed a default room that persistent stores do not create; the
test now creates the isolated room explicitly. All focused and full gates then
passed.

## Not executed

No hosted native, Python interoperability, packaging, public Reticulum, or
long-running soak gate was triggered. Production wire/runtime behavior remains
dormant, so those expensive gates stay batched for the release candidate and
the later activation slice.

## Remaining risk and next step

The policy cannot yet be administered or reported by operators, and there is
no negotiated client evidence. The next smallest unit is the
confirmation-gated stopped-server administration command plus bounded
human/JSON status projection labeled `enforcement=inactive`. Production upload
enforcement must remain disabled in that unit.
