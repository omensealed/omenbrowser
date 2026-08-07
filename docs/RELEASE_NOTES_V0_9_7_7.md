# OMENbrowser_rs and omenchatd v0.9.7-7 release notes

Reticulum/LXMF crate train: exact official registry `0.9.7`

Status: final

## Resource maintenance

- The reusable OMENchat client upload API now applies the same derived
  Reticulum 0.9.7 single-segment-safe ceiling as the ordinary desktop upload
  flow. Unsafe payloads are rejected before sequence reservation, pending
  state, offer-frame creation, or Resource dispatch.
- Smaller server-advertised and room upload limits remain authoritative. A
  larger peer-advertised limit cannot bypass the local exact-train ceiling.
- Native NomadNet diagnostics distinguish ordinary inactivity from observed
  Resource activity that did not produce a valid completion before the existing
  deadline. The diagnostic explicitly states that no retry was attempted and
  retains bounded event-lag evidence.
- Desktop and standalone-server diagnostics expose bounded, redacted,
  runtime-ephemeral counters for unique split-Resource rejection, suppressed
  late completion, and actual rejected-marker TTL expiry. No identifiers,
  metadata, or payload data are retained by these counters.

## Compatibility boundaries

- Package versions advance to `0.9.7-7`; OMENchat protocol version `1` and all
  capability identifiers remain unchanged.
- No database, configuration, cache, identity, destination, message, ticket,
  upload-content, or Reticulum-storage migration is introduced.
- Reticulum/LXMF remains the exact official registry `0.9.7` train. There is no
  Git dependency, private fork, vendoring, or patch override.
- No request/send retry, replay, fallback, backend switch, primitive switch, or
  second dispatch was added.
- The default 512 KiB upload behavior and existing queue, parser, history,
  upload, marker, timeout, retention, and shutdown bounds remain unchanged.
- The maximum-UDP Resource and split-metadata Resource upstream sentinels remain
  visible and separately named. Neither limitation is described as fixed.

## Qualification

The source change was qualified with root and standalone-server full tests and
strict Clippy, quick/full release checks, isolated two-client upload,
reconnect/restart, direct and Resource NomadNet, pinned/current Python
interoperability, and the Linux ARM64 Cross/QEMU package lifecycle. Native
Windows, Intel macOS, and Apple Silicon checks are release gates on the reviewed
candidate.

## Rollback

This revision changes bounded runtime admission, diagnostics, and ephemeral
counters only. A v0.9.7-6 binary can reopen the same configuration, identity,
database, cache, messages, tickets, uploads, and Reticulum storage without
conversion. Counter values intentionally reset with runtime restart.
