# 17 — MicronPlus, Controls, Links, and Form State

This document finishes interactive Micron/MicronPlus behavior.

## Goal

OMENbrowser_rs must render and interact with Micron pages well enough to replace the Python browser. It should also gracefully support OMEN-specific MicronPlus extensions without breaking ordinary Micron content.

## Existing foundation

The current Micron subsystem already supports:

- document metadata;
- typed render rows;
- inline style parsing;
- links;
- LXMF autolinks;
- field/checkbox/radio controls;
- cell-preserving rows;
- ratatui adapter.

Still needed:

- full partial descriptor parity;
- richer control state;
- click/keyboard hit testing for inline links/controls;
- MicronPlus windows/boxes/inputs/buttons fallback and transforms;
- fixture snapshot tests.

## Subsystem boundaries

Micron parser should produce a semantic document.

Micron renderer should produce terminal-cell render output.

UI should consume hit regions and render rows.

Browser session should own field/control state.

Do not let UI code parse raw Micron syntax.

## Parsed model additions

Add or refine:

```rust
pub struct ControlId(String);

pub enum ControlKind {
    TextInput,
    PasswordInput,
    Checkbox,
    Radio { group: String, value: String },
    Button,
    Submit,
    Hidden,
}

pub struct ControlSpec {
    pub id: ControlId,
    pub name: String,
    pub label: Option<String>,
    pub kind: ControlKind,
    pub default_value: Option<String>,
    pub current_value_hint: Option<String>,
    pub required: bool,
    pub link_target: Option<LinkTarget>,
    pub forwarded_fields: Vec<String>,
}

pub struct HitRegion {
    pub row: u16,
    pub col_start: u16,
    pub col_end: u16,
    pub action: HitAction,
}
```

Actual naming may differ. The important requirement is stable identity for every interactive thing.

## Link handling

Support:

- normal page links;
- destination/path links;
- shorthand links;
- LXMF links;
- download links;
- field-forwarding links;
- internal fragment links;
- unsupported external links as explicit external-open action.

Link activation returns a typed action, not a raw string mutation in UI.

## Field state

`BrowserSession` should own current field values.

Rules:

- Defaults are loaded from parsed controls.
- User edits update session state.
- Reload may reset fields unless preserving state is explicitly intended.
- Link activation may include selected field values.
- Hidden fields are included only if present in page/control model.
- Unknown fields should not be invented.

## Keyboard interaction

Minimum:

- Tab / Shift-Tab cycle controls within page when browser content focus is active.
- Enter activates focused link/button/control.
- Space toggles checkbox/radio.
- Typing edits focused text input.
- Esc leaves control editing.

Do not break existing command input behavior. There must be clear focus ownership between command bar and page controls.

## Mouse interaction

Use renderer-provided `HitRegion`s.

Flow:

```text
Mouse click -> ui::mouse maps to page cell -> HitRegion -> HitAction -> App action -> BrowserSession action
```

Do not approximate inline links by re-parsing visible text from the UI.

## MicronPlus fallback

Unsupported MicronPlus constructs should degrade visibly but safely.

Examples:

- window/box: render as bordered block or plain text section;
- input/button: render as normal controls;
- unsupported style: ignore style, preserve text;
- unsupported action: show disabled control marker;
- malformed syntax: preserve raw text.

## Cell-preserving content

Block art and half-block images are core to OMEN/NomadNet usage.

Rules:

- Do not wrap cell-preserving rows.
- Respect width exactly where possible.
- Preserve shade/block characters.
- Avoid trimming meaningful trailing spaces in art rows unless existing Python behavior does.
- Add snapshot tests at 40, 60, 71, and 80 columns.

## Color policy

Terminal colors vary. Keep a renderer-level color policy:

- parse RGB/terminal color commands into semantic color values;
- ratatui adapter maps semantic color to terminal color;
- unknown/unsupported colors degrade safely;
- theme may adjust background but should not destroy page art.

## Tests

Create fixture files:

```text
tests/fixtures/micron/
  links_basic.mu
  links_forwarded_fields.mu
  controls_text_checkbox_radio.mu
  lxmf_autolinks.mu
  micronplus_boxes.mu
  block_art_40.mu
  block_art_60.mu
  block_art_71.mu
  block_art_80.mu
```

Test:

- parsed document model;
- rendered cell width;
- hit region positions;
- control default state;
- field submit request-data;
- malformed markup preservation;
- snapshot output for key examples.

## Done when

- Links and controls can be activated from keyboard and mouse.
- Form state is submitted through browser service request-data.
- MicronPlus content has graceful fallback.
- Block-art/image-like Micron content survives rendering.
