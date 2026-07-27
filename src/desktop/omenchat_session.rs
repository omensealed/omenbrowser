use crate::chat::store::ChatStore;
use crate::chat::ChatSessionId;

use super::DesktopApp;

impl DesktopApp {
    pub(in crate::desktop) fn close_omenchat_session(&mut self, session_id: ChatSessionId) {
        self.clear_omenchat_invitation_room_for_session(session_id);
        #[cfg(feature = "desktop-qr")]
        self.clear_omenchat_invitation_qr_for_session(session_id);
        let server_id = self
            .omenchat
            .chat_client
            .session(session_id)
            .map(|session| session.server.server_id.clone());
        self.omenchat.chat_drafts.remove(&session_id);
        self.omenchat.omenchat_reply_drafts.remove(&session_id);
        self.omenchat.omenchat_selected_mentions.remove(&session_id);
        #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
        {
            self.omenchat.omenchat_revision_drafts.remove(&session_id);
            if self
                .omenchat
                .omenchat_revision_delete_confirmation
                .is_some_and(|confirmation| confirmation.session_id == session_id)
            {
                self.omenchat.omenchat_revision_delete_confirmation = None;
            }
        }
        self.omenchat.omenchat_motds.remove(&session_id);
        for cache_key in self
            .omenchat
            .cancel_media_cache_jobs_for_session(session_id)
        {
            self.omenchat.omenchat_media_cache.remove(&cache_key);
        }
        self.omenchat
            .chat_scroll_offsets
            .retain(|(stored_session_id, _), _| *stored_session_id != session_id);
        self.omenchat
            .chat_event_counts
            .retain(|(stored_session_id, _), _| *stored_session_id != session_id);
        #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
        {
            self.set_omenchat_connection_state(
                session_id,
                crate::chat::ChatConnectionState::Draining,
            );
            let _ = self
                .omenchat
                .omenchat_live_state
                .retire_session_link_state(session_id);
            self.omenchat.omenchat_live_opening.remove(&session_id);
            if let Some(cancel) = self
                .omenchat
                .omenchat_live_open_cancellations
                .remove(&session_id)
            {
                cancel.cancel();
            }
            self.omenchat.omenchat_live_retry_after.remove(&session_id);
            self.omenchat.omenchat_live_retry_count.remove(&session_id);
            self.omenchat.omenchat_live_stable_after.remove(&session_id);
            self.omenchat
                .omenchat_live_reconnect_generation
                .remove(&session_id);
            if let Some(transport) = self.omenchat.omenchat_live_transports.remove(&session_id) {
                let runtime = self.app.runtime.clone();
                let link_id = transport.link_id;
                tokio::spawn(async move {
                    let _ = runtime.close_omenchat_link(link_id).await;
                });
            }
            self.omenchat
                .omenchat_link_sessions
                .retain(|_, stored_session_id| *stored_session_id != session_id);
            self.omenchat.omenchat_connection_states.remove(&session_id);
            crate::operations::connection::remove_omenchat_connection(
                &mut self.app.operation_history,
                session_id,
            );
        }
        self.omenchat.chat_client.remove_session(session_id);
        if let (Some(store), Some(server_id)) =
            (self.omenchat.chat_store.as_mut(), server_id.as_ref())
        {
            if let Err(error) = store.delete_server(server_id) {
                tracing::warn!("failed to delete OMENchat server {server_id} from store: {error}");
                self.app.status.task =
                    format!("closed OMENchat session; cache delete failed: {error}");
                return;
            }
        }
        self.app.status.task = "closed OMENchat session".into();
    }

    pub(in crate::desktop) fn persist_omenchat_session(&mut self, session_id: ChatSessionId) {
        let Some(store) = self.omenchat.chat_store.as_mut() else {
            return;
        };
        if let Err(error) = self.omenchat.chat_client.persist_session(store, session_id) {
            tracing::warn!("failed to persist OMENchat session {session_id}: {error}");
        }
    }

    pub(in crate::desktop) fn set_omenchat_session_status(
        &mut self,
        session_id: ChatSessionId,
        status: String,
    ) {
        if let Some(session) = self.omenchat.chat_client.session_mut(session_id) {
            session.set_status(status);
        }
    }

    #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
    pub(in crate::desktop) fn set_omenchat_connection_state(
        &mut self,
        session_id: ChatSessionId,
        state: crate::chat::ChatConnectionState,
    ) {
        if let Some(destination) = self
            .omenchat
            .chat_client
            .session(session_id)
            .map(|session| session.server.destination.clone())
        {
            self.omenchat
                .omenchat_connection_states
                .insert(session_id, state);
            if let Err(error) = crate::operations::connection::record_omenchat_connection_state(
                &mut self.app.operation_history,
                session_id,
                &destination,
                state,
                i64::try_from(crate::app::current_epoch_ms()).unwrap_or(i64::MAX),
            ) {
                tracing::warn!("failed to project OMENchat connection state: {error}");
            }
        }
    }

    #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
    pub(in crate::desktop) fn omenchat_connection_state(
        &self,
        session_id: ChatSessionId,
    ) -> crate::chat::ChatConnectionState {
        self.omenchat
            .omenchat_connection_states
            .get(&session_id)
            .copied()
            .unwrap_or_default()
    }

    pub(in crate::desktop) fn clear_omenchat_active_room_unread(
        &mut self,
        session_id: ChatSessionId,
    ) {
        let Some(session) = self.omenchat.chat_client.session_mut(session_id) else {
            return;
        };
        let room_id = session.active_room.room_id;
        session.active_room.unread = 0;
        for room in &mut session.rooms {
            if room.room_id == room_id {
                room.unread = 0;
            }
        }
        self.persist_omenchat_session(session_id);
    }
}

#[cfg(all(test, feature = "chat-client"))]
#[path = "omenchat_session_tests.rs"]
mod tests;
