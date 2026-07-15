use iced::widget::text::Wrapping;
use iced::widget::{column, container, row, text};
use iced::{Element, Length};

use crate::app::message_summary_key;
use crate::micron::parse_micron;
use crate::micron::render::render_document;

use super::super::{
    desktop_message_is_retry_candidate, desktop_message_propagation_sync_label,
    desktop_message_retry_labels, failed_message_container_style, format_epoch_secs,
    incoming_message_container_style, lxmf_message_compact_stamp_status,
    lxmf_message_compact_status, lxmf_message_status_lines, outgoing_message_container_style,
    selected_message_container_style, status_container_style, ui_size, ConversationMessage,
    Message, CONVERSATION_MICRON_PREVIEW_WIDTH, CONVERSATION_PREVIEW_CHARS,
    CONVERSATION_PREVIEW_LINES,
};
use super::{
    action_grid, conversation_message_attachment_rows, omen_button_owned, section_card,
    subtle_button, subtle_button_owned,
};

pub(in crate::desktop) fn message_conversation_header(
    conversation: &crate::messaging::Conversation,
) -> Element<'_, Message> {
    let status = if conversation.pending_send.is_some() {
        "sending"
    } else {
        "ready"
    };
    container(
        row![
            text(format!("delivery: {:?}", conversation.delivery_mode)).size(ui_size(13)),
            text(format!(
                "{} messages | {} unread | {status}",
                conversation.thread.messages.len(),
                conversation.thread.unread_count
            ))
            .size(ui_size(13)),
        ]
        .spacing(10)
        .wrap(),
    )
    .style(status_container_style)
    .padding([6, 10])
    .width(Length::Fill)
    .into()
}

pub(in crate::desktop) fn message_bubble<'a>(
    conversation_id: u64,
    message: &'a crate::messaging::MessageSummary,
    selected: bool,
) -> Element<'a, Message> {
    let author = if message.incoming { "Peer" } else { "You" };
    let message_key = message_summary_key(message);
    let mut content = column![
        row![
            text(author).size(ui_size(13)),
            text(format_epoch_secs(message.timestamp)).size(ui_size(12)),
        ]
        .spacing(8)
        .wrap(),
        text(message_title_line(message))
            .size(ui_size(14))
            .width(Length::Fill)
            .wrapping(Wrapping::WordOrGlyph),
        text(message_body_preview(message))
            .size(ui_size(15))
            .width(Length::Fill)
            .wrapping(Wrapping::WordOrGlyph),
    ]
    .spacing(4);

    if let Some(summary) = lxmf_message_compact_status(message) {
        content = content.push(text(summary).size(ui_size(13)));
    }
    if let Some(stamp_summary) = lxmf_message_compact_stamp_status(message) {
        content = content.push(text(stamp_summary).size(ui_size(12)));
    }

    if !message.attachments.is_empty() {
        content = content.push(conversation_message_attachment_rows(message));
    }

    let mut actions = vec![subtle_button_owned(
        if selected {
            "Selected".into()
        } else {
            "Details".into()
        },
        Message::Conversation(ConversationMessage::SelectPaneRow {
            conversation_id,
            key: message_key.clone(),
        }),
    )];
    if selected {
        if desktop_message_is_retry_candidate(message) {
            let retry_key = message_key.clone();
            let labels = desktop_message_retry_labels(message);
            actions.push(subtle_button_owned(
                labels.prepare.into(),
                Message::Conversation(ConversationMessage::PrepareRetryForConversationRow {
                    conversation_id,
                    key: message_key,
                }),
            ));
            actions.push(omen_button_owned(
                labels.send.into(),
                Message::Conversation(ConversationMessage::SendRetryForConversationRow {
                    conversation_id,
                    key: retry_key,
                }),
            ));
        }
        if let Some(sync_label) = desktop_message_propagation_sync_label(message) {
            actions.push(omen_button_owned(
                sync_label.into(),
                Message::Conversation(ConversationMessage::SyncPropagationForConversationRow {
                    conversation_id,
                    key: message_summary_key(message),
                }),
            ));
        }
    }
    if message.failed {
        actions.push(subtle_button_owned(
            "Close".into(),
            Message::Conversation(ConversationMessage::DismissPaneRow {
                conversation_id,
                key: message_summary_key(message),
            }),
        ));
    }
    content = content.push(action_grid(actions, 4));

    let bubble = container(content)
        .style(if selected {
            selected_message_container_style
        } else if message.incoming {
            incoming_message_container_style
        } else if message.failed {
            failed_message_container_style
        } else {
            outgoing_message_container_style
        })
        .padding(12)
        .width(Length::FillPortion(5));

    if message.incoming {
        row![bubble, container(text("")).width(Length::FillPortion(1)),]
            .spacing(8)
            .into()
    } else {
        row![container(text("")).width(Length::FillPortion(1)), bubble,]
            .spacing(8)
            .into()
    }
}

