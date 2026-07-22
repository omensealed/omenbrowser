use std::time::{SystemTime, UNIX_EPOCH};

use iced::Task;

use crate::chat::mutation_intent_worker::await_intent_worker_reply;
use crate::chat::mutation_intents::{
    IntentTransition, OutboundMutationState, OwnedPrepareOutboundMutation,
};
use crate::chat::protocol::{ChatOp, FrameBody, MutationId};
use crate::chat::{ChatClientEvent, ChatSessionId};

use super::omenchat_desktop_state::OmenChatMutationRecoveryState;
use super::{
    is_omenchat_local_echo_event, DesktopApp, Message, OmenChatDraftCommandResult,
    OmenChatMutationCompletionMessage,
};

const DURABLE_MUTATION_INTENT_LIFETIME_SECONDS: i64 = 7 * 24 * 60 * 60;

impl DesktopApp {
    pub(in crate::desktop) fn recover_omenchat_mutation_intents_if_pending(
        &mut self,
    ) -> Task<Message> {
        if self.omenchat.omenchat_mutation_recovery_state != OmenChatMutationRecoveryState::Pending
        {
            return Task::none();
        }
        let Some(worker) = self.omenchat.omenchat_mutation_intent_worker.as_ref() else {
            self.omenchat.omenchat_mutation_recovery_state =
                OmenChatMutationRecoveryState::Unavailable;
            return Task::none();
        };
        let reply = match worker.try_recover() {
            Ok(reply) => reply,
            Err(error) => {
                self.omenchat.omenchat_mutation_recovery_state =
                    OmenChatMutationRecoveryState::Failed;
                self.app.status.task =
                    format!("OMENchat durable mutation recovery was not admitted: {error}");
                return Task::none();
            }
        };
        self.omenchat.omenchat_mutation_recovery_state = OmenChatMutationRecoveryState::InFlight;
        Task::perform(await_intent_worker_reply(reply), |result| {
            Message::OmenChatMutationCompletion(Box::new(
                OmenChatMutationCompletionMessage::Recovered {
                    result: result.map_err(|error| error.to_string()),
                },
            ))
        })
    }

    pub(in crate::desktop) fn send_omenchat_draft_with_durable_intent(
        &mut self,
        session_id: ChatSessionId,
    ) -> Task<Message> {
        let durable_negotiated = self
            .omenchat
            .omenchat_live_state
            .durable_mutations_negotiated(session_id);
        if !durable_negotiated {
            self.send_omenchat_draft(session_id);
            return Task::none();
        }
        if self.omenchat.omenchat_mutation_intent_worker.is_none()
            || self.omenchat.omenchat_authenticated_identity_hash.is_none()
        {
            self.set_omenchat_session_status(
                session_id,
                "durable mutation persistence is unavailable; the draft was not sent via the legacy path"
                    .into(),
            );
            return Task::none();
        }

        let draft = self
            .omenchat
            .chat_drafts
            .get(&session_id)
            .map(|draft| draft.trim().to_owned())
            .unwrap_or_default();
        if draft.is_empty() {
            return Task::none();
        }
        match self.handle_omenchat_draft_command(session_id, &draft) {
            OmenChatDraftCommandResult::NotCommand => {}
            OmenChatDraftCommandResult::HandledClear => {
                self.omenchat.chat_drafts.insert(session_id, String::new());
                return Task::none();
            }
            OmenChatDraftCommandResult::HandledKeep => return Task::none(),
        }

        let Some(client_instance_id) = self.omenchat.omenchat_live_state.client_instance_id()
        else {
            self.set_omenchat_session_status(
                session_id,
                "durable mutation capability is missing its persistent client instance".into(),
            );
            return Task::none();
        };
        let Some(authenticated_identity_hash) =
            self.omenchat.omenchat_authenticated_identity_hash.clone()
        else {
            self.set_omenchat_session_status(
                session_id,
                "durable mutation capability is missing its authenticated identity".into(),
            );
            return Task::none();
        };
        let Some((server_destination, room_id)) = self
            .omenchat
            .chat_client
            .session(session_id)
            .map(|session| {
                (
                    session.server.destination.clone(),
                    session.active_room.room_id,
                )
            })
        else {
            self.set_omenchat_session_status(session_id, "OMENchat session is unavailable".into());
            return Task::none();
        };
        let Some(worker) = self.omenchat.omenchat_mutation_intent_worker.as_ref() else {
            return Task::none();
        };
        let created_at = current_unix_seconds();
        let request = OwnedPrepareOutboundMutation {
            server_destination,
            authenticated_identity_hash,
            client_instance_id,
            op: ChatOp::RoomMessage,
            room_id: Some(room_id),
            body: FrameBody::Text(draft),
            created_at,
            expires_at: created_at.saturating_add(DURABLE_MUTATION_INTENT_LIFETIME_SECONDS),
            correlation_id: None,
        };
        let reply = match worker.try_prepare(request) {
            Ok(reply) => reply,
            Err(error) => {
                self.set_omenchat_session_status(
                    session_id,
                    format!("durable mutation was not admitted for persistence: {error}"),
                );
                return Task::none();
            }
        };
        self.set_omenchat_session_status(
            session_id,
            "persisting durable mutation before transmission".into(),
        );
        Task::perform(await_intent_worker_reply(reply), move |result| {
            Message::OmenChatMutationCompletion(Box::new(
                OmenChatMutationCompletionMessage::Prepared {
                    session_id,
                    result: result.map_err(|error| error.to_string()),
                },
            ))
        })
    }

