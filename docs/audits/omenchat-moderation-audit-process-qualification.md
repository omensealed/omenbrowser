# OMENchat moderation-audit process qualification

Date: 2026-07-28

Baseline: `release/v0.9.6-4` through `814f78d`, plus this non-empty process
extension.

## Scope

This unit adds an explicit non-product
`omenchat-moderation-audit-qualification` feature to both Rust roots. Only that
feature makes the desktop request and omenchatd accept
`moderation-audit-v1`. Canonical `desktop-product`, `server-headless`, and
`server-full` builds continue to omit it, and
`scripts/verify-product-features.sh` rejects accidental activation.

The client now has one bounded manual request function. It requires negotiated
capability state, a joined room, a valid exclusive cursor, and a protocol
limit of 1 through 256. It creates no worker, timer, retry loop, persistent
cache, or automatic refresh.

## Process evidence

`scripts/run-omenchat-moderation-audit-qualification.sh` builds both current
binaries with the qualification feature and runs them against isolated
temporary browser/server roots over a local Reticulum TCP interface. The
harness:

1. creates one isolated moderator identity;
2. registers it with omenchatd;
3. stops the server and promotes that exact user to moderator;
4. restarts omenchatd;
5. creates a second isolated identity with a unique bounded display name and
   holds its identified, joined Link open;
6. negotiates durable mutations and `moderation-audit-v1` on the moderator
   Link;
7. persists a random mutation identity and canonical hash before sending one
   durable `mute` command for the active target;
8. requires the exact typed user result to show the target as muted;
9. requests a bounded audit page and requires the matching `Mute` record,
   target display name, muted result bit, and `ModerationAuditEnd`;
10. closes the target, orderly-restarts omenchatd, and confirms stable server
    destination, preserved moderator identity/role, replacement Link
    negotiation, and the same persisted non-empty page.

The earlier empty-read gate remains valid: an empty result emits only
`ModerationAuditEnd`. This extension also proves the non-empty inline shape.
The initial live page and post-restart page each carried one record followed
by the explicit end marker. No database row was seeded by the harness.

Observed report:

```json
{
  "authorized_nonempty_read": true,
  "explicit_end_observed": true,
  "isolated_loopback": true,
  "qualification_feature_only": true,
  "server_destination_stable": true,
  "server_restart": true,
  "status": "pass"
}
```

## Commands and results

Passed:

```text
cargo test --locked --no-default-features \
  --features desktop-product,omenchat-moderation-audit-qualification \
  moderation_audit --lib

cargo test --locked --no-default-features \
  --features desktop-product,omenchat-moderation-audit-qualification \
  --bin omenbrowser_rs cli_parses_

(cd src/server && cargo test --locked --no-default-features \
  --features server-headless,omenchat-moderation-audit-qualification \
  moderation_audit --lib)

bash scripts/verify-product-features.sh

bash scripts/run-omenchat-moderation-audit-qualification.sh \
  --report /tmp/omen-moderation-audit-qualification-report.json
```

Focused results:

- desktop moderation-audit: 5 passed, 1 explicit measurement ignored;
- desktop CLI parsing: 18 passed;
- omenchatd moderation-audit: 16 passed, 1 explicit measurement ignored;
- current/current process qualification: passed before and after restart.

## Compatibility and resource impact

There is no wire number, schema, retention bound, product default, UI, or
stored client-state change. Old/current ordinary peers request no capability
and see unchanged traffic. Qualification state remains attached to the
authenticated Link and is discarded on Link retirement.

The request path admits one protocol-bounded response at a time through the
existing bounded transport and 1,024-record/512-KiB client projection. The
target client is a separately owned process with an explicit termination path;
the harness removes both isolated browser roots and the server root on success
or failure.

## Remaining gates

This process run proves current/current empty and non-empty inline reads, a
real durable moderation transaction, and restart persistence. It does not
claim:

- an audit Resource over independent processes;
- adjacent-binary live traffic;
- receiver-side cancellation of an active inbound Resource;
- production activation or GUI/TUI presentation.

The locked `reticulum-rs-transport 0.9.6` still exposes no public
receiver-side Resource cancellation method. Production negotiation remains
disabled until the remaining activation decision is reviewed; no private fork
or false cancellation state was introduced.

## Rollback

Remove the qualification features, bounded request function, smoke-only local
display/target arguments, two-client shell hooks, and this audit. No database,
protocol, configuration, or product user-state rollback is required.
