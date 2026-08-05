# OMENbrowser v0.9.7-5 private-path containment baseline

Date: 2026-08-04

- Branch at capture: `main`, then `hardening/v0.9.7-5-private-path`
- Released baseline/tag: `839d746242c1264e2e2f729a4d36d19d64411ad9` / `v0.9.7-4`
- Initial worktree: clean and byte-identical to the released commit
- Host: Linux x86_64 (`7.1.3-2-cachyos`)
- Rust/Cargo: `1.97.1`; repository MSRV remains `1.85`
- Installed targets: Linux x86_64, Linux ARM64, Windows GNU x86_64
- Root/server package versions: `0.9.7-4`
- Reticulum/LXMF: one exact official registry `0.9.7` train in each applicable Cargo root

## Untouched qualification

`bash scripts/release-check.sh quick` passed before production or test changes.
It covered formatting, private-storage/service policy, release version and
dependency train, advisory policy, product feature identities, desktop/TUI and
standalone-server checks, TUI lifecycle/real-PTY shutdown, focused OMENchat
tests, and standalone relocation. No pre-existing quick-gate failure was
observed.

Native Windows/macOS, ARM64, package installation, live Reticulum peers, and
the full pinned/current Python lanes were not rerun during this initial local
capture; they remain later qualification gates.

## Private-path call-site inventory

The configured sensitive fields are `identity_path`, `database_path`, and
`reticulum_config_path`. `ServerConfig::load_or_default` currently reads an
existing `config.toml` by path before private-file validation. `init_files`
classifies identity/database parents with a raw lexical `starts_with` check,
while managed directory creation is recursive and checks only the final path.
This leaves traversal and intermediate-symlink containment unproven.

Production file-backed SQLite opens exist in config initialization and room
administration, `OmenchatStore`, migration backup creation, database recovery,
and one TUI status path. `AdminDatabase` delegates to `OmenchatStore`. Export
destinations are a separate operator-selected output boundary and must retain
their current creation semantics.

Sensitive config, Reticulum config, identity, active log, SQLite main/sidecar,
migration backup, upload, and generated page operations are bounded already.
The containment revision must preserve those bounds and content while moving
trust decisions to validated roots, stable handles, and centralized SQLite
source-open flags.

## Confirmed risks to reproduce

1. `<root>/../outside/...` is lexically classified as managed and can chmod or
   create state outside the selected root.
2. A final `config.toml` symlink is followed during parsing before later
   initialization rejects it.
3. A deeper managed path can traverse an intermediate symlink during recursive
   directory creation.

All reproducers use generated temporary roots and synthetic bytes only.

Each focused reproducer failed against untouched production behavior with exit
`101`, as expected: traversal initialization returned success, the config
symlink was parsed successfully, and recursive managed creation crossed the
intermediate symlink. These failures are the red baseline for the containment
implementation; no sentinel bytes from real user state were involved.
