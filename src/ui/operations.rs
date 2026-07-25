use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Frame;

use crate::app::App;
use crate::operations::presentation::{
    present_operations, OperationPresentationFilter, OperationPresentationQuery,
};
use crate::operations::{EvidenceAuthority, OperationHistory};

const TUI_OPERATIONS_MAX_ROWS: usize = 8;

#[derive(Debug, PartialEq, Eq)]
struct TuiOperationsModel {
    summary: String,
    rows: Vec<TuiOperationRow>,
    empty_message: Option<&'static str>,
}

#[derive(Debug, PartialEq, Eq)]
struct TuiOperationRow {
    headline: String,
    evidence: String,
    needs_attention: bool,
}

fn tui_operations_model(history: &OperationHistory) -> TuiOperationsModel {
    let query = OperationPresentationQuery::new(
        OperationPresentationFilter::All,
        None,
        TUI_OPERATIONS_MAX_ROWS,
    )
    .expect("the fixed TUI Operations query must remain within presentation bounds");
    let presentation = present_operations(history, &query);
    let metrics = history.metrics();
    let summary = if presentation.omitted == 0 {
        format!(
            "{} retained operation(s) | {} retained bytes",
            presentation.total_matches, metrics.bytes
        )
    } else {
        format!(
            "{} shown / {} retained operation(s) | {} omitted | {} retained bytes",
            presentation.rows.len(),
            presentation.total_matches,
            presentation.omitted,
            metrics.bytes
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
                        " | {}/{} bytes",
                        progress.completed_bytes, progress.total_bytes
                    ));
                }
            }
            TuiOperationRow {
                headline,
                evidence: format!("  evidence: {}", row.evidence_summary),
                needs_attention: row.needs_attention,
            }
        })
        .collect::<Vec<_>>();

    TuiOperationsModel {
        summary,
        empty_message: rows
            .is_empty()
            .then_some("No retained operations or transfers."),
        rows,
    }
}

