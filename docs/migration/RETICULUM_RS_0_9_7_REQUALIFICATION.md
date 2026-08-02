# Reticulum/LXMF 0.9.7 requalification

Status: implementation and supported-host local qualification complete. This
document is the living evidence record for the `0.9.7-1` candidate; it is not a
release or a universal interoperability claim.

## Baseline

- Released source: annotated tag `v0.9.6-7`, commit
  `14359fd567660839eb2ab0995b73acf542a1c4ac`.
- Upgrade branch: `upgrade/omenbrowser-v0.9.7-1`.
- Host: x86_64 Linux 7.1.3-2-cachyos.
- Toolchain: rustc 1.97.1, Cargo 1.97.1; the manifests retain MSRV 1.85 and
  edition 2021.
- The unmodified `bash scripts/release-check.sh quick` and
  `bash scripts/release-check.sh full` gates passed.
- The unmodified maximum-UDP Resource sentinel failed as documented: the
  locked 0.9.6 UDP buffer was 456 bytes and the maximum type-one Resource wire
  packet was 483 bytes.
- A five-second isolated no-interface omenchatd sample used 10,500 KiB RSS,
  seven threads, 13 file descriptors, and one CPU tick at 100 ticks/second.

The baseline dependency trees and complete gate logs are retained outside the
repository under `/tmp/omenbrowser-v0971-baseline` for this checkout. They
contain no user identity or message data.

## Dependency result

Both independent Cargo roots resolve only official crates.io packages from the
exact 0.9.7 family. The root resolves `lxmf`, `lxmf-sdk`, `lxmf-wire`,
`reticulum-rs`, `reticulum-rs-core`, `reticulum-rs-rpc`, and
`reticulum-rs-transport` at 0.9.7. The standalone server resolves
`reticulum-rs`, `reticulum-rs-core`, and `reticulum-rs-transport` at 0.9.7.
There is no Git source, patch override, 0.9.6 family member, or duplicate family
version. The private protocol-neutral `omen-ifac-tcp` package keeps its
independent 0.9.5-1 package version while its direct transport pin moves to
0.9.7.

Cargo added `atomicwrites` through the upstream train. The desktop graph also
updated Rustls support packages required by the new resolution. The standalone
server graph gained upstream tempfile/filesystem support packages. No unrelated
direct dependency was changed.

Official upstream source reviewed: tag `v0.9.7`, commit
`bdfbaf2daed2c6ed0f0dd5527092171d91f1ac87`. All required direct crates are
published at 0.9.7 and declare Rust 1.85 support.

## API adaptations

Upstream destination registration now permits immutable access in the paths
OMEN uses. Obsolete `mut` bindings were removed from the server and test
fixtures only where the compiler proved they were unnecessary. Fixtures that
mutate interface configuration retain mutable access. No destination name,
identity derivation, packet context, wire format, storage format, or timeout
policy changed.

Upstream 0.9.7 also increments every received packet's hop count before it
publishes an `AnnounceEvent`, matching reference Reticulum. The pinned-Python
interop assertion now expects a directly connected Python destination to be
one observed network hop away. The same test continues through identity recall,
Link activation, proof ordering, tamper rejection, and reconnect, so this is a
qualified event-semantic update rather than a relaxed connectivity test.

## Behavior decisions

### Worker supervision and outer recovery

Upstream 0.9.7 owns all transport worker handles in a `JoinSet`, cancels the
worker group when one worker returns or panics, attributes failures by worker
name, and drains quietly after normal cancellation. OMEN retains its outer
generation-scoped recovery boundary. Ordinary `connecting` and `reconnecting`
states do not request a competing runtime; only terminal aggregate health may
schedule one delayed, deduplicated recovery. Stop cancels pending recovery and
stale generations cannot become due for a newer runtime.

### SDK/RPC fields

The published 0.9.7 `RpcBackendClient` still discards `idempotency_key`,
`ttl_ms`, `correlation_id`, and `extensions` while constructing
`sdk_send_v2`. Its public typed send path also cannot carry OMEN's explicit
reply ticket. A deterministic loopback capture proves that method, stamp cost,
ticket-inclusion request, and propagation fallback survive, while the four
operation fields remain absent; cancellation uses the daemon-returned message
identity. OMEN therefore retains local deadline enforcement, refuses an
explicit external reply ticket before connecting, never invents daemon
guarantees, and never automatically replays an uncertain send.

### Stamps and tickets

OMEN retains its dynamic authenticated relay/peer advertised-cost policy,
direct safety ceiling, reply-ticket precedence, bounded proof workers, and
single final stamp ownership. Upstream's default propagation stamp is not a
replacement for relay-advertised cost handling.

### IFAC

