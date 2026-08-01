use iced::widget::{column, row, text, text_input};
use iced::{Element, Length};

use crate::interfaces::InterfaceKind;

use super::super::{
    action_grid, app_scrollable, omen_button, omen_button_owned, section_card, subtle_button,
    subtle_button_owned, ui_size, wrapped_panel_text, wrapped_text_owned, ClearwebMessage,
    DesktopApp, DiagnosticsMessage, ExternalBrowserMessage, IdentityMessage, InterfaceMessage,
    Message, RuntimeMessage, ThemeMessage, DESKTOP_THEME_CHOICES,
};

pub(in crate::desktop) fn settings_view(desktop: &DesktopApp) -> Element<'_, Message> {
    let readiness = desktop.app.native_reticulum_readiness();
    let readiness_lines = if readiness.issues.is_empty() {
        vec!["readiness: no blockers reported".to_string()]
    } else {
        readiness
            .issues
            .iter()
            .map(|issue| format!("blocker: {issue}"))
            .collect::<Vec<_>>()
    };
    let readiness_column = readiness_lines.into_iter().fold(
        column![wrapped_text_owned(
            format!(
                "native readiness: ready={} configured={} compiled={} | {}",
                readiness.ready, readiness.configured, readiness.compiled, readiness.summary
            ),
            14
        )]
        .spacing(4),
        |column, line| column.push(wrapped_text_owned(line, 14)),
    );
    let interface_column = desktop.app.native_interface_readiness().into_iter().fold(
        column![text("Interfaces").size(ui_size(18))].spacing(4),
        |column, detail| {
            column.push(wrapped_text_owned(
                format!(
                    "{} | {} | enabled={} | supported={} | blocks={} | {}",
                    detail.name,
                    detail.kind,
                    detail.enabled,
                    detail.supported,
                    detail.blocks_native_startup,
                    detail.reason
                ),
                14,
            ))
        },
    );
    let interfaces = desktop
        .app
        .interfaces_state
        .profiles
        .iter()
        .enumerate()
        .fold(column![].spacing(4), |column, (index, profile)| {
            let summary = row![
                subtle_button(
                    "Select",
                    Message::Interface(InterfaceMessage::SelectProfile(index))
                ),
                wrapped_text_owned(
                    format!(
                        "{} | {:?} | {}",
                        profile.name,
                        profile.kind,
                        if profile.enabled {
                            "enabled"
                        } else {
                            "disabled"
                        }
                    ),
                    14
                ),
            ]
            .spacing(8)
            .wrap();

            let column = column
                .push(summary)
                .push(wrapped_text_owned(format!("profile index: {index}"), 12));

            if profile.kind == InterfaceKind::TcpClient {
                let host_id = profile.profile_id.clone();
                let port_id = profile.profile_id.clone();
                let ifac_network_id = profile.profile_id.clone();
                let ifac_pass_id = profile.profile_id.clone();
                column
                    .push(
                        row![
                            text("TCP host").size(ui_size(14)),
                            text_input("host", &profile.target_host)
                                .on_input(move |value| {
                                    Message::Interface(InterfaceMessage::TcpClientHostChanged {
                                        profile_id: host_id.clone(),
                                        value,
                                    })
                                })
                                .width(Length::FillPortion(2)),
                            text("port").size(ui_size(14)),
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
                    )
                    .push(
                        row![
                            text("IFAC").size(ui_size(14)),
                            text_input("network name", &profile.network_name)
                                .on_input(move |value| {
                                    Message::Interface(
                                        InterfaceMessage::TcpClientIfacNetworkChanged {
                                            profile_id: ifac_network_id.clone(),
                                            value,
                                        },
                                    )
                                })
                                .width(Length::FillPortion(2)),
                            text_input("passphrase", &profile.passphrase)
                                .secure(true)
                                .on_input(move |value| {
                                    Message::Interface(
                                        InterfaceMessage::TcpClientIfacPassphraseChanged {
                                            profile_id: ifac_pass_id.clone(),
                                            value,
                                        },
                                    )
                                })
                                .width(Length::FillPortion(2)),
                        ]
                        .spacing(8)
                        .wrap(),
                    )
            } else if profile.kind == InterfaceKind::TcpServer {
                let host_id = profile.profile_id.clone();
                let port_id = profile.profile_id.clone();
                let ifac_network_id = profile.profile_id.clone();
                let ifac_pass_id = profile.profile_id.clone();
                column
                    .push(
                        row![
                            text("TCP listen").size(ui_size(14)),
                            text_input("listen IP", &profile.target_host)
                                .on_input(move |value| {
                                    Message::Interface(InterfaceMessage::TcpServerHostChanged {
                                        profile_id: host_id.clone(),
                                        value,
                                    })
                                })
                                .width(Length::FillPortion(2)),
                            text("port").size(ui_size(14)),
                            text_input("port", &profile.target_port.to_string())
                                .on_input(move |value| {
                                    Message::Interface(InterfaceMessage::TcpServerPortChanged {
                                        profile_id: port_id.clone(),
                                        value,
                                    })
                                })
                                .width(Length::FillPortion(1)),
                        ]
                        .spacing(8)
                        .wrap(),
                    )
                    .push(
                        row![
                            text("IFAC").size(ui_size(14)),
                            text_input("network name", &profile.network_name)
                                .on_input(move |value| {
                                    Message::Interface(
                                        InterfaceMessage::TcpServerIfacNetworkChanged {
                                            profile_id: ifac_network_id.clone(),
                                            value,
                                        },
                                    )
                                })
                                .width(Length::FillPortion(2)),
                            text_input("passphrase", &profile.passphrase)
                                .secure(true)
                                .on_input(move |value| {
                                    Message::Interface(
                                        InterfaceMessage::TcpServerIfacPassphraseChanged {
                                            profile_id: ifac_pass_id.clone(),
                                            value,
                                        },
                                    )
                                })
                                .width(Length::FillPortion(2)),
                        ]
                        .spacing(8)
                        .wrap(),
                    )
            } else {
                column
            }
        });

    let theme_buttons = DESKTOP_THEME_CHOICES
        .iter()
        .fold(row![].spacing(8), |row, theme| {
            let theme_name = *theme;
            let button = if theme_name == desktop.app.settings.ui.theme_name {
                omen_button(
                    theme,
                    Message::Theme(ThemeMessage::SetTheme(theme_name.into())),
                )
            } else {
                subtle_button(
                    theme,
                    Message::Theme(ThemeMessage::SetTheme(theme_name.into())),
                )
            };
            row.push(button)
        })
        .wrap();
    let themes = column![
        wrapped_text_owned(format!("Theme: {}", desktop.app.settings.ui.theme_name), 14),
        theme_buttons,
    ]
    .spacing(8);

    let font_size = desktop.app.settings.ui.font_size.clamp(10, 24);
    let reduce_motion = desktop.app.settings.ui.reduce_motion;
    let low_power_mode = desktop.app.settings.ui.low_power_mode;
    let appearance = column![
        themes,
        row![
            wrapped_text_owned(format!("Font size: {font_size}px"), 14),
            subtle_button(
                "-",
                Message::Theme(ThemeMessage::SetFontSize(
                    font_size.saturating_sub(1).max(10),
                )),
            ),
            omen_button(
                "+",
                Message::Theme(ThemeMessage::SetFontSize(
                    font_size.saturating_add(1).min(24),
                )),
            ),
        ]
        .spacing(8)
        .wrap(),
        wrapped_text_owned("Font size applies on next launch.", 13),
        row![
            wrapped_text_owned(
                format!(
                    "Reduce motion: {}",
                    if reduce_motion { "On" } else { "Off" }
                ),
                14
            ),
            if reduce_motion {
                omen_button("Disable", Message::Theme(ThemeMessage::ToggleReducedMotion))
            } else {
                subtle_button("Enable", Message::Theme(ThemeMessage::ToggleReducedMotion))
            },
        ]
        .spacing(8)
        .wrap(),
        wrapped_text_owned(
            "Reduced motion pauses animated media previews while preserving a static image.",
            13
        ),
        row![
            wrapped_text_owned(
                format!(
                    "Low-power mode: {}",
                    if low_power_mode { "On" } else { "Off" }
                ),
                14
            ),
            if low_power_mode {
                omen_button("Disable", Message::Theme(ThemeMessage::ToggleLowPower))
            } else {
                subtle_button("Enable", Message::Theme(ThemeMessage::ToggleLowPower))
            },
        ]
        .spacing(8)
        .wrap(),
        wrapped_text_owned(
            "Low-power mode forces static previews and slows visible diagnostics sampling to 5 seconds; network and persistence semantics are unchanged.",
            13
        ),
    ]
    .spacing(8);

    let browser_choice_buttons = desktop.clearweb.external_browsers.iter().enumerate().fold(
        row![].spacing(8),
        |row, (index, browser)| {
            let selected = Some(browser.command.as_str())
                == desktop
                    .app
                    .settings
                    .clearweb
                    .preferred_external_browser_command
                    .as_deref();
            let label = format!("{} ({})", browser.label, browser.command);
            let button = if selected {
                omen_button_owned(
                    label,
                    Message::ExternalBrowser(ExternalBrowserMessage::SelectPreferred(index)),
                )
            } else {
                subtle_button_owned(
                    label,
                    Message::ExternalBrowser(ExternalBrowserMessage::SelectPreferred(index)),
                )
            };
            row.push(button)
        },
    );
    let clearweb = &desktop.app.settings.clearweb;
    let clearweb_card = section_card(
        "Clearweb / Tor",
        column![
            row![
                omen_button(
                    if clearweb.socks_proxy_enabled {
                        "Disable SOCKS5"
                    } else {
                        "Enable SOCKS5"
                    },
                    Message::Clearweb(ClearwebMessage::ToggleSocksProxy),
                ),
                subtle_button(
                    if clearweb.remote_media_enabled {
                        "Disable Remote Media"
                    } else {
                        "Enable Remote Media"
                    },
                    Message::Clearweb(ClearwebMessage::ToggleRemoteMedia),
                ),
                subtle_button(
                    "Clear Browser",
                    Message::ExternalBrowser(ExternalBrowserMessage::ClearPreferred),
                ),
            ]
            .spacing(8)
            .wrap(),
            text(format!(
                "SOCKS5 proxy: {}:{} | {}",
                clearweb.socks_proxy_host,
                clearweb.socks_proxy_port,
                if desktop.clearweb.clearweb_proxy_reachable {
                    "detected"
                } else {
                    "not detected"
                }
            ))
            .size(ui_size(14))
            .wrapping(iced::widget::text::Wrapping::WordOrGlyph)
            .width(Length::Fill),
            wrapped_text_owned(format!(
                "Tor proxy detection also checks {}:9150 for Tor Browser Bundle; active proxy: {}",
                clearweb.socks_proxy_host,
                desktop.clearweb.clearweb_proxy_endpoint
                    .as_ref()
                    .map(|(host, port)| format!("{host}:{port}"))
                    .unwrap_or_else(|| "none".into())
            ), 14),
            wrapped_text_owned(format!(
                "preferred external browser: {}",
                clearweb
                    .preferred_external_browser_command
                    .as_deref()
                    .unwrap_or("none; prompt decides per link")
            ), 14),
            browser_choice_buttons.wrap(),
            wrapped_panel_text("HTTP/HTTPS links from NomadNet and OMENchat are handed to an external browser prompt. Use Copy URL for Tor Browser. Launch buttons are for regular detected browsers or browser profiles you configured yourself."),
            wrapped_panel_text("Remote media remains off by default; rich media previews should use this SOCKS5 policy when OMENbrowser fetches bytes itself."),
        ]
        .spacing(8),
    );

    let theme_card = section_card("Appearance", appearance);
    let mut native_setup_actions = vec![
        omen_button(
            "Create Identity",
            Message::Identity(IdentityMessage::Create),
        ),
        omen_button(
            "Add TCP Gateway",
            Message::Interface(InterfaceMessage::CreateTcpClient),
        ),
    ];
    native_setup_actions.extend(desktop.gateway_preset_buttons());
    native_setup_actions.extend([
        omen_button(
            "Select Native Backend",
            Message::Runtime(RuntimeMessage::SelectNativeBackend),
        ),
        omen_button(
            "Start Native Runtime",
            Message::Runtime(RuntimeMessage::StartNativeRuntime),
        ),
        omen_button(
            "Full Quickstart",
            Message::Runtime(RuntimeMessage::NativeQuickstart),
        ),
    ]);
    let native_card = section_card(
        "First Run / Native Setup",
        column![
            action_grid(native_setup_actions, 3),
            action_grid(
                vec![
                    subtle_button(
                        "Preview Config",
                        Message::Diagnostics(DiagnosticsMessage::PreviewManagedConfig)
                    ),
                    subtle_button(
                        "Export Config",
                        Message::Diagnostics(DiagnosticsMessage::ExportManagedConfig)
                    ),
                    subtle_button(
                        "Preflight",
                        Message::Diagnostics(DiagnosticsMessage::NativePreflight)
                    ),
                    subtle_button(
                        "Dry Smoke",
                        Message::Diagnostics(DiagnosticsMessage::NativeSmokeDryRun)
                    ),
                    subtle_button(
                        "Live Probe",
                        Message::Diagnostics(DiagnosticsMessage::NativeSmokeLiveProbe)
                    ),
                    omen_button(
                        "Live Fetch",
                        Message::Diagnostics(DiagnosticsMessage::NativeLiveFetchValidate)
                    ),
                    subtle_button(
                        "Known Destinations",
                        Message::Diagnostics(DiagnosticsMessage::BeginKnownDestinationsPreload)
                    ),
                ],
                3,
            ),
        ]
        .spacing(8),
    );
    let status_card = section_card(
        "Native Runtime Status",
        column![
            wrapped_text_owned(
                format!("backend: {:?}", desktop.app.settings.runtime_backend),
                14
            ),
            wrapped_text_owned(
                format!(
                    "active runtime: {:?} | connected={} | {}",
                    desktop.app.runtime_status.backend,
                    desktop.app.runtime_status.connected,
                    desktop.app.runtime_status.message
                ),
                14
            ),
            wrapped_text_owned(
                format!(
                    "identity: {}",
                    desktop
                        .app
                        .settings
                        .identity_path
                        .as_ref()
                        .map(|path| path.display().to_string())
                        .unwrap_or_else(|| "none".into())
                ),
                14
            ),
            wrapped_text_owned(
                format!(
                    "Reticulum config: {}",
                    desktop
                        .app
                        .settings
                        .reticulum_config_path
                        .as_ref()
                        .map(|path| path.display().to_string())
                        .unwrap_or_else(|| "managed default".into())
                ),
                14
            ),
        ]
        .spacing(6),
    );
    let lxmf_sync_card = section_card(
        "LXMF Propagation Sync",
        column![
            row![
                omen_button(
                    if desktop.app.settings.auto_sync_after_propagation_accept {
                        "Disable Auto Sync"
                    } else {
                        "Enable Auto Sync"
                    },
                    Message::Runtime(RuntimeMessage::ToggleAutoSyncAfterPropagationAccept),
                ),
                subtle_button(
                    "Sync Now",
                    Message::Diagnostics(DiagnosticsMessage::SyncPropagationNow)
                ),
            ]
            .spacing(8)
            .wrap(),
            text(format!(
                "auto after propagation-node accept: {}",
                desktop.app.settings.auto_sync_after_propagation_accept
            ))
            .size(ui_size(14))
            .wrapping(iced::widget::text::Wrapping::WordOrGlyph)
            .width(Length::Fill),
            text(format!(
                "throttle interval: {}s | sync limit: {}",
                desktop.app.settings.lxmf_sync_interval, desktop.app.settings.lxmf_sync_limit
            ))
            .size(ui_size(14))
            .wrapping(iced::widget::text::Wrapping::WordOrGlyph)
            .width(Length::Fill),
            wrapped_panel_text("Propagation-node acceptance is not peer delivery; auto sync only fetches/updates propagation state."),
        ]
        .spacing(6),
    );
    let readiness_card = section_card(
        "Readiness",
        column![readiness_column, interface_column].spacing(10),
    );
    let interface_card = section_card(
        "Configured Interface Profiles",
        column![interfaces.width(Length::Fill)].spacing(8),
    );

    let setup = column![
        text("Settings").size(ui_size(28)),
        theme_card,
        clearweb_card,
        native_card,
        status_card,
        lxmf_sync_card,
        readiness_card,
        interface_card,
    ]
    .spacing(10)
    .width(Length::Fill);

    app_scrollable(setup).height(Length::Fill).into()
}
