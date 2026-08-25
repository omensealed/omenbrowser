use std::time::Duration;

use iced::Task;

use crate::app::current_epoch_ms;
use crate::chat::{ChatClientEvent, ChatClientRequest, ChatSessionId, OmenChatDescriptor};

use super::{
    hex_bytes, omenchat_live_open_error_status, DesktopApp, DesktopOmenChatTransport, Message,
    OmenChatLiveOpenCompletion, OmenChatLiveReconnectCompletion,
};

impl DesktopApp {
    pub(in crate::desktop) fn open_live_omenchat_task(
        &self,
        descriptor: OmenChatDescriptor,
    ) -> Task<Message> {
        let runtime = self.app.runtime.clone();
        let destination_hash = descriptor.server_destination.clone();
        Task::perform(
            async move {
                let result = runtime
                    .open_omenchat_link(
                        &destination_hash,
                        Duration::from_secs(30),
                        crate::runtime::CancellationToken::new(),
                    )
                    .await
                    .map_err(|error| error.to_string());
                (descriptor, result)
            },
            |(descriptor, result)| {
                Message::OmenChatTransportCompletion(
                    super::OmenChatTransportCompletionMessage::LiveOpen(Box::new(
                        OmenChatLiveOpenCompletion { descriptor, result },
                    )),
                )
            },
        )
    }

    pub(in crate::desktop) fn open_live_omenchat_reconnect_task(
        &mut self,
        session_id: ChatSessionId,
        generation: u64,
        descriptor: OmenChatDescriptor,
    ) -> Task<Message> {
        let runtime = self.app.runtime.clone();
        let destination_hash = descriptor.server_destination.clone();
        let cancel = crate::runtime::CancellationToken::new();
        if let Some(previous) = self
            .omenchat
            .omenchat_live_open_cancellations
            .insert(session_id, cancel.clone())
        {
            previous.cancel();
        }
        Task::perform(
            async move {
                let result = runtime
                    .open_omenchat_link(&destination_hash, Duration::from_secs(30), cancel)
                    .await
                    .map_err(|error| error.to_string());
                (session_id, generation, descriptor, result)
            },
            |(session_id, generation, descriptor, result)| {
                Message::OmenChatTransportCompletion(
                    super::OmenChatTransportCompletionMessage::LiveReconnect(Box::new(
                        OmenChatLiveReconnectCompletion {
                            session_id,
                            generation,
                            descriptor,
                            result,
                        },
                    )),
                )
            },
        )
    }

