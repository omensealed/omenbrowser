# Configuration

## Browser Storage

Default root:

```text
~/.config/OMENbrowser_rs/
```

Use `--app-root` for isolated testing:

```bash
omenbrowser_rs --desktop --app-root /tmp/omenbrowser-rs-alpha
```

Each root owns identities, Reticulum config/storage, messages, caches, plugin
state, and pane layout.

## Server Storage

Default root:

```text
~/.omenchatd/
```

Use `--home` for isolated servers:

```bash
omenchatd init --home /tmp/omenchatd-alpha
```

## Interfaces

Configure interfaces in the browser Interfaces panel or through `omenchatd`
commands.

For `omenchatd`:

```bash
omenchatd interfaces tcp-client <gateway-host:port> --home /tmp/omenchatd-alpha
```

## Tor/SOCKS

OMENbrowser_rs detects common local SOCKS5 Tor ports:

```text
127.0.0.1:9050
127.0.0.1:9150
```

When enabled, clearweb image loading can use SOCKS5. External HTTP/HTTPS links
open through the configured browser flow; users are responsible for choosing a
browser profile with the privacy properties they want.

## Identity Safety

Creating a new managed identity creates separate owned storage for that
identity. Do not reuse an app root for independent test clients.
