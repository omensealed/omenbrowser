use iced::Task;

use super::{DesktopApp, HistorySearchMessage, Message};
use crate::history_search::{search_persisted_local_history, LocalHistorySearchQuery};

fn resolve_lxmf_stored_target(
    conversations: &[crate::messaging::Conversation],
    peer_key: &str,
    message_index: usize,
    message_key: &str,
) -> Result<(usize, u64), &'static str> {
    let Some((conversation_index, conversation)) = conversations
        .iter()
        .enumerate()
        .find(|(_, conversation)| conversation.peer_hash == peer_key)
    else {
        return Err("search result is stored but its LXMF conversation is not open");
    };
    let Some(message) = conversation.thread.messages.get(message_index) else {
        return Err("search result moved; run the search again");
    };
    if crate::app::message_summary_key(message) != message_key {
        return Err("search result changed; run the search again");
    }
    Ok((conversation_index, conversation.id))
}

#[cfg(feature = "chat-client")]
fn resolve_omenchat_stored_target(
    sessions: &[crate::chat::ChatSessionView],
    server_key: &str,
    room_id: u32,
    event_id: u64,
) -> Result<u64, &'static str> {
    let Some(session) = sessions
        .iter()
        .find(|session| session.server.server_id == server_key)
    else {
        return Err("search result is stored but its OMENchat session is not open");
    };
    if !session
        .events
        .iter()
        .any(|event| event.room_id == room_id && event.event_id == event_id)
    {
        return Err("search result is outside resident OMENchat history; load older history");
    }
    Ok(session.session_id)
}

impl DesktopApp {
    pub(super) fn dispatch_history_search_message(
        &mut self,
        message: Message,
    ) -> Result<Task<Message>, Message> {
        match message {
            Message::HistorySearch(message) => match *message {
                HistorySearchMessage::QueryChanged(value) => {
                    self.history_search.update_draft(value);
                    Ok(Task::none())
                }
                HistorySearchMessage::CycleSource => {
                    self.history_search.cycle_source();
                    Ok(Task::none())
                }
                HistorySearchMessage::SubmitCurrent => {
                    let query = self.history_search.current_query();
                    Ok(self.submit_local_history_search(query))
                }
                HistorySearchMessage::Submit(query) => Ok(self.submit_local_history_search(query)),
                HistorySearchMessage::Jump(key) => Ok(self.jump_to_local_history_result(key)),
                HistorySearchMessage::Completed { generation, result } => {
                    Ok(self.complete_local_history_search(generation, result))
                }
            },
            _ => Err(message),
        }
    }

    fn jump_to_local_history_result(
        &mut self,
        key: crate::history_search::LocalHistoryResultKey,
    ) -> Task<Message> {
        use crate::history_search::LocalHistoryResultKey;

        match key {
            LocalHistoryResultKey::LxmfStored {
                peer_key,
                message_index,
                message_key,
            } => {
                let (conversation_index, conversation_id) = match resolve_lxmf_stored_target(
                    &self.app.workspace.conversations,
                    &peer_key,
                    message_index,
                    &message_key,
                ) {
                    Ok(target) => target,
                    Err(status) => {
                        self.app.status.task = status.into();
                        return Task::none();
                    }
                };
                self.app.select_conversation_tab(conversation_index);
                self.app.select_active_conversation_message(message_key);
                self.app.status.task = "opened matching LXMF message".into();
                self.update_restore_desktop_pane(super::DesktopPane::Conversation(conversation_id))
            }
            #[cfg(feature = "chat-client")]
            LocalHistoryResultKey::OmenChatStored {
                server_key,
                room_id,
                event_id,
            } => {
                let session_id = match resolve_omenchat_stored_target(
                    self.omenchat.chat_client.sessions(),
                    &server_key,
                    room_id,
                    event_id,
                ) {
                    Ok(session_id) => session_id,
                    Err(status) => {
                        self.app.status.task = status.into();
                        return Task::none();
                    }
                };
                Task::done(Message::OmenChat(super::OmenChatMessage::JumpToEvent {
                    session_id,
                    room_id,
                    event_id,
                }))
            }
            LocalHistoryResultKey::Lxmf { .. } => {
                self.app.status.task = "resident search key is not a persisted result".into();
                Task::none()
            }
            #[cfg(feature = "chat-client")]
            LocalHistoryResultKey::OmenChat { .. } => {
                self.app.status.task = "resident search key is not a persisted result".into();
                Task::none()
            }
        }
    }

    fn submit_local_history_search(&mut self, query: LocalHistorySearchQuery) -> Task<Message> {
        let Some(job) = self.history_search.submit(query) else {
            self.app.status.task = "history search queued; previous scan is finishing".into();
            return Task::none();
        };
        self.app.status.task = "searching bounded local history".into();
        self.local_history_search_task(job)
    }

