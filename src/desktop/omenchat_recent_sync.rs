use iced::widget::scrollable::RelativeOffset;
use iced::Task;

use crate::app::current_epoch_ms;
use crate::chat::{ChatClientEvent, ChatClientRequest, ChatSessionId};

use super::{
    compact_elapsed_ms, hex_bytes, scroll_offset_is_at_bottom, DesktopApp,
    DesktopOmenChatTransport, Message, OMENCHAT_RECENT_SYNC_MAX_ATTEMPTS,
    OMENCHAT_RECONNECT_STABLE_MS,
};

pub(in crate::desktop) fn omenchat_recent_sync_wants_bottom_restore(
    events: &[ChatClientEvent],
) -> bool {
    events.iter().any(|event| {
        matches!(
            event,
            ChatClientEvent::HistoryPrepended { .. } | ChatClientEvent::HistorySynced { .. }
        )
    })
}

impl DesktopApp {
    #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
    pub(in crate::desktop) fn register_omenchat_live_transport(
        &mut self,
        session_id: ChatSessionId,
        transport: DesktopOmenChatTransport,
    ) -> Task<Message> {
        let link_id = transport.link_id;
        self.remove_omenchat_link_session_mappings(session_id);
        self.omenchat
            .omenchat_link_sessions
            .insert(link_id, session_id);
        self.omenchat
            .omenchat_live_transports
            .insert(session_id, transport);
        self.omenchat
            .omenchat_live_connect_count
            .entry(session_id)
            .and_modify(|count| *count = count.saturating_add(1))
            .or_insert(1);
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
        self.omenchat.omenchat_recent_sync_links.remove(&session_id);
        self.omenchat
            .omenchat_recent_sync_attempts
            .remove(&session_id);
        self.omenchat.omenchat_live_opening.remove(&session_id);
        self.omenchat.omenchat_live_retry_after.remove(&session_id);
        self.omenchat
            .omenchat_live_reconnect_generation
            .remove(&session_id);
        if self
            .omenchat
            .omenchat_live_retry_count
            .contains_key(&session_id)
        {
            self.omenchat.omenchat_live_stable_after.insert(
                session_id,
                current_epoch_ms().saturating_add(OMENCHAT_RECONNECT_STABLE_MS),
            );
        }
        if self
            .omenchat
            .omenchat_recent_sync_pending
            .remove(&session_id)
        {
            tracing::debug!(
                session_id,
                link_id = %hex_bytes(&link_id),
                "OMENchat recent sync running after pending room join"
            );
            let events = self.sync_recent_omenchat_room_history_if_needed(session_id);
            if omenchat_recent_sync_wants_bottom_restore(&events)
                && self
                    .omenchat
                    .chat_scroll_offsets
                    .get(&self.omenchat_scroll_key(session_id))
                    .copied()
                    .map(scroll_offset_is_at_bottom)
                    .unwrap_or(false)
            {
                return self.restore_omenchat_scroll(session_id);
            }
        } else {
            tracing::debug!(
                session_id,
                link_id = %hex_bytes(&link_id),
                "OMENchat recent sync scheduled after live transport registration"
            );
            self.schedule_delayed_omenchat_recent_sync(session_id);
        }
        Task::none()
    }

    #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
    pub(in crate::desktop) fn sync_recent_omenchat_room_history(
        &mut self,
        session_id: ChatSessionId,
    ) -> Vec<ChatClientEvent> {
        tracing::debug!(session_id, "OMENchat recent sync request dispatching");
        let active_room_id = self
            .omenchat
            .chat_client
            .session(session_id)
            .map(|session| session.active_room.room_id)
            .unwrap_or(1);
        let scroll_key = (session_id, active_room_id);
        let was_following_bottom = self
            .omenchat
            .chat_scroll_offsets
            .get(&scroll_key)
            .copied()
            .map(scroll_offset_is_at_bottom)
            .unwrap_or(true);
        if self
            .omenchat
            .omenchat_live_transports
            .contains_key(&session_id)
        {
            self.omenchat
                .omenchat_recent_sync_due_after
                .remove(&session_id);
            self.omenchat
                .omenchat_recent_sync_pending
                .remove(&session_id);
        }
        let events = self.handle_omenchat_request(ChatClientRequest::SyncRecent { session_id });
        let accepted = events.iter().any(|event| {
            matches!(
                event,
                ChatClientEvent::HistoryPrepended { .. } | ChatClientEvent::HistorySynced { .. }
            )
        });
        if accepted && was_following_bottom {
            self.omenchat
                .chat_scroll_offsets
                .insert(scroll_key, RelativeOffset { x: 0.0, y: 1.0 });
            self.schedule_visible_workspace_scroll_restore(2);
        }
        if !accepted
            && self
                .omenchat
                .omenchat_live_transports
                .contains_key(&session_id)
        {
            self.schedule_retry_omenchat_recent_sync_if_unconfirmed(session_id);
        }
        if events
            .iter()
            .any(|event| matches!(event, ChatClientEvent::Error { .. }))
        {
            tracing::warn!(
                session_id,
                "OMENchat recent sync request produced an error event"
            );
        }
        events
    }