    pub(in crate::desktop) fn update_omenchat_mutation_completion(
        &mut self,
        completion: OmenChatMutationCompletionMessage,
    ) -> Task<Message> {
        match completion {
            OmenChatMutationCompletionMessage::Recovered { result } => {
                self.finish_omenchat_mutation_recovery(result);
                Task::none()
            }
            OmenChatMutationCompletionMessage::Prepared { session_id, result } => {
                self.mark_prepared_omenchat_mutation_uncertain(session_id, result)
            }
            OmenChatMutationCompletionMessage::MarkedUncertain { session_id, result } => {
                self.send_persisted_uncertain_omenchat_mutation(session_id, result)
            }
            OmenChatMutationCompletionMessage::Acknowledged {
                session_id,
                mutation_id,
                result,
            } => {
                self.finish_omenchat_mutation_acknowledgement(session_id, mutation_id, result);
                Task::none()
            }
        }
    }

    fn finish_omenchat_mutation_recovery(
        &mut self,
        result: Result<Vec<crate::chat::mutation_intents::OutboundMutationIntent>, String>,
    ) {
        let intents = match result {
            Ok(intents) => intents,
            Err(error) => {
                self.omenchat.omenchat_mutation_recovery_state =
                    OmenChatMutationRecoveryState::Failed;
                self.app.status.task =
                    format!("OMENchat durable mutation recovery failed: {error}");
                return;
            }
        };
        let Some(identity_hash) = self
            .omenchat
            .omenchat_authenticated_identity_hash
            .as_deref()
        else {
            self.omenchat.omenchat_mutation_recovery_state = OmenChatMutationRecoveryState::Failed;
            self.app.status.task =
                "OMENchat durable mutation recovery lacks an authenticated identity".into();
            return;
        };
        let Some(client_instance_id) = self.omenchat.omenchat_live_state.client_instance_id()
        else {
            self.omenchat.omenchat_mutation_recovery_state = OmenChatMutationRecoveryState::Failed;
            self.app.status.task =
                "OMENchat durable mutation recovery lacks a persistent client instance".into();
            return;
        };
        let mut current = Vec::new();
        let mut other_count = 0usize;
        for intent in intents {
            if intent.authenticated_identity_hash == identity_hash
                && intent.client_instance_id == client_instance_id
            {
                current.push(intent);
            } else {
                other_count = other_count.saturating_add(1);
            }
        }
        let now = current_unix_seconds();
        let prepared = current
            .iter()
            .filter(|intent| {
                intent.state == OutboundMutationState::Prepared && intent.expires_at > now
            })
            .count();
        let uncertain = current
            .iter()
            .filter(|intent| {
                intent.state == OutboundMutationState::SentUncertain && intent.expires_at > now
            })
            .count();
        let expired = current
            .iter()
            .filter(|intent| intent.expires_at <= now)
            .count();
        self.omenchat.omenchat_recovered_mutation_intents = current;
        self.omenchat.omenchat_other_identity_mutation_intents = other_count;
        self.omenchat.omenchat_mutation_recovery_state = OmenChatMutationRecoveryState::Loaded;
        let total = prepared.saturating_add(uncertain).saturating_add(expired);
        self.app.status.task = if total == 0 && other_count == 0 {
            "OMENchat durable mutation recovery found no unresolved intents".into()
        } else {
            format!(
                "OMENchat recovered {prepared} prepared, {uncertain} uncertain, and {expired} expired intent(s); {} other-identity intent(s) retained; nothing was resent",
                other_count
            )
        };
    }

