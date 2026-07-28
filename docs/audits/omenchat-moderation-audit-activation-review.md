# OMENchat moderation-audit activation review

Date: 2026-07-28

Baseline: `release/v0.9.6-4` through `1ce6066`

Decision: bounded manual first-page and exclusive-cursor desktop pagination are
implemented behind the real non-product feature. Production activation remains
deferred until shared TUI consumption and the activation live gates pass.

## Evidence reviewed

- Independent desktop/server codecs and immutable ordinary fixtures.
- Schema-10 migration, rollback copies, constrained storage, transactional
  audit/replay coupling, and bounded pruning.
- Current role and room-membership authorization on every page request.
- Bounded client projection, malformed input, delayed Resource, and capability
  loss behavior.
- Current/current empty, inline, Resource, and orderly-restart process gates.
- Immutable `v0.9.6-3` ordinary traffic in both directions with explicit
  negative moderation-audit negotiation.
- The locked `reticulum-rs-transport 0.9.6` sender and receiver Resource API.

## Activation boundary

The implementation previously used a qualification-named feature as both the
real capability switch and the process-test hook. This review separates those
responsibilities:

- `omenchat-moderation-audit` owns capability request/acceptance and paging;
- `omenchat-moderation-audit-qualification` implies the real capability for
  isolated process tests;
- `omenchat-moderation-audit-resource-qualification` additionally forces only
  audit pages through Resource.

All three remain forbidden in canonical desktop/server products. This is
machine-checked. The split changes no wire number, database, product default,
configuration, queue, worker, timer, or persisted client state.

## Receiver cancellation decision

The locked upstream transport can cancel an outbound Resource but exposes no
public operation to cancel one active inbound Resource without closing its
Link. OMEN will not fork upstream, fabricate a cancelled state, or close a
healthy chat Link merely to make a read-only page appear cancellable.

This limitation does not permanently prohibit the capability because audit
reads are:

- explicit user actions, never polling or automatic refresh;
- read-only and safe to ignore after authority/capability loss;
- bounded to 1–256 records per protocol request;
- decoded into a 1,024-record/512-KiB client projection;
- protected by existing Resource offer size, purpose, and pending-item bounds;
- not automatically retried after silence, disconnect, or restart.

Initial presentation must therefore omit a false per-transfer Cancel action and
truthfully describe a Resource page as receiving until completion, failure, or
Link retirement. Upstream receiver cancellation can be adopted later behind
the same project boundary if a public compatible API appears.

## User-facing slice status

Before adding `omenchat-moderation-audit` to product aliases:

1. Complete: explicit moderator/admin-only “Refresh audit” for the selected
   joined room.
2. Complete: one single-flight 64-record first-page request through the
   existing bounded live-client function; no worker, timer, polling, or retry.
3. Complete: newest-first actor/action/target/result/role-or-status/time
   presentation from the project-owned projection.
4. Complete for the first page: ready, empty/end, receiving,
   unavailable/not-negotiated, unauthorized, and bounded failed states.
5. Complete: explicit “Load older” pagination uses the oldest retained ID as
   the exclusive cursor, validates the request sequence and page limit, rejects
   stale page/end responses, and stops at the server end marker or bounded
   client retention ceiling.
6. Complete for local role and Link authority loss; capability and room changes
   are hidden by current negotiated/active-room evidence and need focused
   activation-gate coverage before product enablement.
7. In progress: the reducer/projection is UI-framework-neutral and consumed by
   the desktop GUI; a root TUI consumer is still pending.

The desktop request owner is bounded to at most one room/state entry per
existing chat session. Previous results may remain visible, explicitly marked
as previous, while a manual refresh is receiving. An orderly end marks them
current and terminal. A full page becomes current with “more may exist” rather
than remaining falsely in-flight. Failure retains bounded prior evidence
without fabricating completion.

## Activation gate

After the user-facing slice:

- focused GUI/TUI reduction and rendering tests;
- current moderator and immediate role-loss tests;
- current/current inline and Resource process smoke through the product
  feature rather than a qualification-only capability switch;
- immutable adjacent ordinary traffic with negative capability evidence;
- formatting and strict Clippy for both product profiles;
- canonical feature assertions updated atomically to require the real feature
  and continue forbidding both qualification hooks;
- normal release quick, native CI, interoperability, and packaging gates on the
  release candidate only.

## Commands and results

Passed locally:

```text
cargo fmt --all --check
bash scripts/verify-product-features.sh

cargo test --locked --no-default-features \
  --features desktop-product,omenchat-moderation-audit \
  moderation_audit --lib

cargo test --locked --no-default-features --features desktop-product \
  live_open_requests_supported_durable_extensions_with_persistent_client_identity \
  --lib

cargo clippy --locked --no-default-features \
  --features desktop-product,omenchat-moderation-audit \
  --all-targets -- -D warnings

(cd src/server && cargo test --locked --no-default-features \
  --features server-headless,omenchat-moderation-audit \
  moderation_audit --lib)

(cd src/server && cargo test --locked --no-default-features \
  --features server-headless \
  moderation_audit_capability_follows_product_feature --lib)

(cd src/server && cargo clippy --locked --no-default-features \
  --features server-headless,omenchat-moderation-audit \
  --all-targets -- -D warnings)

bash scripts/run-omenchat-moderation-audit-qualification.sh \
  --report /tmp/omen-moderation-audit-feature-boundary.json
```

Focused results:

- real-feature desktop moderation audit: 5 passed, 1 explicit measurement
  ignored;
- canonical desktop capability absence: 1 passed;
- real-feature omenchatd moderation audit: 17 passed, 1 explicit measurement
  ignored;
- canonical omenchatd capability absence: 1 passed;
- strict Clippy: passed for both real-feature profiles;
- qualification alias Resource/restart process gate: passed;
- root/server Cargo feature trees: qualification implies the real feature and
  Resource qualification implies both, as designed.

No hosted CI, packaging, Python interoperability, public-network peer, native
Windows/macOS runtime, or hardware-interface test was run for this
non-product feature-boundary unit.

## Rollback

Before activation, remove the real feature and make the qualification hook own
the existing switch again. After activation, remove the real feature from both
product aliases together while retaining schema-10 data for operator
reconciliation. Qualification hooks and their isolated roots remain
non-product.
