use iced::widget::{column, container, text};
use iced::{Element, Length};

use super::{status_container_style, ui_size, Message};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(in crate::desktop) struct ProcessResourceUsage {
    pub(in crate::desktop) rss_bytes: u64,
    pub(in crate::desktop) cpu_seconds: f64,
}

pub(in crate::desktop) fn process_resource_usage() -> Option<ProcessResourceUsage> {
    let page_size = 4096u64;
    let statm = std::fs::read_to_string("/proc/self/statm").ok()?;
    let rss_pages = statm.split_whitespace().nth(1)?.parse::<u64>().ok()?;
    let stat = std::fs::read_to_string("/proc/self/stat").ok()?;
    let close = stat.rfind(')')?;
    let fields = stat
        .get(close + 2..)?
        .split_whitespace()
        .collect::<Vec<_>>();
    let utime = fields.get(11)?.parse::<u64>().ok()?;
    let stime = fields.get(12)?.parse::<u64>().ok()?;
    let ticks_per_second = 100.0;
    Some(ProcessResourceUsage {
        rss_bytes: rss_pages.saturating_mul(page_size),
        cpu_seconds: (utime + stime) as f64 / ticks_per_second,
    })
}

pub(in crate::desktop) fn monitoring_metric_card<'a>(
    title: &'static str,
    value: String,
    detail: String,
) -> Element<'a, Message> {
    container(
        column![
            text(title).size(ui_size(13)),
            text(value).size(ui_size(22)),
            text(detail).size(ui_size(12)),
        ]
        .spacing(4),
    )
    .style(status_container_style)
    .padding(12)
    .width(Length::FillPortion(1))
    .into()
}

