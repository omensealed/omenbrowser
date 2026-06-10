use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use crate::directory::DirectoryService;
use crate::error::AppResult;
use crate::messaging::{
    ConversationThread, DeliveryMode, MessageEnvelope, MessageStore, MessageSummary,
};
use crate::runtime::{
    LxmfDeliveryEvidence, LxmfDeliveryEvidenceKind, NetworkRuntime, OutboundDeliveryState,
    OutboundStatus,
};

#[derive(Clone)]
pub struct MessagingService {
    runtime: Arc<dyn NetworkRuntime>,
    store: MessageStore,
    directory: Option<DirectoryService>,
}

impl std::fmt::Debug for MessagingService {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("MessagingService")
            .field("store", &self.store)
            .field("directory", &self.directory)
            .finish_non_exhaustive()
    }
}

impl MessagingService {
    pub fn new(runtime: Arc<dyn NetworkRuntime>, store: MessageStore) -> Self {
        Self {
            runtime,
            store,
            directory: None,
        }
    }

    pub fn with_directory(
        runtime: Arc<dyn NetworkRuntime>,
        store: MessageStore,
        directory: DirectoryService,
    ) -> Self {
        Self {
            runtime,
            store,
            directory: Some(directory),
        }
    }

    pub fn store(&self) -> &MessageStore {
        &self.store
    }

    pub async fn sync_runtime_messages(&self) -> AppResult<Vec<MessageSummary>> {
        let messages = self.runtime.list_messages().await?;
        let mut stored = Vec::new();
        for message in messages {
            stored.push(self.ingest_runtime_message(message)?);
        }
        Ok(stored)
    }

    pub fn ingest_runtime_message(&self, mut message: MessageSummary) -> AppResult<MessageSummary> {
        message.peer_label = self.label_for(&message.peer_hash, Some(&message.peer_label))?;
        self.store.append(message)
    }

    pub fn threads(&self) -> AppResult<Vec<ConversationThread>> {
        self.store.list_threads()
    }

    pub fn conversation(&self, peer_hash: &str) -> AppResult<ConversationThread> {
        self.store.get_thread(peer_hash)
    }

    pub fn mark_read(&self, peer_hash: &str) -> AppResult<()> {
        self.store.mark_read(peer_hash)
    }

    pub fn delete_conversation(&self, peer_hash: &str) -> AppResult<bool> {
        self.store.delete_thread(peer_hash)
    }

    pub async fn add_contact(&self, peer_hash: &str, label: &str) -> AppResult<()> {
        self.store.update_peer_label(peer_hash, label)?;
        self.runtime.create_contact(peer_hash, label).await
    }

    pub fn prepare_conversation(
        &self,
        peer_hash: &str,
        label: Option<&str>,
    ) -> AppResult<ConversationThread> {
        let label = self.label_for(peer_hash, label)?;
        self.store.ensure_thread(peer_hash, Some(&label))
    }

    pub async fn compose(
        &self,
        peer_hash: &str,
        title: &str,
        content: &str,
        delivery_mode: DeliveryMode,
        include_ticket: bool,
        attachments: Vec<PathBuf>,
    ) -> AppResult<MessageSummary> {
        let envelope = MessageEnvelope {
            peer_hash: peer_hash.into(),
            title: title.into(),
            body: content.into(),
            delivery_mode,
            include_ticket,
            attachments,
        };
        let mut sent = self.runtime.send_message(envelope).await?;
        sent.peer_label = self.label_for(peer_hash, Some(&sent.peer_label))?;
        sent.unread = false;
        self.store.append(sent)
    }

