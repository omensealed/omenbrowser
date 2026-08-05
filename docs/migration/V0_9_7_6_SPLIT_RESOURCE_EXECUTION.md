# v0.9.7-6 split-Resource execution evidence

Status: local implementation and qualification complete; hosted native platform gates pending

## Baseline

- Captured: 2026-08-05 (America/New_York)
- Branch: `hardening/v0.9.7-6-split-resource`
- Starting commit/tag: `b2e8b21a56b03cad0a772ac74c412f7ed89e4cfa` / `v0.9.7-5`
- Starting worktree: clean
- Host: Linux x86_64 (`x86_64-unknown-linux-gnu`)
- Rust: `rustc 1.97.1 (8bab26f4f 2026-07-14)`; Cargo `1.97.1`
- Installed targets: `x86_64-unknown-linux-gnu`, `x86_64-pc-windows-gnu`,
  `aarch64-unknown-linux-gnu`
- Root/server packages started at `0.9.7-5` and now report `0.9.7-6`; both
  retain empty default features and independent lockfiles.
- Reticulum/LXMF: exact official registry `0.9.7` in both roots; no Git or
  patch override.

## Upstream status at execution

- `FreeTAKTeam/LXMF-rs#553`: open; split-Resource metadata assembly defect.
- `FreeTAKTeam/LXMF-rs#556`: open; proposed upstream correction.
- `FreeTAKTeam/LXMF-rs#544`: open; outbound Resource auto-compression gap.
- crates.io latest official `reticulum-rs`, `reticulum-rs-transport`, `lxmf`,
  and `lxmf-sdk`: `0.9.7`. This revision does not change the approved train.

## Resource call-site inventory

- Server outbound OMENchat Resources are prepared in `session.rs` and bridged
  by `transport.rs`; the released bridge sends the offer frame before looking
  up and offering the retained payload.
- Server inbound transport events are consumed in `reticulum_live.rs`; the
  released bridge ignores `SegmentComplete` together with ordinary progress.
- Native NomadNet direct-request and request-Resource response loops are in
  `runtime/native/request.rs`; both accept completed response Resources without
  an explicit split-segment rejection.
- Desktop OMENchat Resource events are bridged in `runtime/native/adapter.rs`;
  the released bridge logs and ignores `SegmentComplete`, then can accept the
  later assembled `Complete`.
- Client uploads are admitted in `desktop/omenchat_commands.rs`, retained by
  the bounded live client state, and dispatched after `UploadAccept` in
  `chat/live.rs`.
- Server upload admission, pending reservations, publication, fetch, and
  Resource IDs are owned by `session.rs`. Configured maximums reach 10 MiB;
  the default is 512 KiB.

## Baseline commands

- `bash scripts/release-check.sh quick` — pass on the untouched v0.9.7-5
  baseline before production edits.
- `scripts/verify-reticulum-train.sh` (through quick gate) — pass, exact 0.9.7
  official registry family in both roots.
- `scripts/verify-accepted-advisories.sh` (through quick gate) — pass, zero
  accepted vulnerabilities.

## Confirmed exposure before correction

- Metadata wire total is `3 + metadata bytes + payload bytes`; the affected
  exact train splits above 1,048,575 bytes.
- Every OMENchat Resource metadata value begins with
  `omenchat-resource:` and therefore reaches the affected metadata path.
- The released server ordering can expose an offer before discovering that its
  retained payload cannot be dispatched safely.
- `SegmentComplete(total_segments > 1)` is not a terminal condition in the
  released NomadNet, server, or desktop OMENchat bridges.

## Compatibility constraints

No protocol, capability, database, configuration, cache, identity,
destination, upload-content, ticket, message, or Reticulum-storage migration is
authorized. No retry, replay, fallback, second dispatch, dependency fork, or
application-level fragmentation is introduced.

## Qualification results

The original server exposure test failed before the production correction: an
unsafe retained payload still caused one original offer frame and one Resource
dispatch. After the correction, focused tests prove:

- exact 1,048,575-byte metadata wire total sends one frame and one Resource;
- plus one sends one bounded error and zero original offers/Resources;
- retained payload is consumed exactly once on success, frame failure,
  dispatch failure, and preflight rejection;
- the 512 KiB default is unchanged, 10 MiB configuration remains stored, and
  negotiation/admission uses the derived exact-train maximum;
- desktop upload admission constrains absent and larger peer advertisements;
- native NomadNet checks exact completed metadata size and rejects
  `SegmentComplete(total_segments > 1)` in both request paths;
- server and desktop OMENchat rejection markers are bounded to 256 hashes with
  a two-minute TTL and suppress one later completion;
- server split rejection produces a typed failed terminal without forwarding
  bytes into the payload queue.

The ignored two-process split sentinel was run against the unmodified registry
0.9.7 transport and failed, confirming the upstream limitation remains. It is
separate from the existing ignored maximum-UDP Resource sentinel.

The completed local qualification was:

- pre-bump and post-bump `bash scripts/release-check.sh full` — pass;
- strict all-target Clippy for `desktop-product` and standalone `server-full`
  — pass with `-D warnings`;
- post-bump desktop suite — 1,624 passed and 31 explicitly ignored; binary and
  integration targets also passed;
- post-bump standalone suite — 604 passed and 13 explicitly ignored;
- `cargo audit` in both roots — no vulnerability finding; the desktop graph
  retains five reviewed warning-only advisories;
- `cargo deny check` in both roots — pass, with existing duplicate-package and
  unmatched-license-allowance warnings;
- current two-client OMENchat upload/Resource smoke — pass at `0.9.7-6`;
- continuous OMENchat server-restart/reconnect smoke — pass, including a new
  link generation and reaction/revision/pin recovery;
- current NomadNet direct page request — pass over one direct request;
- pinned Python interoperability — pass against immutable RNS/LXMF references;
- current Python drift interoperability — pass with RNS 1.4.0, LXMF 1.1.0,
  and NomadNet 1.2.7, including no-replay cancellation/timeout cases;
- mixed `0.6.0-1`/`0.9.7-6` SQLite history reopen — pass in both directions;
- Linux ARM64 protocol/headless tests, Cross build, checksum, and QEMU/Podman
  lifecycle — pass (474 active server tests, 13 explicitly ignored);
- `bash scripts/release-package.sh` and
  `bash scripts/release-check.sh package` — pass with isolated two-client
  package smoke.

The local host cannot provide native Windows or macOS execution. Those hosted
jobs remain the final candidate boundary after review/push authorization.
