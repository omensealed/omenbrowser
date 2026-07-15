# omenchatd

`omenchatd` is the standalone Rust OMENchat server. It is hosted under
`OMENbrowser_rs/src/server/` during development, but it must remain movable and
independent.

This directory must not import OMENbrowser_rs modules or depend on the browser
crate. The client and server communicate through the documented protocol in:

```text
../../docs/OMENCHAT_PROTOCOL.md
```

Standalone check:

```bash
cd src/server
cargo check
cargo check --no-default-features --features server-headless
cargo run --no-default-features --features server-headless -- init
cargo run --no-default-features --features server-full -- tui
```

`server-headless` is the daemon/admin CLI product and excludes Ratatui and
Crossterm. `server-full` adds the optional interactive TUI and is used for the
combined release package so existing `omenchatd tui` behavior is preserved.

Initial commands:

```bash
omenchatd init
omenchatd status
omenchatd doctor
omenchatd config show
omenchatd config set --name "My Chat" --operator-label "node-admin"
omenchatd config set --announce-interval 360 --max-message-bytes 2048
omenchatd rooms list
omenchatd rooms add field-ops --topic "Field operations"
omenchatd interfaces tcp-server 127.0.0.1:42420
omenchatd interfaces tcp-client gateway.example:42420
omenchatd tui
omenchatd run
```

Useful local TCP setup for testing:

```bash
omenchatd init --home /tmp/omenchatd-demo --tcp-server 127.0.0.1:42420
omenchatd run --home /tmp/omenchatd-demo --tcp-server 127.0.0.1:42420
```

Useful backbone gateway setup:

```bash
omenchatd init --home /tmp/omenchatd-demo --tcp-client gateway.example:42420
omenchatd run --home /tmp/omenchatd-demo
```

For IFAC-protected gateways, write the credentials into the server-owned
Reticulum config:

```bash
printf '%s\n' 'your passphrase' > /tmp/omenchatd-ifac-passphrase
chmod 600 /tmp/omenchatd-ifac-passphrase
omenchatd interfaces tcp-client gateway.example:42420 \
  --home /tmp/omenchatd-demo \
  --network-name private_ret \
  --passphrase-file /tmp/omenchatd-ifac-passphrase
```

Optional systemd user-service install from the packaged bundle:

```bash
bash scripts/install-omenchatd-user-service.sh \
  --bin "$PWD/bin/omenchatd" \
  --home /tmp/omenchatd-demo
```

Remove only the user service while preserving the server home:

```bash
bash scripts/install-omenchatd-user-service.sh --uninstall
```

Interactive admin console:

```bash
omenchatd tui --home ~/.omenchatd
```

Operator readiness check:

```bash
omenchatd doctor --home ~/.omenchatd
```

`doctor` checks the server-owned config, identity, database, Reticulum config
and storage, NomadNet portal page, active rooms, interface hints, and basic
limits without starting the live server.

With the server stopped, an operator may remove only ledger records that point
to missing files or paths outside the owning identity directory:

```bash
omenchatd uploads repair-ledger --confirm --home ~/.omenchatd
```

Confirmation is mandatory. Repair refuses to create or migrate a database,
never deletes files, and preserves orphan files for manual review. Run
`doctor` again afterward. Do not run repair concurrently with `omenchatd run`
or the admin TUI.

To restore one of omenchatd's generated pre-migration SQLite backups, stop the
server cleanly and run:

```bash
omenchatd database restore-migration-backup \
  --from ~/.omenchatd/omenchat.sqlite.pre-v2-from-v1.bak \
  --confirm --home ~/.omenchatd
```

Restore accepts only a regular, non-symlink sibling whose filename version
matches its older SQLite `user_version`. It refuses corrupt/current-schema
sources and refuses to proceed while active WAL/SHM sidecars exist. The backup
is copied into an owner-only staging database, migrated, checkpointed, and
checked with SQLite integrity and foreign-key checks before atomic publication.
The previous active database is retained as a unique owner-only
`omenchat.sqlite.pre-restore-*.bak`; neither the selected source nor upload
files are modified. Run `doctor` before restarting. Restore is deliberately an
offline, explicit `--confirm` operation.

Available console commands:

```text
status
setup
rooms
users
add-room field-ops Field operations
set-name My OMENchat
set-operator node-admin
set-announce-interval 360
set-upload-quota-bytes 52428800
set-upload-max-file-bytes 524288
set-max-message-bytes 2048
set-history-batch-size 50
set-join-backlog-events 50
set-large-batch-threshold-bytes 4096
set-rate-messages-per-minute 20
set-rate-commands-per-minute 12
ban-user 7
mute-user 7
trust-user 7
set-user-role 7 trusted
delete-user 7
prune-stale-users
tcp-server 127.0.0.1:42420
tcp-client gateway.example:42420
show-config
quit
```

