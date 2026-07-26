use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::error::{ServerError, ServerResult};
use crate::store::{migration_backup_path, OmenchatStore, SCHEMA_VERSION};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DatabaseRestoreReport {
    pub source_version: i64,
    pub preserved_database: PathBuf,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DatabaseSchemaFourExportReport {
    pub source_version: i64,
    pub destination: PathBuf,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DatabaseSchemaFiveExportReport {
    pub source_version: i64,
    pub destination: PathBuf,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DatabaseSchemaSixExportReport {
    pub source_version: i64,
    pub destination: PathBuf,
}

pub fn restore_migration_backup(
    database: &Path,
    backup: &Path,
) -> ServerResult<DatabaseRestoreReport> {
    restore_migration_backup_with_replace(database, backup, atomic_replace)
}

fn restore_migration_backup_with_replace<F>(
    database: &Path,
    backup: &Path,
    mut replace: F,
) -> ServerResult<DatabaseRestoreReport>
where
    F: FnMut(&Path, &Path) -> std::io::Result<()>,
{
    validate_regular_file(database, "active database")?;
    validate_regular_file(backup, "migration backup")?;
    let database_parent = database.parent().ok_or_else(|| {
        ServerError::Message("database restore requires a parent directory".into())
    })?;
    if backup.parent() != Some(database_parent) {
        return Err(ServerError::Message(
            "database restore accepts only a generated sibling migration backup".into(),
        ));
    }
    if sidecar_path(database, "-wal").exists() || sidecar_path(database, "-shm").exists() {
        return Err(ServerError::Message(
            "database restore refused while SQLite WAL/SHM files exist; stop omenchatd cleanly and retry"
                .into(),
        ));
    }

    prove_exclusive_database_access(database)?;
    let source_version = validate_migration_backup(database, backup)?;
    let (stage, stage_reservation) = reserve_sibling(database, "restore-stage", "sqlite")?;
    drop(stage_reservation);
    set_private_permissions(&stage)?;
    let stage_migration_backup = migration_backup_path(&stage, source_version);

    let prepare_result = (|| -> ServerResult<()> {
        copy_sqlite_database(backup, &stage)?;
        let restored = OmenchatStore::open(&stage)?;
        drop(restored);
        checkpoint_staged_database(&stage)?;
        validate_current_database(&stage)?;
        Ok(())
    })();
    let _ = std::fs::remove_file(&stage_migration_backup);
    if let Err(error) = prepare_result {
        remove_sqlite_files(&stage);
        return Err(error);
    }

    let (preserved_database, preserved_file) = reserve_sibling(database, "pre-restore", "bak")?;
    drop(preserved_file);
    if let Err(error) = copy_file_and_sync(database, &preserved_database) {
        let _ = std::fs::remove_file(&preserved_database);
        remove_sqlite_files(&stage);
        return Err(error);
    }
    set_private_permissions(&preserved_database)?;

    if let Err(error) = replace(&stage, database) {
        remove_sqlite_files(&stage);
        return Err(error.into());
    }
    set_private_permissions(database)?;
    sync_directory(database_parent)?;
    validate_current_database(database)?;

    Ok(DatabaseRestoreReport {
        source_version,
        preserved_database,
    })
}

pub fn export_schema_four_copy(
    database: &Path,
    destination: &Path,
) -> ServerResult<DatabaseSchemaFourExportReport> {
    export_schema_four_copy_with_publish(database, destination, atomic_replace)
}

fn export_schema_four_copy_with_publish<F>(
    database: &Path,
    destination: &Path,
    mut publish: F,
) -> ServerResult<DatabaseSchemaFourExportReport>
where
    F: FnMut(&Path, &Path) -> std::io::Result<()>,
{
    export_downgrade_copy_with_publish(database, destination, 4, &mut publish)?;
    Ok(DatabaseSchemaFourExportReport {
        source_version: SCHEMA_VERSION,
        destination: destination.to_path_buf(),
    })
}

pub fn export_schema_five_copy(
    database: &Path,
    destination: &Path,
) -> ServerResult<DatabaseSchemaFiveExportReport> {
    export_schema_five_copy_with_publish(database, destination, atomic_replace)
}

fn export_schema_five_copy_with_publish<F>(
    database: &Path,
    destination: &Path,
    mut publish: F,
) -> ServerResult<DatabaseSchemaFiveExportReport>
where
    F: FnMut(&Path, &Path) -> std::io::Result<()>,
{
    export_downgrade_copy_with_publish(database, destination, 5, &mut publish)?;
    Ok(DatabaseSchemaFiveExportReport {
        source_version: SCHEMA_VERSION,
        destination: destination.to_path_buf(),
    })
}

pub fn export_schema_six_copy(
    database: &Path,
    destination: &Path,
) -> ServerResult<DatabaseSchemaSixExportReport> {
    export_schema_six_copy_with_publish(database, destination, atomic_replace)
}

fn export_schema_six_copy_with_publish<F>(
    database: &Path,
    destination: &Path,
    mut publish: F,
) -> ServerResult<DatabaseSchemaSixExportReport>
where
    F: FnMut(&Path, &Path) -> std::io::Result<()>,
{
    export_downgrade_copy_with_publish(database, destination, 6, &mut publish)?;
    Ok(DatabaseSchemaSixExportReport {
        source_version: SCHEMA_VERSION,
        destination: destination.to_path_buf(),
    })
}

fn export_downgrade_copy_with_publish<F>(
    database: &Path,
    destination: &Path,
    target_version: i64,
    publish: &mut F,
) -> ServerResult<()>
where
    F: FnMut(&Path, &Path) -> std::io::Result<()>,
{
    debug_assert!(matches!(target_version, 4..=6));
    let schema_label = format!("schema-{target_version}");
    validate_regular_file(database, "active database")?;
    if database == destination {
        return Err(ServerError::Message(format!(
            "{schema_label} export destination must differ from the active database"
        )));
    }
    if sidecar_path(database, "-wal").exists() || sidecar_path(database, "-shm").exists() {
        return Err(ServerError::Message(
            format!(
                "{schema_label} export refused while SQLite WAL/SHM files exist; stop omenchatd cleanly and retry"
            ),
        ));
    }
    let destination_parent = destination.parent().ok_or_else(|| {
        ServerError::Message(format!(
            "{schema_label} export requires a destination parent directory"
        ))
    })?;
    let destination_reservation = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)
        .map_err(|error| {
            ServerError::Message(format!(
                "{schema_label} export destination must not already exist: {}: {error}",
                destination.display()
            ))
        })?;
    if let Err(error) = set_private_permissions(destination) {
        let _ = std::fs::remove_file(destination);
        return Err(error);
    }
    drop(destination_reservation);

    let source_validation = (|| -> ServerResult<()> {
        prove_exclusive_database_access(database)?;
        validate_current_database(database)
    })();
    if let Err(error) = source_validation {
        let _ = std::fs::remove_file(destination);
        return Err(error);
    }

    let stage_label = format!("schema{target_version}-stage");
    let (stage, stage_reservation) = match reserve_sibling(destination, &stage_label, "sqlite") {
        Ok(stage) => stage,
        Err(error) => {
            let _ = std::fs::remove_file(destination);
            return Err(error);
        }
    };
    drop(stage_reservation);
    if let Err(error) = set_private_permissions(&stage) {
        remove_sqlite_files(&stage);
        let _ = std::fs::remove_file(destination);
        return Err(error);
    }

    let prepare_result = (|| -> ServerResult<()> {
        copy_sqlite_database(database, &stage)?;
        let connection = rusqlite::Connection::open_with_flags(
            &stage,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE,
        )?;
        let transaction = rusqlite::Transaction::new_unchecked(
            &connection,
            rusqlite::TransactionBehavior::Immediate,
        )?;
        transaction.execute_batch("DROP TABLE IF EXISTS room_event_sequences;")?;
        if target_version <= 5 {
            transaction.execute_batch(
                "DROP INDEX IF EXISTS idx_room_message_revision_events_retention;
                 DROP INDEX IF EXISTS idx_room_message_revision_events_target;
                 DROP INDEX IF EXISTS idx_room_message_revision_state_event;
                 DROP TABLE IF EXISTS room_message_revision_events;
                 DROP TABLE IF EXISTS room_message_revision_state;",
            )?;
        }
        if target_version <= 4 {
            transaction.execute_batch(
                "DROP INDEX IF EXISTS idx_room_reaction_events_retention;
                 DROP INDEX IF EXISTS idx_room_reactions_target;
                 DROP TABLE IF EXISTS room_reaction_events;
                 DROP TABLE IF EXISTS room_reactions;",
            )?;
        }
        transaction.pragma_update(None, "user_version", target_version)?;
        transaction.commit()?;
        drop(connection);
        checkpoint_staged_database(&stage)?;
        validate_downgrade_copy(&stage, target_version)?;
        Ok(())
    })();
    if let Err(error) = prepare_result {
        remove_sqlite_files(&stage);
        let _ = std::fs::remove_file(destination);
        return Err(error);
    }

    if let Err(error) = publish(&stage, destination) {
        remove_sqlite_files(&stage);
        let _ = std::fs::remove_file(destination);
        return Err(error.into());
    }
    let publication_result = (|| -> ServerResult<()> {
        set_private_permissions(destination)?;
        sync_directory(destination_parent)?;
        validate_downgrade_copy(destination, target_version)
    })();
    if let Err(error) = publication_result {
        remove_sqlite_files(destination);
        return Err(error);
    }

    Ok(())
}

fn validate_regular_file(path: &Path, label: &str) -> ServerResult<()> {
    let metadata = std::fs::symlink_metadata(path).map_err(|error| {
        ServerError::Message(format!(
            "{label} is unavailable at {}: {error}",
            path.display()
        ))
    })?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(ServerError::Message(format!(
            "{label} must be a regular non-symlink file: {}",
            path.display()
        )));
    }
    Ok(())
}

fn validate_migration_backup(database: &Path, backup: &Path) -> ServerResult<i64> {
    let connection =
        rusqlite::Connection::open_with_flags(backup, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let integrity: String = connection.query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
    if integrity != "ok" {
        return Err(ServerError::Message(format!(
            "migration backup failed SQLite integrity_check: {integrity}"
        )));
    }
    let version: i64 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if !(0..SCHEMA_VERSION).contains(&version) {
        return Err(ServerError::Message(format!(
            "migration backup must contain an older supported schema; found version {version}"
        )));
    }
    if migration_backup_path(database, version) != backup {
        return Err(ServerError::Message(format!(
            "migration backup filename does not match its schema version {version}"
        )));
    }
    Ok(version)
}

fn prove_exclusive_database_access(database: &Path) -> ServerResult<()> {
    let connection = rusqlite::Connection::open_with_flags(
        database,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE,
    )?;
    connection.busy_timeout(Duration::ZERO)?;
    connection.pragma_update(None, "locking_mode", "EXCLUSIVE")?;
    connection.execute_batch("BEGIN EXCLUSIVE; ROLLBACK;").map_err(|error| {
        ServerError::Message(format!(
            "database restore could not obtain exclusive access; ensure omenchatd is stopped: {error}"
        ))
    })?;
    Ok(())
}

fn validate_current_database(path: &Path) -> ServerResult<()> {
    let connection =
        rusqlite::Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let version: i64 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if version != SCHEMA_VERSION {
        return Err(ServerError::Message(format!(
            "restored database did not reach schema version {SCHEMA_VERSION}; found {version}"
        )));
    }
    let integrity: String = connection.query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
    if integrity != "ok" {
        return Err(ServerError::Message(format!(
            "restored database failed SQLite integrity_check: {integrity}"
        )));
    }
    let foreign_key_failure: Option<String> = connection
        .query_row("PRAGMA foreign_key_check", [], |row| row.get(0))
        .optional()?;
    if let Some(table) = foreign_key_failure {
        return Err(ServerError::Message(format!(
            "restored database failed foreign_key_check in table {table}"
        )));
    }
    let sequence_table: i64 = connection.query_row(
        "SELECT COUNT(*) FROM sqlite_master
         WHERE type = 'table' AND name = 'room_event_sequences'",
        [],
        |row| row.get(0),
    )?;
    if sequence_table != 1 {
        return Err(ServerError::Message(
            "restored database is missing schema-7 room event sequence storage".into(),
        ));
    }
    Ok(())
}

fn validate_downgrade_copy(path: &Path, target_version: i64) -> ServerResult<()> {
    debug_assert!(matches!(target_version, 4..=6));
    let schema_label = format!("schema-{target_version}");
    let connection =
        rusqlite::Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let version: i64 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if version != target_version {
        return Err(ServerError::Message(format!(
            "{schema_label} export has unexpected schema version {version}"
        )));
    }
    let integrity: String = connection.query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
    if integrity != "ok" {
        return Err(ServerError::Message(format!(
            "{schema_label} export failed SQLite integrity_check: {integrity}"
        )));
    }
    let foreign_key_failure: Option<String> = connection
        .query_row("PRAGMA foreign_key_check", [], |row| row.get(0))
        .optional()?;
    if let Some(table) = foreign_key_failure {
        return Err(ServerError::Message(format!(
            "{schema_label} export failed foreign_key_check in table {table}"
        )));
    }
    let revision_objects: i64 = connection.query_row(
        "SELECT COUNT(*) FROM sqlite_master
         WHERE name IN (
           'room_message_revision_state',
           'room_message_revision_events',
           'idx_room_message_revision_state_event',
           'idx_room_message_revision_events_target',
           'idx_room_message_revision_events_retention'
         )",
        [],
        |row| row.get(0),
    )?;
    if target_version <= 5 && revision_objects != 0 {
        return Err(ServerError::Message(format!(
            "{schema_label} export retained schema-6 message revision objects"
        )));
    }
    if target_version == 6 && revision_objects != 5 {
        return Err(ServerError::Message(format!(
            "schema-6 export did not retain the complete schema-6 message revision layer; found {revision_objects} of 5 objects"
        )));
    }
    let reaction_objects: i64 = connection.query_row(
        "SELECT COUNT(*) FROM sqlite_master
         WHERE name IN (
           'room_reactions',
           'room_reaction_events',
           'idx_room_reactions_target',
           'idx_room_reaction_events_retention'
         )",
        [],
        |row| row.get(0),
    )?;
    if target_version == 4 && reaction_objects != 0 {
        return Err(ServerError::Message(
            "schema-4 export retained schema-5 reaction objects".into(),
        ));
    }
    if target_version >= 5 && reaction_objects != 4 {
        return Err(ServerError::Message(format!(
            "{schema_label} export did not retain the complete schema-5 reaction layer; found {reaction_objects} of 4 objects"
        )));
    }
    let sequence_objects: i64 = connection.query_row(
        "SELECT COUNT(*) FROM sqlite_master
         WHERE name = 'room_event_sequences'",
        [],
        |row| row.get(0),
    )?;
    if sequence_objects != 0 {
        return Err(ServerError::Message(format!(
            "{schema_label} export retained schema-7 room event sequence objects"
        )));
    }
    Ok(())
}

fn checkpoint_staged_database(path: &Path) -> ServerResult<()> {
    let connection =
        rusqlite::Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE)?;
    connection.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")?;
    let mode: String = connection.query_row("PRAGMA journal_mode=DELETE", [], |row| row.get(0))?;
    if !mode.eq_ignore_ascii_case("delete") {
        return Err(ServerError::Message(format!(
            "could not finalize staged database journal mode: {mode}"
        )));
    }
    drop(connection);
    OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)?
        .sync_all()?;
    let _ = std::fs::remove_file(sidecar_path(path, "-wal"));
    let _ = std::fs::remove_file(sidecar_path(path, "-shm"));
    Ok(())
}

