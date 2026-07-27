# OMENchat announcement-room process qualification

Date: 2026-07-27

Baseline: `release/v0.9.6-4` through `b7f959f`, plus this standard-member
upload-rejection process unit

Verdict: current/current member rejection plus moderator message/Resource
publication and restart persistence pass over real isolated Reticulum Links; negotiated
`announcement-rooms-v1` remains dormant

## Scope and invariant

This unit extends the existing release OMENchat smoke rather than adding
another process harness. The opt-in
`--announcement-rejection-smoke` case:

1. creates isolated browser and omenchatd roots;
2. starts omenchatd once so its normal owner performs schema migration;
3. stops the server and applies the confirmation-gated lobby policy;
4. starts the server and joins with a standard member over a real Link;
5. sends one explicit room message;
6. requires typed room-policy error `1016`;
7. requires that no committed server event with that message exists; and
8. optionally restarts omenchatd, reopens the same browser identity root on a
   new Link, and repeats those assertions.

The mode is intentionally incompatible with upload, multi-client,
continuous-client, reaction, revision, and pin smoke options. It may be
combined only with `--restart-server`. This keeps one authorization risk class
per run and prevents an expected rejection from being mistaken for a normal
message-echo success.

The wait owner now exits promptly only on the typed announcement-policy error.
Other operation errors remain observable without incorrectly terminating an
unrelated message wait. The wait remains bounded by the existing response
deadline and does not add a worker, channel, timer, retry, cache, or retained
history.

The companion `--announcement-moderator-smoke` case registers exactly one
isolated client while the lobby is ordinary, stops omenchatd, uses the
headless-safe redacted user listing and confirmation-gated role command,
applies moderator role and announcement policy, then restarts. It deliberately
uses the unchanged normal smoke expectation: the message must be committed and
echoed, and an optional upload must complete and be fetched through a
Reticulum Resource.

## Process result

Passed locally with current debug binaries:

```bash
bash scripts/release-omenchat-smoke.sh \
  --browser-bin target/debug/omenbrowser_rs \
  --server-bin src/server/target/debug/omenchatd \
  --tcp 127.0.0.1:42527 \
  --path-wait 20 \
  --out /tmp/omenbrowser-announcement-process \
  --message 'announcement policy qualification' \
  --announcement-rejection-smoke \
  --restart-server
```

Both the initial and post-restart reports recorded:

- outcome `pass`;
- `announcement_rejected: true`;
- `committed_message_seen: false`;
- a joined lobby session; and
- no automatic uncertain-mutation replay.

The restart was orderly, the destination remained stable, schema-11 policy
remained `announcement`, and the second browser process reused the original
isolated identity root while establishing a new Link. The second message was a
new explicit smoke operation, not an automatic retry of the first operation.

The first attempted run correctly failed before networking because the harness
tried stopped-server maintenance against schema version 0. The final harness
preserves that fail-closed rule by performing a bounded initialization
start/stop before policy maintenance.

The moderator/resource case also passed locally:

```bash
bash scripts/release-omenchat-smoke.sh \
  --browser-bin target/debug/omenbrowser_rs \
  --server-bin src/server/target/debug/omenchatd \
  --tcp 127.0.0.1:<unused-port> \
  --path-wait 20 \
  --out /tmp/omenbrowser-announcement-moderator \
  --message 'announcement moderator qualification' \
  --announcement-moderator-smoke \
  --upload-file fixtures/omenchat/v0_6_0_1_wire.rs \
  --restart-server
```

The initial report passed with a committed message, `upload_completed`, and
`upload_resource_available`; the post-restart report passed with another
committed message. The stop was orderly, destination stable, and both
moderator role and announcement policy survived. The canonical
`server-headless` binary was used.

The first moderator attempt exposed that the older pin-smoke setup depended on
the optional TUI. The final headless administration adds only:

```text
omenchatd users list --json --home <root>
omenchatd users role <id> standard|trusted|moderator|administrator \
  --confirm --home <root>
```

