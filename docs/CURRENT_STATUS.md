# Current product status

This page describes the release-qualified `v0.9.9-2` candidate. Source code
and automated tests outrank prose if a development branch has advanced.

## Version and compatibility

| Component | Current value |
|---|---|
| OMENbrowser_rs | `0.9.9-2` |
| standalone omenchatd | `0.9.9-2` |
| Reticulum/LXMF Rust train | exact official crates.io `0.9.9` |
| OMENchat wire protocol | version `1` |
| omenchat-protocol Rust API | `0.2.0` |
| omenchatd SQLite schema | `14` |

The root application and `src/server` remain independent Cargo roots with
independent lockfiles. No Git dependency, private Reticulum fork, vendored
transport, or `[patch.crates-io]` override is part of the product.

## Supported runtime

Managed integrated Reticulum mode is the supported product mode. OMENbrowser
owns its runtime, configured interfaces, identity attachment, bounded workers,
and shutdown.

External/shared mode remains a preserved but deferred configuration. Selecting
it does not start a full shared backend. The optional LXMF SDK/RPC endpoint is
not equivalent to the managed transport and operations requiring unsupported
TTL, idempotency, correlation, extension, or reply-ticket guarantees are
rejected before dispatch.

## Current product capabilities

- multi-tab NomadNet browsing with bounded direct and Resource requests;
- LXMF direct and propagated messaging, receipts, tickets/stamps, history, and
  bounded attachments;
- OMENchat rooms, durable mutations, replies/mentions, reactions, message
  revisions, pins, announcement rooms, slow mode, room media policy,
  moderation audit, and negotiated nickname colours;
- independent `omenchatd` service, TUI, administrative commands, uploads, and
  quiet NomadNet portal;
- GUI and TUI products with isolated identities/storage and mock/offline
  development support.

OMENchat capabilities are activated only after explicit per-Link negotiation.
Legacy peers keep their exact protocol-v1 shapes. The authoritative matrix is
in [OMENchat Protocol](OMENCHAT_PROTOCOL.md).

## Important limitations

- Routed multi-hop Resource retransmission is not fully qualified on upstream
  Reticulum 0.9.9. Direct/local attachment paths are distinct from routed
  qualification, and OMEN never automatically replays an uncertain transfer.
- The independent maximum-UDP Resource sentinel remains visible: upstream's
  fixed transmit buffer is smaller than the maximum serialized wire packet.
- Stock upstream TCP does not enforce Python-compatible IFAC wire transforms.
  OMEN retains its narrow project-local IFAC TCP client adapter.
- Dynamic NomadNet packet/Resource selection from negotiated Link MTU remains
  deferred; current selection uses the conservative public packet boundary.
- Full shared/external runtime ownership and live interface mutation are not
  product capabilities.

See [Reticulum Transport Gaps](RETICULUM_TRANSPORT_API_GAP.md) for evidence and
removal conditions.

## Safety guarantees

- Browser and server identities and storage remain separate.
- Product-owned Unix directories are `0700`; sensitive files are `0600`.
- Queues, caches, histories, uploads, parsers, retries, workers, and retained
  diagnostics are bounded.
- Queue admission, transport acceptance, and receipt observation are reported
  as different states.
- OMEN does not automatically replay a send, request, Resource, or durable
  mutation after an uncertain outcome.

## Release evidence

The current candidate notes are
[v0.9.9-2](RELEASE_NOTES_V0_9_9_2.md). Current commands are in
[Testing](TESTING.md). Hosted CI and package results are evidence only for the
commit SHA on which they ran.
