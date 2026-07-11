use iced::Task;

use super::{
    DesktopApp, Message, OMENCHAT_HEARTBEAT_TIMEOUT_MS, OMENCHAT_MAX_HEARTBEAT_IDLE_MS,
    OMENCHAT_MIN_HEARTBEAT_IDLE_MS,
};

impl DesktopApp {
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
