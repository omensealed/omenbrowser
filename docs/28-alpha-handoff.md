# 28 - Alpha Handoff

This is the short handoff note for a small private alpha of OMENbrowser_rs,
the built-in OMENchat plugin client, and the standalone `omenchatd` server.
Use it alongside `docs/27-alpha-test-runbook.md`.

## Current Verified Package

The current alpha archive is the `.tar.gz` file this document was packaged
with, or the newest archive produced by:

```bash
bash scripts/alpha-package.sh
```

The package helper also refreshes a stable handoff copy in the output
directory:

```text
OMENbrowser_rs-alpha-latest.tar.gz
OMENbrowser_rs-alpha-latest.tar.gz.sha256
OMENbrowser_rs-alpha-latest.txt
```

Before this package was handed off, the current archive was verified with the
package gate:

- `bash scripts/alpha-check.sh package`
- packaged archive extraction
- staged and extracted binary `--help` checks
- isolated `omenchatd init/status`
- bundled redacted collector smoke
- bundled local OMENchat server/client smoke from the unpacked archive
- bundled two-client OMENchat smoke from the unpacked archive during the
  package gate

The full local gate remains the recommended developer preflight before broader
distribution:

```bash
bash scripts/alpha-check.sh full
```

The local smoke helper writes its report under:

```text
/tmp/omenbrowser-rs-omenchat-smoke/
```

Expected local smoke result:

```text
outcome: pass
reason: OMENchat Link opened, room joined, and message echo was observed
```

## What Is Ready For Private Alpha

- Desktop OMENbrowser_rs release binary with native RNS/LXMF support.
- Managed browser identities and isolated `--app-root` testing.
- NomadNet browsing through native Rust RNS crates.
- Micron/MicronPlus rendering and browser pane restore.
- LXMF direct and propagated messaging at alpha quality.
- OMENchat plugin client with multiple server panes, reconnect, room history
  sync, room switching, unread restore tabs, and grouped messages.
- Standalone `omenchatd` with its own identity, database, logs, Reticulum config,
  NomadNet portal page, rooms, moderation, monitoring, and admin TUI.
- `omenchatd` TCP client gateway setup via CLI or the TUI Interfaces panel:

```bash
./bin/omenchatd interfaces tcp-client <gateway-host:port> --home /tmp/omenchatd-alpha
```

For IFAC-protected gateways:

```bash
./bin/omenchatd interfaces tcp-client <gateway-host:port> \
  --home /tmp/omenchatd-alpha \
  --network-name <network-name> \
  --passphrase <passphrase>
```

## First Tester Flow

1. Unpack the archive.
2. Read `TESTERS.md`.
3. Run the local package smoke:

```bash
./bin/omenbrowser_rs --version
./bin/omenchatd --version
bash ./scripts/alpha-omenchat-smoke.sh
```

For the same local smoke with a second isolated browser root and recent-history
verification:

```bash
bash ./scripts/alpha-omenchat-smoke.sh --multi-client
```

4. Start an isolated browser root:

```bash
./bin/omenbrowser_rs --desktop --app-root /tmp/omenbrowser-rs-alpha
```

5. Start an isolated `omenchatd` home:

```bash
./bin/omenchatd init --home /tmp/omenchatd-alpha
./bin/omenchatd interfaces tcp-client <gateway-host:port> --home /tmp/omenchatd-alpha
./bin/omenchatd doctor --home /tmp/omenchatd-alpha
./bin/omenchatd tui --home /tmp/omenchatd-alpha
```

6. In the TUI, use Interfaces or press `w` if gateway setup was not already
   done, press `g` to start the server, then check Monitoring and Logs.
7. Copy `client uri: omenchat://...` from `omenchatd status` or startup logs.
8. Open that URI in OMENbrowser_rs New Chat.
9. Test a second browser client with a different root:

```bash
./bin/omenbrowser_rs --desktop --app-root /tmp/omenbrowser-rs-alpha-2
```

Optional user-service install after gateway setup:

```bash
bash ./scripts/install-omenchatd-user-service.sh \
  --bin "$PWD/bin/omenchatd" \
  --home /tmp/omenchatd-alpha
```

To remove only the user service while keeping server data:

```bash
bash ./scripts/install-omenchatd-user-service.sh --uninstall
```

Optional user-level desktop launcher install:

```bash
bash ./scripts/install-omenbrowser-user-launchers.sh \
  --bin "$PWD/bin/omenbrowser_rs"
```

Add `--second-client` if the tester wants a second isolated-client launcher.
Use `--default-profile` only when intentionally installing a launcher for the
normal OMENbrowser_rs profile. Uninstalling the launchers preserves app data:

```bash
bash ./scripts/install-omenbrowser-user-launchers.sh --uninstall
```

The combined alpha installer wraps the browser launcher and optional server
service installers. Its default action installs only the isolated alpha browser
launcher:

```bash
bash ./scripts/install-alpha.sh
```

Add `--second-client-launcher` for a second isolated browser launcher. Add
`--server-service` only after the server gateway/config is ready. Its
`--uninstall` removes launcher/service files only and preserves app data.

## Known Alpha Risks

- Native LXMF ticket/stamp send is not finished.
- Some Reticulum interface types depend on what the current Rust crates expose.
- Release builds should be used for UI testing; debug builds are not a useful
  performance baseline.
- Testers should use disposable identities until they understand the storage
  model.
- OMENchat rich media/uploads are usable for alpha testing: inline images,
  animated GIFs, Tor/SOCKS clearweb image fetches, a 512 KiB per-file upload
  cap, and rotating server quota exist. Presentation/progress polish may still
  change.
- Distro-native installers are not done yet.

## Report Bundle

For failures, collect a redacted bundle:

```bash
bash ./scripts/alpha-collect.sh \
  --browser-root /tmp/omenbrowser-rs-alpha \
  --browser-root-2 /tmp/omenbrowser-rs-alpha-2 \
  --server-home /tmp/omenchatd-alpha
```

The collector intentionally excludes identity material, message databases, and
Reticulum storage blobs. It also includes package metadata, binary version
output, root-sanity output, optional second-browser-root summaries,
`omenchatd status/doctor` output for the provided server home, and optional
`omenchatd` user-service status. Testers should still review bundles before
sharing. The command prints the bundle directory path; attach that directory,
or archive it after review.