    pub fn update_outbound_status(&self, status: &OutboundStatus) -> AppResult<bool> {
        let Some(message_id) = status.message_id.as_deref() else {
            return Ok(false);
        };
        let mut fields = BTreeMap::new();
        fields.insert(
            "native_lxmf_state".into(),
            match status.state {
                OutboundDeliveryState::SubmittedToRuntime => "submitted_to_runtime",
                OutboundDeliveryState::SubmittedToRnsNet => "submitted_to_rns_net",
                OutboundDeliveryState::Delivered => "delivered",
                OutboundDeliveryState::Failed => "failed",
                OutboundDeliveryState::Unknown => "unknown",
            }
            .into(),
        );
        if let Some(evidence) = &status.evidence {
            fields.insert("native_lxmf_evidence".into(), evidence.clone());
        }
        fields.insert("native_lxmf_packet_hash".into(), message_id.into());
        fields.insert(
            "native_lxmf_proof_state".into(),
            match status.state {
                OutboundDeliveryState::Delivered => "proof_received",
                OutboundDeliveryState::Failed => "failed",
                OutboundDeliveryState::SubmittedToRuntime
                | OutboundDeliveryState::SubmittedToRnsNet => "waiting_for_packet_proof",
                OutboundDeliveryState::Unknown => "unknown",
            }
            .into(),
        );
        fields.insert(
            "native_lxmf_retry_guidance".into(),
            match status.state {
                OutboundDeliveryState::Delivered => {
                    "LXMF router delivered callback received; no retry needed"
                }
                OutboundDeliveryState::Failed => {
                    "send failed; inspect peer/path and retry after the blocker is fixed"
                }
                OutboundDeliveryState::SubmittedToRuntime
                | OutboundDeliveryState::SubmittedToRnsNet => {
                    "wait for packet proof; if no proof arrives, inspect peer/path and retry"
                }
                OutboundDeliveryState::Unknown => {
                    "delivery state unknown; inspect peer/path before retrying"
                }
            }
            .into(),
        );
        if let Some(submitted_at) = status
            .evidence
            .as_deref()
            .and_then(|evidence| extract_evidence_value(evidence, "submitted_at"))
        {
            fields.insert("native_lxmf_submitted_at".into(), submitted_at.into());
        }
        if let Some(transfer_state) = status
            .evidence
            .as_deref()
            .and_then(|evidence| extract_evidence_value(evidence, "propagation_transfer_state"))
        {
            fields.insert(
                "native_lxmf_propagation_transfer_state".into(),
                transfer_state.into(),
            );
            fields.insert(
                "native_lxmf_propagation_state".into(),
                lxmf_propagation_state_for_transfer(transfer_state).into(),
            );
            fields.insert(
                "native_lxmf_receipt_state".into(),
                lxmf_propagation_receipt_state_for_transfer(transfer_state).into(),
            );
            fields.insert(
                "native_lxmf_proof_state".into(),
                lxmf_propagation_proof_state_for_transfer(transfer_state).into(),
            );
            fields.insert(
                "native_lxmf_state".into(),
                lxmf_state_for_propagation_transfer(transfer_state).into(),
            );
            fields.insert(
                "native_lxmf_retry_guidance".into(),
                lxmf_retry_guidance_for_propagation_transfer(transfer_state).into(),
            );
        }
        if let Some(transfer_state) = status
            .evidence
            .as_deref()
            .and_then(|evidence| extract_evidence_value(evidence, "direct_transfer_state"))
        {
            fields.insert(
                "native_lxmf_direct_transfer_state".into(),
                transfer_state.into(),
            );
            fields.insert(
                "native_lxmf_receipt_state".into(),
                lxmf_direct_receipt_state_for_transfer(transfer_state).into(),
            );
            fields.insert(
                "native_lxmf_proof_state".into(),
                lxmf_direct_proof_state_for_transfer(transfer_state).into(),
            );
            fields.insert(
                "native_lxmf_state".into(),
                lxmf_state_for_direct_transfer(transfer_state).into(),
            );
            fields.insert(
                "native_lxmf_retry_guidance".into(),
                lxmf_retry_guidance_for_direct_transfer(transfer_state).into(),
            );
        }
        if let Some(link_id) = status
            .evidence
            .as_deref()
            .and_then(|evidence| extract_evidence_value(evidence, "direct_link_id"))
        {
            fields.insert("native_lxmf_direct_link_id".into(), link_id.into());
        }
        if let Some(link_id) = status
            .evidence
            .as_deref()
            .and_then(|evidence| extract_evidence_value(evidence, "propagation_link_id"))
        {
            fields.insert("native_lxmf_propagation_link_id".into(), link_id.into());
        }
        if let Some(node) = status
            .evidence
            .as_deref()
            .and_then(|evidence| extract_evidence_value(evidence, "propagation_node"))
        {
            fields.insert("native_lxmf_propagation_node".into(), node.into());
        }
        if let Some(received) = status
            .evidence
            .as_deref()
            .and_then(|evidence| extract_evidence_value(evidence, "resource_received"))
        {
            fields.insert("native_lxmf_resource_received".into(), received.into());
        }
        if let Some(total) = status
            .evidence
            .as_deref()
            .and_then(|evidence| extract_evidence_value(evidence, "resource_total"))
        {
            fields.insert("native_lxmf_resource_total".into(), total.into());
        }
        if let Some(reason) = status
            .evidence
            .as_deref()
            .and_then(|evidence| extract_evidence_value(evidence, "failure_reason"))
        {
            fields.insert("native_lxmf_failure_reason".into(), reason.into());
        }
        if let Some(rtt) = status.rtt {
            fields.insert("native_lxmf_rtt".into(), format!("{rtt:.3}"));
        }
        self.store.update_delivery_with_fields(
            &status.peer_hash,
            message_id,
            status.delivered,
            status.failed,
            fields,
        )
    }

