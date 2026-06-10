# Quickstart

## Build

Browser with desktop UI, native networking, and OMENchat:

```bash
cargo build --release --features chat-client-rns
```

Standalone OMENchat server:

```bash
cargo build --release --manifest-path src/server/Cargo.toml --features live-rns-net
```

## Run The Browser

Normal profile:

```bash
./target/release/omenbrowser_rs --desktop
```

Isolated test profile:

```bash
./target/release/omenbrowser_rs --desktop --app-root /tmp/omenbrowser-rs-alpha
```

Use a different `--app-root` for every test client. Separate roots prevent
identity, Reticulum config, message, cache, and pane-layout collisions.

## Start omenchatd

Initialize a server home:

```bash
./src/server/target/release/omenchatd init --home /tmp/omenchatd-alpha
```

Attach a TCP gateway:

```bash
./src/server/target/release/omenchatd interfaces tcp-client <gateway-host:port> --home /tmp/omenchatd-alpha
```

Start the server TUI:

```bash
./src/server/target/release/omenchatd tui --home /tmp/omenchatd-alpha
```

Inside the TUI, press `g` to start the live server, `c` for Monitoring, `l` for
Logs, and `q` to quit.

Show connection targets:

```bash
./src/server/target/release/omenchatd status --home /tmp/omenchatd-alpha
```

The status output includes an `omenchat://...` client URI and a NomadNet portal
URL.