    #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
    pub(in crate::desktop) fn sync_recent_omenchat_room_history_if_needed(
        &mut self,
        session_id: ChatSessionId,
    ) -> Vec<ChatClientEvent> {
        let Some(link_id) = self
            .omenchat
            .omenchat_live_transports
            .get(&session_id)
            .map(|transport| transport.link_id)
        else {
            return Vec::new();
        };
        if self
            .omenchat
            .omenchat_recent_sync_links
            .get(&session_id)
            .is_some_and(|synced_link_id| *synced_link_id == link_id)
        {
            tracing::debug!(
                session_id,
                link_id = %hex_bytes(&link_id),
                "OMENchat recent sync skipped; link already accepted a sync response"
            );
            return Vec::new();
        }
        self.sync_recent_omenchat_room_history(session_id)
    }

    #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
    pub(in crate::desktop) fn schedule_delayed_omenchat_recent_sync(
        &mut self,
        session_id: ChatSessionId,
    ) {
        self.omenchat
            .omenchat_recent_sync_due_after
            .insert(session_id, current_epoch_ms().saturating_add(1_500));
    }

    #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
    pub(in crate::desktop) fn schedule_retry_omenchat_recent_sync_if_unconfirmed(
        &mut self,
        session_id: ChatSessionId,
    ) {
        let attempts = self
            .omenchat
            .omenchat_recent_sync_attempts
            .entry(session_id)
            .and_modify(|attempts| *attempts = attempts.saturating_add(1))
            .or_insert(1);
        if *attempts >= OMENCHAT_RECENT_SYNC_MAX_ATTEMPTS {
            tracing::debug!(
                session_id,
                attempts = *attempts,
                "OMENchat recent sync stopped waiting for an accepted response"
            );
            return;
        }
        self.omenchat
            .omenchat_recent_sync_due_after
            .insert(session_id, current_epoch_ms().saturating_add(3_000));
        tracing::debug!(
            session_id,
            next_attempt = attempts.saturating_add(1),
            "OMENchat recent sync will retry if no accepted response arrives"
        );
    }

    #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
    pub(in crate::desktop) fn schedule_omenchat_recent_sync_after_link_activity(
        &mut self,
        session_id: ChatSessionId,
        now_ms: u64,
    ) {
        let Some(link_id) = self
            .omenchat
            .omenchat_live_transports
            .get(&session_id)
            .map(|transport| transport.link_id)
        else {
            return;
        };
        if self
            .omenchat
            .omenchat_recent_sync_links
            .get(&session_id)
            .is_some_and(|synced_link_id| *synced_link_id == link_id)
        {
            return;
        }
        if self
            .omenchat
            .omenchat_recent_sync_due_after
            .contains_key(&session_id)
        {
            return;
        }
        if self
            .omenchat
            .omenchat_recent_sync_attempts
            .get(&session_id)
            .is_some_and(|attempts| *attempts >= OMENCHAT_RECENT_SYNC_MAX_ATTEMPTS)
        {
            return;
        }
        self.omenchat
            .omenchat_recent_sync_attempts
            .remove(&session_id);
        self.omenchat
            .omenchat_recent_sync_due_after
            .insert(session_id, now_ms.saturating_add(250));
        tracing::debug!(
            session_id,
            link_id = %hex_bytes(&link_id),
            "OMENchat recent sync re-armed after confirmed link activity"
        );
    }