pub(in crate::desktop) fn selected_message_details_card(
    conversation_id: u64,
    conversation: &crate::messaging::Conversation,
) -> Element<'_, Message> {
    let Some(selected_key) = conversation.selected_message_key.as_deref() else {
        return container(text("")).height(Length::Shrink).into();
    };
    let Some(message) = conversation
        .thread
        .messages
        .iter()
        .find(|message| message_summary_key(message) == selected_key)
    else {
        return container(text("")).height(Length::Shrink).into();
    };

    let header = row![
        text(if message.incoming {
            "Incoming message"
        } else {
            "Outgoing message"
        })
        .size(ui_size(14)),
        text(format_epoch_secs(message.timestamp)).size(ui_size(13)),
        text(format!("transport: {:?}", message.transport_method)).size(ui_size(13)),
        subtle_button(
            "Close",
            Message::Conversation(ConversationMessage::ClosePaneDetails { conversation_id })
        ),
    ]
    .spacing(10)
    .wrap();
    let mut header_actions = Vec::new();
    if desktop_message_is_retry_candidate(message) {
        let retry_key = message_summary_key(message);
        let labels = desktop_message_retry_labels(message);
        header_actions.push(subtle_button_owned(
            labels.prepare.into(),
            Message::Conversation(ConversationMessage::PrepareRetryForConversationRow {
                conversation_id,
                key: retry_key.clone(),
            }),
        ));
        header_actions.push(omen_button_owned(
            labels.send.into(),
            Message::Conversation(ConversationMessage::SendRetryForConversationRow {
                conversation_id,
                key: retry_key,
            }),
        ));
    }
    if let Some(sync_label) = desktop_message_propagation_sync_label(message) {
        header_actions.push(omen_button_owned(
            sync_label.into(),
            Message::Conversation(ConversationMessage::SyncPropagationForConversationRow {
                conversation_id,
                key: message_summary_key(message),
            }),
        ));
    }

    let mut body = column![
        header,
        action_grid(header_actions, 3),
        text(format!("subject: {}", message_title_line(message))).size(ui_size(13)),
        text(format!(
            "state: delivered={} failed={} unread={}",
            message.delivered, message.failed, message.unread
        ))
        .size(ui_size(13)),
        text(format!(
            "message id: {}",
            message.message_id.as_deref().unwrap_or("<none>")
        ))
        .size(ui_size(13)),
    ]
    .spacing(5);

    for line in lxmf_message_status_lines(message) {
        body = body.push(text(line).size(ui_size(13)));
    }
    if message.fields.is_empty() {
        body = body.push(text("LXMF fields: none recorded").size(ui_size(13)));
    }

    section_card("Message Details", body)
}

fn message_title_line(message: &crate::messaging::MessageSummary) -> String {
    if message.title.trim().is_empty() {
        "(no subject)".into()
    } else {
        message.title.clone()
    }
}

