use iced::widget::{column, container, text};
use iced::{Element, Length};

use crate::app::LxmfMessagingDiagnosticsSeverity;
use crate::desktop::{
    status_container_style, ui_size, warning_container_style, wrapped_text_owned, DesktopApp,
    Message,
};

pub(in crate::desktop) fn lxmf_messaging_diagnostics_card(
    desktop: &DesktopApp,
) -> Element<'_, Message> {
    let diagnostics = desktop.app.active_lxmf_messaging_diagnostics();
    let body = diagnostics
        .lines
        .into_iter()
        .fold(column![].spacing(4), |column, line| {
            column.push(wrapped_text_owned(line, 14))
        });
    let style = match diagnostics.severity {
        LxmfMessagingDiagnosticsSeverity::Ready | LxmfMessagingDiagnosticsSeverity::Info => {
            status_container_style
        }
        LxmfMessagingDiagnosticsSeverity::Warning | LxmfMessagingDiagnosticsSeverity::Blocked => {
            warning_container_style
        }
    };
    container(column![text(diagnostics.title).size(ui_size(20)), body].spacing(8))
        .style(style)
        .padding(12)
        .width(Length::Fill)
        .into()
}
