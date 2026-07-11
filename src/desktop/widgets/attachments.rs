use std::path::PathBuf;

use iced::widget::text::Wrapping;
use iced::widget::{column, container, row, text};
use iced::{Element, Length};

use super::super::{
    human_bytes, status_container_style, ui_size, Message, ICON_OPEN, ICON_WINDOW_CLOSE,
};
use super::inline_icon_button_owned;

pub(in crate::desktop) fn pick_conversation_attachment_file() -> Result<Option<PathBuf>, String> {
    Ok(rfd::FileDialog::new()
        .set_title("Select LXMF attachment")
        .pick_file())
}

pub(in crate::desktop) fn conversation_attachment_draft_rows(
    conversation_id: u64,
    attachments: &[PathBuf],
) -> Element<'static, Message> {
    if attachments.is_empty() {
        return container(text("")).height(Length::Shrink).into();
    }

    let mut rows = column![text("Attachments").size(ui_size(12))]
        .spacing(3)
        .width(Length::Fill);
    for (index, path) in attachments.iter().enumerate() {
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("attachment")
            .to_string();
        let size = std::fs::metadata(path)
            .ok()
            .filter(|metadata| metadata.is_file())
            .map(|metadata| human_bytes(metadata.len()))
            .unwrap_or_else(|| "missing".into());
        rows = rows.push(
            row![
                text(format!("{name} ({size})"))
                    .size(ui_size(12))
                    .width(Length::Fill)
                    .wrapping(Wrapping::WordOrGlyph),
                inline_icon_button_owned(
                    ICON_OPEN,
                    "Open attachment",
                    Message::OpenConversationAttachment(path.clone())
                ),
                inline_icon_button_owned(
                    ICON_WINDOW_CLOSE,
                    "Remove attachment",
                    Message::RemoveConversationAttachment {
                        conversation_id,
                        index,
                    }
                ),
            ]
            .spacing(6)
            .align_y(iced::Alignment::Center)
            .wrap(),
        );
    }

    container(rows)
        .padding([4, 6])
        .style(status_container_style)
        .width(Length::Fill)
        .into()
}

pub(in crate::desktop) fn conversation_message_attachment_rows<'a>(
    message: &'a crate::messaging::MessageSummary,
) -> Element<'a, Message> {
    if message.attachments.is_empty() {
        return container(text("")).height(Length::Shrink).into();
    }

    let mut rows = column![text("Attachments").size(ui_size(12))]
        .spacing(3)
        .width(Length::Fill);
    for attachment in &message.attachments {
        let mut row = row![text(format!(
            "{} ({})",
            attachment.name,
            human_bytes(attachment.size)
        ))
        .size(ui_size(12))
        .width(Length::Fill)
        .wrapping(Wrapping::WordOrGlyph),]
        .spacing(6)
        .align_y(iced::Alignment::Center);
        if let Some(path) = attachment.path.as_ref() {
            row = row.push(inline_icon_button_owned(
                ICON_OPEN,
                "Open attachment",
                Message::OpenConversationAttachment(path.clone()),
            ));
        }
        rows = rows.push(row.wrap());
    }

    container(rows)
        .padding([4, 6])
        .style(status_container_style)
        .width(Length::Fill)
        .into()
}
