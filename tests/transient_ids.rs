use std::collections::BTreeMap;
use std::path::PathBuf;

use omenbrowser_rs::storage::transient_ids::{
    hex_encode, DeliveredTransientIdStore, LXMF_LOCAL_DELIVERY_CACHE_CORRUPT_BACKUP_MAX_FILES,
    LXMF_LOCAL_DELIVERY_CACHE_MAX_BYTES, LXMF_LOCAL_DELIVERY_CACHE_MAX_ITEMS,
};

fn test_path(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "omenbrowser-rs-transient-{}-{name}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&path);
    path
}

fn corrupt_backups(path: &std::path::Path) -> Vec<PathBuf> {
    let prefix = format!(
        "{}.corrupt.",
        path.file_name().expect("cache filename").to_string_lossy()
    );
    let mut backups = std::fs::read_dir(path.parent().expect("parent"))
        .expect("read parent")
        .filter_map(Result::ok)
        .filter(|entry| {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            name.starts_with(&prefix) && name.ends_with(".bak")
        })
        .map(|entry| entry.path())
        .collect::<Vec<_>>();
    backups.sort();
    backups
}

#[test]
fn missing_cache_loads_empty() {
    let store = DeliveredTransientIdStore::from_path(test_path("missing").join("ids.json"));

    let ids = store.load_or_default().expect("load default cache");

    assert!(ids.is_empty());
}

#[test]
fn cache_round_trips_ids() {
    let path = test_path("round-trip").join("ids.json");
    let store = DeliveredTransientIdStore::from_path(&path);
    let id = [0x42; 32];
    let mut ids = BTreeMap::new();
    DeliveredTransientIdStore::mark_delivered(&mut ids, &id, 123.0);

    store.save(&ids).expect("save transient id cache");
    let loaded = store.load_or_default().expect("load transient id cache");

    assert!(DeliveredTransientIdStore::has_delivered(&loaded, &id));
    assert_eq!(loaded.get(&hex_encode(&id)), Some(&123.0));
}

#[test]
fn legacy_bare_map_round_trips_ids() {
    let path = test_path("legacy-round-trip").join("ids.json");
    std::fs::create_dir_all(path.parent().expect("parent")).expect("create parent");
    let id = [0x24; 32];
    std::fs::write(&path, format!("{{\"{}\":321.0}}", hex_encode(&id)))
        .expect("write legacy cache");

    let loaded = DeliveredTransientIdStore::from_path(&path)
        .load_or_default()
        .expect("load legacy cache");

    assert_eq!(loaded.get(&hex_encode(&id)), Some(&321.0));
}

#[test]
fn versioned_wrapper_retains_unknown_field_compatibility() {
    let path = test_path("versioned-extension").join("ids.json");
    std::fs::create_dir_all(path.parent().expect("parent")).expect("create parent");
    let id = [0x25; 32];
    std::fs::write(
        &path,
        format!(
            "{{\"ids\":{{\"{}\":322.0}},\"future_extension\":true}}",
            hex_encode(&id)
        ),
    )
    .expect("write extended versioned cache");

    let loaded = DeliveredTransientIdStore::from_path(&path)
        .load_or_default()
        .expect("load extended versioned cache");

    assert_eq!(loaded.get(&hex_encode(&id)), Some(&322.0));
}

#[test]
fn corrupt_cache_is_backed_up_exactly_without_changing_source() {
    let path = test_path("corrupt").join("ids.json");
    let _ = std::fs::remove_dir_all(path.parent().expect("parent"));
    std::fs::create_dir_all(path.parent().expect("parent")).expect("create parent");
    std::fs::write(&path, b"not json").expect("write corrupt cache");
    let store = DeliveredTransientIdStore::from_path(&path);

    let loaded = store.load_or_default().expect("load corrupt cache");

    assert!(loaded.is_empty());
    assert_eq!(std::fs::read(&path).expect("read source"), b"not json");
    let backups = corrupt_backups(&path);
    assert_eq!(backups.len(), 1);
    assert_eq!(
        std::fs::read(&backups[0]).expect("read backup"),
        b"not json"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&backups[0])
                .expect("backup metadata")
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
}

