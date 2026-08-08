use super::{BrowserProbeSummary, DirectoryKind};
use crate::browser::BrowserAddress;
use crate::runtime::{
    AnnouncePayload, PageFetchProbeReport, PageFetchProbeStage, PageFetchProbeStep,
};

impl BrowserProbeSummary {
    pub(super) fn from_report(report: &PageFetchProbeReport, mode: &str) -> Self {
        let failed = report.steps.iter().find(|step| !step.ok);
        let status = if report.ready_to_request {
            "ready"
        } else if failed.is_some() {
            "blocked"
        } else {
            "checked"
        };
        let mut detail = failed
            .map(|step| {
                format!(
                    "{}: {}",
                    page_fetch_probe_stage_label(&step.stage),
                    step.detail
                )
            })
            .or_else(|| report.steps.last().map(|step| step.detail.clone()))
            .unwrap_or_else(|| "no probe steps reported".into());
        if let Some(guidance) = page_fetch_probe_retry_guidance(report) {
            detail = format!("{detail}; {guidance}");
        }

        Self {
            url: report.url.clone(),
            mode: mode.into(),
            ready_to_request: report.ready_to_request,
            status: status.into(),
            detail,
        }
    }

    pub(super) fn from_path_discovery_report(report: &serde_json::Value) -> Self {
        let url = report
            .get("active_browser_url")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_string();
        let wait = report
            .get("path_warmup")
            .and_then(|value| value.get("wait"));
        let last_attempt = wait
            .and_then(|value| value.get("attempts"))
            .and_then(serde_json::Value::as_array)
            .and_then(|attempts| attempts.last());
        let has_path = last_attempt
            .and_then(|attempt| attempt.get("has_path"))
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);
        let known_identity = last_attempt
            .and_then(|attempt| attempt.get("inspection"))
            .and_then(|inspection| inspection.get("known_identity"))
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);
        let first_failed_stage = last_attempt
            .and_then(|attempt| attempt.get("dry_run_page_probe"))
            .and_then(|probe| probe.get("first_failed_stage"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or("unknown");
        let warmup_status = report
            .get("path_warmup")
            .and_then(|value| value.get("status"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or("unknown");
        let status = if has_path {
            "path known"
        } else if known_identity {
            "path unknown"
        } else {
            "identity unknown"
        };
        let detail = if has_path {
            "path discovery reports a known route; press Retry/Open or run N to re-check request readiness".into()
        } else if known_identity {
            format!(
                "destination identity known, path not known; warmup={warmup_status}, first failed stage={first_failed_stage}; retry guidance: wait 5-10s for a PathUpdated event after request_path, then press Retry or run Request Path again"
            )
        } else {
            format!(
                "destination identity not known; warmup={warmup_status}, first failed stage={first_failed_stage}"
            )
        };

        Self {
            url,
            mode: "path-discovery".into(),
            ready_to_request: has_path,
            status: status.into(),
            detail,
        }
    }

    pub(super) fn from_native_browser_load_failure(
        target: &str,
        step: &PageFetchProbeStep,
    ) -> Self {
        let status = match step.stage {
            PageFetchProbeStage::DestinationIdentity => "identity unknown",
            PageFetchProbeStage::PathDiscovery => "path unknown",
            PageFetchProbeStage::LinkSetup => "link setup failed",
            PageFetchProbeStage::RequestSend => "request send failed",
            PageFetchProbeStage::ResponseWait => "response wait failed",
            PageFetchProbeStage::ResponseDecode => "response decode failed",
            _ => "blocked",
        };
        let next_action = match step.stage {
            PageFetchProbeStage::DestinationIdentity => {
                "wait for announce, preload known_destinations, or run Diagnostics D"
            }
            PageFetchProbeStage::PathDiscovery => {
                "run Diagnostics D to request path and inspect dry-run state"
            }
            PageFetchProbeStage::LinkSetup => {
                "run Diagnostics X or L for a live probe and inspect Reticulum 0.9 link setup"
            }
            PageFetchProbeStage::RequestSend => {
                "run Diagnostics X or L and inspect request payload/path send traces"
            }
            PageFetchProbeStage::ResponseWait => {
                "run Diagnostics X or L; verify node availability and response timeout"
            }
            PageFetchProbeStage::ResponseDecode => {
                "run Diagnostics X or L; inspect returned payload bytes/content type"
            }
            _ => "run N for dry-run readiness or Diagnostics D for path state",
        };
        let mode = match step.stage {
            PageFetchProbeStage::DestinationIdentity | PageFetchProbeStage::PathDiscovery => {
                "path-discovery"
            }
            _ => "native-load",
        };
        let mut detail = format!(
            "{}: {}; next: {next_action}",
            page_fetch_probe_stage_label(&step.stage),
            step.detail
        );
        if page_fetch_probe_step_queued_path_request(step) {
            detail.push_str("; retry guidance: request_path was queued; wait 5-10s for path discovery, then press Retry/Open. If no PathUpdated event appears, run Request Path or Live Probe again.");
        }

        Self {
            url: target.into(),
            mode: mode.into(),
            ready_to_request: false,
            status: status.into(),
            detail,
        }
    }
}

pub(super) fn page_fetch_probe_step_queued_path_request(step: &PageFetchProbeStep) -> bool {
    if step.stage != PageFetchProbeStage::PathDiscovery || step.ok {
        return false;
    }
    let detail = step.detail.to_ascii_lowercase();
    detail.contains("queued")
        || detail.contains("request_path")
        || step.trace.iter().any(|(key, value)| {
            let key = key.to_ascii_lowercase();
            let value = value.to_ascii_lowercase();
            (key.contains("request_path") || key.contains("path_request"))
                && (value.contains("queued") || value == "true")
        })
}

fn page_fetch_probe_retry_guidance(report: &PageFetchProbeReport) -> Option<&'static str> {
    report
        .steps
        .iter()
        .any(page_fetch_probe_step_queued_path_request)
        .then_some(
            "retry guidance: request_path was queued; wait 5-10s for PathUpdated/announce evidence, then press Retry/Open. If no update appears, run Request Path or Live Probe again",
        )
}

pub(super) fn page_fetch_probe_stage_label(stage: &PageFetchProbeStage) -> &'static str {
    match stage {
        PageFetchProbeStage::AddressParse => "address",
        PageFetchProbeStage::RuntimeSetup => "runtime",
        PageFetchProbeStage::DestinationIdentity => "identity",
        PageFetchProbeStage::PathDiscovery => "path",
        PageFetchProbeStage::LinkSetup => "link",
        PageFetchProbeStage::RequestSend => "request",
        PageFetchProbeStage::ResponseWait => "response",
        PageFetchProbeStage::ResponseDecode => "decode",
    }
}

