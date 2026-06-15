use std::collections::{BTreeMap, VecDeque};

use crate::directory::DirectoryKind;
use crate::runtime::network::{AnnouncePayload, DirectoryCandidate, NetworkSnapshot};

const MAX_RECENT_ANNOUNCES: usize = 256;

#[derive(Clone, Debug, Default)]
pub struct NativeAnnounceState {
    counts: BTreeMap<String, u32>,
    recent: VecDeque<AnnouncePayload>,
    ratchet_announces: u32,
}

impl NativeAnnounceState {
    pub fn ingest(&mut self, payload: AnnouncePayload) {
        *self
            .counts
            .entry(kind_key(&payload.kind).into())
            .or_default() += 1;
        if payload.has_ratchet {
            self.ratchet_announces = self.ratchet_announces.saturating_add(1);
        }
        self.recent
            .retain(|item| item.destination_hash != payload.destination_hash);
        self.recent.push_back(payload);
        while self.recent.len() > MAX_RECENT_ANNOUNCES {
            self.recent.pop_front();
        }
    }

    pub fn snapshot(&self) -> NetworkSnapshot {
        NetworkSnapshot {
            announce_counts: self.counts.clone(),
            pending_announces: self.recent.len() as u32,
            known_destinations: self.recent.len() as u32,
            ratchet_announces: self.ratchet_announces,
            path_table_count: 0,
            request_failures: 0,
            active_propagation_node: None,
            connected_to_shared_instance: false,
            is_shared_instance: false,
        }
    }

    pub fn candidates(&self, limit: Option<usize>) -> Vec<DirectoryCandidate> {
        let mut candidates = self
            .recent
            .iter()
            .rev()
            .map(|announce| DirectoryCandidate {
                destination_hash: announce.destination_hash.clone(),
                display_name: announce.display_name.clone(),
                kind: announce.kind.clone(),
                associated_hash: announce.associated_hash.clone(),
                node_associated_hash: announce.node_associated_hash.clone(),
                has_ratchet: announce.has_ratchet,
                lxmf_stamp_cost: announce.lxmf_stamp_cost,
            })
            .collect::<Vec<_>>();
        if let Some(limit) = limit {
            candidates.truncate(limit);
        }
        candidates
    }
}

pub async fn payload_from_announce_event(
    event: rns_transport::transport::AnnounceEvent,
) -> AnnouncePayload {
    let destination = event.destination.lock().await;
    let identity = destination.desc.identity;
    let destination_hash = destination.desc.address_hash.to_hex_string();
    let kind = kind_from_name_hash(&event.name_hash);
    let app_data = event.app_data.as_slice();
    let display_name =
        display_name_for_kind(&kind, app_data).unwrap_or_else(|| destination_hash.clone());
    let lxmf_stamp_cost = lxmf_delivery_stamp_cost(&kind, app_data);
    let (associated_hash, node_associated_hash) = match kind {
        DirectoryKind::Node => (Some(associated_hash(identity, "lxmf", "delivery")), None),
        DirectoryKind::Peer => (
            Some(associated_hash(identity, "nomadnetwork", "node")),
            None,
        ),
        DirectoryKind::Propagation => (
            Some(associated_hash(identity, "lxmf", "delivery")),
            Some(associated_hash(identity, "nomadnetwork", "node")),
        ),
        DirectoryKind::OmenChat => (None, None),
        DirectoryKind::Unknown => (None, None),
    };

    AnnouncePayload {
        destination_hash,
        display_name,
        kind,
        associated_hash,
        node_associated_hash,
        has_ratchet: event.ratchet.is_some(),
        lxmf_stamp_cost,
    }
}

pub fn display_name_for_kind(kind: &DirectoryKind, app_data: &[u8]) -> Option<String> {
    match kind {
        DirectoryKind::Node | DirectoryKind::OmenChat | DirectoryKind::Unknown => {
            display_name_from_app_data(app_data)
        }
        DirectoryKind::Peer => {
            lxmf_delivery_display_name(app_data).or_else(|| display_name_from_app_data(app_data))
        }
        DirectoryKind::Propagation => {
            lxmf_propagation_display_name(app_data).or_else(|| display_name_from_app_data(app_data))
        }
    }
}

