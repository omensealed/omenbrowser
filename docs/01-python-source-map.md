# 01 — Python Source Map

This file maps the archived Python implementation to the Rust port. Use it before porting each subsystem.

## Archive structure

```text
OMENbrowser_install/
  README.txt
  install.py
  install.sh
  install.ps1
  payload/
    manifest.json
    OMENbrowser/
      README.md
      pyproject.toml
      resources/
      src/omenbrowser/
    bundled_plugins/
      browser_summary_plugin/
      example_plugin/
      micronplus_textui/
```

## Python dependencies

The Python project uses:

- `textual` for TUI/desktop-like UI;
- `rich` for styled terminal text;
- `platformdirs` for app data paths;
- optional `rns` and `lxmf` for Reticulum/LXMF;
- `pytest` and `pillow` in tests/development;
- `pyinstaller` for packaging.

The Rust port should not directly mirror these libraries, but should preserve their responsibilities.

## Core modules

### `core/models.py`

Behavioral source for Rust structs:

- `IdentityProfile`
- `RuntimeStatus`
- `Bookmark`
- `BrowserPage`
- `DownloadedFile`
- `BrowserAddress`
- `MessageSummary`
- `AttachmentSummary`
- `ConversationThread`
- `DirectoryEntry`
- `PluginManifest`
- `ReticulumInterfaceProfile`

Port these early. Make them `serde` serializable where persistence requires it.

### `core/settings.py`

Source for app settings defaults and robust load/save behavior.

Important behaviors:

- default theme is dark;
- runtime backend defaults to `auto`;
- Reticulum instance mode defaults to managed;
- announcement and periodic LXMF sync have settings;
- settings load must tolerate missing or corrupted files;
- corrupted settings should be backed up, not overwritten silently.

### `core/app_data.py`

Source for app path layout.

Important behaviors:

- build platform-appropriate app paths;
- ensure directories exist;
- manage Reticulum config and storage folders;
- preserve legacy migrations if applicable;
- keep managed identity paths deterministic and safe.

### `core/identity.py`

Source for identity workflow.

Important behaviors:

- create new managed identity;
- attach existing identity without copying;
- import identity copy into managed storage;
- backup before overwrite;
- hash identity file bytes for display/profile identity;
- export timestamped backup.

### `core/plugin_manager.py`

Source for plugin installation/discovery/activation concepts.

Important behaviors:

- plugin manifest JSON;
- local plugin folder installation;
- enable/disable registry;
- plugin permission warning;
- hooks loaded from plugin entrypoint.

Rust should not execute arbitrary Python plugins in-process. Use this as a behavior model for a typed plugin/capability layer.

## Renderer modules

### `renderer/micron.py`

This is one of the highest-priority files.

It defines:

- parser state;
- inline styles;
- links;
- fields and controls;
- render fragments;
- document rows;
- cells;
- rendered rows;
- parsing of Micron commands;
- alignment;
- wrapping;
- conversion to Rich text.

Rust must port the parser and renderer into a terminal cell grid. Do not use proportional text assumptions. Preserve half-block image/art behavior.

## Service modules

### `services/browser.py`

Source for browser session behavior.

Important features:

- address parsing;
- relative URL resolution;
- current destination tracking;
- history/back/forward/reload;
- default request data and field forwarding;
- available links;
- downloads;
- clearweb URL detection;
- cache key behavior;
- render state updates.

In Rust, one `BrowserSession` should exist per browser tab.

### `services/browser_partials.py`

Source for partial refresh behavior.

Important features:

- parse partial descriptors;
- extract partial specs from markup;
- compose fetched fragments back into page markup.

In Rust, partial refresh state belongs to the browser tab that owns the page.

### `services/cache.py`

Source for page cache behavior.

Important features:

- hashed cache keys;
- TTL-based storage;
- metadata storage;
- cache load/delete/clean.

### `services/message_store.py`

Source for persistent per-peer threads.

Important features:

- one JSON file per peer thread;
- append incoming/outgoing messages;
- unread counts;
- delivery status update by message ID;
- peer label update;
- pending reconciliation.

### `services/messages.py`

Source for higher-level messaging behavior.

Important features:

- label resolution from directory;
- ingest runtime messages;
- attachment summary conversion;
- compose direct/propagated messages;
- update outbound status;
- reconcile pending.

In Rust, conversation tabs should use this service but keep independent compose state.

### `services/directory.py`

Source for node/peer/propagation directory behavior.

Important features:

- load/save directory entries;
- transient announce handling;
- saved/trusted state;
- trust levels;
- identify-on-connect;
- preferred delivery;
- associated node/peer/propagation relationships;
- sorted and filtered views.

### `services/interfaces.py`

Source for Reticulum interface profile editing.

Important features:

- parse and render managed Reticulum config;
- profile types including TCP, I2P, and RNode/LoRa style settings;
- gateway presets;
- enable/connectable toggles;
- apply config;
- detect local I2P router.

### `services/diagnostics.py`

Source for runtime/app diagnostics snapshot.

### `services/propagation_probe.py`

Source for propagation debugging and diagnosis summaries.

## Protocol modules

### `protocols/mock_adapter.py`

Essential for Rust parity. Port this first before live Reticulum.

Mock adapter provides:

- fake runtime status;
- mock pages;
- mock downloads;
- mock messages;
- fake sends;
- fake directory and propagation state;
- debug/status callbacks.

### `protocols/reticulum_adapter.py`

Largest and most complex Python module. It handles:

- Reticulum initialization;
- LXMF router setup;
- identity attachment;
- announce handling;
- destination remembering;
- path requests and path warming;
- sibling aspect/path alias behavior;
- NomadNet page fetch over links;
- downloads;
- direct and propagated LXMF sends;
- propagation sync;
- propagation node selection;
- diagnostics and snapshots;
- directory candidates;
- destination inspection.

Do not start by rewriting this natively. First define a Rust trait and mock implementation. Then implement an adapter bridge that can use subprocess/sidecar integration if needed.

## UI module

### `ui/app.py`

The Python UI contains:

- `MicronView` scrollable renderer widget;
- `ConversationView` scrollable thread widget;
- `OMENBrowserApp`, a large Textual app class;
- top-level tabs for browser/messages/directory/etc.;
- status bar;
- identity, runtime, directory, browser, message, plugin, interfaces, diagnostics, and log wiring;
- background task management and cancellation;
- plugin hook dispatch.

Rust must not create a single 4,000-line UI file. Split into:

```text
ui/app.rs
ui/layout.rs
ui/events.rs
ui/browser_workspace.rs
ui/messages_workspace.rs
ui/directory_panel.rs
ui/settings_panel.rs
ui/interfaces_panel.rs
ui/diagnostics_panel.rs
ui/log_panel.rs
ui/plugins_panel.rs
ui/widgets/micron_view.rs
ui/widgets/conversation_view.rs
ui/theme.rs
```

## Bundled plugins

### `browser_summary_plugin`

Behavior model for:

- content transformer hook;
- request data enricher;
- document transformer;
- custom row renderer.

### `example_plugin`

Minimal plugin manifest/hook example.

### `micronplus_textui`

Important behavior source for OMEN-specific UI markup.

It transforms MicronPlus tags into fallback-friendly Micron and can render custom rows/widgets. The Rust port should implement MicronPlus behavior as a first-party optional module before exposing arbitrary third-party plugins.

