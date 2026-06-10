#[cfg(feature = "tui")]
use ratatui::style::{Color, Modifier, Style};
#[cfg(feature = "tui")]
use ratatui::text::{Line, Span};
use unicode_width::UnicodeWidthChar;

use crate::micron::parser::{
    Alignment, Document, FieldControl, Fragment, LinkAction, RenderRow, RowKind, TextStyle,
    DEFAULT_FG_DARK,
};

const SECTION_INDENT: usize = 2;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ControlRef {
    pub name: String,
    pub kind: String,
    pub value: String,
    pub offset: usize,
    pub length: usize,
    pub masked: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cell {
    pub ch: char,
    pub style: TextStyle,
    pub link: Option<LinkAction>,
    pub control: Option<ControlRef>,
    pub cursor: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RenderedRow {
    pub cells: Vec<Cell>,
    pub align: Alignment,
    pub depth: usize,
    pub base_style: TextStyle,
    pub wrap: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HitAction {
    Link(LinkAction),
    Control(ControlRef),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HitRegion {
    pub row: u16,
    pub col_start: u16,
    pub col_end: u16,
    pub action: HitAction,
}

impl RenderedRow {
    pub fn text(&self) -> String {
        self.cells.iter().map(|cell| cell.ch).collect()
    }
}

pub fn render_document(document: &Document, width: usize) -> Vec<RenderedRow> {
    document
        .rows
        .iter()
        .flat_map(|row| wrap_rendered_row(render_row(row, width), width))
        .collect()
}

pub fn hit_regions_for_document(document: &Document, width: usize) -> Vec<HitRegion> {
    let rows = render_document(document, width);
    hit_regions_for_rendered_rows(&rows)
}

pub fn hit_regions_for_rendered_rows(rows: &[RenderedRow]) -> Vec<HitRegion> {
    let mut regions = Vec::new();
    for (row_index, row) in rows.iter().enumerate() {
        let mut current: Option<HitRegion> = None;
        for (col, cell) in row.cells.iter().enumerate() {
            let action = cell
                .link
                .clone()
                .map(HitAction::Link)
                .or_else(|| cell.control.clone().map(HitAction::Control));
            let Some(action) = action else {
                if let Some(region) = current.take() {
                    regions.push(region);
                }
                continue;
            };
            let col = col as u16;
            match &mut current {
                Some(region) if region.action == action && region.col_end == col => {
                    region.col_end = col + 1;
                }
                Some(region) => {
                    regions.push(region.clone());
                    current = Some(HitRegion {
                        row: row_index as u16,
                        col_start: col,
                        col_end: col + 1,
                        action,
                    });
                }
                None => {
                    current = Some(HitRegion {
                        row: row_index as u16,
                        col_start: col,
                        col_end: col + 1,
                        action,
                    });
                }
            }
        }
        if let Some(region) = current.take() {
            regions.push(region);
        }
    }
    regions
}

pub fn hit_test_document(
    document: &Document,
    width: usize,
    row: u16,
    col: u16,
) -> Option<HitAction> {
    hit_regions_for_document(document, width)
        .into_iter()
        .find(|region| region.row == row && col >= region.col_start && col < region.col_end)
        .map(|region| region.action)
}

pub fn render_row(row: &RenderRow, width: usize) -> RenderedRow {
    render_row_with_field_focus(row, width, None)
}

fn render_row_with_field_focus(
    row: &RenderRow,
    width: usize,
    field_focus: Option<FieldFocus<'_>>,
) -> RenderedRow {
    let mut cells = Vec::new();
    let indent = " ".repeat(row.depth.saturating_sub(1) * SECTION_INDENT);

    if row.kind == RowKind::Divider {
        let available = width.saturating_sub(indent.len()).max(1);
        let text = format!("{indent}{}", row.divider.to_string().repeat(available));
        return RenderedRow {
            cells: text
                .chars()
                .take(width)
                .map(|ch| Cell {
                    ch,
                    style: row.base_style.clone(),
                    link: None,
                    control: None,
                    cursor: false,
                })
                .collect(),
            align: row.align,
            depth: row.depth,
            base_style: row.base_style.clone(),
            wrap: false,
        };
    }

    for ch in indent.chars() {
        cells.push(Cell {
            ch,
            style: row.base_style.clone(),
            link: None,
            control: None,
            cursor: false,
        });
    }

    for fragment in &row.fragments {
        match fragment {
            Fragment::Span(span) => {
                for ch in span.text.chars() {
                    cells.push(Cell {
                        ch,
                        style: span.style.clone(),
                        link: span.link.clone(),
                        control: None,
                        cursor: false,
                    });
                }
            }
            Fragment::Control(control) => append_control(&mut cells, control, field_focus),
        }
    }

    RenderedRow {
        cells,
        align: row.align,
        depth: row.depth,
        base_style: row.base_style.clone(),
        wrap: !row.cell_preserving,
    }
}

#[derive(Clone, Copy)]
struct FieldFocus<'a> {
    name: &'a str,
    cursor_byte: usize,
}

fn append_control(cells: &mut Vec<Cell>, control: &FieldControl, field_focus: Option<FieldFocus>) {
    let (display, style, cursor_offset, source_start) = match control.kind.as_str() {
        "field" => {
            let focus = field_focus.filter(|focus| focus.name == control.name);
            let value_chars = if control.masked {
                "*".repeat(control.value.chars().count())
                    .chars()
                    .collect::<Vec<_>>()
            } else {
                control.value.chars().collect::<Vec<_>>()
            };
            let cursor_char = focus
                .map(|focus| cursor_char_index(&control.value, focus.cursor_byte))
                .unwrap_or(0);
            let source_start = focus
                .map(|_| field_view_start(value_chars.len(), control.width, cursor_char))
                .unwrap_or(0);
            let mut display = value_chars
                .iter()
                .skip(source_start)
                .take(control.width)
                .collect::<String>();
            let visible_len = display.chars().count();
            if visible_len < control.width {
                display.push_str(&" ".repeat(control.width - visible_len));
            }
            let cursor_offset = focus.map(|_| {
                cursor_char
                    .saturating_sub(source_start)
                    .min(control.width.saturating_sub(1))
            });
            let mut style = control.style.clone();
            if style.bg.is_none() {
                style.bg = Some("223".into());
            }
            style.fg = Some("fff".into());
            (display, style, cursor_offset, source_start)
        }
        "button" => {
            let mut style = control.style.clone();
            if style.bg.is_none() {
                style.bg = Some("335".into());
            }
            if style.fg.as_deref() == Some(DEFAULT_FG_DARK) {
                style.fg = Some("fff".into());
            }
            (format!("[ {} ]", control.label_or_value()), style, None, 0)
        }
        "checkbox" => {
            let mark = if control.prechecked { 'x' } else { ' ' };
            (
                format!("[{mark}] {}", control.label),
                control.style.clone(),
                None,
                0,
            )
        }
        _ => {
            let mark = if control.prechecked { '*' } else { ' ' };
            (
                format!("({mark}) {}", control.label),
                control.style.clone(),
                None,
                0,
            )
        }
    };

    let length = display.chars().count();
    for (offset, ch) in display.chars().enumerate() {
        cells.push(Cell {
            ch,
            style: style.clone(),
            link: None,
            control: Some(ControlRef {
                name: control.name.clone(),
                kind: control.kind.clone(),
                value: control.value.clone(),
                offset: source_start + offset,
                length,
                masked: control.masked,
            }),
            cursor: cursor_offset == Some(offset),
        });
    }
}

pub fn render_document_with_field_cursor(
    document: &Document,
    width: usize,
    focused_control: Option<&str>,
    field_cursor: Option<(&str, usize)>,
) -> Vec<RenderedRow> {
    let Some((field_name, cursor_byte)) = field_cursor else {
        return render_document(document, width);
    };
    if Some(field_name) != focused_control {
        return render_document(document, width);
    }
    let focus = FieldFocus {
        name: field_name,
        cursor_byte,
    };
    document
        .rows
        .iter()
        .flat_map(|row| {
            wrap_rendered_row(render_row_with_field_focus(row, width, Some(focus)), width)
        })
        .collect()
}

fn cursor_char_index(value: &str, cursor_byte: usize) -> usize {
    let bounded = cursor_byte.min(value.len());
    let safe = while_not_char_boundary(value, bounded);
    value[..safe].chars().count()
}

fn field_view_start(value_len: usize, width: usize, cursor_char: usize) -> usize {
    if width == 0 || value_len <= width {
        return 0;
    }
    if cursor_char >= width {
        cursor_char.saturating_sub(width - 1).min(value_len - width)
    } else {
        0
    }
}

fn while_not_char_boundary(value: &str, mut index: usize) -> usize {
    while index > 0 && !value.is_char_boundary(index) {
        index -= 1;
    }
    index
}

trait ControlLabel {
    fn label_or_value(&self) -> &str;
}

impl ControlLabel for FieldControl {
    fn label_or_value(&self) -> &str {
        if self.label.is_empty() {
            &self.value
        } else {
            &self.label
        }
    }
}

fn wrap_rendered_row(row: RenderedRow, width: usize) -> Vec<RenderedRow> {
    if !row.wrap || width == 0 || cells_width(&row.cells) <= width {
        return vec![align_row(row, width)];
    }

    let mut wrapped = Vec::new();
    let mut start = 0;
    while start < row.cells.len() {
        let mut end = start;
        let mut current_width = 0;
        let mut last_break = None;
        while end < row.cells.len() {
            let next_width = cell_width(row.cells[end].ch);
            if current_width + next_width > width {
                break;
            }
            current_width += next_width;
            if row.cells[end].ch == ' '
                && row.cells[end].link.is_none()
                && row.cells[end].control.is_none()
            {
                last_break = Some(end + 1);
            }
            end += 1;
        }

        if end < row.cells.len() && !is_plain_space_cell(&row.cells[end]) {
            if let Some(break_at) = last_break.filter(|break_at| *break_at > start) {
                end = break_at;
            }
        }
        if end == start {
            end += 1;
        }

        let mut segment = row.cells[start..end].to_vec();
        while segment.last().is_some_and(is_plain_space_cell) {
            segment.pop();
        }
        wrapped.push(align_row(
            RenderedRow {
                cells: segment,
                align: row.align,
                depth: row.depth,
                base_style: row.base_style.clone(),
                wrap: row.wrap,
            },
            width,
        ));

        start = end;
        while start < row.cells.len() && is_plain_space_cell(&row.cells[start]) {
            start += 1;
        }
    }

    wrapped
}

fn is_plain_space_cell(cell: &Cell) -> bool {
    cell.ch == ' ' && cell.link.is_none() && cell.control.is_none() && !cell.cursor
}

fn align_row(mut row: RenderedRow, width: usize) -> RenderedRow {
    let row_width = cells_width(&row.cells);
    let padding = width.saturating_sub(row_width);
    let left_padding = match row.align {
        Alignment::Left => 0,
        Alignment::Center => padding / 2,
        Alignment::Right => padding,
    };
    if left_padding > 0 {
        let mut cells = vec![
            Cell {
                ch: ' ',
                style: row.base_style.clone(),
                link: None,
                control: None,
                cursor: false,
            };
            left_padding
        ];
        cells.extend(row.cells);
        row.cells = cells;
    }
    let right_padding = width.saturating_sub(cells_width(&row.cells));
    if right_padding > 0 {
        row.cells.extend(vec![
            Cell {
                ch: ' ',
                style: row.base_style.clone(),
                link: None,
                control: None,
                cursor: false,
            };
            right_padding
        ]);
    }
    row
}

fn cells_width(cells: &[Cell]) -> usize {
    cells.iter().map(|cell| cell_width(cell.ch)).sum()
}

fn cell_width(ch: char) -> usize {
    match ch {
        '▀' | '▄' | '█' | '▌' | '▐' | '░' | '▒' | '▓' => 1,
        _ => UnicodeWidthChar::width(ch).unwrap_or(1).max(1),
    }
}

#[cfg(feature = "tui")]
pub fn document_to_lines(document: &Document) -> Vec<Line<'static>> {
    document_to_lines_with_focus(document, 80, None, None)
}

#[cfg(feature = "tui")]
pub fn document_to_lines_with_focus(
    document: &Document,
    width: usize,
    focused_control: Option<&str>,
    focused_link: Option<&str>,
) -> Vec<Line<'static>> {
    document_to_lines_with_focus_and_cursor(document, width, focused_control, focused_link, None)
}

#[cfg(feature = "tui")]
pub fn document_to_lines_with_focus_and_cursor(
    document: &Document,
    width: usize,
    focused_control: Option<&str>,
    focused_link: Option<&str>,
    field_cursor: Option<(&str, usize)>,
) -> Vec<Line<'static>> {
    rendered_rows_to_lines(
        render_document_with_field_cursor(document, width, focused_control, field_cursor),
        focused_control,
        focused_link,
    )
}

