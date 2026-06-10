use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

use crate::app::BrowserTab;
use crate::messaging::Conversation;

pub fn browser_tab_line(tabs: &[BrowserTab], active_index: usize) -> Line<'static> {
    let spans: Vec<Span<'static>> = tabs
        .iter()
        .enumerate()
        .flat_map(|(index, tab)| {
            let style = if index == active_index {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Gray)
            };
            [
                Span::raw(" "),
                Span::styled(format!("[{}]", tab.title), style),
            ]
        })
        .collect();
    Line::from(spans)
}

pub fn conversation_tab_line(tabs: &[Conversation], active_index: usize) -> Line<'static> {
    let spans: Vec<Span<'static>> = tabs
        .iter()
        .enumerate()
        .flat_map(|(index, tab)| {
            let style = if index == active_index {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Green)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Gray)
            };
            [
                Span::raw(" "),
                Span::styled(format!("[{}]", tab.peer_label), style),
            ]
        })
        .collect();
    Line::from(spans)
}
