use super::model::{
    ChatEvent, ChatEventKind, ChatMessageMetadata, ChatRoomSummary, ChatServerSummary,
    ChatUserSummary, CHAT_CLIENT_MAX_SESSIONS, CHAT_ROOM_NAME_MAX_BYTES, CHAT_ROOM_TOPIC_MAX_BYTES,
    CHAT_SERVER_DESTINATION_MAX_BYTES, CHAT_SERVER_DISPLAY_MAX_BYTES, CHAT_SERVER_ID_MAX_BYTES,
    CHAT_SESSION_MAX_ROOMS, CHAT_SESSION_MAX_ROOM_BYTES, CHAT_SESSION_MAX_USERS,
    CHAT_SESSION_MAX_USER_BYTES, CHAT_USER_DISPLAY_MAX_BYTES,
};
use super::protocol::{EventId, RoomId, ServerId, UserId};
use anyhow::Context;

pub trait ChatStore {
    fn save_server(&mut self, server: ChatServerSummary) -> anyhow::Result<()>;
    fn saved_servers(&self) -> anyhow::Result<Vec<ChatServerSummary>>;
    fn delete_server(&mut self, server_id: &ServerId) -> anyhow::Result<bool>;
    fn set_active_room(&mut self, server_id: &ServerId, room_id: RoomId) -> anyhow::Result<()>;
    fn active_room_id(&self, server_id: &ServerId) -> anyhow::Result<Option<RoomId>>;
    fn set_local_user_id(
        &mut self,
        server_id: &ServerId,
        user_id: Option<UserId>,
    ) -> anyhow::Result<()>;
    fn local_user_id(&self, server_id: &ServerId) -> anyhow::Result<Option<UserId>>;
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
        self.add_column_if_missing(
            "room_events",
            "reply_to_event_id",
            "ALTER TABLE room_events ADD COLUMN reply_to_event_id INTEGER",
        )?;
        self.add_column_if_missing(
            "room_events",
            "mention_user_ids",
            "ALTER TABLE room_events ADD COLUMN mention_user_ids BLOB",
        )?;
        self.add_column_if_missing(
            "saved_servers",
            "local_user_id",
            "ALTER TABLE saved_servers ADD COLUMN local_user_id INTEGER",
        )?;
        Ok(())
    }

    fn add_column_if_missing(
        &self,
        table: &str,
        column: &str,
        statement: &str,
    ) -> anyhow::Result<()> {
        let exists = self.connection.query_row(
            "SELECT EXISTS(
               SELECT 1 FROM pragma_table_info(?1) WHERE name = ?2
             )",
            (table, column),
            |row| row.get::<_, bool>(0),
        )?;
        if !exists {
            self.connection.execute(statement, [])?;
        }
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
            "SELECT server_id, destination, display_name
             FROM saved_servers
             WHERE length(CAST(server_id AS BLOB)) <= ?2
               AND length(CAST(destination AS BLOB)) <= ?3
               AND length(CAST(display_name AS BLOB)) <= ?4
             ORDER BY display_name
             LIMIT ?1",
        )?;
        let rows = statement.query_map(
            (
                CHAT_CLIENT_MAX_SESSIONS as i64,
                CHAT_SERVER_ID_MAX_BYTES as i64,
                CHAT_SERVER_DESTINATION_MAX_BYTES as i64,
                CHAT_SERVER_DISPLAY_MAX_BYTES as i64,
            ),
            |row| {
                Ok(ChatServerSummary {
                    server_id: row.get(0)?,
                    destination: row.get(1)?,
                    display_name: row.get(2)?,
                })
            },
        )?;
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

    fn set_local_user_id(
        &mut self,
        server_id: &ServerId,
        user_id: Option<UserId>,
    ) -> anyhow::Result<()> {
        let updated = self.connection.execute(
            "UPDATE saved_servers SET local_user_id = ?2 WHERE server_id = ?1",
            (server_id, user_id.map(i64::from)),
        )?;
        anyhow::ensure!(
            updated == 1,
            "cannot bind an OMENchat local user to an unknown server"
        );
        Ok(())
    }

    fn local_user_id(&self, server_id: &ServerId) -> anyhow::Result<Option<UserId>> {
        let value = self.connection.query_row(
            "SELECT local_user_id FROM saved_servers WHERE server_id = ?1",
            [server_id],
            |row| row.get::<_, Option<i64>>(0),
        )?;
        value
            .map(|value| {
                u32::try_from(value)
                    .ok()
                    .filter(|value| *value != 0)
                    .context("stored OMENchat local user id must be a positive u32")
            })
            .transpose()
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
               AND length(server_id) + length(name) + COALESCE(length(topic), 0) <= ?2
               AND length(CAST(name AS BLOB)) <= ?4
               AND COALESCE(length(CAST(topic AS BLOB)), 0) <= ?5
             ORDER BY
               room_id = COALESCE(
                 (SELECT active_room_id FROM saved_servers WHERE server_id = ?1), -1
               ) DESC,
               joined DESC,
               name
             LIMIT ?3",
        )?;
        let rows = statement.query_map(
            (
                server_id,
                CHAT_SESSION_MAX_ROOM_BYTES as i64,
                CHAT_SESSION_MAX_ROOMS as i64,
                CHAT_ROOM_NAME_MAX_BYTES as i64,
                CHAT_ROOM_TOPIC_MAX_BYTES as i64,
            ),
            |row| {
                Ok(ChatRoomSummary {
                    server_id: row.get(0)?,
                    room_id: row.get::<_, i64>(1)? as RoomId,
                    name: row.get(2)?,
                    topic: row.get(3)?,
                    unread: 0,
                    joined: row.get::<_, i64>(4)? != 0,
                })
            },
        )?;
        let mut rooms = Vec::with_capacity(CHAT_SESSION_MAX_ROOMS);
        let mut retained_bytes = 0_usize;
        for room in rows {
            let room = room?;
            let room_bytes = std::mem::size_of::<ChatRoomSummary>()
                .saturating_add(room.server_id.len())
                .saturating_add(room.name.len())
                .saturating_add(room.topic.as_ref().map_or(0, String::len));
            if retained_bytes.saturating_add(room_bytes) > CHAT_SESSION_MAX_ROOM_BYTES {
                break;
            }
            retained_bytes = retained_bytes.saturating_add(room_bytes);
            rooms.push(room);
        }
        Ok(rooms)
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
               AND length(u.server_id) + length(u.display_name) <= ?3
               AND length(CAST(u.display_name AS BLOB)) <= ?5
             ORDER BY u.display_name
             LIMIT ?4",
        )?;
        let rows = statement.query_map(
            (
                server_id,
                room_id,
                CHAT_SESSION_MAX_USER_BYTES as i64,
                CHAT_SESSION_MAX_USERS as i64,
                CHAT_USER_DISPLAY_MAX_BYTES as i64,
            ),
            |row| {
                Ok(ChatUserSummary {
                    server_id: row.get(0)?,
                    user_id: row.get::<_, i64>(1)? as u32,
                    display_name: row.get(2)?,
                    role_bits: row.get::<_, i64>(3)? as u64,
                    status_bits: row.get::<_, i64>(4)? as u32,
                    lxmf_available: row.get::<_, i64>(5)? != 0,
                })
            },
        )?;
        let mut users = Vec::with_capacity(CHAT_SESSION_MAX_USERS);
        let mut retained_bytes = 0_usize;
        for user in rows {
            let user = user?;
            let user_bytes = std::mem::size_of::<ChatUserSummary>()
                .saturating_add(user.server_id.len())
                .saturating_add(user.display_name.len());
            if retained_bytes.saturating_add(user_bytes) > CHAT_SESSION_MAX_USER_BYTES {
                break;
            }
            retained_bytes = retained_bytes.saturating_add(user_bytes);
            users.push(user);
        }
        Ok(users)
    }

    fn append_events(&mut self, events: Vec<ChatEvent>) -> anyhow::Result<()> {
        let transaction = self.connection.transaction()?;
        for event in events {
            if is_transient_local_event_id(event.event_id) {
                continue;
            }
            let event_id = i64::try_from(event.event_id).with_context(|| {
                format!("event id {} cannot be stored in sqlite", event.event_id)
            })?;
            let (kind, payload, metadata) = encode_event_kind(&event.kind);
            if let Some(metadata) = metadata {
                omenchat_protocol::RichMessageEventMetadata {
                    reply_to_event_id: metadata.reply_to_event_id,
                    mentioned_user_ids: metadata.mentioned_user_ids.clone(),
                }
                .validate()
                .context("invalid OMENchat reply/mention metadata")?;
            }
            let mention_user_ids = metadata
                .as_ref()
                .map(|metadata| encode_mention_user_ids(&metadata.mentioned_user_ids));
            transaction.execute(
                "INSERT INTO room_events(
                   server_id, room_id, event_id, event_kind, actor_user_id, actor_display_name, at,
                   payload, reply_to_event_id, mention_user_ids
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
                 ON CONFLICT(server_id, room_id, event_id) DO UPDATE SET
                   actor_user_id = COALESCE(excluded.actor_user_id, room_events.actor_user_id),
                   actor_display_name = COALESCE(excluded.actor_display_name, room_events.actor_display_name),
                   reply_to_event_id = COALESCE(excluded.reply_to_event_id, room_events.reply_to_event_id),
                   mention_user_ids = COALESCE(excluded.mention_user_ids, room_events.mention_user_ids)",
                (
                    &event.server_id,
                    event.room_id,
                    event_id,
                    kind,
                    event.actor_user_id.map(i64::from),
                    event.actor_display_name.as_deref(),
                    event.at_unix,
                    payload,
                    metadata
                        .as_ref()
                        .and_then(|metadata| metadata.reply_to_event_id)
                        .and_then(|event_id| i64::try_from(event_id).ok()),
                    mention_user_ids,
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
            "SELECT server_id, room_id, event_id, event_kind, actor_user_id, actor_display_name, at,
                    payload, reply_to_event_id, mention_user_ids
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
            "SELECT server_id, room_id, event_id, event_kind, actor_user_id, actor_display_name, at,
                    payload, reply_to_event_id, mention_user_ids
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
            let reply_to_event_id = match row.get::<_, Option<i64>>(8)? {
                Some(value) => Some(
                    u64::try_from(value)
                        .ok()
                        .filter(|value| *value != 0)
                        .ok_or_else(|| {
                            invalid_metadata_column(8, "reply event id must be positive")
                        })?,
                ),
                None => None,
            };
            let mention_user_ids = match row.get::<_, Option<Vec<u8>>>(9)? {
                Some(bytes) => Some(
                    decode_mention_user_ids(&bytes)
                        .map_err(|message| invalid_metadata_column(9, message))?,
                ),
                None => None,
            };
            if reply_to_event_id.is_some() || mention_user_ids.is_some() {
                omenchat_protocol::RichMessageEventMetadata {
                    reply_to_event_id,
                    mentioned_user_ids: mention_user_ids.clone().unwrap_or_default(),
                }
                .validate()
                .map_err(|error| invalid_metadata_column(9, error.to_string()))?;
            }
            Ok(ChatEvent {
                server_id: row.get(0)?,
                room_id: row.get::<_, i64>(1)? as RoomId,
                event_id: row.get::<_, i64>(2)? as EventId,
                actor_user_id: row.get::<_, Option<i64>>(4)?.map(|value| value as u32),
                actor_display_name: row.get(5)?,
                at_unix: row.get(6)?,
                kind: decode_event_kind(
                    kind,
                    payload.unwrap_or_default(),
                    reply_to_event_id,
                    mention_user_ids,
                ),
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }
}

fn encode_event_kind(kind: &ChatEventKind) -> (i64, String, Option<&ChatMessageMetadata>) {
    match kind {
        ChatEventKind::Message { body } => (1, body.clone(), None),
        ChatEventKind::RichMessage { body, metadata } => (1, body.clone(), Some(metadata)),
        ChatEventKind::Action { body } => (2, body.clone(), None),
        ChatEventKind::Notice { body } => (3, body.clone(), None),
        ChatEventKind::System { body } => (4, body.clone(), None),
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
            None,
        ),
    }
}

fn decode_event_kind(
    kind: i64,
    body: String,
    reply_to_event_id: Option<EventId>,
    mentioned_user_ids: Option<Vec<u32>>,
) -> ChatEventKind {
    match kind {
        1 if reply_to_event_id.is_some()
            || mentioned_user_ids
                .as_ref()
                .is_some_and(|mentions| !mentions.is_empty()) =>
        {
            ChatEventKind::RichMessage {
                body,
                metadata: ChatMessageMetadata {
                    reply_to_event_id,
                    mentioned_user_ids: mentioned_user_ids.unwrap_or_default(),
                },
            }
        }
        1 => ChatEventKind::Message { body },
        2 => ChatEventKind::Action { body },
        3 => ChatEventKind::Notice { body },
        5 => decode_upload_event_kind(&body).unwrap_or(ChatEventKind::System { body }),
        _ => ChatEventKind::System { body },
    }
}

fn encode_mention_user_ids(user_ids: &[u32]) -> Vec<u8> {
    user_ids
        .iter()
        .flat_map(|user_id| user_id.to_be_bytes())
        .collect()
}

fn decode_mention_user_ids(bytes: &[u8]) -> Result<Vec<u32>, &'static str> {
    if bytes.len() > omenchat_protocol::RICH_MESSAGE_MAX_MENTIONS * 4 {
        return Err("mention metadata exceeds its byte limit");
    }
    if bytes.len() % 4 != 0 {
        return Err("mention metadata is not a sequence of u32 values");
    }
    let user_ids = bytes
        .chunks_exact(4)
        .map(|bytes| u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
        .collect::<Vec<_>>();
    Ok(user_ids)
}

fn invalid_metadata_column(column: usize, message: impl Into<String>) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        column,
        rusqlite::types::Type::Blob,
        Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            message.into(),
        )),
    )
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

