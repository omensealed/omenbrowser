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
    Connection::open_with_flags(path, flags | OpenFlags::SQLITE_OPEN_NOFOLLOW)
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
}