The project-local IFAC TCP adapter remains in place. Stock 0.9.7 behavior has
not been proven to replace its Python-compatible enforcement. The adapter's
wire vector is unchanged. Local hardening makes receive-queue backpressure
cancellable, supervises the paired read/write tasks, reports join failure,
uses a constant-time tag comparison, and bounds the TCP read and delimiter-free
HDLC accumulation allocations to approximately 64 KiB and 512 KiB per active
connection respectively. The supported 262,144-byte MTU is unchanged.
Pinned Python and current Python interoperability both pass split/coalesced
HDLC, reconnect, wrong credentials in both directions, tamper/proof ordering,
identity recall, Link activation, and Link data.

### Resources

The exact 0.9.7 maximum-UDP sentinel still fails at 456 versus 483 bytes. The
test remains explicit and ignored in normal suites. This blocks a maximum-size
UDP Resource claim, not smaller UDP, TCP, OMENchat, or NomadNet Resource paths
that pass their own gates. No upstream patch, application fragmentation, or
limit weakening is used.

### Identity and replay

OMENchat protocol v1, all wire fixtures, identity formats, destination aspects,
database/config/cache schemas, and isolated browser/server storage ownership
remain unchanged. Existing malformed-identity tests continue to require
failure without regeneration. NomadNet and LXMF retain the rule that a timeout
or post-dispatch cancellation never selects another primitive or automatically
replays uncertain work.

### Advisory parity inventory

The advanced native-startup/support report now includes the official 0.9.7
`lxmf-sdk` software-parity orientation when that SDK is compiled. The field is
explicitly labeled as advisory implementation capability metadata, not live
interoperability proof. Profiles without the SDK report it as unavailable
rather than fabricating equivalent counts.

## Validation ledger

Completed:

- baseline `release-check quick`: pass;
- baseline `release-check full`: pass;
- exact registry-train verification in both roots: pass;
- required native-LXMF, desktop-product, server-headless, and server-full
  compile checks: pass;
- focused external RPC field/cancellation capture: pass;
- IFAC unit/vector/tamper/wrong-credential/bounds tests: pass;
- IFAC all-target strict Clippy: pass;
- 0.9.7 maximum-UDP sentinel: expected failure, 456 versus 483 bytes;
- canonical desktop-product tests and all-target strict Clippy: pass;
- canonical standalone server-full tests and all-target strict Clippy: pass;
- static-media and root TUI tests, plus TUI strict Clippy: pass;
- standalone relocation verification: pass;
- final `release-check full`: pass;
- current upload, continuous reconnect, and current NomadNet page smokes: pass;
- pinned Python Reticulum/LXMF interoperability: pass;
- current Python drift lane (RNS 1.4.0, LXMF 1.1.0, NomadNet 1.2.7): pass;
- mixed 0.6.0-1/0.9.7-1 direct LXMF, OMENchat history Resource, and
  propagation stamp/ticket lanes: pass;
- isolated native identity CLI matrix: pass;
- Linux ARM64 headless protocol/server tests and packaged lifecycle under
  Cross/Podman QEMU: pass; this is not physical-device or radio evidence;
- bounded durable-retention, queue-backpressure, SQLite, live-link, and runtime
  thread measurements: pass.
- Linux x86_64 release packaging and the archive-level isolated package gate,
  including two-client OMENchat smoke: pass.

Same-host resource evidence:

- The post-upgrade no-interface omenchatd median from three five-second samples
  was 10,424 KiB RSS, seven threads, 13 file descriptors, and one CPU tick at
  100 ticks/second. The single baseline sample was 10,500 KiB, seven threads,
  13 descriptors, and one tick; this is flat within sampling noise.
- Saturated production queues remained at their configured item/byte ceilings,
  rejected overload, kept control latency at or below 21 ms, and drained to
  zero.
- The five-second SQLite sample committed 500 events with database integrity
  `ok`, one in-flight operation maximum, 1,767 microsecond maximum heartbeat,
  and 970,752 bytes RSS growth.
- The five-second reconnect sample ended with zero active/pending links, no
  file-descriptor or task growth, and 303,104 bytes RSS growth.
- Durable replay/intents retained and pruned at their configured bounds; no
  history or queue limit was raised.

External release evidence not established by this local qualification:

- hosted CI and native Windows/macOS packaging and smoke;
- physical interface/radio and ARM64 device evidence.

## Rollback

No persistent format or protocol migration is introduced. Rolling back consists
of using the released `v0.9.6-7` binaries against the same unchanged roots.
Transient 0.9.7 build artifacts may be removed without touching identities,
messages, server databases, uploads, or configuration. The local IFAC buffer
and supervision change can be reverted independently without data migration;
its wire bytes remain identical.
