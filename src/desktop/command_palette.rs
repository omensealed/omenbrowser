use iced::widget::{column, container, opaque, row, text, text_input};
use iced::{Element, Length};

use crate::workspace::WorkspaceSection;

use super::{
    card_container_style, omen_button, subtle_button, wrapped_panel_text, BrowserMessage,
    CommandPaletteCommand, DesktopApp, IdentityMessage, Message, ShellMessage,
    DESKTOP_PANEL_PADDING,
};

const COMMAND_PALETTE_QUERY_MAX_CHARS: usize = 128;
const COMMAND_PALETTE_QUERY_MAX_BYTES: usize = 256;
const COMMAND_PALETTE_MAX_RESULTS: usize = 8;
const COMMAND_PALETTE_INPUT_ID: &str = "omen-command-palette-input";

const COMMANDS: [CommandPaletteCommand; 10] = [
    CommandPaletteCommand::OpenBrowser,
    CommandPaletteCommand::OpenMessages,
    CommandPaletteCommand::OpenDirectory,
    CommandPaletteCommand::OpenNetworkDoctor,
    CommandPaletteCommand::OpenDiagnostics,
    CommandPaletteCommand::OpenMonitoring,
    CommandPaletteCommand::NewBrowserTab,
    CommandPaletteCommand::RequestActiveBrowserPath,
    CommandPaletteCommand::InspectActiveBrowserPath,
    CommandPaletteCommand::CopyActiveIdentityHash,
];

pub(in crate::desktop) fn bounded_command_palette_query(query: &str) -> String {
    let mut output = String::new();
    for character in query.chars().take(COMMAND_PALETTE_QUERY_MAX_CHARS) {
        if output.len().saturating_add(character.len_utf8()) > COMMAND_PALETTE_QUERY_MAX_BYTES {
            break;
        }
        output.push(character);
    }
    output
}

pub(in crate::desktop) fn command_palette_input_id() -> iced::widget::Id {
    iced::widget::Id::new(COMMAND_PALETTE_INPUT_ID)
}

pub(in crate::desktop) fn command_palette_results(query: &str) -> Vec<CommandPaletteCommand> {
    let normalized = query.trim().to_ascii_lowercase();
    COMMANDS
        .iter()
        .copied()
        .filter(|command| {
            normalized.is_empty()
                || normalized
                    .split_whitespace()
                    .all(|term| command.search_text().contains(term))
        })
        .take(COMMAND_PALETTE_MAX_RESULTS)
        .collect()
}

impl CommandPaletteCommand {
    fn label(self) -> &'static str {
        match self {
            Self::OpenBrowser => "Open Browser workspace",
            Self::OpenMessages => "Open Messages workspace",
            Self::OpenDirectory => "Open Directory",
            Self::OpenNetworkDoctor => "Open Network Doctor",
            Self::OpenDiagnostics => "Open Diagnostics",
            Self::OpenMonitoring => "Open Monitoring and Operations",
            Self::NewBrowserTab => "New browser tab",
            Self::RequestActiveBrowserPath => "Request active browser path",
            Self::InspectActiveBrowserPath => "Inspect active browser path",
            Self::CopyActiveIdentityHash => "Copy active identity hash",
        }
    }

    fn search_text(self) -> &'static str {
        match self {
            Self::OpenBrowser => "open browser workspace tab",
            Self::OpenMessages => "open messages lxmf workspace conversation",
            Self::OpenDirectory => "open directory peers nodes propagation",
            Self::OpenNetworkDoctor => "open network doctor status troubleshoot",
            Self::OpenDiagnostics => "open diagnostics reports support",
            Self::OpenMonitoring => "open monitoring operations transfers queues",
            Self::NewBrowserTab => "new browser tab open",
            Self::RequestActiveBrowserPath => "request warm active browser path discovery",
            Self::InspectActiveBrowserPath => "inspect diagnose active browser path",
            Self::CopyActiveIdentityHash => "copy active identity hash clipboard",
        }
    }
}

