# OMENbrowser_rs Packaging

Repository: <https://github.com/omensealed/omenbrowser>

These scripts build local packages from the current source tree. They do not
delete or overwrite user identities, Reticulum storage, message history, or
OMENchat server homes.

Public alpha packages must use the live-tested browser feature set:
`chat-client-rns-clean` with `native-network:on`. The packaging scripts verify
this through the browser `--version` output. Server packages must report
`live-reticulum:on` from `omenchatd --version`.

Do not cut the next public package release until the clean Reticulum 0.6 LXMF
path has a live smoke pass for direct sends, propagation sync, tickets, and
attachments. The packaging scripts can build local artifacts before then, but a
GitHub release tag should wait for that LXMF parity check.

Linux packages must be built on the oldest glibc baseline we intend to
support. Building on a newer rolling distro can produce binaries that fail on
older but still-supported systems with errors like `GLIBC_2.39 not found` or
`GLIBC_2.43 not found`.

The GitHub package workflow builds inside a Debian 11 (`bullseye`) Rust
container so the release binaries target `GLIBC_2.31`. That is a practical
compatibility floor for systems in the Ubuntu 20.04 / Debian 11 / Linux Mint 20
era and newer, without changing the OMENbrowser_rs source code or statically
linking glibc.

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

## Alpha Tarball

```sh
bash scripts/alpha-package.sh dist
```

The tarball is the current public-alpha distribution format. It includes:

- `bin/omenbrowser_rs`
- `bin/omenchatd`
- alpha smoke-test helpers
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

## GitHub Actions

Two workflows are included:

- `.github/workflows/ci.yml`
  - runs on pushes and pull requests;
  - installs Rust and Linux build dependencies;
  - runs `bash scripts/alpha-check.sh quick`;
  - syntax-checks package and installer scripts.
- `.github/workflows/package.yml`
  - manual `workflow_dispatch` only;
  - builds the alpha tarball, `.deb`, and AppImage;
  - can run the packaged local OMENchat smoke;
  - uploads package artifacts and checksums.

The package workflow downloads `appimagetool` on the runner, extracts it, and
uses the extracted `AppRun` so it does not depend on FUSE being available.
