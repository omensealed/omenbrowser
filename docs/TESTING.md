# Testing Guide

## Safety First

Do not share app roots between test clients unless you intentionally want them
to share the same identity and storage.

Recommended local test roots:

```text
/tmp/omenbrowser-rs-alpha
/tmp/omenbrowser-rs-alpha-2
/tmp/omenchatd-alpha
```

Check root isolation:

```bash
bash scripts/alpha-root-sanity.sh \
  --browser-root /tmp/omenbrowser-rs-alpha \
  --browser-root-2 /tmp/omenbrowser-rs-alpha-2 \
  --server-home /tmp/omenchatd-alpha
```

## Quick Test Gate

```bash
bash scripts/alpha-check.sh quick
```

This runs the fast repository checks used before sharing a build.

## OMENchat Smoke

Local single-client smoke:

```bash
bash scripts/alpha-omenchat-smoke.sh
```

Two-client recent-history smoke:

```bash
bash scripts/alpha-omenchat-smoke.sh --multi-client
```

Expected result:

```text
outcome: pass
reason: OMENchat Link opened, room joined, and message echo was observed
```

## Issue Bundles

Collect a redacted report bundle:

```bash
bash scripts/alpha-collect.sh \
  --browser-root /tmp/omenbrowser-rs-alpha \
  --browser-root-2 /tmp/omenbrowser-rs-alpha-2 \
  --server-home /tmp/omenchatd-alpha
```

Review the created directory before sharing it.

## What To Test

- Start the browser with a fresh identity.
- Add a Reticulum gateway/interface.
- Browse a NomadNet page.
- Open multiple browser panes.
- Send and receive LXMF messages.
- Open an OMENchat server from `omenchat://...`.
- Switch OMENchat rooms, send messages, reconnect, and restart the server.
- Upload small images/GIFs in OMENchat.
- Verify scrollback, load older, and recent-history sync.
