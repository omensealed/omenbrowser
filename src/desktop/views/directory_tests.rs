use super::*;
use crate::directory::{
    DeliveryFallbackPolicy, DirectoryEntry, PreferredDelivery, PropagationNodeCompatibility,
    PropagationNodeEvidence, PropagationNodeFreshness, PropagationNodePathState,
    PropagationNodeRecord, PropagationNodeRefreshEvidence, PropagationNodeSelection,
    PropagationNodeSyncEvidence, TrustLevel,
};

const FIXTURE_LXMF_PEER_HASH: &str = "00112233445566778899aabbccddeeff";

#[test]
fn directory_view_scope_filters_live_saved_and_trusted_entries() {
    let now_secs = crate::app::current_epoch_ms() as f64 / 1_000.0;
    let mut live = DirectoryEntry::new("live.node", "Live Node", DirectoryKind::Node);
    live.last_seen = now_secs;
    let mut stale_saved = DirectoryEntry::new("saved.node", "Saved Node", DirectoryKind::Node);
    stale_saved.last_seen = now_secs - 7.0 * 60.0 * 60.0;
    stale_saved.saved = true;
    let mut trusted = stale_saved.clone();
    trusted.destination_hash = "trusted.node".into();
    trusted.trusted = true;

    assert!(directory_entry_matches_view(
        &live,
        &DirectoryKind::Node,
        &DirectoryScope::Live,
        ""
    ));
    assert!(!directory_entry_matches_view(
        &stale_saved,
        &DirectoryKind::Node,
        &DirectoryScope::Live,
        ""
    ));
    assert!(directory_entry_matches_view(
        &stale_saved,
        &DirectoryKind::Node,
        &DirectoryScope::Saved,
        ""
    ));
    assert!(directory_entry_matches_view(
        &trusted,
        &DirectoryKind::Node,
        &DirectoryScope::Trusted,
        ""
    ));
}

#[test]
fn directory_view_filter_matches_python_style_directory_fields() {
    let now_secs = crate::app::current_epoch_ms() as f64 / 1_000.0;
    let mut entry = DirectoryEntry::new("abcdef1234567890", "Archive Node", DirectoryKind::Node);
    entry.last_seen = now_secs;
    entry.associated_hash = Some("peerfeed00112233".into());
    entry.saved = true;

    assert!(directory_entry_matches_view(
        &entry,
        &DirectoryKind::Node,
        &DirectoryScope::Live,
        "archive"
    ));
    assert!(directory_entry_matches_view(
        &entry,
        &DirectoryKind::Node,
        &DirectoryScope::Live,
        "peerfeed"
    ));
    assert!(directory_entry_matches_view(
        &entry,
        &DirectoryKind::Node,
        &DirectoryScope::Saved,
        "saved node"
    ));
    assert!(!directory_entry_matches_view(
        &entry,
        &DirectoryKind::Node,
        &DirectoryScope::Live,
        "lxmf-only"
    ));
}

#[test]
fn directory_row_actions_are_minimal_and_kind_specific() {
    assert_eq!(
        directory_row_action_labels(&DirectoryKind::Node),
        vec!["Select", "Browse Node"]
    );
    assert_eq!(
        directory_row_action_labels(&DirectoryKind::Peer),
        vec!["Select", "Message Peer"]
    );
    assert_eq!(
        directory_row_action_labels(&DirectoryKind::Propagation),
        vec!["Select", "Use Propagation"]
    );
    assert_eq!(
        directory_row_action_labels(&DirectoryKind::OmenChat),
        vec!["Select", "Open Chat"]
    );
    assert_eq!(
        directory_row_action_labels(&DirectoryKind::Unknown),
        vec!["Select"]
    );
}

#[test]
fn directory_selected_details_helpers_summarize_without_losing_full_hash() {
    assert_eq!(short_destination_hash("short.hash"), "short.hash");
    assert_eq!(
        short_destination_hash(FIXTURE_LXMF_PEER_HASH),
        "0011223344...ddeeff"
    );

    let peer = DirectoryEntry::new(FIXTURE_LXMF_PEER_HASH, "Peer", DirectoryKind::Peer);
    assert!(directory_selected_kind_note(&peer).contains("LXMF conversation"));
}

#[test]
fn directory_selected_details_primary_actions_are_kind_specific() {
    assert_eq!(
        directory_selected_primary_action_labels(&DirectoryKind::Node),
        vec!["Browse Node"]
    );
    assert_eq!(
        directory_selected_primary_action_labels(&DirectoryKind::Peer),
        vec!["Message Peer", "Inspect Peer"]
    );
    assert_eq!(
        directory_selected_primary_action_labels(&DirectoryKind::Propagation),
        vec![
            "Use Propagation",
            "Refresh Node",
            "Cancel Refresh",
            "Sync Now"
        ]
    );
    assert_eq!(
        directory_selected_primary_action_labels(&DirectoryKind::OmenChat),
        vec!["Open Chat"]
    );
    assert_eq!(
        directory_selected_primary_action_labels(&DirectoryKind::Unknown),
        vec!["Select"]
    );
}

