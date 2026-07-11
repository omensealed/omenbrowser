use iced::widget::{button, container, text, tooltip, Button};
use iced::{Element, Length, Pixels};

use super::super::{
    inline_icon_button_style, nerd_icon_font, omen_button_style, status_container_style,
    subtle_button_style, ui_size, warning_button_style, Message,
};

pub(in crate::desktop) fn tooltip_icon_button<'a>(
    icon: &'a str,
    label: &'static str,
    message: Message,
) -> Element<'a, Message> {
    tooltip_button(
        button(centered_toolbar_icon(icon))
            .on_press(message)
            .padding(0)
            .width(Length::Fixed(toolbar_icon_button_side()))
            .height(Length::Fixed(toolbar_icon_button_side()))
            .style(subtle_button_style),
        label,
    )
}

pub(in crate::desktop) fn tooltip_omen_icon_button<'a>(
    icon: &'a str,
    label: &'static str,
    message: Message,
) -> Element<'a, Message> {
    tooltip_button(
        button(centered_toolbar_icon(icon))
            .on_press(message)
            .padding(0)
            .width(Length::Fixed(toolbar_icon_button_side()))
            .height(Length::Fixed(toolbar_icon_button_side()))
            .style(omen_button_style),
        label,
    )
}

pub(in crate::desktop) fn tooltip_warning_icon_button<'a>(
    icon: &'a str,
    label: &'static str,
    message: Message,
) -> Element<'a, Message> {
    tooltip_button(
        button(centered_toolbar_icon(icon))
            .on_press(message)
            .padding(0)
            .width(Length::Fixed(toolbar_icon_button_side()))
            .height(Length::Fixed(toolbar_icon_button_side()))
            .style(warning_button_style),
        label,
    )
}

pub(in crate::desktop) fn centered_toolbar_icon(icon: &str) -> Element<'_, Message> {
    let side = toolbar_icon_content_side();
    container(
        text(format!("{icon} "))
            .font(nerd_icon_font())
            .size(ui_size(16)),
    )
    .center_x(Length::Fixed(side))
    .center_y(Length::Fixed(side))
    .into()
}

pub(in crate::desktop) fn toolbar_icon_button_side() -> f32 {
    (ui_size(30) as f32).max(26.0)
}

fn toolbar_icon_content_side() -> f32 {
    (toolbar_icon_button_side() - 4.0).max(22.0)
}

pub(in crate::desktop) fn tooltip_button<'a>(
    button: Button<'a, Message>,
    label: &'static str,
) -> Element<'a, Message> {
    tooltip(
        button,
        container(text(label).size(ui_size(12)))
            .padding([4, 8])
            .style(status_container_style),
        tooltip::Position::Top,
    )
    .gap(Pixels(8.0))
    .into()
}

pub(in crate::desktop) fn inline_icon_button_owned(
    icon: &'static str,
    label: &'static str,
    message: Message,
) -> Element<'static, Message> {
    tooltip_button(
        button(
            text(format!("{icon} "))
                .font(nerd_icon_font())
                .size(ui_size(14)),
        )
        .on_press(message)
        .padding([0, 2])
        .style(inline_icon_button_style),
        label,
    )
}
