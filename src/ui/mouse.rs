use ratatui::layout::Rect;

use crate::app::App;
use crate::storage::settings::RuntimeBackendSetting;
use crate::workspace::WorkspaceSection;

const SIDEBAR_WIDTH: u16 = 28;
const HEADER_HEIGHT: u16 = 3;
const FOOTER_HEIGHT: u16 = 2;
const BROWSER_TAB_HEIGHT: u16 = 2;
const BROWSER_COMMAND_HEIGHT: u16 = 3;
const MESSAGE_TAB_HEIGHT: u16 = 2;
const MESSAGE_COMPOSER_HEIGHT: u16 = 5;
const PLUGIN_DETAIL_HEIGHT: u16 = 9;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MouseAction {
    SwitchSection(WorkspaceSection),
    ActivateSidebarIndex(usize),
    SelectBrowserTab(usize),
    SelectConversationTab(usize),
    FocusBrowserAddress,
    ActivateBrowserCell { row: u16, col: u16, width: u16 },
    SelectDirectoryIndex(usize),
    ActivateDirectoryIndex(usize),
    SaveDirectoryIndex(usize),
    ToggleDirectoryTrustIndex(usize),
    SelectInterfaceIndex(usize),
    ToggleInterfaceEnabledIndex(usize),
    CreateInterfaceTcpClient,
    CreateInterfaceI2p,
    CreateInterfaceRNode,
    DeleteInterfaceIndex(usize),
    EditInterfaceName,
    EditInterfaceTcpHost,
    EditInterfaceTcpPort,
    EditInterfaceI2pPeers,
    ToggleInterfaceConnectable,
    EditInterfaceRNodeDevicePort,
    EditInterfaceRNodeFrequency,
    EditInterfaceRNodeBandwidth,
    EditInterfaceRNodeTxPower,
    EditInterfaceRNodeSpreadingFactor,
    EditInterfaceRNodeCodingRate,
    PreviewManagedReticulumConfig,
    ExportManagedReticulumConfig,
    SelectPluginIndex(usize),
    ActivatePluginIndex(usize),
    ToggleBrowserFormState,
    CycleBrowserFormSensitivePolicy,
    SelectRuntimeBackend(RuntimeBackendSetting),
    CycleReticulumInstanceMode,
    RestartToApplySettings,
    ToggleAnnounceOnStart,
    TogglePeriodicLxmfSync,
    EditPreferredPropagation,
    TogglePluginRemoteContent,
    CreateManagedIdentity,
    RunNativeQuickstart,
    EditSettingsIdentityPath,
    EditSettingsReticulumConfigPath,
    PreviewDiagnosticsBundle,
    ExportDiagnosticsBundle,
    ClearDiagnosticsPreview,
    ProbeActiveBrowserPageFetchDryRun,
    ProbeActiveBrowserPageFetchLive,
    PreviewLiveInteropReport,
    ExportLiveInteropReport,
    RunNativeNetworkSmokeTestDryRun,
    RunNativeNetworkSmokeTestLive,
    RunNativeNetworkLiveFetchValidation,
    RunNativeLxmfSmokeSend,
    RunNativeLxmfLiveInterop,
    SelectLxmfPeerCandidate(usize),
    RunPathDiscoveryDiagnostics,
    PreloadKnownDestinations,
    CycleLogSeverityFilter,
    CycleLogSourceFilter,
    EditSettingsThemeName,
    EditSettingsDefaultStartPage,
    EditSettingsLxmfSyncInterval,
    EditSettingsLxmfSyncLimit,
    EditSettingsBrowserFormMaxAge,
    EditSettingsLogMaxBytes,
    EditSettingsLogRetainFiles,
    EditSettingsLogLoadRecentEntries,
    ForgetCurrentPageFormState,
    ForgetCurrentNodeFormState,
    ForgetAllBrowserFormState,
    FocusMessageTitle,
    FocusMessageBody,
    ToggleMessageDeliveryMode,
    ToggleMessageTicket,
    SendMessageDraft,
}

pub fn action_for_click(app: &App, terminal: Rect, column: u16, row: u16) -> Option<MouseAction> {
    if !contains(terminal, column, row) {
        return None;
    }
    if let Some(action) = header_action(terminal, column, row) {
        return Some(action);
    }

    let body = body_rect(terminal)?;
    if !contains(body, column, row) {
        return None;
    }

    let sidebar = Rect {
        x: body.x,
        y: body.y,
        width: SIDEBAR_WIDTH.min(body.width),
        height: body.height,
    };
    if contains(sidebar, column, row) {
        return sidebar_action(app, sidebar, row);
    }

    let workspace = Rect {
        x: body.x + sidebar.width,
        y: body.y,
        width: body.width.saturating_sub(sidebar.width),
        height: body.height,
    };
    workspace_action(app, workspace, column, row)
}

pub fn browser_content_inner_size(terminal: Rect) -> (u16, u16) {
    let Some(body) = body_rect(terminal) else {
        return (1, 1);
    };
    let workspace_width = body.width.saturating_sub(SIDEBAR_WIDTH.min(body.width));
    let workspace_height = body.height;
    let content_width = workspace_width.saturating_sub(2).max(1);
    let content_height = workspace_height
        .saturating_sub(BROWSER_TAB_HEIGHT + BROWSER_COMMAND_HEIGHT)
        .saturating_sub(2)
        .max(1);
    (content_width, content_height)
}

