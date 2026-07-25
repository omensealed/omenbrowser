use iced::widget::column;
use iced::Element;

use crate::operations::presentation::{
    present_operations, OperationPresentationFilter, OperationPresentationQuery,
};
use crate::operations::{EvidenceAuthority, OperationHistory};

use super::super::{
    human_bytes, section_card, wrapped_panel_text, wrapped_text_owned, DesktopApp, Message,
};

const OPERATIONS_PANEL_MAX_ROWS: usize = 8;

#[derive(Debug, PartialEq, Eq)]
struct OperationsPanelModel {
    summary: String,
    rows: Vec<OperationsPanelRow>,
    empty_message: Option<&'static str>,
}

#[derive(Debug, PartialEq, Eq)]
struct OperationsPanelRow {
    headline: String,
    evidence: String,
}

fn operations_panel_model(history: &OperationHistory) -> OperationsPanelModel {
    let query = OperationPresentationQuery::new(
        OperationPresentationFilter::All,
        None,
        OPERATIONS_PANEL_MAX_ROWS,
    )
    .expect("the fixed Operations panel query must remain within presentation bounds");
    let presentation = present_operations(history, &query);
    let metrics = history.metrics();
    let summary = if presentation.omitted == 0 {
        format!(
            "{} retained operation(s) | {}",
            presentation.total_matches,
            human_bytes(metrics.bytes as u64)
        )
    } else {
        format!(
            "{} shown / {} retained operation(s) | {} omitted | {}",
            presentation.rows.len(),
            presentation.total_matches,
            presentation.omitted,
            human_bytes(metrics.bytes as u64)
        )
    };
    let rows = presentation
        .rows
        .into_iter()
        .map(|row| {
            let mut headline = format!(
                "{} | {} | {} | {}",
                row.domain_label, row.target, row.state_label, row.authority_label
            );
            if row.authority == EvidenceAuthority::Authoritative {
                if let Some(progress) = row.progress {
                    headline.push_str(&format!(
                        " | {} / {}",
                        human_bytes(progress.completed_bytes),
                        human_bytes(progress.total_bytes)
                    ));
                }
            }
            OperationsPanelRow {
                headline,
                evidence: format!("evidence: {}", row.evidence_summary),
            }
        })
        .collect::<Vec<_>>();

    OperationsPanelModel {
        summary,
        empty_message: rows
            .is_empty()
            .then_some("No retained operations or transfers."),
        rows,
    }
}

