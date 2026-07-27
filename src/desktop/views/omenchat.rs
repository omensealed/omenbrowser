use iced::widget::{button, column, container, row, text, text_input};
#[cfg(feature = "desktop-qr")]
use iced::widget::{qr_code, text::Wrapping};
use iced::{Element, Font, Length};

use super::super::*;

#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
const OMENCHAT_RECOVERED_INTENTS_VISIBLE_MAX: usize = 4;

#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
fn recovered_mutation_operation(
    op: crate::chat::protocol::ChatOp,
    body: &crate::chat::protocol::FrameBody,
) -> &'static str {
    use crate::chat::protocol::{ChatOp, FrameBody};

    match op {
        ChatOp::RoomMessage => "room message",
        ChatOp::RoomAction => "room action",
        ChatOp::RoomNotice => "room notice",
        ChatOp::RoomReaction => match crate::chat::protocol::ReactionRequest::from_frame_body(body)
        {
            Ok(request) => match request.action {
                crate::chat::protocol::ReactionAction::Add => "add reaction",
                crate::chat::protocol::ReactionAction::Remove => "remove reaction",
            },
            Err(_) => "reaction",
        },
        ChatOp::RoomMessageRevision => {
            match crate::chat::protocol::MessageRevisionRequest::from_frame_body(body) {
                Ok(request) => match request.action {
                    crate::chat::protocol::MessageRevisionAction::Correct => "message correction",
                    crate::chat::protocol::MessageRevisionAction::Tombstone => "message deletion",
                },
                Err(_) => "message revision",
            }
        }
        ChatOp::PartRoom => "leave room",
        ChatOp::Command => match body {
            FrameBody::Text(command) => match command.split_whitespace().next() {
                Some("topic") => "topic update",
                Some("create") => "room creation",
                Some("role") => "role change",
                Some("unban") => "unban user",
                Some("kick") => "kick user",
                Some("ban") => "ban user",
                Some("mute") => "mute user",
                Some("unmute") => "unmute user",
                _ => "server command",
            },
            _ => "server command",
        },
        _ => "unsupported operation",
    }
}

#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
fn recovered_mutation_expiry_label(expires_at: i64, now: i64) -> String {
    let (prefix, seconds) = if expires_at <= now {
        ("expired", now.saturating_sub(expires_at))
    } else {
        ("expires in", expires_at.saturating_sub(now))
    };
    let value = if seconds < 60 {
        format!("{seconds}s")
    } else if seconds < 60 * 60 {
        format!("{}m", seconds / 60)
    } else if seconds < 24 * 60 * 60 {
        format!("{}h", seconds / (60 * 60))
    } else {
        format!("{}d", seconds / (24 * 60 * 60))
    };
    if expires_at <= now {
        format!("{prefix} {value} ago")
    } else {
        format!("{prefix} {value}")
    }
}

#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
fn compact_recovery_destination(destination: &str) -> String {
    let mut chars = destination.chars();
    let prefix = chars.by_ref().take(12).collect::<String>();
    if chars.next().is_some() {
        format!("{prefix}…")
    } else {
        prefix
    }
}

fn omenchat_reply_line<'a>(
    reply: &ChatTimelineReply,
    _view_owner: &'a DesktopApp,
) -> Element<'a, Message> {
    match reply {
        ChatTimelineReply::Available {
            session_id,
            room_id,
            event_id,
            label,
        } => subtle_button_owned(
            label.clone(),
            Message::OmenChat(OmenChatMessage::JumpToEvent {
                session_id: *session_id,
                room_id: *room_id,
                event_id: *event_id,
            }),
        )
        .width(Length::Fill)
        .into(),
        ChatTimelineReply::Unavailable { event_id } => safe_timeline_text(
            format!("↳ Original message unavailable (event {event_id})"),
            11,
        )
        .into(),
    }
}

