use iced::keyboard;
use iced::{window, Task};
use std::time::Duration;

use crate::app::current_epoch_ms;
use crate::workspace::WorkspaceSection;

use super::{process_resource_usage, DesktopApp, Message, ShellMessage, ShutdownOutcome};

const DESKTOP_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(5);

async fn bounded_shutdown<F, E>(future: F, timeout: Duration) -> ShutdownOutcome
where
    F: std::future::Future<Output = Result<(), E>>,
    E: std::fmt::Display,
{
    match tokio::time::timeout(timeout, future).await {
        Ok(Ok(())) => ShutdownOutcome::Stopped,
        Ok(Err(error)) => ShutdownOutcome::Failed(error.to_string()),
        Err(_) => ShutdownOutcome::TimedOut,
    }
}

impl DesktopApp {
    pub(super) fn dispatch_shell_message(
        &mut self,
        message: Message,
    ) -> Result<Task<Message>, Message> {
        match message {
            Message::Shell(ShellMessage::SwitchSection(section)) => {
                Ok(self.update_switch_section(section))
            }
            Message::Shell(ShellMessage::KeyboardModifiersChanged(modifiers)) => {
                self.update_keyboard_modifiers_changed(modifiers);
                Ok(Task::none())
            }
            Message::Shell(ShellMessage::ToggleNavigation) => {
                self.update_toggle_navigation();
                Ok(Task::none())
            }
            Message::Shell(ShellMessage::WorkspaceScrollTick) => {
                Ok(self.update_workspace_scroll_tick())
            }
            Message::Shell(ShellMessage::InternalEventsReady) => {
                Ok(self.update_internal_events_ready())
            }
            Message::Shell(ShellMessage::PersistenceDeadlineReached) => {
                Ok(self.update_persistence_deadline_reached())
            }
            Message::Shell(ShellMessage::MonitoringTick) => Ok(self.update_monitoring_tick()),
            Message::Shell(ShellMessage::LxmfReconcileDeadlineReached) => {
                Ok(self.update_lxmf_reconcile_deadline_reached())
            }
            Message::Shell(ShellMessage::BrowserPartialDeadlineReached) => {
                Ok(self.update_browser_partial_deadline_reached())
            }
            Message::Shell(ShellMessage::OmenChatMaintenanceDeadlineReached) => {
                Ok(self.update_omenchat_maintenance_deadline_reached())
            }
            Message::Shell(ShellMessage::WindowCloseRequested(window_id)) => {
                Ok(self.update_window_close_requested(window_id))
            }
            Message::Shell(ShellMessage::WindowShutdownBegin(window_id)) => {
                Ok(self.update_window_shutdown_begin(window_id))
            }
            Message::Shell(ShellMessage::WindowShutdownComplete { window_id, outcome }) => {
                Ok(self.update_window_shutdown_complete(window_id, outcome))
            }
            _ => Err(message),
        }
    }

    pub(super) fn update_window_close_requested(&mut self, window_id: window::Id) -> Task<Message> {
        if !self.ui.shutdown_phase.request() {
            return Task::none();
        }
        Task::done(Message::Shell(ShellMessage::WindowShutdownBegin(window_id)))
    }

    pub(super) fn update_window_shutdown_begin(&mut self, window_id: window::Id) -> Task<Message> {
        if !self.ui.shutdown_phase.begin_draining() {
            return Task::none();
        }
        self.app.cancel_propagation_node_refresh_for_shutdown();
        self.app.flush_pending_ui_preferences();
        self.app.flush_pending_directory_persistence();
        let runtime = self.app.runtime.clone();
        let structured_logs = self.app.structured_log_flush_handle();
        Task::perform(
            async move {
                let log_flush = async move {
                    let Some(structured_logs) = structured_logs else {
                        return true;
                    };
                    tokio::task::spawn_blocking(move || {
                        structured_logs.flush(DESKTOP_SHUTDOWN_TIMEOUT)
                    })
                    .await
                    .unwrap_or(false)
                };
                let (outcome, logs_flushed) = tokio::join!(
                    bounded_shutdown(runtime.stop_runtime(), DESKTOP_SHUTDOWN_TIMEOUT),
                    log_flush
                );
                if !logs_flushed {
                    tracing::warn!("structured log flush timed out during desktop shutdown");
                }
                (window_id, outcome)
            },
            |(window_id, outcome)| {
                Message::Shell(ShellMessage::WindowShutdownComplete { window_id, outcome })
            },
        )
    }

    pub(super) fn update_window_shutdown_complete(
        &mut self,
        window_id: window::Id,
        outcome: ShutdownOutcome,
    ) -> Task<Message> {
        if !self.ui.shutdown_phase.finish() {
            return Task::none();
        }
        match outcome {
            ShutdownOutcome::Stopped => {
                tracing::info!("desktop shutdown drained successfully");
            }
            ShutdownOutcome::Failed(error) => {
                tracing::warn!(error = %error, "desktop runtime shutdown failed; closing normally");
            }
            ShutdownOutcome::TimedOut => {
                tracing::warn!(
                    timeout_seconds = DESKTOP_SHUTDOWN_TIMEOUT.as_secs(),
                    "desktop runtime shutdown timed out; closing normally"
                );
            }
        }
        window::close(window_id)
    }

