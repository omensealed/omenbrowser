# omenchatd

`omenchatd` is the standalone Rust OMENchat server. It is hosted under
`OMENbrowser_rs/src/server/` during development, but it must remain movable and
independent.

This directory must not import OMENbrowser_rs modules or depend on the browser
crate. The client and server communicate through the documented protocol in:

```text
../../docs/26-omenchat-protocol-v0.1.md
```

Standalone check:

```bash
cd src/server
cargo check
cargo check --features live-rns-net
cargo run --features live-rns-net -- init
```

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
omenchatd interfaces tcp-client gateway.example:42420 \
  --home /tmp/omenchatd-demo \
  --network-name private_ret \
  --passphrase change-me
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

## Public Addresses

`omenchatd status` and live startup output print copyable addresses:

```text
client uri: omenchat://<omenchat-destination-hash>
portal url: <nomadnet-portal-destination-hash>:/page/index.mu
```

Use `client uri` in OMENbrowser_rs's OMENchat opener. Use `portal url` in the
NomadNet browser to view the quiet MOTD/rules/launch page.

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
