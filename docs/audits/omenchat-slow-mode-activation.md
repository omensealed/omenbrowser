# OMENchat slow-mode product activation

Date: 2026-07-28

Baseline: `release/v0.9.6-4` at `670d91e`

Verdict: the previously qualified slow-mode capability is activated in all
four canonical product profiles. Activation changes neither the OMENchat
protocol vocabulary nor schema 12; it selects already-qualified negotiation,
six-field room projection, transactional admission, typed rejection, and
recovery behavior.

## Product boundary

The root and standalone server manifests now define a dependency-free
`omenchat-slow-mode` feature. It is required by:

- `desktop-product`;
- `desktop-product-static-media`;
- `server-headless`; and
- `server-full`.

`omenchat-slow-mode-qualification` depends on that production feature but is
not included by any product alias. It retains only deterministic test hooks:
isolated GUI auto-open and the controlled live room-policy transition used by
the process harness. `scripts/verify-product-features.sh` requires the
production feature and rejects qualification hooks in product graphs.

## Runtime behavior

A capable client requests `room-slow-mode-v1`. A capable server accepts it
only when the same Link has also negotiated `durable-mutations-v1`. Only that
negotiated shape carries the bounded `slow_mode_seconds` scalar.

New room messages and actions pass through the existing atomic admission
boundary. Durable replay is resolved before admission; a new event, replay
result, and cooldown deadline commit or roll back together. Typed error 1017
preserves the rejected draft. Moderators retain the documented bypass.

Legacy and non-negotiating peers retain byte-exact four-field room values.
Announcement-capable peers without slow mode retain byte-exact five-field
values. An explicitly feature-disabled server preserves the stored scalar but
does not advertise or enforce it.

CLI human/JSON output and the server TUI derive their `active`/`inactive`
label from the same production feature as `SessionEngine`; configured policy
is no longer misreported after activation.

## Resource and compatibility impact

Activation adds no dependency, schema migration, worker, task, timer, queue,
cache, retry loop, or recurring subscription. Existing fixed admission item,
byte, age, and incremental-pruning bounds remain unchanged. The release
measurement preceding activation observed one real Link, empty bounded
transport/event queues, normal shutdown, and no short-sample resource blocker;
see `omenchat-slow-mode-resource-qualification.md`.

The qualification hooks remain separate so ordinary products cannot
automatically mutate room policy or auto-open a test session. Existing
identities, room history, replay records, uploads, and Reticulum state are not
rewritten.

## Rollback

The first rollback step is to remove `omenchat-slow-mode` from the canonical
aliases and rebuild both products. This disables request/acceptance and
admission while retaining schema 12 and its bounded rows, so it requires no
destructive data operation. Exact legacy behavior is covered by the
no-feature regression.

For a full storage downgrade, stop omenchatd and use the existing
confirmation-gated `database export-schema11-copy` command. It creates a
separate copy and never deletes the active database automatically.

## Validation

Passed locally:

```text
cargo fmt --all --check
(cd src/server && cargo fmt --all --check)
bash scripts/verify-product-features.sh
cargo test --locked --no-default-features --features desktop-product \
  slow_mode_projection_is_bounded_and_follows_product_capability
cargo test --locked --no-default-features --features desktop-product \
  live_open_sends_session_open_and_join_then_applies_server_frames
(cd src/server && cargo test --locked --no-default-features \
  --features server-headless \
  slow_mode_product_feature_requires_durable_mutations_and_encodes_exact_shape)
(cd src/server && cargo test --locked --no-default-features \
  --features server-headless \
  room_status_projections_report_effective_slow_mode_without_secrets)
(cd src/server && cargo test --locked --no-default-features \
  dormant_slow_mode_setting_does_not_change_production_session_behavior)
cargo clippy --locked --no-default-features --features desktop-product \
  --all-targets -- -D warnings
(cd src/server && cargo clippy --locked --no-default-features \
  --features server-headless --all-targets -- -D warnings)
(cd src/server && cargo clippy --locked --no-default-features \
  --features server-full --all-targets -- -D warnings)
cargo test --locked --no-default-features --features desktop-product
(cd src/server && cargo test --locked --no-default-features \
  --features server-headless)
(cd src/server && cargo test --locked --no-default-features \
  --features server-full)
```

The first attempted no-feature server test exposed and then corrected an
untyped qualification-only `None`; the repeated command passed. No validation
failure was hidden. The full root product matrix passed (1,508 library tests,
31 intentionally ignored, plus all binary/integration/doc-test targets).
Server headless passed 412 tests with 11 intentionally ignored; server full
passed 535 with the same 11 explicit soak/hardware/upstream-bound cases.

Pending for the batched release-candidate gate:

- complete root product tests and strict Clippy;
- complete independent server headless/full tests and strict Clippy;
- quick release check;
- hosted native platform, Python interoperability, and packaging workflows;
- physical GPU and long-duration public-network evidence.

No hosted workflow is justified for this isolated local unit; it should be
batched with the next release-candidate checkpoint.
