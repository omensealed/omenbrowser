# OMENbrowser_rs Packaging

Repository: <https://github.com/omensealed/omenbrowser>

These scripts build local packages from the current source tree. They do not
delete or overwrite user identities, Reticulum storage, message history, or
OMENchat server homes.

Public release packages must use the canonical `desktop-product` feature with
`chat-client-reticulum:on`, `native-network:on`, and `mock-runtime:off`. The packaging scripts verify
this through the browser `--version` output. Server packages must report
`live-reticulum:on` from `omenchatd --version`.

Do not cut the next public package release until the Reticulum/LXMF 0.9.6
migration's live, interoperability, security, and installer gates pass. The
packaging scripts can build local artifacts before then, but a GitHub release
tag must wait for the complete release checklist.

Linux packages must be built on the oldest glibc baseline we intend to
support. Building on a newer rolling distro can produce binaries that fail on
older but still-supported systems with errors like `GLIBC_2.39 not found` or
`GLIBC_2.43 not found`.

The GitHub package workflow builds inside a Debian 11 (`bullseye`) Rust
container so the release binaries target `GLIBC_2.31`. That is a practical
compatibility floor for systems in the Ubuntu 20.04 / Debian 11 / Linux Mint 20
era and newer, without changing the OMENbrowser_rs source code or statically
linking glibc.

Tag-triggered package runs always execute the isolated packaged OMENchat smoke.
Manual workflow runs may disable that expensive smoke while iterating on package
construction, but that option cannot weaken a `v*` release run.

Before publishing Linux artifacts, run:

```sh
bash scripts/check-glibc-floor.sh 2.31 \
  target/release/omenbrowser_rs \
  src/server/target/release/omenchatd
```

If that check fails, rebuild on the compatibility builder before uploading
`.deb`, AppImage, or tarball artifacts.

On rolling or very new distributions, do not use `scripts/package-deb.sh` for
packages that need to run on older machines. Use the containerized compatibility
builder instead:

```sh
bash scripts/package-deb-compat.sh dist-compat
```

That script requires Docker or Podman and builds inside the same Debian 11 /
glibc 2.31 baseline used by GitHub Actions.

## Release Tarball

```sh
bash scripts/release-package.sh dist
```

The tarball is the current public release distribution format. It includes:

- `bin/omenbrowser_rs`
- `bin/omenchatd`
- release smoke-test helpers
- optional user launcher/service installers
- tester docs

## Debian Package

Same-machine build:

```sh
bash scripts/package-deb.sh dist
```

Compatibility build for testers:

```sh
bash scripts/package-deb-compat.sh dist-compat
```

This creates:

```text
dist/omenbrowser-rs_<version>_<arch>.deb
```

The package installs:

- `/usr/bin/omenbrowser_rs`
- `/usr/bin/omenchatd`
- `/usr/share/applications/omenbrowser-rs.desktop`
- `/usr/lib/systemd/user/omenchatd.service`
- docs under `/usr/share/doc/omenbrowser-rs`

Install locally with:

```sh
sudo apt install ./dist/omenbrowser-rs_<version>_<arch>.deb
```

## AppImage

Install `appimagetool` first, then run:

```sh
bash scripts/package-appimage.sh dist
```

If `appimagetool` is not on `PATH`, set:

```sh
APPIMAGETOOL=/path/to/appimagetool bash scripts/package-appimage.sh dist
```

This creates:

```text
dist/OMENbrowser_rs-<version>-<arch>.AppImage
```

The AppImage launches the desktop browser. It also includes `omenchatd` under
`usr/bin` inside the AppDir for manual use.

## Windows portable ZIPs

The native Windows packaging job runs only after the full Windows/macOS compile,
test, Clippy, lifecycle, and product-identity matrix. It builds the canonical
MSVC desktop and standalone server profiles with locked dependencies:

```powershell
./scripts/package-windows-portable.ps1 -OutDir dist
```

It produces separate unsigned archives and SHA-256 files:

```text
dist/OMENbrowser_rs-<version>-windows-x86_64-portable.zip
dist/omenchatd-<version>-windows-x86_64.zip
```

