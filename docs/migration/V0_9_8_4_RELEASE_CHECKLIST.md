# v0.9.8-4 release qualification checklist

Target: `v0.9.8-4`

Released baseline: `v0.9.8-3` / `966360ce9c9dd95b7a73b9c596357f2136613ed5`

## Release scope

- [x] Protocol crate API 0.2.0; OMENchat wire protocol remains version 1.
- [x] `nickname-colours-v1` is explicitly negotiated with durable mutations.
- [x] Legacy Links receive exact five-field users and no colour events.
- [x] Schema 14 stores nullable RGB24 through the existing guarded migration.
- [x] Duplicate durable mutation returns the exact prior acknowledgement.
- [x] Client automatic colours are deterministic and display contrast is at
      least 4.5:1 or the theme foreground fallback is used.
- [x] Attachment transport and application bounds are unchanged.
- [x] Routed Resource and maximum-UDP upstream boundaries remain visible.
- [x] Exact registry Reticulum/LXMF 0.9.8 remains unchanged.

## Qualification

- [x] Full local root/server format, tests, and strict Clippy.
- [x] Full release check and Linux package candidate/package smoke.
- [x] Schema-13 migration backup and restore verification.
- [x] Current/current and adjacent v0.9.8-3/v0.9.8-4 OMENchat process lanes.
- [x] Pinned Python interoperability. The informational current-Python drift
      lane passed IFAC, NomadNet, and 9/10 LXMF cases but repeatably timed out
      during propagated-stamp Link activation.
- [x] Linux ARM64 Cross/QEMU tests and package lifecycle.
- [ ] Hosted native Windows MSVC, Intel macOS, and Apple Silicon lanes.
- [ ] Reviewed candidate merged, tagged, and published.

## Rollback

Stop omenchatd, preserve the schema-14 database, restore the automatic
pre-migration schema-13 backup, install v0.9.8-3 binaries, validate integrity
and representative counts, then start the old server. Rolling back only the
binary is not sufficient after schema 14 has opened the database.