fn compact_message_preview(content: &str) -> String {
    let mut preview = String::new();
    let mut char_count = 0usize;
    let mut truncated = false;

    for (line_index, line) in content.lines().enumerate() {
        if line_index >= CONVERSATION_PREVIEW_LINES {
            truncated = true;
            break;
        }
        if line_index > 0 {
            preview.push('\n');
        }
        for ch in line.chars() {
            if char_count >= CONVERSATION_PREVIEW_CHARS {
                truncated = true;
                break;
            }
            preview.push(ch);
            char_count += 1;
        }
        if truncated {
            break;
        }
    }

    if preview.is_empty() && !content.is_empty() {
        preview = content.chars().take(CONVERSATION_PREVIEW_CHARS).collect();
        truncated = content.chars().count() > CONVERSATION_PREVIEW_CHARS;
    }
    if truncated {
        preview.push_str("...");
    }
    preview
}

pub(in crate::desktop) fn message_body_preview(
    message: &crate::messaging::MessageSummary,
) -> String {
    if message
        .fields
        .get("native_lxmf_renderer")
        .is_some_and(|renderer| renderer.eq_ignore_ascii_case("micron"))
    {
        return compact_micron_message_preview(&message.content);
    }
    compact_message_preview(&message.content)
}

fn compact_micron_message_preview(content: &str) -> String {
    let document = parse_micron(content);
    let rendered = render_document(&document, CONVERSATION_MICRON_PREVIEW_WIDTH);
    let mut preview = String::new();
    let mut char_count = 0usize;
    let mut truncated = false;

    for (line_index, row) in rendered.iter().enumerate() {
        if line_index >= CONVERSATION_PREVIEW_LINES {
            truncated = true;
            break;
        }
        if line_index > 0 {
            preview.push('\n');
        }
        for ch in row.text().chars() {
            if char_count >= CONVERSATION_PREVIEW_CHARS {
                truncated = true;
                break;
            }
            preview.push(ch);
            char_count += 1;
        }
        if truncated {
            break;
        }
    }

    if preview.is_empty() && !content.is_empty() {
        preview = compact_message_preview(content);
    } else if truncated {
        preview.push_str("...");
    }
    preview
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::messaging::{Conversation, MessageSummary, TransportMethod};
    use std::collections::BTreeMap;

    #[test]
    fn selected_message_details_card_renders_for_selected_message() {
        let mut conversation = Conversation::new(1, "peer", "Peer");
        let message = MessageSummary {
            peer_hash: "peer".into(),
            peer_label: "Peer".into(),
            title: "Subject".into(),
            content: "Body".into(),
            timestamp: 1.0,
            transport_method: TransportMethod::Direct,
            delivered: false,
            failed: true,
            incoming: false,
            unread: false,
            message_id: Some("packet-1".into()),
            fields: BTreeMap::from([("native_lxmf_state".into(), "failed".into())]),
            attachments: Vec::new(),
        };
        conversation.selected_message_key = Some(message_summary_key(&message));
        conversation.push_message(message);

        let _details = selected_message_details_card(conversation.id, &conversation);
    }

    #[test]
    fn lxmf_micron_renderer_hint_uses_micron_message_preview() {
        let message = MessageSummary {
            peer_hash: "peer".into(),
            peer_label: "Peer".into(),
            title: "Subject".into(),
            content: "`cCentered\n`Ff00red".into(),
            timestamp: 1.0,
            transport_method: TransportMethod::Direct,
            delivered: true,
            failed: false,
            incoming: true,
            unread: false,
            message_id: Some("packet-1".into()),
            fields: BTreeMap::from([("native_lxmf_renderer".into(), "micron".into())]),
            attachments: Vec::new(),
        };

        let preview = message_body_preview(&message);

        assert!(preview.contains("Centered"));
        assert!(preview.contains("red"));
        assert!(!preview.contains("`c"));
        assert!(!preview.contains("`Ff00"));
    }
}