    pub(in crate::desktop) fn reconnect_restored_omenchat_sessions_if_ready(
        &mut self,
    ) -> Task<Message> {
        if !self.app.runtime_status.connected {
            return Task::none();
        }
        let mut tasks = Vec::new();
        let now = current_epoch_ms();
        let max_auto_attempts = super::OMENCHAT_RECONNECT_MAX_ATTEMPTS;
        let candidates = self
            .omenchat
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
                        .omenchat_live_retry_after
                        .get(&session.session_id)
                        .is_none_or(|retry_after| now >= *retry_after)
                    && self
                        .omenchat
                        .omenchat_live_retry_count
                        .get(&session.session_id)
                        .copied()
                        .unwrap_or(0)
                        < max_auto_attempts
            })
            .map(|session| {
                (
                    session.session_id,
                    OmenChatDescriptor {
                        server_destination: session.server.destination.clone(),
                        display_name: Some(session.server.display_name.clone()),
                        rooms_hint: vec![session.active_room.name.clone()],
                        local_display_name: Some(self.local_omenchat_display_name()),
                        ..OmenChatDescriptor::default()
                    },
                )
            })
            .collect::<Vec<_>>();

        for (session_id, descriptor) in candidates {
            self.omenchat.omenchat_live_opening.insert(session_id);
            let generation = self.next_omenchat_reconnect_generation(session_id);
            self.set_omenchat_session_status(
                session_id,
                "reconnecting live OMENchat link".to_string(),
            );
            self.set_omenchat_connection_state(
                session_id,
                crate::chat::ChatConnectionState::Reconnecting,
            );
            tasks.push(self.open_live_omenchat_reconnect_task(session_id, generation, descriptor));
        }

        if tasks.is_empty() {
            Task::none()
        } else {
            Task::batch(tasks)
        }
    }

    pub(in crate::desktop) fn handle_omenchat_live_open_result(
        &mut self,
        descriptor: OmenChatDescriptor,
        result: Result<crate::runtime::OmenChatLinkOpened, String>,
    ) -> Task<Message> {
        let opened = match result {
            Ok(opened) => opened,
            Err(error) => {
                self.clear_omenchat_invitation_room_for_destination(
                    descriptor.server_destination.as_str(),
                );
                let Some(session_id) = self.try_open_omenchat_status_session(
                    descriptor,
                    omenchat_live_open_error_status(&error),
                ) else {
                    self.app.status.task = format!(
                        "OMENchat live link failed and the session catalog is full: {error}"
                    );
                    return Task::none();
                };
                self.place_omenchat_session_preferring_active_blank(session_id);
                self.set_omenchat_connection_state(
                    session_id,
                    crate::chat::ChatConnectionState::Failed { retryable: true },
                );
                self.persist_workspace_panes("workspace panes");
                self.app.status.task = format!("OMENchat live link failed: {error}");
                return Task::none();
            }
        };
        let server_destination = descriptor.server_destination.clone();
        let mut transport = DesktopOmenChatTransport::new(opened.link_id, current_epoch_ms());
        let events = crate::chat::live::handle_live_request(
            &mut self.omenchat.chat_client,
            &mut self.omenchat.omenchat_live_state,
            &mut transport,
            ChatClientRequest::OpenServer(descriptor),
        );
        self.apply_omenchat_client_events_status(&events);
        let Some(session_id) = events.iter().find_map(|event| match event {
            ChatClientEvent::ServerOpened { session_id, .. } => Some(*session_id),
            _ => None,
        }) else {
            self.clear_omenchat_invitation_room_for_destination(&server_destination);
            self.app.status.task = "OMENchat live session failed to initialize".into();
            return Task::none();
        };
        self.set_omenchat_connection_state(
            session_id,
            crate::chat::ChatConnectionState::Authenticating,
        );
        self.send_omenchat_outgoing_frames(opened.link_id, transport.take_outgoing_frames());
        self.omenchat.chat_drafts.entry(session_id).or_default();
        self.remember_omenchat_bottom(session_id);
        self.persist_omenchat_session(session_id);
        self.place_omenchat_session_preferring_active_blank(session_id);
        let scroll_task = self.register_omenchat_live_transport(session_id, transport);
        self.persist_workspace_panes("workspace panes");
        self.app.status.task = format!("opened live OMENchat link {}", hex_bytes(&opened.link_id));
        scroll_task
    }

    pub(in crate::desktop) fn handle_omenchat_live_reconnect_result(
        &mut self,
        session_id: ChatSessionId,
        generation: u64,
        descriptor: OmenChatDescriptor,
        result: Result<crate::runtime::OmenChatLinkOpened, String>,
    ) -> Task<Message> {
        if !self.omenchat_reconnect_generation_is_current(session_id, generation) {
            if let Ok(opened) = result {
                let runtime = self.app.runtime.clone();
                tokio::spawn(async move {
                    let _ = runtime.close_omenchat_link(opened.link_id).await;
                });
            }
            return Task::none();
        }
        self.omenchat.omenchat_live_opening.remove(&session_id);
        self.omenchat
            .omenchat_live_open_cancellations
            .remove(&session_id);
        let opened = match result {
            Ok(opened) => opened,
            Err(error) => {
                self.clear_omenchat_invitation_room_for_session(session_id);
                let (attempts, retry_after) =
                    self.schedule_omenchat_reconnect(session_id, current_epoch_ms());
                let status = if retry_after.is_none() {
                    format!(
                        "{}; automatic reconnect paused after {attempts} attempts, use Reconnect to try again",
                        omenchat_live_open_error_status(&error)
                    )
                } else {
                    format!(
                        "{}; automatic reconnect attempt {attempts}/{} scheduled with backoff",
                        omenchat_live_open_error_status(&error),
                        super::OMENCHAT_RECONNECT_MAX_ATTEMPTS,
                    )
                };
                self.set_omenchat_session_status(session_id, status);
                self.set_omenchat_connection_state(
                    session_id,
                    if retry_after.is_none() {
                        crate::chat::ChatConnectionState::Failed { retryable: true }
                    } else {
                        crate::chat::ChatConnectionState::Reconnecting
                    },
                );
                self.app.status.task = format!("OMENchat live reconnect failed: {error}");
                return Task::none();
            }
        };
        if !opened
            .destination_hash
            .eq_ignore_ascii_case(&descriptor.server_destination)
        {
            let runtime = self.app.runtime.clone();
            let rejected_link_id = opened.link_id;
            tokio::spawn(async move {
                let _ = runtime.close_omenchat_link(rejected_link_id).await;
            });
            self.clear_omenchat_reconnect_state(session_id);
            self.clear_omenchat_invitation_room_for_session(session_id);
            self.set_omenchat_session_status(
                session_id,
                "OMENchat reconnect returned a different destination; rejected link without changing the session"
                    .into(),
            );
            self.set_omenchat_connection_state(
                session_id,
                crate::chat::ChatConnectionState::Failed { retryable: true },
            );
            self.app.status.task = "OMENchat live reconnect destination mismatch".into();
            return Task::none();
        }
        let mut transport = DesktopOmenChatTransport::new(opened.link_id, current_epoch_ms());
        let events = crate::chat::live::reconnect_live_server(
            &mut self.omenchat.chat_client,
            &mut self.omenchat.omenchat_live_state,
            &mut transport,
            session_id,
            descriptor,
        );
        self.apply_omenchat_client_events_status(&events);
        self.set_omenchat_connection_state(
            session_id,
            if self
                .omenchat
                .chat_client
                .session(session_id)
                .is_some_and(|session| session.active_room.joined)
            {
                crate::chat::ChatConnectionState::Joined
            } else {
                crate::chat::ChatConnectionState::Authenticating
            },
        );
        self.send_omenchat_outgoing_frames(opened.link_id, transport.take_outgoing_frames());
        self.omenchat.chat_drafts.entry(session_id).or_default();
        self.remember_omenchat_bottom(session_id);
        self.persist_omenchat_session(session_id);
        self.ensure_pane_for_omenchat(session_id);
        let scroll_task = self.register_omenchat_live_transport(session_id, transport);
        self.persist_workspace_panes("workspace panes");
        self.app.status.task = format!(
            "reconnected live OMENchat link {}",
            hex_bytes(&opened.link_id)
        );
        scroll_task
    }

    pub(in crate::desktop) fn next_omenchat_reconnect_generation(
        &mut self,
        session_id: ChatSessionId,
    ) -> u64 {
        let entry = self
            .omenchat
            .omenchat_live_reconnect_generation
            .entry(session_id)
            .or_insert(0);
        *entry = entry.saturating_add(1);
        *entry
    }

    pub(in crate::desktop) fn omenchat_reconnect_generation_is_current(
        &self,
        session_id: ChatSessionId,
        generation: u64,
    ) -> bool {
        self.omenchat
            .omenchat_live_reconnect_generation
            .get(&session_id)
            .copied()
            .unwrap_or(0)
            == generation
    }
}

