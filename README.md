# OMENbrowser_rs

Rust desktop/TUI browser and messaging client for Reticulum, NomadNet, LXMF,
Micron, MicronPlus, and OMENchat. LXST voice is unsupported, for now.

Repository: <https://github.com/omensealed/omenbrowser>

This repository is in public alpha. It is a native Rust browser/client
with pane management, isolated identities, native Reticulum/LXMF integration,
and a built-in OMENchat client.

Project was "vibe coded" not only as a test for my own curiosity with vibe coding consoles
but if it could truly be guided to relative "quality software" in the hands of somebody
who's been doing it for quite some time and push projects that normally take a long time
to personally develop and thought this relatively "new" landscape on RNS and software dev
something like this is where I'd begin testing the capability of codex.

Rust was chosen for a bit of "guardrail" for the AI to chew on while developing due to the
nature of the Rust compiler itself and what Rust offers to keep things "better". 
The project will expand when Rust crates for RNS expand. I have no interest in the "internals" of this and intend to only stay within "desktop browser" area. I'll eventually pass some docs to the Columba dev and he can or decide not to implement a OMENchat client, which will be available in this repo when I get to the "how to" part of that. 

## Screenshots

Click a thumbnail to open the full-size workspace screenshot.

<p>
  <a href="docs/assets/screenshots/workspace-purple.png">
    <img src="docs/assets/screenshots/workspace-purple-thumb.png" alt="OMENbrowser workspace with NomadNet, OMENchat, and LXMF panes in purple theme" width="31%">
  </a>
  <a href="docs/assets/screenshots/workspace-red.png">
    <img src="docs/assets/screenshots/workspace-red-thumb.png" alt="OMENbrowser workspace with NomadNet, OMENchat, and LXMF panes in red theme" width="31%">
  </a>
  <a href="docs/assets/screenshots/workspace-teal.png">
    <img src="docs/assets/screenshots/workspace-teal-thumb.png" alt="OMENbrowser workspace with NomadNet, OMENchat, and LXMF panes in teal theme" width="31%">
  </a>
</p>

## Current Pieces

- `omenbrowser_rs`: desktop OMENbrowser client with NomadNet browsing, LXMF
  messaging, Directory, identities, interfaces, diagnostics, monitoring, logs,
  Micron/MicronPlus rendering, and the OMENchat plugin client.
- `src/server`: standalone `omenchatd` server crate. It is intentionally
  independent from the browser and owns its own storage root.
- `TESTERS.md`: concise public-alpha tester sheet included at the root of the
  packaged archive.
- `docs/TESTING.md`: practical alpha tester path.

## Identity And Storage Safety

Default browser data lives under:

```text
~/.config/OMENbrowser_rs/
```

Default server data lives under:

```text
~/.omenchatd/
```

For parallel testing, use separate browser roots:

```bash
./target/release/omenbrowser_rs --desktop --app-root /tmp/omenbrowser-rs-alpha
./target/release/omenbrowser_rs --desktop --app-root /tmp/omenbrowser-rs-alpha-2
```

Check root isolation before a two-client run:

```bash
bash scripts/alpha-root-sanity.sh \
  --browser-root /tmp/omenbrowser-rs-alpha \
  --browser-root-2 /tmp/omenbrowser-rs-alpha-2 \
  --server-home /tmp/omenchatd-alpha
```

Do not point test clients at the same app root unless you intentionally want them
to share identity, Reticulum config, messages, plugin SQLite data, and pane
layout.

`omenchatd` should not touch `~/.reticulum`, `~/.nomadnetwork`, or `~/.lxmd` by
default. It keeps identity material, Reticulum config/storage, SQLite data, logs,
and NomadNet portal pages under its own home.

## Install From Release

For normal testing, use the packaged release instead of building from source:

<https://github.com/omensealed/omenbrowser/releases/latest>

The current release publishes:

- `omenbrowser-rs_0.1.0_amd64.deb` for Debian-family systems.
- `OMENbrowser_rs-0.1.0-x86_64.AppImage` for general Linux testing.
- `OMENbrowser_rs-alpha-latest.tar.gz` for testers who prefer unpacked
  binaries and helper scripts.
- Matching `.sha256` checksum files.

On Debian, Ubuntu, Linux Mint, and related distributions, the `.deb` is the
preferred install path:

