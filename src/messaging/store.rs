use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::io::{ErrorKind, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use sha2::{Digest, Sha256};

use crate::error::{AppError, AppResult};
use crate::messaging::{
    direct_lxmf_timeout_transition, ConversationThread, MessageSummary, NativeLxmfReplyTicket,
    OutboundOperationIdentity, TransportMethod,
};
use crate::runtime::{LxmfHistoryPage, LxmfHistoryRecord, RuntimeLxmfDeliveryState};

pub const MESSAGE_STORE_THREAD_MAX_BYTES: u64 = 8 * 1024 * 1024;
pub const MESSAGE_STORE_THREAD_MAX_MESSAGES: usize = 4096;
pub const MESSAGE_STORE_MAX_THREADS: usize = 256;
pub const MESSAGE_STORE_MAX_TOTAL_BYTES: u64 = 64 * 1024 * 1024;
pub const MESSAGE_STORE_MAX_SCAN_ENTRIES: usize = 4096;
pub const MESSAGE_STORE_MAX_PUBLICATION_ARTIFACTS: usize = 4096;
pub const MESSAGE_STORE_PEER_KEY_MAX_BYTES: usize = 256;
pub const MESSAGE_STORE_CORRUPT_BACKUP_MAX_FILES: usize = 4;
pub const MESSAGE_STORE_CORRUPT_BACKUP_MAX_TOTAL_BYTES: u64 =
    MESSAGE_STORE_CORRUPT_BACKUP_MAX_FILES as u64 * MESSAGE_STORE_THREAD_MAX_BYTES;

const MESSAGE_STORE_CORRUPT_BACKUP_PREFIX: &str = "omen-message.corrupt.";
const MESSAGE_STORE_CORRUPT_BACKUP_SUFFIX: &str = ".bak";
const MESSAGE_STORE_STAGE_SUFFIX: &str = ".message.tmp";
const MESSAGE_STORE_LEASE_SUFFIX: &str = ".message.tmp.lock";
static MESSAGE_STORE_FILE_SEQUENCE: AtomicU64 = AtomicU64::new(1);
static ACTIVE_MESSAGE_PUBLICATION_LEASES: LazyLock<Mutex<BTreeSet<PathBuf>>> =
    LazyLock::new(|| Mutex::new(BTreeSet::new()));

struct ActiveMessagePublicationLease {
    path: PathBuf,
}

impl ActiveMessagePublicationLease {
    fn register(path: PathBuf) -> AppResult<Self> {
        let mut active = ACTIVE_MESSAGE_PUBLICATION_LEASES.lock().map_err(|_| {
            AppError::Runtime("message publication lease registry is poisoned".into())
        })?;
        if !active.insert(path.clone()) {
            return Err(AppError::Runtime(
                "message publication lease is already active in this process".into(),
            ));
        }
        Ok(Self { path })
    }
}

impl Drop for ActiveMessagePublicationLease {
    fn drop(&mut self) {
        if let Ok(mut active) = ACTIVE_MESSAGE_PUBLICATION_LEASES.lock() {
            active.remove(&self.path);
        }
    }
}

fn message_publication_lease_is_active(path: &Path) -> bool {
    ACTIVE_MESSAGE_PUBLICATION_LEASES
        .lock()
        .map(|active| active.contains(path))
        .unwrap_or(true)
}

#[derive(Clone, Debug)]
pub struct MessageStore {
    root: PathBuf,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MessageStageRecoveryReport {
    pub removed_stages: usize,
    pub removed_leases: usize,
    pub active_stages: usize,
    pub unleased_stages: usize,
    pub unsafe_artifacts: usize,
    pub lock_errors: usize,
}

pub(crate) struct SdkDeliveryStoreUpdate {
    pub seq_no: u64,
    pub terminal: bool,
    pub delivered: bool,
    pub failed: bool,
    pub fields: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct LxmfHistoryReconcileReport {
    pub matched: usize,
    pub imported_inbound: usize,
    pub updated_outbound: usize,
    pub skipped: usize,
    pub changed_messages: Vec<MessageSummary>,
}

impl MessageStore {
    pub fn new(root: PathBuf) -> AppResult<Self> {
        ensure_real_directory(&root)?;
        recover_abandoned_thread_stages(&root)?;
        Ok(Self { root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn recover_abandoned_publications(&self) -> AppResult<MessageStageRecoveryReport> {
        recover_abandoned_thread_stages(&self.root)
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

    pub(crate) fn list_threads_read_only(&self) -> AppResult<Vec<ConversationThread>> {
        let mut threads = Vec::new();
        for (path, _) in inventory_thread_files(&self.root)? {
            let raw = read_thread_file(&path)?;
            let thread = serde_json::from_slice::<ConversationThread>(&raw).map_err(|error| {
                AppError::Runtime(format!(
                    "read-only message thread parse failed for {}: {error}",
                    path.file_name()
                        .and_then(|name| name.to_str())
                        .unwrap_or("<non-UTF-8 filename>")
                ))
            })?;
            validate_thread(&thread)?;
            threads.push(thread);
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

    pub(crate) fn update_sdk_delivery_with_fields(
        &self,
        peer_hash: &str,
        message_id: &str,
        update: SdkDeliveryStoreUpdate,
    ) -> AppResult<bool> {
        let mut thread = self.load_thread(peer_hash, None)?;
        let mut changed = false;
        for message in &mut thread.messages {
            if message.message_id.as_deref() != Some(message_id) {
                continue;
            }
            let current_seq = message
                .fields
                .get("native_lxmf_sdk_seq_no")
                .and_then(|value| value.parse::<u64>().ok());
            if current_seq.is_some_and(|current| current >= update.seq_no) {
                break;
            }
            let current_terminal = message
                .fields
                .get("native_lxmf_sdk_terminal")
                .is_some_and(|value| value == "true");
            if (current_terminal || message.delivered || message.failed) && !update.terminal {
                break;
            }
            message.delivered = update.delivered;
            message.failed = update.failed;
            merge_lxmf_fields_preserving_propagation_handoff(message, update.fields);
            changed = true;
            break;
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

    pub fn reconcile_expired_lxmf(&self, now_ms: u64) -> AppResult<Vec<MessageSummary>> {
        let mut expired = Vec::new();
        for mut thread in self.list_threads()? {
            let mut changed = false;
            for message in &mut thread.messages {
                if message.incoming
                    || message.delivered
                    || message.failed
                    || message
                        .fields
                        .get("native_lxmf_sdk_terminal")
                        .is_some_and(|terminal| terminal == "true")
                {
                    continue;
                }
                let Some(operation) = OutboundOperationIdentity::from_message_at(message, now_ms)
                else {
                    continue;
                };
                if operation.remaining_ttl_ms_at(now_ms).is_some() {
                    continue;
                }
                message.delivered = false;
                message.failed = true;
                message
                    .fields
                    .insert("native_lxmf_sdk_state".into(), "expired".into());
                message
                    .fields
                    .insert("native_lxmf_sdk_terminal".into(), "true".into());
                message
                    .fields
                    .insert("native_lxmf_state".into(), "expired".into());
                message.fields.insert(
                    "native_lxmf_failure_reason".into(),
                    "local outbound TTL elapsed before authoritative delivery".into(),
                );
                message.fields.insert(
                    "native_lxmf_retry_guidance".into(),
                    "LXMF delivery expired; an explicit retry creates a new bounded operation"
                        .into(),
                );
                message.fields.insert(
                    "native_lxmf_next_action".into(),
                    "prepare_new_send_after_expiry".into(),
                );
                message
                    .fields
                    .insert("native_lxmf_expired_at_ms".into(), now_ms.to_string());
                expired.push(message.clone());
                changed = true;
            }
            if changed {
                self.save_thread(&thread)?;
            }
        }
        Ok(expired)
    }

    pub fn reconcile_sdk_history(
        &self,
        page: LxmfHistoryPage,
        reconciled_at_ms: u64,
    ) -> AppResult<LxmfHistoryReconcileReport> {
        let page = page.validate()?;
        let mut threads = self
            .list_threads()?
            .into_iter()
            .map(|thread| (thread.peer_hash.to_ascii_lowercase(), thread))
            .collect::<BTreeMap<_, _>>();
        let mut changed_peers = std::collections::BTreeSet::new();
        let mut report = LxmfHistoryReconcileReport::default();

        for record in page.messages {
            let Some((incoming, peer_hash)) = history_record_peer(&record) else {
                report.skipped = report.skipped.saturating_add(1);
                continue;
            };
            if record.message_id.is_empty() || record.message_id.len() > 4 * 1024 {
                report.skipped = report.skipped.saturating_add(1);
                continue;
            }

            let existing_peer = threads.iter().find_map(|(key, thread)| {
                thread
                    .messages
                    .iter()
                    .any(|message| {
                        message
                            .message_id
                            .as_deref()
                            .is_some_and(|id| id.eq_ignore_ascii_case(&record.message_id))
                    })
                    .then(|| key.clone())
            });
            if let Some(existing_peer) = existing_peer {
                let thread = threads
                    .get_mut(&existing_peer)
                    .expect("history message peer came from current bounded thread map");
                let message = thread
                    .messages
                    .iter_mut()
                    .find(|message| {
                        message
                            .message_id
                            .as_deref()
                            .is_some_and(|id| id.eq_ignore_ascii_case(&record.message_id))
                    })
                    .expect("history message came from current bounded thread map");
                report.matched = report.matched.saturating_add(1);
                if apply_history_metadata(
                    message,
                    record.receipt_status.as_deref(),
                    reconciled_at_ms,
                ) {
                    if !message.incoming {
                        report.updated_outbound = report.updated_outbound.saturating_add(1);
                    }
                    report.changed_messages.push(message.clone());
                    changed_peers.insert(existing_peer);
                }
                continue;
            }

            if !incoming {
                // OMEN's durable outbox is authoritative. Never invent a locally
                // authored message merely because another SDK store contains it.
                report.skipped = report.skipped.saturating_add(1);
                continue;
            }
            let mut message =
                history_record_to_inbound(record, peer_hash.clone(), reconciled_at_ms);
            if validate_message(&message).is_err() {
                report.skipped = report.skipped.saturating_add(1);
                continue;
            }
            let peer_key = peer_hash.to_ascii_lowercase();
            if !threads.contains_key(&peer_key) && threads.len() >= MESSAGE_STORE_MAX_THREADS {
                report.skipped = report.skipped.saturating_add(1);
                continue;
            }
            let thread = threads
                .entry(peer_key.clone())
                .or_insert_with(|| empty_thread(&peer_hash, None));
            if thread.messages.len() >= MESSAGE_STORE_THREAD_MAX_MESSAGES {
                report.skipped = report.skipped.saturating_add(1);
                continue;
            }
            message.peer_label = thread.peer_label.clone();
            thread.unread_count = thread.unread_count.saturating_add(1);
            thread.messages.push(message.clone());
            thread.messages.sort_by(|left, right| {
                left.timestamp
                    .total_cmp(&right.timestamp)
                    .then_with(|| left.message_id.cmp(&right.message_id))
            });
            let encoded_len = serde_json::to_vec(thread)
                .map(|encoded| encoded.len() as u64)
                .unwrap_or(u64::MAX);
            if encoded_len > MESSAGE_STORE_THREAD_MAX_BYTES {
                thread.messages.retain(|candidate| {
                    candidate.message_id.as_deref() != message.message_id.as_deref()
                });
                thread.unread_count = thread.unread_count.saturating_sub(1);
                report.skipped = report.skipped.saturating_add(1);
                continue;
            }
            report.imported_inbound = report.imported_inbound.saturating_add(1);
            report.changed_messages.push(message);
            changed_peers.insert(peer_key);
        }

        for peer in changed_peers {
            if let Some(thread) = threads.get(&peer) {
                self.save_thread(thread)?;
            }
        }
        Ok(report)
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
        crate::private_fs::repair_private_file(path)?;
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

fn history_record_peer(record: &LxmfHistoryRecord) -> Option<(bool, String)> {
    let direction = record.direction.trim().to_ascii_lowercase();
    let (incoming, peer) = match direction.as_str() {
        "in" | "inbound" | "received" => (true, record.source.trim()),
        "out" | "outbound" | "sent" => (false, record.destination.trim()),
        _ => return None,
    };
    (!peer.is_empty()).then(|| (incoming, peer.to_string()))
}

fn history_record_to_inbound(
    record: LxmfHistoryRecord,
    peer_hash: String,
    reconciled_at_ms: u64,
) -> MessageSummary {
    let receipt_status = record.receipt_status.clone();
    let mut message = MessageSummary {
        peer_label: peer_hash.chars().take(8).collect(),
        peer_hash,
        title: record.title,
        content: record.content,
        timestamp: record.timestamp as f64,
        transport_method: TransportMethod::Unknown("sdk_history".into()),
        delivered: false,
        failed: false,
        incoming: true,
        unread: true,
        message_id: Some(record.message_id.clone()),
        fields: BTreeMap::new(),
        attachments: Vec::new(),
    };
    apply_history_metadata(&mut message, receipt_status.as_deref(), reconciled_at_ms);
    message
}

fn apply_history_metadata(
    message: &mut MessageSummary,
    receipt_status: Option<&str>,
    reconciled_at_ms: u64,
) -> bool {
    let mut before_fields = message.fields.clone();
    before_fields.remove("native_lxmf_sdk_history_reconciled_at_ms");
    let before = (message.delivered, message.failed, before_fields);
    message
        .fields
        .insert("native_lxmf_sdk_history_seen".into(), "true".into());
    if let Some(receipt_status) = receipt_status {
        let receipt_status = receipt_status.trim();
        if receipt_status.len() <= 4 * 1024 {
            message.fields.insert(
                "native_lxmf_sdk_history_receipt_status".into(),
                receipt_status.into(),
            );
            if !message.incoming {
                if let Some(state) = history_receipt_state(receipt_status) {
                    let current_terminal = message.delivered
                        || message.failed
                        || message
                            .fields
                            .get("native_lxmf_sdk_terminal")
                            .is_some_and(|terminal| terminal == "true");
                    if !current_terminal || state.is_terminal() {
                        message.delivered = state == RuntimeLxmfDeliveryState::Delivered;
                        message.failed = state.is_failure_terminal();
                        message
                            .fields
                            .insert("native_lxmf_sdk_state".into(), state.as_str().into());
                        message.fields.insert(
                            "native_lxmf_sdk_terminal".into(),
                            state.is_terminal().to_string(),
                        );
                        message
                            .fields
                            .insert("native_lxmf_state".into(), state.as_str().into());
                    }
                }
            }
        }
    }
    let mut after_fields = message.fields.clone();
    after_fields.remove("native_lxmf_sdk_history_reconciled_at_ms");
    let changed = before != (message.delivered, message.failed, after_fields);
    if changed {
        message.fields.insert(
            "native_lxmf_sdk_history_reconciled_at_ms".into(),
            reconciled_at_ms.to_string(),
        );
    }
    changed
}

fn history_receipt_state(value: &str) -> Option<RuntimeLxmfDeliveryState> {
    let prefix = value
        .split_once(':')
        .map_or(value, |(prefix, _)| prefix)
        .trim()
        .to_ascii_lowercase();
    match prefix.as_str() {
        "queued" => Some(RuntimeLxmfDeliveryState::Queued),
        "sending" | "dispatching" => Some(RuntimeLxmfDeliveryState::Dispatching),
        "in_flight" | "in-flight" => Some(RuntimeLxmfDeliveryState::InFlight),
        "sent" => Some(RuntimeLxmfDeliveryState::Sent),
        "delivered" => Some(RuntimeLxmfDeliveryState::Delivered),
        "failed" => Some(RuntimeLxmfDeliveryState::Failed),
        "cancelled" | "canceled" => Some(RuntimeLxmfDeliveryState::Cancelled),
        "expired" => Some(RuntimeLxmfDeliveryState::Expired),
        "rejected" => Some(RuntimeLxmfDeliveryState::Rejected),
        _ => None,
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
    if value.len() % 2 != 0 {
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
    crate::private_fs::ensure_private_dir(path)?;
    if !std::fs::symlink_metadata(path)?.file_type().is_dir() {
        return Err(AppError::Runtime(format!(
            "message storage root must be a directory and not a symbolic link: {}",
            path.display()
        )));
    }
    Ok(())
}

#[derive(Default)]
struct ThreadPublicationArtifacts {
    stage: Option<PathBuf>,
    lease: Option<PathBuf>,
}

#[derive(Clone, Copy)]
enum ThreadPublicationArtifactKind {
    Stage,
    Lease,
}

fn thread_publication_artifact(
    name: &std::ffi::OsStr,
) -> Option<(&str, ThreadPublicationArtifactKind)> {
    let name = name.to_str()?;
    if let Some(key) = name.strip_suffix(MESSAGE_STORE_LEASE_SUFFIX) {
        return valid_thread_publication_key(key)
            .then_some((key, ThreadPublicationArtifactKind::Lease));
    }
    name.strip_suffix(MESSAGE_STORE_STAGE_SUFFIX)
        .filter(|key| valid_thread_publication_key(key))
        .map(|key| (key, ThreadPublicationArtifactKind::Stage))
}

fn valid_thread_publication_key(key: &str) -> bool {
    if !key.starts_with('.') || key.len() > 1024 || key.chars().any(char::is_control) {
        return false;
    }
    let Some((destination_and_pid, sequence)) = key.rsplit_once('.') else {
        return false;
    };
    let Some((destination, process_id)) = destination_and_pid.rsplit_once('.') else {
        return false;
    };
    !destination.is_empty()
        && (destination.ends_with(".json") || destination.ends_with(".bak"))
        && process_id.parse::<u32>().is_ok()
        && sequence.parse::<u64>().is_ok()
}

fn recover_abandoned_thread_stages(root: &Path) -> AppResult<MessageStageRecoveryReport> {
    if !std::fs::symlink_metadata(root)?.file_type().is_dir() {
        return Err(AppError::Runtime(
            "message storage root must be a directory and not a symbolic link".into(),
        ));
    }
    let mut artifacts = BTreeMap::<String, ThreadPublicationArtifacts>::new();
    let mut normal_entries = 0usize;
    let mut artifact_entries = 0usize;
    for entry in std::fs::read_dir(root)? {
        let entry = entry?;
        let file_name = entry.file_name();
        let Some((key, kind)) = thread_publication_artifact(&file_name) else {
            normal_entries = normal_entries.saturating_add(1);
            if normal_entries > MESSAGE_STORE_MAX_SCAN_ENTRIES {
                return Err(AppError::Runtime(format!(
                    "message discovery exceeds the {MESSAGE_STORE_MAX_SCAN_ENTRIES} entry scan limit"
                )));
            }
            continue;
        };
        artifact_entries = artifact_entries.saturating_add(1);
        if artifact_entries > MESSAGE_STORE_MAX_PUBLICATION_ARTIFACTS {
            return Err(AppError::Runtime(format!(
                "message publication recovery exceeds the {MESSAGE_STORE_MAX_PUBLICATION_ARTIFACTS} artifact limit"
            )));
        }
        let pair = artifacts.entry(key.to_owned()).or_default();
        match kind {
            ThreadPublicationArtifactKind::Stage => pair.stage = Some(entry.path()),
            ThreadPublicationArtifactKind::Lease => pair.lease = Some(entry.path()),
        }
    }

    let mut report = MessageStageRecoveryReport::default();
    let mut removed_any = false;
    for pair in artifacts.into_values() {
        let Some(lease_path) = pair.lease else {
            if pair.stage.is_some() {
                report.unleased_stages = report.unleased_stages.saturating_add(1);
            }
            continue;
        };
        if message_publication_lease_is_active(&lease_path) {
            if pair.stage.is_some() {
                report.active_stages = report.active_stages.saturating_add(1);
            }
            continue;
        }
        let lease_metadata = match std::fs::symlink_metadata(&lease_path) {
            Ok(metadata) if metadata.file_type().is_file() => metadata,
            Ok(_) => {
                report.unsafe_artifacts = report.unsafe_artifacts.saturating_add(1);
                continue;
            }
            Err(error) if error.kind() == ErrorKind::NotFound => {
                if pair.stage.is_some() {
                    report.unleased_stages = report.unleased_stages.saturating_add(1);
                }
                continue;
            }
            Err(_) => {
                report.unsafe_artifacts = report.unsafe_artifacts.saturating_add(1);
                continue;
            }
        };
        if lease_metadata.len() != 0 {
            report.unsafe_artifacts = report.unsafe_artifacts.saturating_add(1);
            continue;
        }
        let lease = match std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&lease_path)
        {
            Ok(lease) => lease,
            Err(error) if error.kind() == ErrorKind::NotFound => {
                if pair.stage.is_some() {
                    report.unleased_stages = report.unleased_stages.saturating_add(1);
                }
                continue;
            }
            Err(_) => {
                report.unsafe_artifacts = report.unsafe_artifacts.saturating_add(1);
                continue;
            }
        };
        match fs4::FileExt::try_lock(&lease) {
            Ok(()) => {}
            Err(fs4::TryLockError::WouldBlock) => {
                if pair.stage.is_some() {
                    report.active_stages = report.active_stages.saturating_add(1);
                }
                continue;
            }
            Err(fs4::TryLockError::Error(_)) => {
                report.lock_errors = report.lock_errors.saturating_add(1);
                continue;
            }
        }

        if let Some(stage_path) = pair.stage {
            match std::fs::symlink_metadata(&stage_path) {
                Ok(metadata) if metadata.file_type().is_file() => {
                    std::fs::remove_file(&stage_path)?;
                    report.removed_stages = report.removed_stages.saturating_add(1);
                    removed_any = true;
                }
                Ok(_) => {
                    report.unsafe_artifacts = report.unsafe_artifacts.saturating_add(1);
                    continue;
                }
                Err(error) if error.kind() == ErrorKind::NotFound => {}
                Err(_) => {
                    report.unsafe_artifacts = report.unsafe_artifacts.saturating_add(1);
                    continue;
                }
            }
        }
        drop(lease);
        match std::fs::remove_file(&lease_path) {
            Ok(()) => {
                report.removed_leases = report.removed_leases.saturating_add(1);
                removed_any = true;
            }
            Err(error) if error.kind() == ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    if removed_any {
        sync_directory(root)?;
    }
    Ok(report)
}

fn inventory_thread_files(root: &Path) -> AppResult<Vec<(PathBuf, u64)>> {
    if !std::fs::symlink_metadata(root)?.file_type().is_dir() {
        return Err(AppError::Runtime(
            "message storage root must be a directory and not a symbolic link".into(),
        ));
    }
    let mut files = Vec::new();
    let mut total_bytes = 0_u64;
    let mut scanned = 0usize;
    let mut artifact_entries = 0usize;
    for entry in std::fs::read_dir(root)? {
        let entry = entry?;
        if thread_publication_artifact(&entry.file_name()).is_some() {
            artifact_entries = artifact_entries.saturating_add(1);
            if artifact_entries > MESSAGE_STORE_MAX_PUBLICATION_ARTIFACTS {
                return Err(AppError::Runtime(format!(
                    "message discovery exceeds the {MESSAGE_STORE_MAX_PUBLICATION_ARTIFACTS} publication artifact limit"
                )));
            }
            continue;
        }
        if scanned == MESSAGE_STORE_MAX_SCAN_ENTRIES {
            return Err(AppError::Runtime(format!(
                "message discovery exceeds the {MESSAGE_STORE_MAX_SCAN_ENTRIES} entry scan limit"
            )));
        }
        scanned = scanned.saturating_add(1);
        let path = entry.path();
        if path.extension().and_then(|extension| extension.to_str()) != Some("json")
            || !entry.file_type()?.is_file()
        {
            continue;
        }
        crate::private_fs::repair_private_file(&path)?;
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ThreadPublishBoundary {
    StageCreated,
    StageWritten,
    StageSynced,
    DestinationCommitted,
    DirectorySynced,
}

fn publish_thread_bytes(path: &Path, raw: &[u8], mode: ThreadPublishMode) -> AppResult<()> {
    publish_thread_bytes_with(path, raw, mode, |_| Ok(()))
}

fn publish_thread_bytes_with(
    path: &Path,
    raw: &[u8],
    mode: ThreadPublishMode,
    mut boundary: impl FnMut(ThreadPublishBoundary) -> std::io::Result<()>,
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
    let lease_path = temporary.with_file_name(format!(
        "{}.lock",
        temporary
            .file_name()
            .and_then(|name| name.to_str())
            .expect("generated message stage filename is valid UTF-8")
    ));
    let _active_lease = ActiveMessagePublicationLease::register(lease_path.clone())?;
    let mut lease_options = std::fs::OpenOptions::new();
    lease_options.read(true).write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        lease_options.mode(0o600);
    }
    let lease = lease_options.open(&lease_path)?;
    if let Err(error) = fs4::FileExt::lock(&lease) {
        drop(lease);
        let _ = std::fs::remove_file(&lease_path);
        return Err(error.into());
    }
    let result = (|| -> std::io::Result<()> {
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temporary)?;
        boundary(ThreadPublishBoundary::StageCreated)?;
        file.write_all(raw)?;
        boundary(ThreadPublishBoundary::StageWritten)?;
        file.flush()?;
        file.sync_all()?;
        drop(file);
        boundary(ThreadPublishBoundary::StageSynced)?;
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
        boundary(ThreadPublishBoundary::DestinationCommitted)?;
        sync_directory(parent)?;
        boundary(ThreadPublishBoundary::DirectorySynced)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    drop(lease);
    let lease_cleanup = match std::fs::remove_file(&lease_path) {
        Ok(()) => sync_directory(parent),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    };
    match result {
        Err(error) => Err(error.into()),
        Ok(()) => lease_cleanup.map_err(Into::into),
    }
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
        crate::private_fs::repair_private_file(&entry.path())?;
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
    use super::{
        publish_thread_bytes_with, recover_abandoned_thread_stages, MessageStore,
        ThreadPublishBoundary, ThreadPublishMode, MESSAGE_STORE_FILE_SEQUENCE,
        MESSAGE_STORE_MAX_PUBLICATION_ARTIFACTS,
    };

    const PUBLICATION_CHILD_TEST: &str =
        "messaging::store::publication_tests::process_kill_thread_publication_boundary_child";
    const PUBLICATION_ROOT_ENV: &str = "OMEN_TEST_MESSAGE_PUBLISH_ROOT";
    const PUBLICATION_READY_ENV: &str = "OMEN_TEST_MESSAGE_PUBLISH_READY";
    const PUBLICATION_BOUNDARY_ENV: &str = "OMEN_TEST_MESSAGE_PUBLISH_BOUNDARY";

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

    fn thread_bytes(include_current: bool) -> Vec<u8> {
        let mut messages = vec![serde_json::json!({
            "peer_hash": "peer",
            "peer_label": "Peer",
            "title": "Old",
            "content": "Body",
            "timestamp": 10.0,
            "transport_method": "direct",
            "delivered": false,
            "failed": false,
            "incoming": false,
            "unread": false,
            "message_id": "lxmf-old",
            "fields": {
                "native_lxmf_state": "submitted_unconfirmed",
                "native_lxmf_proof_state": "proof_not_observed",
                "native_lxmf_packet_hash": "0303030303030303030303030303030303030303030303030303030303030303"
            },
            "attachments": []
        })];
        if include_current {
            messages.push(serde_json::json!({
                "peer_hash": "peer",
                "peer_label": "Peer",
                "title": "Current",
                "content": "Body",
                "timestamp": 60.0,
                "transport_method": "direct",
                "delivered": false,
                "failed": false,
                "incoming": false,
                "unread": false,
                "message_id": "lxmf-current",
                "fields": {
                    "native_lxmf_state": "submitted_to_clean_reticulum",
                    "native_lxmf_proof_state": "waiting_for_transport_receipt",
                    "native_lxmf_packet_hash": "0404040404040404040404040404040404040404040404040404040404040404"
                },
                "attachments": []
            }));
        }
        serde_json::to_vec_pretty(&serde_json::json!({
            "peer_hash": "peer",
            "peer_label": "Peer",
            "messages": messages,
            "unread_count": 0
        }))
        .expect("thread fixture bytes")
    }

    #[test]
    fn read_only_thread_listing_preserves_order_without_corruption_side_effects() {
        let root = fixture("read-only-list");
        let store = MessageStore::new(root.clone()).expect("message store");
        std::fs::write(root.join("peer.json"), thread_bytes(true)).expect("valid thread");
        let threads = store.list_threads_read_only().expect("read-only threads");
        assert_eq!(threads.len(), 1);
        assert_eq!(threads[0].messages.len(), 2);
        assert_eq!(
            std::fs::read_dir(&root)
                .expect("thread entries")
                .filter_map(Result::ok)
                .count(),
            1
        );

        std::fs::write(root.join("peer.json"), b"{malformed").expect("corrupt thread");
        assert!(store.list_threads_read_only().is_err());
        let names = std::fs::read_dir(&root)
            .expect("thread entries")
            .filter_map(Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        assert_eq!(names, vec!["peer.json"]);
        std::fs::remove_dir_all(root).expect("remove fixture");
    }

    fn staged_files(root: &std::path::Path) -> usize {
        std::fs::read_dir(root)
            .expect("fixture entries")
            .filter_map(Result::ok)
            .filter(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .ends_with(".message.tmp")
            })
            .count()
    }

    fn lease_files(root: &std::path::Path) -> usize {
        std::fs::read_dir(root)
            .expect("fixture entries")
            .filter_map(Result::ok)
            .filter(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .ends_with(".message.tmp.lock")
            })
            .count()
    }

    fn stage_pair(
        root: &std::path::Path,
        sequence: usize,
    ) -> (std::path::PathBuf, std::path::PathBuf) {
        let stage = root.join(format!(".peer.json.4242.{sequence}.message.tmp"));
        let lease = stage.with_file_name(format!(
            "{}.lock",
            stage.file_name().and_then(|name| name.to_str()).unwrap()
        ));
        (stage, lease)
    }

    fn publication_artifact_stats(root: &std::path::Path) -> (usize, u64) {
        std::fs::read_dir(root)
            .expect("publication artifact entries")
            .filter_map(Result::ok)
            .filter(|entry| {
                let name = entry.file_name();
                let name = name.to_string_lossy();
                name.ends_with(".message.tmp") || name.ends_with(".message.tmp.lock")
            })
            .fold((0usize, 0u64), |(items, bytes), entry| {
                (
                    items.saturating_add(1),
                    bytes.saturating_add(entry.metadata().expect("artifact metadata").len()),
                )
            })
    }

    fn spawn_publication_child_at_boundary(
        root: &std::path::Path,
        ready: &std::path::Path,
        mode: &str,
    ) -> std::process::Child {
        let mut child = std::process::Command::new(
            std::env::current_exe().expect("current unit-test executable"),
        )
        .args([
            "--exact",
            PUBLICATION_CHILD_TEST,
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(PUBLICATION_ROOT_ENV, root)
        .env(PUBLICATION_READY_ENV, ready)
        .env(PUBLICATION_BOUNDARY_ENV, mode)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn message publication boundary child");
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            if ready.is_file() {
                break;
            }
            if let Some(status) = child.try_wait().expect("poll publication child") {
                let output = child.wait_with_output().expect("reap publication child");
                panic!(
                    "publication child exited before {mode} boundary: {status}\nstdout:\n{}\nstderr:\n{}",
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr)
                );
            }
            if std::time::Instant::now() >= deadline {
                let _ = child.kill();
                let output = child.wait_with_output().expect("reap timed-out child");
                panic!(
                    "publication child timed out at {mode}\nstdout:\n{}\nstderr:\n{}",
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr)
                );
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        child
    }

    fn terminate_publication_child_at_boundary(
        root: &std::path::Path,
        ready: &std::path::Path,
        mode: &str,
    ) {
        let mut child = spawn_publication_child_at_boundary(root, ready, mode);
        child.kill().expect("kill publication boundary child");
        let output = child
            .wait_with_output()
            .expect("reap killed publication child");
        assert!(!output.status.success());
    }

    #[test]
    fn precommit_failure_preserves_prior_thread_and_cleans_stage() {
        let root = fixture("replace-fault");
        let target = root.join("peer.json");
        std::fs::write(&target, b"previous thread").expect("previous thread");

        publish_thread_bytes_with(
            &target,
            b"replacement",
            ThreadPublishMode::Replace,
            |boundary| {
                if boundary == ThreadPublishBoundary::StageSynced {
                    Err(std::io::Error::other("injected precommit failure"))
                } else {
                    Ok(())
                }
            },
        )
        .expect_err("publication must fail");

        assert_eq!(
            std::fs::read(&target).expect("preserved thread"),
            b"previous thread"
        );
        assert_eq!(
            std::fs::read_dir(&root)
                .expect("fixture entries")
                .filter_map(Result::ok)
                .count(),
            1
        );
        std::fs::remove_dir_all(root).expect("remove precommit fixture");
    }

    #[test]
    fn every_replace_fault_boundary_leaves_one_complete_thread_and_cleans_stage() {
        for boundary in [
            ThreadPublishBoundary::StageCreated,
            ThreadPublishBoundary::StageWritten,
            ThreadPublishBoundary::StageSynced,
            ThreadPublishBoundary::DestinationCommitted,
            ThreadPublishBoundary::DirectorySynced,
        ] {
            let root = fixture(&format!("replace-{boundary:?}"));
            let target = root.join("peer.json");
            let old = thread_bytes(false);
            let new = thread_bytes(true);
            std::fs::write(&target, &old).expect("seed committed old thread");

            publish_thread_bytes_with(&target, &new, ThreadPublishMode::Replace, |observed| {
                if observed == boundary {
                    Err(std::io::Error::other("injected publication boundary"))
                } else {
                    Ok(())
                }
            })
            .expect_err("injected boundary must fail publication");

            let committed = std::fs::read(&target).expect("complete committed thread");
            let expected_new = matches!(
                boundary,
                ThreadPublishBoundary::DestinationCommitted
                    | ThreadPublishBoundary::DirectorySynced
            );
            assert_eq!(
                committed.as_slice(),
                if expected_new {
                    new.as_slice()
                } else {
                    old.as_slice()
                }
            );
            assert_eq!(staged_files(&root), 0);
            assert_eq!(lease_files(&root), 0);
            let store = MessageStore::new(root.clone()).expect("reopen boundary store");
            let thread = store.get_thread("peer").expect("parse committed thread");
            assert_eq!(thread.messages.len(), if expected_new { 2 } else { 1 });
            assert_eq!(
                thread
                    .messages
                    .last()
                    .and_then(|message| message.message_id.as_deref()),
                Some(if expected_new {
                    "lxmf-current"
                } else {
                    "lxmf-old"
                })
            );
            std::fs::remove_dir_all(root).expect("remove boundary fixture");
        }
    }

    #[test]
    fn process_kill_at_replace_boundaries_preserves_old_or_new_thread() {
        for (mode, expected_new) in [("stage_synced", false), ("destination_committed", true)] {
            let root = fixture(&format!("process-kill-{mode}"));
            let target = root.join("peer.json");
            std::fs::write(&target, thread_bytes(false)).expect("seed process old thread");
            let ready = root.join("boundary.ready");
            terminate_publication_child_at_boundary(&root, &ready, mode);

            assert_eq!(staged_files(&root), usize::from(!expected_new));
            assert_eq!(lease_files(&root), 1);

            let store = MessageStore::new(root.clone()).expect("reopen killed publication store");
            let thread = store
                .get_thread("peer")
                .expect("parse killed publication thread");
            assert_eq!(thread.messages.len(), if expected_new { 2 } else { 1 });
            assert_eq!(
                thread
                    .messages
                    .last()
                    .and_then(|message| message.message_id.as_deref()),
                Some(if expected_new {
                    "lxmf-current"
                } else {
                    "lxmf-old"
                })
            );
            assert_eq!(staged_files(&root), 0);
            assert_eq!(lease_files(&root), 0);
            std::fs::remove_dir_all(root).expect("remove process publication fixture");
        }
    }

    #[test]
    fn abandoned_leased_stage_is_removed_but_unleased_legacy_stage_is_retained() {
        let root = fixture("stage-recovery");
        std::fs::write(root.join("peer.json"), thread_bytes(false)).expect("seed thread");
        let (abandoned_stage, abandoned_lease) = stage_pair(&root, 1);
        std::fs::write(&abandoned_stage, thread_bytes(true)).expect("abandoned stage");
        std::fs::File::create(&abandoned_lease).expect("abandoned lease");
        let (legacy_stage, _) = stage_pair(&root, 2);
        std::fs::write(&legacy_stage, thread_bytes(true)).expect("legacy unleased stage");

        let report = recover_abandoned_thread_stages(&root).expect("recover abandoned stage");
        assert_eq!(report.removed_stages, 1);
        assert_eq!(report.removed_leases, 1);
        assert_eq!(report.unleased_stages, 1);
        assert!(!abandoned_stage.exists());
        assert!(!abandoned_lease.exists());
        assert!(legacy_stage.is_file());

        let store = MessageStore::new(root.clone()).expect("open recovered store");
        assert_eq!(
            store.list_threads().expect("list recovered thread").len(),
            1
        );
        assert_eq!(
            store
                .get_thread("peer")
                .expect("parse old thread")
                .messages
                .len(),
            1
        );
        std::fs::remove_dir_all(root).expect("remove recovery fixture");
    }

    #[test]
    fn recovery_never_removes_a_stage_with_a_live_lease() {
        let root = fixture("live-stage");
        std::fs::write(root.join("peer.json"), thread_bytes(false)).expect("seed live thread");
        let ready = root.join("live-boundary.ready");
        let mut child = spawn_publication_child_at_boundary(&root, &ready, "stage_synced");

        let report = recover_abandoned_thread_stages(&root).expect("inspect live stage");
        child.kill().expect("kill live publication child");
        let output = child
            .wait_with_output()
            .expect("reap live publication child");
        assert!(!output.status.success());
        assert_eq!(report.active_stages, 1);
        assert_eq!(report.removed_stages, 0);
        assert_eq!(staged_files(&root), 1);
        assert_eq!(lease_files(&root), 1);

        let report = recover_abandoned_thread_stages(&root).expect("recover released stage");
        assert_eq!(report.removed_stages, 1);
        assert_eq!(report.removed_leases, 1);
        assert_eq!(staged_files(&root), 0);
        assert_eq!(lease_files(&root), 0);
        std::fs::remove_dir_all(root).expect("remove live-stage fixture");
    }

    #[test]
    fn recovery_never_removes_a_same_process_active_publisher() {
        let root = fixture("same-process-live-stage");
        let target = root.join("peer.json");
        std::fs::write(&target, thread_bytes(false)).expect("seed same-process thread");
        let replacement = thread_bytes(true);
        let writer_target = target.clone();
        let writer_replacement = replacement.clone();
        let (ready_tx, ready_rx) = std::sync::mpsc::sync_channel(0);
        let (release_tx, release_rx) = std::sync::mpsc::sync_channel(0);
        let writer = std::thread::spawn(move || {
            publish_thread_bytes_with(
                &writer_target,
                &writer_replacement,
                ThreadPublishMode::Replace,
                |boundary| {
                    if boundary == ThreadPublishBoundary::StageSynced {
                        ready_tx
                            .send(())
                            .map_err(|_| std::io::Error::other("recovery reader dropped"))?;
                        release_rx
                            .recv()
                            .map_err(|_| std::io::Error::other("recovery release dropped"))?;
                    }
                    Ok(())
                },
            )
        });
        ready_rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("same-process publisher reached stage sync");

        let report = recover_abandoned_thread_stages(&root).expect("inspect active publisher");
        release_tx.send(()).expect("release same-process publisher");
        writer
            .join()
            .expect("join same-process publisher")
            .expect("same-process publication");
        assert_eq!(report.active_stages, 1);
        assert_eq!(report.removed_stages, 0);
        assert_eq!(
            std::fs::read(&target).expect("read replacement"),
            replacement
        );
        assert_eq!(staged_files(&root), 0);
        assert_eq!(lease_files(&root), 0);
        std::fs::remove_dir_all(root).expect("remove same-process fixture");
    }

    #[test]
    fn recovery_retains_unrecognized_names_and_malformed_leases() {
        let root = fixture("unsafe-stage");
        let unrelated = root.join("notes.message.tmp");
        std::fs::write(&unrelated, b"user material").expect("unrecognized temporary file");
        let (stage, lease) = stage_pair(&root, 1);
        std::fs::write(&stage, thread_bytes(true)).expect("unsafe stage");
        std::fs::write(&lease, b"not a zero-byte lease").expect("malformed lease");

        let report = recover_abandoned_thread_stages(&root).expect("inspect unsafe artifacts");
        assert_eq!(report.unsafe_artifacts, 1);
        assert_eq!(report.removed_stages, 0);
        assert_eq!(std::fs::read(&unrelated).unwrap(), b"user material");
        assert!(stage.is_file());
        assert!(lease.is_file());
        std::fs::remove_dir_all(root).expect("remove unsafe-stage fixture");
    }

    #[test]
    fn publication_artifact_inventory_is_bounded() {
        let root = fixture("stage-inventory-limit");
        for sequence in 0..=MESSAGE_STORE_MAX_PUBLICATION_ARTIFACTS {
            let (stage, _) = stage_pair(&root, sequence);
            std::fs::File::create(stage).expect("bounded artifact fixture");
        }
        let error = recover_abandoned_thread_stages(&root)
            .expect_err("publication artifact saturation must be rejected");
        assert!(error.to_string().contains("artifact limit"));
        std::fs::remove_dir_all(root).expect("remove artifact-limit fixture");
    }

    #[test]
    fn exact_publication_artifact_ceiling_recovers_abandoned_and_retains_live_writer() {
        const TOTAL_PAIRS: usize = MESSAGE_STORE_MAX_PUBLICATION_ARTIFACTS / 2;
        const ABANDONED_PAIRS: usize = TOTAL_PAIRS - 1;

        assert_eq!(TOTAL_PAIRS * 2, MESSAGE_STORE_MAX_PUBLICATION_ARTIFACTS);
        let root = fixture("stage-exact-ceiling");
        let target = root.join("peer.json");
        let old = thread_bytes(false);
        let replacement = thread_bytes(true);
        std::fs::write(&target, &old).expect("seed ceiling thread");
        for sequence in 0..ABANDONED_PAIRS {
            let stage = root.join(format!(".ceiling-peer.json.0.{sequence}.message.tmp"));
            let lease = stage.with_file_name(format!(
                "{}.lock",
                stage.file_name().and_then(|name| name.to_str()).unwrap()
            ));
            std::fs::write(stage, &replacement).expect("ceiling abandoned stage");
            std::fs::File::create(lease).expect("ceiling abandoned lease");
        }

        let ready = root.join("ceiling-live.ready");
        let mut child = spawn_publication_child_at_boundary(&root, &ready, "stage_synced");
        let (artifacts_before, bytes_before) = publication_artifact_stats(&root);
        assert_eq!(artifacts_before, MESSAGE_STORE_MAX_PUBLICATION_ARTIFACTS);
        assert_eq!(bytes_before, replacement.len() as u64 * TOTAL_PAIRS as u64);

        let first_started = std::time::Instant::now();
        let first_result = recover_abandoned_thread_stages(&root);
        let first_recovery = first_started.elapsed();
        child.kill().expect("kill ceiling live publisher");
        let output = child
            .wait_with_output()
            .expect("reap ceiling live publisher");
        assert!(!output.status.success());
        let first_report = first_result.expect("recover ceiling abandoned stages");
        let (artifacts_retained, bytes_retained) = publication_artifact_stats(&root);
        assert_eq!(first_report.removed_stages, ABANDONED_PAIRS);
        assert_eq!(first_report.removed_leases, ABANDONED_PAIRS);
        assert_eq!(first_report.active_stages, 1);
        assert_eq!(first_report.unleased_stages, 0);
        assert_eq!(first_report.unsafe_artifacts, 0);
        assert_eq!(first_report.lock_errors, 0);
        assert_eq!(artifacts_retained, 2);
        assert_eq!(bytes_retained, replacement.len() as u64);

        std::fs::remove_file(&ready).expect("remove ceiling readiness marker");
        let final_started = std::time::Instant::now();
        let final_report =
            recover_abandoned_thread_stages(&root).expect("recover released ceiling live stage");
        let final_recovery = final_started.elapsed();
        let (artifacts_after, bytes_after) = publication_artifact_stats(&root);
        assert_eq!(final_report.removed_stages, 1);
        assert_eq!(final_report.removed_leases, 1);
        assert_eq!(artifacts_after, 0);
        assert_eq!(bytes_after, 0);
        assert_eq!(std::fs::read(&target).expect("read ceiling target"), old);
        let store = MessageStore::new(root.clone()).expect("open ceiling store");
        assert_eq!(
            store
                .get_thread("peer")
                .expect("parse ceiling thread")
                .messages
                .len(),
            1
        );
        eprintln!(
            "message_publication_ceiling artifacts_before={artifacts_before} bytes_before={bytes_before} abandoned_pairs={ABANDONED_PAIRS} active_retained={} first_recovery_us={} artifacts_retained={artifacts_retained} bytes_retained={bytes_retained} final_recovery_us={} artifacts_after={artifacts_after} bytes_after={bytes_after}",
            first_report.active_stages,
            first_recovery.as_micros(),
            final_recovery.as_micros()
        );
        std::fs::remove_dir_all(root).expect("remove ceiling fixture");
    }

    #[test]
    fn repeated_precommit_crashes_recover_in_one_bounded_pass() {
        const CRASHES: usize = 16;

        let root = fixture("repeated-process-kill");
        let target = root.join("peer.json");
        let old = thread_bytes(false);
        let replacement = thread_bytes(true);
        std::fs::write(&target, &old).expect("seed repeated-crash old thread");
        let soak_started = std::time::Instant::now();

        for crash in 0..CRASHES {
            let ready = root.join(format!("boundary-{crash}.ready"));
            terminate_publication_child_at_boundary(&root, &ready, "stage_synced");
            std::fs::remove_file(&ready).expect("remove repeated-crash marker");
            assert_eq!(
                std::fs::read(&target).expect("read repeated-crash target"),
                old
            );
        }

        let (artifacts_before, bytes_before) = publication_artifact_stats(&root);
        assert_eq!(artifacts_before, CRASHES * 2);
        assert_eq!(bytes_before, replacement.len() as u64 * CRASHES as u64);
        let recovery_started = std::time::Instant::now();
        let report = recover_abandoned_thread_stages(&root).expect("recover repeated crashes");
        let recovery = recovery_started.elapsed();
        let (artifacts_after, bytes_after) = publication_artifact_stats(&root);

        assert_eq!(report.removed_stages, CRASHES);
        assert_eq!(report.removed_leases, CRASHES);
        assert_eq!(report.active_stages, 0);
        assert_eq!(report.unleased_stages, 0);
        assert_eq!(report.unsafe_artifacts, 0);
        assert_eq!(report.lock_errors, 0);
        assert_eq!(artifacts_after, 0);
        assert_eq!(bytes_after, 0);
        let store = MessageStore::new(root.clone()).expect("open repeated-crash store");
        assert_eq!(
            store
                .get_thread("peer")
                .expect("parse repeated-crash old thread")
                .messages
                .len(),
            1
        );
        eprintln!(
            "message_publication_recovery_soak crashes={CRASHES} artifacts_before={artifacts_before} bytes_before={bytes_before} recovery_us={} artifacts_after={artifacts_after} bytes_after={bytes_after} total_ms={}",
            recovery.as_micros(),
            soak_started.elapsed().as_millis()
        );
        std::fs::remove_dir_all(root).expect("remove repeated-crash fixture");
    }

    #[test]
    #[ignore = "helper is terminated by the message publication boundary regression"]
    fn process_kill_thread_publication_boundary_child() {
        use std::io::Write as _;

        let Some(root) = std::env::var_os(PUBLICATION_ROOT_ENV).map(std::path::PathBuf::from)
        else {
            return;
        };
        let ready = std::path::PathBuf::from(
            std::env::var_os(PUBLICATION_READY_ENV).expect("publication boundary ready marker"),
        );
        let selected = match std::env::var(PUBLICATION_BOUNDARY_ENV).as_deref() {
            Ok("stage_synced") => ThreadPublishBoundary::StageSynced,
            Ok("destination_committed") => ThreadPublishBoundary::DestinationCommitted,
            value => panic!("unsupported publication boundary: {value:?}"),
        };

        publish_thread_bytes_with(
            &root.join("peer.json"),
            &thread_bytes(true),
            ThreadPublishMode::Replace,
            |observed| {
                if observed == selected {
                    let mut marker = std::fs::OpenOptions::new()
                        .create_new(true)
                        .write(true)
                        .open(&ready)?;
                    marker.write_all(format!("{selected:?}\n").as_bytes())?;
                    marker.sync_all()?;
                    drop(marker);
                    loop {
                        std::thread::park();
                    }
                }
                Ok(())
            },
        )
        .expect("parent terminates child at selected boundary");
    }
}
