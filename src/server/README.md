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
cargo test --locked -p omen-ifac-tcp
cargo check
cargo check --no-default-features --features server-headless
cargo run --no-default-features --features server-headless -- init
cargo run --no-default-features --features server-full -- tui
bash scripts/verify-standalone.sh check
```

`scripts/verify-standalone.sh check` copies this directory without its build
output to a temporary root and runs locked, offline metadata, compile, test
compile, and IFAC tests there. The protocol-neutral `omen-ifac-tcp` crate and
the wire-only `omenchat-protocol` crate and compatibility fixture deliberately
live inside this tree so the standalone source package never imports
OMENbrowser application modules. `omenchat-protocol` owns only the shared wire
types/numbers and fixtures; server runtime, SQLite, Reticulum ownership, and
policy remain in omenchatd.

`server-headless` is the daemon/admin CLI product and excludes Ratatui and
Crossterm. `server-full` adds the optional interactive TUI and is used for the
combined release package so existing `omenchatd tui` behavior is preserved.

### Linux ARM64

The first Linux ARM64 product is the standalone `server-headless` daemon. The
manual/reusable `Linux ARM64 headless` workflow checks and tests the shared
protocol and server on a native GitHub-hosted ARM64 VM, then creates an
`omenchatd-<version>-linux-aarch64.tar.gz` archive using an isolated server
home. It does not run on every push or pull request.

Cross/QEMU execution, hosted native ARM execution, and physical Raspberry Pi
(or equivalent) qualification are separate evidence levels. The maintained
local release gate is `bash scripts/test-linux-arm64-headless.sh`; a successful
run is sufficient for Linux ARM64 headless release qualification. See
`../../docs/maintenance/LINUX_ARM64_HEADLESS.md` before describing the package
as Raspberry Pi or physically tested. The archive does not install or
auto-start a system service and never shares OMENbrowser application state.

Initial commands:

```bash
omenchatd init
omenchatd status
omenchatd doctor
omenchatd config show
omenchatd config set --name "My Chat" --operator-label "node-admin"
omenchatd config set --announce-interval 360 --max-message-bytes 2048
omenchatd rooms list
omenchatd rooms list --json
omenchatd rooms add field-ops --topic "Field operations"
omenchatd interfaces tcp-server 127.0.0.1:42420
omenchatd interfaces tcp-client gateway.example:42420
omenchatd interfaces list
omenchatd tui
omenchatd run
```

`interfaces tcp-client` adds a TCP client without replacing existing TCP
clients or listeners. List the redacted endpoints and remove one exact endpoint
with:

```bash
omenchatd interfaces list
omenchatd interfaces delete tcp-client gateway.example:42420
```

Interface edits are capped at 64 sections and a 2 MiB configuration. Before an
add/delete edit, omenchatd writes an owner-only
`config.before-interface-edit.bak` recovery copy. Restart the live server after
editing interfaces. The runtime already starts every enabled supported
interface in the resulting configuration.

`omenchatd run` arms its platform signal handlers before advertising readiness,
then handles Ctrl-C/SIGINT on all supported platforms and SIGTERM on Unix.
Shutdown stops accepting background transport work, closes active links,
cancels and joins the owned Reticulum event/interface workers, releases queued
byte permits, and flushes the server log before returning. A worker join or log
flush timeout produces a non-success exit instead of silently claiming a clean
stop. The TUI Stop Live Server action uses the same shutdown path.

Enabled interface records are validated before any interface worker is spawned.
Missing hosts, zero/missing ports, missing types, and unsupported types fail
startup clearly; disabled records remain inert. Runtime TCP reconnect remains
owned by the Reticulum 0.9 interface worker, so a transient disconnect does not
cause the TUI to create a competing second runtime.

Live interface health is derived from each worker's current runtime state and
liveness, not merely from configured interface records. Monitoring distinguishes
configured, connecting, reconnecting, operational, degraded, terminal, and
no-interface states. Only three consecutive terminal all-worker observations
schedule one deduplicated full-runtime recovery. The five-second recovery
deadline never blocks TUI input, rendering, Stop, quit, or shutdown, and ordinary
TCP reconnect progress cancels any pending recovery. The TUI also requests a
full redraw after live Reticulum events so direct output from the pinned
transport cannot remain painted over the alternate-screen dashboard.

Both headless and TUI-owned live runtimes use an omenchatd-local Tokio policy:
one through four async workers according to available parallelism, at most eight
blocking threads, and stable `omenchatd-headless`/`omenchatd-tui` worker names.
The normal headless event loop waits on its bounded control and payload queues,
shutdown signal, and required deadlines instead of waking every 25 ms.

Identity creation is limited to a missing identity file or omenchatd's exact
first-run placeholder. An existing malformed, unreadable, non-regular, or
symlinked identity aborts startup without replacement. First-run publication is
an owner-only same-directory atomic replacement; identity parse failures never
silently change the server address.

The selected server home is also the private-path trust anchor. Before parsing
an existing `config.toml`, omenchatd requires a stable regular non-symlink file
and performs a bounded handle read. Configured identity, database, and
Reticulum paths reject `..`. Managed descendants are walked component by
component from the canonical root and cannot cross an intermediate symlink.
Clean custom paths remain supported, but their existing ancestors are not
claimed or chmodded and must be controlled by the operator. Server SQLite
source opens use SQLite `NOFOLLOW`. On Unix, the validated parent is first
resolved to its stable filesystem spelling so system aliases such as macOS
`/var` remain usable, while the final database name remains unresolved and
protected by `NOFOLLOW`. This does not change WAL mode, schema, migration,
backup, timeout, or transaction behavior.

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

IFAC is currently enforced only by omenchatd's project-local TCP **client**
adapter. A `TCPServerInterface` containing `network_name` or `passphrase` is
rejected at startup because the published reticulum-rs 0.9.7 stock TCP server
does not apply the Python IFAC wire transform. Run the enforcing gateway as the
server and connect omenchatd to it as shown above.

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

Service monitors can request schema-versioned JSON without exposing local
paths or free-form configuration text:

```bash
omenchatd status --json --home ~/.omenchatd
omenchatd doctor --json --home ~/.omenchatd
```

`status --json` reports the omenchatd/application version, pinned Reticulum
crate train, independent runtime ownership, public destination lines, storage
presence, interface readiness level, room count, and numeric limits. Because
this is an offline command rather than an RPC connection to the running
process, `runtime.live_metrics_available` is explicitly `false`; queue, link,
resource, and event-worker counters remain available in the live TUI and
periodic server log. `doctor --json` reports the overall outcome and typed
check levels. Both formats omit private paths, credentials, private identity
material, operator/MOTD text, and free-form check details that could contain a
path. Consumers must check `schema_version` before relying on fields.

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
  --from ~/.omenchatd/omenchat.sqlite.pre-v5-from-v4.bak \
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

Schema 11 stores an explicit ordinary/announcement policy per room. The
v0.9.6-6 policy-administration contract is deliberately restart-only: stop
omenchatd, run the maintenance command, then restart omenchatd. The command
fails closed while the live server owns the database. This release does not
reload policy or fan out policy deltas from an offline maintenance process:

```bash
# Stop the running omenchatd process or service first.
omenchatd rooms policy 1 announcement --confirm --home ~/.omenchatd
omenchatd rooms list --json --home ~/.omenchatd
# Restart omenchatd using the same service or run configuration.
```

Only `ordinary` and `announcement` are accepted. The policy and room revision
change in one immediate transaction. Announcement rooms remain readable by
members, while content publication is enforced server-side for moderators and
administrators regardless of client capability negotiation.

Headless installations can inspect the bounded administrative user projection
and make an explicit stopped-server role change without enabling the optional
TUI:

```bash
omenchatd users list --json --home ~/.omenchatd
omenchatd users role 7 moderator --confirm --home ~/.omenchatd
```

Role vocabulary is exactly `standard`, `trusted`, `moderator`, or
`administrator`. The JSON listing omits identity hashes and LXMF destinations.
Role changes use the same exclusive maintenance-open boundary as room-policy
changes and fail while omenchatd owns the database.

Schema 12 also stores the future per-room slow-mode interval. Configuration is
restart-only and confirmation-gated through the same exclusive maintenance
boundary:

```bash
# Stop omenchatd first.
omenchatd rooms set-slow-mode 1 30 --confirm --home ~/.omenchatd
omenchatd rooms list --json --home ~/.omenchatd
omenchatd rooms set-slow-mode 1 off --confirm --home ~/.omenchatd
```

Numeric intervals are whole seconds in `1..=86400`; `off` stores zero. The
scalar and room revision change in one immediate transaction, while a no-op
retains the revision. Output reports the prior and configured values and
reports `enforcement=active` in canonical server builds. Room listings use
`slow_mode_config`/`slow_mode_seconds` plus
`slow_mode_enforcement=active`. A capable server accepts
`room-slow-mode-v1` only when the client also negotiates durable mutations;
legacy or non-negotiating sessions retain their prior room shape and behavior.
Builds that deliberately omit `omenchat-slow-mode` preserve the stored scalar
but report enforcement inactive.

Schema 13 stores a nullable per-room upload file ceiling for
`room-media-policy-v1`. `NULL` inherits the global server file ceiling,
zero will disable room uploads, and positive values are constrained to at most
10 MiB. Configuration is restart-only and confirmation-gated through the same
exclusive maintenance boundary:

```bash
# Stop omenchatd first.
omenchatd rooms set-upload-policy 1 262144 --confirm --home ~/.omenchatd
omenchatd rooms list --json --home ~/.omenchatd
omenchatd rooms set-upload-policy 1 disabled --confirm --home ~/.omenchatd
omenchatd rooms set-upload-policy 1 inherit --confirm --home ~/.omenchatd
```

`inherit` stores `NULL`, `disabled` stores zero, and a numeric value must be in
`1..=10485760`. The scalar and room revision change in one immediate
transaction, while a no-op retains the revision. Output reports the prior,
configured, and effective values and reports `enforcement=active` in canonical
server builds.
Human and JSON room listings are bounded to 1,024 rows and 1 MiB of retained
room data, report truncation explicitly, and distinguish configured policy
from the effective room/global minimum.

Canonical omenchatd accepts the capability only from an authenticated Link that
requested the complete dependency set. It resolves inherited, disabled, and
room/global-minimum limits at both offer and Resource-publication boundaries.
Legacy/non-negotiating peers retain global admission for wire compatibility;
use the global ceiling when every peer must share one hard limit. Builds that
deliberately omit `omenchat-room-media-policy` preserve stored values and
report enforcement inactive. The optional TUI reports configured/effective
policy and the real feature state; it does not edit policy while the server is
running.

To prepare a separate schema-12-compatible rollback copy while retaining the
active schema-13 database, stop the server cleanly and run:

```bash
omenchatd database export-schema12-copy \
  --to ~/.omenchatd/omenchat-schema12.sqlite \
  --confirm --home ~/.omenchatd
