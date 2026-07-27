use std::path::Path;

use crate::chat::commands::{parse_client_command, ClientCommand};
use crate::chat::protocol::RoomId;
use crate::chat::{ChatClientEvent, ChatClientRequest, ChatSessionId};
use crate::workspace::WorkspaceSection;

use super::{
    is_omenchat_local_echo_event, omenchat_command_result_from_events,
    omenchat_upload_content_type, omenchat_upload_policy_rejection, unique_chat_users, DesktopApp,
    OmenChatDraftCommandResult,
};

impl DesktopApp {
    pub(in crate::desktop) fn send_omenchat_draft(&mut self, session_id: ChatSessionId) {
        let draft = self
            .omenchat
            .chat_drafts
            .get(&session_id)
            .map(|draft| draft.trim().to_owned())
            .unwrap_or_default();
        if draft.is_empty() {
            return;
        }
        match self.handle_omenchat_draft_command(session_id, &draft) {
            OmenChatDraftCommandResult::NotCommand => {}
            OmenChatDraftCommandResult::HandledClear => {
                self.omenchat.chat_drafts.insert(session_id, String::new());
                return;
            }
            OmenChatDraftCommandResult::HandledKeep => return,
        }
        let events = self.handle_omenchat_request(ChatClientRequest::SendMessage {
            session_id,
            room: self
                .omenchat
                .chat_client
                .session(session_id)
                .map(|session| session.active_room.name.clone())
                .unwrap_or_else(|| "lobby".into()),
            body: draft,
        });
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
    }

    pub(in crate::desktop) fn resend_omenchat_local_echo(
        &mut self,
        session_id: ChatSessionId,
        room_id: RoomId,
        event_id: u64,
        body: String,
        action: bool,
    ) {
        let room = self
            .omenchat
            .chat_client
            .session(session_id)
            .and_then(|session| {
                session
                    .rooms
                    .iter()
                    .find(|room| room.room_id == room_id)
                    .map(|room| room.name.clone())
                    .or_else(|| Some(session.active_room.name.clone()))
            })
            .unwrap_or_else(|| "lobby".into());
        let request = if action {
            ChatClientRequest::SendAction {
                session_id,
                room,
                body,
            }
        } else {
            ChatClientRequest::SendMessage {
                session_id,
                room,
                body,
            }
        };
        let events = self.handle_omenchat_request(request);
        let replacement_queued = events.iter().any(|event| {
            matches!(event, ChatClientEvent::EventAppended { event, .. }
                if is_omenchat_local_echo_event(event))
        });
        if replacement_queued {
            if let Some(session) = self.omenchat.chat_client.session_mut(session_id) {
                session
                    .events
                    .retain(|event| !(event.room_id == room_id && event.event_id == event_id));
            }
        } else if events
            .iter()
            .any(|event| matches!(event, ChatClientEvent::Error { .. }))
        {
            self.set_omenchat_session_status(
                session_id,
                "resend did not leave the client; original local echo kept".into(),
            );
        }
        if events.iter().any(|event| {
            matches!(event, ChatClientEvent::EventAppended { event, .. }
                if !is_omenchat_local_echo_event(event))
        }) || replacement_queued
        {
            self.lock_omenchat_bottom_until_restore_settles(session_id);
            self.schedule_visible_workspace_scroll_restore(2);
        }
    }

