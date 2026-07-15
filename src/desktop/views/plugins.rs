use iced::widget::{column, row, text};
use iced::{Element, Length};

use super::super::{
    action_grid, app_scrollable, omen_button, section_card, subtle_button, ui_size, warning_button,
    wrapped_text_owned, DesktopApp, Message, PluginMessage,
};

pub(in crate::desktop) fn plugins_view(desktop: &DesktopApp) -> Element<'_, Message> {
    let warnings = desktop
        .app
        .plugins_state
        .warnings
        .iter()
        .fold(column![].spacing(4), |column, warning| {
            column.push(wrapped_text_owned(warning.clone(), 14))
        });
    let plugins = desktop.app.plugins_state.installed.iter().enumerate().fold(
        column![].spacing(8),
        |column, (index, plugin)| {
            let title = if Some(index) == desktop.app.plugins_state.selected {
                format!("[selected] {}", plugin.manifest.name)
            } else {
                plugin.manifest.name.clone()
            };
            column.push(section_card(
                title,
                column![
                    row![
                        subtle_button("Select", Message::Plugin(PluginMessage::Select(index))),
                        subtle_button("Toggle", Message::Plugin(PluginMessage::Toggle(index))),
                        warning_button(
                            "Remove",
                            Message::Plugin(PluginMessage::BeginRemove(index)),
                        ),
                    ]
                    .spacing(8)
                    .wrap(),
                    row![
                        wrapped_text_owned(format!("id: {}", plugin.manifest.plugin_id), 14),
                        wrapped_text_owned(format!("v{}", plugin.manifest.version), 14),
                    ]
                    .spacing(8)
                    .wrap(),
                    wrapped_text_owned(
                        format!(
                            "builtin={} enabled={} trusted={}",
                            plugin.builtin, plugin.enabled, plugin.trusted
                        ),
                        14
                    ),
                    wrapped_text_owned(format!("author: {}", plugin.manifest.author), 14),
                    wrapped_text_owned(format!("entrypoint: {}", plugin.manifest.entrypoint), 14),
                    wrapped_text_owned(
                        format!("permissions: {}", plugin.manifest.permissions.len()),
                        14
                    ),
                    wrapped_text_owned(plugin.manifest.description.clone(), 14),
                ]
                .spacing(5),
            ))
        },
    );
    let details = desktop
        .app
        .selected_plugin_detail_lines()
        .into_iter()
        .fold(column![].spacing(3), |column, line| {
            column.push(wrapped_text_owned(line, 13))
        });
    let micronplus_diagnostics = desktop
        .app
        .active_micronplus_diagnostic_lines()
        .into_iter()
        .fold(column![].spacing(3), |column, line| {
            column.push(wrapped_text_owned(line, 13))
        });

    app_scrollable(
        column![
            text("Plugins").size(ui_size(28)),
            section_card(
                "Plugin Runtime",
                column![
                    action_grid(
                        vec![
                            omen_button(
                                "Install Trusted Folder",
                                Message::Plugin(PluginMessage::BeginInstall),
                            ),
                            subtle_button(
                                "Toggle Selected",
                                Message::Plugin(PluginMessage::ToggleSelected),
                            ),
                            warning_button(
                                "Remove Selected",
                                Message::Plugin(PluginMessage::BeginSelectedRemove),
                            ),
                            subtle_button("Refresh", Message::Plugin(PluginMessage::Refresh),),
                            subtle_button(
                                "Open Plugin Logs",
                                Message::Plugin(PluginMessage::ShowLogs),
                            ),
                        ],
                        5,
                    ),
                    wrapped_text_owned(
                        format!(
                            "installed={} manifests={} selected={}",
                            desktop.app.plugins_state.installed.len(),
                            desktop.app.plugins_state.manifests.len(),
                            desktop
                                .app
                                .plugins_state
                                .selected
                                .map(|index| index.to_string())
                                .unwrap_or_else(|| "none".into())
                        ),
                        14
                    ),
                    wrapped_text_owned(
                        "MicronPlus Text UI is built in and still trust-gated by node trust.",
                        14,
                    ),
                ]
                .spacing(4),
            ),
            section_card("Warnings", warnings),
            section_card("MicronPlus Active Page", micronplus_diagnostics),
            plugins,
            section_card("Selected Plugin", details),
        ]
        .spacing(12)
        .width(Length::Fill),
    )
    .height(Length::Fill)
    .into()
}
