use iced::widget::scrollable::Direction as ScrollableDirection;
use iced::widget::{container, scrollable, Scrollable};
use iced::{Element, Length, Padding};

use super::super::{
    compact_scrollbar, desktop_scroll_gutter_right, themed_scrollable_style, Message,
    DESKTOP_SCROLL_OUTER_INSET,
};

pub(in crate::desktop) fn app_scrollable<'a>(
    content: impl Into<Element<'a, Message>>,
) -> Scrollable<'a, Message> {
    scrollable(scroll_outer_inset(scroll_gutter(content)))
        .direction(ScrollableDirection::Vertical(compact_scrollbar()))
        .style(themed_scrollable_style)
        .width(Length::Fill)
}

fn scroll_outer_inset<'a>(content: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    container(content)
        .padding(Padding {
            right: f32::from(DESKTOP_SCROLL_OUTER_INSET),
            ..Padding::default()
        })
        .width(Length::Fill)
        .into()
}

fn scroll_gutter<'a>(content: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    container(content)
        .padding(Padding {
            right: desktop_scroll_gutter_right(),
            ..Padding::default()
        })
        .width(Length::Fill)
        .into()
}