    pub(in crate::desktop) fn send_omenchat_upload_path(
        &mut self,
        session_id: ChatSessionId,
        path: &Path,
    ) -> OmenChatDraftCommandResult {
        let room_id = self
            .omenchat
            .chat_client
            .session(session_id)
            .map(|session| session.active_room.room_id);
        if room_id.is_some_and(|room_id| !self.omenchat_room_publish_available(session_id, room_id))
        {
            self.set_omenchat_session_status(
                session_id,
                "room is read-only for members; upload was not opened".into(),
            );
            return OmenChatDraftCommandResult::HandledKeep;
        }
        if path.as_os_str().is_empty() {
            self.set_omenchat_session_status(session_id, "usage: /upload <path>".into());
            return OmenChatDraftCommandResult::HandledKeep;
        }
        let filename = path
            .file_name()
            .and_then(|value| value.to_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| "upload.bin".into());
        let upload_byte_len = match std::fs::metadata(path) {
            Ok(metadata) if metadata.is_file() => metadata.len(),
            Ok(_) => {
                self.set_omenchat_session_status(session_id, "upload path is not a file".into());
                return OmenChatDraftCommandResult::HandledKeep;
            }
            Err(error) => {
                self.set_omenchat_session_status(
                    session_id,
                    format!("upload metadata failed: {error}"),
                );
                return OmenChatDraftCommandResult::HandledKeep;
            }
        };
        if upload_byte_len == 0 {
            self.set_omenchat_session_status(session_id, "upload file is empty".into());
            return OmenChatDraftCommandResult::HandledKeep;
        }
        if let Some(reason) = omenchat_upload_policy_rejection(
            upload_byte_len,
            self.omenchat_session_upload_quota(session_id),
            self.omenchat_session_upload_max_file_bytes(session_id),
        ) {
            self.set_omenchat_session_status(session_id, reason);
            return OmenChatDraftCommandResult::HandledKeep;
        }
        let bytes = match std::fs::read(path) {
            Ok(bytes) if !bytes.is_empty() => bytes,
            Ok(_) => {
                self.set_omenchat_session_status(session_id, "upload file is empty".into());
                return OmenChatDraftCommandResult::HandledKeep;
            }
            Err(error) => {
                self.set_omenchat_session_status(
                    session_id,
                    format!("upload read failed: {error}"),
                );
                return OmenChatDraftCommandResult::HandledKeep;
            }
        };
        let upload_byte_len = bytes.len() as u64;
        if let Some(reason) = omenchat_upload_policy_rejection(
            upload_byte_len,
            self.omenchat_session_upload_quota(session_id),
            self.omenchat_session_upload_max_file_bytes(session_id),
        ) {
            self.set_omenchat_session_status(session_id, reason);
            return OmenChatDraftCommandResult::HandledKeep;
        }
        let content_type = omenchat_upload_content_type(&filename);
        let pending_key = (session_id, filename.clone(), upload_byte_len);
        self.omenchat
            .omenchat_pending_upload_sources
            .insert(pending_key.clone(), path.to_path_buf());
        let events = self.handle_omenchat_request(ChatClientRequest::SendUpload {
            session_id,
            room: self
                .omenchat
                .chat_client
                .session(session_id)
                .map(|session| session.active_room.name.clone())
                .unwrap_or_else(|| "lobby".into()),
            filename: filename.clone(),
            content_type,
            bytes,
        });
        for event in &events {
            if let ChatClientEvent::UploadAccepted {
                session_id: accepted_session_id,
                resource_id,
                filename: accepted_filename,
                bytes: accepted_bytes,
            } = event
            {
                if *accepted_session_id == session_id
                    && accepted_filename == &filename
                    && *accepted_bytes == upload_byte_len
                {
                    let source_path = self
                        .omenchat
                        .omenchat_pending_upload_sources
                        .remove(&pending_key)
                        .unwrap_or_else(|| path.to_path_buf());
                    match self.cache_omenchat_upload_source_file(
                        session_id,
                        resource_id.as_str(),
                        &filename,
                        &source_path,
                    ) {
                        Ok(path) => {
                            self.set_omenchat_session_status(
                                session_id,
                                format!("upload accepted and cached locally: {path}"),
                            );
                        }
                        Err(error) => {
                            self.set_omenchat_session_status(
                                session_id,
                                format!("upload accepted; local cache failed: {error}"),
                            );
                        }
                    }
                }
            }
        }
        if events.iter().any(|event| {
            matches!(
                event,
                ChatClientEvent::UploadRejected {
                    session_id: rejected_session_id,
                    ..
                } if *rejected_session_id == session_id
            )
        }) {
            self.omenchat
                .omenchat_pending_upload_sources
                .remove(&pending_key);
        }
        self.apply_omenchat_client_events_status(&events);
        omenchat_command_result_from_events(&events)
    }