fn header_action(terminal: Rect, column: u16, row: u16) -> Option<MouseAction> {
    let header = Rect {
        x: terminal.x,
        y: terminal.y,
        width: terminal.width,
        height: HEADER_HEIGHT.min(terminal.height),
    };
    if row != header.y + 1 || !contains(header, column, row) {
        return None;
    }

    let labels = WorkspaceSection::ALL
        .iter()
        .map(|section| (*section, format!(" {} ", section.label())))
        .collect::<Vec<_>>();
    let total_width = labels
        .iter()
        .map(|(_, label)| label.len() as u16)
        .sum::<u16>();
    let mut cursor = header.x + header.width.saturating_sub(total_width) / 2;
    for (section, label) in labels {
        let width = label.len() as u16;
        if column >= cursor && column < cursor + width {
            return Some(MouseAction::SwitchSection(section));
        }
        cursor += width;
    }
    None
}

fn sidebar_action(app: &App, sidebar: Rect, row: u16) -> Option<MouseAction> {
    if row <= sidebar.y || row >= sidebar.y + sidebar.height.saturating_sub(1) {
        return None;
    }
    let index = (row - sidebar.y - 1) as usize;
    let len = WorkspaceSection::ALL.len()
        + 1
        + app.workspace.browser_tabs.len()
        + app.workspace.conversations.len();
    (index < len).then_some(MouseAction::ActivateSidebarIndex(index))
}

fn workspace_action(app: &App, workspace: Rect, column: u16, row: u16) -> Option<MouseAction> {
    match app.workspace.active_section {
        WorkspaceSection::Browser => browser_action(app, workspace, column, row),
        WorkspaceSection::Messages => message_action(app, workspace, column, row),
        WorkspaceSection::Directory => directory_action(app, workspace, column, row),
        WorkspaceSection::Identities => None,
        WorkspaceSection::Interfaces => interfaces_action(app, workspace, column, row),
        WorkspaceSection::Monitoring | WorkspaceSection::NetworkDoctor => None,
        WorkspaceSection::Settings => settings_action(workspace, column, row),
        WorkspaceSection::Diagnostics => diagnostics_action(app, workspace, column, row),
        WorkspaceSection::Logs => logs_action(workspace, column, row),
        WorkspaceSection::Plugins => plugins_action(app, workspace, column, row),
        WorkspaceSection::Help => None,
    }
}

fn row_index(row: u16, workspace: Rect, len: usize) -> Option<usize> {
    if row <= workspace.y || row >= workspace.y + workspace.height.saturating_sub(1) {
        return None;
    }
    let index = row.saturating_sub(workspace.y + 1) as usize;
    (index < len).then_some(index)
}

fn directory_action(app: &App, workspace: Rect, column: u16, row: u16) -> Option<MouseAction> {
    let index = row_index(row, workspace, app.directory_state.entries.len())?;
    let content_col = column.saturating_sub(workspace.x + 1);
    match content_col {
        2..=7 => Some(MouseAction::ActivateDirectoryIndex(index)),
        9..=14 => Some(MouseAction::SaveDirectoryIndex(index)),
        16..=22 => Some(MouseAction::ToggleDirectoryTrustIndex(index)),
        _ => Some(MouseAction::SelectDirectoryIndex(index)),
    }
}

fn diagnostics_action(app: &App, workspace: Rect, column: u16, row: u16) -> Option<MouseAction> {
    let content_col = column.saturating_sub(workspace.x + 1);
    let content_row = row.saturating_sub(workspace.y + 1);
    if content_col <= 4 && content_row >= 18 {
        let index = (content_row - 18) as usize;
        if index < app.lxmf_peer_candidates().len().min(8) {
            return Some(MouseAction::SelectLxmfPeerCandidate(index));
        }
    }
    match content_row {
        0 => match content_col {
            1..=9 => Some(MouseAction::PreviewDiagnosticsBundle),
            29..=36 => Some(MouseAction::ExportDiagnosticsBundle),
            52..=58 => Some(MouseAction::ClearDiagnosticsPreview),
            _ => None,
        },
        1 => match content_col {
            1..=15 => Some(MouseAction::ProbeActiveBrowserPageFetchDryRun),
            41..=52 => Some(MouseAction::ProbeActiveBrowserPageFetchLive),
            _ => None,
        },
        2 => match content_col {
            1..=21 => Some(MouseAction::PreviewLiveInteropReport),
            25..=45 => Some(MouseAction::ExportLiveInteropReport),
            _ => None,
        },
        3 => match content_col {
            1..=20 => Some(MouseAction::RunNativeNetworkSmokeTestDryRun),
            24..=44 => Some(MouseAction::RunNativeNetworkSmokeTestLive),
            48..=66 => Some(MouseAction::RunNativeNetworkLiveFetchValidation),
            _ => None,
        },
        4 => match content_col {
            1..=25 => Some(MouseAction::RunPathDiscoveryDiagnostics),
            29..=45 => Some(MouseAction::RunNativeLxmfSmokeSend),
            49..=68 => Some(MouseAction::RunNativeLxmfLiveInterop),
            _ => None,
        },
        5 => match content_col {
            1..=27 => Some(MouseAction::PreloadKnownDestinations),
            _ => None,
        },
        _ => None,
    }
}

