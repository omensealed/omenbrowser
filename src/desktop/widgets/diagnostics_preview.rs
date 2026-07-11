#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::desktop) struct DiagnosticsReportSummary {
    pub(in crate::desktop) report: String,
    pub(in crate::desktop) outcome: String,
    pub(in crate::desktop) stage: String,
    pub(in crate::desktop) detail: String,
    pub(in crate::desktop) next_step: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::desktop) struct DiagnosticsStageCard {
    pub(in crate::desktop) kind: String,
    pub(in crate::desktop) stage: String,
    pub(in crate::desktop) status: String,
    pub(in crate::desktop) detail: String,
    pub(in crate::desktop) next_step: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::desktop) struct DiagnosticsLiveFetchCard {
    pub(in crate::desktop) outcome: String,
    pub(in crate::desktop) stage_hint: String,
    pub(in crate::desktop) request_backend: String,
    pub(in crate::desktop) response_size: String,
    pub(in crate::desktop) detail: String,
    pub(in crate::desktop) first_failed_stage: String,
    pub(in crate::desktop) next_step: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::desktop) struct DiagnosticsLxmfDeliveryCard {
    pub(in crate::desktop) outcome: String,
    pub(in crate::desktop) send_state: String,
    pub(in crate::desktop) proof_state: String,
    pub(in crate::desktop) inbound_state: String,
    pub(in crate::desktop) event_counts: String,
    pub(in crate::desktop) readiness_stage: String,
    pub(in crate::desktop) detail: String,
    pub(in crate::desktop) next_step: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::desktop) struct DiagnosticsPropagationSyncCard {
    pub(in crate::desktop) outcome: String,
    pub(in crate::desktop) selected_node: String,
    pub(in crate::desktop) before: String,
    pub(in crate::desktop) after: String,
    pub(in crate::desktop) events: String,
    pub(in crate::desktop) event_lines: Vec<String>,
    pub(in crate::desktop) blocker: String,
    pub(in crate::desktop) next_step: String,
}

pub(in crate::desktop) fn diagnostics_preview_report_summary(
    lines: &[String],
) -> Option<DiagnosticsReportSummary> {
    let value: serde_json::Value = serde_json::from_str(&lines.join("\n")).ok()?;
    let report = string_field(&value, &["report"]).unwrap_or_else(|| "diagnostics".into());
    if let Some(classification) = value.get("classification") {
        return Some(DiagnosticsReportSummary {
            report,
            outcome: string_field(classification, &["outcome"]).unwrap_or_else(|| "unknown".into()),
            stage: string_field(classification, &["stage"]).unwrap_or_else(|| "unknown".into()),
            detail: string_field(classification, &["detail", "reason"])
                .unwrap_or_else(|| "no detail in report".into()),
            next_step: string_field(classification, &["next_step", "next_action"])
                .unwrap_or_else(|| "inspect full report".into()),
        });
    }

    if value.get("ready_to_request").is_some() && value.get("steps").is_some() {
        let ready = value
            .get("ready_to_request")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);
        let failed = value
            .get("steps")
            .and_then(serde_json::Value::as_array)
            .and_then(|steps| {
                steps.iter().find(|step| {
                    !step
                        .get("ok")
                        .and_then(serde_json::Value::as_bool)
                        .unwrap_or(false)
                })
            });
        return Some(DiagnosticsReportSummary {
            report: string_field(&value, &["url"]).unwrap_or(report),
            outcome: if ready {
                "ready".into()
            } else {
                "blocked".into()
            },
            stage: failed
                .and_then(|step| string_field(step, &["stage"]))
                .unwrap_or_else(|| "ready_to_request".into()),
            detail: failed
                .and_then(|step| string_field(step, &["detail"]))
                .unwrap_or_else(|| {
                    if ready {
                        "page fetch prerequisites passed".into()
                    } else {
                        "page fetch prerequisites blocked".into()
                    }
                }),
            next_step: if ready {
                "run a live probe or open the page".into()
            } else {
                "inspect failed probe stage and warm/preload paths as needed".into()
            },
        });
    }

    if let Some(status) = value
        .get("path_warmup")
        .and_then(|path_warmup| string_field(path_warmup, &["status"]))
    {
        return Some(DiagnosticsReportSummary {
            report,
            outcome: status.clone(),
            stage: "path_warmup".into(),
            detail: string_field(&value, &["active_browser_url"])
                .or_else(|| string_field(&value, &["destination_hash"]))
                .unwrap_or_else(|| "path warmup report".into()),
            next_step: if status == "path known" || status == "known" {
                "retry the browser request".into()
            } else {
                "wait for path discovery or preload known_destinations".into()
            },
        });
    }

    None
}

