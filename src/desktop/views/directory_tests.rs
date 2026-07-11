use super::*;
use crate::directory::{DirectoryEntry, PreferredDelivery};

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
        vec!["Use Propagation"]
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
    assert!(peer_lines.contains("preferred LXMF delivery: Propagated"));
    assert!(!peer_lines.contains("identify on connect"));

    let omenchat = DirectoryEntry::new("chat.hash", "Chat", DirectoryKind::OmenChat);
    let omenchat_lines = directory_selected_state_lines(&omenchat).join("\n");
    assert!(omenchat_lines.contains("OMENchat server rank"));
    assert!(!omenchat_lines.contains("preferred LXMF delivery"));
    assert!(!omenchat_lines.contains("identify on connect"));
}
