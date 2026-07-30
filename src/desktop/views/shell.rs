use iced::widget::text::Wrapping;
use iced::widget::{column, container, opaque, row, stack, text};
use iced::{Color, Element, Length, Padding};

use crate::workspace::WorkspaceSection;

use super::super::*;

impl DesktopApp {
    pub(in crate::desktop) fn view(&self) -> Element<'_, Message> {
        if !self.ui.shutdown_phase.is_running() {
            return text("Shutting down OMENbrowser_rs...").into();
        }

        let footer_task = compact_footer_status(&self.app.status.task, 120);
        let runtime_icon = if self.app.runtime_status.connected {
            "🟢"
        } else {
            "🔴"
        };
        let identity_label = compact_identity_status_label(&self.app.status.identity);
        let (trusted_unread, untrusted_unread) = self.footer_lxmf_unread_counts();
        let mut status = row![
            tooltip_icon_button(
                ICON_STATUS_MENU,
                "Menu",
                Message::Shell(ShellMessage::ToggleNavigation)
            ),
            tooltip_icon_button(
                ICON_COMMAND_PALETTE,
                "Commands",
                Message::Shell(ShellMessage::OpenCommandPalette)
            ),
            container(text(runtime_icon).font(emoji_font())).width(Length::Fixed(18.0)),
            row![
                text(format!("{ICON_STATUS_IDENTITY} ")).font(nerd_icon_font()),
                text(identity_label)
            ]
            .spacing(6),
        ]
        .spacing(12);
        if trusted_unread > 0 || untrusted_unread > 0 {
            let unread = row![
                text(format!("{ICON_STATUS_UNREAD} ")).font(nerd_icon_font()),
                text(trusted_unread.to_string()).color(Color::from_rgb8(68, 220, 96)),
                text("/"),
                text(untrusted_unread.to_string()).color(Color::from_rgb8(230, 70, 76)),
            ]
            .spacing(4);
            status = status.push(unread);
        }
        status =
            status.push(container(text(footer_task).wrapping(Wrapping::None)).width(Length::Fill));

        let workspace_content = self.browser_messages_workspace_view();
        let section_overlay: Element<'_, Message> = match self.app.workspace.active_section {
            WorkspaceSection::Browser | WorkspaceSection::Messages => container(text(""))
                .width(Length::Shrink)
                .height(Length::Shrink)
                .into(),
            WorkspaceSection::Directory => views::directory::directory_view(self),
            WorkspaceSection::Identities => views::identities::identities_view(self),
            WorkspaceSection::Interfaces => views::interfaces::interfaces_view(self),
            WorkspaceSection::Monitoring => views::monitoring::monitoring_view(self),
            WorkspaceSection::NetworkDoctor => views::network_doctor::network_doctor_view(self),
            WorkspaceSection::Settings => views::settings::settings_view(self),
            WorkspaceSection::Diagnostics => views::diagnostics::diagnostics_view(self),
            WorkspaceSection::Logs => views::logs::logs_view(self),
            WorkspaceSection::Plugins => views::plugins::plugins_view(self),
            WorkspaceSection::Help => views::help::help_view(self),
        };
        let content: Element<'_, Message> = if matches!(
            self.app.workspace.active_section,
            WorkspaceSection::Browser | WorkspaceSection::Messages
        ) {
            stack([workspace_content, section_overlay]).into()
        } else {
            let section_overlay = container(opaque(section_overlay))
                .style(card_container_style)
                .width(Length::Fill)
                .height(Length::Fill);
            stack([workspace_content, section_overlay.into()]).into()
        };

        let content_card = container(content)
            .style(card_container_style)
            .padding(DESKTOP_PANEL_PADDING)
            .width(Length::Fill)
            .height(Length::Fill);
        let status_strip = container(status)
            .style(status_container_style)
            .padding(8)
            .width(Length::Fill)
            .height(Length::Fixed(ui_size(44) as f32));
        let workspace = if self.ui.navigation_open {
            row![self.navigation_sidebar(), content_card]
                .spacing(u32::from(DESKTOP_PANEL_PADDING))
                .height(Length::Fill)
        } else {
            row![content_card].height(Length::Fill)
        };

        let shell: Element<'_, Message> = container(
            column![workspace, status_strip]
                .spacing(u32::from(DESKTOP_PANEL_PADDING))
                .padding(DESKTOP_SHELL_PADDING),
        )
        .style(shell_container_style)
        .width(Length::Fill)
        .height(Length::Fill)
        .into();

        let overlay: Element<'_, Message> = if self.ui.command_palette_open {
            command_palette_overlay(self)
        } else if let Some(prompt) = &self.clearweb.external_link_prompt {
            container(opaque(external_link_prompt_view(
                prompt,
                &self.clearweb.external_browsers,
                self.app
                    .settings
                    .clearweb
                    .preferred_external_browser_command
                    .as_deref(),
                self.app.settings.clearweb.socks_proxy_enabled,
                self.clearweb.clearweb_proxy_endpoint.as_ref(),
            )))
            .padding(Padding {
                right: f32::from(DESKTOP_SHELL_PADDING + DESKTOP_PANEL_PADDING),
                bottom: ui_size(60) as f32,
                left: f32::from(DESKTOP_SHELL_PADDING + DESKTOP_PANEL_PADDING),
                ..Padding::default()
            })
            .align_right(Length::Fill)
            .align_bottom(Length::Fill)
            .into()
        } else {
            container(text(""))
                .width(Length::Fill)
                .height(Length::Fill)
                .into()
        };
        stack([shell, overlay]).into()
    }

    pub(in crate::desktop) fn footer_lxmf_unread_counts(&self) -> (u32, u32) {
        self.app.workspace.conversations.iter().fold(
            (0u32, 0u32),
            |(trusted, untrusted), conversation| {
                let unread = conversation.thread.unread_count;
                if unread == 0 {
                    return (trusted, untrusted);
                }
                if self.app.lxmf_peer_is_trusted(&conversation.peer_hash) {
                    (trusted.saturating_add(unread), untrusted)
                } else {
                    (trusted, untrusted.saturating_add(unread))
                }
            },
        )
    }

    fn navigation_sidebar(&self) -> Element<'_, Message> {
        let sections = WorkspaceSection::ALL
            .iter()
            .filter(|section| **section != WorkspaceSection::Messages)
            .fold(column![].spacing(8), |nav, section| {
                let button = if *section == self.app.workspace.active_section {
                    omen_button(
                        section.label(),
                        Message::Shell(ShellMessage::SwitchSection(*section)),
                    )
                } else {
                    subtle_button(
                        section.label(),
                        Message::Shell(ShellMessage::SwitchSection(*section)),
                    )
                };
                nav.push(button)
            });

        container(
            column![
                sections,
                subtle_button("Hide Menu", Message::Shell(ShellMessage::ToggleNavigation)),
            ]
            .spacing(u32::from(DESKTOP_PANEL_PADDING)),
        )
        .style(card_container_style)
        .padding(DESKTOP_PANEL_PADDING)
        .width(Length::Shrink)
        .height(Length::Fill)
        .into()
    }
}
