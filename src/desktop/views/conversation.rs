use iced::widget::text::Wrapping;
use iced::widget::{button, column, container, row, text, text_editor, text_input};
use iced::{Element, Length};

use crate::app::message_summary_key;

use super::super::*;

pub(in crate::desktop) fn messages_view_for_conversation(
    desktop: &DesktopApp,
    conversation_id: u64,
) -> Element<'_, Message> {
    let Some(conversation) = desktop
        .app
        .workspace
        .conversations
        .iter()
        .find(|conversation| conversation.id == conversation_id)
    else {
        return text("This conversation was closed.")
            .size(ui_size(14))
            .into();
    };
    let messages = conversation
        .thread
        .messages
        .iter()
        .filter(|message| {
            !conversation
                .dismissed_message_keys
                .contains(&message_summary_key(message))
        })
        .rev()
        .take(CONVERSATION_VISIBLE_MESSAGES)
        .collect::<Vec<_>>();
    let messages = messages
        .into_iter()
        .rev()
        .fold(column![].spacing(10), |column, message| {
            let key = message_summary_key(message);
            column.push(message_bubble(
                conversation.id,
                message,
                conversation.selected_message_key.as_deref() == Some(key.as_str()),
            ))
        });
    let selected_details = selected_message_details_card(conversation.id, conversation);
    let trust_label = if desktop.app.lxmf_peer_is_trusted(&conversation.peer_hash) {
        "Untrust"
    } else {
        "Trust"
    };
    let direct_stamp_confirmation: Element<'_, Message> = if let Some(confirmation) =
        conversation.direct_stamp_confirmation.as_ref()
    {
        container(
                column![
                    text(format!(
                        "Peer requires direct stamp cost {} (confirmation threshold: above {}). No message has been sent.",
                        confirmation.advertised_cost, confirmation.ask_above
                    ))
                    .size(ui_size(13)),
                    row![
                        omen_button_owned(
                            format!("Confirm Cost {}", confirmation.advertised_cost),
                            Message::Conversation(
                                ConversationMessage::ConfirmPaneDirectStamp(conversation_id)
                            ),
                        ),
                        subtle_button(
                            "Cancel",
                            Message::Conversation(
                                ConversationMessage::CancelPaneDirectStamp(conversation_id)
                            ),
                        ),
                    ]
                    .spacing(8)
                ]
                .spacing(6),
            )
            .padding([8, 10])
            .width(Length::Fill)
            .style(status_container_style)
            .into()
    } else {
        column![].into()
    };
    let composer = section_card(
        "Write Message",
        column![
            text_input(
                "LXMF peer destination hash",
                conversation.peer_hash.as_str()
            )
            .on_input(
                move |value| Message::Conversation(ConversationMessage::PanePeerChanged {
                    conversation_id,
                    value,
                })
            )
            .width(Length::Fill),
            text_input("subject", &conversation.draft_title)
                .on_input(move |value| Message::Conversation(
                    ConversationMessage::PaneTitleChanged {
                        conversation_id,
                        value,
                    }
                ))
                .width(Length::Fill),
            desktop
                .conversation
                .body_editors
                .get(&conversation_id)
                .map(|editor| {
                    let editor_element: Element<'_, Message> = text_editor(editor)
                        .on_action(move |action| {
                            Message::Conversation(ConversationMessage::PaneBodyEdited {
                                conversation_id,
                                action,
                            })
                        })
                        .wrapping(Wrapping::WordOrGlyph)
                        .height(Length::Fixed(112.0))
                        .into();
                    editor_element
                })
                .unwrap_or_else(|| {
                    text_input("message body", &conversation.draft_body)
                        .on_input(move |value| {
                            Message::Conversation(ConversationMessage::PaneBodyChanged {
                                conversation_id,
                                value,
                            })
                        })
                        .width(Length::Fill)
                        .into()
                }),
            conversation_attachment_draft_rows(conversation_id, &conversation.attachments),
            text("Enter inserts a new line. Use Send to deliver the draft.").size(ui_size(12)),
            conversation_delivery_state_line(conversation),
            direct_stamp_confirmation,
            row![
                tooltip_button(
                    button(centered_toolbar_icon(ICON_ATTACH))
                        .on_press(Message::Conversation(ConversationMessage::PickAttachment(
                            conversation_id,
                        )))
                        .padding(0)
                        .width(Length::Fixed(toolbar_icon_button_side()))
                        .height(Length::Fixed(toolbar_icon_button_side()))
                        .style(subtle_button_style),
                    "Attach file",
                ),
                subtle_button(
                    "Delivery",
                    Message::Conversation(ConversationMessage::TogglePaneDeliveryMode(
                        conversation_id,
                    ))
                ),
                subtle_button(
                    if conversation.include_ticket {
                        "Ticket On"
                    } else {
                        "Ticket Off"
                    },
                    Message::Conversation(ConversationMessage::TogglePaneTicket(conversation_id))
                ),
                omen_button(
                    "Send",
                    Message::Conversation(ConversationMessage::SendPaneDraft(conversation_id)),
                ),
                subtle_button(
                    "Path",
                    Message::Conversation(ConversationMessage::RequestPanePeerPath(
                        conversation_id,
                    ))
                ),
                subtle_button(
                    "Sync Propagation",
                    Message::Diagnostics(super::super::DiagnosticsMessage::SyncPropagationNow)
                ),
                subtle_button(
                    "Sync",
                    Message::Conversation(ConversationMessage::SyncMessages),
                ),
                subtle_button(
                    trust_label,
                    Message::Conversation(ConversationMessage::TogglePaneTrust(conversation_id))
                ),
                subtle_button(
                    "Diag",
                    Message::Conversation(ConversationMessage::PaneDiagnostics(conversation_id))
                ),
            ]
            .spacing(8)
            .wrap(),
        ]
        .spacing(8),
    );

    let message_scroll = app_scrollable(column![messages, selected_details].spacing(12))
        .id(conversation_scroll_id(conversation_id))
        .on_scroll(move |viewport: Viewport| {
            Message::Conversation(ConversationMessage::Scrolled {
                conversation_id,
                offset: sanitize_scroll_offset(viewport.relative_offset()),
            })
        })
        .height(Length::Fill)
        .width(Length::Fill);
    let mut message_area = column![message_scroll].spacing(6).height(Length::Fill);
    if desktop.conversation_is_viewing_history(conversation_id) {
        message_area = message_area.push(
            container(
                column![
                    text("You're viewing older messages").size(ui_size(12)),
                    omen_button(
                        "Jump To Present",
                        Message::Conversation(ConversationMessage::JumpToPresent(conversation_id)),
                    )
                ]
                .spacing(6)
                .width(Length::Fill),
            )
            .padding([6, 8])
            .width(Length::Fill)
            .style(status_container_style),
        );
    }

    column![
        message_conversation_header(conversation),
        message_area,
        composer,
    ]
    .spacing(8)
    .padding(8)
    .into()
}

fn conversation_delivery_state_line<'a>(
    conversation: &crate::messaging::Conversation,
) -> Element<'a, Message> {
    text(format!(
        "delivery: {:?} | ticket: {}",
        conversation.delivery_mode, conversation.include_ticket
    ))
    .size(ui_size(14))
    .into()
}
