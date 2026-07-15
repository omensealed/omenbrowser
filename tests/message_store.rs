use std::collections::BTreeMap;
use std::path::PathBuf;

use omenbrowser_rs::messaging::{
    MessageStore, MessageSummary, TransportMethod, MESSAGE_STORE_CORRUPT_BACKUP_MAX_FILES,
    MESSAGE_STORE_MAX_SCAN_ENTRIES, MESSAGE_STORE_MAX_THREADS, MESSAGE_STORE_THREAD_MAX_BYTES,
    MESSAGE_STORE_THREAD_MAX_MESSAGES,
};

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "omenbrowser-rs-message-store-integration-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

fn message(peer: &str, timestamp: f64, incoming: bool) -> MessageSummary {
    MessageSummary {
        peer_hash: peer.into(),
        peer_label: format!("{peer}-label"),
        title: "title".into(),
        content: "body".into(),
        timestamp,
        transport_method: TransportMethod::Direct,
        delivered: incoming,
        failed: false,
        incoming,
        unread: incoming,
        message_id: None,
        fields: BTreeMap::new(),
        attachments: Vec::new(),
    }
}

#[test]
fn message_store_returns_latest_valid_lxmf_reply_ticket() {
    let store = MessageStore::new(temp_dir("reply-ticket")).expect("store");
    let future_expiry = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("time")
        .as_secs_f64()
        + 3600.0;
    let mut expired = message("peer-a", 1.0, true);
    expired.message_id = Some("expired".into());
    expired.fields.insert(
        "native_lxmf_reply_ticket".into(),
        "000102030405060708090a0b0c0d0e0f".into(),
    );
    expired
        .fields
        .insert("native_lxmf_reply_ticket_expires".into(), "90.0".into());
    let mut invalid = message("peer-a", 2.0, true);
    invalid.message_id = Some("invalid".into());
    invalid
        .fields
        .insert("native_lxmf_reply_ticket".into(), "not-hex".into());
    invalid
        .fields
        .insert("native_lxmf_reply_ticket_expires".into(), "200.0".into());
    let mut valid = message("peer-a", 3.0, true);
    valid.message_id = Some("valid".into());
    valid.fields.insert(
        "native_lxmf_reply_ticket".into(),
        "101112131415161718191a1b1c1d1e1f".into(),
    );
    valid.fields.insert(
        "native_lxmf_reply_ticket_expires".into(),
        future_expiry.to_string(),
    );
    store.append(expired).expect("append expired");
    store.append(invalid).expect("append invalid");
    store.append(valid).expect("append valid");

    let ticket = store
        .latest_valid_lxmf_reply_ticket("peer-a", 100.0)
        .expect("ticket lookup")
        .expect("valid ticket");

    assert!(
        (ticket.expires - future_expiry).abs() < 0.000_001,
        "ticket expiry should round-trip within f64 JSON precision: left={} right={}",
        ticket.expires,
        future_expiry
    );
    assert_eq!(ticket.ticket, (0x10u8..=0x1f).collect::<Vec<_>>());

    let thread = store.get_thread("peer-a").expect("thread");
    assert_eq!(
        thread
            .lxmf_reply_ticket
            .as_ref()
            .map(|ticket| ticket.ticket.clone()),
        Some((0x10u8..=0x1f).collect::<Vec<_>>())
    );
}

#[test]
fn message_store_reply_ticket_lookup_falls_back_for_legacy_threads() {
    let root = temp_dir("legacy-reply-ticket");
    let peer_path = root.join("peer-a.json");
    std::fs::write(
        &peer_path,
        serde_json::json!({
            "peer_hash": "peer-a",
            "peer_label": "Peer A",
            "messages": [
                {
                    "peer_hash": "peer-a",
                    "peer_label": "Peer A",
                    "title": "title",
                    "content": "body",
                    "timestamp": 3.0,
                    "transport_method": "direct",
                    "delivered": true,
                    "failed": false,
                    "incoming": true,
                    "unread": false,
                    "message_id": "valid",
                    "fields": {
                        "native_lxmf_reply_ticket": "202122232425262728292a2b2c2d2e2f",
                        "native_lxmf_reply_ticket_expires": "300.0"
                    },
                    "attachments": []
                }
            ],
            "unread_count": 0
        })
        .to_string(),
    )
    .expect("write legacy thread");
    let store = MessageStore::new(root).expect("store");

    let ticket = store
        .latest_valid_lxmf_reply_ticket("peer-a", 100.0)
        .expect("ticket lookup")
        .expect("legacy ticket");

    assert_eq!(ticket.expires, 300.0);
    assert_eq!(ticket.ticket, (0x20u8..=0x2f).collect::<Vec<_>>());
}

