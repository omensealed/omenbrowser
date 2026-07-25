use std::collections::BTreeMap;

use super::client::{
    enforce_client_event_presentation_bounds, ChatClient, ChatClientEvent, ChatClientRequest,
    ChatSessionId, ChatSessionView,
};
use super::descriptor::OmenChatDescriptor;
use super::model::{
    bounded_chat_text, chat_text_fits, ChatEvent, ChatEventKind, ChatRoomSummary,
    ChatServerSummary, ChatUserSummary, CHAT_ROOM_NAME_MAX_BYTES, CHAT_ROOM_TOPIC_MAX_BYTES,
    CHAT_SERVER_DESTINATION_MAX_BYTES, CHAT_SERVER_DISPLAY_MAX_BYTES,
};
use super::protocol::{RoomId, ServerId};
use super::store::ChatStore;

#[derive(Clone, Debug, Default)]
pub struct MockChatStore {
    servers: BTreeMap<ServerId, ChatServerSummary>,
    active_rooms: BTreeMap<ServerId, RoomId>,
    local_user_ids: BTreeMap<ServerId, u32>,
    rooms: BTreeMap<(ServerId, RoomId), ChatRoomSummary>,
    users: BTreeMap<(ServerId, RoomId), Vec<ChatUserSummary>>,
    events: BTreeMap<(ServerId, RoomId), Vec<ChatEvent>>,
}

impl MockChatStore {
    pub fn seeded() -> Self {
        let mut store = Self::default();
        let server = ChatServerSummary {
            server_id: "mock-server".into(),
            destination: "mockchatdestination".into(),
            display_name: "Mock OMENchat".into(),
        };
        let room = ChatRoomSummary {
            server_id: server.server_id.clone(),
            room_id: 1,
            name: "lobby".into(),
            topic: Some("Default mock lobby".into()),
            unread: 0,
            joined: true,
        };
        let user = ChatUserSummary {
            server_id: server.server_id.clone(),
            user_id: 1,
            display_name: "Operator".into(),
            role_bits: 1,
            status_bits: 0,
            lxmf_available: true,
        };
        let event = ChatEvent {
            server_id: server.server_id.clone(),
            room_id: room.room_id,
            event_id: 1,
            actor_user_id: Some(user.user_id),
            actor_display_name: Some(user.display_name.clone()),
            at_unix: 0,
            kind: ChatEventKind::System {
                body: "Mock OMENchat session ready.".into(),
            },
        };
        let _ = store.save_server(server);
        let _ = store.save_room(room);
        let _ = store.replace_userlist(&"mock-server".to_owned(), 1, vec![user]);
        let _ = store.append_events(vec![event]);
        store
    }
}

pub fn open_mock_session(client: &mut ChatClient) -> Option<ChatSessionId> {
    try_open_mock_session(client)
}

fn try_open_mock_session(client: &mut ChatClient) -> Option<ChatSessionId> {
    let session_id = client.reserve_session_id();
    let server = ChatServerSummary {
        server_id: "mock-server".into(),
        destination: "mockchatdestination".into(),
        display_name: "Mock OMENchat".into(),
    };
    let active_room = ChatRoomSummary {
        server_id: server.server_id.clone(),
        room_id: 1,
        name: "lobby".into(),
        topic: Some("Default mock lobby".into()),
        unread: 0,
        joined: true,
    };
    let rooms = vec![
        active_room.clone(),
        ChatRoomSummary {
            server_id: server.server_id.clone(),
            room_id: 2,
            name: "support".into(),
            topic: Some("Mock support room".into()),
            unread: 0,
            joined: false,
        },
    ];
    let users = vec![
        ChatUserSummary {
            server_id: server.server_id.clone(),
            user_id: 1,
            display_name: "Operator".into(),
            role_bits: 1,
            status_bits: 0,
            lxmf_available: true,
        },
        ChatUserSummary {
            server_id: server.server_id.clone(),
            user_id: 2,
            display_name: "Relay".into(),
            role_bits: 0,
            status_bits: 0,
            lxmf_available: false,
        },
    ];
    let events = vec![
        ChatEvent {
            server_id: server.server_id.clone(),
            room_id: active_room.room_id,
            event_id: 50,
            actor_user_id: None,
            actor_display_name: None,
            at_unix: 0,
            kind: ChatEventKind::System {
                body: "Mock OMENchat session opened.".into(),
            },
        },
        ChatEvent {
            server_id: server.server_id.clone(),
            room_id: active_room.room_id,
            event_id: 51,
            actor_user_id: Some(1),
            actor_display_name: Some("Operator".into()),
            at_unix: 1,
            kind: ChatEventKind::Message {
                body: "Room join returned userlist and latest events.".into(),
            },
        },
    ];
    if !client.push_session(ChatSessionView {
        session_id,
        server,
        rooms,
        active_room,
        users,
        events,
        status: "mock transport connected".into(),
    }) {
        return None;
    }
    Some(session_id)
}

