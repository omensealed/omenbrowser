use std::path::PathBuf;

use omenbrowser_rs::directory::{
    DirectoryEntry, DirectoryKind, DirectoryService, PreferredDelivery, TrustLevel,
};

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "omenbrowser-rs-directory-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

fn entry_with_last_seen(entry: &DirectoryEntry, last_seen: f64) -> serde_json::Value {
    let mut value = serde_json::to_value(entry).expect("entry json");
    value["last_seen"] = serde_json::json!(last_seen);
    value
}

#[test]
fn trust_levels_preserve_python_numeric_values() {
    assert_eq!(u8::from(TrustLevel::Warning), 0x00);
    assert_eq!(u8::from(TrustLevel::Untrusted), 0x01);
    assert_eq!(u8::from(TrustLevel::Unknown), 0x02);
    assert_eq!(u8::from(TrustLevel::Trusted), 0xff);
}

#[test]
fn directory_entry_defaults_match_python_post_init_spirit() {
    let entry = DirectoryEntry::new("abc", "Node", DirectoryKind::Node);

    assert_eq!(entry.trust_level, TrustLevel::Unknown);
    assert!(!entry.trusted);
    assert!(entry.hosts_node);
}

#[test]
fn directory_entry_round_trips_json_with_numeric_trust() {
    let mut entry = DirectoryEntry::new("abc", "Peer", DirectoryKind::Peer);
    entry.set_trust_level(TrustLevel::Trusted);

    let json = serde_json::to_string(&entry).expect("serialize directory entry");
    assert!(json.contains("\"trust_level\":255"));
    let decoded: DirectoryEntry = serde_json::from_str(&json).expect("deserialize directory entry");

    assert_eq!(decoded, entry);
}

#[test]
fn directory_service_loads_missing_as_empty_and_persists_announces() {
    let path = temp_dir("missing").join("directory.json");
    let mut service = DirectoryService::new(path.clone()).expect("service");
    assert!(service.list_entries().is_empty());

    service
        .ingest_announce("mock.node", "Mock Node", DirectoryKind::Node, None, None)
        .expect("announce");
    assert!(!path.exists());
    assert!(service.flush_pending_save().expect("flush live announces"));
    let reloaded = DirectoryService::new(path).expect("reload");

    assert_eq!(reloaded.list_entries().len(), 1);
    assert_eq!(
        reloaded.find("mock.node").map(|entry| entry.display_name),
        Some("Mock Node".into())
    );
}

#[test]
fn directory_service_does_not_rewrite_duplicate_announces_immediately() {
    let path = temp_dir("duplicate-cooldown").join("directory.json");
    let mut service = DirectoryService::new(path.clone()).expect("service");
    service
        .ingest_announce("peer.hash", "Peer", DirectoryKind::Peer, None, None)
        .expect("announce");
    assert!(service.flush_pending_save().expect("flush first announce"));
    let first = std::fs::read_to_string(&path).expect("first snapshot");

    service
        .ingest_announce("peer.hash", "Peer", DirectoryKind::Peer, None, None)
        .expect("duplicate announce");
    assert!(!service.flush_due_save().expect("duplicate still debounced"));
    let second = std::fs::read_to_string(&path).expect("second snapshot");

    assert_eq!(second, first);
    assert!(service.find("peer.hash").is_some());
}

#[test]
fn directory_service_persists_material_announce_changes() {
    let path = temp_dir("material-change").join("directory.json");
    let mut service = DirectoryService::new(path.clone()).expect("service");
    service
        .ingest_announce("peer.hash", "Peer", DirectoryKind::Peer, None, None)
        .expect("announce");
    assert!(service.flush_pending_save().expect("flush first announce"));
    let first = std::fs::read_to_string(&path).expect("first snapshot");

    service
        .ingest_announce("peer.hash", "Peer Renamed", DirectoryKind::Peer, None, None)
        .expect("renamed announce");
    assert!(service
        .flush_pending_save()
        .expect("flush renamed announce"));
    let second = std::fs::read_to_string(&path).expect("second snapshot");

    assert_ne!(second, first);
    assert!(second.contains("Peer Renamed"));
}

