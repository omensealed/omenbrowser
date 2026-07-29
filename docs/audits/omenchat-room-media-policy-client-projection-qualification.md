# OMENchat room media-policy client projection qualification

Date: 2026-07-28

Baseline: `release/v0.9.6-4` at `ec31023`

Status: bounded shared/client projection and static Iced presentation complete;
capability negotiation and production enforcement remain inactive

## Shared and client ownership

The standalone `omenchat-protocol` value-only `RoomPolicyProjection` now
optionally carries one `RoomUploadPolicyProjection`:

```text
no value                 capability evidence unavailable
Inherit                  authoritative nil room scalar
Disabled                 authoritative zero room scalar
MaximumFileBytes(bytes)  authoritative positive scalar, at most 10 MiB
```

This distinguishes inherited policy from a legacy or non-negotiating peer.
The value validates known policy bits, slow mode, and the upload ceiling before
admission. It owns no SQLite, Reticulum, Iced, filesystem, identity, quota,
server policy, or persistence behavior.

`ChatClient` retains the value in its existing map keyed by
`(session_id, room_id)`. The map still admits only known rooms and at most 256
room entries per session. Session removal, room-policy clearing, Link
replacement, and capability loss reuse the existing lifecycle and retain no
stale upload evidence.

## Desktop behavior

Explicit seven-field qualification parsing projects inherited, disabled, and
bounded room values. The desktop combines inherited or positive policy with
the authenticated server-wide maximum:

```text
inherit             effective server maximum
positive room value min(room maximum, server maximum)
disabled            uploads disabled
```

The Iced composer can show:

```text
Uploads ≤ 256.0 KiB · room policy
Uploads disabled · room policy
```

The evidence is static. It adds no countdown, animation, timer subscription,
poll, request, retry, or automatic resend. Authoritative disabled evidence
removes the Attach button action and changes its tooltip; text messages and
drafts remain available. The `/upload` path uses the same effective ceiling,
checks file metadata before allocating file contents, and preserves existing
draft/source behavior after rejection.

Without negotiated evidence the indicator is absent, Attach retains its
legacy behavior, and the existing authenticated server-wide ceiling remains
the local preflight. The current product runtime still selects only
`Legacy`, `PolicyBits`, or `SlowMode`; all three reject unsolicited
seven-field room values. No client requests `room-media-policy-v1`, no server
accepts it, and normal server constructors keep enforcement disabled.

## Compatibility and resource impact

Wire bytes, capability vectors, schema 13, configuration, global upload policy,
Resource handling, destinations, identities, history, and cache formats are
unchanged. This unit adds one optional fixed-size enum to each already bounded
room-policy map entry. It adds no dependency, cache, queue, worker, task,
timer, retry loop, history, media buffer, filesystem read, or recurring
network traffic.

The independently packaged omenchatd TUI continues to report its stored
configuration with `enforcement=inactive`; it does not become an attachment
client or live policy editor.

## Focused validation

Passed locally:

```text
cargo test --locked -p omenchat-protocol \
  room_upload_policy_projection -- --nocapture

1 passed; 0 failed

cargo test --locked --no-default-features --features desktop-product \
  room_media_policy_projection --lib -- --nocapture

1 passed; 0 failed

cargo test --locked --no-default-features --features desktop-product \
  room_upload_policy_indicator --lib -- --nocapture

1 passed; 0 failed

cargo test --locked --no-default-features --features desktop-product \
  negotiated_room_upload_policy --lib -- --nocapture

1 passed; 0 failed

cargo test --locked --no-default-features --features desktop-product \
  omenchat_upload_file_limit --lib -- --nocapture

1 passed; 0 failed
```

Both affected roots compile:

```text
cargo check --locked --no-default-features --features desktop-product
cargo check --locked --manifest-path src/server/Cargo.toml \
  --no-default-features --features server-full

both passed
```

Full local matrices passed:

```text
cargo test --locked -p omenchat-protocol

58 passed; 0 failed

cargo test --locked --no-default-features \
  --features desktop-product --lib

1,526 passed; 0 failed; 31 ignored

cargo test --locked --manifest-path src/server/Cargo.toml \
  --no-default-features --features server-full --lib

552 passed; 0 failed; 11 ignored
```

The ignored cases remain explicit measurement, soak, physical-hardware,
multi-process, Python, or upstream Resource gates. None is claimed as passed
here.

Formatting, both desktop product presentations, and strict Clippy also passed:

```text
cargo fmt --all --check
git diff --check
cargo check --locked --no-default-features \
  --features desktop-product-static-media
cargo clippy --locked --no-default-features \
  --features desktop-product --all-targets -- -D warnings
cargo clippy --locked --manifest-path src/server/Cargo.toml \
  --no-default-features --features server-full --all-targets -- -D warnings

all passed
```

The first exact Iced text assertion expected `512 KiB`; the implementation
correctly used the project's established `human_bytes` presentation,
`512.0 KiB`. The expectation was corrected without changing behavior.

## Remaining gates and rollback

The subsequent qualification slices now cover current/current negotiation,
restart, Link replacement, Resource rejection/success, the upstream
receiver-cancellation decision, isolated retention measurements, and native
Linux Iced attachment acceptance/rejection. Adjacent-version shape
compatibility, live-process resource/shutdown observation, Python
interoperability, hosted native platforms, packaging, and production activation
remain later gates.

Rollback removes this optional projection and static presentation without a
wire, storage, or configuration migration. Existing schema-12 copy export
remains the deeper database downgrade boundary.

The next smallest justified slice is qualification-only capability
request/acceptance and real current/current lifecycle testing. It must remain
outside canonical product graphs until mixed-version, Resource, GUI, and
resource-measurement evidence is complete.
