use crate::app::DirectoryScope;
use crate::directory::{
    DirectoryEntry, DirectoryKind, PropagationNodeCompatibility, PropagationNodeFreshness,
    PropagationNodePathState, PropagationNodeRecord,
};

pub(in crate::desktop) fn directory_kind_title(kind: &DirectoryKind) -> &'static str {
    match kind {
        DirectoryKind::Node => "Nodes",
        DirectoryKind::Peer => "Peers",
        DirectoryKind::Propagation => "Propagation Nodes",
        DirectoryKind::OmenChat => "OMENchat Servers",
        DirectoryKind::Unknown => "Unknown Announces",
    }
}

pub(in crate::desktop) fn directory_empty_text(kind: &DirectoryKind) -> &'static str {
    match kind {
        DirectoryKind::Node => "No recent NomadNet node announces yet.",
        DirectoryKind::Peer => "No recent LXMF peer announces yet.",
        DirectoryKind::Propagation => "No recent LXMF propagation node announces yet.",
        DirectoryKind::OmenChat => "No recent OMENchat server announces yet.",
        DirectoryKind::Unknown => "No unknown announces.",
    }
}

pub(in crate::desktop) fn directory_empty_text_for_scope(
    default: &str,
    scope: &DirectoryScope,
    filter: &str,
) -> String {
    if !filter.trim().is_empty() {
        return format!(
            "No directory entries match \"{}\" in this tab.",
            filter.trim()
        );
    }
    match scope {
        DirectoryScope::Live => default.to_string(),
        DirectoryScope::Saved => "No saved entries in this directory tab.".into(),
        DirectoryScope::Trusted => "No trusted entries in this directory tab.".into(),
    }
}

pub(in crate::desktop) fn short_destination_hash(hash: &str) -> String {
    let trimmed = hash.trim();
    let char_count = trimmed.chars().count();
    if char_count <= 18 {
        return trimmed.to_string();
    }
    let head = trimmed.chars().take(10).collect::<String>();
    let tail = trimmed
        .chars()
        .rev()
        .take(6)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<String>();
    format!("{head}...{tail}")
}

pub(in crate::desktop) fn directory_selected_kind_note(entry: &DirectoryEntry) -> String {
    match entry.kind {
        DirectoryKind::Node => "node: Browse opens /page/index.mu in a browser tab".into(),
        DirectoryKind::Peer => {
            "peer: Message opens an LXMF conversation; Inspect checks identity/path readiness"
                .into()
        }
        DirectoryKind::Propagation => {
            "propagation: Use Propagation selects this node for propagated LXMF sync/send".into()
        }
        DirectoryKind::OmenChat => {
            "omenchat: Open Chat connects to this OMENchat server with a Reticulum Link".into()
        }
        DirectoryKind::Unknown => {
            "unknown: announce is preserved but not classified as node, peer, or propagation".into()
        }
    }
}

pub(in crate::desktop) fn directory_selected_primary_action_labels(
    kind: &DirectoryKind,
) -> Vec<&'static str> {
    match kind {
        DirectoryKind::Node => vec!["Browse Node"],
        DirectoryKind::Peer => vec!["Message Peer", "Inspect Peer"],
        DirectoryKind::Propagation => {
            vec![
                "Use Propagation",
                "Refresh Node",
                "Cancel Refresh",
                "Sync Now",
            ]
        }
        DirectoryKind::OmenChat => vec!["Open Chat"],
        DirectoryKind::Unknown => vec!["Select"],
    }
}

pub(in crate::desktop) fn directory_selected_state_lines(entry: &DirectoryEntry) -> Vec<String> {
    let sort_rank = entry
        .sort_rank
        .map(|rank| rank.to_string())
        .unwrap_or_else(|| "default".into());
    let mut lines = vec![format!(
        "trust: {:?} | trusted={} | saved={}",
        entry.trust_level, entry.trusted, entry.saved
    )];
    match entry.kind {
        DirectoryKind::Node => {
            lines.push(format!(
                "identify on connect: {} | sort rank: {}",
                entry.identify_on_connect, sort_rank
            ));
            lines.push(format!("hosts NomadNet pages: {}", entry.hosts_node));
        }
        DirectoryKind::Peer => {
            let preferred_delivery = entry
                .preferred_delivery
                .map(|delivery| delivery.label().to_string())
                .unwrap_or_else(|| "default".into());
            lines.push(format!("preferred LXMF delivery: {preferred_delivery}"));
            lines.push(format!(
                "direct failure: {}",
                entry.delivery_fallback.label()
            ));
            lines.push(format!(
                "automatic direct stamp limit: {}",
                entry
                    .max_automatic_direct_stamp_cost
                    .map(|cost| cost.to_string())
                    .unwrap_or_else(|| format!(
                        "default ({})",
                        crate::directory::DEFAULT_AUTOMATIC_DIRECT_STAMP_COST
                    ))
            ));
            lines.push(format!(
                "direct stamp confirmation: {}",
                entry
                    .ask_above_direct_stamp_cost
                    .map(|cost| format!("ask above {cost}"))
                    .unwrap_or_else(|| "disabled".into())
            ));
            lines.push(format!(
                "reply ticket default: {}",
                match entry.offer_reply_ticket {
                    Some(true) => "offer",
                    Some(false) => "do not offer",
                    None => "default (off)",
                }
            ));
        }
        DirectoryKind::Propagation => {
            lines.push(format!("propagation candidate rank: {sort_rank}"));
        }
        DirectoryKind::OmenChat => {
            lines.push(format!("OMENchat server rank: {sort_rank}"));
            lines.push(format!(
                "server identity: {}",
                entry
                    .identity_hash
                    .as_deref()
                    .map(|hash| format!("{hash} (announce-verified)"))
                    .unwrap_or_else(|| "unavailable; use a fresh live announce".into())
            ));
        }
        DirectoryKind::Unknown => {
            lines.push(format!("announce sort rank: {sort_rank}"));
        }
    }
    lines
}

