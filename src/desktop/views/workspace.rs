use iced::widget::text::Wrapping;
#[cfg(feature = "chat-client")]
use iced::widget::text_input;
use iced::widget::{column, container, pane_grid, row, text, Button};
use iced::{Element, Length};

use crate::app::TabId;
#[cfg(feature = "chat-client")]
use crate::chat::ChatSessionId;
use crate::workspace::WorkspaceSection;

use super::super::*;

impl DesktopApp {
    pub(in crate::desktop) fn browser_messages_workspace_view(&self) -> Element<'_, Message> {
        let controls = action_grid(self.workspace_primary_buttons(), 5);
        let hidden_workspace_panes = self.hidden_workspace_pane_buttons();
        let hidden_conversation_panes = self.hidden_conversation_pane_buttons();
        #[cfg(feature = "chat-client")]
        let omenchat_opener = row![
            text_input(
                "omenchat://<destination hash>",
                self.omenchat.omenchat_server_entry.as_str()
            )
            .size(ui_size(14))
            .padding(8)
            .width(Length::Fill)
            .on_input(|value| Message::OmenChat(OmenChatMessage::ServerEntryChanged(value)))
            .on_submit(Message::OmenChat(OmenChatMessage::OpenServerEntry)),
            omen_button("Open", Message::OmenChat(OmenChatMessage::OpenServerEntry),),
        ]
        .spacing(8);

