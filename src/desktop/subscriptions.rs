use std::time::Duration;

use iced::futures::{SinkExt, Stream};
use iced::{event, keyboard, stream, time, window, Subscription};

use crate::app::current_epoch_ms;
use crate::app::InternalEventWake;
use crate::workspace::WorkspaceSection;

use super::input::{
    map_browser_field_keyboard_event, map_command_palette_key_press, map_key_press,
    map_keyboard_modifier_event,
};
use super::{section_needs_runtime_interface_sample, Message, ShellMessage, DESKTOP_LIVE_TICK_MS};

impl super::DesktopApp {
    pub(super) fn subscription(&self) -> Subscription<Message> {
        if !self.ui.shutdown_phase.is_running() {
            return Subscription::none();
        }
        let browser_field_active = self.app.workspace.active_section == WorkspaceSection::Browser
            && self.app.active_browser_field_editor().is_some();
        let keyboard_subscription = if self.ui.command_palette_open {
            keyboard::listen().filter_map(|event| match event {
                keyboard::Event::KeyPressed { key, modifiers, .. } => {
                    map_command_palette_key_press(key, modifiers)
                }
                _ => None,
            })
        } else if browser_field_active {
            event::listen_with(map_browser_field_keyboard_event)
        } else {
            keyboard::listen().filter_map(|event| match event {
                keyboard::Event::KeyPressed { key, modifiers, .. } => map_key_press(key, modifiers),
                _ => None,
            })
        };
        let modifier_subscription = if browser_field_active || self.ui.command_palette_open {
            Subscription::none()
        } else {
            event::listen_with(map_keyboard_modifier_event)
        };
        let scroll_subscription = if self.is_workspace_scroll_restore_settling()
            || self.workspace.pending_workspace_bottom_anchor_ticks > 0
        {
            time::every(Duration::from_millis(DESKTOP_LIVE_TICK_MS))
                .map(|_| Message::Shell(ShellMessage::WorkspaceScrollTick))
        } else {
            Subscription::none()
        };
        let persistence_subscription = self
            .app
            .desktop_persistence_deadline()
            .map(|deadline| Subscription::run_with(deadline, persistence_deadline_stream))
            .unwrap_or_else(Subscription::none);
        let monitoring_subscription =
            if section_needs_runtime_interface_sample(self.app.workspace.active_section) {
                time::every(monitoring_sample_interval(
                    self.app.settings.ui.low_power_mode,
                ))
                .map(|_| Message::Shell(ShellMessage::MonitoringTick))
            } else {
                Subscription::none()
            };
        let lxmf_reconcile_subscription = Subscription::run_with(
            self.app.desktop_lxmf_reconcile_deadline(),
            lxmf_reconcile_deadline_stream,
        );
        let browser_partial_subscription = self
            .app
            .desktop_browser_partial_deadline()
            .map(|deadline| Subscription::run_with(deadline, browser_partial_deadline_stream))
            .unwrap_or_else(Subscription::none);
        #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
        let omenchat_maintenance_subscription = self
            .omenchat_maintenance_deadline()
            .map(|deadline| Subscription::run_with(deadline, omenchat_maintenance_deadline_stream))
            .unwrap_or_else(Subscription::none);
        #[cfg(not(any(feature = "chat-client-rns", feature = "chat-client-rns-clean")))]
        let omenchat_maintenance_subscription = Subscription::none();
        Subscription::batch([
            keyboard_subscription,
            modifier_subscription,
            scroll_subscription,
            persistence_subscription,
            monitoring_subscription,
            lxmf_reconcile_subscription,
            browser_partial_subscription,
            omenchat_maintenance_subscription,
            Subscription::run_with(self.app.internal_event_wake(), internal_event_stream),
            window::close_requests()
                .map(|id| Message::Shell(ShellMessage::WindowCloseRequested(id))),
        ])
    }
}

fn monitoring_sample_interval(low_power_mode: bool) -> Duration {
    Duration::from_secs(if low_power_mode { 5 } else { 1 })
}

fn persistence_deadline_stream(deadline: &(u64, u64)) -> impl Stream<Item = Message> + 'static {
    let deadline_epoch_ms = deadline.1;
    stream::channel(1, async move |mut output| {
        let delay_ms = deadline_epoch_ms.saturating_sub(current_epoch_ms());
        tokio::time::sleep(Duration::from_millis(delay_ms)).await;
        let _ = output
            .send(Message::Shell(ShellMessage::PersistenceDeadlineReached))
            .await;
    })
}

fn lxmf_reconcile_deadline_stream(deadline: &(u64, u64)) -> impl Stream<Item = Message> + 'static {
    let deadline_epoch_ms = deadline.1;
    stream::channel(1, async move |mut output| {
        let delay_ms = deadline_epoch_ms.saturating_sub(current_epoch_ms());
        tokio::time::sleep(Duration::from_millis(delay_ms)).await;
        let _ = output
            .send(Message::Shell(ShellMessage::LxmfReconcileDeadlineReached))
            .await;
    })
}