#[cfg(feature = "tui")]
pub fn rendered_rows_to_lines(
    rows: Vec<RenderedRow>,
    focused_control: Option<&str>,
    focused_link: Option<&str>,
) -> Vec<Line<'static>> {
    rows.into_iter()
        .map(|row| {
            let spans: Vec<Span<'static>> =
                row.cells
                    .into_iter()
                    .map(|cell| {
                        let focused =
                            cell.control.as_ref().is_some_and(|control| {
                                Some(control.name.as_str()) == focused_control
                            }) || cell
                                .link
                                .as_ref()
                                .is_some_and(|link| Some(link.target.as_str()) == focused_link);
                        let style = if cell.cursor {
                            style_to_ratatui(&cell.style)
                                .bg(Color::White)
                                .fg(Color::Black)
                                .add_modifier(Modifier::REVERSED)
                        } else if focused {
                            style_to_ratatui(&cell.style)
                                .bg(Color::Cyan)
                                .fg(Color::Black)
                                .add_modifier(Modifier::BOLD)
                        } else {
                            style_to_ratatui(&cell.style)
                        };
                        Span::styled(cell.ch.to_string(), style)
                    })
                    .collect();
            Line::from(spans)
        })
        .collect()
}

#[cfg(feature = "tui")]
fn style_to_ratatui(style: &TextStyle) -> Style {
    let mut out = Style::default();
    if style.bold {
        out = out.add_modifier(Modifier::BOLD);
    }
    if style.italic {
        out = out.add_modifier(Modifier::ITALIC);
    }
    if style.underline {
        out = out.add_modifier(Modifier::UNDERLINED);
    }
    if style.dim {
        out = out.add_modifier(Modifier::DIM);
    }
    if style.reverse {
        out = out.add_modifier(Modifier::REVERSED);
    }
    if let Some(fg) = color_to_ratatui(style.fg.as_deref()) {
        out = out.fg(fg);
    }
    if let Some(bg) = color_to_ratatui(style.bg.as_deref()) {
        out = out.bg(bg);
    }
    out
}

