pub mod mouse;
pub mod status;
pub mod tabs;
pub mod workspace;

use std::io;
use std::time::Duration;

use crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyModifiers,
    MouseButton, MouseEvent, MouseEventKind,
};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use crate::app::{current_epoch_ms, App};
use crate::error::AppResult;
use crate::ui::mouse::MouseAction;

struct TerminalGuard;

impl TerminalGuard {
    fn enter() -> AppResult<Self> {
        enable_raw_mode()?;
        execute!(io::stdout(), EnterAlternateScreen, EnableMouseCapture)?;
        Ok(Self)
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), DisableMouseCapture, LeaveAlternateScreen);
    }
}

pub async fn run(mut app: App) -> AppResult<()> {
    let _guard = TerminalGuard::enter()?;
    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend)?;
    app.start_configured_runtime_nonblocking();

    while !app.should_quit() {
        let now = current_epoch_ms();
        app.refresh_due_browser_partials(now);
        app.flush_due_ui_preferences(now);
        app.drain_internal_events();
        app.drain_browser_task_results();
        app.drain_message_task_results();
        app.drain_diagnostics_task_results();
        let size = terminal.size()?;
        let (browser_width, browser_height) = mouse::browser_content_inner_size(size);
        app.set_browser_viewport(browser_width as usize, browser_height as usize);
        terminal.draw(|frame| workspace::render(frame, &app))?;
        if event::poll(Duration::from_millis(200))? {
            match event::read()? {
                Event::Key(key) => {
                    handle_key(&mut app, key).await;
                    app.drain_browser_task_results();
                    app.drain_message_task_results();
                    app.drain_diagnostics_task_results();
                }
                Event::Mouse(mouse) => {
                    handle_mouse(&mut app, size, mouse).await;
                    app.drain_browser_task_results();
                    app.drain_message_task_results();
                    app.drain_diagnostics_task_results();
                }
                _ => {}
            }
        }
    }
    app.flush_pending_ui_preferences();

    Ok(())
}

async fn handle_mouse(app: &mut App, terminal: ratatui::layout::Rect, event: MouseEvent) {
    if event.kind != MouseEventKind::Down(MouseButton::Left) {
        return;
    }
    let Some(action) = mouse::action_for_click(app, terminal, event.column, event.row) else {
        return;
    };
    apply_mouse_action(app, action).await;
}

