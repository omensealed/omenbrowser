use super::model::{
    chat_message_revisions_fit_bounds, chat_pins_fit_bounds, chat_text_fits, ChatEvent,
    ChatEventKind, ChatMessageMetadata, ChatMessageRevision, ChatPin, ChatReaction,
    ChatRoomSummary, ChatServerSummary, ChatUserSummary, CHAT_CLIENT_MAX_SESSIONS,
    CHAT_PIN_MAX_BYTES, CHAT_PIN_MAX_ROWS, CHAT_PIN_MAX_ROWS_PER_SERVER, CHAT_REACTION_MAX_BYTES,
    CHAT_REACTION_MAX_BYTES_PER_ROOM, CHAT_REACTION_MAX_BYTES_PER_SERVER, CHAT_REACTION_MAX_ROWS,
    CHAT_REACTION_MAX_ROWS_PER_ROOM, CHAT_REACTION_MAX_ROWS_PER_SERVER,
    CHAT_REACTION_MAX_ROWS_PER_TARGET, CHAT_REACTION_MAX_TOKENS_PER_ACTOR_TARGET,
    CHAT_ROOM_NAME_MAX_BYTES, CHAT_ROOM_TOPIC_MAX_BYTES, CHAT_SERVER_DESTINATION_MAX_BYTES,
    CHAT_SERVER_DISPLAY_MAX_BYTES, CHAT_SERVER_ID_MAX_BYTES, CHAT_SESSION_MAX_ROOMS,
    CHAT_SESSION_MAX_ROOM_BYTES, CHAT_SESSION_MAX_USERS, CHAT_SESSION_MAX_USER_BYTES,
    CHAT_USER_DISPLAY_MAX_BYTES,
};
use super::protocol::{
    EventId, MessageRevisionAction, MessageRevisionEvent, MessageRevisionSnapshot,
    MessageRevisionSnapshotEntry, PinAction, PinEvent, PinSnapshot, ReactionAction, ReactionEvent,
    ReactionSnapshot, ReactionToken, RoomId, ServerId, UserId,
    MESSAGE_REVISION_SNAPSHOT_MAX_ENTRIES, REACTION_SNAPSHOT_MAX_ENTRIES,
    ROOM_PIN_SNAPSHOT_MAX_ENTRIES,
};
use anyhow::Context;
use rusqlite::OptionalExtension;

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
    fn set_room_mute_except_mentions(
        &mut self,
        server_id: &ServerId,
        room_id: RoomId,
        enabled: bool,
    ) -> anyhow::Result<()>;
    fn room_mute_except_mentions(
        &self,
        server_id: &ServerId,
        room_id: RoomId,
    ) -> anyhow::Result<bool>;
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
    fn apply_reaction_event(
        &mut self,
        server_id: &ServerId,
        room_id: RoomId,
        event: ReactionEvent,
    ) -> anyhow::Result<bool>;
    fn replace_reaction_snapshot(
        &mut self,
        server_id: &ServerId,
        room_id: RoomId,
        snapshot: ReactionSnapshot,
    ) -> anyhow::Result<()>;
    fn reactions_for_targets(
        &self,
        server_id: &ServerId,
        room_id: RoomId,
        target_event_ids: &[EventId],
    ) -> anyhow::Result<Vec<ChatReaction>>;
    fn apply_message_revision_event(
        &mut self,
        server_id: &ServerId,
        room_id: RoomId,
        event: MessageRevisionEvent,
    ) -> anyhow::Result<bool>;
    fn replace_message_revision_snapshot(
        &mut self,
        server_id: &ServerId,
        room_id: RoomId,
        snapshot: MessageRevisionSnapshot,
    ) -> anyhow::Result<()>;
    fn message_revisions_for_targets(
        &self,
        server_id: &ServerId,
        room_id: RoomId,
        target_event_ids: &[EventId],
    ) -> anyhow::Result<Vec<ChatMessageRevision>>;
    fn apply_pin_event(
        &mut self,
        server_id: &ServerId,
        room_id: RoomId,
        event: PinEvent,
    ) -> anyhow::Result<bool>;
    fn replace_pin_snapshot(
        &mut self,
        server_id: &ServerId,
        room_id: RoomId,
        snapshot: PinSnapshot,
    ) -> anyhow::Result<()>;
    fn pins_for_targets(
        &self,
        server_id: &ServerId,
        room_id: RoomId,
        target_event_ids: &[EventId],
    ) -> anyhow::Result<Vec<ChatPin>>;
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct StoredChatHistoryEvent {
    pub(crate) server_display_name: String,
    pub(crate) room_name: String,
    pub(crate) event: ChatEvent,
}

pub(crate) const CHAT_HISTORY_SEARCH_READ_MAX_ITEMS: usize = 8_192;
pub(crate) const CHAT_HISTORY_SEARCH_READ_MAX_BYTES: usize = 64 * 1024 * 1024;

impl SqliteChatStore {
    pub fn open(path: impl AsRef<std::path::Path>) -> anyhow::Result<Self> {
        if let Some(parent) = path.as_ref().parent() {
            crate::private_fs::ensure_private_parent_dir(parent)?;
        }
        if !crate::private_fs::repair_private_file_if_exists(path.as_ref())? {
            drop(crate::private_fs::create_private_new(path.as_ref())?);
        }
        let connection = rusqlite::Connection::open(path.as_ref())?;
        let store = Self { connection };
        store.migrate()?;
        Ok(store)
    }

