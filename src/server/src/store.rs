use std::time::{SystemTime, UNIX_EPOCH};

use crate::error::ServerResult;
use crate::protocol::{EventId, RoomId, UserId};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServerRoom {
    pub room_id: RoomId,
    pub name: String,
    pub topic: Option<String>,
    pub room_revision: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServerUser {
    pub user_id: UserId,
    pub identity_hash: Vec<u8>,
    pub display_name: String,
    pub role_bits: u64,
    pub status_bits: u32,
    pub lxmf_destination: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServerRoomEvent {
    pub room_id: RoomId,
    pub event_id: EventId,
    pub kind: ServerRoomEventKind,
    pub actor_user_id: Option<UserId>,
    pub actor_display_name: Option<String>,
    pub at_unix: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServerUploadFile {
    pub resource_id: String,
    pub room_id: RoomId,
    pub actor_user_id: UserId,
    pub filename: String,
    pub content_type: Option<String>,
    pub byte_len: u64,
    pub path: String,
    pub created_at: i64,
}

pub struct RecordUploadFile<'a> {
    pub resource_id: &'a str,
    pub room_id: RoomId,
    pub actor_user_id: UserId,
    pub filename: &'a str,
    pub content_type: Option<&'a str>,
    pub byte_len: u64,
    pub path: &'a std::path::Path,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ServerRoomEventKind {
    Message {
        body: String,
    },
    Action {
        body: String,
    },
    Notice {
        body: String,
    },
    System {
        body: String,
    },
    Upload {
        resource_id: String,
        filename: String,
        bytes: u64,
    },
}

pub struct OmenchatStore {
    connection: rusqlite::Connection,
}

impl OmenchatStore {
    pub fn open(path: impl AsRef<std::path::Path>) -> ServerResult<Self> {
        if let Some(parent) = path.as_ref().parent() {
            std::fs::create_dir_all(parent)?;
        }
        let store = Self {
            connection: rusqlite::Connection::open(path)?,
        };
        store.migrate()?;
        Ok(store)
    }

    pub fn in_memory() -> ServerResult<Self> {
        let store = Self {
            connection: rusqlite::Connection::open_in_memory()?,
        };
        store.migrate()?;
        store.ensure_room("lobby", Some("Default OMENchat lobby"))?;
        Ok(store)
    }

    fn migrate(&self) -> ServerResult<()> {
        self.connection
            .execute_batch(include_str!("../migrations/001_init.sql"))?;
        Ok(())
    }

    pub fn ensure_room(&self, name: &str, topic: Option<&str>) -> ServerResult<ServerRoom> {
        let now = current_unix_seconds();
        self.connection.execute(
            "INSERT OR IGNORE INTO rooms(name, topic, created_at) VALUES (?1, ?2, ?3)",
            (name, topic, now),
        )?;
        self.room_by_name(name)?
            .ok_or_else(|| crate::error::ServerError::Message("room was not created".into()))
    }

    pub fn create_room(&self, name: &str, topic: Option<&str>) -> ServerResult<ServerRoom> {
        let room_name = normalize_room_name(name);
        if room_name.is_empty() {
            return Err(crate::error::ServerError::Message(
                "room name must contain at least one ASCII letter, digit, '_' or '-'".into(),
            ));
        }
        let now = current_unix_seconds();
        self.connection.execute(
            "INSERT INTO rooms(name, topic, created_at) VALUES (?1, ?2, ?3)
             ON CONFLICT(name) DO UPDATE SET
               topic = COALESCE(excluded.topic, rooms.topic),
               archived = 0,
               room_revision = room_revision + 1",
            (&room_name, topic, now),
        )?;
        self.room_by_name(&room_name)?
            .ok_or_else(|| crate::error::ServerError::Message("room was not created".into()))
    }

    pub fn room_by_name(&self, name: &str) -> ServerResult<Option<ServerRoom>> {
        let mut statement = self.connection.prepare(
            "SELECT room_id, name, topic, room_revision
             FROM rooms
             WHERE name = ?1 AND archived = 0",
        )?;
        let mut rows = statement.query_map([name], room_from_row)?;
        rows.next().transpose().map_err(Into::into)
    }

    pub fn room_by_id(&self, room_id: RoomId) -> ServerResult<Option<ServerRoom>> {
        let mut statement = self.connection.prepare(
            "SELECT room_id, name, topic, room_revision
             FROM rooms
             WHERE room_id = ?1 AND archived = 0",
        )?;
        let mut rows = statement.query_map([room_id], room_from_row)?;
        rows.next().transpose().map_err(Into::into)
    }

    pub fn list_rooms(&self) -> ServerResult<Vec<ServerRoom>> {
        let mut statement = self.connection.prepare(
            "SELECT room_id, name, topic, room_revision
             FROM rooms
             WHERE archived = 0
             ORDER BY name",
        )?;
        let rows = statement.query_map([], room_from_row)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn update_room_topic(
        &self,
        room_id: RoomId,
        topic: Option<&str>,
    ) -> ServerResult<ServerRoom> {
        self.connection.execute(
            "UPDATE rooms
             SET topic = ?1, room_revision = room_revision + 1
             WHERE room_id = ?2 AND archived = 0",
            (topic, room_id as i64),
        )?;
        self.room_by_id(room_id)?.ok_or_else(|| {
            crate::error::ServerError::Message("room was not found after topic update".into())
        })
    }

    pub fn ensure_user(
        &self,
        identity_hash: &[u8],
        display_name: &str,
        lxmf_destination: Option<&str>,
    ) -> ServerResult<ServerUser> {
        let now = current_unix_seconds();
        self.connection.execute(
            "INSERT INTO users(
               rns_identity_hash, display_name, lxmf_destination, first_seen_at, last_seen_at
             ) VALUES (?1, ?2, ?3, ?4, ?4)
             ON CONFLICT(rns_identity_hash) DO UPDATE SET
               display_name = excluded.display_name,
               lxmf_destination = COALESCE(excluded.lxmf_destination, users.lxmf_destination),
               last_seen_at = excluded.last_seen_at",
            (identity_hash, display_name, lxmf_destination, now),
        )?;
        self.user_by_identity(identity_hash)?
            .ok_or_else(|| crate::error::ServerError::Message("user was not created".into()))
    }

    pub fn user_by_identity(&self, identity_hash: &[u8]) -> ServerResult<Option<ServerUser>> {
        let mut statement = self.connection.prepare(
            "SELECT user_id, rns_identity_hash, display_name, role_bits, status_bits, lxmf_destination
             FROM users
             WHERE rns_identity_hash = ?1",
        )?;
        let mut rows = statement.query_map([identity_hash], user_from_row)?;
        rows.next().transpose().map_err(Into::into)
    }

    pub fn users(&self) -> ServerResult<Vec<ServerUser>> {
        let mut statement = self.connection.prepare(
            "SELECT user_id, rns_identity_hash, display_name, role_bits, status_bits, lxmf_destination
             FROM users
             ORDER BY display_name, user_id",
        )?;
        let rows = statement.query_map([], user_from_row)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn set_user_status_flag(
        &self,
        user_id: UserId,
        flag: u32,
        enabled: bool,
    ) -> ServerResult<ServerUser> {
        let current: i64 = self.connection.query_row(
            "SELECT status_bits FROM users WHERE user_id = ?1",
            [user_id as i64],
            |row| row.get(0),
        )?;
        let mut next = current as u32;
        if enabled {
            next |= flag;
        } else {
            next &= !flag;
        }
        self.connection.execute(
            "UPDATE users SET status_bits = ?1 WHERE user_id = ?2",
            (next as i64, user_id as i64),
        )?;
        self.user_by_id(user_id)?
            .ok_or_else(|| crate::error::ServerError::Message("user was not found".into()))
    }

    pub fn set_user_role_bits(&self, user_id: UserId, role_bits: u64) -> ServerResult<ServerUser> {
        self.connection.execute(
            "UPDATE users SET role_bits = ?1 WHERE user_id = ?2",
            (role_bits as i64, user_id as i64),
        )?;
        self.user_by_id(user_id)?
            .ok_or_else(|| crate::error::ServerError::Message("user was not found".into()))
    }

    pub fn join_room(&self, room_id: RoomId, user_id: UserId) -> ServerResult<()> {
        let now = current_unix_seconds();
        self.connection.execute(
            "INSERT INTO room_members(room_id, user_id, joined_at, last_seen_at)
             VALUES (?1, ?2, ?3, ?3)
             ON CONFLICT(room_id, user_id) DO UPDATE SET last_seen_at = excluded.last_seen_at",
            (room_id, user_id, now),
        )?;
        Ok(())
    }

    pub fn leave_room(&self, room_id: RoomId, user_id: UserId) -> ServerResult<()> {
        self.connection.execute(
            "DELETE FROM room_members WHERE room_id = ?1 AND user_id = ?2",
            (room_id, user_id),
        )?;
        Ok(())
    }

    pub fn room_has_member(&self, room_id: RoomId, user_id: UserId) -> ServerResult<bool> {
        let count: i64 = self.connection.query_row(
            "SELECT COUNT(*) FROM room_members WHERE room_id = ?1 AND user_id = ?2",
            (room_id, user_id),
            |row| row.get(0),
        )?;
        Ok(count > 0)
    }

    pub fn users_for_room(&self, room_id: RoomId) -> ServerResult<Vec<ServerUser>> {
        let mut statement = self.connection.prepare(
            "SELECT u.user_id, u.rns_identity_hash, u.display_name, m.role_bits, m.status_bits, u.lxmf_destination
             FROM room_members m
             JOIN users u ON u.user_id = m.user_id
             WHERE m.room_id = ?1
             ORDER BY u.display_name",
        )?;
        let rows = statement.query_map([room_id], user_from_row)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn append_event(
        &self,
        room_id: RoomId,
        actor_user_id: Option<UserId>,
        kind: ServerRoomEventKind,
    ) -> ServerResult<ServerRoomEvent> {
        let event_id = self.next_event_id(room_id)?;
        let at_unix = current_unix_seconds();
        let (kind_code, payload) = encode_event_kind(&kind);
        self.connection.execute(
            "INSERT INTO room_events(room_id, event_id, event_kind, actor_user_id, at, payload)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            (
                room_id,
                event_id,
                kind_code,
                actor_user_id,
                at_unix,
                payload,
            ),
        )?;
        Ok(ServerRoomEvent {
            room_id,
            event_id,
            kind,
            actor_user_id,
            actor_display_name: actor_user_id
                .and_then(|user_id| self.user_by_id(user_id).ok().flatten())
                .map(|user| user.display_name),
            at_unix,
        })
    }

    pub fn record_upload_file(&self, upload: RecordUploadFile<'_>) -> ServerResult<()> {
        let created_at = current_unix_seconds();
        self.connection.execute(
            "INSERT OR REPLACE INTO upload_files(
               resource_id, room_id, actor_user_id, filename, content_type, byte_len, path, created_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            (
                upload.resource_id,
                upload.room_id,
                upload.actor_user_id,
                upload.filename,
                upload.content_type,
                upload.byte_len as i64,
                upload.path.display().to_string(),
                created_at,
            ),
        )?;
        Ok(())
    }

    pub fn upload_file(&self, resource_id: &str) -> ServerResult<Option<ServerUploadFile>> {
        let mut statement = self.connection.prepare(
            "SELECT resource_id, room_id, actor_user_id, filename, content_type, byte_len, path, created_at
             FROM upload_files
             WHERE resource_id = ?1",
        )?;
        let mut rows = statement.query_map([resource_id], upload_file_from_row)?;
        rows.next().transpose().map_err(Into::into)
    }

    pub fn user_by_id(&self, user_id: UserId) -> ServerResult<Option<ServerUser>> {
        let mut statement = self.connection.prepare(
            "SELECT user_id, rns_identity_hash, display_name, role_bits, status_bits, lxmf_destination
             FROM users
             WHERE user_id = ?1",
        )?;
        let mut rows = statement.query_map([user_id as i64], user_from_row)?;
        rows.next().transpose().map_err(Into::into)
    }

    pub fn latest_events(
        &self,
        room_id: RoomId,
        limit: usize,
    ) -> ServerResult<Vec<ServerRoomEvent>> {
        let mut events = self.query_events(
            "SELECT e.room_id, e.event_id, e.event_kind, e.actor_user_id, e.at, e.payload, u.display_name
             FROM room_events e
             LEFT JOIN users u ON u.user_id = e.actor_user_id
             WHERE e.room_id = ?1 AND e.deleted = 0
             ORDER BY e.event_id DESC
             LIMIT ?2",
            (room_id, limit as i64),
        )?;
        events.reverse();
        Ok(events)
    }

    pub fn events_before(
        &self,
        room_id: RoomId,
        before_event_id: EventId,
        limit: usize,
    ) -> ServerResult<Vec<ServerRoomEvent>> {
        let mut events = self.query_events(
            "SELECT e.room_id, e.event_id, e.event_kind, e.actor_user_id, e.at, e.payload, u.display_name
             FROM room_events e
             LEFT JOIN users u ON u.user_id = e.actor_user_id
             WHERE e.room_id = ?1 AND e.event_id < ?2 AND e.deleted = 0
             ORDER BY e.event_id DESC
             LIMIT ?3",
            (room_id, before_event_id, limit as i64),
        )?;
        events.reverse();
        Ok(events)
    }

    fn next_event_id(&self, room_id: RoomId) -> ServerResult<EventId> {
        let event_id = self.connection.query_row(
            "SELECT COALESCE(MAX(event_id), 0) + 1 FROM room_events WHERE room_id = ?1",
            [room_id],
            |row| row.get::<_, i64>(0),
        )?;
        Ok(event_id as EventId)
    }

    fn query_events<P>(&self, sql: &str, params: P) -> ServerResult<Vec<ServerRoomEvent>>
    where
        P: rusqlite::Params,
    {
        let mut statement = self.connection.prepare(sql)?;
        let rows = statement.query_map(params, |row| {
            let kind: i64 = row.get(2)?;
            let payload: Option<Vec<u8>> = row.get(5)?;
            Ok(ServerRoomEvent {
                room_id: row.get::<_, i64>(0)? as RoomId,
                event_id: row.get::<_, i64>(1)? as EventId,
                kind: decode_event_kind(kind, payload.unwrap_or_default()),
                actor_user_id: row.get::<_, Option<i64>>(3)?.map(|value| value as UserId),
                actor_display_name: row.get(6)?,
                at_unix: row.get(4)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }
}

fn room_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ServerRoom> {
    Ok(ServerRoom {
        room_id: row.get::<_, i64>(0)? as RoomId,
        name: row.get(1)?,
        topic: row.get(2)?,
        room_revision: row.get::<_, i64>(3)? as u64,
    })
}

fn user_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ServerUser> {
    Ok(ServerUser {
        user_id: row.get::<_, i64>(0)? as UserId,
        identity_hash: row.get(1)?,
        display_name: row.get(2)?,
        role_bits: row.get::<_, i64>(3)? as u64,
        status_bits: row.get::<_, i64>(4)? as u32,
        lxmf_destination: row.get(5)?,
    })
}

fn upload_file_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ServerUploadFile> {
    Ok(ServerUploadFile {
        resource_id: row.get(0)?,
        room_id: row.get::<_, i64>(1)? as RoomId,
        actor_user_id: row.get::<_, i64>(2)? as UserId,
        filename: row.get(3)?,
        content_type: row.get(4)?,
        byte_len: row.get::<_, i64>(5)? as u64,
        path: row.get(6)?,
        created_at: row.get(7)?,
    })
}

fn encode_event_kind(kind: &ServerRoomEventKind) -> (i64, Vec<u8>) {
    match kind {
        ServerRoomEventKind::Message { body } => (1, body.as_bytes().to_vec()),
        ServerRoomEventKind::Action { body } => (2, body.as_bytes().to_vec()),
        ServerRoomEventKind::Notice { body } => (3, body.as_bytes().to_vec()),
        ServerRoomEventKind::System { body } => (4, body.as_bytes().to_vec()),
        ServerRoomEventKind::Upload {
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
            )
            .into_bytes(),
        ),
    }
}

fn decode_event_kind(kind: i64, payload: Vec<u8>) -> ServerRoomEventKind {
    let body = String::from_utf8_lossy(&payload).into_owned();
    match kind {
        1 => ServerRoomEventKind::Message { body },
        2 => ServerRoomEventKind::Action { body },
        3 => ServerRoomEventKind::Notice { body },
        5 => decode_upload_event_kind(&body).unwrap_or(ServerRoomEventKind::System { body }),
        _ => ServerRoomEventKind::System { body },
    }
}

fn decode_upload_event_kind(body: &str) -> Option<ServerRoomEventKind> {
    let mut parts = body.splitn(3, '\u{1f}');
    let resource_id = parts.next()?.trim().to_owned();
    let filename = parts.next()?.trim().to_owned();
    let bytes = parts.next()?.trim().parse::<u64>().ok()?;
    if resource_id.is_empty() || filename.is_empty() {
        return None;
    }
    Some(ServerRoomEventKind::Upload {
        resource_id,
        filename,
        bytes,
    })
}

fn current_unix_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}

fn normalize_room_name(name: &str) -> String {
    name.trim()
        .trim_start_matches('#')
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || *ch == '_' || *ch == '-')
        .take(48)
        .collect::<String>()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn store_tracks_room_users_messages_and_history() {
        let store = OmenchatStore::in_memory().expect("store");
        let room = store.ensure_room("radio", Some("field ops")).expect("room");
        let user = store
            .ensure_user(b"identity-a", "Alice", Some("lxmf-a"))
            .expect("user");
        store.join_room(room.room_id, user.user_id).expect("join");

        let event = store
            .append_event(
                room.room_id,
                Some(user.user_id),
                ServerRoomEventKind::Message {
                    body: "hello".into(),
                },
            )
            .expect("message");

        assert_eq!(store.users_for_room(room.room_id).expect("users").len(), 1);
        assert_eq!(event.event_id, 1);
        assert_eq!(event.actor_display_name.as_deref(), Some("Alice"));
        assert_eq!(
            store.latest_events(room.room_id, 20).expect("latest")[0].kind,
            ServerRoomEventKind::Message {
                body: "hello".into()
            }
        );
        assert_eq!(
            store.latest_events(room.room_id, 20).expect("latest")[0]
                .actor_display_name
                .as_deref(),
            Some("Alice")
        );
        assert!(store
            .events_before(room.room_id, event.event_id, 20)
            .expect("history")
            .is_empty());
    }
}
