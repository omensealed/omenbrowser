# OMENbrowser_rs

Rust desktop/TUI port of OMENbrowser for Reticulum, NomadNet, LXMF, Micron,
MicronPlus, and OMENchat.

Repository: <https://github.com/omensealed/omenbrowser>

This repository is still in private alpha. The goal is not a line-by-line Python
translation; it is a native Rust browser/client with stronger pane management,
isolated identities, native Reticulum/LXMF integration, and a built-in OMENchat
client.

## Current Pieces

- `omenbrowser_rs`: desktop OMENbrowser client with NomadNet browsing, LXMF
  messaging, Directory, identities, interfaces, diagnostics, monitoring, logs,
  Micron/MicronPlus rendering, and the OMENchat plugin client.
- `src/server`: standalone `omenchatd` server crate. It is intentionally
  independent from the browser and owns its own storage root.
- `TESTERS.md`: concise private-alpha tester sheet included at the root of the
  packaged archive.
- `docs/27-alpha-test-runbook.md`: practical alpha tester path.

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

## Build

Browser with desktop UI, native network, and OMENchat client:

```bash
cargo build --release --features chat-client-rns
```

Standalone OMENchat server:

```bash
cargo build --release --manifest-path src/server/Cargo.toml --features live-rns-net
```

Private alpha bundle with both binaries and starter docs:

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
[docs/27-alpha-test-runbook.md](docs/27-alpha-test-runbook.md) before giving the
build to another tester. It covers isolated roots, live NomadNet smoke, LXMF
smoke, OMENchat server setup, two-client OMENchat testing, delete/restore
regressions, and issue report bundles.

Use [docs/28-alpha-handoff.md](docs/28-alpha-handoff.md) as the short private
alpha handoff note for what is ready, first tester flow, known alpha risks, and
report bundle instructions.

## Known Alpha Gaps

- Native LXMF ticket/stamp sending is not finished.
- OMENchat rich media/uploads are usable for alpha testing, including inline
  images/GIFs, Tor/SOCKS-gated clearweb image fetches, a 512 KiB per-file cap,
  and server-side rotating quota. Expect UI/progress polish to continue.
- `.deb` and AppImage packaging helpers exist, but need broader distro testing.
- Some Reticulum behavior depends on what the Rust RNS/LXMF crates currently
  expose.