    pub(in crate::desktop) fn handle_omenchat_draft_command(
        &mut self,
        session_id: ChatSessionId,
        draft: &str,
    ) -> OmenChatDraftCommandResult {
        let Some(command) = parse_client_command(draft) else {
            return OmenChatDraftCommandResult::NotCommand;
        };
        match command {
            ClientCommand::Me(body) => {
                if body.trim().is_empty() {
                    self.set_omenchat_session_status(session_id, "usage: /me <action>".into());
                    return OmenChatDraftCommandResult::HandledKeep;
                }
                let events = self.handle_omenchat_request(ChatClientRequest::SendAction {
                    session_id,
                    room: self
                        .omenchat
                        .chat_client
                        .session(session_id)
                        .map(|session| session.active_room.name.clone())
                        .unwrap_or_else(|| "lobby".into()),
                    body,
                });
                if events
                    .iter()
                    .any(|event| matches!(event, ChatClientEvent::EventAppended { .. }))
                {
                    self.persist_omenchat_session(session_id);
                }
                self.apply_omenchat_client_events_status(&events);
                omenchat_command_result_from_events(&events)
            }
            ClientCommand::Join(room) => {
                let room = room.trim().trim_start_matches('#').to_owned();
                if room.is_empty() {
                    self.set_omenchat_session_status(session_id, "usage: /join <room>".into());
                    OmenChatDraftCommandResult::HandledKeep
                } else {
                    self.join_omenchat_room(session_id, room);
                    OmenChatDraftCommandResult::HandledClear
                }
            }
            ClientCommand::Rooms => {
                let events =
                    self.handle_omenchat_request(ChatClientRequest::RefreshRooms { session_id });
                if events
                    .iter()
                    .any(|event| matches!(event, ChatClientEvent::RoomsUpdated { .. }))
                {
                    self.persist_omenchat_session(session_id);
                }
                let rooms = self
                    .omenchat
                    .chat_client
                    .session(session_id)
                    .map(|session| {
                        session
                            .rooms
                            .iter()
                            .map(|room| format!("#{}", room.name))
                            .collect::<Vec<_>>()
                            .join(", ")
                    })
                    .filter(|rooms| !rooms.is_empty())
                    .unwrap_or_else(|| "no rooms advertised yet".into());
                self.set_omenchat_session_status(session_id, format!("rooms: {rooms}"));
                omenchat_command_result_from_events(&events)
            }
            ClientCommand::Who => {
                let users = self
                    .omenchat
                    .chat_client
                    .session(session_id)
                    .map(|session| {
                        unique_chat_users(&session.users)
                            .into_iter()
                            .map(|user| user.display_label())
                            .collect::<Vec<_>>()
                            .join(", ")
                    })
                    .filter(|users| !users.is_empty())
                    .unwrap_or_else(|| "no users visible yet".into());
                self.set_omenchat_session_status(session_id, format!("users: {users}"));
                OmenChatDraftCommandResult::HandledClear
            }
            ClientCommand::Help => {
                self.app.switch_section(WorkspaceSection::Help);
                self.set_omenchat_session_status(
                    session_id,
                    "opened Help; see OMENchat commands".into(),
                );
                OmenChatDraftCommandResult::HandledClear
            }
            ClientCommand::Notice(body) => {
                if body.trim().is_empty() {
                    self.set_omenchat_session_status(session_id, "usage: /notice <text>".into());
                    return OmenChatDraftCommandResult::HandledKeep;
                }
                let events = self.handle_omenchat_request(ChatClientRequest::SendNotice {
                    session_id,
                    room: self
                        .omenchat
                        .chat_client
                        .session(session_id)
                        .map(|session| session.active_room.name.clone())
                        .unwrap_or_else(|| "lobby".into()),
                    body,
                });
                if events
                    .iter()
                    .any(|event| matches!(event, ChatClientEvent::EventAppended { .. }))
                {
                    self.persist_omenchat_session(session_id);
                }
                self.apply_omenchat_client_events_status(&events);
                omenchat_command_result_from_events(&events)
            }
            ClientCommand::Upload(path) => {
                self.send_omenchat_upload_path(session_id, Path::new(path.trim()))
            }
            ClientCommand::Topic(topic) => {
                let events =
                    self.handle_omenchat_request(ChatClientRequest::SetTopic { session_id, topic });
                let updated = events
                    .iter()
                    .any(|event| matches!(event, ChatClientEvent::RoomsUpdated { .. }));
                self.apply_omenchat_client_events_status(&events);
                if updated {
                    self.persist_omenchat_session(session_id);
                }
                omenchat_command_result_from_events(&events)
            }
            ClientCommand::CreateRoom { room, topic } => {
                let room = room.trim().trim_start_matches('#').to_owned();
                if room.is_empty() {
                    self.set_omenchat_session_status(
                        session_id,
                        "usage: /create <room> [topic]".into(),
                    );
                    return OmenChatDraftCommandResult::HandledKeep;
                }
                let events = self.handle_omenchat_request(ChatClientRequest::CreateRoom {
                    session_id,
                    room,
                    topic,
                });
                let updated = events
                    .iter()
                    .any(|event| matches!(event, ChatClientEvent::RoomsUpdated { .. }));
                self.apply_omenchat_client_events_status(&events);
                if updated {
                    self.persist_omenchat_session(session_id);
                }
                omenchat_command_result_from_events(&events)
            }
            ClientCommand::Part(room) => {
                let room = room.map(|room| room.trim().trim_start_matches('#').to_owned());
                let events =
                    self.handle_omenchat_request(ChatClientRequest::PartRoom { session_id, room });
                let updated = events
                    .iter()
                    .any(|event| matches!(event, ChatClientEvent::RoomsUpdated { .. }));
                self.apply_omenchat_client_events_status(&events);
                if updated {
                    self.persist_omenchat_session(session_id);
                }
                omenchat_command_result_from_events(&events)
            }
            ClientCommand::Kick(target) => {
                self.send_omenchat_moderation_command(session_id, "kick", target)
            }
            ClientCommand::Ban(target) => {
                self.send_omenchat_moderation_command(session_id, "ban", target)
            }
            ClientCommand::Unban(target) => {
                self.send_omenchat_moderation_command(session_id, "unban", target)
            }
            ClientCommand::Mute(target) => {
                self.send_omenchat_moderation_command(session_id, "mute", target)
            }
            ClientCommand::Unmute(target) => {
                self.send_omenchat_moderation_command(session_id, "unmute", target)
            }
            ClientCommand::Role { target, role } => {
                let target = target.trim();
                let role = role.trim();
                if target.is_empty() || role.is_empty() {
                    self.set_omenchat_session_status(
                        session_id,
                        "usage: /role <user> <standard|trusted|mod|admin>".into(),
                    );
                    OmenChatDraftCommandResult::HandledKeep
                } else {
                    self.send_omenchat_moderation_command(
                        session_id,
                        "role",
                        format!("{target} {role}"),
                    )
                }
            }
            ClientCommand::Unknown(name) => {
                self.set_omenchat_session_status(
                    session_id,
                    format!("unknown command: /{name}; try /help"),
                );
                OmenChatDraftCommandResult::HandledKeep
            }
            ClientCommand::DirectMessage(_) => {
                self.set_omenchat_session_status(
                    session_id,
                    "that OMENchat command is not implemented yet".into(),
                );
                OmenChatDraftCommandResult::HandledKeep
            }
        }
    }