pub(in crate::desktop) fn monitoring_runtime_attribution_lines(
    monitoring: &crate::app::MonitoringPanelState,
    uptime_secs: u64,
) -> Vec<String> {
    let uptime_minutes = (uptime_secs.max(1) as f64 / 60.0).max(1.0 / 60.0);
    let browser_tx = monitoring
        .outbound_page_requests
        .saturating_add(monitoring.outbound_partial_refreshes)
        .saturating_add(monitoring.outbound_file_downloads);
    let path_tx = monitoring
        .outbound_path_requests
        .saturating_add(monitoring.outbound_path_warmups);
    let lxmf_tx = monitoring
        .outbound_lxmf_sends
        .saturating_add(monitoring.outbound_propagation_syncs);
    let app_tx = monitoring
        .outbound_diagnostics
        .saturating_add(monitoring.outbound_status_updates);
    let page_rx = monitoring
        .inbound_page_responses
        .saturating_add(monitoring.inbound_downloads);
    let discovery_rx = monitoring
        .announces_received
        .saturating_add(monitoring.path_updates_received);
    let lxmf_rx = monitoring
        .inbound_messages
        .saturating_add(monitoring.lxmf_evidence_updates)
        .saturating_add(monitoring.propagation_sync_events);
    let total_tx_ops = browser_tx
        .saturating_add(path_tx)
        .saturating_add(lxmf_tx)
        .saturating_add(app_tx);
    let total_rx_ops = page_rx.saturating_add(discovery_rx).saturating_add(lxmf_rx);
    let tx_classes = [
        ("browser", browser_tx),
        ("path", path_tx),
        ("lxmf", lxmf_tx),
        ("app/status", app_tx),
    ];
    let rx_classes = [
        ("pages/files", page_rx),
        ("discovery", discovery_rx),
        ("lxmf", lxmf_rx),
    ];
    let top_tx = dominant_runtime_class(&tx_classes);
    let top_rx = dominant_runtime_class(&rx_classes);
    let tx_per_min = total_tx_ops as f64 / uptime_minutes;
    let rx_per_min = total_rx_ops as f64 / uptime_minutes;
    let outbound_bytes_per_min = monitoring.estimated_outbound_bytes as f64 / uptime_minutes;
    let inbound_bytes_per_min = monitoring.estimated_inbound_bytes as f64 / uptime_minutes;
    let activity_hint = if total_tx_ops == 0 && total_rx_ops == 0 {
        "activity: idle; no runtime traffic recorded yet".into()
    } else {
        format!(
            "activity: top tx={} ({}) | top rx={} ({}) | {} tx/min, {} rx/min",
            top_tx.0,
            top_tx.1,
            top_rx.0,
            top_rx.1,
            format_rate(tx_per_min),
            format_rate(rx_per_min)
        )
    };
    let mut lines = vec![
        "read this: browser spikes mean page/download traffic; path spikes mean route discovery; lxmf spikes mean message/propagation work".into(),
        format!(
            "tx by class: browser={browser_tx} path={path_tx} lxmf={lxmf_tx} app/status={app_tx}"
        ),
        format!("rx by class: pages/files={page_rx} discovery={discovery_rx} lxmf={lxmf_rx}"),
        activity_hint,
        format!(
            "operation rate: {:.1} tx/min | {:.1} rx/min | {:.1} runtime events/min",
            tx_per_min,
            rx_per_min,
            monitoring.runtime_events_total as f64 / uptime_minutes
        ),
        format!(
            "byte estimate: {} tx / {} rx | rate {} tx/min / {} rx/min",
            crate::desktop::human_bytes(monitoring.estimated_outbound_bytes),
            crate::desktop::human_bytes(monitoring.estimated_inbound_bytes),
            crate::desktop::human_bytes(outbound_bytes_per_min.round() as u64),
            crate::desktop::human_bytes(inbound_bytes_per_min.round() as u64)
        ),
    ];
    if monitoring.runtime_errors > 0 {
        lines.push(format!(
            "attention: {} runtime error(s) recorded; inspect Logs and Diagnostics before blaming traffic volume",
            monitoring.runtime_errors
        ));
    }
    let browser_attempts = monitoring
        .outbound_page_requests
        .saturating_add(monitoring.outbound_file_downloads);
    if browser_attempts > 0 {
        lines.push(format!(
            "browser health: {} page/download request(s), {} response/download result(s), {} path operation(s)",
            browser_attempts,
            page_rx,
            path_tx
        ));
    }
    if browser_attempts >= 2 && page_rx == 0 {
        lines.push(
            "attention: browser requests have no page/file responses yet; check Browser live warning, Request Path, then Diagnostics".into(),
        );
    } else if path_tx > browser_attempts.saturating_mul(2).max(3) {
        lines.push(
            "attention: path traffic is high relative to page loads; wait for path pass before retrying repeated links".into(),
        );
    }
    if monitoring.outbound_lxmf_sends > 0 || monitoring.outbound_propagation_syncs > 0 {
        lines.push(format!(
            "LXMF health: sends={} propagation_syncs={} evidence={} inbound={}",
            monitoring.outbound_lxmf_sends,
            monitoring.outbound_propagation_syncs,
            monitoring.lxmf_evidence_updates,
            monitoring.inbound_messages
        ));
    }
    if monitoring.outbound_lxmf_sends > 0 && monitoring.lxmf_evidence_updates == 0 {
        lines.push(
            "attention: LXMF sends have no delivery evidence yet; check selected peer/path and Diagnostics before resending".into(),
        );
    }
    lines
}