```

The destination must not exist. The command removes only the
`upload_max_file_bytes` room column from a private staged copy. It retains
slow-mode settings and admissions, announcement policy, the upload ledger,
history, identities, and every earlier schema layer. It never edits the active
database.

To prepare a
separate schema-11-compatible rollback copy while retaining the active
schema-13 database, stop the server cleanly and run:

```bash
omenchatd database export-schema11-copy \
  --to ~/.omenchatd/omenchat-schema11.sqlite \
  --confirm --home ~/.omenchatd
```

The destination must not exist. The command removes only schema-12 slow-mode
settings and admission state from a staged copy. It preserves announcement
policy, moderation audit, and every earlier layer.

To prepare a
separate schema-10-compatible rollback copy while retaining the active
schema-13 database, stop the server cleanly and run:

```bash
omenchatd database export-schema10-copy \
  --to ~/.omenchatd/omenchat-schema10.sqlite \
  --confirm --home ~/.omenchatd
```

The destination must not exist. The command removes schema-12 slow-mode
storage and the schema-11 policy column from a staged copy, while preserving
moderation-audit and every earlier layer.

To prepare a separate schema-9-compatible rollback copy while retaining the
active schema-13 database, stop the server cleanly and run:

```bash
omenchatd database export-schema9-copy \
  --to ~/.omenchatd/omenchat-schema9.sqlite \
  --confirm --home ~/.omenchatd