    pub(in crate::desktop) fn join_omenchat_room(
        &mut self,
        session_id: ChatSessionId,
        room: String,
    ) {
        let current = self
            .omenchat
            .chat_client
            .session(session_id)
            .map(|session| session.active_room.name == room)
            .unwrap_or(false);
        if current {
            return;
        }
        let events = self.handle_omenchat_request(ChatClientRequest::JoinRoom { session_id, room });
        if events
            .iter()
            .any(|event| matches!(event, ChatClientEvent::RoomJoined { .. }))
        {
            self.restore_cached_omenchat_room_history(session_id);
        }
    }

    pub(in crate::desktop) fn send_omenchat_moderation_command(
        &mut self,
        session_id: ChatSessionId,
        action: &str,
        target: String,
    ) -> OmenChatDraftCommandResult {
        let target = target.trim().to_owned();
        if target.is_empty() {
            self.set_omenchat_session_status(session_id, format!("usage: /{action} <active user>"));
            return OmenChatDraftCommandResult::HandledKeep;
        }
        let events = self.handle_omenchat_request(ChatClientRequest::ModerateUser {
            session_id,
            action: action.to_owned(),
            target,
        });
        self.apply_omenchat_client_events_status(&events);
        omenchat_command_result_from_events(&events)
    }

    pub(in crate::desktop) fn load_older_omenchat_history(&mut self, session_id: ChatSessionId) {
        let cached_loaded = if let Some(store) = self.omenchat.chat_store.as_ref() {
            match self
                .omenchat
                .chat_client
                .load_cached_history_before(store, session_id, 50)
            {
                Ok(count) => count,
                Err(error) => {
                    self.set_omenchat_session_status(
                        session_id,
                        format!("cached history load failed: {error}"),
                    );
                    0
                }
            }
        } else {
            0
        };
        if cached_loaded > 0 {
            self.persist_omenchat_session(session_id);
            return;
        }
        #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
        if self.omenchat_live_history_request_requires_reconnect(session_id) {
            if cached_loaded == 0 {
                self.set_omenchat_session_status(
                    session_id,
                    "no older cached history; reconnect to request server history".into(),
                );
            }
            return;
        }
        let events = self.handle_omenchat_request(ChatClientRequest::LoadOlder { session_id });
        if events
            .iter()
            .any(|event| matches!(event, ChatClientEvent::HistoryPrepended { .. }))
        {
            self.persist_omenchat_session(session_id);
        }
    }