```bash
sudo apt install ./omenbrowser-rs_0.1.0_amd64.deb
omenbrowser_rs --desktop
```

The release `.deb` is built in a Debian 11 compatible container with an older
glibc floor, so it is intended to run on Debian 11, 12, 13+, and distributions
based on them. Very old or heavily customized systems may still need missing
desktop/graphics libraries installed by the distro package manager.

If package installation is not a good fit, make the AppImage executable and run
it directly:

```bash
chmod +x OMENbrowser_rs-0.1.0-x86_64.AppImage
./OMENbrowser_rs-0.1.0-x86_64.AppImage --desktop
```

## First Run Network Setup

To start seeing real Directory entries, NomadNet pages, LXMF peers, propagation
nodes, and OMENchat servers:

1. Open `Interfaces`.
2. Add or enable the `WNS` and `RMAP` gateway presets.
3. Add any private gateway or RNode/LoRa interface you personally use.
4. Open `Identities` and give your identity a recognizable label.
5. Restart OMENbrowser_rs so the selected interfaces and identity load cleanly
   at startup.

For people following official OMEN development, `WNS` and `RMAP` are the
recommended public presets because OMEN test nodes and services are expected to
stay reachable there. If the preferred gateways change, the docs will be
updated.

See [Getting Online Fast](docs/GETTING_ONLINE.md) for the fuller first-run
path.

## Build

These commands are for developers working from a source checkout.

Browser with desktop UI, native network, and OMENchat client:

```bash
cargo build --release --features chat-client-rns
```

Standalone OMENchat server:

```bash
cargo build --release --manifest-path src/server/Cargo.toml --features live-rns-net
```

Public alpha bundle with both binaries and starter docs:

```bash
bash scripts/alpha-package.sh
```

The package helper also runs `--help` on the staged browser and server binaries
and writes checksums plus package metadata into the bundle. It also creates a
temporary isolated `omenchatd` home to verify `init` and `status`, then creates
a timestamped `.tar.gz` archive and matching `.sha256` file next to the staged
directory. The same archive is copied to `OMENbrowser_rs-alpha-latest.tar.gz`
with a matching checksum and `OMENbrowser_rs-alpha-latest.txt` manifest for
tester handoff.
The bundle includes `TESTERS.md`, `scripts/alpha-collect.sh` so testers can
create redacted issue bundles without cloning this repository, and
`scripts/alpha-omenchat-smoke.sh` for a local isolated server/client OMENchat
smoke before live network testing. It also includes
`scripts/install-omenchatd-user-service.sh` and a systemd user-service template
for testers who want `omenchatd` to run as a user service after manual gateway
setup.

Optional combined alpha installer for local launchers and, if requested, the
`omenchatd` user service:

```bash
bash scripts/install-alpha.sh
```

The installer preserves browser roots, identities, Reticulum storage, messages,
server homes, uploads, and databases when uninstalling.

Local distro package helpers:

```bash
bash scripts/package-deb.sh dist
bash scripts/package-appimage.sh dist
```

See [packaging/README.md](packaging/README.md) for package outputs and
requirements. The AppImage helper requires `appimagetool`.

If you are reading this from an unpacked alpha archive, use the packaged
binaries instead of source-tree build paths:

```bash
./bin/omenbrowser_rs --desktop --app-root /tmp/omenbrowser-rs-alpha
./bin/omenchatd init --home /tmp/omenchatd-alpha
./bin/omenchatd tui --home /tmp/omenchatd-alpha
```

Inside the package, `TESTERS.md` and `ALPHA-START.txt` are the fastest paths.
The `target/release/...` and `src/server/target/release/...` commands below are
for running from this source tree.

## Run The Browser

Normal desktop launch:

```bash
./target/release/omenbrowser_rs --desktop
```

Isolated alpha launch:

```bash
./target/release/omenbrowser_rs --version
./target/release/omenbrowser_rs --desktop --app-root /tmp/omenbrowser-rs-alpha
```

## Run omenchatd

Initialize an isolated server:

```bash
./src/server/target/release/omenchatd --version
./src/server/target/release/omenchatd init --home /tmp/omenchatd-alpha
```

Point the server at a backbone TCP gateway:

```bash
./src/server/target/release/omenchatd interfaces tcp-client <gateway-host:port> --home /tmp/omenchatd-alpha
```