fn logs_action(workspace: Rect, column: u16, row: u16) -> Option<MouseAction> {
    let content_col = column.saturating_sub(workspace.x + 1);
    match row.saturating_sub(workspace.y + 1) {
        0 => match content_col {
            33..=42 => Some(MouseAction::CycleLogSeverityFilter),
            46..=53 => Some(MouseAction::CycleLogSourceFilter),
            _ => None,
        },
        _ => None,
    }
}

fn interfaces_action(app: &App, workspace: Rect, column: u16, row: u16) -> Option<MouseAction> {
    let list_area = Rect {
        x: workspace.x,
        y: workspace.y,
        width: workspace.width,
        height: workspace.height / 2,
    };
    let content_col = column.saturating_sub(workspace.x + 1);
    if let Some(index) = row_index(row, list_area, app.interfaces_state.profiles.len()) {
        return match content_col {
            1..=8 => Some(MouseAction::ToggleInterfaceEnabledIndex(index)),
            10..=17 => Some(MouseAction::DeleteInterfaceIndex(index)),
            _ => Some(MouseAction::SelectInterfaceIndex(index)),
        };
    }

    let detail_area = Rect {
        x: workspace.x,
        y: workspace.y + workspace.height / 2,
        width: workspace.width,
        height: workspace.height.saturating_sub(workspace.height / 2),
    };
    if !contains(detail_area, column, row) {
        return None;
    }
    match row.saturating_sub(detail_area.y + 1) {
        3 => match content_col {
            8..=16 => Some(MouseAction::CreateInterfaceTcpClient),
            68..=72 => Some(MouseAction::CreateInterfaceI2p),
            80..=86 => Some(MouseAction::CreateInterfaceRNode),
            _ => None,
        },
        4 => match content_col {
            10..=17 => Some(MouseAction::EditInterfaceName),
            29..=36 => app
                .interfaces_state
                .selected
                .map(MouseAction::ToggleInterfaceEnabledIndex),
            48..=55 => app
                .interfaces_state
                .selected
                .map(MouseAction::DeleteInterfaceIndex),
            _ => None,
        },
        5 => match content_col {
            5..=10 => Some(MouseAction::EditInterfaceTcpHost),
            19..=24 => Some(MouseAction::EditInterfaceTcpPort),
            _ => None,
        },
        6 => match content_col {
            5..=13 => Some(MouseAction::ToggleInterfaceConnectable),
            24..=30 => Some(MouseAction::EditInterfaceI2pPeers),
            _ => None,
        },
        10 => match content_col {
            7..=14 => Some(MouseAction::EditInterfaceRNodeDevicePort),
            23..=28 => Some(MouseAction::EditInterfaceRNodeFrequency),
            37..=40 => Some(MouseAction::EditInterfaceRNodeBandwidth),
            49..=52 => Some(MouseAction::EditInterfaceRNodeTxPower),
            61..=64 => Some(MouseAction::EditInterfaceRNodeSpreadingFactor),
            73..=76 => Some(MouseAction::EditInterfaceRNodeCodingRate),
            _ => None,
        },
        12 => match content_col {
            9..=17 => Some(MouseAction::PreviewManagedReticulumConfig),
            31..=38 => Some(MouseAction::ExportManagedReticulumConfig),
            _ => None,
        },
        _ => None,
    }
}

fn plugins_action(app: &App, workspace: Rect, column: u16, row: u16) -> Option<MouseAction> {
    let list_area = Rect {
        x: workspace.x,
        y: workspace.y,
        width: workspace.width,
        height: workspace.height.saturating_sub(PLUGIN_DETAIL_HEIGHT),
    };
    let index = row_index(row, list_area, app.plugins_state.manifests.len())?;
    let content_col = column.saturating_sub(workspace.x + 1);
    match content_col {
        2..=9 => Some(MouseAction::ActivatePluginIndex(index)),
        _ => Some(MouseAction::SelectPluginIndex(index)),
    }
}

