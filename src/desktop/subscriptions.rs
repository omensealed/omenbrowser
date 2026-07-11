use std::time::Duration;

use iced::{event, keyboard, time, window, Subscription};

use crate::app::current_epoch_ms;
use crate::workspace::WorkspaceSection;

use super::input::{map_browser_field_keyboard_event, map_key_press, map_keyboard_modifier_event};
use super::{Message, DESKTOP_IDLE_TICK_MS, DESKTOP_LIVE_TICK_MS};

impl super::DesktopApp {
    pub(super) fn subscription(&self) -> Subscription<Message> {
        let browser_field_active = self.app.workspace.active_section == WorkspaceSection::Browser
            && self.app.active_browser_field_editor().is_some();
        let keyboard_subscription = if browser_field_active {
            event::listen_with(map_browser_field_keyboard_event)
        } else {
            keyboard::listen().filter_map(|event| match event {
                keyboard::Event::KeyPressed { key, modifiers, .. } => map_key_press(key, modifiers),
                _ => None,
            })
        };
        let modifier_subscription = if browser_field_active {
            Subscription::none()
        } else {
            event::listen_with(map_keyboard_modifier_event)
        };
        Subscription::batch([
            keyboard_subscription,
            modifier_subscription,
            time::every(Duration::from_millis(self.desktop_tick_ms())).map(|_| Message::Tick),
            window::close_requests().map(Message::WindowCloseRequested),
        ])
    }

    fn desktop_tick_ms(&self) -> u64 {
        if self.workspace.pending_workspace_bottom_anchor_ticks > 0 {
            return DESKTOP_LIVE_TICK_MS;
        }
        if self.app.workspace.active_section == WorkspaceSection::Browser
            && self
                .app
                .browser_partials_need_low_latency_tick(current_epoch_ms())
        {
            DESKTOP_LIVE_TICK_MS
        } else {
            DESKTOP_IDLE_TICK_MS
        }
    }
}
