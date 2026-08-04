# Packaging

## Release Archive

```bash
bash scripts/release-package.sh
```

The archive includes:

- browser binary;
- `omenchatd` binary;
- tester docs;
- helper scripts;
- checksums;
- systemd user-service template.

The user-service template includes `UMask=0077`. Its installer creates or
repairs only the selected `OMENCHATD_HOME` as `0700`, leaves its parent
unchanged, and preserves the home on uninstall. The server binary also enforces
private managed modes when run outside systemd.

## Local `.deb`

```bash
bash scripts/package-deb.sh dist
```

Requires `dpkg-deb`.

## AppImage

```bash
bash scripts/package-appimage.sh dist
```

Requires `appimagetool`.

## GitHub Actions

The repository includes:

- `.github/workflows/ci.yml` for the quick gate;
- `.github/workflows/package.yml` for manual package artifact builds.
