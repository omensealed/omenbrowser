use iced::Task;

use super::{
    DesktopApp, Message, OMENCHAT_HEARTBEAT_TIMEOUT_MS, OMENCHAT_MAX_HEARTBEAT_IDLE_MS,
    OMENCHAT_MIN_HEARTBEAT_IDLE_MS,
};

impl DesktopApp {
    pub(in crate::desktop) fn omenchat_maintenance_deadline(&self) -> Option<(u64, u64)> {
        let heartbeat_deadline = self
            .omenchat
            .omenchat_live_transports
            .values()
            .flat_map(|transport| {
                let last_activity = transport
                    .last_rx_epoch_ms
                    .max(transport.last_tx_epoch_ms)
                    .max(transport.last_ping_epoch_ms);
                let heartbeat_idle_ms = transport.heartbeat_idle_ms.clamp(
                    OMENCHAT_MIN_HEARTBEAT_IDLE_MS,
                    OMENCHAT_MAX_HEARTBEAT_IDLE_MS,
                );
                let heartbeat_timeout_ms = OMENCHAT_HEARTBEAT_TIMEOUT_MS.max(
                    heartbeat_idle_ms
                        .saturating_mul(3)
                        .min(OMENCHAT_MAX_HEARTBEAT_IDLE_MS),
                );
                let idle = last_activity.saturating_add(heartbeat_idle_ms);
                let timeout = transport.awaiting_pong.then_some(
                    transport
                        .last_ping_epoch_ms
                        .saturating_add(heartbeat_timeout_ms),
                );
                std::iter::once(idle).chain(timeout)
            })
            .min();
        let recent_sync_deadline = self
            .omenchat
            .omenchat_recent_sync_due_after
            .values()
            .copied()
            .min();
        let reconnect_deadline = self
            .app
            .runtime_status
            .connected
            .then(|| {
                self.omenchat
                    .chat_client
                    .sessions()
                    .iter()
                    .filter(|session| {
                        session.server.destination != "mockchatdestination"
                            && session.server.destination.len() >= 32
                            && !self
                                .omenchat
                                .omenchat_live_transports
                                .contains_key(&session.session_id)
                            && !self
                                .omenchat
                                .omenchat_live_opening
                                .contains(&session.session_id)
                            && self
                                .omenchat
                                .omenchat_live_retry_count
                                .get(&session.session_id)
                                .copied()
                                .unwrap_or(0)
                                < 5
                    })
                    .map(|session| {
                        self.omenchat
                            .omenchat_live_retry_after
                            .get(&session.session_id)
                            .copied()
                            .unwrap_or(0)
                    })
                    .min()
            })
            .flatten();

        heartbeat_deadline
            .into_iter()
            .chain(recent_sync_deadline)
            .chain(reconnect_deadline)
            .min()
            .map(|deadline| (self.app.internal_event_wake().id(), deadline))
    }

    pub(in crate::desktop) fn maintain_omenchat_live_links(&mut self, now: u64) -> Task<Message> {
        let mut stale_sessions = Vec::new();
        let mut outbound = Vec::new();
        let session_ids = self
            .omenchat
            .omenchat_live_transports
            .keys()
            .copied()
            .collect::<Vec<_>>();

        for session_id in session_ids {
            let Some(transport) = self.omenchat.omenchat_live_transports.get_mut(&session_id)
            else {
                continue;
            };
            let last_activity = transport
                .last_rx_epoch_ms
                .max(transport.last_tx_epoch_ms)
                .max(transport.last_ping_epoch_ms);
            let heartbeat_idle_ms = transport.heartbeat_idle_ms.clamp(
                OMENCHAT_MIN_HEARTBEAT_IDLE_MS,
                OMENCHAT_MAX_HEARTBEAT_IDLE_MS,
            );
            let heartbeat_timeout_ms = OMENCHAT_HEARTBEAT_TIMEOUT_MS.max(
                heartbeat_idle_ms
                    .saturating_mul(3)
                    .min(OMENCHAT_MAX_HEARTBEAT_IDLE_MS),
            );
            if transport.awaiting_pong
                && now.saturating_sub(transport.last_ping_epoch_ms) >= heartbeat_timeout_ms
            {
                stale_sessions.push(session_id);
                continue;
            }
            if now.saturating_sub(last_activity) < heartbeat_idle_ms {
                continue;
            }
            if let Some(event) = crate::chat::live::ping_live_session(
                &mut self.omenchat.omenchat_live_state,
                transport,
                session_id,
            ) {
                self.apply_omenchat_client_events_status(&[event]);
                stale_sessions.push(session_id);
            } else {
                transport.last_ping_epoch_ms = now;
                transport.awaiting_pong = true;
                let link_id = transport.link_id;
                let frames = transport.take_outgoing_frames();
                if !frames.is_empty() {
                    outbound.push((link_id, frames));
                }
            }
        }

        for session_id in stale_sessions {
            self.disconnect_omenchat_session(
                session_id,
                "OMENchat heartbeat timed out; use Reconnect to open a fresh link",
            );
            self.omenchat
                .omenchat_live_retry_after
                .insert(session_id, now.saturating_add(2_000));
        }
        for (link_id, frames) in outbound {
            self.send_omenchat_outgoing_frames(link_id, frames);
        }

        Task::none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::App;
    use crate::desktop::DesktopOmenChatTransport;

    fn desktop_with_temp_root(name: &str) -> DesktopApp {
        let root = std::env::temp_dir().join(format!("{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let paths = crate::config::AppPaths::from_root(root);
        paths.ensure().expect("paths");
        DesktopApp::new(App::new(crate::config::AppConfig {
            paths,
            settings: crate::storage::settings::AppSettings::default(),
        }))
    }

    #[test]
    fn omenchat_maintenance_deadline_tracks_nearest_active_work() {
        let mut desktop = desktop_with_temp_root("omenbrowser-rs-omenchat-maintenance-deadline");
        assert!(desktop.omenchat_maintenance_deadline().is_none());

        desktop
            .omenchat
            .omenchat_live_transports
            .insert(41, DesktopOmenChatTransport::new([0x41; 16], 1_000));
        let (app_id, heartbeat_deadline) = desktop
            .omenchat_maintenance_deadline()
            .expect("heartbeat deadline");
        assert_eq!(heartbeat_deadline, 1_000 + OMENCHAT_MIN_HEARTBEAT_IDLE_MS);

        desktop
            .omenchat
            .omenchat_recent_sync_due_after
            .insert(41, 1_250);
        assert_eq!(
            desktop.omenchat_maintenance_deadline(),
            Some((app_id, 1_250))
        );
        desktop.omenchat.omenchat_recent_sync_due_after.clear();
        desktop.omenchat.omenchat_live_transports.clear();
        assert!(desktop.omenchat_maintenance_deadline().is_none());
    }
}