pub(in crate::desktop) fn command_palette_message(command: CommandPaletteCommand) -> Message {
    match command {
        CommandPaletteCommand::OpenBrowser => {
            Message::Shell(ShellMessage::SwitchSection(WorkspaceSection::Browser))
        }
        CommandPaletteCommand::OpenMessages => {
            Message::Shell(ShellMessage::SwitchSection(WorkspaceSection::Messages))
        }
        CommandPaletteCommand::OpenDirectory => {
            Message::Shell(ShellMessage::SwitchSection(WorkspaceSection::Directory))
        }
        CommandPaletteCommand::OpenNetworkDoctor => {
            Message::Shell(ShellMessage::SwitchSection(WorkspaceSection::NetworkDoctor))
        }
        CommandPaletteCommand::OpenDiagnostics => {
            Message::Shell(ShellMessage::SwitchSection(WorkspaceSection::Diagnostics))
        }
        CommandPaletteCommand::OpenMonitoring => {
            Message::Shell(ShellMessage::SwitchSection(WorkspaceSection::Monitoring))
        }
        CommandPaletteCommand::NewBrowserTab => Message::Browser(BrowserMessage::NewTab),
        CommandPaletteCommand::RequestActiveBrowserPath => {
            Message::Browser(BrowserMessage::WarmPath)
        }
        CommandPaletteCommand::InspectActiveBrowserPath => {
            Message::Browser(BrowserMessage::PathDiagnostics)
        }
        CommandPaletteCommand::CopyActiveIdentityHash => {
            Message::Identity(IdentityMessage::CopyActiveHash)
        }
    }
}

pub(in crate::desktop) fn command_palette_overlay(desktop: &DesktopApp) -> Element<'_, Message> {
    let results = command_palette_results(&desktop.ui.command_palette_query);
    let mut result_list = column![].spacing(6);
    if results.is_empty() {
        result_list = result_list.push(wrapped_panel_text("No matching command."));
    } else {
        for command in results {
            result_list = result_list.push(omen_button(
                command.label(),
                Message::Shell(ShellMessage::ExecuteCommandPalette(command)),
            ));
        }
    }

    let palette = container(
        column![
            row![
                text("Command Palette").size(22),
                subtle_button(
                    "Close",
                    Message::Shell(ShellMessage::CloseCommandPalette)
                )
            ]
            .spacing(12),
            text_input(
                "Filter commands...",
                &desktop.ui.command_palette_query
            )
            .id(command_palette_input_id())
            .on_input(|query| {
                Message::Shell(ShellMessage::CommandPaletteQueryChanged(query))
            })
            .on_submit(Message::Shell(
                ShellMessage::ExecuteFirstCommandPaletteResult
            ))
            .width(Length::Fill),
            result_list,
            wrapped_panel_text("Ctrl+K opens or closes this palette. Enter runs the first match; Escape closes it."),
        ]
        .spacing(10),
    )
    .style(card_container_style)
    .padding(DESKTOP_PANEL_PADDING)
    .width(Length::Fixed(520.0));

    container(opaque(palette))
        .width(Length::Fill)
        .height(Length::Fill)
        .center(Length::Fill)
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_and_results_are_bounded() {
        let query = bounded_command_palette_query(&"é".repeat(300));
        assert!(query.chars().count() <= COMMAND_PALETTE_QUERY_MAX_CHARS);
        assert!(query.len() <= COMMAND_PALETTE_QUERY_MAX_BYTES);
        assert!(command_palette_results("").len() <= COMMAND_PALETTE_MAX_RESULTS);
    }

    #[test]
    fn matching_uses_all_terms_and_routes_through_existing_messages() {
        assert_eq!(
            command_palette_results("browser path"),
            vec![
                CommandPaletteCommand::RequestActiveBrowserPath,
                CommandPaletteCommand::InspectActiveBrowserPath,
            ]
        );
        assert!(matches!(
            command_palette_message(CommandPaletteCommand::OpenNetworkDoctor),
            Message::Shell(ShellMessage::SwitchSection(WorkspaceSection::NetworkDoctor))
        ));
        assert!(matches!(
            command_palette_message(CommandPaletteCommand::CopyActiveIdentityHash),
            Message::Identity(IdentityMessage::CopyActiveHash)
        ));
    }
}