```

The destination must not exist. The command removes only schema-10
client-visible moderation-audit rows and indexes from a staged copy. It
preserves schema-9 pins and every earlier history, replay, identity, user,
room, and upload layer. Canonical server profiles activate the bounded,
capability-negotiated, read-only moderation-audit path; authorization remains
required.

To prepare a separate schema-8-compatible rollback copy while retaining the
active schema-13 database, stop the server cleanly and run:

```bash
omenchatd database export-schema8-copy \
  --to ~/.omenchatd/omenchat-schema8.sqlite \
  --confirm --home ~/.omenchatd
```

The destination must not exist. The command removes schema-10 moderation
history and schema-9 pin state/audit objects from a staged copy and preserves
history usage, event sequences, message revisions, reactions, and ordinary
history.

Schema-9 pin storage and its durable execution path are capability gated.
omenchatd accepts `room-pins-v1` only beside a valid explicit
`durable-mutations-v1` request with persistent client identity. Current
moderator/administrator, membership, retained-target, replay, and bounded
storage checks remain authoritative for every mutation.

To prepare a separate schema-7-compatible rollback copy while retaining the
active schema-13 database, stop the server cleanly and run:

```bash
omenchatd database export-schema7-copy \
  --to ~/.omenchatd/omenchat-schema7.sqlite \
  --confirm --home ~/.omenchatd