fn copy_sqlite_database(source: &Path, destination: &Path) -> ServerResult<()> {
    let destination_path = destination.to_path_buf();
    let source =
        rusqlite::Connection::open_with_flags(source, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let mut destination = rusqlite::Connection::open_with_flags(
        destination,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE,
    )?;
    let backup = rusqlite::backup::Backup::new(&source, &mut destination)?;
    backup.run_to_completion(100, Duration::from_millis(10), None)?;
    drop(backup);
    drop(destination);
    OpenOptions::new()
        .read(true)
        .write(true)
        .open(destination_path)?
        .sync_all()?;
    Ok(())
}

fn copy_file_and_sync(source: &Path, destination: &Path) -> ServerResult<()> {
    let mut source = File::open(source)?;
    let mut destination = OpenOptions::new()
        .write(true)
        .truncate(true)
        .open(destination)?;
    std::io::copy(&mut source, &mut destination)?;
    destination.sync_all()?;
    Ok(())
}

fn reserve_sibling(database: &Path, label: &str, extension: &str) -> ServerResult<(PathBuf, File)> {
    let parent = database.parent().ok_or_else(|| {
        ServerError::Message("database restore requires a parent directory".into())
    })?;
    let name = database
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("omenchat.sqlite");
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    for sequence in 0..1000_u16 {
        let path = parent.join(format!("{name}.{label}-{stamp}-{sequence}.{extension}"));
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => return Ok((path, file)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.into()),
        }
    }
    Err(ServerError::Message(format!(
        "could not reserve a unique {label} database path"
    )))
}

