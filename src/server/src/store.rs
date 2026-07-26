use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rusqlite::OptionalExtension;

use crate::error::ServerResult;
use crate::protocol::{EventId, RichMessageEventMetadata, RoomId, UserId};

pub mod durable_replay;
pub mod reactions;

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
pub struct ServerAdminUser {
    pub user: ServerUser,
    pub first_seen_at: i64,
    pub last_seen_at: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServerRoomEvent {
    pub room_id: RoomId,
    pub event_id: EventId,
    pub kind: ServerRoomEventKind,
    pub actor_user_id: Option<UserId>,
    pub actor_display_name: Option<String>,
    pub at_unix: i64,
    pub metadata: Option<RichMessageEventMetadata>,
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

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UploadLedgerReconciliation {
    pub tracked_files: usize,
    pub tracked_bytes: u64,
    pub disk_files: usize,
    pub disk_bytes: u64,
    pub missing_paths: Vec<std::path::PathBuf>,
    pub mismatched_paths: Vec<std::path::PathBuf>,
    pub orphan_paths: Vec<std::path::PathBuf>,
    pub unsafe_paths: Vec<std::path::PathBuf>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UploadLedgerRepair {
    pub removed_missing_records: usize,
    pub removed_unsafe_records: usize,
    pub preserved_orphan_paths: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UploadLedgerQuotaPlan {
    pub current_bytes: u64,
    pub evict_paths: Vec<std::path::PathBuf>,
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
    verified_upload_ledgers: std::sync::Mutex<std::collections::BTreeSet<UserId>>,
}

const SQLITE_BUSY_TIMEOUT: Duration = Duration::from_secs(5);
pub(crate) const SCHEMA_VERSION: i64 = 5;

impl OmenchatStore {
    pub fn open(path: impl AsRef<std::path::Path>) -> ServerResult<Self> {
        Self::open_with_timeout(path, SQLITE_BUSY_TIMEOUT)
    }

    pub fn open_read_only(path: impl AsRef<std::path::Path>) -> ServerResult<Self> {
        let connection = rusqlite::Connection::open_with_flags(
            path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )?;
        let version: i64 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
        if version > SCHEMA_VERSION {
            return Err(crate::error::ServerError::Message(format!(
                "database schema version {version} is newer than supported version {SCHEMA_VERSION}"
            )));
        }
        Ok(Self::from_connection(connection))
    }

    pub fn open_existing_for_maintenance(path: impl AsRef<std::path::Path>) -> ServerResult<Self> {
        let connection = rusqlite::Connection::open_with_flags(
            path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE,
        )?;
        connection.busy_timeout(SQLITE_BUSY_TIMEOUT)?;
        let version: i64 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
        if version != SCHEMA_VERSION {
            return Err(crate::error::ServerError::Message(format!(
                "maintenance requires database schema version {SCHEMA_VERSION}, found {version}; start the matching omenchatd version normally to perform any supported migration first"
            )));
        }
        Ok(Self::from_connection(connection))
    }

    fn open_with_timeout(
        path: impl AsRef<std::path::Path>,
        busy_timeout: Duration,
    ) -> ServerResult<Self> {
        let backup_required = path
            .as_ref()
            .metadata()
            .map(|metadata| metadata.len() > 0)
            .unwrap_or(false);
        if let Some(parent) = path.as_ref().parent() {
            std::fs::create_dir_all(parent)?;
        }
        let connection = rusqlite::Connection::open(path.as_ref())?;
        configure_connection(&connection, true, busy_timeout)?;
        let store = Self::from_connection(connection);
        store.migrate(backup_required.then_some(path.as_ref()))?;
        Ok(store)
    }

    #[cfg(all(test, feature = "live-reticulum"))]
    pub(crate) fn open_for_lock_test(
        path: impl AsRef<std::path::Path>,
        busy_timeout: Duration,
    ) -> ServerResult<Self> {
        Self::open_with_timeout(path, busy_timeout)
    }

    pub fn in_memory() -> ServerResult<Self> {
        let connection = rusqlite::Connection::open_in_memory()?;
        configure_connection(&connection, false, SQLITE_BUSY_TIMEOUT)?;
        let store = Self::from_connection(connection);
        store.migrate(None)?;
        store.ensure_room("lobby", Some("Default OMENchat lobby"))?;
        Ok(store)
    }

    fn migrate(&self, backup_source: Option<&std::path::Path>) -> ServerResult<()> {
        self.migrate_with_sql(backup_source, include_str!("../migrations/001_init.sql"))
    }

    fn from_connection(connection: rusqlite::Connection) -> Self {
        Self {
            connection,
            verified_upload_ledgers: std::sync::Mutex::new(std::collections::BTreeSet::new()),
        }
    }

    fn migrate_with_sql(
        &self,
        backup_source: Option<&std::path::Path>,
        migration_sql: &str,
    ) -> ServerResult<()> {
        self.migrate_with_sql_and_step(backup_source, migration_sql, ensure_event_metadata_schema)
    }

    fn migrate_with_sql_and_step<F>(
        &self,
        backup_source: Option<&std::path::Path>,
        migration_sql: &str,
        schema_step: F,
    ) -> ServerResult<()>
    where
        F: FnOnce(&rusqlite::Transaction<'_>) -> ServerResult<()>,
    {
        self.migrate_with_sql_step_and_reaction_hook(
            backup_source,
            migration_sql,
            schema_step,
            |_| Ok(()),
        )
    }

    fn migrate_with_sql_step_and_reaction_hook<F, H>(
        &self,
        backup_source: Option<&std::path::Path>,
        migration_sql: &str,
        schema_step: F,
        mut reaction_hook: H,
    ) -> ServerResult<()>
    where
        F: FnOnce(&rusqlite::Transaction<'_>) -> ServerResult<()>,
        H: FnMut(ReactionMigrationBoundary) -> ServerResult<()>,
    {
        let current_version: i64 =
            self.connection
                .pragma_query_value(None, "user_version", |row| row.get(0))?;
        if current_version > SCHEMA_VERSION {
            return Err(crate::error::ServerError::Message(format!(
                "database schema version {current_version} is newer than supported version {SCHEMA_VERSION}"
            )));
        }
        if current_version == SCHEMA_VERSION {
            return Ok(());
        }

        if let Some(source_path) = backup_source {
            create_migration_backup(&self.connection, source_path, current_version)?;
        }

        let transaction = rusqlite::Transaction::new_unchecked(
            &self.connection,
            rusqlite::TransactionBehavior::Immediate,
        )?;
        transaction.execute_batch(migration_sql)?;
        schema_step(&transaction)?;
        ensure_reaction_schema_with_hook(&transaction, &mut reaction_hook)?;
        reaction_hook(ReactionMigrationBoundary::BeforeVersionUpdate)?;
        transaction.pragma_update(None, "user_version", SCHEMA_VERSION)?;
        reaction_hook(ReactionMigrationBoundary::BeforeCommit)?;
        transaction.commit()?;
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

    pub fn archive_room(&self, room_id: RoomId) -> ServerResult<()> {
        if room_id == 1 {
            return Err(crate::error::ServerError::Message(
                "the lobby room cannot be archived".into(),
            ));
        }
        let changed = self.connection.execute(
            "UPDATE rooms
             SET archived = 1, room_revision = room_revision + 1
             WHERE room_id = ?1 AND archived = 0",
            [room_id],
        )?;
        if changed == 0 {
            return Err(crate::error::ServerError::Message("room not found".into()));
        }
        Ok(())
    }

    pub fn ensure_user(
        &self,
        identity_hash: &[u8],
        display_name: &str,
        lxmf_destination: Option<&str>,
    ) -> ServerResult<ServerUser> {
        ensure_user_on(
            &self.connection,
            identity_hash,
            display_name,
            lxmf_destination,
        )
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

    pub fn administrative_users(&self) -> ServerResult<Vec<ServerAdminUser>> {
        let mut statement = self.connection.prepare(
            "SELECT user_id, rns_identity_hash, display_name, role_bits, status_bits, lxmf_destination, first_seen_at, last_seen_at
             FROM users
             ORDER BY COALESCE(last_seen_at, first_seen_at) DESC, display_name",
        )?;
        let rows = statement.query_map([], admin_user_from_row)?;
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

    pub fn set_user_role_flag(
        &self,
        user_id: UserId,
        flag: u64,
        enabled: bool,
    ) -> ServerResult<ServerUser> {
        let current: i64 = self.connection.query_row(
            "SELECT role_bits FROM users WHERE user_id = ?1",
            [user_id as i64],
            |row| row.get(0),
        )?;
        let mut next = current as u64;
        if enabled {
            next |= flag;
        } else {
            next &= !flag;
        }
        self.set_user_role_bits(user_id, next)
    }

    pub fn delete_users(&self, user_ids: &[UserId]) -> ServerResult<usize> {
        let transaction = self.connection.unchecked_transaction()?;
        let mut deleted = 0usize;
        for user_id in user_ids {
            transaction.execute(
                "DELETE FROM room_members WHERE user_id = ?1",
                [*user_id as i64],
            )?;
            deleted = deleted.saturating_add(
                transaction.execute("DELETE FROM users WHERE user_id = ?1", [*user_id as i64])?,
            );
        }
        transaction.commit()?;
        Ok(deleted)
    }

    pub fn join_room(&self, room_id: RoomId, user_id: UserId) -> ServerResult<()> {
        join_room_on(&self.connection, room_id, user_id)
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
        // Acquire the single-writer reservation before reading the current
        // maximum. The ID allocation and insert must be one transaction so
        // separate server connections cannot select the same next ID.
        let transaction = rusqlite::Transaction::new_unchecked(
            &self.connection,
            rusqlite::TransactionBehavior::Immediate,
        )?;
        let event = append_event_in_transaction(&transaction, room_id, actor_user_id, kind)?;
        transaction.commit()?;
        Ok(event)
    }

    #[cfg(test)]
    pub(crate) fn mark_event_deleted_for_test(
        &self,
        room_id: RoomId,
        event_id: EventId,
    ) -> ServerResult<()> {
        self.connection.execute(
            "UPDATE room_events SET deleted = 1
             WHERE room_id = ?1 AND event_id = ?2",
            (room_id, event_id),
        )?;
        Ok(())
    }

    pub fn record_upload_file(&self, upload: RecordUploadFile<'_>) -> ServerResult<()> {
        let created_at = current_unix_seconds();
        let transaction = self.connection.unchecked_transaction()?;
        transaction.execute(
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
        transaction.commit()?;
        Ok(())
    }

    pub fn remove_evicted_upload_records(
        &self,
        actor_user_id: UserId,
        retained_resource_id: &str,
        evicted_paths: &[std::path::PathBuf],
    ) -> ServerResult<usize> {
        let transaction = self.connection.unchecked_transaction()?;
        let mut removed = 0usize;
        for path in evicted_paths {
            removed = removed.saturating_add(transaction.execute(
                "DELETE FROM upload_files
                 WHERE actor_user_id = ?1 AND path = ?2 AND resource_id != ?3",
                (
                    actor_user_id as i64,
                    path.display().to_string(),
                    retained_resource_id,
                ),
            )?);
        }
        transaction.commit()?;
        Ok(removed)
    }

    pub fn invalidate_upload_ledger(&self, actor_user_id: UserId) {
        if let Ok(mut verified) = self.verified_upload_ledgers.lock() {
            verified.remove(&actor_user_id);
        }
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

    pub fn reconcile_upload_ledger(
        &self,
        actor_user_id: UserId,
        identity_dir: &std::path::Path,
    ) -> ServerResult<UploadLedgerReconciliation> {
        let mut statement = self.connection.prepare(
            "SELECT resource_id, room_id, actor_user_id, filename, content_type, byte_len, path, created_at
             FROM upload_files
             WHERE actor_user_id = ?1
             ORDER BY created_at, resource_id",
        )?;
        let tracked = statement
            .query_map([actor_user_id as i64], upload_file_from_row)?
            .collect::<Result<Vec<_>, _>>()?;
        let mut report = UploadLedgerReconciliation {
            tracked_files: tracked.len(),
            tracked_bytes: tracked
                .iter()
                .fold(0u64, |total, upload| total.saturating_add(upload.byte_len)),
            ..UploadLedgerReconciliation::default()
        };
        let tracked_paths = tracked
            .iter()
            .map(|upload| std::path::PathBuf::from(&upload.path))
            .collect::<std::collections::BTreeSet<_>>();
        for upload in &tracked {
            let path = std::path::PathBuf::from(&upload.path);
            if !path.starts_with(identity_dir) {
                report.unsafe_paths.push(path);
            } else {
                match std::fs::symlink_metadata(&path) {
                    Ok(metadata) if metadata.file_type().is_file() => {
                        if metadata.len() != upload.byte_len {
                            report.mismatched_paths.push(path);
                        }
                    }
                    Ok(_) => report.unsafe_paths.push(path),
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                        report.missing_paths.push(path);
                    }
                    Err(error) => return Err(error.into()),
                }
            }
        }
        match std::fs::read_dir(identity_dir) {
            Ok(entries) => {
                for entry in entries {
                    let entry = entry?;
                    let path = entry.path();
                    let metadata = std::fs::symlink_metadata(&path)?;
                    if metadata.file_type().is_file() {
                        report.disk_files = report.disk_files.saturating_add(1);
                        report.disk_bytes = report.disk_bytes.saturating_add(metadata.len());
                        if !tracked_paths.contains(&path) {
                            report.orphan_paths.push(path);
                        }
                    } else if metadata.file_type().is_symlink() {
                        report.orphan_paths.push(path);
                    }
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        report.missing_paths.sort();
        report.mismatched_paths.sort();
        report.orphan_paths.sort();
        report.unsafe_paths.sort();
        Ok(report)
    }

    pub fn plan_upload_from_index(
        &self,
        actor_user_id: UserId,
        identity_dir: &std::path::Path,
        incoming_bytes: u64,
        quota_bytes: u64,
    ) -> ServerResult<UploadLedgerQuotaPlan> {
        let mut verified = self.verified_upload_ledgers.lock().map_err(|_| {
            crate::error::ServerError::Message("upload ledger trust lock poisoned".into())
        })?;
        if !verified.contains(&actor_user_id) {
            let report = self.reconcile_upload_ledger(actor_user_id, identity_dir)?;
            if !report.missing_paths.is_empty()
                || !report.mismatched_paths.is_empty()
                || !report.orphan_paths.is_empty()
                || !report.unsafe_paths.is_empty()
            {
                return Err(crate::error::ServerError::Message(format!(
                    "upload ledger for user {actor_user_id} is not clean (missing={}, mismatched={}, orphan={}, unsafe={}); stop the server, run `omenchatd doctor`, and resolve discrepancies before accepting uploads",
                    report.missing_paths.len(),
                    report.mismatched_paths.len(),
                    report.orphan_paths.len(),
                    report.unsafe_paths.len()
                )));
            }
            verified.insert(actor_user_id);
        }
        drop(verified);

        let mut statement = self.connection.prepare(
            "SELECT byte_len, path
             FROM upload_files
             WHERE actor_user_id = ?1
             ORDER BY created_at, resource_id",
        )?;
        let entries = statement
            .query_map([actor_user_id as i64], |row| {
                Ok((
                    row.get::<_, u64>(0)?,
                    std::path::PathBuf::from(row.get::<_, String>(1)?),
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let current_bytes = entries
            .iter()
            .fold(0u64, |total, (bytes, _)| total.saturating_add(*bytes));
        let mut remaining = current_bytes.saturating_add(incoming_bytes);
        let mut evict_paths = Vec::new();
        for (bytes, path) in entries {
            if remaining <= quota_bytes {
                break;
            }
            remaining = remaining.saturating_sub(bytes);
            evict_paths.push(path);
        }
        Ok(UploadLedgerQuotaPlan {
            current_bytes,
            evict_paths,
        })
    }

    pub fn repair_upload_ledger_records(
        &self,
        actor_user_id: UserId,
        identity_dir: &std::path::Path,
    ) -> ServerResult<UploadLedgerRepair> {
        let report = self.reconcile_upload_ledger(actor_user_id, identity_dir)?;
        let transaction = rusqlite::Transaction::new_unchecked(
            &self.connection,
            rusqlite::TransactionBehavior::Immediate,
        )?;
        let mut removed_missing_records = 0usize;
        for path in &report.missing_paths {
            removed_missing_records = removed_missing_records.saturating_add(transaction.execute(
                "DELETE FROM upload_files WHERE actor_user_id = ?1 AND path = ?2",
                (actor_user_id as i64, path.display().to_string()),
            )?);
        }
        let mut removed_unsafe_records = 0usize;
        for path in &report.unsafe_paths {
            removed_unsafe_records = removed_unsafe_records.saturating_add(transaction.execute(
                "DELETE FROM upload_files WHERE actor_user_id = ?1 AND path = ?2",
                (actor_user_id as i64, path.display().to_string()),
            )?);
        }
        transaction.commit()?;
        Ok(UploadLedgerRepair {
            removed_missing_records,
            removed_unsafe_records,
            preserved_orphan_paths: report.orphan_paths.len(),
        })
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
            "SELECT e.room_id, e.event_id, e.event_kind, e.actor_user_id, e.at, e.payload, u.display_name,
                    e.reply_to_event_id, e.mention_user_ids
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
            "SELECT e.room_id, e.event_id, e.event_kind, e.actor_user_id, e.at, e.payload, u.display_name,
                    e.reply_to_event_id, e.mention_user_ids
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

    fn query_events<P>(&self, sql: &str, params: P) -> ServerResult<Vec<ServerRoomEvent>>
    where
        P: rusqlite::Params,
    {
        let mut statement = self.connection.prepare(sql)?;
        let mut rows = statement.query(params)?;
        let mut events = Vec::new();
        while let Some(row) = rows.next()? {
            let kind: i64 = row.get(2)?;
            let payload: Option<Vec<u8>> = row.get(5)?;
            let metadata = decode_stored_event_metadata(row.get(7)?, row.get(8)?)?;
            events.push(ServerRoomEvent {
                room_id: row.get::<_, i64>(0)? as RoomId,
                event_id: row.get::<_, i64>(1)? as EventId,
                kind: decode_event_kind(kind, payload.unwrap_or_default()),
                actor_user_id: row.get::<_, Option<i64>>(3)?.map(|value| value as UserId),
                actor_display_name: row.get(6)?,
                at_unix: row.get(4)?,
                metadata,
            });
        }
        Ok(events)
    }
}

fn ensure_event_metadata_schema(transaction: &rusqlite::Transaction<'_>) -> ServerResult<()> {
    let mut statement = transaction.prepare("PRAGMA table_info(room_events)")?;
    let columns = statement
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<Result<std::collections::BTreeSet<_>, _>>()?;
    drop(statement);

    if !columns.contains("reply_to_event_id") {
        transaction.execute_batch(
            "ALTER TABLE room_events
             ADD COLUMN reply_to_event_id INTEGER
             CHECK(reply_to_event_id IS NULL OR reply_to_event_id > 0);",
        )?;
    }
    if !columns.contains("mention_user_ids") {
        transaction.execute_batch(
            "ALTER TABLE room_events
             ADD COLUMN mention_user_ids BLOB
             CHECK(
               mention_user_ids IS NULL OR (
                 length(mention_user_ids) BETWEEN 4 AND 64
                 AND length(mention_user_ids) % 4 = 0
               )
             );",
        )?;
    }
    transaction.execute_batch(
        "CREATE INDEX IF NOT EXISTS idx_room_events_reply
         ON room_events(room_id, reply_to_event_id)
         WHERE reply_to_event_id IS NOT NULL;",
    )?;
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ReactionMigrationBoundary {
    BeforeTables,
    BetweenTables,
    BeforeIndexes,
    BeforeVersionUpdate,
    BeforeCommit,
}

fn ensure_reaction_schema_with_hook<H>(
    transaction: &rusqlite::Transaction<'_>,
    hook: &mut H,
) -> ServerResult<()>
where
    H: FnMut(ReactionMigrationBoundary) -> ServerResult<()>,
{
    hook(ReactionMigrationBoundary::BeforeTables)?;
    transaction.execute_batch(
        "CREATE TABLE IF NOT EXISTS room_reactions(
           room_id INTEGER NOT NULL,
           target_event_id INTEGER NOT NULL CHECK(target_event_id > 0),
           actor_user_id INTEGER NOT NULL CHECK(actor_user_id > 0),
           reaction_token TEXT NOT NULL CHECK(length(reaction_token) BETWEEN 1 AND 16),
           created_at INTEGER NOT NULL,
           PRIMARY KEY(room_id, target_event_id, actor_user_id, reaction_token)
         );",
    )?;

    hook(ReactionMigrationBoundary::BetweenTables)?;
    transaction.execute_batch(
        "CREATE TABLE IF NOT EXISTS room_reaction_events(
           room_id INTEGER NOT NULL,
           reaction_event_id INTEGER NOT NULL CHECK(reaction_event_id > 0),
           target_event_id INTEGER NOT NULL CHECK(target_event_id > 0),
           actor_user_id INTEGER NOT NULL CHECK(actor_user_id > 0),
           reaction_token TEXT NOT NULL CHECK(length(reaction_token) BETWEEN 1 AND 16),
           reaction_action INTEGER NOT NULL CHECK(reaction_action IN (1, 2)),
           at INTEGER NOT NULL,
           retained_bytes INTEGER NOT NULL CHECK(retained_bytes >= 0),
           PRIMARY KEY(room_id, reaction_event_id)
         );",
    )?;

    hook(ReactionMigrationBoundary::BeforeIndexes)?;
    transaction.execute_batch(
        "CREATE INDEX IF NOT EXISTS idx_room_reactions_target
         ON room_reactions(room_id, target_event_id, reaction_token, actor_user_id);

         CREATE INDEX IF NOT EXISTS idx_room_reaction_events_retention
         ON room_reaction_events(at, room_id, reaction_event_id);",
    )?;
    Ok(())
}

pub(crate) fn migration_backup_path(
    source_path: &std::path::Path,
    version: i64,
) -> std::path::PathBuf {
    let filename = source_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("omenchat.sqlite");
    source_path.with_file_name(format!(
        "{filename}.pre-v{SCHEMA_VERSION}-from-v{version}.bak"
    ))
}

fn create_migration_backup(
    source: &rusqlite::Connection,
    source_path: &std::path::Path,
    version: i64,
) -> ServerResult<()> {
    let backup_path = migration_backup_path(source_path, version);
    let reservation = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&backup_path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        reservation.set_permissions(std::fs::Permissions::from_mode(0o600))?;
    }
    drop(reservation);

    let backup_result = (|| -> ServerResult<()> {
        let mut destination = rusqlite::Connection::open_with_flags(
            &backup_path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE,
        )?;
        let backup = rusqlite::backup::Backup::new(source, &mut destination)?;
        backup.run_to_completion(100, Duration::from_millis(10), None)?;
        drop(backup);
        drop(destination);
        std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&backup_path)?
            .sync_all()?;
        Ok(())
    })();
    if backup_result.is_err() {
        let _ = std::fs::remove_file(&backup_path);
    }
    backup_result
}

fn configure_connection(
    connection: &rusqlite::Connection,
    persistent: bool,
    busy_timeout: Duration,
) -> ServerResult<()> {
    connection.busy_timeout(busy_timeout)?;
    connection.pragma_update(None, "foreign_keys", true)?;
    connection.pragma_update(None, "synchronous", "NORMAL")?;
    if persistent {
        connection.pragma_update(None, "journal_mode", "WAL")?;
    }
    Ok(())
}

fn next_event_id(
    transaction: &rusqlite::Transaction<'_>,
    room_id: RoomId,
) -> ServerResult<EventId> {
    let event_id = transaction.query_row(
        "SELECT COALESCE(MAX(event_id), 0) + 1 FROM room_events WHERE room_id = ?1",
        [room_id],
        |row| row.get::<_, i64>(0),
    )?;
    Ok(event_id as EventId)
}

fn ensure_user_on(
    connection: &rusqlite::Connection,
    identity_hash: &[u8],
    display_name: &str,
    lxmf_destination: Option<&str>,
) -> ServerResult<ServerUser> {
    let now = current_unix_seconds();
    connection.execute(
        "INSERT INTO users(
           rns_identity_hash, display_name, lxmf_destination, first_seen_at, last_seen_at
         ) VALUES (?1, ?2, ?3, ?4, ?4)
         ON CONFLICT(rns_identity_hash) DO UPDATE SET
           display_name = excluded.display_name,
           lxmf_destination = COALESCE(excluded.lxmf_destination, users.lxmf_destination),
           last_seen_at = excluded.last_seen_at",
        (identity_hash, display_name, lxmf_destination, now),
    )?;
    connection
        .query_row(
            "SELECT user_id, rns_identity_hash, display_name, role_bits, status_bits, lxmf_destination
             FROM users WHERE rns_identity_hash = ?1",
            [identity_hash],
            user_from_row,
        )
        .optional()?
        .ok_or_else(|| crate::error::ServerError::Message("user was not created".into()))
}

fn join_room_on(
    connection: &rusqlite::Connection,
    room_id: RoomId,
    user_id: UserId,
) -> ServerResult<()> {
    let now = current_unix_seconds();
    connection.execute(
        "INSERT INTO room_members(room_id, user_id, joined_at, last_seen_at)
         VALUES (?1, ?2, ?3, ?3)
         ON CONFLICT(room_id, user_id) DO UPDATE SET last_seen_at = excluded.last_seen_at",
        (room_id, user_id, now),
    )?;
    Ok(())
}

fn append_event_in_transaction(
    transaction: &rusqlite::Transaction<'_>,
    room_id: RoomId,
    actor_user_id: Option<UserId>,
    kind: ServerRoomEventKind,
) -> ServerResult<ServerRoomEvent> {
    append_event_with_metadata_in_transaction(transaction, room_id, actor_user_id, kind, None)
}

pub(super) fn append_event_with_metadata_in_transaction(
    transaction: &rusqlite::Transaction<'_>,
    room_id: RoomId,
    actor_user_id: Option<UserId>,
    kind: ServerRoomEventKind,
    metadata: Option<RichMessageEventMetadata>,
) -> ServerResult<ServerRoomEvent> {
    if metadata.is_some() && !matches!(kind, ServerRoomEventKind::Message { .. }) {
        return Err(crate::error::ServerError::Message(
            "reply and mention metadata is valid only for room messages".into(),
        ));
    }
    let (reply_to_event_id, mention_user_ids) = encode_stored_event_metadata(metadata.as_ref())?;
    let event_id = next_event_id(transaction, room_id)?;
    let at_unix = current_unix_seconds();
    let (kind_code, payload) = encode_event_kind(&kind);
    transaction.execute(
        "INSERT INTO room_events(
           room_id, event_id, event_kind, actor_user_id, at, payload,
           reply_to_event_id, mention_user_ids
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        (
            room_id,
            event_id,
            kind_code,
            actor_user_id,
            at_unix,
            payload,
            reply_to_event_id,
            mention_user_ids,
        ),
    )?;
    let actor_display_name = actor_user_id
        .map(|user_id| {
            transaction
                .query_row(
                    "SELECT display_name FROM users WHERE user_id = ?1",
                    [user_id as i64],
                    |row| row.get::<_, String>(0),
                )
                .optional()
        })
        .transpose()?
        .flatten();
    Ok(ServerRoomEvent {
        room_id,
        event_id,
        kind,
        actor_user_id,
        actor_display_name,
        at_unix,
        metadata,
    })
}

fn encode_stored_event_metadata(
    metadata: Option<&RichMessageEventMetadata>,
) -> ServerResult<(Option<i64>, Option<Vec<u8>>)> {
    let Some(metadata) = metadata else {
        return Ok((None, None));
    };
    metadata.validate().map_err(|error| {
        crate::error::ServerError::Message(format!(
            "room event reply/mention metadata is invalid: {error}"
        ))
    })?;
    let reply_to_event_id = metadata
        .reply_to_event_id
        .map(i64::try_from)
        .transpose()
        .map_err(|_| {
            crate::error::ServerError::Message(
                "room event reply identifier exceeds SQLite integer range".into(),
            )
        })?;
    let mention_user_ids = (!metadata.mentioned_user_ids.is_empty()).then(|| {
        let mut encoded =
            Vec::with_capacity(metadata.mentioned_user_ids.len() * std::mem::size_of::<UserId>());
        for user_id in &metadata.mentioned_user_ids {
            encoded.extend_from_slice(&user_id.to_be_bytes());
        }
        encoded
    });
    Ok((reply_to_event_id, mention_user_ids))
}

fn decode_stored_event_metadata(
    reply_to_event_id: Option<i64>,
    mention_user_ids: Option<Vec<u8>>,
) -> ServerResult<Option<RichMessageEventMetadata>> {
    if reply_to_event_id.is_none() && mention_user_ids.is_none() {
        return Ok(None);
    }
    let reply_to_event_id = reply_to_event_id
        .map(|event_id| {
            EventId::try_from(event_id).map_err(|_| {
                crate::error::ServerError::Message(
                    "stored room event reply identifier is invalid".into(),
                )
            })
        })
        .transpose()?;
    let mentioned_user_ids = match mention_user_ids {
        None => Vec::new(),
        Some(bytes)
            if !bytes.is_empty()
                && bytes.len() <= crate::protocol::RICH_MESSAGE_MAX_MENTIONS * 4
                && bytes.len() % 4 == 0 =>
        {
            bytes
                .chunks_exact(4)
                .map(|chunk| UserId::from_be_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
                .collect()
        }
        Some(_) => {
            return Err(crate::error::ServerError::Message(
                "stored room event mention identifiers have an invalid bounded encoding".into(),
            ))
        }
    };
    let metadata = RichMessageEventMetadata {
        reply_to_event_id,
        mentioned_user_ids,
    };
    metadata.validate().map_err(|error| {
        crate::error::ServerError::Message(format!(
            "stored room event reply/mention metadata is invalid: {error}"
        ))
    })?;
    Ok(Some(metadata))
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

fn admin_user_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ServerAdminUser> {
    Ok(ServerAdminUser {
        user: user_from_row(row)?,
        first_seen_at: row.get(6)?,
        last_seen_at: row.get(7)?,
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

pub(crate) fn normalize_room_name(name: &str) -> String {
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

    const CRASH_TEST_DATABASE_ENV: &str = "OMENCHATD_CRASH_TEST_DATABASE";
    const CRASH_TEST_MODE_ENV: &str = "OMENCHATD_CRASH_TEST_MODE";
    const CRASH_TEST_READY_ENV: &str = "OMENCHATD_CRASH_TEST_READY";

    fn isolated_database_path(label: &str) -> std::path::PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "omenchatd-store-{label}-{}-{nonce}.sqlite",
            std::process::id()
        ))
    }

    fn remove_database_files(path: &std::path::Path) {
        for suffix in ["", "-wal", "-shm"] {
            let candidate = std::path::PathBuf::from(format!("{}{suffix}", path.display()));
            match std::fs::remove_file(candidate) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => panic!("remove isolated database: {error}"),
            }
        }
    }

    fn create_version_three_fixture(path: &std::path::Path) {
        let connection = rusqlite::Connection::open(path).expect("version three database");
        connection
            .execute_batch(include_str!("../migrations/001_init.sql"))
            .expect("base schema");
        connection
            .execute_batch(
                "DROP INDEX IF EXISTS idx_room_events_reply;
                 DROP TABLE room_events;
                 CREATE TABLE room_events (
                   room_id INTEGER NOT NULL,
                   event_id INTEGER NOT NULL,
                   event_kind INTEGER NOT NULL,
                   actor_user_id INTEGER,
                   target_user_id INTEGER,
                   at INTEGER NOT NULL,
                   payload BLOB,
                   deleted INTEGER NOT NULL DEFAULT 0,
                   PRIMARY KEY(room_id, event_id)
                 );
                 INSERT INTO rooms(room_id, name, topic, created_at)
                 VALUES (1, 'preserved-v3-room', 'must survive migration', 1);
                 INSERT INTO room_events(
                   room_id, event_id, event_kind, at, payload
                 ) VALUES (1, 1, 1, 1, X'7072657365727665642D7633');
                 PRAGMA user_version = 3;",
            )
            .expect("version three fixture");
    }

    fn create_version_four_fixture(path: &std::path::Path) {
        let connection = rusqlite::Connection::open(path).expect("version four database");
        connection
            .execute_batch(include_str!("../migrations/001_init.sql"))
            .expect("version four schema");
        connection
            .execute_batch(
                "INSERT INTO rooms(room_id, name, topic, created_at)
                 VALUES (1, 'preserved-v4-room', 'must survive migration', 1);
                 INSERT INTO room_events(
                   room_id, event_id, event_kind, at, payload
                 ) VALUES (1, 1, 1, 1, X'7072657365727665642D7634');
                 INSERT INTO durable_mutation_results(
                   identity_hash, client_instance_id, mutation_id, request_hash,
                   result_frame, retained_bytes, created_at, last_seen_at
                 ) VALUES (
                   X'01', zeroblob(16), zeroblob(16), zeroblob(32),
                   X'02', 1, 1, 1
                 );
                 PRAGMA user_version = 4;",
            )
            .expect("version four fixture");
    }

    fn schema_object_exists(connection: &rusqlite::Connection, kind: &str, name: &str) -> bool {
        connection
            .query_row(
                "SELECT EXISTS(
                   SELECT 1 FROM sqlite_master WHERE type = ?1 AND name = ?2
                 )",
                (kind, name),
                |row| row.get(0),
            )
            .expect("schema object lookup")
    }

    fn room_event_columns(connection: &rusqlite::Connection) -> Vec<String> {
        let mut statement = connection
            .prepare("PRAGMA table_info(room_events)")
            .expect("room event columns");
        statement
            .query_map([], |row| row.get(1))
            .expect("query room event columns")
            .collect::<Result<Vec<_>, _>>()
            .expect("collect room event columns")
    }

    #[derive(Debug, PartialEq, Eq)]
    struct LegacyRoomEventRow {
        room_id: i64,
        event_id: i64,
        event_kind: i64,
        actor_user_id: Option<i64>,
        target_user_id: Option<i64>,
        at: i64,
        payload: Option<Vec<u8>>,
        deleted: i64,
    }

    fn legacy_room_event_row(connection: &rusqlite::Connection) -> LegacyRoomEventRow {
        connection
            .query_row(
                "SELECT room_id, event_id, event_kind, actor_user_id,
                        target_user_id, at, payload, deleted
                 FROM room_events WHERE room_id = 1 AND event_id = 1",
                [],
                |row| {
                    Ok(LegacyRoomEventRow {
                        room_id: row.get(0)?,
                        event_id: row.get(1)?,
                        event_kind: row.get(2)?,
                        actor_user_id: row.get(3)?,
                        target_user_id: row.get(4)?,
                        at: row.get(5)?,
                        payload: row.get(6)?,
                        deleted: row.get(7)?,
                    })
                },
            )
            .expect("legacy room event row")
    }

    fn wait_for_crash_boundary_and_kill(
        database: &std::path::Path,
        ready: &std::path::Path,
        mode: &str,
    ) {
        let _ = std::fs::remove_file(ready);
        let mut child = std::process::Command::new(std::env::current_exe().expect("test binary"))
            .args([
                "--exact",
                "store::tests::process_kill_event_boundary_child",
                "--nocapture",
                "--test-threads=1",
            ])
            .env(CRASH_TEST_DATABASE_ENV, database)
            .env(CRASH_TEST_MODE_ENV, mode)
            .env(CRASH_TEST_READY_ENV, ready)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("spawn isolated crash-boundary child");

        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        loop {
            if ready.is_file() {
                break;
            }
            if let Some(status) = child.try_wait().expect("poll crash-boundary child") {
                panic!("crash-boundary child exited before {mode} marker: {status}");
            }
            if std::time::Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                panic!("crash-boundary child did not reach {mode} marker");
            }
            std::thread::sleep(Duration::from_millis(10));
        }

        child.kill().expect("kill crash-boundary child");
        let status = child.wait().expect("reap crash-boundary child");
        assert!(
            !status.success(),
            "killed child unexpectedly exited cleanly"
        );
    }

    fn publish_crash_boundary(ready: &std::path::Path) {
        use std::io::Write as _;

        let mut marker = std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(ready)
            .expect("create crash-boundary marker");
        marker.write_all(b"ready\n").expect("write boundary marker");
        marker.sync_all().expect("sync boundary marker");
    }

    #[test]
    fn process_kill_event_boundary_child() {
        let Some(database) = std::env::var_os(CRASH_TEST_DATABASE_ENV) else {
            return;
        };
        let mode = std::env::var(CRASH_TEST_MODE_ENV).expect("crash test mode");
        let ready = std::path::PathBuf::from(
            std::env::var_os(CRASH_TEST_READY_ENV).expect("crash test ready marker"),
        );

        let open_transaction = match mode.as_str() {
            "committed" => {
                let store = OmenchatStore::open(&database).expect("open committed child store");
                store
                    .append_event(
                        1,
                        None,
                        ServerRoomEventKind::Message {
                            body: "committed-before-kill".into(),
                        },
                    )
                    .expect("commit event before kill");
                None
            }
            "uncommitted" => {
                let connection =
                    rusqlite::Connection::open(&database).expect("open uncommitted child database");
                connection
                    .execute_batch("BEGIN IMMEDIATE")
                    .expect("begin uncommitted event transaction");
                connection
                    .execute(
                        "INSERT INTO room_events(room_id, event_id, event_kind, at, payload)\
                         VALUES (?1, ?2, ?3, ?4, ?5)",
                        (
                            1_i64,
                            2_i64,
                            1_i64,
                            current_unix_seconds(),
                            b"not-committed",
                        ),
                    )
                    .expect("insert uncommitted event");
                Some(connection)
            }
            other => panic!("unknown crash test mode: {other}"),
        };

        publish_crash_boundary(&ready);
        loop {
            std::hint::black_box(&open_transaction);
            std::thread::park();
        }
    }

    #[test]
    fn process_kill_preserves_committed_event_and_rolls_back_in_flight_event() {
        let database = isolated_database_path("process-kill-events");
        let ready = database.with_extension("ready");
        let setup = OmenchatStore::open(&database).expect("setup crash database");
        let room = setup.ensure_room("crash-boundary", None).expect("room");
        assert_eq!(room.room_id, 1, "isolated database must start at room 1");
        drop(setup);

        wait_for_crash_boundary_and_kill(&database, &ready, "committed");
        let store = OmenchatStore::open(&database).expect("reopen after committed kill");
        let events = store
            .latest_events(room.room_id, 10)
            .expect("committed events");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_id, 1);
        assert_eq!(
            events[0].kind,
            ServerRoomEventKind::Message {
                body: "committed-before-kill".into()
            }
        );
        drop(store);

        wait_for_crash_boundary_and_kill(&database, &ready, "uncommitted");
        let store = OmenchatStore::open(&database).expect("reopen after uncommitted kill");
        let events = store
            .latest_events(room.room_id, 10)
            .expect("events after rollback");
        assert_eq!(events.len(), 1, "in-flight event must be rolled back");
        let next = store
            .append_event(
                room.room_id,
                None,
                ServerRoomEventKind::Message {
                    body: "next-after-recovery".into(),
                },
            )
            .expect("append after recovery");
        assert_eq!(next.event_id, 2, "rolled-back ID must remain reusable");
        let integrity: String = store
            .connection
            .query_row("PRAGMA integrity_check", [], |row| row.get(0))
            .expect("SQLite integrity check");
        assert_eq!(integrity, "ok");

        drop(store);
        let _ = std::fs::remove_file(ready);
        remove_database_files(&database);
    }

    #[test]
    fn persistent_store_applies_connection_policy() {
        let path = isolated_database_path("pragmas");
        let store = OmenchatStore::open(&path).expect("store");

        let foreign_keys: i64 = store
            .connection
            .pragma_query_value(None, "foreign_keys", |row| row.get(0))
            .expect("foreign_keys");
        let journal_mode: String = store
            .connection
            .pragma_query_value(None, "journal_mode", |row| row.get(0))
            .expect("journal_mode");
        let synchronous: i64 = store
            .connection
            .pragma_query_value(None, "synchronous", |row| row.get(0))
            .expect("synchronous");
        let busy_timeout: i64 = store
            .connection
            .pragma_query_value(None, "busy_timeout", |row| row.get(0))
            .expect("busy_timeout");
        let schema_version: i64 = store
            .connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .expect("user_version");
        let upload_index_count: i64 = store
            .connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'index' AND name = 'idx_upload_files_actor_created'",
                [],
                |row| row.get(0),
            )
            .expect("upload ledger index");

        assert_eq!(foreign_keys, 1);
        assert_eq!(journal_mode.to_ascii_lowercase(), "wal");
        assert_eq!(synchronous, 1, "SQLite NORMAL is numeric level 1");
        assert_eq!(busy_timeout, SQLITE_BUSY_TIMEOUT.as_millis() as i64);
        assert_eq!(schema_version, SCHEMA_VERSION);
        assert_eq!(upload_index_count, 1);

        drop(store);
        remove_database_files(&path);
    }

    #[test]
    fn legacy_unversioned_database_migrates_transactionally() {
        let path = isolated_database_path("legacy-migration");
        let connection = rusqlite::Connection::open(&path).expect("legacy database");
        connection
            .execute_batch(
                "CREATE TABLE legacy_marker(value TEXT NOT NULL);\
                 INSERT INTO legacy_marker(value) VALUES ('preserve-me');",
            )
            .expect("legacy marker");
        drop(connection);

        let store = OmenchatStore::open(&path).expect("migrated store");
        let backup_path = migration_backup_path(&path, 0);
        let backup = rusqlite::Connection::open_with_flags(
            &backup_path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .expect("migration backup");
        let backup_marker: String = backup
            .query_row("SELECT value FROM legacy_marker", [], |row| row.get(0))
            .expect("backup marker");
        let backup_version: i64 = backup
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .expect("backup version");
        assert_eq!(backup_marker, "preserve-me");
        assert_eq!(backup_version, 0);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&backup_path)
                    .expect("backup metadata")
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
        drop(backup);
        let marker: String = store
            .connection
            .query_row("SELECT value FROM legacy_marker", [], |row| row.get(0))
            .expect("preserved marker");
        let schema_version: i64 = store
            .connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .expect("user_version");
        assert_eq!(marker, "preserve-me");
        assert_eq!(schema_version, SCHEMA_VERSION);
        assert!(store.room_by_name("missing").expect("room query").is_none());

        drop(store);
        std::fs::remove_file(backup_path).expect("remove migration backup");
        remove_database_files(&path);
    }

    #[test]
    fn version_one_database_adds_upload_index_without_losing_rows() {
        let path = isolated_database_path("v1-upload-index");
        let connection = rusqlite::Connection::open(&path).expect("version one database");
        connection
            .execute_batch(
                "CREATE TABLE upload_files (
                   resource_id TEXT PRIMARY KEY,
                   room_id INTEGER NOT NULL,
                   actor_user_id INTEGER NOT NULL,
                   filename TEXT NOT NULL,
                   content_type TEXT,
                   byte_len INTEGER NOT NULL,
                   path TEXT NOT NULL,
                   created_at INTEGER NOT NULL
                 );
                 INSERT INTO upload_files(resource_id, room_id, actor_user_id, filename, byte_len, path, created_at)
                 VALUES ('preserved', 1, 9, 'file.bin', 3, '/isolated/file.bin', 1);
                 PRAGMA user_version = 1;",
            )
            .expect("version one schema");
        drop(connection);

        let store = OmenchatStore::open(&path).expect("current-schema migration");
        let row_count: i64 = store
            .connection
            .query_row("SELECT COUNT(*) FROM upload_files", [], |row| row.get(0))
            .expect("preserved rows");
        let index_count: i64 = store
            .connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'index' AND name = 'idx_upload_files_actor_created'",
                [],
                |row| row.get(0),
            )
            .expect("created index");
        assert_eq!(row_count, 1);
        assert_eq!(index_count, 1);
        assert_eq!(
            store
                .connection
                .pragma_query_value::<i64, _>(None, "user_version", |row| row.get(0))
                .expect("schema version"),
            SCHEMA_VERSION
        );

        let backup_path = migration_backup_path(&path, 1);
        assert!(backup_path.is_file());
        drop(store);
        std::fs::remove_file(backup_path).expect("remove migration backup");
        remove_database_files(&path);
    }

    #[test]
    fn version_two_database_adds_durable_replay_schema_without_losing_rows() {
        let path = isolated_database_path("v2-durable-replay");
        let connection = rusqlite::Connection::open(&path).expect("version two database");
        connection
            .execute_batch(include_str!("../migrations/001_init.sql"))
            .expect("version two base schema");
        connection
            .execute_batch(
                "DROP INDEX IF EXISTS idx_durable_mutation_clients_retired;
                 DROP TABLE IF EXISTS durable_mutation_clients;
                 DROP INDEX IF EXISTS idx_durable_mutation_results_created;
                 DROP TABLE IF EXISTS durable_mutation_results;
                 INSERT INTO rooms(name, topic, created_at)
                 VALUES ('preserved-v2-room', 'must survive migration', 1);
                 INSERT INTO upload_files(
                   resource_id, room_id, actor_user_id, filename, byte_len, path, created_at
                 ) VALUES ('preserved-v2-upload', 1, 9, 'file.bin', 3, '/isolated/file.bin', 1);
                 PRAGMA user_version = 2;",
            )
            .expect("version two fixture");
        drop(connection);

        let store = OmenchatStore::open(&path).expect("current schema migration");
        let room_count: i64 = store
            .connection
            .query_row(
                "SELECT COUNT(*) FROM rooms WHERE name = 'preserved-v2-room'",
                [],
                |row| row.get(0),
            )
            .expect("preserved room");
        let upload_count: i64 = store
            .connection
            .query_row(
                "SELECT COUNT(*) FROM upload_files WHERE resource_id = 'preserved-v2-upload'",
                [],
                |row| row.get(0),
            )
            .expect("preserved upload");
        let table_count: i64 = store
            .connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master
                 WHERE type = 'table' AND name = 'durable_mutation_results'",
                [],
                |row| row.get(0),
            )
            .expect("durable replay table");
        let index_count: i64 = store
            .connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master
                 WHERE type = 'index' AND name = 'idx_durable_mutation_results_created'",
                [],
                |row| row.get(0),
            )
            .expect("durable replay index");
        let client_table_count: i64 = store
            .connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master
                 WHERE type = 'table' AND name = 'durable_mutation_clients'",
                [],
                |row| row.get(0),
            )
            .expect("durable client table");
        let client_index_count: i64 = store
            .connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master
                 WHERE type = 'index' AND name = 'idx_durable_mutation_clients_retired'",
                [],
                |row| row.get(0),
            )
            .expect("durable client index");
        let schema_version: i64 = store
            .connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .expect("schema version");

        assert_eq!(room_count, 1);
        assert_eq!(upload_count, 1);
        assert_eq!(table_count, 1);
        assert_eq!(index_count, 1);
        assert_eq!(client_table_count, 1);
        assert_eq!(client_index_count, 1);
        assert_eq!(schema_version, SCHEMA_VERSION);

        let backup_path = migration_backup_path(&path, 2);
        let backup = rusqlite::Connection::open_with_flags(
            &backup_path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .expect("version two migration backup");
        let backup_version: i64 = backup
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .expect("backup schema version");
        let backup_replay_table_count: i64 = backup
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master
                 WHERE type = 'table' AND name = 'durable_mutation_results'",
                [],
                |row| row.get(0),
            )
            .expect("backup replay table count");
        let backup_client_table_count: i64 = backup
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master
                 WHERE type = 'table' AND name = 'durable_mutation_clients'",
                [],
                |row| row.get(0),
            )
            .expect("backup durable client table count");
        assert_eq!(backup_version, 2);
        assert_eq!(backup_replay_table_count, 0);
        assert_eq!(backup_client_table_count, 0);

        drop(backup);
        drop(store);
        std::fs::remove_file(backup_path).expect("remove migration backup");
        remove_database_files(&path);
    }

    #[test]
    fn version_three_database_adds_reply_metadata_without_rewriting_events() {
        let path = isolated_database_path("v3-reply-metadata");
        create_version_three_fixture(&path);

        let store = OmenchatStore::open(&path).expect("version four migration");
        let columns = room_event_columns(&store.connection);
        assert!(columns.iter().any(|column| column == "reply_to_event_id"));
        assert!(columns.iter().any(|column| column == "mention_user_ids"));
        let index_count: i64 = store
            .connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master
                 WHERE type = 'index' AND name = 'idx_room_events_reply'",
                [],
                |row| row.get(0),
            )
            .expect("reply index");
        let events = store.latest_events(1, 10).expect("preserved events");
        assert_eq!(events.len(), 1);
        assert_eq!(
            events[0].kind,
            ServerRoomEventKind::Message {
                body: "preserved-v3".into()
            }
        );
        assert_eq!(events[0].metadata, None);
        assert_eq!(index_count, 1);
        assert_eq!(
            store
                .connection
                .pragma_query_value::<i64, _>(None, "user_version", |row| row.get(0))
                .expect("schema version"),
            SCHEMA_VERSION
        );

        let backup_path = migration_backup_path(&path, 3);
        let backup = rusqlite::Connection::open_with_flags(
            &backup_path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .expect("version three migration backup");
        assert_eq!(
            backup
                .pragma_query_value::<i64, _>(None, "user_version", |row| row.get(0))
                .expect("backup version"),
            3
        );
        let backup_columns = room_event_columns(&backup);
        assert!(!backup_columns
            .iter()
            .any(|column| column == "reply_to_event_id"));
        assert!(!backup_columns
            .iter()
            .any(|column| column == "mention_user_ids"));
        assert_eq!(
            legacy_room_event_row(&store.connection),
            legacy_room_event_row(&backup),
            "every pre-v4 event column must retain its logical value"
        );

        drop(backup);
        drop(store);
        std::fs::remove_file(backup_path).expect("remove migration backup");
        remove_database_files(&path);
    }

    #[test]
    fn version_four_database_adds_dormant_reaction_schema_without_losing_rows() {
        let path = isolated_database_path("v4-reaction-schema");
        create_version_four_fixture(&path);

        let store = OmenchatStore::open(&path).expect("version five migration");
        assert!(schema_object_exists(
            &store.connection,
            "table",
            "room_reactions"
        ));
        assert!(schema_object_exists(
            &store.connection,
            "table",
            "room_reaction_events"
        ));
        assert!(schema_object_exists(
            &store.connection,
            "index",
            "idx_room_reactions_target"
        ));
        assert!(schema_object_exists(
            &store.connection,
            "index",
            "idx_room_reaction_events_retention"
        ));
        assert_eq!(
            store
                .connection
                .pragma_query_value::<i64, _>(None, "user_version", |row| row.get(0))
                .expect("schema version"),
            SCHEMA_VERSION
        );
        assert_eq!(
            store
                .connection
                .query_row(
                    "SELECT COUNT(*) FROM rooms WHERE name = 'preserved-v4-room'",
                    [],
                    |row| row.get::<_, i64>(0)
                )
                .expect("preserved room"),
            1
        );
        assert_eq!(
            store
                .connection
                .query_row("SELECT COUNT(*) FROM durable_mutation_results", [], |row| {
                    row.get::<_, i64>(0)
                })
                .expect("preserved durable result"),
            1
        );

        for statement in [
            "INSERT INTO room_reactions(
               room_id, target_event_id, actor_user_id, reaction_token, created_at
             ) VALUES (1, 0, 1, 'heart', 1)",
            "INSERT INTO room_reactions(
               room_id, target_event_id, actor_user_id, reaction_token, created_at
             ) VALUES (1, 1, 1, '', 1)",
            "INSERT INTO room_reaction_events(
               room_id, reaction_event_id, target_event_id, actor_user_id,
               reaction_token, reaction_action, at, retained_bytes
             ) VALUES (1, 1, 1, 1, 'heart', 3, 1, 1)",
        ] {
            assert!(
                store.connection.execute(statement, []).is_err(),
                "schema constraint must reject {statement}"
            );
        }

        let backup_path = migration_backup_path(&path, 4);
        let backup = rusqlite::Connection::open_with_flags(
            &backup_path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .expect("version four migration backup");
        assert_eq!(
            backup
                .pragma_query_value::<i64, _>(None, "user_version", |row| row.get(0))
                .expect("backup version"),
            4
        );
        assert!(!schema_object_exists(&backup, "table", "room_reactions"));
        assert!(!schema_object_exists(
            &backup,
            "table",
            "room_reaction_events"
        ));

        drop(backup);
        drop(store);
        std::fs::remove_file(backup_path).expect("remove migration backup");
        remove_database_files(&path);
    }

    #[test]
    fn every_reaction_schema_fault_boundary_rolls_back_to_version_four() {
        for boundary in [
            ReactionMigrationBoundary::BeforeTables,
            ReactionMigrationBoundary::BetweenTables,
            ReactionMigrationBoundary::BeforeIndexes,
            ReactionMigrationBoundary::BeforeVersionUpdate,
            ReactionMigrationBoundary::BeforeCommit,
        ] {
            let path = isolated_database_path(&format!("v5-fault-{boundary:?}"));
            create_version_four_fixture(&path);
            let connection = rusqlite::Connection::open(&path).expect("migration connection");
            configure_connection(&connection, true, SQLITE_BUSY_TIMEOUT)
                .expect("connection policy");
            let store = OmenchatStore::from_connection(connection);
            let error = store
                .migrate_with_sql_step_and_reaction_hook(
                    Some(&path),
                    include_str!("../migrations/001_init.sql"),
                    ensure_event_metadata_schema,
                    |observed| {
                        if observed == boundary {
                            Err(crate::error::ServerError::Message(format!(
                                "injected reaction migration fault at {observed:?}"
                            )))
                        } else {
                            Ok(())
                        }
                    },
                )
                .expect_err("injected schema migration failure")
                .to_string();
            assert!(error.contains("injected reaction migration fault"));
            assert_eq!(
                store
                    .connection
                    .pragma_query_value::<i64, _>(None, "user_version", |row| row.get(0))
                    .expect("rolled-back version"),
                4
            );
            assert!(!schema_object_exists(
                &store.connection,
                "table",
                "room_reactions"
            ));
            assert!(!schema_object_exists(
                &store.connection,
                "table",
                "room_reaction_events"
            ));
            assert_eq!(
                store
                    .connection
                    .query_row(
                        "SELECT COUNT(*) FROM rooms WHERE name = 'preserved-v4-room'",
                        [],
                        |row| row.get::<_, i64>(0)
                    )
                    .expect("preserved room"),
                1
            );

            let backup_path = migration_backup_path(&path, 4);
            let backup = rusqlite::Connection::open_with_flags(
                &backup_path,
                rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
            )
            .expect("retained version four backup");
            assert_eq!(
                backup
                    .pragma_query_value::<i64, _>(None, "user_version", |row| row.get(0))
                    .expect("backup version"),
                4
            );
            assert!(!schema_object_exists(&backup, "table", "room_reactions"));

            drop(backup);
            drop(store);
            std::fs::remove_file(backup_path).expect("remove migration backup");
            remove_database_files(&path);
        }
    }

    #[test]
    fn version_four_metadata_round_trips_and_rejects_invalid_storage_shapes() {
        let store = OmenchatStore::in_memory().expect("store");
        let metadata = RichMessageEventMetadata {
            reply_to_event_id: Some(41),
            mentioned_user_ids: vec![2, 9],
        };
        let transaction = rusqlite::Transaction::new_unchecked(
            &store.connection,
            rusqlite::TransactionBehavior::Immediate,
        )
        .expect("metadata transaction");
        let event = append_event_with_metadata_in_transaction(
            &transaction,
            1,
            None,
            ServerRoomEventKind::Message {
                body: "rich body".into(),
            },
            Some(metadata.clone()),
        )
        .expect("append rich event");
        transaction.commit().expect("commit rich event");
        assert_eq!(event.metadata, Some(metadata.clone()));
        assert_eq!(
            store.latest_events(1, 10).expect("stored rich event")[0].metadata,
            Some(metadata)
        );
        let stored: (i64, Vec<u8>) = store
            .connection
            .query_row(
                "SELECT reply_to_event_id, mention_user_ids
                 FROM room_events WHERE room_id = 1 AND event_id = 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("stored metadata");
        assert_eq!(stored.0, 41);
        assert_eq!(
            stored.1,
            [2_u32.to_be_bytes(), 9_u32.to_be_bytes()].concat()
        );

        let legacy = store
            .append_event(
                1,
                None,
                ServerRoomEventKind::Message {
                    body: "legacy body".into(),
                },
            )
            .expect("ordinary event");
        assert_eq!(legacy.metadata, None);
        let legacy_metadata: (Option<i64>, Option<Vec<u8>>) = store
            .connection
            .query_row(
                "SELECT reply_to_event_id, mention_user_ids
                 FROM room_events WHERE room_id = 1 AND event_id = 2",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("ordinary stored metadata");
        assert_eq!(legacy_metadata, (None, None));

        for malformed in [
            Vec::new(),
            vec![0; 3],
            vec![0; 68],
            [1_u32.to_be_bytes(), 1_u32.to_be_bytes()].concat(),
            0_u32.to_be_bytes().to_vec(),
        ] {
            assert!(decode_stored_event_metadata(None, Some(malformed)).is_err());
        }
        assert!(decode_stored_event_metadata(Some(0), None).is_err());
    }

    #[test]
    fn metadata_is_rejected_for_non_message_events_before_insert() {
        let store = OmenchatStore::in_memory().expect("store");
        let transaction = rusqlite::Transaction::new_unchecked(
            &store.connection,
            rusqlite::TransactionBehavior::Immediate,
        )
        .expect("metadata transaction");
        let error = append_event_with_metadata_in_transaction(
            &transaction,
            1,
            None,
            ServerRoomEventKind::Notice {
                body: "notice".into(),
            },
            Some(RichMessageEventMetadata {
                reply_to_event_id: Some(1),
                mentioned_user_ids: Vec::new(),
            }),
        )
        .expect_err("notice metadata must fail")
        .to_string();
        assert!(error.contains("only for room messages"));
        drop(transaction);
        assert!(store.latest_events(1, 10).expect("events").is_empty());
    }

    #[test]
    fn failed_version_four_schema_step_rolls_back_and_retains_v3_backup() {
        let path = isolated_database_path("v4-migration-rollback");
        create_version_three_fixture(&path);
        let connection = rusqlite::Connection::open(&path).expect("migration connection");
        configure_connection(&connection, true, SQLITE_BUSY_TIMEOUT).expect("connection policy");
        let store = OmenchatStore::from_connection(connection);
        let error = store
            .migrate_with_sql_and_step(
                Some(&path),
                include_str!("../migrations/001_init.sql"),
                |transaction| {
                    transaction.execute_batch(
                        "ALTER TABLE room_events
                         ADD COLUMN reply_to_event_id INTEGER;
                         INSERT INTO missing_v4_table(value) VALUES ('fail');",
                    )?;
                    Ok(())
                },
            )
            .expect_err("injected version four migration failure");
        assert!(matches!(error, crate::error::ServerError::Sqlite(_)));
        assert_eq!(
            store
                .connection
                .pragma_query_value::<i64, _>(None, "user_version", |row| row.get(0))
                .expect("source version"),
            3
        );
        assert!(!room_event_columns(&store.connection)
            .iter()
            .any(|column| column == "reply_to_event_id"));
        let payload: Vec<u8> = store
            .connection
            .query_row(
                "SELECT payload FROM room_events WHERE room_id = 1 AND event_id = 1",
                [],
                |row| row.get(0),
            )
            .expect("preserved source payload");
        assert_eq!(payload, b"preserved-v3");

        let backup_path = migration_backup_path(&path, 3);
        let backup = rusqlite::Connection::open_with_flags(
            &backup_path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .expect("retained version three backup");
        assert_eq!(
            backup
                .pragma_query_value::<i64, _>(None, "user_version", |row| row.get(0))
                .expect("backup version"),
            3
        );

        drop(backup);
        drop(store);
        std::fs::remove_file(backup_path).expect("remove migration backup");
        remove_database_files(&path);
    }

    #[test]
    fn migration_refuses_to_overwrite_existing_backup() {
        let path = isolated_database_path("backup-collision");
        let connection = rusqlite::Connection::open(&path).expect("legacy database");
        connection
            .execute_batch(
                "CREATE TABLE legacy_marker(value TEXT NOT NULL);\
                 INSERT INTO legacy_marker(value) VALUES ('original');",
            )
            .expect("legacy marker");
        drop(connection);
        let backup_path = migration_backup_path(&path, 0);
        std::fs::write(&backup_path, b"operator-owned-backup").expect("backup collision");

        let error = OmenchatStore::open(&path)
            .err()
            .expect("backup collision must abort migration");
        assert!(
            matches!(error, crate::error::ServerError::Io(ref io) if io.kind() == std::io::ErrorKind::AlreadyExists)
        );
        assert_eq!(
            std::fs::read(&backup_path).expect("existing backup"),
            b"operator-owned-backup"
        );
        let connection = rusqlite::Connection::open(&path).expect("reopen legacy database");
        let version: i64 = connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .expect("legacy version");
        let marker: String = connection
            .query_row("SELECT value FROM legacy_marker", [], |row| row.get(0))
            .expect("legacy marker");
        assert_eq!(version, 0);
        assert_eq!(marker, "original");

        drop(connection);
        std::fs::remove_file(backup_path).expect("remove collision backup");
        remove_database_files(&path);
    }

    #[test]
    fn failed_migration_rolls_back_partial_schema_and_retains_backup() {
        let path = isolated_database_path("migration-rollback");
        let connection = rusqlite::Connection::open(&path).expect("legacy database");
        connection
            .execute_batch(
                "CREATE TABLE legacy_marker(value TEXT NOT NULL);\
                 INSERT INTO legacy_marker(value) VALUES ('recoverable');",
            )
            .expect("legacy marker");
        drop(connection);

        let connection = rusqlite::Connection::open(&path).expect("migration connection");
        configure_connection(&connection, true, SQLITE_BUSY_TIMEOUT).expect("connection policy");
        let store = OmenchatStore::from_connection(connection);
        let error = store
            .migrate_with_sql(
                Some(&path),
                "CREATE TABLE partial_schema(value TEXT);\
                 INSERT INTO missing_table(value) VALUES ('fail');",
            )
            .expect_err("injected migration failure");
        assert!(matches!(error, crate::error::ServerError::Sqlite(_)));

        let version: i64 = store
            .connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .expect("source version");
        let marker: String = store
            .connection
            .query_row("SELECT value FROM legacy_marker", [], |row| row.get(0))
            .expect("source marker");
        let partial_count: i64 = store
            .connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'partial_schema'",
                [],
                |row| row.get(0),
            )
            .expect("partial table count");
        assert_eq!(version, 0);
        assert_eq!(marker, "recoverable");
        assert_eq!(partial_count, 0);

        let backup_path = migration_backup_path(&path, 0);
        let backup = rusqlite::Connection::open_with_flags(
            &backup_path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .expect("retained backup");
        let backup_marker: String = backup
            .query_row("SELECT value FROM legacy_marker", [], |row| row.get(0))
            .expect("backup marker");
        assert_eq!(backup_marker, "recoverable");

        drop(backup);
        drop(store);
        std::fs::remove_file(backup_path).expect("remove migration backup");
        remove_database_files(&path);
    }

    #[test]
    fn future_schema_is_refused_without_modification() {
        let path = isolated_database_path("future-schema");
        let future_version = SCHEMA_VERSION + 1;
        let connection = rusqlite::Connection::open(&path).expect("future database");
        connection
            .execute_batch(&format!(
                "CREATE TABLE future_marker(value TEXT NOT NULL);\
                 INSERT INTO future_marker(value) VALUES ('untouched');\
                 PRAGMA user_version = {future_version};"
            ))
            .expect("future schema");
        drop(connection);

        let error = OmenchatStore::open(&path)
            .err()
            .expect("future schema must be rejected")
            .to_string();
        assert!(error.contains("newer than supported"));

        let connection = rusqlite::Connection::open(&path).expect("reopen future database");
        let marker: String = connection
            .query_row("SELECT value FROM future_marker", [], |row| row.get(0))
            .expect("future marker");
        let actual_version: i64 = connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .expect("future version");
        assert_eq!(marker, "untouched");
        assert_eq!(actual_version, future_version);

        drop(connection);
        remove_database_files(&path);
    }

    #[test]
    fn concurrent_connections_allocate_unique_monotonic_room_event_ids() {
        const WRITERS: usize = 12;

        let path = isolated_database_path("event-ids");
        let setup = OmenchatStore::open(&path).expect("setup store");
        let room = setup.ensure_room("concurrent", None).expect("room");
        drop(setup);

        let barrier = std::sync::Arc::new(std::sync::Barrier::new(WRITERS));
        let mut threads = Vec::with_capacity(WRITERS);
        for writer in 0..WRITERS {
            let path = path.clone();
            let barrier = barrier.clone();
            threads.push(std::thread::spawn(move || {
                let store = OmenchatStore::open(path).expect("writer store");
                barrier.wait();
                store
                    .append_event(
                        room.room_id,
                        None,
                        ServerRoomEventKind::Message {
                            body: format!("writer-{writer}"),
                        },
                    )
                    .expect("append event")
                    .event_id
            }));
        }

        let mut event_ids = threads
            .into_iter()
            .map(|thread| thread.join().expect("writer thread"))
            .collect::<Vec<_>>();
        event_ids.sort_unstable();
        assert_eq!(event_ids, (1..=WRITERS as EventId).collect::<Vec<_>>());

        remove_database_files(&path);
    }

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

    #[test]
    fn upload_replacement_retains_old_row_until_physical_eviction_is_confirmed() {
        let store = OmenchatStore::in_memory().expect("store");
        let room = store.ensure_room("files", None).expect("room");
        let user = store
            .ensure_user(b"identity-files", "Files", None)
            .expect("user");
        let old_path = std::path::PathBuf::from("/isolated/old.bin");
        let new_path = std::path::PathBuf::from("/isolated/new.bin");
        store
            .record_upload_file(RecordUploadFile {
                resource_id: "old-resource",
                room_id: room.room_id,
                actor_user_id: user.user_id,
                filename: "old.bin",
                content_type: None,
                byte_len: 3,
                path: &old_path,
            })
            .expect("old row");

        store
            .record_upload_file(RecordUploadFile {
                resource_id: "new-resource",
                room_id: room.room_id,
                actor_user_id: user.user_id,
                filename: "new.bin",
                content_type: None,
                byte_len: 4,
                path: &new_path,
            })
            .expect("replacement commit");

        assert!(
            store
                .upload_file("old-resource")
                .expect("old lookup before physical eviction")
                .is_some(),
            "an interruption before file eviction must conservatively over-count quota"
        );
        assert_eq!(
            store
                .remove_evicted_upload_records(
                    user.user_id,
                    "new-resource",
                    std::slice::from_ref(&old_path),
                )
                .expect("confirmed eviction cleanup"),
            1
        );

        assert!(store
            .upload_file("old-resource")
            .expect("old lookup")
            .is_none());
        assert_eq!(
            store
                .upload_file("new-resource")
                .expect("new lookup")
                .expect("new row")
                .path,
            new_path.display().to_string()
        );
    }

    #[test]
    fn upload_ledger_reconciliation_reports_without_mutating_uncertain_files() {
        let root = isolated_database_path("upload-ledger-root").with_extension("dir");
        let identity_dir = root.join("identity-a");
        std::fs::create_dir_all(&identity_dir).expect("identity dir");
        let existing = identity_dir.join("existing.bin");
        let missing = identity_dir.join("missing.bin");
        let orphan = identity_dir.join("orphan.bin");
        let outside = root.join("outside.bin");
        std::fs::write(&existing, b"abc").expect("existing file");
        std::fs::write(&orphan, b"orphan").expect("orphan file");
        std::fs::write(&outside, b"outside").expect("outside file");

        let store = OmenchatStore::in_memory().expect("store");
        let room = store.ensure_room("ledger", None).expect("room");
        let user = store
            .ensure_user(b"ledger-user", "Ledger", None)
            .expect("user");
        for (resource_id, path, bytes) in [
            ("existing", &existing, 3),
            ("missing", &missing, 4),
            ("outside", &outside, 7),
        ] {
            store
                .record_upload_file(RecordUploadFile {
                    resource_id,
                    room_id: room.room_id,
                    actor_user_id: user.user_id,
                    filename: "file.bin",
                    content_type: None,
                    byte_len: bytes,
                    path,
                })
                .expect("ledger row");
        }

        let report = store
            .reconcile_upload_ledger(user.user_id, &identity_dir)
            .expect("reconciliation");
        assert_eq!(report.tracked_files, 3);
        assert_eq!(report.tracked_bytes, 14);
        assert_eq!(report.disk_files, 2);
        assert_eq!(report.disk_bytes, 9);
        assert_eq!(report.missing_paths, vec![missing]);
        assert_eq!(report.orphan_paths, vec![orphan.clone()]);
        assert_eq!(report.unsafe_paths, vec![outside.clone()]);
        assert!(orphan.exists(), "uncertain orphan must not be deleted");
        assert!(outside.exists(), "unsafe tracked file must not be deleted");
        assert!(store.upload_file("missing").expect("row").is_some());

        std::fs::remove_dir_all(root).expect("remove isolated ledger root");
    }

    #[test]
    fn upload_ledger_repair_removes_only_unusable_records() {
        let root = isolated_database_path("upload-ledger-repair").with_extension("dir");
        let identity_dir = root.join("identity-a");
        std::fs::create_dir_all(&identity_dir).expect("identity dir");
        let existing = identity_dir.join("existing.bin");
        let missing = identity_dir.join("missing.bin");
        let orphan = identity_dir.join("orphan.bin");
        let outside = root.join("outside.bin");
        std::fs::write(&existing, b"abc").expect("existing file");
        std::fs::write(&orphan, b"orphan").expect("orphan file");
        std::fs::write(&outside, b"outside").expect("outside file");

        let store = OmenchatStore::in_memory().expect("store");
        let room = store.ensure_room("repair", None).expect("room");
        let user = store
            .ensure_user(b"repair-user", "Repair", None)
            .expect("user");
        for (resource_id, path) in [
            ("existing", &existing),
            ("missing", &missing),
            ("outside", &outside),
        ] {
            store
                .record_upload_file(RecordUploadFile {
                    resource_id,
                    room_id: room.room_id,
                    actor_user_id: user.user_id,
                    filename: "file.bin",
                    content_type: None,
                    byte_len: 3,
                    path,
                })
                .expect("ledger row");
        }

        let repair = store
            .repair_upload_ledger_records(user.user_id, &identity_dir)
            .expect("repair");
        assert_eq!(repair.removed_missing_records, 1);
        assert_eq!(repair.removed_unsafe_records, 1);
        assert_eq!(repair.preserved_orphan_paths, 1);
        assert!(store.upload_file("existing").expect("row").is_some());
        assert!(store.upload_file("missing").expect("row").is_none());
        assert!(store.upload_file("outside").expect("row").is_none());
        assert!(existing.exists());
        assert!(
            orphan.exists(),
            "orphan files remain operator-owned evidence"
        );
        assert!(
            outside.exists(),
            "repair must never delete an unsafe target"
        );

        let repeated = store
            .repair_upload_ledger_records(user.user_id, &identity_dir)
            .expect("idempotent repair");
        assert_eq!(repeated.removed_missing_records, 0);
        assert_eq!(repeated.removed_unsafe_records, 0);
        assert_eq!(repeated.preserved_orphan_paths, 1);
        std::fs::remove_dir_all(root).expect("remove isolated repair root");
    }

    #[test]
    fn indexed_upload_plan_requires_clean_reconciliation_then_uses_ledger_order() {
        let root = isolated_database_path("upload-index-plan").with_extension("dir");
        let identity_dir = root.join("identity-a");
        std::fs::create_dir_all(&identity_dir).expect("identity dir");
        let first = identity_dir.join("first.bin");
        let second = identity_dir.join("second.bin");
        std::fs::write(&first, b"12345").expect("first file");
        std::fs::write(&second, b"6789").expect("second file");
        let store = OmenchatStore::in_memory().expect("store");
        let room = store.ensure_room("index", None).expect("room");
        let user = store
            .ensure_user(b"index-user", "Index", None)
            .expect("user");
        for (resource_id, path, bytes) in [("first", &first, 5), ("second", &second, 4)] {
            store
                .record_upload_file(RecordUploadFile {
                    resource_id,
                    room_id: room.room_id,
                    actor_user_id: user.user_id,
                    filename: "file.bin",
                    content_type: None,
                    byte_len: bytes,
                    path,
                })
                .expect("ledger row");
            std::thread::sleep(Duration::from_millis(2));
        }

        let plan = store
            .plan_upload_from_index(user.user_id, &identity_dir, 5, 10)
            .expect("clean indexed plan");
        assert_eq!(plan.current_bytes, 9);
        assert_eq!(plan.evict_paths, vec![first]);

        let late_orphan = identity_dir.join("late-orphan.bin");
        std::fs::write(&late_orphan, b"external").expect("late orphan");
        let repeated = store
            .plan_upload_from_index(user.user_id, &identity_dir, 1, 10)
            .expect("verified identity uses index without another directory scan");
        assert_eq!(repeated.current_bytes, 9);
        assert!(repeated.evict_paths.is_empty());
        std::fs::remove_dir_all(root).expect("remove isolated index root");
    }

    #[test]
    fn indexed_upload_plan_refuses_unclean_or_size_mismatched_ledger() {
        for (label, recorded_bytes, create_orphan, expected) in [
            ("mismatch", 99, false, "mismatched=1"),
            ("orphan", 3, true, "orphan=1"),
        ] {
            let root = isolated_database_path(label).with_extension("dir");
            let identity_dir = root.join("identity-a");
            std::fs::create_dir_all(&identity_dir).expect("identity dir");
            let tracked = identity_dir.join("tracked.bin");
            std::fs::write(&tracked, b"abc").expect("tracked file");
            if create_orphan {
                std::fs::write(identity_dir.join("orphan.bin"), b"orphan").expect("orphan file");
            }
            let store = OmenchatStore::in_memory().expect("store");
            let room = store.ensure_room("dirty", None).expect("room");
            let user = store
                .ensure_user(label.as_bytes(), label, None)
                .expect("user");
            store
                .record_upload_file(RecordUploadFile {
                    resource_id: label,
                    room_id: room.room_id,
                    actor_user_id: user.user_id,
                    filename: "tracked.bin",
                    content_type: None,
                    byte_len: recorded_bytes,
                    path: &tracked,
                })
                .expect("ledger row");

            let error = store
                .plan_upload_from_index(user.user_id, &identity_dir, 1, 100)
                .expect_err("unclean ledger must block indexed admission")
                .to_string();
            assert!(error.contains(expected), "unexpected error: {error}");
            std::fs::remove_dir_all(root).expect("remove isolated dirty root");
        }
    }

    #[test]
    fn restart_reconciliation_is_conservative_at_every_upload_commit_boundary() {
        let root = isolated_database_path("upload-crash-boundaries").with_extension("dir");
        let database = root.join("omenchat.sqlite");
        let identity_dir = root.join("identity-a");
        std::fs::create_dir_all(&identity_dir).expect("identity dir");
        let old_path = identity_dir.join("old.bin");
        let new_path = identity_dir.join("new.bin");
        std::fs::write(&old_path, b"12345").expect("old file");
        let store = OmenchatStore::open(&database).expect("store");
        let room = store.ensure_room("crash", None).expect("room");
        let user = store
            .ensure_user(b"crash-user", "Crash", None)
            .expect("user");
        store
            .record_upload_file(RecordUploadFile {
                resource_id: "a-old",
                room_id: room.room_id,
                actor_user_id: user.user_id,
                filename: "old.bin",
                content_type: None,
                byte_len: 5,
                path: &old_path,
            })
            .expect("old row");
        drop(store);

        // Crash after durable rename but before the new ledger commit.
        std::fs::write(&new_path, b"abcde").expect("renamed replacement");
        let store = OmenchatStore::open(&database).expect("restart after rename");
        let error = store
            .plan_upload_from_index(user.user_id, &identity_dir, 1, 6)
            .expect_err("orphan replacement must block admission")
            .to_string();
        assert!(error.contains("orphan=1"));

        // Crash after the new ledger commit but before physical eviction.
        store
            .record_upload_file(RecordUploadFile {
                resource_id: "z-new",
                room_id: room.room_id,
                actor_user_id: user.user_id,
                filename: "new.bin",
                content_type: None,
                byte_len: 5,
                path: &new_path,
            })
            .expect("new row");
        drop(store);
        let store = OmenchatStore::open(&database).expect("restart before eviction");
        let plan = store
            .plan_upload_from_index(user.user_id, &identity_dir, 1, 6)
            .expect("both committed rows are safe to over-count");
        assert_eq!(plan.current_bytes, 10);
        assert_eq!(plan.evict_paths, vec![old_path.clone()]);
        drop(store);

        // Crash after physical eviction but before stale-row cleanup.
        std::fs::remove_file(&old_path).expect("physical eviction");
        let store = OmenchatStore::open(&database).expect("restart after eviction");
        let error = store
            .plan_upload_from_index(user.user_id, &identity_dir, 1, 6)
            .expect_err("missing old file must block admission")
            .to_string();
        assert!(error.contains("missing=1"));
        assert_eq!(
            store
                .remove_evicted_upload_records(
                    user.user_id,
                    "z-new",
                    std::slice::from_ref(&old_path),
                )
                .expect("complete stale-row cleanup"),
            1
        );
        store.invalidate_upload_ledger(user.user_id);
        let clean = store
            .plan_upload_from_index(user.user_id, &identity_dir, 1, 6)
            .expect("completed boundary is clean");
        assert_eq!(clean.current_bytes, 5);
        assert!(clean.evict_paths.is_empty());

        drop(store);
        remove_database_files(&database);
        std::fs::remove_dir_all(root).expect("remove isolated crash root");
    }
}
