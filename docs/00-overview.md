# 00 — OMENbrowser_rs Overview

## Project goal

`OMENbrowser_rs` is the Rust port and evolution of the Python `OMENbrowser`. It is a low-resource NomadNet browser and LXMF client designed for users who should not have to manually install and orchestrate Reticulum, LXMF, and NomadNet from a command line.

The Rust port must preserve the Python application's practical workflow while improving structure, responsiveness, and UI layout.

## Product identity

OMENbrowser is not a generic web browser. It is a terminal-native browser/client for the Reticulum/NomadNet world with strong support for Micron rendering, LXMF messaging, directory discovery, and constrained-network UX.

The app should feel like:

- a browser;
- a messenger;
- a Reticulum status console;
- a Micron renderer reference implementation;
- an onboarding bridge for new users.

It should not become:

- a node-hosting admin panel;
- an Electron app;
- an HTML browser;
- a classic NomadNet clone with the same UI limits;
- a fragile wrapper that only works when all external daemons are manually launched.

## Current Python source summary

The Python project already implements the core app model:

- Textual-based UI shell;
- Micron parser and renderer;
- Reticulum/LXMF adapter with mock fallback;
- browser sessions with history, request fields, cache, downloads, and partials;
- message store and messaging service;
- directory discovery and trust/saved entry system;
- interface profile management for Reticulum config;
- diagnostics/logging;
- plugin manifests and hooks;
- bundled MicronPlus text UI plugin.

The port must treat the Python source as the behavioral reference.

## Major Rust UI improvement

The Python UI has a top-level `TabbedContent` for Browser, Messages, Directory, Settings, Interfaces, Diagnostics, Logs, and Plugins. That is useful, but the Browser and Messages experiences are single-instance.

The Rust version must improve this by supporting:

- multiple browser tabs inside the Browser workspace;
- multiple conversation tabs/windows inside the Messages workspace;
- explicit new/close/rename/switch actions;
- browser tabs with independent history, address, field state, partial refresh state, and loading cancellation;
- conversation tabs with independent selected peer, composer state, attachments, delivery mode, unread state, and scrollback;
- shared global services and runtime state tied cleanly into all tabs.

## Top-level milestones

1. Scaffold Rust workspace and docs.
2. Port core data models.
3. Port Micron parser and cell renderer.
4. Port mock runtime adapter.
5. Port browser session/cache/partials.
6. Port message store and directory service.
7. Build multi-tab workspace UI.
8. Add Reticulum/LXMF bridge.
9. Add plugin capability layer and MicronPlus behavior.
10. Package and test on Linux first.

## Glossary

- **Micron**: NomadNet markup language rendered as styled terminal cells.
- **MicronPlus**: OMEN-specific extension layer for richer text UI blocks/widgets while retaining graceful fallback.
- **Browser tab**: One independent NomadNet browsing session.
- **Conversation tab**: One independent peer conversation and composer context.
- **Runtime adapter**: Boundary between app services and live/mock Reticulum/LXMF operations.
- **Directory entry**: Known node, peer, or propagation destination with trust/save/delivery metadata.
- **Propagation node**: LXMF propagation destination used for async message delivery.

