use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

pub const DEFAULT_FG_DARK: &str = "ccc";
pub const DEFAULT_BG: &str = "default";
pub const MICRON_LINK_RAW_MAX_BYTES: usize = 96 * 1024;
pub const MICRON_LINK_LABEL_MAX_BYTES: usize = 16 * 1024;
pub const MICRON_LINK_TARGET_MAX_BYTES: usize = 8 * 1024;
pub const MICRON_LINK_MAX_FIELDS: usize = 128;
pub const MICRON_LINK_FIELD_MAX_BYTES: usize = 4 * 1024;
pub const MICRON_LINK_FIELDS_MAX_BYTES: usize = 64 * 1024;
pub const MICRON_CONTROL_RAW_MAX_BYTES: usize = 72 * 1024;
pub const MICRON_CONTROL_NAME_MAX_BYTES: usize = 256;
pub const MICRON_CONTROL_VALUE_MAX_BYTES: usize = 64 * 1024;
pub const MICRON_CONTROL_FLAGS_MAX_BYTES: usize = 32;
pub const MICRON_CONTROL_MAX_WIDTH: usize = 256;
pub const MICRON_CONTROL_MAX_ITEMS: usize = 128;
pub const MICRON_CONTROL_MAX_OWNED_BYTES: usize = 4 * 1024 * 1024;
pub const MICRON_DOCUMENT_MAX_ROWS: usize = 16 * 1024;
pub const MICRON_DOCUMENT_MAX_LINE_BYTES: usize = 256 * 1024;
pub const MICRON_METADATA_MAX_ITEMS: usize = 64;
pub const MICRON_METADATA_KEY_MAX_BYTES: usize = 256;
pub const MICRON_METADATA_VALUE_MAX_BYTES: usize = 4 * 1024;
pub const MICRON_METADATA_MAX_OWNED_BYTES: usize = 64 * 1024;
pub const MICRON_DOCUMENT_MAX_FRAGMENTS: usize = 64 * 1024;
pub const MICRON_DOCUMENT_SPAN_TEXT_MAX_BYTES: usize = 4 * 1024 * 1024;
pub const MICRON_DOCUMENT_MAX_LINK_ACTIONS: usize = 4 * 1024;
pub const MICRON_DOCUMENT_LINK_ACTIONS_MAX_BYTES: usize = 4 * 1024 * 1024;
const MICRON_STYLE_METADATA_VALUE_MAX_BYTES: usize = 16;
const MICRON_DOCUMENT_LIMIT_NOTICE: &str =
    "[OMENbrowser: page content was truncated at a safe rendering limit]";

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
    #[serde(default)]
    pub limits_applied: bool,
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
    control_items: usize,
    control_owned_bytes: usize,
    fragment_items: usize,
    span_text_bytes: usize,
    link_action_items: usize,
    link_action_owned_bytes: usize,
    limits_applied: bool,
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
            control_items: 0,
            control_owned_bytes: 0,
            fragment_items: 0,
            span_text_bytes: 0,
            link_action_items: 0,
            link_action_owned_bytes: 0,
            limits_applied: false,
        }
    }
}

pub fn parse_micron(markup: &str) -> Document {
    let mut state = ParserState::default();
    let mut rows = Vec::new();
    let mut metadata = BTreeMap::new();
    let mut limits_applied = false;

    for raw_line in markup.lines() {
        if raw_line.len() > MICRON_DOCUMENT_MAX_LINE_BYTES {
            limits_applied = true;
            continue;
        }
        if let Some(meta) = raw_line.strip_prefix("#!") {
            if let Some((key, value)) = meta.split_once('=') {
                limits_applied |= !apply_metadata_directive(key, value, &mut state, &mut metadata);
            }
            continue;
        }

        if let Some((key, value)) = bare_metadata_directive(raw_line) {
            limits_applied |= !apply_metadata_directive(key, value, &mut state, &mut metadata);
            continue;
        }

        if let Some(mut row) = parse_line(raw_line, &mut state) {
            if rows.len() >= MICRON_DOCUMENT_MAX_ROWS.saturating_sub(1) {
                limits_applied = true;
                break;
            }
            admit_row_fragments(&mut row, &mut state);
            rows.push(row);
        }
    }

    limits_applied |= state.limits_applied;

    if limits_applied {
        rows.push(limit_notice_row(&state));
    }

    Document {
        rows,
        metadata,
        limits_applied,
    }
}

