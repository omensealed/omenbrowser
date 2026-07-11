use iced::widget::{button, text, Button};

use super::super::{nerd_icon_font, subtle_button_style, ui_size, warning_button_style, Message};

pub(in crate::desktop) fn restore_pane_button(
    icon: &'static str,
    label: String,
    message: Message,
    unread: bool,
) -> Button<'static, Message> {
    let style = if unread {
        warning_button_style
    } else {
        subtle_button_style
    };
    button(
        text(format!("{icon} {label}"))
            .font(nerd_icon_font())
            .size(ui_size(15)),
    )
    .on_press(message)
    .style(style)
}