#[cfg(all(
    test,
    any(feature = "chat-client-rns", feature = "chat-client-rns-clean")
))]
mod tests {
    use super::*;
    use crate::app::App;

    const FIXTURE_CHAT_SERVER_HASH: &str = "00112233445566778899aabbccddeeff";

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

    fn test_descriptor() -> OmenChatDescriptor {
        OmenChatDescriptor {
            server_destination: FIXTURE_CHAT_SERVER_HASH.into(),
            display_name: Some("Test OMENchat".into()),
            rooms_hint: vec!["lobby".into()],
            local_display_name: Some("tester".into()),
            ..OmenChatDescriptor::default()
        }
    }

    #[test]
    fn omenchat_live_open_errors_have_user_visible_statuses() {
        assert!(omenchat_live_open_error_status("has no known identity key")
            .contains("path/key missing"));
        assert!(omenchat_live_open_error_status(
            "timed out waiting for Reticulum 0.10.0 link establishment"
        )
        .contains("Link handshake"));
        assert!(
            omenchat_live_open_error_status("native Reticulum runtime is not running")
                .contains("runtime is not running")
        );
    }

    #[test]
    fn omenchat_stale_reconnect_result_does_not_finish_current_attempt() {
        let mut desktop =
            desktop_with_temp_root("omenbrowser-rs-desktop-omenchat-reconnect-generation");
        let descriptor = test_descriptor();
        let session_id = desktop.open_omenchat_status_session(descriptor.clone(), "waiting".into());
        let stale_generation = desktop.next_omenchat_reconnect_generation(session_id);
        let current_generation = desktop.next_omenchat_reconnect_generation(session_id);
        desktop.omenchat.omenchat_live_opening.insert(session_id);
        let current_cancel = crate::runtime::CancellationToken::new();
        desktop
            .omenchat
            .omenchat_live_open_cancellations
            .insert(session_id, current_cancel.clone());

        let _ = desktop.handle_omenchat_live_reconnect_result(
            session_id,
            stale_generation,
            descriptor.clone(),
            Err("old attempt failed".into()),
        );

        assert!(desktop.omenchat.omenchat_live_opening.contains(&session_id));
        assert!(!current_cancel.is_cancelled());
        assert!(desktop
            .omenchat
            .omenchat_live_open_cancellations
            .contains_key(&session_id));
        assert_eq!(
            desktop
                .omenchat
                .omenchat_live_retry_count
                .get(&session_id)
                .copied()
                .unwrap_or(0),
            0
        );

        let _ = desktop.handle_omenchat_live_reconnect_result(
            session_id,
            current_generation,
            descriptor,
            Err("current attempt failed".into()),
        );

        assert!(!desktop.omenchat.omenchat_live_opening.contains(&session_id));
        assert!(!desktop
            .omenchat
            .omenchat_live_open_cancellations
            .contains_key(&session_id));
        assert_eq!(
            desktop
                .omenchat
                .omenchat_live_retry_count
                .get(&session_id)
                .copied()
                .unwrap_or(0),
            1
        );
    }