    pub(super) fn update_switch_section(&mut self, section: WorkspaceSection) -> Task<Message> {
        self.app.switch_section(section);
        if matches!(
            section,
            WorkspaceSection::Browser | WorkspaceSection::Messages
        ) {
            self.schedule_visible_workspace_scroll_restore(2);
            return self.restore_visible_workspace_scrolls();
        }
        Task::none()
    }

    pub(super) fn update_keyboard_modifiers_changed(&mut self, modifiers: keyboard::Modifiers) {
        self.ui.ctrl_down = modifiers.control();
    }

    pub(super) fn update_internal_events_ready(&mut self) -> Task<Message> {
        let active_conversation_readable = self.active_conversation_pane_is_visible();
        let applied = self
            .app
            .drain_internal_events_with_active_conversation_readable(active_conversation_readable);
        if applied > 0 {
            self.sync_conversation_body_editors();
        }
        tracing::debug!(applied, "desktop internal event wake drained");
        #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
        let omenchat_runtime_events = self.drain_omenchat_runtime_events();
        Task::batch([
            #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
            omenchat_runtime_events,
            self.snap_conversations_with_new_messages_to_bottom(),
            #[cfg(feature = "chat-client")]
            self.snap_omenchat_with_new_events_to_bottom(),
        ])
    }

    pub(super) fn update_toggle_navigation(&mut self) {
        self.ui.navigation_open = !self.ui.navigation_open;
    }

    pub(super) fn update_workspace_scroll_tick(&mut self) -> Task<Message> {
        if self
            .workspace
            .restore_workspace_scroll_locks_release_pending
        {
            self.workspace
                .restore_workspace_scroll_locks_release_pending = false;
            self.conversation.scroll_restore_locks.clear();
            #[cfg(feature = "chat-client")]
            self.omenchat.chat_scroll_bottom_locks.clear();
        }
        if self.workspace.restore_workspace_scrolls_pending {
            self.workspace.restore_workspace_scrolls_remaining = self
                .workspace
                .restore_workspace_scrolls_remaining
                .saturating_sub(1);
            self.workspace.restore_workspace_scrolls_pending =
                self.workspace.restore_workspace_scrolls_remaining > 0;
            if !self.workspace.restore_workspace_scrolls_pending {
                self.workspace
                    .restore_workspace_scroll_locks_release_pending = true;
            }
        }
        let restore_workspace_scrolls_due = self.workspace.restore_workspace_scrolls_pending;
        let bottom_anchor_due = if self.workspace.pending_workspace_bottom_anchor_ticks > 0 {
            self.workspace.pending_workspace_bottom_anchor_ticks = self
                .workspace
                .pending_workspace_bottom_anchor_ticks
                .saturating_sub(1);
            self.workspace.pending_workspace_bottom_anchor_ticks == 0
        } else {
            false
        };
        if restore_workspace_scrolls_due || bottom_anchor_due {
            self.restore_visible_workspace_scrolls()
        } else {
            Task::none()
        }
    }

    pub(super) fn update_persistence_deadline_reached(&mut self) -> Task<Message> {
        let now = current_epoch_ms();
        self.app.flush_due_ui_preferences(now);
        self.app.flush_due_directory_persistence();
        Task::none()
    }

    pub(super) fn update_monitoring_tick(&mut self) -> Task<Message> {
        self.monitoring.sample_epoch_ms = current_epoch_ms();
        if self.app.workspace.active_section == WorkspaceSection::Monitoring {
            self.monitoring.process_usage = process_resource_usage();
        }
        self.sample_runtime_interface_stats()
    }

    pub(super) fn update_lxmf_reconcile_deadline_reached(&mut self) -> Task<Message> {
        let now = current_epoch_ms();
        self.app.reconcile_due_lxmf_direct_timeouts(now);
        self.app.reconcile_due_lxmf_propagation_timeouts(now);
        self.app.reconcile_due_lxmf_expiry(now);
        Task::none()
    }

    pub(super) fn update_browser_partial_deadline_reached(&mut self) -> Task<Message> {
        self.app.refresh_due_browser_partials(current_epoch_ms());
        Task::none()
    }

    #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
    pub(super) fn update_omenchat_maintenance_deadline_reached(&mut self) -> Task<Message> {
        let now = current_epoch_ms();
        Task::batch([
            self.sync_due_omenchat_recent_history(now),
            self.maintain_omenchat_live_links(now),
            self.reconnect_restored_omenchat_sessions_if_ready(),
        ])
    }

    #[cfg(not(any(feature = "chat-client-rns", feature = "chat-client-rns-clean")))]
    pub(super) fn update_omenchat_maintenance_deadline_reached(&mut self) -> Task<Message> {
        Task::none()
    }
}

#[cfg(test)]
mod tests {
    use super::{bounded_shutdown, ShutdownOutcome};
    use std::time::Duration;

    #[tokio::test]
    async fn bounded_shutdown_reports_success_failure_and_timeout() {
        assert!(matches!(
            bounded_shutdown(async { Ok::<_, &str>(()) }, Duration::from_millis(10)).await,
            ShutdownOutcome::Stopped
        ));
        assert!(matches!(
            bounded_shutdown(
                async { Err::<(), _>("stop failed") },
                Duration::from_millis(10)
            )
            .await,
            ShutdownOutcome::Failed(error) if error == "stop failed"
        ));
        assert!(matches!(
            bounded_shutdown(
                std::future::pending::<Result<(), &str>>(),
                Duration::from_millis(10)
            )
            .await,
            ShutdownOutcome::TimedOut
        ));
    }
}
