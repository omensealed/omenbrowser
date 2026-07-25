pub mod mouse;
mod operations;
pub mod status;
pub mod tabs;
pub mod workspace;

use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
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

trait TerminalLifecycle {
    fn enable_raw(&mut self) -> io::Result<()>;
    fn enter_alternate_screen(&mut self) -> io::Result<()>;
    fn enable_mouse_capture(&mut self) -> io::Result<()>;
    fn disable_raw(&mut self) -> io::Result<()>;
    fn disable_mouse_capture(&mut self) -> io::Result<()>;
    fn leave_alternate_screen(&mut self) -> io::Result<()>;
}

struct CrosstermLifecycle;

struct ExternalSignalTask(tokio::task::JoinHandle<()>);

impl ExternalSignalTask {
    fn install(shutdown_requested: Arc<AtomicBool>) -> Self {
        Self(tokio::spawn(async move {
            if let Err(error) = listen_for_external_shutdown(&shutdown_requested).await {
                tracing::warn!(%error, "failed to listen for TUI shutdown signal");
            }
        }))
    }
}

impl Drop for ExternalSignalTask {
    fn drop(&mut self) {
        self.0.abort();
    }
}

#[cfg(unix)]
async fn listen_for_external_shutdown(shutdown_requested: &AtomicBool) -> io::Result<()> {
    let mut interrupt = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())?;
    let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
    loop {
        let (name, received) = tokio::select! {
            received = interrupt.recv() => ("SIGINT", received),
            received = terminate.recv() => ("SIGTERM", received),
        };
        if received.is_none() {
            return Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                format!("{name} stream closed"),
            ));
        }
        request_external_shutdown(shutdown_requested);
    }
}

#[cfg(not(unix))]
async fn listen_for_external_shutdown(shutdown_requested: &AtomicBool) -> io::Result<()> {
    loop {
        tokio::signal::ctrl_c().await?;
        request_external_shutdown(shutdown_requested);
    }
}

fn request_external_shutdown(requested: &AtomicBool) {
    requested.store(true, Ordering::Release);
}

impl TerminalLifecycle for CrosstermLifecycle {
    fn enable_raw(&mut self) -> io::Result<()> {
        enable_raw_mode()
    }

    fn enter_alternate_screen(&mut self) -> io::Result<()> {
        execute!(io::stdout(), EnterAlternateScreen)
    }

    fn enable_mouse_capture(&mut self) -> io::Result<()> {
        execute!(io::stdout(), EnableMouseCapture)
    }

    fn disable_raw(&mut self) -> io::Result<()> {
        disable_raw_mode()
    }

    fn disable_mouse_capture(&mut self) -> io::Result<()> {
        execute!(io::stdout(), DisableMouseCapture)
    }

    fn leave_alternate_screen(&mut self) -> io::Result<()> {
        execute!(io::stdout(), LeaveAlternateScreen)
    }
}

struct TerminalGuard<L: TerminalLifecycle> {
    lifecycle: L,
    raw_enabled: bool,
    alternate_screen_entered: bool,
    mouse_capture_enabled: bool,
}

impl TerminalGuard<CrosstermLifecycle> {
    fn enter() -> AppResult<Self> {
        Self::enter_with(CrosstermLifecycle)
    }
}

impl<L: TerminalLifecycle> TerminalGuard<L> {
    fn enter_with(lifecycle: L) -> AppResult<Self> {
        let mut guard = Self {
            lifecycle,
            raw_enabled: false,
            alternate_screen_entered: false,
            mouse_capture_enabled: false,
        };

        guard.raw_enabled = true;
        if let Err(error) = guard.lifecycle.enable_raw() {
            guard.restore();
            return Err(error.into());
        }
        guard.alternate_screen_entered = true;
        if let Err(error) = guard.lifecycle.enter_alternate_screen() {
            guard.restore();
            return Err(error.into());
        }
        guard.mouse_capture_enabled = true;
        if let Err(error) = guard.lifecycle.enable_mouse_capture() {
            guard.restore();
            return Err(error.into());
        }

        Ok(guard)
    }