#[test]
fn prune_removes_expired_ids() {
    let mut ids = BTreeMap::new();
    ids.insert("old".into(), 10.0);
    ids.insert("new".into(), 95.0);

    let removed = DeliveredTransientIdStore::prune_expired(&mut ids, 100.0, 20.0);

    assert_eq!(removed, 1);
    assert!(!ids.contains_key("old"));
    assert!(ids.contains_key("new"));
}

#[test]
fn cache_prunes_oldest_entries_below_item_ceiling() {
    let mut ids = BTreeMap::new();
    for index in 0..=LXMF_LOCAL_DELIVERY_CACHE_MAX_ITEMS {
        ids.insert(format!("{index:064x}"), index as f64);
    }

    let removed = DeliveredTransientIdStore::prune_to_limit(&mut ids);

    assert!(removed > 1);
    assert!(ids.len() < LXMF_LOCAL_DELIVERY_CACHE_MAX_ITEMS);
    assert!(!ids.contains_key(&format!("{:064x}", 0)));
    assert!(ids.contains_key(&format!("{:064x}", LXMF_LOCAL_DELIVERY_CACHE_MAX_ITEMS)));
}

#[test]
fn oversized_cache_is_rejected_without_reading_backup_or_mutation() {
    let path = test_path("oversized").join("ids.json");
    let parent = path.parent().expect("parent");
    let _ = std::fs::remove_dir_all(parent);
    std::fs::create_dir_all(parent).expect("create parent");
    std::fs::File::create(&path)
        .and_then(|file| file.set_len(LXMF_LOCAL_DELIVERY_CACHE_MAX_BYTES + 1))
        .expect("oversized sparse cache");
    let store = DeliveredTransientIdStore::from_path(&path);

    let error = store.load_or_default().expect_err("reject oversized cache");

    assert!(error.to_string().contains("8388608 byte limit"));
    assert_eq!(
        std::fs::metadata(&path).expect("source metadata").len(),
        LXMF_LOCAL_DELIVERY_CACHE_MAX_BYTES + 1
    );
    assert!(corrupt_backups(&path).is_empty());
}

#[test]
fn exact_byte_limit_with_json_whitespace_is_accepted() {
    let path = test_path("exact-limit").join("ids.json");
    std::fs::create_dir_all(path.parent().expect("parent")).expect("create parent");
    let mut raw = b"{\"ids\":{}}".to_vec();
    raw.resize(LXMF_LOCAL_DELIVERY_CACHE_MAX_BYTES as usize, b' ');
    std::fs::write(&path, raw).expect("write exact-limit cache");

    let loaded = DeliveredTransientIdStore::from_path(&path)
        .load_or_default()
        .expect("load exact-limit cache");

    assert!(loaded.is_empty());
    assert!(corrupt_backups(&path).is_empty());
}

#[test]
fn semantic_invalid_cache_is_backed_up_and_defaulted() {
    let path = test_path("invalid-id").join("ids.json");
    std::fs::create_dir_all(path.parent().expect("parent")).expect("create parent");
    let raw = b"{\"ids\":{\"not-a-transient-id\":123.0}}";
    std::fs::write(&path, raw).expect("write invalid cache");

    let loaded = DeliveredTransientIdStore::from_path(&path)
        .load_or_default()
        .expect("default invalid cache");

    assert!(loaded.is_empty());
    assert_eq!(std::fs::read(&path).expect("read source"), raw);
    let backups = corrupt_backups(&path);
    assert_eq!(backups.len(), 1);
    assert_eq!(std::fs::read(&backups[0]).expect("read backup"), raw);
}

