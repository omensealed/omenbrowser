use iced::keyboard;
use iced::{window, Task};
use std::process;

use crate::app::current_epoch_ms;
use crate::workspace::WorkspaceSection;

use super::{process_resource_usage, section_needs_runtime_interface_sample, DesktopApp, Message};

impl DesktopApp {
    pub(super) fn dispatch_shell_message(
        &mut self,
        message: Message,
    ) -> Result<Task<Message>, Message> {
        match message {
            Message::SwitchSection(section) => Ok(self.update_switch_section(section)),
            Message::KeyboardModifiersChanged(modifiers) => {
                self.update_keyboard_modifiers_changed(modifiers);
                Ok(Task::none())
            }
            Message::ToggleNavigation => {
                self.update_toggle_navigation();
                Ok(Task::none())
            }
            Message::Tick => Ok(self.update_tick()),
            Message::WindowCloseRequested(window_id) => {
                Ok(self.update_window_close_requested(window_id))
            }
            Message::WindowShutdownComplete(window_id) => {
                self.update_window_shutdown_complete(window_id)
            }
            _ => Err(message),
        }
    }

    pub(super) fn update_window_close_requested(&mut self, window_id: window::Id) -> Task<Message> {
        self.ui.shutdown_requested = true;
        self.app.flush_pending_ui_preferences();
        let runtime = self.app.runtime.clone();
        Task::perform(
            async move {
                let _ = runtime.stop_runtime().await;
                window_id
            },
            Message::WindowShutdownComplete,
        )
    }

    pub(super) fn update_window_shutdown_complete(&mut self, window_id: window::Id) -> ! {
        let _ = window_id;
        process::exit(0);
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

    pub(super) fn update_toggle_navigation(&mut self) {
        self.ui.navigation_open = !self.ui.navigation_open;
    }

    pub(super) fn update_tick(&mut self) -> Task<Message> {
        let now = current_epoch_ms();
        self.monitoring.debug_tick_count = self.monitoring.debug_tick_count.saturating_add(1);
        if self.monitoring.debug_last_tick_epoch_ms == 0 {
            self.monitoring.debug_last_tick_epoch_ms = now;
        }
        let monitoring_sample_due =
            section_needs_runtime_interface_sample(self.app.workspace.active_section)
                && now.saturating_sub(self.monitoring.sample_epoch_ms) >= 1_000;
        if monitoring_sample_due
            && self.app.workspace.active_section == WorkspaceSection::Monitoring
        {
            self.monitoring.process_usage = process_resource_usage();
        }
        let interface_stats_task = if monitoring_sample_due {
            self.monitoring.sample_epoch_ms = now;
            self.sample_runtime_interface_stats()
        } else {
            Task::none()
        };
        let partials = self.app.refresh_due_browser_partials(now);
        self.app.flush_due_ui_preferences(now);
        self.app.flush_due_directory_persistence();
        let active_conversation_readable = self.active_conversation_pane_is_visible();
        let internal = self
            .app
            .drain_internal_events_with_active_conversation_readable(active_conversation_readable);
        #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
        let omenchat_runtime = self.drain_omenchat_runtime_events();
        #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
        let omenchat_recent_sync = self.sync_due_omenchat_recent_history(now);
        #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
        let omenchat_heartbeat = self.maintain_omenchat_live_links(now);
        #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
        let omenchat_reconnect = self.reconnect_restored_omenchat_sessions_if_ready();
        let browser_tasks = self.app.drain_browser_task_results();
        let message_tasks = self.app.drain_message_task_results();
        if message_tasks > 0 {
            self.sync_conversation_body_editors();
        }
        let direct_timeouts = self.app.reconcile_due_lxmf_direct_timeouts(now);
        let propagation_timeouts = self.app.reconcile_due_lxmf_propagation_timeouts(now);
        let diagnostics = self.app.drain_diagnostics_task_results();
        if now.saturating_sub(self.monitoring.debug_last_tick_epoch_ms) >= 5_000 {
            tracing::debug!(
                target: "desktop_perf",
                ticks = self.monitoring.debug_tick_count,
                partials,
                internal,
                browser_tasks,
                message_tasks,
                direct_timeouts,
                propagation_timeouts,
                diagnostics,
                "desktop tick drain sample"
            );
            self.monitoring.debug_tick_count = 0;
            self.monitoring.debug_last_tick_epoch_ms = now;
        }
        self.remove_workspace_panes_for_missing_targets(None, None);
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
        let mut tasks = vec![
            self.snap_conversations_with_new_messages_to_bottom(),
            #[cfg(feature = "chat-client")]
            self.snap_omenchat_with_new_events_to_bottom(),
            #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
            omenchat_runtime,
            #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
            omenchat_recent_sync,
            #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
            omenchat_heartbeat,
            #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
            omenchat_reconnect,
            interface_stats_task,
        ];
        if restore_workspace_scrolls_due || bottom_anchor_due {
            tasks.push(self.restore_visible_workspace_scrolls());
        }
        Task::batch(tasks)
    }
}
