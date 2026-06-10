use std::collections::BTreeMap;
use std::path::PathBuf;

use omenbrowser_rs::storage::transient_ids::{hex_encode, DeliveredTransientIdStore};

fn test_path(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "omenbrowser-rs-transient-{}-{name}",
        std::process::id()
    ))
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
fn corrupt_cache_is_backed_up_and_recreated_empty() {
    let path = test_path("corrupt").join("ids.json");
    let _ = std::fs::remove_dir_all(path.parent().expect("parent"));
    std::fs::create_dir_all(path.parent().expect("parent")).expect("create parent");
    std::fs::write(&path, b"not json").expect("write corrupt cache");
    let store = DeliveredTransientIdStore::from_path(&path);

    let loaded = store.load_or_default().expect("load corrupt cache");

    assert!(loaded.is_empty());
    assert!(!path.exists());
    let backups = std::fs::read_dir(path.parent().expect("parent"))
        .expect("read parent")
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with("ids.corrupt-")
        })
        .count();
    assert_eq!(backups, 1);
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
