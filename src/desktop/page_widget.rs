use iced::mouse;
use iced::widget::canvas::{self, event, Canvas};
use iced::{
    alignment, Color, Element, Font, Length, Pixels, Point, Rectangle, Renderer, Size, Theme,
};
use std::rc::Rc;

use crate::micron::parser::{TextStyle, DEFAULT_FG_DARK};
#[cfg(test)]
use crate::micron::render::render_document;
use crate::micron::render::{render_document_with_field_cursor, Cell, HitAction, RenderedRow};
use crate::micron::Document;

const CELL_WIDTH: f32 = 9.0;
const CELL_HEIGHT: f32 = 18.0;
const FONT_SIZE: f32 = 15.0;
const PADDING: f32 = 10.0;
const RIGHT_SCROLL_GUTTER: f32 = 14.0;

#[derive(Clone, Debug)]
pub enum PageMessage {
    Activate {
        row: u16,
        col: u16,
        width: usize,
        action: HitAction,
    },
    Scroll {
        delta: isize,
        width: usize,
        height: usize,
    },
}

pub fn nomadnet_page<'a, Message>(
    document: Option<&Document>,
    fallback: Option<&str>,
    scroll_offset: usize,
    focused_control: Option<&str>,
    focused_link: Option<&str>,
    field_cursor: Option<(&str, usize)>,
    map: impl Fn(PageMessage) -> Message + 'a,
) -> Element<'a, Message>
where
    Message: Clone + 'a,
{
    let program = NomadNetPageProgram {
        document: document.cloned(),
        fallback: fallback.map(str::to_string),
        scroll_offset,
        focused_control: focused_control.map(str::to_string),
        focused_link: focused_link.map(str::to_string),
        field_cursor: field_cursor.map(|(name, cursor)| (name.to_string(), cursor)),
        rendered_rows: None,
        row_renderer: None,
        zoom_percent: 100,
    };

    let page: Element<'a, PageMessage> = Canvas::new(program)
        .width(Length::Fill)
        .height(Length::Fill)
        .into();
    page.map(map)
}

pub fn nomadnet_page_with_rendered_rows<'a, Message>(
    props: NomadNetPageProps<'_>,
    map: impl Fn(PageMessage) -> Message + 'a,
) -> Element<'a, Message>
where
    Message: Clone + 'a,
{
    let program = NomadNetPageProgram {
        document: props.document.cloned(),
        fallback: props.fallback.map(str::to_string),
        scroll_offset: props.scroll_offset,
        focused_control: props.focused_control.map(str::to_string),
        focused_link: props.focused_link.map(str::to_string),
        field_cursor: props
            .field_cursor
            .map(|(name, cursor)| (name.to_string(), cursor)),
        rendered_rows: props.rendered_rows,
        row_renderer: None,
        zoom_percent: props.zoom_percent.clamp(50, 200),
    };

    let page: Element<'a, PageMessage> = Canvas::new(program)
        .width(Length::Fill)
        .height(Length::Fill)
        .into();
    page.map(map)
}

pub fn nomadnet_page_with_row_renderer<'a, Message>(
    props: NomadNetPageProps<'_>,
    row_renderer: impl Fn(usize) -> Option<Vec<RenderedRow>> + 'a,
    map: impl Fn(PageMessage) -> Message + 'a,
) -> Element<'a, Message>
where
    Message: Clone + 'a,
{
    let program = NomadNetPageProgram {
        document: props.document.cloned(),
        fallback: props.fallback.map(str::to_string),
        scroll_offset: props.scroll_offset,
        focused_control: props.focused_control.map(str::to_string),
        focused_link: props.focused_link.map(str::to_string),
        field_cursor: props
            .field_cursor
            .map(|(name, cursor)| (name.to_string(), cursor)),
        rendered_rows: props.rendered_rows,
        row_renderer: Some(Rc::new(row_renderer)),
        zoom_percent: props.zoom_percent.clamp(50, 200),
    };

    let page: Element<'a, PageMessage> = Canvas::new(program)
        .width(Length::Fill)
        .height(Length::Fill)
        .into();
    page.map(map)
}

pub struct NomadNetPageProps<'a> {
    pub document: Option<&'a Document>,
    pub rendered_rows: Option<Vec<RenderedRow>>,
    pub fallback: Option<&'a str>,
    pub scroll_offset: usize,
    pub focused_control: Option<&'a str>,
    pub focused_link: Option<&'a str>,
    pub field_cursor: Option<(&'a str, usize)>,
    pub zoom_percent: u16,
}

