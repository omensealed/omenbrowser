# 27 - Alpha Tester Runbook

This runbook is the short outside-tester path for OMENbrowser_rs and
omenchatd. It is intentionally narrower than `docs/24-native-live-runbook.md`.
Use it when the goal is to prove that another person can install, start, use,
and report issues without reading the development history.

## Scope

This is a private alpha checklist, not a public release promise.

Expected working areas:

- OMENbrowser_rs desktop UI.
- Managed Reticulum identity and app-owned storage.
- NomadNet page loading through native Rust RNS/LXMF crates.
- LXMF direct send/receive.
- LXMF propagation send/sync where the selected propagation node is reachable.
- OMENchat plugin client.
- Standalone omenchatd server with app-owned `~/.omenchatd` storage.

Known alpha caveats:

- Native LXMF ticket/stamp sending is not implemented yet.
- Some Reticulum interfaces depend on what the Rust RNS crates currently expose.
- GPU backend support depends on the user's system; release builds should be used
  for UI testing.
- Outside testers should use a disposable identity unless they understand the
  identity/storage model.

## Build Commands

Build the desktop browser with OMENchat/RNS support:

```bash
cargo build --release --features chat-client-rns
```

Build the standalone OMENchat server:

```bash
cargo build --release --manifest-path src/server/Cargo.toml --features live-rns-net
export OMENCHATD_BIN=./src/server/target/release/omenchatd
```

Create a private alpha bundle with both binaries and starter docs:

```bash
bash scripts/alpha-package.sh
```

The script stages `omenbrowser_rs`, `omenchatd`, `ALPHA-START.txt`,
`TESTERS.md`, this runbook, protocol notes, `scripts/alpha-collect.sh`,
`scripts/alpha-omenchat-smoke.sh`, `scripts/install-omenchatd-user-service.sh`,
the systemd user-service template, package metadata, captured `--help` output,
and checksums under `dist/`. It verifies both staged binaries can start far
enough to print help without launching the GUI or live server. It also verifies
`omenchatd init/status` against a temporary isolated server home, then writes a
`.tar.gz` archive and matching `.sha256` file next to the staged directory.
After the archive is written, the script extracts it into a temporary directory
and repeats the binary help plus isolated `omenchatd init/status` checks from
the unpacked copy. It also runs the bundled `scripts/alpha-collect.sh` against
temporary fake app roots and syntax-checks the bundled OMENchat smoke helper
and systemd user-service installer.
For handoff convenience, it also refreshes
`OMENbrowser_rs-alpha-latest.tar.gz`, `OMENbrowser_rs-alpha-latest.tar.gz.sha256`,
and `OMENbrowser_rs-alpha-latest.txt` in the output directory while preserving
the timestamped archive.

Run full local verification before sending a build to testers:

```bash
./target/release/omenbrowser_rs --version
./src/server/target/release/omenchatd --version
bash scripts/alpha-check.sh full
```

Validate the latest packaged archive before handoff:

```bash
bash scripts/alpha-check.sh package
```

The package check extracts the latest tarball, verifies required files, syntax
checks bundled scripts, validates staged binary help, runs isolated
`omenchatd init/status`, verifies the redacted collector output shape, and runs
the bundled local OMENchat smoke with the two-client recent-history check.

The expanded commands are:

```bash
cargo fmt --check
cargo clippy --features chat-client-rns -- -D warnings
cargo test --features chat-client-rns

cargo fmt --manifest-path src/server/Cargo.toml --check
cargo clippy --manifest-path src/server/Cargo.toml --features live-rns-net -- -D warnings
cargo test --manifest-path src/server/Cargo.toml --features live-rns-net
```

## Clean Test Roots

For first outside testing, avoid the developer's normal config paths.
Every parallel browser instance should use its own app root. This prevents
identity, Reticulum config, message history, plugin cache, pane layout, and
known-destination state from colliding with another running copy.

Browser test root:

```bash
export OMENBROWSER_ALPHA_ROOT=/tmp/omenbrowser-rs-alpha
mkdir -p "$OMENBROWSER_ALPHA_ROOT"
```

Optional second browser root for two-client OMENchat or LXMF testing on one
machine:

```bash
export OMENBROWSER_ALPHA_ROOT_2=/tmp/omenbrowser-rs-alpha-2
mkdir -p "$OMENBROWSER_ALPHA_ROOT_2"
```

Server test root:

```bash
export OMENCHATD_ALPHA_HOME=/tmp/omenchatd-alpha
mkdir -p "$OMENCHATD_ALPHA_HOME"
```

Pre-flight root sanity check:

```bash
bash scripts/alpha-root-sanity.sh \
  --browser-root "$OMENBROWSER_ALPHA_ROOT" \
  --browser-root-2 "$OMENBROWSER_ALPHA_ROOT_2" \
  --server-home "$OMENCHATD_ALPHA_HOME"
```

Pass criteria:

- The script prints `root sanity: pass`.
- The two browser roots are different from each other.
- The browser roots are different from the `omenchatd` server home.
- None of those roots are inside `~/.reticulum`, `~/.nomadnetwork`, or
  `~/.lxmd`.

The normal default paths are:

```text
OMENbrowser_rs: ~/.config/OMENbrowser_rs/
omenchatd:      ~/.omenchatd/
```

omenchatd must not use `~/.reticulum`, `~/.nomadnetwork`, or `~/.lxmd` unless an
operator explicitly points it somewhere else. The default server home owns its
identity, database, and Reticulum config.

## Browser Startup

Start the desktop UI from a clean root:

```bash
./target/release/omenbrowser_rs --desktop --app-root "$OMENBROWSER_ALPHA_ROOT"
```

Start a second isolated browser instance only with a different root:

```bash
./target/release/omenbrowser_rs --desktop --app-root "$OMENBROWSER_ALPHA_ROOT_2"
```

Expected result:

- The desktop UI opens without needing Python, rnsd, lxmd, or NomadNet.
- The runtime starts or reports a clear blocked state.
- The Identity page can create or select an identity without overwriting an
  existing one.
- The Interfaces page can configure the test gateway/interface.
- Managed Reticulum config generation uses a stable per-config
  `instance_name`, so two isolated app roots can run at the same time without
  sharing the same Reticulum instance name.

## Browser Live NomadNet Smoke

Use a known reachable node and gateway.

```bash
./target/release/omenbrowser_rs \
  --native-validate <nomadnet-destination-hash>:/page/index.mu \
  --app-root "$OMENBROWSER_ALPHA_ROOT" \
  --tcp-client <host:port> \
  --path-wait 15 \
  --stdout \
  --bundle-report /tmp/omenbrowser-rs-alpha-bundles \
  > /tmp/omenbrowser-rs-alpha-nomadnet.json \
  2> /tmp/omenbrowser-rs-alpha-nomadnet.summary.txt
```

Pass criteria:

- The summary outcome is `pass`.
- The runtime starts.
- A path is known or becomes known.
- A page fetch returns Micron content.

Manual UI check:

- Open the same NomadNet URL in the Browser pane.
- Restart OMENbrowser_rs.
- The restored browser pane should request/wait for path evidence before
  reloading the page, not immediately fail and require manual retry.
- Follow at least three links.
- Submit one basic Micron form if the node provides one.
- Confirm `Ctrl` + mousewheel zoom still affects only the hovered Micron
  viewport.

## LXMF Smoke

Direct LXMF send/receive should be tested with a peer controlled by the tester.

```bash
./target/release/omenbrowser_rs \
  --lxmf-interop \
  --send-lxmf-smoke <lxmf-peer-destination-hash> \
  --lxmf-wait 30 \
  --app-root "$OMENBROWSER_ALPHA_ROOT" \
  --tcp-client <host:port> \
  --stdout \
  --bundle-report /tmp/omenbrowser-rs-alpha-bundles \
  > /tmp/omenbrowser-rs-alpha-lxmf.json \
  2> /tmp/omenbrowser-rs-alpha-lxmf.summary.txt
```

Pass criteria:

- The direct message is visible on the remote peer.
- Inbound reply appears in OMENbrowser_rs.
- Deleting the conversation removes the visible conversation.
- After restart, the deleted conversation does not return unless a newer inbound
  message arrives or the user manually opens that peer again.

Propagation check:

- Select a propagation node in Directory.
- Send a propagated message.
- Sync propagation on the remote Python/NomadNet side if needed.
- Confirm the sent row appears immediately in OMENbrowser_rs.
- Confirm the status does not claim a hard failure after the propagation node
  accepts the handoff.

## omenchatd Setup

Initialize an isolated server:

```bash
"$OMENCHATD_BIN" init --home "$OMENCHATD_ALPHA_HOME"
```

Check the server-owned identity and OMENchat destination:

```bash
"$OMENCHATD_BIN" status --home "$OMENCHATD_ALPHA_HOME"
```

Run the server readiness checker:

```bash
"$OMENCHATD_BIN" doctor --home "$OMENCHATD_ALPHA_HOME"
```

`doctor` checks the server-owned config, identity, database, Reticulum config
and storage, NomadNet portal page, active rooms, interface hints, and basic
limits without starting the live server.