fn settings_action(workspace: Rect, column: u16, row: u16) -> Option<MouseAction> {
    if column <= workspace.x || column >= workspace.x + workspace.width.saturating_sub(1) {
        return None;
    }
    let content_col = column.saturating_sub(workspace.x + 1);
    let row_offset = row.saturating_sub(workspace.y + 1);
    if row_offset == 1 {
        return match content_col {
            0..=28 => Some(MouseAction::CreateManagedIdentity),
            32..=54 => Some(MouseAction::RunNativeQuickstart),
            _ => None,
        };
    }
    let action_row = row_offset.checked_sub(4)?;
    match action_row {
        0 => Some(MouseAction::EditSettingsThemeName),
        1 => Some(MouseAction::EditSettingsDefaultStartPage),
        2 => None,
        3 => None,
        4 => None,
        5 => Some(MouseAction::SelectRuntimeBackend(
            RuntimeBackendSetting::Auto,
        )),
        6 => Some(MouseAction::SelectRuntimeBackend(
            RuntimeBackendSetting::Mock,
        )),
        7 => Some(MouseAction::SelectRuntimeBackend(
            RuntimeBackendSetting::Reticulum,
        )),
        8 => Some(MouseAction::SelectRuntimeBackend(
            RuntimeBackendSetting::Bridge,
        )),
        9 => Some(MouseAction::CycleReticulumInstanceMode),
        10 => Some(MouseAction::RestartToApplySettings),
        11 => Some(MouseAction::ToggleAnnounceOnStart),
        12 => Some(MouseAction::TogglePeriodicLxmfSync),
        13 => Some(MouseAction::EditSettingsLxmfSyncInterval),
        14 => Some(MouseAction::EditSettingsLxmfSyncLimit),
        15 => Some(MouseAction::EditPreferredPropagation),
        16 => Some(MouseAction::TogglePluginRemoteContent),
        17 => Some(MouseAction::CreateManagedIdentity),
        18 => Some(MouseAction::EditSettingsIdentityPath),
        19 => Some(MouseAction::EditSettingsReticulumConfigPath),
        20 => Some(MouseAction::EditSettingsBrowserFormMaxAge),
        21 => Some(MouseAction::EditSettingsLogMaxBytes),
        22 => Some(MouseAction::EditSettingsLogRetainFiles),
        23 => Some(MouseAction::EditSettingsLogLoadRecentEntries),
        24 => Some(MouseAction::ToggleBrowserFormState),
        25 => Some(MouseAction::CycleBrowserFormSensitivePolicy),
        26 => Some(MouseAction::ForgetCurrentPageFormState),
        27 => Some(MouseAction::ForgetCurrentNodeFormState),
        28 => Some(MouseAction::ForgetAllBrowserFormState),
        _ => None,
    }
}

fn browser_action(app: &App, workspace: Rect, column: u16, row: u16) -> Option<MouseAction> {
    let tab_area = Rect {
        x: workspace.x,
        y: workspace.y,
        width: workspace.width,
        height: BROWSER_TAB_HEIGHT.min(workspace.height),
    };
    if contains(tab_area, column, row) {
        return hit_tab_line(
            column,
            workspace.x,
            app.workspace
                .browser_tabs
                .iter()
                .map(|tab| tab.title.as_str()),
        )
        .map(MouseAction::SelectBrowserTab);
    }

    let command_area = Rect {
        x: workspace.x,
        y: workspace.y + BROWSER_TAB_HEIGHT,
        width: workspace.width,
        height: BROWSER_COMMAND_HEIGHT.min(workspace.height.saturating_sub(BROWSER_TAB_HEIGHT)),
    };
    if contains(command_area, column, row) {
        return Some(MouseAction::FocusBrowserAddress);
    }
    let content = Rect {
        x: workspace.x,
        y: workspace.y + BROWSER_TAB_HEIGHT + BROWSER_COMMAND_HEIGHT,
        width: workspace.width,
        height: workspace
            .height
            .saturating_sub(BROWSER_TAB_HEIGHT + BROWSER_COMMAND_HEIGHT),
    };
    if contains(content, column, row)
        && row > content.y
        && row < content.y + content.height.saturating_sub(1)
        && column > content.x
        && column < content.x + content.width.saturating_sub(1)
    {
        return Some(MouseAction::ActivateBrowserCell {
            row: row.saturating_sub(content.y + 1),
            col: column.saturating_sub(content.x + 1),
            width: content.width.saturating_sub(2),
        });
    }
    None
}

fn message_action(app: &App, workspace: Rect, column: u16, row: u16) -> Option<MouseAction> {
    let tab_area = Rect {
        x: workspace.x,
        y: workspace.y,
        width: workspace.width,
        height: MESSAGE_TAB_HEIGHT.min(workspace.height),
    };
    if contains(tab_area, column, row) {
        return hit_tab_line(
            column,
            workspace.x,
            app.workspace
                .conversations
                .iter()
                .map(|conversation| conversation.peer_label.as_str()),
        )
        .map(MouseAction::SelectConversationTab);
    }

    let composer_y = workspace.y + workspace.height.saturating_sub(MESSAGE_COMPOSER_HEIGHT);
    let composer = Rect {
        x: workspace.x,
        y: composer_y,
        width: workspace.width,
        height: MESSAGE_COMPOSER_HEIGHT.min(workspace.height),
    };
    if !contains(composer, column, row) {
        return None;
    }
    match row.saturating_sub(composer.y) {
        1 => Some(MouseAction::FocusMessageTitle),
        2 => Some(MouseAction::FocusMessageBody),
        3 => {
            let relative = column.saturating_sub(composer.x);
            if relative < 20 {
                Some(MouseAction::ToggleMessageDeliveryMode)
            } else if relative < 40 {
                Some(MouseAction::ToggleMessageTicket)
            } else {
                Some(MouseAction::SendMessageDraft)
            }
        }
        _ => None,
    }
}

fn hit_tab_line<'a>(
    column: u16,
    start_x: u16,
    labels: impl Iterator<Item = &'a str>,
) -> Option<usize> {
    let mut cursor = start_x;
    for (index, label) in labels.enumerate() {
        cursor += 1;
        let width = label.len() as u16 + 2;
        if column >= cursor && column < cursor + width {
            return Some(index);
        }
        cursor += width;
    }
    None
}

