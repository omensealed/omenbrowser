#[cfg(feature = "live-rns-net")]
use std::collections::BTreeMap;
use std::io::{self, IsTerminal, Write};
use std::time::Duration;
#[cfg(feature = "live-rns-net")]
use std::time::Instant;

use crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyModifiers,
    MouseButton, MouseEventKind,
};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap};
use ratatui::{Frame, Terminal};

use crate::config::{self, ServerConfig};
use crate::error::{ServerError, ServerResult};
#[cfg(feature = "live-rns-net")]
use crate::live::ActiveLinkSummary;
#[cfg(feature = "live-rns-net")]
use crate::live::ClosedLinkSummary;
#[cfg(feature = "live-rns-net")]
use crate::live::LiveServerStats;
use crate::tui_format::{
    current_unix_secs, human_age, human_age_duration, human_bytes, human_system_time_local,
    human_timestamp, unix_to_utc_string,
};
use crate::tui_layout::{
    action_hitboxes, action_list_label, inner_rect, list_row_at, tab_hitboxes, tab_label,
    tab_panel_height,
};
#[cfg(feature = "live-rns-net")]
use crate::tui_text::closed_link_churn_summary;
#[cfg(feature = "live-rns-net")]
use crate::tui_text::traffic_delta_text;
#[cfg(feature = "live-rns-net")]
use crate::tui_text::{
    active_link_activity_label, active_link_monitoring_line, closed_link_monitoring_line,
    closed_link_status_label, interface_health_label, ActiveLinkMonitoringText,
    ClosedLinkMonitoringText,
};
use crate::tui_text::{
    admin_help_text, announce_interval_update_text, audit_summary_text, command_help_text,
    command_rate_update_text, history_batch_size_update_text,
    identity_panel_text as format_identity_panel_text, interface_operator_summary_text,
    join_backlog_events_update_text, large_batch_threshold_update_text,
    max_message_bytes_update_text, message_rate_update_text, moderation_action_guide_text,
    moderation_selected_user_text, moderation_user_list_label, motd_update_text,
    operator_label_update_text,
    overview_operator_summary_text as format_overview_operator_summary_text,
    ping_interval_update_text, portal_panel_text as format_portal_panel_text,
    reticulum_interface_summary, room_action_guide_text, room_archived_update_text,
    room_console_row_text, room_list_label_text, room_ready_update_text, room_topic_update_text,
    selected_room_text, server_limits_text, server_name_update_text,
    setup_addresses_text as format_setup_addresses_text, setup_checklist_line_text,
    setup_console_text, setup_launch_status_text as format_setup_launch_status_text,
    setup_next_steps_text as format_setup_next_steps_text, upload_max_file_update_text,
    upload_policy_hint, upload_quota_update_text, user_banned_update_text, user_console_row_text,
    user_muted_update_text, user_role_update_text, user_trusted_update_text,
    user_unbanned_update_text, user_unmuted_update_text, user_untrusted_update_text,
    IdentityPanelText, ModerationUserText, OverviewOperatorSummaryText, PortalPanelText,
    RoomConsoleRowText, RoomListLabelText, SetupAddressesText, SetupChecklistLineText,
    SetupConsoleText, SetupLaunchText, SetupNextStepsText, UserConsoleRowText,
};
#[cfg(feature = "live-rns-net")]
use crate::tui_text::{monitoring_operator_summary_text, upload_transfer_summary};
use crate::{parse_tcp_server_override, TcpClientOverride};

#[cfg(feature = "live-rns-net")]
use crate::rns_net_live::{self, RnsNetLiveRuntime};

const SETUP_ACTION_PANEL_HEIGHT: u16 = 21;

pub fn run_admin_console(config: ServerConfig) -> ServerResult<()> {
    if io::stdout().is_terminal() && io::stdin().is_terminal() {
        run_dashboard(config)
    } else {
        run_line_console(config)
    }
}

fn run_dashboard(mut config: ServerConfig) -> ServerResult<()> {
    config::init_files(&config)?;
    let mut terminal = TerminalGuard::enter()?;
    let mut app = AdminTui::new(config);
    app.start_live_runtime();

    loop {
        app.tick_live_runtime();
        terminal.draw(|frame| app.render(frame))?;
        if event::poll(Duration::from_millis(250))? {
            match event::read()? {
                Event::Key(key) => {
                    if app.handle_key(key)? {
                        break;
                    }
                }
                Event::Mouse(mouse) => app.handle_mouse(mouse.kind, mouse.column, mouse.row)?,
                Event::Resize(_, _) => {}
                _ => {}
            }
        }
    }

    config = app.config;
    config.save()?;
    Ok(())
}

struct TerminalGuard {
    terminal: Terminal<CrosstermBackend<io::Stdout>>,
}

