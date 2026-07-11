use iced::Task;

use super::{DesktopApp, Message};

impl DesktopApp {
    pub(super) fn update(&mut self, message: Message) -> Task<Message> {
        let message = match self.dispatch_browser_message(message) {
            Ok(task) => return task,
            Err(message) => message,
        };
        let message = match self.dispatch_conversation_message(message) {
            Ok(task) => return task,
            Err(message) => message,
        };
        let message = match self.dispatch_directory_message(message) {
            Ok(task) => return task,
            Err(message) => message,
        };
        let message = match self.dispatch_diagnostics_message(message) {
            Ok(task) => return task,
            Err(message) => message,
        };
        let message = match self.dispatch_identity_message(message) {
            Ok(task) => return task,
            Err(message) => message,
        };
        let message = match self.dispatch_theme_message(message) {
            Ok(task) => return task,
            Err(message) => message,
        };
        let message = match self.dispatch_clearweb_message(message) {
            Ok(task) => return task,
            Err(message) => message,
        };
        let message = match self.dispatch_external_browser_message(message) {
            Ok(task) => return task,
            Err(message) => message,
        };
        let message = match self.dispatch_runtime_message(message) {
            Ok(task) => return task,
            Err(message) => message,
        };
        let message = match self.dispatch_shell_message(message) {
            Ok(task) => return task,
            Err(message) => message,
        };
        let message = match self.dispatch_interface_message(message) {
            Ok(task) => return task,
            Err(message) => message,
        };
        let message = match self.dispatch_plugin_message(message) {
            Ok(task) => return task,
            Err(message) => message,
        };
        #[cfg(feature = "chat-client")]
        let message = match self.dispatch_omenchat_message(message) {
            Ok(task) => return task,
            Err(message) => message,
        };
        let message = match self.dispatch_workspace_pane_message(message) {
            Ok(task) => return task,
            Err(message) => message,
        };
        debug_assert!(
            false,
            "unhandled desktop message: {message:?}; add a dispatch route"
        );
        Task::none()
    }
}
