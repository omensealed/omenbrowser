# OMENbrowser_rs Public Release Tester Sheet

This build is for public release testing of OMENbrowser_rs, the built-in
OMENchat plugin client, and the standalone `omenchatd` server.

Use release binaries for UI testing. Debug builds are not a useful performance
baseline.

Maintainers collecting repeatable Linux idle measurements can run
`scripts/measure-desktop-idle.sh` from a source checkout. It uses a temporary
isolated app root and records raw evidence under the requested results directory;
set `HEADLESS=1` for a disposable Xvfb/i3 session, and see `docs/TESTING.md` for
the measurement and before/after comparison contract.
Maintainers can also run `scripts/measure-pane-stress.sh` to restore and
close/reopen a deterministic isolated 50-pane workspace across three native
Linux cycles; this never uses the normal identity or message roots.
The standalone-server queue gate is
`scripts/measure-omenchatd-backpressure.sh`; it runs for 60 seconds by default,
uses only generated temporary state, and retains RSS/queue/control-latency
evidence in the requested output directory.
The standalone database-worker gate is
`scripts/measure-omenchatd-db.sh`; it also defaults to 60 seconds, uses only a
generated temporary root, and retains worker/heartbeat latency, RSS, file
descriptor, restart, event-ID, and SQLite-integrity evidence. Neither harness
is a substitute for the live Reticulum interoperability smoke.
Media-performance testers can run `scripts/measure-omenchat-media.sh` from an
interactive graphical session. It uses an isolated temporary identity root and
guides visible/hidden/closed animation phases; do not substitute normal user
data or report missing GPU tooling as zero activity.

For release testing, start with `--app-root /tmp/omenbrowser-rs-test` or another
dedicated root. Launch the default profile only when you intentionally want to
use your normal OMENbrowser_rs identity, storage, messages, and pane layout.

## Start With A Local Smoke

After unpacking the archive, run:

```bash
./bin/omenbrowser_rs --version
./bin/omenchatd --version
bash ./scripts/release-omenchat-smoke.sh
```

Expected result:

```text
outcome: pass
reason: OMENchat Link opened, room joined, and message echo was observed
```

If this fails, collect a report bundle before live network testing.

For a stronger local check that uses two isolated browser roots against the
same temporary server and verifies the second client receives the first
client's recent room history, run:

```bash
bash ./scripts/release-omenchat-smoke.sh --multi-client
```

Developers validating from a source checkout can run the package gate against
the latest archive:

```bash
bash scripts/release-check.sh package /tmp/omenbrowser-rs-dist/OMENbrowser_rs-latest.tar.gz
```

That developer gate is not required for normal tester use.

## Isolated Browser Roots

Use a clean app root for testing:

```bash
./bin/omenbrowser_rs --desktop --app-root /tmp/omenbrowser-rs-test
```

For a second client on the same machine:

```bash
./bin/omenbrowser_rs --desktop --app-root /tmp/omenbrowser-rs-test-2
```

Before multi-client tests, run the root sanity helper:

```bash
bash ./scripts/release-root-sanity.sh \
  --browser-root /tmp/omenbrowser-rs-test \
  --browser-root-2 /tmp/omenbrowser-rs-test-2 \
  --server-home /tmp/omenchatd-test
```

Expected result:

```text
root sanity: pass
```

Do not run two clients against the same app root unless you intentionally want
them to share identity, Reticulum config, messages, plugin cache, and pane
layout.

## Start An OMENchat Server

Initialize a standalone server root:

```bash
./bin/omenchatd init --home /tmp/omenchatd-test
```

Attach it to a TCP gateway:

```bash
./bin/omenchatd interfaces tcp-client <gateway-host:port> --home /tmp/omenchatd-test
```

If your gateway uses IFAC/network credentials:

```bash
printf '%s\n' 'your passphrase' > /tmp/omenchatd-ifac-passphrase
chmod 600 /tmp/omenchatd-ifac-passphrase
./bin/omenchatd interfaces tcp-client <gateway-host:port> \
  --home /tmp/omenchatd-test \
  --network-name <network-name> \
  --passphrase-file /tmp/omenchatd-ifac-passphrase
```

Start the admin TUI:

```bash
./bin/omenchatd tui --home /tmp/omenchatd-test
```

In the TUI:

- `g` starts the live server.
- Use **Announce Now** in Setup, Overview, Interfaces, or Portal to announce
  immediately after the live server is running.
- `w` writes a Connect To Gateway config.
- `c` opens Monitoring.
- `l` opens Logs.
- `q` quits.

In Monitoring, watch the active-link rows while clients join, send messages,
switch rooms, upload files, and reconnect. The TUI prints rate flags beside
each client:

- `high frames` means the client is sending many frames quickly.
- `high history` usually means repeated recent-history or Load Older requests.
- `high ping` means heartbeat traffic is unusually frequent.
- `high upload` means repeated upload offers/fetches.

Short bursts are normal during join, reconnect, and upload fetches. Persistent
flags while the client is idle should be reported.

Show the server addresses:

```bash
./bin/omenchatd status --home /tmp/omenchatd-test
```

Check server readiness:

```bash
./bin/omenchatd doctor --home /tmp/omenchatd-test
```

Use `client uri: omenchat://...` in the OMENbrowser_rs New Chat opener. Use
`portal url: ...:/page/index.mu` in the NomadNet browser.

