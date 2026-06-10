# 20 — Plugin Execution and Capability Safety

This document finishes OMENbrowser_rs plugin support.

## Goal

Plugins should extend browser/messaging behavior without owning the app process or bypassing user permissions.

## Existing foundation

The app already has plugin manifests and typed permission names matching documented capability strings. Execution/discovery is deferred.

## Plugin principles

- Plugin execution is optional.
- Plugins are disabled by default until discovered and trusted/enabled.
- Plugins declare capabilities.
- Capabilities are enforced by the plugin service.
- Plugin failures disable the plugin or mark it degraded, not the whole app.
- Plugins never receive secret identity key material.
- Plugins never call native Reticulum/LXMF objects directly.

## Recommended plugin kinds

### Request-data enricher

Adds fields to a browser request.

Examples:

- session token forwarding;
- OMEN account fields;
- local preference flags.

Must not add secret fields unless user explicitly permits.

### Page post-processor

Transforms fetched page text/document before render.

Examples:

- language translation;
- accessibility cleanup;
- OMEN styling tweaks.

Must preserve unsafe/unknown Micron syntax unless intentionally transforming.

### Message hook

Runs before/after send or on receive.

Examples:

- auto-label;
- local notification;
- spam filter;
- export.

Must not silently send messages.

### Directory hook

Runs on announce ingestion or directory updates.

Examples:

- trust policy suggestion;
- label enrichment;
- known node import.

Should not auto-trust without explicit config.

## Manifest

Suggested manifest format:

```json
{
  "id": "omen.example.plugin",
  "name": "Example Plugin",
  "version": "0.1.0",
  "description": "Example plugin",
  "entry": "plugin.wasm",
  "runtime": "wasm",
  "capabilities": [
    "browser.request.enrich",
    "browser.page.postprocess"
  ],
  "settings_schema": {},
  "enabled_by_default": false
}
```

If using process plugins instead of WASM, entry might be an executable command. WASM is safer for deterministic sandboxing.

## Capability examples

Use explicit names such as:

- `browser.request.enrich`
- `browser.page.postprocess`
- `browser.link.handle`
- `message.before_send`
- `message.after_receive`
- `directory.announce.observe`
- `directory.entry.suggest`
- `storage.plugin_state.read`
- `storage.plugin_state.write`
- `network.none`

Avoid broad capabilities like `all`.

## Execution model options

### WASM sandbox preferred

Pros:

- constrained imports;
- easier permission control;
- deterministic inputs/outputs;
- fewer shell injection risks.

Cons:

- more setup;
- plugins must be compiled to WASM.

### External process fallback

Pros:

- easy to write scripts;
- useful for local power users.

Cons:

- dangerous if not constrained;
- platform differences;
- command injection risk;
- needs timeout/stdin/stdout contract.

If process plugins are supported, require explicit user enablement and store absolute resolved paths.

## Hook contracts

Hooks should use structured JSON input/output.

Example page post-process input:

```json
{
  "address": "...",
  "mime_type": "text/x-micron",
  "text": "...",
  "metadata": {}
}
```

Output:

```json
{
  "text": "...",
  "warnings": []
}
```

Request-data enricher output:

```json
{
  "add": {
    "var_example": "value"
  },
  "warnings": []
}
```

## Security rules

- No plugin receives private identity material.
- No plugin gets arbitrary filesystem access through app APIs.
- Plugin state is stored under plugin-specific data dir.
- Plugin network access is denied unless a future explicit capability exists.
- Plugin stdout/stderr must be size-limited.
- Plugin runtime must have timeout.
- Plugin errors must be logged and surfaced.
- Disable plugin after repeated failures if configured.

## Plugin UI

Plugins panel should show:

- plugin name/id/version;
- enabled/disabled state;
- runtime type;
- capabilities;
- last run status;
- error count;
- settings summary;
- enable/disable action;
- trust warning for process plugins.

## Tests

Add tests for:

- manifest parse;
- invalid manifest rejection;
- capability enforcement;
- disabled plugin skipped;
- timeout handling;
- oversized output rejection;
- request-data merge conflicts;
- page post-process error fallback;
- plugin state path isolation.

## Done when

- Plugins can be discovered and listed.
- Safe hooks can run.
- Capabilities are enforced.
- Plugin failures do not crash OMENbrowser_rs.
