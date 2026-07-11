use crate::chat::{ChatSessionId, OmenChatDescriptor};

use super::{compact_elapsed_ms, DesktopApp};

impl DesktopApp {
    pub(in crate::desktop) fn disconnect_omenchat_session(
        &mut self,
        session_id: ChatSessionId,
        status: &str,
    ) {
        let Some(transport) = self.omenchat.omenchat_live_transports.remove(&session_id) else {
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

    pub(in crate::desktop) fn clear_omenchat_reconnect_state(&mut self, session_id: ChatSessionId) {
        self.omenchat.omenchat_live_opening.remove(&session_id);
        self.omenchat.omenchat_live_retry_after.remove(&session_id);
        self.omenchat.omenchat_live_retry_count.remove(&session_id);
        self.omenchat
            .omenchat_live_reconnect_generation
            .remove(&session_id);
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
            return format!("reconnect: queued in {wait} (attempt {attempts}/5)");
        }
        "reconnect: manual".into()
    }
}