Copy these lines from `status` or from the startup log:

```text
client uri: omenchat://<omenchat-destination-hash>
portal url: <nomadnet-portal-destination-hash>:/page/index.mu
```

Use the `client uri` in the OMENchat opener. Use the `portal url` in the
NomadNet browser when testing the quiet server MOTD/rules/launch page. The
portal page is served from:

```text
$OMENCHATD_ALPHA_HOME/reticulum/storage/pages/index.mu
```

`omenchatd` creates that file only when missing. Operator edits should survive
server restarts.

Configure a gateway/interface using the TUI or CLI. In the TUI, open
Interfaces or press `w` to write a Connect To Gateway config. Press `i` only
when you specifically want a Local TCP Listener test config.
The Overview panel can also edit the server name, operator label, server MOTD,
announce interval, and chat/history limits without hand-editing `config.toml`.
The Portal panel shows the `omenchat://` URI, NomadNet portal URL, page file
path, and current `index.mu` preview from the server-owned Reticulum storage.

Optional systemd user service after gateway setup:

```bash
bash ./scripts/install-omenchatd-user-service.sh \
  --bin "$PWD/bin/omenchatd" \
  --home "$OMENCHATD_ALPHA_HOME"
```

The script writes a unit under `~/.config/systemd/user/`, reloads the user
manager if available, and prints `systemctl --user` commands for status,
start/stop, and logs. Pass `--enable` or `--start` only after confirming the
server-owned Reticulum config is correct.

Remove only the user service, preserving identity, database, logs, Reticulum
config/storage, and portal pages:

```bash
bash ./scripts/install-omenchatd-user-service.sh --uninstall
```

Local TCP smoke example:

```bash
"$OMENCHATD_BIN" init \
  --home "$OMENCHATD_ALPHA_HOME" \
  --tcp-server 127.0.0.1:42420
```

Backbone TCP gateway example:

```bash
"$OMENCHATD_BIN" interfaces tcp-client <gateway-host:port> \
  --home "$OMENCHATD_ALPHA_HOME"
```

If the gateway uses IFAC/network credentials, include them in the generated
isolated Reticulum config:

```bash
"$OMENCHATD_BIN" interfaces tcp-client <gateway-host:port> \
  --home "$OMENCHATD_ALPHA_HOME" \
  --network-name <network-name> \
  --passphrase <passphrase>
```

or during init:

```bash
"$OMENCHATD_BIN" init \
  --home "$OMENCHATD_ALPHA_HOME" \
  --tcp-client <gateway-host:port>
```

Plain `init` still writes an editable baseline Reticulum config at:

```text
$OMENCHATD_ALPHA_HOME/reticulum/config
```

Useful scriptable server settings:

```bash
"$OMENCHATD_BIN" config set \
  --home "$OMENCHATD_ALPHA_HOME" \
  --name "Alpha OMENchat" \
  --operator-label "alpha-admin" \
  --announce-interval 360 \
  --max-message-bytes 2048 \
  --history-batch-size 50 \
  --join-backlog-events 50 \
  --large-batch-threshold-bytes 4096 \
  --rate-messages-per-minute 20 \
  --rate-commands-per-minute 12
```

Add a room from CLI:

```bash
"$OMENCHATD_BIN" rooms add "#help" \
  --home "$OMENCHATD_ALPHA_HOME" \
  --topic "Ask OMEN related questions"
```

Run the server:

```bash
"$OMENCHATD_BIN" run --home "$OMENCHATD_ALPHA_HOME"
```

Run the admin TUI:

```bash
"$OMENCHATD_BIN" tui --home "$OMENCHATD_ALPHA_HOME"
```

TUI live-server smoke:

1. If the gateway was not configured before launch, open Interfaces or press
   `w` and enter `<gateway-host:port>`.
2. Press `g` to start the live server from the TUI.
3. Open the Monitoring panel with `c`.
4. Confirm the configured interface appears and traffic counters remain
   human-readable.
5. Open Logs with `l`.
6. Confirm startup, announce, and interface messages appear without needing to
   leave the TUI.
7. Join with one or more OMENbrowser_rs clients and watch the active-link rows.
   Short `high frames`, `high history`, `high ping`, or `high upload` flags are
   acceptable during join/reconnect/history/upload bursts; persistent flags
   while idle should be captured in a report.
8. Use Help if needed; `q` quits the TUI.

Pass criteria:

- `status` shows the server identity, OMENchat destination, copyable
  `omenchat://` client URI, NomadNet portal destination, and copyable portal URL.