Optional user service install after gateway setup:

```bash
bash ./scripts/install-omenchatd-user-service.sh \
  --bin "$PWD/bin/omenchatd" \
  --home /tmp/omenchatd-test
```

The installer writes a systemd user unit and prints `systemctl --user` commands
for status/start/stop/logs.

To remove only the user service while preserving server data:

```bash
bash ./scripts/install-omenchatd-user-service.sh --uninstall
```

Optional desktop launcher install:

```bash
bash ./scripts/install-omenbrowser-user-launchers.sh \
  --bin "$PWD/bin/omenbrowser_rs"
```

That installs a user-level launcher for the isolated release browser root. Add
`--second-client` for a second isolated-client launcher, or
`--default-profile` only if you intentionally want a launcher for your normal
OMENbrowser_rs profile. To remove launchers while preserving app data:

```bash
bash ./scripts/install-omenbrowser-user-launchers.sh --uninstall
```

For the normal desktop integration path, you can use the combined wrapper
instead. By default it installs only the isolated release browser launcher:

```bash
bash ./scripts/install-release.sh
```

Add `--second-client-launcher` for the second isolated browser launcher. Add
`--server-service` only after the server gateway/config is ready. The wrapper's
`--uninstall` removes launcher/service files only and preserves all app data.

## Things To Test

- Open at least one NomadNet page and follow several links.
- Restart the browser and confirm saved browser panes reload after paths are
  available.
- Open Monitoring in OMENbrowser_rs and watch `Runtime Attribution` while
  browsing. Browser spikes should line up with page/download activity, path
  spikes should line up with path discovery, and LXMF spikes should line up
  with message/propagation work.
- Send and receive a direct LXMF message.
- Send a propagated LXMF message if you have a reachable propagation node.
- Open two OMENchat clients with separate browser roots and join the same
  server.
- Restart `omenchatd` and confirm OMENchat clients reconnect cleanly.
- Switch OMENchat rooms and confirm recent room history syncs.
- Upload an image under 512 KiB and confirm it appears inline for another
  client.
- Try an upload over 512 KiB and confirm it is rejected before transfer.
- Post a clearweb image URL with Tor Browser running. Trusted OMENchat servers
  may preview through SOCKS/Tor automatically when remote media is enabled;
  untrusted servers should require explicit Load and still use SOCKS/Tor rather
  than direct TCP.
- Click a non-image HTTP/HTTPS link and confirm the external-link prompt offers
  Copy URL for Tor Browser instead of trying to launch a locked Tor profile.
- Delete an LXMF conversation and confirm it does not return after restart.

## Stop And Report These First

These are release blockers. If one happens, stop the test and collect a redacted
report bundle before trying many workarounds:

- A browser, OMENchat, or LXMF conversation pane returns after deletion and
  restart.
- A saved identity is overwritten, disappears, or switches unexpectedly.
- `omenchatd` writes to `~/.reticulum`, `~/.nomadnetwork`, or `~/.lxmd` without
  you explicitly configuring that path.
- OMENchat clients stay disconnected or repeatedly reconnect after the server
  is clearly online.
- OMENchat room history differs between two clients after reconnect/restart and
  does not repair within one recent-history sync.
- Uploads under 512 KiB do not appear inline on another client.
- Clearweb images load directly when Tor/SOCKS privacy was expected.
- NomadNet link clicks stay stuck as pending and never fail, retry, or load.

Useful notes to include with a report:

- which binary command you ran;
- which app root or server home was used;
- whether the same issue happens after restart;
- whether Monitoring showed browser/path, LXMF, OMENchat, or upload activity;
- whether the same destination works in another NomadNet client.

## Tested Release Paths

Current tested paths include NomadNet browsing, multiple tiled panes, direct
LXMF conversations with attachments, OMENchat reconnect/restart recovery, room
history sync, inline images/GIFs, 512 KiB per-file upload rejection, and the
external browser prompt for HTTP/HTTPS links.

## Known Release Gaps

- Native LXMF ticketed sends now include reply tickets, inbound reply tickets
  are captured from received messages, valid remembered reply tickets are reused
  for outbound direct ticket stamps, and propagation stamps are generated when
  the propagation node advertises a target cost. Direct peer stamp-cost
  negotiation without a remembered ticket is still active follow-up work.
- `.deb` and AppImage release artifacts are available. The compatible `.deb`
  has been tested on Linux Mint 21.3; broader Debian/Ubuntu derivative testing
  is still useful.
- Some Reticulum behavior depends on what the current Rust RNS/LXMF crates
  expose.

## Collect A Redacted Report

For failures:

```bash
bash ./scripts/release-collect.sh \
  --browser-root /tmp/omenbrowser-rs-test \
  --browser-root-2 /tmp/omenbrowser-rs-test-2 \
  --server-home /tmp/omenchatd-test
```

The collector excludes identity files, message databases, known-destination
caches, and Reticulum storage blobs. It also includes package metadata, binary
version output, root-sanity output, second-browser-root summaries when provided,
`omenchatd status/doctor` output for the provided server home, and optional
`omenchatd` user-service status. Review the bundle before sharing it.

The command prints the bundle directory path. Attach that directory, or archive
it yourself after reviewing the contents.
