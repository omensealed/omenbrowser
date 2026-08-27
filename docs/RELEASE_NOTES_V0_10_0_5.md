# OMENbrowser_rs v0.10.0-5 release notes

Status: release candidate

OMENbrowser_rs and standalone omenchatd are `0.10.0-5`. The exact official
crates.io Reticulum/LXMF train remains `0.10.0`. OMENchat wire protocol `1`,
`omenchat-protocol 0.3.0` adds the negotiated attachment frame API. SQLite
schema `14` and `omen-ifac-tcp 0.9.5-1` are unchanged.

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
- Adds an explicitly negotiated OMENchat Channel attachment transport with
  MDU-derived chunks, bounded backpressure, staged private files, final digest,
  atomic durable publication, exact-Link cleanup, and no post-dispatch Resource
  fallback or automatic replay. Legacy peers retain the Resource path.
- Fixes stale duplicate-Link retirement so an old Link cannot discard pending
  work while a replacement peer Link remains active.

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
results are maintained in the release evidence. The negotiated direct and
three-node routed Channel upload lanes pass locally, as do both immutable
v0.10.0-4 Resource-downgrade directions. Injected impairment remains
release-candidate work. Signing, notarization, public
announce traffic, display/PTY, and physical RNode lanes are unavailable unless
the evidence records an exact candidate run.