    fn complete_local_history_search(
        &mut self,
        generation: u64,
        result: Result<crate::history_search::LocalHistorySearchPage, String>,
    ) -> Task<Message> {
        let Some(next) = self.history_search.complete(generation, result) else {
            if self.history_search.error.is_some() {
                self.app.status.task = "local history search failed".into();
            } else if let Some(page) = &self.history_search.result {
                self.app.status.task = format!(
                    "local history search: {} result(s), {} item(s) examined",
                    page.results.len(),
                    page.scanned_items
                );
            }
            return Task::none();
        };
        self.app.status.task = "searching latest queued local-history query".into();
        self.local_history_search_task(next)
    }

    fn local_history_search_task(
        &self,
        job: super::history_search_state::LocalHistorySearchJob,
    ) -> Task<Message> {
        let generation = job.generation;
        let query = job.query;
        let message_store = self.app.message_store.clone();
        #[cfg(feature = "chat-client")]
        let chat_path = self.omenchat.chat_store.as_ref().map(|_| {
            self.app
                .paths
                .identity_storage_root()
                .join("plugins")
                .join(crate::plugins::BUILTIN_OMENCHAT_PLUGIN_ID)
                .join("chat.sqlite")
        });
        Task::perform(
            async move {
                tokio::task::spawn_blocking(move || {
                    search_persisted_local_history(
                        &message_store,
                        #[cfg(feature = "chat-client")]
                        chat_path.as_deref(),
                        &query,
                    )
                    .map_err(|error| error.to_string())
                })
                .await
                .unwrap_or_else(|error| Err(format!("local history search task failed: {error}")))
            },
            move |result| {
                Message::HistorySearch(Box::new(HistorySearchMessage::Completed {
                    generation,
                    result,
                }))
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message(message_id: &str) -> crate::messaging::MessageSummary {
        crate::messaging::MessageSummary {
            peer_hash: "peer".into(),
            peer_label: "Peer".into(),
            title: "title".into(),
            content: "body".into(),
            timestamp: 1.0,
            transport_method: crate::messaging::TransportMethod::Direct,
            delivered: true,
            failed: false,
            incoming: true,
            unread: false,
            message_id: Some(message_id.into()),
            fields: Default::default(),
            attachments: Vec::new(),
        }
    }

    #[test]
    fn lxmf_jump_requires_the_same_resident_message() {
        let mut conversation = crate::messaging::Conversation::new(7, "peer", "Peer");
        conversation.push_message(message("message-7"));
        let conversations = vec![conversation];

        assert_eq!(
            resolve_lxmf_stored_target(&conversations, "peer", 0, "message-7"),
            Ok((0, 7))
        );
        assert_eq!(
            resolve_lxmf_stored_target(&conversations, "peer", 1, "message-7"),
            Err("search result moved; run the search again")
        );
        assert_eq!(
            resolve_lxmf_stored_target(&conversations, "peer", 0, "different"),
            Err("search result changed; run the search again")
        );
        assert_eq!(
            resolve_lxmf_stored_target(&conversations, "closed", 0, "message-7"),
            Err("search result is stored but its LXMF conversation is not open")
        );
    }

    #[cfg(feature = "chat-client")]
    #[test]
    fn omenchat_jump_requires_the_same_resident_event() {
        let session = crate::chat::ChatSessionView {
            session_id: 9,
            server: crate::chat::ChatServerSummary {
                server_id: "server".into(),
                destination: "destination".into(),
                display_name: "Server".into(),
            },
            rooms: Vec::new(),
            active_room: crate::chat::ChatRoomSummary {
                server_id: "server".into(),
                room_id: 3,
                name: "lobby".into(),
                topic: None,
                unread: 0,
                joined: true,
            },
            users: Vec::new(),
            events: vec![crate::chat::ChatEvent {
                server_id: "server".into(),
                room_id: 3,
                event_id: 11,
                actor_user_id: None,
                actor_display_name: None,
                at_unix: 1,
                kind: crate::chat::ChatEventKind::Message {
                    body: "body".into(),
                },
            }],
            status: "joined".into(),
        };
        let sessions = vec![session];

        assert_eq!(
            resolve_omenchat_stored_target(&sessions, "server", 3, 11),
            Ok(9)
        );
        assert_eq!(
            resolve_omenchat_stored_target(&sessions, "server", 3, 12),
            Err("search result is outside resident OMENchat history; load older history")
        );
        assert_eq!(
            resolve_omenchat_stored_target(&sessions, "closed", 3, 11),
            Err("search result is stored but its OMENchat session is not open")
        );
    }
}