impl TerminalGuard {
    fn enter() -> ServerResult<Self> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
        let backend = CrosstermBackend::new(stdout);
        let terminal = Terminal::new(backend)?;
        Ok(Self { terminal })
    }

    fn draw<F>(&mut self, render: F) -> ServerResult<()>
    where
        F: FnOnce(&mut Frame<'_>),
    {
        self.terminal.draw(render)?;
        Ok(())
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(
            self.terminal.backend_mut(),
            LeaveAlternateScreen,
            DisableMouseCapture
        );
        let _ = self.terminal.show_cursor();
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AdminTab {
    Overview,
    Setup,
    Rooms,
    Moderation,
    Monitoring,
    Audit,
    Identity,
    Interfaces,
    Portal,
    Logs,
    Help,
}

impl AdminTab {
    pub(crate) const ALL: [Self; 11] = [
        Self::Overview,
        Self::Setup,
        Self::Rooms,
        Self::Moderation,
        Self::Monitoring,
        Self::Audit,
        Self::Identity,
        Self::Interfaces,
        Self::Portal,
        Self::Logs,
        Self::Help,
    ];

    pub(crate) fn title(self) -> &'static str {
        match self {
            Self::Overview => "Overview",
            Self::Setup => "Setup",
            Self::Rooms => "Rooms",
            Self::Moderation => "Moderation",
            Self::Monitoring => "Monitoring",
            Self::Audit => "Audit",
            Self::Identity => "Identity",
            Self::Interfaces => "Interfaces",
            Self::Portal => "Portal",
            Self::Logs => "Logs",
            Self::Help => "Help",
        }
    }

    pub(crate) fn compact_title(self) -> &'static str {
        match self {
            Self::Overview => "Ov",
            Self::Setup => "Set",
            Self::Rooms => "Rm",
            Self::Moderation => "Mod",
            Self::Monitoring => "Mon",
            Self::Audit => "Aud",
            Self::Identity => "ID",
            Self::Interfaces => "If",
            Self::Portal => "Por",
            Self::Logs => "Log",
            Self::Help => "Help",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum InputMode {
    Navigate,
    EditName,
    EditOperator,
    EditMotd,
    EditAnnounceInterval,
    EditUploadQuotaBytes,
    EditUploadMaxFileBytes,
    EditPingIntervalSeconds,
    EditMaxMessageBytes,
    EditHistoryBatchSize,
    EditJoinBacklogEvents,
    EditLargeBatchThresholdBytes,
    EditMessageRate,
    EditCommandRate,
    EditTcpServer,
    EditTcpClient,
    EditRoomTopic,
    AddRoomName,
    AddRoomTopic,
}

impl InputMode {
    fn prompt(self) -> &'static str {
        match self {
            Self::Navigate => {
                "Click a tab or use Tab/Shift+Tab | Enter edits current panel | s save | q quit"
            }
            Self::EditName => "Editing server name | Enter save | Esc cancel",
            Self::EditOperator => "Editing operator label | Enter save | Esc cancel",
            Self::EditMotd => "Editing server MOTD | Enter save | Esc cancel",
            Self::EditAnnounceInterval => {
                "Editing announce interval minutes | Enter save | Esc cancel"
            }
            Self::EditUploadQuotaBytes => {
                "Editing upload quota bytes; 0 disables uploads | Enter save | Esc cancel"
            }
            Self::EditUploadMaxFileBytes => {
                "Editing max upload file bytes | Enter save | Esc cancel"
            }
            Self::EditPingIntervalSeconds => {
                "Editing client ping interval seconds | Enter save | Esc cancel"
            }
            Self::EditMaxMessageBytes => "Editing max message bytes | Enter save | Esc cancel",
            Self::EditHistoryBatchSize => {
                "Editing history batch event count | Enter save | Esc cancel"
            }
            Self::EditJoinBacklogEvents => {
                "Editing join backlog event count | Enter save | Esc cancel"
            }
            Self::EditLargeBatchThresholdBytes => {
                "Editing large batch threshold bytes | Enter save | Esc cancel"
            }
            Self::EditMessageRate => "Editing message rate per minute | Enter save | Esc cancel",
            Self::EditCommandRate => "Editing command rate per minute | Enter save | Esc cancel",
            Self::EditTcpServer => {
                "Editing TCP server listen address | Enter write config | Esc cancel"
            }
            Self::EditTcpClient => "Editing TCP gateway address | Enter write config | Esc cancel",
            Self::EditRoomTopic => "Editing selected room topic | Enter save | Esc cancel",
            Self::AddRoomName => "New room name | Enter continue | Esc cancel",
            Self::AddRoomTopic => "New room topic | Enter create | Esc cancel",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AdminAction {
    StartLive,
    StopLive,
    AnnounceNow,
    EditServerName,
    EditOperator,
    EditMotd,
    EditAnnounceInterval,
    EditUploadQuotaBytes,
    EditUploadMaxFileBytes,
    EditPingIntervalSeconds,
    EditMaxMessageBytes,
    EditHistoryBatchSize,
    EditJoinBacklogEvents,
    EditLargeBatchThresholdBytes,
    EditMessageRate,
    EditCommandRate,
    EditTcpServer,
    EditTcpClient,
    SelectTab(AdminTab),
    SaveConfig,
    AddRoom,
    EditRoomTopic,
    ArchiveRoom,
    ToggleBan,
    KickActiveUser,
    ToggleMute,
    ToggleTrust,
    SetRole(u64),
    DeleteStaleUser,
    PruneStaleUsers,
}

struct AdminTui {
    config: ServerConfig,
    tab: AdminTab,
    selected_room: usize,
    input_mode: InputMode,
    input: String,
    pending_room_name: String,
    status: String,
    selected_user: usize,
    pending_archive_room_id: Option<i64>,
    pending_delete_user_id: Option<i64>,
    pending_prune_stale_users: bool,
    tab_clicks: Vec<(Rect, AdminTab)>,
    action_clicks: Vec<(Rect, AdminAction)>,
    room_list_area: Rect,
    user_list_area: Rect,
    help_scroll: u16,
    #[cfg(feature = "live-rns-net")]
    live: Option<TuiLiveRuntime>,
    #[cfg(feature = "live-rns-net")]
    next_live_announce: Instant,
    #[cfg(feature = "live-rns-net")]
    next_live_stats: Instant,
    #[cfg(feature = "live-rns-net")]
    live_status: String,
    last_announce_event: String,
}

#[cfg(feature = "live-rns-net")]
struct TuiLiveRuntime {
    runtime: RnsNetLiveRuntime,
    last_stats: String,
    last_stats_snapshot: LiveServerStats,
    last_stats_at: Instant,
    recent_stats: String,
    last_interface_stats: Vec<String>,
    interface_recovery_samples: u8,
}

impl AdminTui {
    fn new(config: ServerConfig) -> Self {
        Self {
            config,
            tab: AdminTab::Overview,
            selected_room: 0,
            input_mode: InputMode::Navigate,
            input: String::new(),
            pending_room_name: String::new(),
            status: "ready".into(),
            selected_user: 0,
            pending_archive_room_id: None,
            pending_delete_user_id: None,
            pending_prune_stale_users: false,
            tab_clicks: Vec::new(),
            action_clicks: Vec::new(),
            room_list_area: Rect::default(),
            user_list_area: Rect::default(),
            help_scroll: 0,
            #[cfg(feature = "live-rns-net")]
            live: None,
            #[cfg(feature = "live-rns-net")]
            next_live_announce: Instant::now(),
            #[cfg(feature = "live-rns-net")]
            next_live_stats: Instant::now(),
            #[cfg(feature = "live-rns-net")]
            live_status: "live server not started".into(),
            last_announce_event: "none yet".into(),
        }
    }

    fn render(&mut self, frame: &mut Frame<'_>) {
        let root = frame.size();
        let tab_height = tab_panel_height(root.width);
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(tab_height),
                Constraint::Min(8),
                Constraint::Length(3),
            ])
            .split(root);

        self.render_tabs(frame, chunks[0]);
        self.render_body(frame, chunks[1]);
        self.render_status(frame, chunks[2]);

        if self.input_mode != InputMode::Navigate {
            self.render_input_popup(frame, centered_rect(70, 26, root));
        }
    }

    #[cfg(feature = "live-rns-net")]
    fn start_live_runtime(&mut self) {
        if self.live.is_some() {
            self.status =
                "live server already running; verify active clients and interfaces in Monitoring"
                    .into();
            return;
        }
        if let Err(error) = config::init_files(&self.config) {
            self.live_status = format!("live startup failed: {error}; check Logs and Setup");
            self.status = self.live_status.clone();
            return;
        }
        match rns_net_live::start_live_server(&self.config) {
            Ok(runtime) => {
                let last_stats = runtime.live_server.stats().summary_line();
                let last_stats_snapshot = runtime.live_server.stats().clone();
                let last_interface_stats = runtime.interface_stats_lines();
                let destination = hex_lower_local(&runtime.destination_hash);
                self.live = Some(TuiLiveRuntime {
                    runtime,
                    last_stats,
                    last_stats_snapshot,
                    last_stats_at: Instant::now(),
                    recent_stats: "waiting for next sample".into(),
                    last_interface_stats,
                    interface_recovery_samples: 0,
                });
                self.next_live_announce = Instant::now()
                    + Duration::from_secs(self.config.announce_interval_minutes.max(1) * 60);
                self.next_live_stats = Instant::now() + Duration::from_secs(5);
                self.live_status = format!("live server running destination={destination}");
                self.last_announce_event = announce_event_text("startup", &destination);
                self.status = format!(
                    "live server started: omenchat://{destination}; verify Monitoring before sharing"
                );
            }
            Err(error) => {
                self.live_status = format!("live startup failed: {error}; check Logs and Setup");
                self.status = self.live_status.clone();
            }
        }
    }

    #[cfg(not(feature = "live-rns-net"))]
    fn start_live_runtime(&mut self) {
        self.status =
            "live server unavailable: rebuild omenchatd with --features live-rns-net".into();
    }

    #[cfg(feature = "live-rns-net")]
    fn stop_live_runtime(&mut self) {
        if self.live.take().is_some() {
            self.live_status = "live server stopped".into();
            self.status = "live server stopped: active links closed; config was not changed".into();
        } else {
            self.status = "live server is not running; press g or Start Live Server".into();
        }
    }

    #[cfg(not(feature = "live-rns-net"))]
    fn stop_live_runtime(&mut self) {
        self.status =
            "live server unavailable: rebuild omenchatd with --features live-rns-net".into();
    }

    #[cfg(feature = "live-rns-net")]
    fn announce_live_now(&mut self) {
        let Some(live) = self.live.as_mut() else {
            self.status = "live server is not running; start live before announcing".into();
            return;
        };
        match live.runtime.announce() {
            Ok(()) => {
                let destination = hex_lower_local(&live.runtime.destination_hash);
                self.next_live_announce = Instant::now()
                    + Duration::from_secs(self.config.announce_interval_minutes.max(1) * 60);
                self.live_status =
                    format!("live server running destination={destination} | announce sent");
                self.last_announce_event = announce_event_text("manual", &destination);
                self.status = format!("announce sent now: omenchat://{destination}");
            }
            Err(error) => {
                self.live_status = format!("live announce failed: {error}");
                self.status = self.live_status.clone();
            }
        }
    }

    #[cfg(not(feature = "live-rns-net"))]
    fn announce_live_now(&mut self) {
        self.status =
            "live announce unavailable: rebuild omenchatd with --features live-rns-net".into();
    }

    #[cfg(feature = "live-rns-net")]
    fn tick_live_runtime(&mut self) {
        const LIVE_RUNTIME_RESTART_BACKOFF: Duration = Duration::from_secs(5);
        const INTERFACE_RECOVERY_SAMPLES: u8 = 3;
        let Some(live) = self.live.as_mut() else {
            return;
        };
        match rns_net_live::drain_live_events_logged(&mut live.runtime, 64, &self.config) {
            Ok(drained) if drained > 0 => {
                self.live_status = format!(
                    "live server running | drained {drained} event(s) | {}",
                    live.runtime.live_server.stats().summary_line()
                );
            }
            Ok(_) => {}
            Err(error) => {
                self.live_status = format!(
                    "live event handling failed: {error}; restarting rns-net runtime after {}s",
                    LIVE_RUNTIME_RESTART_BACKOFF.as_secs()
                );
                self.status = self.live_status.clone();
                std::thread::sleep(LIVE_RUNTIME_RESTART_BACKOFF);
                match rns_net_live::start_live_server(&self.config) {
                    Ok(runtime) => {
                        let destination = hex_lower_local(&runtime.destination_hash);
                        live.runtime = runtime;
                        live.last_stats = live.runtime.live_server.stats().summary_line();
                        live.last_stats_snapshot = live.runtime.live_server.stats().clone();
                        live.last_stats_at = Instant::now();
                        live.recent_stats = "runtime restarted; waiting for next sample".into();
                        live.last_interface_stats = live.runtime.interface_stats_lines();
                        live.interface_recovery_samples = 0;
                        self.next_live_announce = Instant::now()
                            + Duration::from_secs(
                                self.config.announce_interval_minutes.max(1) * 60,
                            );
                        self.next_live_stats = Instant::now() + Duration::from_secs(5);
                        self.live_status =
                            format!("live runtime restarted destination={destination}");
                        self.last_announce_event =
                            announce_event_text("startup after runtime restart", &destination);
                        self.status = format!(
                            "live runtime restarted: omenchat://{destination}; verify Monitoring"
                        );
                    }
                    Err(restart_error) => {
                        self.live_status = format!("live runtime restart failed: {restart_error}");
                        self.status = self.live_status.clone();
                    }
                }
                return;
            }
        }

        let now = Instant::now();
        if now >= self.next_live_announce {
            match live.runtime.announce() {
                Ok(()) => {
                    let destination = hex_lower_local(&live.runtime.destination_hash);
                    self.status = "live destination announced".into();
                    self.live_status = format!(
                        "live server running destination={} | announce sent",
                        destination
                    );
                    self.last_announce_event = announce_event_text("automatic", &destination);
                }
                Err(error) => {
                    self.live_status = format!(
                        "live announce failed: {error}; restarting rns-net runtime after {}s",
                        LIVE_RUNTIME_RESTART_BACKOFF.as_secs()
                    );
                    self.status = self.live_status.clone();
                    std::thread::sleep(LIVE_RUNTIME_RESTART_BACKOFF);
                    match rns_net_live::start_live_server(&self.config) {
                        Ok(runtime) => {
                            let destination = hex_lower_local(&runtime.destination_hash);
                            live.runtime = runtime;
                            live.last_stats = live.runtime.live_server.stats().summary_line();
                            live.last_stats_snapshot = live.runtime.live_server.stats().clone();
                            live.last_stats_at = Instant::now();
                            live.recent_stats = "runtime restarted after announce failure".into();
                            live.last_interface_stats = live.runtime.interface_stats_lines();
                            live.interface_recovery_samples = 0;
                            self.live_status = format!(
                                "live runtime restarted after announce failure destination={destination}"
                            );
                            self.last_announce_event = announce_event_text(
                                "startup after announce recovery",
                                &destination,
                            );
                            self.status = self.live_status.clone();
                        }
                        Err(restart_error) => {
                            self.live_status =
                                format!("live runtime restart failed: {restart_error}");
                            self.status = self.live_status.clone();
                        }
                    }
                }
            }
            self.next_live_announce =
                now + Duration::from_secs(self.config.announce_interval_minutes.max(1) * 60);
        }

        if now >= self.next_live_stats {
            let stats_snapshot = live.runtime.live_server.stats().clone();
            let stats = stats_snapshot.summary_line();
            let interface_stats = live.runtime.interface_stats_lines();
            let interface_health = live.runtime.interface_health();
            if stats != live.last_stats || interface_stats != live.last_interface_stats {
                self.live_status = format!("live server running | {stats}");
                let elapsed = now
                    .saturating_duration_since(live.last_stats_at)
                    .as_secs_f64()
                    .max(0.001);
                live.recent_stats =
                    traffic_delta_text(&live.last_stats_snapshot, &stats_snapshot, elapsed);
                live.last_stats_snapshot = stats_snapshot;
                live.last_stats_at = now;
                live.last_stats = stats;
                live.last_interface_stats = interface_stats;
            }
            if interface_health.needs_runtime_restart() {
                live.interface_recovery_samples = live.interface_recovery_samples.saturating_add(1);
                self.live_status = format!(
                    "live interface watchdog {}/{}: {}",
                    live.interface_recovery_samples,
                    INTERFACE_RECOVERY_SAMPLES,
                    interface_health.label()
                );
                if live.interface_recovery_samples >= INTERFACE_RECOVERY_SAMPLES {
                    self.status = format!(
                        "interface watchdog restarting live runtime: {}",
                        interface_health.label()
                    );
                    std::thread::sleep(LIVE_RUNTIME_RESTART_BACKOFF);
                    match rns_net_live::start_live_server(&self.config) {
                        Ok(runtime) => {
                            let destination = hex_lower_local(&runtime.destination_hash);
                            live.runtime = runtime;
                            live.last_stats = live.runtime.live_server.stats().summary_line();
                            live.last_stats_snapshot = live.runtime.live_server.stats().clone();
                            live.last_stats_at = Instant::now();
                            live.recent_stats = "runtime restarted by interface watchdog".into();
                            live.last_interface_stats = live.runtime.interface_stats_lines();
                            live.interface_recovery_samples = 0;
                            self.next_live_announce = Instant::now()
                                + Duration::from_secs(
                                    self.config.announce_interval_minutes.max(1) * 60,
                                );
                            self.live_status = format!(
                                "live runtime restarted after interface watchdog destination={destination}"
                            );
                            self.last_announce_event = announce_event_text(
                                "startup after interface watchdog",
                                &destination,
                            );
                            self.status = self.live_status.clone();
                        }
                        Err(restart_error) => {
                            self.live_status =
                                format!("live runtime restart failed: {restart_error}");
                            self.status = self.live_status.clone();
                        }
                    }
                }
            } else {
                live.interface_recovery_samples = 0;
            }
            self.next_live_stats = now + Duration::from_secs(5);
        }
    }

    #[cfg(not(feature = "live-rns-net"))]
    fn tick_live_runtime(&mut self) {}

    fn render_tabs(&mut self, frame: &mut Frame<'_>, area: Rect) {
        self.tab_clicks = tab_hitboxes(area);
        frame.render_widget(admin_block("OMENchatd Admin"), area);
        for (hitbox, tab) in &self.tab_clicks {
            let style = if *tab == self.tab {
                Style::default()
                    .fg(Color::White)
                    .bg(Color::Rgb(70, 55, 120))
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Gray)
            };
            frame.render_widget(
                Paragraph::new(format!(" {} ", tab_label(*tab, area.width))).style(style),
                *hitbox,
            );
        }
    }

    fn live_status_text(&self) -> String {
        #[cfg(feature = "live-rns-net")]
        {
            let mut lines = vec![format!("runtime: {}", self.live_status)];
            if let Some(live) = self.live.as_ref() {
                lines.push(format!(
                    "interface: {}",
                    interface_health_label(&live.last_interface_stats)
                ));
                lines.push(format!(
                    "identity: {}",
                    hex_lower_local(&live.runtime.identity_hash)
                ));
                lines.push(format!(
                    "destination: {} ({})",
                    live.runtime.destination_name,
                    hex_lower_local(&live.runtime.destination_hash)
                ));
                lines.push(live.runtime.live_server.stats().summary_line());
                lines.extend(live.last_interface_stats.iter().cloned());
            }
            lines.join("\n")
        }
        #[cfg(not(feature = "live-rns-net"))]
        {
            "runtime: offline; rebuild with --features live-rns-net for all-in-one live server"
                .into()
        }
    }

    fn live_is_running(&self) -> bool {
        #[cfg(feature = "live-rns-net")]
        {
            self.live.is_some()
        }
        #[cfg(not(feature = "live-rns-net"))]
        {
            false
        }
    }

    fn stateful_actions(&self, actions: &[(AdminAction, &str)]) -> Vec<(AdminAction, String)> {
        string_actions_for_live_state(actions, self.live_is_running())
    }

    fn announce_schedule_text(&self) -> String {
        let interval = self.config.announce_interval_minutes.max(1);
        #[cfg(feature = "live-rns-net")]
        {
            let Some(_) = self.live.as_ref() else {
                return format!(
                    "announce: stopped | interval {interval}m | last: {} | start live before Announce Now",
                    self.last_announce_event
                );
            };
            let remaining = self
                .next_live_announce
                .saturating_duration_since(Instant::now())
                .as_secs()
                .min(i64::MAX as u64) as i64;
            return format!(
                "announce: next automatic in {} | interval {interval}m | last: {} | Announce Now is immediate",
                human_age_duration(remaining),
                self.last_announce_event
            );
        }
        #[cfg(not(feature = "live-rns-net"))]
        {
            format!(
                "announce: unavailable without live-rns-net | interval {interval}m | last: {}",
                self.last_announce_event
            )
        }
    }

    fn monitoring_counter_text(&self) -> String {
        #[cfg(feature = "live-rns-net")]
        {
            let Some(live) = self.live.as_ref() else {
                return [
                    monitoring_operator_summary_text(None, &[], "waiting for next sample", &[]),
                    self.announce_schedule_text(),
                ]
                .join("\n");
            };
            let stats = live.runtime.live_server.stats();
            let closed_links = live.runtime.live_server.recent_closed_link_summaries();
            let close_reasons = closed_links
                .iter()
                .map(|link| link.reason.as_str())
                .collect::<Vec<_>>();
            let mut lines = vec![
                monitoring_operator_summary_text(
                    Some(stats),
                    &live.last_interface_stats,
                    &live.recent_stats,
                    &close_reasons,
                ),
                self.announce_schedule_text(),
                String::new(),
                format!(
                    "destination: {}",
                    hex_lower_local(&live.runtime.destination_hash)
                ),
                format!("active links: {}", stats.active_links),
                format!("links opened: {}", stats.links_opened),
                format!("links closed: {}", stats.links_closed),
                format!(
                    "wire volume: {} received / {} sent",
                    human_bytes(stats.bytes_in),
                    human_bytes(stats.bytes_out)
                ),
                format!(
                    "resource volume: {} offered in {} resource(s)",
                    human_bytes(stats.resource_bytes_out),
                    stats.resources_offered
                ),
                format!("upload transfers: {}", upload_transfer_summary(stats)),
                format!(
                    "frames: {} in / {} out / {} categorized",
                    stats.frames_in,
                    stats.frames_out,
                    stats.traffic_in_frames()
                ),
                "recent activity:".into(),
                indent_lines(&live.recent_stats, "  "),
                "client request mix:".into(),
                format!("  session opens: {}", stats.session_requests_in),
                format!("  room navigation: {}", stats.room_navigation_in),
                format!("  chat messages/actions: {}", stats.chat_messages_in),
                format!("  history requests: {}", stats.history_requests_in),
                format!("  pings: {}", stats.pings_in),
                format!("  commands: {}", stats.commands_in),
                "problem counters:".into(),
                format!("  ignored context packets: {}", stats.ignored_packets),
                format!("  unknown link packets: {}", stats.unknown_link_packets),
                format!("  protocol errors: {}", stats.protocol_errors),
            ];
            if let Some(error) = stats.last_error.as_deref() {
                lines.push(format!("last error: {error}"));
            }
            lines.push(String::new());
            lines.push("active rooms:".into());
            let room_names = config::list_rooms(&self.config)
                .unwrap_or_default()
                .into_iter()
                .map(|(room_id, name, _)| (room_id, name))
                .collect::<BTreeMap<_, _>>();
            let room_counts = live.runtime.live_server.active_room_counts();
            if room_counts.is_empty() {
                lines.push("  none".into());
            } else {
                for (room_id, count) in room_counts {
                    let name = room_names
                        .get(&(room_id as i64))
                        .map(String::as_str)
                        .unwrap_or("unknown");
                    lines.push(format!("  #{name}: {count} link(s)"));
                }
            }
            lines.push(String::new());
            lines.push("active links:".into());
            lines.push(
                "  flags: high frames>=120/min, history>=20/min, ping>=30/min, upload>=10/min"
                    .into(),
            );
            let active_links = live.runtime.live_server.active_link_summaries();
            if active_links.is_empty() {
                lines.push("  none".into());
            } else {
                let now = current_unix_secs();
                for link in active_links {
                    let room = link
                        .room_id
                        .and_then(|room_id| room_names.get(&(room_id as i64)).cloned())
                        .map(|name| format!("#{name}"))
                        .unwrap_or_else(|| "no room".into());
                    lines.push(active_link_monitoring_line(&active_link_monitoring_text(
                        &link, &room, now,
                    )));
                }
            }
            lines.push(String::new());
            lines.push("recent closed links:".into());
            lines.push(format!("  {}", closed_link_churn_summary(&close_reasons)));
            if closed_links.is_empty() {
                lines.push("  none".into());
            } else {
                let now = current_unix_secs();
                for link in closed_links.iter() {
                    let room = link
                        .room_id
                        .and_then(|room_id| room_names.get(&(room_id as i64)).cloned())
                        .map(|name| format!("#{name}"))
                        .unwrap_or_else(|| "no room".into());
                    lines.push(closed_link_monitoring_line(&closed_link_monitoring_text(
                        link, &room, now,
                    )));
                }
            }
            lines.join("\n")
        }
        #[cfg(not(feature = "live-rns-net"))]
        {
            [
                "operator summary:\n  live monitoring unavailable; rebuild omenchatd with --features live-rns-net".to_string(),
                self.announce_schedule_text(),
            ]
            .join("\n")
        }
    }

    fn monitoring_interface_text(&self) -> String {
        #[cfg(feature = "live-rns-net")]
        {
            self.live
                .as_ref()
                .map(|live| live.last_interface_stats.join("\n"))
                .filter(|text| !text.trim().is_empty())
                .unwrap_or_else(|| "live server is stopped".into())
        }
        #[cfg(not(feature = "live-rns-net"))]
        {
            "live interface stats unavailable in this build".into()
        }
    }

    fn monitoring_log_text(&self, max_lines: usize) -> String {
        let path = self.config.log_path();
        read_log_tail(&path, max_lines)
            .ok()
            .filter(|text| !text.trim().is_empty())
            .unwrap_or_else(|| format!("No log entries yet.\n\n{}", path.display()))
    }

    fn render_body(&mut self, frame: &mut Frame<'_>, area: Rect) {
        self.action_clicks.clear();
        match self.tab {
            AdminTab::Overview => self.render_overview(frame, area),
            AdminTab::Setup => self.render_setup(frame, area),
            AdminTab::Rooms => self.render_rooms(frame, area),
            AdminTab::Moderation => self.render_moderation(frame, area),
            AdminTab::Monitoring => self.render_monitoring(frame, area),
            AdminTab::Audit => self.render_audit(frame, area),
            AdminTab::Identity => self.render_identity(frame, area),
            AdminTab::Interfaces => self.render_interfaces(frame, area),
            AdminTab::Portal => self.render_portal(frame, area),
            AdminTab::Logs => self.render_logs(frame, area),
            AdminTab::Help => self.render_help(frame, area),
        }
    }

    fn render_overview(&mut self, frame: &mut Frame<'_>, area: Rect) {
        let columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
            .split(area);
        let right = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(21),
                Constraint::Length(9),
                Constraint::Min(8),
            ])
            .split(columns[1]);
        let live_status = self.live_status_text();
        frame.render_widget(
            Paragraph::new(overview_operator_summary_text(&self.config, &live_status))
                .block(admin_block("Server Overview"))
                .wrap(Wrap { trim: false }),
            columns[0],
        );
        self.render_action_list(
            frame,
            right[0],
            "Operator Actions",
            &self.stateful_actions(&overview_action_specs()),
        );

        frame.render_widget(
            Paragraph::new(server_limits_text(&self.config))
                .block(admin_block("Limits"))
                .wrap(Wrap { trim: false }),
            right[1],
        );

        let setup_items = setup_checklist(&self.config);
        let setup_rows = setup_items
            .iter()
            .map(|item| {
                let marker = if item.ready { "[x]" } else { "[ ]" };
                ListItem::new(setup_checklist_line(
                    marker,
                    item,
                    inner_rect(right[2]).width as usize,
                ))
            })
            .collect::<Vec<_>>();
        frame.render_widget(
            List::new(setup_rows).block(admin_block("Setup Checklist")),
            right[2],
        );
    }

    fn render_setup(&mut self, frame: &mut Frame<'_>, area: Rect) {
        let columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(area);
        let right = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(8),
                Constraint::Length(8),
                Constraint::Length(SETUP_ACTION_PANEL_HEIGHT),
            ])
            .split(columns[1]);

        let setup_rows = setup_checklist(&self.config)
            .iter()
            .map(|item| {
                let marker = if item.ready { "[x]" } else { "[ ]" };
                ListItem::new(setup_checklist_line(
                    marker,
                    item,
                    inner_rect(columns[0]).width as usize,
                ))
            })
            .collect::<Vec<_>>();
        frame.render_widget(
            List::new(setup_rows).block(admin_block("First Run Checklist")),
            columns[0],
        );
        frame.render_widget(
            Paragraph::new(setup_next_steps_text(&self.config))
                .block(admin_block("Next Steps"))
                .wrap(Wrap { trim: false }),
            right[0],
        );
        frame.render_widget(
            Paragraph::new(setup_addresses_text(&self.config))
                .block(admin_block("Join Addresses"))
                .wrap(Wrap { trim: false }),
            right[1],
        );
        self.render_action_list(
            frame,
            right[2],
            "Setup Actions",
            &self.stateful_actions(&setup_action_specs()),
        );
    }

    fn render_rooms(&mut self, frame: &mut Frame<'_>, area: Rect) {
        let columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(48), Constraint::Percentage(52)])
            .split(area);
        self.room_list_area = inner_rect(columns[0]);
        let rooms = config::list_rooms(&self.config).unwrap_or_default();
        let room_list_width = self.room_list_area.width as usize;
        let items = rooms
            .iter()
            .enumerate()
            .map(|(index, (room_id, name, topic))| {
                let marker = if index == self.selected_room {
                    ">"
                } else {
                    " "
                };
                ListItem::new(room_list_label(
                    marker,
                    *room_id,
                    name,
                    topic.as_deref(),
                    room_list_width,
                ))
            })
            .collect::<Vec<_>>();
        frame.render_widget(List::new(items).block(admin_block("Rooms")), columns[0]);

        let right = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(7),
                Constraint::Length(8),
                Constraint::Length(7),
            ])
            .split(columns[1]);
        let selected = rooms.get(self.selected_room);
        let details = if let Some((room_id, name, topic)) = selected {
            selected_room_text(*room_id, name, topic.as_deref())
        } else {
            "No rooms yet.".to_string()
        };
        frame.render_widget(
            Paragraph::new(details)
                .block(admin_block("Selected Room"))
                .wrap(Wrap { trim: false }),
            right[0],
        );
        frame.render_widget(
            Paragraph::new(room_action_guide_text(
                selected.map(|(room_id, name, topic)| (*room_id, name.as_str(), topic.as_deref())),
                self.pending_archive_room_id,
            ))
            .block(admin_block("Action Guide"))
            .wrap(Wrap { trim: false }),
            right[1],
        );
        let actions = room_actions(
            selected.map(|(room_id, name, topic)| (*room_id, name.as_str(), topic.as_deref())),
            self.pending_archive_room_id,
        );
        self.render_action_list(frame, right[2], "Room Actions", &actions);
    }

    fn render_moderation(&mut self, frame: &mut Frame<'_>, area: Rect) {
        let columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(52), Constraint::Percentage(48)])
            .split(area);
        self.user_list_area = inner_rect(columns[0]);
        let users = list_known_users(&self.config).unwrap_or_default();
        let user_list_width = self.user_list_area.width as usize;
        let items = users
            .iter()
            .enumerate()
            .map(|(index, user)| {
                let marker = if index == self.selected_user {
                    ">"
                } else {
                    " "
                };
                let active_links = self.active_user_link_count(user);
                ListItem::new(moderation_user_list_label(
                    marker,
                    &moderation_user_text(user),
                    stale_user_age_secs(user),
                    USER_DELETE_MIN_AGE_SECS,
                    active_links,
                    user_list_width,
                ))
            })
            .collect::<Vec<_>>();
        frame.render_widget(
            List::new(items).block(admin_block("Known Users")),
            columns[0],
        );

        let right = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(8),
                Constraint::Length(9),
                Constraint::Length(10),
            ])
            .split(columns[1]);
        let selected_user = users.get(self.selected_user);
        let selected_active_links = selected_user
            .map(|user| self.active_user_link_count(user))
            .unwrap_or(0);
        let selected_user_text = selected_user.map(moderation_user_text);
        let details = if let Some(user_text) = selected_user_text.as_ref() {
            let stale_delete = selected_user
                .map(stale_delete_status_label)
                .unwrap_or_else(|| "unavailable".into());
            moderation_selected_user_text(user_text, selected_active_links, &stale_delete)
        } else {
            "No users have connected yet.\n\nOnce users join through OMENchat, they will appear here for moderation.".into()
        };
        frame.render_widget(
            Paragraph::new(details)
                .block(admin_block("Selected User"))
                .wrap(Wrap { trim: false }),
            right[0],
        );
        frame.render_widget(
            Paragraph::new(moderation_action_guide_text(
                selected_user_text.as_ref(),
                selected_active_links,
                self.pending_delete_user_id,
                self.pending_prune_stale_users,
            ))
            .block(admin_block("Action Guide"))
            .wrap(Wrap { trim: false }),
            right[1],
        );
        let actions = moderation_actions(
            users.get(self.selected_user),
            self.pending_delete_user_id,
            self.pending_prune_stale_users,
        );
        self.render_action_list(frame, right[2], "User Actions", &actions);
    }

    fn render_action_list(
        &mut self,
        frame: &mut Frame<'_>,
        area: Rect,
        title: &str,
        actions: &[(AdminAction, String)],
    ) {
        let content = inner_rect(area);
        for (hitbox, action) in action_hitboxes(content, actions) {
            self.action_clicks.push((hitbox, action));
        }
        let action_width = content.width as usize;
        let items = actions
            .iter()
            .map(|(_, label)| ListItem::new(action_list_label(label, action_width)))
            .collect::<Vec<_>>();
        frame.render_widget(List::new(items).block(admin_block(title)), area);
    }

    fn render_monitoring(&self, frame: &mut Frame<'_>, area: Rect) {
        let columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(42), Constraint::Percentage(58)])
            .split(area);
        let right = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(12), Constraint::Min(6)])
            .split(columns[1]);

        frame.render_widget(
            Paragraph::new(self.monitoring_counter_text())
                .block(admin_block("Server Health"))
                .wrap(Wrap { trim: false }),
            columns[0],
        );
        frame.render_widget(
            Paragraph::new(self.monitoring_interface_text())
                .block(admin_block("Interface Status"))
                .wrap(Wrap { trim: false }),
            right[0],
        );
        frame.render_widget(
            Paragraph::new(self.monitoring_log_text(right[1].height.saturating_sub(4) as usize))
                .block(admin_block("Runtime Log Tail"))
                .wrap(Wrap { trim: false }),
            right[1],
        );
    }

    fn render_audit(&self, frame: &mut Frame<'_>, area: Rect) {
        let path = self.config.log_path();
        let max_lines = area.height.saturating_sub(4) as usize;
        frame.render_widget(
            Paragraph::new(audit_panel_text(&path, max_lines))
                .block(admin_block("Admin Action History"))
                .wrap(Wrap { trim: false }),
            area,
        );
    }

    fn render_identity(&self, frame: &mut Frame<'_>, area: Rect) {
        frame.render_widget(
            Paragraph::new(identity_panel_text(&self.config))
                .block(admin_block("Identity And Storage"))
                .wrap(Wrap { trim: false }),
            area,
        );
    }

    fn render_interfaces(&mut self, frame: &mut Frame<'_>, area: Rect) {
        let config_path = self.config.reticulum_config_file();
        let preview = std::fs::read_to_string(&config_path)
            .unwrap_or_else(|_| "No Reticulum interface config written yet.".into());
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(7),
                Constraint::Length(9),
                Constraint::Min(6),
            ])
            .split(area);
        let actions = [
            (AdminAction::EditTcpClient, "Connect To Gateway"),
            (AdminAction::EditTcpServer, "Local TCP Listener"),
            (AdminAction::StartLive, "Start Live"),
            (AdminAction::AnnounceNow, "Announce Now"),
            (AdminAction::SelectTab(AdminTab::Monitoring), "Monitoring"),
            (AdminAction::StopLive, "Stop Live"),
            (AdminAction::SaveConfig, "Save Config"),
        ];
        self.render_action_list(
            frame,
            rows[0],
            "Interface Actions",
            &self.stateful_actions(&actions),
        );
        frame.render_widget(
            Paragraph::new(interface_operator_summary_text(&preview, &config_path))
                .block(admin_block("Interface Summary"))
                .wrap(Wrap { trim: false }),
            rows[1],
        );
        frame.render_widget(
            Paragraph::new(preview)
                .block(admin_block("Reticulum Config Preview"))
                .wrap(Wrap { trim: false }),
            rows[2],
        );
    }

    fn render_portal(&mut self, frame: &mut Frame<'_>, area: Rect) {
        let columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(45), Constraint::Percentage(55)])
            .split(area);
        let left = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(10), Constraint::Length(6)])
            .split(columns[0]);
        frame.render_widget(
            Paragraph::new(portal_panel_text(&self.config))
                .block(admin_block("Public Portal"))
                .wrap(Wrap { trim: false }),
            left[0],
        );
        let actions = [
            (AdminAction::EditMotd, "Edit Server MOTD"),
            (AdminAction::AnnounceNow, "Announce Now"),
            (AdminAction::SelectTab(AdminTab::Rooms), "Rooms"),
            (AdminAction::SelectTab(AdminTab::Identity), "Identity"),
        ];
        self.render_action_list(
            frame,
            left[1],
            "Portal Actions",
            &self.stateful_actions(&actions),
        );

        let page = std::fs::read_to_string(self.config.nomadnet_index_page_path())
            .unwrap_or_else(|_| "No portal page exists yet. Start the live server once, or run status/doctor with live-rns-net support, to create the first template after the OMENchat destination hash is available.".into());
        frame.render_widget(
            Paragraph::new(page)
                .block(admin_block("reticulum/storage/pages/index.mu"))
                .wrap(Wrap { trim: false }),
            columns[1],
        );
    }

    fn render_logs(&self, frame: &mut Frame<'_>, area: Rect) {
        let path = self.config.log_path();
        frame.render_widget(
            Paragraph::new(log_panel_text(
                &path,
                area.height.saturating_sub(4) as usize,
            ))
            .block(admin_block("Live Logs"))
            .wrap(Wrap { trim: false }),
            area,
        );
    }

    fn render_help(&self, frame: &mut Frame<'_>, area: Rect) {
        frame.render_widget(
            Paragraph::new(admin_help_text())
                .block(admin_block("Help - wheel/Up/Down scroll"))
                .scroll((self.help_scroll, 0))
                .wrap(Wrap { trim: false }),
            area,
        );
    }

    fn render_status(&self, frame: &mut Frame<'_>, area: Rect) {
        let text = format!(
            "{} | {} | {}",
            self.input_mode.prompt(),
            self.config.root_dir().display(),
            self.status
        );
        frame.render_widget(Paragraph::new(text).block(admin_block("Status")), area);
    }

    fn render_input_popup(&self, frame: &mut Frame<'_>, area: Rect) {
        frame.render_widget(Clear, area);
        let title = self.input_mode.prompt();
        let text = format!("{}\n\n{}", title, self.input);
        frame.render_widget(
            Paragraph::new(text)
                .block(admin_block("Input"))
                .wrap(Wrap { trim: false }),
            area,
        );
    }

    fn handle_key(&mut self, key: KeyEvent) -> ServerResult<bool> {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            return Ok(true);
        }
        if self.input_mode != InputMode::Navigate {
            self.handle_input_key(key)?;
            return Ok(false);
        }
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => Ok(true),
            KeyCode::Tab | KeyCode::Right => {
                self.next_tab();
                Ok(false)
            }
            KeyCode::BackTab | KeyCode::Left => {
                self.previous_tab();
                Ok(false)
            }
            KeyCode::Up => {
                if self.tab == AdminTab::Help {
                    self.scroll_help(false, 1);
                } else {
                    self.select_previous_item();
                }
                Ok(false)
            }
            KeyCode::Down => {
                if self.tab == AdminTab::Help {
                    self.scroll_help(true, 1);
                } else {
                    self.select_next_item();
                }
                Ok(false)
            }
            KeyCode::PageUp => {
                if self.tab == AdminTab::Help {
                    self.scroll_help(false, 8);
                }
                Ok(false)
            }
            KeyCode::PageDown => {
                if self.tab == AdminTab::Help {
                    self.scroll_help(true, 8);
                }
                Ok(false)
            }
            KeyCode::Home => {
                if self.tab == AdminTab::Help {
                    self.help_scroll = 0;
                    self.status = "help scrolled to top".into();
                }
                Ok(false)
            }
            KeyCode::End => {
                if self.tab == AdminTab::Help {
                    self.help_scroll = self.max_help_scroll();
                    self.status = "help scrolled to bottom".into();
                }
                Ok(false)
            }
            KeyCode::Enter => {
                self.start_primary_edit();
                Ok(false)
            }
            KeyCode::Char('n') => {
                self.start_input(InputMode::AddRoomName, String::new());
                Ok(false)
            }
            KeyCode::Char('t') => {
                self.start_selected_room_topic_edit();
                Ok(false)
            }
            KeyCode::Char('d') => {
                if self.tab == AdminTab::Moderation {
                    self.delete_selected_stale_user()?;
                } else {
                    self.archive_selected_room()?;
                }
                Ok(false)
            }
            KeyCode::Char('r') => {
                self.select_tab(AdminTab::Rooms);
                Ok(false)
            }
            KeyCode::Char('o') => {
                self.start_input(InputMode::EditOperator, self.config.operator_label.clone());
                Ok(false)
            }
            KeyCode::Char('a') => {
                self.start_input(InputMode::EditMotd, self.config.motd.clone());
                Ok(false)
            }
            KeyCode::Char('v') => {
                self.start_input(
                    InputMode::EditAnnounceInterval,
                    self.config.announce_interval_minutes.to_string(),
                );
                Ok(false)
            }
            KeyCode::Char('m') => {
                self.select_tab(AdminTab::Moderation);
                Ok(false)
            }
            KeyCode::Char('c') => {
                self.select_tab(AdminTab::Monitoring);
                Ok(false)
            }
            KeyCode::Char('y') => {
                self.select_tab(AdminTab::Audit);
                Ok(false)
            }
            KeyCode::Char('i') => {
                self.start_input(InputMode::EditTcpServer, "127.0.0.1:42420".into());
                Ok(false)
            }
            KeyCode::Char('w') => {
                self.start_input(InputMode::EditTcpClient, "gateway.example:42420".into());
                Ok(false)
            }
            KeyCode::Char('g') => {
                self.start_live_runtime();
                Ok(false)
            }
            KeyCode::Char('x') => {
                self.stop_live_runtime();
                Ok(false)
            }
            KeyCode::Char('l') => {
                self.select_tab(AdminTab::Logs);
                Ok(false)
            }
            KeyCode::Char('b') => {
                self.toggle_selected_user_ban()?;
                Ok(false)
            }
            KeyCode::Char('k') => {
                self.kick_selected_user_links()?;
                Ok(false)
            }
            KeyCode::Char('e') => {
                self.toggle_selected_user_mute()?;
                Ok(false)
            }
            KeyCode::Char('u') => {
                self.toggle_selected_user_trust()?;
                Ok(false)
            }
            KeyCode::Char('p') => {
                self.cycle_selected_user_role()?;
                Ok(false)
            }
            KeyCode::Char('s') => {
                self.save_config_with_status()?;
                Ok(false)
            }
            _ => Ok(false),
        }
    }

    fn handle_input_key(&mut self, key: KeyEvent) -> ServerResult<()> {
        match key.code {
            KeyCode::Esc => {
                self.input_mode = InputMode::Navigate;
                self.input.clear();
                self.pending_room_name.clear();
                self.status = "edit cancelled".into();
            }
            KeyCode::Enter => self.commit_input()?,
            KeyCode::Backspace => {
                self.input.pop();
            }
            KeyCode::Char(ch) => {
                self.input.push(ch);
            }
            _ => {}
        }
        Ok(())
    }

    fn handle_mouse(&mut self, kind: MouseEventKind, column: u16, row: u16) -> ServerResult<()> {
        match kind {
            MouseEventKind::Down(MouseButton::Left) => self.handle_click(column, row),
            MouseEventKind::ScrollDown => {
                self.handle_scroll(column, row, true);
                Ok(())
            }
            MouseEventKind::ScrollUp => {
                self.handle_scroll(column, row, false);
                Ok(())
            }
            _ => Ok(()),
        }
    }

    fn handle_scroll(&mut self, column: u16, row: u16, down: bool) {
        if self.tab == AdminTab::Help {
            self.scroll_help(down, 3);
            return;
        }
        let position = Position::new(column, row);
        let selectable = (self.tab == AdminTab::Rooms && self.room_list_area.contains(position))
            || (self.tab == AdminTab::Moderation && self.user_list_area.contains(position));
        if !selectable {
            return;
        }
        if down {
            self.select_next_item();
        } else {
            self.select_previous_item();
        }
    }

    fn max_help_scroll(&self) -> u16 {
        admin_help_text()
            .lines()
            .count()
            .saturating_sub(1)
            .min(u16::MAX as usize) as u16
    }

    fn scroll_help(&mut self, down: bool, amount: u16) {
        let max_scroll = self.max_help_scroll();
        self.help_scroll = if down {
            self.help_scroll.saturating_add(amount).min(max_scroll)
        } else {
            self.help_scroll.saturating_sub(amount)
        };
        self.status = format!("help scroll line {}", self.help_scroll);
    }

    fn handle_click(&mut self, column: u16, row: u16) -> ServerResult<()> {
        let position = Position::new(column, row);
        if let Some((_, tab)) = self
            .tab_clicks
            .iter()
            .find(|(area, _)| area.contains(position))
        {
            self.select_tab(*tab);
            return Ok(());
        }
        if let Some((_, action)) = self
            .action_clicks
            .iter()
            .find(|(area, _)| area.contains(position))
            .copied()
        {
            return self.handle_admin_action(action);
        }
        if self.tab == AdminTab::Rooms && self.room_list_area.contains(position) {
            let rooms = config::list_rooms(&self.config).unwrap_or_default();
            if let Some(row_index) = list_row_at(self.room_list_area, row, rooms.len()) {
                self.select_room_index(row_index, &rooms);
            }
            return Ok(());
        }
        if self.tab == AdminTab::Moderation && self.user_list_area.contains(position) {
            let users = list_known_users(&self.config).unwrap_or_default();
            if let Some(row_index) = list_row_at(self.user_list_area, row, users.len()) {
                self.select_user_index(row_index, &users);
            }
        }
        Ok(())
    }

    fn handle_admin_action(&mut self, action: AdminAction) -> ServerResult<()> {
        match action {
            AdminAction::StartLive => self.start_live_runtime(),
            AdminAction::StopLive => self.stop_live_runtime(),
            AdminAction::AnnounceNow => self.announce_live_now(),
            AdminAction::EditServerName => {
                self.start_input(InputMode::EditName, self.config.name.clone())
            }
            AdminAction::EditOperator => {
                self.start_input(InputMode::EditOperator, self.config.operator_label.clone())
            }
            AdminAction::EditMotd => {
                self.start_input(InputMode::EditMotd, self.config.motd.clone())
            }
            AdminAction::EditAnnounceInterval => self.start_input(
                InputMode::EditAnnounceInterval,
                self.config.announce_interval_minutes.to_string(),
            ),
            AdminAction::EditUploadQuotaBytes => self.start_input(
                InputMode::EditUploadQuotaBytes,
                self.config.upload_quota_bytes.to_string(),
            ),
            AdminAction::EditUploadMaxFileBytes => self.start_input(
                InputMode::EditUploadMaxFileBytes,
                self.config.upload_max_file_bytes.to_string(),
            ),
            AdminAction::EditPingIntervalSeconds => self.start_input(
                InputMode::EditPingIntervalSeconds,
                self.config.ping_interval_seconds.to_string(),
            ),
            AdminAction::EditMaxMessageBytes => self.start_input(
                InputMode::EditMaxMessageBytes,
                self.config.limits.max_message_bytes.to_string(),
            ),
            AdminAction::EditHistoryBatchSize => self.start_input(
                InputMode::EditHistoryBatchSize,
                self.config.limits.history_batch_size.to_string(),
            ),
            AdminAction::EditJoinBacklogEvents => self.start_input(
                InputMode::EditJoinBacklogEvents,
                self.config.limits.join_backlog_events.to_string(),
            ),
            AdminAction::EditLargeBatchThresholdBytes => self.start_input(
                InputMode::EditLargeBatchThresholdBytes,
                self.config.limits.large_batch_threshold_bytes.to_string(),
            ),
            AdminAction::EditMessageRate => self.start_input(
                InputMode::EditMessageRate,
                self.config.limits.rate_messages_per_minute.to_string(),
            ),
            AdminAction::EditCommandRate => self.start_input(
                InputMode::EditCommandRate,
                self.config.limits.rate_commands_per_minute.to_string(),
            ),
            AdminAction::EditTcpServer => {
                self.start_input(InputMode::EditTcpServer, "127.0.0.1:42420".into())
            }
            AdminAction::EditTcpClient => {
                self.start_input(InputMode::EditTcpClient, "gateway.example:42420".into())
            }
            AdminAction::SelectTab(tab) => self.select_tab(tab),
            AdminAction::SaveConfig => self.save_config_with_status()?,
            AdminAction::AddRoom => self.start_input(InputMode::AddRoomName, String::new()),
            AdminAction::EditRoomTopic => self.start_selected_room_topic_edit(),
            AdminAction::ArchiveRoom => self.archive_selected_room()?,
            AdminAction::ToggleBan => self.toggle_selected_user_ban()?,
            AdminAction::KickActiveUser => self.kick_selected_user_links()?,
            AdminAction::ToggleMute => self.toggle_selected_user_mute()?,
            AdminAction::ToggleTrust => self.toggle_selected_user_trust()?,
            AdminAction::SetRole(role_bits) => self.set_selected_user_role(role_bits)?,
            AdminAction::DeleteStaleUser => self.delete_selected_stale_user()?,
            AdminAction::PruneStaleUsers => self.prune_stale_user_records()?,
        }
        Ok(())
    }

    fn start_primary_edit(&mut self) {
        match self.tab {
            AdminTab::Overview => self.start_input(InputMode::EditName, self.config.name.clone()),
            AdminTab::Setup => {
                self.start_input(InputMode::EditTcpClient, "gateway.example:42420".into())
            }
            AdminTab::Rooms => self.start_input(InputMode::AddRoomName, String::new()),
            AdminTab::Moderation => self.toggle_selected_user_ban_status(),
            AdminTab::Monitoring => self.status = "monitoring updates while the server runs".into(),
            AdminTab::Audit => {
                self.status = format!("reading {}", self.config.log_path().display())
            }
            AdminTab::Identity => self.status = "identity paths are informational here".into(),
            AdminTab::Interfaces => {
                self.start_input(InputMode::EditTcpClient, "gateway.example:42420".into())
            }
            AdminTab::Portal => self.start_input(InputMode::EditMotd, self.config.motd.clone()),
            AdminTab::Logs => self.status = format!("tailing {}", self.config.log_path().display()),
            AdminTab::Help => {}
        }
    }

    fn start_selected_room_topic_edit(&mut self) {
        if self.tab != AdminTab::Rooms {
            self.status = "select the Rooms panel before editing a room topic".into();
            return;
        }
        let rooms = config::list_rooms(&self.config).unwrap_or_default();
        let Some((_, _, topic)) = rooms.get(self.selected_room) else {
            self.status = "no room selected".into();
            return;
        };
        self.start_input(InputMode::EditRoomTopic, topic.clone().unwrap_or_default());
    }

    fn archive_selected_room(&mut self) -> ServerResult<()> {
        if self.tab != AdminTab::Rooms {
            self.status = "select the Rooms panel before archiving a room".into();
            return Ok(());
        }
        let rooms = config::list_rooms(&self.config).unwrap_or_default();
        let Some((room_id, name, _)) = rooms.get(self.selected_room) else {
            self.status = "no room selected".into();
            return Ok(());
        };
        if *room_id == 1 {
            self.pending_archive_room_id = None;
            self.status =
                "#lobby is protected: default room stays visible and cannot be archived".into();
            return Ok(());
        }
        if self.pending_archive_room_id != Some(*room_id) {
            self.pending_archive_room_id = Some(*room_id);
            self.status = format!(
                "archive armed for #{name}: confirm to hide it from clients; history stays stored"
            );
            return Ok(());
        }
        self.pending_archive_room_id = None;
        config::archive_room(&self.config, *room_id)?;
        append_admin_log(
            &self.config,
            format!("admin archived room id={room_id} name={name}"),
        );
        self.selected_room = self.selected_room.saturating_sub(1);
        self.status = format!("archived #{name}: hidden from room lists; history was retained");
        Ok(())
    }

    fn start_input(&mut self, mode: InputMode, value: String) {
        self.input_mode = mode;
        self.input = value;
        self.status = "editing".into();
    }

    fn save_config_with_status(&mut self) -> ServerResult<()> {
        self.config.save()?;
        self.status =
            "config saved; restart live server only if network or limit settings changed".into();
        Ok(())
    }

    fn commit_input(&mut self) -> ServerResult<()> {
        let value = self.input.trim().to_owned();
        match self.input_mode {
            InputMode::Navigate => {}
            InputMode::EditName => {
                if !value.is_empty() {
                    self.config.name = value;
                    self.config.save()?;
                    append_admin_log(&self.config, "admin updated server name");
                    self.status =
                        "server name saved: clients will see it on new session metadata".into();
                }
                self.input_mode = InputMode::Navigate;
            }
            InputMode::EditOperator => {
                if !value.is_empty() {
                    self.config.operator_label = value;
                    self.config.save()?;
                    append_admin_log(&self.config, "admin updated operator label");
                    self.status = "operator label saved: status/setup output now uses it".into();
                }
                self.input_mode = InputMode::Navigate;
            }
            InputMode::EditMotd => {
                self.config.motd = value;
                self.config.save()?;
                append_admin_log(&self.config, "admin updated server MOTD");
                self.status = "server MOTD saved; new OMENchat sessions will receive it".into();
                self.input_mode = InputMode::Navigate;
            }
            InputMode::EditAnnounceInterval => {
                let minutes = value.parse::<u64>().map_err(|_| {
                    ServerError::Message("announce interval must be whole minutes".into())
                })?;
                self.config.announce_interval_minutes = minutes.max(1);
                self.config.save()?;
                append_admin_log(
                    &self.config,
                    format!(
                        "admin updated announce interval minutes={}",
                        self.config.announce_interval_minutes
                    ),
                );
                self.status = format!(
                    "announce interval saved: {} minute(s)",
                    self.config.announce_interval_minutes
                );
                self.input_mode = InputMode::Navigate;
            }
            InputMode::EditUploadQuotaBytes => {
                let bytes = value
                    .parse::<u64>()
                    .map_err(|_| ServerError::Message("upload quota must be bytes".into()))?;
                self.config.upload_quota_bytes = bytes.min(10 * 1024 * 1024 * 1024);
                self.config.save()?;
                append_admin_log(
                    &self.config,
                    format!(
                        "admin updated upload quota bytes={}",
                        self.config.upload_quota_bytes
                    ),
                );
                self.status = if self.config.upload_quota_bytes == 0 {
                    "upload quota saved: uploads disabled".into()
                } else {
                    format!(
                        "upload quota saved: {}",
                        human_bytes(self.config.upload_quota_bytes)
                    )
                };
                self.input_mode = InputMode::Navigate;
            }
            InputMode::EditUploadMaxFileBytes => {
                let bytes = value
                    .parse::<u64>()
                    .map_err(|_| ServerError::Message("upload max file must be bytes".into()))?;
                self.config.upload_max_file_bytes = bytes.clamp(1, 10 * 1024 * 1024);
                self.config.save()?;
                append_admin_log(
                    &self.config,
                    format!(
                        "admin updated upload max file bytes={}",
                        self.config.upload_max_file_bytes
                    ),
                );
                self.status = format!(
                    "upload max file saved: {}",
                    human_bytes(self.config.upload_max_file_bytes)
                );
                self.input_mode = InputMode::Navigate;
            }
            InputMode::EditPingIntervalSeconds => {
                let seconds = value.parse::<u64>().map_err(|_| {
                    ServerError::Message("ping interval must be whole seconds".into())
                })?;
                self.config.ping_interval_seconds = seconds.clamp(5, 600);
                self.config.save()?;
                append_admin_log(
                    &self.config,
                    format!(
                        "admin updated ping interval seconds={}",
                        self.config.ping_interval_seconds
                    ),
                );
                self.status = format!(
                    "ping interval saved: {} second(s); new client sessions will receive it",
                    self.config.ping_interval_seconds
                );
                self.input_mode = InputMode::Navigate;
            }
            InputMode::EditMaxMessageBytes => {
                let bytes = parse_limit_input(&value, "max message bytes")?;
                self.config.limits.max_message_bytes = bytes.clamp(1, 262_144);
                self.config.save()?;
                append_admin_log(
                    &self.config,
                    format!(
                        "admin updated max message bytes={}",
                        self.config.limits.max_message_bytes
                    ),
                );
                self.status = max_message_bytes_update_text(self.config.limits.max_message_bytes);
                self.input_mode = InputMode::Navigate;
            }
            InputMode::EditHistoryBatchSize => {
                let count = parse_limit_input(&value, "history batch size")?;
                self.config.limits.history_batch_size = count.clamp(1, 500);
                self.config.save()?;
                append_admin_log(
                    &self.config,
                    format!(
                        "admin updated history batch size={}",
                        self.config.limits.history_batch_size
                    ),
                );
                self.status = format!(
                    "history batch size saved: {}",
                    self.config.limits.history_batch_size
                );
                self.input_mode = InputMode::Navigate;
            }
            InputMode::EditJoinBacklogEvents => {
                let count = parse_limit_input(&value, "join backlog events")?;
                self.config.limits.join_backlog_events = count.clamp(0, 500);
                self.config.save()?;
                append_admin_log(
                    &self.config,
                    format!(
                        "admin updated join backlog events={}",
                        self.config.limits.join_backlog_events
                    ),
                );
                self.status = format!(
                    "join backlog events saved: {}",
                    self.config.limits.join_backlog_events
                );
                self.input_mode = InputMode::Navigate;
            }
            InputMode::EditLargeBatchThresholdBytes => {
                let bytes = parse_limit_input(&value, "large batch threshold bytes")?;
                self.config.limits.large_batch_threshold_bytes = bytes.clamp(256, 1_048_576);
                self.config.save()?;
                append_admin_log(
                    &self.config,
                    format!(
                        "admin updated large batch threshold bytes={}",
                        self.config.limits.large_batch_threshold_bytes
                    ),
                );
                self.status = large_batch_threshold_update_text(
                    self.config.limits.large_batch_threshold_bytes,
                );
                self.input_mode = InputMode::Navigate;
            }
            InputMode::EditMessageRate => {
                let count = parse_limit_input(&value, "message rate")?;
                self.config.limits.rate_messages_per_minute = count.min(600);
                self.config.save()?;
                append_admin_log(
                    &self.config,
                    format!(
                        "admin updated message rate per minute={}",
                        self.config.limits.rate_messages_per_minute
                    ),
                );
                self.status = format!(
                    "message rate saved: {} per minute",
                    self.config.limits.rate_messages_per_minute
                );
                self.input_mode = InputMode::Navigate;
            }
            InputMode::EditCommandRate => {
                let count = parse_limit_input(&value, "command rate")?;
                self.config.limits.rate_commands_per_minute = count.min(600);
                self.config.save()?;
                append_admin_log(
                    &self.config,
                    format!(
                        "admin updated command rate per minute={}",
                        self.config.limits.rate_commands_per_minute
                    ),
                );
                self.status = format!(
                    "command rate saved: {} per minute",
                    self.config.limits.rate_commands_per_minute
                );
                self.input_mode = InputMode::Navigate;
            }
            InputMode::EditTcpServer => {
                let tcp_server = parse_tcp_server_override(&value)
                    .ok_or_else(|| ServerError::Message("invalid listen_ip:port".into()))?;
                config::write_reticulum_tcp_server_config(&self.config, &tcp_server)?;
                append_admin_log(
                    &self.config,
                    format!(
                        "admin wrote TCPServerInterface listen={}:{}",
                        tcp_server.listen_ip, tcp_server.listen_port
                    ),
                );
                self.status = format!(
                    "local listener saved: {}:{}; restart live server to bind it",
                    tcp_server.listen_ip, tcp_server.listen_port
                );
                self.input_mode = InputMode::Navigate;
            }
            InputMode::EditTcpClient => {
                let tcp_client = parse_tcp_client_override(&value)
                    .ok_or_else(|| ServerError::Message("invalid gateway host:port".into()))?;
                config::write_reticulum_tcp_client_config(&self.config, &tcp_client)?;
                append_admin_log(
                    &self.config,
                    format!(
                        "admin wrote TCPClientInterface target={}:{}",
                        tcp_client.target_host, tcp_client.target_port
                    ),
                );
                self.status = format!(
                    "gateway saved: {}:{}; restart live server, then check Monitoring",
                    tcp_client.target_host, tcp_client.target_port
                );
                self.input_mode = InputMode::Navigate;
            }
            InputMode::EditRoomTopic => {
                let rooms = config::list_rooms(&self.config).unwrap_or_default();
                let Some((room_id, name, _)) = rooms.get(self.selected_room) else {
                    self.status = "no room selected".into();
                    self.input_mode = InputMode::Navigate;
                    self.input.clear();
                    return Ok(());
                };
                let topic = (!value.is_empty()).then_some(value);
                config::update_room_topic(&self.config, *room_id, topic.as_deref())?;
                append_admin_log(
                    &self.config,
                    format!("admin updated room topic id={room_id} name={name}"),
                );
                self.status = format!("updated topic for #{name}: clients will see it on sync");
                self.input_mode = InputMode::Navigate;
            }
            InputMode::AddRoomName => {
                if !value.is_empty() {
                    self.pending_room_name = value;
                    self.start_input(InputMode::AddRoomTopic, String::new());
                }
            }
            InputMode::AddRoomTopic => {
                let topic = (!value.is_empty()).then_some(value);
                config::add_room(&self.config, &self.pending_room_name, topic.as_deref())?;
                append_admin_log(
                    &self.config,
                    format!(
                        "admin added room name={}",
                        self.pending_room_name.trim().trim_start_matches('#')
                    ),
                );
                self.status = format!(
                    "room ready: #{} is visible to clients; mods/admins can edit its topic",
                    self.pending_room_name.trim().trim_start_matches('#')
                );
                self.pending_room_name.clear();
                self.input_mode = InputMode::Navigate;
                self.input.clear();
                self.pending_archive_room_id = None;
                self.pending_delete_user_id = None;
                self.pending_prune_stale_users = false;
                self.tab = AdminTab::Rooms;
            }
        }
        if self.input_mode == InputMode::Navigate {
            self.input.clear();
        }
        Ok(())
    }

    fn next_tab(&mut self) {
        let current = AdminTab::ALL
            .iter()
            .position(|tab| *tab == self.tab)
            .unwrap_or(0);
        self.select_tab(AdminTab::ALL[(current + 1) % AdminTab::ALL.len()]);
    }

    fn previous_tab(&mut self) {
        let current = AdminTab::ALL
            .iter()
            .position(|tab| *tab == self.tab)
            .unwrap_or(0);
        self.select_tab(AdminTab::ALL[(current + AdminTab::ALL.len() - 1) % AdminTab::ALL.len()]);
    }

    fn select_tab(&mut self, tab: AdminTab) {
        if self.tab != tab {
            self.pending_archive_room_id = None;
            self.pending_delete_user_id = None;
            self.pending_prune_stale_users = false;
        }
        self.tab = tab;
        self.status = format!("selected {}", self.tab.title());
    }

    fn select_next_item(&mut self) {
        if self.tab == AdminTab::Moderation {
            self.select_next_user();
            return;
        }
        let room_count = config::list_rooms(&self.config)
            .map(|rooms| {
                let next = (self.selected_room + 1).min(rooms.len().saturating_sub(1));
                self.select_room_index(next, &rooms);
                rooms.len()
            })
            .unwrap_or(0);
        if room_count == 0 {
            self.status = "no rooms to select".into();
        }
    }

    fn select_previous_item(&mut self) {
        if self.tab == AdminTab::Moderation {
            self.select_previous_user();
            return;
        }
        let rooms = config::list_rooms(&self.config).unwrap_or_default();
        if rooms.is_empty() {
            self.status = "no rooms to select".into();
            return;
        }
        let previous = self.selected_room.saturating_sub(1);
        self.select_room_index(previous, &rooms);
    }

    fn select_next_user(&mut self) {
        let users = list_known_users(&self.config).unwrap_or_default();
        if users.is_empty() {
            self.status = "no users to select".into();
            return;
        }
        let next = (self.selected_user + 1).min(users.len() - 1);
        self.select_user_index(next, &users);
    }

    fn select_previous_user(&mut self) {
        let users = list_known_users(&self.config).unwrap_or_default();
        if users.is_empty() {
            self.status = "no users to select".into();
            return;
        }
        let previous = self.selected_user.saturating_sub(1);
        self.select_user_index(previous, &users);
    }

    fn select_room_index(&mut self, index: usize, rooms: &[(i64, String, Option<String>)]) {
        let Some((room_id, name, _)) = rooms.get(index) else {
            return;
        };
        self.selected_room = index;
        self.pending_archive_room_id = None;
        self.status = format!("selected room #{name} (id {room_id})");
    }

    fn select_user_index(&mut self, index: usize, users: &[AdminUserRow]) {
        let Some(user) = users.get(index) else {
            return;
        };
        if index != self.selected_user {
            self.clear_pending_user_delete();
        } else {
            self.pending_prune_stale_users = false;
        }
        self.selected_user = index;
        self.status = format!(
            "selected user {} ({})",
            user.display_name,
            compact_identity(&user.identity_hex)
        );
    }

    fn clear_pending_user_delete(&mut self) {
        self.pending_delete_user_id = None;
        self.pending_prune_stale_users = false;
    }

    fn toggle_selected_user_ban_status(&mut self) {
        if let Err(error) = self.toggle_selected_user_ban() {
            self.status = format!("moderation failed: {error}");
        }
    }

    fn toggle_selected_user_ban(&mut self) -> ServerResult<()> {
        if self.tab != AdminTab::Moderation {
            return Ok(());
        }
        let Some(user) = list_known_users(&self.config)?
            .get(self.selected_user)
            .cloned()
        else {
            self.status = "no user selected".into();
            return Ok(());
        };
        self.clear_pending_user_delete();
        set_user_flag(&self.config, user.user_id, STATUS_BANNED, !user.banned)?;
        self.status = if user.banned {
            append_admin_log(
                &self.config,
                format!(
                    "admin unbanned user id={} name={}",
                    user.user_id, user.display_name
                ),
            );
            format!("{} unbanned; future sessions allowed", user.display_name)
        } else {
            let disconnected = self.disconnect_live_user(&user);
            append_admin_log(
                &self.config,
                format!(
                    "admin banned user id={} name={} active_links_closed={disconnected}",
                    user.user_id, user.display_name
                ),
            );
            if disconnected > 0 {
                format!(
                    "{} banned; closed {disconnected} active link(s); future sessions blocked",
                    user.display_name
                )
            } else {
                format!("{} banned; future sessions blocked", user.display_name)
            }
        };
        Ok(())
    }

    #[cfg(feature = "live-rns-net")]
    fn disconnect_live_user(&mut self, user: &AdminUserRow) -> usize {
        self.live
            .as_mut()
            .map(|live| {
                live.runtime
                    .live_server
                    .disconnect_identity(&user.identity_hash)
            })
            .unwrap_or(0)
    }

    #[cfg(not(feature = "live-rns-net"))]
    fn disconnect_live_user(&mut self, _user: &AdminUserRow) -> usize {
        0
    }

    fn toggle_selected_user_trust(&mut self) -> ServerResult<()> {
        if self.tab != AdminTab::Moderation {
            return Ok(());
        }
        let Some(user) = list_known_users(&self.config)?
            .get(self.selected_user)
            .cloned()
        else {
            self.status = "no user selected".into();
            return Ok(());
        };
        self.clear_pending_user_delete();
        set_user_role_flag(&self.config, user.user_id, ROLE_TRUSTED, !user.trusted)?;
        self.status = if user.trusted {
            append_admin_log(
                &self.config,
                format!(
                    "admin untrusted user id={} name={}",
                    user.user_id, user.display_name
                ),
            );
            format!(
                "{} untrusted; trusted-media affordances removed",
                user.display_name
            )
        } else {
            append_admin_log(
                &self.config,
                format!(
                    "admin trusted user id={} name={}",
                    user.user_id, user.display_name
                ),
            );
            format!(
                "{} trusted; trusted-media affordances enabled",
                user.display_name
            )
        };
        Ok(())
    }

    fn kick_selected_user_links(&mut self) -> ServerResult<()> {
        if self.tab != AdminTab::Moderation {
            return Ok(());
        }
        let Some(user) = list_known_users(&self.config)?
            .get(self.selected_user)
            .cloned()
        else {
            self.status = "no user selected".into();
            return Ok(());
        };
        self.clear_pending_user_delete();
        let disconnected = self.disconnect_live_user(&user);
        append_admin_log(
            &self.config,
            format!(
                "admin kicked active links user id={} name={} active_links_closed={disconnected}",
                user.user_id, user.display_name
            ),
        );
        self.status = if disconnected > 0 {
            format!(
                "{} kicked; closed {disconnected} active link(s)",
                user.display_name
            )
        } else {
            format!("{} has no active links to kick", user.display_name)
        };
        Ok(())
    }

    fn toggle_selected_user_mute(&mut self) -> ServerResult<()> {
        if self.tab != AdminTab::Moderation {
            return Ok(());
        }
        let Some(user) = list_known_users(&self.config)?
            .get(self.selected_user)
            .cloned()
        else {
            self.status = "no user selected".into();
            return Ok(());
        };
        self.clear_pending_user_delete();
        set_user_flag(&self.config, user.user_id, STATUS_MUTED, !user.muted)?;
        self.status = if user.muted {
            append_admin_log(
                &self.config,
                format!(
                    "admin unmuted user id={} name={}",
                    user.user_id, user.display_name
                ),
            );
            format!("{} unmuted; message sending restored", user.display_name)
        } else {
            append_admin_log(
                &self.config,
                format!(
                    "admin muted user id={} name={}",
                    user.user_id, user.display_name
                ),
            );
            format!(
                "{} muted; reading allowed, sending blocked",
                user.display_name
            )
        };
        Ok(())
    }

    fn cycle_selected_user_role(&mut self) -> ServerResult<()> {
        if self.tab != AdminTab::Moderation {
            return Ok(());
        }
        let Some(user) = list_known_users(&self.config)?
            .get(self.selected_user)
            .cloned()
        else {
            self.status = "no user selected".into();
            return Ok(());
        };
        self.clear_pending_user_delete();
        let next_role = next_role_bits(user.role_bits);
        set_user_role_bits(&self.config, user.user_id, next_role)?;
        append_admin_log(
            &self.config,
            format!(
                "admin set user role id={} name={} role={}",
                user.user_id,
                user.display_name,
                role_label(next_role)
            ),
        );
        self.status = format!(
            "{} role set to {}; permissions updated",
            user.display_name,
            role_label(next_role)
        );
        Ok(())
    }

    fn set_selected_user_role(&mut self, role_bits: u64) -> ServerResult<()> {
        if self.tab != AdminTab::Moderation {
            return Ok(());
        }
        let Some(user) = list_known_users(&self.config)?
            .get(self.selected_user)
            .cloned()
        else {
            self.status = "no user selected".into();
            return Ok(());
        };
        self.clear_pending_user_delete();
        set_user_role_bits(&self.config, user.user_id, role_bits)?;
        append_admin_log(
            &self.config,
            format!(
                "admin set user role id={} name={} role={}",
                user.user_id,
                user.display_name,
                role_label(role_bits)
            ),
        );
        self.status = format!(
            "{} role set to {}; permissions updated",
            user.display_name,
            role_label(role_bits)
        );
        Ok(())
    }

    fn delete_selected_stale_user(&mut self) -> ServerResult<()> {
        if self.tab != AdminTab::Moderation {
            return Ok(());
        }
        self.pending_prune_stale_users = false;
        let users = list_known_users(&self.config)?;
        let Some(user) = users.get(self.selected_user).cloned() else {
            self.status = "no user selected".into();
            return Ok(());
        };
        let active_links = self.active_user_link_count(&user);
        if active_links > 0 {
            self.pending_delete_user_id = None;
            self.status = format!(
                "{} has {active_links} active link(s); kick or ban before deleting the stale record",
                user.display_name
            );
            return Ok(());
        }
        let age = stale_user_age_secs(&user);
        if age < USER_DELETE_MIN_AGE_SECS {
            self.pending_delete_user_id = None;
            self.status = format!(
                "{} was seen too recently; delete allowed after 24h stale",
                user.display_name
            );
            return Ok(());
        }
        if self.pending_delete_user_id != Some(user.user_id) {
            self.pending_delete_user_id = Some(user.user_id);
            self.status = format!(
                "select Confirm Delete or press d again to delete stale user {}",
                user.display_name
            );
            return Ok(());
        }
        self.pending_delete_user_id = None;
        delete_user_record(&self.config, user.user_id)?;
        append_admin_log(
            &self.config,
            format!(
                "admin deleted stale user id={} name={} stale_secs={age}",
                user.user_id, user.display_name
            ),
        );
        self.selected_user = self.selected_user.min(users.len().saturating_sub(2));
        self.status = format!("deleted stale user record: {}", user.display_name);
        Ok(())
    }

    fn prune_stale_user_records(&mut self) -> ServerResult<()> {
        if self.tab != AdminTab::Moderation {
            return Ok(());
        }
        let users = list_known_users(&self.config)?;
        let eligible_users = users
            .iter()
            .filter(|user| stale_user_age_secs(user) >= USER_DELETE_MIN_AGE_SECS)
            .filter(|user| self.active_user_link_count(user) == 0)
            .collect::<Vec<_>>();
        let skipped_active = users
            .iter()
            .filter(|user| stale_user_age_secs(user) >= USER_DELETE_MIN_AGE_SECS)
            .filter(|user| self.active_user_link_count(user) > 0)
            .count();

        self.pending_delete_user_id = None;
        if eligible_users.is_empty() {
            self.pending_prune_stale_users = false;
            self.status = if skipped_active > 0 {
                format!("no inactive stale user records to prune; skipped {skipped_active} active")
            } else {
                "no stale user records are old enough to prune".into()
            };
            return Ok(());
        }
        if !self.pending_prune_stale_users {
            self.pending_prune_stale_users = true;
            self.status = if skipped_active > 0 {
                format!(
                    "select Confirm Prune Records to delete {} inactive stale record(s); skips {skipped_active} active",
                    eligible_users.len()
                )
            } else {
                format!(
                    "select Confirm Prune Records to delete {} inactive stale record(s)",
                    eligible_users.len()
                )
            };
            return Ok(());
        }

        let mut pruned = 0usize;
        for user in eligible_users {
            let age = stale_user_age_secs(user);
            delete_user_record(&self.config, user.user_id)?;
            append_admin_log(
                &self.config,
                format!(
                    "admin pruned stale user id={} name={} stale_secs={age}",
                    user.user_id, user.display_name
                ),
            );
            pruned += 1;
        }
        self.pending_prune_stale_users = false;
        self.selected_user = 0;
        self.status = if skipped_active > 0 {
            format!("pruned {pruned} stale user record(s); skipped {skipped_active} active")
        } else {
            format!("pruned {pruned} stale user record(s)")
        };
        Ok(())
    }

    #[cfg(feature = "live-rns-net")]
    fn active_user_link_count(&self, user: &AdminUserRow) -> usize {
        self.live
            .as_ref()
            .map(|live| {
                live.runtime
                    .live_server
                    .active_identity_counts()
                    .into_iter()
                    .find_map(|(identity, count)| (identity == user.identity_hash).then_some(count))
                    .unwrap_or(0)
            })
            .unwrap_or(0)
    }

    #[cfg(not(feature = "live-rns-net"))]
    fn active_user_link_count(&self, _user: &AdminUserRow) -> usize {
        0
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct SetupChecklistItem {
    label: &'static str,
    ready: bool,
    detail: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct AdminUserRow {
    user_id: i64,
    identity_hash: Vec<u8>,
    identity_hex: String,
    display_name: String,
    role_bits: u64,
    status_bits: u32,
    lxmf_destination: Option<String>,
    first_seen_at: i64,
    last_seen_at: Option<i64>,
    trusted: bool,
    banned: bool,
    muted: bool,
}

const STATUS_BANNED: u32 = 1;
const STATUS_MUTED: u32 = 1 << 1;
const ROLE_TRUSTED: u64 = 1;
const ROLE_MODERATOR: u64 = 1 << 1;
const ROLE_ADMIN: u64 = 1 << 2;
const USER_DELETE_MIN_AGE_SECS: i64 = 86_400;

fn setup_checklist(config: &ServerConfig) -> Vec<SetupChecklistItem> {
    let identity_detail = match std::fs::metadata(&config.identity_path) {
        Ok(metadata) if metadata.len() == 64 => {
            format!("real identity at {}", config.identity_path.display())
        }
        Ok(metadata) if metadata.len() > 0 => {
            format!(
                "placeholder or invalid identity at {} ({} bytes)",
                config.identity_path.display(),
                metadata.len()
            )
        }
        Ok(_) => format!("empty identity at {}", config.identity_path.display()),
        Err(_) => format!("missing identity at {}", config.identity_path.display()),
    };
    let identity_ready = std::fs::metadata(&config.identity_path)
        .map(|metadata| metadata.len() == 64)
        .unwrap_or(false);

    let database_ready = config.database_path.exists();
    let reticulum_config = config.reticulum_config_file();
    let reticulum_config_contents = std::fs::read_to_string(&reticulum_config).ok();
    let reticulum_ready = reticulum_config_contents
        .as_deref()
        .map(|contents| {
            let lower = contents.to_ascii_lowercase();
            lower.contains("type")
                && (lower.contains("enabled = yes") || lower.contains("interface_enabled = true"))
        })
        .unwrap_or(false);
    let reticulum_detail =
        reticulum_interface_summary(reticulum_config_contents.as_deref(), &reticulum_config);
    let rooms = config::list_rooms(config).unwrap_or_default();
    let lobby_ready = rooms.iter().any(|(_, name, _)| name == "lobby");

    vec![
        SetupChecklistItem {
            label: "server name",
            ready: !config.name.trim().is_empty(),
            detail: config.name.clone(),
        },
        SetupChecklistItem {
            label: "operator",
            ready: !config.operator_label.trim().is_empty(),
            detail: config.operator_label.clone(),
        },
        SetupChecklistItem {
            label: "chat service",
            ready: true,
            detail: "fixed service type omenchat.node; clients open omenchat://<hash>".into(),
        },
        SetupChecklistItem {
            label: "identity",
            ready: identity_ready,
            detail: identity_detail,
        },
        SetupChecklistItem {
            label: "database",
            ready: database_ready,
            detail: config.database_path.display().to_string(),
        },
        SetupChecklistItem {
            label: "reticulum",
            ready: reticulum_ready,
            detail: reticulum_detail,
        },
        SetupChecklistItem {
            label: "lobby room",
            ready: lobby_ready,
            detail: if lobby_ready {
                "default room exists".into()
            } else {
                "missing lobby; run init/status to repair".into()
            },
        },
        SetupChecklistItem {
            label: "announce interval",
            ready: config.announce_interval_minutes > 0,
            detail: format!(
                "every {} minute(s)",
                config.announce_interval_minutes.max(1)
            ),
        },
    ]
}

fn overview_operator_summary_text(config: &ServerConfig, live_status: &str) -> String {
    let room_count = config::list_rooms(config)
        .map(|rooms| rooms.len())
        .unwrap_or(0);
    let checklist = setup_checklist(config);
    let ready = checklist.iter().filter(|item| item.ready).count();
    let total = checklist.len();
    let first_missing_label = checklist
        .iter()
        .find(|item| !item.ready)
        .map(|item| item.label);
    let reticulum_config = config.reticulum_config_file();
    let reticulum_contents = std::fs::read_to_string(&reticulum_config).ok();
    let live_line = live_status
        .lines()
        .next()
        .filter(|line| !line.trim().is_empty())
        .unwrap_or("runtime: unknown");
    let interface_summary =
        reticulum_interface_summary(reticulum_contents.as_deref(), &reticulum_config);
    let upload_quota = if config.upload_quota_bytes == 0 {
        "disabled".into()
    } else {
        human_bytes(config.upload_quota_bytes)
    };
    format_overview_operator_summary_text(&OverviewOperatorSummaryText {
        ready,
        total,
        first_missing_label,
        live_line,
        interface_summary: &interface_summary,
        room_count,
        upload_max: human_bytes(config.upload_max_file_bytes),
        upload_quota,
    })
}

fn room_actions(
    selected: Option<(i64, &str, Option<&str>)>,
    pending_archive_room_id: Option<i64>,
) -> Vec<(AdminAction, String)> {
    let archive_label = selected
        .map(|(room_id, _, _)| {
            if room_id == 1 {
                "Lobby Protected"
            } else if pending_archive_room_id == Some(room_id) {
                "Confirm Archive"
            } else {
                "Archive Room (admin)"
            }
        })
        .unwrap_or("Archive Room (admin)");
    vec![
        (AdminAction::AddRoom, "Add Room (admin)".to_string()),
        (
            AdminAction::EditRoomTopic,
            "Edit Topic (mod/admin)".to_string(),
        ),
        (AdminAction::ArchiveRoom, archive_label.to_string()),
    ]
}

fn setup_next_steps_text(config: &ServerConfig) -> String {
    let checklist = setup_checklist(config);
    let launch_status = setup_launch_status_text(config);
    let missing_labels = checklist
        .iter()
        .filter(|item| !item.ready)
        .map(|item| item.label)
        .collect::<Vec<_>>();
    let storage_root = config.root_dir().display().to_string();
    let reticulum_config = config.reticulum_config_file();
    let reticulum_contents = std::fs::read_to_string(&reticulum_config).ok();
    let reticulum_summary =
        reticulum_interface_summary(reticulum_contents.as_deref(), &reticulum_config);
    let upload_policy = upload_policy_hint(config);
    format_setup_next_steps_text(&SetupNextStepsText {
        launch_status: &launch_status,
        all_ready: missing_labels.is_empty(),
        missing_labels,
        storage_root: &storage_root,
        reticulum_summary: &reticulum_summary,
        upload_policy: &upload_policy,
    })
}

fn setup_launch_status_text(config: &ServerConfig) -> String {
    let checklist = setup_checklist(config);
    let ready = checklist.iter().filter(|item| item.ready).count();
    let total = checklist.len();
    let first_missing_label = checklist
        .iter()
        .find(|item| !item.ready)
        .map(|item| item.label);
    format_setup_launch_status_text(&SetupLaunchText {
        ready,
        total,
        first_missing_label,
    })
}

fn setup_addresses_text(config: &ServerConfig) -> String {
    let public_addresses = config::render_public_addresses(config);
    let portal_path = config.nomadnet_index_page_path();
    let portal_page_file = portal_path.display().to_string();
    format_setup_addresses_text(&SetupAddressesText {
        public_addresses: &public_addresses,
        portal_page_file: &portal_page_file,
    })
}

fn setup_action_specs() -> [(AdminAction, &'static str); 19] {
    [
        (AdminAction::EditTcpClient, "Connect Gateway"),
        (AdminAction::EditTcpServer, "Local Listener"),
        (AdminAction::StartLive, "Start Live"),
        (AdminAction::AnnounceNow, "Announce Now"),
        (
            AdminAction::SelectTab(AdminTab::Monitoring),
            "Open Monitoring",
        ),
        (AdminAction::SelectTab(AdminTab::Portal), "Portal Addresses"),
        (AdminAction::EditServerName, "Server Name"),
        (AdminAction::EditOperator, "Operator Label"),
        (AdminAction::EditMotd, "MOTD"),
        (AdminAction::EditUploadMaxFileBytes, "Max File Size"),
        (AdminAction::EditUploadQuotaBytes, "Total Upload Quota"),
        (AdminAction::EditPingIntervalSeconds, "Ping Interval"),
        (AdminAction::EditAnnounceInterval, "Announce Interval"),
        (AdminAction::EditMaxMessageBytes, "Max Message Bytes"),
        (AdminAction::EditHistoryBatchSize, "History Batch"),
        (AdminAction::EditJoinBacklogEvents, "Join Backlog"),
        (
            AdminAction::EditLargeBatchThresholdBytes,
            "Large Batch Threshold",
        ),
        (AdminAction::EditMessageRate, "Message Rate"),
        (AdminAction::EditCommandRate, "Command Rate"),
    ]
}

fn overview_action_specs() -> [(AdminAction, &'static str); 13] {
    [
        (AdminAction::StartLive, "Start Live"),
        (AdminAction::AnnounceNow, "Announce Now"),
        (AdminAction::SelectTab(AdminTab::Monitoring), "Monitoring"),
        (AdminAction::EditTcpClient, "Connect Gateway"),
        (AdminAction::SelectTab(AdminTab::Portal), "Copy Addresses"),
        (AdminAction::EditMotd, "Edit MOTD"),
        (AdminAction::SelectTab(AdminTab::Rooms), "Rooms"),
        (AdminAction::SelectTab(AdminTab::Moderation), "Moderation"),
        (AdminAction::EditServerName, "Server Name"),
        (AdminAction::EditOperator, "Operator Label"),
        (AdminAction::SelectTab(AdminTab::Setup), "Setup + Limits"),
        (AdminAction::SaveConfig, "Save Config"),
        (AdminAction::StopLive, "Stop Live"),
    ]
}

fn list_known_users(config: &ServerConfig) -> ServerResult<Vec<AdminUserRow>> {
    config::init_files(config)?;
    let connection = rusqlite::Connection::open(&config.database_path)?;
    let mut statement = connection.prepare(
        "SELECT user_id, rns_identity_hash, display_name, role_bits, status_bits, lxmf_destination, first_seen_at, last_seen_at
         FROM users
         ORDER BY COALESCE(last_seen_at, first_seen_at) DESC, display_name",
    )?;
    let rows = statement.query_map([], |row| {
        let role_bits = row.get::<_, i64>(3)? as u64;
        let status_bits = row.get::<_, i64>(4)? as u32;
        let identity_hash = row.get::<_, Vec<u8>>(1)?;
        Ok(AdminUserRow {
            user_id: row.get(0)?,
            identity_hex: bytes_to_hex(&identity_hash),
            identity_hash,
            display_name: row.get(2)?,
            role_bits,
            status_bits,
            lxmf_destination: row.get(5)?,
            first_seen_at: row.get(6)?,
            last_seen_at: row.get(7)?,
            trusted: role_bits & ROLE_TRUSTED != 0,
            banned: status_bits & STATUS_BANNED != 0,
            muted: status_bits & STATUS_MUTED != 0,
        })
    })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

fn moderation_status_label(user: &AdminUserRow) -> &'static str {
    if user.banned {
        "banned"
    } else if user.muted {
        "muted"
    } else {
        "allowed"
    }
}

fn moderation_user_text(user: &AdminUserRow) -> ModerationUserText<'_> {
    ModerationUserText {
        user_id: user.user_id,
        identity_hex: &user.identity_hex,
        display_name: &user.display_name,
        role_label: role_label(user.role_bits),
        status_label: moderation_status_label(user),
        lxmf_destination: user.lxmf_destination.as_deref(),
        first_seen_at: user.first_seen_at,
        last_seen_at: user.last_seen_at,
        trusted: user.trusted,
        banned: user.banned,
        muted: user.muted,
    }
}

fn room_list_label(
    marker: &str,
    room_id: i64,
    name: &str,
    topic: Option<&str>,
    max_width: usize,
) -> String {
    room_list_label_text(&RoomListLabelText {
        marker,
        room_id,
        name,
        topic,
        max_width,
    })
}

fn setup_checklist_line<'a>(
    marker: &'static str,
    item: &SetupChecklistItem,
    max_width: usize,
) -> Line<'a> {
    let style = if item.ready {
        Style::default().fg(Color::Green)
    } else {
        Style::default().fg(Color::Yellow)
    };
    let text = setup_checklist_line_text(&SetupChecklistLineText {
        marker,
        label: item.label,
        detail: &item.detail,
        max_width,
    });
    let detail = text.strip_prefix(marker).unwrap_or_default().to_string();
    Line::from(vec![Span::styled(marker, style), Span::raw(detail)])
}

fn moderation_actions(
    user: Option<&AdminUserRow>,
    pending_delete_user_id: Option<i64>,
    pending_prune_stale_users: bool,
) -> Vec<(AdminAction, String)> {
    let ban_label = user
        .map(|user| {
            if user.banned {
                "Unban Future Access"
            } else {
                "Ban Access + Close"
            }
        })
        .unwrap_or("Ban Access + Close");
    let mute_label = user
        .map(|user| {
            if user.muted {
                "Unmute Sending"
            } else {
                "Mute Sending"
            }
        })
        .unwrap_or("Mute Sending");
    let trust_label = user
        .map(|user| {
            if user.trusted {
                "Untrust Media"
            } else {
                "Trust Media"
            }
        })
        .unwrap_or("Trust Media");
    let delete_label = user
        .filter(|user| pending_delete_user_id == Some(user.user_id))
        .map(|_| "Confirm Delete Record")
        .unwrap_or("Delete Stale User");
    let prune_label = if pending_prune_stale_users {
        "Confirm Prune Records"
    } else {
        "Prune Stale Users"
    };
    let current_role = user.map(|user| role_label(user.role_bits));
    let mut actions = vec![
        (AdminAction::ToggleBan, ban_label.to_string()),
        (
            AdminAction::KickActiveUser,
            "Close Active Links".to_string(),
        ),
        (AdminAction::ToggleMute, mute_label.to_string()),
        (AdminAction::ToggleTrust, trust_label.to_string()),
    ];
    actions.extend(
        [
            ("standard", 0),
            ("trusted", ROLE_TRUSTED),
            ("mod", ROLE_TRUSTED | ROLE_MODERATOR),
            ("admin", ROLE_TRUSTED | ROLE_MODERATOR | ROLE_ADMIN),
        ]
        .into_iter()
        .map(|(label, role_bits)| {
            let action_label = if current_role == Some(label) {
                format!("Role: {}", title_case_role(label))
            } else {
                format!("Make {}", title_case_role(label))
            };
            (AdminAction::SetRole(role_bits), action_label)
        }),
    );
    actions.push((AdminAction::DeleteStaleUser, delete_label.to_string()));
    actions.push((AdminAction::PruneStaleUsers, prune_label.to_string()));
    actions
}

fn string_actions_for_live_state(
    actions: &[(AdminAction, &str)],
    live_running: bool,
) -> Vec<(AdminAction, String)> {
    actions
        .iter()
        .map(|(action, label)| (*action, stateful_action_label(*action, label, live_running)))
        .collect()
}

fn stateful_action_label(action: AdminAction, label: &str, live_running: bool) -> String {
    match action {
        AdminAction::StartLive if live_running => "Live Running".into(),
        AdminAction::StartLive => label.to_string(),
        AdminAction::StopLive if live_running => label.to_string(),
        AdminAction::StopLive => "Stop Live (stopped)".into(),
        AdminAction::AnnounceNow if live_running => label.to_string(),
        AdminAction::AnnounceNow => "Announce Now (start live first)".into(),
        _ => label.to_string(),
    }
}

#[cfg(feature = "live-rns-net")]
fn announce_event_text(kind: &str, destination: &str) -> String {
    format!(
        "{kind} announce at {} for omenchat://{destination}",
        unix_to_utc_string(current_unix_secs())
    )
}

fn title_case_role(role: &str) -> &'static str {
    match role {
        "standard" => "Standard",
        "trusted" => "Trusted",
        "mod" => "Moderator",
        "admin" => "Admin",
        _ => "Standard",
    }
}

