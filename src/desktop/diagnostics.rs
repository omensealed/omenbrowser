use crate::workspace::WorkspaceSection;

use iced::Task;

use super::{DesktopApp, DiagnosticsMessage, Message};

impl DesktopApp {
    pub(super) fn dispatch_diagnostics_message(
        &mut self,
        message: Message,
    ) -> Result<Task<Message>, Message> {
        match message {
            Message::Diagnostics(DiagnosticsMessage::Show) => {
                self.update_show_diagnostics();
                Ok(Task::none())
            }
            Message::Diagnostics(DiagnosticsMessage::CopyOperationDiagnostics(id)) => {
                Ok(self.update_copy_operation_diagnostics(id))
            }
            Message::Diagnostics(DiagnosticsMessage::PreviewManagedConfig) => {
                self.update_preview_managed_config();
                Ok(Task::none())
            }
            Message::Diagnostics(DiagnosticsMessage::ExportManagedConfig) => {
                self.update_export_managed_config();
                Ok(Task::none())
            }
            Message::Diagnostics(DiagnosticsMessage::PreviewBundle) => {
                self.update_preview_diagnostics_bundle();
                Ok(Task::none())
            }
            Message::Diagnostics(DiagnosticsMessage::ExportBundle) => {
                self.update_export_diagnostics_bundle();
                Ok(Task::none())
            }
            Message::Diagnostics(DiagnosticsMessage::PreviewLiveInteropReport) => {
                self.update_preview_live_interop_report();
                Ok(Task::none())
            }
            Message::Diagnostics(DiagnosticsMessage::ExportLiveInteropReport) => {
                self.update_export_live_interop_report();
                Ok(Task::none())
            }
            Message::Diagnostics(DiagnosticsMessage::NativePreflight) => {
                self.update_native_preflight();
                Ok(Task::none())
            }
            Message::Diagnostics(DiagnosticsMessage::NativeSmokeDryRun) => {
                self.update_native_smoke_dry_run();
                Ok(Task::none())
            }
            Message::Diagnostics(DiagnosticsMessage::NativeSmokeLiveProbe) => {
                self.update_native_smoke_live_probe();
                Ok(Task::none())
            }
            Message::Diagnostics(DiagnosticsMessage::NativeLiveFetchValidate) => {
                self.update_native_live_fetch_validate();
                Ok(Task::none())
            }
            Message::Diagnostics(DiagnosticsMessage::NativeLxmfSmokeSend) => {
                self.update_native_lxmf_smoke_send();
                Ok(Task::none())
            }
            Message::Diagnostics(DiagnosticsMessage::NativeLxmfInterop) => {
                self.update_native_lxmf_interop();
                Ok(Task::none())
            }
            Message::Diagnostics(DiagnosticsMessage::NativeLxmfPropagationDiagnostics) => {
                self.update_native_lxmf_propagation_diagnostics();
                Ok(Task::none())
            }
            Message::Diagnostics(DiagnosticsMessage::SyncPropagationNow) => {
                self.update_sync_propagation_now();
                Ok(Task::none())
            }
            Message::Diagnostics(DiagnosticsMessage::BeginKnownDestinationsPreload) => {
                self.update_begin_known_destinations_preload();
                Ok(Task::none())
            }
            _ => Err(message),
        }
    }

    pub(super) fn update_show_diagnostics(&mut self) {
        self.app.switch_section(WorkspaceSection::Diagnostics);
    }

    pub(super) fn update_copy_operation_diagnostics(
        &mut self,
        id: crate::operations::OperationId,
    ) -> Task<Message> {
        let Some(record) = self
            .app
            .operation_history
            .records()
            .find(|record| record.id == id)
        else {
            self.app.status.task =
                "operation diagnostic is no longer retained; refresh Network Doctor".into();
            return Task::none();
        };
        let diagnostic = crate::operations::presentation::operation_diagnostics_text(record);
        self.app.status.task = format!(
            "copied {} operation diagnostic",
            crate::operations::presentation::domain_label(id.domain)
        );
        iced::clipboard::write(diagnostic)
    }