    fn mark_prepared_omenchat_mutation_uncertain(
        &mut self,
        session_id: ChatSessionId,
        result: Result<crate::chat::mutation_intents::OutboundMutationIntent, String>,
    ) -> Task<Message> {
        let intent = match result {
            Ok(intent) => intent,
            Err(error) => {
                self.set_omenchat_session_status(
                    session_id,
                    format!("durable mutation persistence failed: {error}"),
                );
                return Task::none();
            }
        };
        if !self
            .omenchat
            .omenchat_live_state
            .durable_mutations_negotiated(session_id)
            || !self
                .omenchat
                .omenchat_live_transports
                .contains_key(&session_id)
        {
            self.set_omenchat_session_status(
                session_id,
                "durable mutation is safely prepared but the negotiated session is unavailable; it was not sent"
                    .into(),
            );
            return Task::none();
        }
        let Some(worker) = self.omenchat.omenchat_mutation_intent_worker.as_ref() else {
            self.set_omenchat_session_status(
                session_id,
                "durable mutation is safely prepared but its persistence worker is unavailable"
                    .into(),
            );
            return Task::none();
        };
        let reply = match worker.try_transition(
            intent.mutation_id,
            OutboundMutationState::Prepared,
            OutboundMutationState::SentUncertain,
        ) {
            Ok(reply) => reply,
            Err(error) => {
                self.set_omenchat_session_status(
                    session_id,
                    format!("durable mutation remains prepared because transition admission failed: {error}"),
                );
                return Task::none();
            }
        };
        Task::perform(await_intent_worker_reply(reply), move |result| {
            Message::OmenChatMutationCompletion(Box::new(
                OmenChatMutationCompletionMessage::MarkedUncertain {
                    session_id,
                    result: result.map_err(|error| error.to_string()),
                },
            ))
        })
    }