fn role_label(role_bits: u64) -> &'static str {
    if role_bits & ROLE_ADMIN != 0 {
        "admin"
    } else if role_bits & ROLE_MODERATOR != 0 {
        "mod"
    } else if role_bits & ROLE_TRUSTED != 0 {
        "trusted"
    } else {
        "standard"
    }
}

fn role_bits_from_label(label: &str) -> Option<u64> {
    match label.trim().to_ascii_lowercase().as_str() {
        "standard" | "member" | "user" => Some(0),
        "trusted" | "trust" => Some(ROLE_TRUSTED),
        "mod" | "moderator" => Some(ROLE_TRUSTED | ROLE_MODERATOR),
        "admin" | "administrator" => Some(ROLE_TRUSTED | ROLE_MODERATOR | ROLE_ADMIN),
        _ => None,
    }
}

fn next_role_bits(role_bits: u64) -> u64 {
    match role_label(role_bits) {
        "standard" => ROLE_TRUSTED,
        "trusted" => ROLE_TRUSTED | ROLE_MODERATOR,
        "mod" => ROLE_TRUSTED | ROLE_MODERATOR | ROLE_ADMIN,
        _ => 0,
    }
}

fn stale_user_age_secs(user: &AdminUserRow) -> i64 {
    current_unix_secs().saturating_sub(user.last_seen_at.unwrap_or(user.first_seen_at))
}