fn admit_row_fragments(row: &mut RenderRow, state: &mut ParserState) {
    let mut admitted = Vec::with_capacity(row.fragments.len());
    for mut fragment in std::mem::take(&mut row.fragments) {
        if state.fragment_items >= MICRON_DOCUMENT_MAX_FRAGMENTS.saturating_sub(1) {
            state.limits_applied = true;
            break;
        }
        match &mut fragment {
            Fragment::Span(span) => {
                let Some(next_text_bytes) = state.span_text_bytes.checked_add(span.text.len())
                else {
                    state.limits_applied = true;
                    break;
                };
                if next_text_bytes
                    > MICRON_DOCUMENT_SPAN_TEXT_MAX_BYTES
                        .saturating_sub(MICRON_DOCUMENT_LIMIT_NOTICE.len())
                {
                    state.limits_applied = true;
                    break;
                }
                state.span_text_bytes = next_text_bytes;

                if let Some(link) = span.link.as_ref() {
                    let action_bytes = link
                        .fields
                        .iter()
                        .try_fold(link.target.len(), |total, field| {
                            total.checked_add(field.len())
                        });
                    let action_admitted = state.link_action_items
                        < MICRON_DOCUMENT_MAX_LINK_ACTIONS
                        && action_bytes.is_some_and(|action_bytes| {
                            state
                                .link_action_owned_bytes
                                .checked_add(action_bytes)
                                .is_some_and(|total| {
                                    total <= MICRON_DOCUMENT_LINK_ACTIONS_MAX_BYTES
                                })
                        });
                    if action_admitted {
                        state.link_action_items += 1;
                        state.link_action_owned_bytes +=
                            action_bytes.expect("checked action bytes");
                    } else {
                        span.link = None;
                        state.limits_applied = true;
                    }
                }
            }
            Fragment::Control(_) => {}
        }
        state.fragment_items += 1;
        admitted.push(fragment);
    }
    row.fragments = admitted;
}