pub(in crate::desktop) fn diagnostics_preview_live_fetch_card(
    lines: &[String],
) -> Option<DiagnosticsLiveFetchCard> {
    let value: serde_json::Value = serde_json::from_str(&lines.join("\n")).ok()?;
    let live_fetch = value.get("live_fetch")?;
    let ok = live_fetch
        .get("ok")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    let stage_hint = string_field(live_fetch, &["stage_hint"])
        .or_else(|| first_failed_page_probe_stage_from_report(&value))
        .unwrap_or_else(|| "unknown".into());
    let request_backend = live_fetch
        .get("metadata")
        .and_then(|metadata| string_field(metadata, &["native_request_backend"]))
        .unwrap_or_else(|| {
            if ok {
                "missing metadata".into()
            } else {
                "not reached".into()
            }
        });
    let response_size = if ok {
        let bytes = live_fetch
            .get("markup_bytes")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0);
        let lines = live_fetch
            .get("markup_lines")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0);
        format!("{bytes} bytes, {lines} lines")
    } else {
        "no response body".into()
    };
    let detail = if ok {
        let title = string_field(live_fetch, &["title"]).unwrap_or_else(|| "untitled".into());
        let url = string_field(live_fetch, &["url"]).unwrap_or_else(|| "unknown url".into());
        format!("{title} from {url}")
    } else {
        string_field(live_fetch, &["error", "skipped"])
            .or_else(|| {
                value
                    .get("classification")
                    .and_then(|classification| string_field(classification, &["reason", "detail"]))
            })
            .unwrap_or_else(|| "live fetch did not complete".into())
    };
    let first_failed_stage = first_failed_page_probe_stage_from_report(&value)
        .or_else(|| {
            value
                .get("classification")
                .and_then(|classification| string_field(classification, &["stage"]))
        })
        .unwrap_or_else(|| {
            if ok {
                "none".into()
            } else {
                stage_hint.clone()
            }
        });
    let next_step = if ok {
        "open the Browser view and inspect the rendered page".into()
    } else {
        value
            .get("classification")
            .and_then(|classification| string_field(classification, &["next_step", "next_action"]))
            .unwrap_or_else(|| "fix the failed stage, then run Native Live Fetch again".into())
    };

    Some(DiagnosticsLiveFetchCard {
        outcome: if ok { "pass" } else { "blocked" }.into(),
        stage_hint,
        request_backend,
        response_size,
        detail,
        first_failed_stage,
        next_step,
    })
}

fn first_failed_page_probe_stage_from_report(value: &serde_json::Value) -> Option<String> {
    ["live_page_probe", "dry_run_page_probe"]
        .iter()
        .find_map(|section| {
            value
                .get(*section)
                .and_then(|probe| probe.get("report"))
                .and_then(|report| report.get("steps"))
                .and_then(serde_json::Value::as_array)
                .and_then(|steps| {
                    steps.iter().find_map(|step| {
                        let ok = step
                            .get("ok")
                            .and_then(serde_json::Value::as_bool)
                            .unwrap_or(false);
                        (!ok).then(|| string_field(step, &["stage"])).flatten()
                    })
                })
        })
}

