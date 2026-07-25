use thiserror::Error;

use super::{
    AuthoritativeProgress, EvidenceAuthority, OperationAction, OperationDomain, OperationEvidence,
    OperationEvidenceKind, OperationHistory, OperationId, OperationRecord, OperationState,
};

pub const OPERATION_PRESENTATION_MAX_ROWS: usize = 128;
pub const OPERATION_PRESENTATION_DEFAULT_ROWS: usize = 64;
pub const OPERATION_PRESENTATION_SEARCH_MAX_BYTES: usize = 128;
pub const OPERATION_PRESENTATION_TARGET_MAX_BYTES: usize = 160;
pub const OPERATION_PRESENTATION_SUMMARY_MAX_BYTES: usize = 256;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum OperationPresentationFilter {
    #[default]
    All,
    Active,
    NeedsAttention,
    Completed,
    Domain(OperationDomain),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OperationPresentationQuery {
    filter: OperationPresentationFilter,
    search_ascii_lowercase: Option<String>,
    limit: usize,
}

impl Default for OperationPresentationQuery {
    fn default() -> Self {
        Self {
            filter: OperationPresentationFilter::All,
            search_ascii_lowercase: None,
            limit: OPERATION_PRESENTATION_DEFAULT_ROWS,
        }
    }
}

impl OperationPresentationQuery {
    pub fn new(
        filter: OperationPresentationFilter,
        search: Option<&str>,
        limit: usize,
    ) -> Result<Self, OperationPresentationError> {
        if limit == 0 || limit > OPERATION_PRESENTATION_MAX_ROWS {
            return Err(OperationPresentationError::RowLimit);
        }
        let search_ascii_lowercase = search
            .map(str::trim)
            .filter(|search| !search.is_empty())
            .map(|search| {
                if search.len() > OPERATION_PRESENTATION_SEARCH_MAX_BYTES
                    || search.chars().any(char::is_control)
                {
                    Err(OperationPresentationError::SearchLimit)
                } else {
                    Ok(search.to_ascii_lowercase())
                }
            })
            .transpose()?;
        Ok(Self {
            filter,
            search_ascii_lowercase,
            limit,
        })
    }

    pub fn filter(&self) -> OperationPresentationFilter {
        self.filter
    }

    pub fn search(&self) -> Option<&str> {
        self.search_ascii_lowercase.as_deref()
    }

    pub fn limit(&self) -> usize {
        self.limit
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OperationPresentation {
    pub rows: Vec<OperationPresentationRow>,
    pub total_matches: usize,
    pub omitted: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OperationPresentationRow {
    pub id: OperationId,
    pub domain: OperationDomain,
    pub domain_label: &'static str,
    pub target: String,
    pub state: OperationState,
    pub state_label: &'static str,
    pub authority: EvidenceAuthority,
    pub authority_label: &'static str,
    pub evidence_summary: String,
    pub progress: Option<AuthoritativeProgress>,
    pub updated_at_unix_ms: i64,
    pub needs_attention: bool,
    pub terminal: bool,
    pub valid_actions: Vec<OperationAction>,
}

pub fn present_operations(
    history: &OperationHistory,
    query: &OperationPresentationQuery,
) -> OperationPresentation {
    let mut matching = history
        .records()
        .filter(|record| filter_matches(record, query.filter))
        .filter(|record| search_matches(record, query.search_ascii_lowercase.as_deref()))
        .collect::<Vec<_>>();
    matching.sort_by(|left, right| {
        operation_needs_attention(right)
            .cmp(&operation_needs_attention(left))
            .then_with(|| left.state.is_terminal().cmp(&right.state.is_terminal()))
            .then_with(|| right.updated_at_unix_ms.cmp(&left.updated_at_unix_ms))
            .then_with(|| left.id.cmp(&right.id))
    });
    let total_matches = matching.len();
    let rows = matching
        .into_iter()
        .take(query.limit)
        .map(present_record)
        .collect::<Vec<_>>();
    OperationPresentation {
        omitted: total_matches.saturating_sub(rows.len()),
        total_matches,
        rows,
    }
}

fn present_record(record: &OperationRecord) -> OperationPresentationRow {
    OperationPresentationRow {
        id: record.id,
        domain: record.id.domain,
        domain_label: domain_label(record.id.domain),
        target: bounded_display_text(
            &record.target.label,
            OPERATION_PRESENTATION_TARGET_MAX_BYTES,
        ),
        state: record.state,
        state_label: state_label(record.state),
        authority: record.authority,
        authority_label: authority_label(record.authority),
        evidence_summary: evidence_summary(&record.evidence),
        progress: record.progress,
        updated_at_unix_ms: record.updated_at_unix_ms,
        needs_attention: operation_needs_attention(record),
        terminal: record.state.is_terminal(),
        valid_actions: record.valid_actions.clone(),
    }
}

fn filter_matches(record: &OperationRecord, filter: OperationPresentationFilter) -> bool {
    match filter {
        OperationPresentationFilter::All => true,
        OperationPresentationFilter::Active => !record.state.is_terminal(),
        OperationPresentationFilter::NeedsAttention => operation_needs_attention(record),
        OperationPresentationFilter::Completed => record.state.is_terminal(),
        OperationPresentationFilter::Domain(domain) => record.id.domain == domain,
    }
}

fn search_matches(record: &OperationRecord, search_ascii_lowercase: Option<&str>) -> bool {
    let Some(search) = search_ascii_lowercase else {
        return true;
    };
    contains_ascii_case_insensitive(&record.target.label, search)
        || contains_ascii_case_insensitive(domain_label(record.id.domain), search)
        || contains_ascii_case_insensitive(state_label(record.state), search)
        || contains_ascii_case_insensitive(authority_label(record.authority), search)
        || record.evidence.iter().any(|evidence| {
            contains_ascii_case_insensitive(evidence_label(evidence.kind), search)
                || evidence
                    .detail
                    .as_deref()
                    .is_some_and(|detail| contains_ascii_case_insensitive(detail, search))
        })
}

fn contains_ascii_case_insensitive(haystack: &str, needle_ascii_lowercase: &str) -> bool {
    let needle = needle_ascii_lowercase.as_bytes();
    if needle.is_empty() {
        return true;
    }
    haystack
        .as_bytes()
        .windows(needle.len())
        .any(|window| window.eq_ignore_ascii_case(needle))
}

fn operation_needs_attention(record: &OperationRecord) -> bool {
    !matches!(record.authority, EvidenceAuthority::Authoritative)
        || matches!(
            record.state,
            OperationState::Reconciling
                | OperationState::EventGap
                | OperationState::Failed
                | OperationState::Rejected
        )
}

fn evidence_summary(evidence: &[OperationEvidence]) -> String {
    let Some(latest) = evidence.iter().max_by_key(|evidence| evidence.at_unix_ms) else {
        return "no retained evidence".into();
    };
    let summary = match latest.detail.as_deref() {
        Some(detail) if !detail.is_empty() => {
            format!("{}: {detail}", evidence_label(latest.kind))
        }
        _ => evidence_label(latest.kind).into(),
    };
    bounded_display_text(&summary, OPERATION_PRESENTATION_SUMMARY_MAX_BYTES)
}

pub fn domain_label(domain: OperationDomain) -> &'static str {
    match domain {
        OperationDomain::PathDiscovery => "path discovery",
        OperationDomain::LinkEstablishment => "link establishment",
        OperationDomain::LxmfMessage => "LXMF message",
        OperationDomain::ResourceTransfer => "resource transfer",
        OperationDomain::OmenChatConnection => "OMENchat connection",
        OperationDomain::OmenChatMutation => "OMENchat mutation",
    }
}

pub fn state_label(state: OperationState) -> &'static str {
    match state {
        OperationState::Waiting => "waiting",
        OperationState::Queued => "queued",
        OperationState::Dispatching => "dispatching",
        OperationState::TransportAccepted => "transport accepted",
        OperationState::ReceiptObserved => "receipt observed",
        OperationState::Delivered => "delivered",
        OperationState::Completed => "completed",
        OperationState::Transferring => "transferring",
        OperationState::Active => "active",
        OperationState::Reconciling => "reconciling",
        OperationState::EventGap => "event gap",
        OperationState::Cancelled => "cancelled",
        OperationState::Failed => "failed",
        OperationState::Expired => "expired",
        OperationState::Rejected => "rejected",
    }
}

pub fn authority_label(authority: EvidenceAuthority) -> &'static str {
    match authority {
        EvidenceAuthority::Authoritative => "authoritative",
        EvidenceAuthority::Inferred => "inferred",
        EvidenceAuthority::Stale => "stale",
        EvidenceAuthority::Uncertain => "uncertain",
    }
}

pub fn evidence_label(kind: OperationEvidenceKind) -> &'static str {
    match kind {
        OperationEvidenceKind::QueueAdmission => "queue admission",
        OperationEvidenceKind::Dispatch => "dispatch",
        OperationEvidenceKind::TransportAcceptance => "transport acceptance",
        OperationEvidenceKind::Receipt => "receipt",
        OperationEvidenceKind::PeerDelivery => "peer delivery",
        OperationEvidenceKind::ResourceOffer => "resource offer",
        OperationEvidenceKind::ResourceProgress => "resource progress",
        OperationEvidenceKind::ResourceCompletion => "resource completion",
        OperationEvidenceKind::Cancellation => "cancellation",
        OperationEvidenceKind::Failure => "failure",
        OperationEvidenceKind::Expiration => "expiration",
        OperationEvidenceKind::Rejection => "rejection",
        OperationEvidenceKind::EventGap => "event gap",
        OperationEvidenceKind::Reconciliation => "reconciliation",
    }
}