    fn restore(&mut self) {
        if self.raw_enabled {
            let _ = self.lifecycle.disable_raw();
            self.raw_enabled = false;
        }
        if self.mouse_capture_enabled {
            let _ = self.lifecycle.disable_mouse_capture();
            self.mouse_capture_enabled = false;
        }
        if self.alternate_screen_entered {
            let _ = self.lifecycle.leave_alternate_screen();
            self.alternate_screen_entered = false;
        }
    }
}

impl<L: TerminalLifecycle> Drop for TerminalGuard<L> {
    fn drop(&mut self) {
        self.restore();
    }
}

pub async fn run(mut app: App) -> AppResult<()> {
    let _guard = TerminalGuard::enter()?;
    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend)?;
    let external_shutdown = Arc::new(AtomicBool::new(false));
    let _signal_task = ExternalSignalTask::install(Arc::clone(&external_shutdown));
    app.start_configured_runtime_nonblocking();

    while !app.should_quit() {
        if apply_external_shutdown(&mut app, &external_shutdown) {
            continue;
        }
        let now = current_epoch_ms();
        app.refresh_due_browser_partials(now);
        app.flush_due_ui_preferences(now);
        app.drain_internal_events();
        app.drain_browser_task_results();
        app.drain_message_task_results();
        app.drain_diagnostics_task_results();
        let terminal_area = terminal.size()?.into();
        let (browser_width, browser_height) = mouse::browser_content_inner_size(terminal_area);
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
                    handle_mouse(&mut app, terminal_area, mouse).await;
                    app.drain_browser_task_results();
                    app.drain_message_task_results();
                    app.drain_diagnostics_task_results();
                }
                _ => {}
            }
        }
    }
    app.flush_pending_ui_preferences();
    let _ = app.flush_structured_logs(Duration::from_secs(3));

    Ok(())
}

fn apply_external_shutdown(app: &mut App, requested: &AtomicBool) -> bool {
    consume_external_shutdown(requested, || app.quit())
}

fn consume_external_shutdown(requested: &AtomicBool, synchronous_shutdown: impl FnOnce()) -> bool {
    if !requested.swap(false, Ordering::AcqRel) {
        return false;
    }
    synchronous_shutdown();
    true
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
        MouseAction::ConfirmDirectStamp => {
            app.confirm_active_conversation_direct_stamp();
        }
        MouseAction::CancelDirectStamp => {
            app.cancel_active_conversation_direct_stamp();
        }
    }
}