#[test]
fn directory_service_backs_up_corrupt_file() {
    let dir = temp_dir("corrupt");
    let path = dir.join("directory.json");
    std::fs::write(&path, b"{bad").expect("write corrupt");

    let service = DirectoryService::new(path).expect("service");
    let backups = std::fs::read_dir(dir)
        .expect("read dir")
        .filter_map(Result::ok)
        .filter(|entry| entry.file_name().to_string_lossy().contains(".corrupt."))
        .count();

    assert!(service.list_entries().is_empty());
    assert_eq!(backups, 1);
}

#[test]
fn directory_service_saved_trusted_and_preferences_survive_clear_live() {
    let path = temp_dir("persist").join("directory.json");
    let mut service = DirectoryService::new(path).expect("service");
    service
        .ingest_announce("peer", "Peer", DirectoryKind::Peer, None, None)
        .expect("announce");
    service
        .set_trust_level("peer", TrustLevel::Trusted)
        .expect("trust");
    service
        .set_preferred_delivery("peer", Some(PreferredDelivery::Propagated))
        .expect("delivery");
    service
        .set_identify_on_connect("peer", true)
        .expect("identify");
    service.clear_transient_announces().expect("clear");

    let entry = service.find("peer").expect("entry");
    assert!(entry.saved);
    assert!(entry.trusted);
    assert_eq!(
        entry.preferred_delivery,
        Some(PreferredDelivery::Propagated)
    );
    assert!(service.should_identify_on_connect("peer"));
    assert!(service.list_live_entries().is_empty());
}

#[test]
fn directory_service_filters_and_preserves_better_names() {
    let path = temp_dir("filter").join("directory.json");
    let mut service = DirectoryService::new(path).expect("service");
    service
        .ingest_announce("abcdef123456", "Real Peer", DirectoryKind::Peer, None, None)
        .expect("announce");
    service.save_entry("abcdef123456").expect("save entry");
    service
        .ingest_announce("abcdef123456", "abcdef12", DirectoryKind::Peer, None, None)
        .expect("placeholder announce");

    let saved = service.filtered_entries(Some(DirectoryKind::Peer), "real", Some(true));

    assert_eq!(saved.len(), 1);
    assert_eq!(saved[0].display_name, "Real Peer");
}

#[test]
fn directory_service_prunes_stale_live_only_entries_but_keeps_saved() {
    let path = temp_dir("stale").join("directory.json");
    let stale = DirectoryEntry::new("stale.peer", "Stale Peer", DirectoryKind::Peer);
    let mut saved = DirectoryEntry::new("saved.peer", "Saved Peer", DirectoryKind::Peer);
    saved.saved = true;
    let stale = entry_with_last_seen(&stale, 0.0);
    let saved = entry_with_last_seen(&saved, 0.0);
    let file = serde_json::json!({
        "entries": [stale.clone(), saved.clone()],
        "announce_stream": [stale, saved]
    });
    std::fs::write(&path, serde_json::to_vec_pretty(&file).expect("json")).expect("seed");

    let reloaded = DirectoryService::new(path).expect("reload");

    assert!(reloaded.find("stale.peer").is_none());
    assert!(reloaded.find("saved.peer").is_some());
    assert!(reloaded
        .filtered_entries(Some(DirectoryKind::Peer), "saved", Some(true))
        .iter()
        .any(|entry| entry.destination_hash == "saved.peer"));
}

#[test]
fn directory_live_entries_hide_stale_transients_on_load() {
    let path = temp_dir("stale-live").join("directory.json");
    let stale = DirectoryEntry::new("stale.node", "Stale Node", DirectoryKind::Node);
    let stale = entry_with_last_seen(&stale, 0.0);
    let file = serde_json::json!({
        "entries": [stale.clone()],
        "announce_stream": [stale]
    });
    std::fs::write(&path, serde_json::to_vec_pretty(&file).expect("json")).expect("seed");

    let service = DirectoryService::new(path).expect("service");

    assert!(service.list_live_entries().is_empty());
}

#[test]
fn directory_service_prunes_transient_overflow_during_ingest() {
    let path = temp_dir("overflow-live").join("directory.json");
    let mut service = DirectoryService::new(path).expect("service");

    for index in 0..1026 {
        service
            .ingest_announce(
                format!("peer-{index:04}"),
                format!("Peer {index:04}"),
                DirectoryKind::Peer,
                None,
                None,
            )
            .expect("announce");
    }

    let entries = service.list_entries();
    assert!(entries.len() <= 1024);
    assert!(service.find("peer-0000").is_none());
    assert!(service.find("peer-1025").is_some());
}
