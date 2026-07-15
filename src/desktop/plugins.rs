use iced::Task;

use super::{DesktopApp, Message, PluginMessage};

impl DesktopApp {
    pub(super) fn dispatch_plugin_message(
        &mut self,
        message: Message,
    ) -> Result<Task<Message>, Message> {
        match message {
            Message::Plugin(PluginMessage::Select(index)) => {
                self.update_select_plugin(index);
                Ok(Task::none())
            }
            Message::Plugin(PluginMessage::Toggle(index)) => {
                self.update_toggle_plugin(index);
                Ok(Task::none())
            }
            Message::Plugin(PluginMessage::BeginRemove(index)) => {
                self.update_begin_plugin_remove(index);
                Ok(Task::none())
            }
            Message::Plugin(PluginMessage::ToggleSelected) => {
                self.update_toggle_selected_plugin();
                Ok(Task::none())
            }
            Message::Plugin(PluginMessage::BeginInstall) => {
                self.update_begin_plugin_install();
                Ok(Task::none())
            }
            Message::Plugin(PluginMessage::BeginSelectedRemove) => {
                self.update_begin_selected_plugin_remove();
                Ok(Task::none())
            }
            Message::Plugin(PluginMessage::Refresh) => {
                self.update_refresh_plugins();
                Ok(Task::none())
            }
            Message::Plugin(PluginMessage::ShowLogs) => {
                self.update_show_plugin_logs();
                Ok(Task::none())
            }
            _ => Err(message),
        }
    }

    pub(super) fn update_select_plugin(&mut self, index: usize) {
        self.app.select_plugin(index);
    }

    pub(super) fn update_toggle_plugin(&mut self, index: usize) {
        if self.app.select_plugin(index) {
            self.app.toggle_selected_plugin();
        }
    }

    pub(super) fn update_begin_plugin_remove(&mut self, index: usize) {
        if self.app.select_plugin(index) {
            self.app.begin_selected_plugin_remove_flow();
        }
    }

    pub(super) fn update_toggle_selected_plugin(&mut self) {
        self.app.toggle_selected_plugin();
    }

    pub(super) fn update_begin_plugin_install(&mut self) {
        self.app.begin_plugin_install_flow();
    }

    pub(super) fn update_begin_selected_plugin_remove(&mut self) {
        self.app.begin_selected_plugin_remove_flow();
    }

    pub(super) fn update_refresh_plugins(&mut self) {
        self.app.refresh_plugins_from_registry();
    }

    pub(super) fn update_show_plugin_logs(&mut self) {
        self.app.show_plugin_logs();
    }
}