pub(super) fn render(frame: &mut Frame, area: Rect, app: &App) {
    let model = tui_operations_model(&app.operation_history);
    let mut lines = vec![
        Line::styled(model.summary, Style::default().fg(Color::Gray)),
        Line::from(""),
    ];
    if let Some(empty_message) = model.empty_message {
        lines.push(Line::styled(
            empty_message,
            Style::default().fg(Color::DarkGray),
        ));
    } else {
        for row in model.rows {
            let headline_style = if row.needs_attention {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Cyan)
            };
            lines.push(Line::styled(row.headline, headline_style));
            lines.push(Line::styled(row.evidence, Style::default().fg(Color::Gray)));
        }
    }
    lines.extend([
        Line::from(""),
        Line::from(vec![
            Span::styled(
                "Read-only bounded history. ",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("Transport acceptance and receipt evidence do not claim peer delivery."),
        ]),
    ]);

    frame.render_widget(
        Paragraph::new(lines)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Cyan))
                    .title(" Network Doctor | Operations & Transfers "),
            )
            .wrap(Wrap { trim: false }),
        area,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{AppConfig, AppPaths};
    use crate::operations::{
        AuthoritativeProgress, OperationAction, OperationDomain, OperationEvidence,
        OperationEvidenceKind, OperationId, OperationRecord, OperationState, OperationTarget,
        OperationTargetKind,
    };
    use crate::storage::settings::AppSettings;
    use crate::workspace::WorkspaceSection;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

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
    fn empty_tui_model_is_explicit() {
        let model = tui_operations_model(&OperationHistory::default());

        assert_eq!(model.summary, "0 retained operation(s) | 0 retained bytes");
        assert_eq!(
            model.empty_message,
            Some("No retained operations or transfers.")
        );
        assert!(model.rows.is_empty());
    }

    #[test]
    fn tui_model_uses_shared_labels_and_never_claims_delivery() {
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

        let model = tui_operations_model(&history);
        assert!(model.rows[0]
            .headline
            .contains("LXMF message | peer-bravo | transport accepted | authoritative"));
        assert!(!model.rows[0].headline.contains("delivered"));
        assert_eq!(
            model.rows[0].evidence,
            "  evidence: transport acceptance: accepted by local transport"
        );
    }

    #[test]
    fn tui_model_reports_omissions_and_hides_opaque_ids() {
        let mut history = OperationHistory::default();
        history
            .upsert(record(
                OperationId::opaque_128(OperationDomain::OmenChatMutation, [0xab; 16]),
                "public-server / room 7",
                OperationState::Reconciling,
                EvidenceAuthority::Uncertain,
                1,
            ))
            .expect("opaque fixture");
        for local_id in 0..TUI_OPERATIONS_MAX_ROWS as u64 {
            history
                .upsert(record(
                    OperationId::numeric(OperationDomain::PathDiscovery, local_id),
                    &format!("path-{local_id}"),
                    OperationState::Active,
                    EvidenceAuthority::Authoritative,
                    local_id as i64 + 2,
                ))
                .expect("path fixture");
        }

        let model = tui_operations_model(&history);
        assert_eq!(model.rows.len(), TUI_OPERATIONS_MAX_ROWS);
        assert!(model.summary.contains("8 shown / 9 retained operation(s)"));
        assert!(model.summary.contains("1 omitted"));
        assert!(model.rows[0].needs_attention);
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
    fn tui_model_shows_only_authoritative_exact_progress() {
        let mut authoritative = record(
            OperationId::numeric(OperationDomain::ResourceTransfer, 1),
            "authoritative-resource",
            OperationState::Transferring,
            EvidenceAuthority::Authoritative,
            20,
        );
        authoritative.progress = Some(AuthoritativeProgress::new(3, 7).expect("progress"));
        let mut uncertain = record(
            OperationId::numeric(OperationDomain::ResourceTransfer, 2),
            "uncertain-resource",
            OperationState::Transferring,
            EvidenceAuthority::Uncertain,
            10,
        );
        uncertain.progress = Some(AuthoritativeProgress::new(3, 7).expect("progress"));
        let mut history = OperationHistory::default();
        history
            .upsert(authoritative)
            .expect("authoritative fixture");
        history.upsert(uncertain).expect("uncertain fixture");

        let model = tui_operations_model(&history);
        assert!(model
            .rows
            .iter()
            .find(|row| row.headline.contains("authoritative-resource"))
            .expect("authoritative row")
            .headline
            .contains("3/7 bytes"));
        assert!(!model
            .rows
            .iter()
            .find(|row| row.headline.contains("uncertain-resource"))
            .expect("uncertain row")
            .headline
            .contains("3/7 bytes"));
    }

    #[test]
    fn network_doctor_route_renders_operations_instead_of_placeholder() {
        let root = std::env::temp_dir().join(format!(
            "omenbrowser-rs-tui-operations-{}-{}",
            std::process::id(),
            crate::app::current_epoch_ms()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let mut app = App::new(AppConfig {
            paths: AppPaths::from_root(root.clone()),
            settings: AppSettings::default(),
        });
        app.workspace.active_section = WorkspaceSection::NetworkDoctor;
        app.operation_history
            .upsert(record(
                OperationId::numeric(OperationDomain::LxmfMessage, 1),
                "route-visible-peer",
                OperationState::TransportAccepted,
                EvidenceAuthority::Authoritative,
                20,
            ))
            .expect("route fixture");

        let backend = TestBackend::new(120, 35);
        let mut terminal = Terminal::new(backend).expect("test terminal");
        terminal
            .draw(|frame| crate::ui::workspace::render(frame, &app))
            .expect("render Network Doctor");
        let rendered = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();

        assert!(rendered.contains("Operations & Transfers"));
        assert!(rendered.contains("route-visible-peer"));
        assert!(rendered.contains("transport accepted"));
        assert!(!rendered.contains("panel boundary is reserved"));

        drop(terminal);
        drop(app);
        std::fs::remove_dir_all(root).expect("remove isolated TUI root");
    }
}