pub fn handle_mock_request(
    client: &mut ChatClient,
    request: ChatClientRequest,
) -> Vec<ChatClientEvent> {
    let mut events = match request {
        ChatClientRequest::OpenServer(descriptor) => open_mock_server(client, descriptor),
        ChatClientRequest::JoinRoom { session_id, room } => {
            join_mock_room(client, session_id, room)
        }
        ChatClientRequest::PartRoom { session_id, room } => {
            part_mock_room(client, session_id, room)
        }
        ChatClientRequest::SendMessage {
            session_id, body, ..
        }
        | ChatClientRequest::SendAction {
            session_id, body, ..
        } => {
            if !send_mock_room_event(client, session_id, body, |body| ChatEventKind::Message {
                body,
            }) {
                return vec![ChatClientEvent::Error {
                    session_id: Some(session_id),
                    message: "mock session is not available".into(),
                }];
            }
            client
                .session(session_id)
                .and_then(|session| session.events.last().cloned())
                .map(|event| ChatClientEvent::EventAppended { session_id, event })
                .into_iter()
                .collect()
        }
        ChatClientRequest::SendNotice {
            session_id, body, ..
        } => {
            if !send_mock_room_event(client, session_id, body, |body| ChatEventKind::Notice {
                body,
            }) {
                return vec![ChatClientEvent::Error {
                    session_id: Some(session_id),
                    message: "mock session is not available".into(),
                }];
            }
            client
                .session(session_id)
                .and_then(|session| session.events.last().cloned())
                .map(|event| ChatClientEvent::EventAppended { session_id, event })
                .into_iter()
                .collect()
        }
        ChatClientRequest::SendUpload {
            session_id,
            filename,
            bytes,
            ..
        } => {
            let Some(session) = client.session_mut(session_id) else {
                return vec![ChatClientEvent::Error {
                    session_id: Some(session_id),
                    message: "mock session is not available".into(),
                }];
            };
            session.status = format!("mock accepted upload {filename} ({} B)", bytes.len());
            vec![ChatClientEvent::UploadCompleted {
                session_id,
                resource_id: format!("mock-upload-{session_id}-{}", session.events.len()),
                filename,
                bytes: bytes.len() as u64,
            }]
        }
        ChatClientRequest::RequestUpload {
            session_id,
            resource_id,
            ..
        } => vec![ChatClientEvent::Error {
            session_id: Some(session_id),
            message: format!("mock upload resource is unavailable: {resource_id}"),
        }],
        ChatClientRequest::RefreshRooms { session_id } => client
            .session(session_id)
            .map(|session| {
                vec![ChatClientEvent::RoomsUpdated {
                    session_id,
                    rooms: session.rooms.clone(),
                }]
            })
            .unwrap_or_else(|| {
                vec![ChatClientEvent::Error {
                    session_id: Some(session_id),
                    message: "mock session is not available".into(),
                }]
            }),
        ChatClientRequest::SetTopic { session_id, topic } => {
            if !chat_text_fits(topic.trim(), CHAT_ROOM_TOPIC_MAX_BYTES) {
                return vec![ChatClientEvent::Error {
                    session_id: Some(session_id),
                    message: "room topic exceeds client limits".into(),
                }];
            }
            let Some(session) = client.session_mut(session_id) else {
                return vec![ChatClientEvent::Error {
                    session_id: Some(session_id),
                    message: "mock session is not available".into(),
                }];
            };
            let topic = topic.trim();
            session.active_room.topic = (!topic.is_empty()).then(|| topic.to_owned());
            if let Some(room) = session
                .rooms
                .iter_mut()
                .find(|room| room.room_id == session.active_room.room_id)
            {
                room.topic.clone_from(&session.active_room.topic);
            }
            session.status = "topic updated".into();
            vec![ChatClientEvent::RoomsUpdated {
                session_id,
                rooms: vec![session.active_room.clone()],
            }]
        }
        ChatClientRequest::CreateRoom {
            session_id,
            room,
            topic,
        } => {
            let Some(session) = client.session_mut(session_id) else {
                return vec![ChatClientEvent::Error {
                    session_id: Some(session_id),
                    message: "mock session is not available".into(),
                }];
            };
            let room_name = room.trim().trim_start_matches('#').to_owned();
            if room_name.is_empty()
                || !chat_text_fits(&room_name, CHAT_ROOM_NAME_MAX_BYTES)
                || topic
                    .as_deref()
                    .is_some_and(|topic| !chat_text_fits(topic.trim(), CHAT_ROOM_TOPIC_MAX_BYTES))
            {
                return vec![ChatClientEvent::Error {
                    session_id: Some(session_id),
                    message: "room name or topic is empty or exceeds client limits".into(),
                }];
            }
            let next_room_id = session
                .rooms
                .iter()
                .map(|room| room.room_id)
                .max()
                .unwrap_or(0)
                .saturating_add(1);
            let room = ChatRoomSummary {
                server_id: session.server.server_id.clone(),
                room_id: next_room_id,
                name: room_name,
                topic: topic.and_then(|topic| {
                    let topic = topic.trim().to_owned();
                    (!topic.is_empty()).then_some(topic)
                }),
                unread: 0,
                joined: false,
            };
            session.rooms.push(room.clone());
            session.status = format!("room created: #{}", room.name);
            vec![ChatClientEvent::RoomsUpdated {
                session_id,
                rooms: vec![room],
            }]
        }
        ChatClientRequest::ModerateUser {
            session_id,
            action,
            target,
        } => {
            let Some(session) = client.session_mut(session_id) else {
                return vec![ChatClientEvent::Error {
                    session_id: Some(session_id),
                    message: "mock session is not available".into(),
                }];
            };
            session.status = format!("{action} requested for {target}");
            Vec::new()
        }
        ChatClientRequest::SyncRecent { session_id } => {
            if let Some(session) = client.session_mut(session_id) {
                session.status = "mock recent history is current".into();
            }
            Vec::new()
        }
        ChatClientRequest::LoadOlder { session_id } => {
            let before = client
                .session(session_id)
                .map(|session| {
                    session
                        .events
                        .first()
                        .map(|event| event.event_id)
                        .unwrap_or(0)
                })
                .unwrap_or(0);
            if !load_older_mock_history(client, session_id) {
                return vec![ChatClientEvent::Error {
                    session_id: Some(session_id),
                    message: "mock history is at the beginning".into(),
                }];
            }
            let events = client
                .session(session_id)
                .map(|session| {
                    session
                        .events
                        .iter()
                        .filter(|event| event.event_id < before)
                        .cloned()
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            vec![ChatClientEvent::HistoryPrepended { session_id, events }]
        }
    };
    client.enforce_status_bounds();
    enforce_client_event_presentation_bounds(&mut events);
    events
}

fn join_mock_room(
    client: &mut ChatClient,
    session_id: ChatSessionId,
    room_name: String,
) -> Vec<ChatClientEvent> {
    let normalized = room_name.trim().trim_start_matches('#');
    if !chat_text_fits(normalized, CHAT_ROOM_NAME_MAX_BYTES) {
        return vec![ChatClientEvent::Error {
            session_id: Some(session_id),
            message: "mock room name exceeds client limits".into(),
        }];
    }
    let Some(session) = client.session_mut(session_id) else {
        return vec![ChatClientEvent::Error {
            session_id: Some(session_id),
            message: "mock session is not available".into(),
        }];
    };
    let Some(room) = session
        .rooms
        .iter()
        .find(|room| room.name == normalized)
        .cloned()
    else {
        return vec![ChatClientEvent::Error {
            session_id: Some(session_id),
            message: format!("mock room not found: {room_name}"),
        }];
    };
    session.active_room = room.clone();
    session.events.clear();
    session.users.clear();
    session.status = format!("mock joined #{}", room.name);
    vec![ChatClientEvent::RoomJoined {
        session_id,
        room,
        users: Vec::new(),
        latest_events: Vec::new(),
    }]
}

fn open_mock_server(
    client: &mut ChatClient,
    descriptor: OmenChatDescriptor,
) -> Vec<ChatClientEvent> {
    if !chat_text_fits(
        &descriptor.server_destination,
        CHAT_SERVER_DESTINATION_MAX_BYTES,
    ) {
        return vec![ChatClientEvent::Error {
            session_id: None,
            message: "OMENchat descriptor metadata exceeds client limits".into(),
        }];
    }
    let Some(session_id) = try_open_mock_session(client) else {
        return vec![ChatClientEvent::Error {
            session_id: None,
            message:
                "OMENchat client session limit reached; close a session before opening another"
                    .into(),
        }];
    };
    let Some(session) = client.session_mut(session_id) else {
        return vec![ChatClientEvent::Error {
            session_id: Some(session_id),
            message: "mock session failed to open".into(),
        }];
    };
    if !descriptor.server_destination.is_empty() {
        session.server.destination = descriptor.server_destination;
    }
    if let Some(display_name) = descriptor.display_name {
        session.server.display_name =
            bounded_chat_text(display_name.trim(), CHAT_SERVER_DISPLAY_MAX_BYTES);
    }
    vec![
        ChatClientEvent::ServerOpened {
            session_id,
            server: session.server.clone(),
        },
        ChatClientEvent::RoomJoined {
            session_id,
            room: session.active_room.clone(),
            users: session.users.clone(),
            latest_events: session.events.clone(),
        },
    ]
}

pub fn send_mock_message(client: &mut ChatClient, session_id: ChatSessionId, body: String) -> bool {
    send_mock_room_event(client, session_id, body, |body| ChatEventKind::Message {
        body,
    })
}

fn send_mock_room_event(
    client: &mut ChatClient,
    session_id: ChatSessionId,
    body: String,
    kind: impl FnOnce(String) -> ChatEventKind,
) -> bool {
    let Some(session) = client.session(session_id) else {
        return false;
    };
    let event_id = session
        .events
        .iter()
        .map(|event| event.event_id)
        .max()
        .unwrap_or(0)
        .saturating_add(1);
    let event = ChatEvent {
        server_id: session.server.server_id.clone(),
        room_id: session.active_room.room_id,
        event_id,
        actor_user_id: Some(1),
        actor_display_name: Some("Operator".into()),
        at_unix: current_unix_seconds(),
        kind: kind(body),
    };
    if !client.append_event_bounded(
        session_id,
        event,
        false,
        super::client::HistoryWindowEdge::Newest,
    ) {
        return false;
    }
    if let Some(session) = client.session_mut(session_id) {
        session.status = "mock message appended locally".into();
    }
    true
}

fn part_mock_room(
    client: &mut ChatClient,
    session_id: ChatSessionId,
    room: Option<String>,
) -> Vec<ChatClientEvent> {
    let Some(session) = client.session_mut(session_id) else {
        return vec![ChatClientEvent::Error {
            session_id: Some(session_id),
            message: "mock session is not available".into(),
        }];
    };
    let target_room_id = room
        .as_deref()
        .and_then(|name| {
            let name = name.trim().trim_start_matches('#');
            session
                .rooms
                .iter()
                .find(|room| room.name.eq_ignore_ascii_case(name))
                .map(|room| room.room_id)
        })
        .unwrap_or(session.active_room.room_id);
    let Some(room) = session
        .rooms
        .iter_mut()
        .find(|room| room.room_id == target_room_id)
    else {
        return vec![ChatClientEvent::Error {
            session_id: Some(session_id),
            message: "mock room is not available".into(),
        }];
    };
    room.joined = false;
    let room = room.clone();
    if session.active_room.room_id == target_room_id {
        session.users.clear();
        if let Some(next_room) = session.rooms.iter().find(|room| room.joined).cloned() {
            session.active_room = next_room.clone();
            session.status = format!("left #{}; selected #{}", room.name, next_room.name);
        } else {
            session.active_room.joined = false;
            session.status = format!("left #{}", room.name);
        }
    } else {
        session.status = format!("left #{}", room.name);
    }
    vec![ChatClientEvent::RoomsUpdated {
        session_id,
        rooms: vec![room],
    }]
}

pub fn load_older_mock_history(client: &mut ChatClient, session_id: ChatSessionId) -> bool {
    let Some(session) = client.session(session_id) else {
        return false;
    };
    let first_event_id = session
        .events
        .iter()
        .map(|event| event.event_id)
        .min()
        .unwrap_or(1);
    let insert_count = 5_u64;
    let mut older = (0..insert_count)
        .map(|index| {
            let event_id = first_event_id
                .saturating_sub(insert_count - index)
                .max(index + 1);
            ChatEvent {
                server_id: session.server.server_id.clone(),
                room_id: session.active_room.room_id,
                event_id,
                actor_user_id: None,
                actor_display_name: None,
                at_unix: 0,
                kind: ChatEventKind::System {
                    body: format!("Older mock history event {event_id}"),
                },
            }
        })
        .collect::<Vec<_>>();
    older.retain(|candidate| {
        !session
            .events
            .iter()
            .any(|event| event.event_id == candidate.event_id)
    });
    if older.is_empty() {
        if let Some(session) = client.session_mut(session_id) {
            session.status = "mock history is at the beginning".into();
        }
        return false;
    }
    let added = client.prepend_history_events(session_id, older);
    if added > 0 {
        if let Some(session) = client.session_mut(session_id) {
            session.status = "mock older history loaded".into();
        }
    }
    added > 0
}

fn current_unix_seconds() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}

impl ChatStore for MockChatStore {
    fn save_server(&mut self, server: ChatServerSummary) -> anyhow::Result<()> {
        self.servers.insert(server.server_id.clone(), server);
        Ok(())
    }

    fn saved_servers(&self) -> anyhow::Result<Vec<ChatServerSummary>> {
        Ok(self.servers.values().cloned().collect())
    }

    fn delete_server(&mut self, server_id: &ServerId) -> anyhow::Result<bool> {
        let deleted = self.servers.remove(server_id).is_some();
        self.active_rooms.remove(server_id);
        self.local_user_ids.remove(server_id);
        self.rooms
            .retain(|(stored_server_id, _), _| stored_server_id != server_id);
        self.users
            .retain(|(stored_server_id, _), _| stored_server_id != server_id);
        self.events
            .retain(|(stored_server_id, _), _| stored_server_id != server_id);
        Ok(deleted)
    }

    fn set_active_room(&mut self, server_id: &ServerId, room_id: RoomId) -> anyhow::Result<()> {
        self.active_rooms.insert(server_id.clone(), room_id);
        Ok(())
    }

    fn active_room_id(&self, server_id: &ServerId) -> anyhow::Result<Option<RoomId>> {
        Ok(self.active_rooms.get(server_id).copied())
    }

    fn set_local_user_id(
        &mut self,
        server_id: &ServerId,
        user_id: Option<u32>,
    ) -> anyhow::Result<()> {
        if let Some(user_id) = user_id {
            self.local_user_ids.insert(server_id.clone(), user_id);
        } else {
            self.local_user_ids.remove(server_id);
        }
        Ok(())
    }

    fn local_user_id(&self, server_id: &ServerId) -> anyhow::Result<Option<u32>> {
        Ok(self.local_user_ids.get(server_id).copied())
    }

    fn save_room(&mut self, room: ChatRoomSummary) -> anyhow::Result<()> {
        self.rooms
            .insert((room.server_id.clone(), room.room_id), room);
        Ok(())
    }

    fn rooms_for_server(&self, server_id: &ServerId) -> anyhow::Result<Vec<ChatRoomSummary>> {
        Ok(self
            .rooms
            .values()
            .filter(|room| &room.server_id == server_id)
            .cloned()
            .collect())
    }

    fn replace_userlist(
        &mut self,
        server_id: &ServerId,
        room_id: RoomId,
        users: Vec<ChatUserSummary>,
    ) -> anyhow::Result<()> {
        self.users.insert((server_id.clone(), room_id), users);
        Ok(())
    }

    fn users_for_room(
        &self,
        server_id: &ServerId,
        room_id: RoomId,
    ) -> anyhow::Result<Vec<ChatUserSummary>> {
        Ok(self
            .users
            .get(&(server_id.clone(), room_id))
            .cloned()
            .unwrap_or_default())
    }

    fn append_events(&mut self, events: Vec<ChatEvent>) -> anyhow::Result<()> {
        for event in events {
            self.events
                .entry((event.server_id.clone(), event.room_id))
                .or_default()
                .push(event);
        }
        Ok(())
    }

    fn latest_events(
        &self,
        server_id: &ServerId,
        room_id: RoomId,
        limit: usize,
    ) -> anyhow::Result<Vec<ChatEvent>> {
        let mut events = self
            .events
            .get(&(server_id.clone(), room_id))
            .cloned()
            .unwrap_or_default();
        let start = events.len().saturating_sub(limit);
        Ok(events.split_off(start))
    }

    fn events_before(
        &self,
        server_id: &ServerId,
        room_id: RoomId,
        before_event_id: u64,
        limit: usize,
    ) -> anyhow::Result<Vec<ChatEvent>> {
        let mut events: Vec<_> = self
            .events
            .get(&(server_id.clone(), room_id))
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter(|event| event.event_id < before_event_id)
            .collect();
        let start = events.len().saturating_sub(limit);
        Ok(events.split_off(start))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat::model::CHAT_STATUS_MAX_BYTES;

    #[test]
    fn mock_sessions_get_unique_ids_and_seed_state() {
        let mut client = ChatClient::new();
        let first_id = open_mock_session(&mut client).expect("first mock session");
        let second_id = open_mock_session(&mut client).expect("second mock session");
        let first = client.session(first_id).expect("first session");
        let second = client.session(second_id).expect("second session");

        assert_ne!(first.session_id, second.session_id);
        assert_eq!(first.active_room.name, "lobby");
        assert_eq!(first.users.len(), 2);
        assert!(first.events.len() >= 2);
    }

    #[test]
    fn mock_send_appends_and_history_prepends() {
        let mut client = ChatClient::new();
        let session_id = open_mock_session(&mut client).expect("mock session");

        assert!(send_mock_message(&mut client, session_id, "hello".into()));
        let session = client.session(session_id).expect("session");
        assert!(matches!(
            session.events.last().map(|event| &event.kind),
            Some(ChatEventKind::Message { body }) if body == "hello"
        ));

        assert!(load_older_mock_history(&mut client, session_id));
        let session = client.session(session_id).expect("session");
        assert_eq!(session.events.first().map(|event| event.event_id), Some(45));
    }

    #[test]
    fn mock_part_active_room_selects_next_joined_room() {
        let mut client = ChatClient::new();
        let session_id = open_mock_session(&mut client).expect("mock session");
        if let Some(session) = client.session_mut(session_id) {
            if let Some(room) = session.rooms.iter_mut().find(|room| room.name == "support") {
                room.joined = true;
            }
        }

        let events = handle_mock_request(
            &mut client,
            ChatClientRequest::PartRoom {
                session_id,
                room: None,
            },
        );

        assert!(matches!(
            events.as_slice(),
            [ChatClientEvent::RoomsUpdated { .. }]
        ));
        let session = client.session(session_id).expect("session");
        assert_eq!(session.active_room.name, "support");
        assert!(session.active_room.joined);
        assert_eq!(session.status, "left #lobby; selected #support");
    }

    #[test]
    fn mock_request_handler_opens_sends_and_loads_history() {
        let mut client = ChatClient::new();
        let events = handle_mock_request(
            &mut client,
            ChatClientRequest::OpenServer(OmenChatDescriptor {
                server_destination: "dest".into(),
                display_name: Some("Chat Node".into()),
                ..OmenChatDescriptor::default()
            }),
        );
        let session_id = match events.first() {
            Some(ChatClientEvent::ServerOpened { session_id, .. }) => *session_id,
            other => panic!("unexpected event: {other:?}"),
        };
        assert_eq!(
            client
                .session(session_id)
                .expect("session")
                .server
                .display_name,
            "Chat Node"
        );

        let events = handle_mock_request(
            &mut client,
            ChatClientRequest::SendMessage {
                session_id,
                room: "lobby".into(),
                body: "hello".into(),
            },
        );
        assert!(matches!(
            events.as_slice(),
            [ChatClientEvent::EventAppended { .. }]
        ));

        let events = handle_mock_request(&mut client, ChatClientRequest::LoadOlder { session_id });
        assert!(matches!(
            events.as_slice(),
            [ChatClientEvent::HistoryPrepended { .. }]
        ));
    }

    #[test]
    fn mock_request_handler_bounds_descriptor_and_status_metadata() {
        let mut client = ChatClient::new();
        let events = handle_mock_request(
            &mut client,
            ChatClientRequest::OpenServer(OmenChatDescriptor {
                server_destination: "dest".into(),
                display_name: Some("☃".repeat(CHAT_SERVER_DISPLAY_MAX_BYTES)),
                ..OmenChatDescriptor::default()
            }),
        );
        let session_id = match events.first() {
            Some(ChatClientEvent::ServerOpened { session_id, server }) => {
                assert!(server.display_name.len() <= CHAT_SERVER_DISPLAY_MAX_BYTES);
                *session_id
            }
            other => panic!("unexpected event: {other:?}"),
        };
        let _ = handle_mock_request(
            &mut client,
            ChatClientRequest::ModerateUser {
                session_id,
                action: "ban".into(),
                target: "☃".repeat(CHAT_STATUS_MAX_BYTES),
            },
        );
        assert!(client
            .session(session_id)
            .is_some_and(|session| session.status.len() <= CHAT_STATUS_MAX_BYTES));

        let before = client.sessions().len();
        let events = handle_mock_request(
            &mut client,
            ChatClientRequest::OpenServer(OmenChatDescriptor {
                server_destination: "d".repeat(CHAT_SERVER_DESTINATION_MAX_BYTES + 1),
                ..OmenChatDescriptor::default()
            }),
        );
        assert_eq!(client.sessions().len(), before);
        assert!(matches!(events.as_slice(), [ChatClientEvent::Error { .. }]));

        let session = client.session(session_id).expect("session");
        let room_count = session.rooms.len();
        let active_topic = session.active_room.topic.clone();
        for request in [
            ChatClientRequest::SetTopic {
                session_id,
                topic: "t".repeat(CHAT_ROOM_TOPIC_MAX_BYTES + 1),
            },
            ChatClientRequest::CreateRoom {
                session_id,
                room: "r".repeat(CHAT_ROOM_NAME_MAX_BYTES + 1),
                topic: None,
            },
        ] {
            let events = handle_mock_request(&mut client, request);
            assert!(matches!(events.as_slice(), [ChatClientEvent::Error { .. }]));
        }
        assert_eq!(
            client.session(session_id).expect("session").rooms.len(),
            room_count
        );
        assert_eq!(
            client
                .session(session_id)
                .expect("session")
                .active_room
                .topic,
            active_topic
        );
    }
}