fn dominant_runtime_class<'a>(classes: &'a [(&'a str, u64)]) -> (&'a str, u64) {
    classes
        .iter()
        .copied()
        .max_by(|left, right| left.1.cmp(&right.1).then_with(|| right.0.cmp(left.0)))
        .unwrap_or(("none", 0))
}

fn format_rate(value: f64) -> String {
    if value >= 10.0 {
        format!("{value:.0}")
    } else {
        format!("{value:.1}")
    }
}

pub(in crate::desktop) fn monitoring_meter<'a>(
    label: &'static str,
    value: usize,
    max: usize,
) -> Element<'a, Message> {
    let max = max.max(1);
    let filled = ((value.min(max) * 18) + max / 2) / max;
    let empty = 18usize.saturating_sub(filled);
    let percent = (value.min(max) * 100) / max;
    text(format!(
        "{label:<16} [{}{}] {:>3}% ({value}/{max})",
        "#".repeat(filled),
        ".".repeat(empty),
        percent
    ))
    .size(ui_size(14))
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::MonitoringPanelState;

    #[test]
    fn monitoring_runtime_attribution_lines_group_runtime_traffic() {
        let monitoring = MonitoringPanelState {
            runtime_events_total: 120,
            outbound_page_requests: 3,
            outbound_partial_refreshes: 2,
            outbound_file_downloads: 1,
            outbound_path_requests: 4,
            outbound_path_warmups: 5,
            outbound_lxmf_sends: 6,
            outbound_propagation_syncs: 7,
            outbound_diagnostics: 8,
            outbound_status_updates: 9,
            inbound_page_responses: 10,
            inbound_downloads: 11,
            announces_received: 12,
            path_updates_received: 13,
            inbound_messages: 14,
            lxmf_evidence_updates: 15,
            propagation_sync_events: 16,
            estimated_outbound_bytes: 2048,
            estimated_inbound_bytes: 4096,
            ..MonitoringPanelState::default()
        };

        let lines = monitoring_runtime_attribution_lines(&monitoring, 120);

        assert!(lines[0].contains("browser spikes"));
        assert!(lines[1].contains("browser=6 path=9 lxmf=13 app/status=17"));
        assert!(lines[2].contains("pages/files=21 discovery=25 lxmf=45"));
        assert!(lines[3].contains("top tx=app/status (17)"));
        assert!(lines[3].contains("top rx=lxmf (45)"));
        assert!(lines[3].contains("22 tx/min"));
        assert!(lines[3].contains("46 rx/min"));
        assert!(lines[4].contains("22.5 tx/min"));
        assert!(lines[4].contains("45.5 rx/min"));
        assert!(lines[4].contains("60.0 runtime events/min"));
        assert!(lines[5].contains("2.0 KiB tx / 4.0 KiB rx"));
        assert!(lines[5].contains("1.0 KiB tx/min / 2.0 KiB rx/min"));
        assert!(lines
            .iter()
            .any(|line| line.contains("browser health: 4 page/download request(s)")));
        assert!(lines.iter().any(|line| {
            line.contains("LXMF health: sends=6 propagation_syncs=7 evidence=15 inbound=14")
        }));
    }

    #[test]
    fn monitoring_runtime_attribution_lines_explain_idle_state() {
        let lines = monitoring_runtime_attribution_lines(&MonitoringPanelState::default(), 60);

        assert!(lines
            .iter()
            .any(|line| line.contains("activity: idle; no runtime traffic recorded yet")));
    }

    #[test]
    fn monitoring_runtime_attribution_lines_surface_runtime_errors() {
        let monitoring = MonitoringPanelState {
            runtime_errors: 2,
            outbound_page_requests: 1,
            ..MonitoringPanelState::default()
        };

        let lines = monitoring_runtime_attribution_lines(&monitoring, 60);

        assert!(lines.iter().any(|line| {
            line.contains("attention: 2 runtime error(s)")
                && line.contains("Logs")
                && line.contains("Diagnostics")
        }));
    }

    #[test]
    fn monitoring_runtime_attribution_lines_flag_browser_delivery_gaps() {
        let monitoring = MonitoringPanelState {
            outbound_page_requests: 2,
            outbound_path_requests: 5,
            ..MonitoringPanelState::default()
        };

        let lines = monitoring_runtime_attribution_lines(&monitoring, 60);

        assert!(lines.iter().any(|line| {
            line.contains("attention: browser requests have no page/file responses yet")
                && line.contains("Request Path")
                && line.contains("Diagnostics")
        }));
    }

    #[test]
    fn monitoring_runtime_attribution_lines_flag_lxmf_evidence_gaps() {
        let monitoring = MonitoringPanelState {
            outbound_lxmf_sends: 3,
            lxmf_evidence_updates: 0,
            ..MonitoringPanelState::default()
        };

        let lines = monitoring_runtime_attribution_lines(&monitoring, 60);

        assert!(lines.iter().any(|line| {
            line.contains("attention: LXMF sends have no delivery evidence yet")
                && line.contains("selected peer/path")
        }));
    }
}
