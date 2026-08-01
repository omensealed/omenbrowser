# OMENbrowser_rs and omenchatd v0.9.6-7 release notes

Reticulum/LXMF crate train: exact `0.9.6`

Status: final

## omenchatd recovery and readiness

- Interface health now aggregates current Reticulum interface state and owned
  worker liveness. Diagnostics distinguish configured, connecting,
  reconnecting, operational, degraded, terminal, and no-interface states.
- Ordinary TCP reconnect remains owned by the existing interface worker. Three
  consecutive terminal all-worker observations can schedule one deduplicated
  five-second full-runtime recovery; valid progress cancels it.
- TUI recovery is deadline-driven. It no longer blocks drawing, keyboard input,
  Stop, quit, or shutdown with a production thread sleep.
- The TUI requests a full redraw after live Reticulum events, repairing its
  alternate-screen surface when the pinned transport writes a link-close line
  directly to stdout.
- The headless server waits on bounded control/payload queues, shutdown, and
  required deadlines instead of waking every 25 ms.
- Headless and TUI live runtimes use one standalone-server policy: one through
  four async workers, at most eight blocking threads, and stable worker names.
  Existing readiness text remains present for package/service compatibility.

## Resource lifecycle correctness

- Internal received and terminal events retain the exact 32-byte Reticulum
  Resource hash, bounded expected-size/progress evidence, and a bounded reason
  where the pinned API supplies it.
- Outbound application Resource IDs use a bounded, expiring exact
  `(link_id, resource_hash)` correlation map and release on terminal, link close,
  shutdown, or TTL.
- Pinned 0.9.6 inbound failure events do not expose OMENchat metadata. The server
  removes one pending upload only when authenticated identity plus expected size
  identifies one unique candidate. Unmatched or ambiguous failures remove none;
  identity-wide cleanup remains owned by disconnect/link close/replacement/TTL.
- No OMENchat wire field, protocol number, retry behavior, or persistent schema
  changed.

## Qualification evidence

- Canonical desktop, static-media desktop, root TUI, and standalone server tests
  pass with strict Clippy gates.
- Full release-check, real PTY terminal restoration, multi-client/server-restart
  OMENchat, continuous reconnect, current upload Resource, and current NomadNet
  direct-page smokes pass with isolated roots.
- Linux ARM64 headless protocol/server tests and package lifecycle pass through
  Podman/Cross/QEMU. This is not a physical Raspberry Pi claim.
- A short machine-specific no-interface sample recorded 10,460 KiB RSS, seven
  threads, 13 file descriptors, and one CPU tick over five seconds. It is
  evidence for this host, not a universal resource threshold.

## Known upstream and external boundaries

- The exact locked-0.9.6 maximum UDP Resource reproducer still fails: the
  upstream 456-byte transmit buffer cannot serialize the 483-byte maximum
  Resource packet. OMEN does not hide this with a fork, weaker limit,
  incompatible fragmentation, or retry loop.
- External reticulumd, physical radio/ARM hardware, native Windows/macOS,
  hosted CI, and compositor/GPU evidence are not inferred from local Linux
  qualification. Those remain separate candidate/publication gates.

## Compatibility and rollback

- OMENchat remains protocol version 1 with explicit capability negotiation.
- Reticulum/LXMF dependencies remain exact registry `0.9.6`; no private fork,
  patch override, or `rns-net` dependency was added.
- Browser and omenchatd identities, configuration, databases, and storage roots
  remain separate. No mandatory database or configuration migration is present.
- No uncertain send, request, Resource operation, or durable mutation gains an
  automatic replay.
- Rolling back to `v0.9.6-6` requires no database downgrade. Preserve normal
  identity, configuration, and message backups before replacing binaries.