pub(in crate::desktop) fn propagation_node_state_lines(
    node: &PropagationNodeRecord,
) -> Vec<String> {
    let identity = node.identity_hash.as_deref().unwrap_or("unknown");
    let stamp_cost = node
        .advertised_stamp_cost
        .map(|cost| cost.to_string())
        .unwrap_or_else(|| "unknown".into());
    vec![
        format!(
            "selected={} | freshness={} | path={}",
            node.selected,
            propagation_freshness_label(node.freshness),
            propagation_path_label(node.path_state)
        ),
        format!(
            "compatibility={} | advertised stamp cost={stamp_cost}",
            propagation_compatibility_label(node.compatibility)
        ),
        format!(
            "identity: {identity} | display name authenticated={}",
            node.display_name_authenticated
        ),
    ]
}

fn propagation_freshness_label(value: PropagationNodeFreshness) -> &'static str {
    match value {
        PropagationNodeFreshness::Fresh => "fresh",
        PropagationNodeFreshness::Stale => "stale",
        PropagationNodeFreshness::Unknown => "unknown",
    }
}

fn propagation_path_label(value: PropagationNodePathState) -> &'static str {
    match value {
        PropagationNodePathState::Known => "known",
        PropagationNodePathState::NotKnown => "not-known",
        PropagationNodePathState::Unknown => "unknown",
    }
}

fn propagation_compatibility_label(value: PropagationNodeCompatibility) -> &'static str {
    match value {
        PropagationNodeCompatibility::Compatible => "compatible",
        PropagationNodeCompatibility::Unknown => "unknown",
    }
}

pub(in crate::desktop) fn directory_kind_supports_identify_toggle(kind: &DirectoryKind) -> bool {
    matches!(kind, DirectoryKind::Node)
}

pub(in crate::desktop) fn directory_kind_supports_delivery_preference(
    kind: &DirectoryKind,
) -> bool {
    matches!(kind, DirectoryKind::Peer)
}

pub(in crate::desktop) fn directory_entry_matches_view(
    entry: &DirectoryEntry,
    kind: &DirectoryKind,
    scope: &DirectoryScope,
    filter: &str,
) -> bool {
    if &entry.kind != kind {
        return false;
    }
    let scope_matches = match scope {
        DirectoryScope::Live => directory_entry_is_recent(entry),
        DirectoryScope::Saved => entry.saved,
        DirectoryScope::Trusted => entry.trusted,
    };
    scope_matches && directory_entry_matches_filter(entry, filter)
}

pub(in crate::desktop) fn directory_row_action_labels(kind: &DirectoryKind) -> Vec<&'static str> {
    match kind {
        DirectoryKind::Node => vec!["Select", "Browse Node"],
        DirectoryKind::Peer => vec!["Select", "Message Peer"],
        DirectoryKind::Propagation => vec!["Select", "Use Propagation"],
        DirectoryKind::OmenChat => vec!["Select", "Open Chat"],
        DirectoryKind::Unknown => vec!["Select"],
    }
}

fn directory_entry_matches_filter(entry: &DirectoryEntry, filter: &str) -> bool {
    let filter = filter.trim().to_lowercase();
    if filter.is_empty() {
        return true;
    }

    let mut haystack = format!(
        "{} {} {:?} {:?} {:?}",
        entry.display_name,
        entry.destination_hash,
        entry.kind,
        entry.trust_level,
        entry.preferred_delivery
    )
    .to_lowercase();
    if let Some(hash) = &entry.associated_hash {
        haystack.push(' ');
        haystack.push_str(&hash.to_lowercase());
    }
    if let Some(hash) = &entry.identity_hash {
        haystack.push(' ');
        haystack.push_str(&hash.to_lowercase());
    }
    if let Some(hash) = &entry.node_associated_hash {
        haystack.push(' ');
        haystack.push_str(&hash.to_lowercase());
    }
    if entry.saved {
        haystack.push_str(" saved");
    }
    if entry.trusted {
        haystack.push_str(" trusted");
    }
    if entry.identify_on_connect {
        haystack.push_str(" identify");
    }

    filter
        .split_whitespace()
        .all(|token| haystack.contains(token))
}

fn directory_entry_is_recent(entry: &DirectoryEntry) -> bool {
    let now_secs = crate::app::current_epoch_ms() as f64 / 1_000.0;
    entry.last_seen > 0.0 && now_secs - entry.last_seen <= 6.0 * 60.0 * 60.0
}

#[cfg(test)]
#[path = "directory_tests.rs"]
mod tests;