fn stale_delete_status_label(user: &AdminUserRow) -> String {
    let age = stale_user_age_secs(user);
    if age >= USER_DELETE_MIN_AGE_SECS {
        format!("eligible; last seen {}", human_age(age))
    } else {
        let remaining = USER_DELETE_MIN_AGE_SECS.saturating_sub(age);
        format!("available in {}", human_age_duration(remaining))
    }
}

#[cfg(feature = "live-rns-net")]
fn active_link_monitoring_text(
    link: &ActiveLinkSummary,
    room: &str,
    now_unix: i64,
) -> ActiveLinkMonitoringText {
    let age = if link.connected_at_unix > 0 {
        human_age(now_unix.saturating_sub(link.connected_at_unix))
    } else {
        "unknown age".into()
    };
    let activity = active_link_activity_label(link, now_unix)
        .unwrap_or_else(|| "rate waiting for connection age".into());
    ActiveLinkMonitoringText {
        name: link.display_name.clone(),
        identity: compact_identity(&bytes_to_hex(&link.identity_hash)),
        room: room.to_string(),
        age,
        activity,
        frames: link.traffic.frames_in,
        bytes: human_bytes(link.traffic.bytes_in),
        history_requests: link.traffic.history_requests,
        pings: link.traffic.pings,
        chat_messages: link.traffic.chat_messages,
        commands: link.traffic.commands,
        upload_requests: link.traffic.upload_requests,
        link_id: compact_identity(&bytes_to_hex(&link.link_id)),
    }
}

