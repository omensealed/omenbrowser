use iced::widget::canvas;
use iced::{alignment, Color, Font, Pixels, Point, Rectangle, Renderer, Size};
use unicode_width::UnicodeWidthChar;

use crate::micron::parser::DEFAULT_FG_DARK;
use crate::micron::render::Cell;

use super::emoji_font;

pub(super) const CELL_WIDTH: f32 = 9.0;
pub(super) const CELL_HEIGHT: f32 = 18.0;
pub(super) const FONT_SIZE: f32 = 15.0;
pub(super) const PADDING: f32 = 10.0;
pub(super) const RIGHT_SCROLL_GUTTER: f32 = 14.0;
pub(super) const MAX_CANVAS_COLS: usize = 512;
pub(super) const MAX_CANVAS_ROWS: usize = 2048;

#[derive(Clone, Copy, Debug)]
pub(super) struct PageMetrics {
    pub(super) cell_width: f32,
    pub(super) cell_height: f32,
    pub(super) font_size: f32,
}

impl PageMetrics {
    pub(super) fn new(zoom_percent: u16) -> Self {
        let scale = zoom_percent.clamp(50, 200) as f32 / 100.0;
        Self {
            cell_width: CELL_WIDTH * scale,
            cell_height: CELL_HEIGHT * scale,
            font_size: FONT_SIZE * scale,
        }
    }

    pub(super) fn width_cells_for_bounds(self, bounds: Rectangle) -> usize {
        (self.content_width_for_bounds(bounds) / self.cell_width)
            .floor()
            .max(1.0)
            .min(MAX_CANVAS_COLS as f32) as usize
    }

    pub(super) fn content_width_for_bounds(self, bounds: Rectangle) -> f32 {
        if !bounds.width.is_finite() {
            return self.cell_width * MAX_CANVAS_COLS as f32;
        }
        (bounds.width - PADDING * 2.0 - RIGHT_SCROLL_GUTTER)
            .max(self.cell_width)
            .min(self.cell_width * MAX_CANVAS_COLS as f32)
    }

    pub(super) fn height_rows_for_bounds(self, bounds: Rectangle) -> usize {
        if !bounds.height.is_finite() {
            return MAX_CANVAS_ROWS;
        }
        ((bounds.height - PADDING * 2.0) / self.cell_height)
            .floor()
            .max(1.0)
            .min(MAX_CANVAS_ROWS as f32) as usize
    }

    pub(super) fn cell_at(
        self,
        position: Point,
        row_count: usize,
        width: usize,
    ) -> Option<(u16, u16)> {
        if position.x < PADDING || position.y < PADDING {
            return None;
        }
        let row = ((position.y - PADDING) / self.cell_height).floor() as usize;
        let col = ((position.x - PADDING) / self.cell_width).floor() as usize;
        if row >= row_count || col >= width {
            return None;
        }
        Some((row as u16, col as u16))
    }

    pub(super) fn underline_offset(self) -> f32 {
        (self.cell_height - 3.0 * self.scale()).round()
    }

    pub(super) fn underline_thickness(self) -> f32 {
        self.scale().max(1.0)
    }

    fn scale(self) -> f32 {
        self.font_size / FONT_SIZE
    }
}