    pub(super) fn update_preview_managed_config(&mut self) {
        self.app.preview_managed_reticulum_config();
    }

    pub(super) fn update_export_managed_config(&mut self) {
        self.app.export_managed_reticulum_config();
    }

    pub(super) fn update_preview_diagnostics_bundle(&mut self) {
        self.app.preview_diagnostics_bundle();
    }

    pub(super) fn update_export_diagnostics_bundle(&mut self) {
        self.app.export_diagnostics_bundle();
    }

    pub(super) fn update_preview_live_interop_report(&mut self) {
        self.app.preview_live_interop_report();
    }

    pub(super) fn update_export_live_interop_report(&mut self) {
        self.app.export_live_interop_report();
    }

    pub(super) fn update_native_preflight(&mut self) {
        self.app.run_native_preflight_report();
    }

    pub(super) fn update_native_smoke_dry_run(&mut self) {
        self.app.run_native_network_smoke_test(false);
    }

    pub(super) fn update_native_smoke_live_probe(&mut self) {
        self.app.run_native_network_smoke_test(true);
    }

    pub(super) fn update_native_live_fetch_validate(&mut self) {
        self.app.run_native_network_live_fetch_validation();
    }

    pub(super) fn update_native_lxmf_smoke_send(&mut self) {
        self.app.run_native_lxmf_smoke_send();
    }

    pub(super) fn update_native_lxmf_interop(&mut self) {
        self.app.run_native_lxmf_live_interop();
    }

    pub(super) fn update_native_lxmf_propagation_diagnostics(&mut self) {
        self.app.run_native_lxmf_propagation_diagnostics();
    }

    pub(super) fn update_sync_propagation_now(&mut self) {
        self.app.sync_propagation_messages_now();
    }

    pub(super) fn update_begin_known_destinations_preload(&mut self) {
        self.app.begin_known_destinations_preload_flow();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::App;
    use crate::operations::{
        EvidenceAuthority, OperationAction, OperationDomain, OperationEvidence,
        OperationEvidenceKind, OperationId, OperationRecord, OperationState, OperationTarget,
        OperationTargetKind,
    };

    fn desktop_with_temp_root(name: &str) -> DesktopApp {
        let root = std::env::temp_dir().join(format!("{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let paths = crate::config::AppPaths::from_root(root);
        paths.ensure().expect("paths");
        DesktopApp::new(App::new(crate::config::AppConfig {
            paths,
            settings: crate::storage::settings::AppSettings::default(),
        }))
    }

    fn attention_record(id: OperationId) -> OperationRecord {
        OperationRecord {
            id,
            target: OperationTarget {
                kind: OperationTargetKind::Destination,
                label: "destination".into(),
            },
            state: OperationState::Failed,
            authority: EvidenceAuthority::Authoritative,
            evidence: vec![OperationEvidence {
                kind: OperationEvidenceKind::Failure,
                authority: EvidenceAuthority::Authoritative,
                at_unix_ms: 10,
                detail: Some("path unavailable".into()),
            }],
            progress: None,
            attempt_count: 1,
            stamp_cost: None,
            propagation_node: None,
            created_at_unix_ms: 1,
            updated_at_unix_ms: 10,
            last_error: Some("path unavailable".into()),
            event_cursor: None,
            valid_actions: vec![OperationAction::CopyDiagnostics],
        }
    }

    #[test]
    fn copy_operation_diagnostic_handles_retained_and_evicted_records() {
        let mut desktop = desktop_with_temp_root("omenbrowser-rs-operation-copy");
        let id = OperationId::numeric(OperationDomain::PathDiscovery, 7);
        desktop
            .app
            .operation_history
            .upsert(attention_record(id))
            .expect("record");

        let _ = desktop.update_copy_operation_diagnostics(id);
        assert_eq!(
            desktop.app.status.task,
            "copied path discovery operation diagnostic"
        );

        desktop.app.operation_history.remove(id);
        let _ = desktop.update_copy_operation_diagnostics(id);
        assert!(desktop.app.status.task.contains("no longer retained"));
    }
}