    /// Opens an existing client history database without creating or migrating it.
    ///
    /// This is intended for bounded background readers such as local history
    /// search. The connection is additionally placed in SQLite query-only mode
    /// so an accidentally called mutating store method fails closed.
    pub fn open_read_only(path: impl AsRef<std::path::Path>) -> anyhow::Result<Self> {
        crate::private_fs::repair_private_file(path.as_ref())?;
        let connection = rusqlite::Connection::open_with_flags(
            path.as_ref(),
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .context("open existing OMENchat history read-only")?;
        connection
            .pragma_update(None, "query_only", true)
            .context("enable query-only OMENchat history access")?;
        Ok(Self { connection })
    }

    pub fn in_memory() -> anyhow::Result<Self> {
        let store = Self {
            connection: rusqlite::Connection::open_in_memory()?,
        };
        store.migrate()?;
        Ok(store)
    }

    pub(crate) fn latest_history_search_events(
        &self,
        max_items: usize,
        max_bytes: usize,
    ) -> anyhow::Result<Vec<StoredChatHistoryEvent>> {
        if max_items == 0 || max_bytes == 0 {
            return Ok(Vec::new());
        }
        let max_items = i64::try_from(max_items.min(CHAT_HISTORY_SEARCH_READ_MAX_ITEMS))
            .context("history search item limit is too large")?;
        let max_bytes = i64::try_from(max_bytes.min(CHAT_HISTORY_SEARCH_READ_MAX_BYTES))
            .context("history search byte limit is too large")?;
        let mut statement = self.connection.prepare(
            "WITH bounded_events AS (
               SELECT e.server_id, e.room_id, e.event_id, e.event_kind,
                      e.actor_user_id, e.actor_display_name, e.at, e.payload,
                      e.reply_to_event_id, e.mention_user_ids,
                      s.display_name AS server_display_name,
                      r.name AS room_name,
                      SUM(
                        64
                        + length(CAST(e.server_id AS BLOB))
                        + COALESCE(length(CAST(e.actor_display_name AS BLOB)), 0)
                        + COALESCE(length(CAST(e.payload AS BLOB)), 0)
                        + COALESCE(length(CAST(e.mention_user_ids AS BLOB)), 0)
                        + length(CAST(s.display_name AS BLOB))
                        + length(CAST(r.name AS BLOB))
                      ) OVER (
                        ORDER BY e.at DESC, e.event_id DESC, e.server_id, e.room_id
                      ) AS cumulative_bytes
               FROM room_events e
               JOIN saved_servers s ON s.server_id = e.server_id
               JOIN rooms r ON r.server_id = e.server_id AND r.room_id = e.room_id
               WHERE e.deleted = 0
                 AND e.rowid > (
                   SELECT COALESCE(MAX(rowid), 0) - ?1 FROM room_events
                 )
                 AND length(CAST(e.server_id AS BLOB)) <= ?3
                 AND length(CAST(s.display_name AS BLOB)) <= ?4
                 AND length(CAST(r.name AS BLOB)) <= ?5
                 AND COALESCE(length(CAST(e.actor_display_name AS BLOB)), 0) <= ?6
                 AND COALESCE(length(CAST(e.payload AS BLOB)), 0) <= ?7
             )
             SELECT server_id, room_id, event_id, event_kind, actor_user_id,
                    actor_display_name, at, payload, reply_to_event_id,
                    mention_user_ids, server_display_name, room_name
             FROM bounded_events
             WHERE cumulative_bytes <= ?2
             ORDER BY at DESC, event_id DESC, server_id, room_id
             LIMIT ?1",
        )?;
        let rows = statement.query_map(
            (
                max_items,
                max_bytes,
                CHAT_SERVER_ID_MAX_BYTES as i64,
                CHAT_SERVER_DISPLAY_MAX_BYTES as i64,
                CHAT_ROOM_NAME_MAX_BYTES as i64,
                super::model::CHAT_ACTOR_DISPLAY_MAX_BYTES as i64,
                crate::protocol_limits::OMENCHAT_FRAME_MAX_SCALAR_BYTES as i64,
            ),
            |row| {
                Ok(StoredChatHistoryEvent {
                    event: decode_event_row(row)?,
                    server_display_name: row.get(10)?,
                    room_name: row.get(11)?,
                })
            },
        )?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
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
        self.add_column_if_missing(
            "rooms",
            "mute_except_mentions",
            "ALTER TABLE rooms ADD COLUMN mute_except_mentions INTEGER NOT NULL DEFAULT 0",
        )?;
        let transaction = rusqlite::Transaction::new_unchecked(
            &self.connection,
            rusqlite::TransactionBehavior::Immediate,
        )?;
        transaction.execute_batch(
            "CREATE TABLE IF NOT EXISTS room_reactions(
               server_id TEXT NOT NULL,
               room_id INTEGER NOT NULL CHECK(room_id > 0),
               target_event_id INTEGER NOT NULL CHECK(target_event_id > 0),
               actor_user_id INTEGER NOT NULL CHECK(actor_user_id > 0),
               reaction_token TEXT NOT NULL CHECK(length(reaction_token) BETWEEN 1 AND 16),
               created_at INTEGER NOT NULL CHECK(created_at >= 0),
               PRIMARY KEY(server_id, room_id, target_event_id, actor_user_id, reaction_token)
             );
             CREATE INDEX IF NOT EXISTS idx_client_room_reactions_target
             ON room_reactions(server_id, room_id, target_event_id, reaction_token, actor_user_id);
             CREATE TABLE IF NOT EXISTS room_message_revision_state(
               server_id TEXT NOT NULL,
               room_id INTEGER NOT NULL CHECK(room_id > 0),
               target_event_id INTEGER NOT NULL CHECK(target_event_id > 0),
               latest_revision_event_id INTEGER NOT NULL CHECK(latest_revision_event_id > 0),
               revision_action INTEGER NOT NULL CHECK(revision_action IN (1, 2)),
               actor_user_id INTEGER NOT NULL CHECK(actor_user_id > 0),
               replacement_body TEXT,
               revision_number INTEGER NOT NULL CHECK(revision_number BETWEEN 1 AND 9),
               at INTEGER NOT NULL CHECK(at >= 0),
               retained_bytes INTEGER NOT NULL CHECK(retained_bytes >= 0),
               PRIMARY KEY(server_id, room_id, target_event_id)
             );
             CREATE INDEX IF NOT EXISTS idx_client_room_message_revisions_target
             ON room_message_revision_state(
               server_id, room_id, target_event_id, latest_revision_event_id
             );
             CREATE TABLE IF NOT EXISTS room_pins(
               server_id TEXT NOT NULL,
               room_id INTEGER NOT NULL CHECK(room_id > 0),
               target_event_id INTEGER NOT NULL CHECK(target_event_id > 0),
               pin_event_id INTEGER NOT NULL CHECK(pin_event_id > 0),
               actor_user_id INTEGER NOT NULL CHECK(actor_user_id > 0),
               pinned_at INTEGER NOT NULL CHECK(pinned_at >= 0),
               retained_bytes INTEGER NOT NULL CHECK(retained_bytes >= 0),
               PRIMARY KEY(server_id, room_id, target_event_id)
             );
             CREATE INDEX IF NOT EXISTS idx_client_room_pins_target
             ON room_pins(
               server_id, room_id, target_event_id, pin_event_id
             );",
        )?;
        transaction.commit()?;
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
        transaction.execute(
            "DELETE FROM room_message_revision_state WHERE server_id = ?1",
            [server_id],
        )?;
        transaction.execute("DELETE FROM room_pins WHERE server_id = ?1", [server_id])?;
        transaction.execute(
            "DELETE FROM room_reactions WHERE server_id = ?1",
            [server_id],
        )?;
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

    fn set_room_mute_except_mentions(
        &mut self,
        server_id: &ServerId,
        room_id: RoomId,
        enabled: bool,
    ) -> anyhow::Result<()> {
        let updated = self.connection.execute(
            "UPDATE rooms
             SET mute_except_mentions = ?3
             WHERE server_id = ?1 AND room_id = ?2",
            (server_id, room_id, i64::from(enabled)),
        )?;
        anyhow::ensure!(
            updated == 1,
            "cannot update OMENchat notification policy for an unknown room"
        );
        Ok(())
    }

