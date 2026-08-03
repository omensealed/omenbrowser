# v0.9.7-3 reliability execution record

Baseline: released `v0.9.7-2` commit
`7deaafa6a1827588fec3a444b8707ff93fa1c93d`.

The implementation converts all six synchronous `LiveServerWorker` status and
moderation accessors to typed poison errors, preserves the headless fatal/drain
path, and keeps shutdown cancellation/join work bounded even when best-effort
link enumeration is unavailable. TUI monitoring renders unavailable data as
unavailable and moderation never reports a failed disconnect as success.

The localized native NomadNet change records skipped direct-response and
Resource events while waiting for the existing correlation, cancellation,
stream-close, or deadline. Only final timeout text changes. Request dispatch,
primitive selection, bounds, and cancellation remain unchanged; no replay or
fallback is added.

## Qualification evidence

- `bash scripts/release-check.sh quick`: passed on the unmodified baseline.
- `bash scripts/release-check.sh full`: passed before and after the version
  transition. The post-transition run passed 1,643 browser tests and 572
  full-server tests, strict Clippy, standalone relocation, and real-PTY
  shutdown in 66--68 ms. Established opt-in measurements remained explicitly
  ignored rather than relabeled.
- `cargo audit` and `cargo audit --file src/server/Cargo.lock`: no
  vulnerabilities; five existing root maintenance warnings remain allowed by
  policy. `cargo deny check` passed advisories, bans, licenses, and sources.
- Pinned Python Reticulum/LXMF interoperability passed against immutable RNS
  `15320e4d2cfabb143c1db20ca887e275fd521585` and LXMF
  `727830cefda83d9c6e3982b48675425f3f988f9c` references.
- Current informational Python interoperability passed with RNS 1.4.0, LXMF
  1.1.0, and NomadNet 1.2.7, including IFAC, proof ordering, direct and
  propagated LXMF, stamps/tickets, NomadNet primitive selection,
  timeout/cancellation without replay, retained-link recovery, and release-mode
  measurements.
- Isolated current-product upload/Resource, two-client, continuous
  reconnect/restart, and NomadNet direct-page lanes passed at package version
  `0.9.7-3`.
- The mixed `0.6.0-1`/`0.9.7-3` SQLite history lane passed old/current/old
  reopen ordering and metadata checks.
- Linux ARM64 Cross/Podman/QEMU passed 60 shared-protocol tests, 444 headless
  server tests, release packaging, checksum, and isolated lifecycle execution.
- `bash scripts/release-package.sh` and `bash scripts/release-check.sh package`
  passed checksum, extraction, required-file, redaction, isolated server, and
  two-client packaged smoke checks.

The deliberately ignored maximum-UDP Resource sentinel was run explicitly and
retains its expected upstream failure: the locked UDP transmit buffer is 456
bytes while the maximum serialized Resource packet requires 483 bytes. The
sentinel and limitation remain unchanged and visible.

## External boundaries

Hosted Linux CI, native Windows, Intel macOS, Apple Silicon macOS, and the full
hosted mixed-release matrix remain pending until the candidate is pushed with
maintainer authorization. No physical radio or GPU evidence is claimed. The
local ARM64 gate is emulated Cross/QEMU evidence, not physical Raspberry Pi
qualification.

## Compatibility and rollback

Both Cargo roots report `0.9.7-3` and retain the exact official registry
Reticulum/LXMF 0.9.7 train. No dependency source or family version changed. No
OMENchat wire/capability, database, configuration, cache, identity,
destination, message, upload, ticket, or Reticulum-storage migration exists.
Rollback to `v0.9.7-2` needs no state conversion.
