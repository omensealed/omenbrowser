# OMENchat moderation-audit process qualification

Date: 2026-07-28

Baseline: `release/v0.9.6-4` at `906203e`, plus this qualification unit.

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

1. creates one isolated client identity;
2. registers it with omenchatd;
3. stops the server and promotes that exact user to moderator;
4. restarts omenchatd;
5. negotiates `moderation-audit-v1` on an identified Link;
6. requests a bounded page from the joined room;
7. observes the authoritative empty-read `ModerationAuditEnd`;
8. orderly-restarts omenchatd;
9. confirms stable server destination, preserved identity/role, replacement
   Link negotiation, and the same authorized empty-read result.

The first implementation expected an empty inline page followed by an end
marker. The real server correctly emitted only `ModerationAuditEnd` for an
empty result. The harness now follows the documented protocol instead of
fabricating an empty projection or waiting for a frame that is not required.

Observed report:

```json
{
  "authorized_empty_read": true,
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
existing bounded transport and 1,024-record/512-KiB client projection. Empty
reads retain no audit records. Temporary process roots are removed on success
or failure.

## Remaining gates

This process run proves a current/current authorized empty read and restart. It
does not claim:

- a non-empty inline audit page over independent processes;
- an audit Resource over independent processes;
- adjacent-binary live traffic;
- receiver-side cancellation of an active inbound Resource;
- production activation or GUI/TUI presentation.

The locked `reticulum-rs-transport 0.9.6` still exposes no public
receiver-side Resource cancellation method. Production negotiation remains
disabled until the remaining activation decision is reviewed; no private fork
or false cancellation state was introduced.

## Rollback

Remove the qualification features, bounded request function, CLI/shell smoke
hooks, and this audit. No database, protocol, configuration, or user-state
rollback is required.
