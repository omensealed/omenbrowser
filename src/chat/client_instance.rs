use std::fs::{self, File, OpenOptions};
use std::io::{ErrorKind, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use omenchat_protocol::{ClientInstanceId, CLIENT_INSTANCE_ID_BYTES};
use rand_core::RngCore;

use crate::error::{AppError, AppResult};

const CLIENT_INSTANCE_DIR: &str = "omenchat";
const CLIENT_INSTANCE_FILE: &str = "client-instance-id";
static CLIENT_INSTANCE_TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClientInstanceIdStore {
    path: PathBuf,
}

impl ClientInstanceIdStore {
    pub fn for_identity_storage_root(root: impl AsRef<Path>) -> Self {
        Self {
            path: root
                .as_ref()
                .join(CLIENT_INSTANCE_DIR)
                .join(CLIENT_INSTANCE_FILE),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn load(&self) -> AppResult<Option<ClientInstanceId>> {
        read_client_instance_id(&self.path)
    }

    pub fn load_or_create(&self) -> AppResult<ClientInstanceId> {
        self.load_or_create_with(|| {
            let mut bytes = [0_u8; CLIENT_INSTANCE_ID_BYTES];
            rand_core::OsRng.fill_bytes(&mut bytes);
            bytes
        })
    }

    pub(crate) fn replace_expected(
        &self,
        expected: ClientInstanceId,
        replacement: ClientInstanceId,
    ) -> AppResult<ClientInstanceId> {
        self.replace_expected_with(expected, replacement, || Ok(()))
    }

    fn replace_expected_with(
        &self,
        expected: ClientInstanceId,
        replacement: ClientInstanceId,
        before_commit: impl FnOnce() -> std::io::Result<()>,
    ) -> AppResult<ClientInstanceId> {
        if expected == replacement {
            return Err(AppError::Settings(
                "OMENchat client instance replacement must be different".into(),
            ));
        }
        let current = self.load()?.ok_or_else(|| {
            AppError::Settings("OMENchat client instance is missing during rotation".into())
        })?;
        if current != expected {
            return Err(AppError::Settings(
                "OMENchat client instance changed before rotation".into(),
            ));
        }
        replace_client_instance_id(
            &self.path,
            expected.as_bytes(),
            replacement.as_bytes(),
            before_commit,
        )?;
        Ok(replacement)
    }

    fn load_or_create_with(
        &self,
        generate: impl FnOnce() -> [u8; CLIENT_INSTANCE_ID_BYTES],
    ) -> AppResult<ClientInstanceId> {
        if let Some(existing) = self.load()? {
            return Ok(existing);
        }
        let created = ClientInstanceId::new(generate());
        match publish_client_instance_id(&self.path, created.as_bytes(), || Ok(())) {
            Ok(()) => Ok(created),
            Err(error) if error.kind() == ErrorKind::AlreadyExists => {
                self.load()?.ok_or_else(|| {
                    AppError::Settings(
                        "OMENchat client instance appeared during creation but could not be read"
                            .into(),
                    )
                })
            }
            Err(error) => Err(error.into()),
        }
    }
}

fn read_client_instance_id(path: &Path) -> AppResult<Option<ClientInstanceId>> {
    let path_metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    if !path_metadata.file_type().is_file() {
        return Err(AppError::Settings(format!(
            "OMENchat client instance must be a regular file: {}",
            path.display()
        )));
    }
    if path_metadata.len() != CLIENT_INSTANCE_ID_BYTES as u64 {
        return Err(AppError::Settings(format!(
            "OMENchat client instance must contain exactly {CLIENT_INSTANCE_ID_BYTES} bytes: {}",
            path.display()
        )));
    }
    validate_private_file_permissions(path, &path_metadata)?;

    let mut file = File::open(path)?;
    let opened_metadata = file.metadata()?;
    if !opened_metadata.is_file() || opened_metadata.len() != CLIENT_INSTANCE_ID_BYTES as u64 {
        return Err(AppError::Settings(format!(
            "OMENchat client instance changed while it was being opened: {}",
            path.display()
        )));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if path_metadata.dev() != opened_metadata.dev()
            || path_metadata.ino() != opened_metadata.ino()
        {
            return Err(AppError::Settings(format!(
                "OMENchat client instance changed while it was being opened: {}",
                path.display()
            )));
        }
    }
    let mut bytes = [0_u8; CLIENT_INSTANCE_ID_BYTES];
    file.read_exact(&mut bytes)?;
    let mut trailing = [0_u8; 1];
    if file.read(&mut trailing)? != 0 {
        return Err(AppError::Settings(format!(
            "OMENchat client instance contains trailing data: {}",
            path.display()
        )));
    }
    Ok(Some(ClientInstanceId::new(bytes)))
}

fn publish_client_instance_id(
    path: &Path,
    bytes: &[u8; CLIENT_INSTANCE_ID_BYTES],
    before_commit: impl FnOnce() -> std::io::Result<()>,
) -> std::io::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| std::io::Error::new(ErrorKind::InvalidInput, "missing parent"))?;
    ensure_private_directory(parent)?;
    if fs::symlink_metadata(path).is_ok() {
        return Err(std::io::Error::new(
            ErrorKind::AlreadyExists,
            "client instance already exists",
        ));
    }
    let sequence = CLIENT_INSTANCE_TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let temporary = parent.join(format!(
        ".{CLIENT_INSTANCE_FILE}.{}.{}.tmp",
        std::process::id(),
        sequence
    ));
    let result = (|| {
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temporary)?;
        file.write_all(bytes)?;
        file.flush()?;
        file.sync_all()?;
        drop(file);
        before_commit()?;
        fs::hard_link(&temporary, path)?;
        sync_directory(parent)?;
        fs::remove_file(&temporary)?;
        sync_directory(parent)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn replace_client_instance_id(
    path: &Path,
    expected: &[u8; CLIENT_INSTANCE_ID_BYTES],
    replacement: &[u8; CLIENT_INSTANCE_ID_BYTES],
    before_commit: impl FnOnce() -> std::io::Result<()>,
) -> std::io::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| std::io::Error::new(ErrorKind::InvalidInput, "missing parent"))?;
    ensure_private_directory(parent)?;
    let sequence = CLIENT_INSTANCE_TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let temporary = parent.join(format!(
        ".{CLIENT_INSTANCE_FILE}.rotate.{}.{}.tmp",
        std::process::id(),
        sequence
    ));
    let result = (|| {
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temporary)?;
        file.write_all(replacement)?;
        file.flush()?;
        file.sync_all()?;
        drop(file);
        before_commit()?;

        let current = read_client_instance_id(path)
            .map_err(|error| std::io::Error::other(error.to_string()))?
            .ok_or_else(|| std::io::Error::new(ErrorKind::NotFound, "client instance missing"))?;
        if current.as_bytes() != expected {
            return Err(std::io::Error::new(
                ErrorKind::AlreadyExists,
                "client instance changed during rotation",
            ));
        }
        fs::rename(&temporary, path)?;
        sync_directory(parent)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn ensure_private_directory(path: &Path) -> std::io::Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_dir() => {
            validate_private_directory_permissions(path, &metadata)
        }
        Ok(_) => Err(std::io::Error::new(
            ErrorKind::InvalidInput,
            "OMENchat client instance parent must be a private directory",
        )),
        Err(error) if error.kind() == ErrorKind::NotFound => {
            let grandparent = path.parent().ok_or_else(|| {
                std::io::Error::new(ErrorKind::InvalidInput, "missing storage root")
            })?;
            if !fs::symlink_metadata(grandparent)?.file_type().is_dir() {
                return Err(std::io::Error::new(
                    ErrorKind::InvalidInput,
                    "OMENchat identity storage root must be a directory",
                ));
            }
            let builder = fs::DirBuilder::new();
            #[cfg(unix)]
            let mut builder = builder;
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt;
                builder.mode(0o700);
            }
            match builder.create(path) {
                Ok(()) => {}
                Err(error) if error.kind() == ErrorKind::AlreadyExists => {}
                Err(error) => return Err(error),
            }
            let metadata = fs::symlink_metadata(path)?;
            if !metadata.file_type().is_dir() {
                return Err(std::io::Error::new(
                    ErrorKind::InvalidInput,
                    "OMENchat client instance parent must be a private directory",
                ));
            }
            validate_private_directory_permissions(path, &metadata)
        }
        Err(error) => Err(error),
    }
}