    #[test]
    fn newer_omenchat_reconnect_cancels_prior_open_generation() {
        let mut desktop =
            desktop_with_temp_root("omenbrowser-rs-desktop-omenchat-reconnect-cancellation");
        let descriptor = test_descriptor();
        let session_id = desktop.open_omenchat_status_session(descriptor.clone(), "waiting".into());
        let prior_cancel = crate::runtime::CancellationToken::new();
        desktop
            .omenchat
            .omenchat_live_open_cancellations
            .insert(session_id, prior_cancel.clone());

        let task = desktop.open_live_omenchat_reconnect_task(session_id, 2, descriptor);

        assert!(prior_cancel.is_cancelled());
        assert!(desktop
            .omenchat
            .omenchat_live_open_cancellations
            .get(&session_id)
            .is_some_and(|cancel| !cancel.is_cancelled()));
        drop(task);
    }

    #[tokio::test]
    async fn omenchat_reconnect_rejects_opened_link_for_wrong_destination() {
        let mut desktop =
            desktop_with_temp_root("omenbrowser-rs-desktop-omenchat-wrong-reconnect-destination");
        let descriptor = test_descriptor();
        let session_id = desktop.open_omenchat_status_session(descriptor.clone(), "waiting".into());
        let generation = desktop.next_omenchat_reconnect_generation(session_id);
        desktop.omenchat.omenchat_live_opening.insert(session_id);

        let _ = desktop.handle_omenchat_live_reconnect_result(
            session_id,
            generation,
            descriptor,
            Ok(crate::runtime::OmenChatLinkOpened {
                destination_hash: "ffeeddccbbaa99887766554433221100".into(),
                link_id: [0x44; 16],
                rtt_millis: Some(1),
            }),
        );

        tokio::task::yield_now().await;
        assert!(desktop.omenchat.omenchat_live_transports.is_empty());
        assert!(desktop.omenchat.omenchat_link_sessions.is_empty());
        assert!(desktop.omenchat.omenchat_live_opening.is_empty());
        assert!(desktop
            .omenchat
            .omenchat_live_reconnect_generation
            .is_empty());
        assert_eq!(
            desktop.omenchat_connection_state(session_id),
            crate::chat::ChatConnectionState::Failed { retryable: true }
        );
        assert!(desktop
            .omenchat
            .chat_client
            .session(session_id)
            .expect("session")
            .status
            .contains("different destination"));
    }

