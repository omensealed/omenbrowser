# v0.10.0-3 release evidence

Status: tagged but unpublished; superseded by the v0.10.0-4 live reliability correction.

## Defect and correction

The immutable v0.10.0-2 hosted package run `32909447882` passed both native
macOS prerequisite jobs but failed both macOS package jobs before compilation:

```text
scripts/package-macos.sh: line 77: mapfile: command not found
```

macOS provides Bash 3.2, while `mapfile` was introduced in Bash 4. The package
script and release regression now use command substitution plus `sed` line
extraction, which is supported by Bash 3.2. The mapping itself is unchanged
apart from the release revision: `0.10.0` / `1000.0.3`.

## Scope

Only release versions, lockfile root package records, macOS packaging shell
portability, its regression, and release documentation changed. Product code,
the exact official registry Reticulum/LXMF 0.10.0 train, protocol 1,
`omenchat-protocol` 0.2.0, schema 14, `omen-ifac-tcp` 0.9.5-1, storage,
identities, limits, and reliability behavior are unchanged.

## Corrective gates

- `bash scripts/package-macos.sh --print-version-mapping 0.10.0-3`: pass;
  emitted `0.10.0` and `1000.0.3`.
- `bash scripts/release-check.sh quick`: pass, including documentation,
  independent lockfiles, exact registry 0.10.0 train, binary identities, TUI,
  focused OMENchat behavior, and standalone relocation.
- `bash scripts/release-package.sh`: pass; staged browser and server binaries
  report `0.10.0-3`, and isolated omenchatd initialization succeeds.
- `bash scripts/release-check.sh package`: pass after final documentation was
  included in the archive; checksum, extraction, required contents, collector
  redaction, isolated omenchatd initialization, and two isolated browser-client
  smoke lanes succeeded.

No release is published until both native macOS jobs and the complete hosted
package workflow pass.