#[cfg(feature = "tui")]
fn color_to_ratatui(color: Option<&str>) -> Option<Color> {
    let color = color?;
    if color == crate::micron::parser::DEFAULT_BG || color == "default" {
        return None;
    }
    if color.len() == 6 {
        let red = u8::from_str_radix(&color[0..2], 16).ok()?;
        let green = u8::from_str_radix(&color[2..4], 16).ok()?;
        let blue = u8::from_str_radix(&color[4..6], 16).ok()?;
        return Some(Color::Rgb(red, green, blue));
    }
    if color.len() == 3 && color.starts_with('g') {
        let level = color[1..].parse::<u16>().ok()?.min(99);
        let value = ((level * 255) / 99) as u8;
        return Some(Color::Rgb(value, value, value));
    }
    if color.len() == 3 {
        let red = u8::from_str_radix(&color[0..1].repeat(2), 16).ok()?;
        let green = u8::from_str_radix(&color[1..2].repeat(2), 16).ok()?;
        let blue = u8::from_str_radix(&color[2..3].repeat(2), 16).ok()?;
        return Some(Color::Rgb(red, green, blue));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::micron::parse_micron;

    #[test]
    fn renders_plain_text_to_cells() {
        let doc = parse_micron("hello");
        let rows = render_document(&doc, 80);

        assert_eq!(rows[0].text().trim_end(), "hello");
    }

    #[test]
    fn wraps_text_without_dropping_style_metadata() {
        let doc = parse_micron("`!alpha beta gamma");
        let rows = render_document(&doc, 10);

        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].text(), "alpha beta");
        assert!(rows[0].cells[0].style.bold);
        assert!(rows[1].cells[0].style.bold);
    }

    #[test]
    fn wraps_text_at_word_boundaries_when_possible() {
        let rows = render_document(&parse_micron("hello beautiful world"), 10);

        assert_eq!(rows[0].text().trim_end(), "hello");
        assert_eq!(rows[1].text().trim_end(), "beautiful");
        assert_eq!(rows[2].text().trim_end(), "world");
    }

    #[test]
    fn preserves_half_block_art_without_wrapping_or_corruption() {
        let art = "▀▄█▌▐░▒▓".repeat(10);
        let doc = parse_micron(&art);
        let rows = render_document(&doc, 40);

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].text(), art);
    }

    #[test]
    fn aligns_center_and_right_rows() {
        let center = render_document(&parse_micron("`chello"), 9);
        let right = render_document(&parse_micron("`rhi"), 5);

        assert_eq!(center[0].text(), "  hello  ");
        assert_eq!(right[0].text(), "   hi");
    }

    #[test]
    fn pads_rows_to_width_with_base_style_for_background_bars() {
        let rows = render_document(&parse_micron(">Title"), 8);

        assert_eq!(rows[0].text(), "Title   ");
        assert_eq!(
            rows[0]
                .cells
                .last()
                .and_then(|cell| cell.style.bg.as_deref()),
            Some("bbb")
        );
    }

    #[test]
    fn divider_renders_to_requested_width() {
        let rows = render_document(&parse_micron("-="), 6);

        assert_eq!(rows[0].text(), "======");
    }

    #[test]
    fn exposes_link_and_control_metadata_on_cells() {
        let doc = parse_micron("`[Go`mock.node:/] `<8|name`guest>");
        let rows = render_document(&doc, 80);

        assert_eq!(
            rows[0].cells[0]
                .link
                .as_ref()
                .map(|link| link.target.as_str()),
            Some("mock.node:/")
        );
        assert_eq!(
            rows[0].cells[3]
                .control
                .as_ref()
                .map(|control| control.name.as_str()),
            Some("name")
        );
    }

    #[test]
    fn hit_regions_expose_links_and_controls() {
        let doc = crate::micron::parse_micron("Go `[there`mock.node:/x] `<8|name`guest>");
        let regions = hit_regions_for_document(&doc, 80);

        assert!(regions
            .iter()
            .any(|region| matches!(region.action, HitAction::Link(_))));
        assert!(regions
            .iter()
            .any(|region| matches!(region.action, HitAction::Control(_))));
        assert!(matches!(
            hit_test_document(&doc, 80, 0, 4),
            Some(HitAction::Link(_))
        ));
    }

    #[test]
    fn hit_regions_can_be_extracted_from_rendered_rows() {
        let doc = crate::micron::parse_micron("Go `[there`mock.node:/x] `<8|name`guest>");
        let rows = render_document(&doc, 80);
        let regions = hit_regions_for_rendered_rows(&rows);

        assert!(regions
            .iter()
            .any(|region| matches!(region.action, HitAction::Link(_))));
        assert!(regions
            .iter()
            .any(|region| matches!(region.action, HitAction::Control(_))));
    }

    #[test]
    fn renders_cursor_on_focused_field_cell() {
        let doc = parse_micron("name: `<8|name`guest>");
        let rows = render_document_with_field_cursor(&doc, 80, Some("name"), Some(("name", 2)));
        let cursor_cells = rows[0]
            .cells
            .iter()
            .filter(|cell| cell.cursor)
            .collect::<Vec<_>>();

        assert_eq!(cursor_cells.len(), 1);
        assert_eq!(cursor_cells[0].ch, 'e');
        assert_eq!(
            cursor_cells[0]
                .control
                .as_ref()
                .map(|control| control.offset),
            Some(2)
        );
    }

    #[test]
    fn renders_utf8_cursor_without_splitting_codepoints() {
        let doc = parse_micron("name: `<8|name`aéz>");
        let cursor_after_e = "aé".len();
        let rows = render_document_with_field_cursor(
            &doc,
            80,
            Some("name"),
            Some(("name", cursor_after_e)),
        );
        let cursor = rows[0]
            .cells
            .iter()
            .find(|cell| cell.cursor)
            .expect("cursor cell");

        assert_eq!(cursor.ch, 'z');
        assert_eq!(
            cursor.control.as_ref().map(|control| control.offset),
            Some(2)
        );
    }

    #[test]
    fn renders_end_cursor_on_last_visible_field_cell() {
        let doc = parse_micron("name: `<4|name`abcdef>");
        let rows = render_document_with_field_cursor(
            &doc,
            80,
            Some("name"),
            Some(("name", "abcdef".len())),
        );
        let cursor = rows[0]
            .cells
            .iter()
            .find(|cell| cell.cursor)
            .expect("cursor cell");

        assert!(rows[0].text().contains("cdef"));
        assert_eq!(cursor.ch, 'f');
        assert_eq!(
            cursor.control.as_ref().map(|control| control.offset),
            Some(5)
        );
    }

    #[test]
    fn scrolls_focused_field_viewport_around_cursor() {
        let doc = parse_micron("name: `<5|name`0123456789>");
        let rows = render_document_with_field_cursor(&doc, 80, Some("name"), Some(("name", 7)));
        let field_text = rows[0]
            .cells
            .iter()
            .filter(|cell| {
                cell.control
                    .as_ref()
                    .is_some_and(|control| control.name == "name")
            })
            .map(|cell| cell.ch)
            .collect::<String>();
        let cursor = rows[0]
            .cells
            .iter()
            .find(|cell| cell.cursor)
            .expect("cursor cell");

        assert_eq!(field_text, "34567");
        assert_eq!(cursor.ch, '7');
        assert_eq!(
            cursor.control.as_ref().map(|control| control.offset),
            Some(7)
        );
    }

    #[test]
    fn empty_wrapped_field_keeps_control_cells() {
        let doc = parse_micron("`<24|message`>");
        let rows = render_document(&doc, 8);

        assert!(!rows.is_empty());
        assert!(rows.iter().any(|row| {
            row.cells.iter().any(|cell| {
                cell.ch == ' '
                    && cell
                        .control
                        .as_ref()
                        .is_some_and(|control| control.name == "message")
            })
        }));
    }

    #[test]
    fn python_mock_index_fixture_renders_without_style_code_spill() {
        let doc = parse_micron(include_str!("../../fixtures/micron/python_mock_index.mu"));
        let rows = render_document(&doc, 80);
        let visible = rows
            .iter()
            .map(RenderedRow::text)
            .collect::<Vec<_>>()
            .join("\n");

        assert!(visible.contains("Welcome to OMENbrowser"));
        assert!(visible.contains("Browse sample gallery"));
        assert!(visible.contains("▀▀"));
        assert!(!visible.contains("`F"));
        assert!(!visible.contains("`B"));
        assert!(!visible.contains("`T"));
    }

    #[test]
    fn python_mock_index_fixture_preserves_authored_link_style() {
        let doc = parse_micron(include_str!("../../fixtures/micron/python_mock_index.mu"));
        let rows = render_document(&doc, 80);
        let link_cell = rows
            .iter()
            .flat_map(|row| row.cells.iter())
            .find(|cell| {
                cell.link
                    .as_ref()
                    .is_some_and(|link| link.target == "mock.node:/page/gallery.mu")
            })
            .expect("gallery link cell");

        assert_eq!(link_cell.style.fg.as_deref(), Some("0af"));
    }

    #[test]
    fn captured_file_links_do_not_spill_delimiter_backticks() {
        let doc = parse_micron(include_str!(
            "../../fixtures/micron/captures/generic_file_listing.mu"
        ));
        let rows = render_document(&doc, 80);
        let visible = rows
            .iter()
            .map(RenderedRow::text)
            .collect::<Vec<_>>()
            .join("\n");

        assert!(visible.contains("Example_Document.pdf"));
        assert!(visible.contains("tool.py"));
        assert!(!visible.contains("tool.py`"));
        assert!(!visible.contains(".pdf`"));
    }
}