#[test]
fn message_store_appends_lists_marks_read_and_updates_delivery() {
    let store = MessageStore::new(temp_dir("basic")).expect("store");
    let outgoing = store.append(message("peer-a", 1.0, false)).expect("append");
    store.append(message("peer-b", 2.0, true)).expect("append");

    let threads = store.list_threads().expect("threads");
    assert_eq!(threads[0].peer_hash, "peer-b");
    assert_eq!(threads[0].unread_count, 1);

    store.mark_read("peer-b").expect("mark read");
    assert_eq!(store.get_thread("peer-b").expect("thread").unread_count, 0);

    store
        .update_delivery(
            "peer-a",
            outgoing.message_id.as_deref().expect("message id"),
            true,
            false,
        )
        .expect("update delivery");
    assert!(store.get_thread("peer-a").expect("thread").messages[0].delivered);
}

#[test]
fn message_store_deduplicates_by_message_id() {
    let store = MessageStore::new(temp_dir("dedupe")).expect("store");
    let mut first = message("peer-a", 1.0, true);
    first.message_id = Some("same-id".into());
    let mut duplicate = first.clone();
    duplicate.content = "duplicate body".into();

    let stored = store.append(first).expect("append first");
    let duplicate_result = store.append(duplicate).expect("append duplicate");
    let thread = store.get_thread("peer-a").expect("thread");

    assert_eq!(duplicate_result.content, stored.content);
    assert_eq!(thread.messages.len(), 1);
    assert_eq!(thread.unread_count, 1);
}

#[test]
fn message_store_imports_missing_python_threads_without_overwriting() {
    let source = temp_dir("python-source");
    let target = temp_dir("python-target");
    let source_store = MessageStore::new(source.clone()).expect("source store");
    let target_store = MessageStore::new(target).expect("target store");
    source_store
        .append(message("python-peer", 1.0, true))
        .expect("source append");

    let imported = target_store
        .import_missing_threads_from(&source)
        .expect("import missing");
    let second_import = target_store
        .import_missing_threads_from(&source)
        .expect("second import");

    assert_eq!(imported, 1);
    assert_eq!(second_import, 0);
    let thread = target_store
        .get_thread("python-peer")
        .expect("imported thread");
    assert_eq!(thread.messages.len(), 1);
    assert_eq!(thread.peer_label, "python-peer-label");
}

#[test]
fn corrupted_thread_is_backed_up_and_defaulted() {
    let root = temp_dir("corrupt");
    std::fs::write(root.join("peer.json"), b"{bad").expect("write corrupt");
    let store = MessageStore::new(root.clone()).expect("store");

    let thread = store.get_thread("peer").expect("thread");
    let backups = std::fs::read_dir(root)
        .expect("read dir")
        .filter_map(Result::ok)
        .filter(|entry| entry.file_name().to_string_lossy().contains(".corrupt."))
        .count();

    assert_eq!(thread.messages.len(), 0);
    assert_eq!(backups, 1);
}

#[test]
fn thread_file_admission_accepts_exact_limit_and_rejects_next_byte() {
    let root = temp_dir("thread-byte-limit");
    let store = MessageStore::new(root.clone()).expect("store");
    let raw = serde_json::json!({
        "peer_hash": "exact",
        "peer_label": "Exact",
        "messages": [],
        "unread_count": 0
    })
    .to_string();
    let mut exact = raw.into_bytes();
    exact.resize(MESSAGE_STORE_THREAD_MAX_BYTES as usize, b' ');
    std::fs::write(root.join("exact.json"), exact).expect("exact thread");
    store.get_thread("exact").expect("exact thread admitted");

    let oversized = root.join("oversized.json");
    let file = std::fs::File::create(&oversized).expect("oversized thread");
    file.set_len(MESSAGE_STORE_THREAD_MAX_BYTES + 1)
        .expect("extend oversized thread");
    drop(file);
    let error = store
        .get_thread("oversized")
        .expect_err("next byte must be rejected");
    assert!(error.to_string().contains("byte limit"));
    assert_eq!(
        std::fs::metadata(oversized)
            .expect("oversized metadata")
            .len(),
        MESSAGE_STORE_THREAD_MAX_BYTES + 1
    );
}