pub(super) fn draw_cell(
    frame: &mut canvas::Frame<Renderer>,
    cell: &Cell,
    x: f32,
    y: f32,
    focused: bool,
    metrics: PageMetrics,
    default_fg: Option<Color>,
) {
    if !x.is_finite()
        || !y.is_finite()
        || !metrics.cell_width.is_finite()
        || !metrics.cell_height.is_finite()
        || !metrics.font_size.is_finite()
        || x.abs() > 1_000_000.0
        || y.abs() > 1_000_000.0
    {
        return;
    }

    let (fg, bg) = cell_colors(cell, focused, default_fg);

    if let Some(bg) = bg {
        frame.fill_rectangle(
            Point::new(x, y),
            Size::new(metrics.cell_width, metrics.cell_height),
            bg,
        );
    }
    if cell.ch != ' ' && !micron_canvas_text_disabled() {
        let Some(ch) = safe_canvas_cell_char(cell.ch) else {
            return;
        };
        let weight_offset = if cell.style.bold { 0.7 } else { 0.0 };
        let font = micron_canvas_font_for_char(ch);
        let shaping = micron_canvas_shaping_for_char(ch);
        frame.fill_text(canvas::Text {
            content: ch.to_string(),
            position: Point::new(x, y),
            color: fg,
            size: Pixels(metrics.font_size),
            line_height: iced::widget::text::LineHeight::Absolute(Pixels(metrics.cell_height)),
            font,
            max_width: f32::INFINITY,
            align_x: alignment::Horizontal::Left.into(),
            align_y: alignment::Vertical::Top,
            shaping,
        });
        if cell.style.bold && !is_canvas_cell_emoji(ch) {
            frame.fill_text(canvas::Text {
                content: ch.to_string(),
                position: Point::new(x + weight_offset, y),
                color: fg,
                size: Pixels(metrics.font_size),
                line_height: iced::widget::text::LineHeight::Absolute(Pixels(metrics.cell_height)),
                font,
                max_width: f32::INFINITY,
                align_x: alignment::Horizontal::Left.into(),
                align_y: alignment::Vertical::Top,
                shaping,
            });
        }
    }
    if cell.style.underline {
        frame.fill_rectangle(
            Point::new(x, y + metrics.underline_offset()),
            Size::new(metrics.cell_width, metrics.underline_thickness()),
            fg,
        );
    }
}

fn micron_canvas_text_disabled() -> bool {
    std::env::var_os("OMEN_DISABLE_MICRON_CANVAS_TEXT").is_some()
}

pub(super) fn safe_canvas_cell_char(ch: char) -> Option<char> {
    if ch.is_control() {
        return None;
    }
    match UnicodeWidthChar::width(ch) {
        Some(0) => return None,
        Some(1) => {}
        _ if is_canvas_cell_emoji(ch) => return Some(ch),
        _ => return Some('?'),
    }
    if is_canvas_cell_safe_char(ch) || is_canvas_cell_emoji(ch) {
        Some(ch)
    } else {
        Some('?')
    }
}

fn is_canvas_cell_safe_char(ch: char) -> bool {
    matches!(
        ch as u32,
        0x20..=0x7e
            | 0xa0..=0x024f
            | 0x0370..=0x052f
            | 0x2000..=0x206f
            | 0x2190..=0x21ff
            | 0x2500..=0x259f
            | 0x25a0..=0x25ff
            | 0x2800..=0x28ff
    )
}

pub(super) fn is_canvas_cell_emoji(ch: char) -> bool {
    matches!(
        ch as u32,
        0x1f000..=0x1f02f
            | 0x1f0a0..=0x1f0ff
            | 0x1f100..=0x1f1ff
            | 0x1f200..=0x1f2ff
            | 0x1f300..=0x1f5ff
            | 0x1f600..=0x1f64f
            | 0x1f680..=0x1f6ff
            | 0x1f700..=0x1f77f
            | 0x1f780..=0x1f7ff
            | 0x1f800..=0x1f8ff
            | 0x1f900..=0x1f9ff
            | 0x1fa70..=0x1faff
            | 0x2600..=0x26ff
            | 0x2700..=0x27bf
    )
}

pub(super) fn cell_colors(
    cell: &Cell,
    focused: bool,
    default_fg: Option<Color>,
) -> (Color, Option<Color>) {
    let authored_fg = cell
        .style
        .fg
        .as_deref()
        .filter(|fg| !(default_fg.is_some() && fg.eq_ignore_ascii_case(DEFAULT_FG_DARK)))
        .and_then(|fg| color_from_style(Some(fg)));
    let mut fg = authored_fg
        .or(default_fg)
        .unwrap_or(Color::from_rgb8(204, 204, 204));
    let mut bg = color_from_style(cell.style.bg.as_deref());

    if focused {
        bg = Some(Color::from_rgb8(48, 210, 190));
        fg = Color::from_rgb8(0, 0, 0);
    }
    if cell.cursor {
        bg = Some(Color::WHITE);
        fg = Color::BLACK;
    }

    (fg, bg)
}