    pub fn apply_lxmf_delivery_evidence(&self, evidence: &LxmfDeliveryEvidence) -> AppResult<bool> {
        let Some(message_id) = evidence.message_id.as_deref() else {
            return Ok(false);
        };
        let fields = lxmf_delivery_evidence_fields(evidence);
        match evidence.kind {
            LxmfDeliveryEvidenceKind::LxmfRouterDelivered => self
                .store
                .update_delivery_with_fields(&evidence.peer_hash, message_id, true, false, fields),
            LxmfDeliveryEvidenceKind::LxmfRouterFailed => self.store.update_delivery_with_fields(
                &evidence.peer_hash,
                message_id,
                false,
                true,
                fields,
            ),
            _ => self
                .store
                .update_fields(&evidence.peer_hash, message_id, fields),
        }
    }

    pub fn reconcile_pending(&self, pending_ids: &[String]) -> AppResult<bool> {
        self.store.reconcile_pending(pending_ids, 10.0)
    }

    pub fn reconcile_stale_native_lxmf_direct(
        &self,
        now: f64,
        timeout_seconds: f64,
    ) -> AppResult<Vec<MessageSummary>> {
        self.store
            .reconcile_stale_native_lxmf_direct(now, timeout_seconds)
    }

    pub fn reconcile_stale_native_lxmf_propagated(
        &self,
        now: f64,
        timeout_seconds: f64,
    ) -> AppResult<Vec<MessageSummary>> {
        self.store
            .reconcile_stale_native_lxmf_propagated(now, timeout_seconds)
    }

    fn label_for(&self, peer_hash: &str, fallback: Option<&str>) -> AppResult<String> {
        if let Some(directory) = &self.directory {
            if let Some(entry) = directory.find(peer_hash) {
                if !entry.display_name.is_empty() && entry.display_name != peer_hash {
                    return Ok(entry.display_name);
                }
            }
        }
        let stored = self.store.get_thread(peer_hash)?;
        if !stored.peer_label.is_empty()
            && stored.peer_label != peer_hash.chars().take(8).collect::<String>()
        {
            return Ok(stored.peer_label);
        }
        Ok(fallback
            .map(str::to_string)
            .unwrap_or_else(|| peer_hash.chars().take(8).collect()))
    }
}

fn extract_evidence_value<'a>(evidence: &'a str, key: &str) -> Option<&'a str> {
    let prefix = format!("{key}:");
    evidence
        .split(';')
        .find_map(|part| part.strip_prefix(prefix.as_str()))
}

fn lxmf_propagation_state_for_transfer(transfer_state: &str) -> &'static str {
    match transfer_state {
        "link_packet_sent" | "resource_advertised" | "resource_completed" => {
            "accepted_by_propagation_node"
        }
        "link_packet_failed"
        | "resource_failed"
        | "resource_advertise_failed"
        | "resource_timeout"
        | "router_timeout"
        | "link_timeout" => "failed",
        "resource_progress" => "in_progress",
        "router_deferred" => "deferred",
        _ => "queued",
    }
}