pub(in crate::desktop) fn diagnostics_preview_lxmf_delivery_card(
    lines: &[String],
) -> Option<DiagnosticsLxmfDeliveryCard> {
    let value: serde_json::Value = serde_json::from_str(&lines.join("\n")).ok()?;
    let report = lxmf_interop_report_value(&value)?;
    let classification = report.get("classification");
    let send = report.get("send");
    let wait = report.get("wait");
    let outcome = classification
        .and_then(|value| string_field(value, &["outcome"]))
        .or_else(|| wait.and_then(|value| string_field(value, &["status"])))
        .unwrap_or_else(|| "unknown".into());
    let send_state = send
        .map(lxmf_send_state_line)
        .unwrap_or_else(|| "send: not requested".into());
    let proof_state = wait
        .and_then(|value| string_field(value, &["proof_match_state"]))
        .unwrap_or_else(|| "unknown".into());
    let inbound_state = wait
        .and_then(|value| string_field(value, &["inbound_reply_match_state"]))
        .unwrap_or_else(|| "unknown".into());
    let event_counts = wait
        .map(lxmf_event_counts_line)
        .unwrap_or_else(|| "events unavailable".into());
    let readiness_stage = lxmf_first_failed_readiness_stage(report)
        .unwrap_or_else(|| "ready or not requested".into());
    let detail = classification
        .and_then(|value| string_field(value, &["reason", "detail"]))
        .or_else(|| wait.and_then(|value| string_field(value, &["detail"])))
        .unwrap_or_else(|| "no LXMF delivery detail".into());
    let next_step = classification
        .and_then(|value| string_field(value, &["next_step", "next_action"]))
        .or_else(|| {
            report
                .get("failure_hints")
                .and_then(serde_json::Value::as_array)
                .and_then(|hints| hints.first())
                .and_then(serde_json::Value::as_str)
                .map(ToOwned::to_owned)
        })
        .unwrap_or_else(|| {
            "select an LXMF peer conversation in the app, then run LXMF Interop again".into()
        });

    Some(DiagnosticsLxmfDeliveryCard {
        outcome,
        send_state,
        proof_state,
        inbound_state,
        event_counts,
        readiness_stage,
        detail,
        next_step,
    })
}

pub(in crate::desktop) fn diagnostics_preview_propagation_sync_card(
    lines: &[String],
) -> Option<DiagnosticsPropagationSyncCard> {
    let value: serde_json::Value = serde_json::from_str(&lines.join("\n")).ok()?;
    if value.get("report").and_then(serde_json::Value::as_str)
        != Some("native_lxmf_propagation_diagnostics")
    {
        return None;
    }
    let sync = value.get("sync");
    let sync_ok = sync
        .and_then(|sync| sync.get("ok"))
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    let selected_node = string_field(&value, &["selected_node"]).unwrap_or_else(|| "none".into());
    let before = propagation_state_line(value.get("before"));
    let after = propagation_state_line(value.get("after"));
    let sync_events = value
        .get("sync_events")
        .and_then(serde_json::Value::as_array)
        .cloned()
        .unwrap_or_default();
    let events = if sync_events.is_empty() {
        "events unavailable".into()
    } else {
        let status_count = sync_events
            .iter()
            .filter(|event| {
                event.get("kind").and_then(serde_json::Value::as_str) == Some("propagation_status")
            })
            .count();
        let debug_count = sync_events
            .iter()
            .filter(|event| event.get("kind").and_then(serde_json::Value::as_str) == Some("debug"))
            .count();
        let message_count = sync_events
            .iter()
            .filter(|event| {
                event.get("kind").and_then(serde_json::Value::as_str) == Some("message_received")
            })
            .count();
        let structured_count = sync_events
            .iter()
            .filter(|event| {
                event.get("kind").and_then(serde_json::Value::as_str) == Some("propagation_sync")
            })
            .count();
        format!(
            "structured={structured_count}, status={status_count}, debug={debug_count}, messages={message_count}, total={}",
            sync_events.len()
        )
    };
    let event_lines = sync_events
        .iter()
        .rev()
        .take(8)
        .map(propagation_sync_event_line)
        .collect::<Vec<_>>();
    let blocker = string_field(&value, &["blocker"]).unwrap_or_else(|| "unknown".into());
    let next_step =
        string_field(&value, &["next_step"]).unwrap_or_else(|| "inspect runtime logs".into());
    let blocked = blocker != "no propagation blocker reported";

    Some(DiagnosticsPropagationSyncCard {
        outcome: if sync_ok && !blocked {
            "complete"
        } else {
            "blocked"
        }
        .into(),
        selected_node,
        before,
        after,
        events,
        event_lines,
        blocker,
        next_step,
    })
}

