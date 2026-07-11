pub(crate) fn lxmf_delivery_evidence_label(kind: &str) -> &'static str {
    crate::messaging::lxmf_labels::delivery_evidence(kind)
}

pub(crate) fn lxmf_state_label(state: &str) -> &'static str {
    crate::messaging::lxmf_labels::state(state)
}

pub(crate) fn lxmf_proof_state_label(state: &str) -> &'static str {
    crate::messaging::lxmf_labels::proof_state(state)
}

pub(crate) fn lxmf_receipt_state_label(state: &str) -> &'static str {
    crate::messaging::lxmf_labels::receipt_state(state)
}

pub(crate) fn lxmf_fallback_label(fallback: &str) -> &'static str {
    crate::messaging::lxmf_labels::fallback(fallback)
}

pub(crate) fn lxmf_propagation_transfer_label(transfer: &str) -> &'static str {
    crate::messaging::lxmf_labels::propagation_transfer(transfer)
}
