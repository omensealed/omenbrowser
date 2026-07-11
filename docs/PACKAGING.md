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
