# OMENbrowser_rs and omenchatd v0.10.0-2 release notes

Status: final

Reticulum/LXMF crate train: exact official crates.io `0.10.0`.

## Packaging correction

- Fix the macOS bundle-version preflight that rejected product minor versions
  above 9 before compilation. v0.10.0 now maps to
  `CFBundleShortVersionString 0.10.0` and revision 2 maps to
  `CFBundleVersion 1000.0.2`.
- Preserve the historical v0.9 numeric mapping and monotonic ordering into the
  v0.10 train.
- Add a host-independent release-gate regression for the v0.10 mapping so the
  failure is detected on Linux before a release tag is created.

## Compatibility

This revision changes no protocol, schema, runtime, identity, storage, upload,
reconnect, Resource, IFAC, telemetry, or dependency behavior. OMENchat wire
protocol 1, `omenchat-protocol 0.2.0`, SQLite schema 14,
`omen-ifac-tcp 0.9.5-1`, and the exact official registry Reticulum/LXMF 0.10.0
train remain unchanged.

The routed Resource retransmission and maximum-wire UDP sentinels remain the
same separately documented upstream limitations. No workaround, retry, replay,
fallback, fragmentation, backend switch, or second dispatch is introduced.

## Release evidence

The v0.10.0-1 CI and generic/Windows package prerequisite jobs passed. Both
macOS package jobs failed only at the old numeric mapping guard before product
compilation. v0.10.0-2 retains the v0.10.0-1 product qualification and adds the
corrected mapping regression and rerun package evidence described in
`migration/V0_10_0_2_RELEASE_EVIDENCE.md`.