#[cfg(feature = "live-rns-net")]
fn closed_link_monitoring_text(
    link: &ClosedLinkSummary,
    room: &str,
    now_unix: i64,
) -> ClosedLinkMonitoringText {
    let connected_for = if link.connected_at_unix > 0 && link.closed_at_unix > 0 {
        human_age(link.closed_at_unix.saturating_sub(link.connected_at_unix))
    } else {
        "unknown age".into()
    };
    let closed_ago = if link.closed_at_unix > 0 {
        human_age(now_unix.saturating_sub(link.closed_at_unix))
    } else {
        "unknown".into()
    };
    let identity = link
        .identity_hash
        .as_deref()
        .map(|bytes| compact_identity(&bytes_to_hex(bytes)))
        .unwrap_or_else(|| "unknown".into());
    ClosedLinkMonitoringText {
        name: link.display_name.clone(),
        identity,
        room: room.to_string(),
        status: closed_link_status_label(&link.reason).to_string(),
        connected_for,
        closed_ago,
        link_id: compact_identity(&bytes_to_hex(&link.link_id)),
        reason: link.reason.clone(),
    }
}

#[cfg(feature = "live-rns-net")]
fn indent_lines(text: &str, prefix: &str) -> String {
    text.lines()
        .map(|line| format!("{prefix}{line}"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn set_user_flag(
    config: &ServerConfig,
    user_id: i64,
    flag: u32,
    enabled: bool,
) -> ServerResult<()> {
    config::init_files(config)?;
    let connection = rusqlite::Connection::open(&config.database_path)?;
    let current: i64 = connection.query_row(
        "SELECT status_bits FROM users WHERE user_id = ?1",
        [user_id],
        |row| row.get(0),
    )?;
    let mut next = current as u32;
    if enabled {
        next |= flag;
    } else {
        next &= !flag;
    }
    connection.execute(
        "UPDATE users SET status_bits = ?1 WHERE user_id = ?2",
        (next as i64, user_id),
    )?;
    Ok(())
}

fn set_user_role_flag(
    config: &ServerConfig,
    user_id: i64,
    flag: u64,
    enabled: bool,
) -> ServerResult<()> {
    config::init_files(config)?;
    let connection = rusqlite::Connection::open(&config.database_path)?;
    let current: i64 = connection.query_row(
        "SELECT role_bits FROM users WHERE user_id = ?1",
        [user_id],
        |row| row.get(0),
    )?;
    let mut next = current as u64;
    if enabled {
        next |= flag;
    } else {
        next &= !flag;
    }
    connection.execute(
        "UPDATE users SET role_bits = ?1 WHERE user_id = ?2",
        (next as i64, user_id),
    )?;
    Ok(())
}

fn set_user_role_bits(config: &ServerConfig, user_id: i64, role_bits: u64) -> ServerResult<()> {
    config::init_files(config)?;
    let connection = rusqlite::Connection::open(&config.database_path)?;
    connection.execute(
        "UPDATE users SET role_bits = ?1 WHERE user_id = ?2",
        (role_bits as i64, user_id),
    )?;
    Ok(())
}

fn delete_user_record(config: &ServerConfig, user_id: i64) -> ServerResult<()> {
    config::init_files(config)?;
    let connection = rusqlite::Connection::open(&config.database_path)?;
    connection.execute("DELETE FROM room_members WHERE user_id = ?1", [user_id])?;
    connection.execute("DELETE FROM users WHERE user_id = ?1", [user_id])?;
    Ok(())
}

fn prune_stale_user_records_for_config(
    config: &ServerConfig,
    log_action: &str,
) -> ServerResult<usize> {
    let users = list_known_users(config)?;
    let mut pruned = 0usize;
    for user in users
        .iter()
        .filter(|user| stale_user_age_secs(user) >= USER_DELETE_MIN_AGE_SECS)
    {
        let age = stale_user_age_secs(user);
        delete_user_record(config, user.user_id)?;
        append_admin_log(
            config,
            format!(
                "{log_action} stale user id={} name={} stale_secs={age}",
                user.user_id, user.display_name
            ),
        );
        pruned += 1;
    }
    Ok(pruned)
}

fn bytes_to_hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>()
}

fn compact_identity(hex: &str) -> String {
    if hex.len() <= 8 {
        hex.to_string()
    } else {
        hex[..8].to_string()
    }
}

#[cfg(feature = "live-rns-net")]
fn hex_lower_local(bytes: &[u8]) -> String {
    bytes_to_hex(bytes)
}

fn identity_panel_text(config: &ServerConfig) -> String {
    let managed_root = config.root_dir();
    let checklist = identity_operator_checklist(config);
    let destinations = compact_identity_destination_text(config);
    let identity_file = config.identity_path.display().to_string();
    let storage_root = managed_root.display().to_string();
    let database_path = config.database_path.display().to_string();
    let reticulum_path = config.reticulum_config_path.display().to_string();
    let reticulum_config_path = config.reticulum_config_file().display().to_string();
    format_identity_panel_text(&IdentityPanelText {
        identity_file: &identity_file,
        storage_root: &storage_root,
        checklist: &checklist,
        destinations: &destinations,
        database_path: &database_path,
        reticulum_path: &reticulum_path,
        reticulum_config_path: &reticulum_config_path,
    })
}

fn identity_operator_checklist(config: &ServerConfig) -> String {
    let identity_state = if config.identity_path.is_file() {
        "identity exists; back it up before public testing"
    } else {
        "identity missing; run init/status/live startup to create it"
    };
    [
        "identity safety:".to_string(),
        format!("  state: {identity_state}"),
        format!(
            "  backup now: copy {} to offline/private storage",
            config.identity_path.display()
        ),
        "  losing this file changes the OMENchat and NomadNet portal addresses".to_string(),
        "  OMENchat and portal destinations are derived from this same identity".to_string(),
        "  keep this server root separate from browser, NomadNet, LXMF, and system Reticulum roots"
            .to_string(),
        "  never replace identity material while users still know the old server hash".to_string(),
    ]
    .join("\n")
}

fn portal_panel_text(config: &ServerConfig) -> String {
    let destination = identity_destination_text(config);
    let page_path = config.nomadnet_index_page_path();
    let page_state = std::fs::metadata(&page_path)
        .map(|metadata| {
            format!(
                "{} ({}, modified {})",
                page_path.display(),
                human_bytes(metadata.len()),
                metadata
                    .modified()
                    .map(human_system_time_local)
                    .unwrap_or_else(|_| "unknown".into())
            )
        })
        .unwrap_or_else(|_| format!("{} (missing)", page_path.display()));
    let motd = if config.motd.trim().is_empty() {
        "(none)"
    } else {
        config.motd.trim()
    };
    let checklist = portal_operator_checklist(config);
    format_portal_panel_text(&PortalPanelText {
        checklist: &checklist,
        destination: &destination,
        page_state: &page_state,
        motd,
    })
}

fn portal_operator_checklist(config: &ServerConfig) -> String {
    let page_path = config.nomadnet_index_page_path();
    let page_exists = page_path.is_file();
    let motd_state = if config.motd.trim().is_empty() {
        "set MOTD if you want a short server notice"
    } else {
        "MOTD is set"
    };
    let page_state = if page_exists {
        "portal page exists; edit it directly for rules/help"
    } else {
        "portal page missing; start live/status to create template"
    };
    [
        "portal readiness:".to_string(),
        format!("  page: {page_state}"),
        format!("  motd: {motd_state}"),
        "  publish: verify Monitoring before sharing either address".to_string(),
    ]
    .join("\n")
}

fn compact_identity_destination_text(config: &ServerConfig) -> String {
    let full = identity_destination_text(config);
    let mut lines = Vec::new();
    for line in full.lines() {
        if line.starts_with("identity hash:")
            || line.starts_with("destination:")
            || line.starts_with("nomadnet portal:")
            || line.starts_with("destination: unavailable")
        {
            lines.push(format!("  {line}"));
        }
    }
    if lines.is_empty() {
        format!("  {}", full.trim())
    } else {
        lines.join("\n")
    }
}

#[cfg(feature = "live-rns-net")]
fn identity_destination_text(config: &ServerConfig) -> String {
    crate::rns_net_live::configured_destination_status(config)
        .unwrap_or_else(|error| format!("destination: unavailable ({error})\n"))
}

#[cfg(not(feature = "live-rns-net"))]
fn identity_destination_text(_config: &ServerConfig) -> String {
    "destination: unavailable (rebuild with --features live-rns-net)\n".into()
}

fn read_log_tail(path: &std::path::Path, max_lines: usize) -> ServerResult<String> {
    ensure_log_file(path)?;
    let contents = std::fs::read_to_string(path)?;
    let max_lines = max_lines.max(1);
    let lines = contents.lines().collect::<Vec<_>>();
    let start = lines.len().saturating_sub(max_lines);
    Ok(lines[start..].join("\n"))
}

fn read_admin_audit_tail(path: &std::path::Path, max_lines: usize) -> ServerResult<String> {
    ensure_log_file(path)?;
    let contents = std::fs::read_to_string(path)?;
    let max_lines = max_lines.max(1);
    let lines = contents
        .lines()
        .filter(|line| line.contains(" admin "))
        .collect::<Vec<_>>();
    let start = lines.len().saturating_sub(max_lines);
    Ok(lines[start..].join("\n"))
}

fn log_panel_text(path: &std::path::Path, max_lines: usize) -> String {
    let tail = read_log_tail(path, max_lines)
        .unwrap_or_else(|error| format!("log read failed: {error}"))
        .trim()
        .to_string();
    let body = if tail.is_empty() {
        "No log entries yet. Start the live server or run admin actions to populate this file."
            .to_string()
    } else {
        tail
    };
    format!(
        "Logs: runtime and network detail\nfile: {}\nnormal: startup/manual/automatic announces, ping/pong, duplicate identity reconnects\nwatch: repeated timeouts, protocol errors, interface watchdog restarts, announce failures\nadmin changes: Audit tab\n\n{body}",
        path.display()
    )
}

fn audit_panel_text(path: &std::path::Path, max_lines: usize) -> String {
    let tail = read_admin_audit_tail(path, max_lines)
        .unwrap_or_else(|error| format!("audit read failed: {error}"))
        .trim()
        .to_string();
    let summary = audit_summary_text(&tail);
    let body = if tail.is_empty() {
        "No admin actions recorded yet. Actions such as config edits, room archive, user moderation, and stale-user pruning will appear here."
            .to_string()
    } else {
        tail
    };
    format!(
        "Audit: local admin changes only\nfile: {}\n{summary}\nruntime/network detail: Logs tab\n\n{body}",
        path.display()
    )
}

fn ensure_log_file(path: &std::path::Path) -> ServerResult<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    Ok(())
}

fn append_admin_log(config: &ServerConfig, message: impl AsRef<str>) {
    let path = config.log_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let timestamp = unix_to_utc_string(current_unix_secs());
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        let _ = writeln!(file, "{timestamp} {}", message.as_ref());
    }
}

fn admin_block(title: &str) -> Block<'_> {
    Block::default()
        .title(title)
        .title_alignment(Alignment::Left)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Rgb(180, 90, 165)))
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(vertical[1])[1]
}

