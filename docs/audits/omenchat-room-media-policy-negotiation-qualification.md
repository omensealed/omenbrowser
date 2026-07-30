# OMENchat Room Media-Policy Negotiation Qualification

Date: 2026-07-28

## Scope

This slice qualifies the first live request/acceptance boundary for the
reviewed `room-media-policy-v1` contract. It deliberately remains outside all
canonical product profiles. It does not activate the capability for release
users and does not change the OMENchat protocol version, schema version,
configuration format, identity ownership, storage roots, or dependencies.

## Design and invariants

Both independent Cargo roots define
`omenchat-room-media-policy-qualification`. The feature depends on the existing
announcement-room and slow-mode product features because the seven-field room
shape is cumulative. A client requests media policy only while also requesting:

- `durable-mutations-v1`;
- `announcement-rooms-v1`;
- `room-slow-mode-v1`.

omenchatd accepts media policy only when its qualification capability is
enabled and all four requests are present. The client retains negotiated
authority only when all four acceptances are present. Missing prerequisites,
unsolicited acceptance, malformed or wrong-shaped room values, capability
loss, identity replacement, Link close, session retirement, and reconnect
clear or refuse the media-policy projection.

The server selects the seven-field shape per authenticated Link. It binds that
selection to the identified peer identity and removes it whenever Link or
identity ownership changes. Legacy, announcement-only, and slow-mode-only peers
continue using their exact prior shapes.

No worker, timer, recurring subscription, queue, cache, retry loop, or runtime
dependency was added. Room projections remain within the existing client bound
of 256 rooms per session and 512 KiB per room catalog.

## Process qualification

`scripts/run-omenchat-room-media-policy-qualification.sh` reuses the existing
release smoke instead of creating another network implementation. It:

1. builds the desktop and standalone server roots independently with the
   non-product feature;
2. creates isolated roots and a dynamically allocated loopback TCP endpoint;
3. starts and orderly-stops omenchatd once so the normal runtime owns database
   creation and migration;
4. applies a 262,144-byte lobby policy through the stopped-server command;
5. opens a real Reticulum Link;
6. requires all cumulative capability evidence and the exact seven-field
   projection;
7. completes an ordinary room-message round trip;
8. uses existing bounded startup/client deadlines and removes isolated state.

The observed report was:

```json
{
  "cumulative_capabilities": true,
  "isolated_loopback": true,
  "message_round_trip": true,
  "qualification_feature_only": true,
  "real_link": true,
  "room_media_policy_negotiated": true,
  "room_upload_max_file_bytes": 262144,
  "status": "pass"
}
```

## Commands and results

Passed:

```bash
cargo fmt --all -- --check

cargo test --locked --no-default-features \
  --features desktop-product,omenchat-room-media-policy-qualification \
  test_only_room_media_policy --lib -- --nocapture
# 1 passed

cargo test --locked --no-default-features --features desktop-product \
  live_open_requests_supported --lib -- --nocapture
# 1 passed; canonical request vector excludes room-media-policy-v1

cargo test --locked --manifest-path src/server/Cargo.toml \
  --no-default-features \
  --features server-headless,omenchat-room-media-policy-qualification \
  room_media_policy_qualification --lib -- --nocapture
# 2 passed

cargo test --locked --no-default-features \
  --features desktop-product,omenchat-room-media-policy-qualification --lib
# 1,527 passed; 31 ignored

cargo clippy --locked --no-default-features \
  --features desktop-product,omenchat-room-media-policy-qualification \
  --all-targets -- -D warnings
# pass

cargo test --locked --manifest-path src/server/Cargo.toml \
  --no-default-features \
  --features server-headless,omenchat-room-media-policy-qualification --lib
# 431 passed; 11 ignored

cargo clippy --locked --manifest-path src/server/Cargo.toml \
  --no-default-features \
  --features server-headless,omenchat-room-media-policy-qualification \
  --all-targets -- -D warnings
# pass

bash -n scripts/release-omenchat-smoke.sh \
  scripts/run-omenchat-room-media-policy-qualification.sh

bash scripts/run-omenchat-room-media-policy-qualification.sh \
  --report /tmp/omen-room-media-policy-report.json
# pass

bash scripts/verify-product-features.sh
# pass
```

The first process attempt correctly exposed that `omenchatd init` does not
open or migrate the database and stopped-server maintenance therefore refused
schema version zero. The harness was corrected to use a bounded normal startup
and orderly shutdown before maintenance; validation was not weakened.

The first broad qualification matrix also found that coupling capability
acceptance to the existing enforcement test hook changed upload rejection for
non-negotiating sessions. The normal direct-engine path therefore remained
dormant in this slice. The following Resource slice moved enforcement to
explicit authenticated-Link authority without weakening legacy behavior.

## Compatibility and rollback

Canonical root and server aliases do not include the qualification feature.
`scripts/verify-product-features.sh` rejects it if it leaks into any product
graph. Rollback removes the feature declarations, request/acceptance and
Link-shape state, process mode, and this documentation. Schema 13, stored room
policy, identities, messages, uploads, and existing wire fixtures need no
rollback because this slice did not alter them.

## Remaining activation gates

- native Linux GUI attachment preflight and authoritative rejection smoke is
  complete; hosted native presentation remains a release-candidate gate;
- live-process CPU/shutdown measurements; the isolated optimized
  retention/latency/storage/RSS measurement is complete;
- production activation is complete; see
  `omenchat-room-media-policy-activation-review.md`.

Until those pass, product capability vectors remain unchanged and the policy
must not be advertised as active.
