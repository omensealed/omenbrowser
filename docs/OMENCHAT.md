# OMENchat

OMENchat is OMENbrowser's room-oriented chat service over Reticulum Links.
`omenchatd` is a separately buildable and deployable server with its own
identity, configuration, Reticulum storage, database, logs, and uploads.

The current products use OMENchat wire protocol version 1 with explicit
per-Link capability negotiation. See [OMENchat Protocol](OMENCHAT_PROTOCOL.md)
for exact frame shapes and the authoritative capability matrix.

## Start an isolated server

Build:

```bash
cargo build --release --locked --manifest-path src/server/Cargo.toml \
  --no-default-features --features server-full
```

Initialize and inspect a test home:

```bash
./src/server/target/release/omenchatd init --home /tmp/omenchatd-test
./src/server/target/release/omenchatd status --home /tmp/omenchatd-test
./src/server/target/release/omenchatd doctor --home /tmp/omenchatd-test
```

Run headless or with the TUI:

```bash
./src/server/target/release/omenchatd run --home /tmp/omenchatd-test
./src/server/target/release/omenchatd tui --home /tmp/omenchatd-test
```

In the TUI, `g` starts the live server, `c` opens Monitoring, `l` opens
Logs, and `q` quits. Use **Announce Now** after startup when testing
discovery.

Never point a test at a normal server home. Browser and server identities must
remain separate.

## Configure Reticulum interfaces

Add, list, or remove TCP clients without replacing other configured clients:

```bash
omenchatd interfaces tcp-client gateway.example:42420 \
  --home /tmp/omenchatd-test
omenchatd interfaces list --home /tmp/omenchatd-test
omenchatd interfaces delete tcp-client gateway.example:42420 \
  --home /tmp/omenchatd-test
```

Changes take effect after the live server restarts. Multiple enabled clients
start independently.

For an IFAC-protected gateway, avoid putting the passphrase in argv:

```bash
printf '%s\n' 'your passphrase' > /tmp/omenchatd-ifac-passphrase
chmod 600 /tmp/omenchatd-ifac-passphrase
omenchatd interfaces tcp-client gateway.example:42420 \
  --home /tmp/omenchatd-test \
  --network-name private-reticulum \
  --passphrase-file /tmp/omenchatd-ifac-passphrase
```

`--passphrase-prompt` and `--passphrase-stdin` are also supported. Stock
Reticulum 0.9.9 TCP does not enforce the Python-compatible IFAC transform;
OMEN retains its narrow project-local TCP client adapter. Run the enforcing
gateway as the server.

## Connect from OMENbrowser

Open:

```text
omenchat://<destination_hash>
```

Announced servers also appear in Directory. “Announce verified” means Reticulum
authenticated the announce and its destination/identity relationship; it is
not operator trust. Saved and Trusted remain user-managed states.

Each server opens an independent session and pane. A restored pane starts
disconnected and reconciles against current network evidence. It does not treat
persisted connection text as live truth.

## Rooms and negotiated features

Current canonical clients and servers negotiate:

- durable mutations and durable notice acknowledgements;
- replies and mentions;
- reactions;
- message corrections and tombstones;
- room pins;
- announcement rooms;
- slow mode;
- per-room media policy;
- authorized moderation audit;
- persistent nickname colours.

Capabilities never activate from an application version or descriptor alone.
Legacy peers keep their exact protocol-v1 shapes. Nickname colour is
presentation metadata, not identity, trust, role, or moderation evidence.

## Delivery and uncertain mutations

Queue admission, server acceptance, transport acceptance, and peer delivery are
different states. The client does not automatically resend a mutation after an
uncertain result.

Recovered uncertain mutations appear in a bounded review panel. Retry is an
explicit user action and is available only when the original identity, server,
room, mutation data, capability set, and expiry still agree. Exact duplicate
durable mutations return their original acknowledgement without a second
server write or broadcast.

An expired LXMF receipt-observation window means peer delivery is unconfirmed;
it is not authoritative failure evidence.

## Uploads and attachments

The default server per-file upload limit is 512 KiB and the default rotating
per-user quota is 50 MiB. Server and room policy may impose a smaller limit.
Client, server, transport, parser, queue, and storage bounds all remain
authoritative.

Direct/local Reticulum Resource attachments are supported by the maintained
smoke matrix. Routed multi-hop retransmission is not fully qualified on the
official Reticulum 0.9.9 train. A terminal route failure does not trigger an
automatic retry, alternate primitive, or application-level fragmentation.
Retry manually after a route or condition change.

The server commits an upload only after the Resource and durable storage steps
complete. Existing oversized stored content is not silently deleted or
rewritten.

Media previews enforce encoded size, decoded size, dimension, frame, cache,
worker, and queue bounds. Reduced-motion and static-media builds avoid animated
rendering without changing network or storage behavior. Clearweb media follows
the configured privacy/proxy policy; ordinary external links use the external
browser prompt.

## Storage and migrations

Default server data lives below `~/.omenchatd`; a selected `--home` replaces
that root. Product-owned Unix directories are `0700` and sensitive files are
`0600`.

Important files include:

```text
config.toml
identity
omenchat.sqlite
omenchatd.log
reticulum/config
reticulum/storage/
uploads/
```

The current server schema is 14. Migration creates a private pre-migration
backup before changing the database. Rolling back to a binary that expects
schema 13 requires stopping the server and restoring the generated schema-13
backup; it is not a binary-only rollback. Never regenerate a valid identity to
repair configuration or permissions.

Use the offline, confirmation-gated recovery commands described by:

```bash
omenchatd database --help
omenchatd uploads --help
```

`doctor` is non-mutating. Upload-ledger repair never deletes payload files
implicitly.

## Administration

Use the server CLI or TUI for room and moderation operations:

```bash
omenchatd rooms --help
omenchatd users --help
omenchatd moderation --help
```

Authorization is enforced by the server; client presentation is not authority.
Moderation audit access requires moderator or administrator authority. Slow
mode, announcement policy, media ceilings, role changes, bans, mutes, and room
updates remain durable and bounded.

## Recovery and shutdown

Ordinary TCP reconnect belongs to the interface worker. The server performs one
bounded runtime recovery only after terminal or conservatively proven stalled
interface conditions. Shutdown cancels owned workers, drains bounded queues,
joins tasks with deadlines, flushes logs, and remains idempotent.

TUI recovery is deadline-driven and remains responsive to input, Stop, quit,
and shutdown. Runtime logs are rendered inside the TUI rather than written over
terminal widgets.

## Backpressure and diagnostics

Frames, Resources, uploads, database work, logs, histories, replay records,
catalogs, and event channels all have item and byte bounds. Saturation rejects
new work explicitly; it does not create unbounded waiting tasks.

Monitoring reports configured versus effective limits, interface/runtime
health, queue occupancy, Resource lifecycle, database-worker state, log
backpressure, and reconnect evidence. Diagnostic reports are bounded and redact
message bodies, drafts, room/user catalogs, filenames, local paths,
credentials, tickets, and private identity material.

## Testing

Run the isolated two-client and upload gates:

```bash
bash scripts/release-root-sanity.sh \
  --browser-root /tmp/omenbrowser-rs-test \
  --browser-root-2 /tmp/omenbrowser-rs-test-2 \
  --server-home /tmp/omenchatd-test
bash scripts/smoke/03_omenchat_two_client.sh
bash scripts/smoke/04_omenchat_resource_transfer.sh
bash scripts/run-omenchat-continuous-reconnect.sh
bash scripts/run-omenchat-current-upload.sh
```

See [Testing](TESTING.md) for the full Cargo, interoperability, package, and
platform matrix.
