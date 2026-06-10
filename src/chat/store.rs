use super::model::{ChatEvent, ChatEventKind, ChatRoomSummary, ChatServerSummary, ChatUserSummary};
use super::protocol::{EventId, RoomId, ServerId};

pub trait ChatStore {
    fn save_server(&mut self, server: ChatServerSummary) -> anyhow::Result<()>;
    fn saved_servers(&self) -> anyhow::Result<Vec<ChatServerSummary>>;
    fn delete_server(&mut self, server_id: &ServerId) -> anyhow::Result<bool>;
    fn set_active_room(&mut self, server_id: &ServerId, room_id: RoomId) -> anyhow::Result<()>;
    fn active_room_id(&self, server_id: &ServerId) -> anyhow::Result<Option<RoomId>>;
    fn save_room(&mut self, room: ChatRoomSummary) -> anyhow::Result<()>;
    fn rooms_for_server(&self, server_id: &ServerId) -> anyhow::Result<Vec<ChatRoomSummary>>;
    fn replace_userlist(
        &mut self,
        server_id: &ServerId,
        room_id: RoomId,
        users: Vec<ChatUserSummary>,
    ) -> anyhow::Result<()>;
    fn users_for_room(
        &self,
        server_id: &ServerId,
        room_id: RoomId,
    ) -> anyhow::Result<Vec<ChatUserSummary>>;
    fn append_events(&mut self, events: Vec<ChatEvent>) -> anyhow::Result<()>;
    fn latest_events(
        &self,
        server_id: &ServerId,
        room_id: RoomId,
        limit: usize,
    ) -> anyhow::Result<Vec<ChatEvent>>;
    fn events_before(
        &self,
        server_id: &ServerId,
        room_id: RoomId,
        before_event_id: EventId,
        limit: usize,
    ) -> anyhow::Result<Vec<ChatEvent>>;
}

pub struct SqliteChatStore {
    connection: rusqlite::Connection,
}

impl SqliteChatStore {
    pub fn open(path: impl AsRef<std::path::Path>) -> anyhow::Result<Self> {
        if let Some(parent) = path.as_ref().parent() {
            std::fs::create_dir_all(parent)?;
        }
        let connection = rusqlite::Connection::open(path)?;
        let store = Self { connection };
        store.migrate()?;
        Ok(store)
    }

    pub fn in_memory() -> anyhow::Result<Self> {
        let store = Self {
            connection: rusqlite::Connection::open_in_memory()?,
        };
        store.migrate()?;
        Ok(store)
    }

    fn migrate(&self) -> anyhow::Result<()> {
        self.connection
            .execute_batch(include_str!("migrations/001_init.sql"))?;
        let _ = self.connection.execute(
            "ALTER TABLE room_events ADD COLUMN actor_display_name TEXT",
            [],
        );
        let _ = self.connection.execute(
            "ALTER TABLE saved_servers ADD COLUMN active_room_id INTEGER",
            [],
        );
        Ok(())
    }
}

impl ChatStore for SqliteChatStore {
    fn save_server(&mut self, server: ChatServerSummary) -> anyhow::Result<()> {
        self.connection.execute(
            "INSERT INTO saved_servers(server_id, destination, display_name, created_at)
             VALUES (?1, ?2, ?3, 0)
             ON CONFLICT(server_id) DO UPDATE SET
               destination = excluded.destination,
               display_name = excluded.display_name",
            (&server.server_id, &server.destination, &server.display_name),
        )?;
        Ok(())
    }

