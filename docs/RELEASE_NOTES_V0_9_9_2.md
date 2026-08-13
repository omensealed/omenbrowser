# OMENbrowser_rs and omenchatd v0.9.9-2 release notes

Status: final

Reticulum/LXMF crate train: exact official crates.io `0.9.9`.

## Maintenance and evidence corrections

- Current transport documentation now correctly records that the
  current-Python four-quadrant matrix qualifies both oversized request
  Resources and response Resources with exact correlation and bytes.
- A capability ledger uses explicit `supported`, `unsupported`, and `unknown`
  evidence states. Its structural verifier rejects the stale Request Resource
  claim and any unsupported promotion of routed fragment-loss or maximum-UDP
  behavior.
- The Resource compatibility verifier now protects both separately named
  upstream limitation sentinels, their exact ignore reasons, evidence
  documents, and critical assertions. Fixture tests prove renamed, unignored,
  or weakened sentinels fail verification.
- Split metadata remains a normal passing exact-byte regression on official
  0.9.9. It is not merged with either known limitation.

## SDK/RPC and dispatch safety

- The real published SDK/RPC capture still proves that direct/propagated
  method, stamp cost, fresh-ticket request, propagation fallback choice, and
  daemon cancellation identity are represented.
- TTL, idempotency, correlation, extensions, and explicit remembered reply
  tickets remain unrepresentable. Focused tests now prove each guarantee is
  rejected before endpoint connection as well as before dispatch.
- Managed integrated mode remains primary. External/shared mode is preserved
  but deferred; endpoint availability is not treated as send equivalence.
- NomadNet Request/Response selection, subscribe-before-dispatch ordering,
  cancellation ownership, exact request correlation, and exactly one dispatch
  are unchanged.
- The storage-only omenchatd crash-recovery test closure no longer requires the
  optional transport crate; production transport behavior is unchanged.

## Compatibility and retained limits

- Routed multi-hop Resource retransmission after downstream fragment loss
  remains an upstream `reticulum-rs-transport 0.9.9` limitation. The sentinel
  remains ignored and deliberately fails for its exact documented reason.
- Maximum-size UDP Resource wire serialization remains an independent upstream
  0.9.9 limitation: the observed 456-byte buffer is smaller than the 483-byte
  maximum serialized packet. Its separate sentinel remains visible.
- OMEN carries no upstream patch, fork, vendor copy, Git override, private
  registry, application fragmentation, automatic retry, primitive fallback,
  backend switch, or second dispatch.
- Direct/local Resource qualification does not imply routed fragment-loss
  qualification. Existing upload, parser, queue, item, byte, deadline,
  cancellation, retention, and negotiated smaller limits are unchanged.
- OMENchat wire protocol remains 1, `omenchat-protocol` remains 0.2.0,
  omenchatd SQLite schema remains 14, and `omen-ifac-tcp` remains 0.9.5-1.
- No database, configuration, cache, identity, destination, message, ticket,
  upload-content, or Reticulum-storage migration is introduced. Adjacent
  rollback to v0.9.9-1 uses copied isolated state and requires no migration.

## Diagnostics and maintainability decision

No new runtime diagnostic fields, polling, or retained history were added.
The existing typed lifecycle/capability surfaces already expose authoritative
project-owned evidence; proposed upstream details without a public authoritative
API remain omitted rather than fabricated. No shared Reticulum testkit or large
native-module refactor was added because neither had two stable consumers or a
release correctness need.
