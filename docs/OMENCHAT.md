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
/tmp/omenchatd-alpha
```

The server home contains identity material, Reticulum config/storage, the SQLite
database, logs, uploads, and the NomadNet portal page.

## Server Commands

```bash
omenchatd init --home /tmp/omenchatd-alpha
omenchatd status --home /tmp/omenchatd-alpha
omenchatd doctor --home /tmp/omenchatd-alpha
omenchatd tui --home /tmp/omenchatd-alpha
omenchatd run --home /tmp/omenchatd-alpha
```

Add a TCP gateway:

```bash
omenchatd interfaces tcp-client <gateway-host:port> --home /tmp/omenchatd-alpha
```

For IFAC-protected gateways:

```bash
omenchatd interfaces tcp-client <gateway-host:port> \
  --home /tmp/omenchatd-alpha \
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
