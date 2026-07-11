use iced::widget::pane_grid;
use iced::Task;

use crate::app::TabId;
#[cfg(feature = "chat-client")]
use crate::chat::ChatSessionId;

#[cfg(feature = "chat-client")]
use super::is_pending_omenchat_destination;
use super::{DesktopApp, DesktopPane, Message};

impl DesktopApp {
    pub(super) fn dispatch_workspace_pane_message(
        &mut self,
        message: Message,
    ) -> Result<Task<Message>, Message> {
        match message {
            Message::NewConversationPane => Ok(self.update_new_conversation_pane()),
            Message::CloseConversationPaneTab(conversation_id) => {
                self.update_close_conversation_pane_tab(conversation_id);
                Ok(Task::none())
            }
            Message::WorkspacePaneClicked(pane) => {
                self.update_workspace_pane_clicked(pane);
                Ok(Task::none())
            }
            Message::WorkspacePaneDragged(event) => {
                self.update_workspace_pane_dragged(event);
                Ok(Task::none())
            }
            Message::WorkspacePaneResized(event) => {
                self.update_workspace_pane_resized(event);
                Ok(Task::none())
            }
            Message::WorkspacePaneMaximize(pane) => {
                self.update_workspace_pane_maximize(pane);
                Ok(Task::none())
            }
            Message::WorkspacePaneRestore => {
                self.update_workspace_pane_restore();
                Ok(Task::none())
            }
            Message::WorkspacePaneClose(pane) => Ok(self.update_workspace_pane_close(pane)),
            Message::RestoreDesktopPane(kind) => Ok(self.update_restore_desktop_pane(kind)),
            _ => Err(message),
        }
    }

    pub(super) fn update_workspace_pane_clicked(&mut self, pane: pane_grid::Pane) {
        self.focus_workspace_pane(pane);
    }

    pub(super) fn update_workspace_pane_dragged(&mut self, event: pane_grid::DragEvent) {
        match event {
            pane_grid::DragEvent::Dropped { pane, target } => {
                self.workspace.workspace_panes.drop(pane, target);
                self.workspace.active_workspace_pane = pane;
                self.persist_workspace_panes("workspace panes");
            }
            pane_grid::DragEvent::Picked { pane } | pane_grid::DragEvent::Canceled { pane } => {
                self.workspace.active_workspace_pane = pane;
            }
        }
    }

    pub(super) fn update_workspace_pane_resized(&mut self, event: pane_grid::ResizeEvent) {
        self.workspace
            .workspace_panes
            .resize(event.split, event.ratio);
        self.schedule_workspace_panes_persist("workspace panes");
        self.schedule_visible_workspace_scroll_restore(2);
    }

    pub(super) fn update_workspace_pane_maximize(&mut self, pane: pane_grid::Pane) {
        self.workspace.workspace_panes.maximize(pane);
        self.focus_workspace_pane(pane);
        self.schedule_workspace_panes_persist("workspace panes");
    }

    pub(super) fn update_workspace_pane_restore(&mut self) {
        self.workspace.workspace_panes.restore();
        self.schedule_workspace_panes_persist("workspace panes");
    }

    pub(super) fn update_workspace_pane_close(&mut self, pane: pane_grid::Pane) -> Task<Message> {
        self.close_workspace_pane(pane);
        self.persist_workspace_panes("workspace panes");
        self.restore_visible_workspace_scrolls()
    }

    pub(super) fn update_restore_desktop_pane(&mut self, kind: DesktopPane) -> Task<Message> {
        let restore_scroll = self.restore_desktop_pane(kind);
        self.persist_workspace_panes("workspace panes");
        restore_scroll
    }

    pub(in crate::desktop) fn focus_workspace_pane(&mut self, pane: pane_grid::Pane) {
        self.workspace.active_workspace_pane = pane;
        let Some(kind) = self.workspace.workspace_panes.get(pane).cloned() else {
            return;
        };
        match kind {
            DesktopPane::Browser(tab_id) => {
                if let Some(index) = self
                    .app
                    .workspace
                    .browser_tabs
                    .iter()
                    .position(|tab| tab.id == tab_id)
                {
                    self.app.select_browser_tab(index);
                }
            }
            DesktopPane::Conversation(conversation_id) => {
                if let Some(index) = self
                    .app
                    .workspace
                    .conversations
                    .iter()
                    .position(|conversation| conversation.id == conversation_id)
                {
                    self.app.select_conversation_tab(index);
                }
            }
            #[cfg(feature = "chat-client")]
            DesktopPane::OmenChat(session_id) => {
                self.ensure_omenchat_bottom_entry(session_id);
            }
        }
    }

