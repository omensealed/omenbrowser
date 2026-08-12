use iced::widget::scrollable::RelativeOffset;
use iced::Task;

use crate::workspace::WorkspaceSection;

use super::{DesktopApp, DesktopPane, Message};

#[cfg(all(test, feature = "chat-client"))]
#[path = "workspace_scroll_tests.rs"]
mod tests;

pub(in crate::desktop) fn sanitize_scroll_offset(offset: RelativeOffset) -> RelativeOffset {
    RelativeOffset {
        x: if offset.x.is_finite() {
            offset.x.clamp(0.0, 1.0)
        } else {
            0.0
        },
        y: if offset.y.is_finite() {
            offset.y.clamp(0.0, 1.0)
        } else {
            1.0
        },
    }
}

pub(in crate::desktop) fn scroll_offset_is_at_bottom(offset: RelativeOffset) -> bool {
    sanitize_scroll_offset(offset).y >= 0.95
}

pub(in crate::desktop) fn scroll_offset_should_show_history_notice(offset: RelativeOffset) -> bool {
    sanitize_scroll_offset(offset).y <= 0.88
}

impl DesktopApp {
    pub(in crate::desktop) fn restore_visible_workspace_scrolls(&self) -> Task<Message> {
        #[cfg(not(feature = "chat-client"))]
        {
            self.restore_visible_conversation_scrolls()
        }
        #[cfg(feature = "chat-client")]
        let tasks = {
            let mut tasks = vec![self.restore_visible_conversation_scrolls()];
            tasks.push(self.restore_visible_omenchat_scrolls());
            tasks
        };
        #[cfg(feature = "chat-client")]
        {
            Task::batch(tasks)
        }
    }

    pub(in crate::desktop) fn is_workspace_scroll_restore_settling(&self) -> bool {
        self.workspace.restore_workspace_scrolls_pending
            || self
                .workspace
                .restore_workspace_scroll_locks_release_pending
    }

    pub(in crate::desktop) fn workspace_scroll_pane_is_visible(&self, pane: DesktopPane) -> bool {
        self.workspace_pane_is_visible(&pane)
    }

    pub(in crate::desktop) fn workspace_pane_is_visible(&self, pane: &DesktopPane) -> bool {
        matches!(
            self.app.workspace.active_section,
            WorkspaceSection::Browser | WorkspaceSection::Messages
        ) && self.find_workspace_pane(pane).is_some_and(|pane_id| {
            self.workspace
                .workspace_panes
                .maximized()
                .is_none_or(|maximized| maximized == pane_id)
        })
    }

    pub(in crate::desktop) fn has_visible_lxmf_conversation_pane(&self) -> bool {
        self.workspace.workspace_panes.iter().any(|(_, pane)| {
            matches!(pane, DesktopPane::Conversation(_)) && self.workspace_pane_is_visible(pane)
        })
    }

    pub(in crate::desktop) fn schedule_visible_workspace_scroll_restore(&mut self, ticks: u8) {
        self.workspace.restore_workspace_scrolls_pending = true;
        self.workspace.restore_workspace_scrolls_remaining = self
            .workspace
            .restore_workspace_scrolls_remaining
            .max(ticks.max(1));
        self.workspace
            .restore_workspace_scroll_locks_release_pending = false;
        self.conversation.scroll_restore_locks.extend(
            self.workspace
                .workspace_panes
                .iter()
                .filter_map(|(_, kind)| match kind {
                    DesktopPane::Conversation(conversation_id) => Some(*conversation_id),
                    DesktopPane::Browser(_) => None,
                    #[cfg(feature = "chat-client")]
                    DesktopPane::OmenChat(_) => None,
                }),
        );
        #[cfg(feature = "chat-client")]
        {
            let keys = self
                .workspace
                .workspace_panes
                .iter()
                .filter_map(|(_, kind)| match kind {
                    DesktopPane::OmenChat(session_id) => {
                        Some(self.omenchat_scroll_key(*session_id))
                    }
                    DesktopPane::Browser(_) | DesktopPane::Conversation(_) => None,
                })
                .collect::<Vec<_>>();
            self.omenchat.chat_scroll_bottom_locks.extend(keys);
        }
    }

    pub(in crate::desktop) fn remember_visible_workspace_scroll_bottoms(&mut self) {
        self.conversation
            .scroll_offsets
            .extend(
                self.workspace
                    .workspace_panes
                    .iter()
                    .filter_map(|(_, kind)| match kind {
                        DesktopPane::Conversation(conversation_id) => {
                            Some((*conversation_id, RelativeOffset { x: 0.0, y: 1.0 }))
                        }
                        DesktopPane::Browser(_) => None,
                        #[cfg(feature = "chat-client")]
                        DesktopPane::OmenChat(_) => None,
                    }),
            );
        #[cfg(feature = "chat-client")]
        {
            let keys = self
                .workspace
                .workspace_panes
                .iter()
                .filter_map(|(_, kind)| match kind {
                    DesktopPane::OmenChat(session_id) => {
                        Some(self.omenchat_scroll_key(*session_id))
                    }
                    DesktopPane::Browser(_) | DesktopPane::Conversation(_) => None,
                })
                .collect::<Vec<_>>();
            for key in keys {
                self.omenchat
                    .chat_scroll_offsets
                    .insert(key, RelativeOffset { x: 0.0, y: 1.0 });
            }
        }
    }

    pub(in crate::desktop) fn schedule_visible_workspace_bottom_anchor(&mut self, ticks: u8) {
        self.schedule_visible_workspace_scroll_restore(ticks);
        self.remember_visible_workspace_scroll_bottoms();
        self.workspace.pending_workspace_bottom_anchor_ticks = self
            .workspace
            .pending_workspace_bottom_anchor_ticks
            .max(ticks.max(1));
    }

    pub(in crate::desktop) fn anchor_visible_workspace_scrolls_to_bottom_now(
        &mut self,
        ticks: u8,
    ) -> Task<Message> {
        self.schedule_visible_workspace_scroll_restore(ticks);
        self.remember_visible_workspace_scroll_bottoms();
        self.workspace.pending_workspace_bottom_anchor_ticks = 0;
        self.restore_visible_workspace_scrolls()
    }
}
