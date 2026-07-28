# OMENchat moderation-audit process qualification

Date: 2026-07-28

Baseline: `release/v0.9.6-4` through `cb13e85`, plus this Resource process
extension.

## Scope

The existing non-product `omenchat-moderation-audit-qualification` feature
makes the desktop request and omenchatd accept `moderation-audit-v1`. This
extension adds `omenchat-moderation-audit-resource-qualification`, which
implies the first feature and forces only moderation-audit pages through the
existing bounded Resource path. Canonical `desktop-product`,
`server-headless`, and `server-full` builds continue to omit both features,
and `scripts/verify-product-features.sh` rejects accidental activation.

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
   target display name, muted result bit, Resource transport provenance, and
   `ModerationAuditEnd`;
10. closes the target, orderly-restarts omenchatd, and confirms stable server
    destination, preserved moderator identity/role, replacement Link
    negotiation, and the same persisted non-empty page.

The earlier empty-read and non-empty inline gates remain valid. This extension
proves the non-empty Resource shape. The initial live page and post-restart
page each carried one record in a Reticulum Resource followed by the explicit
end marker. No database row was seeded by the harness.

The first attempted harness used a one-byte global batch threshold. That also
forced unrelated join/catalog snapshots through Resources and introduced
contention that was not specific to moderation audit. It was rejected. The
replacement feature changes only moderation-audit response selection and is
for qualification, not a product default.

This run found a real server bridge defect. `SessionEngine` emitted
`ModerationAuditResource` and retained its payload, but the production
transport allowlist omitted that operation. The offer frame crossed the Link
while the payload remained pending and server counters showed no Resource
offered. The bridge now classifies and releases moderation-audit Resources
through the same bounded ownership path as history, user-list, reaction, and
revision Resources. A focused regression guards the classification.

Observed report:

```json
{
  "authorized_nonempty_read": true,
  "explicit_end_observed": true,
  "isolated_loopback": true,
  "qualification_feature_only": true,
  "resource_delivery": true,
  "server_destination_stable": true,
  "server_restart": true,
  "status": "pass"
}
```

## Commands and results

Passed:

```text
cargo test --locked --no-default-features \
  --features desktop-product,omenchat-moderation-audit-resource-qualification \
  moderation_audit --lib

cargo test --locked --no-default-features \
  --features desktop-product,omenchat-moderation-audit-resource-qualification \
  --bin omenbrowser_rs cli_parses_

(cd src/server && cargo test --locked --no-default-features \
  --features server-headless,omenchat-moderation-audit-resource-qualification \
  moderation_audit --lib)

(cd src/server && cargo test --locked --no-default-features \
  --features server-headless,omenchat-moderation-audit-resource-qualification \
  moderation_audit_resource_uses_the_payload_bridge --lib)

bash scripts/verify-product-features.sh

bash scripts/run-omenchat-moderation-audit-qualification.sh \
  --report /tmp/omen-moderation-audit-qualification-report.json
```

Focused results:

- desktop moderation-audit: 5 passed, 1 explicit measurement ignored;
- desktop CLI parsing: 18 passed;
- omenchatd moderation-audit: 17 passed, 1 explicit measurement ignored;
- current/current Resource process qualification: passed before and after
  restart.

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

Together, the process gates prove current/current empty, non-empty inline, and
non-empty Resource reads, a real durable moderation transaction, and restart
persistence. They do not claim:

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
