use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

pub const DEFAULT_FG_DARK: &str = "ccc";
pub const DEFAULT_BG: &str = "default";

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum Alignment {
    Left,
    Center,
    Right,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct TextStyle {
    pub fg: Option<String>,
    pub bg: Option<String>,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub dim: bool,
    pub reverse: bool,
}

impl Default for TextStyle {
    fn default() -> Self {
        Self {
            fg: Some(DEFAULT_FG_DARK.into()),
            bg: None,
            bold: false,
            italic: false,
            underline: false,
            dim: false,
            reverse: false,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct LinkAction {
    pub target: String,
    pub fields: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct TextSpan {
    pub text: String,
    pub style: TextStyle,
    pub link: Option<LinkAction>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct FieldControl {
    pub kind: String,
    pub name: String,
    pub value: String,
    pub label: String,
    pub width: usize,
    pub masked: bool,
    pub prechecked: bool,
    pub style: TextStyle,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum Fragment {
    Span(TextSpan),
    Control(FieldControl),
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum RowKind {
    Blank,
    Text,
    Heading,
    Divider,
    Partial,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct RenderRow {
    pub kind: RowKind,
    pub depth: usize,
    pub fragments: Vec<Fragment>,
    pub align: Alignment,
    pub base_style: TextStyle,
    pub divider: char,
    pub cell_preserving: bool,
    pub partial: Option<BTreeMap<String, serde_json::Value>>,
    pub raw: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Document {
    pub rows: Vec<RenderRow>,
    pub metadata: BTreeMap<String, String>,
}

#[derive(Clone, Debug)]
pub struct ParserState {
    literal: bool,
    depth: usize,
    align: Alignment,
    default_align: Alignment,
    default_fg: String,
    default_bg: Option<String>,
    style: TextStyle,
}

impl Default for ParserState {
    fn default() -> Self {
        Self {
            literal: false,
            depth: 0,
            align: Alignment::Left,
            default_align: Alignment::Left,
            default_fg: DEFAULT_FG_DARK.into(),
            default_bg: None,
            style: TextStyle::default(),
        }
    }
}

pub fn parse_micron(markup: &str) -> Document {
    let mut state = ParserState::default();
    let mut rows = Vec::new();
    let mut metadata = BTreeMap::new();

    for raw_line in markup.lines() {
        if let Some(meta) = raw_line.strip_prefix("#!") {
            if let Some((key, value)) = meta.split_once('=') {
                apply_metadata_directive(key, value, &mut state, &mut metadata);
            }
            continue;
        }

        if let Some((key, value)) = bare_metadata_directive(raw_line) {
            apply_metadata_directive(key, value, &mut state, &mut metadata);
            continue;
        }

        if let Some(row) = parse_line(raw_line, &mut state) {
            rows.push(row);
        }
    }

    Document { rows, metadata }
}

fn parse_line(line: &str, state: &mut ParserState) -> Option<RenderRow> {
    if line == "`=" {
        state.literal = !state.literal;
        return None;
    }

    if state.literal {
        return Some(RenderRow {
            kind: RowKind::Text,
            depth: state.depth,
            fragments: vec![Fragment::Span(TextSpan {
                text: if line == "\\`=" { "`=" } else { line }.into(),
                style: state.style.clone(),
                link: None,
            })],
            align: state.align,
            base_style: state.style.clone(),
            divider: '─',
            cell_preserving: true,
            partial: None,
            raw: line.into(),
        });
    }

    if line.is_empty() {
        return Some(blank_row(state, line));
    }

    let mut pre_escape = false;
    let mut logical = line;
    if let Some(stripped) = logical.strip_prefix('\\') {
        logical = stripped;
        pre_escape = true;
    }

    if logical.starts_with('#') {
        return None;
    }

    if let Some(style_color) = bare_color_directive(logical) {
        state.style.fg = Some(style_color.to_string());
        return None;
    }

    if let Some((style_color, rest)) = color_prefixed_line(logical) {
        state.style.fg = Some(style_color.to_string());
        logical = rest;
        if logical.is_empty() {
            return None;
        }
    }

    if let Some(raw_partial) = logical.strip_prefix("`{") {
        return Some(parse_partial(raw_partial, state, line));
    }

    if logical.starts_with('<') && !logical.starts_with("<<") {
        state.depth = 0;
        return if logical.len() > 1 {
            parse_line(&logical[1..], state)
        } else {
            Some(blank_row(state, line))
        };
    }

    if logical.starts_with('>') {
        let depth = logical.chars().take_while(|ch| *ch == '>').count();
        state.depth = depth;
        let heading_text = logical.trim_start_matches('>');
        let mut style = state.style.clone();
        match depth {
            1 => {
                style.fg = Some("222".into());
                style.bg = Some("bbb".into());
            }
            2 => {
                style.fg = Some("111".into());
                style.bg = Some("999".into());
            }
            _ => {
                style.fg = Some("000".into());
                style.bg = Some("777".into());
            }
        }
        let fragments =
            parse_inline_and_controls_with_style(heading_text, state, style.clone(), false);
        return Some(RenderRow {
            kind: RowKind::Heading,
            depth,
            fragments,
            align: state.align,
            base_style: style,
            divider: '─',
            cell_preserving: true,
            partial: None,
            raw: line.into(),
        });
    }

    if logical.starts_with('-') {
        let divider = if logical.chars().count() == 2 {
            logical.chars().nth(1).unwrap_or('─')
        } else {
            '─'
        };
        return Some(RenderRow {
            kind: RowKind::Divider,
            depth: state.depth,
            fragments: Vec::new(),
            align: state.align,
            base_style: state.style.clone(),
            divider,
            cell_preserving: true,
            partial: None,
            raw: line.into(),
        });
    }

    let fragments = parse_inline_and_controls(logical, state, pre_escape);
    if fragments.is_empty() {
        return None;
    }
    let cell_preserving = fragments.iter().any(|fragment| match fragment {
        Fragment::Span(span) => span
            .text
            .chars()
            .any(|ch| matches!(ch, '▀' | '▄' | '█' | '▌' | '▐' | '░' | '▒' | '▓')),
        Fragment::Control(_) => false,
    });

    Some(RenderRow {
        kind: RowKind::Text,
        depth: state.depth,
        fragments,
        align: state.align,
        base_style: state.style.clone(),
        divider: '─',
        cell_preserving,
        partial: None,
        raw: line.into(),
    })
}

fn blank_row(state: &ParserState, raw: &str) -> RenderRow {
    RenderRow {
        kind: RowKind::Blank,
        depth: state.depth,
        fragments: Vec::new(),
        align: state.align,
        base_style: state.style.clone(),
        divider: '─',
        cell_preserving: true,
        partial: None,
        raw: raw.into(),
    }
}

fn apply_metadata_directive(
    key: &str,
    value: &str,
    state: &mut ParserState,
    metadata: &mut BTreeMap<String, String>,
) {
    let key = key.trim().to_string();
    let value = value.trim().to_string();
    if key == "fg" {
        state.default_fg = value.clone();
        state.style.fg = Some(value.clone());
    } else if key == "bg" {
        state.default_bg = Some(value.clone());
        state.style.bg = Some(value.clone());
    } else if key == "c" {
        state.align = if value == "1" {
            Alignment::Center
        } else {
            Alignment::Left
        };
        state.default_align = state.align;
    }
    metadata.insert(key, value);
}

fn bare_metadata_directive(line: &str) -> Option<(&str, &str)> {
    let (key, value) = line.split_once('=')?;
    let key = key.trim();
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    matches!(key, "fg" | "bg" | "c").then_some((key, value))
}

fn bare_color_directive(line: &str) -> Option<&str> {
    let candidate = line.trim();
    (candidate.len() == 3 && is_legacy_color_code(candidate)).then_some(candidate)
}

fn color_prefixed_line(line: &str) -> Option<(&str, &str)> {
    let prefix = line.get(..3)?;
    if !is_legacy_color_code(prefix) {
        return None;
    }
    let rest = line.get(3..)?;
    if rest.is_empty() {
        return Some((prefix, rest));
    }
    if rest.starts_with('=') {
        return None;
    }
    Some((prefix, rest))
}

fn is_legacy_color_code(value: &str) -> bool {
    let mut chars = value.chars();
    let first = chars.next();
    let all_hex = value.len() == 3 && value.chars().all(|ch| ch.is_ascii_hexdigit());
    if !all_hex {
        return false;
    }
    value.chars().any(|ch| ch.is_ascii_digit())
        || first.is_some_and(|first| value.chars().all(|ch| ch == first))
}

fn parse_partial(raw: &str, state: &ParserState, original: &str) -> RenderRow {
    let mut partial = BTreeMap::new();
    let descriptor = raw.trim_end_matches('}');
    partial.insert("raw".into(), serde_json::Value::String(descriptor.into()));
    let target = descriptor
        .split([' ', '|', ';'])
        .find(|part| !part.is_empty())
        .unwrap_or("invalid");
    partial.insert("url".into(), serde_json::Value::String(target.into()));
    RenderRow {
        kind: RowKind::Partial,
        depth: state.depth,
        fragments: Vec::new(),
        align: state.align,
        base_style: state.style.clone(),
        divider: '─',
        cell_preserving: true,
        partial: Some(partial),
        raw: original.into(),
    }
}

fn parse_inline_and_controls(
    line: &str,
    state: &mut ParserState,
    pre_escape: bool,
) -> Vec<Fragment> {
    parse_inline_and_controls_with_style(line, state, state.style.clone(), pre_escape)
}

fn parse_inline_and_controls_with_style(
    line: &str,
    state: &mut ParserState,
    mut current_style: TextStyle,
    pre_escape: bool,
) -> Vec<Fragment> {
    let mut fragments = Vec::new();
    let mut buffer = String::new();
    let chars: Vec<char> = line.chars().collect();
    let mut i = 0;
    let mut escape = pre_escape;

    while i < chars.len() {
        let ch = chars[i];
        if ch == '\\' {
            if escape {
                buffer.push(ch);
                escape = false;
            } else {
                escape = true;
            }
            i += 1;
            continue;
        }

        if ch != '`' || escape {
            buffer.push(ch);
            escape = false;
            i += 1;
            continue;
        }

        flush_text(&mut fragments, &mut buffer, &current_style);
        i += 1;
        if i >= chars.len() {
            // Python/NomadNet Micron commonly writes commands as delimited
            // runs, e.g. `_`[label`:/target]`_`. The final delimiter is
            // syntax, not visible text.
            break;
        }

        match chars[i] {
            ' ' | '\t' => {
                // A command delimiter may be followed by whitespace between
                // inline elements. Preserve the whitespace but not the
                // delimiter itself.
                buffer.push(chars[i]);
                i += 1;
            }
            '_' => {
                current_style.underline = !current_style.underline;
                state.style.underline = current_style.underline;
                i += 1;
            }
            '!' => {
                current_style.bold = !current_style.bold;
                state.style.bold = current_style.bold;
                i += 1;
            }
            '*' => {
                current_style.italic = !current_style.italic;
                state.style.italic = current_style.italic;
                i += 1;
            }
            '`' => {
                current_style = TextStyle {
                    fg: Some(state.default_fg.clone()),
                    bg: state.default_bg.clone(),
                    ..TextStyle::default()
                };
                state.style = current_style.clone();
                state.align = state.default_align;
                while i < chars.len() && chars[i] == '`' {
                    i += 1;
                }
            }
            '=' => {
                if let Some(next) = chars.get(i + 1) {
                    buffer.push(*next);
                    i += 2;
                } else {
                    i += 1;
                }
            }
            'F' | 'B' => {
                let color_start = i + 1;
                let (color, consumed) = parse_color(&chars[color_start..]);
                if consumed == 0 {
                    buffer.push('`');
                    buffer.push(chars[i]);
                    i += 1;
                } else {
                    if chars[i] == 'F' {
                        current_style.fg = Some(color);
                        state.style.fg = current_style.fg.clone();
                    } else {
                        current_style.bg = if color == DEFAULT_BG {
                            None
                        } else {
                            Some(color)
                        };
                        state.style.bg = current_style.bg.clone();
                    }
                    i = color_start + consumed;
                }
            }
            'f' => {
                current_style.fg = Some(state.default_fg.clone());
                state.style.fg = current_style.fg.clone();
                i += 1;
            }
            'b' => {
                current_style.bg = state.default_bg.clone();
                state.style.bg = current_style.bg.clone();
                i += 1;
            }
            'g' => {
                if i + 2 < chars.len() {
                    current_style.fg = Some(format!("g{}{}", chars[i + 1], chars[i + 2]));
                    state.style.fg = current_style.fg.clone();
                    i += 3;
                } else {
                    i += 1;
                }
            }
            'a' if chars.get(i + 1) == Some(&'=') => {
                if let Some(end) = find_sequence(&chars, i + 2, &['`', 'a']) {
                    let shorthand = chars[i + 2..end].iter().collect::<String>();
                    let (target, label) = shorthand
                        .split_once('|')
                        .map(|(target, label)| (target.to_string(), label.to_string()))
                        .unwrap_or_else(|| (shorthand.clone(), shorthand));
                    fragments.push(Fragment::Span(TextSpan {
                        text: label,
                        style: current_style.clone(),
                        link: Some(LinkAction {
                            target,
                            fields: Vec::new(),
                        }),
                    }));
                    i = end + 2;
                } else {
                    buffer.push_str("`a");
                    i += 1;
                }
            }
            'c' | 'l' | 'r' | 'a' => {
                state.align = match chars[i] {
                    'c' => Alignment::Center,
                    'l' => Alignment::Left,
                    'r' => Alignment::Right,
                    _ => state.default_align,
                };
                i += 1;
            }
            '[' => {
                if let Some(end) = find_char(&chars, i + 1, ']') {
                    let raw = chars[i + 1..end].iter().collect::<String>();
                    if let Some(span) = parse_link(&raw, &current_style) {
                        fragments.push(Fragment::Span(span));
                    } else {
                        buffer.push_str("`[");
                        buffer.push_str(&raw);
                        buffer.push(']');
                    }
                    i = end + 1;
                } else {
                    buffer.push_str("`[");
                    i += 1;
                }
            }
            '<' => {
                if let Some(end) = find_char(&chars, i + 1, '>') {
                    let raw = chars[i + 1..end].iter().collect::<String>();
                    if let Some(control) = parse_control(&raw, &current_style) {
                        fragments.push(Fragment::Control(control));
                    } else {
                        buffer.push_str("`<");
                        buffer.push_str(&raw);
                        buffer.push('>');
                    }
                    i = end + 1;
                } else {
                    buffer.push_str("`<");
                    i += 1;
                }
            }
            _other => {
                // Python OMENbrowser keeps the raw row available but does not spill
                // unknown inline commands into visible page text.
                i += 1;
            }
        }
    }

    flush_text(&mut fragments, &mut buffer, &current_style);
    state.style = current_style;
    fragments
}

fn flush_text(fragments: &mut Vec<Fragment>, buffer: &mut String, style: &TextStyle) {
    if buffer.is_empty() {
        return;
    }
    fragments.extend(autolink_lxmf(buffer, style));
    buffer.clear();
}

fn autolink_lxmf(text: &str, style: &TextStyle) -> Vec<Fragment> {
    let mut fragments = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find("lxmf@") {
        if start > 0 {
            fragments.push(Fragment::Span(TextSpan {
                text: rest[..start].into(),
                style: style.clone(),
                link: None,
            }));
        }
        let candidate = &rest[start..];
        let end = candidate
            .find(|ch: char| !(ch.is_ascii_hexdigit() || matches!(ch, 'l' | 'x' | 'm' | 'f' | '@')))
            .unwrap_or(candidate.len());
        let address = &candidate[..end];
        fragments.push(Fragment::Span(TextSpan {
            text: address.into(),
            style: style.clone(),
            link: (address.len() >= "lxmf@".len() + 16).then(|| LinkAction {
                target: address.into(),
                fields: Vec::new(),
            }),
        }));
        rest = &candidate[end..];
    }
    if !rest.is_empty() {
        fragments.push(Fragment::Span(TextSpan {
            text: rest.into(),
            style: style.clone(),
            link: None,
        }));
    }
    fragments
}

fn parse_color(chars: &[char]) -> (String, usize) {
    if chars.len() >= 7 && chars[0] == 'T' && chars[1..7].iter().all(|ch| ch.is_ascii_hexdigit()) {
        return (chars[1..7].iter().collect(), 7);
    }
    if chars.len() >= 3 {
        return (chars[..3].iter().collect(), 3);
    }
    (String::new(), 0)
}

fn parse_link(raw: &str, style: &TextStyle) -> Option<TextSpan> {
    if raw.is_empty() {
        return None;
    }
    let mut parts = raw.split('`');
    let label = parts.next()?.to_string();
    let target = parts.next().unwrap_or(&label).to_string();
    let fields = parts
        .next()
        .map(|blob| {
            blob.split('|')
                .filter(|field| !field.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    Some(TextSpan {
        text: label,
        style: style.clone(),
        link: Some(LinkAction { target, fields }),
    })
}

fn parse_control(raw: &str, style: &TextStyle) -> Option<FieldControl> {
    let (descriptor, value) = raw.split_once('`')?;
    let mut kind = "field";
    let mut name = descriptor;
    let mut width = 24;
    let mut masked = false;
    let mut prechecked = false;
    let mut field_value = "";

    if descriptor.contains('|') {
        let parts: Vec<_> = descriptor.split('|').collect();
        let mut flags = parts.first().copied().unwrap_or_default().to_string();
        name = parts.get(1).copied().unwrap_or(descriptor);
        if flags.contains('^') {
            kind = "radio";
            flags = flags.replace('^', "");
        } else if flags.contains('?') {
            kind = "checkbox";
            flags = flags.replace('?', "");
        } else if flags.contains('!') {
            masked = true;
            flags = flags.replace('!', "");
        }
        if !flags.is_empty() {
            width = flags.parse().unwrap_or(24);
        }
        field_value = parts.get(2).copied().unwrap_or_default();
        prechecked = parts.get(3).copied() == Some("*");
    }

    Some(FieldControl {
        kind: kind.into(),
        name: name.into(),
        value: if field_value.is_empty() {
            value
        } else {
            field_value
        }
        .into(),
        label: if matches!(kind, "checkbox" | "radio") {
            value.into()
        } else {
            String::new()
        },
        width,
        masked,
        prechecked,
        style: style.clone(),
    })
}

fn find_char(chars: &[char], start: usize, target: char) -> Option<usize> {
    chars
        .iter()
        .enumerate()
        .skip(start)
        .find_map(|(index, ch)| (*ch == target).then_some(index))
}

fn find_sequence(chars: &[char], start: usize, target: &[char]) -> Option<usize> {
    chars
        .windows(target.len())
        .enumerate()
        .skip(start)
        .find_map(|(index, window)| (window == target).then_some(index))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn span_text(row: &RenderRow) -> String {
        row.fragments
            .iter()
            .filter_map(|fragment| match fragment {
                Fragment::Span(span) => Some(span.text.as_str()),
                Fragment::Control(_) => None,
            })
            .collect()
    }

    #[test]
    fn parses_plain_text_without_destroying_spaces() {
        let doc = parse_micron("hello   world");

        assert_eq!(span_text(&doc.rows[0]), "hello   world");
    }

    #[test]
    fn parses_color_state_and_metadata() {
        let doc = parse_micron("#!fg=abc\n`F123red\nstill red");
        let Fragment::Span(first) = &doc.rows[0].fragments[0] else {
            panic!("expected span");
        };
        let Fragment::Span(second) = &doc.rows[1].fragments[0] else {
            panic!("expected span");
        };

        assert_eq!(doc.metadata.get("fg").map(String::as_str), Some("abc"));
        assert_eq!(first.style.fg.as_deref(), Some("123"));
        assert_eq!(second.style.fg.as_deref(), Some("123"));
    }

    #[test]
    fn parses_truecolor_and_background() {
        let doc = parse_micron("`FTaabbccfg `BT112233bg");
        let Fragment::Span(fg) = &doc.rows[0].fragments[0] else {
            panic!("expected span");
        };
        let Fragment::Span(bg) = &doc.rows[0].fragments[1] else {
            panic!("expected span");
        };

        assert_eq!(fg.style.fg.as_deref(), Some("aabbcc"));
        assert_eq!(bg.style.bg.as_deref(), Some("112233"));
    }

    #[test]
    fn parses_basic_style_markers_and_reset() {
        let doc = parse_micron("a `!bold`! normal ```reset");
        let Fragment::Span(bold) = &doc.rows[0].fragments[1] else {
            panic!("expected styled span");
        };
        let Fragment::Span(reset) = doc.rows[0].fragments.last().expect("reset span") else {
            panic!("expected reset span");
        };

        assert!(bold.style.bold);
        assert!(!reset.style.bold);
        assert_eq!(reset.style.fg.as_deref(), Some(DEFAULT_FG_DARK));
    }

    #[test]
    fn parses_links_with_forwarded_fields() {
        let doc = parse_micron("open `[label`mock.node:/path`name|query]");
        let Fragment::Span(span) = &doc.rows[0].fragments[1] else {
            panic!("expected link span");
        };

        let link = span.link.as_ref().expect("link metadata");
        assert_eq!(span.text, "label");
        assert_eq!(link.target, "mock.node:/path");
        assert_eq!(link.fields, vec!["name", "query"]);
    }

    #[test]
    fn links_inherit_document_default_foreground() {
        let doc = parse_micron("#!fg=f11\nopen `[label`mock.node:/path]");
        let Fragment::Span(span) = &doc.rows[0].fragments[1] else {
            panic!("expected link span");
        };

        assert_eq!(span.style.fg.as_deref(), Some("f11"));
        assert!(span.link.is_some());
    }

    #[test]
    fn parses_shorthand_links_and_lxmf_autolinks() {
        let doc = parse_micron("`a=mock.node:/|Home`a lxmf@0011223344556677");
        let Fragment::Span(short) = &doc.rows[0].fragments[0] else {
            panic!("expected shorthand link");
        };
        let Fragment::Span(lxmf) = &doc.rows[0].fragments[2] else {
            panic!("expected lxmf link");
        };

        assert_eq!(short.text, "Home");
        assert_eq!(
            short.link.as_ref().map(|link| link.target.as_str()),
            Some("mock.node:/")
        );
        assert!(lxmf.link.is_some());
    }

    #[test]
    fn parses_controls() {
        let doc = parse_micron("name: `<12|username`guest>");
        let Fragment::Control(control) = &doc.rows[0].fragments[1] else {
            panic!("expected control");
        };

        assert_eq!(control.name, "username");
        assert_eq!(control.value, "guest");
        assert_eq!(control.width, 12);
    }

    #[test]
    fn parses_checkbox_and_radio_controls() {
        let doc = parse_micron("`<?|agree|yes|*`Agree> `<^|mode|fast`Fast>");
        let Fragment::Control(checkbox) = &doc.rows[0].fragments[0] else {
            panic!("expected checkbox");
        };
        let Fragment::Control(radio) = &doc.rows[0].fragments[2] else {
            panic!("expected radio");
        };

        assert_eq!(checkbox.kind, "checkbox");
        assert!(checkbox.prechecked);
        assert_eq!(radio.kind, "radio");
    }

    #[test]
    fn parses_alignment_heading_divider_and_partials() {
        let doc = parse_micron("`cCentered\n>Heading\n-\n`{mock.node:/slot interval=5}");

        assert_eq!(doc.rows[0].align, Alignment::Center);
        assert_eq!(doc.rows[1].kind, RowKind::Heading);
        assert_eq!(doc.rows[2].kind, RowKind::Divider);
        assert_eq!(doc.rows[3].kind, RowKind::Partial);
        assert!(doc.rows[3].partial.is_some());
        assert!(doc.rows[3].fragments.is_empty());
    }

    #[test]
    fn literal_mode_preserves_unknown_markup() {
        let doc = parse_micron("`=\n`!not bold\n`=");
        let Fragment::Span(span) = &doc.rows[0].fragments[0] else {
            panic!("expected literal span");
        };

        assert_eq!(span.text, "`!not bold");
        assert!(!span.style.bold);
    }

    #[test]
    fn suppresses_unknown_inline_commands_but_preserves_raw_row() {
        let doc = parse_micron("before `Z after");

        assert_eq!(span_text(&doc.rows[0]), "before  after");
        assert_eq!(doc.rows[0].raw, "before `Z after");
    }

    #[test]
    fn consumes_python_style_three_character_color_commands() {
        let doc = parse_micron("`Fbluquiet `gxxalso quiet");
        let Fragment::Span(first) = &doc.rows[0].fragments[0] else {
            panic!("expected span");
        };
        let Fragment::Span(second) = &doc.rows[0].fragments[1] else {
            panic!("expected span");
        };

        assert_eq!(span_text(&doc.rows[0]), "quiet also quiet");
        assert_eq!(first.style.fg.as_deref(), Some("blu"));
        assert_eq!(second.style.fg.as_deref(), Some("gxx"));
    }

    #[test]
    fn consumes_nomadnet_bare_metadata_and_color_lines() {
        let doc = parse_micron("c=0\nfg=f00\nf00\nWELCOME\nddd\n8f8Sysop\n400`Ff00 NETWORK");

        assert_eq!(doc.metadata.get("c").map(String::as_str), Some("0"));
        assert_eq!(doc.metadata.get("fg").map(String::as_str), Some("f00"));
        assert_eq!(doc.rows.len(), 3);
        assert_eq!(span_text(&doc.rows[0]), "WELCOME");
        assert_eq!(span_text(&doc.rows[1]), "Sysop");
        assert_eq!(span_text(&doc.rows[2]), " NETWORK");

        let Fragment::Span(sysop) = &doc.rows[1].fragments[0] else {
            panic!("expected span");
        };
        let Fragment::Span(network) = &doc.rows[2].fragments[0] else {
            panic!("expected span");
        };
        assert_eq!(sysop.style.fg.as_deref(), Some("8f8"));
        assert_eq!(network.style.fg.as_deref(), Some("f00"));
    }

    #[test]
    fn consumes_python_delimited_link_style_without_trailing_backtick() {
        let doc = parse_micron("`Ff66•`f `_`[v4.py`:/file/v4.py]`_`");
        let link = doc.rows[0]
            .fragments
            .iter()
            .filter_map(|fragment| match fragment {
                Fragment::Span(span) if span.link.is_some() => Some(span),
                _ => None,
            })
            .next()
            .expect("link span");

        assert_eq!(span_text(&doc.rows[0]), "• v4.py");
        assert_eq!(link.text, "v4.py");
        assert_eq!(
            link.link.as_ref().map(|link| link.target.as_str()),
            Some(":/file/v4.py")
        );
    }

    #[test]
    fn malformed_control_does_not_panic_or_disappear() {
        let doc = parse_micron("before `<missing terminator");

        assert_eq!(span_text(&doc.rows[0]), "before `<missing terminator");
    }

    #[test]
    fn detects_half_block_rows_as_cell_preserving() {
        let art = "▀▄█▌▐░▒▓";
        let doc = parse_micron(art);

        assert!(doc.rows[0].cell_preserving);
        assert_eq!(span_text(&doc.rows[0]), art);
    }
}
