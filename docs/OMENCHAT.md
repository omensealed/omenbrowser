# OMENchat

OMENchat consists of:

- the built-in OMENbrowser_rs chat client;
- the standalone `omenchatd` server under `src/server`.

The server owns its own home directory and must not use `~/.reticulum`,
`~/.nomadnetwork`, or `~/.lxmd` by default.

## Server Storage

Default server home:

```text
~/.omenchatd/
```

Typical isolated test home:

```text
/tmp/omenchatd-test
```

The server home contains identity material, Reticulum config/storage, the SQLite
database, logs, uploads, and the NomadNet portal page.

## Server Commands

```bash
omenchatd init --home /tmp/omenchatd-test
omenchatd status --home /tmp/omenchatd-test
omenchatd doctor --home /tmp/omenchatd-test
omenchatd tui --home /tmp/omenchatd-test
omenchatd run --home /tmp/omenchatd-test
```

Add a TCP gateway:

```bash
omenchatd interfaces tcp-client <gateway-host:port> --home /tmp/omenchatd-test
```

For IFAC-protected gateways:

```bash
omenchatd interfaces tcp-client <gateway-host:port> \
  --home /tmp/omenchatd-test \
  --network-name <network-name> \
  --passphrase <passphrase>
```

## Browser Client

Open a chat server with:

```text
omenchat://<destination_hash>
```

The Directory also lists announced OMENchat servers when their announces are
seen.

In `omenchatd tui`, use **Announce Now** after the live server is running to
send the OMENchat and NomadNet portal announces immediately. This is useful for
testing discovery without waiting for the configured announce interval.

## Interface Recovery

`omenchatd` watches live Reticulum interface stats while the server is running.
If configured interfaces repeatedly report disconnected, or interface stats stop
responding, the server rebuilds its live Reticulum runtime and announces again.
This is intended to recover after a TCP gateway, private gateway, or local RNS
instance restarts without requiring an `omenchatd` process restart.

Check the TUI Monitoring panel or `~/.omenchatd/omenchatd.log` for lines that
include `interface watchdog` and `live runtime restarted after interface
watchdog`.

## Uploads And Media

- Default per-file upload limit: `512 KiB`.
- Default rotating per-user upload quota: `50 MiB`.
- Server admins can change limits in `omenchatd` config.
- NomadNet/Reticulum images can be loaded inline.
- Clearweb image loading is gated by media privacy settings and SOCKS5/Tor
  detection.
- Non-image clearweb links open through the external browser prompt. Use Copy
  URL for Tor Browser and paste into the running Tor Browser window.

## Expected Client Behavior

- Reconnect when a live link drops.
- Preserve recent history after restart.
- Sync recent room history on join/reconnect.
- Keep local echo messages and retry failed sends.
- Show unread state when chat panes are minimized.
