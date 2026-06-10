# OMENbrowser_rs Private Alpha Tester Sheet

This build is for private alpha testing of OMENbrowser_rs, the built-in
OMENchat plugin client, and the standalone `omenchatd` server.

Use release binaries for UI testing. Debug builds are not a useful performance
baseline.

For alpha testing, start with `--app-root /tmp/omenbrowser-rs-alpha` or another
dedicated root. Launch the default profile only when you intentionally want to
use your normal OMENbrowser_rs identity, storage, messages, and pane layout.

## Start With A Local Smoke

After unpacking the archive, run:

```bash
./bin/omenbrowser_rs --version
./bin/omenchatd --version
bash ./scripts/alpha-omenchat-smoke.sh
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
bash ./scripts/alpha-omenchat-smoke.sh --multi-client
```

Developers validating from a source checkout can run the package gate against
the latest archive:

```bash
bash scripts/alpha-check.sh package /tmp/omenbrowser-rs-alpha-dist/OMENbrowser_rs-alpha-latest.tar.gz
```

That developer gate is not required for normal tester use.

## Isolated Browser Roots

Use a clean app root for testing:

```bash
./bin/omenbrowser_rs --desktop --app-root /tmp/omenbrowser-rs-alpha
```

For a second client on the same machine:

```bash
./bin/omenbrowser_rs --desktop --app-root /tmp/omenbrowser-rs-alpha-2
```

Before multi-client tests, run the root sanity helper:

```bash
bash ./scripts/alpha-root-sanity.sh \
  --browser-root /tmp/omenbrowser-rs-alpha \
  --browser-root-2 /tmp/omenbrowser-rs-alpha-2 \
  --server-home /tmp/omenchatd-alpha
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
./bin/omenchatd init --home /tmp/omenchatd-alpha
```

Attach it to a TCP gateway:

```bash
./bin/omenchatd interfaces tcp-client <gateway-host:port> --home /tmp/omenchatd-alpha
```

If your gateway uses IFAC/network credentials:

```bash
./bin/omenchatd interfaces tcp-client <gateway-host:port> \
  --home /tmp/omenchatd-alpha \
  --network-name <network-name> \
  --passphrase <passphrase>
```

Start the admin TUI:

```bash
./bin/omenchatd tui --home /tmp/omenchatd-alpha
```

In the TUI:

- `g` starts the live server.
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
./bin/omenchatd status --home /tmp/omenchatd-alpha
```

Check server readiness:

```bash
./bin/omenchatd doctor --home /tmp/omenchatd-alpha
```

Use `client uri: omenchat://...` in the OMENbrowser_rs New Chat opener. Use
`portal url: ...:/page/index.mu` in the NomadNet browser.

Optional user service install after gateway setup:

```bash
bash ./scripts/install-omenchatd-user-service.sh \
  --bin "$PWD/bin/omenchatd" \
  --home /tmp/omenchatd-alpha
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

That installs a user-level launcher for the isolated alpha browser root. Add
`--second-client` for a second isolated-client launcher, or
`--default-profile` only if you intentionally want a launcher for your normal
OMENbrowser_rs profile. To remove launchers while preserving app data:

```bash
bash ./scripts/install-omenbrowser-user-launchers.sh --uninstall
```

For the normal alpha desktop integration path, you can use the combined wrapper
instead. By default it installs only the isolated alpha browser launcher:

```bash
bash ./scripts/install-alpha.sh
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
- Delete an LXMF conversation and confirm it does not return after restart.

## Stop And Report These First

These are alpha blockers. If one happens, stop the test and collect a redacted
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

## Known Alpha Gaps

- Native LXMF ticket/stamp sending is not finished.
- OMENchat rich media/uploads are usable for alpha testing: inline images,
  animated GIFs, Tor/SOCKS clearweb fetches, a 512 KiB per-file upload cap, and
  rotating server quota exist. Sizing/progress polish may still change.
- Distro-native installers are not available yet.
- Some Reticulum behavior depends on what the current Rust RNS/LXMF crates
  expose.

## Collect A Redacted Report

For failures:

```bash
bash ./scripts/alpha-collect.sh \
  --browser-root /tmp/omenbrowser-rs-alpha \
  --browser-root-2 /tmp/omenbrowser-rs-alpha-2 \
  --server-home /tmp/omenchatd-alpha
```

The collector excludes identity files, message databases, known-destination
caches, and Reticulum storage blobs. It also includes package metadata, binary
version output, root-sanity output, second-browser-root summaries when provided,
`omenchatd status/doctor` output for the provided server home, and optional
`omenchatd` user-service status. Review the bundle before sharing it.

The command prints the bundle directory path. Attach that directory, or archive
it yourself after reviewing the contents.