fn is_transient_local_event_id(event_id: EventId) -> bool {
    event_id > u64::MAX.saturating_sub(1_000_000)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn isolated_store_path(label: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "omenbrowser-chat-store-{label}-{}-{}.sqlite",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system time")
                .as_nanos()
        ))
    }

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
    fn sqlite_catalog_reads_apply_item_and_byte_admission_limits() {
        let mut store = SqliteChatStore::in_memory().expect("store");
        for index in 0..=CHAT_CLIENT_MAX_SESSIONS {
            store
                .save_server(ChatServerSummary {
                    server_id: format!("server-{index:04}"),
                    destination: format!("destination-{index:04}"),
                    display_name: format!("Server {index:04}"),
                })
                .expect("save server");
        }
        assert_eq!(
            store.saved_servers().expect("bounded servers").len(),
            CHAT_CLIENT_MAX_SESSIONS
        );

        let server = ChatServerSummary {
            server_id: "catalog-server".into(),
            destination: "catalog-destination".into(),
            display_name: "Catalog Server".into(),
        };
        store
            .save_server(server.clone())
            .expect("save catalog server");
        for room_id in 1..=CHAT_SESSION_MAX_ROOMS as RoomId + 1 {
            store
                .save_room(ChatRoomSummary {
                    server_id: server.server_id.clone(),
                    room_id,
                    name: format!("room-{room_id:04}"),
                    topic: None,
                    unread: 0,
                    joined: room_id % 2 == 0,
                })
                .expect("save room");
        }
        let active_room_id = CHAT_SESSION_MAX_ROOMS as RoomId + 1;
        store
            .set_active_room(&server.server_id, active_room_id)
            .expect("set active room");
        let rooms = store
            .rooms_for_server(&server.server_id)
            .expect("bounded rooms");
        assert_eq!(rooms.len(), CHAT_SESSION_MAX_ROOMS);
        assert_eq!(rooms[0].room_id, active_room_id);

        let users = (1..=CHAT_SESSION_MAX_USERS as u32 + 1)
            .map(|user_id| ChatUserSummary {
                server_id: server.server_id.clone(),
                user_id,
                display_name: format!("user-{user_id:04}"),
                role_bits: 0,
                status_bits: 0,
                lxmf_available: false,
            })
            .collect();
        store
            .replace_userlist(&server.server_id, active_room_id, users)
            .expect("save users");
        assert_eq!(
            store
                .users_for_room(&server.server_id, active_room_id)
                .expect("bounded users")
                .len(),
            CHAT_SESSION_MAX_USERS
        );

        store
            .save_room(ChatRoomSummary {
                server_id: server.server_id.clone(),
                room_id: active_room_id + 1,
                name: "x".repeat(CHAT_SESSION_MAX_ROOM_BYTES + 1),
                topic: None,
                unread: 0,
                joined: true,
            })
            .expect("save oversized room");
        assert!(store
            .rooms_for_server(&server.server_id)
            .expect("rooms after oversized row")
            .iter()
            .all(|room| room.room_id != active_room_id + 1));
    }

    #[test]
    fn sqlite_presentation_metadata_reads_reject_oversized_rows() {
        let mut store = SqliteChatStore::in_memory().expect("store");
        let server = sample_server();
        store.save_server(server.clone()).expect("save server");
        store
            .save_server(ChatServerSummary {
                server_id: "oversized-server".into(),
                destination: "destination".into(),
                display_name: "☃".repeat(CHAT_SERVER_DISPLAY_MAX_BYTES),
            })
            .expect("save oversized server");
        assert_eq!(
            store.saved_servers().expect("bounded servers"),
            vec![server.clone()]
        );

        store
            .save_room(ChatRoomSummary {
                server_id: server.server_id.clone(),
                room_id: 1,
                name: "lobby".into(),
                topic: None,
                unread: 0,
                joined: true,
            })
            .expect("save valid room");
        store
            .save_room(ChatRoomSummary {
                server_id: server.server_id.clone(),
                room_id: 2,
                name: "r".repeat(CHAT_ROOM_NAME_MAX_BYTES + 1),
                topic: None,
                unread: 0,
                joined: false,
            })
            .expect("save oversized room name");
        store
            .save_room(ChatRoomSummary {
                server_id: server.server_id.clone(),
                room_id: 3,
                name: "topic".into(),
                topic: Some("t".repeat(CHAT_ROOM_TOPIC_MAX_BYTES + 1)),
                unread: 0,
                joined: false,
            })
            .expect("save oversized topic");
        assert_eq!(
            store
                .rooms_for_server(&server.server_id)
                .expect("bounded rooms")
                .len(),
            1
        );

        store
            .replace_userlist(
                &server.server_id,
                1,
                vec![
                    ChatUserSummary {
                        server_id: server.server_id.clone(),
                        user_id: 1,
                        display_name: "Alice".into(),
                        role_bits: 0,
                        status_bits: 0,
                        lxmf_available: false,
                    },
                    ChatUserSummary {
                        server_id: server.server_id.clone(),
                        user_id: 2,
                        display_name: "u".repeat(CHAT_USER_DISPLAY_MAX_BYTES + 1),
                        role_bits: 0,
                        status_bits: 0,
                        lxmf_available: false,
                    },
                ],
            )
            .expect("save users");
        assert_eq!(
            store
                .users_for_room(&server.server_id, 1)
                .expect("bounded users")
                .len(),
            1
        );
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

    #[test]
    fn rich_message_metadata_survives_client_store_restart() {
        let path = isolated_store_path("rich-restart");
        let event = ChatEvent {
            server_id: "server-a".into(),
            room_id: 7,
            event_id: 42,
            actor_user_id: Some(3),
            actor_display_name: Some("Alice".into()),
            at_unix: 99,
            kind: ChatEventKind::RichMessage {
                body: "reply".into(),
                metadata: ChatMessageMetadata {
                    reply_to_event_id: Some(41),
                    mentioned_user_ids: vec![2, 9],
                },
            },
        };
        {
            let mut store = SqliteChatStore::open(&path).expect("create store");
            store
                .append_events(vec![event.clone()])
                .expect("persist rich");
        }
        let store = SqliteChatStore::open(&path).expect("reopen store");
        assert_eq!(
            store
                .latest_events(&"server-a".into(), 7, 10)
                .expect("history"),
            vec![event]
        );
        drop(store);
        std::fs::remove_file(path).expect("remove store");
    }

    #[test]
    fn client_metadata_columns_preserve_legacy_history_during_migration() {
        let path = isolated_store_path("legacy-migration");
        {
            let connection = rusqlite::Connection::open(&path).expect("legacy database");
            connection
                .execute_batch(include_str!("migrations/001_init.sql"))
                .expect("legacy schema");
            connection
                .execute(
                    "INSERT INTO room_events(
                       server_id, room_id, event_id, event_kind, actor_user_id, at, payload
                     ) VALUES ('server-a', 1, 10, 1, 7, 99, 'legacy')",
                    [],
                )
                .expect("legacy event");
        }
        let store = SqliteChatStore::open(&path).expect("migrate legacy store");
        let events = store
            .latest_events(&"server-a".into(), 1, 10)
            .expect("legacy history");
        assert_eq!(events.len(), 1);
        assert_eq!(
            events[0].kind,
            ChatEventKind::Message {
                body: "legacy".into()
            }
        );
        let columns = store
            .connection
            .prepare("SELECT name FROM pragma_table_info('room_events')")
            .and_then(|mut statement| {
                statement
                    .query_map([], |row| row.get::<_, String>(0))?
                    .collect::<Result<Vec<_>, _>>()
            })
            .expect("columns");
        assert!(columns.iter().any(|column| column == "reply_to_event_id"));
        assert!(columns.iter().any(|column| column == "mention_user_ids"));
        assert!(store
            .connection
            .query_row(
                "SELECT EXISTS(
                   SELECT 1 FROM pragma_table_info('saved_servers')
                   WHERE name = 'local_user_id'
                 )",
                [],
                |row| row.get::<_, bool>(0),
            )
            .expect("saved server columns"));
        drop(store);
        std::fs::remove_file(path).expect("remove store");
    }

    #[test]
    fn malformed_client_metadata_fails_closed_instead_of_becoming_plain_text() {
        let store = SqliteChatStore::in_memory().expect("store");
        store
            .connection
            .execute(
                "INSERT INTO room_events(
                   server_id, room_id, event_id, event_kind, at, payload, mention_user_ids
                 ) VALUES ('server-a', 1, 1, 1, 1, 'hello', X'00000002FF')",
                [],
            )
            .expect("seed malformed metadata");
        assert!(store.latest_events(&"server-a".into(), 1, 10).is_err());
    }

    #[test]
    fn local_user_binding_is_server_scoped_and_survives_store_restart() {
        let path = isolated_store_path("local-user");
        {
            let mut store = SqliteChatStore::open(&path).expect("create store");
            store.save_server(sample_server()).expect("save server");
            store
                .set_local_user_id(&"server-a".into(), Some(7))
                .expect("bind user");
            assert_eq!(
                store.local_user_id(&"server-a".into()).expect("read user"),
                Some(7)
            );
        }
        let store = SqliteChatStore::open(&path).expect("reopen store");
        assert_eq!(
            store
                .local_user_id(&"server-a".into())
                .expect("restart user"),
            Some(7)
        );
        store
            .connection
            .execute(
                "UPDATE saved_servers SET local_user_id = -1 WHERE server_id = 'server-a'",
                [],
            )
            .expect("seed invalid user");
        assert!(store.local_user_id(&"server-a".into()).is_err());
        drop(store);
        std::fs::remove_file(path).expect("remove store");
    }
}
