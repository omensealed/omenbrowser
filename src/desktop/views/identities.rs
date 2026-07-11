use iced::widget::{column, container, row, text, text_input};
use iced::{Element, Length};

use crate::workspace::WorkspaceSection;

use super::super::{
    action_grid, app_scrollable, compact_label, omen_button, section_card, status_container_style,
    subtle_button, subtle_button_owned, ui_size, warning_button, warning_container_style,
    wrapped_text_owned, DesktopApp, Message,
};

pub(in crate::desktop) fn identities_view(desktop: &DesktopApp) -> Element<'_, Message> {
    let active_path = desktop
        .app
        .settings
        .identity_path
        .as_ref()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| "none".into());
    let active_label = desktop
        .app
        .settings
        .active_identity_label
        .clone()
        .unwrap_or_default();
    let active_hash = desktop
        .app
        .runtime_status
        .active_identity
        .as_ref()
        .map(|identity| identity.hash_hex.clone())
        .unwrap_or_else(|| "not attached".into());
    let active_storage_root = desktop
        .app
        .settings
        .identity_path
        .as_ref()
        .map(|path| {
            desktop
                .app
                .paths
                .storage_root_for_identity_path(path)
                .display()
                .to_string()
        })
        .unwrap_or_else(|| "none".into());
    let active_reticulum_storage = desktop
        .app
        .settings
        .identity_path
        .as_ref()
        .map(|path| {
            desktop
                .app
                .paths
                .scoped_to_identity_path(path)
                .reticulum_storage_dir
                .display()
                .to_string()
        })
        .unwrap_or_else(|| "none".into());
    let managed_profiles = desktop.app.managed_identity_profiles();
    let has_managed_profiles = !managed_profiles.is_empty();
    let rows = managed_profiles
        .into_iter()
        .fold(column![].spacing(8), |column, profile| {
            let is_active = desktop
                .app
                .settings
                .identity_path
                .as_ref()
                .is_some_and(|path| *path == profile.path);
            let status = if is_active { "active" } else { "managed" };
            let mut header = row![wrapped_text_owned(
                format!(
                    "{} | {} | {}",
                    profile.label,
                    compact_label(&profile.hash_hex, 16),
                    status
                ),
                14
            )]
            .spacing(8);
            if !is_active {
                header = header.push(subtle_button_owned(
                    "Use".to_string(),
                    Message::ActivateManagedIdentity(profile.path.display().to_string()),
                ));
            }
            let storage_paths = desktop.app.paths.with_identity_storage_root(
                desktop
                    .app
                    .paths
                    .storage_root_for_identity_profile(&profile),
            );
            column.push(
                container(
                    column![
                        header.wrap(),
                        wrapped_text_owned(format!("identity: {}", profile.path.display()), 12),
                        wrapped_text_owned(
                            format!(
                                "storage: {}",
                                desktop
                                    .app
                                    .paths
                                    .storage_root_for_identity_profile(&profile)
                                    .display()
                            ),
                            12
                        ),
                        wrapped_text_owned(
                            format!(
                                "reticulum: {}",
                                storage_paths.reticulum_storage_dir.display()
                            ),
                            12
                        ),
                        wrapped_text_owned(
                            format!("messages: {}", storage_paths.messages_dir.display()),
                            12
                        ),
                    ]
                    .spacing(4),
                )
                .style(status_container_style)
                .padding(10)
                .width(Length::Fill),
            )
        });
    let managed = if has_managed_profiles {
        rows
    } else {
        column![text("No managed identities found.").size(ui_size(14))].spacing(8)
    };

    app_scrollable(
        column![
            section_card(
                "Active Identity",
                column![
                    text_input("identity name", &active_label)
                        .on_input(Message::ActiveIdentityLabelChanged)
                        .width(Length::Fill),
                    row![
                        wrapped_text_owned(format!("hash: {active_hash}"), 14),
                        subtle_button("Copy", Message::CopyActiveIdentityHash),
                    ]
                    .spacing(8)
                    .align_y(iced::Alignment::Center)
                    .wrap(),
                    wrapped_text_owned(format!("identity: {active_path}"), 12),
                    wrapped_text_owned(format!("storage: {active_storage_root}"), 12),
                    wrapped_text_owned(format!(
                        "reticulum storage: {active_reticulum_storage}"
                    ), 12),
                    action_grid(
                        vec![
                            omen_button("Create Identity", Message::CreateIdentity),
                            subtle_button("Announce Now", Message::AnnounceIdentityNow),
                            subtle_button("Clear Active", Message::ClearActiveIdentity),
                            warning_button("Delete Active", Message::DeleteActiveIdentity),
                        ],
                        4,
                    ),
                    identity_delete_confirmation_view(desktop),
                ]
                .spacing(8),
            ),
            section_card("Managed Identities", managed),
            section_card(
                "Paths",
                column![
                    wrapped_text_owned(format!(
                        "managed identities: {}",
                        desktop.app.paths.identities_dir.display()
                    ), 14),
                    wrapped_text_owned(format!(
                        "identity storage roots: {}",
                        desktop.app.paths.identity_storage_dir.display()
                    ), 14),
                    wrapped_text_owned(
                        "External identity and custom Reticulum config paths remain editable in Settings.",
                        14,
                    ),
                    subtle_button("Open Settings", Message::SwitchSection(WorkspaceSection::Settings)),
                ]
                .spacing(8),
            ),
        ]
        .spacing(12)
        .width(Length::Fill),
    )
    .height(Length::Fill)
    .into()
}

fn identity_delete_confirmation_view(desktop: &DesktopApp) -> Element<'_, Message> {
    if !desktop.ui.identity_delete_confirming {
        return text("").size(ui_size(1)).into();
    }
    container(
        column![
            text("Delete the active identity?").size(ui_size(16)),
            wrapped_text_owned(
                "A backup is created first, but identity loss is critical. Confirm only if this identity should no longer be usable here.",
                13,
            ),
            row![
                warning_button("Confirm Delete", Message::ConfirmDeleteActiveIdentity),
                subtle_button("Cancel", Message::CancelDeleteActiveIdentity),
            ]
            .spacing(8)
            .wrap(),
        ]
        .spacing(8),
    )
    .style(warning_container_style)
    .padding(10)
    .width(Length::Fill)
    .into()
}