        let grid = pane_grid(
            &self.workspace.workspace_panes,
            |pane, kind, is_maximized| {
                let title = self.workspace_pane_title(kind);
                let subtitle = self.workspace_pane_subtitle(kind);
                let focused = pane == self.workspace.active_workspace_pane;
                let controls = if is_maximized {
                    let row = row![].spacing(6);
                    #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
                    let row = if let DesktopPane::OmenChat(session_id) = kind {
                        row.push(tooltip_icon_button(
                            ICON_OMENCHAT_PATH,
                            "Request path",
                            Message::OmenChat(OmenChatMessage::RequestPath(*session_id)),
                        ))
                        .push(tooltip_omen_icon_button(
                            ICON_OMENCHAT_RECONNECT,
                            "Reconnect",
                            Message::OmenChat(OmenChatMessage::ReconnectSession(*session_id)),
                        ))
                    } else {
                        row
                    };
                    row.push(tooltip_icon_button(
                        ICON_WINDOW_MAX,
                        "Restore tiled panes",
                        Message::WorkspacePane(WorkspacePaneMessage::Restore),
                    ))
                    .wrap()
                } else {
                    let row = row![].spacing(6);
                    #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
                    let row = if let DesktopPane::OmenChat(session_id) = kind {
                        row.push(tooltip_icon_button(
                            ICON_OMENCHAT_PATH,
                            "Request path",
                            Message::OmenChat(OmenChatMessage::RequestPath(*session_id)),
                        ))
                        .push(tooltip_omen_icon_button(
                            ICON_OMENCHAT_RECONNECT,
                            "Reconnect",
                            Message::OmenChat(OmenChatMessage::ReconnectSession(*session_id)),
                        ))
                    } else {
                        row
                    };
                    let mut row = row
                        .push(tooltip_icon_button(
                            ICON_WINDOW_MAX,
                            "Maximize pane",
                            Message::WorkspacePane(WorkspacePaneMessage::Maximize(pane)),
                        ))
                        .push(tooltip_icon_button(
                            ICON_WINDOW_HIDE,
                            "Close pane to restore tabs",
                            Message::WorkspacePane(WorkspacePaneMessage::Close(pane)),
                        ));
                    row = match kind {
                        DesktopPane::Browser(tab_id) => row.push(tooltip_warning_icon_button(
                            ICON_WINDOW_CLOSE,
                            "Delete browser tab",
                            Message::Browser(BrowserMessage::ClosePaneTab(*tab_id)),
                        )),
                        DesktopPane::Conversation(conversation_id) => {
                            row.push(tooltip_warning_icon_button(
                                ICON_WINDOW_CLOSE,
                                "Delete conversation history",
                                Message::WorkspacePane(WorkspacePaneMessage::CloseConversationTab(
                                    *conversation_id,
                                )),
                            ))
                        }
                        #[cfg(feature = "chat-client")]
                        DesktopPane::OmenChat(session_id) => row.push(tooltip_warning_icon_button(
                            ICON_WINDOW_CLOSE,
                            "Disconnect and close chat",
                            Message::OmenChat(OmenChatMessage::CloseSession(*session_id)),
                        )),
                    };
                    row.wrap()
                };
                let compact_controls = if is_maximized {
                    let row = row![].spacing(6);
                    #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
                    let row = if let DesktopPane::OmenChat(session_id) = kind {
                        row.push(tooltip_icon_button(
                            ICON_OMENCHAT_PATH,
                            "Request path",
                            Message::OmenChat(OmenChatMessage::RequestPath(*session_id)),
                        ))
                        .push(tooltip_omen_icon_button(
                            ICON_OMENCHAT_RECONNECT,
                            "Reconnect",
                            Message::OmenChat(OmenChatMessage::ReconnectSession(*session_id)),
                        ))
                    } else {
                        row
                    };
                    row.push(tooltip_icon_button(
                        ICON_WINDOW_MAX,
                        "Restore tiled panes",
                        Message::WorkspacePane(WorkspacePaneMessage::Restore),
                    ))
                    .wrap()
                } else {
                    let row = row![].spacing(6);
                    #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
                    let row = if let DesktopPane::OmenChat(session_id) = kind {
                        row.push(tooltip_icon_button(
                            ICON_OMENCHAT_PATH,
                            "Request path",
                            Message::OmenChat(OmenChatMessage::RequestPath(*session_id)),
                        ))
                        .push(tooltip_omen_icon_button(
                            ICON_OMENCHAT_RECONNECT,
                            "Reconnect",
                            Message::OmenChat(OmenChatMessage::ReconnectSession(*session_id)),
                        ))
                    } else {
                        row
                    };
                    let mut row = row
                        .push(tooltip_icon_button(
                            ICON_WINDOW_MAX,
                            "Maximize pane",
                            Message::WorkspacePane(WorkspacePaneMessage::Maximize(pane)),
                        ))
                        .push(tooltip_icon_button(
                            ICON_WINDOW_HIDE,
                            "Close pane to restore tabs",
                            Message::WorkspacePane(WorkspacePaneMessage::Close(pane)),
                        ));
                    row = match kind {
                        DesktopPane::Browser(tab_id) => row.push(tooltip_warning_icon_button(
                            ICON_WINDOW_CLOSE,
                            "Delete browser tab",
                            Message::Browser(BrowserMessage::ClosePaneTab(*tab_id)),
                        )),
                        DesktopPane::Conversation(conversation_id) => {
                            row.push(tooltip_warning_icon_button(
                                ICON_WINDOW_CLOSE,
                                "Delete conversation history",
                                Message::WorkspacePane(WorkspacePaneMessage::CloseConversationTab(
                                    *conversation_id,
                                )),
                            ))
                        }
                        #[cfg(feature = "chat-client")]
                        DesktopPane::OmenChat(session_id) => row.push(tooltip_warning_icon_button(
                            ICON_WINDOW_CLOSE,
                            "Disconnect and close chat",
                            Message::OmenChat(OmenChatMessage::CloseSession(*session_id)),
                        )),
                    };
                    row.wrap()
                };
                let title_content = container(
                    column![
                        row![
                            text(if focused { "*" } else { " " }).size(ui_size(13)),
                            text(title)
                                .size(ui_size(15))
                                .width(Length::Fill)
                                .wrapping(Wrapping::WordOrGlyph),
                        ]
                        .spacing(6)
                        .width(Length::Fill),
                        text(subtitle.unwrap_or_default())
                            .size(ui_size(12))
                            .width(Length::Fill)
                            .wrapping(Wrapping::WordOrGlyph),
                    ]
                    .spacing(2)
                    .width(Length::Fill),
                )
                .width(Length::Fill)
                .clip(true);
                let title_bar = pane_grid::TitleBar::new(title_content)
                    .controls(pane_grid::Controls::dynamic(controls, compact_controls))
                    .padding(8)
                    .style(pane_title_container_style);

                pane_grid::Content::new(
                    container(self.workspace_pane_body(kind, self.workspace_pane_is_visible(kind)))
                        .width(Length::Fill)
                        .height(Length::Fill)
                        .clip(true),
                )
                .title_bar(title_bar)
                .style(workspace_pane_container_style)
            },
        )
        .spacing(8)
        .on_click(|pane| Message::WorkspacePane(WorkspacePaneMessage::Clicked(pane)))
        .on_drag(|event| Message::WorkspacePane(WorkspacePaneMessage::Dragged(event)))
        .on_resize(8, |event| {
            Message::WorkspacePane(WorkspacePaneMessage::Resized(event))
        });

