# OMENbrowser_rs and omenchatd v0.10.0-4 release notes

Status: final

Reticulum/LXMF crate train: exact official crates.io `0.10.0`.

## Durable client-instance recovery

OMENbrowser now rotates its persistent durable-mutation client instance after
the server explicitly reports that the instance's bounded replay state has
expired. The rejected operation is first persisted as terminal and is never
retried. Rotation is permitted only when no prepared or uncertain mutation can
be orphaned, and the client reconnects to negotiate the replacement instance
before admitting later user operations.

Previously, a client instance retired by normal server replay retention stayed
in the client identity store indefinitely. Every later message used that
retired instance and received `DurableMutationResultExpired`, leaving a healthy
Link and ping stream but no room commit or fan-out.

## Identification hardening

omenchatd now preserves link-scoped negotiated capability state when Reticulum
delivers a duplicate `PeerIdentified` callback for the same identity. A genuine
identity change still clears durable-mutation and optional capability authority
before the replacement identity can use the Link.

## SQLite WAL durability

omenchatd now enables SQLite persistent-WAL file control on every owning store
connection. A short-lived independent reader can no longer unlink the WAL and
shared-memory sidecars while the live server retains its owner, which had
allowed fan-out from an invisible WAL branch without an authoritative commit
visible to reopened readers.

This changes neither schema 14 nor the atomic application-commit boundary. A
filesystem-level regressions prove that closing an unmanaged reader leaves the
managed WAL linked, subsequent writes remain visible to a newly opened observer,
and clean owner shutdown restores normal WAL sidecar cleanup for offline
maintenance.

## Live reaction projection

OMENbrowser now marks a sender's confirmed local echo as an authoritative live
reaction target when `MessageAck` promotes it to the server event ID. Previously
the receiving peers displayed later reaction deltas, while the original sender
persisted the same delta but hid it because only `RoomEvent` arrivals established
that live authority. Sender and receiver paths now use the same bounded reaction
projection semantics.

No rejected message was committed, and this revision does not replay or retry
those operations. Protocol 1, `omenchat-protocol` 0.2.0, schema 14,
storage, identities, bounds, and the exact dependency train remain unchanged.

The Bash 3.2-compatible macOS packaging correction from v0.10.0-3 is retained,
with bundle versions `0.10.0` / `1000.0.4`.

See `migration/V0_10_0_4_RELEASE_EVIDENCE.md` for qualification evidence.