fn lxmf_state_for_propagation_transfer(transfer_state: &str) -> &'static str {
    match transfer_state {
        "link_packet_sent" | "resource_advertised" | "resource_completed" => {
            "propagation_transfer_completed"
        }
        "link_packet_failed"
        | "resource_failed"
        | "resource_advertise_failed"
        | "resource_timeout"
        | "router_timeout"
        | "link_timeout" => "failed",
        _ => "queued_for_propagation",
    }
}

fn lxmf_propagation_receipt_state_for_transfer(transfer_state: &str) -> &'static str {
    match transfer_state {
        "link_packet_sent" | "resource_advertised" | "resource_completed" => {
            "propagation_node_accepted_peer_unconfirmed"
        }
        "link_packet_failed"
        | "resource_failed"
        | "resource_advertise_failed"
        | "resource_timeout"
        | "router_timeout"
        | "link_timeout" => "propagation_resource_failed",
        "resource_progress" => "propagation_resource_in_progress",
        _ => "propagation_queued",
    }
}

fn lxmf_propagation_proof_state_for_transfer(transfer_state: &str) -> &'static str {
    match transfer_state {
        "link_packet_sent" | "resource_advertised" | "resource_completed" => {
            "peer_delivery_unconfirmed"
        }
        "link_packet_failed"
        | "resource_failed"
        | "resource_advertise_failed"
        | "resource_timeout"
        | "router_timeout"
        | "link_timeout" => "failed",
        "resource_progress" => "propagation_resource_in_progress",
        _ => "propagation_queued",
    }
}

fn lxmf_retry_guidance_for_propagation_transfer(transfer_state: &str) -> &'static str {
    match transfer_state {
        "link_packet_sent" => {
            "LXMF propagation envelope was sent to the propagation node; sync propagation or wait for peer delivery evidence"
        }
        "resource_completed" => {
            "LXMF wire payload transferred to propagation node; sync propagation or wait for peer delivery evidence"
        }
        "resource_advertised" => {
            "LXMF propagation resource was handed to the propagation node; sync propagation or wait for peer delivery evidence"
        }
        "link_packet_failed"
        | "resource_failed"
        | "resource_advertise_failed"
        | "resource_timeout"
        | "router_timeout"
        | "link_timeout" => {
            "propagation transfer failed; run Prop Diag and retry after the blocker is fixed"
        }
        "resource_progress" => {
            "propagation transfer is in progress; wait for resource completion or failure"
        }
        _ => "propagation message is queued; verify propagation node path/app-data if it does not progress",
    }
}

fn lxmf_state_for_direct_transfer(transfer_state: &str) -> &'static str {
    match transfer_state {
        "link_packet_sent" => "submitted_unconfirmed",
        "resource_completed" => "submitted_unconfirmed",
        "resource_failed" => "failed",
        "resource_timeout" => "submitted_unconfirmed",
        _ => "submitted_to_rns_net",
    }
}

fn lxmf_direct_receipt_state_for_transfer(transfer_state: &str) -> &'static str {
    match transfer_state {
        "link_packet_sent" => "direct_link_packet_sent_peer_unconfirmed",
        "resource_completed" => "direct_resource_completed_peer_unconfirmed",
        "resource_failed" => "lxmf_failed",
        "resource_timeout" => "direct_resource_timeout",
        "resource_progress" | "resource_advertised" => "direct_resource_in_progress",
        _ => "direct_resource_state_unknown",
    }
}

fn lxmf_direct_proof_state_for_transfer(transfer_state: &str) -> &'static str {
    match transfer_state {
        "link_packet_sent" => "link_packet_sent",
        "resource_completed" => "resource_completed",
        "resource_failed" => "failed",
        "resource_timeout" => "resource_timeout",
        "resource_progress" => "resource_progress",
        "resource_advertised" => "resource_advertised",
        _ => "direct_resource_state_unknown",
    }
}

