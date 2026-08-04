use std::path::PathBuf;

use omenbrowser_rs::config::AppPaths;

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "omenbrowser-rs-paths-integration-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

#[test]
fn app_paths_from_root_uses_documented_layout() {
    let root = PathBuf::from("/tmp/omenbrowser-rs-layout");
    let paths = AppPaths::from_root(root.clone());

    assert_eq!(paths.settings_file, root.join("settings.json"));
    assert_eq!(paths.identities_dir, root.join("identities"));
    assert_eq!(
        paths.identity_backups_dir,
        root.join("identities").join("backups")
    );
    assert_eq!(paths.identity_storage_dir, root.join("identity_storage"));
    assert_eq!(
        paths.reticulum_storage_dir,
        root.join("reticulum").join("storage")
    );
    assert_eq!(paths.downloads_dir, root.join("downloads"));
    assert_eq!(paths.attachments_dir, root.join("attachments"));
    assert_eq!(paths.interfaces_file, root.join("interfaces.json"));
    assert_eq!(
        paths.browser_form_state_file,
        root.join("browser_form_state.json")
    );
}

#[test]
fn app_paths_discover_prefers_config_omenbrowser_rs_when_home_exists() {
    let paths = AppPaths::discover().expect("discover paths");
    if std::env::var_os("HOME").is_some() {
        assert!(paths.root.ends_with(".config/OMENbrowser_rs"));
    }
}

#[test]
fn app_paths_ensure_creates_required_directories() {
    let root = temp_dir("ensure");
    let paths = AppPaths::from_root(root);

    paths.ensure().expect("ensure app paths");

    for dir in [
        paths.root,
        paths.identities_dir,
        paths.identity_backups_dir,
        paths.identity_storage_dir,
        paths.reticulum_config_dir,
        paths.reticulum_storage_dir,
        paths.messages_dir,
        paths.attachments_dir,
        paths.cache_dir,
        paths.downloads_dir,
        paths.plugins_dir,
        paths.logs_dir,
        paths.diagnostics_dir,
    ] {
        assert!(dir.is_dir(), "expected {} to exist", dir.display());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&dir)
                    .expect("directory metadata")
                    .permissions()
                    .mode()
                    & 0o777,
                0o700
            );
        }
    }
}

