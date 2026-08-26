# v0.10.0-4 release evidence

Status: locally release-qualified; hosted qualification and publication pending.

## Live failure evidence

Two clients maintained active Links and ping/pong traffic to one omenchatd.
After a server restart, each replacement Link sent SessionOpen, joined lobby,
and recovered history.

At Unix times `1787703205` through `1787703208`, the v0.10 client sent five
RoomMessage envelopes on the active Link. omenchatd received all five but
returned 60-byte fail-closed errors. Later attempts after the corrected server
restart behaved identically. Server and both client databases remained
at room event 417, proving no commit or fan-out occurred. The second v0.9.9-2
client received only pings during the window. The sender was joined, unmuted,
unbanned, and exempt from slow mode; lobby was open with slow mode disabled.

The sender's mutation-intent database recorded every attempt as terminal
`Expired`. The server's `durable_mutation_clients` row for identity
`bfa0abbe6b0647df3fa55d7fd2648a4b` and client instance
`f5e49a86e1b135d75c7ea18e273904c5` had `retired_at=1787589707`. SessionOpen
correctly retained the retirement marker because replay results for that
instance had already been pruned. The client nevertheless continued using the
same instance for every new user operation. This was the live outage cause.

## Correction

After persisting `DurableMutationResultExpired` as terminal, OMENbrowser now
uses the existing atomic, quiescence-gated rotation boundary to replace the
client instance. It then retires the old Link and performs normal bounded
reconnection so SessionOpen negotiates the replacement before later sends.
The rejected mutation is not retried or replayed.

Separately, `OmenchatLinkEvent::PeerIdentified` now clears negotiated maps only
when the authenticated identity actually changes. Duplicate same-identity
callbacks preserve durable and optional capability state. This is valid
identification hardening, but the live database evidence did not support it as
the cause of this outage. Existing identity-change and Link-close cleanup
remains fail-closed.

No database schema, protocol, Reticulum identity, dependency, automatic retry,
or replay behavior changed. The rejected operations remain uncommitted and are
not replayed.

## WAL split-brain correction

During live diagnosis, omenchatd broadcast room event 418 to both connected
clients while an independent reader still observed authoritative event 417.
The server held `omenchat.sqlite-wal (deleted)` and `omenchat.sqlite-shm
(deleted)` descriptors. Graceful shutdown did not materialize the broadcast
event in the authoritative database. Both client projections were backed up and
the two non-authoritative event rows were removed while all processes were
stopped; identities and mutation intents were unchanged.

Each owning `OmenchatStore` connection now enables
`SQLITE_FCNTL_PERSIST_WAL` for its lifetime and disables it before clean close.
Raw validation and one-shot maintenance connections retain ordinary SQLite
cleanup semantics. The focused
`unmanaged_reader_cannot_unlink_managed_wal_or_hide_later_writes` regression
and `disabling_persistent_wal_before_close_restores_clean_shutdown_sidecars`
regression passed. After deploying the corrected server, an independent read
left the live WAL and shared-memory descriptors linked. New message event 419
and its durable mutation result were visible authoritatively, with two active
Links, empty queues, and no protocol or write errors.

## Live reaction correction

A thumbs-up from user 8 on OMENtest message event 420 committed as reaction
audit event 52 and was broadcast to both negotiated Links. OMENtest persisted
the exact reaction row but did not render it live. The sender's message had been
promoted from a transient local echo by `MessageAck`; unlike a peer receiving a
`RoomEvent`, that acknowledgement path did not mark the confirmed event as an
authoritative live reaction target, so the bounded view correctly refused to
show an authority-incomplete projection.

`MessageAck` now applies the same live reaction-target authority transition as
`RoomEvent` when reactions were negotiated. The focused
`live_send_message_local_echo_is_confirmed_and_reaction_authoritative_by_message_ack`
regression passed. Both isolated v0.10.0-4 clients were rebuilt and restarted;
a fresh OMENtest message followed by a main-client reaction updated live on the
sender as expected.

## Gates

The focused server duplicate-identification, client-instance rotation,
persistent-WAL, clean-shutdown, and sender-side reaction-authority regressions
passed. `scripts/release-check.sh quick` passed. `scripts/release-check.sh full`
passed, including the 1,691-test browser suite, 617 active server tests, both
Clippy lanes, native/TUI checks, and standalone server relocation.
`scripts/release-package.sh` produced the versioned archive referenced by
`dist/OMENbrowser_rs-latest.tar.gz`, and
`scripts/release-check.sh package` passed checksum, extraction, root sanity,
isolated server, collector, and two-client OMENchat smoke checks against that
archive. Hosted native/package workflows and GitHub release publication remain
pending; no push, tag, publication, or GitHub release was performed locally.