fn browser_partial_deadline_stream(deadline: &(u64, u64)) -> impl Stream<Item = Message> + 'static {
    let deadline_epoch_ms = deadline.1;
    stream::channel(1, async move |mut output| {
        let delay_ms = deadline_epoch_ms.saturating_sub(current_epoch_ms());
        tokio::time::sleep(Duration::from_millis(delay_ms)).await;
        let _ = output
            .send(Message::Shell(ShellMessage::BrowserPartialDeadlineReached))
            .await;
    })
}

fn omenchat_maintenance_deadline_stream(
    deadline: &(u64, u64),
) -> impl Stream<Item = Message> + 'static {
    let deadline_epoch_ms = deadline.1;
    stream::channel(1, async move |mut output| {
        let delay_ms = deadline_epoch_ms.saturating_sub(current_epoch_ms());
        tokio::time::sleep(Duration::from_millis(delay_ms)).await;
        let _ = output
            .send(Message::Shell(
                ShellMessage::OmenChatMaintenanceDeadlineReached,
            ))
            .await;
    })
}

fn internal_event_stream(wake: &InternalEventWake) -> impl Stream<Item = Message> + 'static {
    let mut receiver = wake.receiver();
    stream::channel(1, async move |mut output| {
        if output
            .send(Message::Shell(ShellMessage::InternalEventsReady))
            .await
            .is_err()
        {
            return;
        }
        loop {
            if receiver.changed().await.is_err() {
                break;
            }
            if output
                .send(Message::Shell(ShellMessage::InternalEventsReady))
                .await
                .is_err()
            {
                break;
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced::futures::StreamExt;

    #[test]
    fn low_power_mode_reduces_visible_monitoring_wakeups_without_disabling_samples() {
        assert_eq!(monitoring_sample_interval(false), Duration::from_secs(1));
        assert_eq!(monitoring_sample_interval(true), Duration::from_secs(5));
    }

    #[tokio::test]
    async fn persistence_deadline_stream_emits_once_at_the_requested_deadline() {
        let deadline = (7, current_epoch_ms().saturating_add(10));
        let mut events = Box::pin(persistence_deadline_stream(&deadline));
        let event = tokio::time::timeout(Duration::from_millis(100), events.next())
            .await
            .expect("deadline timeout");
        assert!(matches!(
            event,
            Some(Message::Shell(ShellMessage::PersistenceDeadlineReached))
        ));
        assert!(
            tokio::time::timeout(Duration::from_millis(10), events.next())
                .await
                .expect("stream completion timeout")
                .is_none()
        );
    }

    #[tokio::test]
    async fn lxmf_reconcile_stream_emits_once_at_the_requested_deadline() {
        let deadline = (11, current_epoch_ms().saturating_add(10));
        let mut events = Box::pin(lxmf_reconcile_deadline_stream(&deadline));
        let event = tokio::time::timeout(Duration::from_millis(100), events.next())
            .await
            .expect("LXMF deadline timeout");
        assert!(matches!(
            event,
            Some(Message::Shell(ShellMessage::LxmfReconcileDeadlineReached))
        ));
        assert!(
            tokio::time::timeout(Duration::from_millis(10), events.next())
                .await
                .expect("LXMF stream completion timeout")
                .is_none()
        );
    }

    #[tokio::test]
    async fn browser_partial_stream_emits_once_at_the_requested_deadline() {
        let deadline = (13, current_epoch_ms().saturating_add(10));
        let mut events = Box::pin(browser_partial_deadline_stream(&deadline));
        let event = tokio::time::timeout(Duration::from_millis(100), events.next())
            .await
            .expect("browser partial deadline timeout");
        assert!(matches!(
            event,
            Some(Message::Shell(ShellMessage::BrowserPartialDeadlineReached))
        ));
        assert!(
            tokio::time::timeout(Duration::from_millis(10), events.next())
                .await
                .expect("browser partial stream completion timeout")
                .is_none()
        );
    }

    #[tokio::test]
    async fn omenchat_maintenance_stream_emits_once_at_the_requested_deadline() {
        let deadline = (17, current_epoch_ms().saturating_add(10));
        let mut events = Box::pin(omenchat_maintenance_deadline_stream(&deadline));
        let event = tokio::time::timeout(Duration::from_millis(100), events.next())
            .await
            .expect("OMENchat maintenance deadline timeout");
        assert!(matches!(
            event,
            Some(Message::Shell(
                ShellMessage::OmenChatMaintenanceDeadlineReached
            ))
        ));
        assert!(
            tokio::time::timeout(Duration::from_millis(10), events.next())
                .await
                .expect("OMENchat maintenance stream completion timeout")
                .is_none()
        );
    }
}