pub fn kind_from_name_hash(
    name_hash: &[u8; rns_transport::destination::NAME_HASH_LENGTH],
) -> DirectoryKind {
    let node = rns_transport::destination::DestinationName::new("nomadnetwork", "node");
    let peer = rns_transport::destination::DestinationName::new("lxmf", "delivery");
    let propagation = rns_transport::destination::DestinationName::new("lxmf", "propagation");
    let omenchat = rns_transport::destination::DestinationName::new("omenchat", "node");

    if name_hash == node.as_name_hash_slice() {
        DirectoryKind::Node
    } else if name_hash == peer.as_name_hash_slice() {
        DirectoryKind::Peer
    } else if name_hash == propagation.as_name_hash_slice() {
        DirectoryKind::Propagation
    } else if name_hash == omenchat.as_name_hash_slice() {
        DirectoryKind::OmenChat
    } else {
        DirectoryKind::Unknown
    }
}

#[cfg(feature = "native-lxmf")]
fn lxmf_delivery_display_name(app_data: &[u8]) -> Option<String> {
    crate::runtime::native_lxmf::codec::delivery_display_name_from_app_data(app_data)
}

#[cfg(not(feature = "native-lxmf"))]
fn lxmf_delivery_display_name(_app_data: &[u8]) -> Option<String> {
    None
}

#[cfg(feature = "native-lxmf")]
fn lxmf_delivery_stamp_cost(kind: &DirectoryKind, app_data: &[u8]) -> Option<u8> {
    if *kind == DirectoryKind::Peer {
        crate::runtime::native_lxmf::codec::delivery_announce_stamp_cost(app_data)
    } else {
        None
    }
}

#[cfg(not(feature = "native-lxmf"))]
fn lxmf_delivery_stamp_cost(_kind: &DirectoryKind, _app_data: &[u8]) -> Option<u8> {
    None
}

#[cfg(feature = "native-lxmf")]
fn lxmf_propagation_display_name(app_data: &[u8]) -> Option<String> {
    crate::runtime::native_lxmf::codec::propagation_display_name_from_app_data(app_data)
}

#[cfg(not(feature = "native-lxmf"))]
fn lxmf_propagation_display_name(_app_data: &[u8]) -> Option<String> {
    None
}

pub fn display_name_from_app_data(app_data: &[u8]) -> Option<String> {
    if app_data.is_empty() {
        return None;
    }
    let value = std::str::from_utf8(app_data).ok()?.trim();
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

fn associated_hash(
    identity: rns_transport::identity::Identity,
    app_name: &str,
    aspect: &str,
) -> String {
    rns_transport::destination::SingleOutputDestination::new(
        identity,
        rns_transport::destination::DestinationName::new(app_name, aspect),
    )
    .desc
    .address_hash
    .to_hex_string()
}

fn kind_key(kind: &DirectoryKind) -> &'static str {
    match kind {
        DirectoryKind::Node => "node",
        DirectoryKind::Peer => "peer",
        DirectoryKind::Propagation => "propagation",
        DirectoryKind::OmenChat => "omenchat",
        DirectoryKind::Unknown => "unknown",
    }
}

#[cfg(test)]
mod tests {
    use rand_core::OsRng;

    use super::*;

    #[test]
    fn maps_known_announce_name_hashes_to_directory_kinds() {
        let node = rns_transport::destination::DestinationName::new("nomadnetwork", "node");
        let peer = rns_transport::destination::DestinationName::new("lxmf", "delivery");
        let propagation = rns_transport::destination::DestinationName::new("lxmf", "propagation");
        let unknown = rns_transport::destination::DestinationName::new("other", "aspect");

        assert_eq!(
            kind_from_name_hash(node.as_name_hash_slice().try_into().unwrap()),
            DirectoryKind::Node
        );
        assert_eq!(
            kind_from_name_hash(peer.as_name_hash_slice().try_into().unwrap()),
            DirectoryKind::Peer
        );
        assert_eq!(
            kind_from_name_hash(propagation.as_name_hash_slice().try_into().unwrap()),
            DirectoryKind::Propagation
        );
        assert_eq!(
            kind_from_name_hash(unknown.as_name_hash_slice().try_into().unwrap()),
            DirectoryKind::Unknown
        );
    }