Listing is read-only and omits identity hashes/LXMF destinations. Role changes
require an existing current database and its exclusive stopped-server
maintenance lock. No dependency or product feature changed.

The shared harness refactor also exposed an overly broad early-stop condition
from the preceding rejection-smoke unit: any decoded error could stop a later
message wait. A real pin smoke produced an unrelated snapshot-ordering error
before its message echo and demonstrated the regression. The wait now
short-circuits only for the exact typed announcement restriction. The canonical
headless pin smoke then passed, and the member announcement-rejection smoke
was rerun successfully.

The standard-member upload boundary passed over a real Link with the same
873-byte deterministic fixture:

```bash
bash scripts/release-omenchat-smoke.sh \
  --browser-bin target/debug/omenbrowser_rs \
  --server-bin src/server/target/debug/omenchatd \
  --tcp 127.0.0.1:<unused-port> \
  --path-wait 20 \
  --out /tmp/omenbrowser-announcement-upload-rejection \
  --message 'announcement upload rejection qualification' \
  --announcement-upload-rejection-smoke \
  --upload-file fixtures/omenchat/v0_6_0_1_wire.rs \
  --restart-server
```

Initial and post-restart reports both passed with:

- typed announcement-policy rejection;
- `upload_accepted: false`;
- `upload_completed: false`; and
- `committed_upload_seen: false`.

The isolated server reported `tracked=0 files/0 B` and
`disk=0 files/0 B`, all discrepancy counters zero, and no regular file in its
upload root. The check runs before and after restart. The normal
machine-readable doctor intentionally omits private detail, so this local
isolated harness reads the existing human detail rather than weakening
redaction.

## Deterministic and adjacent evidence

Focused tests passed:

```text
cargo test --locked --no-default-features --features desktop-product \
  announcement_rejection_evidence_requires_the_typed_policy_error \
  --bin omenbrowser_rs -- --nocapture
cargo test --locked --no-default-features --features desktop-product \
  dormant_announcement_policy_projects_only_when_requested_and_clears_on_loss \
  --lib -- --nocapture
cargo test --locked --no-default-features --features desktop-product \
  v0_9_6_3_ordinary_message_remains_byte_exact --lib -- --nocapture
cargo test --locked -p omenchat-protocol room_policy -- --nocapture
cargo test --locked --manifest-path src/server/Cargo.toml \
  --no-default-features --features server-headless \
  announcement_room --lib -- --nocapture
cargo test --locked --manifest-path src/server/Cargo.toml \
  --no-default-features --features server-headless \
  v0_9_6_3_ordinary_message_remains_byte_exact --lib -- --nocapture
```

Results were respectively 1, 1, 1, 3, 4, and 1 passing tests. The adjacent
`v0.9.6-3` ordinary frame remains byte-exact in both independent codecs.
Because that release cannot request `announcement-rooms-v1`, policy is never
projected to it and four-field room values remain the compatibility contract.
Server authorization still applies to all clients regardless of negotiation.

This is deterministic adjacent-format evidence, not adjacent-binary live
announcement traffic. An old browser has no expected-policy-rejection smoke
mode, and an old server has neither schema-11 policy nor capability support.
Running a negotiated policy case against either peer would fabricate a feature
they cannot advertise. Live adjacent ordinary traffic remains covered by the
existing mixed-release harness.

## Compatibility, storage, and rollback

Production capability request and acceptance vectors are unchanged. Protocol
version, database schema, identity ownership, state paths, ordinary room
behavior, and packaged feature profiles are unchanged. The only persistent
write in this smoke is inside its disposable server root.

Rollback removes the CLI/shell smoke option and this evidence. It does not
alter the already-qualified schema-11 policy or server authorization.

## Remaining activation gates

- negotiated current/current five-field room catalog and delta process traffic;
- same-process live client replacement-Link capability loss/recovery;
- native GUI member/moderator observation;
- a documented restart-only policy contract or a separately reviewed live
  policy reload/fanout design;
- joint review before production request/acceptance activation.
