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

        let current_backup = root.join("omenchat.sqlite.pre-v2-from-v2.bak");
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
}