fn fallback_rows(fallback: Option<&str>) -> Vec<RenderedRow> {
    fallback
        .unwrap_or("No page loaded yet.")
        .lines()
        .map(|line| RenderedRow {
            cells: line
                .chars()
                .map(|ch| Cell {
                    ch,
                    style: TextStyle::default(),
                    link: None,
                    control: None,
                    cursor: false,
                })
                .collect(),
            align: crate::micron::parser::Alignment::Left,
            depth: 0,
            base_style: TextStyle::default(),
            wrap: false,
        })
        .collect()
}

type RowRenderer<'a> = Rc<dyn Fn(usize) -> Option<Vec<RenderedRow>> + 'a>;

#[derive(Clone)]
struct NomadNetPageProgram<'a> {
    document: Option<Document>,
    fallback: Option<String>,
    scroll_offset: usize,
    focused_control: Option<String>,
    focused_link: Option<String>,
    field_cursor: Option<(String, usize)>,
    rendered_rows: Option<Vec<RenderedRow>>,
    row_renderer: Option<RowRenderer<'a>>,
    zoom_percent: u16,
}

impl<'a> canvas::Program<PageMessage> for NomadNetPageProgram<'a> {
    type State = ();

    fn update(
        &self,
        _state: &mut Self::State,
        event: canvas::Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> (event::Status, Option<PageMessage>) {
        match event {
            canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                let metrics = PageMetrics::new(self.zoom_percent);
                let width = metrics.width_cells_for_bounds(bounds);
                let rows = self.rendered_rows(width);
                let Some(position) = cursor.position_in(bounds) else {
                    return (event::Status::Ignored, None);
                };
                let Some((visible_row, col)) = metrics.cell_at(position, rows.len(), width) else {
                    return (event::Status::Ignored, None);
                };
                let document_row = self.scroll_offset.saturating_add(visible_row as usize);
                let Some(cell) = rows
                    .get(document_row)
                    .and_then(|rendered| rendered.cells.get(col as usize))
                else {
                    return (event::Status::Ignored, None);
                };
                if cell.link.is_none() && cell.control.is_none() {
                    return (event::Status::Ignored, None);
                }
                let action = cell
                    .link
                    .clone()
                    .map(HitAction::Link)
                    .or_else(|| cell.control.clone().map(HitAction::Control));
                if let Some(action) = action {
                    (
                        event::Status::Captured,
                        Some(PageMessage::Activate {
                            row: visible_row,
                            col,
                            width,
                            action,
                        }),
                    )
                } else {
                    (event::Status::Ignored, None)
                }
            }
            canvas::Event::Mouse(mouse::Event::WheelScrolled { delta }) => {
                if cursor.position_in(bounds).is_none() {
                    return (event::Status::Ignored, None);
                }
                let wheel_delta = match delta {
                    mouse::ScrollDelta::Lines { y, .. } => y,
                    mouse::ScrollDelta::Pixels { y, .. } => {
                        y / PageMetrics::new(self.zoom_percent).cell_height
                    }
                };
                if wheel_delta.abs() < f32::EPSILON {
                    return (event::Status::Ignored, None);
                }
                (
                    event::Status::Captured,
                    Some(PageMessage::Scroll {
                        delta: -wheel_delta.round() as isize,
                        width: PageMetrics::new(self.zoom_percent).width_cells_for_bounds(bounds),
                        height: PageMetrics::new(self.zoom_percent).height_rows_for_bounds(bounds),
                    }),
                )
            }
            _ => (event::Status::Ignored, None),
        }
    }

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let metrics = PageMetrics::new(self.zoom_percent);
        let rows = self.rendered_rows(metrics.width_cells_for_bounds(bounds));
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        let page_bg = self
            .document
            .as_ref()
            .and_then(|document| document.metadata.get("bg"))
            .and_then(|color| color_from_style(Some(color.as_str())))
            .unwrap_or(Color::BLACK);
        let page_fg = self
            .document
            .as_ref()
            .and_then(|document| document.metadata.get("fg"))
            .and_then(|color| color_from_style(Some(color.as_str())));
        frame.fill_rectangle(Point::ORIGIN, bounds.size(), page_bg);
        let content_width = metrics.content_width_for_bounds(bounds);

        for (visible_row_index, row) in rows.iter().skip(self.scroll_offset).enumerate() {
            let y = PADDING + visible_row_index as f32 * metrics.cell_height;
            if y > bounds.height {
                break;
            }
            if let Some(bg) = color_from_style(row.base_style.bg.as_deref()) {
                frame.fill_rectangle(
                    Point::new(PADDING, y),
                    Size::new(content_width, metrics.cell_height),
                    bg,
                );
            }
            for (col_index, cell) in row.cells.iter().enumerate() {
                let x = PADDING + col_index as f32 * metrics.cell_width;
                if x >= PADDING + content_width {
                    break;
                }
                draw_cell(
                    &mut frame,
                    cell,
                    x,
                    y,
                    is_focused(cell, &self.focused_control, &self.focused_link),
                    metrics,
                    page_fg,
                );
            }
        }

