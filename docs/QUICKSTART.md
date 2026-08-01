# Quickstart

## Build

Browser with desktop UI, native networking, and OMENchat using the canonical
Reticulum/LXMF 0.9 path:

```bash
cargo build --release --locked --no-default-features --features desktop-product
```

`chat-client-reticulum` uses the `reticulum-rs`/`lxmf` 0.9 stack without
pulling in the old `rns-net` compatibility crates. `chat-client-rns-clean`
remains as a compatibility alias for older local commands.

Standalone OMENchat server:

```bash
cargo build --release --locked --manifest-path src/server/Cargo.toml \
  --no-default-features --features server-full
```

## Run The Browser

Normal profile:

```bash
./target/release/omenbrowser_rs --desktop
```

Isolated test profile:

```bash
./target/release/omenbrowser_rs --desktop --app-root /tmp/omenbrowser-rs-test
```

Use a different `--app-root` for every test client. Separate roots prevent
identity, Reticulum config, message, cache, and pane-layout collisions.

## Get Onto RNS

For a new normal profile:

1. Open `Interfaces`.
2. Add or enable `WNS` and `RMAP`.
3. Add your private gateway or RNode/LoRa interface if you use one.
4. Open `Identities` and rename the identity label.
5. Restart OMENbrowser_rs.

That gives the browser a clean startup with the selected Reticulum interfaces
and usually gets Directory/announce traffic moving quickly. See
[Getting Online Fast](GETTING_ONLINE.md) for the fuller note.

Keep Reticulum instance mode set to **Managed** for v0.9.6-7. External/shared
mode is retained as a readable deferred configuration but intentionally does
not start integrated interfaces until a real shared backend is negotiated. See
[Network Backends](NETWORK_BACKENDS.md).

## Start omenchatd

Initialize a server home:

```bash
./src/server/target/release/omenchatd init --home /tmp/omenchatd-test
```

Attach a TCP gateway:

```bash
./src/server/target/release/omenchatd interfaces tcp-client <gateway-host:port> --home /tmp/omenchatd-test
```

Start the server TUI:

```bash
./src/server/target/release/omenchatd tui --home /tmp/omenchatd-test
```

Inside the TUI, press `g` to start the live server, `c` for Monitoring, `l` for
Logs, and `q` to quit.

Show connection targets:

```bash
./src/server/target/release/omenchatd status --home /tmp/omenchatd-test
```

The status output includes an `omenchat://...` client URI and a NomadNet portal
URL.