```

The destination must not exist. The command removes only schema-8 room-history
usage metadata from a staged copy and preserves event sequences, message
revisions, reactions, and ordinary history.

To prepare a separate schema-6-compatible rollback copy while retaining the
active schema-13 database, stop the server cleanly and run:

```bash
omenchatd database export-schema6-copy \
  --to ~/.omenchatd/omenchat-schema6.sqlite \
  --confirm --home ~/.omenchatd
```

The destination must not exist. The command removes schema-8 usage and
schema-7 room-event sequence metadata from a staged copy, sets
`user_version = 6`, and validates integrity and foreign keys before atomic
publication. Message revisions, reactions, ordinary history, and the active
database are preserved. Retention is not active, so exporting the accounting
metadata cannot conceal prior compaction.

To prepare a separate schema-5-compatible rollback copy while retaining the
active schema-13 database, stop the server cleanly and run:

```bash
omenchatd database export-schema5-copy \
  --to ~/.omenchatd/omenchat-schema5.sqlite \
  --confirm --home ~/.omenchatd
```

The destination must not exist. The command refuses active WAL/SHM state,
proves exclusive access, copies through SQLite's backup API, removes only the
schema-6 message-revision tables and indexes in a staged transaction, sets
`user_version = 5`, and validates integrity and foreign keys before atomic
publication. The active database is never replaced or modified. Reaction
state and ordinary history are preserved; revision state is intentionally
absent from the compatibility copy.

For a deeper schema-4-compatible rollback copy, run:

```bash
omenchatd database export-schema4-copy \
  --to ~/.omenchatd/omenchat-schema4.sqlite \
  --confirm --home ~/.omenchatd