    pub(in crate::desktop) fn select_browser_tab_by_id(&mut self, tab_id: TabId) -> bool {
        let Some(index) = self
            .app
            .workspace
            .browser_tabs
            .iter()
            .position(|tab| tab.id == tab_id)
        else {
            return false;
        };
        self.app.select_browser_tab(index);
        if let Some(pane) = self.find_workspace_pane(&DesktopPane::Browser(tab_id)) {
            self.workspace.active_workspace_pane = pane;
        }
        true
    }

    pub(in crate::desktop) fn ensure_pane_for_active_browser(&mut self) {
        let tab_id = self.app.active_browser_tab().id;
        if let Some(pane) = self.find_workspace_pane(&DesktopPane::Browser(tab_id)) {
            self.workspace.active_workspace_pane = pane;
            return;
        }
        self.split_workspace_from_active(DesktopPane::Browser(tab_id));
    }

    pub(in crate::desktop) fn ensure_pane_for_active_conversation(&mut self) {
        let conversation_id = self.app.active_conversation().id;
        if let Some(pane) = self.find_workspace_pane(&DesktopPane::Conversation(conversation_id)) {
            self.workspace.active_workspace_pane = pane;
            return;
        }
        self.split_workspace_from_active(DesktopPane::Conversation(conversation_id));
    }

    #[cfg(feature = "chat-client")]
    pub(in crate::desktop) fn ensure_pane_for_omenchat(&mut self, session_id: ChatSessionId) {
        if let Some(pane) = self.find_workspace_pane(&DesktopPane::OmenChat(session_id)) {
            self.workspace.active_workspace_pane = pane;
            return;
        }
        self.split_workspace_from_active(DesktopPane::OmenChat(session_id));
    }

    #[cfg(feature = "chat-client")]
    pub(in crate::desktop) fn place_omenchat_session_preferring_active_blank(
        &mut self,
        session_id: ChatSessionId,
    ) {
        let active_blank = self
            .workspace
            .workspace_panes
            .get(self.workspace.active_workspace_pane)
            .and_then(|kind| match kind {
                DesktopPane::OmenChat(blank_id)
                    if *blank_id != session_id
                        && self.omenchat.chat_client.session(*blank_id).is_some_and(
                            |session| is_pending_omenchat_destination(&session.server.destination),
                        ) =>
                {
                    Some(*blank_id)
                }
                _ => None,
            });

        let Some(blank_id) = active_blank else {
            self.ensure_pane_for_omenchat(session_id);
            return;
        };

        let blank_pane = self.workspace.active_workspace_pane;
        if let Some(existing_pane) = self.find_workspace_pane(&DesktopPane::OmenChat(session_id)) {
            if self.workspace.workspace_panes.len() > 1 {
                self.close_workspace_pane(blank_pane);
            } else if let Some(kind) = self.workspace.workspace_panes.get_mut(blank_pane) {
                *kind = DesktopPane::OmenChat(session_id);
            }
            self.workspace.active_workspace_pane = existing_pane;
        } else if let Some(kind) = self.workspace.workspace_panes.get_mut(blank_pane) {
            *kind = DesktopPane::OmenChat(session_id);
            self.workspace.active_workspace_pane = blank_pane;
        } else {
            self.ensure_pane_for_omenchat(session_id);
        }
        self.remove_blank_omenchat_session_state(blank_id);
    }