    fn saved_servers(&self) -> anyhow::Result<Vec<ChatServerSummary>> {
        let mut statement = self.connection.prepare(
            "SELECT server_id, destination, display_name FROM saved_servers ORDER BY display_name",
        )?;
        let rows = statement.query_map([], |row| {
            Ok(ChatServerSummary {
                server_id: row.get(0)?,
                destination: row.get(1)?,
                display_name: row.get(2)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    fn delete_server(&mut self, server_id: &ServerId) -> anyhow::Result<bool> {
        let transaction = self.connection.transaction()?;
        transaction.execute("DELETE FROM drafts WHERE server_id = ?1", [server_id])?;
        transaction.execute(
            "DELETE FROM history_ranges WHERE server_id = ?1",
            [server_id],
        )?;
        transaction.execute("DELETE FROM room_events WHERE server_id = ?1", [server_id])?;
        transaction.execute(
            "DELETE FROM room_userlist WHERE server_id = ?1",
            [server_id],
        )?;
        transaction.execute("DELETE FROM users WHERE server_id = ?1", [server_id])?;
        transaction.execute("DELETE FROM rooms WHERE server_id = ?1", [server_id])?;
        let deleted = transaction.execute(
            "DELETE FROM saved_servers WHERE server_id = ?1",
            [server_id],
        )?;
        transaction.commit()?;
        Ok(deleted > 0)
    }

    fn set_active_room(&mut self, server_id: &ServerId, room_id: RoomId) -> anyhow::Result<()> {
        self.connection.execute(
            "UPDATE saved_servers SET active_room_id = ?2 WHERE server_id = ?1",
            (server_id, room_id as i64),
        )?;
        Ok(())
    }

    fn active_room_id(&self, server_id: &ServerId) -> anyhow::Result<Option<RoomId>> {
        let mut statement = self
            .connection
            .prepare("SELECT active_room_id FROM saved_servers WHERE server_id = ?1")?;
        let mut rows = statement.query([server_id])?;
        let Some(row) = rows.next()? else {
            return Ok(None);
        };
        Ok(row
            .get::<_, Option<i64>>(0)?
            .map(|room_id| room_id as RoomId))
    }

    fn save_room(&mut self, room: ChatRoomSummary) -> anyhow::Result<()> {
        self.connection.execute(
            "INSERT INTO rooms(server_id, room_id, name, topic, joined)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(server_id, room_id) DO UPDATE SET
               name = excluded.name,
               topic = excluded.topic,
               joined = excluded.joined",
            (
                &room.server_id,
                room.room_id,
                &room.name,
                room.topic.as_deref(),
                if room.joined { 1_i64 } else { 0_i64 },
            ),
        )?;
        Ok(())
    }

    fn rooms_for_server(&self, server_id: &ServerId) -> anyhow::Result<Vec<ChatRoomSummary>> {
        let mut statement = self.connection.prepare(
            "SELECT server_id, room_id, name, topic, joined
             FROM rooms
             WHERE server_id = ?1
             ORDER BY name",
        )?;
        let rows = statement.query_map([server_id], |row| {
            Ok(ChatRoomSummary {
                server_id: row.get(0)?,
                room_id: row.get::<_, i64>(1)? as RoomId,
                name: row.get(2)?,
                topic: row.get(3)?,
                unread: 0,
                joined: row.get::<_, i64>(4)? != 0,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    fn replace_userlist(
        &mut self,
        server_id: &ServerId,
        room_id: RoomId,
        users: Vec<ChatUserSummary>,
    ) -> anyhow::Result<()> {
        let transaction = self.connection.transaction()?;
        transaction.execute(
            "DELETE FROM room_userlist WHERE server_id = ?1 AND room_id = ?2",
            (server_id, room_id),
        )?;
        for user in users {
            transaction.execute(
                "INSERT INTO users(server_id, user_id, display_name, role_bits, status_bits, lxmf_available)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                 ON CONFLICT(server_id, user_id) DO UPDATE SET
                   display_name = excluded.display_name,
                   role_bits = excluded.role_bits,
                   status_bits = excluded.status_bits,
                   lxmf_available = excluded.lxmf_available",
                (
                    &user.server_id,
                    user.user_id,
                    &user.display_name,
                    user.role_bits as i64,
                    user.status_bits as i64,
                    if user.lxmf_available { 1_i64 } else { 0_i64 },
                ),
            )?;
            transaction.execute(
                "INSERT OR REPLACE INTO room_userlist(server_id, room_id, user_id, role_bits)
                 VALUES (?1, ?2, ?3, ?4)",
                (server_id, room_id, user.user_id, user.role_bits as i64),
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    fn users_for_room(
        &self,
        server_id: &ServerId,
        room_id: RoomId,
    ) -> anyhow::Result<Vec<ChatUserSummary>> {
        let mut statement = self.connection.prepare(
            "SELECT u.server_id, u.user_id, u.display_name, u.role_bits, u.status_bits, u.lxmf_available
             FROM room_userlist r
             JOIN users u ON u.server_id = r.server_id AND u.user_id = r.user_id
             WHERE r.server_id = ?1 AND r.room_id = ?2
             ORDER BY u.display_name",
        )?;
        let rows = statement.query_map((server_id, room_id), |row| {
            Ok(ChatUserSummary {
                server_id: row.get(0)?,
                user_id: row.get::<_, i64>(1)? as u32,
                display_name: row.get(2)?,
                role_bits: row.get::<_, i64>(3)? as u64,
                status_bits: row.get::<_, i64>(4)? as u32,
                lxmf_available: row.get::<_, i64>(5)? != 0,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    fn append_events(&mut self, events: Vec<ChatEvent>) -> anyhow::Result<()> {
        let transaction = self.connection.transaction()?;
        for event in events {
            let (kind, payload) = encode_event_kind(&event.kind);
            transaction.execute(
                "INSERT INTO room_events(
                   server_id, room_id, event_id, event_kind, actor_user_id, actor_display_name, at, payload
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                 ON CONFLICT(server_id, room_id, event_id) DO UPDATE SET
                   actor_user_id = COALESCE(excluded.actor_user_id, room_events.actor_user_id),
                   actor_display_name = COALESCE(excluded.actor_display_name, room_events.actor_display_name)",
                (
                    &event.server_id,
                    event.room_id,
                    event.event_id,
                    kind,
                    event.actor_user_id.map(i64::from),
                    event.actor_display_name.as_deref(),
                    event.at_unix,
                    payload,
                ),
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    fn latest_events(
        &self,
        server_id: &ServerId,
        room_id: RoomId,
        limit: usize,
    ) -> anyhow::Result<Vec<ChatEvent>> {
        let mut events = self.query_events(
            "SELECT server_id, room_id, event_id, event_kind, actor_user_id, actor_display_name, at, payload
             FROM room_events
             WHERE server_id = ?1 AND room_id = ?2 AND deleted = 0
             ORDER BY event_id DESC
             LIMIT ?3",
            (server_id, room_id, limit as i64),
        )?;
        events.reverse();
        Ok(events)
    }

    fn events_before(
        &self,
        server_id: &ServerId,
        room_id: RoomId,
        before_event_id: EventId,
        limit: usize,
    ) -> anyhow::Result<Vec<ChatEvent>> {
        let mut events = self.query_events(
            "SELECT server_id, room_id, event_id, event_kind, actor_user_id, actor_display_name, at, payload
             FROM room_events
             WHERE server_id = ?1 AND room_id = ?2 AND event_id < ?3 AND deleted = 0
             ORDER BY event_id DESC
             LIMIT ?4",
            (server_id, room_id, before_event_id, limit as i64),
        )?;
        events.reverse();
        Ok(events)
    }
}

impl SqliteChatStore {
    fn query_events<P>(&self, sql: &str, params: P) -> anyhow::Result<Vec<ChatEvent>>
    where
        P: rusqlite::Params,
    {
        let mut statement = self.connection.prepare(sql)?;
        let rows = statement.query_map(params, |row| {
            let kind: i64 = row.get(3)?;
            let payload: Option<String> = row.get(7)?;
            Ok(ChatEvent {
                server_id: row.get(0)?,
                room_id: row.get::<_, i64>(1)? as RoomId,
                event_id: row.get::<_, i64>(2)? as EventId,
                actor_user_id: row.get::<_, Option<i64>>(4)?.map(|value| value as u32),
                actor_display_name: row.get(5)?,
                at_unix: row.get(6)?,
                kind: decode_event_kind(kind, payload.unwrap_or_default()),
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }
}

fn encode_event_kind(kind: &ChatEventKind) -> (i64, String) {
    match kind {
        ChatEventKind::Message { body } => (1, body.clone()),
        ChatEventKind::Action { body } => (2, body.clone()),
        ChatEventKind::Notice { body } => (3, body.clone()),
        ChatEventKind::System { body } => (4, body.clone()),
        ChatEventKind::Upload {
            resource_id,
            filename,
            bytes,
        } => (
            5,
            format!(
                "{}\u{1f}{}\u{1f}{}",
                resource_id,
                filename.replace('\u{1f}', "_"),
                bytes
            ),
        ),
    }
}

fn decode_event_kind(kind: i64, body: String) -> ChatEventKind {
    match kind {
        1 => ChatEventKind::Message { body },
        2 => ChatEventKind::Action { body },
        3 => ChatEventKind::Notice { body },
        5 => decode_upload_event_kind(&body).unwrap_or(ChatEventKind::System { body }),
        _ => ChatEventKind::System { body },
    }
}

fn decode_upload_event_kind(body: &str) -> Option<ChatEventKind> {
    let mut parts = body.splitn(3, '\u{1f}');
    let resource_id = parts.next()?.trim().to_owned();
    let filename = parts.next()?.trim().to_owned();
    let bytes = parts.next()?.trim().parse::<u64>().ok()?;
    if resource_id.is_empty() || filename.is_empty() {
        return None;
    }
    Some(ChatEventKind::Upload {
        resource_id,
        filename,
        bytes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_server() -> ChatServerSummary {
        ChatServerSummary {
            server_id: "server-a".into(),
            destination: "abcd1234".into(),
            display_name: "Server A".into(),
        }
    }

    #[test]
    fn sqlite_store_saves_server_room_and_events_idempotently() {
        let mut store = SqliteChatStore::in_memory().expect("store");
        let server = sample_server();
        store.save_server(server.clone()).expect("save server");
        store
            .save_room(ChatRoomSummary {
                server_id: server.server_id.clone(),
                room_id: 1,
                name: "lobby".into(),
                topic: Some("Lobby topic".into()),
                unread: 0,
                joined: true,
            })
            .expect("save room");

        let event = ChatEvent {
            server_id: server.server_id.clone(),
            room_id: 1,
            event_id: 10,
            actor_user_id: Some(7),
            actor_display_name: None,
            at_unix: 42,
            kind: ChatEventKind::Message {
                body: "hello".into(),
            },
        };
        store
            .append_events(vec![event.clone(), event])
            .expect("append events");
        store
            .append_events(vec![ChatEvent {
                server_id: server.server_id.clone(),
                room_id: 1,
                event_id: 10,
                actor_user_id: Some(7),
                actor_display_name: Some("Alice".into()),
                at_unix: 42,
                kind: ChatEventKind::Message {
                    body: "hello".into(),
                },
            }])
            .expect("backfill display name");

        assert_eq!(
            store.saved_servers().expect("servers"),
            vec![server.clone()]
        );
        assert_eq!(
            store.rooms_for_server(&server.server_id).expect("rooms")[0]
                .topic
                .as_deref(),
            Some("Lobby topic")
        );
        let events = store
            .latest_events(&server.server_id, 1, 50)
            .expect("latest events");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_id, 10);
        assert_eq!(events[0].actor_display_name.as_deref(), Some("Alice"));
    }

    #[test]
    fn sqlite_store_replaces_userlist() {
        let mut store = SqliteChatStore::in_memory().expect("store");
        let server = sample_server();
        store.save_server(server.clone()).expect("save server");
        store
            .save_room(ChatRoomSummary {
                server_id: server.server_id.clone(),
                room_id: 1,
                name: "lobby".into(),
                topic: None,
                unread: 0,
                joined: true,
            })
            .expect("save room");

        store
            .replace_userlist(
                &server.server_id,
                1,
                vec![
                    ChatUserSummary {
                        server_id: server.server_id.clone(),
                        user_id: 1,
                        display_name: "Alice".into(),
                        role_bits: 1,
                        status_bits: 0,
                        lxmf_available: true,
                    },
                    ChatUserSummary {
                        server_id: server.server_id.clone(),
                        user_id: 2,
                        display_name: "Bob".into(),
                        role_bits: 0,
                        status_bits: 0,
                        lxmf_available: false,
                    },
                ],
            )
            .expect("replace userlist");

        let users = store
            .users_for_room(&server.server_id, 1)
            .expect("users for room");
        assert_eq!(users.len(), 2);
        assert_eq!(users[0].display_name, "Alice");

        store
            .replace_userlist(
                &server.server_id,
                1,
                vec![ChatUserSummary {
                    server_id: server.server_id.clone(),
                    user_id: 2,
                    display_name: "Bob".into(),
                    role_bits: 0,
                    status_bits: 0,
                    lxmf_available: false,
                }],
            )
            .expect("replace userlist again");
        let users = store
            .users_for_room(&server.server_id, 1)
            .expect("users for room");
        assert_eq!(users.len(), 1);
        assert_eq!(users[0].display_name, "Bob");
    }

    #[test]
    fn sqlite_store_deletes_server_owned_rows() {
        let mut store = SqliteChatStore::in_memory().expect("store");
        let server = sample_server();
        store.save_server(server.clone()).expect("save server");
        store
            .save_room(ChatRoomSummary {
                server_id: server.server_id.clone(),
                room_id: 1,
                name: "lobby".into(),
                topic: None,
                unread: 0,
                joined: true,
            })
            .expect("save room");
        store
            .replace_userlist(
                &server.server_id,
                1,
                vec![ChatUserSummary {
                    server_id: server.server_id.clone(),
                    user_id: 1,
                    display_name: "Alice".into(),
                    role_bits: 1,
                    status_bits: 0,
                    lxmf_available: true,
                }],
            )
            .expect("users");
        store
            .append_events(vec![ChatEvent {
                server_id: server.server_id.clone(),
                room_id: 1,
                event_id: 1,
                actor_user_id: Some(1),
                actor_display_name: Some("Alice".into()),
                at_unix: 1,
                kind: ChatEventKind::Message { body: "hi".into() },
            }])
            .expect("event");

        assert!(store
            .delete_server(&server.server_id)
            .expect("delete server"));
        assert!(store.saved_servers().expect("servers").is_empty());
        assert!(store
            .rooms_for_server(&server.server_id)
            .expect("rooms")
            .is_empty());
        assert!(store
            .latest_events(&server.server_id, 1, 50)
            .expect("events")
            .is_empty());
        assert!(store
            .users_for_room(&server.server_id, 1)
            .expect("users")
            .is_empty());
    }
}