```

The destination must not exist. The command refuses active WAL/SHM state,
proves exclusive access, copies through SQLite's backup API, removes the
schema-6 message-revision layer and schema-5 reaction layer in a staged
transaction, sets
`user_version = 4`, and validates integrity and foreign keys before atomic
publication. The active database is never replaced or modified. Reaction state
is intentionally absent from the rollback copy; rooms, users, ordinary
history, uploads, durable replay records, identities, and configuration remain.

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
tcp-client-delete gateway.example:42420
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

On Unix, the selected server home and exact server-owned directories are
created/repaired as `0700`; config, identity, SQLite main/WAL/SHM, migration
backups, uploads, generated pages, and active/rotated logs are `0600`. Custom
database or identity files are protected without chmodding arbitrary existing
ancestors. The systemd user unit also uses `UMask=0077`, but direct binary runs
receive the same code-level protection. See `docs/PRIVATE_STORAGE.md` in the
repository/package root.

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

Generated OMENchat history, user-list, and upload-fetch Resource payloads are
also bounded before they reach the transport: at most 64 pending payloads,
16 MiB total, and 4 MiB per payload. Admission beyond any boundary fails the
request explicitly without evicting a payload promised by an earlier response.
A payload remains available through all recipients of one room fan-out and is
then released; transport send failure releases every payload generated for that
response batch. The periodic `stats:` line reports pending resource items,
bytes, and cumulative rejected admissions.

Accepted inbound upload offers retain metadata only within a fixed admission
budget: 256 offers globally and eight per identified peer. Filenames and
content types are each limited to 255 encoded bytes. Excess offers receive an
explicit rejection without displacing another peer's reservation. An unused
offer expires after six hours and all offers owned by a link identity are
released when that link closes, is administratively disconnected, or is
replaced. Presenting another identity's resource ID does not consume the true
owner's reservation. Live `stats:` output reports pending offer items,
identities, rejected admissions, and expirations. These are fixed abuse-control
ceilings, not operator-adjustable quota settings.

Reticulum 0.9 Resource terminals are projected onto the bounded event control
lane instead of being discarded. Outbound completion, failure, and
cancellation remain counted even when their link has already closed. An
outbound application Resource ID is correlated to the exact Reticulum hash in a
bounded, expiring per-link map and released on terminal, link close, shutdown,
or TTL. Upstream inbound failure events expose the exact hash and expected byte
count but not the failed Resource metadata/application ID. The server therefore
releases one pending upload only when identity plus expected size selects exactly
one candidate; unmatched or ambiguous failures remove none. Identity-wide
cleanup remains reserved for link close, disconnect, replacement, and TTL. The
`stats:` line reports exact outbound, unique inbound, unmatched, and ambiguous
correlation counts. Successful Resource handling and wire fields are
unchanged. A deterministic isolated regression feeds the public Reticulum 0.9
terminal variants through the production Resource-event receiver and bounded
control lane, then proves permits drain and the owned worker joins after
shutdown. This covers the crate-to-project callback boundary but is not a
claim of physical initiator-cancel or mixed-version wire interoperability.
An explicit ignored loopback test supplies the next evidence layer: two real
Reticulum 0.9 transports establish a point-to-point UDP link, the receiver
observes both the Resource advertisement and initiator-cancel packets, the
production bridge emits the outbound-cancel terminal, and both link ends remain
active through bounded shutdown. Run it with the command documented in
`docs/TESTING.md`; it uses ephemeral identities, ports, and isolated roots.
Post-cancel Resource completion and Python/mixed-version peers remain separate
interop gates.

The separate two-process completion/cancel/reuse gate was rerun on the exact
locked crates.io 0.9.7 transport on 2026-08-01. It remains red and blocks a
claim of live UDP Resource parity, but the maintainer classifies the published
upstream limitation as non-blocking for the version-aligned OMEN release. Its
receiver obtains the baseline
advertisement and sends valid requests; its sender receives, decrypts, and
hash-matches every request but sends no Resource parts before the receiver's
retry budget expires. The explicit command and evidence boundary are recorded
in `docs/TESTING.md`. The test remains ignored in fast suites and must pass
before UDP Resource completion is claimed. Current OMENchat upload/history and
NomadNet Resource paths retain their separate passing interoperability evidence.
The failure is isolated to the published Reticulum 0.9.7 UDP worker: its 456-byte
layout-derived transmit buffer cannot serialize a 483-byte maximum Resource
wire packet and silently drops the serialization error. This remains unchanged
in the exact registry 0.9.7 train as checked on 2026-08-01; no protocol-limit or
application-fragmentation workaround is enabled.

Persistent SQLite connections enable foreign-key checks, WAL journaling,
NORMAL synchronization, and a five-second busy timeout. Event ID allocation and
event insertion share one immediate transaction, so concurrent writers cannot
commit duplicate per-room IDs. SQLite work is still synchronous in this unit;
live Reticulum session/database calls execute through a one-in-flight blocking
worker. Concurrent admission fails explicitly instead of accumulating blocking
tasks; pending network events remain in the existing bounded ingress queue.
Queue monitoring includes worker completion, rejection, and latency counters.
Live status also reports the bounded room-mutation replay cache: exact same-link
replay hits, sequence/content collisions, rejected cache admissions, retained
items, and retained bytes. The cache prevents duplicate room message/action/
notice, part, and mutating-command execution on one link and is cleared when
that link closes. Part and kick/ban live-link side effects require a successful
typed engine result; rate-limited or denied moderation cannot disconnect a
target. This is not a claim of cross-link or post-restart idempotency.

Only one active client link is retained for an identified peer. When a newer
link presents the same authenticated identity, omenchatd retires the older
link, asks the Reticulum transport to close it, and releases its room,
response-context, replay-cache, timing, and traffic state before continuing
with the replacement. Room traffic is forwarded only to active links joined to
that room. This is peer-link lifecycle behavior, not server federation:
omenchatd does not currently define a server-to-server wire protocol, cache
repair exchange, or cross-link mutation idempotency contract.

Live admission retains at most 256 links and at most 32 incomplete handshakes.
A handshake is complete only after Reticulum reports the peer identity and the
client sends a valid OMENchat `SessionOpen`; the two events may arrive in either
order. Incomplete links are physically closed after 30 seconds, with a
one-second deadline sweep. Rejected and expired links do not retain room,
response-context, replay-cache, traffic, or upload-offer ownership. The
`stats:` line reports pending, rejected, and expired handshakes. These fixed
ceilings are process-safety boundaries, not operator configuration or wire
protocol fields.

Linux maintainers can run
`scripts/measure-omenchatd-links.sh /tmp/omenchatd-link-results` from the
repository root for the optimized 60-second admission/reconnect qualification.
It holds 224 authenticated sessions, repeatedly fills the remaining 32
handshake slots, verifies overload rejection and exact timeout cleanup,
replaces identified links, then drains every link. The test fails on bound or
accounting violations, close latency over 250 ms, RSS growth over 64 MiB, more
than four additional file descriptors, more than two additional tasks, or any
retained final link. It uses an in-memory database and discard/count transport;
it does not contact the operator's Reticulum instance or claim wire interop.
The 2026-07-16 reference run completed 4,587 saturation/recovery cycles,
rejected 4,587 excess links, expired 146,784 slow handshakes, reached exactly
256 active/32 pending links, drained to zero, observed 691 us maximum
close-path latency, grew RSS by 176,128 bytes, and added no file descriptors or
tasks.

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

The schema currently uses SQLite `user_version = 14`. Version 2 added the upload
ledger actor/time index used by quota planning. Version 3 adds the
bounded-shape durable-mutation replay table, client-instance retirement table,
and their indexes. Version 4 adds nullable reply-event and bounded mention-ID
metadata to room events plus a partial reply index. Existing event rows retain
their original columns byte-for-byte and read as messages without metadata.
The negotiated reply/mention path validates joined senders,
same-room non-deleted reply targets, and current numeric mention membership in
the same durable transaction that inserts the event and replay result. Its
single event encoder preserves metadata across fan-out, inline history, and
resource history; restart replay cannot duplicate the event. Version 5 adds
constrained active-reaction and append-only reaction-audit tables plus their
target/retention indexes. The production `reactions-v1` path implements
bounded add/remove state, exact replay/conflict handling, incremental audit
retention, authoritative inline/resource snapshots, and capability-scoped live
fan-out.
Version 6 adds constrained message-revision current-state and append-only audit
tables plus lookup/retention indexes. The production
`message-revisions-v1` executor enforces author/moderator/mute policy,
immutable originals, correction and storage ceilings, incremental audit
pruning, reaction cleanup, exact durable replay, and inline/Resource
snapshots. The current client requests the capability and exposes bounded
correction/tombstone actions only after explicit acceptance.
Version 7 adds a persistent per-room event-ID high-water mark. Existing rooms
seed it lazily from their indexed maximum on the first new event, so migration
does not scan history. Event allocation advances the high-water mark and
inserts the event in one immediate transaction; rollback can reuse only an ID
that was never committed. Deleting retained history can no longer make a
committed event ID reusable. Room-history retention remains disabled and no
events are deleted by this schema change.
Version 8 adds an empty per-room history item/byte usage ledger. Migration does
not scan history. New events are accounted in their existing immediate
transaction, while legacy rows advance by at most 256 per append or explicit
maintenance call. The backfill cursor survives restart and retention remains
disabled until accounting is complete. Accounting failure rolls back event
insertion and sequence advancement.
Versions 9 and 10 add bounded pin and moderation-audit storage. Canonical
profiles activate both only through explicit negotiation, with moderation
audit additionally restricted by role. Version 11 adds
the constrained announcement-room policy scalar, and version 12 adds the
bounded slow-mode scalar plus its admission table and expiry index. Version 13
adds nullable, constrained `upload_max_file_bytes` room storage. Canonical
profiles activate its enforcement only after cumulative
`durable-mutations-v1`, `announcement-rooms-v1`,
`room-slow-mode-v1`, and `room-media-policy-v1` acceptance.
Version 14 adds nullable, checked `users.nickname_colour_rgb`, with `NULL`
meaning automatic. It reuses `profile_revision`; exact durable replay returns
the stored acknowledgement without a second revision or broadcast. The
pre-migration schema-13 backup is required for rollback to v0.9.8-3.

The authoritative current client/server capability matrix is maintained in
`../../docs/OMENCHAT_PROTOCOL.md`. Older “dormant” descriptions in migration
records describe their staging release and are not current product state.
The store also contains an explicit compaction primitive. It requires complete
accounting, removes no more than 64 original events per immediate transaction,
bounds dependent reply/reaction/revision work to 20,000 rows, and updates all
projections and usage accounting atomically. Upload storage, durable replay,
and event-ID sequences are preserved. Only live event admission calls it when
the typed policy below is enabled; there is no startup sweep, timer, RPC
compactor, or UI deletion control.
Generated configuration includes:

```toml
[history_retention]
enabled = false
max_age_days = 365
max_events_per_room = 100000
max_bytes_per_room = 268435456
```

Enabled zero limits are rejected. Values above 3,650 days, 1,000,000 events,
or 10 GiB are clamped to those documented maxima. `status` and `status --json`
inspect at most 256 room usage ledgers through a read-only connection and
report truncation plus complete/incomplete/missing accounting. They never
advance backfill or compact history. Status reports configured admission
behavior and explicitly says that it cannot observe live runtime activity.
With `enabled = true`, only the live server store applies the policy. Ordinary
and durable event insertions independently enforce age, item, and byte ceilings
inside their existing transaction, deleting at most 64 older originals plus
dependent projections. A sole newest event may exceed the byte ceiling.
Incomplete accounting or saturation requiring another batch rejects the event
and rolls back insertion and cleanup together. There is no startup sweep,
timer, polling worker, RPC compactor, or TUI deletion action.

For an upgraded room whose usage ledger needs more than one backfill batch,
stop omenchatd and advance one 256-event metadata batch explicitly:

```bash
omenchatd database advance-history-usage \
  --room-id 1 \
  --confirm \
  --home ~/.omenchatd