pub fn action_label(action: OperationAction) -> &'static str {
    match action {
        OperationAction::ExplicitSend => "send explicitly",
        OperationAction::Cancel => "cancel",
        OperationAction::Reconcile => "reconcile",
        OperationAction::ExplicitSafeRetry => "retry explicitly",
        OperationAction::CopyDiagnostics => "copy diagnostics",
    }
}

pub fn filter_label(filter: OperationPresentationFilter) -> &'static str {
    match filter {
        OperationPresentationFilter::All => "all",
        OperationPresentationFilter::Active => "active",
        OperationPresentationFilter::NeedsAttention => "needs attention",
        OperationPresentationFilter::Completed => "completed",
        OperationPresentationFilter::Domain(_) => "domain",
    }
}

fn bounded_display_text(value: &str, max_bytes: usize) -> String {
    let sanitized = value
        .chars()
        .map(|character| {
            if character.is_control() {
                ' '
            } else {
                character
            }
        })
        .collect::<String>();
    if sanitized.len() <= max_bytes {
        return sanitized;
    }
    const ELLIPSIS: &str = "…";
    let content_limit = max_bytes.saturating_sub(ELLIPSIS.len());
    let mut end = content_limit.min(sanitized.len());
    while !sanitized.is_char_boundary(end) {
        end = end.saturating_sub(1);
    }
    let mut bounded = sanitized[..end].to_string();
    bounded.push_str(ELLIPSIS);
    bounded
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum OperationPresentationError {
    #[error("operation presentation row limit must be between 1 and 128")]
    RowLimit,
    #[error("operation presentation search exceeds its bound or contains controls")]
    SearchLimit,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::operations::{OperationKey, OperationTarget, OperationTargetKind};

    fn record(
        local_id: u64,
        domain: OperationDomain,
        state: OperationState,
        authority: EvidenceAuthority,
        updated_at_unix_ms: i64,
        target: &str,
    ) -> OperationRecord {
        let evidence_kind = if state == OperationState::Delivered {
            OperationEvidenceKind::PeerDelivery
        } else {
            OperationEvidenceKind::QueueAdmission
        };
        OperationRecord {
            id: OperationId::numeric(domain, local_id),
            target: OperationTarget {
                kind: OperationTargetKind::Peer,
                label: target.into(),
            },
            state,
            authority,
            evidence: vec![OperationEvidence {
                kind: evidence_kind,
                authority,
                at_unix_ms: updated_at_unix_ms,
                detail: Some(format!("detail-{local_id}")),
            }],
            progress: None,
            attempt_count: 1,
            stamp_cost: None,
            propagation_node: None,
            created_at_unix_ms: 1,
            updated_at_unix_ms,
            last_error: None,
            event_cursor: None,
            valid_actions: vec![OperationAction::CopyDiagnostics],
        }
    }

    fn fixture_history() -> OperationHistory {
        let mut history = OperationHistory::default();
        history
            .upsert(record(
                1,
                OperationDomain::PathDiscovery,
                OperationState::Active,
                EvidenceAuthority::Authoritative,
                10,
                "node-alpha",
            ))
            .expect("active path");
        history
            .upsert(record(
                2,
                OperationDomain::LxmfMessage,
                OperationState::TransportAccepted,
                EvidenceAuthority::Authoritative,
                30,
                "peer-bravo",
            ))
            .expect("accepted message");
        history
            .upsert(record(
                3,
                OperationDomain::OmenChatMutation,
                OperationState::Reconciling,
                EvidenceAuthority::Uncertain,
                20,
                "server-charlie / room 7",
            ))
            .expect("uncertain mutation");
        history
            .upsert(record(
                4,
                OperationDomain::ResourceTransfer,
                OperationState::Delivered,
                EvidenceAuthority::Authoritative,
                40,
                "resource-delta",
            ))
            .expect("delivered resource");
        history
    }

    #[test]
    fn shared_rows_keep_transport_receipt_and_delivery_labels_distinct() {
        assert_eq!(
            state_label(OperationState::TransportAccepted),
            "transport accepted"
        );
        assert_eq!(
            state_label(OperationState::ReceiptObserved),
            "receipt observed"
        );
        assert_eq!(state_label(OperationState::Delivered), "delivered");
        assert_eq!(state_label(OperationState::Completed), "completed");
        assert_ne!(
            state_label(OperationState::TransportAccepted),
            state_label(OperationState::Delivered)
        );
        assert_ne!(
            state_label(OperationState::Completed),
            state_label(OperationState::Delivered)
        );
    }

    #[test]
    fn presentation_is_bounded_sorted_and_reports_omissions() {
        let query = OperationPresentationQuery::new(OperationPresentationFilter::All, None, 3)
            .expect("query");
        let presentation = present_operations(&fixture_history(), &query);
        assert_eq!(presentation.total_matches, 4);
        assert_eq!(presentation.omitted, 1);
        assert_eq!(
            presentation
                .rows
                .iter()
                .map(|row| row.id.key)
                .collect::<Vec<_>>(),
            vec![
                OperationKey::Numeric(3),
                OperationKey::Numeric(2),
                OperationKey::Numeric(1),
            ]
        );
    }

    #[test]
    fn filters_and_search_use_public_text_but_not_opaque_ids() {
        let history = fixture_history();
        let attention = present_operations(
            &history,
            &OperationPresentationQuery::new(OperationPresentationFilter::NeedsAttention, None, 8)
                .expect("attention query"),
        );
        assert_eq!(attention.rows.len(), 1);
        assert_eq!(attention.rows[0].state, OperationState::Reconciling);

        let search = present_operations(
            &history,
            &OperationPresentationQuery::new(OperationPresentationFilter::All, Some("BRAVO"), 8)
                .expect("search query"),
        );
        assert_eq!(search.rows.len(), 1);
        assert_eq!(search.rows[0].target, "peer-bravo");

        let domain_search = present_operations(
            &history,
            &OperationPresentationQuery::new(OperationPresentationFilter::All, Some("lxmf"), 8)
                .expect("domain search query"),
        );
        assert_eq!(domain_search.rows.len(), 1);
        assert_eq!(domain_search.rows[0].domain, OperationDomain::LxmfMessage);

        let mut opaque = record(
            9,
            OperationDomain::OmenChatMutation,
            OperationState::Reconciling,
            EvidenceAuthority::Uncertain,
            50,
            "public-server",
        );
        opaque.id = OperationId::opaque_128(OperationDomain::OmenChatMutation, [0xab; 16]);
        let mut opaque_history = OperationHistory::default();
        opaque_history.upsert(opaque).expect("opaque record");
        let hidden_id = present_operations(
            &opaque_history,
            &OperationPresentationQuery::new(OperationPresentationFilter::All, Some("abababab"), 8)
                .expect("opaque search"),
        );
        assert!(hidden_id.rows.is_empty());
    }

    #[test]
    fn rows_sanitize_controls_and_utf8_truncate_target_and_evidence() {
        let mut source = record(
            1,
            OperationDomain::LxmfMessage,
            OperationState::Active,
            EvidenceAuthority::Authoritative,
            10,
            &format!("target\n{}", "界".repeat(100)),
        );
        source.evidence[0].detail = Some(format!("detail-line\r{}", "é".repeat(200)));
        let mut history = OperationHistory::default();
        history.upsert(source).expect("bounded source");
        let presentation = present_operations(&history, &OperationPresentationQuery::default());
        let row = &presentation.rows[0];
        assert!(!row.target.chars().any(char::is_control));
        assert!(!row.evidence_summary.chars().any(char::is_control));
        assert!(row.target.len() <= OPERATION_PRESENTATION_TARGET_MAX_BYTES);
        assert!(row.evidence_summary.len() <= OPERATION_PRESENTATION_SUMMARY_MAX_BYTES);
        assert!(row.target.ends_with('…'));
        assert!(row.evidence_summary.ends_with('…'));
    }

    #[test]
    fn authoritative_progress_and_valid_actions_are_preserved_exactly() {
        let mut source = record(
            1,
            OperationDomain::ResourceTransfer,
            OperationState::Transferring,
            EvidenceAuthority::Authoritative,
            10,
            "resource",
        );
        source.progress = Some(AuthoritativeProgress::new(3, 7).expect("progress"));
        source.valid_actions = vec![OperationAction::Cancel, OperationAction::CopyDiagnostics];
        let mut history = OperationHistory::default();
        history.upsert(source).expect("resource");
        let presentation = present_operations(&history, &OperationPresentationQuery::default());
        assert_eq!(
            presentation.rows[0].progress,
            Some(AuthoritativeProgress {
                completed_bytes: 3,
                total_bytes: 7,
            })
        );
        assert_eq!(
            presentation.rows[0].valid_actions,
            vec![OperationAction::Cancel, OperationAction::CopyDiagnostics]
        );
    }

    #[test]
    fn query_rejects_unbounded_rows_search_and_controls() {
        assert_eq!(
            OperationPresentationQuery::new(OperationPresentationFilter::All, None, 0),
            Err(OperationPresentationError::RowLimit)
        );
        assert_eq!(
            OperationPresentationQuery::new(
                OperationPresentationFilter::All,
                None,
                OPERATION_PRESENTATION_MAX_ROWS + 1,
            ),
            Err(OperationPresentationError::RowLimit)
        );
        assert_eq!(
            OperationPresentationQuery::new(
                OperationPresentationFilter::All,
                Some(&"x".repeat(OPERATION_PRESENTATION_SEARCH_MAX_BYTES + 1)),
                8,
            ),
            Err(OperationPresentationError::SearchLimit)
        );
        assert_eq!(
            OperationPresentationQuery::new(
                OperationPresentationFilter::All,
                Some("line\nbreak"),
                8,
            ),
            Err(OperationPresentationError::SearchLimit)
        );
    }

    #[test]
    fn shared_filter_and_action_labels_are_stable() {
        assert_eq!(
            filter_label(OperationPresentationFilter::NeedsAttention),
            "needs attention"
        );
        assert_eq!(
            action_label(OperationAction::ExplicitSend),
            "send explicitly"
        );
        assert_eq!(
            action_label(OperationAction::ExplicitSafeRetry),
            "retry explicitly"
        );
        assert_ne!(
            action_label(OperationAction::ExplicitSend),
            action_label(OperationAction::ExplicitSafeRetry)
        );
    }
}