    #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
    pub(in crate::desktop) fn sync_due_omenchat_recent_history(
        &mut self,
        now_ms: u64,
    ) -> Task<Message> {
        if self.omenchat.omenchat_recent_sync_due_after.is_empty() {
            return Task::none();
        }
        let due_sessions = self
            .omenchat
            .omenchat_recent_sync_due_after
            .iter()
            .filter_map(|(session_id, due_after)| (now_ms >= *due_after).then_some(*session_id))
            .collect::<Vec<_>>();
        let mut tasks = Vec::new();
        for session_id in due_sessions {
            self.omenchat
                .omenchat_recent_sync_due_after
                .remove(&session_id);
            tracing::debug!(session_id, "OMENchat delayed recent sync is due");
            let events = self.sync_recent_omenchat_room_history_if_needed(session_id);
            if omenchat_recent_sync_wants_bottom_restore(&events)
                && self
                    .omenchat
                    .chat_scroll_offsets
                    .get(&self.omenchat_scroll_key(session_id))
                    .copied()
                    .map(scroll_offset_is_at_bottom)
                    .unwrap_or(false)
            {
                tasks.push(self.restore_omenchat_scroll(session_id));
            }
        }
        if tasks.is_empty() {
            Task::none()
        } else {
            Task::batch(tasks)
        }
    }

    #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
    pub(in crate::desktop) fn mark_omenchat_recent_sync_complete(
        &mut self,
        session_id: ChatSessionId,
    ) {
        if let Some(link_id) = self
            .omenchat
            .omenchat_live_transports
            .get(&session_id)
            .map(|transport| transport.link_id)
        {
            tracing::debug!(
                session_id,
                link_id = %hex_bytes(&link_id),
                "OMENchat recent sync completed for live link"
            );
            self.omenchat
                .omenchat_recent_sync_links
                .insert(session_id, link_id);
            self.omenchat
                .omenchat_recent_sync_due_after
                .remove(&session_id);
            self.omenchat
                .omenchat_recent_sync_pending
                .remove(&session_id);
            self.omenchat
                .omenchat_recent_sync_attempts
                .remove(&session_id);
        }
    }

    #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
    pub(in crate::desktop) fn omenchat_recent_sync_monitor_label(
        &self,
        session_id: ChatSessionId,
        now_ms: u64,
    ) -> String {
        let live_link = self
            .omenchat
            .omenchat_live_transports
            .get(&session_id)
            .map(|transport| transport.link_id);
        if let Some(link_id) = live_link {
            if self
                .omenchat
                .omenchat_recent_sync_links
                .get(&session_id)
                .is_some_and(|synced_link_id| *synced_link_id == link_id)
            {
                return "history sync: current for live link".into();
            }
        }
        if self
            .omenchat
            .omenchat_recent_sync_pending
            .contains(&session_id)
        {
            return "history sync: waiting for live transport".into();
        }
        if let Some(due_after) = self
            .omenchat
            .omenchat_recent_sync_due_after
            .get(&session_id)
        {
            let attempts = self
                .omenchat
                .omenchat_recent_sync_attempts
                .get(&session_id)
                .copied()
                .unwrap_or(0);
            if now_ms >= *due_after {
                return format!("history sync: due now after {attempts} attempt(s)");
            }
            return format!(
                "history sync: retry in {} after {attempts} attempt(s)",
                compact_elapsed_ms(due_after.saturating_sub(now_ms))
            );
        }
        if let Some(attempts) = self.omenchat.omenchat_recent_sync_attempts.get(&session_id) {
            if *attempts >= OMENCHAT_RECENT_SYNC_MAX_ATTEMPTS {
                return format!("history sync: stopped after {attempts} attempt(s)");
            }
        }
        if live_link.is_some() {
            "history sync: not yet confirmed".into()
        } else {
            "history sync: offline".into()
        }
    }
}

#[cfg(all(
    test,
    any(feature = "chat-client-rns", feature = "chat-client-rns-clean")
))]
mod tests {
    use super::*;
    use crate::app::App;
    use crate::chat::model::ChatRoomSummary;
    use crate::chat::{ChatEventKind, OmenChatDescriptor};

    const FIXTURE_CHAT_SERVER_HASH: &str = "00112233445566778899aabbccddeeff";