- The configured interface is visible and connected when a gateway is available.
- The server home contains `config.toml`, `identity`, `omenchat.sqlite`, and
  `reticulum/`; it does not create or depend on `~/.reticulum`,
  `~/.nomadnetwork`, or `~/.lxmd`.
- The operator-owned portal file is under
  `$OMENCHATD_ALPHA_HOME/reticulum/storage/pages/index.mu`.
- Logs show startup, announce, link open/close, room joins, moderation actions,
  and protocol errors in human-readable form.
- Monitoring shows human-readable totals/rates for traffic and command types.
- Monitoring active-link rows show readable rates and noisy-client flags for
  frame, history, ping, and upload activity.
- TUI Setup, Identity, Interfaces, Portal, Rooms, Moderation, Monitoring, Logs,
  Audit, and Help panels are usable with keyboard, and mouse clicks should
  select visible actions/rows.
- The Moderation panel can set a selected user directly to Standard, Trusted,
  Moderator, or Admin without cycling blindly through roles.
- The Rooms and Moderation panels show the role permission summary beside their
  action lists, including admin-only room creation/archive and moderator topic
  and moderation powers.

## OMENchat Client Smoke

Use the `client uri` printed by `omenchatd status` or startup logs.

Local isolated smoke from the repository or an unpacked alpha bundle:

```bash
bash scripts/alpha-omenchat-smoke.sh
```

That helper starts an isolated local `omenchatd`, runs the browser
`--omenchat-smoke` command against it, and writes the JSON report plus server
logs under `/tmp/omenbrowser-rs-omenchat-smoke` by default. It sets the isolated
server announce interval to one minute and waits longer than the manual smoke
because a fresh local TCP client can miss the server's startup announce before
the TCP connection exists.

Use the stronger two-client form before handing a package to testers:

```bash
bash scripts/alpha-omenchat-smoke.sh --multi-client
```

That creates a second isolated browser root, runs a second OMENchat smoke
against the same temporary server, and fails if the second client does not see
the first client's room message in recent history.

Headless smoke:

```bash
./target/release/omenbrowser_rs \
  --omenchat-smoke <omenchat-server-destination-hash> \
  --app-root "$OMENBROWSER_ALPHA_ROOT" \
  --tcp-client <host:port> \
  --path-wait 15 \
  --stdout
```

Desktop smoke:

1. Start OMENbrowser_rs desktop.
2. Enter `omenchat://<omenchat-server-destination-hash>` in the OMENchat opener.
3. Click Open.
4. Send a message in `#lobby`.
5. Start a second isolated browser instance with `OMENBROWSER_ALPHA_ROOT_2`.
6. Open the same `omenchat://` destination from the second instance.
7. Confirm both clients see each other and messages flow both directions.
8. Restart omenchatd.
9. Confirm the client shows disconnect/reconnect state clearly.
10. Click Reconnect and confirm the session resumes.

Pass criteria:

- Room list loads.
- User list shows one entry per connected identity, not stale duplicates.
- Messages sent by the same user are grouped.
- Recent room history syncs after join, reconnect, server restart, and
  OMENbrowser_rs restart, respecting the server join backlog limit.
- `Load Older` fetches older rows beyond the current local floor and does not
  duplicate existing events.
- Room switching preserves room-scoped history.
- `/me`, `/topic`, `/create-room`, moderation commands, and permission errors
  behave according to the user's role.
- If an inactive room receives activity, the room button and hidden pane restore
  tab show unread state until selected/restored.
- Red-X destructive close disconnects the live Link and does not restore that
  chat after restart.

## Desktop Monitoring Checklist

Open OMENbrowser_rs Monitoring while performing live actions:

- Browse a NomadNet page and click several links. `Runtime Attribution` should
  show browser/page activity, with path activity only when path discovery is
  needed.
- Send a direct LXMF message and run propagation sync if configured. LXMF and
  propagation counters should move without unexplained browser/path spikes.
- Join OMENchat, switch rooms, send messages, and upload an image under
  512 KiB. The OMENchat Monitoring card should show live links, heartbeat RTT,
  history sync state, upload counters, pending resources, and media cache
  counts.
- With Tor Browser or another SOCKS proxy active on `127.0.0.1:9050` or
  `127.0.0.1:9150`, post a clearweb image URL. Trusted OMENchat servers may
  preview it automatically when remote media is enabled; untrusted servers
  should require explicit Load and still use SOCKS/Tor rather than direct TCP.
- Leave clients idle for several minutes. OMENchat heartbeat and server TUI
  flags should remain low-noise.
- Restart `omenchatd` and confirm reconnect state is visible without repeated
  flapping once the session is live again.