#[test]
fn directory_selected_management_controls_are_kind_specific() {
    assert!(directory_kind_supports_identify_toggle(
        &DirectoryKind::Node
    ));
    assert!(!directory_kind_supports_identify_toggle(
        &DirectoryKind::OmenChat
    ));
    assert!(!directory_kind_supports_identify_toggle(
        &DirectoryKind::Propagation
    ));
    assert!(directory_kind_supports_delivery_preference(
        &DirectoryKind::Peer
    ));
    assert!(!directory_kind_supports_delivery_preference(
        &DirectoryKind::Node
    ));
    assert!(!directory_kind_supports_delivery_preference(
        &DirectoryKind::OmenChat
    ));
    assert!(!directory_kind_supports_delivery_preference(
        &DirectoryKind::Propagation
    ));
}

#[test]
fn directory_selected_state_lines_are_kind_specific() {
    let mut node = DirectoryEntry::new("node.hash", "Node", DirectoryKind::Node);
    node.identify_on_connect = true;
    let node_lines = directory_selected_state_lines(&node).join("\n");
    assert!(node_lines.contains("identify on connect: true"));
    assert!(!node_lines.contains("preferred LXMF delivery"));

    let mut peer = DirectoryEntry::new("peer.hash", "Peer", DirectoryKind::Peer);
    peer.preferred_delivery = Some(PreferredDelivery::Propagated);
    let peer_lines = directory_selected_state_lines(&peer).join("\n");
    assert!(peer_lines.contains("preferred LXMF delivery: propagated preferred"));
    assert!(peer_lines.contains("direct failure: ask before fallback"));
    assert!(peer_lines.contains("automatic direct stamp limit: default (8)"));
    assert!(peer_lines.contains("direct stamp confirmation: disabled"));
    assert!(peer_lines.contains("reply ticket default: default (off)"));
    assert!(!peer_lines.contains("identify on connect"));

    peer.preferred_delivery = Some(PreferredDelivery::DirectOnly);
    let peer_lines = directory_selected_state_lines(&peer).join("\n");
    assert!(peer_lines.contains("preferred LXMF delivery: direct only"));
    peer.delivery_fallback = DeliveryFallbackPolicy::Automatic;
    peer.max_automatic_direct_stamp_cost = Some(2);
    peer.ask_above_direct_stamp_cost = Some(1);
    peer.offer_reply_ticket = Some(true);
    let peer_lines = directory_selected_state_lines(&peer).join("\n");
    assert!(peer_lines.contains("direct failure: automatic safe fallback"));
    assert!(peer_lines.contains("automatic direct stamp limit: 2"));
    assert!(peer_lines.contains("direct stamp confirmation: ask above 1"));
    assert!(peer_lines.contains("reply ticket default: offer"));

    let mut omenchat = DirectoryEntry::new("chat.hash", "Chat", DirectoryKind::OmenChat);
    omenchat.identity_hash = Some("00112233445566778899aabbccddeeff".into());
    let omenchat_lines = directory_selected_state_lines(&omenchat).join("\n");
    assert!(omenchat_lines.contains("OMENchat server rank"));
    assert!(omenchat_lines
        .contains("server identity: 00112233445566778899aabbccddeeff (announce-verified)"));
    assert!(!omenchat_lines.contains("preferred LXMF delivery"));
    assert!(!omenchat_lines.contains("identify on connect"));
}

#[test]
fn propagation_node_state_lines_keep_unknown_and_negative_evidence_distinct() {
    let record = PropagationNodeRecord {
        destination_hash: FIXTURE_LXMF_PEER_HASH.into(),
        identity_hash: None,
        display_name: FIXTURE_LXMF_PEER_HASH.into(),
        display_name_authenticated: false,
        selected: true,
        selection: PropagationNodeSelection::Pinned,
        saved: true,
        trusted: false,
        trust_level: TrustLevel::Unknown,
        last_seen: 0.0,
        announce_age_seconds: None,
        freshness: PropagationNodeFreshness::Unknown,
        path_state: PropagationNodePathState::NotKnown,
        advertised_stamp_cost: None,
        compatibility: PropagationNodeCompatibility::Unknown,
        evidence: PropagationNodeEvidence::UnverifiedIdentity,
        refresh: Some(PropagationNodeRefreshEvidence::NoPath),
        refresh_observed_epoch_ms: Some(10),
        refresh_cooldown_remaining_seconds: Some(3),
        sync: Some(PropagationNodeSyncEvidence::Failed),
        last_sync_epoch_ms: Some(20),
        last_successful_sync_epoch_ms: Some(5),
        last_sync_error: Some("path unavailable".into()),
    };
    let lines = propagation_node_state_lines(&record).join("\n");
    assert!(lines
        .contains("selection=pinned | freshness=unknown | announce age=unknown | path=not-known"));
    assert!(lines.contains(
        "compatibility=unknown | evidence=unverified identity | advertised stamp cost=unknown"
    ));
    assert!(lines.contains("identity: unknown | display name authenticated=false"));
    assert!(lines.contains("refresh=no path | observed=10 | cooldown snapshot=3s"));
    assert!(lines.contains("sync=failed | last=20 | last successful=5"));
    assert!(lines.contains("last sync error: path unavailable"));
}

#[test]
fn directory_view_filter_matches_verified_identity_hash() {
    let now_secs = crate::app::current_epoch_ms() as f64 / 1_000.0;
    let mut entry = DirectoryEntry::new("chat.hash", "Chat", DirectoryKind::OmenChat);
    entry.last_seen = now_secs;
    entry.identity_hash = Some("00112233445566778899aabbccddeeff".into());

    assert!(directory_entry_matches_view(
        &entry,
        &DirectoryKind::OmenChat,
        &DirectoryScope::Live,
        "aabbccdd"
    ));
}
