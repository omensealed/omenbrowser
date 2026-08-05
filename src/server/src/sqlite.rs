use std::path::Path;

use rusqlite::{Connection, OpenFlags, Result};

const DEFAULT_FILE_FLAGS: OpenFlags = OpenFlags::SQLITE_OPEN_READ_WRITE
    .union(OpenFlags::SQLITE_OPEN_CREATE)
    .union(OpenFlags::SQLITE_OPEN_URI)
    .union(OpenFlags::SQLITE_OPEN_NO_MUTEX);

pub(crate) fn open(path: &Path) -> Result<Connection> {
    open_with_flags(path, DEFAULT_FILE_FLAGS)
}

pub(crate) fn open_read_only(path: &Path) -> Result<Connection> {
    open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
}

pub(crate) fn open_read_write(path: &Path) -> Result<Connection> {
    open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_WRITE)
}

pub(crate) fn open_with_flags(path: &Path, flags: OpenFlags) -> Result<Connection> {
    let path = stable_file_path(path)?;
    Connection::open_with_flags(path, flags | OpenFlags::SQLITE_OPEN_NOFOLLOW)
}

/// Resolve only the established parent directory, never the final database
/// component. macOS temporary paths commonly pass through `/var`, which is a
/// system-owned symlink to `/private/var`; SQLite's `NOFOLLOW` rejects that
/// lexical spelling even though the final database is a regular file. The
/// private-path policy validates ownership and containment before this helper
/// is reached, and retaining the final component lets `NOFOLLOW` continue to
/// reject a database symlink.
#[cfg(unix)]
fn stable_file_path(path: &Path) -> Result<std::path::PathBuf> {
    let parent = path
        .parent()
        .ok_or_else(|| rusqlite::Error::InvalidPath(path.to_path_buf()))?;
    let file_name = path
        .file_name()
        .ok_or_else(|| rusqlite::Error::InvalidPath(path.to_path_buf()))?;
    let parent = std::fs::canonicalize(parent)
        .map_err(|_| rusqlite::Error::InvalidPath(path.to_path_buf()))?;
    Ok(parent.join(file_name))
}

#[cfg(not(unix))]
fn stable_file_path(path: &Path) -> Result<std::path::PathBuf> {
    Ok(path.to_path_buf())
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn source_database_symlink_is_rejected_without_creating_an_unintended_database() {
        let root = std::env::temp_dir().join(format!(
            "omenchatd-sqlite-nofollow-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).expect("root");
        let target = root.join("outside.sqlite");
        let link = root.join("managed.sqlite");
        symlink(&target, &link).expect("database symlink");

        assert!(open(&link).is_err());
        assert!(!target.exists());

        std::fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn operator_controlled_ancestor_symlink_resolves_without_resolving_final_file() {
        let root = std::env::temp_dir().join(format!(
            "omenchatd-sqlite-ancestor-link-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        let real_parent = root.join("real");
        std::fs::create_dir_all(&real_parent).expect("real parent");
        let linked_parent = root.join("linked");
        symlink(&real_parent, &linked_parent).expect("ancestor link");
        let lexical_database = linked_parent.join("omen.sqlite");

        let stable = stable_file_path(&lexical_database).expect("stable database path");
        assert_eq!(
            stable,
            std::fs::canonicalize(&real_parent)
                .expect("canonical parent")
                .join("omen.sqlite")
        );
        let connection = open(&lexical_database).expect("open through stable parent");
        connection
            .execute_batch("CREATE TABLE evidence(value INTEGER NOT NULL);")
            .expect("create evidence");
        drop(connection);
        assert!(real_parent.join("omen.sqlite").is_file());

        std::fs::remove_dir_all(root).expect("cleanup");
    }
}