        #[cfg(feature = "chat-client")]
        let content = column![
            controls,
            hidden_workspace_panes,
            hidden_conversation_panes,
            omenchat_opener,
            grid
        ]
        .spacing(8)
        .height(Length::Fill)
        .width(Length::Fill)
        .into();
        #[cfg(not(feature = "chat-client"))]
        let content = column![
            controls,
            hidden_workspace_panes,
            hidden_conversation_panes,
            grid
        ]
        .spacing(8)
        .height(Length::Fill)
        .width(Length::Fill)
        .into();
        content
    }

    pub(in crate::desktop) fn workspace_primary_buttons(&self) -> Vec<Button<'_, Message>> {
        let controls = vec![
            omen_button("New Browser", Message::Browser(BrowserMessage::NewTab)),
            omen_button(
                "New Conversation",
                Message::WorkspacePane(WorkspacePaneMessage::NewConversation),
            ),
            subtle_button(
                "Directory",
                Message::Shell(ShellMessage::SwitchSection(WorkspaceSection::Directory)),
            ),
        ];
        #[cfg(feature = "chat-client")]
        {
            let mut controls = controls;
            controls.insert(
                2,
                omen_button("New Chat", Message::OmenChat(OmenChatMessage::NewPane)),
            );
            controls
        }
        #[cfg(not(feature = "chat-client"))]
        {
            controls
        }
    }

    pub(in crate::desktop) fn hidden_workspace_pane_buttons(&self) -> Element<'_, Message> {
        let buttons = self
            .hidden_browser_panes()
            .into_iter()
            .map(|(tab_id, label)| {
                restore_pane_button(
                    ICON_RESTORE_BROWSER,
                    label,
                    Message::WorkspacePane(WorkspacePaneMessage::RestoreDesktop(
                        DesktopPane::Browser(tab_id),
                    )),
                    false,
                )
            })
            .chain({
                #[cfg(feature = "chat-client")]
                {
                    self.hidden_omenchat_panes()
                        .into_iter()
                        .map(|(session_id, label, unread)| {
                            restore_pane_button(
                                ICON_RESTORE_CHAT,
                                label,
                                Message::WorkspacePane(WorkspacePaneMessage::RestoreDesktop(
                                    DesktopPane::OmenChat(session_id),
                                )),
                                unread,
                            )
                        })
                        .collect::<Vec<_>>()
                }
                #[cfg(not(feature = "chat-client"))]
                {
                    Vec::new()
                }
            })
            .collect::<Vec<_>>();
        if buttons.is_empty() {
            return text("").size(ui_size(1)).into();
        }
        action_grid(buttons, 5)
    }

    pub(in crate::desktop) fn hidden_conversation_pane_buttons(&self) -> Element<'_, Message> {
        let buttons = self
            .hidden_conversation_panes()
            .into_iter()
            .map(|(conversation_id, label, unread)| {
                restore_pane_button(
                    ICON_RESTORE_MESSAGES,
                    label,
                    Message::WorkspacePane(WorkspacePaneMessage::RestoreDesktop(
                        DesktopPane::Conversation(conversation_id),
                    )),
                    unread,
                )
            })
            .collect::<Vec<_>>();
        if buttons.is_empty() {
            return text("").size(ui_size(1)).into();
        }
        action_grid(buttons, 5)
    }

    pub(in crate::desktop) fn workspace_pane_title(&self, kind: &DesktopPane) -> String {
        match kind {
            DesktopPane::Browser(tab_id) => self
                .app
                .workspace
                .browser_tabs
                .iter()
                .find(|tab| tab.id == *tab_id)
                .map(|tab| format!("{} - Browser", compact_label(&tab.title, 32)))
                .unwrap_or_else(|| "closed tab - Browser".into()),
            DesktopPane::Conversation(conversation_id) => self
                .app
                .workspace
                .conversations
                .iter()
                .find(|conversation| conversation.id == *conversation_id)
                .map(|conversation| {
                    format!("{} - Messages", compact_label(&conversation.peer_label, 32))
                })
                .unwrap_or_else(|| "closed conversation - Messages".into()),
            #[cfg(feature = "chat-client")]
            DesktopPane::OmenChat(session_id) => self
                .omenchat
                .chat_client
                .session(*session_id)
                .map(|session| {
                    format!(
                        "{} - OMENchat",
                        compact_label(&session.server.display_name, 32)
                    )
                })
                .unwrap_or_else(|| "closed session - OMENchat".into()),
        }
    }

    pub(in crate::desktop) fn workspace_pane_subtitle(&self, kind: &DesktopPane) -> Option<String> {
        match kind {
            DesktopPane::Browser(_) => None,
            DesktopPane::Conversation(conversation_id) => self
                .app
                .workspace
                .conversations
                .iter()
                .find(|conversation| conversation.id == *conversation_id)
                .and_then(|conversation| {
                    let peer_hash = printable_label(conversation.peer_hash.trim());
                    (!peer_hash.is_empty()).then(|| format!("peer: {peer_hash}"))
                }),
            #[cfg(feature = "chat-client")]
            DesktopPane::OmenChat(session_id) => self
                .omenchat
                .chat_client
                .session(*session_id)
                .map(|session| {
                    let room = compact_label(&session.active_room.name, 18);
                    let status = compact_label(&session.status, 42);
                    format!(
                        "room: #{} | {} users | {}",
                        room,
                        unique_chat_users(&session.users).len(),
                        status
                    )
                }),
        }
    }

    pub(in crate::desktop) fn workspace_pane_body(
        &self,
        kind: &DesktopPane,
        pane_visible: bool,
    ) -> Element<'_, Message> {
        #[cfg(not(feature = "chat-client"))]
        let _ = pane_visible;
        match kind {
            DesktopPane::Browser(tab_id) => views::browser::browser_view_for_tab(self, *tab_id),
            DesktopPane::Conversation(conversation_id) => {
                views::conversation::messages_view_for_conversation(self, *conversation_id)
            }
            #[cfg(feature = "chat-client")]
            DesktopPane::OmenChat(session_id) => {
                let animate_media = views::omenchat::omenchat_media_animation_allowed(
                    pane_visible,
                    self.app.settings.ui.reduce_motion,
                );
                views::omenchat::omenchat_view_for_session(self, *session_id, animate_media)
            }
        }
    }

    pub(in crate::desktop) fn active_conversation_pane_is_visible(&self) -> bool {
        let Some(conversation) = self
            .app
            .workspace
            .conversations
            .get(self.app.workspace.active_conversation)
        else {
            return false;
        };
        self.find_workspace_pane(&DesktopPane::Conversation(conversation.id))
            .is_some()
    }

    pub(in crate::desktop) fn hidden_browser_panes(&self) -> Vec<(TabId, String)> {
        self.app
            .workspace
            .browser_tabs
            .iter()
            .filter(|tab| {
                self.find_workspace_pane(&DesktopPane::Browser(tab.id))
                    .is_none()
            })
            .map(|tab| (tab.id, compact_label(&tab.title, 18)))
            .collect()
    }

    pub(in crate::desktop) fn hidden_conversation_panes(&self) -> Vec<(u64, String, bool)> {
        self.app
            .workspace
            .conversations
            .iter()
            .filter(|conversation| !Self::conversation_is_empty_restore_placeholder(conversation))
            .filter(|conversation| {
                self.find_workspace_pane(&DesktopPane::Conversation(conversation.id))
                    .is_none()
            })
            .map(|conversation| {
                let unread = conversation.thread.unread_count > 0
                    || conversation
                        .thread
                        .messages
                        .iter()
                        .any(|message| message.unread);
                (
                    conversation.id,
                    compact_label(&conversation.peer_label, 18),
                    unread,
                )
            })
            .collect()
    }

    fn conversation_is_empty_restore_placeholder(
        conversation: &crate::messaging::Conversation,
    ) -> bool {
        conversation.peer_hash.trim().is_empty()
            && conversation
                .peer_label
                .trim()
                .eq_ignore_ascii_case("New Conversation")
            && conversation.draft_title.trim().is_empty()
            && conversation.draft_body.trim().is_empty()
            && conversation.attachments.is_empty()
            && conversation.thread.messages.is_empty()
    }

    #[cfg(feature = "chat-client")]
    pub(in crate::desktop) fn hidden_omenchat_panes(&self) -> Vec<(ChatSessionId, String, bool)> {
        self.omenchat
            .chat_client
            .sessions()
            .iter()
            .filter(|session| {
                self.find_workspace_pane(&DesktopPane::OmenChat(session.session_id))
                    .is_none()
            })
            .map(|session| {
                let unread = session.active_room.unread > 0
                    || session.rooms.iter().any(|room| room.unread > 0);
                (
                    session.session_id,
                    compact_label(&session.server.display_name, 18),
                    unread,
                )
            })
            .collect()
    }
}