#[cfg(unix)]
#[test]
fn app_paths_ensure_repairs_managed_directories_without_changing_parent() {
    use std::os::unix::fs::PermissionsExt;

    let parent = temp_dir("repair-parent");
    std::fs::create_dir_all(&parent).expect("parent");
    std::fs::set_permissions(&parent, std::fs::Permissions::from_mode(0o755)).expect("parent mode");
    let paths = AppPaths::from_root(parent.join("managed"));
    std::fs::create_dir_all(&paths.logs_dir).expect("permissive managed tree");
    std::fs::set_permissions(&paths.root, std::fs::Permissions::from_mode(0o755))
        .expect("root mode");
    std::fs::set_permissions(&paths.logs_dir, std::fs::Permissions::from_mode(0o755))
        .expect("logs mode");

    paths.ensure().expect("repair paths");

    assert_eq!(
        std::fs::metadata(&paths.root)
            .expect("root metadata")
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
    assert_eq!(
        std::fs::metadata(&paths.logs_dir)
            .expect("logs metadata")
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
    assert_eq!(
        std::fs::metadata(&parent)
            .expect("parent metadata")
            .permissions()
            .mode()
            & 0o777,
        0o755
    );
    std::fs::remove_dir_all(parent).expect("cleanup");
}

#[cfg(unix)]
#[test]
fn app_paths_ensure_repairs_known_private_files_without_changing_bytes() {
    use std::os::unix::fs::PermissionsExt;

    let root = temp_dir("repair-files");
    let paths = AppPaths::from_root(root.clone());
    paths.ensure().expect("create paths");
    let files = [
        paths.settings_file.clone(),
        paths.directory_file.clone(),
        paths.interfaces_file.clone(),
        paths.gateways_file.clone(),
        paths.browser_form_state_file.clone(),
    ];
    for (index, path) in files.iter().enumerate() {
        std::fs::write(path, format!("preserved-{index}")).expect("private fixture");
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o644))
            .expect("permissive file mode");
    }

    paths.ensure().expect("repair paths");

    for (index, path) in files.iter().enumerate() {
        assert_eq!(
            std::fs::read_to_string(path).expect("preserved file"),
            format!("preserved-{index}")
        );
        assert_eq!(
            std::fs::metadata(path)
                .expect("file metadata")
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
    std::fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn app_paths_scope_identity_owned_storage_without_moving_global_files() {
    let root = PathBuf::from("/tmp/omenbrowser-rs-layout");
    let paths = AppPaths::from_root(root.clone());
    let identity_path = root.join("identities").join("default_identity");
    let scoped = paths.scoped_to_identity_path(&identity_path);

    assert_eq!(scoped.root, root);
    assert_eq!(scoped.settings_file, root.join("settings.json"));
    assert_eq!(scoped.identities_dir, root.join("identities"));
    assert_eq!(scoped.plugins_dir, root.join("plugins"));
    assert_eq!(scoped.logs_dir, root.join("logs"));
    assert!(scoped
        .messages_dir
        .starts_with(root.join("identity_storage")));
    assert!(scoped.cache_dir.starts_with(root.join("identity_storage")));
    assert!(scoped
        .directory_file
        .starts_with(root.join("identity_storage")));
    assert!(scoped
        .reticulum_storage_dir
        .starts_with(root.join("identity_storage")));
}

#[test]
fn app_paths_adopt_legacy_app_storage_once_without_overwriting() {
    let root = temp_dir("adopt-legacy");
    let legacy = AppPaths::from_root(root.clone());
    legacy.ensure().expect("ensure legacy");
    std::fs::write(legacy.messages_dir.join("thread.json"), b"old").expect("message");
    std::fs::write(legacy.cache_dir.join("page.json"), b"cache").expect("cache");
    std::fs::write(&legacy.directory_file, b"{\"entries\":[]}").expect("directory");
    std::fs::write(&legacy.browser_form_state_file, b"{}").expect("form state");
    std::fs::write(legacy.reticulum_config_dir.join("config"), b"rns").expect("config");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&legacy.messages_dir, std::fs::Permissions::from_mode(0o755))
            .expect("legacy source mode");
    }

    let scoped = legacy.scoped_to_identity_path(&legacy.identities_dir.join("default_identity"));
    std::fs::create_dir_all(&scoped.messages_dir).expect("scoped messages");
    std::fs::write(scoped.messages_dir.join("thread.json"), b"new").expect("existing message");

    let migration = scoped
        .adopt_legacy_app_storage_once(&legacy)
        .expect("migration")
        .expect("migration ran");

    assert_eq!(
        std::fs::read_to_string(scoped.messages_dir.join("thread.json")).expect("read existing"),
        "new"
    );
    assert_eq!(
        std::fs::read_to_string(scoped.cache_dir.join("page.json")).expect("read cache"),
        "cache"
    );
    assert_eq!(
        std::fs::read_to_string(&scoped.directory_file).expect("read directory"),
        "{\"entries\":[]}"
    );
    assert_eq!(
        std::fs::read_to_string(scoped.reticulum_config_dir.join("config")).expect("read config"),
        "rns"
    );
    assert!(migration.copied_files >= 4);
    assert_eq!(migration.skipped_existing, 1);
    assert!(legacy
        .identity_storage_dir
        .join(".app_level_storage_adopted")
        .is_file());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&legacy.messages_dir)
                .expect("legacy source metadata")
                .permissions()
                .mode()
                & 0o777,
            0o755,
            "legacy source permissions must remain unchanged"
        );
        assert_eq!(
            std::fs::metadata(
                legacy
                    .identity_storage_dir
                    .join(".app_level_storage_adopted")
            )
            .expect("marker metadata")
            .permissions()
            .mode()
                & 0o777,
            0o600
        );
    }

    let second = scoped
        .adopt_legacy_app_storage_once(&legacy)
        .expect("second migration");
    assert!(second.is_none());
}
