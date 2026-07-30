use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
use ratatui::Frame;

use crate::app::{App, NETWORK_DOCTOR_OPERATION_ROWS};
use crate::input::InputTarget;
use crate::operations::presentation::{
    filter_label, present_operations, OperationPresentationFilter, OperationPresentationQuery,
};
use crate::operations::{EvidenceAuthority, OperationHistory, OperationId};

#[derive(Debug, PartialEq, Eq)]
struct TuiOperationsModel {
    summary: String,
    rows: Vec<TuiOperationRow>,
    empty_message: Option<&'static str>,
    query_rejected: bool,
}

#[derive(Debug, PartialEq, Eq)]
struct TuiOperationRow {
    id: OperationId,
    headline: String,
    evidence: String,
    needs_attention: bool,
}

fn tui_operations_model(
    history: &OperationHistory,
    filter: OperationPresentationFilter,
    search: Option<&str>,
) -> TuiOperationsModel {
    let (query, query_rejected) =
        match OperationPresentationQuery::new(filter, search, NETWORK_DOCTOR_OPERATION_ROWS) {
            Ok(query) => (query, false),
            Err(_) => (
                OperationPresentationQuery::new(filter, None, NETWORK_DOCTOR_OPERATION_ROWS)
                    .expect("the fixed TUI Operations row limit must remain valid"),
                true,
            ),
        };
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
                id: row.id,
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
        query_rejected,
    }
}

pub(super) fn render(frame: &mut Frame, area: Rect, app: &App) {
    let search = app
        .input
        .active
        .as_ref()
        .filter(|active| active.target == InputTarget::OperationsSearch)
        .map(|active| active.buffer.as_str())
        .unwrap_or(&app.network_doctor_state.operations_search);
    let model = tui_operations_model(
        &app.operation_history,
        app.network_doctor_state.operations_filter,
        Some(search),
    );
    let displayed_search = if model.query_rejected {
        "(invalid query)"
    } else if search.is_empty() {
        "(none)"
    } else {
        search
    };
    let mut lines = vec![
        Line::from(format!(
            "filter={} | search={}{}",
            filter_label(app.network_doctor_state.operations_filter),
            displayed_search,
            if model.query_rejected {
                " | invalid query ignored"
            } else {
                ""
            }
        )),
        Line::from("/ search | f filter | c clear search | Esc cancel edit"),
        Line::styled(model.summary, Style::default().fg(Color::Gray)),
        internal_event_queue_line(app),
        Line::from(""),
    ];
    if let Some(empty_message) = model.empty_message {
        lines.push(Line::styled(
            empty_message,
            Style::default().fg(Color::DarkGray),
        ));
    } else {
        for row in model.rows {
            let selected = app.network_doctor_state.selected_operation == Some(row.id);
            let headline_style = if row.needs_attention {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Cyan)
            };
            lines.push(Line::styled(
                format!("{}{}", if selected { "> " } else { "  " }, row.headline),
                if selected {
                    headline_style.add_modifier(Modifier::REVERSED)
                } else {
                    headline_style
                },
            ));
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
        Line::from("Up/Down or j/k select | Enter/v copy-select diagnostic"),
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

    if app.network_doctor_state.operation_select_mode {
        render_operation_select_mode(frame, area, app);
    }
}

fn internal_event_queue_line(app: &App) -> Line<'static> {
    let metrics = app.internal_event_payload_metrics();
    let style = if metrics.rejected_events > 0 {
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Gray)
    };
    Line::styled(
        format!(
            "app event queue: items={}/{} | payload_items={} bytes={}/{} | payload_rejected={}",
            metrics.channel_queued_items,
            metrics.channel_max_items,
            metrics.queued_items,
            metrics.queued_bytes,
            metrics.max_bytes,
            metrics.rejected_events
        ),
        style,
    )
}

