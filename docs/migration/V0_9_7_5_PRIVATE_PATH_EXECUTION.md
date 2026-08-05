# v0.9.7-5 private-path containment execution record

Baseline: released `v0.9.7-4` commit
`839d746242c1264e2e2f729a4d36d19d64411ad9`.

The untouched quick release baseline passed before production edits. Focused
reproducers then confirmed three v0.9.7-4 boundaries: lexical `..` could be
classified below the managed root, a symlinked `config.toml` was parsed before
later initialization rejected it, and recursive managed-directory creation
could cross an intermediate symlink. All fixtures used isolated temporary
roots and outside sentinels.

The server now owns a small private-path policy. It establishes one clean
lexical selected root plus canonical managed anchor, rejects parent components,
walks managed suffixes component by component, and retains clean custom paths
without claiming their ancestors. Existing private-file reads and appends use
path/handle identity checks on Unix. Production file-backed SQLite opens use a
central `NOFOLLOW` wrapper without changing connection policy.

## Pre-version qualification evidence

- Focused config/path/SQLite containment tests pass.
- The complete standalone `server-full` suite passes 593 tests with the 12
  established opt-in/known-boundary tests still visible as ignored.
- Strict standalone all-target Clippy passes.
- The existing permissive-umask SQLite test continues to verify main/WAL/SHM
  `0600` modes and representative committed data.
- Existing migration, downgrade/export, upload, log, Reticulum, TUI, shutdown,
  protocol, and no-replay tests remain green in the complete server suite.

## Final qualification evidence

- Root and standalone package versions are `0.9.7-5`; both retain the exact
  official registry Reticulum/LXMF `0.9.7` train.
- Post-version `release-check.sh full` passed after the final fixes: the root
  suite reported 1,617 passing tests and 31 established ignores; the standalone
  server suite reported 594 passing tests and 12 established ignores.
- The isolated current-upload lane passed with a second client Resource fetch;
  continuous reconnect passed with stable destination identity and post-restart
  message/reaction/revision/pin recovery; the current NomadNet page lane passed
  with a direct request and exact returned page bytes.
- Pinned Python interoperability passed against the immutable RNS/LXMF refs.
  The informational current-Python lane passed against RNS 1.4.0, LXMF 1.1.0,
  and NomadNet 1.2.7, including request cancellation/no-replay and retained-link
  recovery.
- Mixed v0.6.0-1/v0.9.7-5 direct LXMF, SQLite history reopening, live OMENchat,
  and propagated LXMF lanes passed.
- The exact ignored maximum-UDP Resource sentinel was invoked and failed at the
  unchanged upstream boundary: a 456-byte transmit buffer versus a 483-byte
  maximum wire packet. No local workaround or weakened assertion was added.
- Linux ARM64 passed protocol and headless tests under Cross/QEMU, then produced
  a checksum-verified archive and passed the emulated lifecycle smoke. The
  initial ARM run exposed an unsupported-headless-TUI ordering regression; the
  command now rejects before loading configuration or creating a selected home,
  and the native focused test plus the complete rerun passed.
- The Linux package candidate passed archive checksum/extraction, required-file
  checks, isolated server init/status, support collection, and two-client
  OMENchat smoke.

Native Windows and macOS remain hosted-CI boundaries because they cannot be
claimed from this Linux host. No physical radio/interface claim is made. No
push, tag, release, or publication has occurred.