        vec![frame.into_geometry()]
    }

    fn mouse_interaction(
        &self,
        _state: &Self::State,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        let Some(position) = cursor.position_in(bounds) else {
            return mouse::Interaction::default();
        };
        let metrics = PageMetrics::new(self.zoom_percent);
        let width = metrics.width_cells_for_bounds(bounds);
        let rows = self.rendered_rows(width);
        let Some((visible_row, col)) = metrics.cell_at(position, rows.len(), width) else {
            return mouse::Interaction::default();
        };
        let document_row = self.scroll_offset.saturating_add(visible_row as usize);
        let actionable = self
            .rendered_rows(width)
            .get(document_row)
            .and_then(|rendered| rendered.cells.get(col as usize))
            .is_some_and(|cell| cell.link.is_some() || cell.control.is_some());
        if actionable {
            mouse::Interaction::Pointer
        } else {
            mouse::Interaction::default()
        }
    }
}

impl<'a> NomadNetPageProgram<'a> {
    fn rendered_rows(&self, width: usize) -> Vec<RenderedRow> {
        if let Some(render) = &self.row_renderer {
            if let Some(rows) = render(width) {
                return rows;
            }
        }
        if let Some(rows) = &self.rendered_rows {
            return rows.clone();
        }
        self.document
            .as_ref()
            .map(|document| {
                render_document_with_field_cursor(
                    document,
                    width,
                    self.focused_control.as_deref(),
                    self.field_cursor
                        .as_ref()
                        .map(|(name, cursor)| (name.as_str(), *cursor)),
                )
            })
            .unwrap_or_else(|| fallback_rows(self.fallback.as_deref()))
    }
}

#[derive(Clone, Copy, Debug)]
struct PageMetrics {
    cell_width: f32,
    cell_height: f32,
    font_size: f32,
}

impl PageMetrics {
    fn new(zoom_percent: u16) -> Self {
        let scale = zoom_percent.clamp(50, 200) as f32 / 100.0;
        Self {
            cell_width: CELL_WIDTH * scale,
            cell_height: CELL_HEIGHT * scale,
            font_size: FONT_SIZE * scale,
        }
    }

    fn width_cells_for_bounds(self, bounds: Rectangle) -> usize {
        (self.content_width_for_bounds(bounds) / self.cell_width)
            .floor()
            .max(1.0) as usize
    }

    fn content_width_for_bounds(self, bounds: Rectangle) -> f32 {
        (bounds.width - PADDING * 2.0 - RIGHT_SCROLL_GUTTER).max(self.cell_width)
    }

    fn height_rows_for_bounds(self, bounds: Rectangle) -> usize {
        ((bounds.height - PADDING * 2.0) / self.cell_height)
            .floor()
            .max(1.0) as usize
    }

