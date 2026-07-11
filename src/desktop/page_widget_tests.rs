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
fn canvas_metrics_cap_absurd_layout_bounds() {
    let metrics = PageMetrics::new(100);
    let huge = Rectangle {
        x: 0.0,
        y: 0.0,
        width: f32::INFINITY,
        height: f32::INFINITY,
    };

    assert_eq!(metrics.width_cells_for_bounds(huge), MAX_CANVAS_COLS);
    assert_eq!(metrics.height_rows_for_bounds(huge), MAX_CANVAS_ROWS);

    let huge_finite = Rectangle {
        x: 0.0,
        y: 0.0,
        width: f32::MAX,
        height: f32::MAX,
    };
    assert_eq!(metrics.width_cells_for_bounds(huge_finite), MAX_CANVAS_COLS);
    assert_eq!(metrics.height_rows_for_bounds(huge_finite), MAX_CANVAS_ROWS);
}

#[test]
fn visible_scroll_offset_is_clamped_to_rendered_rows() {
    assert_eq!(clamped_scroll_offset(0, 10, 4), 0);
    assert_eq!(clamped_scroll_offset(6, 10, 4), 6);
    assert_eq!(clamped_scroll_offset(7, 10, 4), 6);
    assert_eq!(clamped_scroll_offset(usize::MAX, 10, 4), 6);
    assert_eq!(clamped_scroll_offset(usize::MAX, 3, 10), 0);
}

#[test]
fn canvas_cell_chars_are_restricted_to_single_cell_safe_glyphs() {
    assert_eq!(safe_canvas_cell_char('A'), Some('A'));
    assert_eq!(safe_canvas_cell_char('█'), Some('█'));
    assert_eq!(safe_canvas_cell_char('═'), Some('═'));
    assert_eq!(safe_canvas_cell_char('☠'), Some('☠'));
    assert_eq!(safe_canvas_cell_char('中'), Some('?'));
    assert_eq!(safe_canvas_cell_char('😊'), Some('😊'));
    assert_eq!(safe_canvas_cell_char('\u{fe0f}'), None);
    assert_eq!(safe_canvas_cell_char('\n'), None);
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

    let action = <NomadNetPageProgram as canvas::Program<PageMessage>>::update(
        &program,
        &mut (),
        &canvas::Event::Mouse(mouse::Event::WheelScrolled {
            delta: mouse::ScrollDelta::Lines { x: 0.0, y: -1.0 },
        }),
        bounds,
        mouse::Cursor::Available(Point::new(500.0, 500.0)),
    );

    assert!(action.is_none());
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
