use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

const ANNOUNCE_STREAM_MAXLENGTH: usize = 256;
const TRANSIENT_ENTRY_RETENTION_SECONDS: f64 = 6.0 * 60.0 * 60.0;
const TRANSIENT_ENTRY_MAXCOUNT: usize = 1024;
const DUPLICATE_ANNOUNCE_SAVE_COOLDOWN_SECONDS: f64 = 5.0 * 60.0;
const LIVE_ANNOUNCE_SAVE_DEBOUNCE_SECONDS: f64 = 30.0;

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum DirectoryKind {
    #[default]
    Node,
    Peer,
    Propagation,
    OmenChat,
    Unknown,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(into = "u8", try_from = "u8")]
pub enum TrustLevel {
    Warning,
    Untrusted,
    Unknown,
    Trusted,
}

impl From<TrustLevel> for u8 {
    fn from(value: TrustLevel) -> Self {
        match value {
            TrustLevel::Warning => 0x00,
            TrustLevel::Untrusted => 0x01,
            TrustLevel::Unknown => 0x02,
            TrustLevel::Trusted => 0xff,
        }
    }
}

impl TryFrom<u8> for TrustLevel {
    type Error = String;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0x00 => Ok(Self::Warning),
            0x01 => Ok(Self::Untrusted),
            0x02 => Ok(Self::Unknown),
            0xff => Ok(Self::Trusted),
            _ => Ok(Self::Unknown),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PreferredDelivery {
    Direct,
    Propagated,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct DirectoryEntry {
    pub destination_hash: String,
    pub display_name: String,
    pub kind: DirectoryKind,
    pub trusted: bool,
    pub trust_level: TrustLevel,
    pub saved: bool,
    pub identify_on_connect: bool,
    pub preferred_delivery: Option<PreferredDelivery>,
    pub sort_rank: Option<i32>,
    pub hosts_node: bool,
    pub associated_hash: Option<String>,
    pub node_associated_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lxmf_stamp_cost: Option<u8>,
    pub last_seen: f64,
}

impl DirectoryEntry {
    pub fn new(
        destination_hash: impl Into<String>,
        display_name: impl Into<String>,
        kind: DirectoryKind,
    ) -> Self {
        let hosts_node = kind == DirectoryKind::Node;
        Self {
            destination_hash: destination_hash.into(),
            display_name: display_name.into(),
            kind,
            trusted: false,
            trust_level: TrustLevel::Unknown,
            saved: false,
            identify_on_connect: false,
            preferred_delivery: None,
            sort_rank: None,
            hosts_node,
            associated_hash: None,
            node_associated_hash: None,
            lxmf_stamp_cost: None,
            last_seen: 0.0,
        }
    }

    pub fn set_trust_level(&mut self, trust_level: TrustLevel) {
        self.trusted = trust_level == TrustLevel::Trusted;
        self.trust_level = trust_level;
    }
}

#[derive(Clone, Debug)]
pub struct DirectoryService {
    path: PathBuf,
    entries: BTreeMap<String, DirectoryEntry>,
    announce_stream: Vec<DirectoryEntry>,
    pending_live_save: bool,
    pending_live_save_due: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
struct DirectoryFile {
    entries: Vec<DirectoryEntry>,
    announce_stream: Vec<DirectoryEntry>,
}

impl DirectoryService {
    pub fn new(path: PathBuf) -> crate::error::AppResult<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut service = Self {
            path,
            entries: BTreeMap::new(),
            announce_stream: Vec::new(),
            pending_live_save: false,
            pending_live_save_due: 0.0,
        };
        service.load()?;
        Ok(service)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn clear_transient_announces(&mut self) -> crate::error::AppResult<()> {
        self.announce_stream.clear();
        self.save()
    }

    pub fn ingest_announce(
        &mut self,
        destination_hash: impl Into<String>,
        display_name: impl Into<String>,
        kind: DirectoryKind,
        associated_hash: Option<String>,
        node_associated_hash: Option<String>,
    ) -> crate::error::AppResult<DirectoryEntry> {
        self.ingest_announce_with_metadata(
            destination_hash,
            display_name,
            kind,
            associated_hash,
            node_associated_hash,
            None,
        )
    }

    pub fn ingest_announce_with_metadata(
        &mut self,
        destination_hash: impl Into<String>,
        display_name: impl Into<String>,
        kind: DirectoryKind,
        associated_hash: Option<String>,
        node_associated_hash: Option<String>,
        lxmf_stamp_cost: Option<u8>,
    ) -> crate::error::AppResult<DirectoryEntry> {
        let destination_hash = destination_hash.into();
        let existing = self.entries.get(&destination_hash).cloned();
        let now = timestamp_secs();
        let mut entry = DirectoryEntry::new(
            destination_hash.clone(),
            preferred_display_name(
                Some(display_name.into()),
                existing.as_ref().map(|entry| entry.display_name.as_str()),
                &destination_hash,
            ),
            kind.clone(),
        );
        if let Some(existing) = existing.as_ref() {
            entry.trust_level = existing.trust_level;
            entry.trusted = existing.trusted;
            entry.saved = existing.saved;
            entry.identify_on_connect = existing.identify_on_connect;
            entry.preferred_delivery = existing.preferred_delivery.clone();
            entry.sort_rank = existing.sort_rank;
            entry.hosts_node = existing.hosts_node || kind == DirectoryKind::Node;
            entry.associated_hash = associated_hash.or_else(|| existing.associated_hash.clone());
            entry.node_associated_hash =
                node_associated_hash.or_else(|| existing.node_associated_hash.clone());
            entry.lxmf_stamp_cost = lxmf_stamp_cost.or(existing.lxmf_stamp_cost);
        } else {
            entry.associated_hash = associated_hash;
            entry.node_associated_hash = node_associated_hash;
            entry.lxmf_stamp_cost = lxmf_stamp_cost;
        }
        entry.last_seen = now;
        if kind == DirectoryKind::Node && entry.trust_level == TrustLevel::Trusted {
            entry.saved = true;
        }
        let should_save_entry = existing.as_ref().is_none_or(|existing| {
            !directory_entries_match_ignoring_last_seen(existing, &entry)
                || now - existing.last_seen >= DUPLICATE_ANNOUNCE_SAVE_COOLDOWN_SECONDS
        });
        self.entries.insert(destination_hash.clone(), entry.clone());
        self.announce_stream
            .retain(|item| item.destination_hash != destination_hash);
        self.announce_stream.insert(0, entry.clone());
        self.trim_announce_stream();
        let pruned = self.prune_transient_entries();
        if should_save_entry || pruned {
            self.schedule_live_save();
        }
        Ok(entry)
    }

    pub fn sync_discoveries(
        &mut self,
        payloads: &[crate::runtime::DirectoryCandidate],
    ) -> crate::error::AppResult<Vec<DirectoryEntry>> {
        let mut changed = Vec::new();
        for payload in payloads {
            let before = self.find(&payload.destination_hash);
            let entry = self.ingest_announce_with_metadata(
                payload.destination_hash.clone(),
                payload.display_name.clone(),
                payload.kind.clone(),
                payload.associated_hash.clone(),
                payload.node_associated_hash.clone(),
                payload.lxmf_stamp_cost,
            )?;
            if before.as_ref() != Some(&entry) {
                changed.push(entry);
            }
        }
        Ok(changed)
    }

    pub fn save_entry(
        &mut self,
        destination_hash: &str,
    ) -> crate::error::AppResult<Option<DirectoryEntry>> {
        let Some(mut entry) = self.find(destination_hash) else {
            return Ok(None);
        };
        entry.saved = true;
        self.entries.insert(destination_hash.into(), entry.clone());
        self.save()?;
        Ok(Some(entry))
    }

    pub fn remove_saved_entry(
        &mut self,
        destination_hash: &str,
    ) -> crate::error::AppResult<Option<DirectoryEntry>> {
        let Some(mut entry) = self.find(destination_hash) else {
            return Ok(None);
        };
        entry.saved = false;
        entry.trusted = false;
        entry.trust_level = TrustLevel::Unknown;
        entry.identify_on_connect = false;
        entry.preferred_delivery = None;
        self.entries.insert(destination_hash.into(), entry.clone());
        self.save()?;
        Ok(Some(entry))
    }

    pub fn set_trusted(
        &mut self,
        destination_hash: &str,
        trusted: bool,
    ) -> crate::error::AppResult<Option<DirectoryEntry>> {
        self.set_trust_level(
            destination_hash,
            if trusted {
                TrustLevel::Trusted
            } else {
                TrustLevel::Unknown
            },
        )
    }

    pub fn set_trust_level(
        &mut self,
        destination_hash: &str,
        trust_level: TrustLevel,
    ) -> crate::error::AppResult<Option<DirectoryEntry>> {
        let Some(mut entry) = self.find(destination_hash) else {
            return Ok(None);
        };
        entry.set_trust_level(trust_level);
        if trust_level == TrustLevel::Trusted {
            entry.saved = true;
        }
        self.entries.insert(destination_hash.into(), entry.clone());
        self.save()?;
        Ok(Some(entry))
    }

    pub fn trust_level(&self, destination_hash: &str, announced_name: Option<&str>) -> TrustLevel {
        let Some(entry) = self.find(destination_hash) else {
            return TrustLevel::Unknown;
        };
        if let Some(announced_name) = announced_name {
            if entry.trust_level != TrustLevel::Trusted
                && self.entries.values().any(|candidate| {
                    candidate.destination_hash != destination_hash
                        && candidate.display_name == announced_name
                })
            {
                return TrustLevel::Warning;
            }
        }
        entry.trust_level
    }

    pub fn preferred_delivery(&self, destination_hash: &str) -> PreferredDelivery {
        self.find(destination_hash)
            .and_then(|entry| entry.preferred_delivery)
            .unwrap_or(PreferredDelivery::Direct)
    }

    pub fn set_preferred_delivery(
        &mut self,
        destination_hash: &str,
        preferred_delivery: Option<PreferredDelivery>,
    ) -> crate::error::AppResult<Option<DirectoryEntry>> {
        let Some(mut entry) = self.find(destination_hash) else {
            return Ok(None);
        };
        entry.preferred_delivery = preferred_delivery;
        entry.saved = true;
        self.entries.insert(destination_hash.into(), entry.clone());
        self.save()?;
        Ok(Some(entry))
    }

    pub fn set_identify_on_connect(
        &mut self,
        destination_hash: &str,
        enabled: bool,
    ) -> crate::error::AppResult<Option<DirectoryEntry>> {
        let Some(mut entry) = self.find(destination_hash) else {
            return Ok(None);
        };
        entry.identify_on_connect = enabled;
        entry.saved = true;
        self.entries.insert(destination_hash.into(), entry.clone());
        self.save()?;
        Ok(Some(entry))
    }

    pub fn should_identify_on_connect(&self, destination_hash: &str) -> bool {
        self.entries
            .get(destination_hash)
            .is_some_and(|entry| entry.identify_on_connect)
    }

    pub fn find(&self, destination_hash: &str) -> Option<DirectoryEntry> {
        self.entries.get(destination_hash).cloned().or_else(|| {
            self.announce_stream
                .iter()
                .find(|entry| entry.destination_hash == destination_hash)
                .cloned()
        })
    }

    pub fn list_entries(&self) -> Vec<DirectoryEntry> {
        let mut merged = self
            .announce_stream
            .iter()
            .map(|entry| (entry.destination_hash.clone(), entry.clone()))
            .collect::<BTreeMap<_, _>>();
        for (hash, entry) in &self.entries {
            let merged_entry = merged_entry(merged.get(hash), entry);
            merged.insert(hash.clone(), merged_entry);
        }
        let mut entries = merged.into_values().collect::<Vec<_>>();
        entries.sort_by(sort_entries);
        entries
    }

    pub fn known_nodes(&self) -> Vec<DirectoryEntry> {
        let mut nodes = self
            .entries
            .values()
            .filter(|entry| {
                (entry.hosts_node || entry.kind == DirectoryKind::Node)
                    && (entry.saved || entry.trusted)
            })
            .cloned()
            .collect::<Vec<_>>();
        nodes.sort_by(sort_entries);
        nodes
    }

    pub fn propagation_hash_for_node(&self, node_hash: &str) -> Option<String> {
        self.entries
            .values()
            .find(|entry| {
                entry.kind == DirectoryKind::Propagation
                    && entry.node_associated_hash.as_deref() == Some(node_hash)
            })
            .map(|entry| entry.destination_hash.clone())
    }

    pub fn list_live_entries(&self) -> Vec<DirectoryEntry> {
        let now = timestamp_secs();
        let mut merged = BTreeMap::new();
        for entry in &self.announce_stream {
            if !is_persistent_entry(entry)
                && now - entry.last_seen > TRANSIENT_ENTRY_RETENTION_SECONDS
            {
                continue;
            }
            merged.insert(
                entry.destination_hash.clone(),
                merged_entry(
                    Some(entry),
                    self.entries.get(&entry.destination_hash).unwrap_or(entry),
                ),
            );
        }
        let mut entries = merged.into_values().collect::<Vec<_>>();
        entries.sort_by(|left, right| {
            right
                .last_seen
                .total_cmp(&left.last_seen)
                .then_with(|| {
                    left.display_name
                        .to_lowercase()
                        .cmp(&right.display_name.to_lowercase())
                })
                .then_with(|| left.destination_hash.cmp(&right.destination_hash))
        });
        entries
    }

    pub fn filtered_entries(
        &self,
        kind: Option<DirectoryKind>,
        query: &str,
        saved_only: Option<bool>,
    ) -> Vec<DirectoryEntry> {
        let entries = match saved_only {
            Some(true) => {
                let mut entries = self
                    .entries
                    .values()
                    .filter(|entry| entry.saved)
                    .cloned()
                    .collect::<Vec<_>>();
                entries.sort_by(sort_entries);
                entries
            }
            Some(false) => self
                .list_live_entries()
                .into_iter()
                .filter(|entry| !entry.saved)
                .collect(),
            None => self.list_live_entries(),
        };
        let query = query.trim().to_lowercase();
        entries
            .into_iter()
            .filter(|entry| kind.as_ref().is_none_or(|kind| &entry.kind == kind))
            .filter(|entry| {
                if query.is_empty() {
                    return true;
                }
                [
                    entry.display_name.as_str(),
                    entry.destination_hash.as_str(),
                    entry.associated_hash.as_deref().unwrap_or_default(),
                    entry.node_associated_hash.as_deref().unwrap_or_default(),
                ]
                .join(" ")
                .to_lowercase()
                .contains(&query)
            })
            .collect()
    }

    fn load(&mut self) -> crate::error::AppResult<()> {
        if !self.path.exists() {
            return Ok(());
        }
        let raw = std::fs::read_to_string(&self.path)?;
        match serde_json::from_str::<DirectoryFile>(&raw) {
            Ok(file) => {
                self.entries = file
                    .entries
                    .into_iter()
                    .map(|entry| (entry.destination_hash.clone(), entry))
                    .collect();
                self.announce_stream = file.announce_stream;
                if self.announce_stream.is_empty() && !self.entries.is_empty() {
                    self.rebuild_announce_stream_from_entries();
                }
                self.trim_announce_stream();
                if self.prune_transient_entries() {
                    self.save()?;
                }
                Ok(())
            }
            Err(_) => {
                backup_corrupt_file(&self.path)?;
                self.entries.clear();
                self.announce_stream.clear();
                Ok(())
            }
        }
    }

    pub fn flush_due_save(&mut self) -> crate::error::AppResult<bool> {
        if self.pending_live_save && self.pending_live_save_due <= timestamp_secs() {
            self.save()?;
            return Ok(true);
        }
        Ok(false)
    }

    pub fn flush_pending_save(&mut self) -> crate::error::AppResult<bool> {
        if self.pending_live_save {
            self.save()?;
            return Ok(true);
        }
        Ok(false)
    }

    fn save(&mut self) -> crate::error::AppResult<()> {
        let mut snapshot = self.clone();
        snapshot.prune_transient_entries();
        let file = DirectoryFile {
            entries: snapshot.entries.values().cloned().collect(),
            announce_stream: snapshot
                .announce_stream
                .iter()
                .take(ANNOUNCE_STREAM_MAXLENGTH)
                .cloned()
                .collect(),
        };
        let temp = self
            .path
            .with_extension(format!("json.tmp.{}", std::process::id()));
        std::fs::write(
            &temp,
            serde_json::to_vec_pretty(&file).expect("directory serializes"),
        )?;
        std::fs::rename(temp, &self.path)?;
        self.pending_live_save = false;
        self.pending_live_save_due = 0.0;
        Ok(())
    }

    fn schedule_live_save(&mut self) {
        let due = timestamp_secs() + LIVE_ANNOUNCE_SAVE_DEBOUNCE_SECONDS;
        if !self.pending_live_save || self.pending_live_save_due > due {
            self.pending_live_save = true;
            self.pending_live_save_due = due;
        }
    }

    fn trim_announce_stream(&mut self) {
        self.announce_stream.truncate(ANNOUNCE_STREAM_MAXLENGTH);
    }

    fn rebuild_announce_stream_from_entries(&mut self) {
        let mut entries = self.entries.values().cloned().collect::<Vec<_>>();
        entries.sort_by(|left, right| {
            right
                .last_seen
                .total_cmp(&left.last_seen)
                .then_with(|| left.destination_hash.cmp(&right.destination_hash))
        });
        self.announce_stream = entries
            .into_iter()
            .take(ANNOUNCE_STREAM_MAXLENGTH)
            .collect();
    }

    fn prune_transient_entries(&mut self) -> bool {
        let now = timestamp_secs();
        let mut changed = false;
        self.announce_stream.retain(|entry| {
            let retain = is_persistent_entry(entry)
                || now - entry.last_seen <= TRANSIENT_ENTRY_RETENTION_SECONDS;
            changed |= !retain;
            retain
        });
        self.trim_announce_stream();

        let transient_hashes = self
            .entries
            .values()
            .filter(|entry| !is_persistent_entry(entry))
            .cloned()
            .collect::<Vec<_>>();
        let mut removable_hashes = transient_hashes
            .iter()
            .filter(|entry| now - entry.last_seen > TRANSIENT_ENTRY_RETENTION_SECONDS)
            .map(|entry| entry.destination_hash.clone())
            .collect::<std::collections::BTreeSet<_>>();

        let retained_transients = transient_hashes
            .into_iter()
            .filter(|entry| !removable_hashes.contains(&entry.destination_hash))
            .collect::<Vec<_>>();
        if retained_transients.len() > TRANSIENT_ENTRY_MAXCOUNT {
            let overflow_count = retained_transients.len() - TRANSIENT_ENTRY_MAXCOUNT;
            let mut overflow = retained_transients;
            overflow.sort_by(|left, right| {
                left.last_seen
                    .total_cmp(&right.last_seen)
                    .then_with(|| left.destination_hash.cmp(&right.destination_hash))
            });
            for entry in overflow.into_iter().take(overflow_count) {
                removable_hashes.insert(entry.destination_hash);
            }
        }

        if !removable_hashes.is_empty() {
            changed = true;
            for hash in &removable_hashes {
                self.entries.remove(hash);
            }
            self.announce_stream
                .retain(|entry| !removable_hashes.contains(&entry.destination_hash));
        }
        changed
    }
}

fn is_persistent_entry(entry: &DirectoryEntry) -> bool {
    entry.saved || entry.trusted || entry.identify_on_connect || entry.preferred_delivery.is_some()
}

fn directory_entries_match_ignoring_last_seen(
    left: &DirectoryEntry,
    right: &DirectoryEntry,
) -> bool {
    left.destination_hash == right.destination_hash
        && left.display_name == right.display_name
        && left.kind == right.kind
        && left.trusted == right.trusted
        && left.trust_level == right.trust_level
        && left.saved == right.saved
        && left.identify_on_connect == right.identify_on_connect
        && left.preferred_delivery == right.preferred_delivery
        && left.sort_rank == right.sort_rank
        && left.hosts_node == right.hosts_node
        && left.associated_hash == right.associated_hash
        && left.node_associated_hash == right.node_associated_hash
        && left.lxmf_stamp_cost == right.lxmf_stamp_cost
}

fn merged_entry(primary: Option<&DirectoryEntry>, secondary: &DirectoryEntry) -> DirectoryEntry {
    let Some(primary) = primary else {
        return secondary.clone();
    };
    let mut entry = primary.clone();
    entry.display_name = preferred_display_name(
        Some(primary.display_name.clone()),
        Some(&secondary.display_name),
        &primary.destination_hash,
    );
    entry.trusted = primary.trusted || secondary.trusted;
    entry.trust_level = if secondary.trust_level == TrustLevel::Trusted
        || primary.trust_level != TrustLevel::Trusted
    {
        secondary.trust_level
    } else {
        primary.trust_level
    };
    entry.saved = primary.saved || secondary.saved;
    entry.identify_on_connect = primary.identify_on_connect || secondary.identify_on_connect;
    entry.preferred_delivery = primary
        .preferred_delivery
        .clone()
        .or_else(|| secondary.preferred_delivery.clone());
    entry.sort_rank = primary.sort_rank.or(secondary.sort_rank);
    entry.hosts_node = primary.hosts_node || secondary.hosts_node;
    entry.associated_hash = primary
        .associated_hash
        .clone()
        .or_else(|| secondary.associated_hash.clone());
    entry.node_associated_hash = primary
        .node_associated_hash
        .clone()
        .or_else(|| secondary.node_associated_hash.clone());
    entry.lxmf_stamp_cost = primary.lxmf_stamp_cost.or(secondary.lxmf_stamp_cost);
    entry.last_seen = primary.last_seen.max(secondary.last_seen);
    entry
}

fn sort_entries(left: &DirectoryEntry, right: &DirectoryEntry) -> std::cmp::Ordering {
    left.sort_rank
        .unwrap_or(1 << 20)
        .cmp(&right.sort_rank.unwrap_or(1 << 20))
        .then_with(|| {
            (left.trust_level != TrustLevel::Trusted)
                .cmp(&(right.trust_level != TrustLevel::Trusted))
        })
        .then_with(|| right.last_seen.total_cmp(&left.last_seen))
        .then_with(|| {
            left.display_name
                .to_lowercase()
                .cmp(&right.display_name.to_lowercase())
        })
        .then_with(|| left.destination_hash.cmp(&right.destination_hash))
}

fn preferred_display_name(
    incoming_name: Option<String>,
    existing_name: Option<&str>,
    destination_hash: &str,
) -> String {
    let incoming = incoming_name.unwrap_or_default();
    if is_placeholder_name(&incoming, destination_hash)
        && existing_name.is_some_and(|name| !is_placeholder_name(name, destination_hash))
    {
        return existing_name.unwrap_or_default().into();
    }
    if !incoming.is_empty() {
        incoming
    } else if let Some(existing) = existing_name {
        existing.into()
    } else {
        destination_hash.chars().take(8).collect()
    }
}

fn is_placeholder_name(display_name: &str, destination_hash: &str) -> bool {
    let normalized = display_name.trim();
    if normalized.is_empty() {
        return true;
    }
    let lowered_name = normalized.to_lowercase();
    let lowered_hash = destination_hash.to_lowercase();
    lowered_name == lowered_hash
        || lowered_name == lowered_hash.chars().take(8).collect::<String>()
        || (normalized.starts_with('<') && normalized.ends_with('>'))
}

fn backup_corrupt_file(path: &Path) -> crate::error::AppResult<()> {
    let backup = path.with_file_name(format!(
        "{}.corrupt.{}.bak",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("directory.json"),
        timestamp_millis()
    ));
    std::fs::copy(path, backup)?;
    Ok(())
}

fn timestamp_secs() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64()
}

fn timestamp_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}
