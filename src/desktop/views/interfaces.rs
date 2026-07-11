use iced::widget::{column, row, text, text_input};
use iced::{Element, Length};

use crate::interfaces::InterfaceKind;
use crate::workspace::WorkspaceSection;

use super::super::{
    action_grid, app_scrollable, interface_config_preview_lines, interface_config_summary_lines,
    interface_kind_display_label, interface_restart_recommendation_line,
    interface_runtime_state_line, interface_runtime_status_label, omen_button,
    optional_interface_runtime_detail_line, section_card, subtle_button, ui_size, warning_button,
    wrapped_panel_text, DesktopApp, Message,
};

pub(in crate::desktop) fn interfaces_view(desktop: &DesktopApp) -> Element<'_, Message> {
    let selected = desktop.app.interfaces_state.selected;
    let runtime_interface_stats = desktop.app.monitoring_state.last_interface_stats.as_ref();
    let profiles = desktop
        .app
        .interfaces_state
        .profiles
        .iter()
        .enumerate()
        .fold(column![].spacing(8), |column, (index, profile)| {
            let title = if Some(index) == selected {
                format!("[selected] {}", profile.name)
            } else {
                profile.name.clone()
            };
            let mut body = column![
                row![
                    subtle_button("Select", Message::SelectInterfaceProfile(index)),
                    subtle_button(
                        if profile.enabled { "Disable" } else { "Enable" },
                        Message::ToggleInterfaceEnabled(index)
                    ),
                    warning_button("Delete", Message::DeleteInterfaceProfile(index)),
                ]
                .spacing(8)
                .wrap(),
                row![
                    text(interface_kind_display_label(&profile.kind)).size(ui_size(14)),
                    text(if profile.enabled {
                        "profile: enabled"
                    } else {
                        "profile: disabled"
                    })
                    .size(ui_size(14)),
                ]
                .spacing(8)
                .wrap(),
                text(format!("id: {}", profile.profile_id))
                    .size(ui_size(14))
                    .wrapping(iced::widget::text::Wrapping::WordOrGlyph)
                    .width(Length::Fill),
                text(interface_runtime_state_line(
                    profile,
                    runtime_interface_stats
                ))
                .size(ui_size(14))
                .wrapping(iced::widget::text::Wrapping::WordOrGlyph)
                .width(Length::Fill),
                text(interface_runtime_status_label(
                    profile,
                    runtime_interface_stats
                ))
                .size(ui_size(14))
                .wrapping(iced::widget::text::Wrapping::WordOrGlyph)
                .width(Length::Fill),
                optional_interface_runtime_detail_line(profile, runtime_interface_stats),
                text_input("interface name", &profile.name)
                    .on_input({
                        let profile_id = profile.profile_id.clone();
                        move |value| Message::InterfaceNameChanged {
                            profile_id: profile_id.clone(),
                            value,
                        }
                    })
                    .padding(6)
                    .width(Length::Fill),
                text(format!("network: {}", profile.network_name))
                    .size(ui_size(14))
                    .wrapping(iced::widget::text::Wrapping::WordOrGlyph)
                    .width(Length::Fill),
            ]
            .spacing(5);
            body = match profile.kind {
                InterfaceKind::TcpClient => {
                    let ifac_network_id = profile.profile_id.clone();
                    let ifac_pass_id = profile.profile_id.clone();
                    body.push(
                        column![
                            text(format!(
                                "TCP gateway: {}:{}",
                                profile.target_host, profile.target_port
                            ))
                            .size(ui_size(14))
                            .wrapping(iced::widget::text::Wrapping::WordOrGlyph)
                            .width(Length::Fill),
                            text(format!(
                                "IFAC: network={} passphrase={}",
                                if profile.network_name.is_empty() {
                                    "not set"
                                } else {
                                    profile.network_name.as_str()
                                },
                                if profile.passphrase.is_empty() {
                                    "not set"
                                } else {
                                    "configured"
                                }
                            ))
                            .size(ui_size(14))
                            .wrapping(iced::widget::text::Wrapping::WordOrGlyph)
                            .width(Length::Fill),
                            row![
                                text_input("host", &profile.target_host)
                                    .on_input({
                                        let profile_id = profile.profile_id.clone();
                                        move |value| Message::TcpClientHostChanged {
                                            profile_id: profile_id.clone(),
                                            value,
                                        }
                                    })
                                    .padding(6)
                                    .width(Length::FillPortion(3)),
                                text_input("port", &profile.target_port.to_string())
                                    .on_input({
                                        let profile_id = profile.profile_id.clone();
                                        move |value| Message::TcpClientPortChanged {
                                            profile_id: profile_id.clone(),
                                            value,
                                        }
                                    })
                                    .padding(6)
                                    .width(Length::FillPortion(1)),
                            ]
                            .spacing(8)
                            .wrap(),
                            row![
                                text_input("IFAC network name", &profile.network_name)
                                    .on_input(move |value| Message::TcpClientIfacNetworkChanged {
                                        profile_id: ifac_network_id.clone(),
                                        value,
                                    })
                                    .padding(6)
                                    .width(Length::FillPortion(1)),
                                text_input("IFAC passphrase", &profile.passphrase)
                                    .secure(true)
                                    .on_input(move |value| {
                                        Message::TcpClientIfacPassphraseChanged {
                                            profile_id: ifac_pass_id.clone(),
                                            value,
                                        }
                                    })
                                    .padding(6)
                                    .width(Length::FillPortion(1)),
                            ]
                            .spacing(8)
                            .wrap(),
                        ]
                        .spacing(5),
                    )
                }
                InterfaceKind::TcpServer => {
                    let host_id = profile.profile_id.clone();
                    let port_id = profile.profile_id.clone();
                    let ifac_network_id = profile.profile_id.clone();
                    let ifac_pass_id = profile.profile_id.clone();
                    body.push(
                        column![
                            text(format!(
                                "TCP server listen: {}:{}",
                                profile.target_host, profile.target_port
                            ))
                            .size(ui_size(14))
                            .wrapping(iced::widget::text::Wrapping::WordOrGlyph)
                            .width(Length::Fill),
                            text(format!(
                                "IFAC: network={} passphrase={}",
                                if profile.network_name.is_empty() {
                                    "not set"
                                } else {
                                    profile.network_name.as_str()
                                },
                                if profile.passphrase.is_empty() {
                                    "not set"
                                } else {
                                    "configured"
                                }
                            ))
                            .size(ui_size(14))
                            .wrapping(iced::widget::text::Wrapping::WordOrGlyph)
                            .width(Length::Fill),
                            row![
                                text_input("listen IP", &profile.target_host)
                                    .on_input(move |value| Message::TcpServerHostChanged {
                                        profile_id: host_id.clone(),
                                        value,
                                    })
                                    .padding(6)
                                    .width(Length::FillPortion(3)),
                                text_input("listen port", &profile.target_port.to_string())
                                    .on_input(move |value| Message::TcpServerPortChanged {
                                        profile_id: port_id.clone(),
                                        value,
                                    })
                                    .padding(6)
                                    .width(Length::FillPortion(1)),
                            ]
                            .spacing(8)
                            .wrap(),
                            row![
                                text_input("IFAC network name", &profile.network_name)
                                    .on_input(move |value| {
                                        Message::TcpServerIfacNetworkChanged {
                                            profile_id: ifac_network_id.clone(),
                                            value,
                                        }
                                    })
                                    .padding(6)
                                    .width(Length::FillPortion(1)),
                                text_input("IFAC passphrase", &profile.passphrase)
                                    .secure(true)
                                    .on_input(move |value| {
                                        Message::TcpServerIfacPassphraseChanged {
                                            profile_id: ifac_pass_id.clone(),
                                            value,
                                        }
                                    })
                                    .padding(6)
                                    .width(Length::FillPortion(1)),
                            ]
                            .spacing(8)
                            .wrap(),
                        ]
                        .spacing(5),
                    )
                }
                InterfaceKind::I2p => body.push(
                    column![
                        subtle_button(
                            if profile.connectable {
                                "Set Not Connectable"
                            } else {
                                "Set Connectable"
                            },
                            Message::ToggleI2pConnectable(index)
                        ),
                        text(format!("I2P connectable: {}", profile.connectable)).size(ui_size(14)),
                        text(format!(
                            "I2P peers: {}",
                            if profile.peers.is_empty() {
                                "none".into()
                            } else {
                                profile.peers.join(", ")
                            }
                        ))
                        .size(ui_size(14))
                        .wrapping(iced::widget::text::Wrapping::WordOrGlyph)
                        .width(Length::Fill),
                        text_input("comma-separated I2P peers", &profile.peers.join(", "))
                            .on_input({
                                let profile_id = profile.profile_id.clone();
                                move |value| Message::I2pPeersChanged {
                                    profile_id: profile_id.clone(),
                                    value,
                                }
                            })
                            .padding(6)
                            .width(Length::Fill),
                    ]
                    .spacing(5),
                ),
                InterfaceKind::RNode => body.push(
                    column![
                        text(format!(
                            "RNode device: {}",
                            if profile.device_port.is_empty() {
                                "none"
                            } else {
                                profile.device_port.as_str()
                            }
                        ))
                        .size(ui_size(14))
                        .wrapping(iced::widget::text::Wrapping::WordOrGlyph)
                        .width(Length::Fill),
                        text(format!(
                            "radio: frequency={} bandwidth={} tx_power={} spreading={} coding={}",
                            profile.frequency,
                            profile.bandwidth,
                            profile.tx_power,
                            profile.spreading_factor,
                            profile.coding_rate
                        ))
                        .size(ui_size(14))
                        .wrapping(iced::widget::text::Wrapping::WordOrGlyph)
                        .width(Length::Fill),
                        text_input("device port, e.g. /dev/ttyUSB0", &profile.device_port)
                            .on_input({
                                let profile_id = profile.profile_id.clone();
                                move |value| Message::RNodeDevicePortChanged {
                                    profile_id: profile_id.clone(),
                                    value,
                                }
                            })
                            .padding(6)
                            .width(Length::Fill),
                        row![
                            text_input("frequency Hz", &profile.frequency.to_string())
                                .on_input({
                                    let profile_id = profile.profile_id.clone();
                                    move |value| Message::RNodeFrequencyChanged {
                                        profile_id: profile_id.clone(),
                                        value,
                                    }
                                })
                                .padding(6),
                            text_input("bandwidth Hz", &profile.bandwidth.to_string())
                                .on_input({
                                    let profile_id = profile.profile_id.clone();
                                    move |value| Message::RNodeBandwidthChanged {
                                        profile_id: profile_id.clone(),
                                        value,
                                    }
                                })
                                .padding(6),
                        ]
                        .spacing(8)
                        .wrap(),
                        row![
                            text_input("TX power dBm", &profile.tx_power.to_string())
                                .on_input({
                                    let profile_id = profile.profile_id.clone();
                                    move |value| Message::RNodeTxPowerChanged {
                                        profile_id: profile_id.clone(),
                                        value,
                                    }
                                })
                                .padding(6),
                            text_input("spreading factor", &profile.spreading_factor.to_string())
                                .on_input({
                                    let profile_id = profile.profile_id.clone();
                                    move |value| Message::RNodeSpreadingFactorChanged {
                                        profile_id: profile_id.clone(),
                                        value,
                                    }
                                })
                                .padding(6),
                            text_input("coding rate", &profile.coding_rate.to_string())
                                .on_input({
                                    let profile_id = profile.profile_id.clone();
                                    move |value| Message::RNodeCodingRateChanged {
                                        profile_id: profile_id.clone(),
                                        value,
                                    }
                                })
                                .padding(6),
                        ]
                        .spacing(8)
                        .wrap(),
                    ]
                    .spacing(5),
                ),
                InterfaceKind::Auto | InterfaceKind::Unknown(_) => body.push(
                    column![
                        text("Generic interface: no kind-specific settings are available.")
                            .size(ui_size(14))
                            .wrapping(iced::widget::text::Wrapping::WordOrGlyph)
                            .width(Length::Fill)
                    ]
                    .spacing(5),
                ),
            };
            column.push(section_card(title, body))
        });
    let preview = desktop
        .app
        .interfaces_state
        .config_preview
        .as_ref()
        .cloned()
        .unwrap_or_else(|| "No generated Reticulum config preview loaded.".into());

    let mut interface_setup_actions = vec![
        omen_button("Add TCP Gateway", Message::CreateTcpClientInterface),
        subtle_button("Add I2P", Message::CreateI2pInterface),
        subtle_button("Add RNode", Message::CreateRNodeInterface),
    ];
    interface_setup_actions.extend(desktop.gateway_preset_buttons());
    interface_setup_actions.push(subtle_button(
        "Settings",
        Message::SwitchSection(WorkspaceSection::Settings),
    ));

    let mut native_runtime_body = column![
        wrapped_panel_text(
            "First run: add or enable WNS/RMAP, add any private gateway or RNode, rename your identity, then restart so Directory announces and OMEN services start cleanly.",
        ),
        action_grid(interface_setup_actions, 3),
        text(desktop.gateway_preset_status_line())
            .size(ui_size(14))
            .wrapping(iced::widget::text::Wrapping::WordOrGlyph)
            .width(Length::Fill),
    ]
    .spacing(6);
    if let Some(restart_line) = interface_restart_recommendation_line(
        &desktop.app.interfaces_state.profiles,
        runtime_interface_stats,
    ) {
        native_runtime_body = native_runtime_body.push(
            text(restart_line)
                .size(ui_size(14))
                .wrapping(iced::widget::text::Wrapping::WordOrGlyph)
                .width(Length::Fill),
        );
    }
    native_runtime_body = native_runtime_body
        .push(action_grid(
            vec![
                omen_button("Start Native Runtime", Message::StartNativeRuntime),
                subtle_button("Preview Config", Message::PreviewManagedConfig),
                subtle_button("Export Config", Message::ExportManagedConfig),
                subtle_button("Preflight", Message::NativePreflight),
                subtle_button("Dry Smoke", Message::NativeSmokeDryRun),
                subtle_button("Live Probe", Message::NativeSmokeLiveProbe),
                omen_button("Live Fetch", Message::NativeLiveFetchValidate),
            ],
            4,
        ))
        .push(
            text(format!(
                "profiles={} selected={}",
                desktop.app.interfaces_state.profiles.len(),
                selected
                    .map(|index| index.to_string())
                    .unwrap_or_else(|| "none".into())
            ))
            .size(ui_size(14))
            .wrapping(iced::widget::text::Wrapping::WordOrGlyph)
            .width(Length::Fill),
        )
        .push(
            text(format!(
                "export: {}",
                desktop
                    .app
                    .interfaces_state
                    .last_config_export_path
                    .as_ref()
                    .map(|path| path.display().to_string())
                    .unwrap_or_else(|| "none".into())
            ))
            .size(ui_size(14))
            .wrapping(iced::widget::text::Wrapping::WordOrGlyph)
            .width(Length::Fill),
        )
        .push(
            text(format!(
                "config: {}",
                desktop.app.interface_service.config_path().display()
            ))
            .size(ui_size(13))
            .wrapping(iced::widget::text::Wrapping::WordOrGlyph)
            .width(Length::Fill),
        );

    let mut content = column![
        text("Interfaces").size(ui_size(28)),
        section_card("Native Runtime Interfaces", native_runtime_body),
    ]
    .spacing(12);
    if let Some(profile) = desktop.app.pending_interface_delete_profile() {
        content = content.push(section_card(
            "Confirm Interface Delete",
            column![
                text(format!(
                    "Delete '{}' ({:?}) from the managed Reticulum interface config?",
                    profile.name, profile.kind
                ))
                .size(ui_size(15)),
                text("This removes the profile and reapplies the generated config. The last remaining profile cannot be deleted.")
                    .size(ui_size(13)),
                row![
                    warning_button("Confirm Delete", Message::ConfirmInterfaceDelete),
                    subtle_button("Cancel", Message::CancelInterfaceDelete),
                ]
                .spacing(10)
                .wrap(),
            ]
            .spacing(8),
        ));
    }
    let preview_summary = interface_config_summary_lines(&desktop.app.interfaces_state.profiles)
        .into_iter()
        .fold(column![].spacing(2), |column, line| {
            column.push(
                text(line)
                    .size(ui_size(13))
                    .wrapping(iced::widget::text::Wrapping::WordOrGlyph)
                    .width(Length::Fill),
            )
        });
    let preview_lines = interface_config_preview_lines(&preview).into_iter().fold(
        column![].spacing(2),
        |column, line| {
            column.push(
                text(line)
                    .size(ui_size(13))
                    .wrapping(iced::widget::text::Wrapping::WordOrGlyph)
                    .width(Length::Fill),
            )
        },
    );
    let preview_body = column![preview_summary, preview_lines].spacing(8);
    content = content
        .push(profiles)
        .push(section_card("Config Preview", preview_body));

    app_scrollable(content.width(Length::Fill))
        .height(Length::Fill)
        .into()
}