The browser archive does not install or auto-start omenchatd. The server archive
contains no service installer.

## Windows installers

The native Windows job pins `cargo-packager` 0.11.8 and creates two additional
unsigned browser packages after the portable build:

```text
dist/OMENbrowser_rs-<version>-windows-x86_64-setup-unsigned.exe
dist/OMENbrowser_rs-<version>-windows-x86_64-unsigned.msi
```

The setup executable uses current-user NSIS installation. The MSI uses WiX and
maps the Cargo numeric revision deterministically: `0.9.6-1` becomes MSI
`0.9.6.1`. Both reject downgrades. Neither package contains, installs, or starts
omenchatd; the standalone server remains the separate ZIP above.

For v0.10.0-5, MSI `ProductVersion` is `0.10.0.5`; lifecycle qualification
uses `0.10.0-4` as the prior revision and preserves isolated user data.

Before artifact upload, the job creates a prior-revision installer fixture from
the same reviewed binary, installs it, upgrades to the current package, launches
the installed GUI against an explicit temporary `--app-root`, uninstalls, and
proves that a sentinel in the isolated user-data root remains. The NSIS and WiX
tool archives and plugins come from immutable release URLs and are verified
against repository-pinned SHA-256 values before extraction. The final artifacts
are explicitly checked as unsigned and receive separate SHA-256 files.

## macOS DMGs

The native package workflow builds separately named unsigned Intel and Apple
Silicon DMGs after the full native prerequisite matrix passes:

```text
dist/OMENbrowser_rs-<version>-macos-x86_64-unsigned.dmg
dist/OMENbrowser_rs-<version>-macos-aarch64-unsigned.dmg
```

`scripts/package-macos.sh` builds and smoke-tests each `.app` on its matching
native runner before creating the DMG. It then mounts the DMG read-only,
launches the mounted application against an explicit isolated application root,
verifies version/product/architecture identity and normal shutdown, unmounts
it, and generates a separate SHA-256 file. It also produces a separate native
omenchatd `.tar.gz`; the browser DMG does not install or auto-start it.
The bundle's numeric build mapping is deterministic: `0.9.6-2` becomes
`CFBundleShortVersionString=0.9.6` and `CFBundleVersion=906.2`.
For v0.10.0-5 the mapping is short version `0.10.0`, build `1000.0.5`.

Do not claim a universal binary, signing, notarization, or normal Gatekeeper
acceptance until those paths are deliberately implemented and tested. Release
notes and filenames identify these as unsigned tester packages and must
document the required macOS tester action.

## GitHub Actions

Two workflows are included:

- `.github/workflows/ci.yml`
  - runs on pushes and pull requests;
  - installs Rust and Linux build dependencies;
  - runs `bash scripts/release-check.sh quick`;
  - syntax-checks package and installer scripts.
- `.github/workflows/package.yml`
  - runs manually or for `v*` tags;
  - allows a manual `macos` scope to qualify only the two native DMG jobs,
    while tag builds still require the complete release graph;
  - builds the Linux release tarball, `.deb`, and AppImage;
  - builds separate Windows desktop and omenchatd portable ZIPs on Windows;
  - builds and lifecycle-tests unsigned browser NSIS and WiX installers;
  - builds separate native Intel and Apple Silicon unsigned DMGs and omenchatd
    archives, then mounts and lifecycle-tests each DMG with isolated state;
  - can run the packaged local OMENchat smoke;
  - uploads package artifacts and checksums from a read-only build job;
  - publishes tag artifacts only from a dependent `release` environment job
    with narrowly scoped `contents: write` permission.

Configure the repository's `release` GitHub environment with required reviewer
approval before enabling public tag publication. The build job does not receive
write authority or release secrets. All actions use reviewed commit SHAs.

The package workflow downloads the immutable AppImageTool 1.9.1 x86-64 asset,
checks its reviewed SHA-256 before execution, extracts it, and uses the extracted
`AppRun` so it does not depend on FUSE being available. Run
`bash scripts/verify-workflow-security.sh` whenever updating actions, workflow
permissions, or packaging tools.
