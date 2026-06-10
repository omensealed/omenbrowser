# 09 — Plugins

## Plugin goal

The Python app supports plugin manifests and hook functions. The Rust port should preserve the idea of extensibility, but should not execute arbitrary Python code inside the Rust process.

Use the existing bundled plugins as behavior references, not as direct runtime dependencies.

## Python source

Reference files:

```text
src/omenbrowser/core/plugin_manager.py
bundled_plugins/browser_summary_plugin/plugin.json
bundled_plugins/browser_summary_plugin/plugin.py
bundled_plugins/example_plugin/plugin.json
bundled_plugins/example_plugin/plugin.py
bundled_plugins/micronplus_textui/plugin.json
bundled_plugins/micronplus_textui/plugin.py
```

## Manifest model

Port manifest fields:

```rust
pub struct PluginManifest {
    pub plugin_id: String,
    pub name: String,
    pub version: String,
    pub author: String,
    pub description: String,
    pub entrypoint: String,
    pub min_app_version: String,
    pub permissions: Vec<PluginPermission>,
}
```

For Rust, `entrypoint` may be a command, WASM module, dynamic library, or declarative transform file depending on the plugin system chosen. Do not imply arbitrary local code is safe.

## First-generation safe plugin approach

Implement a typed, capability-gated plugin system.

Recommended plugin types:

1. Built-in Rust plugins.
2. Declarative transform plugins.
3. External command plugins with explicit permission warning.
4. WASM plugins later if needed.

For the first port milestone, built-in plugins plus manifest discovery are enough.

## Hook concepts to preserve

The Python plugins demonstrate these hooks:

- content transform before parse;
- request data enrichment;
- document transform after parse;
- custom row rendering;
- browser page setup;
- browser interaction handling;
- widget state get/set;
- event subscribe/emit.

Rust hook traits can be:

```rust
pub trait ContentTransformer {
    fn transform_markup(&self, markup: &str, context: &BrowserPluginContext) -> anyhow::Result<String>;
}

pub trait RequestDataEnricher {
    fn enrich_request_data(&self, context: &BrowserPluginContext) -> anyhow::Result<BTreeMap<String, String>>;
}

pub trait DocumentTransformer {
    fn transform_document(&self, document: Document, context: &BrowserPluginContext) -> anyhow::Result<Document>;
}

pub trait RowRenderer {
    fn render_row(&self, row: &RenderRow, context: &RenderContext) -> Option<Vec<RenderedRow>>;
}

pub trait InteractionHandler {
    fn handle_interaction(&self, action: PluginAction, context: &mut BrowserPluginContext) -> anyhow::Result<bool>;
}
```

## Capability model

Permissions should be explicit:

- `browser:transform_content`
- `browser:enrich_request_data`
- `browser:render_rows`
- `browser:handle_interaction`
- `runtime:read_status`
- `runtime:request_path`
- `messages:compose`
- `filesystem:read_user_selected`
- `filesystem:write_plugin_data`
- `network:external`

Never grant filesystem or network access implicitly.

## Remote content gate

The Python app has logic around remote plugin gating. Preserve the security concept:

- plugins should not freely transform remote content unless enabled;
- high-risk plugin behavior should be local-only by default;
- the UI should visibly indicate when plugins are active for a page;
- a user should be able to disable plugins globally or per destination later.

## Built-in MicronPlus plugin

`micronplus_textui` should become a built-in Rust module, not a risky third-party plugin requirement.

Required behavior:

- detect MicronPlus markup;
- transform windows/boxes/columns/input/textbox/button/status/live elements;
- render custom rows where needed;
- preserve fallback output for clients that do not understand MicronPlus;
- attach control metadata for UI interactions.

The Rust built-in keeps both lowered Micron fallback markup and a structured MicronPlus widget tree. The tree-backed renderer is the primary path for interactive MicronPlus pages because it can keep non-column live slots, nested scrollboxes/logs, buttons, statuses, and field controls alive across partial refreshes. Partial responses must update only the targeted widget tree slot and must not create parent layout metadata from a fragment that happens to contain columns.

## Built-in OMENchat plugin

`omenchat_lxmf` is the next first-party Rust plugin. It is a native LXMF-backed
room chat client scaffold and must remain capability-limited like MicronPlus. It
must not require Python plugin execution. The client/server plan lives in
`docs/25-omenchat-plugin-server-plan.md`.

## Plugin manager behavior

Preserve these manager operations:

- load manifest;
- install from local folder;
- discover installed plugins;
- enable/disable;
- remove;
- show warnings;
- report activation errors.

In Rust, activation may mean registering a built-in plugin by ID or launching an external sandboxed plugin process. Keep the UI behavior stable even if the execution backend changes.