fn lxmf_retry_guidance_for_direct_transfer(transfer_state: &str) -> &'static str {
    match transfer_state {
        "link_packet_sent" => "LXMF direct link packet was sent on an established encrypted link; wait for LXMF router evidence or peer activity before treating it as final",
        "resource_completed" => "LXMF direct resource transfer completed; wait for LXMF router evidence or peer activity before treating it as final",
        "resource_failed" => {
            "LXMF direct resource transfer failed; inspect peer/path and retry after the blocker is fixed"
        }
        "resource_timeout" => {
            "LXMF direct resource was accepted by the native transport but no sender-side completion proof arrived"
        }
        "resource_progress" | "resource_advertised" => {
            "LXMF direct resource is in progress; wait for completion, failure, or peer activity"
        }
        _ => "LXMF direct resource state is unknown; inspect peer/path before retrying",
    }
}

fn lxmf_delivery_evidence_fields(
    evidence: &LxmfDeliveryEvidence,
) -> std::collections::BTreeMap<String, String> {
    let mut fields = BTreeMap::new();
    fields.insert(
        "native_lxmf_delivery_evidence_kind".into(),
        lxmf_delivery_evidence_kind_label(evidence.kind).into(),
    );
    if let Some(detail) = &evidence.detail {
        fields.insert(
            "native_lxmf_delivery_evidence_detail".into(),
            detail.clone(),
        );
        for (key, field) in [
            ("packet_hash", "native_lxmf_evidence_packet_hash"),
            ("direct_transfer_state", "native_lxmf_direct_transfer_state"),
            ("direct_link_id", "native_lxmf_direct_link_id"),
            (
                "propagation_transfer_state",
                "native_lxmf_propagation_transfer_state",
            ),
            ("propagation_link_id", "native_lxmf_propagation_link_id"),
            ("propagation_node", "native_lxmf_propagation_node"),
            ("receipt_state", "native_lxmf_receipt_state"),
            ("delivery_state", "native_lxmf_delivery_state"),
            ("proof_destination", "native_lxmf_proof_destination"),
            ("matched_pending", "native_lxmf_proof_matched_pending"),
            (
                "direct_timeout_age_secs",
                "native_lxmf_direct_timeout_age_secs",
            ),
            (
                "peer_activity_observed",
                "native_lxmf_peer_activity_observed",
            ),
            ("requested", "native_lxmf_propagation_sync_requested"),
            ("decoded", "native_lxmf_propagation_sync_decoded"),
            ("haves", "native_lxmf_propagation_sync_haves"),
            ("proof_state", "native_lxmf_evidence_proof_state"),
        ] {
            if let Some(value) = extract_lxmf_evidence_value(detail, key) {
                fields.insert(field.into(), value.into());
            }
        }
        if let Some(transfer_state) = extract_lxmf_evidence_value(detail, "direct_transfer_state") {
            fields.insert(
                "native_lxmf_receipt_state".into(),
                lxmf_direct_receipt_state_for_transfer(transfer_state).into(),
            );
            fields.insert(
                "native_lxmf_proof_state".into(),
                lxmf_direct_proof_state_for_transfer(transfer_state).into(),
            );
            fields.insert(
                "native_lxmf_state".into(),
                lxmf_state_for_direct_transfer(transfer_state).into(),
            );
            fields.insert(
                "native_lxmf_retry_guidance".into(),
                lxmf_retry_guidance_for_direct_transfer(transfer_state).into(),
            );
        }
    }
    if let Some(observed_at) = evidence.observed_at {
        fields.insert(
            "native_lxmf_delivery_evidence_observed_at".into(),
            format!("{observed_at:.3}"),
        );
    }
    if let Some(rtt) = evidence.rtt {
        fields.insert("native_lxmf_rtt".into(), format!("{rtt:.3}"));
    }
    if let Some(retry_after) = lxmf_retry_after_epoch_secs_for_evidence(evidence) {
        fields.insert(
            "native_lxmf_retry_after_epoch_secs".into(),
            format!("{retry_after:.3}"),
        );
    }
    match evidence.kind {
        LxmfDeliveryEvidenceKind::PacketSubmitted => {
            fields
                .entry("native_lxmf_receipt_state".into())
                .or_insert_with(|| "packet_submitted".into());
        }
        LxmfDeliveryEvidenceKind::RnsPacketProof => {
            fields.insert(
                "native_lxmf_proof_state".into(),
                "rns_packet_proof_peer_unconfirmed".into(),
            );
            fields.insert(
                "native_lxmf_receipt_state".into(),
                "rns_packet_proof_peer_delivery_unconfirmed".into(),
            );
            fields.insert(
                "native_lxmf_state".into(),
                "transport_proof_received".into(),
            );
            fields.insert(
                "native_lxmf_retry_guidance".into(),
                "RNS packet proof received, but native Rust has not observed LXMF router delivery or peer activity"
                    .into(),
            );
        }
        LxmfDeliveryEvidenceKind::PropagationNodeAccepted => {
            fields.insert(
                "native_lxmf_receipt_state".into(),
                "propagation_node_accepted_peer_unconfirmed".into(),
            );
            fields.insert(
                "native_lxmf_proof_state".into(),
                "peer_delivery_unconfirmed".into(),
            );
            fields.insert(
                "native_lxmf_state".into(),
                "propagation_node_accepted".into(),
            );
            fields.insert("native_lxmf_next_action".into(), "sync_propagation".into());
            fields.insert(
                "native_lxmf_retry_guidance".into(),
                "propagation node accepted the payload; sync propagation or wait for peer activity before resending"
                    .into(),
            );
        }
        LxmfDeliveryEvidenceKind::PropagationNodeFailed => {
            fields.insert(
                "native_lxmf_receipt_state".into(),
                "propagation_node_failed".into(),
            );
            fields.insert("native_lxmf_proof_state".into(), "failed".into());
            fields.insert("native_lxmf_state".into(), "failed".into());
            fields.insert(
                "native_lxmf_next_action".into(),
                "retry_propagated_send".into(),
            );
            fields.insert(
                "native_lxmf_retry_guidance".into(),
                "propagation node transfer failed; inspect propagation readiness, then retry after the backoff"
                    .into(),
            );
        }
        LxmfDeliveryEvidenceKind::PropagationSyncNoPayloads => {
            fields.insert(
                "native_lxmf_receipt_state".into(),
                "propagation_sync_no_peer_payload".into(),
            );
            fields.insert(
                "native_lxmf_proof_state".into(),
                "peer_delivery_unconfirmed".into(),
            );
            fields.insert(
                "native_lxmf_state".into(),
                "propagation_sync_no_payloads".into(),
            );
            fields.insert(
                "native_lxmf_retry_guidance".into(),
                "propagation sync completed but returned no peer payload; wait for the backoff, sync again, or watch for peer activity before resending"
                    .into(),
            );
            fields.insert(
                "native_lxmf_next_action".into(),
                "sync_propagation_again".into(),
            );
        }
        LxmfDeliveryEvidenceKind::LxmfRouterDelivered => {
            fields.insert("native_lxmf_receipt_state".into(), "lxmf_delivered".into());
            fields.insert(
                "native_lxmf_proof_state".into(),
                "lxmf_router_callback".into(),
            );
            fields.insert("native_lxmf_state".into(), "delivered".into());
            fields.insert(
                "native_lxmf_retry_guidance".into(),
                "LXMF router delivered callback received; no retry needed".into(),
            );
        }
        LxmfDeliveryEvidenceKind::LxmfRouterFailed => {
            fields.insert("native_lxmf_receipt_state".into(), "lxmf_failed".into());
            fields.insert("native_lxmf_proof_state".into(), "failed".into());
            fields.insert("native_lxmf_state".into(), "failed".into());
            fields.insert(
                "native_lxmf_retry_guidance".into(),
                "LXMF router failed callback received; inspect peer/path and retry after the blocker is fixed"
                    .into(),
            );
        }
        LxmfDeliveryEvidenceKind::InboundPeerMessage => {
            fields.insert("native_lxmf_state".into(), "peer_activity_observed".into());
            fields.insert(
                "native_lxmf_receipt_state".into(),
                "peer_activity_after_send".into(),
            );
            fields.insert("native_lxmf_peer_activity_observed".into(), "true".into());
            fields
                .entry("native_lxmf_proof_state".into())
                .or_insert_with(|| "peer_activity_observed".into());
            fields.insert(
                "native_lxmf_retry_guidance".into(),
                "inbound LXMF activity from this peer was observed after the outbound send; do not retry unless the peer reports they did not receive it"
                    .into(),
            );
        }
        LxmfDeliveryEvidenceKind::NoReceiptObserved => {
            let direct_transfer_state = evidence
                .detail
                .as_deref()
                .and_then(|detail| extract_lxmf_evidence_value(detail, "direct_transfer_state"));
            if direct_transfer_state.is_none() {
                fields.insert(
                    "native_lxmf_receipt_state".into(),
                    "lxmf_delivery_receipt_unavailable_native_wire".into(),
                );
                fields.insert(
                    "native_lxmf_proof_state".into(),
                    "proof_not_observed".into(),
                );
            }
            let fallback_ready = evidence
                .detail
                .as_deref()
                .and_then(|detail| extract_lxmf_evidence_value(detail, "fallback_ready"))
                == Some("true");
            if direct_transfer_state.is_none() || fallback_ready {
                fields.insert(
                    "native_lxmf_state".into(),
                    if fallback_ready {
                        "propagation_retry_ready"
                    } else {
                        "submitted_unconfirmed"
                    }
                    .into(),
                );
                fields.insert(
                    "native_lxmf_retry_guidance".into(),
                    if fallback_ready {
                        "direct LXMF transfer has no confirmed peer receipt; retry is prepared to use the selected propagation node"
                    } else {
                        "direct LXMF transfer has no confirmed peer receipt from the native wire path"
                    }
                    .into(),
                );
            }
            fields.insert(
                "native_lxmf_uncertain_reason".into(),
                if fallback_ready {
                    "native direct send did not receive confirmed peer delivery before timeout; explicit retry will use propagation when requested"
                } else {
                    "native direct send currently has transfer evidence but no confirmed peer-side receipt"
                }
                .into(),
            );
            if let Some(node) = evidence
                .detail
                .as_deref()
                .and_then(|detail| extract_lxmf_evidence_value(detail, "propagation_node"))
            {
                fields.insert(
                    "native_lxmf_propagation_fallback_available".into(),
                    "true".into(),
                );
                fields.insert("native_lxmf_propagation_node".into(), node.into());
            }
        }
    }
    fields
}

