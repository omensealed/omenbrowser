# OMENchat Room Media-Policy Resource Qualification

Date: 2026-07-28

## Scope and decision

This slice qualifies real upload admission and Resource publication under the
non-product `room-media-policy-v1` feature. Canonical desktop and omenchatd
profiles remain unchanged and reject the qualification feature.

Enforcement is not process-global. `OmenchatLiveServer` derives authority from
the current authenticated Link binding and passes it explicitly to both upload
offer admission and Resource publication. Identity replacement, Link close,
session replacement, reconnect, or capability loss removes the binding.
Direct engine tests retain a separate explicit enforcement constructor.

## Typed rejection

The shared protocol owns stable typed reasons:

- `1`: room uploads disabled;
- `2`: room file-size ceiling exceeded.

Negotiated policy rejection preserves the existing reason, effective-limit,
and incoming-byte fields and appends the numeric reason. The client exposes the
typed reason only while that session has negotiated media policy. Unknown
codes and unsolicited fourth fields remain generic rejection; human text is
never parsed to drive controls. Legacy and non-negotiating peers retain the
three-field shape and prior upload admission behavior.

## Real-process evidence

`scripts/run-omenchat-room-media-policy-qualification.sh` builds both
independent roots and runs three isolated loopback homes:

1. With a 256-KiB room ceiling, a 64-KiB upload is accepted, transferred as a
   real Resource, committed, fetched back, and remains available across an
   orderly omenchatd restart. The restarted Link renegotiates and projects the
   same ceiling.
2. With the same ceiling, a 300,000-byte offer receives typed reason `2`
   before Resource acceptance. Doctor confirms zero upload rows/files.
3. With uploads disabled, a 64-KiB offer receives typed reason `1` before
   Resource acceptance. Doctor again confirms zero upload rows/files.

The observed report was:

```json
{
  "cumulative_capabilities": true,
  "disabled_ledger_clean": true,
  "disabled_typed_rejection": true,
  "isolated_loopback": true,
  "message_round_trip": true,
  "over_limit_ledger_clean": true,
  "over_limit_typed_rejection": true,
  "qualification_feature_only": true,
  "real_link": true,
  "restart_projection_recovered": true,
  "room_media_policy_negotiated": true,
  "room_upload_max_file_bytes": 262144,
  "status": "pass",
  "under_limit_resource_committed": true,
  "under_limit_resource_fetched": true
}
```

Each process case owns isolated browser, Reticulum, identity, SQLite, upload,
cache, and server roots. Startup and client waits are bounded; cleanup is
owned by the existing release-smoke trap.

## Commands and results

Passed:

```bash
cargo test --locked -p omenchat-protocol \
  room_upload_reject_reason_codes -- --nocapture
# 1 passed

cargo test --locked --manifest-path src/server/Cargo.toml \
  --no-default-features \
  --features server-headless,omenchat-room-media-policy-qualification \
  room_media_policy --lib -- --nocapture
# 8 passed

cargo test --locked --no-default-features \
  --features desktop-product,omenchat-room-media-policy-qualification \
  negotiated_room_upload_rejection --lib -- --nocapture
# 1 passed

cargo test --locked --no-default-features \
  --features desktop-product,omenchat-room-media-policy-qualification \
  cli_keeps_room_media_policy_upload_rejection_isolated \
  --bin omenbrowser_rs -- --nocapture
# 1 passed

bash scripts/run-omenchat-room-media-policy-qualification.sh \
  --report /tmp/omen-room-media-policy-report.json
# pass

cargo test --locked --no-default-features \
  --features desktop-product,omenchat-room-media-policy-qualification --lib
# 1,528 passed; 31 ignored

cargo test --locked --no-default-features \
  --features desktop-product,omenchat-room-media-policy-qualification \
  --bin omenbrowser_rs
# 43 passed

cargo clippy --locked --no-default-features \
  --features desktop-product,omenchat-room-media-policy-qualification \
  --all-targets -- -D warnings
# pass

cargo test --locked --manifest-path src/server/Cargo.toml \
  --no-default-features \
  --features server-headless,omenchat-room-media-policy-qualification --lib
# 432 passed; 11 ignored

cargo clippy --locked --manifest-path src/server/Cargo.toml \
  --no-default-features \
  --features server-headless,omenchat-room-media-policy-qualification \
  --all-targets -- -D warnings
# pass

bash scripts/verify-product-features.sh
# pass
```

Not run in this slice: hosted CI, Python interoperability, native
Windows/macOS packaging, physical gateways, GUI automation, receiver-side
Resource cancellation, and long-duration resource measurements.

## Resource impact

No worker, timer, queue, cache, history, subscription, retry loop, runtime
dependency, or recurring network traffic was added. The server reuses existing
bounded pending-upload and Resource transport owners. Rejection occurs before
pending Resource admission. Publication rechecks policy after taking the exact
identity-bound offer and therefore cannot retain a permit after rejection.

## Compatibility and rollback

Capability request/acceptance remains outside product aliases. Rollback removes
the explicit Link-authority arguments, typed event projection, expanded smoke
mode, and this documentation. It does not require database, configuration,
identity, history, or upload migration.

## Remaining activation gates

- adjacent current/previous mixed-version qualification;
- receiver-side cancellation during an admitted upload Resource;
- native GUI attachment acceptance/rejection smoke;
- bounded CPU, RSS, latency, pending-offer, shutdown, and storage measurements;
- explicit production activation review.