    #[cfg(feature = "chat-client")]
    pub(in crate::desktop) fn remove_blank_omenchat_session_state(
        &mut self,
        session_id: ChatSessionId,
    ) {
        let Some(session) = self.omenchat.chat_client.session(session_id) else {
            return;
        };
        if !is_pending_omenchat_destination(&session.server.destination) {
            return;
        }
        self.omenchat.chat_drafts.remove(&session_id);
        self.omenchat.omenchat_motds.remove(&session_id);
        self.omenchat.omenchat_upload_quotas.remove(&session_id);
        self.omenchat
            .omenchat_upload_max_file_bytes
            .remove(&session_id);
        self.omenchat
            .chat_scroll_offsets
            .retain(|(stored_session_id, _), _| *stored_session_id != session_id);
        self.omenchat
            .chat_event_counts
            .retain(|(stored_session_id, _), _| *stored_session_id != session_id);
        self.omenchat.chat_client.remove_session(session_id);
    }

    pub(in crate::desktop) fn restore_desktop_pane(&mut self, kind: DesktopPane) -> Task<Message> {
        match kind {
            DesktopPane::Browser(tab_id) => {
                if self.select_browser_tab_by_id(tab_id) {
                    self.ensure_pane_for_active_browser();
                }
                Task::none()
            }
            DesktopPane::Conversation(conversation_id) => {
                if self.select_conversation_by_id(conversation_id) {
                    self.ensure_pane_for_active_conversation();
                    self.remember_conversation_bottom(conversation_id);
                    self.schedule_visible_workspace_scroll_restore(3);
                    return self.restore_conversation_scroll(conversation_id);
                }
                Task::none()
            }
            #[cfg(feature = "chat-client")]
            DesktopPane::OmenChat(session_id) => {
                if self.omenchat.chat_client.session(session_id).is_some() {
                    self.omenchat.chat_drafts.entry(session_id).or_default();
                    self.clear_omenchat_active_room_unread(session_id);
                    self.ensure_pane_for_omenchat(session_id);
                    self.lock_omenchat_bottom_until_restore_settles(session_id);
                    self.schedule_visible_workspace_scroll_restore(3);
                    return self.restore_omenchat_scroll(session_id);
                }
                Task::none()
            }
        }
    }

    pub(in crate::desktop) fn split_workspace_from_active(&mut self, kind: DesktopPane) {
        self.schedule_visible_workspace_scroll_restore(2);
        let target = self
            .workspace
            .workspace_panes
            .get(self.workspace.active_workspace_pane)
            .map(|_| self.workspace.active_workspace_pane)
            .or_else(|| {
                self.workspace
                    .workspace_panes
                    .iter()
                    .next()
                    .map(|(pane, _)| *pane)
            });
        let Some(target) = target else {
            let (panes, pane) = pane_grid::State::new(kind);
            self.workspace.workspace_panes = panes;
            self.workspace.active_workspace_pane = pane;
            return;
        };
        if let Some((pane, _)) =
            self.workspace
                .workspace_panes
                .split(pane_grid::Axis::Vertical, target, kind)
        {
            self.workspace.active_workspace_pane = pane;
        }
    }

    pub(in crate::desktop) fn find_workspace_pane(
        &self,
        kind: &DesktopPane,
    ) -> Option<pane_grid::Pane> {
        self.workspace
            .workspace_panes
            .iter()
            .find_map(|(pane, pane_kind)| (pane_kind == kind).then_some(*pane))
    }

    pub(in crate::desktop) fn close_workspace_pane(&mut self, pane: pane_grid::Pane) {
        if self.workspace.workspace_panes.len() <= 1 {
            return;
        }
        self.schedule_visible_workspace_scroll_restore(2);
        if let Some((_, sibling)) = self.workspace.workspace_panes.close(pane) {
            self.workspace.active_workspace_pane = sibling;
            self.focus_workspace_pane(sibling);
        }
    }

    pub(in crate::desktop) fn close_or_replace_deleted_conversation_pane(
        &mut self,
        pane: Option<pane_grid::Pane>,
    ) {
        let Some(pane) = pane else {
            return;
        };
        if self.workspace.workspace_panes.len() > 1 {
            self.close_workspace_pane(pane);
            return;
        }
        if let Some(kind) = self.workspace.workspace_panes.get_mut(pane) {
            *kind = DesktopPane::Browser(self.app.active_browser_tab().id);
            self.workspace.active_workspace_pane = pane;
            self.focus_workspace_pane(pane);
        }
    }
}

#[cfg(test)]
#[path = "workspace_pane_tests.rs"]
mod tests;