Use `setup` when onboarding or checking a headless server. It prints the
first-run checklist, `omenchat://` join URI, NomadNet portal URL, storage rule,
address rule, and active upload policy in one place.

Use `users` before moderation commands. It prints moderation ids, role/status
labels, first/last seen times, and whether a stale user record can be deleted.
`delete-user` intentionally refuses records seen within the last 24 hours.

`omenchatd` is all-in-one server software. By default, every server-owned file
lives under `~/.omenchatd/`, including:

```text
~/.omenchatd/config.toml
~/.omenchatd/identity
~/.omenchatd/omenchat.sqlite
~/.omenchatd/reticulum/
~/.omenchatd/omenchatd.log
```

It must not read from or write to `~/.reticulum`, `~/.nomadnetwork`, `~/.lxmd`,
or OMENbrowser_rs client identity storage unless an operator explicitly changes
paths in the server config.

The TUI admin console is the primary interactive configuration surface for
first-run setup, server identity, Reticulum interfaces, rooms, limits,
moderation, logs, monitoring, MOTD, NomadNet portal preview, and audit review.
When attached to a terminal, `omenchatd tui` opens the Ratatui dashboard. In
non-TTY contexts it falls back to the line console commands shown above.

Server policy values live in `config.toml` and can be changed from the TUI,
line console, or `config set`. The upload quota defaults to 50 MiB per identity;
set `upload_quota_bytes = 0` to disable uploads. The per-file upload cap
defaults to 512 KiB. The client live-link ping
interval defaults to 30 seconds and is advertised to OMENbrowser_rs clients
when a session opens.

`config.toml` is typed and versioned (`version = 1`). Unknown or misspelled
keys, malformed types, future versions, and unsupported changes to fixed policy
fields stop loading with a diagnostic instead of being silently ignored.
Legacy version-0 flat keys remain readable.
Successful updates retain the previous valid document as `config.toml.bak`.
Replacement uses a private synchronized same-directory temporary file; a
pre-commit failure leaves the existing config loadable and removes the temp.

The Reticulum live runtime bounds queued payloads by item count, global bytes,
and per-link bytes. The TUI Monitoring panel and periodic `queues:` log line
report transport/event queue depth, bytes, actual oldest remaining item age,
and overload rejects.
Persistent rejects indicate a slow transport, abusive peer, or reconnect burst;
increasing limits should follow measurement rather than being the first remedy.

Persistent SQLite connections enable foreign-key checks, WAL journaling,
NORMAL synchronization, and a five-second busy timeout. Event ID allocation and
event insertion share one immediate transaction, so concurrent writers cannot
commit duplicate per-room IDs. SQLite work is still synchronous in this unit;
live Reticulum session/database calls execute through a one-in-flight blocking
worker. Concurrent admission fails explicitly instead of accumulating blocking
tasks; pending network events remain in the existing bounded ingress queue.
Queue monitoring includes worker completion, rejection, and latency counters.
CLI room administration, line-console room/user administration, dashboard
room/moderation work, and upload-ledger inspection/repair now use single-owner
database actors with a bounded
16-item queue, non-waiting overload rejection, a finite response deadline, and
queue/in-flight/completion/rejection/latency metrics. The interactive dashboard
retains at most 1,024 rooms/1 MiB and 4,096 users/2 MiB, and polls completions
without blocking render/input. Doctor opens its actor read-only. Confirmed
offline repair opens only an existing current-schema database and waits for the
owned worker to report its final commit result, avoiding timeout ambiguity.
Linux maintainers can run
`scripts/measure-omenchatd-db.sh /tmp/omenchatd-db-results` from the repository
root for a 60-second isolated release-mode worker/store soak. The harness
checks explicit saturation, Tokio heartbeat latency, RSS and file-descriptor
bounds, restart persistence, consecutive event IDs, and SQLite integrity. The
2026-07-13 reference run committed 6,000 events, rejected 42,000 concurrent
submissions, observed 1,272 us maximum worker latency and 1,817 us maximum
heartbeat lateness, grew RSS by 794,624 bytes, and held 13 file descriptors.
This exercises production session/database code with a discard-only transport;
it is not Reticulum wire interoperability evidence.

