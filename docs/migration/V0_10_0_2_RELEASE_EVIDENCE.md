# v0.10.0-2 release evidence

Status: release-qualified packaging correction; not yet tagged or published.

## Defect

GitHub Package run `32893236970` passed generic release artifacts, Windows, and
all three native prerequisites, then failed both macOS package jobs before
compilation. `scripts/package-macos.sh` rejected `0.10.0-1` because its bundle
mapping constrained every numeric component to one digit.

## Correction

The macOS mapping now preserves existing v0.9 values and maps v0.10.0 revision
2 to short version `0.10.0` and build version `1000.0.2`. The mapping-only mode
runs on every host in `scripts/release-check.sh`, preventing recurrence before
tagging. No product/runtime behavior changed.

## Retained evidence and limitations

All v0.10.0-1 product, protocol, reconnect, upload, Python, mixed-version,
security, rollback, performance, ARM64-emulated, and package evidence remains
applicable. The two unchanged upstream Resource sentinels remain red and no
workaround was added.

## Local corrective gates

- `bash scripts/release-check.sh quick`: pass, including documentation,
  independent `0.10.0-2` lockfiles, exact registry 0.10.0 train, the
  host-independent macOS mapping regression, binary identities, TUI lifecycle,
  focused OMENchat behavior, and standalone relocation.
- `bash scripts/release-package.sh`: pass; staged browser and server binaries
  report `0.10.0-2` and isolated server initialization succeeds.
- `bash scripts/release-check.sh package`: pass; finalization, checksum,
  extraction, required contents, collector redaction, and two isolated browser
  clients against one extracted `omenchatd` succeed.

The native Intel and Apple-Silicon DMG lifecycle results remain open until the
corrective immutable tag runs on GitHub's macOS hosts.