    fn desktop_with_temp_root(name: &str) -> DesktopApp {
        let root = std::env::temp_dir().join(format!("{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let paths = crate::config::AppPaths::from_root(root);
        paths.ensure().expect("paths");
        let app = App::new(crate::config::AppConfig {
            paths,
            settings: crate::storage::settings::AppSettings::default(),
        });
        DesktopApp::new(app)
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

    fn open_test_session(desktop: &mut DesktopApp, status: &str) -> ChatSessionId {
        desktop.open_omenchat_status_session(test_descriptor(), status.into())
    }

    fn history_transport(
        link_id: [u8; 16],
        seq: u32,
        event_id: u64,
        body: &str,
    ) -> DesktopOmenChatTransport {
        let mut transport = DesktopOmenChatTransport::new(link_id, current_epoch_ms());
        let recent = crate::chat::protocol::Frame::new(
            crate::chat::protocol::ChatOp::HistoryInline,
            seq,
            Some(1),
            crate::chat::protocol::batch::compressed_values_body(&[
                crate::chat::protocol::FrameValue::Array(vec![
                    crate::chat::protocol::FrameValue::U64(event_id),
                    crate::chat::protocol::FrameValue::U64(1),
                    crate::chat::protocol::FrameValue::U64(2),
                    crate::chat::protocol::FrameValue::I64(120 + event_id as i64),
                    crate::chat::protocol::FrameValue::String(body.into()),
                    crate::chat::protocol::FrameValue::String("Peer".into()),
                ]),
            ])
            .expect("history body"),
        );
        transport.push_incoming_frame(
            crate::chat::codec::encode_frame(&recent).expect("encode frame"),
            current_epoch_ms(),
        );
        transport
    }

    fn room_joined_event(session_id: ChatSessionId) -> ChatClientEvent {
        ChatClientEvent::RoomJoined {
            session_id,
            room: ChatRoomSummary {
                server_id: FIXTURE_CHAT_SERVER_HASH.into(),
                room_id: 1,
                name: "lobby".into(),
                topic: None,
                unread: 0,
                joined: true,
            },
            users: Vec::new(),
            latest_events: Vec::new(),
        }
    }

    fn session_has_message(
        desktop: &DesktopApp,
        session_id: ChatSessionId,
        event_id: u64,
        body: &str,
    ) -> bool {
        desktop
            .omenchat.chat_client
            .session(session_id)
            .expect("session")
            .events
            .iter()
            .any(|event| {
                event.room_id == 1
                    && event.event_id == event_id
                    && matches!(&event.kind, ChatEventKind::Message { body: event_body } if event_body == body)
            })
    }

    #[tokio::test]
    async fn omenchat_recent_sync_monitor_label_reports_retry_and_current() {
        let mut desktop = desktop_with_temp_root("omenbrowser-rs-desktop-omenchat-sync-monitor");
        let session_id = open_test_session(&mut desktop, "connected");
        let link_id = [0x72; 16];
        desktop
            .omenchat
            .omenchat_live_transports
            .insert(session_id, DesktopOmenChatTransport::new(link_id, 1_000));
        desktop
            .omenchat
            .omenchat_recent_sync_due_after
            .insert(session_id, 2_000);
        desktop
            .omenchat
            .omenchat_recent_sync_attempts
            .insert(session_id, 1);

        let retry = desktop.omenchat_recent_sync_monitor_label(session_id, 1_250);
        assert!(retry.contains("retry in"));
        assert!(retry.contains("1 attempt"));

        desktop.mark_omenchat_recent_sync_complete(session_id);

        assert_eq!(
            desktop.omenchat_recent_sync_monitor_label(session_id, 2_500),
            "history sync: current for live link"
        );
    }

    #[tokio::test]
    async fn omenchat_registered_live_transport_syncs_recent_history() {
        let mut desktop = desktop_with_temp_root("omenbrowser-rs-desktop-omenchat-sync-recent");
        let session_id = open_test_session(&mut desktop, "connected");
        let _ = desktop.register_omenchat_live_transport(
            session_id,
            history_transport([0x61; 16], 9, 7, "missed while offline"),
        );

        let events = desktop.sync_recent_omenchat_room_history(session_id);

        assert!(
            matches!(
                events.as_slice(),
                [ChatClientEvent::HistoryPrepended { events, .. }]
                    if events.iter().map(|event| event.event_id).collect::<Vec<_>>() == vec![7]
            ),
            "events: {events:?}"
        );
        assert!(session_has_message(
            &desktop,
            session_id,
            7,
            "missed while offline"
        ));
        assert_eq!(
            desktop.omenchat.chat_scroll_offsets.get(&(session_id, 1)),
            Some(&RelativeOffset { x: 0.0, y: 1.0 })
        );
        assert!(desktop.workspace.restore_workspace_scrolls_pending);
    }

    #[tokio::test]
    async fn omenchat_recent_sync_preserves_manual_scrollback() {
        let mut desktop =
            desktop_with_temp_root("omenbrowser-rs-desktop-omenchat-sync-preserve-scrollback");
        let session_id = open_test_session(&mut desktop, "connected");
        desktop
            .omenchat
            .chat_scroll_offsets
            .insert((session_id, 1), RelativeOffset { x: 0.0, y: 0.41 });
        let _ = desktop.register_omenchat_live_transport(
            session_id,
            history_transport([0x66; 16], 13, 11, "history while reading"),
        );

        let events = desktop.sync_recent_omenchat_room_history(session_id);

        assert!(matches!(
            events.as_slice(),
            [ChatClientEvent::HistoryPrepended { .. }]
        ));
        assert_eq!(
            desktop.omenchat.chat_scroll_offsets.get(&(session_id, 1)),
            Some(&RelativeOffset { x: 0.0, y: 0.41 })
        );
    }

    #[tokio::test]
    async fn omenchat_live_transport_due_sync_catches_restored_room_without_join_event() {
        let mut desktop = desktop_with_temp_root("omenbrowser-rs-desktop-omenchat-due-sync-recent");
        let session_id = open_test_session(&mut desktop, "restored");
        let link_id = [0x63; 16];
        let _ = desktop.register_omenchat_live_transport(
            session_id,
            history_transport(link_id, 11, 9, "restored missed event"),
        );
        assert!(desktop
            .omenchat
            .omenchat_recent_sync_due_after
            .contains_key(&session_id));

        let _ = desktop.sync_due_omenchat_recent_history(current_epoch_ms().saturating_add(2_000));

        assert!(!desktop
            .omenchat
            .omenchat_recent_sync_due_after
            .contains_key(&session_id));
        assert_eq!(
            desktop.omenchat.omenchat_recent_sync_links.get(&session_id),
            Some(&link_id)
        );
        assert!(session_has_message(
            &desktop,
            session_id,
            9,
            "restored missed event"
        ));
    }

    #[tokio::test]
    async fn omenchat_recent_sync_request_alone_does_not_suppress_later_join_sync() {
        let mut desktop =
            desktop_with_temp_root("omenbrowser-rs-desktop-omenchat-unsatisfied-sync");
        let session_id = open_test_session(&mut desktop, "restored");
        let link_id = [0x64; 16];
        let _ = desktop.register_omenchat_live_transport(
            session_id,
            DesktopOmenChatTransport::new(link_id, current_epoch_ms()),
        );

        let _ = desktop.sync_due_omenchat_recent_history(current_epoch_ms().saturating_add(2_000));

        assert!(!desktop
            .omenchat
            .omenchat_recent_sync_links
            .contains_key(&session_id));
        assert!(desktop
            .omenchat
            .omenchat_recent_sync_due_after
            .contains_key(&session_id));
        assert_eq!(
            desktop
                .omenchat
                .omenchat_recent_sync_attempts
                .get(&session_id)
                .copied(),
            Some(1)
        );

        desktop.omenchat.omenchat_live_transports.insert(
            session_id,
            history_transport(link_id, 12, 10, "join-triggered sync"),
        );
        desktop.apply_omenchat_client_events_status(&[room_joined_event(session_id)]);

        assert_eq!(
            desktop.omenchat.omenchat_recent_sync_links.get(&session_id),
            Some(&link_id)
        );
        assert!(session_has_message(
            &desktop,
            session_id,
            10,
            "join-triggered sync"
        ));
    }

    #[tokio::test]
    async fn omenchat_room_join_before_transport_registers_defers_recent_sync() {
        let mut desktop = desktop_with_temp_root("omenbrowser-rs-desktop-omenchat-deferred-sync");
        let session_id = open_test_session(&mut desktop, "joining");

        desktop.apply_omenchat_client_events_status(&[room_joined_event(session_id)]);
        assert!(desktop
            .omenchat
            .omenchat_recent_sync_pending
            .contains(&session_id));

        let _ = desktop.register_omenchat_live_transport(
            session_id,
            history_transport([0x62; 16], 10, 8, "deferred missed event"),
        );

        assert!(!desktop
            .omenchat
            .omenchat_recent_sync_pending
            .contains(&session_id));
        assert!(session_has_message(
            &desktop,
            session_id,
            8,
            "deferred missed event"
        ));
    }
}
