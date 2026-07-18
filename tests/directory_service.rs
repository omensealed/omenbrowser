use std::path::PathBuf;

use omenbrowser_rs::directory::{
    DirectoryAnnounceMetadata, DirectoryEntry, DirectoryKind, DirectoryService, PreferredDelivery,
    TrustLevel, DIRECTORY_CORRUPT_BACKUP_MAX_FILES, DIRECTORY_FILE_MAX_BYTES,
    DIRECTORY_MAX_DISPLAY_NAME_BYTES, DIRECTORY_MAX_ENTRIES,
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

fn corrupt_backups(path: &std::path::Path) -> Vec<PathBuf> {
    let prefix = format!(
        "{}.corrupt.",
        path.file_name()
            .expect("directory filename")
            .to_string_lossy()
    );
    let mut backups = std::fs::read_dir(path.parent().expect("parent"))
        .expect("read parent")
        .filter_map(Result::ok)
        .filter(|entry| {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            name.strip_prefix(&prefix)
                .and_then(|name| name.strip_suffix(".bak"))
                .is_some_and(|body| {
                    body.split('.').count() == 3
                        && body.split('.').all(|part| {
                            !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit())
                        })
                })
        })
        .map(|entry| entry.path())
        .collect::<Vec<_>>();
    backups.sort();
    backups
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
    let reloaded = DirectoryService::new(path.clone()).expect("reload");

    assert_eq!(reloaded.list_entries().len(), 1);
    assert_eq!(
        reloaded.find("mock.node").map(|entry| entry.display_name),
        Some("Mock Node".into())
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&path)
                .expect("directory metadata")
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
}

#[test]
fn omenchat_announce_identity_is_validated_persisted_and_immutable_per_destination() {
    let path = temp_dir("omenchat-identity").join("directory.json");
    let mut service = DirectoryService::new(path.clone()).expect("service");
    let destination = "00112233445566778899aabbccddeeff";
    let identity = "ffeeddccbbaa99887766554433221100";

    let entry = service
        .ingest_announce_with_identity_metadata(
            destination,
            "Verified Chat",
            DirectoryKind::OmenChat,
            DirectoryAnnounceMetadata {
                identity_hash: Some(identity.into()),
                ..DirectoryAnnounceMetadata::default()
            },
        )
        .expect("verified announce");
    assert_eq!(entry.identity_hash.as_deref(), Some(identity));
    assert!(service.flush_pending_save().expect("flush announce"));
    let reloaded = DirectoryService::new(path).expect("reload");
    assert_eq!(
        reloaded
            .find(destination)
            .and_then(|entry| entry.identity_hash),
        Some(identity.into())
    );

    let error = service
        .ingest_announce_with_identity_metadata(
            destination,
            "Impostor",
            DirectoryKind::OmenChat,
            DirectoryAnnounceMetadata {
                identity_hash: Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into()),
                ..DirectoryAnnounceMetadata::default()
            },
        )
        .expect_err("identity mutation must fail closed");
    assert!(error.to_string().contains("identity changed"));
    assert_eq!(
        service
            .find(destination)
            .and_then(|entry| entry.identity_hash),
        Some(identity.into())
    );
}