fn omenchat_reaction_summary_row(
    summaries: &[crate::chat::ChatReactionSummary],
) -> Element<'static, Message> {
    let mut summaries_row = row![].spacing(4);
    for summary in summaries {
        let style = if summary.reacted_by_local_user {
            selected_message_container_style
        } else {
            status_container_style
        };
        let local_label = if summary.reacted_by_local_user {
            " · you"
        } else {
            ""
        };
        summaries_row = summaries_row.push(
            container(
                row![
                    text(reaction_token_presentation(summary.token).emoji)
                        .font(emoji_font())
                        .size(ui_size(13)),
                    text(format!("{}{local_label}", summary.actor_count)).size(ui_size(11)),
                ]
                .spacing(3)
                .align_y(iced::Alignment::Center),
            )
            .padding([2, 6])
            .style(style),
        );
    }
    summaries_row.wrap().into()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ReactionTokenPresentation {
    emoji: &'static str,
    label: &'static str,
}

fn reaction_token_presentation(
    token: crate::chat::protocol::ReactionToken,
) -> ReactionTokenPresentation {
    use crate::chat::protocol::ReactionToken;
    match token {
        ReactionToken::ThumbsUp => ReactionTokenPresentation {
            emoji: "👍",
            label: "Like",
        },
        ReactionToken::Heart => ReactionTokenPresentation {
            emoji: "❤️",
            label: "Heart",
        },
        ReactionToken::Laugh => ReactionTokenPresentation {
            emoji: "😂",
            label: "Laugh",
        },
        ReactionToken::Surprised => ReactionTokenPresentation {
            emoji: "😮",
            label: "Surprised",
        },
        ReactionToken::Sad => ReactionTokenPresentation {
            emoji: "😢",
            label: "Sad",
        },
        ReactionToken::ThumbsDown => ReactionTokenPresentation {
            emoji: "👎",
            label: "Dislike",
        },
        ReactionToken::Celebrate => ReactionTokenPresentation {
            emoji: "🎉",
            label: "Celebrate",
        },
        ReactionToken::Question => ReactionTokenPresentation {
            emoji: "❓",
            label: "Question",
        },
    }
}

#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
fn omenchat_reaction_controls(
    session_id: crate::chat::ChatSessionId,
    room_id: crate::chat::protocol::RoomId,
    event_id: u64,
) -> Element<'static, Message> {
    let mut controls = row![].spacing(1);
    for token in crate::chat::protocol::ReactionToken::ALL {
        let presentation = reaction_token_presentation(token);
        controls = controls.push(tooltip_button(
            button(
                text(presentation.emoji)
                    .font(emoji_font())
                    .size(ui_size(15)),
            )
            .on_press(Message::OmenChat(OmenChatMessage::ToggleReaction {
                session_id,
                room_id,
                event_id,
                token,
            }))
            .padding([1, 3])
            .style(inline_icon_button_style),
            presentation.label,
        ));
    }
    controls.wrap().into()
}

#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
fn omenchat_message_revision_controls(
    session_id: crate::chat::ChatSessionId,
    room_id: crate::chat::protocol::RoomId,
    event_id: u64,
    correction: bool,
    deletion: bool,
) -> Element<'static, Message> {
    let mut controls = row![].spacing(4).align_y(iced::Alignment::Center);
    if correction {
        controls = controls.push(tooltip_button(
            button(centered_toolbar_icon(ICON_EDIT))
                .on_press(Message::OmenChat(OmenChatMessage::BeginMessageCorrection {
                    session_id,
                    room_id,
                    event_id,
                }))
                .padding(0)
                .width(Length::Fixed(toolbar_icon_button_side()))
                .height(Length::Fixed(toolbar_icon_button_side()))
                .style(inline_icon_button_style),
            "Correct this message",
        ));
    }
    if deletion {
        controls = controls.push(tooltip_button(
            button(centered_toolbar_icon(ICON_DELETE))
                .on_press(Message::OmenChat(OmenChatMessage::BeginMessageDeletion {
                    session_id,
                    room_id,
                    event_id,
                }))
                .padding(0)
                .width(Length::Fixed(toolbar_icon_button_side()))
                .height(Length::Fixed(toolbar_icon_button_side()))
                .style(inline_icon_button_style),
            "Delete this message",
        ));
    }
    controls.into()
}

