use iced::Task;

use crate::chat::{ChatClientEvent, ChatClientRequest, OmenChatDescriptor};
use crate::micron::render::HitAction;

use super::{
    apply_omenchat_link_fields, is_pending_omenchat_destination, request_session_id, DesktopApp,
    Message,
};

impl DesktopApp {
    #[cfg(feature = "chat-client")]
    pub(in crate::desktop) fn handle_omenchat_request(
        &mut self,
        request: ChatClientRequest,
    ) -> Vec<ChatClientEvent> {
        if let Some(session_id) = request_session_id(&request) {
            if self
                .omenchat
                .chat_client
                .session(session_id)
                .is_some_and(|session| is_pending_omenchat_destination(&session.server.destination))
            {
                return vec![ChatClientEvent::Error {
                    session_id: Some(session_id),
                    message: "enter an OMENchat destination and press Open before chatting".into(),
                }];
            }
        }
        #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
        if let Some(session_id) = request_session_id(&request) {
            if self
                .omenchat
                .omenchat_live_transports
                .contains_key(&session_id)
            {
                return self.handle_live_omenchat_request(request);
            }
            if self
                .omenchat
                .chat_client
                .session(session_id)
                .is_some_and(|session| {
                    session.server.destination != "mockchatdestination"
                        && session.server.destination.len() >= 32
                })
            {
                return vec![ChatClientEvent::Error {
                    session_id: Some(session_id),
                    message: "OMENchat is disconnected; use Reconnect before sending".into(),
                }];
            }
        }
        #[cfg(feature = "mock-runtime")]
        {
            crate::chat::mock::handle_mock_request(&mut self.omenchat.chat_client, request)
        }
        #[cfg(not(feature = "mock-runtime"))]
        {
            vec![ChatClientEvent::Error {
                session_id: request_session_id(&request),
                message: "OMENchat is disconnected; use Reconnect before sending".into(),
            }]
        }
    }

    #[cfg(feature = "chat-client")]
    pub(in crate::desktop) fn activate_focused_omenchat_link(&mut self) -> Option<Task<Message>> {
        let link = self
            .app
            .active_browser_tab()
            .focused_link
            .as_ref()
            .map(|link| crate::micron::LinkAction {
                target: link.target.clone(),
                fields: link.fields.clone(),
            })?;
        self.open_omenchat_link(link)
    }

    #[cfg(feature = "chat-client")]
    pub(in crate::desktop) fn activate_omenchat_hit_action_if_needed(
        &mut self,
        action: &HitAction,
    ) -> Option<Task<Message>> {
        let HitAction::Link(link) = action else {
            return None;
        };
        self.open_omenchat_link(link.clone())
    }

