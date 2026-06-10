use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};

use crate::app::StatusModel;

pub fn status_line(status: &StatusModel) -> Line<'static> {
    Line::from(vec![
        Span::styled(status.identity.clone(), Style::default().fg(Color::Cyan)),
        Span::raw(" | "),
        Span::styled(status.backend.clone(), Style::default().fg(Color::Yellow)),
        Span::raw(" | "),
        Span::raw(status.destination.clone()),
        Span::raw(" | "),
        Span::raw(status.propagation.clone()),
        Span::raw(" | "),
        Span::styled(status.task.clone(), Style::default().fg(Color::Green)),
    ])
}