async fn apply_mouse_action(app: &mut App, action: MouseAction) {
    match action {
        MouseAction::SwitchSection(section) => app.switch_section(section),
        MouseAction::ActivateSidebarIndex(index) => {
            app.workspace.sidebar_index = index;
            app.activate_sidebar_selection();
        }
        MouseAction::SelectBrowserTab(index) => app.select_browser_tab(index),
        MouseAction::SelectConversationTab(index) => app.select_conversation_tab(index),
        MouseAction::FocusBrowserAddress => app.focus_address_bar(),
        MouseAction::ActivateBrowserCell { row, col, width } => {
            app.activate_browser_cell(row, col, width as usize);
        }
        MouseAction::SelectDirectoryIndex(index) => {
            app.select_directory_entry(index);
        }
        MouseAction::ActivateDirectoryIndex(index) => {
            app.select_directory_entry(index);
            app.open_selected_directory_entry();
        }
        MouseAction::SaveDirectoryIndex(index) => {
            app.select_directory_entry(index);
            app.save_selected_directory_entry();
        }
        MouseAction::ToggleDirectoryTrustIndex(index) => {
            app.select_directory_entry(index);
            app.toggle_selected_directory_trust();
        }
        MouseAction::SelectInterfaceIndex(index) => {
            app.select_interface_profile(index);
        }
        MouseAction::ToggleInterfaceEnabledIndex(index) => {
            app.select_interface_profile(index);
            app.toggle_selected_interface_enabled();
        }
        MouseAction::CreateInterfaceTcpClient => {
            app.create_tcp_client_interface_profile();
        }
        MouseAction::CreateInterfaceI2p => {
            app.create_i2p_interface_profile();
        }
        MouseAction::CreateInterfaceRNode => {
            app.create_rnode_interface_profile();
        }
        MouseAction::DeleteInterfaceIndex(index) => {
            app.select_interface_profile(index);
            app.begin_selected_interface_delete_flow();
        }
        MouseAction::EditInterfaceName => {
            app.edit_selected_interface_name();
        }
        MouseAction::EditInterfaceTcpHost => {
            app.edit_selected_interface_tcp_host();
        }
        MouseAction::EditInterfaceTcpPort => {
            app.edit_selected_interface_tcp_port();
        }
        MouseAction::EditInterfaceI2pPeers => {
            app.edit_selected_interface_i2p_peers();
        }
        MouseAction::ToggleInterfaceConnectable => {
            app.toggle_selected_interface_connectable();
        }
        MouseAction::EditInterfaceRNodeDevicePort => {
            app.edit_selected_interface_rnode_device_port();
        }
        MouseAction::EditInterfaceRNodeFrequency => {
            app.edit_selected_interface_rnode_frequency();
        }
        MouseAction::EditInterfaceRNodeBandwidth => {
            app.edit_selected_interface_rnode_bandwidth();
        }
        MouseAction::EditInterfaceRNodeTxPower => {
            app.edit_selected_interface_rnode_tx_power();
        }
        MouseAction::EditInterfaceRNodeSpreadingFactor => {
            app.edit_selected_interface_rnode_spreading_factor();
        }
        MouseAction::EditInterfaceRNodeCodingRate => {
            app.edit_selected_interface_rnode_coding_rate();
        }
        MouseAction::PreviewManagedReticulumConfig => {
            app.preview_managed_reticulum_config();
        }
        MouseAction::ExportManagedReticulumConfig => {
            app.export_managed_reticulum_config();
        }
        MouseAction::SelectPluginIndex(index) => {
            app.select_plugin(index);
        }
        MouseAction::ActivatePluginIndex(index) => {
            app.select_plugin(index);
            app.toggle_selected_plugin();
        }
        MouseAction::ToggleBrowserFormState => {
            app.toggle_browser_form_state_enabled();
        }
        MouseAction::CycleBrowserFormSensitivePolicy => {
            app.cycle_browser_form_sensitive_policy();
        }
        MouseAction::SelectRuntimeBackend(backend) => {
            app.set_runtime_backend_setting(backend);
        }
        MouseAction::CycleReticulumInstanceMode => {
            app.cycle_reticulum_instance_mode();
        }
        MouseAction::RestartToApplySettings => {
            app.restart_to_apply_settings();
        }
        MouseAction::ToggleAnnounceOnStart => {
            app.toggle_announce_on_start();
        }
        MouseAction::TogglePeriodicLxmfSync => {
            app.toggle_periodic_lxmf_sync();
        }
        MouseAction::EditPreferredPropagation => {
            app.edit_settings_preferred_propagation();
        }
        MouseAction::TogglePluginRemoteContent => {
            app.toggle_plugin_remote_content();
        }
        MouseAction::CreateManagedIdentity => {
            app.create_settings_managed_identity();
        }
        MouseAction::RunNativeQuickstart => {
            app.run_native_quickstart();
        }
        MouseAction::EditSettingsIdentityPath => {
            app.edit_settings_identity_path();
        }
        MouseAction::EditSettingsReticulumConfigPath => {
            app.edit_settings_reticulum_config_path();
        }
        MouseAction::PreviewDiagnosticsBundle => {
            app.preview_diagnostics_bundle();
        }
        MouseAction::ExportDiagnosticsBundle => {
            app.export_diagnostics_bundle();
        }
        MouseAction::ClearDiagnosticsPreview => {
            app.clear_diagnostics_preview();
        }
        MouseAction::ProbeActiveBrowserPageFetchDryRun => {
            app.probe_active_browser_page_fetch(false);
        }
        MouseAction::ProbeActiveBrowserPageFetchLive => {
            app.probe_active_browser_page_fetch(true);
        }
        MouseAction::PreviewLiveInteropReport => {
            app.preview_live_interop_report();
        }
        MouseAction::ExportLiveInteropReport => {
            app.export_live_interop_report();
        }
        MouseAction::RunNativeNetworkSmokeTestDryRun => {
            app.run_native_network_smoke_test(false);
        }
        MouseAction::RunNativeNetworkSmokeTestLive => {
            app.run_native_network_smoke_test(true);
        }
        MouseAction::RunNativeNetworkLiveFetchValidation => {
            app.run_native_network_live_fetch_validation();
        }
        MouseAction::RunNativeLxmfSmokeSend => {
            app.run_native_lxmf_smoke_send();
        }
        MouseAction::RunNativeLxmfLiveInterop => {
            app.run_native_lxmf_live_interop();
        }
        MouseAction::SelectLxmfPeerCandidate(index) => {
            app.select_lxmf_peer_candidate(index);
        }
        MouseAction::RunPathDiscoveryDiagnostics => {
            app.run_active_browser_path_discovery_diagnostics();
        }
        MouseAction::PreloadKnownDestinations => {
            app.begin_known_destinations_preload_flow();
        }
        MouseAction::CycleLogSeverityFilter => {
            app.cycle_log_severity_filter();
        }
        MouseAction::CycleLogSourceFilter => {
            app.cycle_log_source_filter();
        }
        MouseAction::EditSettingsThemeName => {
            app.edit_settings_theme_name();
        }
        MouseAction::EditSettingsDefaultStartPage => {
            app.edit_settings_default_start_page();
        }
        MouseAction::EditSettingsLxmfSyncInterval => {
            app.edit_settings_lxmf_sync_interval();
        }
        MouseAction::EditSettingsLxmfSyncLimit => {
            app.edit_settings_lxmf_sync_limit();
        }
        MouseAction::EditSettingsBrowserFormMaxAge => {
            app.edit_settings_browser_form_max_age();
        }
        MouseAction::EditSettingsLogMaxBytes => {
            app.edit_settings_log_max_bytes();
        }
        MouseAction::EditSettingsLogRetainFiles => {
            app.edit_settings_log_retain_files();
        }
        MouseAction::EditSettingsLogLoadRecentEntries => {
            app.edit_settings_log_load_recent_entries();
        }
        MouseAction::ForgetCurrentPageFormState => {
            app.forget_current_page_form_state();
        }
        MouseAction::ForgetCurrentNodeFormState => {
            app.forget_current_node_form_state();
        }
        MouseAction::ForgetAllBrowserFormState => {
            app.forget_all_browser_form_state();
        }
        MouseAction::FocusMessageTitle => app.focus_message_title(),
        MouseAction::FocusMessageBody => app.focus_message_body(),
        MouseAction::ToggleMessageDeliveryMode => app.toggle_active_conversation_delivery_mode(),
        MouseAction::ToggleMessageTicket => app.toggle_active_conversation_ticket(),
        MouseAction::SendMessageDraft => app.send_active_conversation_draft(),
    }
}