fn propagation_sync_event_line(event: &serde_json::Value) -> String {
    match event.get("kind").and_then(serde_json::Value::as_str) {
        Some("propagation_status") => {
            let transfer =
                string_field(event, &["transfer_state"]).unwrap_or_else(|| "unknown".into());
            let link = string_field(event, &["link_state"]).unwrap_or_else(|| "unknown".into());
            let path = event
                .get("has_path")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false);
            let app_data = event
                .get("known_app_data")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false);
            format!("status: {link}/{transfer} path={path} app_data={app_data}")
        }
        Some("debug") => string_field(event, &["message"])
            .map(|message| format!("debug: {message}"))
            .unwrap_or_else(|| "debug: <missing message>".into()),
        Some("message_received") => {
            let peer = string_field(event, &["peer_label", "peer_hash"])
                .unwrap_or_else(|| "unknown peer".into());
            let message_id = string_field(event, &["message_id"]).unwrap_or_else(|| "no id".into());
            format!("message: {peer} id={message_id}")
        }
        Some("propagation_sync") => {
            let stage = string_field(event, &["stage"]).unwrap_or_else(|| "unknown".into());
            let status = string_field(event, &["status"]).unwrap_or_else(|| "unknown".into());
            let detail = string_field(event, &["detail"]).unwrap_or_default();
            format!("sync: {stage}/{status} {detail}")
        }
        Some(kind) => {
            let message = string_field(event, &["message"]).unwrap_or_default();
            format!("{kind}: {message}")
        }
        None => "unknown event".into(),
    }
}

fn propagation_state_line(value: Option<&serde_json::Value>) -> String {
    let Some(value) = value else {
        return "unavailable".into();
    };
    if let Some(error) = string_field(value, &["error"]) {
        return format!("error: {error}");
    }
    let link = string_field(value, &["link_state"]).unwrap_or_else(|| "unknown".into());
    let transfer = string_field(value, &["transfer_state"]).unwrap_or_else(|| "unknown".into());
    let has_path = value
        .get("has_path")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    let app_data = value
        .get("known_app_data")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    format!("link={link}, transfer={transfer}, path={has_path}, app_data={app_data}")
}

fn lxmf_interop_report_value(value: &serde_json::Value) -> Option<&serde_json::Value> {
    if value.get("report").and_then(serde_json::Value::as_str) == Some("native_lxmf_live_interop") {
        return Some(value);
    }
    value.get("lxmf_live_interop").filter(|nested| {
        nested.get("report").and_then(serde_json::Value::as_str) == Some("native_lxmf_live_interop")
    })
}

fn lxmf_send_state_line(send: &serde_json::Value) -> String {
    let requested = send
        .get("requested")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    if !requested {
        return "not requested".into();
    }
    let ok = send
        .get("ok")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    let message_id = string_field(send, &["message_id", "packet_hash"])
        .unwrap_or_else(|| "no message id".into());
    let state = string_field(
        send,
        &["native_lxmf_state", "stage_hint", "skipped", "error"],
    )
    .unwrap_or_else(|| {
        if ok {
            "submitted".into()
        } else {
            "failed".into()
        }
    });
    format!(
        "{} | {} | {}",
        if ok { "submitted" } else { "not sent" },
        state,
        message_id
    )
}

fn lxmf_event_counts_line(wait: &serde_json::Value) -> String {
    let inbound = wait
        .get("inbound_messages")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    let delivery = wait
        .get("delivery_updates")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    let proofs = wait
        .get("packet_proofs")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    format!("inbound={inbound}, delivery_updates={delivery}, packet_proofs={proofs}")
}