    fn room_mute_except_mentions(
        &self,
        server_id: &ServerId,
        room_id: RoomId,
    ) -> anyhow::Result<bool> {
        let value = self.connection.query_row(
            "SELECT mute_except_mentions
             FROM rooms
             WHERE server_id = ?1 AND room_id = ?2",
            (server_id, room_id),
            |row| row.get::<_, i64>(0),
        )?;
        match value {
            0 => Ok(false),
            1 => Ok(true),
            _ => anyhow::bail!("stored OMENchat mute-except-mentions value must be 0 or 1"),
        }
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

    fn apply_reaction_event(
        &mut self,
        server_id: &ServerId,
        room_id: RoomId,
        event: ReactionEvent,
    ) -> anyhow::Result<bool> {
        event
            .into_frame_body()
            .context("invalid OMENchat reaction event")?;
        anyhow::ensure!(
            chat_text_fits(server_id, CHAT_SERVER_ID_MAX_BYTES),
            "reaction server id exceeds client limits"
        );
        let target_event_id = i64::try_from(event.target_event_id)
            .context("reaction target event id does not fit SQLite")?;
        let transaction = self.connection.transaction()?;
        ensure_reaction_target(&transaction, server_id, room_id, target_event_id)?;
        let changed = match event.action {
            ReactionAction::Add => {
                let inserted = transaction.execute(
                    "INSERT OR IGNORE INTO room_reactions(
                       server_id, room_id, target_event_id, actor_user_id,
                       reaction_token, created_at
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    (
                        server_id,
                        room_id,
                        target_event_id,
                        event.actor_user_id,
                        event.token.as_str(),
                        event.at_unix,
                    ),
                )?;
                if inserted > 0
                    && !reaction_capacity_ok(
                        &transaction,
                        server_id,
                        room_id,
                        target_event_id,
                        event.actor_user_id,
                    )?
                {
                    anyhow::bail!("reaction state exceeds client retention limits");
                }
                inserted > 0
            }
            ReactionAction::Remove => {
                transaction.execute(
                    "DELETE FROM room_reactions
                     WHERE server_id = ?1 AND room_id = ?2 AND target_event_id = ?3
                       AND actor_user_id = ?4 AND reaction_token = ?5",
                    (
                        server_id,
                        room_id,
                        target_event_id,
                        event.actor_user_id,
                        event.token.as_str(),
                    ),
                )? > 0
            }
        };
        transaction.commit()?;
        Ok(changed)
    }

    fn replace_reaction_snapshot(
        &mut self,
        server_id: &ServerId,
        room_id: RoomId,
        snapshot: ReactionSnapshot,
    ) -> anyhow::Result<()> {
        snapshot
            .clone()
            .into_frame_body()
            .context("invalid OMENchat reaction snapshot")?;
        anyhow::ensure!(
            chat_text_fits(server_id, CHAT_SERVER_ID_MAX_BYTES),
            "reaction server id exceeds client limits"
        );
        let target_event_ids = snapshot
            .target_event_ids
            .iter()
            .map(|event_id| {
                i64::try_from(*event_id).context("reaction target event id does not fit SQLite")
            })
            .collect::<anyhow::Result<Vec<_>>>()?;
        let transaction = self.connection.transaction()?;
        for target_event_id in &target_event_ids {
            ensure_reaction_target(&transaction, server_id, room_id, *target_event_id)?;
        }
        if !target_event_ids.is_empty() {
            let placeholders = std::iter::repeat_n("?", target_event_ids.len())
                .collect::<Vec<_>>()
                .join(",");
            let sql = format!(
                "DELETE FROM room_reactions
                 WHERE server_id = ? AND room_id = ? AND target_event_id IN ({placeholders})"
            );
            let mut parameters = Vec::<rusqlite::types::Value>::with_capacity(
                target_event_ids.len().saturating_add(2),
            );
            parameters.push(server_id.clone().into());
            parameters.push(i64::from(room_id).into());
            parameters.extend(target_event_ids.iter().copied().map(Into::into));
            transaction.execute(&sql, rusqlite::params_from_iter(parameters))?;
        }
        for entry in &snapshot.entries {
            transaction.execute(
                "INSERT INTO room_reactions(
                   server_id, room_id, target_event_id, actor_user_id,
                   reaction_token, created_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                (
                    server_id,
                    room_id,
                    i64::try_from(entry.target_event_id)
                        .context("snapshot target event id does not fit SQLite")?,
                    entry.actor_user_id,
                    entry.token.as_str(),
                    entry.created_at_unix,
                ),
            )?;
        }
        if !reaction_snapshot_capacity_ok(&transaction, server_id, room_id, &target_event_ids)? {
            anyhow::bail!("reaction snapshot exceeds client retention limits");
        }
        transaction.commit()?;
        Ok(())
    }

    fn reactions_for_targets(
        &self,
        server_id: &ServerId,
        room_id: RoomId,
        target_event_ids: &[EventId],
    ) -> anyhow::Result<Vec<ChatReaction>> {
        ReactionSnapshot {
            target_event_ids: target_event_ids.to_vec(),
            entries: Vec::new(),
        }
        .into_frame_body()
        .context("invalid reaction target set")?;
        if target_event_ids.is_empty() {
            return Ok(Vec::new());
        }
        let target_event_ids = target_event_ids
            .iter()
            .map(|event_id| {
                i64::try_from(*event_id).context("reaction target event id does not fit SQLite")
            })
            .collect::<anyhow::Result<Vec<_>>>()?;
        let placeholders = std::iter::repeat_n("?", target_event_ids.len())
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!(
            "SELECT target_event_id, actor_user_id, reaction_token, created_at
             FROM room_reactions
             WHERE server_id = ? AND room_id = ? AND target_event_id IN ({placeholders})
             ORDER BY target_event_id, reaction_token, actor_user_id
             LIMIT ?"
        );
        let mut parameters =
            Vec::<rusqlite::types::Value>::with_capacity(target_event_ids.len().saturating_add(3));
        parameters.push(server_id.clone().into());
        parameters.push(i64::from(room_id).into());
        parameters.extend(target_event_ids.iter().copied().map(Into::into));
        parameters.push(((REACTION_SNAPSHOT_MAX_ENTRIES + 1) as i64).into());
        let mut statement = self.connection.prepare(&sql)?;
        let rows = statement
            .query_map(rusqlite::params_from_iter(parameters), |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        anyhow::ensure!(
            rows.len() <= REACTION_SNAPSHOT_MAX_ENTRIES,
            "stored reaction target set exceeds snapshot limits"
        );
        rows.into_iter()
            .map(|(target_event_id, actor_user_id, token, created_at_unix)| {
                Ok(ChatReaction {
                    server_id: server_id.clone(),
                    room_id,
                    target_event_id: u64::try_from(target_event_id)
                        .context("stored reaction target id is invalid")?,
                    actor_user_id: u32::try_from(actor_user_id)
                        .ok()
                        .filter(|user_id| *user_id != 0)
                        .context("stored reaction actor id is invalid")?,
                    token: ReactionToken::try_from(token.as_str())
                        .context("stored reaction token is invalid")?,
                    created_at_unix,
                })
            })
            .collect()
    }

    fn apply_message_revision_event(
        &mut self,
        server_id: &ServerId,
        room_id: RoomId,
        event: MessageRevisionEvent,
    ) -> anyhow::Result<bool> {
        event
            .clone()
            .into_frame_body()
            .context("invalid OMENchat message revision event")?;
        anyhow::ensure!(
            chat_text_fits(server_id, CHAT_SERVER_ID_MAX_BYTES),
            "message revision server id exceeds client limits"
        );
        let target_event_id = i64::try_from(event.target_event_id)
            .context("message revision target event id does not fit SQLite")?;
        let revision = ChatMessageRevision {
            server_id: server_id.clone(),
            room_id,
            target_event_id: event.target_event_id,
            latest_revision_event_id: event.revision_event_id,
            action: event.action,
            actor_user_id: event.actor_user_id,
            replacement_body: event.replacement,
            at_unix: event.at_unix,
            revision_number: event.revision_number,
        };
        let transaction = self.connection.transaction()?;
        ensure_message_revision_target(&transaction, server_id, room_id, target_event_id)?;
        if let Some(current) =
            load_message_revision(&transaction, server_id, room_id, target_event_id)?
        {
            if current == revision {
                transaction.commit()?;
                return Ok(false);
            }
            anyhow::ensure!(
                revision.latest_revision_event_id > current.latest_revision_event_id
                    && revision.revision_number > current.revision_number
                    && current.action != MessageRevisionAction::Tombstone,
                "message revision event is stale or conflicts with retained state"
            );
        }
        upsert_message_revision(&transaction, &revision)?;
        anyhow::ensure!(
            message_revision_scope_capacity_ok(&transaction, server_id, room_id)?,
            "message revision state exceeds client retention limits"
        );
        transaction.commit()?;
        Ok(true)
    }

    fn replace_message_revision_snapshot(
        &mut self,
        server_id: &ServerId,
        room_id: RoomId,
        snapshot: MessageRevisionSnapshot,
    ) -> anyhow::Result<()> {
        snapshot
            .clone()
            .into_frame_body()
            .context("invalid OMENchat message revision snapshot")?;
        anyhow::ensure!(
            chat_text_fits(server_id, CHAT_SERVER_ID_MAX_BYTES),
            "message revision server id exceeds client limits"
        );
        let target_event_ids = snapshot
            .target_event_ids
            .iter()
            .map(|event_id| {
                i64::try_from(*event_id)
                    .context("message revision target event id does not fit SQLite")
            })
            .collect::<anyhow::Result<Vec<_>>>()?;
        let transaction = self.connection.transaction()?;
        for target_event_id in &target_event_ids {
            ensure_message_revision_target(&transaction, server_id, room_id, *target_event_id)?;
        }
        if !target_event_ids.is_empty() {
            let placeholders = std::iter::repeat_n("?", target_event_ids.len())
                .collect::<Vec<_>>()
                .join(",");
            let sql = format!(
                "DELETE FROM room_message_revision_state
                 WHERE server_id = ? AND room_id = ? AND target_event_id IN ({placeholders})"
            );
            let mut parameters = Vec::<rusqlite::types::Value>::with_capacity(
                target_event_ids.len().saturating_add(2),
            );
            parameters.push(server_id.clone().into());
            parameters.push(i64::from(room_id).into());
            parameters.extend(target_event_ids.iter().copied().map(Into::into));
            transaction.execute(&sql, rusqlite::params_from_iter(parameters))?;
        }
        for entry in snapshot.entries {
            upsert_message_revision(
                &transaction,
                &ChatMessageRevision {
                    server_id: server_id.clone(),
                    room_id,
                    target_event_id: entry.target_event_id,
                    latest_revision_event_id: entry.latest_revision_event_id,
                    action: entry.action,
                    actor_user_id: entry.actor_user_id,
                    replacement_body: entry.replacement,
                    at_unix: entry.at_unix,
                    revision_number: entry.revision_number,
                },
            )?;
        }
        anyhow::ensure!(
            message_revision_scope_capacity_ok(&transaction, server_id, room_id)?,
            "message revision snapshot exceeds client retention limits"
        );
        transaction.commit()?;
        Ok(())
    }

    fn message_revisions_for_targets(
        &self,
        server_id: &ServerId,
        room_id: RoomId,
        target_event_ids: &[EventId],
    ) -> anyhow::Result<Vec<ChatMessageRevision>> {
        MessageRevisionSnapshot {
            target_event_ids: target_event_ids.to_vec(),
            entries: Vec::new(),
        }
        .into_frame_body()
        .context("invalid message revision target set")?;
        if target_event_ids.is_empty() {
            return Ok(Vec::new());
        }
        let target_event_ids = target_event_ids
            .iter()
            .map(|event_id| {
                i64::try_from(*event_id)
                    .context("message revision target event id does not fit SQLite")
            })
            .collect::<anyhow::Result<Vec<_>>>()?;
        let placeholders = std::iter::repeat_n("?", target_event_ids.len())
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!(
            "SELECT target_event_id, latest_revision_event_id, revision_action,
                    actor_user_id, replacement_body, at, revision_number, retained_bytes
             FROM room_message_revision_state
             WHERE server_id = ? AND room_id = ? AND target_event_id IN ({placeholders})
             ORDER BY target_event_id
             LIMIT ?"
        );
        let mut parameters =
            Vec::<rusqlite::types::Value>::with_capacity(target_event_ids.len().saturating_add(3));
        parameters.push(server_id.clone().into());
        parameters.push(i64::from(room_id).into());
        parameters.extend(target_event_ids.iter().copied().map(Into::into));
        parameters.push(((MESSAGE_REVISION_SNAPSHOT_MAX_ENTRIES + 1) as i64).into());
        let mut statement = self.connection.prepare(&sql)?;
        let revisions = statement
            .query_map(rusqlite::params_from_iter(parameters), |row| {
                decode_message_revision_row(server_id, room_id, row)
            })?
            .collect::<Result<Vec<_>, _>>()?;
        anyhow::ensure!(
            revisions.len() <= MESSAGE_REVISION_SNAPSHOT_MAX_ENTRIES,
            "stored message revision target set exceeds snapshot limits"
        );
        anyhow::ensure!(
            chat_message_revisions_fit_bounds(&revisions),
            "stored message revision state exceeds client retention limits"
        );
        Ok(revisions)
    }

    fn apply_pin_event(
        &mut self,
        server_id: &ServerId,
        room_id: RoomId,
        event: PinEvent,
    ) -> anyhow::Result<bool> {
        event
            .into_frame_body()
            .context("invalid OMENchat pin event")?;
        anyhow::ensure!(
            chat_text_fits(server_id, CHAT_SERVER_ID_MAX_BYTES),
            "pin server id exceeds client limits"
        );
        let target_event_id =
            i64::try_from(event.target_event_id).context("pin target id does not fit SQLite")?;
        let transaction = self.connection.transaction()?;
        ensure_pin_target(&transaction, server_id, room_id, target_event_id)?;
        let current = load_pin(&transaction, server_id, room_id, target_event_id)?;
        let changed = match event.action {
            PinAction::Pin => {
                if let Some(current) = current.as_ref() {
                    if current.pin_event_id == event.pin_event_id
                        && current.actor_user_id == event.actor_user_id
                        && current.pinned_at_unix == event.at_unix
                    {
                        transaction.commit()?;
                        return Ok(false);
                    }
                    anyhow::ensure!(
                        event.pin_event_id > current.pin_event_id,
                        "pin event is stale or conflicts with retained state"
                    );
                }
                upsert_pin(
                    &transaction,
                    &ChatPin {
                        server_id: server_id.clone(),
                        room_id,
                        target_event_id: event.target_event_id,
                        pin_event_id: event.pin_event_id,
                        actor_user_id: event.actor_user_id,
                        pinned_at_unix: event.at_unix,
                    },
                )?;
                true
            }
            PinAction::Unpin => {
                if let Some(current) = current {
                    anyhow::ensure!(
                        event.pin_event_id > current.pin_event_id,
                        "pin event is stale or conflicts with retained state"
                    );
                    transaction.execute(
                        "DELETE FROM room_pins
                         WHERE server_id = ?1 AND room_id = ?2 AND target_event_id = ?3",
                        (server_id, room_id, target_event_id),
                    )? > 0
                } else {
                    false
                }
            }
        };
        anyhow::ensure!(
            pin_scope_capacity_ok(&transaction, server_id)?,
            "pin state exceeds client retention limits"
        );
        transaction.commit()?;
        Ok(changed)
    }

    fn replace_pin_snapshot(
        &mut self,
        server_id: &ServerId,
        room_id: RoomId,
        snapshot: PinSnapshot,
    ) -> anyhow::Result<()> {
        snapshot
            .clone()
            .into_frame_body()
            .context("invalid OMENchat pin snapshot")?;
        anyhow::ensure!(
            chat_text_fits(server_id, CHAT_SERVER_ID_MAX_BYTES),
            "pin server id exceeds client limits"
        );
        let target_event_ids = snapshot
            .target_event_ids
            .iter()
            .map(|event_id| i64::try_from(*event_id).context("pin target id does not fit SQLite"))
            .collect::<anyhow::Result<Vec<_>>>()?;
        let transaction = self.connection.transaction()?;
        for target_event_id in &target_event_ids {
            ensure_pin_target(&transaction, server_id, room_id, *target_event_id)?;
        }
        if !target_event_ids.is_empty() {
            let placeholders = std::iter::repeat_n("?", target_event_ids.len())
                .collect::<Vec<_>>()
                .join(",");
            let sql = format!(
                "DELETE FROM room_pins
                 WHERE server_id = ? AND room_id = ? AND target_event_id IN ({placeholders})"
            );
            let mut parameters = Vec::<rusqlite::types::Value>::with_capacity(
                target_event_ids.len().saturating_add(2),
            );
            parameters.push(server_id.clone().into());
            parameters.push(i64::from(room_id).into());
            parameters.extend(target_event_ids.iter().copied().map(Into::into));
            transaction.execute(&sql, rusqlite::params_from_iter(parameters))?;
        }
        for entry in snapshot.entries {
            upsert_pin(
                &transaction,
                &ChatPin {
                    server_id: server_id.clone(),
                    room_id,
                    target_event_id: entry.target_event_id,
                    pin_event_id: entry.pin_event_id,
                    actor_user_id: entry.actor_user_id,
                    pinned_at_unix: entry.pinned_at_unix,
                },
            )?;
        }
        anyhow::ensure!(
            pin_scope_capacity_ok(&transaction, server_id)?,
            "pin snapshot exceeds client retention limits"
        );
        transaction.commit()?;
        Ok(())
    }

    fn pins_for_targets(
        &self,
        server_id: &ServerId,
        room_id: RoomId,
        target_event_ids: &[EventId],
    ) -> anyhow::Result<Vec<ChatPin>> {
        PinSnapshot {
            target_event_ids: target_event_ids.to_vec(),
            entries: Vec::new(),
        }
        .into_frame_body()
        .context("invalid pin target set")?;
        if target_event_ids.is_empty() {
            return Ok(Vec::new());
        }
        let target_event_ids = target_event_ids
            .iter()
            .map(|event_id| i64::try_from(*event_id).context("pin target id does not fit SQLite"))
            .collect::<anyhow::Result<Vec<_>>>()?;
        let placeholders = std::iter::repeat_n("?", target_event_ids.len())
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!(
            "SELECT target_event_id, pin_event_id, actor_user_id, pinned_at, retained_bytes
             FROM room_pins
             WHERE server_id = ? AND room_id = ? AND target_event_id IN ({placeholders})
             ORDER BY target_event_id
             LIMIT ?"
        );
        let mut parameters =
            Vec::<rusqlite::types::Value>::with_capacity(target_event_ids.len().saturating_add(3));
        parameters.push(server_id.clone().into());
        parameters.push(i64::from(room_id).into());
        parameters.extend(target_event_ids.iter().copied().map(Into::into));
        parameters.push(((ROOM_PIN_SNAPSHOT_MAX_ENTRIES + 1) as i64).into());
        let mut statement = self.connection.prepare(&sql)?;
        let pins = statement
            .query_map(rusqlite::params_from_iter(parameters), |row| {
                decode_pin_row(server_id, room_id, row)
            })?
            .collect::<Result<Vec<_>, _>>()?;
        anyhow::ensure!(
            pins.len() <= ROOM_PIN_SNAPSHOT_MAX_ENTRIES,
            "stored pin target set exceeds snapshot limits"
        );
        anyhow::ensure!(
            chat_pins_fit_bounds(&pins),
            "stored pin state exceeds client retention limits"
        );
        Ok(pins)
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

fn ensure_pin_target(
    transaction: &rusqlite::Transaction<'_>,
    server_id: &ServerId,
    room_id: RoomId,
    target_event_id: i64,
) -> anyhow::Result<()> {
    let eligible = transaction.query_row(
        "SELECT EXISTS(
           SELECT 1 FROM room_events
           WHERE server_id = ?1 AND room_id = ?2 AND event_id = ?3
             AND deleted = 0 AND event_kind IN (1, 2, 3, 5)
         )",
        (server_id, room_id, target_event_id),
        |row| row.get::<_, bool>(0),
    )?;
    anyhow::ensure!(
        eligible,
        "pin target is not retained as an eligible room event"
    );
    Ok(())
}

fn load_pin(
    transaction: &rusqlite::Transaction<'_>,
    server_id: &ServerId,
    room_id: RoomId,
    target_event_id: i64,
) -> anyhow::Result<Option<ChatPin>> {
    transaction
        .query_row(
            "SELECT target_event_id, pin_event_id, actor_user_id, pinned_at, retained_bytes
             FROM room_pins
             WHERE server_id = ?1 AND room_id = ?2 AND target_event_id = ?3",
            (server_id, room_id, target_event_id),
            |row| decode_pin_row(server_id, room_id, row),
        )
        .optional()
        .map_err(Into::into)
}

fn upsert_pin(transaction: &rusqlite::Transaction<'_>, pin: &ChatPin) -> anyhow::Result<()> {
    PinSnapshot {
        target_event_ids: vec![pin.target_event_id],
        entries: vec![super::protocol::PinSnapshotEntry {
            target_event_id: pin.target_event_id,
            pin_event_id: pin.pin_event_id,
            actor_user_id: pin.actor_user_id,
            pinned_at_unix: pin.pinned_at_unix,
        }],
    }
    .into_frame_body()
    .context("invalid retained pin")?;
    let retained_bytes =
        i64::try_from(pin.retained_bytes()).context("pin retained bytes do not fit SQLite")?;
    transaction.execute(
        "INSERT INTO room_pins(
           server_id, room_id, target_event_id, pin_event_id,
           actor_user_id, pinned_at, retained_bytes
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
         ON CONFLICT(server_id, room_id, target_event_id) DO UPDATE SET
           pin_event_id = excluded.pin_event_id,
           actor_user_id = excluded.actor_user_id,
           pinned_at = excluded.pinned_at,
           retained_bytes = excluded.retained_bytes",
        rusqlite::params![
            &pin.server_id,
            i64::from(pin.room_id),
            i64::try_from(pin.target_event_id).context("pin target id does not fit SQLite")?,
            i64::try_from(pin.pin_event_id).context("pin event id does not fit SQLite")?,
            pin.actor_user_id,
            pin.pinned_at_unix,
            retained_bytes,
        ],
    )?;
    Ok(())
}

fn decode_pin_row(
    server_id: &ServerId,
    room_id: RoomId,
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<ChatPin> {
    let pin = ChatPin {
        server_id: server_id.clone(),
        room_id,
        target_event_id: positive_u64_column(row, 0, "pin target id")?,
        pin_event_id: positive_u64_column(row, 1, "pin event id")?,
        actor_user_id: row
            .get::<_, i64>(2)
            .ok()
            .and_then(|value| u32::try_from(value).ok())
            .filter(|value| *value != 0)
            .ok_or_else(|| invalid_stored_column(2, "pin actor id is invalid"))?,
        pinned_at_unix: row.get(3)?,
    };
    let retained_bytes = row.get::<_, i64>(4)?;
    let expected = i64::try_from(pin.retained_bytes())
        .map_err(|_| invalid_stored_column(4, "pin retained bytes overflow"))?;
    if retained_bytes != expected {
        return Err(invalid_stored_column(
            4,
            "pin retained-byte accounting is invalid",
        ));
    }
    PinSnapshot {
        target_event_ids: vec![pin.target_event_id],
        entries: vec![super::protocol::PinSnapshotEntry {
            target_event_id: pin.target_event_id,
            pin_event_id: pin.pin_event_id,
            actor_user_id: pin.actor_user_id,
            pinned_at_unix: pin.pinned_at_unix,
        }],
    }
    .into_frame_body()
    .map_err(|error| invalid_stored_column(3, error.to_string()))?;
    Ok(pin)
}

fn pin_scope_capacity_ok(
    transaction: &rusqlite::Transaction<'_>,
    server_id: &ServerId,
) -> anyhow::Result<bool> {
    let (server_rows, server_bytes): (i64, i64) = transaction.query_row(
        "SELECT COUNT(*), COALESCE(SUM(retained_bytes), 0)
         FROM room_pins WHERE server_id = ?1",
        [server_id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    let (global_rows, global_bytes): (i64, i64) = transaction.query_row(
        "SELECT COUNT(*), COALESCE(SUM(retained_bytes), 0) FROM room_pins",
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    Ok(server_rows <= CHAT_PIN_MAX_ROWS_PER_SERVER as i64
        && server_bytes <= CHAT_PIN_MAX_BYTES as i64
        && global_rows <= CHAT_PIN_MAX_ROWS as i64
        && global_bytes <= CHAT_PIN_MAX_BYTES as i64)
}

fn ensure_message_revision_target(
    transaction: &rusqlite::Transaction<'_>,
    server_id: &ServerId,
    room_id: RoomId,
    target_event_id: i64,
) -> anyhow::Result<()> {
    let eligible = transaction.query_row(
        "SELECT EXISTS(
           SELECT 1 FROM room_events
           WHERE server_id = ?1 AND room_id = ?2 AND event_id = ?3
             AND deleted = 0 AND event_kind = 1
         )",
        (server_id, room_id, target_event_id),
        |row| row.get::<_, bool>(0),
    )?;
    anyhow::ensure!(
        eligible,
        "message revision target is not retained as an eligible room message"
    );
    Ok(())
}

fn load_message_revision(
    transaction: &rusqlite::Transaction<'_>,
    server_id: &ServerId,
    room_id: RoomId,
    target_event_id: i64,
) -> anyhow::Result<Option<ChatMessageRevision>> {
    transaction
        .query_row(
            "SELECT target_event_id, latest_revision_event_id, revision_action,
                    actor_user_id, replacement_body, at, revision_number, retained_bytes
             FROM room_message_revision_state
             WHERE server_id = ?1 AND room_id = ?2 AND target_event_id = ?3",
            (server_id, room_id, target_event_id),
            |row| decode_message_revision_row(server_id, room_id, row),
        )
        .optional()
        .map_err(Into::into)
}

fn upsert_message_revision(
    transaction: &rusqlite::Transaction<'_>,
    revision: &ChatMessageRevision,
) -> anyhow::Result<()> {
    MessageRevisionSnapshot {
        target_event_ids: vec![revision.target_event_id],
        entries: vec![MessageRevisionSnapshotEntry {
            target_event_id: revision.target_event_id,
            latest_revision_event_id: revision.latest_revision_event_id,
            action: revision.action,
            actor_user_id: revision.actor_user_id,
            at_unix: revision.at_unix,
            replacement: revision.replacement_body.clone(),
            revision_number: revision.revision_number,
        }],
    }
    .into_frame_body()
    .context("invalid retained message revision")?;
    let retained_bytes = i64::try_from(revision.retained_bytes())
        .context("message revision retained bytes do not fit SQLite")?;
    transaction.execute(
        "INSERT INTO room_message_revision_state(
           server_id, room_id, target_event_id, latest_revision_event_id,
           revision_action, actor_user_id, replacement_body, revision_number,
           at, retained_bytes
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
         ON CONFLICT(server_id, room_id, target_event_id) DO UPDATE SET
           latest_revision_event_id = excluded.latest_revision_event_id,
           revision_action = excluded.revision_action,
           actor_user_id = excluded.actor_user_id,
           replacement_body = excluded.replacement_body,
           revision_number = excluded.revision_number,
           at = excluded.at,
           retained_bytes = excluded.retained_bytes",
        rusqlite::params![
            &revision.server_id,
            i64::from(revision.room_id),
            i64::try_from(revision.target_event_id)
                .context("message revision target id does not fit SQLite")?,
            i64::try_from(revision.latest_revision_event_id)
                .context("message revision event id does not fit SQLite")?,
            revision.action as u8,
            revision.actor_user_id,
            revision.replacement_body.as_deref(),
            i64::try_from(revision.revision_number)
                .context("message revision number does not fit SQLite")?,
            revision.at_unix,
            retained_bytes,
        ],
    )?;
    Ok(())
}

fn decode_message_revision_row(
    server_id: &ServerId,
    room_id: RoomId,
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<ChatMessageRevision> {
    let target_event_id = positive_u64_column(row, 0, "message revision target id")?;
    let latest_revision_event_id = positive_u64_column(row, 1, "message revision event id")?;
    let action_value = positive_u64_column(row, 2, "message revision action")?;
    let action = MessageRevisionAction::try_from(action_value)
        .map_err(|error| invalid_stored_column(2, error.to_string()))?;
    let actor_user_id = row
        .get::<_, i64>(3)
        .ok()
        .and_then(|value| u32::try_from(value).ok())
        .filter(|value| *value != 0)
        .ok_or_else(|| invalid_stored_column(3, "message revision actor id is invalid"))?;
    let revision = ChatMessageRevision {
        server_id: server_id.clone(),
        room_id,
        target_event_id,
        latest_revision_event_id,
        action,
        actor_user_id,
        replacement_body: row.get(4)?,
        at_unix: row.get(5)?,
        revision_number: positive_u64_column(row, 6, "message revision number")?,
    };
    let retained_bytes = row.get::<_, i64>(7)?;
    let expected = i64::try_from(revision.retained_bytes())
        .map_err(|_| invalid_stored_column(7, "message revision retained bytes overflow"))?;
    if retained_bytes != expected {
        return Err(invalid_stored_column(
            7,
            "message revision retained-byte accounting is invalid",
        ));
    }
    MessageRevisionSnapshot {
        target_event_ids: vec![revision.target_event_id],
        entries: vec![MessageRevisionSnapshotEntry {
            target_event_id: revision.target_event_id,
            latest_revision_event_id: revision.latest_revision_event_id,
            action: revision.action,
            actor_user_id: revision.actor_user_id,
            at_unix: revision.at_unix,
            replacement: revision.replacement_body.clone(),
            revision_number: revision.revision_number,
        }],
    }
    .into_frame_body()
    .map_err(|error| invalid_stored_column(4, error.to_string()))?;
    Ok(revision)
}

fn positive_u64_column(
    row: &rusqlite::Row<'_>,
    index: usize,
    label: &str,
) -> rusqlite::Result<u64> {
    row.get::<_, i64>(index)
        .ok()
        .and_then(|value| u64::try_from(value).ok())
        .filter(|value| *value != 0)
        .ok_or_else(|| invalid_stored_column(index, format!("{label} is invalid")))
}

fn invalid_stored_column(index: usize, message: impl Into<String>) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        index,
        rusqlite::types::Type::Integer,
        Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            message.into(),
        )),
    )
}

fn message_revision_scope_capacity_ok(
    transaction: &rusqlite::Transaction<'_>,
    server_id: &ServerId,
    room_id: RoomId,
) -> anyhow::Result<bool> {
    let (room_rows, room_bytes): (i64, i64) = transaction.query_row(
        "SELECT COUNT(*), COALESCE(SUM(retained_bytes), 0)
         FROM room_message_revision_state WHERE server_id = ?1 AND room_id = ?2",
        (server_id, room_id),
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    let (server_rows, server_bytes): (i64, i64) = transaction.query_row(
        "SELECT COUNT(*), COALESCE(SUM(retained_bytes), 0)
         FROM room_message_revision_state WHERE server_id = ?1",
        [server_id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    let (global_rows, global_bytes): (i64, i64) = transaction.query_row(
        "SELECT COUNT(*), COALESCE(SUM(retained_bytes), 0)
         FROM room_message_revision_state",
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    Ok(
        room_rows <= super::model::CHAT_MESSAGE_REVISION_MAX_ROWS_PER_ROOM as i64
            && room_bytes <= super::model::CHAT_MESSAGE_REVISION_MAX_BYTES_PER_ROOM as i64
            && server_rows <= super::model::CHAT_MESSAGE_REVISION_MAX_ROWS_PER_SERVER as i64
            && server_bytes <= super::model::CHAT_MESSAGE_REVISION_MAX_BYTES_PER_SERVER as i64
            && global_rows <= super::model::CHAT_MESSAGE_REVISION_MAX_ROWS as i64
            && global_bytes <= super::model::CHAT_MESSAGE_REVISION_MAX_BYTES as i64,
    )
}

fn ensure_reaction_target(
    transaction: &rusqlite::Transaction<'_>,
    server_id: &ServerId,
    room_id: RoomId,
    target_event_id: i64,
) -> anyhow::Result<()> {
    let eligible = transaction.query_row(
        "SELECT EXISTS(
           SELECT 1 FROM room_events
           WHERE server_id = ?1 AND room_id = ?2 AND event_id = ?3
             AND deleted = 0 AND event_kind IN (1, 2, 3, 5)
         )",
        (server_id, room_id, target_event_id),
        |row| row.get::<_, bool>(0),
    )?;
    anyhow::ensure!(
        eligible,
        "reaction target is not retained as an eligible room event"
    );
    Ok(())
}

fn reaction_capacity_ok(
    transaction: &rusqlite::Transaction<'_>,
    server_id: &ServerId,
    room_id: RoomId,
    target_event_id: i64,
    actor_user_id: UserId,
) -> anyhow::Result<bool> {
    let actor_target_rows = transaction.query_row(
        "SELECT COUNT(*) FROM room_reactions
         WHERE server_id = ?1 AND room_id = ?2 AND target_event_id = ?3
           AND actor_user_id = ?4",
        (server_id, room_id, target_event_id, actor_user_id),
        |row| row.get::<_, i64>(0),
    )?;
    let target_rows = transaction.query_row(
        "SELECT COUNT(*) FROM room_reactions
         WHERE server_id = ?1 AND room_id = ?2 AND target_event_id = ?3",
        (server_id, room_id, target_event_id),
        |row| row.get::<_, i64>(0),
    )?;
    Ok(
        actor_target_rows <= CHAT_REACTION_MAX_TOKENS_PER_ACTOR_TARGET as i64
            && target_rows <= CHAT_REACTION_MAX_ROWS_PER_TARGET as i64
            && reaction_scope_capacity_ok(transaction, server_id, room_id)?,
    )
}

fn reaction_snapshot_capacity_ok(
    transaction: &rusqlite::Transaction<'_>,
    server_id: &ServerId,
    room_id: RoomId,
    target_event_ids: &[i64],
) -> anyhow::Result<bool> {
    for target_event_id in target_event_ids {
        let over_actor_limit = transaction.query_row(
            "SELECT EXISTS(
               SELECT 1 FROM room_reactions
               WHERE server_id = ?1 AND room_id = ?2 AND target_event_id = ?3
               GROUP BY actor_user_id
               HAVING COUNT(*) > ?4
             )",
            (
                server_id,
                room_id,
                target_event_id,
                CHAT_REACTION_MAX_TOKENS_PER_ACTOR_TARGET as i64,
            ),
            |row| row.get::<_, bool>(0),
        )?;
        let target_rows = transaction.query_row(
            "SELECT COUNT(*) FROM room_reactions
             WHERE server_id = ?1 AND room_id = ?2 AND target_event_id = ?3",
            (server_id, room_id, target_event_id),
            |row| row.get::<_, i64>(0),
        )?;
        if over_actor_limit || target_rows > CHAT_REACTION_MAX_ROWS_PER_TARGET as i64 {
            return Ok(false);
        }
    }
    reaction_scope_capacity_ok(transaction, server_id, room_id)
}

fn reaction_scope_capacity_ok(
    transaction: &rusqlite::Transaction<'_>,
    server_id: &ServerId,
    room_id: RoomId,
) -> anyhow::Result<bool> {
    let fixed = std::mem::size_of::<ChatReaction>() as i64;
    let (room_rows, room_bytes): (i64, i64) = transaction.query_row(
        "SELECT COUNT(*),
                COALESCE(SUM(?3 + length(CAST(server_id AS BLOB))
                                 + length(CAST(reaction_token AS BLOB))), 0)
         FROM room_reactions WHERE server_id = ?1 AND room_id = ?2",
        (server_id, room_id, fixed),
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    let (server_rows, server_bytes): (i64, i64) = transaction.query_row(
        "SELECT COUNT(*),
                COALESCE(SUM(?2 + length(CAST(server_id AS BLOB))
                                 + length(CAST(reaction_token AS BLOB))), 0)
         FROM room_reactions WHERE server_id = ?1",
        (server_id, fixed),
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    let (global_rows, global_bytes): (i64, i64) = transaction.query_row(
        "SELECT COUNT(*),
                COALESCE(SUM(?1 + length(CAST(server_id AS BLOB))
                                 + length(CAST(reaction_token AS BLOB))), 0)
         FROM room_reactions",
        [fixed],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    Ok(room_rows <= CHAT_REACTION_MAX_ROWS_PER_ROOM as i64
        && room_bytes <= CHAT_REACTION_MAX_BYTES_PER_ROOM as i64
        && server_rows <= CHAT_REACTION_MAX_ROWS_PER_SERVER as i64
        && server_bytes <= CHAT_REACTION_MAX_BYTES_PER_SERVER as i64
        && global_rows <= CHAT_REACTION_MAX_ROWS as i64
        && global_bytes <= CHAT_REACTION_MAX_BYTES as i64)
}

impl SqliteChatStore {
    fn query_events<P>(&self, sql: &str, params: P) -> anyhow::Result<Vec<ChatEvent>>
    where
        P: rusqlite::Params,
    {
        let mut statement = self.connection.prepare(sql)?;
        let rows = statement.query_map(params, decode_event_row)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }
}

fn decode_event_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ChatEvent> {
    let kind: i64 = row.get(3)?;
    let payload: Option<String> = row.get(7)?;
    let reply_to_event_id = match row.get::<_, Option<i64>>(8)? {
        Some(value) => Some(
            u64::try_from(value)
                .ok()
                .filter(|value| *value != 0)
                .ok_or_else(|| invalid_metadata_column(8, "reply event id must be positive"))?,
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
    use crate::chat::protocol::{PinSnapshotEntry, ReactionSnapshotEntry};

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
    fn read_only_store_reads_existing_history_and_fails_closed_on_writes() {
        let path = isolated_store_path("read-only");
        {
            let mut store = SqliteChatStore::open(&path).expect("create store");
            let server = sample_server();
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
                .expect("room");
            store
                .append_events(vec![ChatEvent {
                    server_id: server.server_id,
                    room_id: 1,
                    event_id: 1,
                    actor_user_id: Some(7),
                    actor_display_name: Some("Alice".into()),
                    at_unix: 10,
                    kind: ChatEventKind::Message {
                        body: "read-only history".into(),
                    },
                }])
                .expect("event");
        }

        let mut store = SqliteChatStore::open_read_only(&path).expect("read-only store");
        assert_eq!(
            store
                .latest_events(&"server-a".into(), 1, 10)
                .expect("read history")
                .len(),
            1
        );
        assert_eq!(
            store
                .connection
                .pragma_query_value(None, "query_only", |row| row.get::<_, i64>(0))
                .expect("query-only state"),
            1
        );
        assert!(store.save_server(sample_server()).is_err());
        drop(store);
        std::fs::remove_file(path).expect("remove store");
    }

    #[test]
    fn read_only_store_does_not_create_a_missing_database_or_parent() {
        let parent = std::env::temp_dir().join(format!(
            "omenbrowser-chat-store-read-only-missing-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system time")
                .as_nanos()
        ));
        let path = parent.join("chat.sqlite");
        assert!(SqliteChatStore::open_read_only(&path).is_err());
        assert!(!path.exists());
        assert!(!parent.exists());
    }

    #[test]
    fn history_search_loader_is_newest_first_and_bounded_by_items_and_bytes() {
        let path = isolated_store_path("history-search-loader");
        {
            let mut store = SqliteChatStore::open(&path).expect("create store");
            let server = sample_server();
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
                .expect("room");
            store
                .append_events(
                    [10_i64, 30, 20]
                        .into_iter()
                        .enumerate()
                        .map(|(index, at_unix)| ChatEvent {
                            server_id: server.server_id.clone(),
                            room_id: 1,
                            event_id: index as u64 + 1,
                            actor_user_id: Some(7),
                            actor_display_name: Some("Alice".into()),
                            at_unix,
                            kind: ChatEventKind::Message {
                                body: format!("event-{index}-{}", "x".repeat(100)),
                            },
                        })
                        .collect(),
                )
                .expect("events");
        }

        let store = SqliteChatStore::open_read_only(&path).expect("read-only store");
        let item_bounded = store
            .latest_history_search_events(2, 1024 * 1024)
            .expect("item-bounded history");
        assert_eq!(
            item_bounded
                .iter()
                .map(|stored| stored.event.at_unix)
                .collect::<Vec<_>>(),
            vec![30, 20]
        );
        assert!(item_bounded.iter().all(|stored| {
            stored.server_display_name == "Server A" && stored.room_name == "lobby"
        }));

        let byte_bounded = store
            .latest_history_search_events(10, 250)
            .expect("byte-bounded history");
        assert_eq!(byte_bounded.len(), 1);
        assert_eq!(byte_bounded[0].event.at_unix, 30);
        assert!(store
            .latest_history_search_events(0, 1024)
            .expect("zero items")
            .is_empty());
        assert!(store
            .latest_history_search_events(10, 0)
            .expect("zero bytes")
            .is_empty());
        assert_eq!(
            store
                .latest_history_search_events(usize::MAX, usize::MAX)
                .expect("hard-clamped bounds")
                .len(),
            3
        );
        drop(store);
        std::fs::remove_file(path).expect("remove store");
    }

    #[cfg(feature = "portable-sqlite")]
    #[test]
    fn packaged_sqlite_build_exposes_working_fts5() {
        let store = SqliteChatStore::in_memory().expect("store");
        let enabled = store
            .connection
            .query_row(
                "SELECT sqlite_compileoption_used('ENABLE_FTS5')",
                [],
                |row| row.get::<_, bool>(0),
            )
            .expect("compile option");
        assert!(enabled);
        store
            .connection
            .execute_batch(
                "CREATE VIRTUAL TABLE temp.local_search_fts5_probe USING fts5(body);
                 INSERT INTO local_search_fts5_probe(body) VALUES ('bounded search probe');",
            )
            .expect("create FTS5 probe");
        assert_eq!(
            store
                .connection
                .query_row(
                    "SELECT COUNT(*) FROM local_search_fts5_probe
                     WHERE local_search_fts5_probe MATCH 'bounded'",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .expect("FTS5 query"),
            1
        );
    }

    #[test]
    fn room_mute_except_mentions_defaults_off_and_survives_restart() {
        let path = isolated_store_path("mute-except-mentions");
        {
            let mut store = SqliteChatStore::open(&path).expect("store");
            let server = sample_server();
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
                .expect("room");
            assert!(!store
                .room_mute_except_mentions(&server.server_id, 1)
                .expect("default"));
            store
                .set_room_mute_except_mentions(&server.server_id, 1, true)
                .expect("enable");
        }
        {
            let store = SqliteChatStore::open(&path).expect("reopen");
            assert!(store
                .room_mute_except_mentions(&sample_server().server_id, 1)
                .expect("restored"));
            store
                .connection
                .execute(
                    "UPDATE rooms SET mute_except_mentions = 2
                     WHERE server_id = ?1 AND room_id = 1",
                    [sample_server().server_id],
                )
                .expect("inject invalid setting");
            assert!(store
                .room_mute_except_mentions(&sample_server().server_id, 1)
                .is_err());
        }
        let _ = std::fs::remove_file(path);
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
    fn reaction_store_is_additive_restart_safe_and_snapshot_authoritative() {
        let path = isolated_store_path("reactions");
        let server = sample_server();
        {
            let mut store = SqliteChatStore::open(&path).expect("store");
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
                .expect("room");
            store
                .append_events(vec![ChatEvent {
                    server_id: server.server_id.clone(),
                    room_id: 1,
                    event_id: 10,
                    actor_user_id: Some(1),
                    actor_display_name: Some("Alice".into()),
                    at_unix: 1,
                    kind: ChatEventKind::Message {
                        body: "target".into(),
                    },
                }])
                .expect("target");
            let add = ReactionEvent {
                reaction_event_id: 1,
                target_event_id: 10,
                actor_user_id: 7,
                token: ReactionToken::Heart,
                action: ReactionAction::Add,
                at_unix: 2,
            };
            assert!(store
                .apply_reaction_event(&server.server_id, 1, add)
                .expect("add"));
            assert!(!store
                .apply_reaction_event(&server.server_id, 1, add)
                .expect("duplicate add"));
        }
        {
            let mut store = SqliteChatStore::open(&path).expect("reopen");
            assert_eq!(
                store
                    .reactions_for_targets(&server.server_id, 1, &[10])
                    .expect("restored reactions"),
                vec![ChatReaction {
                    server_id: server.server_id.clone(),
                    room_id: 1,
                    target_event_id: 10,
                    actor_user_id: 7,
                    token: ReactionToken::Heart,
                    created_at_unix: 2,
                }]
            );
            store
                .replace_reaction_snapshot(
                    &server.server_id,
                    1,
                    ReactionSnapshot {
                        target_event_ids: vec![10],
                        entries: vec![ReactionSnapshotEntry {
                            target_event_id: 10,
                            actor_user_id: 8,
                            token: ReactionToken::Celebrate,
                            created_at_unix: 3,
                        }],
                    },
                )
                .expect("replace snapshot");
            let reactions = store
                .reactions_for_targets(&server.server_id, 1, &[10])
                .expect("snapshot reactions");
            assert_eq!(reactions.len(), 1);
            assert_eq!(reactions[0].actor_user_id, 8);
            assert_eq!(reactions[0].token, ReactionToken::Celebrate);

            store
                .replace_reaction_snapshot(
                    &server.server_id,
                    1,
                    ReactionSnapshot {
                        target_event_ids: vec![10],
                        entries: Vec::new(),
                    },
                )
                .expect("empty authoritative snapshot");
            assert!(store
                .reactions_for_targets(&server.server_id, 1, &[10])
                .expect("cleared reactions")
                .is_empty());
        }
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn message_revision_store_is_restart_safe_ordered_and_snapshot_authoritative() {
        let path = isolated_store_path("message-revisions");
        let server = sample_server();
        {
            let mut store = SqliteChatStore::open(&path).expect("store");
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
                .expect("room");
            store
                .append_events(vec![ChatEvent {
                    server_id: server.server_id.clone(),
                    room_id: 1,
                    event_id: 10,
                    actor_user_id: Some(1),
                    actor_display_name: Some("Alice".into()),
                    at_unix: 1,
                    kind: ChatEventKind::Message {
                        body: "immutable original".into(),
                    },
                }])
                .expect("target");
            let correction = MessageRevisionEvent {
                revision_event_id: 20,
                target_event_id: 10,
                action: MessageRevisionAction::Correct,
                actor_user_id: 1,
                at_unix: 2,
                replacement: Some("corrected".into()),
                revision_number: 1,
                actor_display_name: Some("Alice".into()),
            };
            assert!(store
                .apply_message_revision_event(&server.server_id, 1, correction.clone())
                .expect("correction"));
            assert!(!store
                .apply_message_revision_event(&server.server_id, 1, correction)
                .expect("exact duplicate"));
            assert!(store
                .apply_message_revision_event(
                    &server.server_id,
                    1,
                    MessageRevisionEvent {
                        revision_event_id: 19,
                        target_event_id: 10,
                        action: MessageRevisionAction::Correct,
                        actor_user_id: 1,
                        at_unix: 3,
                        replacement: Some("stale".into()),
                        revision_number: 1,
                        actor_display_name: None,
                    },
                )
                .is_err());
        }
        {
            let mut store = SqliteChatStore::open(&path).expect("reopen");
            let retained = store
                .message_revisions_for_targets(&server.server_id, 1, &[10])
                .expect("restored revision");
            assert_eq!(retained.len(), 1);
            assert_eq!(retained[0].replacement_body.as_deref(), Some("corrected"));
            assert_eq!(
                store
                    .latest_events(&server.server_id, 1, 10)
                    .expect("original history")[0]
                    .kind,
                ChatEventKind::Message {
                    body: "immutable original".into()
                }
            );
            store
                .replace_message_revision_snapshot(
                    &server.server_id,
                    1,
                    MessageRevisionSnapshot {
                        target_event_ids: vec![10],
                        entries: vec![MessageRevisionSnapshotEntry {
                            target_event_id: 10,
                            latest_revision_event_id: 21,
                            action: MessageRevisionAction::Tombstone,
                            actor_user_id: 2,
                            at_unix: 4,
                            replacement: None,
                            revision_number: 2,
                        }],
                    },
                )
                .expect("authoritative tombstone");
            assert_eq!(
                store
                    .message_revisions_for_targets(&server.server_id, 1, &[10])
                    .expect("tombstone")[0]
                    .action,
                MessageRevisionAction::Tombstone
            );
            store
                .replace_message_revision_snapshot(
                    &server.server_id,
                    1,
                    MessageRevisionSnapshot {
                        target_event_ids: vec![10],
                        entries: Vec::new(),
                    },
                )
                .expect("authoritative clear");
            assert!(store
                .message_revisions_for_targets(&server.server_id, 1, &[10])
                .expect("cleared")
                .is_empty());
        }
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn pin_store_is_restart_safe_ordered_and_snapshot_authoritative() {
        let path = isolated_store_path("pins");
        let server = sample_server();
        {
            let mut store = SqliteChatStore::open(&path).expect("store");
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
                .expect("room");
            store
                .append_events(vec![ChatEvent {
                    server_id: server.server_id.clone(),
                    room_id: 1,
                    event_id: 10,
                    actor_user_id: Some(1),
                    actor_display_name: Some("Alice".into()),
                    at_unix: 1,
                    kind: ChatEventKind::Message {
                        body: "target".into(),
                    },
                }])
                .expect("target");
            let pin = PinEvent {
                pin_event_id: 20,
                target_event_id: 10,
                action: PinAction::Pin,
                actor_user_id: 7,
                at_unix: 2,
            };
            assert!(store
                .apply_pin_event(&server.server_id, 1, pin)
                .expect("pin"));
            assert!(!store
                .apply_pin_event(&server.server_id, 1, pin)
                .expect("exact duplicate"));
            assert!(store
                .apply_pin_event(
                    &server.server_id,
                    1,
                    PinEvent {
                        pin_event_id: 19,
                        ..pin
                    },
                )
                .is_err());
        }
        {
            let mut store = SqliteChatStore::open(&path).expect("reopen");
            assert_eq!(
                store
                    .pins_for_targets(&server.server_id, 1, &[10])
                    .expect("restored pin")[0]
                    .pin_event_id,
                20
            );
            store
                .replace_pin_snapshot(
                    &server.server_id,
                    1,
                    PinSnapshot {
                        target_event_ids: vec![10],
                        entries: vec![PinSnapshotEntry {
                            target_event_id: 10,
                            pin_event_id: 21,
                            actor_user_id: 8,
                            pinned_at_unix: 3,
                        }],
                    },
                )
                .expect("authoritative replacement");
            assert_eq!(
                store
                    .pins_for_targets(&server.server_id, 1, &[10])
                    .expect("replacement")[0]
                    .actor_user_id,
                8
            );
            store
                .replace_pin_snapshot(
                    &server.server_id,
                    1,
                    PinSnapshot {
                        target_event_ids: vec![10],
                        entries: Vec::new(),
                    },
                )
                .expect("authoritative clear");
            assert!(store
                .pins_for_targets(&server.server_id, 1, &[10])
                .expect("cleared")
                .is_empty());
            assert!(store
                .apply_pin_event(
                    &server.server_id,
                    2,
                    PinEvent {
                        pin_event_id: 22,
                        target_event_id: 10,
                        action: PinAction::Pin,
                        actor_user_id: 8,
                        at_unix: 4,
                    },
                )
                .is_err());
        }
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn message_revision_snapshot_capacity_rolls_back_without_partial_replacement() {
        let mut store = SqliteChatStore::in_memory().expect("store");
        let server = sample_server();
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
            .expect("room");
        let target_count = super::super::model::CHAT_MESSAGE_REVISION_MAX_ROWS_PER_ROOM + 1;
        store
            .append_events(
                (1..=target_count as u64)
                    .map(|event_id| ChatEvent {
                        server_id: server.server_id.clone(),
                        room_id: 1,
                        event_id,
                        actor_user_id: Some(1),
                        actor_display_name: None,
                        at_unix: 1,
                        kind: ChatEventKind::Message {
                            body: "target".into(),
                        },
                    })
                    .collect(),
            )
            .expect("targets");
        for targets in (1..=super::super::model::CHAT_MESSAGE_REVISION_MAX_ROWS_PER_ROOM as u64)
            .collect::<Vec<_>>()
            .chunks(MESSAGE_REVISION_SNAPSHOT_MAX_ENTRIES)
        {
            store
                .replace_message_revision_snapshot(
                    &server.server_id,
                    1,
                    MessageRevisionSnapshot {
                        target_event_ids: targets.to_vec(),
                        entries: targets
                            .iter()
                            .map(|target| MessageRevisionSnapshotEntry {
                                target_event_id: *target,
                                latest_revision_event_id: target.saturating_add(10_000),
                                action: MessageRevisionAction::Tombstone,
                                actor_user_id: 1,
                                at_unix: 2,
                                replacement: None,
                                revision_number: 1,
                            })
                            .collect(),
                    },
                )
                .expect("bounded page");
        }
        let overflow_target = target_count as u64;
        assert!(store
            .replace_message_revision_snapshot(
                &server.server_id,
                1,
                MessageRevisionSnapshot {
                    target_event_ids: vec![overflow_target],
                    entries: vec![MessageRevisionSnapshotEntry {
                        target_event_id: overflow_target,
                        latest_revision_event_id: overflow_target.saturating_add(10_000),
                        action: MessageRevisionAction::Tombstone,
                        actor_user_id: 1,
                        at_unix: 3,
                        replacement: None,
                        revision_number: 1,
                    }],
                },
            )
            .is_err());
        assert_eq!(
            store
                .message_revisions_for_targets(&server.server_id, 1, &[1])
                .expect("prior state")
                .len(),
            1
        );
        assert!(store
            .message_revisions_for_targets(&server.server_id, 1, &[overflow_target])
            .expect("overflow rollback")
            .is_empty());
    }

    #[test]
    fn reaction_snapshot_overload_rolls_back_prior_page_state() {
        let mut store = SqliteChatStore::in_memory().expect("store");
        let server = sample_server();
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
            .expect("room");
        store
            .append_events(vec![ChatEvent {
                server_id: server.server_id.clone(),
                room_id: 1,
                event_id: 10,
                actor_user_id: Some(1),
                actor_display_name: None,
                at_unix: 1,
                kind: ChatEventKind::Message {
                    body: "target".into(),
                },
            }])
            .expect("target");
        store
            .apply_reaction_event(
                &server.server_id,
                1,
                ReactionEvent {
                    reaction_event_id: 1,
                    target_event_id: 10,
                    actor_user_id: 7,
                    token: ReactionToken::Heart,
                    action: ReactionAction::Add,
                    at_unix: 2,
                },
            )
            .expect("baseline");
        let overloaded = ReactionSnapshot {
            target_event_ids: vec![10],
            entries: [
                ReactionToken::ThumbsUp,
                ReactionToken::Heart,
                ReactionToken::Laugh,
                ReactionToken::Celebrate,
            ]
            .into_iter()
            .map(|token| ReactionSnapshotEntry {
                target_event_id: 10,
                actor_user_id: 7,
                token,
                created_at_unix: 3,
            })
            .collect(),
        };
        assert!(store
            .replace_reaction_snapshot(&server.server_id, 1, overloaded)
            .is_err());
        let retained = store
            .reactions_for_targets(&server.server_id, 1, &[10])
            .expect("retained baseline");
        assert_eq!(retained.len(), 1);
        assert_eq!(retained[0].token, ReactionToken::Heart);
        assert_eq!(retained[0].created_at_unix, 2);
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
        for (object_type, name) in [
            ("table", "room_reactions"),
            ("index", "idx_client_room_reactions_target"),
            ("table", "room_message_revision_state"),
            ("index", "idx_client_room_message_revisions_target"),
        ] {
            assert!(store
                .connection
                .query_row(
                    "SELECT EXISTS(
                       SELECT 1 FROM sqlite_master WHERE type = ?1 AND name = ?2
                     )",
                    (object_type, name),
                    |row| row.get::<_, bool>(0),
                )
                .expect("reaction schema object"));
        }
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
