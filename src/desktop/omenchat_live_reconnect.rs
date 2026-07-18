use crate::chat::{ChatSessionId, OmenChatDescriptor};

use super::{
    compact_elapsed_ms, DesktopApp, OMENCHAT_RECONNECT_BASE_DELAY_MS,
    OMENCHAT_RECONNECT_MAX_ATTEMPTS, OMENCHAT_RECONNECT_MAX_DELAY_MS,
};

pub(in crate::desktop) fn omenchat_reconnect_delay_ms(
    session_id: ChatSessionId,
    attempt: u8,
) -> u64 {
    let exponent = attempt.saturating_sub(1).min(6) as u32;
    let base = OMENCHAT_RECONNECT_BASE_DELAY_MS
        .saturating_mul(1u64 << exponent)
        .min(OMENCHAT_RECONNECT_MAX_DELAY_MS);
    let mixed = session_id
        .wrapping_mul(0x9e37_79b9_7f4a_7c15)
        .rotate_left(u32::from(attempt % 63))
        ^ u64::from(attempt).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    let jitter_percent = 80 + mixed % 41;
    base.saturating_mul(jitter_percent) / 100
}

impl DesktopApp {
    pub(in crate::desktop) fn disconnect_omenchat_session(
        &mut self,
        session_id: ChatSessionId,
        status: &str,
    ) {
        if let Some(cancel) = self
            .omenchat
            .omenchat_live_open_cancellations
            .remove(&session_id)
        {
            cancel.cancel();
        }
        self.set_omenchat_connection_state(session_id, crate::chat::ChatConnectionState::Draining);
        self.omenchat.omenchat_live_stable_after.remove(&session_id);
        let Some(transport) = self.omenchat.omenchat_live_transports.remove(&session_id) else {
            self.set_omenchat_connection_state(
                session_id,
                crate::chat::ChatConnectionState::Disconnected,
            );
            return;
        };
        let link_id = transport.link_id;
        self.omenchat
            .omenchat_live_disconnect_count
            .entry(session_id)
            .and_modify(|count| *count = count.saturating_add(1))
            .or_insert(1);
        self.omenchat
            .omenchat_live_last_disconnect_reason
            .insert(session_id, status.to_string());
        self.remove_omenchat_link_session_mappings(session_id);
        self.set_omenchat_session_status(session_id, status.to_string());
        self.set_omenchat_connection_state(
            session_id,
            crate::chat::ChatConnectionState::Disconnected,
        );
        let runtime = self.app.runtime.clone();
        tokio::spawn(async move {
            let _ = runtime.close_omenchat_link(link_id).await;
        });
    }

    pub(in crate::desktop) fn remove_omenchat_link_session_mappings(
        &mut self,
        session_id: ChatSessionId,
    ) {
        self.omenchat
            .omenchat_link_sessions
            .retain(|_, mapped_session_id| *mapped_session_id != session_id);
    }

    pub(in crate::desktop) fn clear_omenchat_pending_reconnect(
        &mut self,
        session_id: ChatSessionId,
    ) {
        if let Some(cancel) = self
            .omenchat
            .omenchat_live_open_cancellations
            .remove(&session_id)
        {
            cancel.cancel();
        }
        self.omenchat.omenchat_live_opening.remove(&session_id);
        self.omenchat.omenchat_live_retry_after.remove(&session_id);
        self.omenchat
            .omenchat_live_reconnect_generation
            .remove(&session_id);
    }

    pub(in crate::desktop) fn clear_omenchat_reconnect_state(&mut self, session_id: ChatSessionId) {
        self.clear_omenchat_pending_reconnect(session_id);
        self.omenchat.omenchat_live_retry_count.remove(&session_id);
        self.omenchat.omenchat_live_stable_after.remove(&session_id);
    }

    pub(in crate::desktop) fn schedule_omenchat_reconnect(
        &mut self,
        session_id: ChatSessionId,
        now: u64,
    ) -> (u8, Option<u64>) {
        self.omenchat.omenchat_live_stable_after.remove(&session_id);
        let prior_attempts = self
            .omenchat
            .omenchat_live_retry_count
            .get(&session_id)
            .copied()
            .unwrap_or_default();
        if prior_attempts >= OMENCHAT_RECONNECT_MAX_ATTEMPTS {
            self.omenchat.omenchat_live_retry_after.remove(&session_id);
            return (prior_attempts, None);
        }
        let attempt = prior_attempts.saturating_add(1);
        self.omenchat
            .omenchat_live_retry_count
            .insert(session_id, attempt);
        let delay = omenchat_reconnect_delay_ms(session_id, attempt);
        let due = now.saturating_add(delay);
        self.omenchat
            .omenchat_live_retry_after
            .insert(session_id, due);
        (attempt, Some(due))
    }

    pub(in crate::desktop) fn omenchat_descriptor_for_session(
        &self,
        session_id: ChatSessionId,
    ) -> Option<OmenChatDescriptor> {
        let session = self.omenchat.chat_client.session(session_id)?;
        Some(OmenChatDescriptor {
            server_destination: session.server.destination.clone(),
            display_name: Some(session.server.display_name.clone()),
            rooms_hint: vec![session.active_room.name.clone()],
            local_display_name: Some(self.local_omenchat_display_name()),
            ..OmenChatDescriptor::default()
        })
    }

    pub(in crate::desktop) fn omenchat_reconnect_state_label(
        &self,
        session_id: ChatSessionId,
        now: u64,
    ) -> String {
        if self
            .omenchat
            .omenchat_live_transports
            .contains_key(&session_id)
        {
            return if self.omenchat.omenchat_live_opening.contains(&session_id)
                || self
                    .omenchat
                    .omenchat_live_retry_after
                    .contains_key(&session_id)
                || self
                    .omenchat
                    .omenchat_live_reconnect_generation
                    .contains_key(&session_id)
            {
                "reconnect: stale state clearing".into()
            } else {
                "reconnect: idle".into()
            };
        }
        if self.omenchat.omenchat_live_opening.contains(&session_id) {
            return "reconnect: opening".into();
        }
        if let Some(due_after) = self.omenchat.omenchat_live_retry_after.get(&session_id) {
            let attempts = self
                .omenchat
                .omenchat_live_retry_count
                .get(&session_id)
                .copied()
                .unwrap_or_default();
            let wait = compact_elapsed_ms(due_after.saturating_sub(now));
            return format!(
                "reconnect: queued in {wait} (attempt {attempts}/{OMENCHAT_RECONNECT_MAX_ATTEMPTS})"
            );
        }
        "reconnect: manual".into()
    }
}