#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
fn recovered_mutation_notice(count: usize, connection: crate::chat::ChatConnectionState) -> String {
    let noun = if count == 1 { "send" } else { "sends" };
    let verb = if count == 1 { "needs" } else { "need" };
    format!(
        "{count} earlier {noun} {verb} review. Current connection: {}. Joining does not determine whether an earlier send committed.",
        connection.label()
    )
}

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
            let mention_count = desktop
                .omenchat
                .chat_client
                .retained_mention_count(session.session_id, room.room_id);
            let mention_label = if mention_count > 0 {
                format!(" · @{mention_count}")
            } else {
                String::new()
            };
            let unread = if room.unread > 0 {
                format!(" ({})", room.unread)
            } else {
                String::new()
            };
            let label = if room.room_id == session.active_room.room_id {
                format!("[#{}{}]", room.name, mention_label)
            } else {
                format!("#{}{}{}", room.name, unread, mention_label)
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
        room_column = room_column.push(subtle_button(
            "Load Older",
            Message::OmenChat(OmenChatMessage::LoadOlderHistory(session.session_id)),
        ));
        if desktop
            .omenchat
            .chat_client
            .local_user_id(session.session_id)
            .is_some()
        {
            let mute_except_mentions = desktop
                .omenchat
                .chat_client
                .room_mute_except_mentions(session.session_id, session.active_room.room_id);
            room_column = room_column.push(subtle_button_owned(
                if mute_except_mentions {
                    "Mentions only: On".to_string()
                } else {
                    "Mentions only: Off".to_string()
                },
                Message::OmenChat(OmenChatMessage::ToggleMuteExceptMentions {
                    session_id: session.session_id,
                    room_id: session.active_room.room_id,
                }),
            ));
        }
        room_column.width(Length::Shrink)
    } else {
        column![].width(Length::Shrink)
    };

    let mut timeline = column![].spacing(8).width(Length::Fill);
    let local_user_id = desktop
        .omenchat
        .chat_client
        .local_user_id(session.session_id);
    let authoritative_reaction_targets = desktop
        .omenchat
        .chat_client
        .authoritative_reaction_targets(session.session_id, session.active_room.room_id);
    let reaction_target_ids = session
        .events
        .iter()
        .filter(|event| {
            event.room_id == session.active_room.room_id
                && authoritative_reaction_targets.contains(&event.event_id)
        })
        .map(|event| event.event_id)
        .collect::<Vec<_>>();
    let reactions = desktop.omenchat.chat_client.reactions_for_targets(
        session.session_id,
        session.active_room.room_id,
        &reaction_target_ids,
    );
    let revisions = session
        .events
        .iter()
        .filter(|event| {
            event.room_id == session.active_room.room_id
                && desktop
                    .omenchat
                    .chat_client
                    .message_revision_target_authoritative(
                        session.session_id,
                        session.active_room.room_id,
                        event.event_id,
                    )
        })
        .filter_map(|event| {
            desktop.omenchat.chat_client.message_revision_for_target(
                session.session_id,
                session.active_room.room_id,
                event.event_id,
            )
        })
        .collect::<Vec<_>>();
    #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
    let (revision_correction_targets, revision_deletion_targets) = desktop
        .omenchat_message_revision_action_targets(session.session_id, session.active_room.room_id);
    for group in chat_timeline_groups_for_local_user_reactions_and_revisions(
        session,
        local_user_id,
        &reactions,
        revisions,
    ) {
        let header = row![
            text(group.actor).size(ui_size(12)),
            text(chat_event_time_label(group.at_unix)).size(ui_size(11)),
        ]
        .spacing(8)
        .wrap();
        let mut group_content: iced::widget::Column<'_, Message> =
            column![header].spacing(1).width(Length::Fill);
        for body in group.bodies {
            if let Some(reply) = body.reply.as_ref() {
                group_content = group_content.push(omenchat_reply_line(reply, desktop));
            }
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
            } else if let Some(event_id) = body
                .reply_target
                .filter(|_| desktop.omenchat_reply_mentions_available(session.session_id))
            {
                group_content = group_content.push(
                    row![
                        line,
                        inline_icon_button_owned(
                            ICON_REPLY,
                            "Reply",
                            Message::OmenChat(OmenChatMessage::BeginReply {
                                session_id: session.session_id,
                                room_id: session.active_room.room_id,
                                event_id,
                            }),
                        )
                    ]
                    .spacing(8)
                    .align_y(iced::Alignment::Center),
                );
            } else {
                group_content = group_content.push(line);
            }
            #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
            if let Some(event_id) = body.reply_target {
                let correction = revision_correction_targets.contains(&event_id);
                let deletion = revision_deletion_targets.contains(&event_id);
                if correction || deletion {
                    group_content = group_content.push(omenchat_message_revision_controls(
                        session.session_id,
                        session.active_room.room_id,
                        event_id,
                        correction,
                        deletion,
                    ));
                }
            }
            if !body.reactions.is_empty() {
                group_content = group_content.push(omenchat_reaction_summary_row(&body.reactions));
            }
            #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
            if let Some(event_id) = body.reaction_target.filter(|event_id| {
                desktop.omenchat_reactions_available(session.session_id)
                    && desktop.omenchat.chat_client.reaction_snapshot_complete(
                        session.session_id,
                        session.active_room.room_id,
                        *event_id,
                    )
            }) {
                group_content = group_content.push(omenchat_reaction_controls(
                    session.session_id,
                    session.active_room.room_id,
                    event_id,
                ));
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
    let rich_composer_available = desktop.omenchat_reply_mentions_available(session.session_id);
    let local_user_id = desktop
        .omenchat
        .chat_client
        .local_user_id(session.session_id);
    for user in unique_chat_users(&session.users) {
        if rich_composer_available && Some(user.user_id) != local_user_id {
            let selected = desktop
                .omenchat
                .omenchat_selected_mentions
                .get(&session.session_id)
                .is_some_and(|mentions| mentions.contains(&user.user_id));
            let label = if selected {
                format!("✓ {}", user.display_label())
            } else {
                user.display_label()
            };
            userlist = userlist.push(subtle_button_owned(
                label,
                Message::OmenChat(OmenChatMessage::ToggleMention {
                    session_id: session.session_id,
                    user_id: user.user_id,
                }),
            ));
        } else {
            userlist = userlist.push(text(user.display_label()).size(ui_size(13)));
        }
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
        tooltip_button(
            button(centered_toolbar_icon(ICON_SHARE))
                .on_press(Message::OmenChat(OmenChatMessage::CopyInvitation(
                    session.session_id,
                )))
                .padding(0)
                .width(Length::Fixed(toolbar_icon_button_side()))
                .height(Length::Fixed(toolbar_icon_button_side()))
                .style(subtle_button_style),
            "Copy invitation for this room"
        ),
        #[cfg(feature = "desktop-qr")]
        tooltip_button(
            button(centered_toolbar_icon(ICON_QR))
                .on_press(Message::OmenChat(OmenChatMessage::ToggleInvitationQr(
                    session.session_id,
                )))
                .padding(0)
                .width(Length::Fixed(toolbar_icon_button_side()))
                .height(Length::Fixed(toolbar_icon_button_side()))
                .style(subtle_button_style),
            "Show invitation QR"
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
    let mut composer_panel = column![].spacing(6).width(Length::Fill);
    #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
    if let Some(revision) = desktop
        .omenchat
        .omenchat_revision_drafts
        .get(&session.session_id)
        .filter(|revision| {
            revision.room_id == active_room_id
                && revision_correction_targets.contains(&revision.event_id)
        })
    {
        let session_id = session.session_id;
        composer_panel = composer_panel.push(
            container(
                column![
                    text(format!("Editing message #{}", revision.event_id)).size(ui_size(12)),
                    row![
                        text_input("Corrected message", &revision.replacement)
                            .size(ui_size(14))
                            .padding(8)
                            .width(Length::Fill)
                            .on_input(move |value| {
                                Message::OmenChat(OmenChatMessage::MessageCorrectionChanged {
                                    session_id,
                                    value,
                                })
                            })
                            .on_submit(Message::OmenChat(
                                OmenChatMessage::SubmitMessageCorrection(session_id),
                            )),
                        omen_button(
                            "Save correction",
                            Message::OmenChat(OmenChatMessage::SubmitMessageCorrection(session_id)),
                        ),
                        subtle_button(
                            "Cancel",
                            Message::OmenChat(OmenChatMessage::CancelMessageCorrection(session_id)),
                        ),
                    ]
                    .spacing(8)
                    .align_y(iced::Alignment::Center),
                ]
                .spacing(4),
            )
            .padding([6, 8])
            .width(Length::Fill)
            .style(status_container_style),
        );
    }
    #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
    if let Some(confirmation) = desktop
        .omenchat
        .omenchat_revision_delete_confirmation
        .filter(|confirmation| {
            confirmation.session_id == session.session_id
                && confirmation.room_id == active_room_id
                && revision_deletion_targets.contains(&confirmation.event_id)
        })
    {
        composer_panel = composer_panel.push(
            container(
                row![
                    text(format!(
                        "Delete message #{}? This cannot be undone.",
                        confirmation.event_id
                    ))
                    .size(ui_size(12))
                    .width(Length::Fill),
                    omen_button(
                        "Confirm delete",
                        Message::OmenChat(OmenChatMessage::ConfirmMessageDeletion),
                    ),
                    subtle_button(
                        "Cancel",
                        Message::OmenChat(OmenChatMessage::CancelMessageDeletion),
                    ),
                ]
                .spacing(8)
                .align_y(iced::Alignment::Center),
            )
            .padding([6, 8])
            .width(Length::Fill)
            .style(warning_container_style),
        );
    }
    if let Some(reply) = desktop
        .omenchat
        .omenchat_reply_drafts
        .get(&session.session_id)
        .filter(|reply| reply.room_id == active_room_id)
    {
        let label = if rich_composer_available {
            format!("Replying to message #{}", reply.event_id)
        } else {
            format!(
                "Reply to message #{} unavailable until capability returns",
                reply.event_id
            )
        };
        composer_panel = composer_panel.push(
            row![
                text(label).size(ui_size(12)),
                subtle_button_owned(
                    "Cancel reply".to_string(),
                    Message::OmenChat(OmenChatMessage::CancelReply(session.session_id)),
                )
            ]
            .spacing(8)
            .align_y(iced::Alignment::Center),
        );
    }
    let mention_count = desktop
        .omenchat
        .omenchat_selected_mentions
        .get(&session.session_id)
        .map_or(0, std::collections::BTreeSet::len);
    if mention_count > 0 {
        let label = if rich_composer_available {
            format!("Mentioning {mention_count} member(s)")
        } else {
            format!("{mention_count} mention(s) unavailable until capability returns")
        };
        composer_panel = composer_panel.push(
            row![
                text(label).size(ui_size(12)),
                subtle_button_owned(
                    "Clear mentions".to_string(),
                    Message::OmenChat(OmenChatMessage::ClearMentions(session.session_id)),
                )
            ]
            .spacing(8)
            .align_y(iced::Alignment::Center),
        );
    }
    #[cfg(feature = "desktop-qr")]
    if let Some(qr) = desktop
        .omenchat
        .omenchat_invitation_qr
        .as_ref()
        .filter(|qr| qr.session_id == session.session_id)
    {
        composer_panel = composer_panel.push(
            container(
                column![
                    row![
                        text("OMENchat room invitation")
                            .size(ui_size(13))
                            .width(Length::Fill),
                        subtle_button(
                            "Copy URI",
                            Message::OmenChat(OmenChatMessage::CopyInvitation(session.session_id))
                        ),
                        subtle_button(
                            "Close",
                            Message::OmenChat(OmenChatMessage::CloseInvitationQr)
                        ),
                    ]
                    .spacing(8)
                    .align_y(iced::Alignment::Center),
                    container(qr_code(&qr.data).total_size(180.0))
                        .center_x(Length::Fill)
                        .width(Length::Fill),
                    text(qr.uri.as_str())
                        .size(ui_size(11))
                        .width(Length::Fill)
                        .wrapping(Wrapping::WordOrGlyph),
                    text(
                        "Public connection metadata only; recipients still confirm before opening."
                    )
                    .size(ui_size(11)),
                ]
                .spacing(6)
                .width(Length::Fill),
            )
            .padding(8)
            .width(Length::Fill)
            .style(status_container_style),
        );
    }
    composer_panel = composer_panel.push(composer);

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
        omenchat_recovered_mutations_panel(desktop, session.session_id, &session.server.destination)
    {
        timeline_panel = timeline_panel.push(recovered);
    }
    timeline_panel = timeline_panel.push(
        app_scrollable(timeline)
            .anchor_bottom()
            .id(omenchat_scroll_id(session.session_id, active_room_id))
            .on_scroll(move |viewport: Viewport| {
                Message::OmenChat(OmenChatMessage::Scrolled {
                    session_id,
                    room_id: active_room_id,
                    offset: omenchat_offset_from_bottom_anchored_viewport(
                        viewport.relative_offset(),
                    ),
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
        composer_panel
    ]
    .spacing(10)
    .padding(10)
    .height(Length::Fill)
    .into()
}

#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
fn omenchat_recovered_mutations_panel(
    desktop: &DesktopApp,
    session_id: crate::chat::ChatSessionId,
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
    let expanded = desktop
        .omenchat
        .omenchat_recovered_mutations_expanded_for
        .as_deref()
        == Some(server_destination);
    if !expanded {
        let notice = recovered_mutation_notice(
            matching_count,
            desktop.omenchat_connection_state(session_id),
        );
        return Some(
            container(
                row![
                    text(notice).size(ui_size(12)).width(Length::Fill),
                    subtle_button(
                        "Review",
                        Message::OmenChat(OmenChatMessage::ToggleRecoveredMutationReview(
                            server_destination.to_owned(),
                        )),
                    ),
                ]
                .spacing(8),
            )
            .padding([6, 8])
            .width(Length::Fill)
            .style(status_container_style)
            .into(),
        );
    }
    let server = desktop
        .omenchat
        .chat_client
        .sessions()
        .iter()
        .find(|session| session.server.destination == server_destination);
    let server_label = server
        .map(|session| {
            format!(
                "{} ({})",
                session.server.display_name,
                compact_recovery_destination(server_destination)
            )
        })
        .unwrap_or_else(|| compact_recovery_destination(server_destination));
    let mut content = column![row![
        text(format!(
            "Earlier sends needing review — current connection: {}",
            desktop.omenchat_connection_state(session_id).label()
        ))
        .size(ui_size(13))
        .width(Length::Fill),
        subtle_button(
            "Collapse",
            Message::OmenChat(OmenChatMessage::ToggleRecoveredMutationReview(
                server_destination.to_owned(),
            )),
        ),
    ]
    .spacing(8)]
    .spacing(6)
    .width(Length::Fill);
    content = content.push(
        text("Nothing was resent automatically. Joining or receiving pings does not resolve an earlier send whose acknowledgement was lost.")
            .size(ui_size(11)),
    );
    for intent in desktop
        .omenchat
        .omenchat_recovered_mutation_intents
        .iter()
        .filter(|intent| intent.server_destination == server_destination)
        .take(OMENCHAT_RECOVERED_INTENTS_VISIBLE_MAX)
    {
        let past_expiry = intent.expires_at <= now;
        let retry_unavailable = (!past_expiry)
            .then(|| desktop.recovered_omenchat_retry_session_id(intent).err())
            .flatten();
        let operation = crate::operations::omenchat::recovered_mutation_record(
            intent,
            now,
            retry_unavailable.is_none(),
        )
        .ok();
        let state = match operation
            .as_ref()
            .map(|record| (record.state, record.authority))
        {
            Some((
                crate::operations::OperationState::Waiting,
                crate::operations::EvidenceAuthority::Authoritative,
            )) => "prepared; not transmitted",
            Some((
                crate::operations::OperationState::Reconciling,
                crate::operations::EvidenceAuthority::Uncertain,
            )) if !past_expiry => "uncertain; server may have committed it",
            Some((crate::operations::OperationState::Reconciling, _)) if past_expiry => {
                "expired; outcome still requires explicit resolution"
            }
            _ => "unexpected recovered state",
        };
        let room = intent
            .room_id
            .map(|room_id| {
                server
                    .and_then(|session| session.rooms.iter().find(|room| room.room_id == room_id))
                    .map(|room| format!("#{} ({room_id})", room.name))
                    .unwrap_or_else(|| format!("room {room_id}"))
            })
            .unwrap_or_else(|| "no room".into());
        let label = format!(
            "Operation: {} | Server: {server_label} | Room: {room} | State: {state} | {}",
            recovered_mutation_operation(intent.op, &intent.body),
            recovered_mutation_expiry_label(intent.expires_at, now),
        );
        let transmission_available = operation.as_ref().is_some_and(|record| {
            record.valid_actions.iter().any(|action| {
                matches!(
                    action,
                    crate::operations::OperationAction::ExplicitSend
                        | crate::operations::OperationAction::ExplicitSafeRetry
                )
            })
        });
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
                        "Confirm Safe Replay"
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
                    "Keep Reviewing",
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
            } else if transmission_available {
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
            } else {
                row![subtle_button(
                    "Stop Tracking",
                    Message::OmenChat(OmenChatMessage::BeginMutationResolution {
                        mutation_id: intent.mutation_id,
                        action: OmenChatMutationResolutionAction::Abandon,
                    }),
                )]
            }
        };
        let mut recovered = column![text(label).size(ui_size(12))].spacing(4);
        if let Some(reason) = retry_unavailable {
            recovered = recovered.push(
                text(format!("Send/retry unavailable: {reason}"))
                    .size(ui_size(11))
                    .style(iced::widget::text::danger),
            );
        }
        content = content.push(recovered.push(actions));
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
    use super::{
        compact_recovery_destination, omenchat_media_animation_allowed,
        reaction_token_presentation, recovered_mutation_expiry_label, recovered_mutation_notice,
        recovered_mutation_operation,
    };
    use crate::chat::protocol::{ChatOp, FrameBody, ReactionToken};
    use crate::chat::ChatConnectionState;

    #[test]
    fn reduced_motion_and_hidden_panes_withhold_animated_media() {
        assert!(omenchat_media_animation_allowed(true, false));
        assert!(!omenchat_media_animation_allowed(false, false));
        assert!(!omenchat_media_animation_allowed(true, true));
        assert!(!omenchat_media_animation_allowed(false, true));
    }

    #[test]
    fn recovered_mutation_labels_are_redacted_and_semantic() {
        let secret_body = FrameBody::Text("ban private-target-name".into());
        assert_eq!(
            recovered_mutation_operation(ChatOp::Command, &secret_body),
            "ban user"
        );
        assert_eq!(
            recovered_mutation_operation(
                ChatOp::RoomMessage,
                &FrameBody::Text("private message body".into())
            ),
            "room message"
        );
        let reaction = crate::chat::protocol::ReactionRequest {
            target_event_id: 7,
            token: crate::chat::protocol::ReactionToken::Heart,
            action: crate::chat::protocol::ReactionAction::Add,
        }
        .into_frame_body()
        .expect("reaction body");
        assert_eq!(
            recovered_mutation_operation(ChatOp::RoomReaction, &reaction),
            "add reaction"
        );
        let correction = crate::chat::protocol::MessageRevisionRequest {
            target_event_id: 7,
            action: crate::chat::protocol::MessageRevisionAction::Correct,
            replacement: Some("private corrected body".into()),
        }
        .into_frame_body()
        .expect("message revision body");
        assert_eq!(
            recovered_mutation_operation(ChatOp::RoomMessageRevision, &correction),
            "message correction"
        );
        assert!(
            !recovered_mutation_operation(ChatOp::RoomMessageRevision, &correction)
                .contains("private corrected body")
        );
        assert!(!recovered_mutation_operation(ChatOp::Command, &secret_body)
            .contains("private-target-name"));
        assert_eq!(
            recovered_mutation_expiry_label(1_030, 1_000),
            "expires in 30s"
        );
        assert_eq!(
            recovered_mutation_expiry_label(900, 1_000),
            "expired 1m ago"
        );
        assert_eq!(
            compact_recovery_destination("00112233445566778899aabbccddeeff"),
            "001122334455…"
        );
    }

    #[test]
    fn recovered_mutation_notice_separates_join_health_from_earlier_send_outcome() {
        let notice = recovered_mutation_notice(1, ChatConnectionState::Joined);
        assert!(notice.contains("1 earlier send"));
        assert!(notice.contains("Current connection: joined"));
        assert!(notice.contains("does not determine"));

        let plural = recovered_mutation_notice(2, ChatConnectionState::Reconnecting);
        assert!(plural.contains("2 earlier sends"));
        assert!(plural.contains("Current connection: reconnecting"));
    }

    #[test]
    fn reaction_controls_have_compact_emoji_and_semantic_labels() {
        let presentations = ReactionToken::ALL.map(reaction_token_presentation);
        assert_eq!(
            presentations.map(|presentation| presentation.emoji),
            ["👍", "❤️", "😂", "😮", "😢", "👎", "🎉", "❓"]
        );
        assert_eq!(
            presentations.map(|presentation| presentation.label),
            [
                "Like",
                "Heart",
                "Laugh",
                "Surprised",
                "Sad",
                "Dislike",
                "Celebrate",
                "Question",
            ]
        );
        assert_eq!(crate::desktop::ICON_REPLY, "\u{f086}");
    }
}
