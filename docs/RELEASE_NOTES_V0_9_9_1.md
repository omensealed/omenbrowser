# OMENbrowser_rs and omenchatd v0.9.9-1 release notes

Status: final

Reticulum/LXMF crate train: exact official crates.io `0.9.9`.

## Dependency-train maintenance

- OMENbrowser_rs, standalone omenchatd, and the private IFAC adapter now use
  one coherent official Reticulum/LXMF 0.9.9 registry train. No Git source,
  fork, vendored transport, private registry, or `[patch.crates-io]` override
  is present.
- Existing public Link request/response packet helpers, Request ID
  correlation, subscribe-before-dispatch ordering, independent response
  primitive selection, bounded cancellation, and exactly-one-dispatch behavior
  are preserved.
- Managed integrated mode remains the primary backend. The optional external
  LXMF SDK/RPC sender still cannot preserve all OMEN-required TTL,
  idempotency, correlation, extension, and explicit reply-ticket guarantees;
  affected sends continue to fail closed before dispatch.
- The immutable pinned Python lane remains release evidence. The informational
  current-drift lane targets RNS 1.4.2, LXMF 1.1.1, and NomadNet 1.2.8.

## Resource and transport evidence

- The corrected split-metadata Resource path continues to pass exact-byte,
  incompressible multi-segment coverage on official 0.9.9.
- Routed Resource retransmission after downstream fragment loss remains an
  upstream limitation. The 0.9.9 duplicate filter admits repeated Resource
  requests but still suppresses the corresponding repeated Resource data/proof
  packets at a forwarding transport. OMEN does not automatically replay the
  application transfer or switch primitives.
- The separate maximum-UDP sentinel remains visible: the layout-derived
  456-byte upstream buffer cannot hold the 483-byte maximum serialized packet.
  No private transport patch or application fragmentation was added.
- Direct/local OMENchat attachments retain the product's existing 512 KiB
  default and all configured, peer, room, queue, parser, timeout, and retention
  bounds. This release does not claim routed attachment parity.
- The project-local Python-compatible IFAC TCP client adapter remains in use;
  stock upstream IFAC configuration alone is not treated as wire enforcement.

## Compatibility, persistence, and rollback

- OMENchat wire protocol remains version 1, `omenchat-protocol` remains 0.2.0,
  omenchatd SQLite schema remains 14, and `omen-ifac-tcp` remains 0.9.5-1.
- There is no database, configuration, cache, identity, destination, message,
  ticket, upload-content, or Reticulum-storage migration.
- There is no automatic send, request, Resource, or durable-mutation retry;
  no post-dispatch backend/primitive fallback; and no second dispatch.
- Product limits and negotiated smaller limits are unchanged.
- Copied-state qualification passed across v0.9.8-5 and v0.9.9-1 in both
  directions for OMENchat history and live restart, and the adjacent LXMF lane
  preserved identities/destinations and reused state roots. Rollback to
  v0.9.8-5 is therefore binary-only after normal shutdown and preservation of
  the existing application/server roots.
