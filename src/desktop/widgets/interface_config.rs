use iced::widget::text::Wrapping;
use iced::widget::{column, container, row, text, text_input};
use iced::{Element, Length};

use crate::app::App;
use crate::interfaces::{InterfaceKind, ReticulumInterfaceProfile};
use crate::workspace::WorkspaceSection;

use super::super::{subtle_button, ui_size, InterfaceMessage, Message};
use super::interface_status::interface_runtime_detail_line;

pub(in crate::desktop) fn optional_interface_runtime_detail_line<'a>(
    profile: &ReticulumInterfaceProfile,
    stats: Option<&crate::runtime::InterfaceStats>,
) -> Element<'a, Message> {
    match interface_runtime_detail_line(profile, stats) {
        Some(line) => text(line)
            .size(ui_size(13))
            .wrapping(Wrapping::WordOrGlyph)
            .width(Length::Fill)
            .into(),
        None => container(column![]).into(),
    }
}

pub(in crate::desktop) fn setup_tcp_client_editor(app: &App) -> Element<'_, Message> {
    if let Some(profile) = setup_tcp_client_profile(app) {
        let host_id = profile.profile_id.clone();
        let port_id = profile.profile_id.clone();
        let ifac_network_id = profile.profile_id.clone();
        let ifac_pass_id = profile.profile_id.clone();
        column![
            row![
                text("TCP gateway").size(ui_size(14)),
                text_input("host", &profile.target_host)
                    .on_input(move |value| {
                        Message::Interface(InterfaceMessage::TcpClientHostChanged {
                            profile_id: host_id.clone(),
                            value,
                        })
                    })
                    .width(Length::FillPortion(2)),
                text_input("port", &profile.target_port.to_string())
                    .on_input(move |value| {
                        Message::Interface(InterfaceMessage::TcpClientPortChanged {
                            profile_id: port_id.clone(),
                            value,
                        })
                    })
                    .width(Length::FillPortion(1)),
            ]
            .spacing(8)
            .wrap(),
            row![
                text("IFAC").size(ui_size(14)),
                text_input("network name", &profile.network_name)
                    .on_input(move |value| {
                        Message::Interface(InterfaceMessage::TcpClientIfacNetworkChanged {
                            profile_id: ifac_network_id.clone(),
                            value,
                        })
                    })
                    .width(Length::FillPortion(2)),
                text_input("passphrase", &profile.passphrase)
                    .secure(true)
                    .on_input(move |value| {
                        Message::Interface(InterfaceMessage::TcpClientIfacPassphraseChanged {
                            profile_id: ifac_pass_id.clone(),
                            value,
                        })
                    })
                    .width(Length::FillPortion(2)),
            ]
            .spacing(8)
            .wrap(),
        ]
        .spacing(6)
        .into()
    } else {
        row![
            text("TCP gateway: none configured").size(ui_size(14)),
            subtle_button(
                "Add TCP Gateway",
                Message::Interface(InterfaceMessage::CreateTcpClient)
            ),
            subtle_button(
                "Open Interfaces",
                Message::Shell(super::super::ShellMessage::SwitchSection(
                    WorkspaceSection::Interfaces,
                ))
            ),
        ]
        .spacing(8)
        .wrap()
        .into()
    }
}

pub(in crate::desktop) fn setup_tcp_client_profile(
    app: &App,
) -> Option<&ReticulumInterfaceProfile> {
    app.interfaces_state
        .profiles
        .iter()
        .find(|profile| profile.kind == InterfaceKind::TcpClient)
}

#[cfg(test)]
#[path = "interface_config_tests.rs"]
mod tests;
