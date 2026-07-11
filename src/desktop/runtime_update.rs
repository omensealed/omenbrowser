use iced::Task;

use crate::runtime::InterfaceStats;
use crate::storage::settings::RuntimeBackendSetting;

use super::{DesktopApp, Message};

impl DesktopApp {
    pub(super) fn dispatch_runtime_message(
        &mut self,
        message: Message,
    ) -> Result<Task<Message>, Message> {
        match message {
            Message::ToggleAutoSyncAfterPropagationAccept => {
                self.update_toggle_auto_sync_after_propagation_accept();
                Ok(Task::none())
            }
            Message::SelectNativeBackend => {
                self.update_select_native_backend();
                Ok(Task::none())
            }
            Message::StartNativeRuntime => {
                self.update_start_native_runtime();
                Ok(Task::none())
            }
            Message::NativeQuickstart => {
                self.update_native_quickstart();
                Ok(Task::none())
            }
            Message::InterfaceStatsSampled(result) => {
                self.update_interface_stats_sampled(result);
                Ok(Task::none())
            }
            _ => Err(message),
        }
    }

    pub(super) fn sample_runtime_interface_stats(&self) -> Task<Message> {
        let runtime = self.app.runtime.clone();
        Task::perform(
            async move {
                runtime
                    .interface_stats()
                    .await
                    .map_err(|error| error.to_string())
            },
            Message::InterfaceStatsSampled,
        )
    }

    pub(super) fn update_toggle_auto_sync_after_propagation_accept(&mut self) {
        self.app.toggle_auto_sync_after_propagation_accept();
    }

    pub(super) fn update_select_native_backend(&mut self) {
        self.app
            .set_runtime_backend_setting(RuntimeBackendSetting::Reticulum);
    }

    pub(super) fn update_start_native_runtime(&mut self) {
        self.app.start_configured_runtime_nonblocking();
    }

    pub(super) fn update_native_quickstart(&mut self) {
        self.app.run_native_quickstart();
    }

    pub(super) fn update_interface_stats_sampled(
        &mut self,
        result: Result<InterfaceStats, String>,
    ) {
        match result {
            Ok(stats) => {
                self.app.monitoring_state.last_interface_stats = Some(stats.clone());
                self.app.status.task = if stats.available {
                    format!("interfaces: {}", stats.interfaces.len())
                } else {
                    stats
                        .reason
                        .unwrap_or_else(|| "interfaces unavailable".into())
                };
            }
            Err(error) => {
                self.app.status.task = format!("interface status failed: {error}");
            }
        }
    }
}