fn micron_canvas_font() -> Font {
    Font::with_name("Adwaita Mono")
}

fn micron_canvas_font_for_char(ch: char) -> Font {
    if is_canvas_cell_emoji(ch) {
        emoji_font()
    } else {
        micron_canvas_font()
    }
}

fn micron_canvas_shaping_for_char(ch: char) -> iced::widget::text::Shaping {
    if is_canvas_cell_emoji(ch) {
        iced::widget::text::Shaping::Advanced
    } else {
        iced::widget::text::Shaping::Basic
    }
}

pub(super) fn is_focused(
    cell: &Cell,
    focused_control: &Option<String>,
    focused_link: &Option<String>,
) -> bool {
    cell.control
        .as_ref()
        .is_some_and(|control| focused_control.as_deref() == Some(control.name.as_str()))
        || cell
            .link
            .as_ref()
            .is_some_and(|link| focused_link.as_deref() == Some(link.target.as_str()))
}

pub(super) fn width_cells_for_bounds(bounds: Rectangle) -> usize {
    ((bounds.width - PADDING * 2.0 - RIGHT_SCROLL_GUTTER).max(CELL_WIDTH) / CELL_WIDTH)
        .floor()
        .max(1.0) as usize
}

pub(super) fn height_rows_for_bounds(bounds: Rectangle) -> usize {
    ((bounds.height - PADDING * 2.0) / CELL_HEIGHT)
        .floor()
        .max(1.0) as usize
}

pub(super) fn clamped_scroll_offset(
    requested: usize,
    row_count: usize,
    visible_rows: usize,
) -> usize {
    requested.min(row_count.saturating_sub(visible_rows.max(1)))
}

pub(super) fn cell_at(position: Point, row_count: usize, width: usize) -> Option<(u16, u16)> {
    if position.x < PADDING || position.y < PADDING {
        return None;
    }
    let row = ((position.y - PADDING) / CELL_HEIGHT).floor() as usize;
    let col = ((position.x - PADDING) / CELL_WIDTH).floor() as usize;
    if row >= row_count || col >= width {
        return None;
    }
    Some((row as u16, col as u16))
}

pub(crate) fn color_from_style(color: Option<&str>) -> Option<Color> {
    let color = color?;
    if color == "default" || color == crate::micron::parser::DEFAULT_BG {
        return None;
    }
    if color.len() == 3 && color.starts_with('g') {
        let level = color[1..].parse::<u16>().ok()?.min(99);
        let value = ((level * 255) / 99) as u8;
        return Some(Color::from_rgb8(value, value, value));
    }
    if color.len() == 3 {
        let red = u8::from_str_radix(&color[0..1].repeat(2), 16).ok()?;
        let green = u8::from_str_radix(&color[1..2].repeat(2), 16).ok()?;
        let blue = u8::from_str_radix(&color[2..3].repeat(2), 16).ok()?;
        return Some(Color::from_rgb8(red, green, blue));
    }
    if color.len() == 6 {
        let red = u8::from_str_radix(&color[0..2], 16).ok()?;
        let green = u8::from_str_radix(&color[2..4], 16).ok()?;
        let blue = u8::from_str_radix(&color[4..6], 16).ok()?;
        return Some(Color::from_rgb8(red, green, blue));
    }
    match color {
        "red" | "r" => Some(Color::from_rgb8(255, 85, 85)),
        "green" | "g" => Some(Color::from_rgb8(90, 220, 120)),
        "yellow" | "y" => Some(Color::from_rgb8(255, 209, 102)),
        "blue" | "b" => Some(Color::from_rgb8(92, 172, 255)),
        "magenta" | "m" => Some(Color::from_rgb8(255, 120, 220)),
        "cyan" | "c" => Some(Color::from_rgb8(78, 203, 255)),
        "white" | "w" => Some(Color::WHITE),
        "black" | "k" => Some(Color::BLACK),
        _ => None,
    }
}
