use super::*;

impl App {
    pub fn apply_diagnostics_task_result(&mut self, result: DiagnosticsTaskResult) -> bool {
        match result {
            DiagnosticsTaskResult::ExportBundle {
                generation,
                snapshot,
            } => {
                if self.diagnostics_export_pending != Some(generation) {
                    return false;
                }
                self.diagnostics_export_pending = None;
                match snapshot {
                    Ok(snapshot) => self.write_diagnostics_bundle_export(Some(&snapshot)),
                    Err(error) => {
                        self.status.task =
                            format!("failed to collect diagnostics snapshot: {error}");
                        self.logs
                            .lines
                            .push(format!("failed to collect diagnostics snapshot: {error}"));
                        false
                    }
                }
            }
            DiagnosticsTaskResult::LiveInteropReport {
                generation,
                export,
                report,
            } => match report {
                Ok(report) => {
                    if export {
                        self.write_live_interop_report_export(generation, &report)
                    } else {
                        self.preview_live_interop_report_value(generation, &report)
                    }
                }
                Err(error) => {
                    self.status.task = format!("failed to collect live interop report: {error}");
                    self.logs.push_with_source(
                        LogSeverity::Error,
                        LogSource::Diagnostics,
                        format!("failed to collect live interop report generation {generation}: {error}"),
                    );
                    false
                }
            },
            DiagnosticsTaskResult::NativeNetworkSmokeTest {
                generation,
                execute_live_probe,
                execute_live_fetch,
                report,
            } => match report {
                Ok(report) => {
                    let rendered = self.preview_json_diagnostics_value(&report, |line_count| {
                        format!(
                            "native-network smoke test generation {generation} complete ({line_count} lines, live={execute_live_probe}, fetch={execute_live_fetch})"
                        )
                    });
                    if rendered {
                        self.logs.push_with_source(
                            LogSeverity::Info,
                            LogSource::Diagnostics,
                            format!(
                                "native-network smoke test generation {generation} complete live={execute_live_probe} fetch={execute_live_fetch}"
                            ),
                        );
                    }
                    rendered
                }
                Err(error) => {
                    self.status.task = format!("failed to run native-network smoke test: {error}");
                    self.logs.push_with_source(
                        LogSeverity::Error,
                        LogSource::Diagnostics,
                        format!(
                            "failed to run native-network smoke test generation {generation}: {error}"
                        ),
                    );
                    false
                }
            },
            DiagnosticsTaskResult::NativePreflight { generation, report } => match report {
                Ok(report) => {
                    let rendered = self.preview_json_diagnostics_value(&report, |line_count| {
                        format!(
                            "native preflight generation {generation} complete ({line_count} lines)"
                        )
                    });
                    if rendered {
                        self.logs.push_with_source(
                            LogSeverity::Info,
                            LogSource::Diagnostics,
                            format!("native preflight generation {generation} complete"),
                        );
                    }
                    rendered
                }
                Err(error) => {
                    self.status.task = format!("failed to run native preflight: {error}");
                    self.logs.push_with_source(
                        LogSeverity::Error,
                        LogSource::Diagnostics,
                        format!("failed to run native preflight generation {generation}: {error}"),
                    );
                    false
                }
            },
            DiagnosticsTaskResult::NativeLxmfSmokeSend {
                generation,
                peer_hash,
                report,
            } => match report {
                Ok(report) => {
                    let rendered = self.preview_json_diagnostics_value(&report, |line_count| {
                        format!(
                            "native LXMF smoke send generation {generation} complete ({line_count} lines, peer={peer_hash})"
                        )
                    });
                    if rendered {
                        self.logs.push_with_source(
                            LogSeverity::Info,
                            LogSource::Diagnostics,
                            format!(
                                "native LXMF smoke send generation {generation} complete peer={peer_hash}"
                            ),
                        );
                    }
                    rendered
                }
                Err(error) => {
                    self.status.task = format!("failed to run native LXMF smoke send: {error}");
                    self.logs.push_with_source(
                        LogSeverity::Error,
                        LogSource::Diagnostics,
                        format!(
                            "failed to run native LXMF smoke send generation {generation} peer={peer_hash}: {error}"
                        ),
                    );
                    false
                }
            },
            DiagnosticsTaskResult::NativeLxmfLiveInterop {
                generation,
                peer_hash,
                wait_secs,
                report,
            } => match report {
                Ok(report) => {
                    let rendered = self.preview_json_diagnostics_value(&report, |line_count| {
                        format!(
                            "native LXMF live interop generation {generation} complete ({line_count} lines, wait={wait_secs}s)"
                        )
                    });
                    if rendered {
                        self.logs.push_with_source(
                            LogSeverity::Info,
                            LogSource::Diagnostics,
                            format!(
                                "native LXMF live interop generation {generation} complete peer={}",
                                peer_hash.as_deref().unwrap_or("none")
                            ),
                        );
                    }
                    rendered
                }
                Err(error) => {
                    self.status.task = format!("failed to run native LXMF live interop: {error}");
                    self.logs.push_with_source(
                        LogSeverity::Error,
                        LogSource::Diagnostics,
                        format!(
                            "failed to run native LXMF live interop generation {generation} peer={}: {error}",
                            peer_hash.as_deref().unwrap_or("none")
                        ),
                    );
                    false
                }
            },
            DiagnosticsTaskResult::NativeLxmfPropagationDiagnostics { generation, report } => {
                match report {
                    Ok(report) => {
                        let blocker = report
                            .get("blocker")
                            .and_then(serde_json::Value::as_str)
                            .unwrap_or("unknown");
                        let rendered =
                            self.preview_json_diagnostics_value(&report, |line_count| {
                                format!(
                                    "native LXMF propagation diagnostics generation {generation} complete ({line_count} lines)"
                                )
                            });
                        if rendered {
                            self.logs.push_with_source(
                                LogSeverity::Info,
                                LogSource::Diagnostics,
                                format!(
                                    "native LXMF propagation diagnostics generation {generation} complete blocker={blocker}"
                                ),
                            );
                        }
                        rendered
                    }
                    Err(error) => {
                        self.status.task =
                            format!("failed to run native LXMF propagation diagnostics: {error}");
                        self.logs.push_with_source(
                            LogSeverity::Error,
                            LogSource::Diagnostics,
                            format!(
                                "failed to run native LXMF propagation diagnostics generation {generation}: {error}"
                            ),
                        );
                        false
                    }
                }
            }
            DiagnosticsTaskResult::PathDiscoveryDiagnostics {
                tab_id,
                generation,
                report,
            } => match report {
                Ok(report) => {
                    self.apply_browser_path_discovery_summary(tab_id, &report);
                    let rendered = self.preview_json_diagnostics_value(&report, |line_count| {
                        format!(
                            "path discovery diagnostics generation {generation} complete ({line_count} lines)"
                        )
                    });
                    if rendered {
                        self.logs.push_with_source(
                            LogSeverity::Info,
                            LogSource::Diagnostics,
                            format!("path discovery diagnostics generation {generation} complete"),
                        );
                    }
                    rendered
                }
                Err(error) => {
                    self.status.task = format!("failed to run path discovery diagnostics: {error}");
                    self.logs.push_with_source(
                        LogSeverity::Error,
                        LogSource::Diagnostics,
                        format!(
                            "failed to run path discovery diagnostics generation {generation}: {error}"
                        ),
                    );
                    false
                }
            },
            DiagnosticsTaskResult::BrowserPathWarmup {
                tab_id,
                generation,
                report,
            } => {
                let Some(index) = self.browser_tab_index(tab_id) else {
                    return false;
                };
                if self.workspace.browser_tabs[index]
                    .path_warmup
                    .as_ref()
                    .map(|warmup| warmup.generation)
                    != Some(generation)
                {
                    return false;
                }
                self.workspace.browser_tabs[index].path_warmup = None;
                match report {
                    Ok(report) => {
                        self.apply_browser_path_discovery_summary(tab_id, &report);
                        let status = report
                            .get("path_warmup")
                            .and_then(|value| value.get("status"))
                            .and_then(serde_json::Value::as_str)
                            .unwrap_or("unknown");
                        self.status.task =
                            format!("browser path request complete: status={status}");
                        self.logs.push_with_source(
                            LogSeverity::Info,
                            LogSource::Diagnostics,
                            format!(
                                "browser path request generation {generation} complete status={status}"
                            ),
                        );
                        true
                    }
                    Err(error) => {
                        self.status.task = format!("browser path request failed: {error}");
                        self.logs.push_with_source(
                            LogSeverity::Error,
                            LogSource::Diagnostics,
                            format!("browser path request generation {generation} failed: {error}"),
                        );
                        false
                    }
                }
            }
            DiagnosticsTaskResult::KnownDestinationsPreload {
                tab_id,
                generation,
                report,
            } => match report {
                Ok(report) => {
                    self.apply_browser_path_discovery_summary(tab_id, &report);
                    let rendered = self.preview_json_diagnostics_value(&report, |line_count| {
                        format!(
                            "known_destinations preload generation {generation} complete ({line_count} lines)"
                        )
                    });
                    if rendered {
                        let loaded = report
                            .get("known_destinations_preload")
                            .and_then(|value| value.get("loaded"))
                            .and_then(serde_json::Value::as_u64)
                            .unwrap_or(0);
                        self.logs.push_with_source(
                            LogSeverity::Info,
                            LogSource::Diagnostics,
                            format!(
                                "known_destinations preload generation {generation} complete loaded={loaded}"
                            ),
                        );
                    }
                    rendered
                }
                Err(error) => {
                    self.status.task = format!("known_destinations preload failed: {error}");
                    self.logs.push_with_source(
                        LogSeverity::Error,
                        LogSource::Diagnostics,
                        format!(
                            "known_destinations preload generation {generation} failed: {error}"
                        ),
                    );
                    false
                }
            },
            DiagnosticsTaskResult::PageFetchProbe {
                tab_id,
                generation,
                execute_request,
                show_diagnostics,
                report,
            } => {
                let mode = if execute_request { "live" } else { "dry-run" };
                match report {
                    Ok(report) => match serde_json::to_string_pretty(&report) {
                        Ok(content) => {
                            let line_count = content.lines().count();
                            self.apply_browser_probe_summary(
                                tab_id,
                                BrowserProbeSummary::from_report(&report, mode),
                            );
                            self.apply_browser_probe_report_retry_state(tab_id, &report);
                            self.diagnostics_state.preview_lines =
                                content.lines().map(ToOwned::to_owned).collect();
                            self.diagnostics_state.preview_scroll = 0;
                            if show_diagnostics {
                                self.workspace.active_section = WorkspaceSection::Diagnostics;
                            }
                            self.status.task = format!(
                                "{mode} page probe complete: {} step(s), ready={}",
                                report.steps.len(),
                                report.ready_to_request
                            );
                            self.logs.push_with_source(
                                LogSeverity::Info,
                                LogSource::Diagnostics,
                                format!(
                                    "{mode} page probe generation {generation} rendered ({line_count} lines)"
                                ),
                            );
                            for (index, step) in report.steps.iter().enumerate() {
                                self.logs.push_with_source(
                                    if step.ok {
                                        LogSeverity::Debug
                                    } else {
                                        LogSeverity::Warn
                                    },
                                    LogSource::Diagnostics,
                                    format_page_probe_trace_log_line(mode, index, step),
                                );
                            }
                            true
                        }
                        Err(error) => {
                            self.status.task =
                                format!("failed to render {mode} page probe: {error}");
                            self.logs.push_with_source(
                                LogSeverity::Error,
                                LogSource::Diagnostics,
                                format!("failed to render {mode} page probe: {error}"),
                            );
                            false
                        }
                    },
                    Err(error) => {
                        self.apply_browser_probe_summary(
                            tab_id,
                            BrowserProbeSummary {
                                url: self
                                    .browser_tab_index(tab_id)
                                    .map(|index| {
                                        self.workspace.browser_tabs[index].address_input.clone()
                                    })
                                    .unwrap_or_default(),
                                mode: mode.into(),
                                ready_to_request: false,
                                status: "failed".into(),
                                detail: error.clone(),
                            },
                        );
                        self.status.task = format!("{mode} page probe failed: {error}");
                        self.logs.push_with_source(
                            LogSeverity::Error,
                            LogSource::Diagnostics,
                            format!("{mode} page probe failed: {error}"),
                        );
                        false
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stale_export_result_does_not_replace_the_pending_generation() {
        let root = std::env::temp_dir().join(format!(
            "omenbrowser-rs-diagnostics-result-{}-{}",
            std::process::id(),
            current_epoch_ms()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let config = AppConfig {
            paths: AppPaths::from_root(root.clone()),
            settings: AppSettings {
                runtime_backend: RuntimeBackendSetting::Mock,
                ..AppSettings::default()
            },
        };
        let mut app = App::new(config);
        app.diagnostics_export_pending = Some(9);

        assert!(
            !app.apply_diagnostics_task_result(DiagnosticsTaskResult::ExportBundle {
                generation: 8,
                snapshot: Err("stale result must not be observed".into()),
            })
        );
        assert_eq!(app.diagnostics_export_pending, Some(9));

        drop(app);
        let _ = std::fs::remove_dir_all(root);
    }
}