#[test]
fn omenchat_announce_rejects_malformed_identity_hash_before_mutation() {
    let path = temp_dir("omenchat-invalid-identity").join("directory.json");
    let mut service = DirectoryService::new(path).expect("service");

    let error = service
        .ingest_announce_with_identity_metadata(
            "00112233445566778899aabbccddeeff",
            "Invalid Chat",
            DirectoryKind::OmenChat,
            DirectoryAnnounceMetadata {
                identity_hash: Some("not-a-reticulum-identity".into()),
                ..DirectoryAnnounceMetadata::default()
            },
        )
        .expect_err("malformed identity must fail closed");

    assert!(error.to_string().contains("32-character hexadecimal"));
    assert!(service.list_entries().is_empty());
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
fn directory_service_records_lxmf_stamp_cost_from_peer_announces() {
    let path = temp_dir("lxmf-stamp-cost").join("directory.json");
    let mut service = DirectoryService::new(path).expect("service");
    service
        .ingest_announce_with_metadata(
            "peer.hash",
            "Peer",
            DirectoryKind::Peer,
            None,
            None,
            Some(8),
        )
        .expect("announce with cost");

    assert_eq!(
        service
            .find("peer.hash")
            .and_then(|entry| entry.lxmf_stamp_cost),
        Some(8)
    );

    service
        .ingest_announce("peer.hash", "Peer", DirectoryKind::Peer, None, None)
        .expect("announce without cost");

    assert_eq!(
        service
            .find("peer.hash")
            .and_then(|entry| entry.lxmf_stamp_cost),
        Some(8)
    );
}

#[test]
fn directory_service_backs_up_corrupt_file() {
    let dir = temp_dir("corrupt");
    let path = dir.join("directory.json");
    let raw = b"{bad";
    std::fs::write(&path, raw).expect("write corrupt");

    let service = DirectoryService::new(path.clone()).expect("service");
    let backups = corrupt_backups(&path);

    assert!(service.list_entries().is_empty());
    assert_eq!(std::fs::read(&path).expect("read source"), raw);
    assert_eq!(backups.len(), 1);
    assert_eq!(std::fs::read(&backups[0]).expect("read backup"), raw);
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
fn oversized_directory_file_is_rejected_without_read_backup_or_mutation() {
    let path = temp_dir("oversized").join("directory.json");
    std::fs::File::create(&path)
        .and_then(|file| file.set_len(DIRECTORY_FILE_MAX_BYTES + 1))
        .expect("write oversized sparse directory");

    let error = DirectoryService::new(path.clone()).expect_err("reject oversized directory");

    assert!(error.to_string().contains("8388608 byte limit"));
    assert_eq!(
        std::fs::metadata(&path).expect("source metadata").len(),
        DIRECTORY_FILE_MAX_BYTES + 1
    );
    assert!(corrupt_backups(&path).is_empty());
}

#[test]
fn exact_byte_limit_with_json_whitespace_is_accepted() {
    let path = temp_dir("exact-limit").join("directory.json");
    let mut raw = b"{\"entries\":[],\"announce_stream\":[]}".to_vec();
    raw.resize(DIRECTORY_FILE_MAX_BYTES as usize, b' ');
    std::fs::write(&path, raw).expect("write exact-limit directory");

    let service = DirectoryService::new(path.clone()).expect("load exact-limit directory");

    assert!(service.list_entries().is_empty());
    assert!(corrupt_backups(&path).is_empty());
}

#[test]
fn directory_path_is_rejected_without_backup() {
    let path = temp_dir("directory-path").join("directory.json");
    std::fs::create_dir(&path).expect("create directory target");

    let error = DirectoryService::new(path.clone()).expect_err("reject directory target");

    assert!(error.to_string().contains("regular file"));
    assert!(path.is_dir());
    assert!(corrupt_backups(&path).is_empty());
}

#[cfg(unix)]
#[test]
fn symlink_directory_file_is_rejected_without_touching_target() {
    use std::os::unix::fs::symlink;

    let root = temp_dir("symlink");
    let path = root.join("directory.json");
    let target = root.join("target.json");
    std::fs::write(&target, b"target bytes").expect("write target");
    symlink(&target, &path).expect("create symlink");

    let error = DirectoryService::new(path.clone()).expect_err("reject symlink target");

    assert!(error.to_string().contains("regular file"));
    assert!(std::fs::symlink_metadata(&path)
        .expect("symlink metadata")
        .file_type()
        .is_symlink());
    assert_eq!(std::fs::read(target).expect("read target"), b"target bytes");
    assert!(corrupt_backups(&path).is_empty());
}

#[test]
fn corrupt_backup_retention_is_bounded_and_ignores_legacy_names() {
    let root = temp_dir("backup-retention");
    let path = root.join("directory.json");
    std::fs::write(&path, b"invalid").expect("write invalid directory");
    let legacy = root.join("directory.json.corrupt.legacy.bak");
    std::fs::write(&legacy, b"legacy").expect("write legacy backup");

    for _ in 0..DIRECTORY_CORRUPT_BACKUP_MAX_FILES + 3 {
        assert!(DirectoryService::new(path.clone())
            .expect("load invalid directory")
            .list_entries()
            .is_empty());
    }

    assert_eq!(
        corrupt_backups(&path).len(),
        DIRECTORY_CORRUPT_BACKUP_MAX_FILES
    );
    assert_eq!(
        std::fs::read(legacy).expect("read legacy backup"),
        b"legacy"
    );
}

#[test]
fn failed_trust_save_restores_prior_entry_state() {
    let path = temp_dir("trust-rollback").join("directory.json");
    let mut service = DirectoryService::new(path.clone()).expect("service");
    service
        .ingest_announce("peer", "Peer", DirectoryKind::Peer, None, None)
        .expect("announce");
    service.flush_pending_save().expect("flush announce");
    std::fs::remove_file(&path).expect("remove directory file");
    std::fs::create_dir(&path).expect("replace target with directory");

    let error = service
        .set_trust_level("peer", TrustLevel::Trusted)
        .expect_err("reject unsafe save target");

    assert!(error.to_string().contains("regular file"));
    let entry = service.find("peer").expect("restored entry");
    assert_eq!(entry.trust_level, TrustLevel::Unknown);
    assert!(!entry.saved);
    assert!(!entry.trusted);
}

#[test]
fn oversized_live_display_name_is_rejected_without_state_mutation() {
    let path = temp_dir("display-limit").join("directory.json");
    let mut service = DirectoryService::new(path).expect("service");
    service
        .ingest_announce("peer", "Peer", DirectoryKind::Peer, None, None)
        .expect("valid announce");

    let error = service
        .ingest_announce(
            "oversized",
            "x".repeat(DIRECTORY_MAX_DISPLAY_NAME_BYTES + 1),
            DirectoryKind::Peer,
            None,
            None,
        )
        .expect_err("reject oversized display name");

    assert!(error.to_string().contains("display name"));
    assert!(service.find("oversized").is_none());
    assert_eq!(
        service.find("peer").expect("prior entry").display_name,
        "Peer"
    );
}

#[test]
fn excessive_persisted_entries_are_backed_up_and_defaulted() {
    let path = temp_dir("entry-limit").join("directory.json");
    let entries = (0..=DIRECTORY_MAX_ENTRIES)
        .map(|index| {
            let mut entry =
                DirectoryEntry::new(format!("peer-{index}"), "Peer", DirectoryKind::Peer);
            entry.saved = true;
            entry
        })
        .collect::<Vec<_>>();
    let raw = serde_json::to_vec(&serde_json::json!({
        "entries": entries,
        "announce_stream": []
    }))
    .expect("serialize excessive directory");
    assert!(raw.len() as u64 <= DIRECTORY_FILE_MAX_BYTES);
    std::fs::write(&path, &raw).expect("write excessive directory");

    let service = DirectoryService::new(path.clone()).expect("default excessive directory");

    assert!(service.list_entries().is_empty());
    assert_eq!(std::fs::read(&path).expect("read source"), raw);
    let backups = corrupt_backups(&path);
    assert_eq!(backups.len(), 1);
    assert_eq!(std::fs::read(&backups[0]).expect("read backup"), raw);
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