#[test]
fn thread_message_count_is_rejected_and_peer_derived_paths_are_contained() {
    let root = temp_dir("thread-semantic-limit");
    let store = MessageStore::new(root.clone()).expect("store");
    let thread = omenbrowser_rs::messaging::ConversationThread {
        peer_hash: "peer-limit".into(),
        peer_label: "Peer".into(),
        messages: vec![message("peer-limit", 1.0, false); MESSAGE_STORE_THREAD_MAX_MESSAGES + 1],
        unread_count: 0,
        lxmf_reply_ticket: None,
    };
    std::fs::write(
        root.join("peer-limit.json"),
        serde_json::to_vec(&thread).expect("thread fixture"),
    )
    .expect("thread fixture");
    let error = store
        .get_thread("peer-limit")
        .expect_err("message count must be rejected");
    assert!(error.to_string().contains("message limit"));

    let outside = root.parent().expect("fixture parent").join("escaped.json");
    let _ = std::fs::remove_file(&outside);
    store
        .append(message("../escaped", 1.0, false))
        .expect("filesystem-unsafe peer must use a contained mapped filename");
    assert!(!outside.exists());
    assert_eq!(
        store
            .get_thread("../escaped")
            .expect("mapped peer thread")
            .messages
            .len(),
        1
    );
}

#[test]
fn thread_discovery_is_item_scan_and_total_byte_bounded() {
    let root = temp_dir("thread-discovery-limit");
    let store = MessageStore::new(root.clone()).expect("store");
    let raw = serde_json::json!({
        "peer_hash": "peer",
        "peer_label": "Peer",
        "messages": [],
        "unread_count": 0
    })
    .to_string();
    for index in 0..MESSAGE_STORE_MAX_THREADS {
        std::fs::write(root.join(format!("peer-{index:04}.json")), &raw).expect("thread file");
    }
    assert_eq!(
        store.list_threads().expect("exact thread limit").len(),
        MESSAGE_STORE_MAX_THREADS
    );
    let error = store
        .append(message("new-peer", 1.0, false))
        .expect_err("append must not exceed thread capacity");
    assert!(error.to_string().contains("cannot exceed"));
    std::fs::write(root.join("peer-over.json"), &raw).expect("extra thread");
    let error = store
        .list_threads()
        .expect_err("next thread must be rejected");
    assert!(error.to_string().contains("thread limit"));

    let scan_root = temp_dir("thread-scan-limit");
    let scan_store = MessageStore::new(scan_root.clone()).expect("scan store");
    for index in 0..=MESSAGE_STORE_MAX_SCAN_ENTRIES {
        std::fs::create_dir(scan_root.join(format!("ignored-{index:04}"))).expect("ignored entry");
    }
    let error = scan_store
        .list_threads()
        .expect_err("scan saturation must be rejected");
    assert!(error.to_string().contains("entry scan limit"));
    std::fs::remove_dir_all(scan_root).expect("remove scan fixture");

    let byte_root = temp_dir("thread-total-byte-limit");
    let byte_store = MessageStore::new(byte_root.clone()).expect("byte store");
    for index in 0..9 {
        let file = std::fs::File::create(byte_root.join(format!("peer-{index}.json")))
            .expect("sparse thread");
        file.set_len(MESSAGE_STORE_THREAD_MAX_BYTES)
            .expect("extend sparse thread");
    }
    let error = byte_store
        .list_threads()
        .expect_err("aggregate byte saturation must be rejected");
    assert!(error.to_string().contains("retained byte limit"));
    std::fs::remove_dir_all(byte_root).expect("remove byte fixture");
}

#[test]
fn corruption_backups_are_bounded_without_pruning_legacy_material() {
    let root = temp_dir("corrupt-retention");
    let store = MessageStore::new(root.clone()).expect("store");
    let thread = root.join("peer.json");
    std::fs::write(&thread, b"{bad").expect("corrupt thread");
    let legacy = root.join("peer.json.corrupt.legacy.bak");
    std::fs::write(&legacy, b"legacy backup").expect("legacy backup");

    for _ in 0..MESSAGE_STORE_CORRUPT_BACKUP_MAX_FILES + 3 {
        store.get_thread("peer").expect("corrupt recovery");
    }
    let backups = std::fs::read_dir(&root)
        .expect("backup entries")
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .file_name()
                .to_str()
                .is_some_and(|name| name.starts_with("omen-message.corrupt."))
        })
        .count();
    assert_eq!(backups, MESSAGE_STORE_CORRUPT_BACKUP_MAX_FILES);
    assert_eq!(
        std::fs::read(legacy).expect("legacy backup"),
        b"legacy backup"
    );
}