pub(in crate::desktop) fn operations_panel(desktop: &DesktopApp) -> Element<'_, Message> {
    let model = operations_panel_model(&desktop.app.operation_history);
    let mut body = column![wrapped_text_owned(model.summary, 14)].spacing(6);
    if let Some(empty_message) = model.empty_message {
        body = body.push(wrapped_panel_text(empty_message));
    } else {
        for row in model.rows {
            body = body
                .push(wrapped_text_owned(row.headline, 14))
                .push(wrapped_text_owned(row.evidence, 13));
        }
    }
    body = body.push(wrapped_panel_text(
        "Read-only bounded history. Transport acceptance and receipt evidence do not claim peer delivery.",
    ));

    section_card("Operations & Transfers", body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::operations::{
        AuthoritativeProgress, OperationAction, OperationDomain, OperationEvidence,
        OperationEvidenceKind, OperationId, OperationRecord, OperationState, OperationTarget,
        OperationTargetKind,
    };

    fn record(
        id: OperationId,
        target: &str,
        state: OperationState,
        authority: EvidenceAuthority,
        updated_at_unix_ms: i64,
    ) -> OperationRecord {
        OperationRecord {
            id,
            target: OperationTarget {
                kind: OperationTargetKind::Peer,
                label: target.into(),
            },
            state,
            authority,
            evidence: vec![OperationEvidence {
                kind: OperationEvidenceKind::TransportAcceptance,
                authority,
                at_unix_ms: updated_at_unix_ms,
                detail: Some("accepted by local transport".into()),
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

    #[test]
    fn empty_panel_is_explicit_and_bounded() {
        let model = operations_panel_model(&OperationHistory::default());

        assert_eq!(model.summary, "0 retained operation(s) | 0 B");
        assert_eq!(
            model.empty_message,
            Some("No retained operations or transfers.")
        );
        assert!(model.rows.is_empty());
    }

    #[test]
    fn panel_uses_shared_transport_and_authority_terminology() {
        let mut history = OperationHistory::default();
        history
            .upsert(record(
                OperationId::numeric(OperationDomain::LxmfMessage, 1),
                "peer-bravo",
                OperationState::TransportAccepted,
                EvidenceAuthority::Authoritative,
                20,
            ))
            .expect("fixture");

        let model = operations_panel_model(&history);
        assert_eq!(model.rows.len(), 1);
        assert!(model.rows[0]
            .headline
            .contains("LXMF message | peer-bravo | transport accepted | authoritative"));
        assert!(!model.rows[0].headline.contains("delivered"));
        assert_eq!(
            model.rows[0].evidence,
            "evidence: transport acceptance: accepted by local transport"
        );
    }

    #[test]
    fn attention_rows_sort_first_and_opaque_ids_are_not_displayed() {
        let mut history = OperationHistory::default();
        history
            .upsert(record(
                OperationId::numeric(OperationDomain::PathDiscovery, 1),
                "ordinary-path",
                OperationState::Active,
                EvidenceAuthority::Authoritative,
                30,
            ))
            .expect("ordinary fixture");
        history
            .upsert(record(
                OperationId::opaque_128(OperationDomain::OmenChatMutation, [0xab; 16]),
                "public-server / room 7",
                OperationState::Reconciling,
                EvidenceAuthority::Uncertain,
                10,
            ))
            .expect("attention fixture");

        let model = operations_panel_model(&history);
        assert!(model.rows[0]
            .headline
            .contains("public-server / room 7 | reconciling | uncertain"));
        let displayed = model
            .rows
            .iter()
            .flat_map(|row| [&row.headline, &row.evidence])
            .cloned()
            .collect::<Vec<_>>()
            .join(" ");
        assert!(!displayed.contains("abababab"));
    }

    #[test]
    fn panel_reports_rows_omitted_by_its_fixed_limit() {
        let mut history = OperationHistory::default();
        for local_id in 0..(OPERATIONS_PANEL_MAX_ROWS as u64 + 2) {
            history
                .upsert(record(
                    OperationId::numeric(OperationDomain::PathDiscovery, local_id),
                    &format!("path-{local_id}"),
                    OperationState::Active,
                    EvidenceAuthority::Authoritative,
                    local_id as i64 + 1,
                ))
                .expect("fixture");
        }

        let model = operations_panel_model(&history);
        assert_eq!(model.rows.len(), OPERATIONS_PANEL_MAX_ROWS);
        assert!(model.summary.contains("8 shown / 10 retained operation(s)"));
        assert!(model.summary.contains("2 omitted"));
    }

    #[test]
    fn panel_shows_only_authoritative_byte_progress() {
        let mut authoritative = record(
            OperationId::numeric(OperationDomain::ResourceTransfer, 1),
            "resource-authoritative",
            OperationState::Transferring,
            EvidenceAuthority::Authoritative,
            20,
        );
        authoritative.progress = Some(AuthoritativeProgress::new(512, 1024).expect("progress"));
        let mut uncertain = record(
            OperationId::numeric(OperationDomain::ResourceTransfer, 2),
            "resource-uncertain",
            OperationState::Transferring,
            EvidenceAuthority::Uncertain,
            10,
        );
        uncertain.progress = Some(AuthoritativeProgress::new(512, 1024).expect("progress"));
        let mut history = OperationHistory::default();
        history
            .upsert(authoritative)
            .expect("authoritative fixture");
        history.upsert(uncertain).expect("uncertain fixture");

        let model = operations_panel_model(&history);
        let authoritative_row = model
            .rows
            .iter()
            .find(|row| row.headline.contains("resource-authoritative"))
            .expect("authoritative row");
        let uncertain_row = model
            .rows
            .iter()
            .find(|row| row.headline.contains("resource-uncertain"))
            .expect("uncertain row");
        assert!(authoritative_row.headline.contains("512 B / 1.0 KiB"));
        assert!(!uncertain_row.headline.contains("512 B / 1.0 KiB"));
    }
}