    #[cfg(feature = "chat-client")]
    pub(in crate::desktop) fn open_omenchat_link(
        &mut self,
        link: crate::micron::LinkAction,
    ) -> Option<Task<Message>> {
        let mut descriptor = OmenChatDescriptor::from_omenchat_link(&link.target)?;
        if !apply_omenchat_link_fields(&mut descriptor, &link.fields) {
            self.app.status.task = "OMENchat link metadata exceeds client limits".into();
            return None;
        }
        descriptor.local_display_name = Some(self.local_omenchat_display_name());
        if let Some(session_id) = self
            .omenchat
            .chat_client
            .sessions()
            .iter()
            .find(|session| session.server.destination == descriptor.server_destination)
            .map(|session| session.session_id)
        {
            self.omenchat.chat_drafts.entry(session_id).or_default();
            self.ensure_omenchat_bottom_entry(session_id);
            self.place_omenchat_session_preferring_active_blank(session_id);
            self.persist_workspace_panes("workspace panes");
            #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
            if self.app.runtime_status.connected
                && !self
                    .omenchat
                    .omenchat_live_transports
                    .contains_key(&session_id)
                && !self.omenchat.omenchat_live_opening.contains(&session_id)
                && descriptor.server_destination != "mockchatdestination"
                && descriptor.server_destination.len() >= 32
            {
                self.omenchat.omenchat_live_opening.insert(session_id);
                self.omenchat.omenchat_live_retry_after.remove(&session_id);
                self.omenchat.omenchat_live_retry_count.remove(&session_id);
                let generation = self.next_omenchat_reconnect_generation(session_id);
                self.set_omenchat_session_status(
                    session_id,
                    "opening live OMENchat link".to_string(),
                );
                self.set_omenchat_connection_state(
                    session_id,
                    crate::chat::ChatConnectionState::Connecting,
                );
                self.app.status.task = format!(
                    "reconnecting OMENchat session: {}",
                    descriptor.server_destination
                );
                return Some(
                    self.open_live_omenchat_reconnect_task(session_id, generation, descriptor),
                );
            }
            self.app.status.task = format!(
                "restored existing OMENchat session: {}",
                descriptor.server_destination
            );
            return Some(Task::none());
        }
        #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
        if self.app.runtime_status.connected
            && descriptor.server_destination != "mockchatdestination"
            && descriptor.server_destination.len() >= 32
        {
            self.app.status.task = format!(
                "opening live OMENchat link: {}",
                descriptor.server_destination
            );
            return Some(self.open_live_omenchat_task(descriptor));
        }
        let events = self.handle_omenchat_request(ChatClientRequest::OpenServer(descriptor));
        let Some(session_id) = events.iter().find_map(|event| match event {
            ChatClientEvent::ServerOpened { session_id, .. } => Some(*session_id),
            _ => None,
        }) else {
            self.app.status.task = "failed to open OMENchat descriptor".into();
            return Some(Task::none());
        };
        self.omenchat.chat_drafts.entry(session_id).or_default();
        self.remember_omenchat_bottom(session_id);
        self.persist_omenchat_session(session_id);
        self.place_omenchat_session_preferring_active_blank(session_id);
        self.persist_workspace_panes("workspace panes");
        self.app.status.task = "opened OMENchat descriptor".into();
        Some(Task::none())
    }

    #[cfg(feature = "chat-client")]
    pub(in crate::desktop) fn local_omenchat_display_name(&self) -> String {
        self.app
            .settings
            .active_identity_label
            .as_deref()
            .map(str::trim)
            .filter(|label| !label.is_empty())
            .unwrap_or("OMENbrowser_rs")
            .chars()
            .take(48)
            .collect()
    }

    #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
    pub(in crate::desktop) fn handle_live_omenchat_request(
        &mut self,
        request: ChatClientRequest,
    ) -> Vec<ChatClientEvent> {
        let Some(session_id) = request_session_id(&request) else {
            return vec![ChatClientEvent::Error {
                session_id: None,
                message: "OMENchat live request missing session id".into(),
            }];
        };
        let Some(transport) = self.omenchat.omenchat_live_transports.get_mut(&session_id) else {
            return vec![ChatClientEvent::Error {
                session_id: Some(session_id),
                message: "OMENchat is disconnected; use Reconnect before sending".into(),
            }];
        };
        let (link_id, events, outgoing, resources) = {
            let link_id = transport.link_id;
            let events = crate::chat::live::handle_live_request(
                &mut self.omenchat.chat_client,
                &mut self.omenchat.omenchat_live_state,
                transport,
                request,
            );
            let outgoing = transport.take_outgoing_frames();
            let resources = transport.take_outgoing_resources();
            (link_id, events, outgoing, resources)
        };
        self.apply_omenchat_client_events_status(&events);
        self.send_omenchat_outgoing_frames(link_id, outgoing);
        self.send_omenchat_outgoing_resources(link_id, resources);
        events
    }
}

#[cfg(all(
    test,
    any(feature = "chat-client-rns", feature = "chat-client-rns-clean")
))]
mod tests {
    include!("omenchat_live_transport_tests.rs");
}