fn render_operation_select_mode(frame: &mut Frame, area: Rect, app: &App) {
    let diagnostic = app
        .selected_operation_diagnostic()
        .unwrap_or_else(|| "The selected operation is no longer retained.".into());
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(diagnostic)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Yellow))
                    .title(
                        " Operation Copy/Select | terminal mouse selection enabled | Esc returns ",
                    ),
            )
            .scroll((app.network_doctor_state.operation_diagnostic_scroll, 0))
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
        let model = tui_operations_model(
            &OperationHistory::default(),
            OperationPresentationFilter::All,
            None,
        );

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

        let model = tui_operations_model(&history, OperationPresentationFilter::All, None);
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
        for local_id in 0..NETWORK_DOCTOR_OPERATION_ROWS as u64 {
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

        let model = tui_operations_model(&history, OperationPresentationFilter::All, None);
        assert_eq!(model.rows.len(), NETWORK_DOCTOR_OPERATION_ROWS);
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

        let model = tui_operations_model(&history, OperationPresentationFilter::All, None);
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

    #[test]
    fn copy_select_mode_renders_the_bounded_selected_diagnostic() {
        let root = std::env::temp_dir().join(format!(
            "omenbrowser-rs-tui-operation-select-render-{}-{}",
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
                OperationId::numeric(OperationDomain::PathDiscovery, 1),
                "copyable-path",
                OperationState::Failed,
                EvidenceAuthority::Authoritative,
                20,
            ))
            .expect("selection fixture");
        assert!(app.move_operation_selection(1));
        assert!(app.open_operation_select_mode());

        let backend = TestBackend::new(120, 35);
        let mut terminal = Terminal::new(backend).expect("test terminal");
        terminal
            .draw(|frame| crate::ui::workspace::render(frame, &app))
            .expect("render copy/select mode");
        let rendered = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();

        assert!(rendered.contains("Operation Copy/Select"));
        assert!(rendered.contains("copyable-path"));
        assert!(rendered.contains("terminal mouse selection enabled"));

        drop(terminal);
        drop(app);
        std::fs::remove_dir_all(root).expect("remove isolated TUI root");
    }

    #[test]
    fn operations_render_reports_bounded_event_queue_pressure() {
        let root = std::env::temp_dir().join(format!(
            "omenbrowser-rs-tui-operation-queue-render-{}-{}",
            std::process::id(),
            crate::app::current_epoch_ms()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let mut app = App::new(AppConfig {
            paths: AppPaths::from_root(root.clone()),
            settings: AppSettings::default(),
        });
        app.workspace.active_section = WorkspaceSection::NetworkDoctor;
        assert!(app.enqueue_log_event("queued for visibility"));

        let backend = TestBackend::new(120, 35);
        let mut terminal = Terminal::new(backend).expect("test terminal");
        terminal
            .draw(|frame| crate::ui::workspace::render(frame, &app))
            .expect("render queue visibility");
        let rendered = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();

        assert!(rendered.contains("app event queue: items=1/256"));
        assert!(rendered.contains("payload_items=0 bytes=0/33554432"));
        assert!(rendered.contains("payload_rejected=0"));

        drop(terminal);
        drop(app);
        std::fs::remove_dir_all(root).expect("remove isolated TUI root");
    }

    #[test]
    fn tui_model_reuses_shared_search_and_attention_filters() {
        let mut history = OperationHistory::default();
        history
            .upsert(record(
                OperationId::numeric(OperationDomain::PathDiscovery, 1),
                "ordinary-path",
                OperationState::Active,
                EvidenceAuthority::Authoritative,
                10,
            ))
            .expect("ordinary");
        history
            .upsert(record(
                OperationId::numeric(OperationDomain::OmenChatMutation, 2),
                "attention-room",
                OperationState::Reconciling,
                EvidenceAuthority::Uncertain,
                20,
            ))
            .expect("attention");

        let attention =
            tui_operations_model(&history, OperationPresentationFilter::NeedsAttention, None);
        assert_eq!(attention.rows.len(), 1);
        assert!(attention.rows[0].headline.contains("attention-room"));

        let searched =
            tui_operations_model(&history, OperationPresentationFilter::All, Some("ordinary"));
        assert_eq!(searched.rows.len(), 1);
        assert!(searched.rows[0].headline.contains("ordinary-path"));

        let invalid = "x"
            .repeat(crate::operations::presentation::OPERATION_PRESENTATION_SEARCH_MAX_BYTES + 1);
        let fallback = tui_operations_model(
            &history,
            OperationPresentationFilter::NeedsAttention,
            Some(&invalid),
        );
        assert!(fallback.query_rejected);
        assert_eq!(fallback.rows.len(), 1);
        assert!(fallback.rows[0].headline.contains("attention-room"));
    }
}