#[test]
fn corrupt_backup_retention_is_bounded_and_ignores_legacy_names() {
    let path = test_path("backup-retention").join("ids.json");
    let parent = path.parent().expect("parent");
    std::fs::create_dir_all(parent).expect("create parent");
    std::fs::write(&path, b"invalid").expect("write invalid cache");
    let legacy = parent.join("ids.corrupt-legacy");
    std::fs::write(&legacy, b"legacy").expect("write legacy backup");
    let store = DeliveredTransientIdStore::from_path(&path);

    for _ in 0..LXMF_LOCAL_DELIVERY_CACHE_CORRUPT_BACKUP_MAX_FILES + 3 {
        assert!(store
            .load_or_default()
            .expect("load invalid cache")
            .is_empty());
    }

    assert_eq!(
        corrupt_backups(&path).len(),
        LXMF_LOCAL_DELIVERY_CACHE_CORRUPT_BACKUP_MAX_FILES
    );
    assert_eq!(
        std::fs::read(legacy).expect("read legacy backup"),
        b"legacy"
    );
}

#[test]
fn save_is_private_and_replaces_complete_cache() {
    let path = test_path("private-save").join("ids.json");
    let store = DeliveredTransientIdStore::from_path(&path);
    let mut first = BTreeMap::new();
    DeliveredTransientIdStore::mark_delivered(&mut first, &[1; 32], 1.0);
    store.save(&first).expect("save first cache");
    let mut second = BTreeMap::new();
    DeliveredTransientIdStore::mark_delivered(&mut second, &[2; 32], 2.0);
    store.save(&second).expect("replace cache");

    assert_eq!(store.load_or_default().expect("load replacement"), second);
    assert_eq!(
        std::fs::read_dir(path.parent().expect("parent"))
            .expect("list cache parent")
            .count(),
        1
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&path)
                .expect("cache metadata")
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
}

#[test]
fn invalid_save_preserves_previous_cache_without_staging() {
    let path = test_path("invalid-save").join("ids.json");
    let store = DeliveredTransientIdStore::from_path(&path);
    let mut valid = BTreeMap::new();
    DeliveredTransientIdStore::mark_delivered(&mut valid, &[3; 32], 3.0);
    store.save(&valid).expect("save valid cache");
    let previous = std::fs::read(&path).expect("read valid cache");
    let invalid = BTreeMap::from([("invalid".to_owned(), 4.0)]);

    let error = store.save(&invalid).expect_err("reject invalid cache");

    assert!(error.to_string().contains("invalid transient id"));
    assert_eq!(
        std::fs::read(&path).expect("read preserved cache"),
        previous
    );
    assert_eq!(
        std::fs::read_dir(path.parent().expect("parent"))
            .expect("list cache parent")
            .count(),
        1
    );
}

#[test]
fn directory_cache_is_rejected_without_backup() {
    let path = test_path("directory").join("ids.json");
    std::fs::create_dir_all(&path).expect("create directory cache");

    let error = DeliveredTransientIdStore::from_path(&path)
        .load_or_default()
        .expect_err("reject directory cache");

    assert!(error.to_string().contains("regular file"));
    assert!(path.is_dir());
    assert!(corrupt_backups(&path).is_empty());
}

#[cfg(unix)]
#[test]
fn symlink_cache_is_rejected_without_touching_target() {
    use std::os::unix::fs::symlink;

    let root = test_path("symlink");
    std::fs::create_dir_all(&root).expect("create root");
    let target = root.join("target.json");
    let path = root.join("ids.json");
    std::fs::write(&target, b"target bytes").expect("write target");
    symlink(&target, &path).expect("create symlink");

    let error = DeliveredTransientIdStore::from_path(&path)
        .load_or_default()
        .expect_err("reject symlink");

    assert!(error.to_string().contains("regular file"));
    assert!(std::fs::symlink_metadata(&path)
        .expect("symlink metadata")
        .file_type()
        .is_symlink());
    assert_eq!(std::fs::read(target).expect("read target"), b"target bytes");
    assert!(corrupt_backups(&path).is_empty());
}