fn run_line_console(mut config: ServerConfig) -> ServerResult<()> {
    config::init_files(&config)?;
    print_dashboard(&config)?;
    print_commands();

    let stdin = io::stdin();
    loop {
        print!("omenchatd> ");
        io::stdout().flush()?;

        let mut line = String::new();
        if stdin.read_line(&mut line)? == 0 {
            println!();
            return Ok(());
        }
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        match apply_admin_command(&mut config, line) {
            Ok(AdminConsoleAction::Continue) => {}
            Ok(AdminConsoleAction::Quit) => return Ok(()),
            Err(error) => println!("error: {error}"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum AdminConsoleAction {
    Continue,
    Quit,
}

fn apply_admin_command(config: &mut ServerConfig, line: &str) -> ServerResult<AdminConsoleAction> {
    let mut parts = line.split_whitespace();
    let Some(command) = parts.next() else {
        return Ok(AdminConsoleAction::Continue);
    };
    match command {
        "q" | "quit" | "exit" => Ok(AdminConsoleAction::Quit),
        "h" | "help" | "?" => {
            print_commands();
            Ok(AdminConsoleAction::Continue)
        }
        "r" | "refresh" | "status" => {
            print_dashboard(config)?;
            Ok(AdminConsoleAction::Continue)
        }
        "rooms" => {
            print_rooms(config)?;
            Ok(AdminConsoleAction::Continue)
        }
        "setup" => {
            print_setup_checklist(config);
            Ok(AdminConsoleAction::Continue)
        }
        "users" => {
            print_users(config)?;
            Ok(AdminConsoleAction::Continue)
        }
        "add-room" => {
            let Some(name) = parts.next() else {
                return Err(ServerError::Message(
                    "usage: add-room <name> [topic words...]".into(),
                ));
            };
            let topic = parts.collect::<Vec<_>>().join(" ");
            let topic = (!topic.trim().is_empty()).then_some(topic);
            config::add_room(config, name, topic.as_deref())?;
            append_admin_log(
                config,
                format!(
                    "admin console added room name={}",
                    name.trim().trim_start_matches('#')
                ),
            );
            println!("{}", room_ready_update_text(name));
            Ok(AdminConsoleAction::Continue)
        }
        "room-topic" => {
            let Some(room_id) = parts.next().and_then(|value| value.parse::<i64>().ok()) else {
                return Err(ServerError::Message(
                    "usage: room-topic <room_id> [topic words...]".into(),
                ));
            };
            let topic = parts.collect::<Vec<_>>().join(" ");
            let topic = (!topic.trim().is_empty()).then_some(topic);
            config::update_room_topic(config, room_id, topic.as_deref())?;
            append_admin_log(
                config,
                format!("admin console updated room topic id={room_id}"),
            );
            println!("{}", room_topic_update_text(room_id));
            Ok(AdminConsoleAction::Continue)
        }
        "archive-room" => {
            let Some(room_id) = parts.next().and_then(|value| value.parse::<i64>().ok()) else {
                return Err(ServerError::Message("usage: archive-room <room_id>".into()));
            };
            config::archive_room(config, room_id)?;
            append_admin_log(config, format!("admin console archived room id={room_id}"));
            println!("{}", room_archived_update_text(room_id));
            Ok(AdminConsoleAction::Continue)
        }
        "set-name" => {
            let value = require_rest(line, command, "usage: set-name <server name>")?;
            config.name = value;
            config.save()?;
            append_admin_log(config, "admin console updated server name");
            println!("{}", server_name_update_text());
            Ok(AdminConsoleAction::Continue)
        }
        "set-operator" => {
            let value = require_rest(line, command, "usage: set-operator <label>")?;
            config.operator_label = value;
            config.save()?;
            append_admin_log(config, "admin console updated operator label");
            println!("{}", operator_label_update_text());
            Ok(AdminConsoleAction::Continue)
        }
        "set-motd" => {
            let value = require_rest(line, command, "usage: set-motd <message>")?;
            config.motd = value;
            config.save()?;
            append_admin_log(config, "admin console updated MOTD");
            println!("{}", motd_update_text());
            Ok(AdminConsoleAction::Continue)
        }
        "set-announce-interval" => {
            let Some(value) = parts.next() else {
                return Err(ServerError::Message(
                    "usage: set-announce-interval <minutes>".into(),
                ));
            };
            let minutes = value.parse::<u64>().map_err(|_| {
                ServerError::Message("announce interval must be whole minutes".into())
            })?;
            config.announce_interval_minutes = minutes.max(1);
            config.save()?;
            append_admin_log(
                config,
                format!(
                    "admin console updated announce interval minutes={}",
                    config.announce_interval_minutes
                ),
            );
            println!(
                "{}",
                announce_interval_update_text(config.announce_interval_minutes)
            );
            Ok(AdminConsoleAction::Continue)
        }
        "set-upload-quota-bytes" => {
            let Some(value) = parts.next() else {
                return Err(ServerError::Message(
                    "usage: set-upload-quota-bytes <bytes|0>".into(),
                ));
            };
            let bytes = value
                .parse::<u64>()
                .map_err(|_| ServerError::Message("upload quota must be bytes".into()))?;
            config.upload_quota_bytes = bytes.min(10 * 1024 * 1024 * 1024);
            config.save()?;
            append_admin_log(
                config,
                format!(
                    "admin console updated upload quota bytes={}",
                    config.upload_quota_bytes
                ),
            );
            println!("{}", upload_quota_update_text(config.upload_quota_bytes));
            Ok(AdminConsoleAction::Continue)
        }
        "set-upload-max-file-bytes" => {
            let Some(value) = parts.next() else {
                return Err(ServerError::Message(
                    "usage: set-upload-max-file-bytes <bytes>".into(),
                ));
            };
            let bytes = value
                .parse::<u64>()
                .map_err(|_| ServerError::Message("upload max file must be bytes".into()))?;
            config.upload_max_file_bytes = bytes.clamp(1, 10 * 1024 * 1024);
            config.save()?;
            append_admin_log(
                config,
                format!(
                    "admin console updated upload max file bytes={}",
                    config.upload_max_file_bytes
                ),
            );
            println!(
                "{}",
                upload_max_file_update_text(config.upload_max_file_bytes)
            );
            Ok(AdminConsoleAction::Continue)
        }
        "set-ping-interval" | "set-ping-interval-seconds" => {
            let Some(value) = parts.next() else {
                return Err(ServerError::Message(
                    "usage: set-ping-interval <seconds>".into(),
                ));
            };
            let seconds = value
                .parse::<u64>()
                .map_err(|_| ServerError::Message("ping interval must be whole seconds".into()))?;
            config.ping_interval_seconds = seconds.clamp(5, 600);
            config.save()?;
            append_admin_log(
                config,
                format!(
                    "admin console updated ping interval seconds={}",
                    config.ping_interval_seconds
                ),
            );
            println!(
                "{}",
                ping_interval_update_text(config.ping_interval_seconds)
            );
            Ok(AdminConsoleAction::Continue)
        }
        "set-max-message-bytes" => {
            let value = parse_usize_arg(&mut parts, "usage: set-max-message-bytes <bytes>")?;
            config.limits.max_message_bytes = value.clamp(1, 262_144);
            config.save()?;
            append_admin_log(
                config,
                format!(
                    "admin console updated max message bytes={}",
                    config.limits.max_message_bytes
                ),
            );
            println!(
                "{}",
                max_message_bytes_update_text(config.limits.max_message_bytes)
            );
            Ok(AdminConsoleAction::Continue)
        }
        "set-history-batch-size" => {
            let value = parse_usize_arg(&mut parts, "usage: set-history-batch-size <count>")?;
            config.limits.history_batch_size = value.clamp(1, 500);
            config.save()?;
            append_admin_log(
                config,
                format!(
                    "admin console updated history batch size={}",
                    config.limits.history_batch_size
                ),
            );
            println!(
                "{}",
                history_batch_size_update_text(config.limits.history_batch_size)
            );
            Ok(AdminConsoleAction::Continue)
        }
        "set-join-backlog-events" => {
            let value = parse_usize_arg(&mut parts, "usage: set-join-backlog-events <count>")?;
            config.limits.join_backlog_events = value.clamp(0, 500);
            config.save()?;
            append_admin_log(
                config,
                format!(
                    "admin console updated join backlog events={}",
                    config.limits.join_backlog_events
                ),
            );
            println!(
                "{}",
                join_backlog_events_update_text(config.limits.join_backlog_events)
            );
            Ok(AdminConsoleAction::Continue)
        }
        "set-large-batch-threshold-bytes" => {
            let value =
                parse_usize_arg(&mut parts, "usage: set-large-batch-threshold-bytes <bytes>")?;
            config.limits.large_batch_threshold_bytes = value.clamp(256, 1_048_576);
            config.save()?;
            append_admin_log(
                config,
                format!(
                    "admin console updated large batch threshold bytes={}",
                    config.limits.large_batch_threshold_bytes
                ),
            );
            println!(
                "{}",
                large_batch_threshold_update_text(config.limits.large_batch_threshold_bytes)
            );
            Ok(AdminConsoleAction::Continue)
        }
        "set-rate-messages-per-minute" => {
            let value = parse_usize_arg(&mut parts, "usage: set-rate-messages-per-minute <count>")?;
            config.limits.rate_messages_per_minute = value.min(600);
            config.save()?;
            append_admin_log(
                config,
                format!(
                    "admin console updated message rate per minute={}",
                    config.limits.rate_messages_per_minute
                ),
            );
            println!(
                "{}",
                message_rate_update_text(config.limits.rate_messages_per_minute)
            );
            Ok(AdminConsoleAction::Continue)
        }
        "set-rate-commands-per-minute" => {
            let value = parse_usize_arg(&mut parts, "usage: set-rate-commands-per-minute <count>")?;
            config.limits.rate_commands_per_minute = value.min(600);
            config.save()?;
            append_admin_log(
                config,
                format!(
                    "admin console updated command rate per minute={}",
                    config.limits.rate_commands_per_minute
                ),
            );
            println!(
                "{}",
                command_rate_update_text(config.limits.rate_commands_per_minute)
            );
            Ok(AdminConsoleAction::Continue)
        }
        "tcp-server" => {
            let Some(value) = parts.next() else {
                return Err(ServerError::Message(
                    "usage: tcp-server <listen_ip:port>".into(),
                ));
            };
            let tcp_server = parse_tcp_server_override(value)
                .ok_or_else(|| ServerError::Message("invalid listen_ip:port".into()))?;
            config::write_reticulum_tcp_server_config(config, &tcp_server)?;
            append_admin_log(
                config,
                format!(
                    "admin console wrote TCPServerInterface listen={}:{}",
                    tcp_server.listen_ip, tcp_server.listen_port
                ),
            );
            println!("updated {}", config.reticulum_config_file().display());
            Ok(AdminConsoleAction::Continue)
        }
        "tcp-client" => {
            let Some(value) = parts.next() else {
                return Err(ServerError::Message(
                    "usage: tcp-client <gateway_host:port>".into(),
                ));
            };
            let tcp_client = parse_tcp_client_override(value)
                .ok_or_else(|| ServerError::Message("invalid gateway host:port".into()))?;
            config::write_reticulum_tcp_client_config(config, &tcp_client)?;
            append_admin_log(
                config,
                format!(
                    "admin console wrote TCPClientInterface target={}:{}",
                    tcp_client.target_host, tcp_client.target_port
                ),
            );
            println!("updated {}", config.reticulum_config_file().display());
            Ok(AdminConsoleAction::Continue)
        }
        "ban-user" => {
            let user_id = parse_user_id(&mut parts, "usage: ban-user <user_id>")?;
            set_user_flag(config, user_id, STATUS_BANNED, true)?;
            append_admin_log(config, format!("admin console banned user id={user_id}"));
            println!("{}", user_banned_update_text(user_id));
            Ok(AdminConsoleAction::Continue)
        }
        "unban-user" => {
            let user_id = parse_user_id(&mut parts, "usage: unban-user <user_id>")?;
            set_user_flag(config, user_id, STATUS_BANNED, false)?;
            append_admin_log(config, format!("admin console unbanned user id={user_id}"));
            println!("{}", user_unbanned_update_text(user_id));
            Ok(AdminConsoleAction::Continue)
        }
        "mute-user" => {
            let user_id = parse_user_id(&mut parts, "usage: mute-user <user_id>")?;
            set_user_flag(config, user_id, STATUS_MUTED, true)?;
            append_admin_log(config, format!("admin console muted user id={user_id}"));
            println!("{}", user_muted_update_text(user_id));
            Ok(AdminConsoleAction::Continue)
        }
        "unmute-user" => {
            let user_id = parse_user_id(&mut parts, "usage: unmute-user <user_id>")?;
            set_user_flag(config, user_id, STATUS_MUTED, false)?;
            append_admin_log(config, format!("admin console unmuted user id={user_id}"));
            println!("{}", user_unmuted_update_text(user_id));
            Ok(AdminConsoleAction::Continue)
        }
        "trust-user" => {
            let user_id = parse_user_id(&mut parts, "usage: trust-user <user_id>")?;
            set_user_role_flag(config, user_id, ROLE_TRUSTED, true)?;
            append_admin_log(config, format!("admin console trusted user id={user_id}"));
            println!("{}", user_trusted_update_text(user_id));
            Ok(AdminConsoleAction::Continue)
        }
        "untrust-user" => {
            let user_id = parse_user_id(&mut parts, "usage: untrust-user <user_id>")?;
            set_user_role_flag(config, user_id, ROLE_TRUSTED, false)?;
            append_admin_log(config, format!("admin console untrusted user id={user_id}"));
            println!("{}", user_untrusted_update_text(user_id));
            Ok(AdminConsoleAction::Continue)
        }
        "delete-user" => {
            let user_id = parse_user_id(&mut parts, "usage: delete-user <user_id>")?;
            let user = list_known_users(config)?
                .into_iter()
                .find(|user| user.user_id == user_id)
                .ok_or_else(|| ServerError::Message(format!("unknown user id={user_id}")))?;
            let age = stale_user_age_secs(&user);
            if age < USER_DELETE_MIN_AGE_SECS {
                return Err(ServerError::Message(format!(
                    "user was seen too recently; delete allowed after 24h stale ({})",
                    stale_delete_status_label(&user)
                )));
            }
            delete_user_record(config, user_id)?;
            append_admin_log(
                config,
                format!(
                    "admin console deleted stale user id={user_id} name={} stale_secs={age}",
                    user.display_name
                ),
            );
            println!("deleted stale user: id={user_id}");
            Ok(AdminConsoleAction::Continue)
        }
        "prune-stale-users" => {
            let pruned = prune_stale_user_records_for_config(config, "admin console pruned")?;
            println!("pruned stale users: {pruned}");
            Ok(AdminConsoleAction::Continue)
        }
        "set-user-role" => {
            let user_id = parse_user_id(&mut parts, "usage: set-user-role <user_id> <role>")?;
            let Some(role) = parts.next().and_then(role_bits_from_label) else {
                return Err(ServerError::Message(
                    "usage: set-user-role <user_id> <standard|trusted|mod|admin>".into(),
                ));
            };
            set_user_role_bits(config, user_id, role)?;
            append_admin_log(
                config,
                format!(
                    "admin console set user role id={user_id} role={}",
                    role_label(role)
                ),
            );
            println!("{}", user_role_update_text(user_id, role_label(role)));
            Ok(AdminConsoleAction::Continue)
        }
        "show-config" => {
            print!("{}", config.render_toml());
            Ok(AdminConsoleAction::Continue)
        }
        _ => Err(ServerError::Message(format!(
            "unknown command '{command}', type help"
        ))),
    }
}

fn parse_user_id<'a>(
    parts: &mut impl Iterator<Item = &'a str>,
    usage: &'static str,
) -> ServerResult<i64> {
    parts
        .next()
        .and_then(|value| value.parse::<i64>().ok())
        .ok_or_else(|| ServerError::Message(usage.into()))
}

fn parse_usize_arg<'a>(
    parts: &mut impl Iterator<Item = &'a str>,
    usage: &'static str,
) -> ServerResult<usize> {
    parts
        .next()
        .and_then(|value| value.parse::<usize>().ok())
        .ok_or_else(|| ServerError::Message(usage.into()))
}

fn parse_limit_input(value: &str, label: &str) -> ServerResult<usize> {
    value
        .parse::<usize>()
        .map_err(|_| ServerError::Message(format!("{label} must be a whole number")))
}

fn parse_tcp_client_override(value: &str) -> Option<TcpClientOverride> {
    let (target_host, port) = value.rsplit_once(':')?;
    let target_port = port.parse::<u16>().ok()?;
    if target_host.trim().is_empty() {
        return None;
    }
    Some(TcpClientOverride {
        target_host: target_host.trim().to_string(),
        target_port,
        network_name: None,
        passphrase: None,
    })
}

fn require_rest(line: &str, command: &str, usage: &str) -> ServerResult<String> {
    let value = line
        .trim()
        .strip_prefix(command)
        .unwrap_or_default()
        .trim()
        .to_owned();
    if value.is_empty() {
        return Err(ServerError::Message(usage.into()));
    }
    Ok(value)
}

fn print_dashboard(config: &ServerConfig) -> ServerResult<()> {
    println!();
    println!("== OMENchatd Admin ==");
    print!("{}", config::render_status(config));
    print_setup_checklist(config);
    print_rooms(config)?;
    println!();
    Ok(())
}

fn print_setup_checklist(config: &ServerConfig) {
    print!("{}", setup_checklist_text(config));
}

fn setup_checklist_text(config: &ServerConfig) -> String {
    let mut checklist = String::new();
    for item in setup_checklist(config) {
        let marker = if item.ready { "[x]" } else { "[ ]" };
        checklist.push_str(&format!("{marker} {:<18} {}\n", item.label, item.detail));
    }
    let addresses = setup_addresses_text(config);
    let next_steps = setup_next_steps_text(config);
    setup_console_text(&SetupConsoleText {
        checklist: &checklist,
        addresses: &addresses,
        next_steps: &next_steps,
    })
}

fn print_rooms(config: &ServerConfig) -> ServerResult<()> {
    print!("rooms:\n{}", rooms_text(config)?);
    Ok(())
}

fn rooms_text(config: &ServerConfig) -> ServerResult<String> {
    let mut text = String::new();
    for (room_id, name, topic) in config::list_rooms(config)? {
        let topic = topic.unwrap_or_default();
        text.push_str(&room_console_row_text(&RoomConsoleRowText {
            room_id,
            name: &name,
            topic: &topic,
        }));
    }
    Ok(text)
}

fn print_users(config: &ServerConfig) -> ServerResult<()> {
    print!("{}", users_text(config)?);
    Ok(())
}

fn users_text(config: &ServerConfig) -> ServerResult<String> {
    let mut text = String::from("users:\n");
    for user in list_known_users(config)? {
        let first_seen = human_timestamp(user.first_seen_at);
        let last_seen = user
            .last_seen_at
            .map(human_timestamp)
            .unwrap_or_else(|| "never".into());
        let stale_delete = stale_delete_status_label(&user);
        let lxmf_destination = user.lxmf_destination.as_deref().unwrap_or("-");
        text.push_str(&user_console_row_text(&UserConsoleRowText {
            user_id: user.user_id,
            display_name: &user.display_name,
            role_label: role_label(user.role_bits),
            status_label: moderation_status_label(&user),
            first_seen: &first_seen,
            last_seen: &last_seen,
            stale_delete: &stale_delete,
            identity_hex: &user.identity_hex,
            lxmf_destination,
        }));
    }
    if text == "users:\n" {
        text.push_str("  (none)\n");
    }
    Ok(text)
}

fn print_commands() {
    print!("{}", command_help_text());
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;
    use crate::TcpServerOverride;

    fn temp_root(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "omenchatd-tui-{label}-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ))
    }

    #[test]
    fn admin_console_commands_update_config_rooms_and_interface() {
        let root = temp_root("commands");
        let mut config = ServerConfig::for_root(root.clone());
        config::init_files(&config).expect("init");

        assert_eq!(
            apply_admin_command(&mut config, "set-name Field Chat").expect("set name"),
            AdminConsoleAction::Continue
        );
        assert_eq!(
            apply_admin_command(&mut config, "set-operator alice").expect("set operator"),
            AdminConsoleAction::Continue
        );
        assert_eq!(
            apply_admin_command(&mut config, "set-motd Welcome to the field node")
                .expect("set motd"),
            AdminConsoleAction::Continue
        );
        assert_eq!(
            apply_admin_command(&mut config, "set-announce-interval 42")
                .expect("set announce interval"),
            AdminConsoleAction::Continue
        );
        assert_eq!(
            apply_admin_command(&mut config, "set-upload-quota-bytes 123456")
                .expect("set upload quota"),
            AdminConsoleAction::Continue
        );
        assert_eq!(
            apply_admin_command(&mut config, "set-ping-interval 45").expect("set ping interval"),
            AdminConsoleAction::Continue
        );
        assert_eq!(
            apply_admin_command(&mut config, "set-upload-max-file-bytes 654321")
                .expect("set upload max file"),
            AdminConsoleAction::Continue
        );
        assert_eq!(
            apply_admin_command(&mut config, "set-max-message-bytes 4096")
                .expect("set max message bytes"),
            AdminConsoleAction::Continue
        );
        assert_eq!(
            apply_admin_command(&mut config, "set-history-batch-size 25")
                .expect("set history batch size"),
            AdminConsoleAction::Continue
        );
        assert_eq!(
            apply_admin_command(&mut config, "set-join-backlog-events 12")
                .expect("set join backlog"),
            AdminConsoleAction::Continue
        );
        assert_eq!(
            apply_admin_command(&mut config, "set-large-batch-threshold-bytes 8192")
                .expect("set large batch threshold"),
            AdminConsoleAction::Continue
        );
        assert_eq!(
            apply_admin_command(&mut config, "set-rate-messages-per-minute 33")
                .expect("set message rate"),
            AdminConsoleAction::Continue
        );
        assert_eq!(
            apply_admin_command(&mut config, "set-rate-commands-per-minute 17")
                .expect("set command rate"),
            AdminConsoleAction::Continue
        );
        assert_eq!(
            apply_admin_command(&mut config, "add-room ops Field operations").expect("add room"),
            AdminConsoleAction::Continue
        );
        let room_id = config::list_rooms(&config)
            .expect("rooms")
            .into_iter()
            .find(|(_, name, _)| name == "ops")
            .map(|(room_id, _, _)| room_id)
            .expect("ops room");
        assert_eq!(
            apply_admin_command(&mut config, &format!("room-topic {room_id} Updated ops"))
                .expect("set topic"),
            AdminConsoleAction::Continue
        );
        assert_eq!(
            apply_admin_command(&mut config, "tcp-server 127.0.0.1:42420").expect("tcp"),
            AdminConsoleAction::Continue
        );
        assert_eq!(
            apply_admin_command(&mut config, "tcp-client gateway.example:42420")
                .expect("tcp client"),
            AdminConsoleAction::Continue
        );

        let loaded = ServerConfig::load_or_default(root.clone()).expect("load");
        assert_eq!(loaded.name, "Field Chat");
        assert_eq!(loaded.operator_label, "alice");
        assert_eq!(loaded.motd, "Welcome to the field node");
        assert_eq!(loaded.chat_aspect, "node");
        assert_eq!(loaded.announce_interval_minutes, 42);
        assert_eq!(loaded.upload_quota_bytes, 123456);
        assert_eq!(loaded.upload_max_file_bytes, 654321);
        assert_eq!(loaded.ping_interval_seconds, 45);
        assert_eq!(loaded.limits.max_message_bytes, 4096);
        assert_eq!(loaded.limits.history_batch_size, 25);
        assert_eq!(loaded.limits.join_backlog_events, 12);
        assert_eq!(loaded.limits.large_batch_threshold_bytes, 8192);
        assert_eq!(loaded.limits.rate_messages_per_minute, 33);
        assert_eq!(loaded.limits.rate_commands_per_minute, 17);
        assert!(config::list_rooms(&loaded)
            .expect("rooms")
            .iter()
            .any(|(_, name, topic)| name == "ops" && topic.as_deref() == Some("Updated ops")));
        assert!(loaded.reticulum_config_file().exists());
        let reticulum_config =
            std::fs::read_to_string(loaded.reticulum_config_file()).expect("reticulum config");
        assert!(reticulum_config.contains("TCPClientInterface"));
        assert!(reticulum_config.contains("target_host = gateway.example"));
        assert!(reticulum_config.contains("target_port = 42420"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn setup_checklist_reports_interface_readiness() {
        let root = temp_root("setup");
        let config = ServerConfig::for_root(root.clone());
        config::init_files(&config).expect("init");

        let before = setup_checklist(&config);
        assert!(before.iter().any(|item| {
            item.label == "chat service"
                && item.detail.contains("omenchat.node")
                && item.detail.contains("omenchat://<hash>")
        }));
        assert!(before
            .iter()
            .any(|item| item.label == "reticulum" && !item.ready));

        config::write_reticulum_tcp_server_config(
            &config,
            &TcpServerOverride {
                listen_ip: "127.0.0.1".into(),
                listen_port: 42420,
            },
        )
        .expect("tcp config");

        let after = setup_checklist(&config);
        assert!(after
            .iter()
            .any(|item| item.label == "reticulum" && item.ready));
        assert!(after
            .iter()
            .any(|item| item.label == "lobby room" && item.ready));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn overview_operator_summary_points_to_first_actions() {
        let root = temp_root("overview-summary");
        let mut config = ServerConfig::for_root(root.clone());
        config.upload_quota_bytes = 50 * 1024 * 1024;
        config.upload_max_file_bytes = 512 * 1024;

        let text = overview_operator_summary_text(&config, "runtime: live server running");

        assert!(text.contains("overview:"));
        assert!(text.contains("launch: needs setup"));
        assert!(text.contains("live: live server running"));
        assert!(!text.contains("live: runtime:"));
        assert!(text.contains("network:"));
        assert!(text.contains("reticulum/config"));
        assert!(text.contains("rooms:"));
        assert!(text.contains("uploads: max 512.0 KiB, quota 50.0 MiB"));
        assert!(text.contains("share: Portal tab"));
        assert!(text.contains("next:"));

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn setup_next_steps_explain_chat_and_portal_addresses() {
        let root = temp_root("setup-address-rule");
        let config = ServerConfig::for_root(root.clone());
        config::init_files(&config).expect("init");

        let text = setup_next_steps_text(&config);

        assert!(text.contains("OMENchat announces as omenchat.node"));
        assert!(text.contains("share omenchat:// for chat"));
        assert!(text.contains("NomadNet portal URL"));
        assert!(text.contains("Limits: uploads: max file 512.0 KiB"));
        assert!(
            text.contains("Use: Connect To Gateway for normal hosting")
                || text.contains("Use: Start Live Server or run init/status")
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn setup_next_steps_for_ready_server_remind_upload_policy() {
        let root = temp_root("setup-ready-upload-policy");
        let config = ServerConfig::for_root(root.clone());
        config::init_files(&config).expect("init");
        std::fs::write(&config.identity_path, [7u8; 64]).expect("identity");
        config::write_reticulum_tcp_client_config(
            &config,
            &TcpClientOverride {
                target_host: "gateway.example".into(),
                target_port: 42420,
                network_name: None,
                passphrase: None,
            },
        )
        .expect("gateway");

        let text = setup_next_steps_text(&config);

        assert!(text.contains("Launch status: READY for live testing"));
        assert!(text.contains("1. Start Live or press g."));
        assert!(text.contains("Limits:"));
        assert!(text.contains("max file 512.0 KiB"));
        assert!(text.contains("Network: TCP gateway client -> gateway.example:42420"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn setup_addresses_text_keeps_join_and_portal_paths_visible() {
        let root = temp_root("setup-addresses");
        let config = ServerConfig::for_root(root.clone());

        let text = setup_addresses_text(&config);

        assert!(text.contains("Share only after Monitoring shows a connected interface:"));
        assert!(text.contains("Chat invite: omenchat:// URI"));
        assert!(text.contains("Portal page: NomadNet /page/index.mu URL"));
        assert!(text.contains("destination:"));
        assert!(text.contains("Portal file:"));
        assert!(text.contains("reticulum/storage/pages/index.mu"));
        assert!(text.contains("Invite format: omenchat://<destination_hash>"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn setup_actions_include_public_launch_and_policy_controls() {
        let actions = setup_action_specs();
        let labels = actions.iter().map(|(_, label)| *label).collect::<Vec<_>>();
        let action_kinds = actions
            .iter()
            .map(|(action, _)| *action)
            .collect::<Vec<_>>();

        assert!(SETUP_ACTION_PANEL_HEIGHT as usize >= actions.len() + 2);
        assert_eq!(labels[0], "Connect Gateway");
        assert_eq!(labels[1], "Local Listener");
        assert_eq!(labels[2], "Start Live");
        assert_eq!(labels[3], "Announce Now");
        assert_eq!(labels[4], "Open Monitoring");
        assert!(labels.contains(&"Total Upload Quota"));
        assert!(labels.contains(&"Max File Size"));
        assert!(labels.contains(&"Ping Interval"));
        assert!(labels.contains(&"Portal Addresses"));
        assert!(action_kinds.contains(&AdminAction::AnnounceNow));
        assert!(action_kinds.contains(&AdminAction::EditUploadQuotaBytes));
        assert!(action_kinds.contains(&AdminAction::EditUploadMaxFileBytes));
        assert!(action_kinds.contains(&AdminAction::EditPingIntervalSeconds));
        assert!(action_kinds.contains(&AdminAction::EditTcpClient));
        assert!(action_kinds.contains(&AdminAction::EditTcpServer));
        assert!(action_kinds.contains(&AdminAction::SelectTab(AdminTab::Portal)));
    }

    #[test]
    fn overview_actions_prioritize_operator_launch_flow() {
        let actions = overview_action_specs();
        let labels = actions.iter().map(|(_, label)| *label).collect::<Vec<_>>();
        let action_kinds = actions
            .iter()
            .map(|(action, _)| *action)
            .collect::<Vec<_>>();

        assert_eq!(labels[0], "Start Live");
        assert_eq!(labels[1], "Announce Now");
        assert_eq!(labels[2], "Monitoring");
        assert!(labels.contains(&"Connect Gateway"));
        assert!(labels.contains(&"Copy Addresses"));
        assert!(labels.contains(&"Setup + Limits"));
        assert!(labels.contains(&"Stop Live"));
        assert!(action_kinds.contains(&AdminAction::SelectTab(AdminTab::Rooms)));
        assert!(action_kinds.contains(&AdminAction::SelectTab(AdminTab::Moderation)));
        assert!(!action_kinds.contains(&AdminAction::EditMaxMessageBytes));
        assert!(!action_kinds.contains(&AdminAction::EditHistoryBatchSize));
        assert!(!action_kinds.contains(&AdminAction::EditCommandRate));
    }

    #[test]
    fn live_dependent_action_labels_explain_current_state() {
        let actions = [
            (AdminAction::StartLive, "Start Live"),
            (AdminAction::AnnounceNow, "Announce Now"),
            (AdminAction::StopLive, "Stop Live"),
            (AdminAction::SelectTab(AdminTab::Monitoring), "Monitoring"),
        ];

        let stopped = string_actions_for_live_state(&actions, false)
            .into_iter()
            .map(|(_, label)| label)
            .collect::<Vec<_>>();
        assert_eq!(stopped[0], "Start Live");
        assert_eq!(stopped[1], "Announce Now (start live first)");
        assert_eq!(stopped[2], "Stop Live (stopped)");
        assert_eq!(stopped[3], "Monitoring");

        let running = string_actions_for_live_state(&actions, true)
            .into_iter()
            .map(|(_, label)| label)
            .collect::<Vec<_>>();
        assert_eq!(running[0], "Live Running");
        assert_eq!(running[1], "Announce Now");
        assert_eq!(running[2], "Stop Live");
        assert_eq!(running[3], "Monitoring");
    }

    #[test]
    fn monitoring_text_reports_announce_schedule() {
        let root = temp_root("monitoring-announce-schedule");
        let mut config = ServerConfig::for_root(root.clone());
        config.announce_interval_minutes = 42;
        let app = AdminTui::new(config);

        let text = app.monitoring_counter_text();

        assert!(text.contains("announce:"));
        assert!(text.contains("interval 42m"));
        assert!(text.contains("last: none yet"));
        #[cfg(feature = "live-rns-net")]
        assert!(text.contains("start live before Announce Now"));
        #[cfg(not(feature = "live-rns-net"))]
        assert!(text.contains("unavailable without live-rns-net"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn setup_checklist_names_reticulum_interface_mode() {
        let root = temp_root("setup-interface-mode");
        let config = ServerConfig::for_root(root.clone());
        config::init_files(&config).expect("init");
        config::write_reticulum_tcp_server_config(
            &config,
            &crate::TcpServerOverride {
                listen_ip: "127.0.0.1".into(),
                listen_port: 42420,
            },
        )
        .expect("tcp server");

        let reticulum = setup_checklist(&config)
            .into_iter()
            .find(|item| item.label == "reticulum")
            .expect("reticulum checklist item");

        assert!(reticulum.ready);
        assert!(reticulum.detail.contains("local TCP server listener"));
        assert!(reticulum.detail.contains("127.0.0.1:42420"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn setup_action_hitboxes_cover_every_visible_action() {
        let actions = string_actions_for_live_state(&setup_action_specs(), false);
        let panel = Rect::new(10, 5, 40, SETUP_ACTION_PANEL_HEIGHT);
        let hitboxes = action_hitboxes(inner_rect(panel), &actions);

        assert_eq!(hitboxes.len(), actions.len());
        for (index, ((hitbox, action), (expected_action, _))) in
            hitboxes.iter().zip(actions.iter()).enumerate()
        {
            assert_eq!(*action, *expected_action);
            assert_eq!(hitbox.y, panel.y + 1 + index as u16);
            assert!(hitbox.right() <= panel.right());
            assert!(hitbox.bottom() < panel.bottom());
        }
    }

    #[test]
    fn line_console_setup_text_includes_next_steps_and_upload_policy() {
        let root = temp_root("setup-console-guidance");
        let config = ServerConfig::for_root(root.clone());
        config::init_files(&config).expect("init");

        let text = setup_checklist_text(&config);

        assert!(text.contains("setup:"));
        assert!(text.contains("[x] database"));
        assert!(text.contains("next steps:"));
        assert!(text.contains("Launch status: NEEDS SETUP"));
        assert!(text.contains("Next action:"));
        assert!(text.contains("Connect To Gateway"));
        assert!(text.contains("Use: Connect To Gateway for normal hosting"));
        assert!(text.contains("Fix first, then Start Live:"));
        assert!(text.contains("Storage: server files stay under this omenchatd home"));
        assert!(text.contains("share omenchat:// for chat"));
        assert!(text.contains("Limits: uploads: max file 512.0 KiB"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[cfg(feature = "live-rns-net")]
    #[test]
    fn line_console_setup_text_includes_join_addresses() {
        let root = temp_root("setup-console-addresses");
        let config = ServerConfig::for_root(root.clone());
        config::init_files(&config).expect("init");

        let text = setup_checklist_text(&config);

        assert!(text.contains("addresses:"));
        assert!(text.contains("client uri: omenchat://"));
        assert!(text.contains("nomadnet portal: nomadnetwork.node ("));
        assert!(text.contains("portal url: "));
        assert!(text.contains(":/page/index.mu"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn dashboard_input_creates_room_and_updates_config() {
        let root = temp_root("dashboard");
        let config = ServerConfig::for_root(root.clone());
        config::init_files(&config).expect("init");
        let mut app = AdminTui::new(config);

        app.start_input(InputMode::EditName, "Guild Hall".into());
        app.commit_input().expect("name");
        assert_eq!(app.config.name, "Guild Hall");

        app.start_input(InputMode::AddRoomName, "ops".into());
        app.commit_input().expect("room name");
        assert_eq!(app.input_mode, InputMode::AddRoomTopic);
        app.input = "Operations".into();
        app.commit_input().expect("room topic");

        assert!(config::list_rooms(&app.config)
            .expect("rooms")
            .iter()
            .any(|(_, name, topic)| name == "ops" && topic.as_deref() == Some("Operations")));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn dashboard_input_edits_server_limits() {
        let root = temp_root("dashboard-limits");
        let config = ServerConfig::for_root(root.clone());
        config::init_files(&config).expect("init");
        let mut app = AdminTui::new(config);

        app.start_input(InputMode::EditMaxMessageBytes, "4096".into());
        app.commit_input().expect("max message");
        app.start_input(InputMode::EditUploadMaxFileBytes, "524288".into());
        app.commit_input().expect("max upload file");
        app.start_input(InputMode::EditHistoryBatchSize, "25".into());
        app.commit_input().expect("history");
        app.start_input(InputMode::EditJoinBacklogEvents, "12".into());
        app.commit_input().expect("join backlog");
        app.start_input(InputMode::EditLargeBatchThresholdBytes, "8192".into());
        app.commit_input().expect("large threshold");
        app.start_input(InputMode::EditMessageRate, "33".into());
        app.commit_input().expect("message rate");
        app.start_input(InputMode::EditCommandRate, "17".into());
        app.commit_input().expect("command rate");

        let loaded = ServerConfig::load_or_default(root.clone()).expect("load");
        assert_eq!(loaded.limits.max_message_bytes, 4096);
        assert_eq!(loaded.upload_max_file_bytes, 524288);
        assert_eq!(loaded.limits.history_batch_size, 25);
        assert_eq!(loaded.limits.join_backlog_events, 12);
        assert_eq!(loaded.limits.large_batch_threshold_bytes, 8192);
        assert_eq!(loaded.limits.rate_messages_per_minute, 33);
        assert_eq!(loaded.limits.rate_commands_per_minute, 17);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn dashboard_edits_and_archives_selected_room() {
        let root = temp_root("room-admin");
        let config = ServerConfig::for_root(root.clone());
        config::init_files(&config).expect("init");
        config::add_room(&config, "ops", Some("Old")).expect("room");
        let mut app = AdminTui::new(config);
        app.tab = AdminTab::Rooms;
        app.selected_room = config::list_rooms(&app.config)
            .expect("rooms")
            .iter()
            .position(|(_, name, _)| name == "ops")
            .expect("ops room");

        app.start_selected_room_topic_edit();
        assert_eq!(app.input_mode, InputMode::EditRoomTopic);
        app.input = "New".into();
        app.commit_input().expect("topic");
        assert!(config::list_rooms(&app.config)
            .expect("rooms")
            .iter()
            .any(|(_, name, topic)| name == "ops" && topic.as_deref() == Some("New")));

        app.selected_room = config::list_rooms(&app.config)
            .expect("rooms")
            .iter()
            .position(|(_, name, _)| name == "ops")
            .expect("ops room");
        app.archive_selected_room().expect("arm archive");
        assert!(app.status.contains("archive armed"));
        assert!(config::list_rooms(&app.config)
            .expect("rooms")
            .iter()
            .any(|(_, name, _)| name == "ops"));
        app.archive_selected_room().expect("archive");
        assert!(!config::list_rooms(&app.config)
            .expect("rooms")
            .iter()
            .any(|(_, name, _)| name == "ops"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn room_action_status_text_explains_operator_effects() {
        let root = temp_root("room-status-effects");
        let config = ServerConfig::for_root(root.clone());
        config::init_files(&config).expect("init");
        config::add_room(&config, "ops", Some("Old")).expect("room");
        let mut app = AdminTui::new(config);
        app.tab = AdminTab::Rooms;

        app.start_input(InputMode::AddRoomName, "radio".into());
        app.commit_input().expect("room name");
        app.input = "Radio room".into();
        app.commit_input().expect("room topic");
        assert!(app.status.contains("#radio is visible to clients"));
        assert!(app.status.contains("mods/admins can edit"));

        app.selected_room = config::list_rooms(&app.config)
            .expect("rooms")
            .iter()
            .position(|(_, name, _)| name == "ops")
            .expect("ops room");
        app.start_selected_room_topic_edit();
        app.input = "New".into();
        app.commit_input().expect("topic");
        assert!(app.status.contains("clients will see it on sync"));

        app.selected_room = config::list_rooms(&app.config)
            .expect("rooms")
            .iter()
            .position(|(_, name, _)| name == "ops")
            .expect("ops room");
        app.archive_selected_room().expect("arm archive");
        assert!(app.status.contains("hide it from clients"));
        assert!(app.status.contains("history stays stored"));
        app.archive_selected_room().expect("archive");
        assert!(app.status.contains("hidden from room lists"));
        assert!(app.status.contains("history was retained"));

        app.selected_room = config::list_rooms(&app.config)
            .expect("rooms")
            .iter()
            .position(|(room_id, _, _)| *room_id == 1)
            .expect("lobby room");
        app.archive_selected_room().expect("archive lobby");
        assert!(app.status.contains("#lobby is protected"));
        assert!(app.status.contains("cannot be archived"));

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn dashboard_does_not_arm_lobby_archive() {
        let root = temp_root("lobby-archive");
        let config = ServerConfig::for_root(root.clone());
        config::init_files(&config).expect("init");
        let mut app = AdminTui::new(config);
        app.tab = AdminTab::Rooms;
        app.selected_room = config::list_rooms(&app.config)
            .expect("rooms")
            .iter()
            .position(|(room_id, _, _)| *room_id == 1)
            .expect("lobby room");

        app.archive_selected_room().expect("archive lobby");

        assert_eq!(app.pending_archive_room_id, None);
        assert!(app.status.contains("cannot be archived"));
        assert!(config::list_rooms(&app.config)
            .expect("rooms")
            .iter()
            .any(|(room_id, name, _)| *room_id == 1 && name == "lobby"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn tab_hitboxes_match_drawn_tab_order() {
        let root = temp_root("hitboxes");
        let config = ServerConfig::for_root(root.clone());
        let mut app = AdminTui::new(config);
        app.tab_clicks = tab_hitboxes(Rect::new(0, 0, 120, 3));

        let moderation_hit = app
            .tab_clicks
            .iter()
            .find(|(_, tab)| *tab == AdminTab::Moderation)
            .map(|(area, _)| Position::new(area.x, area.y))
            .expect("moderation hitbox");
        app.handle_click(moderation_hit.x, moderation_hit.y)
            .expect("click");
        assert_eq!(app.tab, AdminTab::Moderation);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn tab_hitboxes_do_not_drift_from_rendered_spans() {
        let root = temp_root("hitbox-drift");
        let config = ServerConfig::for_root(root.clone());
        let mut app = AdminTui::new(config);
        app.tab_clicks = tab_hitboxes(Rect::new(0, 0, 120, 3));

        for (area, tab) in app.tab_clicks.clone() {
            app.handle_click(area.x + area.width / 2, area.y)
                .expect("click tab");
            assert_eq!(app.tab, tab);
        }
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn room_actions_show_required_admin_roles() {
        let empty = room_actions(None, None);
        assert_eq!(
            empty,
            vec![
                (AdminAction::AddRoom, "Add Room (admin)".into()),
                (AdminAction::EditRoomTopic, "Edit Topic (mod/admin)".into()),
                (AdminAction::ArchiveRoom, "Archive Room (admin)".into()),
            ]
        );

        let lobby = room_actions(Some((1, "lobby", None)), None);
        assert_eq!(
            lobby[2],
            (AdminAction::ArchiveRoom, "Lobby Protected".into())
        );

        let confirm = room_actions(Some((2, "ops", Some("Ops"))), Some(2));
        assert_eq!(
            confirm[2],
            (AdminAction::ArchiveRoom, "Confirm Archive".into())
        );
    }

    #[test]
    fn moderation_actions_name_the_next_selected_user_action() {
        let mut user = AdminUserRow {
            user_id: 7,
            identity_hash: b"peer".to_vec(),
            identity_hex: "70656572".into(),
            display_name: "Peer".into(),
            role_bits: 0,
            status_bits: 0,
            lxmf_destination: None,
            first_seen_at: 1,
            last_seen_at: Some(2),
            trusted: false,
            banned: false,
            muted: false,
        };
        let labels = moderation_actions(Some(&user), None, false)
            .into_iter()
            .collect::<Vec<_>>();
        assert_eq!(
            labels[0],
            (AdminAction::ToggleBan, "Ban Access + Close".into())
        );
        assert_eq!(
            labels[1],
            (AdminAction::KickActiveUser, "Close Active Links".into())
        );
        assert_eq!(labels[2], (AdminAction::ToggleMute, "Mute Sending".into()));
        assert_eq!(labels[3], (AdminAction::ToggleTrust, "Trust Media".into()));
        assert_eq!(
            labels[4],
            (AdminAction::SetRole(0), "Role: Standard".into())
        );
        assert_eq!(
            labels[5],
            (AdminAction::SetRole(ROLE_TRUSTED), "Make Trusted".into())
        );
        assert_eq!(
            labels[6],
            (
                AdminAction::SetRole(ROLE_TRUSTED | ROLE_MODERATOR),
                "Make Moderator".into()
            )
        );
        assert_eq!(
            labels[7],
            (
                AdminAction::SetRole(ROLE_TRUSTED | ROLE_MODERATOR | ROLE_ADMIN),
                "Make Admin".into()
            )
        );

        user.trusted = true;
        user.banned = true;
        user.muted = true;
        user.role_bits = ROLE_TRUSTED;
        let labels = moderation_actions(Some(&user), None, false)
            .into_iter()
            .collect::<Vec<_>>();
        assert_eq!(
            labels[0],
            (AdminAction::ToggleBan, "Unban Future Access".into())
        );
        assert_eq!(
            labels[1],
            (AdminAction::KickActiveUser, "Close Active Links".into())
        );
        assert_eq!(
            labels[2],
            (AdminAction::ToggleMute, "Unmute Sending".into())
        );
        assert_eq!(
            labels[3],
            (AdminAction::ToggleTrust, "Untrust Media".into())
        );
        assert_eq!(labels[4], (AdminAction::SetRole(0), "Make Standard".into()));
        assert_eq!(
            labels[5],
            (AdminAction::SetRole(ROLE_TRUSTED), "Role: Trusted".into())
        );
        assert_eq!(
            labels[6],
            (
                AdminAction::SetRole(ROLE_TRUSTED | ROLE_MODERATOR),
                "Make Moderator".into()
            )
        );

        user.role_bits = ROLE_TRUSTED | ROLE_MODERATOR | ROLE_ADMIN;
        let labels = moderation_actions(Some(&user), None, false)
            .into_iter()
            .collect::<Vec<_>>();
        assert_eq!(labels[4], (AdminAction::SetRole(0), "Make Standard".into()));
        assert_eq!(
            labels[7],
            (
                AdminAction::SetRole(ROLE_TRUSTED | ROLE_MODERATOR | ROLE_ADMIN),
                "Role: Admin".into()
            )
        );
        let labels = moderation_actions(Some(&user), Some(user.user_id), false)
            .into_iter()
            .collect::<Vec<_>>();
        assert_eq!(
            labels[8],
            (AdminAction::DeleteStaleUser, "Confirm Delete Record".into())
        );
        assert_eq!(
            labels[9],
            (AdminAction::PruneStaleUsers, "Prune Stale Users".into())
        );
        let labels = moderation_actions(Some(&user), None, true)
            .into_iter()
            .collect::<Vec<_>>();
        assert_eq!(
            labels[9],
            (AdminAction::PruneStaleUsers, "Confirm Prune Records".into())
        );
    }

    #[test]
    fn admin_tui_list_labels_fit_narrow_widths() {
        let room = room_list_label(
            ">",
            42,
            "long-room-name",
            Some("A very long topic that would otherwise overflow the room list"),
            24,
        );
        assert!(room.chars().count() <= 24);
        assert!(room.ends_with("..."));

        let user = AdminUserRow {
            user_id: 7,
            identity_hash: b"peer".to_vec(),
            identity_hex: "70656572".into(),
            display_name: "Extremely Long Display Name".into(),
            role_bits: ROLE_TRUSTED | ROLE_MODERATOR,
            status_bits: STATUS_MUTED,
            lxmf_destination: None,
            first_seen_at: 1,
            last_seen_at: Some(2),
            trusted: true,
            banned: false,
            muted: true,
        };
        let label = moderation_user_list_label(
            ">",
            &moderation_user_text(&user),
            stale_user_age_secs(&user),
            USER_DELETE_MIN_AGE_SECS,
            3,
            32,
        );
        assert!(label.chars().count() <= 32);
        assert!(label.ends_with("..."));
    }

    #[test]
    fn setup_checklist_lines_fit_narrow_widths() {
        let ready = SetupChecklistItem {
            label: "reticulum",
            ready: true,
            detail: "TCP gateway client -> very-long-gateway.example:42420".into(),
        };
        let line = setup_checklist_line("[x]", &ready, 28);
        let text = line
            .spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect::<String>();

        assert!(text.chars().count() <= 28);
        assert!(text.starts_with("[x] reticulum"));
        assert!(text.ends_with("..."));
        assert_eq!(line.spans[0].style.fg, Some(Color::Green));

        let missing = SetupChecklistItem {
            label: "identity",
            ready: false,
            detail: "missing identity".into(),
        };
        let line = setup_checklist_line("[ ]", &missing, 80);
        assert_eq!(line.spans[0].style.fg, Some(Color::Yellow));
    }

    #[test]
    fn dashboard_action_clicks_run_room_actions() {
        let root = temp_root("room-action-clicks");
        let config = ServerConfig::for_root(root.clone());
        config::init_files(&config).expect("init");
        let mut app = AdminTui::new(config);
        app.tab = AdminTab::Rooms;
        app.action_clicks = vec![(Rect::new(4, 4, 20, 1), AdminAction::AddRoom)];

        app.handle_click(5, 4).expect("click action");

        assert_eq!(app.input_mode, InputMode::AddRoomName);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn dashboard_action_clicks_run_overview_actions() {
        let root = temp_root("overview-action-clicks");
        let config = ServerConfig::for_root(root.clone());
        config::init_files(&config).expect("init");
        let mut app = AdminTui::new(config);
        app.tab = AdminTab::Overview;
        app.action_clicks = vec![(Rect::new(4, 4, 30, 1), AdminAction::EditOperator)];

        app.handle_click(5, 4).expect("click edit operator");

        assert_eq!(app.input_mode, InputMode::EditOperator);
        app.input_mode = InputMode::Navigate;
        app.action_clicks = vec![
            (Rect::new(4, 4, 30, 1), AdminAction::EditMaxMessageBytes),
            (Rect::new(4, 5, 30, 1), AdminAction::EditUploadMaxFileBytes),
            (
                Rect::new(4, 6, 30, 1),
                AdminAction::SelectTab(AdminTab::Rooms),
            ),
            (Rect::new(4, 7, 30, 1), AdminAction::SaveConfig),
        ];
        app.handle_click(5, 4).expect("click max message limit");
        assert_eq!(app.input_mode, InputMode::EditMaxMessageBytes);
        assert_eq!(app.input, app.config.limits.max_message_bytes.to_string());
        app.input_mode = InputMode::Navigate;
        app.handle_click(5, 5).expect("click max upload file limit");
        assert_eq!(app.input_mode, InputMode::EditUploadMaxFileBytes);
        assert_eq!(app.input, app.config.upload_max_file_bytes.to_string());
        app.input_mode = InputMode::Navigate;
        app.handle_click(5, 6).expect("click rooms");
        assert_eq!(app.tab, AdminTab::Rooms);
        app.handle_click(5, 7).expect("click save");
        assert!(app.status.contains("config saved"));
        assert!(app.status.contains("restart live server"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn setup_and_overview_status_text_explains_next_steps() {
        let root = temp_root("setup-status-effects");
        let config = ServerConfig::for_root(root.clone());
        config::init_files(&config).expect("init");
        let mut app = AdminTui::new(config);

        app.start_input(InputMode::EditName, "Signal Lodge".into());
        app.commit_input().expect("server name");
        assert!(app.status.contains("clients will see it"));

        app.start_input(InputMode::EditOperator, "Ops".into());
        app.commit_input().expect("operator");
        assert!(app.status.contains("status/setup output"));

        app.start_input(InputMode::EditTcpClient, "gateway.example:42420".into());
        app.commit_input().expect("tcp client");
        assert!(app.status.contains("gateway saved: gateway.example:42420"));
        assert!(app.status.contains("restart live server"));
        assert!(app.status.contains("check Monitoring"));

        app.start_input(InputMode::EditTcpServer, "127.0.0.1:42420".into());
        app.commit_input().expect("tcp server");
        assert!(app.status.contains("local listener saved: 127.0.0.1:42420"));
        assert!(app.status.contains("restart live server"));

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn live_and_save_status_text_explains_operator_state() {
        let root = temp_root("live-save-status");
        let config = ServerConfig::for_root(root.clone());
        config::init_files(&config).expect("init");
        let mut app = AdminTui::new(config);

        app.save_config_with_status().expect("save");
        assert!(app.status.contains("config saved"));
        assert!(app.status.contains("restart live server"));

        #[cfg(not(feature = "live-rns-net"))]
        {
            app.start_live_runtime();
            assert!(app.status.contains("live server unavailable"));
            assert!(app.status.contains("live-rns-net"));

            app.stop_live_runtime();
            assert!(app.status.contains("live server unavailable"));
            assert!(app.status.contains("live-rns-net"));
        }

        #[cfg(feature = "live-rns-net")]
        {
            app.stop_live_runtime();
            assert!(app.status.contains("live server is not running"));
            assert!(app.status.contains("Start Live Server"));
        }

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn dashboard_action_clicks_report_live_and_save_status() {
        let root = temp_root("live-save-clicks");
        let config = ServerConfig::for_root(root.clone());
        config::init_files(&config).expect("init");
        let mut app = AdminTui::new(config);
        app.tab = AdminTab::Overview;
        app.action_clicks = vec![
            (Rect::new(4, 4, 30, 1), AdminAction::SaveConfig),
            (Rect::new(4, 5, 30, 1), AdminAction::StartLive),
            (Rect::new(4, 6, 30, 1), AdminAction::StopLive),
        ];

        app.handle_click(5, 4).expect("click save");
        assert!(app.status.contains("config saved"));
        assert!(app.status.contains("restart live server"));

        app.handle_click(5, 5).expect("click start live");
        #[cfg(not(feature = "live-rns-net"))]
        {
            assert!(app.status.contains("live server unavailable"));
            assert!(app.status.contains("live-rns-net"));
        }
        #[cfg(feature = "live-rns-net")]
        {
            assert!(
                app.status.contains("live server started")
                    || app.status.contains("live startup failed")
            );
        }

        app.handle_click(5, 6).expect("click stop live");
        #[cfg(not(feature = "live-rns-net"))]
        {
            assert!(app.status.contains("live server unavailable"));
            assert!(app.status.contains("live-rns-net"));
        }
        #[cfg(feature = "live-rns-net")]
        {
            assert!(
                app.status.contains("live server stopped")
                    || app.status.contains("live server is not running")
            );
        }

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn dashboard_announce_now_reports_when_live_is_stopped() {
        let root = temp_root("announce-now-stopped");
        let config = ServerConfig::for_root(root.clone());
        config::init_files(&config).expect("init");
        let mut app = AdminTui::new(config);

        app.handle_admin_action(AdminAction::AnnounceNow)
            .expect("announce now");

        #[cfg(feature = "live-rns-net")]
        assert!(app.status.contains("live server is not running"));
        #[cfg(not(feature = "live-rns-net"))]
        assert!(app.status.contains("live announce unavailable"));

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn dashboard_action_clicks_run_interface_actions() {
        let root = temp_root("interface-action-clicks");
        let config = ServerConfig::for_root(root.clone());
        config::init_files(&config).expect("init");
        let mut app = AdminTui::new(config);
        app.tab = AdminTab::Interfaces;
        app.action_clicks = vec![(Rect::new(4, 4, 30, 1), AdminAction::EditTcpServer)];

        app.handle_click(5, 4).expect("click interface action");

        assert_eq!(app.input_mode, InputMode::EditTcpServer);
        app.input_mode = InputMode::Navigate;
        app.action_clicks = vec![(
            Rect::new(4, 5, 30, 1),
            AdminAction::SelectTab(AdminTab::Monitoring),
        )];
        app.handle_click(5, 5).expect("click monitoring");
        assert_eq!(app.tab, AdminTab::Monitoring);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn dashboard_action_clicks_run_user_actions() {
        let root = temp_root("user-action-clicks");
        let config = ServerConfig::for_root(root.clone());
        config::init_files(&config).expect("init");
        let connection = rusqlite::Connection::open(&config.database_path).expect("db");
        connection
            .execute(
                "INSERT INTO users(rns_identity_hash, display_name, role_bits, status_bits, first_seen_at, last_seen_at)
                 VALUES (?1, 'Alice', 0, 0, 1, 1)",
                [b"peer-a".as_slice()],
            )
            .expect("insert user");
        drop(connection);
        let mut app = AdminTui::new(config);
        app.tab = AdminTab::Moderation;
        app.action_clicks = vec![(Rect::new(4, 4, 20, 1), AdminAction::ToggleMute)];

        app.handle_click(5, 4).expect("click action");

        let user = list_known_users(&app.config).expect("users").remove(0);
        assert!(user.muted);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn dashboard_can_kick_selected_user_without_changing_moderation_flags() {
        let root = temp_root("user-kick-links");
        let config = ServerConfig::for_root(root.clone());
        config::init_files(&config).expect("init");
        let connection = rusqlite::Connection::open(&config.database_path).expect("db");
        connection
            .execute(
                "INSERT INTO users(rns_identity_hash, display_name, role_bits, status_bits, first_seen_at, last_seen_at)
                 VALUES (?1, 'Alice', 0, 0, 1, 1)",
                [b"peer-a".as_slice()],
            )
            .expect("insert user");
        drop(connection);
        let mut app = AdminTui::new(config);
        app.tab = AdminTab::Moderation;

        app.kick_selected_user_links().expect("kick links");

        assert!(app.status.contains("no active links to kick"));
        let user = list_known_users(&app.config).expect("users").remove(0);
        assert!(!user.banned);
        assert!(!user.muted);
        let log = std::fs::read_to_string(app.config.log_path()).expect("log");
        assert!(log.contains("admin kicked active links user"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn mouse_wheel_only_changes_selection_over_active_list() {
        let root = temp_root("mouse-wheel-hitbox");
        let config = ServerConfig::for_root(root.clone());
        config::init_files(&config).expect("init");
        config::add_room(&config, "ops", Some("Ops")).expect("room");
        let mut app = AdminTui::new(config);
        app.tab = AdminTab::Rooms;
        app.room_list_area = Rect::new(2, 4, 30, 3);

        app.handle_scroll(80, 4, true);
        assert_eq!(app.selected_room, 0);
        app.handle_scroll(3, 4, true);
        assert_eq!(app.selected_room, 1);
        app.handle_scroll(3, 4, false);
        assert_eq!(app.selected_room, 0);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn help_panel_mouse_wheel_scrolls_help_text_without_room_selection() {
        let root = temp_root("help-scroll");
        let config = ServerConfig::for_root(root.clone());
        config::init_files(&config).expect("init");
        config::add_room(&config, "ops", Some("Ops")).expect("room");
        let mut app = AdminTui::new(config);
        app.tab = AdminTab::Help;

        app.handle_scroll(80, 4, true);
        assert_eq!(app.selected_room, 0);
        assert!(app.help_scroll > 0);

        app.handle_scroll(80, 4, false);
        assert_eq!(app.help_scroll, 0);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn room_selection_clears_pending_archive_confirmation() {
        let root = temp_root("archive-selection-clear");
        let config = ServerConfig::for_root(root.clone());
        config::init_files(&config).expect("init");
        config::add_room(&config, "ops", Some("Ops")).expect("ops");
        config::add_room(&config, "dev", Some("Dev")).expect("dev");
        let mut app = AdminTui::new(config);
        app.tab = AdminTab::Rooms;
        app.room_list_area = Rect::new(2, 4, 30, 4);
        app.selected_room = config::list_rooms(&app.config)
            .expect("rooms")
            .iter()
            .position(|(_, name, _)| name == "ops")
            .expect("ops room");

        app.archive_selected_room().expect("arm archive");
        assert!(app.pending_archive_room_id.is_some());
        app.handle_click(3, 4).expect("select another row");
        assert_eq!(app.pending_archive_room_id, None);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn room_mouse_selection_reports_selected_room() {
        let root = temp_root("room-select-status");
        let config = ServerConfig::for_root(root.clone());
        config::init_files(&config).expect("init");
        config::add_room(&config, "ops", Some("Operations")).expect("room");

        let mut app = AdminTui::new(config);
        app.tab = AdminTab::Rooms;
        app.room_list_area = Rect::new(2, 4, 30, 4);
        app.pending_archive_room_id = Some(2);

        app.handle_click(3, 5).expect("select second row");

        assert_eq!(app.selected_room, 1);
        assert_eq!(app.pending_archive_room_id, None);
        assert!(app.status.contains("selected room #ops"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn moderation_flags_known_users() {
        let root = temp_root("moderation");
        let config = ServerConfig::for_root(root.clone());
        config::init_files(&config).expect("init");
        let connection = rusqlite::Connection::open(&config.database_path).expect("db");
        connection
            .execute(
                "INSERT INTO users(rns_identity_hash, display_name, role_bits, status_bits, first_seen_at, last_seen_at)
                 VALUES (?1, 'Alice', 0, 0, 1, 1)",
                [b"peer-a".as_slice()],
            )
            .expect("insert user");
        drop(connection);

        let mut app = AdminTui::new(config);
        app.tab = AdminTab::Moderation;
        app.toggle_selected_user_trust().expect("trust");
        app.toggle_selected_user_mute().expect("mute");
        app.toggle_selected_user_ban().expect("ban");
        app.cycle_selected_user_role().expect("role");
        app.set_selected_user_role(ROLE_TRUSTED | ROLE_MODERATOR | ROLE_ADMIN)
            .expect("set admin role");

        let users = list_known_users(&app.config).expect("users");
        assert_eq!(role_label(users[0].role_bits), "admin");
        assert!(users[0].muted);
        assert!(users[0].banned);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn moderation_status_text_explains_action_effects() {
        let root = temp_root("moderation-status-copy");
        let config = ServerConfig::for_root(root.clone());
        config::init_files(&config).expect("init");
        let connection = rusqlite::Connection::open(&config.database_path).expect("db");
        connection
            .execute(
                "INSERT INTO users(rns_identity_hash, display_name, role_bits, status_bits, first_seen_at, last_seen_at)
                 VALUES (?1, 'Alice', 0, 0, 1, 1)",
                [b"peer-a".as_slice()],
            )
            .expect("insert user");
        drop(connection);

        let mut app = AdminTui::new(config);
        app.tab = AdminTab::Moderation;

        app.toggle_selected_user_trust().expect("trust");
        assert!(app.status.contains("trusted-media affordances enabled"));
        app.toggle_selected_user_mute().expect("mute");
        assert!(app.status.contains("reading allowed, sending blocked"));
        app.set_selected_user_role(ROLE_MODERATOR).expect("role");
        assert!(app.status.contains("role set to mod"));
        app.toggle_selected_user_ban().expect("ban");
        assert!(app.status.contains("future sessions blocked"));

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn admin_console_moderates_known_users_by_id() {
        let root = temp_root("console-moderation");
        let mut config = ServerConfig::for_root(root.clone());
        config::init_files(&config).expect("init");
        let connection = rusqlite::Connection::open(&config.database_path).expect("db");
        connection
            .execute(
                "INSERT INTO users(rns_identity_hash, display_name, role_bits, status_bits, first_seen_at, last_seen_at)
                 VALUES (?1, 'Alice', 0, 0, 1, 1)",
                [b"peer-a".as_slice()],
            )
            .expect("insert user");
        drop(connection);

        let user_id = list_known_users(&config).expect("users")[0].user_id;
        apply_admin_command(&mut config, &format!("trust-user {user_id}")).expect("trust");
        apply_admin_command(&mut config, &format!("mute-user {user_id}")).expect("mute");
        apply_admin_command(&mut config, &format!("set-user-role {user_id} admin")).expect("admin");
        apply_admin_command(&mut config, &format!("ban-user {user_id}")).expect("ban");
        let user = list_known_users(&config).expect("users").remove(0);
        assert_eq!(role_label(user.role_bits), "admin");
        assert!(user.muted);
        assert!(user.banned);

        apply_admin_command(&mut config, &format!("set-user-role {user_id} standard"))
            .expect("standard");
        apply_admin_command(&mut config, &format!("unmute-user {user_id}")).expect("unmute");
        apply_admin_command(&mut config, &format!("untrust-user {user_id}")).expect("untrust");
        apply_admin_command(&mut config, &format!("unban-user {user_id}")).expect("unban");
        let user = list_known_users(&config).expect("users").remove(0);
        assert!(!user.trusted);
        assert!(!user.muted);
        assert!(!user.banned);
        let log = std::fs::read_to_string(config.log_path()).expect("audit log");
        assert!(log.contains("admin console trusted user"));
        assert!(log.contains("admin console muted user"));
        assert!(log.contains("admin console set user role"));
        assert!(log.contains("admin console banned user"));
        assert!(log.contains("admin console untrusted user"));
        assert!(log.contains("admin console unmuted user"));
        assert!(log.contains("admin console unbanned user"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn users_text_reports_human_times_and_delete_eligibility() {
        let root = temp_root("users-text");
        let config = ServerConfig::for_root(root.clone());
        config::init_files(&config).expect("init");
        let now = current_unix_secs();
        let connection = rusqlite::Connection::open(&config.database_path).expect("db");
        connection
            .execute(
                "INSERT INTO users(rns_identity_hash, display_name, role_bits, status_bits, lxmf_destination, first_seen_at, last_seen_at)
                 VALUES (?1, 'Old Peer', ?2, ?3, 'lxmf-old', ?4, ?5)",
                rusqlite::params![
                    b"old-peer".as_slice(),
                    (ROLE_TRUSTED | ROLE_MODERATOR) as i64,
                    STATUS_MUTED as i64,
                    now - 100_000,
                    now - 90_000
                ],
            )
            .expect("insert old user");
        connection
            .execute(
                "INSERT INTO users(rns_identity_hash, display_name, role_bits, status_bits, first_seen_at, last_seen_at)
                 VALUES (?1, 'Fresh Peer', 0, 0, ?2, ?3)",
                rusqlite::params![b"fresh-peer".as_slice(), now - 100, now],
            )
            .expect("insert fresh user");
        drop(connection);

        let text = users_text(&config).expect("users text");

        assert!(text.contains("Old Peer"));
        assert!(text.contains("role=mod"));
        assert!(text.contains("status=muted"));
        assert!(text.contains("first="));
        assert!(text.contains("last="));
        assert!(text.contains("stale_delete=\"eligible"));
        assert!(text.contains("lxmf=lxmf-old"));
        assert!(text.contains("Fresh Peer"));
        assert!(text.contains("stale_delete=\"available in"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn dashboard_deletes_only_stale_known_users() {
        let root = temp_root("stale-user-delete");
        let config = ServerConfig::for_root(root.clone());
        config::init_files(&config).expect("init");
        let now = current_unix_secs();
        let connection = rusqlite::Connection::open(&config.database_path).expect("db");
        connection
            .execute(
                "INSERT INTO users(rns_identity_hash, display_name, role_bits, status_bits, first_seen_at, last_seen_at)
                 VALUES (?1, 'Old Peer', 0, 0, ?2, ?3)",
                rusqlite::params![b"old-peer".as_slice(), now - 100_000, now - 90_000],
            )
            .expect("insert old user");
        connection
            .execute(
                "INSERT INTO users(rns_identity_hash, display_name, role_bits, status_bits, first_seen_at, last_seen_at)
                 VALUES (?1, 'Fresh Peer', 0, 0, ?2, ?3)",
                rusqlite::params![b"fresh-peer".as_slice(), now - 100, now],
            )
            .expect("insert fresh user");
        drop(connection);

        let mut app = AdminTui::new(config);
        app.tab = AdminTab::Moderation;
        let users = list_known_users(&app.config).expect("users");
        app.selected_user = users
            .iter()
            .position(|user| user.display_name == "Fresh Peer")
            .expect("fresh user");
        app.delete_selected_stale_user().expect("fresh blocked");
        assert!(app.status.contains("seen too recently"));
        assert_eq!(app.pending_delete_user_id, None);
        assert!(list_known_users(&app.config)
            .expect("users")
            .iter()
            .any(|user| user.display_name == "Fresh Peer"));

        let users = list_known_users(&app.config).expect("users");
        app.selected_user = users
            .iter()
            .position(|user| user.display_name == "Old Peer")
            .expect("old user");
        app.delete_selected_stale_user().expect("arm delete old");
        assert!(app.status.contains("Confirm Delete"));
        assert!(app.pending_delete_user_id.is_some());
        assert!(list_known_users(&app.config)
            .expect("users")
            .iter()
            .any(|user| user.display_name == "Old Peer"));
        app.delete_selected_stale_user().expect("delete old");
        let users = list_known_users(&app.config).expect("users");
        assert!(!users.iter().any(|user| user.display_name == "Old Peer"));
        assert!(users.iter().any(|user| user.display_name == "Fresh Peer"));
        let log = std::fs::read_to_string(app.config.log_path()).expect("audit log");
        assert!(log.contains("admin deleted stale user"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn dashboard_clears_stale_delete_confirmation_on_user_navigation_and_actions() {
        let root = temp_root("stale-user-delete-clear");
        let config = ServerConfig::for_root(root.clone());
        config::init_files(&config).expect("init");
        let now = current_unix_secs();
        let connection = rusqlite::Connection::open(&config.database_path).expect("db");
        connection
            .execute(
                "INSERT INTO users(rns_identity_hash, display_name, role_bits, status_bits, first_seen_at, last_seen_at)
                 VALUES (?1, 'Old One', 0, 0, ?2, ?3)",
                rusqlite::params![b"old-one".as_slice(), now - 100_000, now - 90_000],
            )
            .expect("insert old one");
        connection
            .execute(
                "INSERT INTO users(rns_identity_hash, display_name, role_bits, status_bits, first_seen_at, last_seen_at)
                 VALUES (?1, 'Old Two', 0, 0, ?2, ?3)",
                rusqlite::params![b"old-two".as_slice(), now - 100_000, now - 90_000],
            )
            .expect("insert old two");
        drop(connection);

        let mut app = AdminTui::new(config);
        app.tab = AdminTab::Moderation;
        app.selected_user = 0;
        app.delete_selected_stale_user().expect("arm delete");
        assert!(app.pending_delete_user_id.is_some());
        app.select_next_user();
        assert_eq!(app.pending_delete_user_id, None);

        app.delete_selected_stale_user().expect("arm delete again");
        assert!(app.pending_delete_user_id.is_some());
        app.toggle_selected_user_mute()
            .expect("mute clears pending");
        assert_eq!(app.pending_delete_user_id, None);
        assert_eq!(list_known_users(&app.config).expect("users").len(), 2);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn dashboard_clears_destructive_confirmations_on_tab_change() {
        let root = temp_root("tab-clears-confirmations");
        let config = ServerConfig::for_root(root.clone());
        config::init_files(&config).expect("init");
        let mut app = AdminTui::new(config);
        app.tab = AdminTab::Rooms;
        app.pending_archive_room_id = Some(2);
        app.pending_delete_user_id = Some(7);
        app.pending_prune_stale_users = true;

        app.select_tab(AdminTab::Moderation);

        assert_eq!(app.pending_archive_room_id, None);
        assert_eq!(app.pending_delete_user_id, None);
        assert!(!app.pending_prune_stale_users);

        app.pending_archive_room_id = Some(3);
        app.pending_delete_user_id = Some(8);
        app.pending_prune_stale_users = true;
        app.select_tab(AdminTab::Moderation);

        assert_eq!(app.pending_archive_room_id, Some(3));
        assert_eq!(app.pending_delete_user_id, Some(8));
        assert!(app.pending_prune_stale_users);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn admin_console_deletes_only_stale_known_users_by_id() {
        let root = temp_root("console-stale-user-delete");
        let mut config = ServerConfig::for_root(root.clone());
        config::init_files(&config).expect("init");
        let now = current_unix_secs();
        let connection = rusqlite::Connection::open(&config.database_path).expect("db");
        connection
            .execute(
                "INSERT INTO users(rns_identity_hash, display_name, role_bits, status_bits, first_seen_at, last_seen_at)
                 VALUES (?1, 'Old Peer', 0, 0, ?2, ?3)",
                rusqlite::params![b"old-peer".as_slice(), now - 100_000, now - 90_000],
            )
            .expect("insert old user");
        connection
            .execute(
                "INSERT INTO users(rns_identity_hash, display_name, role_bits, status_bits, first_seen_at, last_seen_at)
                 VALUES (?1, 'Fresh Peer', 0, 0, ?2, ?3)",
                rusqlite::params![b"fresh-peer".as_slice(), now - 100, now],
            )
            .expect("insert fresh user");
        drop(connection);

        let users = list_known_users(&config).expect("users");
        let fresh_id = users
            .iter()
            .find(|user| user.display_name == "Fresh Peer")
            .expect("fresh user")
            .user_id;
        let fresh_result = apply_admin_command(&mut config, &format!("delete-user {fresh_id}"));
        let error = fresh_result.expect_err("fresh delete blocked");
        assert!(error.to_string().contains("delete allowed after 24h stale"));
        assert!(error.to_string().contains("available in"));
        assert!(list_known_users(&config)
            .expect("users")
            .iter()
            .any(|user| user.display_name == "Fresh Peer"));

        let old_id = list_known_users(&config)
            .expect("users")
            .iter()
            .find(|user| user.display_name == "Old Peer")
            .expect("old user")
            .user_id;
        apply_admin_command(&mut config, &format!("delete-user {old_id}")).expect("delete old");
        let users = list_known_users(&config).expect("users");
        assert!(!users.iter().any(|user| user.display_name == "Old Peer"));
        assert!(users.iter().any(|user| user.display_name == "Fresh Peer"));
        let log = std::fs::read_to_string(config.log_path()).expect("audit log");
        assert!(log.contains("admin console deleted stale user"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn admin_console_prunes_only_stale_known_users() {
        let root = temp_root("console-stale-user-prune");
        let mut config = ServerConfig::for_root(root.clone());
        config::init_files(&config).expect("init");
        let now = current_unix_secs();
        let connection = rusqlite::Connection::open(&config.database_path).expect("db");
        for (identity, name, first_seen, last_seen) in [
            ("old-one", "Old One", now - 200_000, now - 100_000),
            ("old-two", "Old Two", now - 190_000, now - 90_000),
            ("fresh", "Fresh Peer", now - 100, now),
        ] {
            connection
                .execute(
                    "INSERT INTO users(rns_identity_hash, display_name, role_bits, status_bits, first_seen_at, last_seen_at)
                     VALUES (?1, ?2, 0, 0, ?3, ?4)",
                    rusqlite::params![identity.as_bytes(), name, first_seen, last_seen],
                )
                .expect("insert user");
        }
        drop(connection);

        apply_admin_command(&mut config, "prune-stale-users").expect("prune");
        let users = list_known_users(&config).expect("users");

        assert_eq!(users.len(), 1);
        assert_eq!(users[0].display_name, "Fresh Peer");
        let log = std::fs::read_to_string(config.log_path()).expect("audit log");
        assert!(log.contains("admin console pruned stale user"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn dashboard_prunes_stale_known_users_with_confirmation() {
        let root = temp_root("dashboard-stale-user-prune");
        let config = ServerConfig::for_root(root.clone());
        config::init_files(&config).expect("init");
        let now = current_unix_secs();
        let connection = rusqlite::Connection::open(&config.database_path).expect("db");
        for (identity, name, first_seen, last_seen) in [
            ("old-one", "Old One", now - 200_000, now - 100_000),
            ("old-two", "Old Two", now - 190_000, now - 90_000),
            ("fresh", "Fresh Peer", now - 100, now),
        ] {
            connection
                .execute(
                    "INSERT INTO users(rns_identity_hash, display_name, role_bits, status_bits, first_seen_at, last_seen_at)
                     VALUES (?1, ?2, 0, 0, ?3, ?4)",
                    rusqlite::params![identity.as_bytes(), name, first_seen, last_seen],
                )
                .expect("insert user");
        }
        drop(connection);

        let mut app = AdminTui::new(config);
        app.tab = AdminTab::Moderation;
        app.prune_stale_user_records().expect("arm prune");

        assert!(app.pending_prune_stale_users);
        assert!(app.status.contains("Confirm Prune Records"));
        assert_eq!(list_known_users(&app.config).expect("users").len(), 3);

        app.prune_stale_user_records().expect("prune");
        let users = list_known_users(&app.config).expect("users");

        assert!(!app.pending_prune_stale_users);
        assert_eq!(users.len(), 1);
        assert_eq!(users[0].display_name, "Fresh Peer");
        let log = std::fs::read_to_string(app.config.log_path()).expect("audit log");
        assert!(log.contains("admin pruned stale user"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn dashboard_actions_write_admin_audit_log() {
        let root = temp_root("dashboard-audit");
        let config = ServerConfig::for_root(root.clone());
        config::init_files(&config).expect("init");
        config::add_room(&config, "ops", Some("Old")).expect("room");
        let connection = rusqlite::Connection::open(&config.database_path).expect("db");
        connection
            .execute(
                "INSERT INTO users(rns_identity_hash, display_name, role_bits, status_bits, first_seen_at, last_seen_at)
                 VALUES (?1, 'Alice', 0, 0, 1, 1)",
                [b"peer-a".as_slice()],
            )
            .expect("insert user");
        drop(connection);

        let mut app = AdminTui::new(config);
        app.tab = AdminTab::Rooms;
        app.selected_room = config::list_rooms(&app.config)
            .expect("rooms")
            .iter()
            .position(|(_, name, _)| name == "ops")
            .expect("ops room");
        app.archive_selected_room().expect("arm archive");
        app.archive_selected_room().expect("archive");

        app.tab = AdminTab::Moderation;
        app.toggle_selected_user_ban().expect("ban");
        app.toggle_selected_user_trust().expect("trust");

        let log = std::fs::read_to_string(app.config.log_path()).expect("audit log");
        assert!(log.contains("admin archived room"));
        assert!(log.contains("admin banned user"));
        assert!(log.contains("admin trusted user"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[cfg(feature = "live-rns-net")]
    #[test]
    fn identity_panel_reports_configured_destination_hash() {
        let root = temp_root("identity-panel-live-destination");
        let config = ServerConfig::for_root(root.clone());
        config::init_files(&config).expect("init");

        let text = identity_panel_text(&config);

        assert!(text.contains("identity:"));
        assert!(text.contains("file:"));
        assert!(text.contains("identity safety:"));
        assert!(text.contains("backup before public testing"));
        assert!(text.contains("never overwrite active identity material"));
        assert!(text.contains(&format!(
            "backup now: copy {} to offline/private storage",
            config.identity_path.display()
        )));
        assert!(text.contains("isolation: omenchatd owns this home"));
        assert!(text.contains("identity exists; back it up before public testing"));
        assert!(
            text.contains("losing this file changes the OMENchat and NomadNet portal addresses")
        );
        assert!(text.contains("destinations are derived from this same identity"));
        assert!(text.contains(
            "never replace identity material while users still know the old server hash"
        ));
        assert!(text.contains("identity hash: "));
        assert!(text.contains("destination: omenchat.node ("));
        assert!(text.contains("nomadnet portal: nomadnetwork.node ("));
        assert!(!text.contains("client uri: omenchat://"));
        assert!(!text.contains("portal url: "));
        assert!(text.contains("database: "));
        assert!(!text.contains("destination: unavailable"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[cfg(not(feature = "live-rns-net"))]
    #[test]
    fn identity_panel_reports_live_destination_feature_requirement() {
        let root = temp_root("identity-panel-live-destination-unavailable");
        let config = ServerConfig::for_root(root.clone());
        config::init_files(&config).expect("init");

        let text = identity_panel_text(&config);

        assert!(text.contains("destination: unavailable"));
        assert!(text.contains("rebuild with --features live-rns-net"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn identity_operator_checklist_reports_missing_identity() {
        let root = temp_root("identity-checklist-missing");
        let config = ServerConfig::for_root(root.clone());

        let text = identity_operator_checklist(&config);

        assert!(text.contains("state: identity missing; run init/status/live startup to create it"));
        assert!(text.contains("backup now: copy"));
        assert!(text.contains("losing this file changes"));
        assert!(text.contains("server root separate"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn portal_panel_explains_public_addresses_and_page_ownership() {
        let root = temp_root("portal-panel");
        let mut config = ServerConfig::for_root(root.clone());
        config.motd = "Read the rules".into();
        config::init_files(&config).expect("init");
        std::fs::create_dir_all(
            config
                .nomadnet_index_page_path()
                .parent()
                .expect("page parent"),
        )
        .expect("page dir");
        std::fs::write(config.nomadnet_index_page_path(), "Welcome").expect("page");

        let text = portal_panel_text(&config);

        assert!(text.contains("share:"));
        assert!(text.contains("chat: omenchat:// URI"));
        assert!(text.contains("portal: NomadNet /page/index.mu URL"));
        assert!(text.contains("portal use: MOTD, rules, help, launch links"));
        assert!(text.contains("chat traffic stays on OMENchat"));
        assert!(text.contains("portal readiness:"));
        assert!(text.contains("page: portal page exists; edit it directly for rules/help"));
        assert!(text.contains("motd: MOTD is set"));
        assert!(text.contains("publish: verify Monitoring before sharing either address"));
        assert!(text.contains("edit: reticulum/storage/pages/index.mu"));
        assert!(text.contains("served: /page/index.mu"));
        assert!(text.contains("file:"));
        assert!(text.contains("MOTD: Read the rules"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn portal_operator_checklist_reports_missing_page_and_motd() {
        let root = temp_root("portal-checklist-missing");
        let mut config = ServerConfig::for_root(root.clone());
        config.motd.clear();
        config::init_files(&config).expect("init");

        let text = portal_operator_checklist(&config);

        assert!(text.contains("page: portal page missing; start live/status to create template"));
        assert!(text.contains("motd: set MOTD if you want a short server notice"));
        assert!(!text.contains("share omenchat:// URI"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn log_tail_reads_latest_lines() {
        let root = temp_root("log-tail");
        let config = ServerConfig::for_root(root.clone());
        config::init_files(&config).expect("init");
        std::fs::write(config.log_path(), "one\ntwo\nthree\n").expect("write log");

        let tail = read_log_tail(&config.log_path(), 2).expect("tail");
        assert_eq!(tail, "two\nthree");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn log_tail_creates_missing_log_file() {
        let root = temp_root("log-tail-missing");
        let config = ServerConfig::for_root(root.clone());
        config::init_files(&config).expect("init");
        std::fs::remove_file(config.log_path()).expect("remove log");

        let tail = read_log_tail(&config.log_path(), 25).expect("tail");

        assert!(tail.is_empty());
        assert!(config.log_path().is_file());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn log_panel_text_explains_runtime_log_and_empty_state() {
        let root = temp_root("log-panel-text");
        let config = ServerConfig::for_root(root.clone());
        config::init_files(&config).expect("init");
        std::fs::remove_file(config.log_path()).expect("remove log");

        let empty = log_panel_text(&config.log_path(), 25);
        assert!(empty.contains("Logs: runtime and network detail"));
        assert!(empty.contains("file:"));
        assert!(empty.contains("normal: startup/manual/automatic announces"));
        assert!(empty.contains("watch: repeated timeouts"));
        assert!(empty.contains("interface watchdog restarts"));
        assert!(empty.contains("admin changes: Audit tab"));
        assert!(empty.contains("No log entries yet"));
        assert!(config.log_path().is_file());

        std::fs::write(config.log_path(), "server started\nadmin updated motd\n").expect("log");
        let text = log_panel_text(&config.log_path(), 25);
        assert!(text.contains("server started"));
        assert!(text.contains("admin updated motd"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn audit_tail_creates_missing_log_file() {
        let root = temp_root("audit-tail-missing");
        let config = ServerConfig::for_root(root.clone());
        config::init_files(&config).expect("init");
        std::fs::remove_file(config.log_path()).expect("remove log");

        let tail = read_admin_audit_tail(&config.log_path(), 25).expect("tail");

        assert!(tail.is_empty());
        assert!(config.log_path().is_file());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn audit_tail_filters_admin_actions_from_runtime_log() {
        let root = temp_root("audit-tail");
        let config = ServerConfig::for_root(root.clone());
        config::init_files(&config).expect("init");
        std::fs::write(
            config.log_path(),
            "1 startup complete\n2 admin updated server name\n3 stats: active_links=1\n4 admin console banned user id=7\n",
        )
        .expect("write log");

        let tail = read_admin_audit_tail(&config.log_path(), 10).expect("tail");
        assert!(tail.contains("admin updated server name"));
        assert!(tail.contains("admin console banned user"));
        assert!(!tail.contains("startup complete"));
        assert!(!tail.contains("active_links"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn audit_panel_text_explains_admin_filter_and_empty_state() {
        let root = temp_root("audit-panel-text");
        let config = ServerConfig::for_root(root.clone());
        config::init_files(&config).expect("init");
        std::fs::write(
            config.log_path(),
            "1 startup complete\n2 admin updated server name\n3 stats: active_links=1\n4 admin archived room id=2 name=ops\n5 admin console banned user id=7\n6 admin pruned stale user id=8 name=Old stale_secs=90000\n7 admin wrote TCPClientInterface target=gateway.example:42420\n",
        )
        .expect("write log");

        let text = audit_panel_text(&config.log_path(), 25);
        assert!(text.contains("Audit: local admin changes only"));
        assert!(text.contains("file:"));
        assert!(text.contains(
            "summary: 5 action(s) | config 1 | interfaces 1 | rooms 1 | moderation 1 | stale cleanup 1"
        ));
        assert!(text.contains("runtime/network detail: Logs tab"));
        assert!(text.contains("admin updated server name"));
        assert!(text.contains("admin archived room"));
        assert!(text.contains("admin console banned user"));
        assert!(!text.contains("startup complete"));

        std::fs::write(config.log_path(), "1 startup complete\n").expect("write log");
        let empty = audit_panel_text(&config.log_path(), 25);
        assert!(empty.contains("summary: no admin actions in this view"));
        assert!(empty.contains("No admin actions recorded yet"));
        let _ = std::fs::remove_dir_all(root);
    }
}