#[cfg(unix)]
#[test]
fn thread_publication_is_private_and_thread_symlinks_are_not_followed() {
    use std::os::unix::fs::{symlink, PermissionsExt};

    let root = temp_dir("thread-publication-safety");
    let store = MessageStore::new(root.clone()).expect("store");
    store
        .append(message("private-peer", 1.0, false))
        .expect("append private thread");
    let path = root.join("private-peer.json");
    assert_eq!(
        std::fs::metadata(&path)
            .expect("thread metadata")
            .permissions()
            .mode()
            & 0o777,
        0o600
    );

    let outside = root.join("outside");
    std::fs::write(&outside, b"outside sentinel").expect("outside file");
    symlink(&outside, root.join("linked.json")).expect("thread symlink");
    let error = store
        .get_thread("linked")
        .expect_err("thread symlink must be rejected");
    assert!(error.to_string().contains("symbolic link"));
    assert_eq!(
        std::fs::read(outside).expect("outside sentinel"),
        b"outside sentinel"
    );
}

#[cfg(unix)]
#[test]
fn existing_single_component_nonportable_filename_remains_update_compatible() {
    let root = temp_dir("legacy-nonportable-filename");
    let legacy_path = root.join("legacy:peer.json");
    let legacy_thread = omenbrowser_rs::messaging::ConversationThread {
        peer_hash: "legacy:peer".into(),
        peer_label: "Legacy Peer".into(),
        messages: vec![message("legacy:peer", 1.0, false)],
        unread_count: 0,
        lxmf_reply_ticket: None,
    };
    std::fs::write(
        &legacy_path,
        serde_json::to_vec(&legacy_thread).expect("legacy thread"),
    )
    .expect("legacy filename");
    let store = MessageStore::new(root.clone()).expect("store");

    assert_eq!(
        store
            .get_thread("legacy:peer")
            .expect("legacy load")
            .messages
            .len(),
        1
    );
    store
        .append(message("legacy:peer", 2.0, false))
        .expect("legacy update");

    assert_eq!(
        store
            .get_thread("legacy:peer")
            .expect("updated legacy thread")
            .messages
            .len(),
        2
    );
    assert!(legacy_path.exists());
    assert_eq!(
        std::fs::read_dir(root)
            .expect("message entries")
            .filter_map(Result::ok)
            .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "json"))
            .count(),
        1
    );
}

#[test]
fn direct_lxmf_reconciliation_marks_only_stale_waiting_messages_unconfirmed() {
    let store = MessageStore::new(temp_dir("native-lxmf-stale")).expect("store");
    let mut stale = message("peer-a", 10.0, false);
    stale.message_id = Some("packet-a".into());
    stale
        .fields
        .insert("native_lxmf_state".into(), "submitted_to_rns_net".into());
    stale.fields.insert(
        "native_lxmf_proof_state".into(),
        "waiting_for_packet_proof".into(),
    );
    stale
        .fields
        .insert("native_lxmf_submitted_at".into(), "10.0".into());
    let mut fresh = message("peer-a", 95.0, false);
    fresh.message_id = Some("packet-b".into());
    fresh
        .fields
        .insert("native_lxmf_state".into(), "submitted_to_rns_net".into());
    fresh.fields.insert(
        "native_lxmf_proof_state".into(),
        "waiting_for_packet_proof".into(),
    );
    fresh
        .fields
        .insert("native_lxmf_submitted_at".into(), "95.0".into());
    store.append(stale).expect("append stale");
    store.append(fresh).expect("append fresh");

    let changed = store
        .reconcile_stale_native_lxmf_direct(100.0, 45.0)
        .expect("reconcile");
    let thread = store.get_thread("peer-a").expect("thread");

    assert_eq!(changed.len(), 1);
    assert_eq!(changed[0].message_id.as_deref(), Some("packet-a"));
    assert_eq!(
        thread.messages[0]
            .fields
            .get("native_lxmf_state")
            .map(String::as_str),
        Some("submitted_unconfirmed")
    );
    assert_eq!(
        thread.messages[0]
            .fields
            .get("native_lxmf_delivery_evidence_kind")
            .map(String::as_str),
        Some("no_receipt_observed")
    );
    assert_eq!(
        thread.messages[1]
            .fields
            .get("native_lxmf_state")
            .map(String::as_str),
        Some("submitted_to_rns_net")
    );
}