    fn send_persisted_uncertain_omenchat_mutation(
        &mut self,
        session_id: ChatSessionId,
        result: Result<IntentTransition, String>,
    ) -> Task<Message> {
        let intent = match result {
            Ok(IntentTransition::Updated(intent))
                if intent.state == OutboundMutationState::SentUncertain =>
            {
                intent
            }
            Ok(IntentTransition::Updated(_)) => {
                self.set_omenchat_session_status(
                    session_id,
                    "durable mutation reached an unexpected persistence state; it was not sent"
                        .into(),
                );
                return Task::none();
            }
            Ok(IntentTransition::Missing) => {
                self.set_omenchat_session_status(
                    session_id,
                    "durable mutation disappeared before transmission; it was not sent".into(),
                );
                return Task::none();
            }
            Ok(IntentTransition::StateMismatch { current }) => {
                self.set_omenchat_session_status(
                    session_id,
                    format!(
                        "durable mutation persistence state changed to {current:?}; it was not sent"
                    ),
                );
                return Task::none();
            }
            Err(error) => {
                self.set_omenchat_session_status(
                    session_id,
                    format!("durable mutation transition failed; it was not sent: {error}"),
                );
                return Task::none();
            }
        };
        let Some(transport) = self.omenchat.omenchat_live_transports.get_mut(&session_id) else {
            self.set_omenchat_session_status(
                session_id,
                "durable mutation is uncertain but the live session closed before transmission; it was not retried"
                    .into(),
            );
            return Task::none();
        };
        let (link_id, events, outgoing, resources) = {
            let link_id = transport.link_id;
            let events = crate::chat::live::send_uncertain_durable_room_text(
                &mut self.omenchat.chat_client,
                &mut self.omenchat.omenchat_live_state,
                transport,
                session_id,
                &intent,
            );
            let outgoing = transport.take_outgoing_frames();
            let resources = transport.take_outgoing_resources();
            (link_id, events, outgoing, resources)
        };
        self.apply_omenchat_client_events_status(&events);
        self.send_omenchat_outgoing_frames(link_id, outgoing);
        self.send_omenchat_outgoing_resources(link_id, resources);
        let failed = events
            .iter()
            .any(|event| matches!(event, ChatClientEvent::Error { .. }));
        if !failed {
            self.omenchat.chat_drafts.insert(session_id, String::new());
        }
        if events.iter().any(|event| {
            matches!(event, ChatClientEvent::EventAppended { event, .. }
                if !is_omenchat_local_echo_event(event))
        }) {
            self.persist_omenchat_session(session_id);
        }
        let tasks = self.omenchat_mutation_acknowledgement_tasks(&events);
        if tasks.is_empty() {
            Task::none()
        } else {
            Task::batch(tasks)
        }
    }

    pub(in crate::desktop) fn omenchat_mutation_acknowledgement_tasks(
        &mut self,
        events: &[ChatClientEvent],
    ) -> Vec<Task<Message>> {
        let acknowledgements = events.iter().filter_map(|event| match event {
            ChatClientEvent::DurableMutationAcknowledged {
                session_id,
                mutation_id,
            } => Some((*session_id, *mutation_id)),
            _ => None,
        });
        let Some(worker) = self.omenchat.omenchat_mutation_intent_worker.as_ref() else {
            return Vec::new();
        };
        acknowledgements
            .filter_map(|(session_id, mutation_id)| {
                let reply = match worker.try_transition(
                    mutation_id,
                    OutboundMutationState::SentUncertain,
                    OutboundMutationState::Acknowledged,
                ) {
                    Ok(reply) => reply,
                    Err(error) => {
                        tracing::warn!(
                            session_id,
                            ?mutation_id,
                            %error,
                            "durable OMENchat acknowledgement was not admitted for persistence"
                        );
                        return None;
                    }
                };
                Some(Task::perform(
                    await_intent_worker_reply(reply),
                    move |result| {
                        Message::OmenChatMutationCompletion(Box::new(
                            OmenChatMutationCompletionMessage::Acknowledged {
                                session_id,
                                mutation_id,
                                result: result.map_err(|error| error.to_string()),
                            },
                        ))
                    },
                ))
            })
            .collect()
    }

