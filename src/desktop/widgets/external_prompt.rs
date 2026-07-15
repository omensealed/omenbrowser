use iced::widget::text::Wrapping;
use iced::widget::{button, column, container, row, text, Button};
use iced::{Element, Length};

use super::super::{
    status_container_style, subtle_button_style, ui_size, ExternalBrowserChoice,
    ExternalBrowserMessage, ExternalLinkPrompt, Message,
};
use super::omen_button_style;

pub(in crate::desktop) fn external_link_prompt_view<'a>(
    prompt: &'a ExternalLinkPrompt,
    browsers: &'a [ExternalBrowserChoice],
    preferred_command: Option<&'a str>,
    socks_proxy_enabled: bool,
    proxy_endpoint: Option<&'a (String, u16)>,
) -> Element<'a, Message> {
    let actions = browsers
        .iter()
        .enumerate()
        .fold(
            row![].spacing(8).align_y(iced::Alignment::Center),
            |row, (index, browser)| {
                let label = if Some(browser.command.as_str()) == preferred_command {
                    format!("{} *", browser.label)
                } else {
                    browser.label.clone()
                };
                let width = external_prompt_button_width(&label);
                row.push(
                    external_prompt_subtle_button(
                        label,
                        Message::ExternalBrowser(ExternalBrowserMessage::OpenWith(index)),
                    )
                    .width(Length::Fixed(width)),
                )
            },
        )
        .push(
            external_prompt_omen_button(
                "Copy URL",
                Message::ExternalBrowser(ExternalBrowserMessage::CopyUrl),
            )
            .width(Length::Fixed(90.0)),
        )
        .push(
            external_prompt_subtle_button(
                "X",
                Message::ExternalBrowser(ExternalBrowserMessage::DismissPrompt),
            )
            .width(Length::Fixed(38.0)),
        )
        .wrap();
    let proxy_status = external_prompt_proxy_status(socks_proxy_enabled, proxy_endpoint);
    let url = container(
        text(prompt.url.clone())
            .size(ui_size(12))
            .wrapping(Wrapping::WordOrGlyph)
            .width(Length::Fill),
    )
    .width(Length::Fill)
    .clip(true);

    container(
        column![
            row![
                text("Open external URL").size(ui_size(14)),
                text(proxy_status).size(ui_size(12)).width(Length::Fill),
            ]
            .spacing(10)
            .align_y(iced::Alignment::Center),
            actions,
            url,
        ]
        .spacing(8)
        .width(Length::Fill),
    )
    .style(status_container_style)
    .padding(10)
    .width(Length::Fill)
    .max_width(860.0)
    .into()
}

fn external_prompt_subtle_button(
    label: impl Into<String>,
    message: Message,
) -> Button<'static, Message> {
    button(external_prompt_button_label(label))
        .on_press(message)
        .padding(0)
        .height(Length::Fixed(external_prompt_button_height()))
        .style(subtle_button_style)
}

fn external_prompt_omen_button(
    label: impl Into<String>,
    message: Message,
) -> Button<'static, Message> {
    button(external_prompt_button_label(label))
        .on_press(message)
        .padding(0)
        .height(Length::Fixed(external_prompt_button_height()))
        .style(omen_button_style)
}

fn external_prompt_button_label(label: impl Into<String>) -> Element<'static, Message> {
    container(
        text(label.into())
            .size(ui_size(12))
            .wrapping(Wrapping::None),
    )
    .center_x(Length::Fill)
    .center_y(Length::Fixed(external_prompt_button_height()))
    .clip(true)
    .into()
}

fn external_prompt_button_height() -> f32 {
    (ui_size(26) as f32).max(26.0)
}

fn external_prompt_proxy_status(
    socks_proxy_enabled: bool,
    proxy_endpoint: Option<&(String, u16)>,
) -> String {
    if !socks_proxy_enabled {
        return "SOCKS5 off".into();
    }
    if let Some((host, port)) = proxy_endpoint {
        format!("SOCKS5 {host}:{port}")
    } else {
        "SOCKS5 not detected".into()
    }
}

fn external_prompt_button_width(label: &str) -> f32 {
    let label_units = label.chars().count().clamp(5, 18) as f32;
    28.0 + label_units * 9.0
}
