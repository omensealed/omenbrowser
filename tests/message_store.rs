use std::collections::BTreeMap;
use std::path::PathBuf;

use omenbrowser_rs::messaging::{MessageStore, MessageSummary, TransportMethod};

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

    assert_eq!(ticket.expires, future_expiry);
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