fn validate_private_file_permissions(path: &Path, metadata: &fs::Metadata) -> AppResult<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o077 != 0 {
            return Err(AppError::Settings(format!(
                "OMENchat client instance permissions must be owner-only: {}",
                path.display()
            )));
        }
    }
    #[cfg(not(unix))]
    let _ = (path, metadata);
    Ok(())
}

fn validate_private_directory_permissions(
    path: &Path,
    metadata: &fs::Metadata,
) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o077 != 0 {
            return Err(std::io::Error::new(
                ErrorKind::PermissionDenied,
                format!(
                    "OMENchat client instance directory permissions must be owner-only: {}",
                    path.display()
                ),
            ));
        }
    }
    #[cfg(not(unix))]
    let _ = (path, metadata);
    Ok(())
}

fn sync_directory(path: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    File::open(path)?.sync_all()?;
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn isolated_root(label: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "omenbrowser-client-instance-{label}-{}-{}",
            std::process::id(),
            CLIENT_INSTANCE_TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("create isolated identity root");
        root
    }

    #[test]
    fn missing_instance_is_atomically_created_owner_only_and_reused() {
        let root = isolated_root("create");
        let store = ClientInstanceIdStore::for_identity_storage_root(&root);
        let expected = [7_u8; CLIENT_INSTANCE_ID_BYTES];
        let created = store
            .load_or_create_with(|| expected)
            .expect("create instance");
        assert_eq!(created.as_bytes(), &expected);
        let loaded = store
            .load_or_create_with(|| panic!("existing instance must be reused"))
            .expect("reload instance");
        assert_eq!(loaded, created);
        assert_eq!(fs::read(store.path()).expect("read raw instance"), expected);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(store.path())
                    .expect("instance metadata")
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
            assert_eq!(
                fs::metadata(store.path().parent().expect("parent"))
                    .expect("directory metadata")
                    .permissions()
                    .mode()
                    & 0o777,
                0o700
            );
        }
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn corrupt_instance_fails_closed_without_regeneration_or_rewrite() {
        let root = isolated_root("corrupt");
        let store = ClientInstanceIdStore::for_identity_storage_root(&root);
        fs::create_dir_all(store.path().parent().expect("parent")).expect("create parent");
        #[cfg(unix)]
        fs::set_permissions(
            store.path().parent().expect("parent"),
            std::os::unix::fs::PermissionsExt::from_mode(0o700),
        )
        .expect("private parent");
        fs::write(store.path(), b"short").expect("seed corrupt instance");
        let error = store
            .load_or_create_with(|| panic!("corrupt instance must not regenerate"))
            .expect_err("reject corrupt instance");
        assert!(error.to_string().contains("exactly 16 bytes"));
        assert_eq!(
            fs::read(store.path()).expect("preserve corrupt bytes"),
            b"short"
        );
        assert_eq!(
            fs::read_dir(store.path().parent().expect("parent"))
                .expect("list parent")
                .count(),
            1
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn precommit_failure_leaves_no_instance_or_staging_file() {
        let root = isolated_root("fault");
        let path = root.join(CLIENT_INSTANCE_DIR).join(CLIENT_INSTANCE_FILE);
        let result = publish_client_instance_id(&path, &[3; CLIENT_INSTANCE_ID_BYTES], || {
            Err(std::io::Error::other("injected precommit failure"))
        });
        assert!(result.is_err());
        assert!(!path.exists());
        assert_eq!(
            fs::read_dir(path.parent().expect("parent"))
                .expect("list parent")
                .count(),
            0
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn concurrent_first_creation_converges_on_one_persisted_identifier() {
        let root = isolated_root("concurrent");
        let store = ClientInstanceIdStore::for_identity_storage_root(&root);
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
        let first_store = store.clone();
        let first_barrier = barrier.clone();
        let first = std::thread::spawn(move || {
            first_store.load_or_create_with(|| {
                first_barrier.wait();
                [1; CLIENT_INSTANCE_ID_BYTES]
            })
        });
        let second_store = store.clone();
        let second = std::thread::spawn(move || {
            second_store.load_or_create_with(|| {
                barrier.wait();
                [2; CLIENT_INSTANCE_ID_BYTES]
            })
        });

        let first = first.join().expect("first creator").expect("first result");
        let second = second
            .join()
            .expect("second creator")
            .expect("second result");
        assert_eq!(first, second);
        assert_eq!(store.load().expect("load winner"), Some(first));
        assert_eq!(
            fs::read_dir(store.path().parent().expect("parent"))
                .expect("list parent")
                .count(),
            1
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn expected_instance_is_atomically_replaced_and_reused_after_restart() {
        let root = isolated_root("replace");
        let store = ClientInstanceIdStore::for_identity_storage_root(&root);
        let original = store
            .load_or_create_with(|| [1; CLIENT_INSTANCE_ID_BYTES])
            .expect("original instance");
        let replacement = ClientInstanceId::new([2; CLIENT_INSTANCE_ID_BYTES]);
        assert_eq!(
            store
                .replace_expected(original, replacement)
                .expect("replace instance"),
            replacement
        );
        assert_eq!(store.load().expect("reload replacement"), Some(replacement));
        assert_eq!(
            fs::read_dir(store.path().parent().expect("parent"))
                .expect("list parent")
                .count(),
            1
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn failed_or_stale_rotation_preserves_existing_instance() {
        let root = isolated_root("replace-failure");
        let store = ClientInstanceIdStore::for_identity_storage_root(&root);
        let original = store
            .load_or_create_with(|| [3; CLIENT_INSTANCE_ID_BYTES])
            .expect("original instance");
        let replacement = ClientInstanceId::new([4; CLIENT_INSTANCE_ID_BYTES]);
        let error = store
            .replace_expected_with(original, replacement, || {
                Err(std::io::Error::other("injected rotation failure"))
            })
            .expect_err("injected failure");
        assert!(error.to_string().contains("injected rotation failure"));
        assert_eq!(store.load().expect("preserved instance"), Some(original));

        let stale = store
            .replace_expected(ClientInstanceId::new([9; 16]), replacement)
            .expect_err("stale expected instance");
        assert!(stale.to_string().contains("changed before rotation"));
        assert_eq!(store.load().expect("still preserved"), Some(original));
        assert_eq!(
            fs::read_dir(store.path().parent().expect("parent"))
                .expect("list parent")
                .count(),
            1
        );
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn symlink_instance_and_parent_are_rejected_without_touching_targets() {
        use std::os::unix::fs::{symlink, PermissionsExt};

        let root = isolated_root("symlink");
        let outside = root.join("outside");
        fs::write(&outside, [8; CLIENT_INSTANCE_ID_BYTES]).expect("outside target");

        let store = ClientInstanceIdStore::for_identity_storage_root(&root);
        fs::create_dir_all(store.path().parent().expect("parent")).expect("create parent");
        fs::set_permissions(
            store.path().parent().expect("parent"),
            fs::Permissions::from_mode(0o700),
        )
        .expect("private parent");
        symlink(&outside, store.path()).expect("instance symlink");
        assert!(store.load().is_err());
        assert_eq!(fs::read(&outside).expect("outside remains"), [8; 16]);

        fs::remove_file(store.path()).expect("remove instance symlink");
        fs::remove_dir(store.path().parent().expect("parent")).expect("remove parent");
        let outside_dir = root.join("outside-dir");
        fs::create_dir(&outside_dir).expect("outside dir");
        symlink(&outside_dir, store.path().parent().expect("parent")).expect("parent symlink");
        assert!(store.load_or_create_with(|| [9; 16]).is_err());
        assert_eq!(fs::read_dir(&outside_dir).expect("outside dir").count(), 0);
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn permissive_existing_file_is_rejected_without_repair() {
        use std::os::unix::fs::PermissionsExt;

        let root = isolated_root("permissions");
        let store = ClientInstanceIdStore::for_identity_storage_root(&root);
        fs::create_dir_all(store.path().parent().expect("parent")).expect("create parent");
        fs::set_permissions(
            store.path().parent().expect("parent"),
            fs::Permissions::from_mode(0o700),
        )
        .expect("private parent");
        fs::write(store.path(), [4; CLIENT_INSTANCE_ID_BYTES]).expect("seed instance");
        fs::set_permissions(store.path(), fs::Permissions::from_mode(0o644))
            .expect("make permissive");
        assert!(store.load().is_err());
        assert_eq!(
            fs::metadata(store.path())
                .expect("metadata")
                .permissions()
                .mode()
                & 0o777,
            0o644
        );
        let _ = fs::remove_dir_all(root);
    }
}