fn sidecar_path(database: &Path, suffix: &str) -> PathBuf {
    PathBuf::from(format!("{}{suffix}", database.display()))
}

fn remove_sqlite_files(database: &Path) {
    let _ = std::fs::remove_file(database);
    let _ = std::fs::remove_file(sidecar_path(database, "-wal"));
    let _ = std::fs::remove_file(sidecar_path(database, "-shm"));
}

#[cfg(unix)]
fn set_private_permissions(path: &Path) -> ServerResult<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    Ok(())
}

#[cfg(not(unix))]
fn set_private_permissions(_path: &Path) -> ServerResult<()> {
    Ok(())
}

#[cfg(unix)]
fn sync_directory(path: &Path) -> ServerResult<()> {
    File::open(path)?.sync_all()?;
    Ok(())
}

#[cfg(not(unix))]
fn sync_directory(_path: &Path) -> ServerResult<()> {
    Ok(())
}

#[cfg(not(windows))]
fn atomic_replace(source: &Path, destination: &Path) -> std::io::Result<()> {
    std::fs::rename(source, destination)
}

#[cfg(windows)]
fn atomic_replace(source: &Path, destination: &Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn MoveFileExW(existing: *const u16, replacement: *const u16, flags: u32) -> i32;
    }
    let source = source
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let destination = destination
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    // SAFETY: both pointers reference live NUL-terminated UTF-16 buffers.
    let result = unsafe { MoveFileExW(source.as_ptr(), destination.as_ptr(), 0x1 | 0x8) };
    if result == 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