Capture a report if counters keep increasing while the app appears idle, or if
server/client Monitoring disagree about whether a chat link is live.

## Delete/Restore Regression Checklist

Before sending a build to another person, test these exact destructive flows:

- Delete an LXMF conversation, restart, confirm it stays gone.
- Delete an OMENchat pane with the destructive close button, restart, confirm it
  stays gone.
- Close a pane non-destructively, restart, confirm restore tabs behave as
  expected.
- Delete a browser tab with the destructive close button, restart, confirm it
  stays gone.
- Hide and restore panes from the top restore-tab rows.
- Minimize an OMENchat pane, send a message from another client, confirm the
  restore tab highlights, then restore it and confirm the unread marker clears.

## Stop-Test Blockers

Stop the current test and collect a redacted bundle if any of these happen:

- an identity is overwritten, missing, or unexpectedly swapped;
- `omenchatd` writes outside its configured server home into `~/.reticulum`,
  `~/.nomadnetwork`, or `~/.lxmd`;
- deleted browser, LXMF, or OMENchat panes return after restart;
- OMENchat reconnect flaps indefinitely while the server is reachable;
- two OMENchat clients remain out of sync after reconnect/restart and recent
  history sync;
- an upload under 512 KiB does not render inline for another client;
- clearweb image previews bypass the expected Tor/SOCKS privacy path;
- NomadNet link clicks remain pending forever instead of loading, failing, or
  offering a useful retry.

In the report, include the exact launch command, app root/server home, whether
the issue survives restart, what Monitoring showed, and whether the same
destination works in Python NomadNet/OMENbrowser if that comparison is
available.

## Issue Report Bundle

For desktop/server failures, collect a redacted local bundle first:

```bash
bash scripts/alpha-collect.sh \
  --browser-root "$OMENBROWSER_ALPHA_ROOT" \
  --browser-root-2 "$OMENBROWSER_ALPHA_ROOT_2" \
  --server-home "$OMENCHATD_ALPHA_HOME"
```

When running from an unpacked alpha archive, use the bundled script path:

```bash
bash ./scripts/alpha-collect.sh \
  --browser-root "$OMENBROWSER_ALPHA_ROOT" \
  --browser-root-2 "$OMENBROWSER_ALPHA_ROOT_2" \
  --server-home "$OMENCHATD_ALPHA_HOME"
```

The collector writes a timestamped directory under
`/tmp/omenbrowser-rs-alpha-bundles` by default. It summarizes file layout and
tails text logs, but intentionally excludes identity files, message JSON,
SQLite databases, known-destination caches, and Reticulum storage blobs. It also
captures package metadata/checksum summaries, binary version output from an
unpacked alpha archive, root-sanity output, optional second-browser-root
summaries, `omenchatd status/doctor` output for the provided server home, and
the selected `omenchatd` systemd user-service status/unit file when present.
Testers should still skim bundles before sharing them publicly because logs can
contain destination hashes, hostnames, interface names, and user-entered labels.

For browser/LXMF failures, prefer a diagnostic bundle:

```bash
./target/release/omenbrowser_rs \
  --native-validate <destination-hash>:/page/index.mu \
  --app-root "$OMENBROWSER_ALPHA_ROOT" \
  --tcp-client <host:port> \
  --path-wait 15 \
  --stdout \
  --bundle-report /tmp/omenbrowser-rs-alpha-bundles \
  > /tmp/omenbrowser-rs-alpha-report.json \
  2> /tmp/omenbrowser-rs-alpha-report.summary.txt
```

Do not paste private identity material, private keys, full private config files,
or message bodies into public issue reports.

Useful tester report fields:

- OS/distribution and desktop session type.
- Exact command used.
- Whether it was release or debug build.
- Runtime backend status line.
- Destination hash and page path.
- Whether path request passed.
- Whether the same action works in Python NomadNet/OMENbrowser.
- Any bundle path or redacted JSON report.

## Alpha Exit Criteria

The build is ready for a small outside group when:

- Release build starts consistently.
- Browser page restore works after restart with at least two tiled browser panes.
- Direct LXMF send and receive works against a Python/NomadNet peer.
- Propagated LXMF send has a visible local row and sane propagation-node status.
- omenchatd can run from its own home directory with no Python dependency.
- OMENchat plugin connects to omenchatd over a real configured interface.
- Restart/delete/restore behavior is stable for browser, LXMF conversations, and
  OMENchat panes.
- OMENchat recent history stays synchronized between two clients after client
  restart, server restart, room switching, reconnect, and `Load Older`.
- Logs are useful but not flooded by routine success messages.