async fn handle_key(app: &mut App, key: KeyEvent) {
    match (key.modifiers, key.code) {
        (modifiers, KeyCode::Char(character))
            if modifiers.contains(KeyModifiers::CONTROL) && matches!(character, 'c' | 'C') =>
        {
            app.quit();
        }
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
        (_, KeyCode::Char(ch))
            if key.modifiers.is_empty()
                && matches!(
                    app.input.active.as_ref().map(|active| &active.target),
                    Some(crate::input::InputTarget::OperationsSearch)
                ) =>
        {
            app.edit_operations_search_char(ch);
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
        (KeyModifiers::CONTROL, KeyCode::Char('a'))
            if app.workspace.active_section == workspace::WorkspaceSection::Messages =>
        {
            app.confirm_active_conversation_direct_stamp();
        }
        (KeyModifiers::CONTROL, KeyCode::Char('x'))
            if app.workspace.active_section == workspace::WorkspaceSection::Messages =>
        {
            app.cancel_active_conversation_direct_stamp();
        }
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
        (_, KeyCode::Char('/'))
            if app.workspace.active_section == workspace::WorkspaceSection::NetworkDoctor =>
        {
            app.focus_operations_search();
        }
        (_, KeyCode::Char('f'))
            if app.workspace.active_section == workspace::WorkspaceSection::NetworkDoctor =>
        {
            app.cycle_operations_filter();
        }
        (_, KeyCode::Char('c'))
            if app.workspace.active_section == workspace::WorkspaceSection::NetworkDoctor =>
        {
            app.clear_operations_search();
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
        (_, KeyCode::Char('r'))
            if app.workspace.active_section == workspace::WorkspaceSection::Directory =>
        {
            app.refresh_selected_propagation_node();
        }
        (_, KeyCode::Char('x'))
            if app.workspace.active_section == workspace::WorkspaceSection::Directory =>
        {
            app.cancel_propagation_node_refresh();
        }
        (_, KeyCode::Char('p'))
            if app.workspace.active_section == workspace::WorkspaceSection::Directory =>
        {
            app.use_selected_directory_propagation_node();
        }
        (_, KeyCode::Char('g'))
            if app.workspace.active_section == workspace::WorkspaceSection::Directory =>
        {
            app.sync_propagation_messages_now();
        }
        (_, KeyCode::Char('k'))
            if app.workspace.active_section == workspace::WorkspaceSection::Directory =>
        {
            app.cycle_selected_directory_reply_ticket_preference();
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{mpsc, Arc, Mutex};

    use ratatui::backend::TestBackend;

    use crate::config::{AppConfig, AppPaths};
    use crate::storage::settings::AppSettings;

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum EnterFailure {
        None,
        Raw,
        AlternateScreen,
        MouseCapture,
    }

    struct RecordingLifecycle {
        calls: Arc<Mutex<Vec<&'static str>>>,
        failure: EnterFailure,
    }

    impl RecordingLifecycle {
        fn record(&self, call: &'static str) -> io::Result<()> {
            self.calls.lock().expect("record lifecycle call").push(call);
            let should_fail = matches!(
                (self.failure, call),
                (EnterFailure::Raw, "enable_raw")
                    | (EnterFailure::AlternateScreen, "enter_alternate_screen")
                    | (EnterFailure::MouseCapture, "enable_mouse_capture")
            );
            if should_fail {
                Err(io::Error::other(format!("injected {call} failure")))
            } else {
                Ok(())
            }
        }
    }

    impl TerminalLifecycle for RecordingLifecycle {
        fn enable_raw(&mut self) -> io::Result<()> {
            self.record("enable_raw")
        }

        fn enter_alternate_screen(&mut self) -> io::Result<()> {
            self.record("enter_alternate_screen")
        }

        fn enable_mouse_capture(&mut self) -> io::Result<()> {
            self.record("enable_mouse_capture")
        }

        fn disable_raw(&mut self) -> io::Result<()> {
            self.record("disable_raw")
        }

        fn disable_mouse_capture(&mut self) -> io::Result<()> {
            self.record("disable_mouse_capture")
        }

        fn leave_alternate_screen(&mut self) -> io::Result<()> {
            self.record("leave_alternate_screen")
        }
    }

    fn lifecycle(failure: EnterFailure) -> (RecordingLifecycle, Arc<Mutex<Vec<&'static str>>>) {
        let calls = Arc::new(Mutex::new(Vec::new()));
        (
            RecordingLifecycle {
                calls: Arc::clone(&calls),
                failure,
            },
            calls,
        )
    }

    #[test]
    fn terminal_guard_restores_every_successfully_entered_mode_on_drop() {
        let (lifecycle, calls) = lifecycle(EnterFailure::None);
        let guard = TerminalGuard::enter_with(lifecycle).expect("enter terminal lifecycle");
        drop(guard);

        assert_eq!(
            *calls.lock().expect("read lifecycle calls"),
            [
                "enable_raw",
                "enter_alternate_screen",
                "enable_mouse_capture",
                "disable_raw",
                "disable_mouse_capture",
                "leave_alternate_screen",
            ]
        );
    }

    #[test]
    fn terminal_guard_rolls_back_every_partial_enter_failure() {
        let cases = [
            (EnterFailure::Raw, vec!["enable_raw", "disable_raw"]),
            (
                EnterFailure::AlternateScreen,
                vec![
                    "enable_raw",
                    "enter_alternate_screen",
                    "disable_raw",
                    "leave_alternate_screen",
                ],
            ),
            (
                EnterFailure::MouseCapture,
                vec![
                    "enable_raw",
                    "enter_alternate_screen",
                    "enable_mouse_capture",
                    "disable_raw",
                    "disable_mouse_capture",
                    "leave_alternate_screen",
                ],
            ),
        ];

        for (failure, expected) in cases {
            let (lifecycle, calls) = lifecycle(failure);
            let result = TerminalGuard::enter_with(lifecycle);
            assert!(result.is_err(), "{failure:?} must fail");
            assert_eq!(*calls.lock().expect("read lifecycle calls"), expected);
        }
    }

    #[tokio::test]
    async fn isolated_tui_render_and_quit_smoke_preserves_root_boundary() {
        let root = std::env::temp_dir().join(format!(
            "omenbrowser-rs-tui-lifecycle-{}-{}",
            std::process::id(),
            current_epoch_ms()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let mut app = App::new(AppConfig {
            paths: AppPaths::from_root(root.clone()),
            settings: AppSettings::default(),
        });
        assert_eq!(app.paths.root, root);

        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).expect("create test terminal");
        terminal
            .draw(|frame| workspace::render(frame, &app))
            .expect("render initial TUI frame");
        let rendered = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(rendered.contains("OMENbrowser_rs"));

        handle_key(
            &mut app,
            KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE),
        )
        .await;
        assert!(app.should_quit());
        app.flush_pending_ui_preferences();
        assert_eq!(app.paths.root, root);

        drop(app);
        std::fs::remove_dir_all(root).expect("remove isolated TUI root");
    }

    #[tokio::test]
    async fn control_c_requests_graceful_quit_even_while_editing() {
        let root = std::env::temp_dir().join(format!(
            "omenbrowser-rs-tui-control-c-{}-{}",
            std::process::id(),
            current_epoch_ms()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let mut app = App::new(AppConfig {
            paths: AppPaths::from_root(root.clone()),
            settings: AppSettings::default(),
        });
        app.focus_address_bar();
        assert!(app.input.active.is_some());

        handle_key(
            &mut app,
            KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL),
        )
        .await;

        assert!(app.should_quit());
        assert_eq!(app.paths.root, root);
        drop(app);
        std::fs::remove_dir_all(root).expect("remove isolated TUI root");
    }

    #[tokio::test]
    async fn network_doctor_search_and_filter_keys_are_bounded_and_ephemeral() {
        let root = std::env::temp_dir().join(format!(
            "omenbrowser-rs-tui-operation-search-{}-{}",
            std::process::id(),
            current_epoch_ms()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let mut app = App::new(AppConfig {
            paths: AppPaths::from_root(root.clone()),
            settings: AppSettings::default(),
        });
        app.switch_section(workspace::WorkspaceSection::NetworkDoctor);

        handle_key(
            &mut app,
            KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE),
        )
        .await;
        assert!(matches!(
            app.input.active.as_ref().map(|active| &active.target),
            Some(crate::input::InputTarget::OperationsSearch)
        ));
        for character in "attention room".chars() {
            handle_key(
                &mut app,
                KeyEvent::new(KeyCode::Char(character), KeyModifiers::NONE),
            )
            .await;
        }
        handle_key(&mut app, KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)).await;
        assert_eq!(app.network_doctor_state.operations_search, "attention room");

        handle_key(
            &mut app,
            KeyEvent::new(KeyCode::Char('f'), KeyModifiers::NONE),
        )
        .await;
        assert_eq!(
            app.network_doctor_state.operations_filter,
            crate::operations::presentation::OperationPresentationFilter::Active
        );
        handle_key(
            &mut app,
            KeyEvent::new(KeyCode::Char('c'), KeyModifiers::NONE),
        )
        .await;
        assert!(app.network_doctor_state.operations_search.is_empty());

        app.focus_operations_search();
        for _ in 0..crate::operations::presentation::OPERATION_PRESENTATION_SEARCH_MAX_BYTES {
            assert!(app.edit_operations_search_char('x'));
        }
        assert!(!app.edit_operations_search_char('y'));
        assert_eq!(
            app.input
                .active
                .as_ref()
                .expect("search input")
                .buffer
                .as_str()
                .len(),
            crate::operations::presentation::OPERATION_PRESENTATION_SEARCH_MAX_BYTES
        );

        drop(app);
        std::fs::remove_dir_all(root).expect("remove isolated TUI root");
    }

    #[test]
    fn repeated_external_signal_requests_coalesce_into_graceful_quit() {
        let root = std::env::temp_dir().join(format!(
            "omenbrowser-rs-tui-external-signal-{}-{}",
            std::process::id(),
            current_epoch_ms()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let mut app = App::new(AppConfig {
            paths: AppPaths::from_root(root.clone()),
            settings: AppSettings::default(),
        });
        let requested = AtomicBool::new(false);

        request_external_shutdown(&requested);
        request_external_shutdown(&requested);
        assert!(requested.load(Ordering::Acquire));
        assert!(apply_external_shutdown(&mut app, &requested));
        assert!(app.should_quit());
        assert!(!requested.load(Ordering::Acquire));
        assert!(!apply_external_shutdown(&mut app, &requested));
        assert_eq!(app.paths.root, root);

        drop(app);
        std::fs::remove_dir_all(root).expect("remove isolated TUI root");
    }

    #[test]
    fn repeated_signal_during_synchronous_shutdown_stays_bounded_and_restores_terminal() {
        let root = std::env::temp_dir().join(format!(
            "omenbrowser-rs-tui-signal-during-persistence-{}-{}",
            std::process::id(),
            current_epoch_ms()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let mut app = App::new(AppConfig {
            paths: AppPaths::from_root(root.clone()),
            settings: AppSettings::default(),
        });
        app.toggle_help();

        let (lifecycle, calls) = lifecycle(EnterFailure::None);
        let guard = TerminalGuard::enter_with(lifecycle).expect("enter terminal lifecycle");
        let requested = Arc::new(AtomicBool::new(false));
        request_external_shutdown(&requested);

        let (shutdown_entered_tx, shutdown_entered_rx) = mpsc::sync_channel(0);
        let (release_shutdown_tx, release_shutdown_rx) = mpsc::sync_channel(0);
        let repeated_request = Arc::clone(&requested);
        let requester = std::thread::spawn(move || {
            shutdown_entered_rx
                .recv_timeout(Duration::from_secs(1))
                .expect("synchronous shutdown boundary entered");
            request_external_shutdown(&repeated_request);
            request_external_shutdown(&repeated_request);
            release_shutdown_tx
                .send(())
                .expect("release synchronous shutdown boundary");
        });

        assert!(consume_external_shutdown(&requested, || {
            shutdown_entered_tx
                .send(())
                .expect("announce synchronous shutdown boundary");
            release_shutdown_rx
                .recv_timeout(Duration::from_secs(1))
                .expect("receive shutdown release");
            app.quit();
        }));
        requester.join().expect("join repeated signal requester");

        assert!(app.should_quit());
        assert!(requested.swap(false, Ordering::AcqRel));
        assert!(!requested.load(Ordering::Acquire));
        let settings = std::fs::read_to_string(&app.paths.settings_file)
            .expect("read settings flushed by graceful quit");
        serde_json::from_str::<serde_json::Value>(&settings)
            .expect("parse settings flushed by graceful quit");

        drop(guard);
        assert_eq!(
            *calls.lock().expect("read lifecycle calls"),
            [
                "enable_raw",
                "enter_alternate_screen",
                "enable_mouse_capture",
                "disable_raw",
                "disable_mouse_capture",
                "leave_alternate_screen",
            ]
        );
        assert_eq!(app.paths.root, root);
        drop(app);
        std::fs::remove_dir_all(root).expect("remove isolated TUI root");
    }
}
