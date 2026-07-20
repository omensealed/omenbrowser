use iced::Task;

use crate::app::DirectoryScope;
use crate::directory::DirectoryKind;
#[cfg(feature = "chat-client")]
use crate::micron::LinkAction;

use super::{DesktopApp, DirectoryMessage, Message};

impl DesktopApp {
    pub(super) fn dispatch_directory_message(
        &mut self,
        message: Message,
    ) -> Result<Task<Message>, Message> {
        match message {
            Message::Directory(DirectoryMessage::SwitchKind(kind)) => {
                self.update_switch_directory_kind(kind);
                Ok(Task::none())
            }
            Message::Directory(DirectoryMessage::SwitchScope(scope)) => {
                self.update_switch_directory_scope(scope);
                Ok(Task::none())
            }
            Message::Directory(DirectoryMessage::FilterChanged(value)) => {
                self.update_directory_filter_changed(value);
                Ok(Task::none())
            }
            Message::Directory(DirectoryMessage::SelectEntry(index)) => {
                self.update_select_directory_entry(index);
                Ok(Task::none())
            }
            Message::Directory(DirectoryMessage::OpenEntry(index)) => {
                Ok(self.update_open_directory_entry(index))
            }
            Message::Directory(DirectoryMessage::OpenPeerChat(index)) => {
                Ok(self.update_open_peer_chat(index))
            }
            #[cfg(feature = "chat-client")]
            Message::Directory(DirectoryMessage::OpenOmenChat(index)) => {
                Ok(self.update_open_directory_omenchat(index))
            }
            Message::Directory(DirectoryMessage::InspectPeer(index)) => {
                self.update_inspect_directory_peer(index);
                Ok(Task::none())
            }
            Message::Directory(DirectoryMessage::SaveEntry(index)) => {
                self.update_save_directory_entry(index);
                Ok(Task::none())
            }
            Message::Directory(DirectoryMessage::ToggleTrust(index)) => {
                self.update_toggle_directory_trust(index);
                Ok(Task::none())
            }
            Message::Directory(DirectoryMessage::ToggleIdentify(index)) => {
                self.update_toggle_directory_identify(index);
                Ok(Task::none())
            }
            Message::Directory(DirectoryMessage::CycleDelivery(index)) => {
                self.update_cycle_directory_delivery(index);
                Ok(Task::none())
            }
            Message::Directory(DirectoryMessage::RequestPath(index)) => {
                self.update_request_directory_path(index);
                Ok(Task::none())
            }
            Message::Directory(DirectoryMessage::RefreshPropagation(index)) => {
                self.update_refresh_directory_propagation(index);
                Ok(Task::none())
            }
            Message::Directory(DirectoryMessage::CancelPropagationRefresh) => {
                self.app.cancel_propagation_node_refresh();
                Ok(Task::none())
            }
            Message::Directory(DirectoryMessage::UsePropagation(index)) => {
                self.update_use_directory_propagation(index);
                Ok(Task::none())
            }
            Message::Directory(DirectoryMessage::ClearPropagation) => {
                self.update_clear_directory_propagation();
                Ok(Task::none())
            }
            _ => Err(message),
        }
    }

    pub(super) fn update_switch_directory_kind(&mut self, kind: DirectoryKind) {
        self.app.switch_directory_kind(kind);
    }

    pub(super) fn update_switch_directory_scope(&mut self, scope: DirectoryScope) {
        self.app.switch_directory_scope(scope);
    }

    pub(super) fn update_directory_filter_changed(&mut self, value: String) {
        self.app.set_directory_filter(value);
    }

    pub(super) fn update_select_directory_entry(&mut self, index: usize) {
        self.app.select_directory_entry(index);
    }

    pub(super) fn update_open_directory_entry(&mut self, index: usize) -> Task<Message> {
        if self.app.select_directory_entry(index) {
            self.app.open_selected_directory_entry();
            self.ensure_pane_for_active_browser();
            self.persist_workspace_panes("workspace panes");
            return self.restore_visible_workspace_scrolls();
        }
        Task::none()
    }

    pub(super) fn update_open_peer_chat(&mut self, index: usize) -> Task<Message> {
        if self.app.select_directory_entry(index) {
            self.app.message_selected_directory_peer();
            self.ensure_pane_for_active_conversation();
            self.persist_workspace_panes("workspace panes");
            return self.restore_visible_workspace_scrolls();
        }
        Task::none()
    }

    #[cfg(feature = "chat-client")]
    pub(super) fn update_open_directory_omenchat(&mut self, index: usize) -> Task<Message> {
        if self.app.select_directory_entry(index) {
            if let Some(entry) = self.app.selected_directory_entry() {
                let target = format!("omenchat://{}", entry.destination_hash);
                if let Some(task) = self.open_omenchat_link(LinkAction {
                    target,
                    fields: Vec::new(),
                }) {
                    return task;
                }
            }
        }
        Task::none()
    }

    pub(super) fn update_inspect_directory_peer(&mut self, index: usize) {
        if self.app.select_directory_entry(index) {
            self.app.inspect_selected_directory_peer();
        }
    }

    pub(super) fn update_save_directory_entry(&mut self, index: usize) {
        if self.app.select_directory_entry(index) {
            self.app.save_selected_directory_entry();
        }
    }

    pub(super) fn update_toggle_directory_trust(&mut self, index: usize) {
        if self.app.select_directory_entry(index) {
            self.app.toggle_selected_directory_trust();
        }
    }

    pub(super) fn update_toggle_directory_identify(&mut self, index: usize) {
        if self.app.select_directory_entry(index) {
            self.app.toggle_selected_directory_identify_on_connect();
        }
    }

    pub(super) fn update_cycle_directory_delivery(&mut self, index: usize) {
        if self.app.select_directory_entry(index) {
            self.app.cycle_selected_directory_preferred_delivery();
        }
    }

    pub(super) fn update_request_directory_path(&mut self, index: usize) {
        if self.app.select_directory_entry(index) {
            self.app.request_selected_directory_path();
        }
    }

    pub(super) fn update_refresh_directory_propagation(&mut self, index: usize) {
        if self.app.select_directory_entry(index) {
            self.app.refresh_selected_propagation_node();
        }
    }

    pub(super) fn update_use_directory_propagation(&mut self, index: usize) {
        if self.app.select_directory_entry(index) {
            self.app.use_selected_directory_propagation_node();
        }
    }

    pub(super) fn update_clear_directory_propagation(&mut self) {
        self.app.clear_preferred_propagation_node();
    }
}