Reticulum callback logging is non-blocking and bounded: 1,024 records, 1 MiB
of queued log text, and 16 KiB per record. The `queues:` line reports log depth,
bytes, oldest age, drops, and write failures. Buffered handles flush periodically
and on explicit flush requests. The active log rotates before exceeding 8 MiB
and retains three numbered backups, bounding the active set to about 32 MiB.
Older generations are removed inside the background writer.
Routine `Info` logging uses 896 records/768 KiB; call sites explicitly classify
operational failures, overload, timeout, lag, stopped-queue, and malformed
requests as typed `Warning` or `Error` events. Those events use an independent
reserved 128 records/256 KiB and drain first. Message text never chooses queue
priority. Monitoring exposes priority depth and drops, so routine frame floods
cannot consume warning admission capacity. Severity is admission metadata only;
the compatible timestamp/text file lines are unchanged.
Run `scripts/measure-omenchatd-logging.sh <results-dir>` from the repository for
the optimized 60-second slow-writer qualification. It repeats three isolated
writer lifecycles with real rotating files and a deterministic 2 ms delay at
the write boundary, then checks non-blocking admission, explicit overload,
priority survival, graceful drain, RSS/FD stability, and the 32 MiB per-writer
retention cap. The delay is a reproducible slow-disk simulation, not a benchmark
of a particular storage device.

The schema currently uses SQLite `user_version = 2`; version 2 adds the upload
ledger actor/time index used by quota planning. Older files are migrated
transactionally. Files with a newer schema version are rejected
without modification; run the matching or newer omenchatd rather than forcing
the version backward.
Migration of a non-empty older database first retains an online SQLite backup
at `omenchat.sqlite.pre-v2-from-v<old>.bak`. The backup is owner-only on
Unix and is never overwritten. If that path already exists or backup creation
fails, startup aborts before changing the source database.
Migration schema work and its version update are transactional. On failure the
source remains at its old version without partially created migration objects,
and the completed pre-migration backup remains available.
The confirmation-gated restore command described above validates and migrates
that retained artifact through a staging database before replacement, and
preserves the prior active database for rollback.

The SQLite store can compare its upload ledger with an identity directory and
report missing, byte-mismatched, orphaned, and out-of-root paths without
deleting them. Admission performs this scan once per identity before switching
to indexed planning.
`omenchatd doctor` reports aggregate tracked/disk totals and discrepancy counts
using a read-only database handle. Missing/orphan state warns; out-of-root
tracked paths fail. Offline repair remains explicit and confirmation-gated.

Upload replacement is commit-before-evict. Per-identity quota mutation is
serialized; new bytes are written and synchronized through a same-directory
owner-only temporary file, renamed, and inserted into SQLite before old files
are removed. Pre-commit failures preserve the previous committed upload set.
Rows for old uploads remain until their physical removal succeeds. An
interruption therefore leaves an orphan, a conservative over-count, or a
missing old row that the next process reconciliation blocks; it never creates
an undetected physical-quota undercount.
Linux tests additionally exercise kernel `ENOSPC` through `/dev/full` without
mounting a filesystem or touching operator data.

## Public Addresses

`omenchatd status` and live startup output print copyable addresses:

```text
client uri: omenchat://<omenchat-destination-hash>
portal url: <nomadnet-portal-destination-hash>:/page/index.mu
```

Use `client uri` in OMENbrowser_rs's OMENchat opener. Use `portal url` in the
NomadNet browser to view the quiet MOTD/rules/launch page.

When running `omenchatd tui`, use **Announce Now** after the live server is
started to publish both addresses immediately instead of waiting for the
configured announce interval.

The OMENchat protocol destination always announces as:

```text
omenchat.node
```

The NomadNet portal destination announces separately as:

```text
nomadnetwork.node
```

The portal is for discovery and server messages only. Chat traffic uses the
`omenchat://` link.

## NomadNet Portal Page

NomadNet discovery is served from the server-owned Reticulum storage root:

```text
~/.omenchatd/reticulum/storage/pages/index.mu
```

`omenchatd` creates this page on first run if it is missing. After that it is
operator-owned: restart will not overwrite edits, and live page requests read
the file from disk. `omenchatd status` prints the exact page file path, size,
and modified age.

## Moderation And Rooms

Rooms are server-side and persisted in `omenchat.sqlite`.

- Admins can create/archive rooms, change roles, unban users, and perform
  moderator actions.
- Moderators can change topics, kick, ban, mute, unmute, trust, and send
  notices.
- The TUI Moderation panel exposes explicit Standard, Trusted, Moderator, and
  Admin role actions so operators do not need to cycle through roles blindly.
- The TUI Rooms and Moderation panels show the effective role permissions beside
  the action lists, so operators can see which actions require moderator or
  admin authority without opening external docs.
- User records are keyed by the identified OMENbrowser Reticulum identity when
  the client identifies successfully, not by transient Link ids.
- Stale user records older than 24 hours can be deleted from the TUI Moderation
  panel.

## Client Expectations

OMENbrowser_rs opens servers with:

```text
omenchat://<omenchat-destination-hash>
```

The server sends bounded recent history on join/reconnect and answers compact
history fingerprints with either "current" or a small inline backlog batch.
`Load Older` asks for history before the local room floor.

For two local client tests, run each browser with a different `--app-root`.
Sharing a browser app root shares identity, Reticulum config, plugin SQLite
cache, message history, and pane layout.