#[test]
fn direct_lxmf_reconciliation_surfaces_propagation_retry_when_fallback_exists() {
    let store = MessageStore::new(temp_dir("native-lxmf-fallback")).expect("store");
    let mut stale = message("peer-a", 10.0, false);
    stale.message_id = Some("packet-a".into());
    stale
        .fields
        .insert("native_lxmf_state".into(), "submitted_to_rns_net".into());
    stale.fields.insert(
        "native_lxmf_proof_state".into(),
        "waiting_for_packet_proof".into(),
    );
    stale
        .fields
        .insert("native_lxmf_submitted_at".into(), "10.0".into());
    stale.fields.insert(
        "native_lxmf_propagation_fallback_available".into(),
        "true".into(),
    );
    stale.fields.insert(
        "native_lxmf_propagation_node".into(),
        "fedcba98765432100123456789abcdef".into(),
    );
    store.append(stale).expect("append stale");

    let changed = store
        .reconcile_stale_native_lxmf_direct(100.0, 45.0)
        .expect("reconcile");
    let thread = store.get_thread("peer-a").expect("thread");

    assert_eq!(changed.len(), 1);
    assert_eq!(
        thread.messages[0]
            .fields
            .get("native_lxmf_state")
            .map(String::as_str),
        Some("propagation_retry_ready")
    );
    assert_eq!(
        thread.messages[0]
            .fields
            .get("native_lxmf_propagation_node")
            .map(String::as_str),
        Some("fedcba98765432100123456789abcdef")
    );
}

#[test]
fn propagated_lxmf_reconciliation_marks_stale_router_deferred_rows_failed() {
    let store = MessageStore::new(temp_dir("native-lxmf-propagated-stale")).expect("store");
    let mut stale = message("peer-a", 10.0, false);
    stale.transport_method = TransportMethod::Propagated;
    stale.message_id = Some("prop-a".into());
    stale
        .fields
        .insert("native_lxmf_state".into(), "queued_for_propagation".into());
    stale.fields.insert(
        "native_lxmf_propagation_transfer_state".into(),
        "router_deferred".into(),
    );
    stale
        .fields
        .insert("native_lxmf_submitted_at".into(), "10.0".into());
    let mut fresh = message("peer-a", 90.0, false);
    fresh.transport_method = TransportMethod::Propagated;
    fresh.message_id = Some("prop-b".into());
    fresh
        .fields
        .insert("native_lxmf_state".into(), "queued_for_propagation".into());
    fresh.fields.insert(
        "native_lxmf_propagation_transfer_state".into(),
        "router_deferred".into(),
    );
    fresh
        .fields
        .insert("native_lxmf_submitted_at".into(), "90.0".into());
    let mut progress = message("peer-a", 20.0, false);
    progress.transport_method = TransportMethod::Propagated;
    progress.message_id = Some("prop-c".into());
    progress
        .fields
        .insert("native_lxmf_state".into(), "queued_for_propagation".into());
    progress.fields.insert(
        "native_lxmf_propagation_transfer_state".into(),
        "resource_progress".into(),
    );
    progress
        .fields
        .insert("native_lxmf_submitted_at".into(), "20.0".into());
    let mut advertised = message("peer-a", 30.0, false);
    advertised.transport_method = TransportMethod::Propagated;
    advertised.message_id = Some("prop-d".into());
    advertised
        .fields
        .insert("native_lxmf_state".into(), "queued_for_propagation".into());
    advertised.fields.insert(
        "native_lxmf_propagation_transfer_state".into(),
        "resource_advertised".into(),
    );
    advertised
        .fields
        .insert("native_lxmf_submitted_at".into(), "30.0".into());
    store.append(stale).expect("append stale");
    store.append(fresh).expect("append fresh");
    store.append(progress).expect("append progress");
    store.append(advertised).expect("append advertised");

    let changed = store
        .reconcile_stale_native_lxmf_propagated(100.0, 45.0)
        .expect("reconcile");
    let thread = store.get_thread("peer-a").expect("thread");

    assert_eq!(changed.len(), 2);
    assert!(thread.messages[0].failed);
    assert!(thread.messages[1].failed);
    assert!(!thread.messages[2].failed);
    assert!(!thread.messages[3].failed);
    assert_eq!(
        thread.messages[0]
            .fields
            .get("native_lxmf_propagation_transfer_state")
            .map(String::as_str),
        Some("router_timeout")
    );
    assert_eq!(
        thread.messages[1]
            .fields
            .get("native_lxmf_propagation_transfer_state")
            .map(String::as_str),
        Some("resource_timeout")
    );
    assert_eq!(
        thread.messages[2]
            .fields
            .get("native_lxmf_propagation_transfer_state")
            .map(String::as_str),
        Some("resource_advertised")
    );
}
