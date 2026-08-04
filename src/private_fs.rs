use std::fs::{File, OpenOptions};
use std::io;
use std::path::Path;

const PRIVATE_DIRECTORY_MODE: u32 = 0o700;
const PRIVATE_FILE_MODE: u32 = 0o600;

pub(crate) fn ensure_private_dir(path: &Path) -> io::Result<()> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) => require_directory(&metadata)?,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            let mut builder = std::fs::DirBuilder::new();
            builder.recursive(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt;
                builder.mode(PRIVATE_DIRECTORY_MODE);
            }
            builder.create(path)?;
            require_directory(&std::fs::symlink_metadata(path)?)?;
        }
        Err(error) => return Err(error),
    }
    set_mode(path, PRIVATE_DIRECTORY_MODE)
}

/// Validate an existing external/custom parent without claiming its mode, or
/// privately create the missing final directory that will hold managed state.
pub(crate) fn ensure_private_parent_dir(path: &Path) -> io::Result<()> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) => require_directory(&metadata),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            let parent = path.parent().ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "private directory has no parent",
                )
            })?;
            require_directory(&std::fs::symlink_metadata(parent)?)?;
            let builder = std::fs::DirBuilder::new();
            #[cfg(unix)]
            let mut builder = builder;
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt;
                builder.mode(PRIVATE_DIRECTORY_MODE);
            }
            builder.create(path)?;
            require_directory(&std::fs::symlink_metadata(path)?)?;
            set_mode(path, PRIVATE_DIRECTORY_MODE)
        }
        Err(error) => Err(error),
    }
}

pub(crate) fn repair_private_file(path: &Path) -> io::Result<()> {
    require_regular_file(&std::fs::symlink_metadata(path)?)?;
    set_mode(path, PRIVATE_FILE_MODE)
}

pub(crate) fn repair_private_file_if_exists(path: &Path) -> io::Result<bool> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) => {
            require_regular_file(&metadata)?;
            set_mode(path, PRIVATE_FILE_MODE)?;
            Ok(true)
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error),
    }
}

pub(crate) fn create_private_new(path: &Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(PRIVATE_FILE_MODE);
    }
    let file = options.open(path)?;
    repair_private_file(path)?;
    Ok(file)
}

pub(crate) fn open_private_append(path: &Path) -> io::Result<File> {
    if repair_private_file_if_exists(path)? {
        return OpenOptions::new().append(true).open(path);
    }
    match create_private_append_new(path) {
        Ok(file) => Ok(file),
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            repair_private_file(path)?;
            OpenOptions::new().append(true).open(path)
        }
        Err(error) => Err(error),
    }
}

fn create_private_append_new(path: &Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.append(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(PRIVATE_FILE_MODE);
    }
    let file = options.open(path)?;
    repair_private_file(path)?;
    Ok(file)
}

fn require_directory(metadata: &std::fs::Metadata) -> io::Result<()> {
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "private directory path is not a directory",
        ));
    }
    Ok(())
}

fn require_regular_file(metadata: &std::fs::Metadata) -> io::Result<()> {
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "private file path is not a regular file",
        ));
    }
    Ok(())
}

#[cfg(unix)]
fn set_mode(path: &Path, mode: u32) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))
}

#[cfg(not(unix))]
fn set_mode(_path: &Path, _mode: u32) -> io::Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn root(label: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "omenbrowser-private-fs-{label}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ))
    }

    #[cfg(unix)]
    #[test]
    fn repairs_owned_directory_and_sensitive_file_without_changing_bytes() {
        use std::os::unix::fs::PermissionsExt;

        let root = root("repair");
        std::fs::create_dir_all(&root).expect("root");
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o755))
            .expect("permissive directory");
        let file = root.join("state");
        std::fs::write(&file, b"preserved").expect("state");
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644))
            .expect("permissive file");

        ensure_private_dir(&root).expect("protect directory");
        repair_private_file(&file).expect("protect file");

        assert_eq!(
            std::fs::metadata(&root)
                .expect("metadata")
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        assert_eq!(
            std::fs::metadata(&file)
                .expect("metadata")
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        assert_eq!(std::fs::read(file).expect("read"), b"preserved");
        std::fs::remove_dir_all(root).expect("cleanup");
    }

    #[cfg(unix)]
    #[test]
    fn refuses_symlinked_private_paths() {
        use std::os::unix::fs::symlink;

        let root = root("symlink");
        std::fs::create_dir_all(&root).expect("root");
        let target = root.join("target");
        std::fs::write(&target, b"untouched").expect("target");
        let link = root.join("link");
        symlink(&target, &link).expect("link");

        assert_eq!(
            repair_private_file(&link).expect_err("must refuse").kind(),
            io::ErrorKind::InvalidInput
        );
        assert_eq!(std::fs::read(target).expect("read"), b"untouched");
        std::fs::remove_dir_all(root).expect("cleanup");
    }

    #[cfg(unix)]
    #[test]
    fn existing_custom_parent_is_validated_without_changing_its_mode() {
        use std::os::unix::fs::PermissionsExt;

        let root = root("custom-parent");
        std::fs::create_dir_all(&root).expect("root");
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o755))
            .expect("parent mode");

        ensure_private_parent_dir(&root).expect("validate parent");

        assert_eq!(
            std::fs::metadata(&root)
                .expect("metadata")
                .permissions()
                .mode()
                & 0o777,
            0o755
        );
        std::fs::remove_dir_all(root).expect("cleanup");
    }

    #[cfg(unix)]
    #[test]
    fn missing_custom_final_directory_is_private_without_creating_ancestors() {
        use std::os::unix::fs::PermissionsExt;

        let root = root("custom-final");
        std::fs::create_dir_all(&root).expect("root");
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o755))
            .expect("parent mode");
        let final_dir = root.join("managed-final");

        ensure_private_parent_dir(&final_dir).expect("create final directory");

        assert_eq!(
            std::fs::metadata(&root)
                .expect("root metadata")
                .permissions()
                .mode()
                & 0o777,
            0o755
        );
        assert_eq!(
            std::fs::metadata(&final_dir)
                .expect("final metadata")
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        let nested = root.join("missing-ancestor").join("managed-final");
        assert_eq!(
            ensure_private_parent_dir(&nested)
                .expect_err("must not create unrelated ancestor")
                .kind(),
            io::ErrorKind::NotFound
        );
        assert!(!root.join("missing-ancestor").exists());
        std::fs::remove_dir_all(root).expect("cleanup");
    }
}
