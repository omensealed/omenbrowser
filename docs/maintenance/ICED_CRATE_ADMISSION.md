# Iced crate admission

The desktop UI uses Iced 0.14 with defaults disabled and explicit product
features. This file is an admission boundary, not a backlog of libraries to
enable.

## Canonical products

- `desktop-product` uses Iced, bounded static image decoding, optional bounded
  GIF preview through `iced_gif`, and the existing QR encoder.
- `desktop-product-static-media` keeps the same networking and OMENchat
  behavior without animated GIF decoding.
- TUI and standalone `omenchatd` must not resolve Iced.

`scripts/verify-product-features.sh` enforces one Iced version and rejects
development-only adjuncts from canonical release graphs.

## Optional crates

| Crate | Product status | Admission rule |
|---|---|---|
| `iced` | active | Desktop framework; replacement is out of scope. |
| `iced_gif` | animated product only | Input and cache bounds remain mandatory; static-media is the tested removal path. |
| `qrcode` through Iced | active | Bounded OMENchat invitation text only; no camera or image decoding. |
| `iced_aw` | not in canonical products | Requires a concrete UI need, feature review, tests, and measurements. |
| `iced_drop` | not in canonical products | Requires an explicit bounded import/attachment workflow. |
| `iced_anim` | not in canonical products | Reduced-motion and ownership design required before admission. |
| `iced_table` | not in canonical products | Requires measured large-row need and bounded state. |

Do not add a complete icon-font crate for one glyph. Desktop icons use the
curated constants and font fallback in `src/desktop/icons.rs` and
`src/desktop/fonts.rs`.

## Required checks

```bash
bash scripts/verify-product-features.sh
bash scripts/smoke_iced_dependency_tree.sh
bash scripts/smoke_iced_features.sh
cargo test --locked --no-default-features --features desktop-product
cargo test --locked --no-default-features \
  --features desktop-product-static-media
```

Any activation also requires strict Clippy, native Windows/macOS compile
evidence, dependency audit/deny, and UI resource measurements appropriate to
the new behavior.

## Design constraints

An admitted UI dependency must not introduce unbounded animation, background
polling, detached tasks, arbitrary file access, or a second networking state
machine. Existing Tokio, Iced, storage, queue, and shutdown ownership remain
authoritative.
