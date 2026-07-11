use iced::widget::{column, row, text};
use iced::{Element, Length};

use crate::app::current_epoch_ms;
use crate::workspace::WorkspaceSection;

use super::super::{
    action_grid, app_scrollable, compact_elapsed_ms, human_bytes,
    monitoring_interface_reconnect_line, monitoring_interface_status_lines, recent_activity_column,
    section_card, subtle_button, ui_size, wrapped_panel_text, wrapped_text_owned, DesktopApp,
    Message,
};
use super::network_doctor_model::{
    network_doctor_active_resource_rows, network_doctor_health_summary,
    network_doctor_messaging_summary, network_doctor_path_rows, network_doctor_transfer_rows,
};

pub(in crate::desktop) fn network_doctor_view(desktop: &DesktopApp) -> Element<'_, Message> {
    let monitoring = &desktop.app.monitoring_state;
    let runtime = &desktop.app.runtime_status;
    let uptime_secs = current_epoch_ms()
        .saturating_sub(monitoring.started_epoch_ms)
        .max(1)
        / 1_000;
    let pending_messages = desktop
        .app
        .workspace
        .conversations
        .iter()
        .filter(|conversation| conversation.pending_send.is_some())
        .count();
    let directory_entries = desktop.app.directory_service.list_entries();
    let saved_entries = directory_entries.iter().filter(|entry| entry.saved).count();
    let trusted_entries = directory_entries
        .iter()
        .filter(|entry| entry.trusted)
        .count();
    let health_summary = network_doctor_health_summary(runtime, monitoring, pending_messages);
    let messaging_summary =
        network_doctor_messaging_summary(&desktop.app.workspace.conversations, monitoring);
    let path_rows = network_doctor_path_rows(monitoring);
    let transfer_rows =
        network_doctor_transfer_rows(&desktop.app.workspace.conversations, monitoring);
    let recent_path_rows = desktop.app.network_doctor_state.recent_paths.clone();
    let recent_link_rows = desktop.app.network_doctor_state.recent_links.clone();
    let recent_resource_rows = desktop.app.network_doctor_state.recent_resources.clone();
    let recent_lxmf_rows = desktop.app.network_doctor_state.recent_lxmf.clone();
    let active_resource_rows =
        network_doctor_active_resource_rows(&desktop.app.network_doctor_state.active_resources);
    let now_epoch_ms = current_epoch_ms();

    let health_card = section_card(
        "Health Summary",
        column![
            wrapped_text_owned(health_summary.runtime_line, 14),
            wrapped_text_owned(health_summary.interface_line, 14),
            wrapped_text_owned(health_summary.traffic_line, 14),
            health_summary
                .attention_lines
                .into_iter()
                .fold(column![].spacing(4), |column, line| {
                    column.push(wrapped_text_owned(line, 13))
                }),
        ]
        .spacing(4),
    );

    let identity_card = section_card(
        "Identity",
        column![
            wrapped_text_owned(
                format!("status identity: {}", desktop.app.status.identity),
                14
            ),
            wrapped_text_owned(
                runtime
                    .active_identity
                    .as_ref()
                    .map(|identity| {
                        format!(
                            "runtime identity: {} / {}",
                            identity.label, identity.hash_hex
                        )
                    })
                    .unwrap_or_else(|| "runtime identity: none".into()),
                14
            ),
            wrapped_text_owned(
                format!("app root: {}", desktop.app.paths.root.display()),
                14
            ),
            wrapped_text_owned(
                format!(
                    "managed reticulum config: {}",
                    desktop.app.paths.reticulum_config_dir.display()
                ),
                14
            ),
        ]
        .spacing(4),
    );

    let runtime_card = section_card(
        "Runtime Backend",
        column![
            wrapped_text_owned(
                format!(
                    "backend: {:?} | connected={} | {}",
                    runtime.backend, runtime.connected, runtime.message
                ),
                14
            ),
            wrapped_text_owned(format!("task: {}", desktop.app.status.task), 14),
            wrapped_text_owned(
                format!("uptime: {}", compact_elapsed_ms(uptime_secs * 1_000)),
                14
            ),
            wrapped_text_owned(
                monitoring_interface_reconnect_line(monitoring.last_interface_stats.as_ref()),
                14
            ),
        ]
        .spacing(4),
    );

    let interface_card = if let Some(stats) = &monitoring.last_interface_stats {
        section_card(
            "Interfaces",
            monitoring_interface_status_lines(stats)
                .into_iter()
                .fold(column![].spacing(4), |column, line| {
                    column.push(wrapped_text_owned(line, 13))
                }),
        )
    } else {
        section_card(
            "Interfaces",
            column![
                wrapped_panel_text("No interface snapshot has been sampled yet."),
                wrapped_panel_text("Open Interfaces, Monitoring, or run Diagnostics to populate runtime interface status."),
            ]
            .spacing(4),
        )
    };

    let path_card = section_card(
        "Paths",
        column![
            wrapped_text_owned(
                format!(
                    "path operations: {} requests / {} warmups / {} updates",
                    monitoring.outbound_path_requests,
                    monitoring.outbound_path_warmups,
                    monitoring.path_updates_received
                ),
                14
            ),
            path_rows
                .into_iter()
                .fold(column![].spacing(4), |column, row| {
                    column.push(wrapped_text_owned(row.display_line(), 13))
                }),
            recent_activity_column(
                "recent path activity",
                ("target", "state", "detail"),
                "no recent path activity recorded",
                recent_path_rows,
                now_epoch_ms,
            ),
            wrapped_panel_text("Detailed per-destination path state is not normalized into the desktop facade yet. Use Diagnostics for current path reports."),
            action_grid(
                vec![
                    subtle_button("Path Diagnostics", Message::PathDiagnostics),
                    subtle_button(
                        "Diagnostics",
                        Message::SwitchSection(WorkspaceSection::Diagnostics)
                    ),
                ],
                2,
            ),
        ]
        .spacing(4),
    );

    let resource_card = section_card(
        "Resources / Transfers",
        column![
            wrapped_text_owned(messaging_summary.resource_line.clone(), 14),
            transfer_rows
                .into_iter()
                .fold(column![].spacing(4), |column, row| {
                    column.push(wrapped_text_owned(row.display_line(), 13))
                }),
            recent_activity_column(
                "current transfers",
                ("transfer", "state", "detail"),
                "no active or recently completed transfers tracked",
                active_resource_rows,
                now_epoch_ms,
            ),
            recent_activity_column(
                "recent link activity",
                ("link", "state", "detail"),
                "no recent link activity recorded",
                recent_link_rows,
                now_epoch_ms,
            ),
            recent_activity_column(
                "recent resource activity",
                ("transfer", "state", "detail"),
                "no recent resource activity recorded",
                recent_resource_rows,
                now_epoch_ms,
            ),
            wrapped_text_owned(
                format!(
                    "browser files: {} outbound downloads / {} inbound downloads",
                    monitoring.outbound_file_downloads, monitoring.inbound_downloads
                ),
                14
            ),
            wrapped_text_owned(
                format!(
                    "traffic estimate: {} rx / {} tx",
                    human_bytes(monitoring.estimated_inbound_bytes),
                    human_bytes(monitoring.estimated_outbound_bytes)
                ),
                14
            ),
            wrapped_panel_text("Current transfers are keyed by transfer ID and updated by typed runtime resource events. Recent activity remains as an append-only operator log."),
        ]
        .spacing(4),
    );

    let lxmf_card = section_card(
        "LXMF Queue",
        column![
            wrapped_text_owned(messaging_summary.conversation_line, 14),
            wrapped_text_owned(messaging_summary.delivery_line, 14),
            wrapped_text_owned(messaging_summary.ticket_line, 14),
            recent_activity_column(
                "recent LXMF activity",
                ("peer/event", "state", "detail"),
                "no recent LXMF activity recorded",
                recent_lxmf_rows,
                now_epoch_ms,
            ),
            wrapped_text_owned(
                format!(
                    "conversations: {} | pending drafts/sends: {}",
                    desktop.app.workspace.conversations.len(),
                    pending_messages
                ),
                14
            ),
            wrapped_text_owned(
                format!(
                    "lxmf sends: {} | propagation syncs: {} | evidence updates: {} | inbound: {}",
                    monitoring.outbound_lxmf_sends,
                    monitoring.outbound_propagation_syncs,
                    monitoring.lxmf_evidence_updates,
                    monitoring.inbound_messages
                ),
                14
            ),
            wrapped_panel_text("Detailed ticket/stamp/queued state is shown inside each LXMF conversation today; Network Doctor will consume normalized LXMF service events next."),
        ]
        .spacing(4),
    );

    let directory_card = section_card(
        "Directory / Discovery",
        column![
            wrapped_text_owned(
                format!(
                    "entries: {} saved / {} trusted / {} total",
                    saved_entries,
                    trusted_entries,
                    directory_entries.len()
                ),
                14
            ),
            wrapped_text_owned(
                format!(
                    "announces: {} | live entries: {}",
                    monitoring.announces_received,
                    desktop.app.directory_service.list_live_entries().len()
                ),
                14
            ),
            subtle_button(
                "Open Directory",
                Message::SwitchSection(WorkspaceSection::Directory)
            ),
        ]
        .spacing(4),
    );

    app_scrollable(
        column![
            text("Network Doctor").size(ui_size(28)),
            wrapped_panel_text("Passive runtime dashboard for Reticulum, LXMF, OMENchat, and NomadNet state. Opening this view does not start probes or touch live user Reticulum/NomadNet/LXMF config directories."),
            health_card,
            row![identity_card, runtime_card].spacing(8).wrap(),
            interface_card,
            row![path_card, resource_card].spacing(8).wrap(),
            row![lxmf_card, directory_card].spacing(8).wrap(),
            desktop.omenchat_monitoring_card(),
        ]
        .spacing(12)
        .width(Length::Fill),
    )
    .height(Length::Fill)
    .into()
}