    #[tokio::test]
    async fn omenchat_reconnect_128_generation_soak_keeps_one_current_link() {
        const RECONNECT_CYCLES: u64 = 128;

        let mut desktop =
            desktop_with_temp_root("omenbrowser-rs-desktop-omenchat-reconnect-128-soak");
        let descriptor = test_descriptor();
        let session_id = desktop.open_omenchat_status_session(descriptor.clone(), "waiting".into());
        let initial_link_id = [0xEE; 16];
        let _ = desktop.register_omenchat_live_transport(
            session_id,
            DesktopOmenChatTransport::new(initial_link_id, current_epoch_ms()),
        );

        for cycle in 1..=RECONNECT_CYCLES {
            let old_link_id = desktop
                .omenchat
                .omenchat_live_transports
                .get(&session_id)
                .expect("one active transport")
                .link_id;
            let mut replacement_link_id = [0u8; 16];
            replacement_link_id[..8].copy_from_slice(&cycle.to_be_bytes());
            replacement_link_id[8..].copy_from_slice(&(!cycle).to_be_bytes());
            let generation = desktop.next_omenchat_reconnect_generation(session_id);
            desktop.omenchat.omenchat_live_opening.insert(session_id);

            let task = desktop.handle_omenchat_live_reconnect_result(
                session_id,
                generation,
                descriptor.clone(),
                Ok(crate::runtime::OmenChatLinkOpened {
                    destination_hash: descriptor.server_destination.clone(),
                    link_id: replacement_link_id,
                    rtt_millis: Some(1),
                }),
            );
            drop(task);

            assert_eq!(desktop.omenchat.omenchat_live_transports.len(), 1);
            assert_eq!(desktop.omenchat.omenchat_link_sessions.len(), 1);
            assert_eq!(
                desktop
                    .omenchat
                    .omenchat_live_transports
                    .get(&session_id)
                    .map(|transport| transport.link_id),
                Some(replacement_link_id)
            );
            assert_eq!(
                desktop
                    .omenchat
                    .omenchat_link_sessions
                    .get(&replacement_link_id),
                Some(&session_id)
            );
            assert!(desktop.omenchat.omenchat_live_opening.is_empty());
            assert!(desktop
                .omenchat
                .omenchat_live_reconnect_generation
                .is_empty());

            desktop
                .omenchat
                .omenchat_link_sessions
                .insert(old_link_id, session_id);
            assert!(desktop.app.enqueue_runtime_event(
                crate::runtime::RuntimeBusEvent::OmenChatLinkClosed(
                    crate::runtime::OmenChatLinkClosed {
                        link_id: old_link_id,
                        reason: Some("late old-generation close".into()),
                    },
                ),
            ));
            assert_eq!(desktop.app.drain_internal_events(), 1);
            let drain_task = desktop.drain_omenchat_runtime_events();
            drop(drain_task);

            assert_eq!(desktop.omenchat.omenchat_live_transports.len(), 1);
            assert_eq!(desktop.omenchat.omenchat_link_sessions.len(), 1);
            assert_eq!(
                desktop
                    .omenchat
                    .omenchat_live_transports
                    .get(&session_id)
                    .map(|transport| transport.link_id),
                Some(replacement_link_id)
            );
            assert!(desktop.omenchat.omenchat_live_retry_after.is_empty());
        }

        assert_eq!(
            desktop
                .omenchat
                .omenchat_live_connect_count
                .get(&session_id),
            Some(&(RECONNECT_CYCLES + 1))
        );
        assert_eq!(
            desktop
                .omenchat
                .omenchat_live_disconnect_count
                .get(&session_id)
                .copied()
                .unwrap_or_default(),
            0
        );
        assert_eq!(
            desktop
                .omenchat
                .omenchat_live_state
                .pending_upload_metrics()
                .items,
            0
        );
    }

