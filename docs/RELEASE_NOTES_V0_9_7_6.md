# OMENbrowser_rs and omenchatd v0.9.7-6 release notes

Reticulum/LXMF crate train: exact official registry `0.9.7`

Status: final

## Split-Resource safety

- Official `reticulum-rs-transport 0.9.7` is affected by upstream issue #553:
  metadata-bearing Resources split above the 1,048,575-byte efficient boundary
  can lose application bytes from segment two and later. Upstream PR #556 is
  tracked but is not vendored or patched into OMEN.
- Both products use checked `3 + metadata length + payload length` accounting.
  Unsafe outbound OMENchat Resources fail before the peer-visible offer frame
  or Resource dispatch. There is no fragmentation, fallback, retry, replay, or
  second dispatch.
- The default 512 KiB upload maximum is unchanged. Larger configured and room
  values remain stored, while negotiation and runtime admission use the safe
  effective limit for the exact affected train. Existing oversized stored
  uploads are retained; Resource fetch reports an explicit compatibility error.
- Native NomadNet and OMENchat reject split inbound Resource evidence, cancel
  owned transfer state where the public API permits it, and close only the
  affected Link. A later completion cannot publish rejected bytes.
- The issue #553 split-metadata sentinel is separate from the existing ignored
  maximum-UDP Resource sentinel. Neither upstream limitation is described as
  fixed.

## Compatibility boundaries

- No OMENchat wire, operation, capability, database schema, configuration
  schema, cache, identity, destination, message, ticket, upload-content, or
  Reticulum-storage migration is introduced.
- Reticulum/LXMF remains the exact official registry `0.9.7` train. There is no
  Git dependency, private fork, vendoring, or patch override.
- Normal single-segment Resources, the 512 KiB upload default, cancellation,
  queue bounds, and persistent upload retention remain unchanged.
- The temporary guard may be removed only after both Cargo roots adopt an
  official fixed registry train and the ignored split-metadata interoperability
  sentinel passes unchanged against that train.

## Rollback

This revision changes runtime admission and fail-closed handling only. A
v0.9.7-5 binary can reopen the same configuration, identity, database, cache,
messages, tickets, uploads, and Reticulum storage without conversion.
