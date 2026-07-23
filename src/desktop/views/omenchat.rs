use iced::widget::{button, column, container, row, text, text_input};
use iced::{Element, Font, Length};

use super::super::*;

#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
const OMENCHAT_RECOVERED_INTENTS_VISIBLE_MAX: usize = 4;

pub(in crate::desktop) fn omenchat_media_animation_allowed(
    pane_visible: bool,
    reduce_motion: bool,
) -> bool {
    pane_visible && !reduce_motion
}

pub(in crate::desktop) fn omenchat_view_for_session(
    desktop: &DesktopApp,
    session_id: ChatSessionId,
    animate_media: bool,
) -> Element<'_, Message> {
    let Some(session) = desktop.omenchat.chat_client.session(session_id) else {
        return text("This OMENchat session was closed.")
            .size(ui_size(14))
            .into();
    };

    let room_list = if desktop.omenchat.omenchat_rooms_visible {
        let mut rooms = session.rooms.clone();
        if !rooms
            .iter()
            .any(|room| room.room_id == session.active_room.room_id)
        {
            rooms.push(session.active_room.clone());
        }
        rooms.sort_by(|left, right| {
            left.name
                .cmp(&right.name)
                .then(left.room_id.cmp(&right.room_id))
        });
        let mut room_column = column![].spacing(8);
        room_column = room_column.push(text("Rooms").size(ui_size(16)));
        for room in rooms {
            let unread = if room.unread > 0 {
                format!(" ({})", room.unread)
            } else {
                String::new()
            };
            let label = if room.room_id == session.active_room.room_id {
                format!("[#{}]", room.name)
            } else {
                format!("#{}{}", room.name, unread)
            };
            let message = Message::OmenChat(OmenChatMessage::JoinRoom {
                session_id: session.session_id,
                room: room.name.clone(),
            });
            room_column = room_column.push(if room.unread > 0 {
                warning_button_owned(label, message)
            } else {
                subtle_button_owned(label, message)
            });
        }
        room_column
            .push(subtle_button(
                "Load Older",
                Message::OmenChat(OmenChatMessage::LoadOlderHistory(session.session_id)),
            ))
            .width(Length::Shrink)
    } else {
        column![].width(Length::Shrink)
    };

    let mut timeline = column![].spacing(8).width(Length::Fill);
    for group in chat_timeline_groups(session) {
        let header = row![
            text(group.actor).size(ui_size(12)),
            text(chat_event_time_label(group.at_unix)).size(ui_size(11)),
        ]
        .spacing(8)
        .wrap();
        let mut group_content = column![header].spacing(1).width(Length::Fill);
        for body in group.bodies {
            let media_hints = omenchat_media_hints(
                &body.text,
                &desktop.app.settings.clearweb,
                desktop.clearweb.clearweb_proxy_endpoint.as_ref(),
                desktop.app.directory_service.trust_level(
                    &session.server.destination,
                    Some(&session.server.display_name),
                ) == crate::directory::TrustLevel::Trusted,
                &desktop.omenchat.omenchat_media_cache,
            );
            let line_text = chat_timeline_body_text(&body);
            let mut line = safe_timeline_text(line_text, 14);
            if body.is_action {
                line = line.font(Font {
                    style: FontStyle::Italic,
                    ..desktop_ui_font()
                });
            }
            if let Some(upload) = body.upload.as_ref() {
                let key = omenchat_upload_cache_key(upload.session_id, &upload.resource_id);
                group_content = group_content.push(omenchat_upload_action_row(
                    line,
                    upload.clone(),
                    desktop.omenchat.omenchat_media_cache.get(&key).cloned(),
                ));
            } else if let Some(resend) = body.resend {
                group_content = group_content.push(omenchat_resend_action_row(
                    line,
                    resend.session_id,
                    resend.room_id,
                    resend.event_id,
                    resend.body,
                    resend.action,
                ));
            } else {
                group_content = group_content.push(line);
            }
            for hint in media_hints {
                if let Some(row) = omenchat_media_hint_row(hint.clone()) {
                    group_content = group_content.push(row);
                }
                if let Some(path) = hint.image_path.as_ref() {
                    if let Some(preview) = omenchat_media_hint_preview(
                        path,
                        hint.animated,
                        animate_media
                            .then(|| desktop.omenchat.omenchat_gif_frames.get(path))
                            .flatten(),
                    ) {
                        group_content = group_content.push(preview);
                        if let Some(caption) = hint.caption {
                            group_content =
                                group_content.push(safe_timeline_text(caption.to_string(), 11));
                        }
                    }
                }
            }
            if let Some(upload) = body.upload {
                let key = omenchat_upload_cache_key(upload.session_id, &upload.resource_id);
                let upload_state = desktop.omenchat.omenchat_media_cache.get(&key).cloned();
                if let Some(state) = upload_state.as_ref() {
                    if let Some(preview) = omenchat_upload_preview(
                        state,
                        omenchat_media_state_image_path(state)
                            .as_ref()
                            .and_then(|path| {
                                animate_media
                                    .then(|| desktop.omenchat.omenchat_gif_frames.get(path))
                                    .flatten()
                            }),
                    ) {
                        group_content = group_content.push(preview);
                    }
                }
            }
        }
        timeline = timeline.push(container(group_content).padding([2, 8]).width(Length::Fill));
    }

    let mut userlist = column![text("Users").size(ui_size(16))]
        .spacing(6)
        .width(Length::Fixed(82.0));
    for user in unique_chat_users(&session.users) {
        userlist = userlist.push(text(user.display_label()).size(ui_size(13)));
    }

    let draft = desktop
        .omenchat
        .chat_drafts
        .get(&session.session_id)
        .map(String::as_str)
        .unwrap_or_default();
    let session_id = session.session_id;
    let active_room_id = session.active_room.room_id;
    let composer = row![
        tooltip_button(
            button(centered_toolbar_icon(ICON_MENU))
                .on_press(Message::OmenChat(OmenChatMessage::ToggleRooms))
                .padding(0)
                .width(Length::Fixed(toolbar_icon_button_side()))
                .height(Length::Fixed(toolbar_icon_button_side()))
                .style(subtle_button_style),
            "Rooms"
        ),
        tooltip_button(
            button(centered_toolbar_icon(ICON_ATTACH))
                .on_press(Message::OmenChat(OmenChatMessage::PickUpload(
                    session.session_id,
                )))
                .padding(0)
                .width(Length::Fixed(toolbar_icon_button_side()))
                .height(Length::Fixed(toolbar_icon_button_side()))
                .style(subtle_button_style),
            "Attach file"
        ),
        text_input(&format!("Message #{}", session.active_room.name), draft)
            .size(ui_size(14))
            .padding(8)
            .width(Length::Fill)
            .on_input(move |value| {
                Message::OmenChat(OmenChatMessage::DraftChanged { session_id, value })
            })
            .on_submit(Message::OmenChat(OmenChatMessage::SendDraft(
                session.session_id,
            ))),
        omen_button(
            "Send",
            Message::OmenChat(OmenChatMessage::SendDraft(session.session_id)),
        ),
    ]
    .spacing(8);

    let mut timeline_panel = column![].spacing(8).width(Length::Fill);
    if let Some(motd) = desktop
        .omenchat
        .omenchat_motds
        .get(&session.session_id)
        .map(String::as_str)
        .map(str::trim)
        .filter(|motd| !motd.is_empty())
    {
        timeline_panel = timeline_panel.push(
            container(text(motd).size(ui_size(13)))
                .padding([6, 8])
                .width(Length::Fill)
                .style(status_container_style),
        );
    }
    #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
    if let Some(progress) = omenchat_session_resource_progress(desktop, session.session_id) {
        timeline_panel = timeline_panel.push(
            container(text(omenchat_session_resource_progress_line(&progress)).size(ui_size(12)))
                .padding([6, 8])
                .width(Length::Fill)
                .style(status_container_style),
        );
    }
    #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
    if let Some(recovered) =
        omenchat_recovered_mutations_panel(desktop, &session.server.destination)
    {
        timeline_panel = timeline_panel.push(recovered);
    }
    timeline_panel = timeline_panel.push(
        app_scrollable(timeline)
            .id(omenchat_scroll_id(session.session_id, active_room_id))
            .on_scroll(move |viewport: Viewport| {
                Message::OmenChat(OmenChatMessage::Scrolled {
                    session_id,
                    room_id: active_room_id,
                    offset: sanitize_scroll_offset(viewport.relative_offset()),
                })
            })
            .height(Length::Fill),
    );
    if desktop.omenchat_is_viewing_history(session.session_id, active_room_id) {
        timeline_panel = timeline_panel.push(
            container(
                column![
                    text("You're viewing older messages").size(ui_size(12)),
                    omen_button(
                        "Jump To Present",
                        Message::OmenChat(OmenChatMessage::JumpToPresent {
                            session_id: session.session_id,
                            room_id: active_room_id,
                        }),
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
        row![room_list, timeline_panel, userlist]
            .spacing(10)
            .height(Length::Fill),
        composer
    ]
    .spacing(10)
    .padding(10)
    .height(Length::Fill)
    .into()
}

#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
fn omenchat_recovered_mutations_panel(
    desktop: &DesktopApp,
    server_destination: &str,
) -> Option<Element<'static, Message>> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .and_then(|duration| i64::try_from(duration.as_secs()).ok())
        .unwrap_or_default();
    let matching_count = desktop
        .omenchat
        .omenchat_recovered_mutation_intents
        .iter()
        .filter(|intent| intent.server_destination == server_destination)
        .count();
    if matching_count == 0 {
        return None;
    }
    let mut content =
        column![
            text("Recovered durable mutations — nothing was resent automatically")
                .size(ui_size(13))
        ]
        .spacing(6)
        .width(Length::Fill);
    for intent in desktop
        .omenchat
        .omenchat_recovered_mutation_intents
        .iter()
        .filter(|intent| intent.server_destination == server_destination)
        .take(OMENCHAT_RECOVERED_INTENTS_VISIBLE_MAX)
    {
        let past_expiry = intent.expires_at <= now;
        let state = if past_expiry {
            "past expiry"
        } else {
            match intent.state {
                crate::chat::mutation_intents::OutboundMutationState::Prepared => {
                    "prepared; not transmitted"
                }
                crate::chat::mutation_intents::OutboundMutationState::SentUncertain => {
                    "uncertain; server may have committed it"
                }
                _ => "unexpected recovered state",
            }
        };
        let preview = match &intent.body {
            crate::chat::protocol::FrameBody::Text(body) => {
                let mut chars = body.chars();
                let mut preview = chars.by_ref().take(96).collect::<String>();
                if chars.next().is_some() {
                    preview.push('…');
                }
                preview
            }
            _ => "non-text mutation".into(),
        };
        let label = format!(
            "Room {} | {state} | {preview}",
            intent
                .room_id
                .map(|room_id| room_id.to_string())
                .unwrap_or_else(|| "unknown".into())
        );
        let confirming = desktop
            .omenchat
            .omenchat_mutation_resolution_confirmation
            .filter(|confirmation| confirmation.mutation_id == intent.mutation_id);
        let actions = if let Some(confirmation) = confirming {
            let confirmation_label = match confirmation.next {
                crate::chat::mutation_intents::OutboundMutationState::SentUncertain => {
                    if confirmation.expected
                        == crate::chat::mutation_intents::OutboundMutationState::Prepared
                    {
                        "Confirm Send"
                    } else {
                        "Confirm Retry"
                    }
                }
                crate::chat::mutation_intents::OutboundMutationState::Expired => "Confirm Expired",
                _ => "Confirm Stop Tracking",
            };
            row![
                warning_button(
                    confirmation_label,
                    Message::OmenChat(OmenChatMessage::ConfirmMutationResolution),
                ),
                subtle_button(
                    "Cancel",
                    Message::OmenChat(OmenChatMessage::CancelMutationResolution),
                ),
            ]
            .spacing(6)
        } else {
            if past_expiry {
                row![warning_button(
                    "Finalize Expired",
                    Message::OmenChat(OmenChatMessage::BeginMutationResolution {
                        mutation_id: intent.mutation_id,
                        action: OmenChatMutationResolutionAction::Expire,
                    }),
                )]
            } else {
                row![
                    warning_button(
                        if intent.state
                            == crate::chat::mutation_intents::OutboundMutationState::Prepared
                        {
                            "Send Prepared"
                        } else {
                            "Retry Safely"
                        },
                        Message::OmenChat(OmenChatMessage::BeginMutationResolution {
                            mutation_id: intent.mutation_id,
                            action: OmenChatMutationResolutionAction::Retry,
                        }),
                    ),
                    subtle_button(
                        "Stop Tracking",
                        Message::OmenChat(OmenChatMessage::BeginMutationResolution {
                            mutation_id: intent.mutation_id,
                            action: OmenChatMutationResolutionAction::Abandon,
                        }),
                    ),
                ]
                .spacing(6)
            }
        };
        content = content.push(column![text(label).size(ui_size(12)), actions].spacing(4));
    }
    if matching_count > OMENCHAT_RECOVERED_INTENTS_VISIBLE_MAX {
        content = content.push(
            text(format!(
                "{} additional recovered mutation(s) are hidden; resolve visible entries to continue",
                matching_count - OMENCHAT_RECOVERED_INTENTS_VISIBLE_MAX
            ))
            .size(ui_size(12)),
        );
    }
    Some(
        container(content)
            .padding([6, 8])
            .width(Length::Fill)
            .style(warning_container_style)
            .into(),
    )
}

#[cfg(test)]
mod accessibility_tests {
    use super::omenchat_media_animation_allowed;

    #[test]
    fn reduced_motion_and_hidden_panes_withhold_animated_media() {
        assert!(omenchat_media_animation_allowed(true, false));
        assert!(!omenchat_media_animation_allowed(false, false));
        assert!(!omenchat_media_animation_allowed(true, true));
        assert!(!omenchat_media_animation_allowed(false, true));
    }
}
