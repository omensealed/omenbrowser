use std::collections::BTreeMap;
use std::fs::File;
use std::io::{ErrorKind, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use sha2::{Digest, Sha256};

use crate::error::{AppError, AppResult};
use crate::messaging::{
    direct_lxmf_timeout_transition, ConversationThread, MessageSummary, NativeLxmfReplyTicket,
    TransportMethod,
};

pub const MESSAGE_STORE_THREAD_MAX_BYTES: u64 = 8 * 1024 * 1024;
pub const MESSAGE_STORE_THREAD_MAX_MESSAGES: usize = 4096;
pub const MESSAGE_STORE_MAX_THREADS: usize = 256;
pub const MESSAGE_STORE_MAX_TOTAL_BYTES: u64 = 64 * 1024 * 1024;
pub const MESSAGE_STORE_MAX_SCAN_ENTRIES: usize = 4096;
pub const MESSAGE_STORE_PEER_KEY_MAX_BYTES: usize = 256;
pub const MESSAGE_STORE_CORRUPT_BACKUP_MAX_FILES: usize = 4;
pub const MESSAGE_STORE_CORRUPT_BACKUP_MAX_TOTAL_BYTES: u64 =
    MESSAGE_STORE_CORRUPT_BACKUP_MAX_FILES as u64 * MESSAGE_STORE_THREAD_MAX_BYTES;

const MESSAGE_STORE_CORRUPT_BACKUP_PREFIX: &str = "omen-message.corrupt.";
const MESSAGE_STORE_CORRUPT_BACKUP_SUFFIX: &str = ".bak";
static MESSAGE_STORE_FILE_SEQUENCE: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Debug)]
pub struct MessageStore {
    root: PathBuf,
}