fn body_rect(terminal: Rect) -> Option<Rect> {
    let used = HEADER_HEIGHT.saturating_add(FOOTER_HEIGHT);
    (terminal.height > used).then_some(Rect {
        x: terminal.x,
        y: terminal.y + HEADER_HEIGHT,
        width: terminal.width,
        height: terminal.height - used,
    })
}

fn contains(area: Rect, column: u16, row: u16) -> bool {
    column >= area.x && column < area.x + area.width && row >= area.y && row < area.y + area.height
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::App;
    use crate::config::{AppConfig, AppPaths};
    use crate::directory::DirectoryKind;
    use crate::storage::settings::AppSettings;

    fn app(name: &str) -> App {
        let root = std::env::temp_dir().join(format!(
            "omenbrowser-rs-mouse-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        App::new(AppConfig {
            paths: AppPaths::from_root(root),
            settings: AppSettings::default(),
        })
    }

    #[test]
    fn maps_sidebar_rows_to_sidebar_indices() {
        let app = app("sidebar");
        let terminal = Rect::new(0, 0, 100, 30);

        assert_eq!(
            action_for_click(&app, terminal, 2, HEADER_HEIGHT + 1),
            Some(MouseAction::ActivateSidebarIndex(0))
        );
        assert_eq!(
            action_for_click(&app, terminal, 2, HEADER_HEIGHT + 4),
            Some(MouseAction::ActivateSidebarIndex(3))
        );
    }

    #[test]
    fn maps_browser_workspace_clicks() {
        let mut app = app("browser");
        app.new_browser_tab();
        let terminal = Rect::new(0, 0, 120, 40);
        let workspace_x = SIDEBAR_WIDTH;
        let workspace_y = HEADER_HEIGHT;

        assert_eq!(
            action_for_click(&app, terminal, workspace_x + 2, workspace_y),
            Some(MouseAction::SelectBrowserTab(0))
        );
        assert_eq!(
            action_for_click(&app, terminal, workspace_x + 2, workspace_y + 2),
            Some(MouseAction::FocusBrowserAddress)
        );
        assert_eq!(
            action_for_click(&app, terminal, workspace_x + 4, workspace_y + 6),
            Some(MouseAction::ActivateBrowserCell {
                row: 0,
                col: 3,
                width: 90
            })
        );
    }

    #[test]
    fn maps_message_workspace_clicks() {
        let mut app = app("messages");
        app.new_conversation();
        app.switch_section(WorkspaceSection::Messages);
        let terminal = Rect::new(0, 0, 120, 40);
        let workspace_x = SIDEBAR_WIDTH;
        let workspace_y = HEADER_HEIGHT;
        let composer_y =
            HEADER_HEIGHT + (40 - HEADER_HEIGHT - FOOTER_HEIGHT) - MESSAGE_COMPOSER_HEIGHT;

        assert_eq!(
            action_for_click(&app, terminal, workspace_x + 2, workspace_y),
            Some(MouseAction::SelectConversationTab(0))
        );
        assert_eq!(
            action_for_click(&app, terminal, workspace_x + 4, composer_y + 1),
            Some(MouseAction::FocusMessageTitle)
        );
        assert_eq!(
            action_for_click(&app, terminal, workspace_x + 4, composer_y + 2),
            Some(MouseAction::FocusMessageBody)
        );
        assert_eq!(
            action_for_click(&app, terminal, workspace_x + 45, composer_y + 3),
            Some(MouseAction::SendMessageDraft)
        );
    }

    #[test]
    fn maps_directory_workspace_rows_to_entry_activation() {
        let mut app = app("directory");
        app.directory_service
            .ingest_announce("node.hash", "Node", DirectoryKind::Node, None, None)
            .expect("announce");
        app.refresh_panels_from_services();
        app.switch_section(WorkspaceSection::Directory);
        let terminal = Rect::new(0, 0, 120, 40);
        let workspace_x = SIDEBAR_WIDTH;
        let workspace_y = HEADER_HEIGHT;

        assert_eq!(
            action_for_click(&app, terminal, workspace_x + 3, workspace_y + 1),
            Some(MouseAction::ActivateDirectoryIndex(0))
        );
        assert_eq!(
            action_for_click(&app, terminal, workspace_x + 10, workspace_y + 1),
            Some(MouseAction::SaveDirectoryIndex(0))
        );
        assert_eq!(
            action_for_click(&app, terminal, workspace_x + 17, workspace_y + 1),
            Some(MouseAction::ToggleDirectoryTrustIndex(0))
        );
        assert_eq!(
            action_for_click(&app, terminal, workspace_x + 30, workspace_y + 1),
            Some(MouseAction::SelectDirectoryIndex(0))
        );
    }

    #[test]
    fn maps_interface_workspace_rows_to_selection() {
        let mut app = app("interfaces");
        app.switch_section(WorkspaceSection::Interfaces);
        let terminal = Rect::new(0, 0, 120, 40);
        let workspace_x = SIDEBAR_WIDTH;
        let workspace_y = HEADER_HEIGHT;

        assert_eq!(
            action_for_click(&app, terminal, workspace_x + 2, workspace_y + 1),
            Some(MouseAction::ToggleInterfaceEnabledIndex(0))
        );
        assert_eq!(
            action_for_click(&app, terminal, workspace_x + 13, workspace_y + 1),
            Some(MouseAction::DeleteInterfaceIndex(0))
        );
        assert_eq!(
            action_for_click(&app, terminal, workspace_x + 24, workspace_y + 1),
            Some(MouseAction::SelectInterfaceIndex(0))
        );
        assert_eq!(
            action_for_click(&app, terminal, workspace_x + 2, workspace_y + 20),
            None
        );
        assert_eq!(
            action_for_click(&app, terminal, workspace_x + 11, workspace_y + 21),
            Some(MouseAction::CreateInterfaceTcpClient)
        );
        assert_eq!(
            action_for_click(&app, terminal, workspace_x + 33, workspace_y + 21),
            None
        );
        assert_eq!(
            action_for_click(&app, terminal, workspace_x + 70, workspace_y + 21),
            Some(MouseAction::CreateInterfaceI2p)
        );
        assert_eq!(
            action_for_click(&app, terminal, workspace_x + 82, workspace_y + 21),
            Some(MouseAction::CreateInterfaceRNode)
        );
        assert_eq!(
            action_for_click(&app, terminal, workspace_x + 12, workspace_y + 22),
            Some(MouseAction::EditInterfaceName)
        );
        assert_eq!(
            action_for_click(&app, terminal, workspace_x + 31, workspace_y + 22),
            Some(MouseAction::ToggleInterfaceEnabledIndex(0))
        );
        assert_eq!(
            action_for_click(&app, terminal, workspace_x + 50, workspace_y + 22),
            Some(MouseAction::DeleteInterfaceIndex(0))
        );
        assert_eq!(
            action_for_click(&app, terminal, workspace_x + 7, workspace_y + 23),
            Some(MouseAction::EditInterfaceTcpHost)
        );
        assert_eq!(
            action_for_click(&app, terminal, workspace_x + 21, workspace_y + 23),
            Some(MouseAction::EditInterfaceTcpPort)
        );

        app.create_tcp_server_interface_profile();
        assert_eq!(
            action_for_click(&app, terminal, workspace_x + 21, workspace_y + 23),
            Some(MouseAction::EditInterfaceTcpPort)
        );

        app.create_i2p_interface_profile();
        assert_eq!(
            action_for_click(&app, terminal, workspace_x + 7, workspace_y + 24),
            Some(MouseAction::ToggleInterfaceConnectable)
        );
        assert_eq!(
            action_for_click(&app, terminal, workspace_x + 26, workspace_y + 24),
            Some(MouseAction::EditInterfaceI2pPeers)
        );

        app.create_rnode_interface_profile();
        assert_eq!(
            action_for_click(&app, terminal, workspace_x + 8, workspace_y + 28),
            Some(MouseAction::EditInterfaceRNodeDevicePort)
        );
        assert_eq!(
            action_for_click(&app, terminal, workspace_x + 25, workspace_y + 28),
            Some(MouseAction::EditInterfaceRNodeFrequency)
        );
        assert_eq!(
            action_for_click(&app, terminal, workspace_x + 38, workspace_y + 28),
            Some(MouseAction::EditInterfaceRNodeBandwidth)
        );
        assert_eq!(
            action_for_click(&app, terminal, workspace_x + 50, workspace_y + 28),
            Some(MouseAction::EditInterfaceRNodeTxPower)
        );
        assert_eq!(
            action_for_click(&app, terminal, workspace_x + 62, workspace_y + 28),
            Some(MouseAction::EditInterfaceRNodeSpreadingFactor)
        );
        assert_eq!(
            action_for_click(&app, terminal, workspace_x + 74, workspace_y + 28),
            Some(MouseAction::EditInterfaceRNodeCodingRate)
        );
        assert_eq!(
            action_for_click(&app, terminal, workspace_x + 11, workspace_y + 30),
            Some(MouseAction::PreviewManagedReticulumConfig)
        );
        assert_eq!(
            action_for_click(&app, terminal, workspace_x + 33, workspace_y + 30),
            Some(MouseAction::ExportManagedReticulumConfig)
        );
    }

    #[test]
    fn maps_plugin_workspace_rows_to_plugin_activation() {
        let mut app = app("plugins");
        app.switch_section(WorkspaceSection::Plugins);
        let terminal = Rect::new(0, 0, 120, 40);
        let workspace_x = SIDEBAR_WIDTH;
        let workspace_y = HEADER_HEIGHT;

        assert_eq!(
            action_for_click(&app, terminal, workspace_x + 3, workspace_y + 1),
            Some(MouseAction::ActivatePluginIndex(0))
        );
        assert_eq!(
            action_for_click(&app, terminal, workspace_x + 20, workspace_y + 1),
            Some(MouseAction::SelectPluginIndex(0))
        );
    }

    #[test]
    fn maps_diagnostics_workspace_actions() {
        let mut app = app("diagnostics");
        app.switch_section(WorkspaceSection::Diagnostics);
        let terminal = Rect::new(0, 0, 120, 40);
        let workspace_x = SIDEBAR_WIDTH;
        let workspace_y = HEADER_HEIGHT;

        assert_eq!(
            action_for_click(&app, terminal, workspace_x + 3, workspace_y + 1),
            Some(MouseAction::PreviewDiagnosticsBundle)
        );
        assert_eq!(
            action_for_click(&app, terminal, workspace_x + 31, workspace_y + 1),
            Some(MouseAction::ExportDiagnosticsBundle)
        );
        assert_eq!(
            action_for_click(&app, terminal, workspace_x + 54, workspace_y + 1),
            Some(MouseAction::ClearDiagnosticsPreview)
        );
        assert_eq!(
            action_for_click(&app, terminal, workspace_x + 3, workspace_y + 2),
            Some(MouseAction::ProbeActiveBrowserPageFetchDryRun)
        );
        assert_eq!(
            action_for_click(&app, terminal, workspace_x + 43, workspace_y + 2),
            Some(MouseAction::ProbeActiveBrowserPageFetchLive)
        );
        assert_eq!(
            action_for_click(&app, terminal, workspace_x + 3, workspace_y + 3),
            Some(MouseAction::PreviewLiveInteropReport)
        );
        assert_eq!(
            action_for_click(&app, terminal, workspace_x + 28, workspace_y + 3),
            Some(MouseAction::ExportLiveInteropReport)
        );
        assert_eq!(
            action_for_click(&app, terminal, workspace_x + 3, workspace_y + 4),
            Some(MouseAction::RunNativeNetworkSmokeTestDryRun)
        );
        assert_eq!(
            action_for_click(&app, terminal, workspace_x + 28, workspace_y + 4),
            Some(MouseAction::RunNativeNetworkSmokeTestLive)
        );
        assert_eq!(
            action_for_click(&app, terminal, workspace_x + 52, workspace_y + 4),
            Some(MouseAction::RunNativeNetworkLiveFetchValidation)
        );
        assert_eq!(
            action_for_click(&app, terminal, workspace_x + 3, workspace_y + 5),
            Some(MouseAction::RunPathDiscoveryDiagnostics)
        );
        assert_eq!(
            action_for_click(&app, terminal, workspace_x + 32, workspace_y + 5),
            Some(MouseAction::RunNativeLxmfSmokeSend)
        );
        assert_eq!(
            action_for_click(&app, terminal, workspace_x + 53, workspace_y + 5),
            Some(MouseAction::RunNativeLxmfLiveInterop)
        );
        assert_eq!(
            action_for_click(&app, terminal, workspace_x + 3, workspace_y + 6),
            Some(MouseAction::PreloadKnownDestinations)
        );
        assert_eq!(
            action_for_click(&app, terminal, workspace_x + 20, workspace_y + 1),
            None
        );
    }

    #[test]
    fn maps_diagnostics_lxmf_peer_candidate_rows() {
        let mut app = app("diagnostics-lxmf-peer-candidates");
        app.directory_service
            .ingest_announce(
                "00112233445566778899aabbccddeeff",
                "Peer A",
                DirectoryKind::Peer,
                None,
                None,
            )
            .expect("peer announce");
        app.refresh_panels_from_services();
        app.switch_section(WorkspaceSection::Diagnostics);
        let terminal = Rect::new(0, 0, 120, 40);
        let workspace_x = SIDEBAR_WIDTH;
        let workspace_y = HEADER_HEIGHT;

        assert_eq!(
            action_for_click(&app, terminal, workspace_x + 3, workspace_y + 19),
            Some(MouseAction::SelectLxmfPeerCandidate(0))
        );
    }

    #[test]
    fn maps_logs_workspace_filters() {
        let mut app = app("logs");
        app.switch_section(WorkspaceSection::Logs);
        let terminal = Rect::new(0, 0, 120, 40);
        let workspace_x = SIDEBAR_WIDTH;
        let workspace_y = HEADER_HEIGHT;

        assert_eq!(
            action_for_click(&app, terminal, workspace_x + 35, workspace_y + 1),
            Some(MouseAction::CycleLogSeverityFilter)
        );
        assert_eq!(
            action_for_click(&app, terminal, workspace_x + 48, workspace_y + 1),
            Some(MouseAction::CycleLogSourceFilter)
        );
        assert_eq!(
            action_for_click(&app, terminal, workspace_x + 20, workspace_y + 1),
            None
        );
    }

    #[test]
    fn maps_settings_workspace_clicks_to_form_state_actions() {
        let mut app = app("settings");
        app.switch_section(WorkspaceSection::Settings);
        let terminal = Rect::new(0, 0, 120, 40);
        let workspace_x = SIDEBAR_WIDTH;
        let workspace_y = HEADER_HEIGHT;
        let action_row = |row: u16| workspace_y + 5 + row;

        assert_eq!(
            action_for_click(&app, terminal, workspace_x + 2, workspace_y + 2),
            Some(MouseAction::CreateManagedIdentity)
        );
        assert_eq!(
            action_for_click(&app, terminal, workspace_x + 34, workspace_y + 2),
            Some(MouseAction::RunNativeQuickstart)
        );
        assert_eq!(
            action_for_click(&app, terminal, workspace_x + 2, action_row(0)),
            Some(MouseAction::EditSettingsThemeName)
        );
        assert_eq!(
            action_for_click(&app, terminal, workspace_x + 2, action_row(1)),
            Some(MouseAction::EditSettingsDefaultStartPage)
        );
        assert_eq!(
            action_for_click(&app, terminal, workspace_x + 2, action_row(2)),
            None
        );
        assert_eq!(
            action_for_click(&app, terminal, workspace_x + 2, action_row(3)),
            None
        );
        assert_eq!(
            action_for_click(&app, terminal, workspace_x + 2, action_row(5)),
            Some(MouseAction::SelectRuntimeBackend(
                RuntimeBackendSetting::Auto
            ))
        );
        assert_eq!(
            action_for_click(&app, terminal, workspace_x + 2, action_row(6)),
            Some(MouseAction::SelectRuntimeBackend(
                RuntimeBackendSetting::Mock
            ))
        );
        assert_eq!(
            action_for_click(&app, terminal, workspace_x + 2, action_row(7)),
            Some(MouseAction::SelectRuntimeBackend(
                RuntimeBackendSetting::Reticulum
            ))
        );
        assert_eq!(
            action_for_click(&app, terminal, workspace_x + 2, action_row(8)),
            Some(MouseAction::SelectRuntimeBackend(
                RuntimeBackendSetting::Bridge
            ))
        );
        assert_eq!(
            action_for_click(&app, terminal, workspace_x + 2, action_row(9)),
            Some(MouseAction::CycleReticulumInstanceMode)
        );
        assert_eq!(
            action_for_click(&app, terminal, workspace_x + 2, action_row(10)),
            Some(MouseAction::RestartToApplySettings)
        );
        assert_eq!(
            action_for_click(&app, terminal, workspace_x + 2, action_row(11)),
            Some(MouseAction::ToggleAnnounceOnStart)
        );
        assert_eq!(
            action_for_click(&app, terminal, workspace_x + 2, action_row(12)),
            Some(MouseAction::TogglePeriodicLxmfSync)
        );
        assert_eq!(
            action_for_click(&app, terminal, workspace_x + 2, action_row(13)),
            Some(MouseAction::EditSettingsLxmfSyncInterval)
        );
        assert_eq!(
            action_for_click(&app, terminal, workspace_x + 2, action_row(14)),
            Some(MouseAction::EditSettingsLxmfSyncLimit)
        );
        assert_eq!(
            action_for_click(&app, terminal, workspace_x + 2, action_row(15)),
            Some(MouseAction::EditPreferredPropagation)
        );
        assert_eq!(
            action_for_click(&app, terminal, workspace_x + 2, action_row(16)),
            Some(MouseAction::TogglePluginRemoteContent)
        );
        assert_eq!(
            action_for_click(&app, terminal, workspace_x + 2, action_row(17)),
            Some(MouseAction::CreateManagedIdentity)
        );
        assert_eq!(
            action_for_click(&app, terminal, workspace_x + 2, action_row(18)),
            Some(MouseAction::EditSettingsIdentityPath)
        );
        assert_eq!(
            action_for_click(&app, terminal, workspace_x + 2, action_row(19)),
            Some(MouseAction::EditSettingsReticulumConfigPath)
        );
        assert_eq!(
            action_for_click(&app, terminal, workspace_x + 2, action_row(20)),
            Some(MouseAction::EditSettingsBrowserFormMaxAge)
        );
        assert_eq!(
            action_for_click(&app, terminal, workspace_x + 2, action_row(21)),
            Some(MouseAction::EditSettingsLogMaxBytes)
        );
        assert_eq!(
            action_for_click(&app, terminal, workspace_x + 2, action_row(22)),
            Some(MouseAction::EditSettingsLogRetainFiles)
        );
        assert_eq!(
            action_for_click(&app, terminal, workspace_x + 2, action_row(23)),
            Some(MouseAction::EditSettingsLogLoadRecentEntries)
        );
        assert_eq!(
            action_for_click(&app, terminal, workspace_x + 2, action_row(24)),
            Some(MouseAction::ToggleBrowserFormState)
        );
        assert_eq!(
            action_for_click(&app, terminal, workspace_x + 2, action_row(25)),
            Some(MouseAction::CycleBrowserFormSensitivePolicy)
        );
        assert_eq!(
            action_for_click(&app, terminal, workspace_x + 2, action_row(26)),
            Some(MouseAction::ForgetCurrentPageFormState)
        );
        assert_eq!(
            action_for_click(&app, terminal, workspace_x + 2, action_row(27)),
            Some(MouseAction::ForgetCurrentNodeFormState)
        );
        assert_eq!(
            action_for_click(&app, terminal, workspace_x + 2, action_row(28)),
            Some(MouseAction::ForgetAllBrowserFormState)
        );
    }

    #[test]
    fn maps_header_nav_click_to_section() {
        let app = app("header");
        let terminal = Rect::new(0, 0, 120, 40);

        assert_eq!(
            action_for_click(&app, terminal, 25, 1),
            Some(MouseAction::SwitchSection(WorkspaceSection::Browser))
        );
    }
}