fn limit_notice_row(state: &ParserState) -> RenderRow {
    RenderRow {
        kind: RowKind::Text,
        depth: 0,
        fragments: vec![Fragment::Span(TextSpan {
            text: MICRON_DOCUMENT_LIMIT_NOTICE.into(),
            style: state.style.clone(),
            link: None,
        })],
        align: Alignment::Left,
        base_style: state.style.clone(),
        divider: '─',
        cell_preserving: false,
        partial: None,
        raw: MICRON_DOCUMENT_LIMIT_NOTICE.into(),
    }
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
) -> bool {
    let key = key.trim();
    let value = value.trim();
    if key.is_empty()
        || key.len() > MICRON_METADATA_KEY_MAX_BYTES
        || value.len() > MICRON_METADATA_VALUE_MAX_BYTES
        || (matches!(key, "fg" | "bg") && value.len() > MICRON_STYLE_METADATA_VALUE_MAX_BYTES)
        || (!metadata.contains_key(key) && metadata.len() >= MICRON_METADATA_MAX_ITEMS)
    {
        return false;
    }
    let retained_bytes = metadata
        .iter()
        .filter(|(current_key, _)| current_key.as_str() != key)
        .fold(0usize, |total, (current_key, current_value)| {
            total.saturating_add(current_key.len().saturating_add(current_value.len()))
        });
    if retained_bytes
        .checked_add(key.len())
        .and_then(|total| total.checked_add(value.len()))
        .is_none_or(|total| total > MICRON_METADATA_MAX_OWNED_BYTES)
    {
        return false;
    }
    let key = key.to_string();
    let value = value.to_string();
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
    true
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
                    let raw_chars = &chars[i + 2..end];
                    if chars_utf8_len(raw_chars)
                        .is_some_and(|bytes| bytes <= MICRON_LINK_RAW_MAX_BYTES)
                    {
                        let shorthand = raw_chars.iter().collect::<String>();
                        if let Some(span) = parse_shorthand_link(&shorthand, &current_style) {
                            fragments.push(Fragment::Span(span));
                        } else {
                            fragments.push(plain_span(format!("`a={shorthand}`a"), &current_style));
                        }
                    } else {
                        let mut literal = String::from("`a=");
                        literal.extend(raw_chars);
                        literal.push_str("`a");
                        fragments.push(plain_span(literal, &current_style));
                    }
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
                    let raw_chars = &chars[i + 1..end];
                    if chars_utf8_len(raw_chars)
                        .is_some_and(|bytes| bytes <= MICRON_LINK_RAW_MAX_BYTES)
                    {
                        let raw = raw_chars.iter().collect::<String>();
                        if let Some(span) = parse_link(&raw, &current_style) {
                            fragments.push(Fragment::Span(span));
                        } else {
                            fragments.push(plain_span(format!("`[{raw}]"), &current_style));
                        }
                    } else {
                        let mut literal = String::from("`[");
                        literal.extend(raw_chars);
                        literal.push(']');
                        fragments.push(plain_span(literal, &current_style));
                    }
                    i = end + 1;
                } else {
                    buffer.push_str("`[");
                    i += 1;
                }
            }
            '<' => {
                if let Some(end) = find_char(&chars, i + 1, '>') {
                    let raw_chars = &chars[i + 1..end];
                    if chars_utf8_len(raw_chars)
                        .is_some_and(|bytes| bytes <= MICRON_CONTROL_RAW_MAX_BYTES)
                    {
                        let raw = raw_chars.iter().collect::<String>();
                        let parsed = (state.control_items < MICRON_CONTROL_MAX_ITEMS)
                            .then(|| parse_control(&raw, &current_style))
                            .flatten();
                        if let Some((control, owned_bytes)) = parsed.filter(|(_, owned_bytes)| {
                            state
                                .control_owned_bytes
                                .checked_add(*owned_bytes)
                                .is_some_and(|total| total <= MICRON_CONTROL_MAX_OWNED_BYTES)
                        }) {
                            state.control_items += 1;
                            state.control_owned_bytes += owned_bytes;
                            fragments.push(Fragment::Control(control));
                        } else {
                            fragments.push(plain_span(format!("`<{raw}>"), &current_style));
                        }
                    } else {
                        let mut literal = String::from("`<");
                        literal.extend(raw_chars);
                        literal.push('>');
                        fragments.push(plain_span(literal, &current_style));
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

fn plain_span(text: String, style: &TextStyle) -> Fragment {
    Fragment::Span(TextSpan {
        text,
        style: style.clone(),
        link: None,
    })
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
            link: (address.len() >= "lxmf@".len() + 16
                && address.len() <= MICRON_LINK_TARGET_MAX_BYTES)
                .then(|| LinkAction {
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
    if raw.is_empty() || raw.len() > MICRON_LINK_RAW_MAX_BYTES {
        return None;
    }
    let mut parts = raw.split('`');
    let label = parts.next()?;
    let target = parts.next().unwrap_or(label);
    if label.len() > MICRON_LINK_LABEL_MAX_BYTES
        || target.is_empty()
        || target.len() > MICRON_LINK_TARGET_MAX_BYTES
    {
        return None;
    }
    let fields = match parts.next() {
        Some(blob) => {
            collect_bounded_link_fields(blob.split('|').filter(|field| !field.is_empty()))?
        }
        None => Vec::new(),
    };
    Some(TextSpan {
        text: label.to_owned(),
        style: style.clone(),
        link: Some(LinkAction {
            target: target.to_owned(),
            fields,
        }),
    })
}

fn parse_shorthand_link(raw: &str, style: &TextStyle) -> Option<TextSpan> {
    if raw.is_empty() || raw.len() > MICRON_LINK_RAW_MAX_BYTES {
        return None;
    }
    let (target, label) = raw.split_once('|').unwrap_or((raw, raw));
    if target.is_empty()
        || target.len() > MICRON_LINK_TARGET_MAX_BYTES
        || label.len() > MICRON_LINK_LABEL_MAX_BYTES
    {
        return None;
    }
    Some(TextSpan {
        text: label.to_owned(),
        style: style.clone(),
        link: Some(LinkAction {
            target: target.to_owned(),
            fields: Vec::new(),
        }),
    })
}

pub(crate) fn collect_bounded_link_fields<'a>(
    input: impl IntoIterator<Item = &'a str>,
) -> Option<Vec<String>> {
    let mut fields = Vec::new();
    let mut total_bytes = 0usize;
    for field in input {
        if fields.len() >= MICRON_LINK_MAX_FIELDS || field.len() > MICRON_LINK_FIELD_MAX_BYTES {
            return None;
        }
        total_bytes = total_bytes.checked_add(field.len())?;
        if total_bytes > MICRON_LINK_FIELDS_MAX_BYTES {
            return None;
        }
        fields.push(field.to_owned());
    }
    Some(fields)
}

fn chars_utf8_len(chars: &[char]) -> Option<usize> {
    chars
        .iter()
        .try_fold(0usize, |total, ch| total.checked_add(ch.len_utf8()))
}

fn parse_control(raw: &str, style: &TextStyle) -> Option<(FieldControl, usize)> {
    if raw.len() > MICRON_CONTROL_RAW_MAX_BYTES {
        return None;
    }
    let (descriptor, value) = raw.split_once('`')?;
    let mut kind = "field";
    let mut name = descriptor;
    let mut width = 24;
    let mut masked = false;
    let mut prechecked = false;
    let mut field_value = "";

    if descriptor.contains('|') {
        let mut parts = descriptor.split('|');
        let raw_flags = parts.next().unwrap_or_default();
        name = parts.next().unwrap_or(descriptor);
        field_value = parts.next().unwrap_or_default();
        prechecked = parts.next() == Some("*");
        if parts.next().is_some() || raw_flags.len() > MICRON_CONTROL_FLAGS_MAX_BYTES {
            return None;
        }
        let mut flags = raw_flags.to_string();
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
            width = match flags.parse() {
                Ok(width @ 1..=MICRON_CONTROL_MAX_WIDTH) => width,
                Ok(_) => return None,
                Err(_) => 24,
            };
        }
    }

    if name.is_empty()
        || name.len() > MICRON_CONTROL_NAME_MAX_BYTES
        || value.len() > MICRON_CONTROL_VALUE_MAX_BYTES
        || field_value.len() > MICRON_CONTROL_VALUE_MAX_BYTES
    {
        return None;
    }
    let selected_value = if field_value.is_empty() {
        value
    } else {
        field_value
    };
    let label = matches!(kind, "checkbox" | "radio").then_some(value);
    let owned_bytes = kind
        .len()
        .checked_add(name.len())?
        .checked_add(selected_value.len())?
        .checked_add(label.map_or(0, str::len))?
        .checked_add(style.fg.as_deref().map_or(0, str::len))?
        .checked_add(style.bg.as_deref().map_or(0, str::len))?;
    let control = FieldControl {
        kind: kind.into(),
        name: name.into(),
        value: selected_value.into(),
        label: label.unwrap_or_default().into(),
        width,
        masked,
        prechecked,
        style: style.clone(),
    };
    Some((control, owned_bytes))
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

    fn document_link_count(document: &Document) -> usize {
        document
            .rows
            .iter()
            .flat_map(|row| &row.fragments)
            .filter(|fragment| matches!(fragment, Fragment::Span(TextSpan { link: Some(_), .. })))
            .count()
    }

    fn document_control_count(document: &Document) -> usize {
        document
            .rows
            .iter()
            .flat_map(|row| &row.fragments)
            .filter(|fragment| matches!(fragment, Fragment::Control(_)))
            .count()
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
    fn rejects_link_targets_and_field_collections_above_parser_budgets() {
        let too_many_fields = (0..=MICRON_LINK_MAX_FIELDS)
            .map(|index| format!("field{index}"))
            .collect::<Vec<_>>()
            .join("|");
        let too_many_field_bytes = (0..17)
            .map(|index| format!("{index:02}{}", "x".repeat(4_000)))
            .collect::<Vec<_>>()
            .join("|");
        for markup in [
            format!("`[label`{}]", "t".repeat(MICRON_LINK_TARGET_MAX_BYTES + 1)),
            format!(
                "`[label`target`{}]",
                "f".repeat(MICRON_LINK_FIELD_MAX_BYTES + 1)
            ),
            format!("`[label`target`{too_many_fields}]"),
            format!("`[label`target`{too_many_field_bytes}]"),
        ] {
            assert_eq!(document_link_count(&parse_micron(&markup)), 0);
        }
    }

    #[test]
    fn shorthand_and_lxmf_autolinks_reject_oversized_targets() {
        let shorthand = format!(
            "`a={}|label`a",
            "t".repeat(MICRON_LINK_TARGET_MAX_BYTES + 1)
        );
        assert_eq!(document_link_count(&parse_micron(&shorthand)), 0);

        let lxmf = format!("lxmf@{}", "a".repeat(MICRON_LINK_TARGET_MAX_BYTES + 1));
        assert_eq!(document_link_count(&parse_micron(&lxmf)), 0);
    }

    #[test]
    fn oversized_raw_link_syntax_is_not_materialized_as_an_action() {
        let markup = format!("`[label`target`{}]", "x".repeat(MICRON_LINK_RAW_MAX_BYTES));
        let document = parse_micron(&markup);
        assert_eq!(document_link_count(&document), 0);
        assert_eq!(span_text(&document.rows[0]), markup);

        let embedded_autolink = format!(
            "`[label`{} lxmf@0011223344556677]",
            "t".repeat(MICRON_LINK_TARGET_MAX_BYTES)
        );
        let document = parse_micron(&embedded_autolink);
        assert_eq!(document_link_count(&document), 0);
        assert_eq!(span_text(&document.rows[0]), embedded_autolink);
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
    fn oversized_or_malformed_controls_remain_non_actionable() {
        for markup in [
            format!(
                "`<{} `lxmf@0011223344556677>",
                "n".repeat(MICRON_CONTROL_NAME_MAX_BYTES + 1)
            ),
            format!("`<name`{}>", "v".repeat(MICRON_CONTROL_VALUE_MAX_BYTES + 1)),
            format!("`<{}|name`value>", MICRON_CONTROL_MAX_WIDTH + 1),
            "`<24|name|value|*|extra`label>".into(),
        ] {
            let document = parse_micron(&markup);
            assert_eq!(document_control_count(&document), 0);
            assert_eq!(document_link_count(&document), 0);
            assert_eq!(span_text(&document.rows[0]), markup);
        }
    }

    #[test]
    fn document_controls_are_item_and_owned_byte_bounded() {
        let item_markup = (0..=MICRON_CONTROL_MAX_ITEMS)
            .map(|index| format!("`<field{index}`value>"))
            .collect::<Vec<_>>()
            .join("\n");
        assert_eq!(
            document_control_count(&parse_micron(&item_markup)),
            MICRON_CONTROL_MAX_ITEMS
        );

        let value = "v".repeat(MICRON_CONTROL_VALUE_MAX_BYTES);
        let byte_markup = (0..MICRON_CONTROL_MAX_ITEMS)
            .map(|index| format!("`<field{index}`{value}>"))
            .collect::<Vec<_>>()
            .join("\n");
        let document = parse_micron(&byte_markup);
        let controls = document
            .rows
            .iter()
            .flat_map(|row| &row.fragments)
            .filter_map(|fragment| match fragment {
                Fragment::Control(control) => Some(control),
                Fragment::Span(_) => None,
            })
            .collect::<Vec<_>>();
        assert!(!controls.is_empty());
        assert!(controls.len() < MICRON_CONTROL_MAX_ITEMS);
        assert!(
            controls
                .iter()
                .map(|control| {
                    control.kind.len()
                        + control.name.len()
                        + control.value.len()
                        + control.label.len()
                        + control.style.fg.as_deref().map_or(0, str::len)
                        + control.style.bg.as_deref().map_or(0, str::len)
                })
                .sum::<usize>()
                <= MICRON_CONTROL_MAX_OWNED_BYTES
        );
    }

    #[test]
    fn document_metadata_is_item_and_owned_byte_bounded() {
        let item_markup = (0..=MICRON_METADATA_MAX_ITEMS)
            .map(|index| format!("#!key-{index}=value"))
            .chain(std::iter::once("body".into()))
            .collect::<Vec<_>>()
            .join("\n");
        let item_document = parse_micron(&item_markup);
        assert_eq!(item_document.metadata.len(), MICRON_METADATA_MAX_ITEMS);
        assert!(item_document.limits_applied);
        assert!(item_document
            .rows
            .last()
            .is_some_and(|row| row.raw == MICRON_DOCUMENT_LIMIT_NOTICE));

        let value = "v".repeat(MICRON_METADATA_VALUE_MAX_BYTES);
        let byte_markup = (0..MICRON_METADATA_MAX_ITEMS)
            .map(|index| format!("#!key-{index}={value}"))
            .chain(std::iter::once("body".into()))
            .collect::<Vec<_>>()
            .join("\n");
        let byte_document = parse_micron(&byte_markup);
        let retained_bytes = byte_document
            .metadata
            .iter()
            .map(|(key, value)| key.len() + value.len())
            .sum::<usize>();
        assert!(retained_bytes <= MICRON_METADATA_MAX_OWNED_BYTES);
        assert!(byte_document.metadata.len() < MICRON_METADATA_MAX_ITEMS);
        assert!(byte_document.limits_applied);

        let style_document = parse_micron(&format!(
            "#!fg={}\nbody",
            "f".repeat(MICRON_STYLE_METADATA_VALUE_MAX_BYTES + 1)
        ));
        assert!(!style_document.metadata.contains_key("fg"));
        assert_eq!(
            style_document.rows[0].base_style.fg.as_deref(),
            Some(DEFAULT_FG_DARK)
        );
        assert!(style_document.limits_applied);
    }

    #[test]
    fn document_source_lines_and_rows_are_bounded_with_visible_notice() {
        let oversized_line = "x".repeat(MICRON_DOCUMENT_MAX_LINE_BYTES + 1);
        let line_document = parse_micron(&format!("{oversized_line}\nsafe tail"));
        assert!(line_document.limits_applied);
        assert_eq!(line_document.rows.len(), 2);
        assert_eq!(line_document.rows[0].raw, "safe tail");
        assert_eq!(line_document.rows[1].raw, MICRON_DOCUMENT_LIMIT_NOTICE);

        let row_markup = std::iter::repeat_n("row", MICRON_DOCUMENT_MAX_ROWS)
            .collect::<Vec<_>>()
            .join("\n");
        let row_document = parse_micron(&row_markup);
        assert!(row_document.limits_applied);
        assert_eq!(row_document.rows.len(), MICRON_DOCUMENT_MAX_ROWS);
        assert_eq!(
            row_document.rows.last().map(|row| row.raw.as_str()),
            Some(MICRON_DOCUMENT_LIMIT_NOTICE)
        );
    }

    #[test]
    fn document_fragments_and_span_text_are_aggregate_bounded() {
        let fragment_markup = "x`!".repeat(MICRON_DOCUMENT_MAX_FRAGMENTS);
        let fragment_document = parse_micron(&fragment_markup);
        let fragment_count = fragment_document
            .rows
            .iter()
            .map(|row| row.fragments.len())
            .sum::<usize>();
        assert_eq!(fragment_count, MICRON_DOCUMENT_MAX_FRAGMENTS);
        assert!(fragment_document.limits_applied);
        assert_eq!(
            fragment_document.rows.last().map(|row| row.raw.as_str()),
            Some(MICRON_DOCUMENT_LIMIT_NOTICE)
        );

        let line = "t".repeat(MICRON_DOCUMENT_MAX_LINE_BYTES);
        let text_document =
            parse_micron(&std::iter::repeat_n(line, 17).collect::<Vec<_>>().join("\n"));
        let span_text_bytes = text_document
            .rows
            .iter()
            .flat_map(|row| row.fragments.iter())
            .filter_map(|fragment| match fragment {
                Fragment::Span(span) => Some(span.text.len()),
                Fragment::Control(_) => None,
            })
            .sum::<usize>();
        assert!(span_text_bytes <= MICRON_DOCUMENT_SPAN_TEXT_MAX_BYTES);
        assert!(text_document.limits_applied);
        assert_eq!(
            text_document.rows.last().map(|row| row.raw.as_str()),
            Some(MICRON_DOCUMENT_LIMIT_NOTICE)
        );
    }

    #[test]
    fn document_link_actions_are_item_and_owned_byte_bounded() {
        let item_markup =
            std::iter::repeat_n("`[x`mock.node:/]", MICRON_DOCUMENT_MAX_LINK_ACTIONS + 1)
                .collect::<Vec<_>>()
                .join("\n");
        let item_document = parse_micron(&item_markup);
        let item_links = document_links(&item_document);
        assert_eq!(item_links.len(), MICRON_DOCUMENT_MAX_LINK_ACTIONS);
        assert!(item_document.limits_applied);

        let target = "d".repeat(MICRON_LINK_TARGET_MAX_BYTES);
        let byte_markup = std::iter::repeat_n(
            format!("`[x`{target}]"),
            MICRON_DOCUMENT_LINK_ACTIONS_MAX_BYTES / MICRON_LINK_TARGET_MAX_BYTES + 1,
        )
        .collect::<Vec<_>>()
        .join("\n");
        let byte_document = parse_micron(&byte_markup);
        let byte_links = document_links(&byte_document);
        let retained_bytes = byte_links
            .iter()
            .map(|link| link.target.len() + link.fields.iter().map(String::len).sum::<usize>())
            .sum::<usize>();
        assert!(retained_bytes <= MICRON_DOCUMENT_LINK_ACTIONS_MAX_BYTES);
        assert_eq!(
            byte_links.len(),
            MICRON_DOCUMENT_LINK_ACTIONS_MAX_BYTES / MICRON_LINK_TARGET_MAX_BYTES
        );
        assert!(byte_document.limits_applied);
        assert!(byte_document.rows.iter().any(|row| {
            row.fragments.iter().any(|fragment| {
                matches!(
                    fragment,
                    Fragment::Span(span) if span.text == "x" && span.link.is_none()
                )
            })
        }));
    }

    fn document_links(document: &Document) -> Vec<&LinkAction> {
        document
            .rows
            .iter()
            .flat_map(|row| row.fragments.iter())
            .filter_map(|fragment| match fragment {
                Fragment::Span(span) => span.link.as_ref(),
                Fragment::Control(_) => None,
            })
            .collect()
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