    #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
    pub(in crate::desktop) fn omenchat_live_history_request_requires_reconnect(
        &self,
        session_id: ChatSessionId,
    ) -> bool {
        !self
            .omenchat
            .omenchat_live_transports
            .contains_key(&session_id)
            && self
                .omenchat
                .chat_client
                .session(session_id)
                .is_some_and(|session| {
                    session.server.destination != "mockchatdestination"
                        && session.server.destination.len() >= 32
                })
    }
}

#[cfg(all(test, feature = "chat-client"))]
mod tests {
    use super::*;
    use crate::app::App;
    use crate::chat::store::ChatStore;
    use crate::chat::{ChatEvent, ChatEventKind, OmenChatDescriptor};

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

    fn descriptor(destination: &str, display_name: &str) -> OmenChatDescriptor {
        OmenChatDescriptor {
            server_destination: destination.into(),
            display_name: Some(display_name.into()),
            rooms_hint: vec!["lobby".into()],
            local_display_name: Some("tester".into()),
            ..OmenChatDescriptor::default()
        }
    }

    fn message(destination: &str, event_id: u64, at_unix: i64, body: &str) -> ChatEvent {
        ChatEvent {
            server_id: destination.into(),
            room_id: 1,
            event_id,
            actor_user_id: None,
            actor_display_name: Some("Alice".into()),
            at_unix,
            kind: ChatEventKind::Message { body: body.into() },
        }
    }

    fn append_cached_events(desktop: &mut DesktopApp, events: Vec<ChatEvent>) {
        desktop
            .omenchat
            .chat_store
            .as_mut()
            .expect("chat store")
            .append_events(events)
            .expect("cached events");
    }

    #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
    #[test]
    fn omenchat_load_older_uses_cache_when_live_session_is_disconnected() {
        let mut desktop =
            desktop_with_temp_root("omenbrowser-rs-desktop-omenchat-load-older-cache");
        let destination = FIXTURE_CHAT_SERVER_HASH;
        let session_id = desktop.open_omenchat_status_session(
            descriptor(destination, "Test OMENchat"),
            "disconnected".into(),
        );
        if let Some(session) = desktop.omenchat.chat_client.session_mut(session_id) {
            session.active_room.joined = true;
            session.rooms[0].joined = true;
            session.events.push(message(destination, 3, 3, "newest"));
        }
        append_cached_events(
            &mut desktop,
            vec![
                message(destination, 1, 1, "older one"),
                message(destination, 2, 2, "older two"),
            ],
        );

        desktop.load_older_omenchat_history(session_id);

        let session = desktop
            .omenchat
            .chat_client
            .session(session_id)
            .expect("session");
        assert_eq!(
            session
                .events
                .iter()
                .map(|event| event.event_id)
                .collect::<Vec<_>>(),
            vec![1, 2, 3]
        );
        assert_eq!(session.status, "loaded 2 older cached event(s)");
    }

    #[test]
    fn omenchat_load_older_cache_hit_does_not_send_second_request() {
        let mut desktop =
            desktop_with_temp_root("omenbrowser-rs-desktop-omenchat-load-older-cache-only");
        let destination = "mockchatdestination";
        let session_id = desktop.open_omenchat_status_session(
            descriptor(destination, "Mock OMENchat"),
            "connected".into(),
        );
        if let Some(session) = desktop.omenchat.chat_client.session_mut(session_id) {
            session.active_room.joined = true;
            session.rooms[0].joined = true;
            session.events.push(message(destination, 3, 3, "newest"));
        }
        append_cached_events(
            &mut desktop,
            vec![
                message(destination, 1, 1, "older one"),
                message(destination, 2, 2, "older two"),
            ],
        );

        desktop.load_older_omenchat_history(session_id);

        let session = desktop
            .omenchat
            .chat_client
            .session(session_id)
            .expect("session");
        assert_eq!(
            session
                .events
                .iter()
                .map(|event| event.event_id)
                .collect::<Vec<_>>(),
            vec![1, 2, 3]
        );
        assert_eq!(session.status, "loaded 2 older cached event(s)");
    }

    #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
    #[test]
    fn omenchat_load_older_reports_reconnect_when_disconnected_cache_is_empty() {
        let mut desktop =
            desktop_with_temp_root("omenbrowser-rs-desktop-omenchat-load-older-empty-cache");
        let session_id = desktop.open_omenchat_status_session(
            descriptor(FIXTURE_CHAT_SERVER_HASH, "Test OMENchat"),
            "disconnected".into(),
        );

        desktop.load_older_omenchat_history(session_id);

        let session = desktop
            .omenchat
            .chat_client
            .session(session_id)
            .expect("session");
        assert!(session.events.is_empty());
        assert_eq!(
            session.status,
            "no older cached history; reconnect to request server history"
        );
    }
}
