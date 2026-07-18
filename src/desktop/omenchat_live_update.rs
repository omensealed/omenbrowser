use iced::Task;

use crate::chat::ChatSessionId;
use crate::chat::OmenChatDescriptor;
use crate::runtime::OmenChatLinkOpened;

use super::{delayed_omenchat_reconnect_if_disconnected_task, DesktopApp, Message};

impl DesktopApp {
    pub(super) fn update_request_omenchat_path(
        &mut self,
        session_id: ChatSessionId,
    ) -> Task<Message> {
        self.request_omenchat_path_task(session_id)
    }

    pub(super) fn update_reconnect_omenchat_session(
        &mut self,
        session_id: ChatSessionId,
    ) -> Task<Message> {
        self.reconnect_omenchat_session_task(session_id)
    }

    pub(super) fn update_reconnect_omenchat_session_if_disconnected(
        &mut self,
        session_id: ChatSessionId,
    ) -> Task<Message> {
        self.reconnect_omenchat_session_if_disconnected_task(session_id)
    }

    pub(super) fn update_omenchat_path_request_result(
        &mut self,
        session_id: ChatSessionId,
        destination: String,
        result: Result<bool, String>,
    ) -> Task<Message> {
        match result {
            Ok(true) => {
                self.app.status.task = format!("OMENchat path request queued: {destination}");
                if self
                    .omenchat
                    .omenchat_live_transports
                    .contains_key(&session_id)
                {
                    self.clear_omenchat_pending_reconnect(session_id);
                    let state = if self
                        .omenchat
                        .chat_client
                        .session(session_id)
                        .is_some_and(|session| session.active_room.joined)
                    {
                        crate::chat::ChatConnectionState::Joined
                    } else {
                        crate::chat::ChatConnectionState::Authenticating
                    };
                    self.set_omenchat_connection_state(session_id, state);
                    self.set_omenchat_session_status(
                        session_id,
                        format!("path request queued for {destination}; live link remains active"),
                    );
                } else if self.omenchat.omenchat_live_opening.contains(&session_id) {
                    self.set_omenchat_connection_state(
                        session_id,
                        crate::chat::ChatConnectionState::Reconnecting,
                    );
                    self.set_omenchat_session_status(
                        session_id,
                        format!("path request queued for {destination}; reconnect already pending"),
                    );
                } else {
                    self.set_omenchat_connection_state(
                        session_id,
                        crate::chat::ChatConnectionState::Reconnecting,
                    );
                    self.set_omenchat_session_status(
                        session_id,
                        format!(
                            "path request queued for {destination}; reconnecting after announce wait"
                        ),
                    );
                    return delayed_omenchat_reconnect_if_disconnected_task(session_id);
                }
            }
            Ok(false) => {
                self.set_omenchat_connection_state(
                    session_id,
                    crate::chat::ChatConnectionState::Failed { retryable: true },
                );
                self.set_omenchat_session_status(
                    session_id,
                    format!("path request not queued for {destination}"),
                );
                self.app.status.task = format!("OMENchat path request not queued: {destination}");
            }
            Err(error) => {
                self.set_omenchat_connection_state(
                    session_id,
                    crate::chat::ChatConnectionState::Failed { retryable: true },
                );
                self.set_omenchat_session_status(
                    session_id,
                    format!("path request failed: {error}"),
                );
                self.app.status.task = format!("OMENchat path request failed: {error}");
            }
        }
        Task::none()
    }

    pub(super) fn update_omenchat_live_open_result(
        &mut self,
        descriptor: OmenChatDescriptor,
        result: Result<OmenChatLinkOpened, String>,
    ) -> Task<Message> {
        self.handle_omenchat_live_open_result(descriptor, result)
    }

    pub(super) fn update_omenchat_live_reconnect_result(
        &mut self,
        session_id: ChatSessionId,
        generation: u64,
        descriptor: OmenChatDescriptor,
        result: Result<OmenChatLinkOpened, String>,
    ) -> Task<Message> {
        self.handle_omenchat_live_reconnect_result(session_id, generation, descriptor, result)
    }
}