async fn handle_key(app: &mut App, key: KeyEvent) {
    match (key.modifiers, key.code) {
        (_, KeyCode::Enter)
            if matches!(
                app.input.active.as_ref().map(|active| &active.target),
                Some(crate::input::InputTarget::BrowserField { .. })
            ) =>
        {
            app.submit_active_input();
        }
        (_, KeyCode::Char(ch))
            if key.modifiers.is_empty()
                && matches!(
                    app.input.active.as_ref().map(|active| &active.target),
                    Some(crate::input::InputTarget::BrowserField { .. })
                ) =>
        {
            app.edit_address_char(ch);
        }
        (_, KeyCode::Char(ch)) if key.modifiers.is_empty() && app.input.active.is_some() => {
            app.edit_address_char(ch);
        }
        (_, KeyCode::Char('q')) => app.quit(),
        (_, KeyCode::Char('?')) => app.toggle_help(),
        (_, KeyCode::Char('o'))
            if app.workspace.active_section == workspace::WorkspaceSection::Browser =>
        {
            app.cycle_browser_overlay_mode();
        }
        (_, KeyCode::Char('O'))
            if app.workspace.active_section == workspace::WorkspaceSection::Browser =>
        {
            app.toggle_browser_overlay_expanded();
        }
        (_, KeyCode::Char('N'))
            if app.workspace.active_section == workspace::WorkspaceSection::Browser =>
        {
            app.probe_active_browser_page_fetch_inline();
        }
        (_, KeyCode::Char('D'))
            if app.workspace.active_section == workspace::WorkspaceSection::Browser =>
        {
            app.warm_active_browser_path();
        }
        (_, KeyCode::Char('R'))
            if app.workspace.active_section == workspace::WorkspaceSection::Browser =>
        {
            app.retry_active_browser_after_path_discovery();
        }
        (KeyModifiers::SHIFT, KeyCode::BackTab)
            if app.workspace.active_section == workspace::WorkspaceSection::Browser
                && app.workspace.focus == workspace::FocusArea::Workspace =>
        {
            if !app.focus_previous_browser_item() {
                app.cycle_focus();
            }
        }
        (_, KeyCode::Tab)
            if app.workspace.active_section == workspace::WorkspaceSection::Browser
                && app.workspace.focus == workspace::FocusArea::Workspace =>
        {
            if !app.focus_browser_item(false) {
                app.cycle_focus();
            }
        }
        (_, KeyCode::Tab) => app.cycle_focus(),
        (KeyModifiers::CONTROL, KeyCode::Char('l')) => app.focus_address_bar(),
        (KeyModifiers::CONTROL, KeyCode::Char('y')) => app.focus_message_title(),
        (KeyModifiers::CONTROL, KeyCode::Char('e')) => app.focus_message_body(),
        (KeyModifiers::CONTROL, KeyCode::Char('s')) => app.send_active_conversation_draft(),
        (KeyModifiers::CONTROL, KeyCode::Char('g')) => app.sync_runtime_messages(),
        (KeyModifiers::CONTROL, KeyCode::Char('p')) => {
            app.toggle_active_conversation_delivery_mode()
        }
        (KeyModifiers::CONTROL, KeyCode::Char('u')) => app.toggle_active_conversation_ticket(),
        (KeyModifiers::CONTROL, KeyCode::Char('r')) => app.reload_active_browser(),
        (KeyModifiers::CONTROL, KeyCode::Char('f')) => {
            app.refresh_active_browser_partials();
        }
        (KeyModifiers::CONTROL, KeyCode::Char('d')) => app.download_active_browser_url(),
        (KeyModifiers::CONTROL, KeyCode::Char('t')) => app.new_browser_tab(),
        (KeyModifiers::CONTROL, KeyCode::Char('w')) => app.close_active_browser_tab(),
        (KeyModifiers::CONTROL, KeyCode::Char('n')) => app.new_conversation(),
        (_, KeyCode::Enter) if app.workspace.focus == workspace::FocusArea::Command => {
            app.submit_active_input();
        }
        (_, KeyCode::Enter)
            if app.workspace.active_section == workspace::WorkspaceSection::Browser
                && app.workspace.focus == workspace::FocusArea::Workspace
                && app.active_browser_tab().focused_control.is_some() =>
        {
            app.activate_focused_browser_control();
        }
        (_, KeyCode::Enter) => app.activate_workspace_selection(),
        (_, KeyCode::Char(' '))
            if app.workspace.active_section == workspace::WorkspaceSection::Browser
                && app.workspace.focus == workspace::FocusArea::Workspace
                && app.active_browser_tab().focused_control.is_some() =>
        {
            app.activate_focused_browser_control();
        }
        (_, KeyCode::Esc) => {
            if app.cancel_active_input() {
            } else if app.workspace.focus == workspace::FocusArea::Command {
                app.workspace.focus = workspace::FocusArea::Workspace;
            } else {
                app.cancel_active_browser_load();
            }
        }
        (_, KeyCode::F(1)) => app.switch_section(workspace::WorkspaceSection::Browser),
        (_, KeyCode::F(2)) => app.switch_section(workspace::WorkspaceSection::Messages),
        (_, KeyCode::F(3)) => app.switch_section(workspace::WorkspaceSection::Directory),
        (_, KeyCode::F(4)) => app.switch_section(workspace::WorkspaceSection::Interfaces),
        (_, KeyCode::F(5)) => app.switch_section(workspace::WorkspaceSection::Settings),
        (_, KeyCode::F(6)) => app.switch_section(workspace::WorkspaceSection::Diagnostics),
        (_, KeyCode::F(7)) => app.switch_section(workspace::WorkspaceSection::Logs),
        (_, KeyCode::F(8)) => app.switch_section(workspace::WorkspaceSection::Plugins),
        (_, KeyCode::Char('f'))
            if app.workspace.active_section == workspace::WorkspaceSection::Logs =>
        {
            app.cycle_log_severity_filter();
        }
        (_, KeyCode::Char('s'))
            if app.workspace.active_section == workspace::WorkspaceSection::Logs =>
        {
            app.cycle_log_source_filter();
        }
        (_, KeyCode::Char('P'))
            if app.workspace.active_section == workspace::WorkspaceSection::Diagnostics =>
        {
            app.preview_diagnostics_bundle();
        }
        (_, KeyCode::Char('E'))
            if app.workspace.active_section == workspace::WorkspaceSection::Diagnostics =>
        {
            app.export_diagnostics_bundle();
        }
        (_, KeyCode::Char('C'))
            if app.workspace.active_section == workspace::WorkspaceSection::Diagnostics =>
        {
            app.clear_diagnostics_preview();
        }
        (_, KeyCode::Char('N'))
            if app.workspace.active_section == workspace::WorkspaceSection::Diagnostics =>
        {
            app.probe_active_browser_page_fetch(false);
        }
        (_, KeyCode::Char('X'))
            if app.workspace.active_section == workspace::WorkspaceSection::Diagnostics =>
        {
            app.probe_active_browser_page_fetch(true);
        }
        (_, KeyCode::Char('I'))
            if app.workspace.active_section == workspace::WorkspaceSection::Diagnostics =>
        {
            app.preview_live_interop_report();
        }
        (_, KeyCode::Char('O'))
            if app.workspace.active_section == workspace::WorkspaceSection::Diagnostics =>
        {
            app.export_live_interop_report();
        }
        (_, KeyCode::Char('S'))
            if app.workspace.active_section == workspace::WorkspaceSection::Diagnostics =>
        {
            app.run_native_network_smoke_test(false);
        }
        (_, KeyCode::Char('L'))
            if app.workspace.active_section == workspace::WorkspaceSection::Diagnostics =>
        {
            app.run_native_network_smoke_test(true);
        }
        (_, KeyCode::Char('V'))
            if app.workspace.active_section == workspace::WorkspaceSection::Diagnostics =>
        {
            app.run_native_network_live_fetch_validation();
        }
        (_, KeyCode::Char('M'))
            if app.workspace.active_section == workspace::WorkspaceSection::Diagnostics =>
        {
            app.run_native_lxmf_smoke_send();
        }
        (_, KeyCode::Char('Y'))
            if app.workspace.active_section == workspace::WorkspaceSection::Diagnostics =>
        {
            app.run_native_lxmf_live_interop();
        }
        (_, KeyCode::Char('A'))
            if app.workspace.active_section == workspace::WorkspaceSection::Diagnostics =>
        {
            app.select_lxmf_peer_for_interop();
        }
        (_, KeyCode::Char('D'))
            if app.workspace.active_section == workspace::WorkspaceSection::Diagnostics =>
        {
            app.run_active_browser_path_discovery_diagnostics();
        }
        (_, KeyCode::Char('K'))
            if app.workspace.active_section == workspace::WorkspaceSection::Diagnostics =>
        {
            app.begin_known_destinations_preload_flow();
        }
        (KeyModifiers::ALT, KeyCode::Left) => app.browser_back(),
        (KeyModifiers::ALT, KeyCode::Right) => app.browser_forward(),
        (_, KeyCode::PageDown)
            if app.workspace.active_section == workspace::WorkspaceSection::Diagnostics =>
        {
            app.scroll_diagnostics_preview_page(12, 1);
        }
        (_, KeyCode::PageUp)
            if app.workspace.active_section == workspace::WorkspaceSection::Diagnostics =>
        {
            app.scroll_diagnostics_preview_page(12, -1);
        }
        (KeyModifiers::CONTROL, KeyCode::Char('j'))
            if app.workspace.active_section == workspace::WorkspaceSection::Diagnostics =>
        {
            app.scroll_diagnostics_preview_lines(1);
        }
        (KeyModifiers::CONTROL, KeyCode::Char('k'))
            if app.workspace.active_section == workspace::WorkspaceSection::Diagnostics =>
        {
            app.scroll_diagnostics_preview_lines(-1);
        }
        (_, KeyCode::PageDown) => {
            app.scroll_active_browser_page(app.browser_viewport_height().max(1), 1);
        }
        (_, KeyCode::PageUp) => {
            app.scroll_active_browser_page(app.browser_viewport_height().max(1), -1);
        }
        (KeyModifiers::CONTROL, KeyCode::Char('j')) => {
            app.scroll_active_browser_lines(1);
        }
        (KeyModifiers::CONTROL, KeyCode::Char('k')) => {
            app.scroll_active_browser_lines(-1);
        }
        (_, KeyCode::Right) if !app.input_move_right() => app.next_browser_tab(),
        (_, KeyCode::Left) if !app.input_move_left() => app.previous_browser_tab(),
        (_, KeyCode::Home) => {
            app.input_move_home();
        }
        (_, KeyCode::End) => {
            app.input_move_end();
        }
        (_, KeyCode::Up) => app.select_previous_sidebar_item(),
        (_, KeyCode::Down) => app.select_next_sidebar_item(),
        (_, KeyCode::Char('t'))
            if app.workspace.active_section == workspace::WorkspaceSection::Directory =>
        {
            app.toggle_selected_directory_trust();
        }
        (_, KeyCode::Char('s'))
            if app.workspace.active_section == workspace::WorkspaceSection::Directory =>
        {
            app.save_selected_directory_entry();
        }
        (_, KeyCode::Char('e'))
            if app.workspace.active_section == workspace::WorkspaceSection::Interfaces =>
        {
            app.toggle_selected_interface_enabled();
        }
        (_, KeyCode::Char('a'))
            if app.workspace.active_section == workspace::WorkspaceSection::Interfaces =>
        {
            app.create_tcp_client_interface_profile();
        }
        (_, KeyCode::Char('1'))
            if app.workspace.active_section == workspace::WorkspaceSection::Interfaces =>
        {
            app.create_rmap_gateway_interface_profile();
        }
        (_, KeyCode::Char('2'))
            if app.workspace.active_section == workspace::WorkspaceSection::Interfaces =>
        {
            app.create_wns_gateway_interface_profile();
        }
        (_, KeyCode::Char('G'))
            if matches!(
                app.workspace.active_section,
                workspace::WorkspaceSection::Interfaces
                    | workspace::WorkspaceSection::Settings
                    | workspace::WorkspaceSection::Diagnostics
            ) =>
        {
            app.run_native_quickstart();
        }
        (_, KeyCode::Char('i'))
            if app.workspace.active_section == workspace::WorkspaceSection::Interfaces =>
        {
            app.create_i2p_interface_profile();
        }
        (_, KeyCode::Char('v'))
            if app.workspace.active_section == workspace::WorkspaceSection::Interfaces =>
        {
            app.create_rnode_interface_profile();
        }
        (_, KeyCode::Char('c'))
            if app.workspace.active_section == workspace::WorkspaceSection::Interfaces =>
        {
            app.toggle_selected_interface_connectable();
        }
        (_, KeyCode::Char('x'))
            if app.workspace.active_section == workspace::WorkspaceSection::Interfaces =>
        {
            app.begin_selected_interface_delete_flow();
        }
        (_, KeyCode::Char('n'))
            if app.workspace.active_section == workspace::WorkspaceSection::Interfaces =>
        {
            app.edit_selected_interface_name();
        }
        (_, KeyCode::Char('h'))
            if app.workspace.active_section == workspace::WorkspaceSection::Interfaces =>
        {
            app.edit_selected_interface_tcp_host();
        }
        (_, KeyCode::Char('p'))
            if app.workspace.active_section == workspace::WorkspaceSection::Interfaces =>
        {
            app.edit_selected_interface_tcp_port();
        }
        (_, KeyCode::Char('r'))
            if app.workspace.active_section == workspace::WorkspaceSection::Interfaces =>
        {
            app.edit_selected_interface_i2p_peers();
        }
        (_, KeyCode::Char('d'))
            if app.workspace.active_section == workspace::WorkspaceSection::Interfaces =>
        {
            app.edit_selected_interface_rnode_device_port();
        }
        (_, KeyCode::Char('f'))
            if app.workspace.active_section == workspace::WorkspaceSection::Interfaces =>
        {
            app.edit_selected_interface_rnode_frequency();
        }
        (_, KeyCode::Char('b'))
            if app.workspace.active_section == workspace::WorkspaceSection::Interfaces =>
        {
            app.edit_selected_interface_rnode_bandwidth();
        }
        (_, KeyCode::Char('y'))
            if app.workspace.active_section == workspace::WorkspaceSection::Interfaces =>
        {
            app.edit_selected_interface_rnode_tx_power();
        }
        (_, KeyCode::Char('g'))
            if app.workspace.active_section == workspace::WorkspaceSection::Interfaces =>
        {
            app.edit_selected_interface_rnode_spreading_factor();
        }
        (_, KeyCode::Char('o'))
            if app.workspace.active_section == workspace::WorkspaceSection::Interfaces =>
        {
            app.edit_selected_interface_rnode_coding_rate();
        }
        (_, KeyCode::Char('P'))
            if app.workspace.active_section == workspace::WorkspaceSection::Interfaces =>
        {
            app.preview_managed_reticulum_config();
        }
        (_, KeyCode::Char('E'))
            if app.workspace.active_section == workspace::WorkspaceSection::Interfaces =>
        {
            app.export_managed_reticulum_config();
        }
        (_, KeyCode::Char('e'))
            if app.workspace.active_section == workspace::WorkspaceSection::Plugins =>
        {
            app.toggle_selected_plugin();
        }
        (_, KeyCode::Char('i'))
            if app.workspace.active_section == workspace::WorkspaceSection::Plugins =>
        {
            app.begin_plugin_install_flow();
        }
        (_, KeyCode::Char('x'))
            if app.workspace.active_section == workspace::WorkspaceSection::Plugins =>
        {
            app.begin_selected_plugin_remove_flow();
        }
        (_, KeyCode::Char('r'))
            if app.workspace.active_section == workspace::WorkspaceSection::Plugins =>
        {
            app.refresh_plugins_from_registry();
        }
        (_, KeyCode::Char('l'))
            if app.workspace.active_section == workspace::WorkspaceSection::Plugins =>
        {
            app.show_plugin_logs();
        }
        (_, KeyCode::Char('e'))
            if app.workspace.active_section == workspace::WorkspaceSection::Settings =>
        {
            app.toggle_browser_form_state_enabled();
        }
        (_, KeyCode::Char('p'))
            if app.workspace.active_section == workspace::WorkspaceSection::Settings =>
        {
            app.cycle_browser_form_sensitive_policy();
        }
        (_, KeyCode::Char('1'))
            if app.workspace.active_section == workspace::WorkspaceSection::Settings =>
        {
            app.set_runtime_backend_setting(crate::storage::settings::RuntimeBackendSetting::Auto);
        }
        (_, KeyCode::Char('2'))
            if app.workspace.active_section == workspace::WorkspaceSection::Settings =>
        {
            app.set_runtime_backend_setting(crate::storage::settings::RuntimeBackendSetting::Mock);
        }
        (_, KeyCode::Char('3'))
            if app.workspace.active_section == workspace::WorkspaceSection::Settings =>
        {
            app.set_runtime_backend_setting(
                crate::storage::settings::RuntimeBackendSetting::Reticulum,
            );
        }
        (_, KeyCode::Char('4'))
            if app.workspace.active_section == workspace::WorkspaceSection::Settings =>
        {
            app.set_runtime_backend_setting(
                crate::storage::settings::RuntimeBackendSetting::Bridge,
            );
        }
        (_, KeyCode::Char('r'))
            if app.workspace.active_section == workspace::WorkspaceSection::Settings =>
        {
            app.cycle_reticulum_instance_mode();
        }
        (_, KeyCode::Char('R'))
            if app.workspace.active_section == workspace::WorkspaceSection::Settings =>
        {
            app.restart_to_apply_settings();
        }
        (_, KeyCode::Char('g'))
            if app.workspace.active_section == workspace::WorkspaceSection::Settings =>
        {
            app.toggle_announce_on_start();
        }
        (_, KeyCode::Char('l'))
            if app.workspace.active_section == workspace::WorkspaceSection::Settings =>
        {
            app.toggle_periodic_lxmf_sync();
        }
        (_, KeyCode::Char('v'))
            if app.workspace.active_section == workspace::WorkspaceSection::Settings =>
        {
            app.edit_settings_lxmf_sync_interval();
        }
        (_, KeyCode::Char('c'))
            if app.workspace.active_section == workspace::WorkspaceSection::Settings =>
        {
            app.edit_settings_lxmf_sync_limit();
        }
        (_, KeyCode::Char('z'))
            if app.workspace.active_section == workspace::WorkspaceSection::Settings =>
        {
            app.edit_settings_preferred_propagation();
        }
        (_, KeyCode::Char('u'))
            if app.workspace.active_section == workspace::WorkspaceSection::Settings =>
        {
            app.toggle_plugin_remote_content();
        }
        (_, KeyCode::Char('I'))
            if app.workspace.active_section == workspace::WorkspaceSection::Settings =>
        {
            app.create_settings_managed_identity();
        }
        (_, KeyCode::Char('i'))
            if app.workspace.active_section == workspace::WorkspaceSection::Settings =>
        {
            app.edit_settings_identity_path();
        }
        (_, KeyCode::Char('k'))
            if app.workspace.active_section == workspace::WorkspaceSection::Settings =>
        {
            app.edit_settings_reticulum_config_path();
        }
        (_, KeyCode::Char('t'))
            if app.workspace.active_section == workspace::WorkspaceSection::Settings =>
        {
            app.edit_settings_theme_name();
        }
        (_, KeyCode::Char('h'))
            if app.workspace.active_section == workspace::WorkspaceSection::Settings =>
        {
            app.edit_settings_default_start_page();
        }
        (_, KeyCode::Char('m'))
            if app.workspace.active_section == workspace::WorkspaceSection::Settings =>
        {
            app.edit_settings_browser_form_max_age();
        }
        (_, KeyCode::Char('B'))
            if app.workspace.active_section == workspace::WorkspaceSection::Settings =>
        {
            app.edit_settings_log_max_bytes();
        }
        (_, KeyCode::Char('F'))
            if app.workspace.active_section == workspace::WorkspaceSection::Settings =>
        {
            app.edit_settings_log_retain_files();
        }
        (_, KeyCode::Char('L'))
            if app.workspace.active_section == workspace::WorkspaceSection::Settings =>
        {
            app.edit_settings_log_load_recent_entries();
        }
        (_, KeyCode::Char('f'))
            if app.workspace.active_section == workspace::WorkspaceSection::Settings =>
        {
            app.forget_current_page_form_state();
        }
        (_, KeyCode::Char('n'))
            if app.workspace.active_section == workspace::WorkspaceSection::Settings =>
        {
            app.forget_current_node_form_state();
        }
        (_, KeyCode::Char('a'))
            if app.workspace.active_section == workspace::WorkspaceSection::Settings =>
        {
            app.forget_all_browser_form_state();
        }
        (_, KeyCode::Backspace) => app.address_backspace(),
        (_, KeyCode::Delete) => app.input_delete(),
        (_, KeyCode::Char(ch)) => app.edit_address_char(ch),
        _ => {}
    }
}