```

The command requires an existing current-schema database, updates only the
selected room's usage metadata, and reports the cursor, captured target, item
and byte totals, and completion state. Repeat it while the server remains
stopped until `complete=true`, then inspect `status` before enabling retention.
It never deletes or compacts room history.

The isolated durable store boundary already enforces exact
request replay, conflicting-hash refusal, a 64 KiB encoded-result ceiling,
bounded global/per-identity item and byte budgets, and at most 128 incremental
deletions per commit. Before deleting a replay result it permanently retires
the associated authenticated identity/client-instance pair; all later requests
from that instance return `Expired` without mutation execution, including after
restart. Remembered instances are capped at 100,000 globally and 1,024 per
identity, with capacity exhaustion failing closed. Protocol-v1 error numbers
1011 through 1015 describe negotiated durable outcomes; ordinary sessions do
not emit them. Error 1016 is the typed announcement-room publication
restriction and can apply to legacy or durable mutations independently of
capability negotiation. Older files are migrated
transactionally. Files with
a newer schema version are rejected without modification; run the matching or
newer omenchatd rather than forcing the version backward.
Migration of a non-empty older database first retains an online SQLite backup
at `omenchat.sqlite.pre-v13-from-v<old>.bak`. The backup is owner-only on
Unix and is never overwritten. If that path already exists or backup creation
fails, startup aborts before changing the source database.
Migration schema work and its version update are transactional. On failure the
source remains at its old version without partially created migration objects,
and the completed pre-migration backup remains available.
The confirmation-gated restore command described above validates and migrates
that retained artifact through a staging database before replacement, and
preserves the prior active database for rollback.
The separate `export-schema12-copy` command removes only per-room
upload-ceiling storage while preserving slow-mode state and every earlier
layer.
The separate `export-schema11-copy` command removes only slow-mode settings and
admission state while preserving announcement-room policy and every earlier
layer.
The separate `export-schema10-copy` command removes slow-mode and
announcement-room
policy storage while preserving moderation-audit and every earlier layer.
The separate `export-schema9-copy` command removes only moderation-audit
storage while preserving schema-9 pins and every earlier layer.
`export-schema8-copy` removes moderation-audit and pin storage while preserving
every schema-8 history layer. `export-schema7-copy` removes
usage metadata while
preserving event sequences and all history layers. `export-schema6-copy`
provides a non-destructive downgrade artifact without usage or sequence
metadata while preserving revisions, reactions, and history.
`export-schema5-copy` omits revision state while preserving reactions.
The deeper `export-schema4-copy` artifact omits both reactions and revisions.
No export command edits the active schema-13 database.

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

The portal accepts Python-compatible direct request-context packets for normal
small page requests and retains request-resource handling for oversized packed
requests. Direct responses are sent only on the active inbound link's bound
interface; portal file reads are serialized through one owned blocking worker.

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
