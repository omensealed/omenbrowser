use iced::mouse;
use iced::widget::canvas::{self, Canvas};
use iced::{Color, Element, Length, Point, Rectangle, Renderer, Size, Theme};
use std::rc::Rc;

use crate::micron::parser::TextStyle;
#[cfg(test)]
use crate::micron::render::render_document;
use crate::micron::render::{render_document_with_field_cursor, Cell, HitAction, RenderedRow};
use crate::micron::Document;

pub(crate) use super::page_widget_canvas::color_from_style;
#[cfg(test)]
use super::page_widget_canvas::{
    cell_at, cell_colors, height_rows_for_bounds, safe_canvas_cell_char, width_cells_for_bounds,
    CELL_HEIGHT, CELL_WIDTH, MAX_CANVAS_COLS, MAX_CANVAS_ROWS, RIGHT_SCROLL_GUTTER,
};
use super::page_widget_canvas::{
    clamped_scroll_offset, draw_cell, is_focused, PageMetrics, PADDING,
};

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
        event: &canvas::Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<canvas::Action<PageMessage>> {
        match event {
            canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                let metrics = PageMetrics::new(self.zoom_percent);
                let width = metrics.width_cells_for_bounds(bounds);
                let rows = self.rendered_rows(width);
                let Some(position) = cursor.position_in(bounds) else {
                    return None;
                };
                let first_row = clamped_scroll_offset(
                    self.scroll_offset,
                    rows.len(),
                    metrics.height_rows_for_bounds(bounds),
                );
                let visible_rows = rows.len().saturating_sub(first_row);
                let Some((visible_row, col)) = metrics.cell_at(position, visible_rows, width)
                else {
                    return None;
                };
                let document_row = first_row.saturating_add(visible_row as usize);
                let Some(cell) = rows
                    .get(document_row)
                    .and_then(|rendered| rendered.cells.get(col as usize))
                else {
                    return None;
                };
                if cell.link.is_none() && cell.control.is_none() {
                    return None;
                }
                let action = cell
                    .link
                    .clone()
                    .map(HitAction::Link)
                    .or_else(|| cell.control.clone().map(HitAction::Control));
                if let Some(action) = action {
                    Some(
                        canvas::Action::publish(PageMessage::Activate {
                            row: visible_row,
                            col,
                            width,
                            action,
                        })
                        .and_capture(),
                    )
                } else {
                    None
                }
            }
            canvas::Event::Mouse(mouse::Event::WheelScrolled { delta }) => {
                if cursor.position_in(bounds).is_none() {
                    return None;
                }
                let wheel_delta = match delta {
                    mouse::ScrollDelta::Lines { y, .. } => *y,
                    mouse::ScrollDelta::Pixels { y, .. } => {
                        y / PageMetrics::new(self.zoom_percent).cell_height
                    }
                };
                if wheel_delta.abs() < f32::EPSILON {
                    return None;
                }
                Some(
                    canvas::Action::publish(PageMessage::Scroll {
                        delta: -wheel_delta.round() as isize,
                        width: PageMetrics::new(self.zoom_percent).width_cells_for_bounds(bounds),
                        height: PageMetrics::new(self.zoom_percent).height_rows_for_bounds(bounds),
                    })
                    .and_capture(),
                )
            }
            _ => None,
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

        let visible_rows = metrics.height_rows_for_bounds(bounds);
        let first_row = clamped_scroll_offset(self.scroll_offset, rows.len(), visible_rows);

        for (visible_row_index, row) in rows.iter().skip(first_row).take(visible_rows).enumerate() {
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
        let first_row = clamped_scroll_offset(
            self.scroll_offset,
            rows.len(),
            metrics.height_rows_for_bounds(bounds),
        );
        let visible_rows = rows.len().saturating_sub(first_row);
        let Some((visible_row, col)) = metrics.cell_at(position, visible_rows, width) else {
            return mouse::Interaction::default();
        };
        let document_row = first_row.saturating_add(visible_row as usize);
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

#[cfg(test)]
#[path = "page_widget_tests.rs"]
mod tests;