use rusqlite::OptionalExtension as _;

#[cfg(test)]
mod tests {
    use super::*;

    fn isolated_root(label: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "omenchatd-database-restore-{label}-{}-{nonce}",
            std::process::id()
        ))
    }

    fn setup_current_and_version_one_backup(label: &str) -> (PathBuf, PathBuf, PathBuf) {
        let root = isolated_root(label);
        std::fs::create_dir_all(&root).expect("restore root");
        let database = root.join("omenchat.sqlite");
        let current = OmenchatStore::open(&database).expect("current database");
        current
            .ensure_room("current-only", Some("preserve before restore"))
            .expect("current marker");
        drop(current);

        let backup = migration_backup_path(&database, 1);
        let source = rusqlite::Connection::open(&backup).expect("version-one backup");
        source
            .execute_batch(include_str!("../migrations/001_init.sql"))
            .expect("backup schema");
        source
            .execute(
                "INSERT INTO rooms(name, topic, created_at) VALUES ('backup-only', 'restored', 1)",
                [],
            )
            .expect("backup marker");
        source
            .pragma_update(None, "user_version", 1)
            .expect("backup version");
        drop(source);
        (root, database, backup)
    }

    fn setup_current_for_schema_four_export(label: &str) -> (PathBuf, PathBuf, PathBuf) {
        let root = isolated_root(label);
        std::fs::create_dir_all(&root).expect("export root");
        let database = root.join("omenchat.sqlite");
        let destination = root.join("omenchat-schema4.sqlite");
        let store = OmenchatStore::open(&database).expect("current database");
        let room = store
            .ensure_room("preserved-export", Some("schema four copy"))
            .expect("export room");
        drop(store);

        let connection = rusqlite::Connection::open(&database).expect("export fixture");
        connection
            .execute(
                "INSERT INTO room_events(
                   room_id, event_id, event_kind, at, payload
                 ) VALUES (?1, 1, 1, 1, X'707265736572766564')",
                [room.room_id],
            )
            .expect("preserved room event");
        connection
            .execute(
                "INSERT INTO room_reactions(
                   room_id, target_event_id, actor_user_id, reaction_token, created_at
                 ) VALUES (?1, 1, 7, 'heart', 1)",
                [room.room_id],
            )
            .expect("reaction state");
        connection
            .execute(
                "INSERT INTO room_reaction_events(
                   room_id, reaction_event_id, target_event_id, actor_user_id,
                   reaction_token, reaction_action, at, retained_bytes
                 ) VALUES (?1, 1, 1, 7, 'heart', 1, 1, 32)",
                [room.room_id],
            )
            .expect("reaction audit");
        connection
            .execute(
                "INSERT INTO room_message_revision_state(
                   room_id, target_event_id, latest_revision_event_id, revision_action,
                   actor_user_id, replacement_body, revision_number, at, retained_bytes
                 ) VALUES (?1, 1, 2, 1, 7, X'636F72726563746564', 1, 2, 41)",
                [room.room_id],
            )
            .expect("message revision state");
        connection
            .execute(
                "INSERT INTO room_message_revision_events(
                   room_id, revision_event_id, target_event_id, actor_user_id,
                   revision_action, replacement_body, revision_number, at, retained_bytes
                 ) VALUES (?1, 2, 1, 7, 1, X'636F72726563746564', 1, 2, 41)",
                [room.room_id],
            )
            .expect("message revision audit");
        drop(connection);
        (root, database, destination)
    }

    #[test]
    fn validated_restore_migrates_backup_and_preserves_previous_database() {
        let (root, database, backup) = setup_current_and_version_one_backup("success");
        let report = restore_migration_backup(&database, &backup).expect("restore backup");
        assert_eq!(report.source_version, 1);
        assert!(report.preserved_database.is_file());

        let restored = OmenchatStore::open_existing_for_maintenance(&database)
            .expect("restored current schema");
        assert!(restored
            .room_by_name("backup-only")
            .expect("backup room")
            .is_some());
        assert!(restored
            .room_by_name("current-only")
            .expect("old room absent")
            .is_none());
        drop(restored);

        let preserved = OmenchatStore::open_read_only(&report.preserved_database)
            .expect("preserved prior database");
        assert!(preserved
            .room_by_name("current-only")
            .expect("preserved marker")
            .is_some());
        drop(preserved);
        assert!(backup.is_file(), "source backup must never be modified");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&database)
                    .expect("database metadata")
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
            assert_eq!(
                std::fs::metadata(&report.preserved_database)
                    .expect("preserved metadata")
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
        std::fs::remove_dir_all(root).expect("remove successful restore root");
    }

    #[test]
    fn replacement_failure_leaves_active_and_source_databases_unchanged() {
        let (root, database, backup) = setup_current_and_version_one_backup("replace-failure");
        let error = restore_migration_backup_with_replace(&database, &backup, |_, _| {
            Err(std::io::Error::other("injected atomic replacement failure"))
        })
        .expect_err("replacement must fail")
        .to_string();
        assert!(error.contains("injected atomic replacement failure"));

        let current = OmenchatStore::open_existing_for_maintenance(&database)
            .expect("unchanged active database");
        assert!(current
            .room_by_name("current-only")
            .expect("current marker")
            .is_some());
        assert!(current
            .room_by_name("backup-only")
            .expect("backup marker absent")
            .is_none());
        drop(current);
        assert_eq!(
            rusqlite::Connection::open_with_flags(
                &backup,
                rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY
            )
            .expect("unchanged source backup")
            .pragma_query_value(None, "user_version", |row| row.get::<_, i64>(0))
            .expect("source version"),
            1
        );
        assert!(
            std::fs::read_dir(&root)
                .expect("restore files")
                .filter_map(Result::ok)
                .all(|entry| !entry
                    .file_name()
                    .to_string_lossy()
                    .contains("restore-stage")),
            "failed replacement must remove staging files"
        );
        std::fs::remove_dir_all(root).expect("remove failed restore root");
    }

    #[test]
    fn restore_rejects_corrupt_current_schema_and_active_wal_inputs() {
        let (root, database, backup) = setup_current_and_version_one_backup("refusals");
        let corrupt = root.join("omenchat.sqlite.pre-v2-from-v0.bak");
        std::fs::write(&corrupt, b"not sqlite").expect("corrupt backup");
        let error = restore_migration_backup(&database, &corrupt)
            .expect_err("corrupt backup must fail")
            .to_string();
        assert!(error.contains("SQLite"), "unexpected error: {error}");

        let current_backup = root.join(format!(
            "omenchat.sqlite.pre-v{SCHEMA_VERSION}-from-v{SCHEMA_VERSION}.bak"
        ));
        std::fs::copy(&database, &current_backup).expect("current schema copy");
        let error = restore_migration_backup(&database, &current_backup)
            .expect_err("current schema source must fail")
            .to_string();
        assert!(error.contains("older supported schema"));

        std::fs::write(sidecar_path(&database, "-wal"), b"active").expect("WAL sentinel");
        let error = restore_migration_backup(&database, &backup)
            .expect_err("WAL presence must fail closed")
            .to_string();
        assert!(error.contains("WAL/SHM"));
        std::fs::remove_file(sidecar_path(&database, "-wal")).expect("remove WAL sentinel");

        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            let real_backup = root.join("operator-selected-real-backup.sqlite");
            std::fs::rename(&backup, &real_backup).expect("move real backup");
            symlink(&real_backup, &backup).expect("backup symlink");
            let error = restore_migration_backup(&database, &backup)
                .expect_err("backup symlink must fail")
                .to_string();
            assert!(error.contains("non-symlink"));
        }

        let current = OmenchatStore::open_existing_for_maintenance(&database)
            .expect("active database remains valid");
        assert!(current
            .room_by_name("current-only")
            .expect("current marker")
            .is_some());
        drop(current);
        std::fs::remove_dir_all(root).expect("remove refusal root");
    }

    #[test]
    fn schema_four_export_is_separate_integral_and_preserves_non_reaction_data() {
        let (root, database, destination) = setup_current_for_schema_four_export("schema4-success");
        let report = export_schema_four_copy(&database, &destination).expect("schema four export");
        assert_eq!(report.source_version, SCHEMA_VERSION);
        assert_eq!(report.destination, destination);

        let active = rusqlite::Connection::open_with_flags(
            &database,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .expect("active database");
        assert_eq!(
            active
                .pragma_query_value::<i64, _>(None, "user_version", |row| row.get(0))
                .expect("active version"),
            SCHEMA_VERSION
        );
        assert_eq!(
            active
                .query_row("SELECT COUNT(*) FROM room_reactions", [], |row| {
                    row.get::<_, i64>(0)
                })
                .expect("active reactions"),
            1
        );
        assert_eq!(
            active
                .query_row(
                    "SELECT COUNT(*) FROM room_message_revision_state",
                    [],
                    |row| row.get::<_, i64>(0)
                )
                .expect("active revision state"),
            1
        );

        let exported = rusqlite::Connection::open_with_flags(
            &destination,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .expect("schema four database");
        assert_eq!(
            exported
                .pragma_query_value::<i64, _>(None, "user_version", |row| row.get(0))
                .expect("export version"),
            4
        );
        assert_eq!(
            exported
                .query_row(
                    "SELECT COUNT(*) FROM rooms WHERE name = 'preserved-export'",
                    [],
                    |row| row.get::<_, i64>(0)
                )
                .expect("preserved room"),
            1
        );
        assert_eq!(
            exported
                .query_row("SELECT COUNT(*) FROM room_events", [], |row| {
                    row.get::<_, i64>(0)
                })
                .expect("preserved room events"),
            1
        );
        assert_eq!(
            exported
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master
                     WHERE name IN (
                       'room_reactions',
                       'room_reaction_events',
                       'room_message_revision_state',
                       'room_message_revision_events'
                     )",
                    [],
                    |row| row.get::<_, i64>(0)
                )
                .expect("post-schema-four tables absent"),
            0
        );
        let integrity: String = exported
            .query_row("PRAGMA integrity_check", [], |row| row.get(0))
            .expect("integrity check");
        assert_eq!(integrity, "ok");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&destination)
                    .expect("export metadata")
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }

        drop(exported);
        drop(active);
        std::fs::remove_dir_all(root).expect("remove export root");
    }

    #[test]
    fn schema_five_export_preserves_reactions_and_omits_message_revisions() {
        let (root, database, _) = setup_current_for_schema_four_export("schema5-success");
        let destination = root.join("omenchat-schema5.sqlite");
        let report = export_schema_five_copy(&database, &destination).expect("schema five export");
        assert_eq!(report.source_version, SCHEMA_VERSION);
        assert_eq!(report.destination, destination);

        let active = rusqlite::Connection::open_with_flags(
            &database,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .expect("active database");
        assert_eq!(
            active
                .pragma_query_value::<i64, _>(None, "user_version", |row| row.get(0))
                .expect("active version"),
            SCHEMA_VERSION
        );
        assert_eq!(
            active
                .query_row(
                    "SELECT COUNT(*) FROM room_message_revision_events",
                    [],
                    |row| row.get::<_, i64>(0)
                )
                .expect("active revision audit"),
            1
        );

        let exported = rusqlite::Connection::open_with_flags(
            &destination,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .expect("schema five database");
        assert_eq!(
            exported
                .pragma_query_value::<i64, _>(None, "user_version", |row| row.get(0))
                .expect("export version"),
            5
        );
        assert_eq!(
            exported
                .query_row("SELECT COUNT(*) FROM room_reactions", [], |row| {
                    row.get::<_, i64>(0)
                })
                .expect("preserved reaction state"),
            1
        );
        assert_eq!(
            exported
                .query_row("SELECT COUNT(*) FROM room_reaction_events", [], |row| {
                    row.get::<_, i64>(0)
                })
                .expect("preserved reaction audit"),
            1
        );
        assert_eq!(
            exported
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master
                     WHERE name IN (
                       'room_message_revision_state',
                       'room_message_revision_events',
                       'idx_room_message_revision_state_event',
                       'idx_room_message_revision_events_target',
                       'idx_room_message_revision_events_retention'
                     )",
                    [],
                    |row| row.get::<_, i64>(0)
                )
                .expect("revision objects absent"),
            0
        );
        let integrity: String = exported
            .query_row("PRAGMA integrity_check", [], |row| row.get(0))
            .expect("integrity check");
        assert_eq!(integrity, "ok");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&destination)
                    .expect("export metadata")
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }

        drop(exported);
        drop(active);
        std::fs::remove_dir_all(root).expect("remove schema-five export root");
    }

    #[test]
    fn schema_six_export_preserves_history_reactions_and_message_revisions() {
        let (root, database, _) = setup_current_for_schema_four_export("schema6-success");
        let destination = root.join("omenchat-schema6.sqlite");
        let report = export_schema_six_copy(&database, &destination).expect("schema six export");
        assert_eq!(report.source_version, SCHEMA_VERSION);
        assert_eq!(report.destination, destination);

        let active = rusqlite::Connection::open_with_flags(
            &database,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .expect("active database");
        assert_eq!(
            active
                .pragma_query_value::<i64, _>(None, "user_version", |row| row.get(0))
                .expect("active version"),
            SCHEMA_VERSION
        );
        assert_eq!(
            active
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master
                     WHERE type = 'table' AND name = 'room_event_sequences'",
                    [],
                    |row| row.get::<_, i64>(0)
                )
                .expect("active sequence table"),
            1
        );

        let exported = rusqlite::Connection::open_with_flags(
            &destination,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .expect("schema six database");
        assert_eq!(
            exported
                .pragma_query_value::<i64, _>(None, "user_version", |row| row.get(0))
                .expect("export version"),
            6
        );
        for table in [
            "room_events",
            "room_reactions",
            "room_reaction_events",
            "room_message_revision_state",
            "room_message_revision_events",
        ] {
            assert_eq!(
                exported
                    .query_row(
                        "SELECT COUNT(*) FROM sqlite_master
                         WHERE type = 'table' AND name = ?1",
                        [table],
                        |row| row.get::<_, i64>(0)
                    )
                    .expect("preserved table lookup"),
                1,
                "schema-6 export must preserve {table}"
            );
        }
        assert_eq!(
            exported
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master
                     WHERE name = 'room_event_sequences'",
                    [],
                    |row| row.get::<_, i64>(0)
                )
                .expect("sequence object absent"),
            0
        );
        assert_eq!(
            exported
                .query_row("SELECT COUNT(*) FROM room_events", [], |row| {
                    row.get::<_, i64>(0)
                })
                .expect("preserved room history"),
            1
        );
        let integrity: String = exported
            .query_row("PRAGMA integrity_check", [], |row| row.get(0))
            .expect("integrity check");
        assert_eq!(integrity, "ok");

        drop(exported);
        drop(active);
        std::fs::remove_dir_all(root).expect("remove schema-six export root");
    }

    #[test]
    fn schema_four_export_refuses_overwrite_active_sidecars_and_source_replacement() {
        let (root, database, destination) =
            setup_current_for_schema_four_export("schema4-refusals");

        std::fs::write(&destination, b"operator-owned").expect("existing destination");
        let error = export_schema_four_copy(&database, &destination)
            .expect_err("existing destination must fail")
            .to_string();
        assert!(error.contains("must not already exist"));
        assert_eq!(
            std::fs::read(&destination).expect("preserved destination"),
            b"operator-owned"
        );
        std::fs::remove_file(&destination).expect("remove destination");

        std::fs::write(sidecar_path(&database, "-wal"), b"active").expect("WAL sentinel");
        let error = export_schema_four_copy(&database, &destination)
            .expect_err("WAL presence must fail")
            .to_string();
        assert!(error.contains("WAL/SHM"));
        std::fs::remove_file(sidecar_path(&database, "-wal")).expect("remove WAL sentinel");

        let error = export_schema_four_copy_with_publish(&database, &destination, |_, _| {
            Err(std::io::Error::other("injected schema-4 publish failure"))
        })
        .expect_err("publish failure")
        .to_string();
        assert!(
            error.contains("injected schema-4 publish failure"),
            "unexpected export failure: {error}"
        );
        assert!(!destination.exists());
        assert!(std::fs::read_dir(&root)
            .expect("export files")
            .filter_map(Result::ok)
            .all(|entry| !entry
                .file_name()
                .to_string_lossy()
                .contains("schema4-stage")));

        let active = rusqlite::Connection::open_with_flags(
            &database,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .expect("unchanged active database");
        assert_eq!(
            active
                .pragma_query_value::<i64, _>(None, "user_version", |row| row.get(0))
                .expect("active version"),
            SCHEMA_VERSION
        );
        assert_eq!(
            active
                .query_row("SELECT COUNT(*) FROM room_reactions", [], |row| {
                    row.get::<_, i64>(0)
                })
                .expect("active reaction"),
            1
        );
        drop(active);
        std::fs::remove_dir_all(root).expect("remove refusal root");
    }

    #[test]
    fn schema_five_export_refuses_overwrite_sidecars_and_publish_failure() {
        let (root, database, _) = setup_current_for_schema_four_export("schema5-refusals");
        let destination = root.join("omenchat-schema5.sqlite");

        std::fs::write(&destination, b"operator-owned").expect("existing destination");
        let error = export_schema_five_copy(&database, &destination)
            .expect_err("existing destination must fail")
            .to_string();
        assert!(error.contains("must not already exist"));
        assert_eq!(
            std::fs::read(&destination).expect("preserved destination"),
            b"operator-owned"
        );
        std::fs::remove_file(&destination).expect("remove destination");

        std::fs::write(sidecar_path(&database, "-wal"), b"active").expect("WAL sentinel");
        let error = export_schema_five_copy(&database, &destination)
            .expect_err("WAL presence must fail")
            .to_string();
        assert!(error.contains("WAL/SHM"));
        std::fs::remove_file(sidecar_path(&database, "-wal")).expect("remove WAL sentinel");

        let error = export_schema_five_copy_with_publish(&database, &destination, |_, _| {
            Err(std::io::Error::other("injected schema-5 publish failure"))
        })
        .expect_err("publish failure")
        .to_string();
        assert!(
            error.contains("injected schema-5 publish failure"),
            "unexpected export failure: {error}"
        );
        assert!(!destination.exists());
        assert!(std::fs::read_dir(&root)
            .expect("export files")
            .filter_map(Result::ok)
            .all(|entry| !entry
                .file_name()
                .to_string_lossy()
                .contains("schema5-stage")));

        let active = rusqlite::Connection::open_with_flags(
            &database,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .expect("unchanged active database");
        assert_eq!(
            active
                .pragma_query_value::<i64, _>(None, "user_version", |row| row.get(0))
                .expect("active version"),
            SCHEMA_VERSION
        );
        assert_eq!(
            active
                .query_row(
                    "SELECT COUNT(*) FROM room_message_revision_state",
                    [],
                    |row| row.get::<_, i64>(0)
                )
                .expect("active revision state"),
            1
        );
        drop(active);
        std::fs::remove_dir_all(root).expect("remove schema-five refusal root");
    }
}
