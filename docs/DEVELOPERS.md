# Developer Notes

## Main Crates

- Root crate: `omenbrowser_rs`.
- Standalone server crate: `src/server`.

`omenchatd` must remain movable and independent. Do not import browser modules
from the server crate.

## Feature Flags

Common browser build:

```bash
cargo build --features chat-client-rns
```

Server live RNS build:

```bash
cargo build --manifest-path src/server/Cargo.toml --features live-rns-net
```

## Checks

```bash
cargo fmt --check
cargo test --features chat-client-rns
cargo fmt --manifest-path src/server/Cargo.toml --check
cargo test --manifest-path src/server/Cargo.toml --features live-rns-net
```

Fast pre-share gate:

```bash
bash scripts/alpha-check.sh quick
```

## Source Areas

- `src/micron` - Micron parser/rendering.
- `src/browser` - browsing, cache, partials, MicronPlus helpers.
- `src/messaging` - LXMF conversations and store.
- `src/runtime` - runtime abstraction and native networking.
- `src/chat` - OMENchat client plugin.
- `src/server` - standalone `omenchatd`.
- `src/desktop` and `src/ui` - Iced UI shell and widgets.

## Dependency Policy

Prefer published crates. Do not vendor or patch networking crates in this
repository unless there is an explicit release-blocking reason and a follow-up
plan to upstream or remove the patch.
