use iced::widget::text::Wrapping;
use iced::widget::{text, Text};
use iced::Length;

use super::super::{content_font, printable_label, ui_size};

pub(in crate::desktop) fn safe_timeline_text<'a>(
    content: impl Into<String>,
    size: u16,
) -> Text<'a> {
    const MAX_TIMELINE_TEXT_CHARS: usize = 16_384;
    let mut content = printable_label(&content.into());
    if content.chars().count() > MAX_TIMELINE_TEXT_CHARS {
        content = format!(
            "{}\n[message preview truncated for renderer safety]",
            content
                .chars()
                .take(MAX_TIMELINE_TEXT_CHARS)
                .collect::<String>()
        );
    }
    let font = content_font(&content);
    text(content)
        .font(font)
        .size(ui_size(size))
        .wrapping(Wrapping::WordOrGlyph)
        .width(Length::Fill)
}
