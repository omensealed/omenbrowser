# OMENbrowser_rs Packaging

Repository: <https://github.com/omensealed/omenbrowser>

These scripts build local packages from the current source tree. They do not
delete or overwrite user identities, Reticulum storage, message history, or
OMENchat server homes.

## Alpha Tarball

```sh
bash scripts/alpha-package.sh dist
```

The tarball is the current private-alpha distribution format. It includes:

- `bin/omenbrowser_rs`
- `bin/omenchatd`
- alpha smoke-test helpers
- optional user launcher/service installers
- tester docs

## Debian Package

```sh
bash scripts/package-deb.sh dist
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
