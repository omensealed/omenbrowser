use iced::widget::text::Wrapping;
use iced::widget::{button, column, container, row, text, Button, Text};
use iced::{Element, Length};

mod attachments;
mod conversation;
mod diagnostics_preview;
mod external_prompt;
mod formatting;
mod gateway;
mod hover_actions;
mod interface_config;
mod interface_status;
mod lxmf_status;
mod monitoring;
mod native_status;
#[cfg(feature = "chat-client")]
mod omenchat_helpers;
#[cfg(feature = "chat-client")]
mod omenchat_media;
#[cfg(feature = "chat-client")]
mod omenchat_media_format;
mod omenchat_monitoring;
#[cfg(feature = "chat-client")]
mod omenchat_timeline;
mod recent_activity;
mod restore;
mod scroll;
mod timeline_text;
mod toolbar;
pub(in crate::desktop) use attachments::*;
pub(in crate::desktop) use conversation::*;
pub(in crate::desktop) use diagnostics_preview::*;
pub(in crate::desktop) use external_prompt::*;
pub(in crate::desktop) use formatting::*;
pub(in crate::desktop) use gateway::*;
pub(in crate::desktop) use hover_actions::*;
pub(in crate::desktop) use interface_config::*;
pub(in crate::desktop) use interface_status::*;
pub(in crate::desktop) use lxmf_status::*;
pub(in crate::desktop) use monitoring::*;
pub(in crate::desktop) use native_status::*;
#[cfg(feature = "chat-client")]
pub(in crate::desktop) use omenchat_helpers::*;
#[cfg(feature = "chat-client")]
pub(in crate::desktop) use omenchat_media::*;
pub(in crate::desktop) use omenchat_monitoring::*;
#[cfg(feature = "chat-client")]
pub(in crate::desktop) use omenchat_timeline::*;
pub(in crate::desktop) use recent_activity::*;
pub(in crate::desktop) use restore::*;
pub(in crate::desktop) use scroll::*;
pub(in crate::desktop) use timeline_text::*;
pub(in crate::desktop) use toolbar::*;

use super::{
    address_display_container_style, card_container_style, omen_button_style,
    status_container_style, subtle_button_style, ui_size, warning_button_style, Message,
};

pub(in crate::desktop) fn omen_button<'a>(label: &'a str, message: Message) -> Button<'a, Message> {
    button(text(label))
        .on_press(message)
        .style(omen_button_style)
}

pub(in crate::desktop) fn omen_button_owned(
    label: String,
    message: Message,
) -> Button<'static, Message> {
    button(text(label))
        .on_press(message)
        .style(omen_button_style)
}

pub(in crate::desktop) fn subtle_button<'a>(
    label: &'a str,
    message: Message,
) -> Button<'a, Message> {
    button(text(label))
        .on_press(message)
        .style(subtle_button_style)
}

pub(in crate::desktop) fn subtle_button_owned(
    label: String,
    message: Message,
) -> Button<'static, Message> {
    button(text(label))
        .on_press(message)
        .style(subtle_button_style)
}

pub(in crate::desktop) fn warning_button<'a>(
    label: &'a str,
    message: Message,
) -> Button<'a, Message> {
    button(text(label))
        .on_press(message)
        .style(warning_button_style)
}

pub(in crate::desktop) fn warning_button_owned(
    label: String,
    message: Message,
) -> Button<'static, Message> {
    button(text(label))
        .on_press(message)
        .style(warning_button_style)
}

pub(in crate::desktop) fn action_grid<'a>(
    actions: Vec<Button<'a, Message>>,
    max_per_row: usize,
) -> Element<'a, Message> {
    let max_per_row = max_per_row.max(1);
    let mut rows = column![].spacing(8).width(Length::Fill);
    let mut current_row = row![].spacing(8);
    let mut current_count = 0usize;

    for action in actions {
        if current_count >= max_per_row {
            rows = rows.push(current_row.wrap());
            current_row = row![].spacing(8);
            current_count = 0;
        }
        current_row = current_row.push(action);
        current_count += 1;
    }
    if current_count > 0 {
        rows = rows.push(current_row.wrap());
    }
    rows.into()
}

pub(in crate::desktop) fn section_card<'a>(
    title: impl Into<String>,
    body: impl Into<Element<'a, Message>>,
) -> Element<'a, Message> {
    container(
        column![
            text(title.into())
                .size(ui_size(20))
                .wrapping(Wrapping::WordOrGlyph)
                .width(Length::Fill),
            body.into()
        ]
        .spacing(10),
    )
    .style(card_container_style)
    .padding(14)
    .width(Length::Fill)
    .into()
}

pub(in crate::desktop) fn wrapped_panel_text(content: &str) -> Text<'_> {
    text(content)
        .size(ui_size(14))
        .wrapping(Wrapping::WordOrGlyph)
        .width(Length::Fill)
}

pub(in crate::desktop) fn wrapped_text_owned(
    content: impl Into<String>,
    size: u16,
) -> Text<'static> {
    text(content.into())
        .size(ui_size(size))
        .wrapping(Wrapping::WordOrGlyph)
        .width(Length::Fill)
}

pub(in crate::desktop) fn inert_address_display(value: String) -> Element<'static, Message> {
    container(text(value).size(ui_size(14)))
        .width(Length::Fill)
        .padding([6, 8])
        .style(address_display_container_style)
        .into()
}
