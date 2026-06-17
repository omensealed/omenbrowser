use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::error::AppResult;
use crate::messaging::{
    direct_lxmf_timeout_transition, ConversationThread, MessageSummary, NativeLxmfReplyTicket,
    TransportMethod,
};

#[derive(Clone, Debug)]
pub struct MessageStore {
    root: PathBuf,
}

impl MessageStore {
    pub fn new(root: PathBuf) -> AppResult<Self> {
        std::fs::create_dir_all(&root)?;
        Ok(Self { root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn import_missing_threads_from(&self, source_root: &Path) -> AppResult<usize> {
        if !source_root.is_dir() {
            return Ok(0);
        }

        std::fs::create_dir_all(&self.root)?;
        let mut imported = 0usize;
        for entry in std::fs::read_dir(source_root)? {
            let path = entry?.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
                continue;
            }
            let Some(file_name) = path.file_name() else {
                continue;
            };
            let target = self.root.join(file_name);
            if target.exists() {
                continue;
            }
            if serde_json::from_str::<ConversationThread>(&std::fs::read_to_string(&path)?).is_err()
            {
                continue;
            }
            std::fs::copy(&path, target)?;
            imported += 1;
        }
        Ok(imported)
    }

    pub fn append(&self, mut message: MessageSummary) -> AppResult<MessageSummary> {
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
            thread.unread_count += 1;
        }
        thread.messages.push(message.clone());
        remember_reply_ticket_from_message(&mut thread, &message, timestamp_secs());
        thread.messages.sort_by(|left, right| {
            left.timestamp
                .total_cmp(&right.timestamp)
                .then_with(|| left.message_id.cmp(&right.message_id))
                .then_with(|| left.content.cmp(&right.content))
        });
        self.save_thread(&thread)?;
        Ok(message)
    }

    pub fn list_threads(&self) -> AppResult<Vec<ConversationThread>> {
        let mut threads = Vec::new();
        for entry in std::fs::read_dir(&self.root)? {
            let path = entry?.path();
            if path.extension().and_then(|ext| ext.to_str()) == Some("json") {
                let peer_hash = path
                    .file_stem()
                    .and_then(|stem| stem.to_str())
                    .unwrap_or_default();
                threads.push(self.load_thread(peer_hash, None)?);
            }
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
        let path = self.thread_path(peer_hash);
        if path.exists() {
            std::fs::remove_file(&path)?;
            removed = true;
        }

        if self.root.is_dir() {
            for entry in std::fs::read_dir(&self.root)? {
                let path = entry?.path();
                if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
                    continue;
                }
                if path == self.thread_path(peer_hash) {
                    continue;
                }
                let Ok(raw) = std::fs::read_to_string(&path) else {
                    continue;
                };
                let Ok(thread) = serde_json::from_str::<ConversationThread>(&raw) else {
                    continue;
                };
                if thread.peer_hash.trim().eq_ignore_ascii_case(peer_hash) {
                    std::fs::remove_file(path)?;
                    removed = true;
                }
            }
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

    fn thread_path(&self, peer_hash: &str) -> PathBuf {
        self.root.join(format!("{peer_hash}.json"))
    }

    fn load_thread(
        &self,
        peer_hash: &str,
        peer_label: Option<&str>,
    ) -> AppResult<ConversationThread> {
        let path = self.thread_path(peer_hash);
        if !path.exists() {
            return Ok(ConversationThread {
                peer_hash: peer_hash.into(),
                peer_label: peer_label
                    .map(str::to_string)
                    .unwrap_or_else(|| peer_hash.chars().take(8).collect()),
                messages: Vec::new(),
                unread_count: 0,
                lxmf_reply_ticket: None,
            });
        }
        let raw = std::fs::read_to_string(&path)?;
        match serde_json::from_str::<ConversationThread>(&raw) {
            Ok(mut thread) => {
                if thread.peer_label.is_empty() {
                    thread.peer_label = peer_label
                        .map(str::to_string)
                        .unwrap_or_else(|| peer_hash.chars().take(8).collect());
                }
                Ok(thread)
            }
            Err(_) => {
                backup_corrupt_file(&path)?;
                Ok(ConversationThread {
                    peer_hash: peer_hash.into(),
                    peer_label: peer_label
                        .map(str::to_string)
                        .unwrap_or_else(|| peer_hash.chars().take(8).collect()),
                    messages: Vec::new(),
                    unread_count: 0,
                    lxmf_reply_ticket: None,
                })
            }
        }
    }

    fn save_thread(&self, thread: &ConversationThread) -> AppResult<()> {
        std::fs::create_dir_all(&self.root)?;
        let path = self.thread_path(&thread.peer_hash);
        let temp = path.with_extension(format!("json.tmp.{}", std::process::id()));
        std::fs::write(
            &temp,
            serde_json::to_vec_pretty(thread).expect("thread serializes"),
        )?;
        std::fs::rename(temp, path)?;
        Ok(())
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

fn backup_corrupt_file(path: &Path) -> AppResult<()> {
    let backup = path.with_file_name(format!(
        "{}.corrupt.{}.bak",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("thread.json"),
        timestamp_millis()
    ));
    std::fs::copy(path, backup)?;
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
