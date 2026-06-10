use super::descriptor::OmenChatDescriptor;
use super::model::{ChatEvent, ChatRoomSummary, ChatServerSummary, ChatUserSummary};
use super::protocol::{EventId, RoomId, ServerId};
use super::store::ChatStore;

pub type ChatSessionId = u64;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChatClientRequest {
    OpenServer(OmenChatDescriptor),
    JoinRoom {
        session_id: ChatSessionId,
        room: String,
    },
    PartRoom {
        session_id: ChatSessionId,
        room: Option<String>,
    },
    SendMessage {
        session_id: ChatSessionId,
        room: String,
        body: String,
    },
    SendAction {
        session_id: ChatSessionId,
        room: String,
        body: String,
    },
    SendNotice {
        session_id: ChatSessionId,
        room: String,
        body: String,
    },
    SendUpload {
        session_id: ChatSessionId,
        room: String,
        filename: String,
        content_type: Option<String>,
        bytes: Vec<u8>,
    },
    RequestUpload {
        session_id: ChatSessionId,
        room: String,
        resource_id: String,
    },
    RefreshRooms {
        session_id: ChatSessionId,
    },
    SetTopic {
        session_id: ChatSessionId,
        topic: String,
    },
    CreateRoom {
        session_id: ChatSessionId,
        room: String,
        topic: Option<String>,
    },
    ModerateUser {
        session_id: ChatSessionId,
        action: String,
        target: String,
    },
    SyncRecent {
        session_id: ChatSessionId,
    },
    LoadOlder {
        session_id: ChatSessionId,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChatClientEvent {
    ServerOpened {
        session_id: ChatSessionId,
        server: ChatServerSummary,
    },
    RoomJoined {
        session_id: ChatSessionId,
        room: ChatRoomSummary,
        users: Vec<ChatUserSummary>,
        latest_events: Vec<ChatEvent>,
    },
    RoomsUpdated {
        session_id: ChatSessionId,
        rooms: Vec<ChatRoomSummary>,
    },
    ServerMotd {
        session_id: ChatSessionId,
        motd: String,
    },
    ServerPolicy {
        session_id: ChatSessionId,
        upload_quota_bytes: u64,
        upload_max_file_bytes: u64,
        ping_interval_seconds: u64,
    },
    UserUpdated {
        session_id: ChatSessionId,
        user: ChatUserSummary,
    },
    EventAppended {
        session_id: ChatSessionId,
        event: ChatEvent,
    },
    HistoryPrepended {
        session_id: ChatSessionId,
        events: Vec<ChatEvent>,
    },
    HistorySynced {
        session_id: ChatSessionId,
        room_id: RoomId,
    },
    HistorySyncNeeded {
        session_id: ChatSessionId,
        room_id: RoomId,
    },
    UploadAccepted {
        session_id: ChatSessionId,
        resource_id: String,
        filename: String,
        bytes: u64,
    },
    UploadRejected {
        session_id: ChatSessionId,
        reason: String,
    },
    UploadCompleted {
        session_id: ChatSessionId,
        resource_id: String,
        filename: String,
        bytes: u64,
    },
    UploadResourceAvailable {
        session_id: ChatSessionId,
        resource_id: String,
        filename: String,
        content_type: Option<String>,
        bytes: Vec<u8>,
    },
    UploadResourceProgress {
        session_id: ChatSessionId,
        resource_id: String,
        filename: String,
        received: u64,
        total: u64,
    },
    Error {
        session_id: Option<ChatSessionId>,
        message: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChatSessionView {
    pub session_id: ChatSessionId,
    pub server: ChatServerSummary,
    pub rooms: Vec<ChatRoomSummary>,
    pub active_room: ChatRoomSummary,
    pub users: Vec<ChatUserSummary>,
    pub events: Vec<ChatEvent>,
    pub status: String,
}

#[derive(Clone, Debug, Default)]
pub struct ChatClient {
    next_session_id: ChatSessionId,
    sessions: Vec<ChatSessionView>,
}

impl ChatClient {
    pub fn new() -> Self {
        Self {
            next_session_id: 1,
            sessions: Vec::new(),
        }
    }

    pub fn reserve_session_id(&mut self) -> ChatSessionId {
        let id = self.next_session_id;
        self.next_session_id = self.next_session_id.saturating_add(1).max(1);
        id
    }

    pub fn sessions(&self) -> &[ChatSessionView] {
        &self.sessions
    }

    pub fn session(&self, session_id: ChatSessionId) -> Option<&ChatSessionView> {
        self.sessions
            .iter()
            .find(|session| session.session_id == session_id)
    }

    pub fn session_mut(&mut self, session_id: ChatSessionId) -> Option<&mut ChatSessionView> {
        self.sessions
            .iter_mut()
            .find(|session| session.session_id == session_id)
    }

    pub fn push_session(&mut self, session: ChatSessionView) {
        self.sessions.push(session);
    }

    pub fn remove_session(&mut self, session_id: ChatSessionId) -> Option<ChatSessionView> {
        let index = self
            .sessions
            .iter()
            .position(|session| session.session_id == session_id)?;
        Some(self.sessions.remove(index))
    }

    pub fn persist_session<S: ChatStore>(
        &self,
        store: &mut S,
        session_id: ChatSessionId,
    ) -> anyhow::Result<()> {
        let Some(session) = self.session(session_id) else {
            return Ok(());
        };
        store.save_server(session.server.clone())?;
        for room in &session.rooms {
            store.save_room(room.clone())?;
        }
        store.save_room(session.active_room.clone())?;
        store.set_active_room(&session.server.server_id, session.active_room.room_id)?;
        if session.active_room.joined {
            store.replace_userlist(
                &session.server.server_id,
                session.active_room.room_id,
                session.users.clone(),
            )?;
        } else {
            store.replace_userlist(
                &session.server.server_id,
                session.active_room.room_id,
                Vec::new(),
            )?;
        }
        store.append_events(session.events.clone())?;
        Ok(())
    }

    pub fn prepend_history_events(
        &mut self,
        session_id: ChatSessionId,
        events: Vec<ChatEvent>,
    ) -> usize {
        if events.is_empty() {
            return 0;
        }
        let Some(session) = self.session_mut(session_id) else {
            return 0;
        };
        let before = session.events.len();
        for event in events {
            if !session.events.iter().any(|existing| {
                existing.room_id == event.room_id && existing.event_id == event.event_id
            }) {
                session.events.push(event);
            }
        }
        session
            .events
            .sort_by_key(|event| (event.room_id, event.event_id));
        let added = session.events.len().saturating_sub(before);
        if added > 0 {
            session.status = format!("loaded {added} older cached event(s)");
        }
        added
    }

    pub fn load_cached_history_before<S: ChatStore>(
        &mut self,
        store: &S,
        session_id: ChatSessionId,
        limit: usize,
    ) -> anyhow::Result<usize> {
        let Some(session) = self.session(session_id) else {
            return Ok(0);
        };
        let server_id = session.server.server_id.clone();
        let room_id = session.active_room.room_id;
        let before = session
            .events
            .iter()
            .filter(|event| event.room_id == room_id)
            .map(|event| event.event_id)
            .min()
            .unwrap_or(EventId::MAX);
        let events = store.events_before(&server_id, room_id, before, limit)?;
        Ok(self.prepend_history_events(session_id, events))
    }

    pub fn load_cached_room_history<S: ChatStore>(
        &mut self,
        store: &S,
        session_id: ChatSessionId,
        limit: usize,
    ) -> anyhow::Result<usize> {
        let Some(session) = self.session(session_id) else {
            return Ok(0);
        };
        let server_id = session.server.server_id.clone();
        let room_id = session.active_room.room_id;
        let events = store.latest_events(&server_id, room_id, limit)?;
        Ok(self.prepend_history_events(session_id, events))
    }

    pub fn restore_from_store<S: ChatStore>(
        &mut self,
        store: &S,
        event_limit: usize,
    ) -> anyhow::Result<usize> {
        let mut restored = 0;
        for server in store.saved_servers()? {
            if !is_restorable_server_destination(&server.destination) {
                continue;
            }
            let rooms = store.rooms_for_server(&server.server_id)?;
            let Some(room) =
                restore_room_for_server(store, &server.server_id, &rooms, event_limit)?
            else {
                continue;
            };
            let users = store.users_for_room(&server.server_id, room.room_id)?;
            let mut events = Vec::new();
            for known_room in &rooms {
                events.extend(store.latest_events(
                    &server.server_id,
                    known_room.room_id,
                    event_limit,
                )?);
            }
            events.sort_by_key(|event| (event.room_id, event.event_id));
            events.dedup_by_key(|event| (event.room_id, event.event_id));
            let session_id = self.reserve_session_id();
            self.push_session(ChatSessionView {
                session_id,
                server,
                rooms,
                active_room: room,
                users,
                events,
                status: "restored from local cache".into(),
            });
            restored += 1;
        }
        Ok(restored)
    }
}

fn restore_room_for_server<S: ChatStore>(
    store: &S,
    server_id: &ServerId,
    rooms: &[ChatRoomSummary],
    event_limit: usize,
) -> anyhow::Result<Option<ChatRoomSummary>> {
    if let Some(active_room_id) = store.active_room_id(server_id)? {
        if let Some(room) = rooms
            .iter()
            .find(|room| room.room_id == active_room_id)
            .cloned()
        {
            return Ok(Some(room));
        }
    }
    if let Some(room) = rooms.iter().find(|room| room.joined).cloned() {
        return Ok(Some(room));
    }
    for room in rooms {
        if !store
            .latest_events(server_id, room.room_id, event_limit)?
            .is_empty()
        {
            let mut room = room.clone();
            room.joined = true;
            return Ok(Some(room));
        }
    }
    Ok(rooms.first().cloned().map(|mut room| {
        room.joined = true;
        room
    }))
}

pub fn is_restorable_server_destination(destination: &str) -> bool {
    let destination = destination.trim();
    destination.len() >= 32 && destination.chars().all(|ch| ch.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat::model::ChatEventKind;
    use crate::chat::store::SqliteChatStore;

    const TEST_DESTINATION: &str = "abcd1234abcd1234abcd1234abcd1234";

    #[test]
    fn client_persists_and_restores_sessions() {
        let mut store = SqliteChatStore::in_memory().expect("store");
        let mut client = ChatClient::new();
        let session_id = client.reserve_session_id();
        client.push_session(ChatSessionView {
            session_id,
            server: ChatServerSummary {
                server_id: "server-a".into(),
                destination: TEST_DESTINATION.into(),
                display_name: "Server A".into(),
            },
            active_room: ChatRoomSummary {
                server_id: "server-a".into(),
                room_id: 1,
                name: "lobby".into(),
                topic: Some("Lobby topic".into()),
                unread: 0,
                joined: true,
            },
            rooms: vec![
                ChatRoomSummary {
                    server_id: "server-a".into(),
                    room_id: 1,
                    name: "lobby".into(),
                    topic: Some("Lobby topic".into()),
                    unread: 0,
                    joined: true,
                },
                ChatRoomSummary {
                    server_id: "server-a".into(),
                    room_id: 2,
                    name: "support".into(),
                    topic: Some("Support topic".into()),
                    unread: 0,
                    joined: false,
                },
            ],
            users: vec![ChatUserSummary {
                server_id: "server-a".into(),
                user_id: 1,
                display_name: "Alice".into(),
                role_bits: 1,
                status_bits: 0,
                lxmf_available: true,
            }],
            events: vec![ChatEvent {
                server_id: "server-a".into(),
                room_id: 1,
                event_id: 1,
                actor_user_id: Some(1),
                actor_display_name: None,
                at_unix: 0,
                kind: ChatEventKind::Message {
                    body: "hello".into(),
                },
            }],
            status: "test".into(),
        });
        store
            .append_events(vec![ChatEvent {
                server_id: "server-a".into(),
                room_id: 2,
                event_id: 1,
                actor_user_id: Some(1),
                actor_display_name: Some("Alice".into()),
                at_unix: 1,
                kind: ChatEventKind::Message {
                    body: "support cache".into(),
                },
            }])
            .expect("support event");
        client
            .persist_session(&mut store, session_id)
            .expect("persist session");

        let mut restored = ChatClient::new();
        assert_eq!(
            restored
                .restore_from_store(&store, 50)
                .expect("restore sessions"),
            1
        );
        let session = restored.sessions().first().expect("restored session");
        assert_eq!(session.server.display_name, "Server A");
        assert_eq!(session.active_room.name, "lobby");
        assert!(session.active_room.joined);
        assert_eq!(session.rooms.len(), 2);
        assert_eq!(session.users[0].display_name, "Alice");
        assert_eq!(
            session
                .events
                .iter()
                .map(|event| (event.room_id, event.event_id))
                .collect::<Vec<_>>(),
            vec![(1, 1), (2, 1)]
        );
    }

    #[test]
    fn client_restore_ignores_unrestorable_dev_servers() {
        let mut store = SqliteChatStore::in_memory().expect("store");
        for (server_id, destination, display_name) in [
            ("mock-server", "mockchatdestination", "Mock OMENchat"),
            ("pending-server", "pending-omenchat-1", "New Chat"),
            ("real-server", TEST_DESTINATION, "Real OMENchat"),
        ] {
            store
                .save_server(ChatServerSummary {
                    server_id: server_id.into(),
                    destination: destination.into(),
                    display_name: display_name.into(),
                })
                .expect("save server");
            store
                .save_room(ChatRoomSummary {
                    server_id: server_id.into(),
                    room_id: 1,
                    name: "lobby".into(),
                    topic: None,
                    unread: 0,
                    joined: true,
                })
                .expect("save room");
        }

        let mut restored = ChatClient::new();
        assert_eq!(
            restored
                .restore_from_store(&store, 50)
                .expect("restore sessions"),
            1
        );
        let session = restored.sessions().first().expect("restored session");
        assert_eq!(session.server.display_name, "Real OMENchat");
        assert_eq!(session.server.destination, TEST_DESTINATION);
    }

    #[test]
    fn client_persistence_preserves_parted_active_room_state() {
        let mut store = SqliteChatStore::in_memory().expect("store");
        let mut client = ChatClient::new();
        let session_id = client.reserve_session_id();
        client.push_session(ChatSessionView {
            session_id,
            server: ChatServerSummary {
                server_id: "server-a".into(),
                destination: TEST_DESTINATION.into(),
                display_name: "Server A".into(),
            },
            active_room: ChatRoomSummary {
                server_id: "server-a".into(),
                room_id: 1,
                name: "lobby".into(),
                topic: None,
                unread: 0,
                joined: false,
            },
            rooms: vec![ChatRoomSummary {
                server_id: "server-a".into(),
                room_id: 1,
                name: "lobby".into(),
                topic: None,
                unread: 0,
                joined: true,
            }],
            users: vec![ChatUserSummary {
                server_id: "server-a".into(),
                user_id: 1,
                display_name: "Alice".into(),
                role_bits: 0,
                status_bits: 0,
                lxmf_available: true,
            }],
            events: Vec::new(),
            status: "left #lobby".into(),
        });

        client
            .persist_session(&mut store, session_id)
            .expect("persist parted session");

        let rooms = store
            .rooms_for_server(&"server-a".into())
            .expect("stored rooms");
        assert_eq!(rooms.len(), 1);
        assert!(!rooms[0].joined);
        assert!(store
            .users_for_room(&"server-a".into(), 1)
            .expect("stored userlist")
            .is_empty());
    }

    #[test]
    fn client_persistence_preserves_joined_rooms_and_active_room() {
        let mut store = SqliteChatStore::in_memory().expect("store");
        let mut client = ChatClient::new();
        let session_id = client.reserve_session_id();
        client.push_session(ChatSessionView {
            session_id,
            server: ChatServerSummary {
                server_id: "server-a".into(),
                destination: TEST_DESTINATION.into(),
                display_name: "Server A".into(),
            },
            active_room: ChatRoomSummary {
                server_id: "server-a".into(),
                room_id: 2,
                name: "support".into(),
                topic: None,
                unread: 0,
                joined: true,
            },
            rooms: vec![
                ChatRoomSummary {
                    server_id: "server-a".into(),
                    room_id: 1,
                    name: "lobby".into(),
                    topic: None,
                    unread: 0,
                    joined: true,
                },
                ChatRoomSummary {
                    server_id: "server-a".into(),
                    room_id: 2,
                    name: "support".into(),
                    topic: None,
                    unread: 0,
                    joined: true,
                },
            ],
            users: Vec::new(),
            events: Vec::new(),
            status: "joined support".into(),
        });

        client
            .persist_session(&mut store, session_id)
            .expect("persist switched session");

        let rooms = store
            .rooms_for_server(&"server-a".into())
            .expect("stored rooms");
        assert_eq!(
            store
                .active_room_id(&"server-a".into())
                .expect("active room"),
            Some(2)
        );
        assert_eq!(
            rooms
                .iter()
                .find(|room| room.name == "lobby")
                .map(|room| room.joined),
            Some(true)
        );
        assert_eq!(
            rooms
                .iter()
                .find(|room| room.name == "support")
                .map(|room| room.joined),
            Some(true)
        );
    }

    #[test]
    fn client_restore_prefers_persisted_active_room_over_joined_order() {
        let mut store = SqliteChatStore::in_memory().expect("store");
        let server = ChatServerSummary {
            server_id: "server-a".into(),
            destination: TEST_DESTINATION.into(),
            display_name: "Server A".into(),
        };
        store.save_server(server.clone()).expect("server");
        store
            .save_room(ChatRoomSummary {
                server_id: server.server_id.clone(),
                room_id: 1,
                name: "lobby".into(),
                topic: None,
                unread: 0,
                joined: true,
            })
            .expect("lobby");
        store
            .save_room(ChatRoomSummary {
                server_id: server.server_id.clone(),
                room_id: 2,
                name: "help".into(),
                topic: None,
                unread: 0,
                joined: true,
            })
            .expect("help");
        store
            .set_active_room(&server.server_id, 2)
            .expect("active room");

        let mut restored = ChatClient::new();
        assert_eq!(
            restored
                .restore_from_store(&store, 50)
                .expect("restore sessions"),
            1
        );
        let session = restored.sessions().first().expect("restored session");
        assert_eq!(session.active_room.name, "help");
    }

    #[test]
    fn client_restore_prefers_room_with_cached_events_when_join_marker_is_missing() {
        let mut store = SqliteChatStore::in_memory().expect("store");
        let server = ChatServerSummary {
            server_id: "server-a".into(),
            destination: TEST_DESTINATION.into(),
            display_name: "Server A".into(),
        };
        store.save_server(server.clone()).expect("server");
        store
            .save_room(ChatRoomSummary {
                server_id: server.server_id.clone(),
                room_id: 1,
                name: "lobby".into(),
                topic: None,
                unread: 0,
                joined: false,
            })
            .expect("lobby");
        store
            .save_room(ChatRoomSummary {
                server_id: server.server_id.clone(),
                room_id: 2,
                name: "support".into(),
                topic: None,
                unread: 0,
                joined: false,
            })
            .expect("support");
        store
            .append_events(vec![ChatEvent {
                server_id: server.server_id.clone(),
                room_id: 2,
                event_id: 7,
                actor_user_id: None,
                actor_display_name: None,
                at_unix: 0,
                kind: ChatEventKind::Message {
                    body: "cached support event".into(),
                },
            }])
            .expect("events");

        let mut restored = ChatClient::new();
        assert_eq!(
            restored
                .restore_from_store(&store, 50)
                .expect("restore sessions"),
            1
        );
        let session = restored.sessions().first().expect("restored session");
        assert_eq!(session.active_room.name, "support");
        assert_eq!(session.events.len(), 1);
    }

    #[test]
    fn client_loads_older_history_from_cached_store() {
        let mut store = SqliteChatStore::in_memory().expect("store");
        let server = ChatServerSummary {
            server_id: "server-a".into(),
            destination: TEST_DESTINATION.into(),
            display_name: "Server A".into(),
        };
        let room = ChatRoomSummary {
            server_id: server.server_id.clone(),
            room_id: 1,
            name: "lobby".into(),
            topic: None,
            unread: 0,
            joined: true,
        };
        store.save_server(server.clone()).expect("server");
        store.save_room(room.clone()).expect("room");
        store
            .append_events(vec![
                ChatEvent {
                    server_id: server.server_id.clone(),
                    room_id: room.room_id,
                    event_id: 1,
                    actor_user_id: None,
                    actor_display_name: None,
                    at_unix: 0,
                    kind: ChatEventKind::Message {
                        body: "older one".into(),
                    },
                },
                ChatEvent {
                    server_id: server.server_id.clone(),
                    room_id: room.room_id,
                    event_id: 2,
                    actor_user_id: None,
                    actor_display_name: None,
                    at_unix: 0,
                    kind: ChatEventKind::Message {
                        body: "older two".into(),
                    },
                },
            ])
            .expect("events");
        let mut client = ChatClient::new();
        let session_id = client.reserve_session_id();
        client.push_session(ChatSessionView {
            session_id,
            server,
            rooms: vec![room.clone()],
            active_room: room,
            users: Vec::new(),
            events: vec![ChatEvent {
                server_id: "server-a".into(),
                room_id: 1,
                event_id: 3,
                actor_user_id: None,
                actor_display_name: None,
                at_unix: 0,
                kind: ChatEventKind::Message {
                    body: "newest".into(),
                },
            }],
            status: String::new(),
        });

        assert_eq!(
            client
                .load_cached_history_before(&store, session_id, 50)
                .expect("load cached history"),
            2
        );
        let session = client.session(session_id).expect("session");
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
    fn client_loads_active_room_history_from_cached_store_after_room_switch() {
        let mut store = SqliteChatStore::in_memory().expect("store");
        let server = ChatServerSummary {
            server_id: "server-a".into(),
            destination: TEST_DESTINATION.into(),
            display_name: "Server A".into(),
        };
        let room = ChatRoomSummary {
            server_id: server.server_id.clone(),
            room_id: 2,
            name: "help".into(),
            topic: None,
            unread: 0,
            joined: true,
        };
        store.save_server(server.clone()).expect("server");
        store.save_room(room.clone()).expect("room");
        store
            .append_events(vec![ChatEvent {
                server_id: server.server_id.clone(),
                room_id: room.room_id,
                event_id: 9,
                actor_user_id: None,
                actor_display_name: None,
                at_unix: 0,
                kind: ChatEventKind::Message {
                    body: "cached help event".into(),
                },
            }])
            .expect("events");
        let mut client = ChatClient::new();
        let session_id = client.reserve_session_id();
        client.push_session(ChatSessionView {
            session_id,
            server,
            rooms: vec![room.clone()],
            active_room: room,
            users: Vec::new(),
            events: Vec::new(),
            status: String::new(),
        });

        assert_eq!(
            client
                .load_cached_room_history(&store, session_id, 50)
                .expect("load active room history"),
            1
        );
        let session = client.session(session_id).expect("session");
        assert_eq!(session.events.len(), 1);
        assert_eq!(session.events[0].event_id, 9);
    }

    #[test]
    fn client_cached_room_history_merge_does_not_drop_live_room_events() {
        let store = SqliteChatStore::in_memory().expect("store");
        let server = ChatServerSummary {
            server_id: "server-a".into(),
            destination: TEST_DESTINATION.into(),
            display_name: "Server A".into(),
        };
        let room = ChatRoomSummary {
            server_id: server.server_id.clone(),
            room_id: 1,
            name: "lobby".into(),
            topic: None,
            unread: 0,
            joined: true,
        };
        let mut client = ChatClient::new();
        let session_id = client.reserve_session_id();
        client.push_session(ChatSessionView {
            session_id,
            server,
            rooms: vec![room.clone()],
            active_room: room,
            users: Vec::new(),
            events: vec![ChatEvent {
                server_id: "server-a".into(),
                room_id: 1,
                event_id: 42,
                actor_user_id: None,
                actor_display_name: None,
                at_unix: 42,
                kind: ChatEventKind::Message {
                    body: "live history row".into(),
                },
            }],
            status: String::new(),
        });

        assert_eq!(
            client
                .load_cached_room_history(&store, session_id, 50)
                .expect("load active room history"),
            0
        );
        let session = client.session(session_id).expect("session");
        assert_eq!(session.events.len(), 1);
        assert_eq!(session.events[0].event_id, 42);
    }

    #[test]
    fn client_treats_cached_event_ids_as_room_scoped() {
        let mut store = SqliteChatStore::in_memory().expect("store");
        let server = ChatServerSummary {
            server_id: "server-a".into(),
            destination: TEST_DESTINATION.into(),
            display_name: "Server A".into(),
        };
        let lobby = ChatRoomSummary {
            server_id: server.server_id.clone(),
            room_id: 1,
            name: "lobby".into(),
            topic: None,
            unread: 0,
            joined: true,
        };
        let help = ChatRoomSummary {
            server_id: server.server_id.clone(),
            room_id: 2,
            name: "help".into(),
            topic: None,
            unread: 0,
            joined: false,
        };
        store.save_server(server.clone()).expect("server");
        store.save_room(lobby.clone()).expect("lobby");
        store.save_room(help.clone()).expect("help");
        store
            .append_events(vec![
                ChatEvent {
                    server_id: server.server_id.clone(),
                    room_id: 1,
                    event_id: 1,
                    actor_user_id: None,
                    actor_display_name: None,
                    at_unix: 0,
                    kind: ChatEventKind::Message {
                        body: "lobby one".into(),
                    },
                },
                ChatEvent {
                    server_id: server.server_id.clone(),
                    room_id: 2,
                    event_id: 1,
                    actor_user_id: None,
                    actor_display_name: None,
                    at_unix: 0,
                    kind: ChatEventKind::Message {
                        body: "help one".into(),
                    },
                },
            ])
            .expect("events");
        let mut client = ChatClient::new();
        let session_id = client.reserve_session_id();
        client.push_session(ChatSessionView {
            session_id,
            server,
            rooms: vec![lobby.clone(), help.clone()],
            active_room: lobby,
            users: Vec::new(),
            events: vec![ChatEvent {
                server_id: "server-a".into(),
                room_id: 2,
                event_id: 1,
                actor_user_id: None,
                actor_display_name: None,
                at_unix: 0,
                kind: ChatEventKind::Message {
                    body: "stale help row".into(),
                },
            }],
            status: String::new(),
        });

        assert_eq!(
            client
                .load_cached_room_history(&store, session_id, 50)
                .expect("load lobby history"),
            1
        );

        let session = client.session(session_id).expect("session");
        let mut event_keys = session
            .events
            .iter()
            .map(|event| (event.room_id, event.event_id))
            .collect::<Vec<_>>();
        event_keys.sort_unstable();
        assert_eq!(event_keys, vec![(1, 1), (2, 1)]);
    }

    #[test]
    fn client_load_older_history_uses_active_room_event_floor() {
        let mut store = SqliteChatStore::in_memory().expect("store");
        let server = ChatServerSummary {
            server_id: "server-a".into(),
            destination: TEST_DESTINATION.into(),
            display_name: "Server A".into(),
        };
        let lobby = ChatRoomSummary {
            server_id: server.server_id.clone(),
            room_id: 1,
            name: "lobby".into(),
            topic: None,
            unread: 0,
            joined: false,
        };
        let help = ChatRoomSummary {
            server_id: server.server_id.clone(),
            room_id: 2,
            name: "help".into(),
            topic: None,
            unread: 0,
            joined: true,
        };
        store.save_server(server.clone()).expect("server");
        store.save_room(lobby.clone()).expect("lobby");
        store.save_room(help.clone()).expect("help");
        store
            .append_events(vec![
                ChatEvent {
                    server_id: server.server_id.clone(),
                    room_id: 2,
                    event_id: 8,
                    actor_user_id: None,
                    actor_display_name: Some("Alice".into()),
                    at_unix: 8,
                    kind: ChatEventKind::Message {
                        body: "older help".into(),
                    },
                },
                ChatEvent {
                    server_id: server.server_id.clone(),
                    room_id: 2,
                    event_id: 9,
                    actor_user_id: None,
                    actor_display_name: Some("Alice".into()),
                    at_unix: 9,
                    kind: ChatEventKind::Message {
                        body: "also older help".into(),
                    },
                },
                ChatEvent {
                    server_id: server.server_id.clone(),
                    room_id: 2,
                    event_id: 10,
                    actor_user_id: None,
                    actor_display_name: Some("Alice".into()),
                    at_unix: 10,
                    kind: ChatEventKind::Message {
                        body: "loaded help".into(),
                    },
                },
            ])
            .expect("events");

        let mut client = ChatClient::new();
        let session_id = client.reserve_session_id();
        client.push_session(ChatSessionView {
            session_id,
            server,
            rooms: vec![lobby.clone(), help.clone()],
            active_room: help,
            users: Vec::new(),
            events: vec![
                ChatEvent {
                    server_id: "server-a".into(),
                    room_id: 1,
                    event_id: 1,
                    actor_user_id: None,
                    actor_display_name: Some("Bob".into()),
                    at_unix: 1,
                    kind: ChatEventKind::Message {
                        body: "old lobby should not set help floor".into(),
                    },
                },
                ChatEvent {
                    server_id: "server-a".into(),
                    room_id: 2,
                    event_id: 10,
                    actor_user_id: None,
                    actor_display_name: Some("Alice".into()),
                    at_unix: 10,
                    kind: ChatEventKind::Message {
                        body: "loaded help".into(),
                    },
                },
            ],
            status: String::new(),
        });

        assert_eq!(
            client
                .load_cached_history_before(&store, session_id, 50)
                .expect("load older active room history"),
            2
        );
        let session = client.session(session_id).expect("session");
        assert_eq!(
            session
                .events
                .iter()
                .filter(|event| event.room_id == 2)
                .map(|event| event.event_id)
                .collect::<Vec<_>>(),
            vec![8, 9, 10]
        );
    }
}
