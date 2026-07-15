use iced::widget::{column, container, row, text, text_input, Button};
use iced::{Element, Length};

use super::super::*;

impl DesktopApp {
    pub(in crate::desktop) fn startup_status_card(&self) -> Element<'_, Message> {
        let readiness = self.app.native_reticulum_readiness();
        let steps = native_setup_steps(&self.app);
        let completed = steps.iter().filter(|step| step.ready).count();
        let browser_address = self.app.active_browser_tab().address_input.clone();
        let rows = steps
            .into_iter()
            .fold(column![].spacing(3), |column, step| {
                column.push(
                    text(format!(
                        "{}: {} - {}",
                        step.title,
                        if step.ready { "ready" } else { "needs action" },
                        step.detail
                    ))
                    .size(ui_size(13)),
                )
            });
        let title = format!("Startup Status ({completed}/6 ready)");
        let blocker = readiness
            .issues
            .first()
            .map(|issue| format!("blocking live networking: {issue}"))
            .unwrap_or_else(|| "identity/runtime bootstrap has no reported blockers".into());
        let content = column![
            row![
                text(title).size(ui_size(18)),
                text(format!(
                    "compiled={} configured={} backend={:?} connected={}",
                    readiness.compiled,
                    readiness.configured,
                    self.app.settings.runtime_backend,
                    self.app.runtime_status.connected
                ))
                .size(ui_size(14)),
            ]
            .spacing(12),
            text(readiness.summary).size(ui_size(14)),
            text(blocker).size(ui_size(14)),
            rows,
            action_grid(
                vec![
                    omen_button(
                        "Retry Startup",
                        Message::Runtime(RuntimeMessage::StartNativeRuntime),
                    ),
                    omen_button(
                        "Auto Configure",
                        Message::Runtime(RuntimeMessage::NativeQuickstart),
                    ),
                    subtle_button(
                        "Create Identity",
                        Message::Identity(IdentityMessage::Create),
                    ),
                    subtle_button(
                        "Use Native",
                        Message::Runtime(RuntimeMessage::SelectNativeBackend),
                    ),
                    subtle_button(
                        "Add TCP",
                        Message::Interface(super::super::InterfaceMessage::CreateTcpClient)
                    ),
                    subtle_button(
                        "Interfaces",
                        Message::Shell(ShellMessage::SwitchSection(WorkspaceSection::Interfaces))
                    ),
                ],
                6,
            ),
            setup_tcp_client_editor(&self.app),
            {
                let field_editor_active = self.app.active_browser_field_editor().is_some();
                let input: Element<'_, Message> = if field_editor_active {
                    inert_address_display(browser_address.clone())
                } else {
                    text_input("destination:/path", &browser_address)
                        .on_input(|value| Message::Browser(BrowserMessage::AddressChanged(value)))
                        .on_submit(Message::Browser(BrowserMessage::OpenSetupAddress))
                        .width(Length::Fill)
                        .into()
                };
                row![
                    text("Open live NomadNet").size(ui_size(14)),
                    input,
                    omen_button(
                        "Open Address",
                        Message::Browser(BrowserMessage::OpenSetupAddress),
                    ),
                ]
                .spacing(8)
                .wrap()
            },
            action_grid(
                vec![
                    subtle_button(
                        "Directory",
                        Message::Shell(ShellMessage::SwitchSection(WorkspaceSection::Directory))
                    ),
                    subtle_button(
                        "Diagnostics",
                        Message::Shell(ShellMessage::SwitchSection(WorkspaceSection::Diagnostics))
                    ),
                    subtle_button(
                        "Preflight",
                        Message::Diagnostics(DiagnosticsMessage::NativePreflight)
                    ),
                    subtle_button(
                        "Live Probe",
                        Message::Diagnostics(DiagnosticsMessage::NativeSmokeLiveProbe)
                    ),
                    omen_button(
                        "Live Fetch",
                        Message::Diagnostics(DiagnosticsMessage::NativeLiveFetchValidate)
                    ),
                ],
                5,
            ),
        ]
        .spacing(5);

        let styled = container(content)
            .padding(10)
            .width(Length::Fill)
            .style(if readiness.ready {
                status_container_style
            } else {
                warning_container_style
            });
        styled.into()
    }

    pub(in crate::desktop) fn lxmf_messaging_diagnostics_card(&self) -> Element<'_, Message> {
        lxmf_messaging_diagnostics_card(self)
    }

    pub(in crate::desktop) fn gateway_preset_buttons(&self) -> Vec<Button<'static, Message>> {
        gateway_preset_buttons(self)
    }

    pub(in crate::desktop) fn gateway_preset_status_line(&self) -> String {
        gateway_preset_status_line(self)
    }

    #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
    pub(in crate::desktop) fn omenchat_live_monitor_totals(&self) -> OmenChatLiveMonitorTotals {
        omenchat_live_monitor_totals(self)
    }

    pub(in crate::desktop) fn omenchat_monitoring_card(&self) -> Element<'_, Message> {
        omenchat_monitoring_card(self)
    }

    pub(in crate::desktop) fn native_action_status_lines(&self) -> Vec<String> {
        native_action_status_lines(self)
    }
}