impl MessageStore {
    pub fn new(root: PathBuf) -> AppResult<Self> {
        ensure_real_directory(&root)?;
        Ok(Self { root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn import_missing_threads_from(&self, source_root: &Path) -> AppResult<usize> {
        match std::fs::symlink_metadata(source_root) {
            Ok(metadata) if metadata.file_type().is_dir() => {}
            Ok(_) => {
                return Err(AppError::Runtime(
                    "message import root must be a directory and not a symbolic link".into(),
                ));
            }
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(0),
            Err(error) => return Err(error.into()),
        }
        ensure_real_directory(&self.root)?;
        let target_inventory = inventory_thread_files(&self.root)?;
        let mut target_threads = target_inventory.len();
        let mut target_bytes = target_inventory
            .iter()
            .fold(0_u64, |total, (_, bytes)| total.saturating_add(*bytes));
        let mut imported = 0usize;
        for (path, _) in inventory_thread_files(source_root)? {
            let raw = read_thread_file(&path)?;
            let Ok(thread) = serde_json::from_slice::<ConversationThread>(&raw) else {
                continue;
            };
            if validate_thread(&thread).is_err() {
                continue;
            }
            let target = self.thread_path(&thread.peer_hash)?;
            match std::fs::symlink_metadata(&target) {
                Ok(_) => continue,
                Err(error) if error.kind() == ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
            ensure_thread_capacity(target_threads, target_bytes, raw.len() as u64)?;
            publish_thread_bytes(&target, &raw, ThreadPublishMode::CreateNew)?;
            target_threads = target_threads.saturating_add(1);
            target_bytes = target_bytes.saturating_add(raw.len() as u64);
            imported += 1;
        }
        Ok(imported)
    }

    pub fn append(&self, mut message: MessageSummary) -> AppResult<MessageSummary> {
        validate_message(&message)?;
        if message.message_id.is_none() {
            message.message_id = Some(format!("msg-{}", timestamp_millis()));
        }
        let mut thread = self.load_thread(&message.peer_hash, Some(&message.peer_label))?;
        if !message.peer_label.is_empty() {
            thread.peer_label = message.peer_label.clone();
        }
        if let Some(message_id) = message.message_id.as_deref() {
            if let Some(existing) = thread
                .messages
                .iter()
                .find(|existing| existing.message_id.as_deref() == Some(message_id))
                .cloned()
            {
                return Ok(existing);
            }
        }
        if message.incoming && message.unread {
            thread.unread_count = thread.unread_count.saturating_add(1);
        }
        thread.messages.push(message.clone());
        remember_reply_ticket_from_message(&mut thread, &message, timestamp_secs());
        thread.messages.sort_by(|left, right| {
            left.timestamp
                .total_cmp(&right.timestamp)
                .then_with(|| left.message_id.cmp(&right.message_id))
                .then_with(|| left.content.cmp(&right.content))
        });
        validate_thread(&thread)?;
        self.save_thread(&thread)?;
        Ok(message)
    }

    pub fn list_threads(&self) -> AppResult<Vec<ConversationThread>> {
        let mut threads = Vec::new();
        for (path, _) in inventory_thread_files(&self.root)? {
            let peer_hash = path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or_default();
            threads.push(self.load_thread_path(&path, peer_hash, None)?);
        }
        threads.sort_by(|left, right| {
            recent_timestamp(right)
                .total_cmp(&recent_timestamp(left))
                .then_with(|| left.peer_hash.cmp(&right.peer_hash))
        });
        Ok(threads)
    }

    pub fn get_thread(&self, peer_hash: &str) -> AppResult<ConversationThread> {
        self.load_thread(peer_hash, None)
    }

    pub fn latest_valid_lxmf_reply_ticket(
        &self,
        peer_hash: &str,
        now: f64,
    ) -> AppResult<Option<NativeLxmfReplyTicket>> {
        let thread = self.load_thread(peer_hash, None)?;
        if let Some(ticket) = thread
            .lxmf_reply_ticket
            .as_ref()
            .filter(|ticket| ticket.expires > now && ticket.ticket.len() == 16)
        {
            return Ok(Some(ticket.clone()));
        }
        Ok(thread
            .messages
            .iter()
            .rev()
            .filter_map(|message| reply_ticket_from_message(message, now))
            .next())
    }

    pub fn ensure_thread(
        &self,
        peer_hash: &str,
        peer_label: Option<&str>,
    ) -> AppResult<ConversationThread> {
        let thread = self.load_thread(peer_hash, peer_label)?;
        self.save_thread(&thread)?;
        Ok(thread)
    }

    pub fn delete_thread(&self, peer_hash: &str) -> AppResult<bool> {
        let peer_hash = peer_hash.trim();
        if peer_hash.is_empty() {
            return Ok(false);
        }
        let mut removed = false;
        let canonical_path = self.thread_path(peer_hash)?;
        let path = self
            .existing_thread_path(peer_hash)?
            .unwrap_or(canonical_path);
        if std::fs::symlink_metadata(&path).is_ok() {
            std::fs::remove_file(&path)?;
            removed = true;
        }

        if self.root.is_dir() {
            for (candidate, _) in inventory_thread_files(&self.root)? {
                if candidate == path {
                    continue;
                }
                let Ok(raw) = read_thread_file(&candidate) else {
                    continue;
                };
                let Ok(thread) = serde_json::from_slice::<ConversationThread>(&raw) else {
                    continue;
                };
                if thread.peer_hash.trim().eq_ignore_ascii_case(peer_hash) {
                    std::fs::remove_file(candidate)?;
                    removed = true;
                }
            }
        }

        if removed {
            sync_directory(&self.root)?;
        }

        Ok(removed)
    }

    pub fn mark_read(&self, peer_hash: &str) -> AppResult<()> {
        let mut thread = self.load_thread(peer_hash, None)?;
        thread.unread_count = 0;
        for message in &mut thread.messages {
            if message.incoming {
                message.unread = false;
            }
        }
        self.save_thread(&thread)
    }

    pub fn update_delivery(
        &self,
        peer_hash: &str,
        message_id: &str,
        delivered: bool,
        failed: bool,
    ) -> AppResult<bool> {
        let mut thread = self.load_thread(peer_hash, None)?;
        let mut changed = false;
        for message in &mut thread.messages {
            if message.message_id.as_deref() == Some(message_id) {
                message.delivered = delivered;
                message.failed = failed;
                changed = true;
                break;
            }
        }
        if changed {
            self.save_thread(&thread)?;
        }
        Ok(changed)
    }

    pub fn update_delivery_with_fields(
        &self,
        peer_hash: &str,
        message_id: &str,
        delivered: bool,
        failed: bool,
        fields: std::collections::BTreeMap<String, String>,
    ) -> AppResult<bool> {
        let mut thread = self.load_thread(peer_hash, None)?;
        let mut changed = false;
        for message in &mut thread.messages {
            if message.message_id.as_deref() == Some(message_id) {
                message.delivered = delivered;
                message.failed = failed;
                merge_lxmf_fields_preserving_propagation_handoff(message, fields);
                changed = true;
                break;
            }
        }
        if changed {
            self.save_thread(&thread)?;
        }
        Ok(changed)
    }

    pub fn update_fields(
        &self,
        peer_hash: &str,
        message_id: &str,
        fields: std::collections::BTreeMap<String, String>,
    ) -> AppResult<bool> {
        let mut thread = self.load_thread(peer_hash, None)?;
        let mut changed = false;
        for message in &mut thread.messages {
            if message.message_id.as_deref() == Some(message_id) {
                merge_lxmf_fields_preserving_propagation_handoff(message, fields);
                changed = true;
                break;
            }
        }
        if changed {
            self.save_thread(&thread)?;
        }
        Ok(changed)
    }

    pub fn update_peer_label(&self, peer_hash: &str, peer_label: &str) -> AppResult<()> {
        let mut thread = self.load_thread(peer_hash, Some(peer_label))?;
        thread.peer_label = peer_label.into();
        for message in &mut thread.messages {
            message.peer_label = peer_label.into();
        }
        self.save_thread(&thread)
    }

    pub fn reconcile_pending(&self, pending_ids: &[String], grace_seconds: f64) -> AppResult<bool> {
        let pending = pending_ids.iter().map(String::as_str).collect::<Vec<_>>();
        let now = timestamp_secs();
        let mut changed_any = false;
        for mut thread in self.list_threads()? {
            let mut changed = false;
            for message in &mut thread.messages {
                if message.incoming
                    || message.delivered
                    || message.failed
                    || message.message_id.is_none()
                    || message.transport_method == TransportMethod::Propagated
                    || pending.contains(&message.message_id.as_deref().unwrap_or_default())
                    || now < message.timestamp + grace_seconds
                {
                    continue;
                }
                message.failed = true;
                changed = true;
            }
            if changed {
                self.save_thread(&thread)?;
                changed_any = true;
            }
        }
        Ok(changed_any)
    }

    pub fn reconcile_stale_native_lxmf_direct(
        &self,
        now: f64,
        timeout_seconds: f64,
    ) -> AppResult<Vec<MessageSummary>> {
        let mut stale = Vec::new();
        for mut thread in self.list_threads()? {
            let mut changed = false;
            for message in &mut thread.messages {
                let Some(transition) =
                    direct_lxmf_timeout_transition(message, now, timeout_seconds)
                else {
                    continue;
                };
                message.delivered = false;
                message.failed = false;
                transition.apply_to_fields(&mut message.fields);
                stale.push(message.clone());
                changed = true;
            }
            if changed {
                self.save_thread(&thread)?;
            }
        }
        Ok(stale)
    }

    pub fn reconcile_stale_native_lxmf_propagated(
        &self,
        now: f64,
        timeout_seconds: f64,
    ) -> AppResult<Vec<MessageSummary>> {
        let mut stale = Vec::new();
        for mut thread in self.list_threads()? {
            let mut changed = false;
            for message in &mut thread.messages {
                if !is_stale_native_lxmf_propagated(message, now, timeout_seconds) {
                    continue;
                }
                message.delivered = false;
                message.failed = true;
                message
                    .fields
                    .insert("native_lxmf_state".into(), "failed".into());
                let previous_state = message
                    .fields
                    .get("native_lxmf_propagation_transfer_state")
                    .cloned()
                    .unwrap_or_else(|| "unknown".into());
                let timeout_state = match previous_state.as_str() {
                    "resource_progress" => "resource_timeout",
                    "link_establishing" | "link_timeout" => "link_timeout",
                    "resource_advertise_failed" => "resource_advertise_failed",
                    _ => "router_timeout",
                };
                message.fields.insert(
                    "native_lxmf_propagation_transfer_state".into(),
                    timeout_state.into(),
                );
                message.fields.insert(
                    "native_lxmf_failure_reason".into(),
                    match previous_state.as_str() {
                        "resource_progress" => {
                            "native propagation resource did not report progress completion before timeout"
                        }
                        "link_establishing" | "link_timeout" => {
                            "native propagation link did not establish before timeout"
                        }
                        "resource_advertise_failed" => {
                            "native propagation resource advertisement failed"
                        }
                        _ => "native propagation router transfer did not start before timeout",
                    }
                    .into(),
                );
                message.fields.insert(
                    "native_lxmf_retry_guidance".into(),
                    "verify/select a propagation node, run Prop Diag, then retry when native router transfer is available"
                        .into(),
                );
                message.fields.insert(
                    "native_lxmf_next_action".into(),
                    "retry_propagated_send".into(),
                );
                message.fields.insert(
                    "native_lxmf_retry_after_epoch_secs".into(),
                    format!("{:.3}", now + 30.0),
                );
                message.fields.insert(
                    "native_lxmf_retry_attempt".into(),
                    next_lxmf_retry_attempt(&message.fields).to_string(),
                );
                stale.push(message.clone());
                changed = true;
            }
            if changed {
                self.save_thread(&thread)?;
            }
        }
        Ok(stale)
    }

    fn thread_path(&self, peer_hash: &str) -> AppResult<PathBuf> {
        validate_peer_key(peer_hash)?;
        Ok(self
            .root
            .join(format!("{}.json", portable_peer_file_stem(peer_hash))))
    }

    fn existing_thread_path(&self, peer_hash: &str) -> AppResult<Option<PathBuf>> {
        let canonical = self.thread_path(peer_hash)?;
        match std::fs::symlink_metadata(&canonical) {
            Ok(_) => return Ok(Some(canonical)),
            Err(error) if error.kind() == ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        if peer_hash.is_empty()
            || matches!(peer_hash, "." | "..")
            || peer_hash.contains('/')
            || peer_hash.contains('\\')
        {
            return Ok(None);
        }
        let legacy = self.root.join(format!("{peer_hash}.json"));
        if legacy == canonical {
            return Ok(None);
        }
        match std::fs::symlink_metadata(&legacy) {
            Ok(_) => Ok(Some(legacy)),
            Err(error) if matches!(error.kind(), ErrorKind::NotFound | ErrorKind::InvalidInput) => {
                Ok(None)
            }
            Err(error) => Err(error.into()),
        }
    }

    fn load_thread(
        &self,
        peer_hash: &str,
        peer_label: Option<&str>,
    ) -> AppResult<ConversationThread> {
        let Some(path) = self.existing_thread_path(peer_hash)? else {
            return Ok(empty_thread(peer_hash, peer_label));
        };
        self.load_thread_path(&path, peer_hash, peer_label)
    }

    fn load_thread_path(
        &self,
        path: &Path,
        fallback_peer_hash: &str,
        peer_label: Option<&str>,
    ) -> AppResult<ConversationThread> {
        let raw = read_thread_file(path)?;
        match serde_json::from_slice::<ConversationThread>(&raw) {
            Ok(mut thread) => {
                validate_thread(&thread)?;
                if thread.peer_label.is_empty() {
                    thread.peer_label = peer_label
                        .map(str::to_string)
                        .unwrap_or_else(|| fallback_peer_hash.chars().take(8).collect());
                }
                Ok(thread)
            }
            Err(_) => {
                backup_corrupt_file(&self.root, &raw)?;
                Ok(empty_thread(fallback_peer_hash, peer_label))
            }
        }
    }

    fn save_thread(&self, thread: &ConversationThread) -> AppResult<()> {
        validate_thread(thread)?;
        ensure_real_directory(&self.root)?;
        let canonical_path = self.thread_path(&thread.peer_hash)?;
        let path = self
            .existing_thread_path(&thread.peer_hash)?
            .unwrap_or(canonical_path);
        let raw = serde_json::to_vec_pretty(thread).map_err(|error| {
            AppError::Runtime(format!("message thread serialization failed: {error}"))
        })?;
        if raw.len() as u64 > MESSAGE_STORE_THREAD_MAX_BYTES {
            return Err(AppError::Runtime(format!(
                "message thread exceeds the {MESSAGE_STORE_THREAD_MAX_BYTES} byte limit"
            )));
        }
        if std::fs::symlink_metadata(&path).is_err_and(|error| error.kind() == ErrorKind::NotFound)
        {
            let inventory = inventory_thread_files(&self.root)?;
            let total_bytes = inventory
                .iter()
                .fold(0_u64, |total, (_, bytes)| total.saturating_add(*bytes));
            ensure_thread_capacity(inventory.len(), total_bytes, raw.len() as u64)?;
        }
        publish_thread_bytes(&path, &raw, ThreadPublishMode::Replace)
    }
}

fn empty_thread(peer_hash: &str, peer_label: Option<&str>) -> ConversationThread {
    ConversationThread {
        peer_hash: peer_hash.into(),
        peer_label: peer_label
            .map(str::to_string)
            .unwrap_or_else(|| peer_hash.chars().take(8).collect()),
        messages: Vec::new(),
        unread_count: 0,
        lxmf_reply_ticket: None,
    }
}

fn reply_ticket_from_message(message: &MessageSummary, now: f64) -> Option<NativeLxmfReplyTicket> {
    let expires = message
        .fields
        .get("native_lxmf_reply_ticket_expires")?
        .parse::<f64>()
        .ok()?;
    if expires <= now {
        return None;
    }
    let ticket = hex_to_bytes(message.fields.get("native_lxmf_reply_ticket")?)?;
    if ticket.len() != 16 {
        return None;
    }
    Some(NativeLxmfReplyTicket { ticket, expires })
}

fn remember_reply_ticket_from_message(
    thread: &mut ConversationThread,
    message: &MessageSummary,
    now: f64,
) {
    let Some(ticket) = reply_ticket_from_message(message, now) else {
        return;
    };
    let should_replace = thread
        .lxmf_reply_ticket
        .as_ref()
        .map(|existing| existing.expires < ticket.expires)
        .unwrap_or(true);
    if should_replace {
        thread.lxmf_reply_ticket = Some(ticket);
    }
}

fn hex_to_bytes(value: &str) -> Option<Vec<u8>> {
    let value = value.trim();
    if !value.len().is_multiple_of(2) {
        return None;
    }
    let mut bytes = Vec::with_capacity(value.len() / 2);
    for index in (0..value.len()).step_by(2) {
        bytes.push(u8::from_str_radix(&value[index..index + 2], 16).ok()?);
    }
    Some(bytes)
}

fn merge_lxmf_fields_preserving_propagation_handoff(
    message: &mut MessageSummary,
    mut fields: std::collections::BTreeMap<String, String>,
) {
    let sync_no_payload = fields
        .get("native_lxmf_delivery_evidence_kind")
        .is_some_and(|kind| kind == "propagation_sync_no_payloads");
    let accepted_by_propagation_node = message_has_propagation_handoff_evidence(message);

    if sync_no_payload && accepted_by_propagation_node {
        if let Some(kind) = fields.remove("native_lxmf_delivery_evidence_kind") {
            fields.insert("native_lxmf_propagation_sync_evidence_kind".into(), kind);
        }
        if let Some(detail) = fields.remove("native_lxmf_delivery_evidence_detail") {
            fields.insert("native_lxmf_propagation_sync_detail".into(), detail);
        }
        if let Some(observed_at) = fields.remove("native_lxmf_delivery_evidence_observed_at") {
            fields.insert(
                "native_lxmf_propagation_sync_observed_at".into(),
                observed_at,
            );
        }
        if let Some(state) = fields.remove("native_lxmf_state") {
            fields.insert("native_lxmf_propagation_sync_state".into(), state);
        }
        if let Some(receipt) = fields.remove("native_lxmf_receipt_state") {
            fields.insert("native_lxmf_propagation_sync_receipt_state".into(), receipt);
        }
        if let Some(transfer_state) = fields.remove("native_lxmf_propagation_transfer_state") {
            fields.insert(
                "native_lxmf_propagation_sync_transfer_state".into(),
                transfer_state,
            );
        }
        if let Some(guidance) = fields.remove("native_lxmf_retry_guidance") {
            fields.insert("native_lxmf_propagation_sync_guidance".into(), guidance);
        }
        if !matches!(
            message.fields.get("native_lxmf_state").map(String::as_str),
            Some("propagation_node_accepted" | "propagation_transfer_completed")
        ) {
            message.fields.insert(
                "native_lxmf_state".into(),
                "propagation_node_accepted".into(),
            );
        }
        message
            .fields
            .entry("native_lxmf_receipt_state".into())
            .or_insert_with(|| "propagation_node_accepted_peer_unconfirmed".into());
    }

    if message.delivered
        && matches!(
            fields.get("native_lxmf_state").map(String::as_str),
            Some("transport_proof_received" | "submitted_to_rns_net" | "submitted_unconfirmed")
        )
    {
        fields.remove("native_lxmf_state");
        fields.remove("native_lxmf_proof_state");
        fields.remove("native_lxmf_receipt_state");
        fields.remove("native_lxmf_retry_guidance");
    }

    message.fields.extend(fields);
}

fn message_has_propagation_handoff_evidence(message: &MessageSummary) -> bool {
    matches!(
        message.fields.get("native_lxmf_state").map(String::as_str),
        Some("propagation_node_accepted" | "propagation_transfer_completed")
    ) || matches!(
        message
            .fields
            .get("native_lxmf_receipt_state")
            .map(String::as_str),
        Some("propagation_node_accepted_peer_unconfirmed")
    ) || matches!(
        message
            .fields
            .get("native_lxmf_delivery_evidence_kind")
            .map(String::as_str),
        Some("propagation_node_accepted")
    ) || matches!(
        message
            .fields
            .get("native_lxmf_propagation_transfer_state")
            .map(String::as_str),
        Some("link_packet_sent" | "resource_advertised" | "resource_completed")
    ) || matches!(
        message
            .fields
            .get("native_lxmf_propagation_state")
            .map(String::as_str),
        Some("accepted_by_propagation_node")
    )
}

fn recent_timestamp(thread: &ConversationThread) -> f64 {
    thread
        .messages
        .last()
        .map(|message| message.timestamp)
        .unwrap_or_default()
}

fn validate_peer_key(peer_hash: &str) -> AppResult<()> {
    if peer_hash.len() > MESSAGE_STORE_PEER_KEY_MAX_BYTES || peer_hash.chars().any(char::is_control)
    {
        return Err(AppError::Runtime(
            "message peer key is too long or contains control characters".into(),
        ));
    }
    Ok(())
}

fn portable_peer_file_stem(peer_hash: &str) -> String {
    let portable = !peer_hash.is_empty()
        && !matches!(peer_hash, "." | "..")
        && !peer_hash.ends_with('.')
        && !peer_hash.ends_with(' ')
        && !peer_hash.chars().any(|character| {
            matches!(
                character,
                '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|'
            )
        });
    if portable {
        return peer_hash.to_owned();
    }
    let digest = Sha256::digest(peer_hash.as_bytes());
    format!("peer-{digest:x}")
}

fn validate_message(message: &MessageSummary) -> AppResult<()> {
    const LABEL_MAX_BYTES: usize = 4 * 1024;
    const TITLE_MAX_BYTES: usize = 64 * 1024;
    const CONTENT_MAX_BYTES: usize = 1024 * 1024;
    const MESSAGE_ID_MAX_BYTES: usize = 4 * 1024;
    const FIELD_MAX_ITEMS: usize = 128;
    const FIELD_KEY_MAX_BYTES: usize = 4 * 1024;
    const FIELD_VALUE_MAX_BYTES: usize = 512 * 1024;
    const FIELD_TOTAL_MAX_BYTES: usize = 2 * 1024 * 1024;
    const ATTACHMENT_MAX_ITEMS: usize = 64;
    const ATTACHMENT_NAME_MAX_BYTES: usize = 4 * 1024;
    const ATTACHMENT_PATH_MAX_BYTES: usize = 32 * 1024;

    validate_peer_key(&message.peer_hash)?;
    if message.peer_label.len() > LABEL_MAX_BYTES
        || message.title.len() > TITLE_MAX_BYTES
        || message.content.len() > CONTENT_MAX_BYTES
        || message
            .message_id
            .as_ref()
            .is_some_and(|message_id| message_id.len() > MESSAGE_ID_MAX_BYTES)
        || !message.timestamp.is_finite()
    {
        return Err(AppError::Runtime(
            "message retained scalar exceeds its safety limit".into(),
        ));
    }
    if message.fields.len() > FIELD_MAX_ITEMS {
        return Err(AppError::Runtime(format!(
            "message fields exceed the {FIELD_MAX_ITEMS} item limit"
        )));
    }
    let mut field_bytes = 0_usize;
    for (key, value) in &message.fields {
        if key.len() > FIELD_KEY_MAX_BYTES || value.len() > FIELD_VALUE_MAX_BYTES {
            return Err(AppError::Runtime(
                "message field key or value exceeds its safety limit".into(),
            ));
        }
        field_bytes = field_bytes
            .checked_add(key.len().saturating_add(value.len()))
            .ok_or_else(|| AppError::Runtime("message field byte count overflow".into()))?;
    }
    if field_bytes > FIELD_TOTAL_MAX_BYTES {
        return Err(AppError::Runtime(format!(
            "message fields exceed the {FIELD_TOTAL_MAX_BYTES} byte limit"
        )));
    }
    if message.attachments.len() > ATTACHMENT_MAX_ITEMS
        || message.attachments.iter().any(|attachment| {
            attachment.name.len() > ATTACHMENT_NAME_MAX_BYTES
                || attachment.path.as_ref().is_some_and(|path| {
                    path.as_os_str().to_string_lossy().len() > ATTACHMENT_PATH_MAX_BYTES
                })
        })
    {
        return Err(AppError::Runtime(
            "message attachments exceed their retained item or string limit".into(),
        ));
    }
    if let TransportMethod::Unknown(value) = &message.transport_method {
        if value.len() > 256 {
            return Err(AppError::Runtime(
                "message transport label exceeds the 256 byte limit".into(),
            ));
        }
    }
    Ok(())
}

fn validate_thread(thread: &ConversationThread) -> AppResult<()> {
    validate_peer_key(&thread.peer_hash)?;
    if thread.peer_label.len() > 4 * 1024 {
        return Err(AppError::Runtime(
            "message thread peer label exceeds the 4096 byte limit".into(),
        ));
    }
    if thread.messages.len() > MESSAGE_STORE_THREAD_MAX_MESSAGES {
        return Err(AppError::Runtime(format!(
            "message thread exceeds the {MESSAGE_STORE_THREAD_MAX_MESSAGES} message limit"
        )));
    }
    for message in &thread.messages {
        validate_message(message)?;
    }
    if thread
        .lxmf_reply_ticket
        .as_ref()
        .is_some_and(|ticket| ticket.ticket.len() > 64 || !ticket.expires.is_finite())
    {
        return Err(AppError::Runtime(
            "message thread reply ticket exceeds its retained limit".into(),
        ));
    }
    Ok(())
}

fn ensure_real_directory(path: &Path) -> AppResult<()> {
    std::fs::create_dir_all(path)?;
    if !std::fs::symlink_metadata(path)?.file_type().is_dir() {
        return Err(AppError::Runtime(format!(
            "message storage root must be a directory and not a symbolic link: {}",
            path.display()
        )));
    }
    Ok(())
}

fn inventory_thread_files(root: &Path) -> AppResult<Vec<(PathBuf, u64)>> {
    if !std::fs::symlink_metadata(root)?.file_type().is_dir() {
        return Err(AppError::Runtime(
            "message storage root must be a directory and not a symbolic link".into(),
        ));
    }
    let mut files = Vec::new();
    let mut total_bytes = 0_u64;
    for (scanned, entry) in std::fs::read_dir(root)?.enumerate() {
        if scanned == MESSAGE_STORE_MAX_SCAN_ENTRIES {
            return Err(AppError::Runtime(format!(
                "message discovery exceeds the {MESSAGE_STORE_MAX_SCAN_ENTRIES} entry scan limit"
            )));
        }
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|extension| extension.to_str()) != Some("json")
            || !entry.file_type()?.is_file()
        {
            continue;
        }
        let peer_key = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .ok_or_else(|| {
                AppError::Runtime("message thread filename is not valid UTF-8".into())
            })?;
        if peer_key.len() > 512 || peer_key.chars().any(char::is_control) {
            return Err(AppError::Runtime(
                "message thread filename exceeds its portable admission limit".into(),
            ));
        }
        if files.len() == MESSAGE_STORE_MAX_THREADS {
            return Err(AppError::Runtime(format!(
                "message discovery exceeds the {MESSAGE_STORE_MAX_THREADS} thread limit"
            )));
        }
        let bytes = entry.metadata()?.len();
        if bytes > MESSAGE_STORE_THREAD_MAX_BYTES {
            return Err(AppError::Runtime(format!(
                "message thread exceeds the {MESSAGE_STORE_THREAD_MAX_BYTES} byte limit"
            )));
        }
        total_bytes = total_bytes.saturating_add(bytes);
        if total_bytes > MESSAGE_STORE_MAX_TOTAL_BYTES {
            return Err(AppError::Runtime(format!(
                "message discovery exceeds the {MESSAGE_STORE_MAX_TOTAL_BYTES} retained byte limit"
            )));
        }
        files.push((path, bytes));
    }
    Ok(files)
}

fn ensure_thread_capacity(
    thread_count: usize,
    total_bytes: u64,
    incoming_bytes: u64,
) -> AppResult<()> {
    if thread_count >= MESSAGE_STORE_MAX_THREADS {
        return Err(AppError::Runtime(format!(
            "message store cannot exceed {MESSAGE_STORE_MAX_THREADS} threads"
        )));
    }
    if total_bytes.saturating_add(incoming_bytes) > MESSAGE_STORE_MAX_TOTAL_BYTES {
        return Err(AppError::Runtime(format!(
            "message store cannot exceed {MESSAGE_STORE_MAX_TOTAL_BYTES} retained bytes"
        )));
    }
    Ok(())
}

fn read_thread_file(path: &Path) -> AppResult<Vec<u8>> {
    let path_metadata = std::fs::symlink_metadata(path)?;
    if !path_metadata.file_type().is_file() {
        return Err(AppError::Runtime(
            "message thread must be a regular file and not a symbolic link".into(),
        ));
    }
    if path_metadata.len() > MESSAGE_STORE_THREAD_MAX_BYTES {
        return Err(AppError::Runtime(format!(
            "message thread exceeds the {MESSAGE_STORE_THREAD_MAX_BYTES} byte limit"
        )));
    }
    let file = File::open(path)?;
    let opened_metadata = file.metadata()?;
    if !opened_metadata.is_file() {
        return Err(AppError::Runtime(
            "message thread must open as a regular file".into(),
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;

        if path_metadata.dev() != opened_metadata.dev()
            || path_metadata.ino() != opened_metadata.ino()
        {
            return Err(AppError::Runtime(
                "message thread changed while it was being opened".into(),
            ));
        }
    }
    let mut raw = Vec::with_capacity(path_metadata.len() as usize);
    file.take(MESSAGE_STORE_THREAD_MAX_BYTES.saturating_add(1))
        .read_to_end(&mut raw)?;
    if raw.len() as u64 > MESSAGE_STORE_THREAD_MAX_BYTES {
        return Err(AppError::Runtime(format!(
            "message thread exceeds the {MESSAGE_STORE_THREAD_MAX_BYTES} byte limit"
        )));
    }
    Ok(raw)
}

#[derive(Clone, Copy)]
enum ThreadPublishMode {
    CreateNew,
    Replace,
}

fn publish_thread_bytes(path: &Path, raw: &[u8], mode: ThreadPublishMode) -> AppResult<()> {
    publish_thread_bytes_with(path, raw, mode, || Ok(()))
}

fn publish_thread_bytes_with(
    path: &Path,
    raw: &[u8],
    mode: ThreadPublishMode,
    before_commit: impl FnOnce() -> std::io::Result<()>,
) -> AppResult<()> {
    if raw.len() as u64 > MESSAGE_STORE_THREAD_MAX_BYTES {
        return Err(AppError::Runtime(format!(
            "message thread exceeds the {MESSAGE_STORE_THREAD_MAX_BYTES} byte limit"
        )));
    }
    let parent = path.parent().ok_or_else(|| {
        std::io::Error::new(ErrorKind::InvalidInput, "message destination has no parent")
    })?;
    ensure_real_directory(parent)?;
    match (mode, std::fs::symlink_metadata(path)) {
        (ThreadPublishMode::CreateNew, Ok(_)) => {
            return Err(std::io::Error::new(
                ErrorKind::AlreadyExists,
                "message destination already exists",
            )
            .into());
        }
        (ThreadPublishMode::Replace, Ok(metadata)) if !metadata.file_type().is_file() => {
            return Err(AppError::Runtime(
                "message destination must be a regular file and not a symbolic link".into(),
            ));
        }
        (_, Err(error)) if error.kind() != ErrorKind::NotFound => return Err(error.into()),
        _ => {}
    }
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| {
            std::io::Error::new(
                ErrorKind::InvalidInput,
                "message destination has no safe filename",
            )
        })?;
    let sequence = MESSAGE_STORE_FILE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let temporary = parent.join(format!(
        ".{file_name}.{}.{}.message.tmp",
        std::process::id(),
        sequence
    ));
    let result = (|| -> std::io::Result<()> {
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temporary)?;
        file.write_all(raw)?;
        file.flush()?;
        file.sync_all()?;
        drop(file);
        before_commit()?;
        match mode {
            ThreadPublishMode::CreateNew => {
                std::fs::hard_link(&temporary, path)?;
                sync_directory(parent)?;
                std::fs::remove_file(&temporary)?;
            }
            ThreadPublishMode::Replace => {
                crate::storage::files::atomic_replace(&temporary, path)?;
            }
        }
        sync_directory(parent)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result.map_err(Into::into)
}

fn sync_directory(path: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    std::fs::File::open(path)?.sync_all()?;
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

fn backup_corrupt_file(root: &Path, raw: &[u8]) -> AppResult<()> {
    let sequence = MESSAGE_STORE_FILE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let backup = root.join(format!(
        "{MESSAGE_STORE_CORRUPT_BACKUP_PREFIX}{}.{}.{}{MESSAGE_STORE_CORRUPT_BACKUP_SUFFIX}",
        timestamp_millis(),
        std::process::id(),
        sequence
    ));
    publish_thread_bytes(&backup, raw, ThreadPublishMode::CreateNew)?;
    prune_corrupt_backups(root)?;
    Ok(())
}

fn is_corrupt_backup_name(name: &str) -> bool {
    let Some(body) = name
        .strip_prefix(MESSAGE_STORE_CORRUPT_BACKUP_PREFIX)
        .and_then(|name| name.strip_suffix(MESSAGE_STORE_CORRUPT_BACKUP_SUFFIX))
    else {
        return false;
    };
    let mut parts = body.split('.');
    matches!(
        (parts.next(), parts.next(), parts.next(), parts.next()),
        (Some(timestamp), Some(process), Some(sequence), None)
            if [timestamp, process, sequence]
                .iter()
                .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
    )
}

fn prune_corrupt_backups(root: &Path) -> AppResult<()> {
    let mut backups = Vec::new();
    let mut total_bytes = 0_u64;
    for (scanned, entry) in std::fs::read_dir(root)?.enumerate() {
        if scanned == MESSAGE_STORE_MAX_SCAN_ENTRIES {
            return Err(AppError::Runtime(format!(
                "message backup discovery exceeds the {MESSAGE_STORE_MAX_SCAN_ENTRIES} entry scan limit"
            )));
        }
        let entry = entry?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        if !is_corrupt_backup_name(name) || !entry.file_type()?.is_file() {
            continue;
        }
        let bytes = entry.metadata()?.len();
        total_bytes = total_bytes.saturating_add(bytes);
        backups.push((name.to_owned(), entry.path(), bytes));
    }
    backups.sort_by(|left, right| left.0.cmp(&right.0));
    let mut retained = backups.len();
    let mut removed = false;
    for (_, path, bytes) in backups {
        if retained <= MESSAGE_STORE_CORRUPT_BACKUP_MAX_FILES
            && total_bytes <= MESSAGE_STORE_CORRUPT_BACKUP_MAX_TOTAL_BYTES
        {
            break;
        }
        std::fs::remove_file(path)?;
        retained = retained.saturating_sub(1);
        total_bytes = total_bytes.saturating_sub(bytes);
        removed = true;
    }
    if removed {
        sync_directory(root)?;
    }
    Ok(())
}

fn timestamp_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

fn timestamp_secs() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64()
}

fn next_lxmf_retry_attempt(fields: &BTreeMap<String, String>) -> u32 {
    fields
        .get("native_lxmf_retry_attempt")
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(0)
        .saturating_add(1)
}

fn is_stale_native_lxmf_propagated(
    message: &MessageSummary,
    now: f64,
    timeout_seconds: f64,
) -> bool {
    if message.incoming
        || message.delivered
        || message.failed
        || message.transport_method != TransportMethod::Propagated
        || message.message_id.is_none()
    {
        return false;
    }
    if message.fields.get("native_lxmf_state").map(String::as_str) != Some("queued_for_propagation")
    {
        return false;
    }
    if !matches!(
        message
            .fields
            .get("native_lxmf_propagation_transfer_state")
            .map(String::as_str),
        Some("router_deferred")
            | Some("link_establishing")
            | Some("link_timeout")
            | Some("resource_progress")
            | Some("resource_advertise_failed")
    ) {
        return false;
    }
    let submitted_at = message
        .fields
        .get("native_lxmf_submitted_at")
        .and_then(|value| value.parse::<f64>().ok())
        .unwrap_or(message.timestamp);
    now >= submitted_at + timeout_seconds
}

#[cfg(test)]
mod publication_tests {
    use super::{publish_thread_bytes_with, ThreadPublishMode, MESSAGE_STORE_FILE_SEQUENCE};

    fn fixture(name: &str) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!(
            "omenbrowser-message-publication-{name}-{}-{}",
            std::process::id(),
            MESSAGE_STORE_FILE_SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("publication fixture");
        root
    }

    #[test]
    fn precommit_failure_preserves_prior_thread_and_cleans_stage() {
        let root = fixture("replace-fault");
        let target = root.join("peer.json");
        std::fs::write(&target, b"previous thread").expect("previous thread");

        publish_thread_bytes_with(&target, b"replacement", ThreadPublishMode::Replace, || {
            Err(std::io::Error::other("injected precommit failure"))
        })
        .expect_err("publication must fail");

        assert_eq!(
            std::fs::read(&target).expect("preserved thread"),
            b"previous thread"
        );
        assert_eq!(
            std::fs::read_dir(root)
                .expect("fixture entries")
                .filter_map(Result::ok)
                .count(),
            1
        );
    }
}