    fn finish_omenchat_mutation_acknowledgement(
        &mut self,
        session_id: ChatSessionId,
        mutation_id: MutationId,
        result: Result<IntentTransition, String>,
    ) {
        match result {
            Ok(IntentTransition::Updated(intent))
                if intent.state == OutboundMutationState::Acknowledged =>
            {
                tracing::debug!(
                    session_id,
                    ?mutation_id,
                    "persisted durable OMENchat acknowledgement"
                );
            }
            Ok(IntentTransition::StateMismatch {
                current: OutboundMutationState::Acknowledged,
            }) => {}
            Ok(other) => tracing::warn!(
                session_id,
                ?mutation_id,
                transition = ?other,
                "durable OMENchat acknowledgement did not reach its terminal state"
            ),
            Err(error) => tracing::warn!(
                session_id,
                ?mutation_id,
                %error,
                "failed to persist durable OMENchat acknowledgement"
            ),
        }
    }
}

fn current_unix_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|duration| i64::try_from(duration.as_secs()).ok())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{current_epoch_ms, App};
    use crate::chat::codec::encode_frame;
    use crate::chat::protocol::{
        with_session_accept_negotiation, ClientInstanceId, Frame, FrameValue,
        SessionAcceptNegotiation, DURABLE_MUTATION_CAPABILITY, PROTOCOL_NAME,
    };
    use crate::chat::{ChatClientRequest, OmenChatDescriptor};
    use crate::desktop::DesktopOmenChatTransport;

    #[tokio::test]
    async fn negotiated_room_send_persists_before_transport_and_persists_ack() {
        let root = std::env::temp_dir().join(format!(
            "omenbrowser-desktop-durable-mutation-{}-{}",
            std::process::id(),
            current_epoch_ms()
        ));
        let paths = crate::config::AppPaths::from_root(root.clone());
        paths.ensure().expect("isolated paths");
        let mut desktop = DesktopApp::new(App::new(crate::config::AppConfig {
            paths,
            settings: crate::storage::settings::AppSettings::default(),
        }));
        let client_instance_id = ClientInstanceId::new([0x31; 16]);
        desktop
            .omenchat
            .omenchat_live_state
            .set_client_instance_id(Some(client_instance_id));
        desktop.omenchat.omenchat_authenticated_identity_hash = Some(vec![0x42; 16]);
        desktop.omenchat.omenchat_mutation_intent_worker = Some(
            crate::chat::mutation_intent_worker::MutationIntentWorker::start(
                desktop.app.paths.identity_storage_root(),
            )
            .expect("intent worker"),
        );

        let link_id = [0x53; 16];
        let mut transport = DesktopOmenChatTransport::new(link_id, current_epoch_ms());
        let opened = crate::chat::live::handle_live_request(
            &mut desktop.omenchat.chat_client,
            &mut desktop.omenchat.omenchat_live_state,
            &mut transport,
            ChatClientRequest::OpenServer(OmenChatDescriptor {
                server_destination: "00112233445566778899aabbccddeeff".into(),
                local_display_name: Some("tester".into()),
                ..OmenChatDescriptor::default()
            }),
        );
        let session_id = opened
            .iter()
            .find_map(|event| match event {
                ChatClientEvent::ServerOpened { session_id, .. } => Some(*session_id),
                _ => None,
            })
            .expect("opened session");
        let _ = transport.take_outgoing_frames();
        let accept_body = with_session_accept_negotiation(
            FrameBody::Fields(vec![
                FrameValue::String(PROTOCOL_NAME.into()),
                FrameValue::Array(Vec::new()),
            ]),
            &SessionAcceptNegotiation {
                accepted_capabilities: vec![DURABLE_MUTATION_CAPABILITY.into()],
            },
        )
        .expect("accepted negotiation");
        assert!(transport.push_incoming_frame(
            encode_frame(&Frame::new(ChatOp::SessionAccept, 1, None, accept_body))
                .expect("accept frame"),
            current_epoch_ms(),
        ));
        let _ = crate::chat::live::drain_live_events_with_state(
            &mut desktop.omenchat.chat_client,
            &mut desktop.omenchat.omenchat_live_state,
            &mut transport,
            Some(session_id),
        );
        let _ = transport.take_outgoing_frames();
        assert!(desktop
            .omenchat
            .omenchat_live_state
            .durable_mutations_negotiated(session_id));
        desktop
            .omenchat
            .omenchat_live_transports
            .insert(session_id, transport);
        desktop
            .omenchat
            .chat_drafts
            .insert(session_id, "persist me".into());

        let worker = desktop
            .omenchat
            .omenchat_mutation_intent_worker
            .take()
            .expect("worker");
        let _blocked_task = desktop.send_omenchat_draft_with_durable_intent(session_id);
        assert_eq!(
            desktop
                .omenchat
                .chat_drafts
                .get(&session_id)
                .map(String::as_str),
            Some("persist me")
        );
        assert_eq!(
            desktop
                .omenchat
                .omenchat_live_transports
                .get(&session_id)
                .map(|transport| transport.chat_frames_out),
            Some(0)
        );
        desktop.omenchat.omenchat_mutation_intent_worker = Some(worker);

        let _prepare_task = desktop.send_omenchat_draft_with_durable_intent(session_id);
        let prepared = recover_intents(&desktop);
        assert_eq!(prepared.len(), 1);
        assert_eq!(prepared[0].state, OutboundMutationState::Prepared);
        assert_eq!(
            desktop
                .omenchat
                .chat_drafts
                .get(&session_id)
                .map(String::as_str),
            Some("persist me")
        );

        let mutation_id = prepared[0].mutation_id;
        let _transition_task =
            desktop.mark_prepared_omenchat_mutation_uncertain(session_id, Ok(prepared[0].clone()));
        let uncertain = recover_intents(&desktop);
        assert_eq!(uncertain.len(), 1);
        assert_eq!(uncertain[0].state, OutboundMutationState::SentUncertain);

        let _send_task = desktop.send_persisted_uncertain_omenchat_mutation(
            session_id,
            Ok(IntentTransition::Updated(uncertain[0].clone())),
        );
        assert_eq!(
            desktop
                .omenchat
                .chat_drafts
                .get(&session_id)
                .map(String::as_str),
            Some("")
        );
        assert_eq!(
            desktop
                .omenchat
                .omenchat_live_transports
                .get(&session_id)
                .map(|transport| transport.chat_frames_out),
            Some(1)
        );

        let _ack_tasks = desktop.omenchat_mutation_acknowledgement_tasks(&[
            ChatClientEvent::DurableMutationAcknowledged {
                session_id,
                mutation_id,
            },
        ]);
        assert!(recover_intents(&desktop).is_empty());

        desktop
            .omenchat
            .omenchat_mutation_intent_worker
            .take()
            .expect("worker")
            .shutdown()
            .expect("worker shutdown");
        drop(desktop);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn restart_recovery_is_identity_scoped_visible_and_never_transmits() {
        let root = std::env::temp_dir().join(format!(
            "omenbrowser-desktop-mutation-recovery-{}-{}",
            std::process::id(),
            current_epoch_ms()
        ));
        let paths = crate::config::AppPaths::from_root(root.clone());
        paths.ensure().expect("isolated paths");
        let mut desktop = DesktopApp::new(App::new(crate::config::AppConfig {
            paths,
            settings: crate::storage::settings::AppSettings::default(),
        }));
        let client_instance_id = ClientInstanceId::new([0x61; 16]);
        desktop
            .omenchat
            .omenchat_live_state
            .set_client_instance_id(Some(client_instance_id));
        desktop.omenchat.omenchat_authenticated_identity_hash = Some(vec![0x62; 16]);
        let now = current_unix_seconds();
        let store =
            crate::chat::mutation_intents::MutationIntentStore::open_for_identity_storage_root(
                desktop.app.paths.identity_storage_root(),
            )
            .expect("intent store");
        let prepared = persist_recovery_fixture(
            &store,
            vec![0x62; 16],
            client_instance_id,
            OutboundMutationState::Prepared,
            now.saturating_sub(10),
            now.saturating_add(100),
        );
        let uncertain = persist_recovery_fixture(
            &store,
            vec![0x62; 16],
            client_instance_id,
            OutboundMutationState::SentUncertain,
            now.saturating_sub(10),
            now.saturating_add(100),
        );
        let _other_identity = persist_recovery_fixture(
            &store,
            vec![0x66; 16],
            client_instance_id,
            OutboundMutationState::SentUncertain,
            now.saturating_sub(10),
            now.saturating_add(100),
        );
        let _expired = persist_recovery_fixture(
            &store,
            vec![0x62; 16],
            client_instance_id,
            OutboundMutationState::Prepared,
            now.saturating_sub(100),
            now.saturating_sub(1),
        );
        drop(store);
        let reopened =
            crate::chat::mutation_intents::MutationIntentStore::open_for_identity_storage_root(
                desktop.app.paths.identity_storage_root(),
            )
            .expect("reopened intent store");
        let recovered = reopened.recover_nonterminal().expect("restart recovery");
        drop(reopened);

        desktop.finish_omenchat_mutation_recovery(Ok(recovered));

        assert_eq!(
            desktop.omenchat.omenchat_mutation_recovery_state,
            OmenChatMutationRecoveryState::Loaded
        );
        assert_eq!(
            desktop.omenchat.omenchat_recovered_mutation_intents.len(),
            3
        );
        assert_eq!(desktop.omenchat.omenchat_other_identity_mutation_intents, 1);
        assert!(desktop.app.status.task.contains("1 prepared"));
        assert!(desktop.app.status.task.contains("1 uncertain"));
        assert!(desktop.app.status.task.contains("1 expired"));
        assert!(desktop.app.status.task.contains("nothing was resent"));
        assert!(desktop.omenchat.omenchat_live_transports.is_empty());
        assert!(desktop
            .omenchat
            .omenchat_recovered_mutation_intents
            .iter()
            .any(|intent| intent.mutation_id == prepared.mutation_id));
        assert!(desktop
            .omenchat
            .omenchat_recovered_mutation_intents
            .iter()
            .any(|intent| intent.mutation_id == uncertain.mutation_id));
        drop(desktop);
        let _ = std::fs::remove_dir_all(root);
    }

    fn recover_intents(
        desktop: &DesktopApp,
    ) -> Vec<crate::chat::mutation_intents::OutboundMutationIntent> {
        desktop
            .omenchat
            .omenchat_mutation_intent_worker
            .as_ref()
            .expect("worker")
            .try_recover()
            .expect("recovery admitted")
            .recv()
            .expect("recovery reply")
            .expect("recovery result")
    }

    fn persist_recovery_fixture(
        store: &crate::chat::mutation_intents::MutationIntentStore,
        authenticated_identity_hash: Vec<u8>,
        client_instance_id: ClientInstanceId,
        state: OutboundMutationState,
        created_at: i64,
        expires_at: i64,
    ) -> crate::chat::mutation_intents::OutboundMutationIntent {
        let request = OwnedPrepareOutboundMutation {
            server_destination: "00112233445566778899aabbccddeeff".into(),
            authenticated_identity_hash,
            client_instance_id,
            op: ChatOp::RoomMessage,
            room_id: Some(1),
            body: FrameBody::Text("recovered body".into()),
            created_at,
            expires_at,
            correlation_id: None,
        };
        let prepared = store
            .persist_prepared(request.as_borrowed())
            .expect("persist recovery fixture");
        if state == OutboundMutationState::Prepared {
            return prepared;
        }
        match store
            .transition(prepared.mutation_id, OutboundMutationState::Prepared, state)
            .expect("transition recovery fixture")
        {
            IntentTransition::Updated(intent) => intent,
            other => panic!("unexpected recovery fixture transition: {other:?}"),
        }
    }
}
