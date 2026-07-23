use iced::widget::operation::snap_to;
use iced::widget::scrollable::RelativeOffset;
use iced::widget::Id as ScrollableId;
use iced::Task;

use crate::chat::protocol::RoomId;
use crate::chat::ChatSessionId;

use super::{
    omenchat_event_counts_by_room, workspace_scroll::scroll_offset_is_at_bottom,
    workspace_scroll::scroll_offset_should_show_history_notice, DesktopApp, DesktopPane, Message,
};

pub(in crate::desktop) fn omenchat_scroll_id(
    session_id: ChatSessionId,
    room_id: RoomId,
) -> ScrollableId {
    ScrollableId::from(format!("omenchat-scroll-{session_id}-{room_id}"))
}

impl DesktopApp {
    pub(in crate::desktop) fn omenchat_is_viewing_history(
        &self,
        session_id: ChatSessionId,
        room_id: RoomId,
    ) -> bool {
        self.omenchat
            .chat_scroll_offsets
            .get(&(session_id, room_id))
            .copied()
            .map(scroll_offset_should_show_history_notice)
            .unwrap_or(false)
    }

    pub(in crate::desktop) fn restore_visible_omenchat_scrolls(&self) -> Task<Message> {
        let tasks = self
            .workspace
            .workspace_panes
            .iter()
            .filter_map(|(_, kind)| match kind {
                DesktopPane::OmenChat(session_id) => {
                    Some(self.restore_omenchat_scroll(*session_id))
                }
                DesktopPane::Browser(_) | DesktopPane::Conversation(_) => None,
            })
            .collect::<Vec<_>>();
        if tasks.is_empty() {
            Task::none()
        } else {
            Task::batch(tasks)
        }
    }

    pub(in crate::desktop) fn restore_omenchat_scroll(
        &self,
        session_id: ChatSessionId,
    ) -> Task<Message> {
        let room_id = self.omenchat_active_room_id(session_id);
        let offset = self
            .omenchat
            .chat_scroll_offsets
            .get(&(session_id, room_id))
            .copied()
            .unwrap_or(RelativeOffset { x: 0.0, y: 1.0 });
        snap_to(omenchat_scroll_id(session_id, room_id), offset)
    }

    pub(in crate::desktop) fn omenchat_active_room_id(&self, session_id: ChatSessionId) -> RoomId {
        self.omenchat
            .chat_client
            .session(session_id)
            .map(|session| session.active_room.room_id)
            .unwrap_or(1)
    }

    pub(in crate::desktop) fn omenchat_scroll_key(
        &self,
        session_id: ChatSessionId,
    ) -> (ChatSessionId, RoomId) {
        (session_id, self.omenchat_active_room_id(session_id))
    }

    pub(in crate::desktop) fn ensure_omenchat_bottom_entry(&mut self, session_id: ChatSessionId) {
        let key = self.omenchat_scroll_key(session_id);
        self.omenchat
            .chat_scroll_offsets
            .entry(key)
            .or_insert(RelativeOffset { x: 0.0, y: 1.0 });
    }

    pub(in crate::desktop) fn remember_omenchat_bottom(&mut self, session_id: ChatSessionId) {
        let key = self.omenchat_scroll_key(session_id);
        self.omenchat
            .chat_scroll_offsets
            .insert(key, RelativeOffset { x: 0.0, y: 1.0 });
    }

    pub(in crate::desktop) fn lock_omenchat_bottom_until_restore_settles(
        &mut self,
        session_id: ChatSessionId,
    ) {
        let key = self.omenchat_scroll_key(session_id);
        self.omenchat.chat_scroll_bottom_locks.insert(key);
        self.omenchat
            .chat_scroll_offsets
            .insert(key, RelativeOffset { x: 0.0, y: 1.0 });
    }