fn lxmf_retry_after_epoch_secs_for_evidence(evidence: &LxmfDeliveryEvidence) -> Option<f64> {
    let observed_at = evidence.observed_at?;
    match evidence.kind {
        LxmfDeliveryEvidenceKind::PropagationNodeFailed
        | LxmfDeliveryEvidenceKind::PropagationSyncNoPayloads => Some(observed_at + 30.0),
        _ => None,
    }
}

fn extract_lxmf_evidence_value<'a>(evidence: &'a str, key: &str) -> Option<&'a str> {
    let prefix = format!("{key}:");
    evidence
        .split(';')
        .find_map(|part| part.strip_prefix(prefix.as_str()))
}

fn lxmf_delivery_evidence_kind_label(kind: LxmfDeliveryEvidenceKind) -> &'static str {
    match kind {
        LxmfDeliveryEvidenceKind::PacketSubmitted => "packet_submitted",
        LxmfDeliveryEvidenceKind::RnsPacketProof => "rns_packet_proof",
        LxmfDeliveryEvidenceKind::PropagationNodeAccepted => "propagation_node_accepted",
        LxmfDeliveryEvidenceKind::PropagationNodeFailed => "propagation_node_failed",
        LxmfDeliveryEvidenceKind::PropagationSyncNoPayloads => "propagation_sync_no_payloads",
        LxmfDeliveryEvidenceKind::LxmfRouterDelivered => "lxmf_router_delivered",
        LxmfDeliveryEvidenceKind::LxmfRouterFailed => "lxmf_router_failed",
        LxmfDeliveryEvidenceKind::InboundPeerMessage => "inbound_peer_message",
        LxmfDeliveryEvidenceKind::NoReceiptObserved => "no_receipt_observed",
    }
}
