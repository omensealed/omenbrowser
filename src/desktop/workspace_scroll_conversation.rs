use std::collections::HashMap;

use iced::widget::operation::snap_to;
use iced::widget::scrollable::RelativeOffset;
use iced::widget::Id as ScrollableId;
use iced::Task;

use super::{
    workspace_scroll::scroll_offset_is_at_bottom,
    workspace_scroll::scroll_offset_should_show_history_notice, DesktopApp, DesktopPane, Message,
};

pub(in crate::desktop) fn conversation_scroll_id(conversation_id: u64) -> ScrollableId {
    ScrollableId::from(format!("conversation-scroll-{conversation_id}"))
}

impl DesktopApp {
    pub(in crate::desktop) fn restore_active_conversation_scroll(&self) -> Task<Message> {
        let conversation_id = self.app.active_conversation().id;
        self.restore_conversation_scroll(conversation_id)
    }

    pub(in crate::desktop) fn restore_visible_conversation_scrolls(&self) -> Task<Message> {
        let tasks = self
            .workspace
            .workspace_panes
            .iter()
            .filter_map(|(_, kind)| match kind {
                DesktopPane::Conversation(conversation_id) => {
                    Some(self.restore_conversation_scroll(*conversation_id))
                }
                DesktopPane::Browser(_) => None,
                #[cfg(feature = "chat-client")]
                DesktopPane::OmenChat(_) => None,
            })
            .collect::<Vec<_>>();
        if tasks.is_empty() {
            self.restore_active_conversation_scroll()
        } else {
            Task::batch(tasks)
        }
    }

    pub(in crate::desktop) fn conversation_is_viewing_history(&self, conversation_id: u64) -> bool {
        self.conversation
            .scroll_offsets
            .get(&conversation_id)
            .copied()
            .map(scroll_offset_should_show_history_notice)
            .unwrap_or(false)
    }

    pub(in crate::desktop) fn remember_conversation_bottom(&mut self, conversation_id: u64) {
        self.conversation
            .scroll_offsets
            .insert(conversation_id, RelativeOffset { x: 0.0, y: 1.0 });
    }

    pub(in crate::desktop) fn restore_conversation_scroll(
        &self,
        conversation_id: u64,
    ) -> Task<Message> {
        let offset = self
            .conversation
            .scroll_offsets
            .get(&conversation_id)
            .copied()
            .unwrap_or(RelativeOffset { x: 0.0, y: 1.0 });
        snap_to(conversation_scroll_id(conversation_id), offset)
    }

    pub(in crate::desktop) fn snap_conversations_with_new_messages_to_bottom(
        &mut self,
    ) -> Task<Message> {
        let current_counts = self
            .app
            .workspace
            .conversations
            .iter()
            .map(|conversation| (conversation.id, conversation.thread.messages.len()))
            .collect::<HashMap<_, _>>();
        let tasks = self
            .workspace
            .workspace_panes
            .iter()
            .filter_map(|(_, pane)| match pane {
                DesktopPane::Conversation(conversation_id) => {
                    let previous = self
                        .conversation
                        .message_counts
                        .get(&conversation_id)
                        .copied()
                        .unwrap_or(0);
                    let current = current_counts.get(&conversation_id).copied().unwrap_or(0);
                    if current <= previous {
                        return None;
                    }
                    let was_following_bottom = self
                        .conversation
                        .scroll_offsets
                        .get(&conversation_id)
                        .copied()
                        .map(scroll_offset_is_at_bottom)
                        .unwrap_or(true);
                    if !was_following_bottom {
                        return None;
                    }
                    self.conversation
                        .scroll_offsets
                        .insert(*conversation_id, RelativeOffset { x: 0.0, y: 1.0 });
                    Some(snap_to(
                        conversation_scroll_id(*conversation_id),
                        RelativeOffset { x: 0.0, y: 1.0 },
                    ))
                }
                DesktopPane::Browser(_) => None,
                #[cfg(feature = "chat-client")]
                DesktopPane::OmenChat(_) => None,
            })
            .collect::<Vec<_>>();
        self.conversation.message_counts = current_counts;
        if tasks.is_empty() {
            Task::none()
        } else {
            Task::batch(tasks)
        }
    }
}
