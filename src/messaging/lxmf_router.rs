use crate::messaging::{MessageSummary, TransportMethod};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DirectLxmfRouterState {
    Submitted,
    ProofReceived,
    PeerActivityObserved,
    NoReceiptObserved,
    PropagationRetryReady,
    Failed,
    Unknown,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DirectLxmfRouterRecord {
    pub state: DirectLxmfRouterState,
    pub submitted_at: Option<f64>,
    pub propagation_fallback_node: Option<String>,
}

impl DirectLxmfRouterRecord {
    pub fn from_message(message: &MessageSummary) -> Self {
        let propagation_fallback_node = message
            .fields
            .get("native_lxmf_propagation_fallback_available")
            .filter(|value| value.as_str() == "true")
            .and_then(|_| message.fields.get("native_lxmf_propagation_node"))
            .filter(|value| !value.is_empty())
            .cloned();
        let submitted_at = message
            .fields
            .get("native_lxmf_submitted_at")
            .and_then(|value| value.parse::<f64>().ok())
            .or(Some(message.timestamp));
        Self {
            state: direct_router_state_from_message(message),
            submitted_at,
            propagation_fallback_node,
        }
    }

    pub fn stale_outcome(&self, now: f64, timeout_seconds: f64) -> Option<DirectLxmfRouterState> {
        if !matches!(self.state, DirectLxmfRouterState::Submitted) {
            return None;
        }
        let submitted_at = self.submitted_at?;
        if now < submitted_at + timeout_seconds {
            return None;
        }
        Some(if self.propagation_fallback_node.is_some() {
            DirectLxmfRouterState::PropagationRetryReady
        } else {
            DirectLxmfRouterState::NoReceiptObserved
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DirectLxmfTimeoutTransition {
    pub state: DirectLxmfRouterState,
    pub state_field: &'static str,
    pub proof_state: &'static str,
    pub receipt_state: &'static str,
    pub evidence_kind: &'static str,
    pub retry_guidance: &'static str,
    pub uncertain_reason: &'static str,
}

impl DirectLxmfTimeoutTransition {
    pub fn for_state(state: DirectLxmfRouterState) -> Option<Self> {
        match state {
            DirectLxmfRouterState::NoReceiptObserved => Some(Self {
                state,
                state_field: direct_lxmf_state_field(state),
                proof_state: "proof_not_observed",
                receipt_state: "lxmf_delivery_receipt_unavailable_native_wire",
                evidence_kind: "no_receipt_observed",
                retry_guidance: "packet was submitted; no RNS proof or LXMF delivery receipt was observed by the native wire path",
                uncertain_reason: "native direct send currently has packet submission evidence but no LXMF router callback parity",
            }),
            DirectLxmfRouterState::PropagationRetryReady => Some(Self {
                state,
                state_field: direct_lxmf_state_field(state),
                proof_state: "proof_not_observed",
                receipt_state: "lxmf_delivery_receipt_unavailable_native_wire",
                evidence_kind: "no_receipt_observed",
                retry_guidance: "direct packet was submitted with no proof observed; retry is prepared to use the selected propagation node",
                uncertain_reason: "native direct send did not receive proof before timeout; Python LXMF would retry failed direct sends through propagation when configured",
            }),
            _ => None,
        }
    }

    pub fn apply_to_fields(&self, fields: &mut std::collections::BTreeMap<String, String>) {
        fields.insert("native_lxmf_state".into(), self.state_field.into());
        fields.insert("native_lxmf_proof_state".into(), self.proof_state.into());
        fields.insert(
            "native_lxmf_receipt_state".into(),
            self.receipt_state.into(),
        );
        fields.insert(
            "native_lxmf_delivery_evidence_kind".into(),
            self.evidence_kind.into(),
        );
        fields.insert(
            "native_lxmf_retry_guidance".into(),
            self.retry_guidance.into(),
        );
        fields.insert(
            "native_lxmf_uncertain_reason".into(),
            self.uncertain_reason.into(),
        );
    }
}

pub fn direct_lxmf_timeout_transition(
    message: &MessageSummary,
    now: f64,
    timeout_seconds: f64,
) -> Option<DirectLxmfTimeoutTransition> {
    if message.incoming
        || message.delivered
        || message.failed
        || message.transport_method != TransportMethod::Direct
        || message.message_id.is_none()
    {
        return None;
    }
    if !matches!(
        message
            .fields
            .get("native_lxmf_proof_state")
            .map(String::as_str),
        Some("waiting_for_packet_proof" | "waiting_for_transport_receipt")
    ) {
        return None;
    }
    let record = DirectLxmfRouterRecord::from_message(message);
    let state = record.stale_outcome(now, timeout_seconds)?;
    DirectLxmfTimeoutTransition::for_state(state)
}

fn direct_router_state_from_message(message: &MessageSummary) -> DirectLxmfRouterState {
    if message.delivered {
        return DirectLxmfRouterState::ProofReceived;
    }
    if message.failed {
        return DirectLxmfRouterState::Failed;
    }
    if message
        .fields
        .get("native_lxmf_peer_activity_observed")
        .is_some_and(|value| value == "true")
        || message
            .fields
            .get("native_lxmf_receipt_state")
            .is_some_and(|value| value == "peer_activity_after_send")
    {
        return DirectLxmfRouterState::PeerActivityObserved;
    }
    match message.fields.get("native_lxmf_state").map(String::as_str) {
        Some("submitted_to_rns_net")
        | Some("submitted_to_runtime")
        | Some("submitted_to_clean_reticulum") => DirectLxmfRouterState::Submitted,
        Some("submitted_unconfirmed") => DirectLxmfRouterState::NoReceiptObserved,
        Some("propagation_retry_ready") => DirectLxmfRouterState::PropagationRetryReady,
        Some("delivered") => DirectLxmfRouterState::ProofReceived,
        Some("failed") => DirectLxmfRouterState::Failed,
        _ => DirectLxmfRouterState::Unknown,
    }
}

pub fn direct_lxmf_state_field(state: DirectLxmfRouterState) -> &'static str {
    match state {
        DirectLxmfRouterState::Submitted => "submitted_to_rns_net",
        DirectLxmfRouterState::ProofReceived => "delivered",
        DirectLxmfRouterState::PeerActivityObserved => "peer_activity_observed",
        DirectLxmfRouterState::NoReceiptObserved => "submitted_unconfirmed",
        DirectLxmfRouterState::PropagationRetryReady => "propagation_retry_ready",
        DirectLxmfRouterState::Failed => "failed",
        DirectLxmfRouterState::Unknown => "unknown",
    }
}