    fn cell_at(self, position: Point, row_count: usize, width: usize) -> Option<(u16, u16)> {
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

    fn underline_offset(self) -> f32 {
        (self.cell_height - 3.0 * self.scale()).round()
    }

    fn underline_thickness(self) -> f32 {
        self.scale().max(1.0)
    }

    fn scale(self) -> f32 {
        self.font_size / FONT_SIZE
    }
}

fn draw_cell(
    frame: &mut canvas::Frame<Renderer>,
    cell: &Cell,
    x: f32,
    y: f32,
    focused: bool,
    metrics: PageMetrics,
    default_fg: Option<Color>,
) {
    let (fg, bg) = cell_colors(cell, focused, default_fg);

    if let Some(bg) = bg {
        frame.fill_rectangle(
            Point::new(x, y),
            Size::new(metrics.cell_width, metrics.cell_height),
            bg,
        );
    }
    if cell.ch != ' ' {
        let weight_offset = if cell.style.bold { 0.7 } else { 0.0 };
        frame.fill_text(canvas::Text {
            content: cell.ch.to_string(),
            position: Point::new(x, y),
            color: fg,
            size: Pixels(metrics.font_size),
            line_height: iced::widget::text::LineHeight::Absolute(Pixels(metrics.cell_height)),
            font: micron_canvas_font(),
            horizontal_alignment: alignment::Horizontal::Left,
            vertical_alignment: alignment::Vertical::Top,
            shaping: iced::widget::text::Shaping::Advanced,
        });
        if cell.style.bold {
            frame.fill_text(canvas::Text {
                content: cell.ch.to_string(),
                position: Point::new(x + weight_offset, y),
                color: fg,
                size: Pixels(metrics.font_size),
                line_height: iced::widget::text::LineHeight::Absolute(Pixels(metrics.cell_height)),
                font: micron_canvas_font(),
                horizontal_alignment: alignment::Horizontal::Left,
                vertical_alignment: alignment::Vertical::Top,
                shaping: iced::widget::text::Shaping::Advanced,
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

fn cell_colors(cell: &Cell, focused: bool, default_fg: Option<Color>) -> (Color, Option<Color>) {
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

fn is_focused(
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

fn width_cells_for_bounds(bounds: Rectangle) -> usize {
    ((bounds.width - PADDING * 2.0 - RIGHT_SCROLL_GUTTER).max(CELL_WIDTH) / CELL_WIDTH)
        .floor()
        .max(1.0) as usize
}

fn height_rows_for_bounds(bounds: Rectangle) -> usize {
    ((bounds.height - PADDING * 2.0) / CELL_HEIGHT)
        .floor()
        .max(1.0) as usize
}

fn cell_at(position: Point, row_count: usize, width: usize) -> Option<(u16, u16)> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::micron::parse_micron;

    #[test]
    fn fallback_rows_preserve_plain_text_lines() {
        let rows = fallback_rows(Some("one\ntwo"));
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].text(), "one");
        assert_eq!(rows[1].text(), "two");
    }

    #[test]
    fn cell_coordinates_map_canvas_position_to_grid() {
        assert_eq!(cell_at(Point::new(PADDING, PADDING), 1, 100), Some((0, 0)));
        assert_eq!(
            cell_at(
                Point::new(PADDING + CELL_WIDTH * 2.2, PADDING + CELL_HEIGHT * 1.1),
                3,
                100
            ),
            Some((1, 2))
        );
        assert_eq!(cell_at(Point::new(PADDING - 1.0, PADDING), 1, 100), None);
        assert_eq!(
            cell_at(Point::new(PADDING + CELL_WIDTH * 9.0, PADDING), 1, 4),
            None
        );
    }

    #[test]
    fn canvas_bounds_determine_render_width_and_height() {
        let bounds = Rectangle {
            x: 0.0,
            y: 0.0,
            width: PADDING * 2.0 + RIGHT_SCROLL_GUTTER + CELL_WIDTH * 42.0,
            height: PADDING * 2.0 + CELL_HEIGHT * 12.0,
        };
        assert_eq!(width_cells_for_bounds(bounds), 42);
        assert_eq!(height_rows_for_bounds(bounds), 12);
    }

    #[test]
    fn canvas_width_reserves_right_scroll_gutter() {
        let without_gutter = Rectangle {
            x: 0.0,
            y: 0.0,
            width: PADDING * 2.0 + CELL_WIDTH * 42.0,
            height: PADDING * 2.0 + CELL_HEIGHT * 12.0,
        };

        assert!(width_cells_for_bounds(without_gutter) < 42);
    }

    #[test]
    fn width_aware_row_renderer_uses_current_canvas_width() {
        let program = NomadNetPageProgram {
            document: Some(parse_micron("`cCentered")),
            fallback: None,
            scroll_offset: 0,
            focused_control: None,
            focused_link: None,
            field_cursor: None,
            rendered_rows: Some(render_document(&parse_micron("`cCentered"), 9)),
            row_renderer: Some(Rc::new(|width| {
                Some(render_document(&parse_micron("`cCentered"), width))
            })),
            zoom_percent: 100,
        };

        let rows = program.rendered_rows(21);

        assert_eq!(rows[0].text(), "      Centered       ");
    }

    #[test]
    fn underline_offset_scales_with_micron_zoom() {
        let half = PageMetrics::new(50);
        let normal = PageMetrics::new(100);
        let double = PageMetrics::new(200);

        assert_eq!(half.underline_offset(), 8.0);
        assert_eq!(normal.underline_offset(), 15.0);
        assert_eq!(double.underline_offset(), 30.0);
        assert_eq!(half.underline_thickness(), 1.0);
        assert_eq!(double.underline_thickness(), 2.0);
    }

    #[test]
    fn wheel_scroll_is_ignored_when_cursor_is_outside_canvas_bounds() {
        let program = NomadNetPageProgram {
            document: Some(parse_micron("one\ntwo\nthree")),
            fallback: None,
            scroll_offset: 0,
            focused_control: None,
            focused_link: None,
            field_cursor: None,
            rendered_rows: None,
            row_renderer: None,
            zoom_percent: 100,
        };
        let bounds = Rectangle {
            x: 10.0,
            y: 10.0,
            width: 300.0,
            height: 200.0,
        };

        let (status, message) = <NomadNetPageProgram as canvas::Program<PageMessage>>::update(
            &program,
            &mut (),
            canvas::Event::Mouse(mouse::Event::WheelScrolled {
                delta: mouse::ScrollDelta::Lines { x: 0.0, y: -1.0 },
            }),
            bounds,
            mouse::Cursor::Available(Point::new(500.0, 500.0)),
        );

        assert!(matches!(status, event::Status::Ignored));
        assert!(message.is_none());
    }

    #[test]
    fn plain_page_cells_do_not_force_text_cursor() {
        let program = NomadNetPageProgram {
            document: Some(parse_micron("plain text")),
            fallback: None,
            scroll_offset: 0,
            focused_control: None,
            focused_link: None,
            field_cursor: None,
            rendered_rows: None,
            row_renderer: None,
            zoom_percent: 100,
        };
        let bounds = Rectangle {
            x: 0.0,
            y: 0.0,
            width: 300.0,
            height: 200.0,
        };

        let interaction = <NomadNetPageProgram as canvas::Program<PageMessage>>::mouse_interaction(
            &program,
            &(),
            bounds,
            mouse::Cursor::Available(Point::new(PADDING + 2.0, PADDING + 2.0)),
        );

        assert_eq!(interaction, mouse::Interaction::default());
    }

    #[test]
    fn actionable_page_cells_keep_pointer_cursor() {
        let program = NomadNetPageProgram {
            document: Some(parse_micron("`[Open`mock.node:/]")),
            fallback: None,
            scroll_offset: 0,
            focused_control: None,
            focused_link: None,
            field_cursor: None,
            rendered_rows: None,
            row_renderer: None,
            zoom_percent: 100,
        };
        let bounds = Rectangle {
            x: 0.0,
            y: 0.0,
            width: 300.0,
            height: 200.0,
        };

        let interaction = <NomadNetPageProgram as canvas::Program<PageMessage>>::mouse_interaction(
            &program,
            &(),
            bounds,
            mouse::Cursor::Available(Point::new(PADDING + 2.0, PADDING + 2.0)),
        );

        assert_eq!(interaction, mouse::Interaction::Pointer);
    }

    #[test]
    fn rendered_document_exposes_actionable_cells_for_canvas() {
        let document = parse_micron("`[Open`mock.node:/]");
        let rows = render_document(&document, 100);
        assert!(rows
            .iter()
            .flat_map(|row| row.cells.iter())
            .any(|cell| cell.link.is_some()));
    }

    #[test]
    fn desktop_color_conversion_matches_micron_short_colors() {
        assert_eq!(
            color_from_style(Some("abc")),
            Some(Color::from_rgb8(0xaa, 0xbb, 0xcc))
        );
        assert_eq!(
            color_from_style(Some("g50")),
            Some(Color::from_rgb8(128, 128, 128))
        );
        assert_eq!(
            color_from_style(Some("112233")),
            Some(Color::from_rgb8(0x11, 0x22, 0x33))
        );
        assert_eq!(color_from_style(Some("default")), None);
    }

    #[test]
    fn actionable_cells_keep_page_authored_colors_until_focused() {
        let document = parse_micron("`Ff00`[Open`mock.node:/] `B444`<8|name`guest>");
        let rows = render_document(&document, 100);
        let link_cell = rows[0]
            .cells
            .iter()
            .find(|cell| cell.link.is_some())
            .expect("link cell");
        let control_cell = rows[0]
            .cells
            .iter()
            .find(|cell| cell.control.is_some())
            .expect("control cell");

        assert_eq!(
            cell_colors(link_cell, false, None).0,
            Color::from_rgb8(0xff, 0x00, 0x00)
        );
        assert_eq!(cell_colors(control_cell, false, None).0, Color::WHITE);
        assert_eq!(
            cell_colors(control_cell, false, None).1,
            Some(Color::from_rgb8(0x44, 0x44, 0x44))
        );
        assert_eq!(cell_colors(link_cell, true, None).0, Color::BLACK);
    }

    #[test]
    fn unstyled_cells_inherit_document_default_foreground() {
        let cell = Cell {
            ch: 'x',
            style: TextStyle::default(),
            link: None,
            control: None,
            cursor: false,
        };

        assert_eq!(
            cell_colors(&cell, false, Some(Color::from_rgb8(0x11, 0x22, 0x33))).0,
            Color::from_rgb8(0x11, 0x22, 0x33)
        );
    }
}
