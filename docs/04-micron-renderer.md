# 04 — Micron Renderer

## Renderer goal

The Micron renderer is a core reason OMENbrowser exists. MeshChat and other clients can mishandle converted image/half-block Micron output. `OMENbrowser_rs` must treat Micron as a styled terminal cell-grid format and render it predictably.

Do not render Micron by converting it to HTML. Do not rely on proportional layout. Do not collapse spacing that may be meaningful art.

## Python source

Reference file:

```text
src/omenbrowser/renderer/micron.py
```

It defines:

- `Alignment`
- `TextStyle`
- `ParserState`
- `LinkAction`
- `TextSpan`
- `RenderFragment`
- `FieldControl`
- `RenderRow`
- `Document`
- `Cell`
- `RenderedRow`
- `parse_micron`
- `parse_line`
- `parse_inline_and_controls`
- `parse_inline`
- `parse_control`
- `parse_partial`
- `render_document`
- `render_row`
- wrapping helpers
- color conversion
- Rich conversion helpers

The Rust port should keep equivalent concepts but does not need equivalent names everywhere.

## Recommended Rust data model

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Alignment { Left, Center, Right }

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextStyle {
    pub fg: Option<ColorSpec>,
    pub bg: Option<ColorSpec>,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub dim: bool,
    pub reverse: bool,
}

#[derive(Clone, Debug)]
pub struct ParserState {
    pub align: Alignment,
    pub style: TextStyle,
    pub dark_theme: bool,
}

#[derive(Clone, Debug)]
pub struct LinkAction {
    pub target: String,
    pub fields: Vec<String>,
}

#[derive(Clone, Debug)]
pub enum Fragment {
    Text(TextSpan),
    Control(FieldControl),
    PluginMarker(PluginMarker),
}

#[derive(Clone, Debug)]
pub struct RenderRow {
    pub fragments: Vec<Fragment>,
    pub align: Alignment,
    pub metadata: BTreeMap<String, serde_json::Value>,
}

#[derive(Clone, Debug)]
pub struct Cell {
    pub ch: char,
    pub style: TextStyle,
    pub link: Option<LinkAction>,
    pub control: Option<ControlRef>,
    pub plugin_data: Option<serde_json::Value>,
}

#[derive(Clone, Debug)]
pub struct RenderedRow {
    pub cells: Vec<Cell>,
}
```

## Parser requirements

The parser must:

- preserve text content and significant spaces;
- track parser state across lines where Micron semantics require it;
- support foreground/background color commands used in existing OMEN Micron content;
- support style reset;
- support links and link targets;
- support request links with forwarded fields;
- support input-like controls used by the Python parser;
- support alignment directives;
- autolink obvious URLs if Python behavior does;
- tolerate malformed markup without panics;
- output diagnostics for malformed controls if useful;
- allow document transformers before rendering;
- allow row renderers after parsing but before display.

## Rendering requirements

The renderer must:

- render to fixed-width rows;
- preserve half-block characters and other Unicode glyphs;
- not split combining sequences incorrectly when practical;
- handle wide characters conservatively;
- align rows left/center/right;
- wrap text rows without destroying cell styles;
- expose link/control hit targets to the UI;
- expose focusable controls in document order;
- allow scroll views to request visible line ranges efficiently.

## Width handling

Use Unicode width handling, for example with `unicode-width`, but be careful with block art. A block glyph that displays as one terminal cell should be treated as width 1.

When width is uncertain, prefer preserving layout over aggressive wrapping.

MicronPlus inputs are rendered with viewport context. Explicit `width=`, `cols=`, or `size=` values remain fixed character widths. A MicronPlus `[input]` or `[textbox]` without an explicit width should fill the available rendered row for its viewport or column, while plain Micron controls remain fixed-width cell controls.

## Half-block image/art fidelity

OMEN image-to-Micron conversion often uses Unicode half blocks, shaded blocks, braille-like density characters, and precise spaces. Tests must include fixtures with:

- `▀`, `▄`, `█`, `▌`, `▐`;
- shaded blocks `░`, `▒`, `▓`;
- color changes across a single row;
- background color changes;
- rows exactly 40, 60, 71, and 80 cells wide;
- centered and bordered art.

No renderer change is accepted if it corrupts these fixtures.

## MicronPlus handling

The archived plugin `bundled_plugins/micronplus_textui/plugin.py` implements OMEN-specific tags for richer text UI. Rust should port that behavior as a first-party optional transform.

Supported behavior should include:

- detecting MicronPlus markup;
- parsing tag-like lines;
- preserving a structured widget tree for MicronPlus documents, not only lowered Micron fallback text;
- transforming windows, boxes, inputs, textboxes, buttons, status, live widgets, and columns into fallback-friendly render rows;
- rendering nested rows inside boxes, windows, scrollboxes, logs, live slots, and columns;
- keeping unknown attributes non-fatal;
- passing widget/control metadata to the UI.

The structured tree is required for pages such as `nomadnet-m.mu`, where live slots can appear outside the main column group and later receive nested windows, scrollboxes, statuses, and buttons. Partial refreshes must update the matching tree slot without replacing unrelated page content or clobbering sibling live slots. Column-layout pages may still fall back to normal Micron rendering on narrow viewports.

Do not require third-party plugins to get basic MicronPlus behavior working.

## Renderer test strategy

Every parser feature needs a test. Snapshot tests should compare rendered cell text and selected style/link/control metadata.

Minimum tests:

1. plain text parse;
2. color state parse;
3. bold/underline/reset;
4. links;
5. request links with fields;
6. input controls;
7. alignment;
8. wrapping with style preservation;
9. half-block art no corruption;
10. malformed markup no panic;
11. MicronPlus box/window transform;
12. MicronPlus input/button control metadata.
13. MicronPlus live partials updating multiple structured slots without leaking raw tags.

## UI integration

The `MicronView` equivalent should receive a `Document` or rendered rows and display only visible rows. It should support:

- scroll up/down/page/home/end;
- focus next/previous interactive target;
- activate link/control;
- mouse click hit testing;
- field editing where applicable;
- plugin interaction dispatch.

The renderer must not fetch pages or mutate browser history. It only parses/renders and reports user interactions.