pub(super) fn format_page_probe_trace_log_line(
    mode: &str,
    index: usize,
    step: &PageFetchProbeStep,
) -> String {
    let mut line = format!(
        "{mode} page probe step {} {} {}: {}",
        index + 1,
        page_fetch_probe_stage_label(&step.stage),
        if step.ok { "ok" } else { "failed" },
        step.detail
    );
    if !step.trace.is_empty() {
        let trace = step
            .trace
            .iter()
            .map(|(key, value)| format!("{key}={value}"))
            .collect::<Vec<_>>()
            .join(" ");
        line.push_str(" | ");
        line.push_str(&trace);
    }
    line
}

fn native_page_load_failure_needs_probe_hint(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    message.contains("native Reticulum")
        || message.contains("Reticulum path")
        || message.contains("rns-net")
        || message.contains("destination identity")
        || browser_load_failure_is_app_timeout(&lower)
}

pub(super) fn browser_load_failure_is_app_timeout(lower_message: &str) -> bool {
    lower_message.contains("timed out after") && lower_message.contains("request cancelled")
}

pub(super) fn compact_app_timeout_status(message: &str) -> String {
    let timeout = message
        .split(';')
        .next()
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .unwrap_or("browser request timed out");
    format!("{timeout}; press Retry when path/link is ready")
}

pub(super) fn native_browser_load_failure_step(
    target: &str,
    message: &str,
) -> Option<PageFetchProbeStep> {
    if !native_page_load_failure_needs_probe_hint(message) {
        return None;
    }

    let stage = native_page_load_failure_stage(message);
    let suggested_action = native_browser_load_failure_status_action(&stage);
    let mut step = PageFetchProbeStep::failed(stage, message)
        .with_trace("origin", "browser_load")
        .with_trace("target", target)
        .with_trace("suggested_action", suggested_action);
    if let Some(address) = BrowserAddress::parse(target) {
        step = step
            .with_trace("destination", address.destination)
            .with_trace("path", address.path);
    }
    Some(step)
}

pub(super) fn native_page_load_failure_stage(message: &str) -> PageFetchProbeStage {
    let lower = message.to_lowercase();
    if lower.contains("identity") || lower.contains("signing public key") {
        PageFetchProbeStage::DestinationIdentity
    } else if browser_load_failure_is_app_timeout(&lower) {
        PageFetchProbeStage::ResponseWait
    } else if lower.contains("path") {
        PageFetchProbeStage::PathDiscovery
    } else if lower.contains("link") {
        PageFetchProbeStage::LinkSetup
    } else if lower.contains("decode") {
        PageFetchProbeStage::ResponseDecode
    } else if lower.contains("response") {
        PageFetchProbeStage::ResponseWait
    } else if lower.contains("send") || lower.contains("request") {
        PageFetchProbeStage::RequestSend
    } else if lower.contains("timed out") {
        PageFetchProbeStage::ResponseWait
    } else {
        PageFetchProbeStage::RuntimeSetup
    }
}

