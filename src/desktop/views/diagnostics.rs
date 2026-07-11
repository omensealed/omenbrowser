use iced::widget::{column, text};
use iced::{Element, Length};

use super::super::{
    action_grid, app_scrollable, diagnostics_preview_live_fetch_card,
    diagnostics_preview_lxmf_delivery_card, diagnostics_preview_propagation_sync_card,
    diagnostics_preview_report_summary, diagnostics_preview_stage_cards, omen_button, section_card,
    subtle_button, ui_size, wrapped_text_owned, DesktopApp, Message,
};

pub(in crate::desktop) fn diagnostics_view(desktop: &DesktopApp) -> Element<'_, Message> {
    let summary = diagnostics_preview_report_summary(&desktop.app.diagnostics_state.preview_lines);
    let summary_card = if let Some(summary) = summary {
        section_card(
            format!("Report Summary: {}", summary.report),
            column![
                wrapped_text_owned(format!("outcome: {}", summary.outcome), 15),
                wrapped_text_owned(format!("stage: {}", summary.stage), 15),
                wrapped_text_owned(format!("detail: {}", summary.detail), 14),
                wrapped_text_owned(format!("next: {}", summary.next_step), 14),
            ]
            .spacing(4),
        )
    } else {
        section_card(
            "Report Summary",
            text("Run or preview a native diagnostic report to see outcome/stage/next-step here.")
                .size(ui_size(14)),
        )
    };
    let blockers = desktop
        .native_action_status_lines()
        .into_iter()
        .fold(column![].spacing(3), |column, line| {
            column.push(wrapped_text_owned(line, 14))
        });
    let stage_cards = diagnostics_preview_stage_cards(&desktop.app.diagnostics_state.preview_lines)
        .into_iter()
        .take(12)
        .fold(column![].spacing(8), |column, stage| {
            column.push(section_card(
                format!("{}: {}", stage.kind, stage.stage),
                column![
                    wrapped_text_owned(format!("status: {}", stage.status), 14),
                    wrapped_text_owned(format!("detail: {}", stage.detail), 14),
                    wrapped_text_owned(format!("next: {}", stage.next_step), 14),
                ]
                .spacing(3),
            ))
        });
    let live_fetch_card = if let Some(fetch) =
        diagnostics_preview_live_fetch_card(&desktop.app.diagnostics_state.preview_lines)
    {
        section_card(
            "Live Fetch Result",
            column![
                wrapped_text_owned(format!("outcome: {}", fetch.outcome), 14),
                wrapped_text_owned(format!("stage: {}", fetch.stage_hint), 14),
                wrapped_text_owned(format!("request backend: {}", fetch.request_backend), 14),
                wrapped_text_owned(format!("response: {}", fetch.response_size), 14),
                wrapped_text_owned(format!("detail: {}", fetch.detail), 14),
                wrapped_text_owned(
                    format!("first failed stage: {}", fetch.first_failed_stage),
                    14
                ),
                wrapped_text_owned(format!("next: {}", fetch.next_step), 14),
            ]
            .spacing(3),
        )
    } else {
        section_card(
            "Live Fetch Result",
            wrapped_text_owned(
                "Run Native Live Fetch to see fetch_page stage, backend, and response metadata here.",
                14,
            ),
        )
    };
    let lxmf_delivery_card = if let Some(lxmf) =
        diagnostics_preview_lxmf_delivery_card(&desktop.app.diagnostics_state.preview_lines)
    {
        section_card(
            "LXMF Delivery Result",
            column![
                wrapped_text_owned(format!("outcome: {}", lxmf.outcome), 14),
                wrapped_text_owned(format!("send: {}", lxmf.send_state), 14),
                wrapped_text_owned(format!("proof: {}", lxmf.proof_state), 14),
                wrapped_text_owned(format!("inbound: {}", lxmf.inbound_state), 14),
                wrapped_text_owned(format!("events: {}", lxmf.event_counts), 14),
                wrapped_text_owned(format!("readiness: {}", lxmf.readiness_stage), 14),
                wrapped_text_owned(format!("detail: {}", lxmf.detail), 14),
                wrapped_text_owned(format!("next: {}", lxmf.next_step), 14),
            ]
            .spacing(3),
        )
    } else {
        section_card(
            "LXMF Delivery Result",
            wrapped_text_owned(
                "Run LXMF Interop to see send/proof/inbound evidence here.",
                14,
            ),
        )
    };
    let propagation_sync_card = if let Some(sync) =
        diagnostics_preview_propagation_sync_card(&desktop.app.diagnostics_state.preview_lines)
    {
        let event_lines = sync
            .event_lines
            .iter()
            .fold(column![].spacing(2), |column, line| {
                column.push(wrapped_text_owned(line.clone(), 12))
            });
        section_card(
            "LXMF Propagation Sync",
            column![
                wrapped_text_owned(format!("outcome: {}", sync.outcome), 14),
                wrapped_text_owned(format!("selected node: {}", sync.selected_node), 14),
                wrapped_text_owned(format!("before: {}", sync.before), 14),
                wrapped_text_owned(format!("after: {}", sync.after), 14),
                wrapped_text_owned(format!("events: {}", sync.events), 14),
                section_card("Recent Sync Events", event_lines),
                wrapped_text_owned(format!("blocker: {}", sync.blocker), 14),
                wrapped_text_owned(format!("next: {}", sync.next_step), 14),
            ]
            .spacing(3),
        )
    } else {
        section_card(
            "LXMF Propagation Sync",
            wrapped_text_owned(
                "Run Sync Propagation to see propagation-node /get status, haves/wants, and failures here.",
                14,
            ),
        )
    };
    let preview = desktop
        .app
        .diagnostics_state
        .preview_lines
        .iter()
        .take(80)
        .fold(column![].spacing(3), |column, line| {
            column.push(wrapped_text_owned(line.clone(), 13))
        });
    let snapshot = desktop
        .app
        .diagnostics_state
        .last_snapshot
        .as_ref()
        .cloned()
        .unwrap_or_else(|| "No diagnostics snapshot captured yet.".into());
    let diagnostic_target = column![
        wrapped_text_owned(
            format!(
                "kind: {}",
                desktop
                    .app
                    .diagnostics_state
                    .target_kind
                    .as_deref()
                    .unwrap_or("none")
            ),
            14
        ),
        wrapped_text_owned(
            format!(
                "address: {}",
                desktop
                    .app
                    .diagnostics_state
                    .target_address
                    .as_deref()
                    .unwrap_or("none")
            ),
            14
        ),
        wrapped_text_owned(
            "Browser and conversation Diag buttons update this target before running their report.",
            13,
        ),
    ]
    .spacing(3);

    app_scrollable(
        column![
            text("Diagnostics").size(ui_size(28)),
            section_card("Diagnostic Target", diagnostic_target),
            section_card(
                "Runtime Readiness",
                column![
                    wrapped_text_owned(
                        format!(
                            "backend: {:?} | connected={} | {}",
                            desktop.app.runtime_status.backend,
                            desktop.app.runtime_status.connected,
                            desktop.app.runtime_status.message
                        ),
                        14
                    ),
                    wrapped_text_owned(format!("task: {}", desktop.app.status.task), 14),
                    wrapped_text_owned(format!("identity: {}", desktop.app.status.identity), 14),
                    wrapped_text_owned(desktop.app.native_lxmf_sdk_rpc_probe_line(), 14),
                ]
                .spacing(4),
            ),
            section_card("Native Action Prerequisites", blockers),
            summary_card,
            live_fetch_card,
            lxmf_delivery_card,
            propagation_sync_card,
            section_card("Report Stages", stage_cards),
            section_card(
                "Last Export",
                column![
                    action_grid(
                        vec![
                            omen_button("Native Preflight", Message::NativePreflight),
                            omen_button("Native Dry Smoke", Message::NativeSmokeDryRun),
                            omen_button("Native Live Probe", Message::NativeSmokeLiveProbe),
                            omen_button("Native Live Fetch", Message::NativeLiveFetchValidate),
                            subtle_button("Path Diagnostics", Message::PathDiagnostics),
                            subtle_button(
                                "Known Destinations",
                                Message::BeginKnownDestinationsPreload
                            ),
                        ],
                        3,
                    ),
                    action_grid(
                        vec![
                            omen_button("LXMF Smoke Send", Message::NativeLxmfSmokeSend),
                            omen_button("LXMF Interop", Message::NativeLxmfInterop),
                            omen_button(
                                "Sync Propagation",
                                Message::NativeLxmfPropagationDiagnostics
                            ),
                            subtle_button("Preview Live Report", Message::PreviewLiveInteropReport),
                            subtle_button("Export Live Report", Message::ExportLiveInteropReport),
                            subtle_button("Preview Bundle", Message::PreviewDiagnosticsBundle),
                            subtle_button("Export Bundle", Message::ExportDiagnosticsBundle),
                        ],
                        3
                    ),
                    wrapped_text_owned(
                        format!(
                            "path: {}",
                            desktop
                                .app
                                .diagnostics_state
                                .last_export_path
                                .as_ref()
                                .map(|path| path.display().to_string())
                                .unwrap_or_else(|| "none".into())
                        ),
                        14
                    ),
                    wrapped_text_owned(
                        format!(
                            "summary: {}",
                            desktop
                                .app
                                .diagnostics_state
                                .last_export_summary
                                .as_deref()
                                .unwrap_or("none")
                        ),
                        14
                    ),
                    wrapped_text_owned(
                        format!(
                            "preview scroll: {}",
                            desktop.app.diagnostics_state.preview_scroll
                        ),
                        14
                    ),
                ]
                .spacing(4),
            ),
            section_card("Snapshot", wrapped_text_owned(snapshot, 13),),
            section_card("Preview", preview),
        ]
        .spacing(12)
        .width(Length::Fill),
    )
    .height(Length::Fill)
    .into()
}
