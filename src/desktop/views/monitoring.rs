use iced::widget::{column, row, text};
use iced::{Element, Length};

use crate::app::current_epoch_ms;

use super::super::{
    app_scrollable, human_bytes, monitoring_interface_reconnect_line,
    monitoring_interface_status_lines, monitoring_meter, monitoring_metric_card,
    monitoring_runtime_attribution_lines, section_card, ui_size, wrapped_panel_text,
    wrapped_text_owned, DesktopApp, Message,
};

pub(in crate::desktop) fn monitoring_view(desktop: &DesktopApp) -> Element<'_, Message> {
    let monitoring = &desktop.app.monitoring_state;
    let runtime = &desktop.app.runtime_status;
    let resources = desktop.monitoring.process_usage;
    let uptime_secs = current_epoch_ms()
        .saturating_sub(monitoring.started_epoch_ms)
        .max(1)
        / 1_000;
    let event_rate = monitoring.runtime_events_total as f64 / uptime_secs.max(1) as f64;
    let outbound_messages = desktop
        .app
        .workspace
        .conversations
        .iter()
        .flat_map(|conversation| conversation.thread.messages.iter())
        .filter(|message| !message.incoming)
        .count();
    let inbound_messages = desktop
        .app
        .workspace
        .conversations
        .iter()
        .flat_map(|conversation| conversation.thread.messages.iter())
        .filter(|message| message.incoming)
        .count();
    let pending_messages = desktop
        .app
        .workspace
        .conversations
        .iter()
        .filter(|conversation| conversation.pending_send.is_some())
        .count();
    let directory_entries = desktop.app.directory_service.list_entries();
    let live_entries = desktop.app.directory_service.list_live_entries();
    let saved_entries = directory_entries.iter().filter(|entry| entry.saved).count();
    let trusted_entries = directory_entries
        .iter()
        .filter(|entry| entry.trusted)
        .count();

    let traffic_cards = row![
        monitoring_metric_card(
            "RX estimate",
            human_bytes(monitoring.estimated_inbound_bytes),
            format!(
                "{} announces / {} inbound LXMF",
                monitoring.announces_received, monitoring.inbound_messages
            ),
        ),
        monitoring_metric_card(
            "TX estimate",
            human_bytes(monitoring.estimated_outbound_bytes),
            format!(
                "{} page / {} path / {} LXMF",
                monitoring.outbound_page_requests,
                monitoring.outbound_path_requests + monitoring.outbound_path_warmups,
                monitoring.outbound_lxmf_sends
            ),
        ),
        monitoring_metric_card(
            "Runtime events",
            monitoring.runtime_events_total.to_string(),
            format!(
                "{event_rate:.2}/sec, {} debug",
                monitoring.runtime_debug_events
            ),
        ),
    ]
    .spacing(8)
    .wrap();

    let network_lines = column![
        wrapped_text_owned(
            format!(
                "backend: {:?} | connected={} | {}",
                runtime.backend, runtime.connected, runtime.message
            ),
            14
        ),
        wrapped_text_owned(
            monitoring_interface_reconnect_line(monitoring.last_interface_stats.as_ref()),
            14,
        ),
        wrapped_text_owned(
            format!(
                "identity: {}",
                runtime
                    .active_identity
                    .as_ref()
                    .map(|identity| format!("{} / {}", identity.label, identity.hash_hex))
                    .unwrap_or_else(|| "none".into())
            ),
            14
        ),
        wrapped_text_owned(
            format!(
                "path updates: {} | page probes: {} | propagation sync events: {}",
                monitoring.path_updates_received,
                monitoring.page_fetch_probes,
                monitoring.propagation_sync_events
            ),
            14
        ),
        wrapped_text_owned(
            format!(
                "outgoing: pages={} partials={} downloads={} diagnostics={}",
                monitoring.outbound_page_requests,
                monitoring.outbound_partial_refreshes,
                monitoring.outbound_file_downloads,
                monitoring.outbound_diagnostics
            ),
            14
        ),
        wrapped_text_owned(
            format!(
                "outgoing paths/messages: path_requests={} path_warmups={} lxmf_sends={} prop_syncs={}",
                monitoring.outbound_path_requests,
                monitoring.outbound_path_warmups,
                monitoring.outbound_lxmf_sends,
                monitoring.outbound_propagation_syncs
            ),
            14
        ),
        wrapped_text_owned(
            format!(
                "incoming: page_responses={} downloads={} announces={} inbound_lxmf={}",
                monitoring.inbound_page_responses,
                monitoring.inbound_downloads,
                monitoring.announces_received,
                monitoring.inbound_messages
            ),
            14
        ),
        wrapped_text_owned(
            format!(
                "LXMF evidence: {} | outbound status updates: {} | runtime errors: {}",
                monitoring.lxmf_evidence_updates,
                monitoring.outbound_status_updates,
                monitoring.runtime_errors
            ),
            14
        ),
    ]
    .spacing(4);
    let attribution_lines = monitoring_runtime_attribution_lines(monitoring, uptime_secs)
        .into_iter()
        .fold(column![].spacing(4), |lines, line| {
            lines.push(wrapped_text_owned(line, 14))
        });

    let directory_lines = column![
        monitoring_meter(
            "live directory",
            live_entries.len(),
            directory_entries.len().max(1)
        ),
        monitoring_meter("saved", saved_entries, directory_entries.len().max(1)),
        monitoring_meter("trusted", trusted_entries, directory_entries.len().max(1)),
        wrapped_text_owned(
            format!(
                "nodes={} peers={} propagation={} total={}",
                directory_entries
                    .iter()
                    .filter(|entry| entry.kind == crate::directory::DirectoryKind::Node)
                    .count(),
                directory_entries
                    .iter()
                    .filter(|entry| entry.kind == crate::directory::DirectoryKind::Peer)
                    .count(),
                directory_entries
                    .iter()
                    .filter(|entry| entry.kind == crate::directory::DirectoryKind::Propagation)
                    .count(),
                directory_entries.len()
            ),
            14
        ),
    ]
    .spacing(4);

    let message_lines = column![
        monitoring_meter(
            "incoming share",
            inbound_messages,
            inbound_messages + outbound_messages
        ),
        monitoring_meter(
            "outgoing share",
            outbound_messages,
            inbound_messages + outbound_messages
        ),
        wrapped_text_owned(
            format!(
                "conversations={} inbound={} outbound={} pending={pending_messages}",
                desktop.app.workspace.conversations.len(),
                inbound_messages,
                outbound_messages
            ),
            14
        ),
    ]
    .spacing(4);

    let resource_lines = match resources {
        Some(resources) => column![
            monitoring_meter("rss", resources.rss_bytes as usize, 512 * 1024 * 1024),
            wrapped_text_owned(format!("memory: {}", human_bytes(resources.rss_bytes)), 14),
            wrapped_text_owned(
                format!("process cpu time: {:.2}s", resources.cpu_seconds),
                14
            ),
        ]
        .spacing(4),
        None => column![wrapped_text_owned(
            "Process resource stats are unavailable on this platform.",
            14
        )]
        .spacing(4),
    };

    let interface_card = if let Some(stats) = &monitoring.last_interface_stats {
        let interface_lines = monitoring_interface_status_lines(stats);
        section_card(
            "Interfaces",
            interface_lines
                .into_iter()
                .fold(column![].spacing(4), |column, line| {
                    column.push(wrapped_text_owned(line, 14))
                }),
        )
    } else {
        section_card(
            "Interfaces",
            wrapped_panel_text("No runtime interface stats have been sampled yet. Run Diagnostics or native startup to populate rnstatus-like interface data."),
        )
    };
    let omenchat_card = desktop.omenchat_monitoring_card();

    app_scrollable(
        column![
            text("Monitoring").size(ui_size(28)),
            wrapped_panel_text(
                "Runtime traffic and resource pressure for keeping OMENbrowser_rs quiet on Reticulum."
            ),
            traffic_cards,
            row![
                section_card("Network Runtime", network_lines),
                section_card("Runtime Attribution", attribution_lines),
                section_card("Process Resources", resource_lines),
            ]
            .spacing(8)
            .wrap(),
            row![
                section_card("Directory Noise Surface", directory_lines),
                section_card("LXMF Message Mix", message_lines),
            ]
            .spacing(8)
            .wrap(),
            interface_card,
            omenchat_card,
        ]
        .spacing(12)
        .width(Length::Fill),
    )
    .height(Length::Fill)
    .into()
}
