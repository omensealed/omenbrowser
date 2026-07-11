use iced::Task;

use crate::app::TabId;

use super::{DesktopApp, DesktopPane, Message};

impl DesktopApp {
    pub(super) fn update_select_browser_tab(&mut self, index: usize) {
        self.app.select_browser_tab(index);
        self.ensure_pane_for_active_browser();
    }

    pub(super) fn update_new_browser_tab(&mut self) -> Task<Message> {
        self.app.finish_active_browser_field_edit_preserving_value();
        self.app.new_browser_tab();
        self.ensure_pane_for_active_browser();
        self.persist_workspace_panes("workspace panes");
        self.restore_visible_workspace_scrolls()
    }

    pub(super) fn update_close_active_browser_tab(&mut self) {
        let closing_id = self.app.active_browser_tab().id;
        self.app.close_active_browser_tab();
        self.remove_workspace_panes_for_missing_targets(Some(closing_id), None);
        self.app.flush_pending_ui_preferences();
        self.persist_workspace_panes("workspace panes");
    }

    pub(super) fn update_close_browser_pane_tab(&mut self, tab_id: TabId) {
        if self.select_browser_tab_by_id(tab_id) {
            let closing_id = self.app.active_browser_tab().id;
            self.app.close_active_browser_tab();
            self.remove_workspace_panes_for_missing_targets(Some(closing_id), None);
            self.app.flush_pending_ui_preferences();
            self.persist_workspace_panes("workspace panes");
        }
    }

    pub(super) fn update_new_conversation_pane(&mut self) -> Task<Message> {
        self.app.new_conversation();
        self.ensure_pane_for_active_conversation();
        self.persist_workspace_panes("workspace panes");
        self.restore_visible_workspace_scrolls()
    }

    pub(super) fn update_close_conversation_pane_tab(&mut self, conversation_id: u64) {
        if self.select_conversation_by_id(conversation_id) {
            let closing_id = self.app.active_conversation().id;
            let closing_pane = self.find_workspace_pane(&DesktopPane::Conversation(closing_id));
            self.app.delete_active_conversation();
            self.close_or_replace_deleted_conversation_pane(closing_pane);
            self.remove_workspace_panes_for_missing_targets(None, Some(closing_id));
            self.conversation.body_editors.remove(&closing_id);
            self.conversation.message_counts.remove(&closing_id);
            self.conversation.scroll_offsets.remove(&closing_id);
            self.conversation.scroll_restore_locks.remove(&closing_id);
            self.app.flush_pending_ui_preferences();
            self.persist_workspace_panes("workspace panes");
        }
    }
}
