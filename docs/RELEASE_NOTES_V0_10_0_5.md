# OMENbrowser_rs v0.10.0-5 release notes

Status: final

OMENbrowser_rs and standalone omenchatd are `0.10.0-5`. The exact official
crates.io Reticulum/LXMF train remains `0.10.0`. OMENchat wire protocol `1`,
`omenchat-protocol 0.2.0`, SQLite schema `14`, and `omen-ifac-tcp 0.9.5-1` are
unchanged.

## Changes

- Adds source-backed capability guards for upstream passive announce retention
  (#581) and the incomplete announce broadcast ladder (#578).
- Adds one bounded, redacted notice to omenchatd doctor and browser native
  network diagnostics. It does not expose identities or private table state.
- Re-audits OMEN-owned event, history, Link, reconnect, upload, diagnostic, and
  worker retention. Existing bounds and terminal cleanup remain in force; no
  automatic retry, replay, restart workaround, or transport role change was added.
- Clarifies managed versus rendered/external-only interface support. RNode is
  deferred without named hardware evidence.
- Defers Channel/Buffer attachments because the complete negotiated,
  three-node, bounded, and adjacent-release proof gate is not available.

## Known upstream limitations

Four independent limitations remain: passive announce retention, incomplete
announce-broadcast policy, routed Resource retransmission after fragment loss,
and maximum-size UDP serialization. Direct/local Resource success does not
promote routed support. Resource completion is not a durable application commit.

OMEN uses no private fork, Git dependency, vendor copy, copied PR,
`[patch.crates-io]`, automatic uncertain replay, or automatic restart workaround.

## Packaging and evidence

macOS maps to short `0.10.0` and build `1000.0.5`; Windows MSI maps to
`0.10.0.5` and qualifies upgrade from `0.10.0-4`. Candidate SHA and final local,
hosted, package, checksum, Python, mixed-release, platform, and unavailable-lane
results are maintained in the release evidence. Local quick, full, smoke,
reconnect, upload, NomadNet, adjacent direct interop, and archive creation gates
passed on 2026-08-27. Signing, notarization, public
announce traffic, display/PTY, and physical RNode lanes are unavailable unless
the evidence records an exact candidate run.