    #[test]
    fn display_name_uses_utf8_app_data_when_available() {
        assert_eq!(
            display_name_from_app_data(b" Node One "),
            Some("Node One".into())
        );
        assert_eq!(display_name_from_app_data(b""), None);
        assert_eq!(display_name_from_app_data(&[0xff, 0xfe]), None);
    }

    #[test]
    fn peer_display_name_falls_back_to_utf8_without_lxmf_feature() {
        assert_eq!(
            display_name_for_kind(&DirectoryKind::Peer, b"Peer Name"),
            Some("Peer Name".into())
        );
    }

    #[test]
    fn propagation_display_name_preserves_safe_utf8_fallback() {
        assert_eq!(
            display_name_for_kind(&DirectoryKind::Propagation, b"Relay Node"),
            Some("Relay Node".into())
        );
        assert_eq!(
            display_name_for_kind(&DirectoryKind::Propagation, &[0xff, 0xfe]),
            None
        );
    }

    #[cfg(feature = "native-lxmf")]
    #[test]
    fn peer_display_name_uses_lxmf_delivery_announce_parser() {
        let encoded = lxmf::wire::announce::encode_delivery_display_name_app_data("Alice Relay")
            .expect("encoded delivery app data");

        assert_eq!(
            display_name_for_kind(&DirectoryKind::Peer, encoded.as_slice()),
            Some("Alice Relay".into())
        );
    }

    #[test]
    fn announce_state_deduplicates_and_counts_payloads() {
        let mut state = NativeAnnounceState::default();
        let first = AnnouncePayload {
            destination_hash: "abc".into(),
            display_name: "Node A".into(),
            kind: DirectoryKind::Node,
            associated_hash: Some("peer".into()),
            node_associated_hash: None,
            has_ratchet: false,
            lxmf_stamp_cost: None,
        };
        let second = AnnouncePayload {
            display_name: "Node A Updated".into(),
            ..first.clone()
        };

        state.ingest(first);
        state.ingest(second);
        let snapshot = state.snapshot();
        let candidates = state.candidates(None);

        assert_eq!(snapshot.announce_counts.get("node"), Some(&2));
        assert_eq!(snapshot.pending_announces, 1);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].display_name, "Node A Updated");
    }

    #[test]
    fn announce_state_counts_ratchet_announces_without_leaking_bytes() {
        let mut state = NativeAnnounceState::default();
        state.ingest(AnnouncePayload {
            destination_hash: "peer".into(),
            display_name: "Peer".into(),
            kind: DirectoryKind::Peer,
            associated_hash: None,
            node_associated_hash: None,
            has_ratchet: true,
            lxmf_stamp_cost: None,
        });

        let snapshot = state.snapshot();
        let candidates = state.candidates(None);

        assert_eq!(snapshot.ratchet_announces, 1);
        assert!(candidates[0].has_ratchet);
    }

    #[test]
    fn associated_hashes_follow_python_announce_relationships() {
        let private_identity = rns_transport::identity::PrivateIdentity::new_from_rand(OsRng);
        let identity = *private_identity.as_identity();
        let node_associated = associated_hash(identity, "lxmf", "delivery");
        let peer_associated = associated_hash(identity, "nomadnetwork", "node");

        assert_eq!(node_associated.len(), 32);
        assert_eq!(peer_associated.len(), 32);
        assert_ne!(node_associated, peer_associated);
    }

    #[test]
    fn announce_state_preserves_propagation_node_association() {
        let mut state = NativeAnnounceState::default();
        state.ingest(AnnouncePayload {
            destination_hash: "prop".into(),
            display_name: "Propagation".into(),
            kind: DirectoryKind::Propagation,
            associated_hash: Some("peer".into()),
            node_associated_hash: Some("node".into()),
            has_ratchet: false,
            lxmf_stamp_cost: None,
        });

        let candidates = state.candidates(None);

        assert_eq!(candidates[0].associated_hash.as_deref(), Some("peer"));
        assert_eq!(candidates[0].node_associated_hash.as_deref(), Some("node"));
    }
}