pub(super) fn native_browser_load_failure_status_action(
    stage: &PageFetchProbeStage,
) -> &'static str {
    match stage {
        PageFetchProbeStage::DestinationIdentity | PageFetchProbeStage::PathDiscovery => {
            "run Diagnostics D for path state"
        }
        PageFetchProbeStage::LinkSetup
        | PageFetchProbeStage::RequestSend
        | PageFetchProbeStage::ResponseWait
        | PageFetchProbeStage::ResponseDecode => {
            "run Diagnostics X or L for link/request/response report"
        }
        _ => "run N for dry-run readiness",
    }
}

pub(super) fn native_browser_load_failure_allows_one_auto_retry(
    stage: &PageFetchProbeStage,
) -> bool {
    // Link setup has not dispatched the executable NomadNet request. Once
    // request construction/send or response waiting is reached, the remote
    // outcome can be uncertain and retry must remain an explicit user action.
    matches!(stage, PageFetchProbeStage::LinkSetup)
}

pub(super) fn browser_probe_summary_waits_for_network_evidence(
    summary: &BrowserProbeSummary,
) -> bool {
    if summary.ready_to_request || summary.status == "running" {
        return false;
    }

    matches!(
        summary.status.as_str(),
        "blocked" | "suggested" | "path known" | "path unknown"
    ) || summary.detail.contains("identity")
        || summary.detail.contains("signing key")
        || summary.detail.contains("path")
}

pub(super) fn announce_matches_browser_probe(
    announce: &AnnouncePayload,
    summary_url: &str,
    tab_address_input: &str,
) -> bool {
    let mut candidate_hashes = vec![announce.destination_hash.as_str()];
    if let Some(hash) = announce.associated_hash.as_deref() {
        candidate_hashes.push(hash);
    }
    if let Some(hash) = announce.node_associated_hash.as_deref() {
        candidate_hashes.push(hash);
    }

    browser_address_matches_any_hash(summary_url, &candidate_hashes)
        || browser_address_matches_any_hash(tab_address_input, &candidate_hashes)
        || candidate_hashes
            .iter()
            .any(|hash| summary_url.contains(hash))
}

fn browser_address_matches_any_hash(input: &str, hashes: &[&str]) -> bool {
    BrowserAddress::parse(input)
        .map(|address| {
            hashes
                .iter()
                .any(|hash| address.destination.eq_ignore_ascii_case(hash))
        })
        .unwrap_or(false)
}

pub(super) fn announce_browser_probe_status(kind: &DirectoryKind) -> &'static str {
    match kind {
        DirectoryKind::Node => "identity known",
        DirectoryKind::Peer => "peer known",
        DirectoryKind::Propagation => "propagation known",
        DirectoryKind::OmenChat => "OMENchat known",
        DirectoryKind::Unknown => "announce seen",
    }
}

pub(super) fn announce_kind_label(kind: &DirectoryKind) -> &'static str {
    match kind {
        DirectoryKind::Node => "node",
        DirectoryKind::Peer => "peer",
        DirectoryKind::Propagation => "propagation",
        DirectoryKind::OmenChat => "omenchat",
        DirectoryKind::Unknown => "unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::RuntimeBackendName;

    #[test]
    fn queued_path_request_trace_enables_retry_guidance() {
        let step = PageFetchProbeStep::failed(
            PageFetchProbeStage::PathDiscovery,
            "path request still pending",
        )
        .with_trace("request_path", "queued");
        let report = PageFetchProbeReport {
            backend: RuntimeBackendName::Reticulum,
            url: "0123456789abcdef:/".into(),
            destination_hash: Some("0123456789abcdef".into()),
            path: Some("/".into()),
            execute_request: false,
            ready_to_request: false,
            steps: vec![step.clone()],
        };

        assert!(page_fetch_probe_step_queued_path_request(&step));
        assert!(BrowserProbeSummary::from_report(&report, "dry-run")
            .detail
            .contains("wait 5-10s"));
    }

    #[test]
    fn native_failure_stage_preserves_timeout_and_decode_distinction() {
        assert_eq!(
            native_page_load_failure_stage(
                "browser request timed out after 30s; request cancelled"
            ),
            PageFetchProbeStage::ResponseWait
        );
        assert_eq!(
            native_page_load_failure_stage("response decode rejected trailing data"),
            PageFetchProbeStage::ResponseDecode
        );
    }

    #[test]
    fn automatic_retry_stops_before_uncertain_request_dispatch() {
        assert!(native_browser_load_failure_allows_one_auto_retry(
            &PageFetchProbeStage::LinkSetup
        ));
        assert!(!native_browser_load_failure_allows_one_auto_retry(
            &PageFetchProbeStage::RequestSend
        ));
        assert!(!native_browser_load_failure_allows_one_auto_retry(
            &PageFetchProbeStage::ResponseWait
        ));
    }
}