    #[test]
    fn omenchat_reconnect_limit_projects_retryable_failed_state() {
        let mut desktop =
            desktop_with_temp_root("omenbrowser-rs-desktop-omenchat-reconnect-failed-state");
        let descriptor = test_descriptor();
        let session_id = desktop.open_omenchat_status_session(descriptor.clone(), "waiting".into());
        desktop
            .omenchat
            .omenchat_live_retry_count
            .insert(session_id, 5);
        let generation = desktop.next_omenchat_reconnect_generation(session_id);
        desktop.omenchat.omenchat_live_opening.insert(session_id);

        let _ = desktop.handle_omenchat_live_reconnect_result(
            session_id,
            generation,
            descriptor,
            Err("link unavailable".into()),
        );

        assert_eq!(
            desktop.omenchat_connection_state(session_id),
            crate::chat::ChatConnectionState::Failed { retryable: true }
        );
        assert_eq!(
            desktop
                .omenchat
                .omenchat_live_retry_count
                .get(&session_id)
                .copied(),
            Some(5)
        );
    }

    #[test]
    fn omenchat_path_result_uses_guarded_delayed_reconnect_status() {
        let mut desktop = desktop_with_temp_root("omenbrowser-rs-desktop-omenchat-path-reconnect");
        let session_id = desktop.open_omenchat_status_session(test_descriptor(), "waiting".into());
        let destination = FIXTURE_CHAT_SERVER_HASH.to_string();
        desktop.omenchat.omenchat_live_transports.insert(
            session_id,
            DesktopOmenChatTransport::new([0x31; 16], current_epoch_ms()),
        );

        let _ = desktop.update(Message::OmenChatTransportCompletion(
            crate::desktop::OmenChatTransportCompletionMessage::PathRequest {
                session_id,
                destination: destination.clone(),
                result: Ok(true),
            },
        ));

        assert!(desktop
            .omenchat
            .chat_client
            .session(session_id)
            .expect("session")
            .status
            .contains("live link remains active"));

        desktop
            .omenchat
            .omenchat_live_transports
            .remove(&session_id);
        desktop.omenchat.omenchat_live_opening.insert(session_id);
        let _ = desktop.update(Message::OmenChatTransportCompletion(
            crate::desktop::OmenChatTransportCompletionMessage::PathRequest {
                session_id,
                destination: destination.clone(),
                result: Ok(true),
            },
        ));

        assert!(desktop
            .omenchat
            .chat_client
            .session(session_id)
            .expect("session")
            .status
            .contains("reconnect already pending"));

        desktop.omenchat.omenchat_live_opening.remove(&session_id);
        let _ = desktop.update(Message::OmenChatTransportCompletion(
            crate::desktop::OmenChatTransportCompletionMessage::PathRequest {
                session_id,
                destination,
                result: Ok(true),
            },
        ));

        assert!(desktop
            .omenchat
            .chat_client
            .session(session_id)
            .expect("session")
            .status
            .contains("reconnecting after announce wait"));
    }

    #[test]
    fn omenchat_delayed_reconnect_clears_pending_work_but_preserves_active_retry_budget() {
        let mut desktop =
            desktop_with_temp_root("omenbrowser-rs-desktop-omenchat-active-reconnect-clear");
        let session_id =
            desktop.open_omenchat_status_session(test_descriptor(), "connected".into());
        desktop.omenchat.omenchat_live_transports.insert(
            session_id,
            DesktopOmenChatTransport::new([0x52; 16], current_epoch_ms()),
        );
        desktop.omenchat.omenchat_live_opening.insert(session_id);
        desktop
            .omenchat
            .omenchat_live_retry_after
            .insert(session_id, 123);
        desktop
            .omenchat
            .omenchat_live_retry_count
            .insert(session_id, 3);
        desktop
            .omenchat
            .omenchat_live_reconnect_generation
            .insert(session_id, 9);

        let _ = desktop.update(Message::OmenChat(
            crate::desktop::OmenChatMessage::ReconnectSessionIfDisconnected(session_id),
        ));

        assert!(desktop
            .omenchat
            .omenchat_live_transports
            .contains_key(&session_id));
        assert!(!desktop.omenchat.omenchat_live_opening.contains(&session_id));
        assert!(!desktop
            .omenchat
            .omenchat_live_retry_after
            .contains_key(&session_id));
        assert_eq!(
            desktop.omenchat.omenchat_live_retry_count.get(&session_id),
            Some(&3)
        );
        assert!(!desktop
            .omenchat
            .omenchat_live_reconnect_generation
            .contains_key(&session_id));
        assert!(desktop
            .omenchat
            .chat_client
            .session(session_id)
            .expect("session")
            .status
            .contains("already active"));
    }
}