    pub(in crate::desktop) fn lock_omenchat_current_scroll_until_restore_settles(
        &mut self,
        session_id: ChatSessionId,
    ) {
        let key = self.omenchat_scroll_key(session_id);
        self.omenchat.chat_scroll_bottom_locks.insert(key);
        self.omenchat
            .chat_scroll_offsets
            .entry(key)
            .or_insert(RelativeOffset { x: 0.0, y: 1.0 });
    }

    pub(in crate::desktop) fn remember_omenchat_bottom_if_missing(
        &mut self,
        session_id: ChatSessionId,
    ) {
        let key = self.omenchat_scroll_key(session_id);
        self.omenchat
            .chat_scroll_offsets
            .entry(key)
            .or_insert(RelativeOffset { x: 0.0, y: 1.0 });
    }

    pub(in crate::desktop) fn preserve_visible_omenchat_bottom_after_layout_change(
        &mut self,
        ticks: u8,
    ) {
        let followed_keys = self
            .workspace
            .workspace_panes
            .iter()
            .filter_map(|(_, pane)| match pane {
                DesktopPane::OmenChat(session_id)
                    if self
                        .workspace_scroll_pane_is_visible(DesktopPane::OmenChat(*session_id)) =>
                {
                    let key = self.omenchat_scroll_key(*session_id);
                    self.omenchat
                        .chat_scroll_offsets
                        .get(&key)
                        .copied()
                        .map(scroll_offset_is_at_bottom)
                        .unwrap_or(true)
                        .then_some(key)
                }
                DesktopPane::OmenChat(_)
                | DesktopPane::Browser(_)
                | DesktopPane::Conversation(_) => None,
            })
            .collect::<Vec<_>>();
        if followed_keys.is_empty() {
            return;
        }

        self.schedule_visible_workspace_scroll_restore(ticks);
        for key in followed_keys {
            self.omenchat.chat_scroll_bottom_locks.insert(key);
            self.omenchat
                .chat_scroll_offsets
                .insert(key, RelativeOffset { x: 0.0, y: 1.0 });
        }
        self.workspace.pending_workspace_bottom_anchor_ticks = self
            .workspace
            .pending_workspace_bottom_anchor_ticks
            .max(ticks.max(1));
    }

    pub(in crate::desktop) fn snap_omenchat_with_new_events_to_bottom(&mut self) -> Task<Message> {
        let current_counts = omenchat_event_counts_by_room(self.omenchat.chat_client.sessions());
        let visible_sessions = self
            .workspace
            .workspace_panes
            .iter()
            .filter_map(|(_, pane)| match pane {
                DesktopPane::OmenChat(session_id) => Some(*session_id),
                DesktopPane::Browser(_) | DesktopPane::Conversation(_) => None,
            })
            .collect::<Vec<_>>();
        let tasks = visible_sessions
            .into_iter()
            .filter_map(|session_id| {
                let key = self.omenchat_scroll_key(session_id);
                let previous = self
                    .omenchat
                    .chat_event_counts
                    .get(&key)
                    .copied()
                    .unwrap_or(0);
                let current = current_counts.get(&key).copied().unwrap_or(0);
                if current <= previous {
                    return None;
                }
                let was_following_bottom = self
                    .omenchat
                    .chat_scroll_offsets
                    .get(&key)
                    .copied()
                    .map(scroll_offset_is_at_bottom)
                    .unwrap_or(true);
                if !was_following_bottom {
                    return None;
                }
                self.omenchat
                    .chat_scroll_offsets
                    .insert(key, RelativeOffset { x: 0.0, y: 1.0 });
                Some(snap_to(
                    omenchat_scroll_id(session_id, key.1),
                    RelativeOffset { x: 0.0, y: 1.0 },
                ))
            })
            .collect::<Vec<_>>();
        self.omenchat.chat_event_counts = current_counts;
        if tasks.is_empty() {
            Task::none()
        } else {
            Task::batch(tasks)
        }
    }
}