fn lxmf_first_failed_readiness_stage(report: &serde_json::Value) -> Option<String> {
    report
        .get("readiness_probe")
        .or_else(|| {
            report
                .get("readiness_retry")
                .and_then(|retry| retry.get("followup_probe"))
        })
        .and_then(|probe| probe.get("steps"))
        .and_then(serde_json::Value::as_array)
        .and_then(|steps| {
            steps.iter().find_map(|step| {
                let ok = step
                    .get("ok")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false);
                (!ok)
                    .then(|| string_field(step, &["stage"]))
                    .flatten()
                    .map(|stage| {
                        let detail =
                            string_field(step, &["detail"]).unwrap_or_else(|| "blocked".into());
                        format!("{stage}: {detail}")
                    })
            })
        })
}

pub(in crate::desktop) fn diagnostics_preview_stage_cards(
    lines: &[String],
) -> Vec<DiagnosticsStageCard> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&lines.join("\n")) else {
        return Vec::new();
    };
    if let Some(stages) = value.get("stages").and_then(serde_json::Value::as_array) {
        return stages
            .iter()
            .filter_map(|stage| {
                Some(DiagnosticsStageCard {
                    kind: "preflight".into(),
                    stage: string_field(stage, &["stage"])?,
                    status: string_field(stage, &["outcome"]).unwrap_or_else(|| "unknown".into()),
                    detail: string_field(stage, &["detail"]).unwrap_or_else(|| "no detail".into()),
                    next_step: string_field(stage, &["next_step"])
                        .unwrap_or_else(|| "inspect report".into()),
                })
            })
            .collect();
    }
    if let Some(verdicts) = value.get("verdicts").and_then(serde_json::Value::as_object) {
        let mut cards = verdicts
            .iter()
            .map(|(stage, verdict)| DiagnosticsStageCard {
                kind: "smoke".into(),
                stage: stage.clone(),
                status: string_field(verdict, &["status"]).unwrap_or_else(|| "unknown".into()),
                detail: string_field(verdict, &["detail"]).unwrap_or_else(|| "no detail".into()),
                next_step: string_field(verdict, &["next_action", "next_step"])
                    .unwrap_or_else(|| "continue".into()),
            })
            .collect::<Vec<_>>();
        cards.sort_by(|left, right| left.stage.cmp(&right.stage));
        return cards;
    }
    if let Some(report) = value
        .get("readiness_probe")
        .or_else(|| value.get("lxmf_delivery_probe"))
        .and_then(|probe| probe.get("report"))
    {
        return lxmf_step_cards(report);
    }
    if value.get("report").and_then(serde_json::Value::as_str) == Some("native_lxmf_live_interop") {
        if let Some(report) = value
            .get("readiness_retry")
            .or_else(|| value.get("readiness_probe"))
            .and_then(|probe| probe.get("report"))
        {
            return lxmf_step_cards(report);
        }
    }
    Vec::new()
}

fn lxmf_step_cards(report: &serde_json::Value) -> Vec<DiagnosticsStageCard> {
    report
        .get("steps")
        .and_then(serde_json::Value::as_array)
        .map(|steps| {
            steps
                .iter()
                .filter_map(|step| {
                    Some(DiagnosticsStageCard {
                        kind: "lxmf".into(),
                        stage: string_field(step, &["stage"])?,
                        status: if step
                            .get("ok")
                            .and_then(serde_json::Value::as_bool)
                            .unwrap_or(false)
                        {
                            "pass".into()
                        } else {
                            "fail".into()
                        },
                        detail: string_field(step, &["detail"])
                            .unwrap_or_else(|| "no detail".into()),
                        next_step: "inspect LXMF readiness and retry when fixed".into(),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

fn string_field(value: &serde_json::Value, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| {
        value.get(*key).and_then(|field| {
            field
                .as_str()
                .map(ToOwned::to_owned)
                .or_else(|| field.as_bool().map(|value| value.to_string()))
                .or_else(|| field.as_u64().map(|value| value.to_string()))
        })
    })
}

#[cfg(test)]
#[path = "diagnostics_preview_tests.rs"]
mod tests;