If the gateway uses IFAC/network credentials, include them when writing the
isolated Reticulum config:

```bash
./src/server/target/release/omenchatd interfaces tcp-client <gateway-host:port> \
  --home /tmp/omenchatd-alpha \
  --network-name <network-name> \
  --passphrase <passphrase>
```

You can also set it during init/run:

```bash
./src/server/target/release/omenchatd init --home /tmp/omenchatd-alpha --tcp-client <gateway-host:port>
./src/server/target/release/omenchatd run --home /tmp/omenchatd-alpha --tcp-client <gateway-host:port>
```

Start the admin TUI:

```bash
./src/server/target/release/omenchatd tui --home /tmp/omenchatd-alpha
```

Inside the TUI, press `g` to start the live server, `c` for Monitoring, `l` for
Logs, and `q` to quit.

While running, `omenchatd` watches Reticulum interface health. If every
configured interface repeatedly reports disconnected after a gateway restart,
the server rebuilds its live runtime and announces again. Check Monitoring or
`~/.omenchatd/omenchatd.log` for `interface watchdog` lines.

Run the live server:

```bash
./src/server/target/release/omenchatd run --home /tmp/omenchatd-alpha
```

Install an optional systemd user service:

```bash
bash scripts/install-omenchatd-user-service.sh \
  --bin ./src/server/target/release/omenchatd \
  --home /tmp/omenchatd-alpha
```

Remove only the user service while preserving server data:

```bash
bash scripts/install-omenchatd-user-service.sh --uninstall
```

Show copyable connection targets:

```bash
./src/server/target/release/omenchatd status --home /tmp/omenchatd-alpha
```

Check server readiness before public hosting:

```bash
./src/server/target/release/omenchatd doctor --home /tmp/omenchatd-alpha
```

Look for:

```text
client uri: omenchat://<omenchat-destination-hash>
portal url: <nomadnet-portal-destination-hash>:/page/index.mu
```

Use the `client uri` in the OMENchat opener. Use the `portal url` in the
NomadNet browser to test the quiet server MOTD/rules/launch page.

## Test

Quick alpha readiness check:

```bash
bash scripts/alpha-check.sh quick
```

Full local alpha gate:

```bash
bash scripts/alpha-check.sh full
```

Validate the latest packaged alpha archive:

```bash
bash scripts/alpha-check.sh package
```

Redacted alpha issue bundle:

```bash
bash scripts/alpha-collect.sh \
  --browser-root /tmp/omenbrowser-rs-alpha \
  --browser-root-2 /tmp/omenbrowser-rs-alpha-2 \
  --server-home /tmp/omenchatd-alpha
```

The collector summarizes package metadata, binary version output, root-sanity
output, optional second-browser-root summaries, and optional `omenchatd`
user-service state while excluding identity material, message stores,
known-destination caches, and Reticulum storage blobs.

Browser:

```bash
cargo fmt --check
cargo test --features chat-client-rns
```

Server:

```bash
cargo fmt --manifest-path src/server/Cargo.toml --check
cargo test --manifest-path src/server/Cargo.toml --features live-rns-net
```

## Alpha Runbook

Use [TESTERS.md](TESTERS.md) as the first sheet for outside testers. Use
[docs/TESTING.md](docs/TESTING.md) before giving the build to another tester.
It covers isolated roots, local smoke tests, live NomadNet/LXMF checks,
OMENchat server setup, two-client OMENchat testing, and issue report bundles.

Use [docs/QUICKSTART.md](docs/QUICKSTART.md) for the shortest build/run path
and [docs/OMENCHAT.md](docs/OMENCHAT.md) for chat server/client setup.

## Tested Alpha Paths

Current tested paths include NomadNet browsing, multiple tiled panes, LXMF
conversations with attachments, OMENchat reconnect/restart recovery, room
history sync, inline image/GIF previews, upload rejection over the configured
per-file cap, and the external browser prompt for HTTP/HTTPS links.

## Known Alpha Gaps

- Native LXMF ticket/stamp sending is not finished.
- The compatible `.deb` has been tested on Linux Mint 21.3; broader
  Debian/Ubuntu derivative testing is still useful.
- Some Reticulum behavior depends on what the Rust RNS/LXMF crates currently
  expose.
