mod message_status;
mod page_widget;

use std::borrow::Cow;

#[cfg(feature = "chat-client")]
use iced::font::Style as FontStyle;
use iced::theme::Palette;
use iced::widget::scrollable::{
    Direction as ScrollableDirection, Id as ScrollableId, RelativeOffset, Scrollbar,
    Status as ScrollableStatus, Viewport,
};
use iced::widget::text::Wrapping;
use iced::widget::{
    button, column, container, image, pane_grid, row, scrollable, text, text_editor, text_input,
    tooltip, Button, Scrollable, Text,
};
use iced::{
    event, keyboard, time, window, Background, Border, Color, ContentFit, Element, Font, Length,
    Padding, Pixels, Settings, Shadow, Subscription, Task, Theme,
};
#[cfg(feature = "chat-client-rns")]
use std::collections::{BTreeMap, VecDeque};
use std::collections::{HashMap, HashSet};
#[cfg(feature = "chat-client")]
use std::io::Read;
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{self, Command};
use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::OnceLock;
use std::time::Duration;

use crate::app::{
    current_epoch_ms, message_summary_key, App, BrowserFieldEditor, BrowserRequestStatus,
    DirectoryScope, LxmfMessagingDiagnosticsSeverity, TabId,
};
use crate::browser::{BrowserAddress, BrowserSession, DownloadedFile};
#[cfg(feature = "chat-client")]
use crate::chat::client::is_restorable_server_destination;
#[cfg(feature = "chat-client")]
use crate::chat::commands::{parse_client_command, ClientCommand};
#[cfg(feature = "chat-client")]
use crate::chat::protocol::RoomId;
#[cfg(feature = "chat-client-rns")]
use crate::chat::rns::{
    replay_pending_resource_offers, resource_id_from_metadata, ChatLinkTransport,
};
#[cfg(feature = "chat-client")]
use crate::chat::store::{ChatStore, SqliteChatStore};
#[cfg(feature = "chat-client")]
use crate::chat::{
    ChatClient, ChatClientEvent, ChatClientRequest, ChatEvent, ChatEventKind, ChatSessionId,
    ChatSessionView, ChatUserSummary, OmenChatDescriptor,
};
use crate::interfaces::InterfaceKind;
#[cfg(feature = "chat-client")]
use crate::media::{
    decide_remote_media, extract_link_candidates, RemoteMediaContext, RemoteMediaDecision,
    RemoteMediaTransport,
};
use crate::micron::render::HitAction;
#[cfg(feature = "chat-client")]
use crate::storage::files::next_available_download_path;
use crate::storage::settings::{
    DesktopWorkspaceLayoutNode, DesktopWorkspacePaneKind, DesktopWorkspacePaneSettings,
    DesktopWorkspaceSplitAxis, RuntimeBackendSetting,
};
use crate::workspace::WorkspaceSection;
use message_status::{
    desktop_message_is_retry_candidate, desktop_message_propagation_sync_label,
    desktop_message_retry_labels, lxmf_message_compact_status, lxmf_message_status_lines,
};
use page_widget::{
    color_from_style, nomadnet_page_with_row_renderer, NomadNetPageProps, PageMessage,
};

const DIRECTORY_RENDER_LIMIT: usize = 80;
const IDENTIFY_ICON: &str = "\u{f2bb}";
const IDENTIFY_ICON_CHARSET: &str = "f2bb";
const ICON_BACK: &str = "\u{f060}";
const ICON_FORWARD: &str = "\u{f061}";
const ICON_RELOAD: &str = "\u{f01e}";
const ICON_STOP: &str = "\u{f256}";
const ICON_REQUEST_PATH: &str = "\u{f4d7}";
const ICON_CAPTURE: &str = "\u{f083}";
const ICON_DIAGNOSTICS: &str = "\u{f0f0}";
const ICON_MENU: &str = "\u{f0c9}";
const ICON_ATTACH: &str = "\u{f0c6}";
const ICON_STATUS_MENU: &str = "\u{f142}";
const ICON_STATUS_IDENTITY: &str = "\u{f2c2}";
const ICON_STATUS_UNREAD: &str = "\u{f27a}";
const ICON_WINDOW_MAX: &str = "\u{f2d0}";
const ICON_WINDOW_HIDE: &str = "\u{f2d1}";
const ICON_WINDOW_CLOSE: &str = "\u{f00d}";
const ICON_RESTORE_BROWSER: &str = "\u{f233}";
const ICON_RESTORE_MESSAGES: &str = "\u{f199}";
const ICON_RESTORE_CHAT: &str = "\u{f075}";
const ICON_OMENCHAT_PATH: &str = "\u{f1e5}";
const ICON_OMENCHAT_RECONNECT: &str = "\u{f01e}";
const ICON_OMENCHAT_ATTACH: &str = "\u{f0c6}";
const ICON_DOWNLOAD: &str = "\u{f019}";
const ICON_OPEN: &str = "\u{f06e}";
#[cfg(feature = "chat-client-rns")]
const OMENCHAT_RECENT_SYNC_MAX_ATTEMPTS: u8 = 3;
const EMOJI_CHARSET: &str = "1f408";
const DESKTOP_IDLE_TICK_MS: u64 = 1_000;
const DESKTOP_LIVE_TICK_MS: u64 = 250;
// rns-core clamps Link keepalive to a 5s minimum and marks low-RTT links stale
// after roughly 10s without inbound traffic. A lightweight OMENchat ping below
// that window is less noisy than repeated Link teardown/reconnect/history sync.
const OMENCHAT_HEARTBEAT_IDLE_MS: u64 = 4_000;
const OMENCHAT_HEARTBEAT_TIMEOUT_MS: u64 = 18_000;
const OMENCHAT_MIN_HEARTBEAT_IDLE_MS: u64 = 5_000;
const OMENCHAT_MAX_HEARTBEAT_IDLE_MS: u64 = 600_000;
const OMENCHAT_PATH_RECONNECT_DELAY_MS: u64 = 2_000;
const OMENCHAT_MESSAGE_GROUP_GAP_SECS: i64 = 5 * 60;
#[cfg(feature = "chat-client")]
const OMENCHAT_LOCAL_ECHO_RESEND_SECS: i64 = 15;
#[cfg(feature = "chat-client")]
const OMENCHAT_INLINE_MEDIA_HEADER_BYTES: usize = 128 * 1024;
#[cfg(feature = "chat-client")]
const OMENCHAT_GIF_ANIMATION_SCAN_BYTES: usize = 512 * 1024;
#[cfg(feature = "chat-client")]
const OMENCHAT_INLINE_MEDIA_MAX_WIDTH: f32 = 520.0;
#[cfg(feature = "chat-client")]
const OMENCHAT_INLINE_MEDIA_MAX_HEIGHT: f32 = 360.0;
const COMMON_TOR_SOCKS_PORTS: &[u16] = &[9050, 9150];
#[cfg(feature = "chat-client")]
const OMENCHAT_PENDING_DESTINATION_PREFIX: &str = "pending-omenchat-";
const DESKTOP_SCROLLBAR_WIDTH: u16 = 7;
const DESKTOP_SCROLLBAR_SCROLLER_WIDTH: u16 = 4;
const DESKTOP_SCROLLBAR_MARGIN: u16 = 4;
const DESKTOP_SCROLL_GUTTER_EXTRA: u16 = 12;
const DESKTOP_PANEL_PADDING: u16 = 12;
const DESKTOP_SHELL_PADDING: u16 = 16;
const CONVERSATION_VISIBLE_MESSAGES: usize = 8;
const CONVERSATION_PREVIEW_CHARS: usize = 220;
const CONVERSATION_PREVIEW_LINES: usize = 5;
const LOG_VISIBLE_ENTRIES: usize = 48;
const LXMF_HELP_LINES: &[&str] = &[
    "Messages can be direct or propagated. Direct sends use live paths; propagated sends hand the envelope to the selected propagation node.",
    "Sync Propagation checks the selected propagation node. Path/Diag buttons help inspect peer and path state without burying it in logs.",
    "Native LXMF ticket/stamp sending is not implemented yet. Ticketed send state is disabled/downgraded before native sends so the UI does not claim a ticketed delivery path that cannot be honored.",
    "Transport proof, propagation-node acceptance, and inbound peer activity are useful evidence, but they are not the same as a guaranteed peer-side LXMF receipt.",
    "Unread counts clear when the matching conversation becomes active. Delete removes local conversation history for that peer.",
];
#[cfg(feature = "chat-client")]
const OMENCHAT_ALPHA_TEST_HELP_LINES: &[&str] = &[
    "For a second local test client, start OMENbrowser_rs with an isolated app root so it does not reuse your main identity, settings, message store, or plugin database.",
    "Example first client: ./target/release/omenbrowser_rs --desktop --app-root /tmp/omenbrowser-rs-alpha",
    "Example second client: ./target/release/omenbrowser_rs --desktop --app-root /tmp/omenbrowser-rs-alpha-2",
    "Use a separate OMENbrowser_rs config/storage root for each tester identity. Newly generated Reticulum configs include an instance_name suffix to avoid interface instance-name collisions.",
    "Open OMENchat servers with omenchat://<destination hash>. If the path/key is missing, use Path, wait for the path result, then Reconnect.",
    "A minimized OMENchat pane should highlight when new room messages arrive. Restoring the pane marks the active room read.",
];
#[cfg(feature = "chat-client")]
const OMENCHAT_HISTORY_HELP_LINES: &[&str] = &[
    "When an OMENchat pane opens or reconnects, it asks the server for the bounded recent room history and merges missing events into the local cache.",
    "The server-side history limit controls how much recent backlog is offered on join/reconnect. The client should not need Load Older just to catch up with the latest messages.",
    "Load Older requests the next older batch before the oldest locally cached event in the active room. If the server has nothing older, the pane should report that the room is already current.",
    "History batches are inserted by server event id, so recovered messages should appear in chronological order and still obey the normal same-user message stacking rules.",
    "If two clients disagree after reconnect, check Logs for HistoryRecent/HistoryBefore frames and the server TUI Monitoring/Logs for history resource offers or protocol errors.",
];
#[cfg(feature = "chat-client")]
const OMENCHAT_MEDIA_HELP_LINES: &[&str] = &[
    "OMENchat can preview cached images and animated GIFs inline. NomadNet/Reticulum media stays on the Reticulum path and does not use direct clearweb TCP.",
    "Clearweb HTTP/HTTPS image previews are privacy-gated. Remote media is off by default; when enabled, trusted OMENchat servers can auto-load images only through a detected SOCKS/Tor proxy on 127.0.0.1:9050 or 127.0.0.1:9150.",
    "Untrusted clearweb images require an explicit Load action. Non-image clearweb links open through the external browser prompt; use Copy URL for Tor Browser.",
    "Uploads use the native file picker from the attach button or /upload <path>. The server advertises both total upload quota and max file size; current defaults are 50 MiB quota per identity and 512 KiB max per file.",
    "Accepted upload images/GIFs are cached under the active identity's OMENchat media cache and rendered inline for supported image types. Oversized or rejected files should fail before transfer.",
];
#[cfg(feature = "chat-client")]
const OMENCHATD_OPERATOR_HELP_LINES: &[&str] = &[
    "omenchatd is standalone. Its default server root is ~/.omenchatd, including identity material, Reticulum config, SQLite database, logs, and operator-owned NomadNet pages.",
    "The server must not use ~/.reticulum, ~/.nomadnetwork, ~/.lxmd, or OMENbrowser_rs identity storage unless an operator explicitly points it somewhere else.",
    "The public chat destination announces as omenchat.node. The quiet NomadNet portal announces separately as nomadnetwork.node and serves /page/index.mu from reticulum/storage/pages/index.mu.",
    "Edit reticulum/storage/pages/index.mu for MOTD, server rules, room summaries, and the omenchat:// link. omenchatd creates the file only if it is missing and should not overwrite operator edits.",
    "Typical server start: cargo run --manifest-path src/server/Cargo.toml --features live-rns-net -- run",
    "Typical server setup UI: cargo run --manifest-path src/server/Cargo.toml --features live-rns-net -- tui",
    "Use omenchatd tui for setup, interfaces, rooms, moderation, monitoring, logs, audit, and help. Use omenchatd status for copyable identity, destination, portal path, limits, and storage information.",
    "Room creation is admin-only. Topic edits and kick/ban/mute actions are moderator/admin operations. Use the Moderation panel or slash commands from a privileged OMENchat client.",
];

static NERD_FONT_FAMILY: OnceLock<Option<&'static str>> = OnceLock::new();
static EMOJI_FONT_FAMILY: OnceLock<Option<&'static str>> = OnceLock::new();
static DESKTOP_FONT_SIZE: AtomicU16 = AtomicU16::new(16);

const MICRON_VIEWPORT_FONT_BYTES: &[u8] =
    include_bytes!("../../assets/fonts/adwaita/AdwaitaMono-Regular.ttf");

fn desktop_ui_font() -> Font {
    Font::MONOSPACE
}

fn nerd_icon_font() -> Font {
    detected_nerd_font_family()
        .map(Font::with_name)
        .unwrap_or(Font::MONOSPACE)
}

fn emoji_font() -> Font {
    detected_emoji_font_family()
        .map(Font::with_name)
        .unwrap_or(Font::DEFAULT)
}

fn detected_nerd_font_family() -> Option<&'static str> {
    *NERD_FONT_FAMILY.get_or_init(|| {
        detect_system_font_family_for_char(IDENTIFY_ICON_CHARSET)
            .map(|family| Box::leak(family.into_boxed_str()) as &'static str)
    })
}

fn detected_emoji_font_family() -> Option<&'static str> {
    *EMOJI_FONT_FAMILY.get_or_init(|| {
        detect_system_font_family_for_query("emoji", EMOJI_CHARSET)
            .or_else(|| detect_system_font_family_for_query("sans", EMOJI_CHARSET))
            .map(|family| Box::leak(family.into_boxed_str()) as &'static str)
    })
}

fn detect_system_font_family_for_char(charset: &str) -> Option<String> {
    fc_match_families_output("monospace", charset)
        .and_then(|output| select_nerd_font_family_from_fc_match(&output))
}

fn detect_system_font_family_for_query(family: &str, charset: &str) -> Option<String> {
    fc_match_families_output(family, charset)
        .and_then(|output| select_first_font_family_from_fc_match(&output))
}

fn fc_match_families_output(family: &str, charset: &str) -> Option<String> {
    let query = format!("{family}:charset={charset}");
    let output = Command::new("fc-match")
        .arg("--format=%{family}\n")
        .arg(query)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout).ok()
}

fn detect_external_browsers(preferred_command: Option<&str>) -> Vec<ExternalBrowserChoice> {
    let candidates = [
        ("Default browser", "xdg-open"),
        ("Firefox", "firefox"),
        ("LibreWolf", "librewolf"),
        ("Mullvad Browser", "mullvad-browser"),
        ("Brave", "brave-browser"),
        ("Brave", "brave"),
        ("Chromium", "chromium"),
        ("Chromium", "chromium-browser"),
        ("Chrome", "google-chrome"),
        ("Chrome", "google-chrome-stable"),
        ("Chrome", "chrome"),
        ("Chrome", "chromium-freeworld"),
        ("Qutebrowser", "qutebrowser"),
    ];
    detect_external_browsers_from_candidates(preferred_command, &candidates, command_available)
}

fn detect_external_browsers_from_candidates(
    preferred_command: Option<&str>,
    candidates: &[(&str, &str)],
    available: impl Fn(&str) -> bool,
) -> Vec<ExternalBrowserChoice> {
    let mut choices = Vec::new();
    let mut seen_labels = HashSet::new();
    if let Some(command) = preferred_command {
        if let Some((label, _)) = candidates
            .iter()
            .find(|(_, candidate)| *candidate == command)
        {
            if available(command) {
                choices.push(ExternalBrowserChoice {
                    label: (*label).into(),
                    command: command.into(),
                    kind: external_browser_kind(command),
                });
                seen_labels.insert((*label).to_string());
            }
        }
    }
    for (label, command) in candidates.iter().copied() {
        if !available(command) {
            continue;
        }
        if seen_labels.contains(label) {
            continue;
        }
        let kind = external_browser_kind(command);
        if !choices
            .iter()
            .any(|choice: &ExternalBrowserChoice| choice.command == command)
        {
            choices.push(ExternalBrowserChoice {
                label: label.into(),
                command: command.into(),
                kind,
            });
            seen_labels.insert(label.into());
        }
    }
    if choices.is_empty() {
        choices.push(ExternalBrowserChoice {
            label: "Default browser".into(),
            command: "xdg-open".into(),
            kind: ExternalBrowserKind::Default,
        });
    }
    if let Some(command) = preferred_command {
        choices.sort_by_key(|choice| usize::from(choice.command != command));
    }
    choices
}

fn local_tcp_reachable(host: &str, port: u16, timeout: Duration) -> bool {
    let Ok(addr) = format!("{host}:{port}").parse::<SocketAddr>() else {
        return false;
    };
    TcpStream::connect_timeout(&addr, timeout).is_ok()
}

fn detect_clearweb_socks_proxy(host: &str, configured_port: u16) -> Option<(String, u16)> {
    let timeout = Duration::from_millis(75);
    std::iter::once(configured_port)
        .chain(COMMON_TOR_SOCKS_PORTS.iter().copied())
        .find(|port| local_tcp_reachable(host, *port, timeout))
        .map(|port| (host.into(), port))
}

#[cfg(feature = "chat-client-rns")]
fn omenchat_monitor_frame_label(op: crate::chat::protocol::ChatOp) -> String {
    match op {
        crate::chat::protocol::ChatOp::SessionOpen => "session open",
        crate::chat::protocol::ChatOp::SessionAccept => "session accepted",
        crate::chat::protocol::ChatOp::SessionReject => "session rejected",
        crate::chat::protocol::ChatOp::JoinRoom => "join room",
        crate::chat::protocol::ChatOp::JoinAccept => "join accepted",
        crate::chat::protocol::ChatOp::PartRoom => "part room",
        crate::chat::protocol::ChatOp::RoomSubscribe => "room subscribe",
        crate::chat::protocol::ChatOp::RoomUnsubscribe => "room unsubscribe",
        crate::chat::protocol::ChatOp::RoomMessage => "room message",
        crate::chat::protocol::ChatOp::RoomAction => "room action",
        crate::chat::protocol::ChatOp::RoomNotice => "room notice",
        crate::chat::protocol::ChatOp::RoomEvent => "room event",
        crate::chat::protocol::ChatOp::MessageAck => "message ack",
        crate::chat::protocol::ChatOp::UserListSnapshotInline => "userlist inline",
        crate::chat::protocol::ChatOp::UserListSnapshotResource => "userlist resource",
        crate::chat::protocol::ChatOp::UserDelta => "user delta",
        crate::chat::protocol::ChatOp::RoomDelta => "room delta",
        crate::chat::protocol::ChatOp::RoleDelta => "role delta",
        crate::chat::protocol::ChatOp::HistoryBefore => "history before",
        crate::chat::protocol::ChatOp::HistoryInline => "history inline",
        crate::chat::protocol::ChatOp::HistoryResourceOffer => "history resource",
        crate::chat::protocol::ChatOp::HistoryEnd => "history end",
        crate::chat::protocol::ChatOp::HistoryRecent => "history recent",
        crate::chat::protocol::ChatOp::HistoryCurrent => "history current",
        crate::chat::protocol::ChatOp::Command => "command",
        crate::chat::protocol::ChatOp::CommandResult => "command result",
        crate::chat::protocol::ChatOp::ContactRequest => "contact request",
        crate::chat::protocol::ChatOp::ContactOffer => "contact offer",
        crate::chat::protocol::ChatOp::ContactAccept => "contact accepted",
        crate::chat::protocol::ChatOp::ContactReject => "contact rejected",
        crate::chat::protocol::ChatOp::UploadOffer => "upload offer",
        crate::chat::protocol::ChatOp::UploadAccept => "upload accepted",
        crate::chat::protocol::ChatOp::UploadReject => "upload rejected",
        crate::chat::protocol::ChatOp::UploadComplete => "upload complete",
        crate::chat::protocol::ChatOp::UploadFetch => "upload fetch",
        crate::chat::protocol::ChatOp::UploadResourceOffer => "upload resource",
        crate::chat::protocol::ChatOp::UploadInlineChunk => "upload chunk",
        crate::chat::protocol::ChatOp::Ping => "ping",
        crate::chat::protocol::ChatOp::Pong => "pong",
        crate::chat::protocol::ChatOp::Error => "error",
    }
    .into()
}

fn command_available(command: &str) -> bool {
    Command::new("sh")
        .arg("-c")
        .arg(format!("command -v {}", shell_word(command)))
        .output()
        .is_ok_and(|output| output.status.success())
}

fn external_browser_kind(command: &str) -> ExternalBrowserKind {
    let executable = Path::new(command)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(command);
    if executable == "xdg-open" {
        ExternalBrowserKind::Default
    } else {
        ExternalBrowserKind::Standard
    }
}

fn shell_word(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn open_external_url_with_choice(choice: &ExternalBrowserChoice, url: &str) -> Result<(), String> {
    let candidates = external_browser_open_candidates(choice, url);
    let mut errors = Vec::new();
    for (program, args) in candidates {
        match Command::new(&program).args(&args).spawn() {
            Ok(_) => return Ok(()),
            Err(error) => {
                errors.push(format!("{program} {:?}: {error}", args));
            }
        }
    }
    Err(errors.join(" | "))
}

fn external_browser_open_candidates(
    choice: &ExternalBrowserChoice,
    url: &str,
) -> Vec<(String, Vec<String>)> {
    vec![(choice.command.clone(), vec![url.into()])]
}

fn set_desktop_font_size(size: u16) {
    DESKTOP_FONT_SIZE.store(size.clamp(10, 24), Ordering::Relaxed);
}

fn ui_size(design_size: u16) -> u16 {
    let base = DESKTOP_FONT_SIZE.load(Ordering::Relaxed).clamp(10, 24);
    scaled_ui_size(design_size, base)
}

fn scaled_ui_size(design_size: u16, base_size: u16) -> u16 {
    let base = base_size.clamp(10, 24);
    let scaled = (u32::from(design_size) * u32::from(base) + 8) / 16;
    scaled.clamp(1, 64) as u16
}

fn select_first_font_family_from_fc_match(output: &str) -> Option<String> {
    output
        .lines()
        .flat_map(|line| line.split(','))
        .map(str::trim)
        .find(|family| !family.is_empty())
        .map(ToOwned::to_owned)
}

fn select_nerd_font_family_from_fc_match(output: &str) -> Option<String> {
    let families = output
        .lines()
        .flat_map(|line| line.split(','))
        .map(str::trim)
        .filter(|family| !family.is_empty());
    let mut first = None;
    for family in families {
        if first.is_none() {
            first = Some(family.to_string());
        }
        if family.to_ascii_lowercase().contains("nerd font") {
            return Some(family.to_string());
        }
    }
    first
}

pub fn run(app: App) -> iced::Result {
    let mut app = app;
    let default_text_size = app.settings.ui.font_size.clamp(10, 24) as f32;
    set_desktop_font_size(app.settings.ui.font_size);
    app.reset_runtime_log_display_for_session();
    app.bootstrap_runtime_on_launch();
    iced::application("OMENbrowser_rs", DesktopApp::update, DesktopApp::view)
        .settings(Settings {
            default_font: desktop_ui_font(),
            default_text_size: Pixels(default_text_size),
            fonts: vec![Cow::Borrowed(MICRON_VIEWPORT_FONT_BYTES)],
            ..Settings::default()
        })
        .theme(DesktopApp::theme)
        .style(|_, theme| omen_application_style(theme))
        .subscription(DesktopApp::subscription)
        .exit_on_close_request(false)
        .run_with(|| {
            let mut desktop = DesktopApp::new(app);
            let startup_scroll = desktop.anchor_visible_workspace_scrolls_to_bottom_now(2);
            (desktop, startup_scroll)
        })
}

struct DesktopApp {
    app: App,
    conversation_body_editors: HashMap<u64, text_editor::Content>,
    conversation_message_counts: HashMap<u64, usize>,
    conversation_scroll_offsets: HashMap<u64, RelativeOffset>,
    conversation_scroll_restore_locks: HashSet<u64>,
    navigation_open: bool,
    identity_delete_confirming: bool,
    workspace_panes: pane_grid::State<DesktopPane>,
    active_workspace_pane: pane_grid::Pane,
    ctrl_down: bool,
    restore_workspace_scrolls_pending: bool,
    restore_workspace_scrolls_remaining: u8,
    restore_workspace_scroll_locks_release_pending: bool,
    pending_workspace_bottom_anchor_ticks: u8,
    shutdown_requested: bool,
    monitoring_sample_epoch_ms: u64,
    monitoring_process_usage: Option<ProcessResourceUsage>,
    debug_tick_count: u64,
    debug_last_tick_epoch_ms: u64,
    external_link_prompt: Option<ExternalLinkPrompt>,
    external_browsers: Vec<ExternalBrowserChoice>,
    clearweb_proxy_reachable: bool,
    clearweb_proxy_endpoint: Option<(String, u16)>,
    #[cfg(feature = "chat-client")]
    chat_client: ChatClient,
    #[cfg(feature = "chat-client")]
    chat_store: Option<SqliteChatStore>,
    #[cfg(feature = "chat-client")]
    chat_drafts: HashMap<ChatSessionId, String>,
    #[cfg(feature = "chat-client")]
    chat_event_counts: HashMap<(ChatSessionId, RoomId), usize>,
    #[cfg(feature = "chat-client")]
    chat_scroll_offsets: HashMap<(ChatSessionId, RoomId), RelativeOffset>,
    #[cfg(feature = "chat-client")]
    chat_scroll_bottom_locks: HashSet<(ChatSessionId, RoomId)>,
    #[cfg(feature = "chat-client")]
    omenchat_motds: HashMap<ChatSessionId, String>,
    #[cfg(feature = "chat-client")]
    omenchat_upload_quotas: HashMap<ChatSessionId, u64>,
    #[cfg(feature = "chat-client")]
    omenchat_upload_max_file_bytes: HashMap<ChatSessionId, u64>,
    #[cfg(feature = "chat-client")]
    omenchat_media_cache: HashMap<String, OmenChatMediaLoadState>,
    #[cfg(feature = "chat-client")]
    omenchat_gif_frames: HashMap<String, iced_gif::Frames>,
    #[cfg(feature = "chat-client")]
    omenchat_server_entry: String,
    #[cfg(feature = "chat-client")]
    omenchat_rooms_visible: bool,
    #[cfg(feature = "chat-client")]
    omenchat_pending_upload_sources: HashMap<(ChatSessionId, String, u64), PathBuf>,
    #[cfg(feature = "chat-client-rns")]
    omenchat_live_state: crate::chat::live::LiveChatClientState,
    #[cfg(feature = "chat-client-rns")]
    omenchat_live_transports: HashMap<ChatSessionId, DesktopOmenChatTransport>,
    #[cfg(feature = "chat-client-rns")]
    omenchat_link_sessions: HashMap<[u8; 16], ChatSessionId>,
    #[cfg(feature = "chat-client-rns")]
    omenchat_live_opening: HashSet<ChatSessionId>,
    #[cfg(feature = "chat-client-rns")]
    omenchat_live_retry_after: HashMap<ChatSessionId, u64>,
    #[cfg(feature = "chat-client-rns")]
    omenchat_live_retry_count: HashMap<ChatSessionId, u8>,
    #[cfg(feature = "chat-client-rns")]
    omenchat_live_connect_count: HashMap<ChatSessionId, u64>,
    #[cfg(feature = "chat-client-rns")]
    omenchat_live_disconnect_count: HashMap<ChatSessionId, u64>,
    #[cfg(feature = "chat-client-rns")]
    omenchat_live_last_disconnect_reason: HashMap<ChatSessionId, String>,
    #[cfg(feature = "chat-client-rns")]
    omenchat_recent_sync_pending: HashSet<ChatSessionId>,
    #[cfg(feature = "chat-client-rns")]
    omenchat_recent_sync_links: HashMap<ChatSessionId, [u8; 16]>,
    #[cfg(feature = "chat-client-rns")]
    omenchat_recent_sync_due_after: HashMap<ChatSessionId, u64>,
    #[cfg(feature = "chat-client-rns")]
    omenchat_recent_sync_attempts: HashMap<ChatSessionId, u8>,
    #[cfg(feature = "chat-client-rns")]
    omenchat_live_reconnect_generation: HashMap<ChatSessionId, u64>,
}

#[cfg(feature = "chat-client")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OmenChatDraftCommandResult {
    NotCommand,
    HandledClear,
    HandledKeep,
}

#[cfg(feature = "chat-client-rns")]
#[derive(Clone, Debug, Default)]
struct DesktopOmenChatTransport {
    link_id: [u8; 16],
    incoming_frames: VecDeque<Vec<u8>>,
    resources: BTreeMap<String, Vec<u8>>,
    pending_resource_offers: BTreeMap<String, VecDeque<Vec<u8>>>,
    outgoing_frames: Vec<Vec<u8>>,
    outgoing_resources: Vec<(String, Vec<u8>)>,
    last_rx_epoch_ms: u64,
    last_tx_epoch_ms: u64,
    last_ping_epoch_ms: u64,
    last_pong_epoch_ms: u64,
    last_ping_rtt_ms: Option<u64>,
    connected_since_epoch_ms: u64,
    frames_in: u64,
    frames_out: u64,
    bytes_in: u64,
    bytes_out: u64,
    resource_bytes_in: u64,
    resources_in: u64,
    history_frames_in: u64,
    history_frames_out: u64,
    room_events_in: u64,
    chat_frames_out: u64,
    userlist_frames_in: u64,
    resource_offers_in: u64,
    upload_fetches_out: u64,
    upload_resource_offers_in: u64,
    upload_inline_chunks_in: u64,
    upload_inline_bytes_in: u64,
    upload_resources_in: u64,
    upload_resource_bytes_in: u64,
    pings_in: u64,
    pings_out: u64,
    pongs_in: u64,
    pongs_out: u64,
    last_rx_frame: Option<String>,
    last_tx_frame: Option<String>,
    awaiting_pong: bool,
    heartbeat_idle_ms: u64,
}

#[cfg(feature = "chat-client-rns")]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct OmenChatLiveMonitorTotals {
    sessions: usize,
    connected: usize,
    opening: usize,
    reconnect_timers: usize,
    history_sync_waiting: usize,
    pending_resources: usize,
    frames_in: u64,
    frames_out: u64,
    bytes_in: u64,
    bytes_out: u64,
    resources_in: u64,
    resource_bytes_in: u64,
    upload_fetches_out: u64,
    upload_resource_offers_in: u64,
    upload_inline_chunks_in: u64,
    upload_inline_bytes_in: u64,
    upload_resources_in: u64,
    upload_resource_bytes_in: u64,
    awaiting_pongs: usize,
}

#[cfg(feature = "chat-client-rns")]
impl DesktopOmenChatTransport {
    fn new(link_id: [u8; 16], now_ms: u64) -> Self {
        Self {
            link_id,
            last_rx_epoch_ms: now_ms,
            last_tx_epoch_ms: now_ms,
            connected_since_epoch_ms: now_ms,
            heartbeat_idle_ms: OMENCHAT_HEARTBEAT_IDLE_MS,
            ..Self::default()
        }
    }

    fn push_incoming_frame(&mut self, bytes: Vec<u8>, now_ms: u64) {
        self.frames_in = self.frames_in.saturating_add(1);
        self.bytes_in = self.bytes_in.saturating_add(bytes.len() as u64);
        let op = self.note_incoming_frame(&bytes);
        self.last_rx_epoch_ms = now_ms;
        if matches!(op, Some(crate::chat::protocol::ChatOp::Pong)) {
            self.last_pong_epoch_ms = now_ms;
            if self.last_ping_epoch_ms > 0 {
                self.last_ping_rtt_ms = Some(now_ms.saturating_sub(self.last_ping_epoch_ms));
            }
            self.awaiting_pong = false;
        } else if op.is_some() {
            self.awaiting_pong = false;
        }
        self.incoming_frames.push_back(bytes);
    }

    fn push_resource(&mut self, metadata: Option<Vec<u8>>, data: Vec<u8>, now_ms: u64) {
        self.last_rx_epoch_ms = now_ms;
        self.awaiting_pong = false;
        self.resources_in = self.resources_in.saturating_add(1);
        self.resource_bytes_in = self.resource_bytes_in.saturating_add(data.len() as u64);
        let inferred_resource_id = if metadata.is_none() && self.pending_resource_offer_count() == 1
        {
            self.pending_resource_offers.keys().next().cloned()
        } else {
            None
        };
        if let Some(resource_id) =
            resource_id_from_metadata(metadata.as_deref()).or(inferred_resource_id)
        {
            if resource_id.starts_with("upload:") {
                self.upload_resources_in = self.upload_resources_in.saturating_add(1);
                self.upload_resource_bytes_in = self
                    .upload_resource_bytes_in
                    .saturating_add(data.len() as u64);
            }
            self.resources.insert(resource_id.clone(), data);
            replay_pending_resource_offers(
                &mut self.incoming_frames,
                &mut self.pending_resource_offers,
                &resource_id,
            );
        }
    }

    fn take_outgoing_frames(&mut self) -> Vec<Vec<u8>> {
        let frames = std::mem::take(&mut self.outgoing_frames);
        if !frames.is_empty() {
            self.last_tx_epoch_ms = current_epoch_ms();
        }
        frames
    }

    fn take_outgoing_resources(&mut self) -> Vec<(String, Vec<u8>)> {
        let resources = std::mem::take(&mut self.outgoing_resources);
        if !resources.is_empty() {
            self.last_tx_epoch_ms = current_epoch_ms();
        }
        resources
    }

    fn pending_resource_offer_count(&self) -> usize {
        self.pending_resource_offers
            .values()
            .map(VecDeque::len)
            .sum()
    }

    fn note_incoming_frame(&mut self, bytes: &[u8]) -> Option<crate::chat::protocol::ChatOp> {
        let Ok(frame) = crate::chat::codec::decode_frame(bytes) else {
            self.last_rx_frame = Some("decode error".into());
            return None;
        };
        self.last_rx_frame = Some(omenchat_monitor_frame_label(frame.op));
        match frame.op {
            crate::chat::protocol::ChatOp::HistoryInline
            | crate::chat::protocol::ChatOp::HistoryResourceOffer
            | crate::chat::protocol::ChatOp::HistoryEnd
            | crate::chat::protocol::ChatOp::HistoryCurrent => {
                self.history_frames_in = self.history_frames_in.saturating_add(1);
            }
            crate::chat::protocol::ChatOp::RoomEvent => {
                self.room_events_in = self.room_events_in.saturating_add(1);
            }
            crate::chat::protocol::ChatOp::UserListSnapshotInline
            | crate::chat::protocol::ChatOp::UserListSnapshotResource => {
                self.userlist_frames_in = self.userlist_frames_in.saturating_add(1);
            }
            crate::chat::protocol::ChatOp::UploadResourceOffer => {
                self.upload_resource_offers_in = self.upload_resource_offers_in.saturating_add(1);
            }
            crate::chat::protocol::ChatOp::UploadInlineChunk => {
                self.upload_inline_chunks_in = self.upload_inline_chunks_in.saturating_add(1);
                if let crate::chat::protocol::FrameBody::Fields(fields) = &frame.body {
                    if let Some(crate::chat::protocol::FrameValue::Bytes(chunk)) = fields.get(5) {
                        self.upload_inline_bytes_in = self
                            .upload_inline_bytes_in
                            .saturating_add(chunk.len() as u64);
                    }
                }
            }
            crate::chat::protocol::ChatOp::Pong => {
                self.pongs_in = self.pongs_in.saturating_add(1);
            }
            crate::chat::protocol::ChatOp::Ping => {
                self.pings_in = self.pings_in.saturating_add(1);
            }
            _ => {}
        }
        if matches!(
            frame.op,
            crate::chat::protocol::ChatOp::HistoryResourceOffer
                | crate::chat::protocol::ChatOp::UserListSnapshotResource
                | crate::chat::protocol::ChatOp::UploadResourceOffer
        ) {
            self.resource_offers_in = self.resource_offers_in.saturating_add(1);
        }
        Some(frame.op)
    }

    fn note_outgoing_frame(&mut self, bytes: &[u8]) {
        let Ok(frame) = crate::chat::codec::decode_frame(bytes) else {
            self.last_tx_frame = Some("decode error".into());
            return;
        };
        self.last_tx_frame = Some(omenchat_monitor_frame_label(frame.op));
        match frame.op {
            crate::chat::protocol::ChatOp::HistoryBefore
            | crate::chat::protocol::ChatOp::HistoryRecent => {
                self.history_frames_out = self.history_frames_out.saturating_add(1);
            }
            crate::chat::protocol::ChatOp::RoomMessage
            | crate::chat::protocol::ChatOp::RoomAction
            | crate::chat::protocol::ChatOp::RoomNotice
            | crate::chat::protocol::ChatOp::Command => {
                self.chat_frames_out = self.chat_frames_out.saturating_add(1);
            }
            crate::chat::protocol::ChatOp::Ping => {
                self.pings_out = self.pings_out.saturating_add(1);
            }
            crate::chat::protocol::ChatOp::UploadFetch => {
                self.upload_fetches_out = self.upload_fetches_out.saturating_add(1);
            }
            crate::chat::protocol::ChatOp::Pong => {
                self.pongs_out = self.pongs_out.saturating_add(1);
            }
            _ => {}
        }
    }
}

#[cfg(feature = "chat-client-rns")]
impl ChatLinkTransport for DesktopOmenChatTransport {
    fn send_frame(&mut self, frame_bytes: Vec<u8>) -> anyhow::Result<()> {
        self.frames_out = self.frames_out.saturating_add(1);
        self.bytes_out = self.bytes_out.saturating_add(frame_bytes.len() as u64);
        self.note_outgoing_frame(&frame_bytes);
        self.outgoing_frames.push(frame_bytes);
        Ok(())
    }

    fn recv_frame(&mut self) -> anyhow::Result<Option<Vec<u8>>> {
        Ok(self.incoming_frames.pop_front())
    }

    fn fetch_resource(&mut self, resource_id: &str) -> anyhow::Result<Option<Vec<u8>>> {
        Ok(self.resources.get(resource_id).cloned())
    }

    fn send_resource(&mut self, resource_id: &str, payload: Vec<u8>) -> anyhow::Result<()> {
        self.bytes_out = self.bytes_out.saturating_add(payload.len() as u64);
        self.outgoing_resources
            .push((resource_id.to_owned(), payload));
        Ok(())
    }

    fn defer_resource_offer(
        &mut self,
        resource_id: &str,
        frame_bytes: Vec<u8>,
    ) -> anyhow::Result<()> {
        self.pending_resource_offers
            .entry(resource_id.to_owned())
            .or_default()
            .push_back(frame_bytes);
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum DesktopPane {
    Browser(TabId),
    Conversation(u64),
    #[cfg(feature = "chat-client")]
    OmenChat(ChatSessionId),
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ExternalLinkPrompt {
    url: String,
    source_tab: Option<TabId>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ExternalBrowserChoice {
    label: String,
    command: String,
    kind: ExternalBrowserKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ExternalBrowserKind {
    Default,
    Standard,
}

#[cfg(feature = "chat-client")]
#[derive(Clone, Debug, PartialEq, Eq)]
enum OmenChatMediaLoadState {
    Loading {
        message: String,
        received: Option<u64>,
        total: Option<u64>,
    },
    Cached {
        path: String,
        content_type: String,
        animated: bool,
    },
    Failed {
        message: String,
    },
}

#[derive(Clone, Debug)]
enum Message {
    SwitchSection(WorkspaceSection),
    SelectBrowserTab(usize),
    NewBrowserTab,
    CloseBrowserTab,
    CloseBrowserPaneTab(TabId),
    NewConversationPane,
    CloseConversationPaneTab(u64),
    #[cfg(feature = "chat-client")]
    NewOmenChatPane,
    #[cfg(feature = "chat-client")]
    OmenChatServerEntryChanged(String),
    #[cfg(feature = "chat-client")]
    OpenOmenChatServerEntry,
    #[cfg(feature = "chat-client")]
    ToggleOmenChatRooms,
    #[cfg(feature = "chat-client")]
    JoinOmenChatRoom {
        session_id: ChatSessionId,
        room: String,
    },
    #[cfg(feature = "chat-client")]
    OmenChatDraftChanged {
        session_id: ChatSessionId,
        value: String,
    },
    #[cfg(feature = "chat-client")]
    OmenChatScrolled {
        session_id: ChatSessionId,
        room_id: RoomId,
        offset: RelativeOffset,
    },
    #[cfg(feature = "chat-client")]
    JumpOmenChatToPresent {
        session_id: ChatSessionId,
        room_id: RoomId,
    },
    #[cfg(feature = "chat-client")]
    SendOmenChatDraft(ChatSessionId),
    #[cfg(feature = "chat-client")]
    ResendOmenChatLocalEcho {
        session_id: ChatSessionId,
        room_id: RoomId,
        event_id: u64,
        body: String,
        action: bool,
    },
    #[cfg(feature = "chat-client")]
    LoadOlderOmenChatHistory(ChatSessionId),
    #[cfg(feature = "chat-client")]
    #[cfg(feature = "chat-client-rns")]
    RequestOmenChatPath(ChatSessionId),
    #[cfg(feature = "chat-client-rns")]
    ReconnectOmenChatSession(ChatSessionId),
    #[cfg(feature = "chat-client-rns")]
    ReconnectOmenChatSessionIfDisconnected(ChatSessionId),
    #[cfg(feature = "chat-client-rns")]
    OmenChatPathRequestResult {
        session_id: ChatSessionId,
        destination: String,
        result: Result<bool, String>,
    },
    #[cfg(feature = "chat-client-rns")]
    OmenChatLiveOpenResult {
        descriptor: OmenChatDescriptor,
        result: Result<crate::runtime::OmenChatLinkOpened, String>,
    },
    #[cfg(feature = "chat-client-rns")]
    OmenChatLiveReconnectResult {
        session_id: ChatSessionId,
        generation: u64,
        descriptor: OmenChatDescriptor,
        result: Result<crate::runtime::OmenChatLinkOpened, String>,
    },
    AddressChanged(String),
    OpenAddress,
    BrowserPaneAddressChanged {
        tab_id: TabId,
        value: String,
    },
    OpenBrowserPaneAddress(TabId),
    ReloadBrowserPane(TabId),
    BrowserPaneBack(TabId),
    BrowserPaneForward(TabId),
    BrowserPaneTop(TabId),
    StopBrowserPaneTask(TabId),
    InlineProbeBrowserPane(TabId),
    LiveProbeBrowserPane(TabId),
    WarmBrowserPanePath(TabId),
    RetryBrowserPaneAfterPath(TabId),
    BrowserPanePathDiagnostics(TabId),
    CaptureBrowserPaneRender(TabId),
    DismissBrowserPaneWarning(TabId),
    DismissBrowserPaneRequest(TabId),
    ToggleBrowserPaneIdentify(TabId),
    OpenSetupAddress,
    ReloadBrowser,
    BrowserBack,
    BrowserForward,
    StopBrowserTask,
    InlineProbe,
    LiveProbe,
    WarmPath,
    RetryAfterPath,
    PathDiagnostics,
    CaptureBrowserRender,
    ShowDiagnostics,
    PreviewManagedConfig,
    ExportManagedConfig,
    PreviewDiagnosticsBundle,
    ExportDiagnosticsBundle,
    PreviewLiveInteropReport,
    ExportLiveInteropReport,
    NativePreflight,
    NativeSmokeDryRun,
    NativeSmokeLiveProbe,
    NativeLiveFetchValidate,
    NativeLxmfSmokeSend,
    NativeLxmfInterop,
    NativeLxmfPropagationDiagnostics,
    SyncPropagationNow,
    BeginKnownDestinationsPreload,
    CreateIdentity,
    ActivateManagedIdentity(String),
    ActiveIdentityLabelChanged(String),
    DeleteActiveIdentity,
    ConfirmDeleteActiveIdentity,
    CancelDeleteActiveIdentity,
    ClearActiveIdentity,
    AnnounceIdentityNow,
    CreateTcpClientInterface,
    CreateI2pInterface,
    CreateRNodeInterface,
    CreateGatewayPreset(String),
    SwitchConversation(usize),
    ConversationScrolled {
        conversation_id: u64,
        offset: RelativeOffset,
    },
    JumpConversationToPresent(u64),
    ConversationTitleChanged(String),
    ConversationBodyChanged(String),
    ConversationPanePeerChanged {
        conversation_id: u64,
        value: String,
    },
    ConversationPaneTitleChanged {
        conversation_id: u64,
        value: String,
    },
    ConversationPaneBodyChanged {
        conversation_id: u64,
        value: String,
    },
    ConversationPaneBodyEdited {
        conversation_id: u64,
        action: text_editor::Action,
    },
    PickConversationAttachment(u64),
    ConversationAttachmentPicked {
        conversation_id: u64,
        result: Result<Option<PathBuf>, String>,
    },
    RemoveConversationAttachment {
        conversation_id: u64,
        index: usize,
    },
    OpenConversationAttachment(PathBuf),
    ToggleConversationPaneDeliveryMode(u64),
    ToggleConversationPaneTicket(u64),
    SendConversationPaneDraft(u64),
    PrepareLatestLxmfRetryForConversation(u64),
    SendLatestLxmfRetryForConversation(u64),
    SelectConversationPaneRow {
        conversation_id: u64,
        key: String,
    },
    PrepareLxmfRetryForConversationRow {
        conversation_id: u64,
        key: String,
    },
    SendLxmfRetryForConversationRow {
        conversation_id: u64,
        key: String,
    },
    DismissConversationPaneRow {
        conversation_id: u64,
        key: String,
    },
    CloseConversationPaneDetails {
        conversation_id: u64,
    },
    SyncPropagationForConversationRow {
        conversation_id: u64,
        key: String,
    },
    InspectConversationPanePeer(u64),
    RequestConversationPanePeerPath(u64),
    ConversationPaneDiagnostics(u64),
    ToggleConversationPaneTrust(u64),
    ToggleConversationDeliveryMode,
    ToggleConversationTicket,
    SendConversationDraft,
    PrepareLatestLxmfRetry,
    SendLatestLxmfRetry,
    SelectConversationRow(String),
    PrepareLxmfRetryForRow(String),
    SendLxmfRetryForRow(String),
    SyncPropagationForRow(String),
    SyncMessages,
    InspectLxmfPeer,
    RequestLxmfPeerPath,
    SwitchDirectoryKind(crate::directory::DirectoryKind),
    SwitchDirectoryScope(DirectoryScope),
    DirectoryFilterChanged(String),
    SelectDirectoryEntry(usize),
    OpenDirectoryEntry(usize),
    OpenPeerChat(usize),
    #[cfg(feature = "chat-client")]
    OpenDirectoryOmenChat(usize),
    InspectDirectoryPeer(usize),
    SaveDirectoryEntry(usize),
    ToggleDirectoryTrust(usize),
    ToggleDirectoryIdentify(usize),
    CycleDirectoryDelivery(usize),
    RequestDirectoryPath(usize),
    UseDirectoryPropagation(usize),
    ClearDirectoryPropagation,
    SelectInterfaceProfile(usize),
    ToggleInterfaceEnabled(usize),
    DeleteInterfaceProfile(usize),
    ConfirmInterfaceDelete,
    CancelInterfaceDelete,
    SelectPlugin(usize),
    TogglePlugin(usize),
    BeginPluginRemove(usize),
    ToggleSelectedPlugin,
    BeginPluginInstall,
    BeginSelectedPluginRemove,
    RefreshPlugins,
    ShowPluginLogs,
    InterfaceNameChanged {
        profile_id: String,
        value: String,
    },
    TcpClientHostChanged {
        profile_id: String,
        value: String,
    },
    TcpClientPortChanged {
        profile_id: String,
        value: String,
    },
    TcpClientIfacNetworkChanged {
        profile_id: String,
        value: String,
    },
    TcpClientIfacPassphraseChanged {
        profile_id: String,
        value: String,
    },
    TcpServerHostChanged {
        profile_id: String,
        value: String,
    },
    TcpServerPortChanged {
        profile_id: String,
        value: String,
    },
    TcpServerIfacNetworkChanged {
        profile_id: String,
        value: String,
    },
    TcpServerIfacPassphraseChanged {
        profile_id: String,
        value: String,
    },
    ToggleI2pConnectable(usize),
    I2pPeersChanged {
        profile_id: String,
        value: String,
    },
    RNodeDevicePortChanged {
        profile_id: String,
        value: String,
    },
    RNodeFrequencyChanged {
        profile_id: String,
        value: String,
    },
    RNodeBandwidthChanged {
        profile_id: String,
        value: String,
    },
    RNodeTxPowerChanged {
        profile_id: String,
        value: String,
    },
    RNodeSpreadingFactorChanged {
        profile_id: String,
        value: String,
    },
    RNodeCodingRateChanged {
        profile_id: String,
        value: String,
    },
    SetTheme(String),
    SetFontSize(u16),
    SelectPreferredExternalBrowser(usize),
    ClearPreferredExternalBrowser,
    ToggleClearwebSocksProxy,
    ToggleClearwebRemoteMedia,
    ToggleAutoSyncAfterPropagationAccept,
    SelectNativeBackend,
    StartNativeRuntime,
    NativeQuickstart,
    InterfaceStatsSampled(Result<crate::runtime::InterfaceStats, String>),
    ToggleNavigation,
    BrowserFieldKey(BrowserFieldKey),
    SubmitBrowserFieldDraft,
    CancelBrowserFieldDraft,
    FocusBrowserItem {
        reverse: bool,
    },
    ActivateFocusedBrowserItem,
    ScrollBrowserPage {
        direction: isize,
    },
    Tick,
    Page(PageMessage),
    PageForTab {
        tab_id: TabId,
        page: PageMessage,
    },
    WorkspacePaneClicked(pane_grid::Pane),
    WorkspacePaneDragged(pane_grid::DragEvent),
    WorkspacePaneResized(pane_grid::ResizeEvent),
    WorkspacePaneMaximize(pane_grid::Pane),
    WorkspacePaneRestore,
    WorkspacePaneClose(pane_grid::Pane),
    RestoreDesktopPane(DesktopPane),
    #[cfg(feature = "chat-client")]
    CloseOmenChatSession(ChatSessionId),
    WindowCloseRequested(window::Id),
    WindowShutdownComplete(window::Id),
    KeyboardModifiersChanged(keyboard::Modifiers),
    OpenExternalLinkWith(usize),
    CopyExternalLinkUrl,
    CopyActiveIdentityHash,
    PromptExternalUrl(String),
    #[cfg(feature = "chat-client")]
    OpenCachedOmenChatMedia(String),
    #[cfg(feature = "chat-client")]
    LoadOmenChatMedia(String),
    #[cfg(feature = "chat-client")]
    FetchOmenChatUploadResource {
        session_id: ChatSessionId,
        resource_id: String,
    },
    #[cfg(feature = "chat-client")]
    PickOmenChatUpload(ChatSessionId),
    #[cfg(feature = "chat-client")]
    OmenChatUploadPicked {
        session_id: ChatSessionId,
        result: Result<Option<PathBuf>, String>,
    },
    #[cfg(feature = "chat-client")]
    OmenChatGifFramesLoaded {
        path: String,
        result: Result<Vec<u8>, String>,
    },
    #[cfg(feature = "chat-client")]
    OmenChatMediaLoaded {
        url: String,
        result: Result<DownloadedFile, String>,
    },
    DismissExternalLinkPrompt,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum BrowserFieldKey {
    Insert(String),
    Backspace,
    Delete,
    MoveLeft,
    MoveRight,
    MoveHome,
    MoveEnd,
}

impl DesktopApp {
    fn new(app: App) -> Self {
        #[cfg(feature = "chat-client")]
        let (chat_client, chat_store) = {
            let chat_store_path = app
                .paths
                .identity_storage_root()
                .join("plugins")
                .join(crate::plugins::BUILTIN_OMENCHAT_PLUGIN_ID)
                .join("chat.sqlite");
            match SqliteChatStore::open(&chat_store_path) {
                Ok(mut store) => {
                    prune_unrestorable_omenchat_servers(&mut store);
                    let mut client = ChatClient::new();
                    if let Err(error) = client.restore_from_store(&store, 100) {
                        tracing::warn!(
                            "failed to restore OMENchat sessions from {}: {error}",
                            chat_store_path.display()
                        );
                    }
                    (client, Some(store))
                }
                Err(error) => {
                    tracing::warn!(
                        "failed to open OMENchat store at {}: {error}",
                        chat_store_path.display()
                    );
                    (ChatClient::new(), None)
                }
            }
        };
        #[cfg(feature = "chat-client")]
        let omenchat_session_ids = chat_client
            .sessions()
            .iter()
            .map(|session| session.session_id)
            .collect::<Vec<_>>();
        #[cfg(not(feature = "chat-client"))]
        let omenchat_session_ids = Vec::new();

        let mut workspace_panes = restored_desktop_pane_state(&app, &omenchat_session_ids);
        let mut pane_order = desktop_pane_order(workspace_panes.layout());
        let first_pane = pane_order
            .first()
            .copied()
            .or_else(|| workspace_panes.iter().next().map(|(pane, _)| *pane))
            .expect("desktop workspace has at least one pane");
        if workspace_panes.len() == 1 {
            let has_conversation = workspace_panes
                .iter()
                .any(|(_, pane)| matches!(pane, DesktopPane::Conversation(_)));
            if !has_conversation {
                if let Some((new_pane, _)) = workspace_panes.split(
                    pane_grid::Axis::Vertical,
                    first_pane,
                    DesktopPane::Conversation(app.active_conversation().id),
                ) {
                    pane_order.push(new_pane);
                }
            }
        }
        if pane_order.is_empty() {
            pane_order = desktop_pane_order(workspace_panes.layout());
        }
        let active_workspace_pane = app
            .settings
            .ui
            .active_desktop_workspace_pane
            .and_then(|index| pane_order.get(index).copied())
            .unwrap_or(first_pane);

        let conversation_body_editors = app
            .workspace
            .conversations
            .iter()
            .map(|conversation| {
                (
                    conversation.id,
                    text_editor::Content::with_text(&conversation.draft_body),
                )
            })
            .collect::<HashMap<_, _>>();
        let conversation_message_counts = app
            .workspace
            .conversations
            .iter()
            .map(|conversation| (conversation.id, 0))
            .collect::<HashMap<_, _>>();
        let conversation_scroll_offsets = workspace_panes
            .iter()
            .filter_map(|(_, pane)| match pane {
                DesktopPane::Conversation(conversation_id) => {
                    Some((*conversation_id, RelativeOffset { x: 0.0, y: 1.0 }))
                }
                DesktopPane::Browser(_) => None,
                #[cfg(feature = "chat-client")]
                DesktopPane::OmenChat(_) => None,
            })
            .collect::<HashMap<_, _>>();
        let conversation_scroll_restore_locks = conversation_scroll_offsets
            .keys()
            .copied()
            .collect::<HashSet<_>>();

        #[cfg(feature = "chat-client")]
        let chat_drafts = HashMap::new();
        #[cfg(feature = "chat-client")]
        let chat_event_counts = omenchat_event_counts_by_room(chat_client.sessions());
        #[cfg(feature = "chat-client")]
        let chat_scroll_offsets = workspace_panes
            .iter()
            .filter_map(|(_, pane)| match pane {
                DesktopPane::OmenChat(session_id) => {
                    let room_id = chat_client
                        .session(*session_id)
                        .map(|session| session.active_room.room_id)
                        .unwrap_or(1);
                    Some(((*session_id, room_id), RelativeOffset { x: 0.0, y: 1.0 }))
                }
                DesktopPane::Browser(_) | DesktopPane::Conversation(_) => None,
            })
            .collect::<HashMap<_, _>>();
        #[cfg(feature = "chat-client")]
        let restore_workspace_scrolls_pending =
            !conversation_scroll_offsets.is_empty() || !chat_scroll_offsets.is_empty();
        #[cfg(not(feature = "chat-client"))]
        let restore_workspace_scrolls_pending = !conversation_scroll_offsets.is_empty();
        #[cfg(feature = "chat-client")]
        let chat_scroll_bottom_locks = chat_scroll_offsets.keys().copied().collect::<HashSet<_>>();
        let external_browsers = detect_external_browsers(
            app.settings
                .clearweb
                .preferred_external_browser_command
                .as_deref(),
        );
        let clearweb_proxy_endpoint = detect_clearweb_socks_proxy(
            &app.settings.clearweb.socks_proxy_host,
            app.settings.clearweb.socks_proxy_port,
        );
        let clearweb_proxy_reachable = clearweb_proxy_endpoint.is_some();

        Self {
            app,
            conversation_body_editors,
            conversation_message_counts,
            conversation_scroll_offsets,
            conversation_scroll_restore_locks,
            navigation_open: true,
            identity_delete_confirming: false,
            workspace_panes,
            active_workspace_pane,
            ctrl_down: false,
            restore_workspace_scrolls_pending,
            restore_workspace_scrolls_remaining: if restore_workspace_scrolls_pending {
                5
            } else {
                0
            },
            restore_workspace_scroll_locks_release_pending: false,
            pending_workspace_bottom_anchor_ticks: if restore_workspace_scrolls_pending {
                2
            } else {
                0
            },
            shutdown_requested: false,
            monitoring_sample_epoch_ms: 0,
            monitoring_process_usage: None,
            debug_tick_count: 0,
            debug_last_tick_epoch_ms: 0,
            external_link_prompt: None,
            external_browsers,
            clearweb_proxy_reachable,
            clearweb_proxy_endpoint,
            #[cfg(feature = "chat-client")]
            chat_client,
            #[cfg(feature = "chat-client")]
            chat_store,
            #[cfg(feature = "chat-client")]
            chat_drafts,
            #[cfg(feature = "chat-client")]
            chat_event_counts,
            #[cfg(feature = "chat-client")]
            chat_scroll_offsets,
            #[cfg(feature = "chat-client")]
            chat_scroll_bottom_locks,
            #[cfg(feature = "chat-client")]
            omenchat_motds: HashMap::new(),
            #[cfg(feature = "chat-client")]
            omenchat_upload_quotas: HashMap::new(),
            #[cfg(feature = "chat-client")]
            omenchat_upload_max_file_bytes: HashMap::new(),
            #[cfg(feature = "chat-client")]
            omenchat_media_cache: HashMap::new(),
            #[cfg(feature = "chat-client")]
            omenchat_gif_frames: HashMap::new(),
            #[cfg(feature = "chat-client")]
            omenchat_server_entry: String::new(),
            #[cfg(feature = "chat-client")]
            omenchat_rooms_visible: true,
            #[cfg(feature = "chat-client")]
            omenchat_pending_upload_sources: HashMap::new(),
            #[cfg(feature = "chat-client-rns")]
            omenchat_live_state: crate::chat::live::LiveChatClientState::default(),
            #[cfg(feature = "chat-client-rns")]
            omenchat_live_transports: HashMap::new(),
            #[cfg(feature = "chat-client-rns")]
            omenchat_link_sessions: HashMap::new(),
            #[cfg(feature = "chat-client-rns")]
            omenchat_live_opening: HashSet::new(),
            #[cfg(feature = "chat-client-rns")]
            omenchat_live_retry_after: HashMap::new(),
            #[cfg(feature = "chat-client-rns")]
            omenchat_live_retry_count: HashMap::new(),
            #[cfg(feature = "chat-client-rns")]
            omenchat_live_connect_count: HashMap::new(),
            #[cfg(feature = "chat-client-rns")]
            omenchat_live_disconnect_count: HashMap::new(),
            #[cfg(feature = "chat-client-rns")]
            omenchat_live_last_disconnect_reason: HashMap::new(),
            #[cfg(feature = "chat-client-rns")]
            omenchat_recent_sync_pending: HashSet::new(),
            #[cfg(feature = "chat-client-rns")]
            omenchat_recent_sync_links: HashMap::new(),
            #[cfg(feature = "chat-client-rns")]
            omenchat_recent_sync_due_after: HashMap::new(),
            #[cfg(feature = "chat-client-rns")]
            omenchat_recent_sync_attempts: HashMap::new(),
            #[cfg(feature = "chat-client-rns")]
            omenchat_live_reconnect_generation: HashMap::new(),
        }
    }

    fn theme(&self) -> Theme {
        theme_from_name(&self.app.settings.ui.theme_name)
    }

    fn subscription(&self) -> Subscription<Message> {
        let browser_field_active = self.app.workspace.active_section == WorkspaceSection::Browser
            && self.app.active_browser_field_editor().is_some();
        let keyboard_subscription = if browser_field_active {
            event::listen_with(map_browser_field_keyboard_event)
        } else {
            keyboard::on_key_press(map_key_press)
        };
        let modifier_subscription = if browser_field_active {
            Subscription::none()
        } else {
            event::listen_with(map_keyboard_modifier_event)
        };
        Subscription::batch([
            keyboard_subscription,
            modifier_subscription,
            time::every(Duration::from_millis(self.desktop_tick_ms())).map(|_| Message::Tick),
            window::close_requests().map(Message::WindowCloseRequested),
        ])
    }

    fn desktop_tick_ms(&self) -> u64 {
        if self.pending_workspace_bottom_anchor_ticks > 0 {
            return DESKTOP_LIVE_TICK_MS;
        }
        if self.app.workspace.active_section == WorkspaceSection::Browser
            && self
                .app
                .browser_partials_need_low_latency_tick(current_epoch_ms())
        {
            DESKTOP_LIVE_TICK_MS
        } else {
            DESKTOP_IDLE_TICK_MS
        }
    }

    fn sample_runtime_interface_stats(&self) -> Task<Message> {
        let runtime = self.app.runtime.clone();
        Task::perform(
            async move {
                runtime
                    .interface_stats()
                    .await
                    .map_err(|error| error.to_string())
            },
            Message::InterfaceStatsSampled,
        )
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::SwitchSection(section) => {
                self.app.switch_section(section);
                if matches!(
                    section,
                    WorkspaceSection::Browser | WorkspaceSection::Messages
                ) {
                    self.schedule_visible_workspace_scroll_restore(2);
                    return self.restore_visible_workspace_scrolls();
                }
            }
            Message::SelectBrowserTab(index) => {
                self.app.select_browser_tab(index);
                self.ensure_pane_for_active_browser();
            }
            Message::NewBrowserTab => {
                self.app.finish_active_browser_field_edit_preserving_value();
                self.app.new_browser_tab();
                self.ensure_pane_for_active_browser();
                self.persist_workspace_panes("workspace panes");
                return self.anchor_visible_workspace_scrolls_to_bottom_now(2);
            }
            Message::CloseBrowserTab => {
                let closing_id = self.app.active_browser_tab().id;
                self.app.close_active_browser_tab();
                self.remove_workspace_panes_for_missing_targets(Some(closing_id), None);
                self.app.flush_pending_ui_preferences();
                self.persist_workspace_panes("workspace panes");
            }
            Message::CloseBrowserPaneTab(tab_id) => {
                if self.select_browser_tab_by_id(tab_id) {
                    let closing_id = self.app.active_browser_tab().id;
                    self.app.close_active_browser_tab();
                    self.remove_workspace_panes_for_missing_targets(Some(closing_id), None);
                    self.app.flush_pending_ui_preferences();
                    self.persist_workspace_panes("workspace panes");
                }
            }
            Message::NewConversationPane => {
                self.app.new_conversation();
                self.ensure_pane_for_active_conversation();
                self.persist_workspace_panes("workspace panes");
                return self.anchor_visible_workspace_scrolls_to_bottom_now(2);
            }
            #[cfg(feature = "chat-client")]
            Message::NewOmenChatPane => {
                let session_id = self.create_blank_omenchat_session();
                self.ensure_pane_for_omenchat(session_id);
                self.persist_workspace_panes("workspace panes");
                return self.anchor_visible_workspace_scrolls_to_bottom_now(2);
            }
            #[cfg(feature = "chat-client")]
            Message::OmenChatServerEntryChanged(value) => {
                self.omenchat_server_entry = value;
            }
            #[cfg(feature = "chat-client")]
            Message::OpenOmenChatServerEntry => {
                let Some(target) = normalize_omenchat_manual_target(&self.omenchat_server_entry)
                else {
                    self.app.status.task =
                        "enter an OMENchat destination hash or omenchat://<hash>".into();
                    return Task::none();
                };
                self.omenchat_server_entry.clear();
                return self
                    .open_omenchat_link(crate::micron::LinkAction {
                        target,
                        fields: Vec::new(),
                    })
                    .unwrap_or_else(Task::none);
            }
            #[cfg(feature = "chat-client")]
            Message::ToggleOmenChatRooms => {
                self.omenchat_rooms_visible = !self.omenchat_rooms_visible;
            }
            #[cfg(feature = "chat-client")]
            Message::JoinOmenChatRoom { session_id, room } => {
                self.join_omenchat_room(session_id, room);
                self.schedule_visible_workspace_scroll_restore(2);
                return self.restore_omenchat_scroll(session_id);
            }
            #[cfg(feature = "chat-client")]
            Message::OmenChatDraftChanged { session_id, value } => {
                self.chat_drafts.insert(session_id, value);
            }
            #[cfg(feature = "chat-client")]
            Message::OmenChatScrolled {
                session_id,
                room_id,
                offset,
            } => {
                if !self.workspace_scroll_pane_is_visible(DesktopPane::OmenChat(session_id))
                    || self.is_workspace_scroll_restore_settling()
                    || self
                        .chat_scroll_bottom_locks
                        .contains(&(session_id, room_id))
                {
                    return Task::none();
                }
                self.chat_scroll_offsets
                    .insert((session_id, room_id), sanitize_scroll_offset(offset));
            }
            #[cfg(feature = "chat-client")]
            Message::JumpOmenChatToPresent {
                session_id,
                room_id,
            } => {
                self.chat_scroll_offsets
                    .insert((session_id, room_id), RelativeOffset { x: 0.0, y: 1.0 });
                return iced::widget::scrollable::snap_to(
                    omenchat_scroll_id(session_id, room_id),
                    RelativeOffset { x: 0.0, y: 1.0 },
                );
            }
            #[cfg(feature = "chat-client")]
            Message::SendOmenChatDraft(session_id) => {
                self.send_omenchat_draft(session_id);
            }
            #[cfg(feature = "chat-client")]
            Message::ResendOmenChatLocalEcho {
                session_id,
                room_id,
                event_id,
                body,
                action,
            } => {
                self.resend_omenchat_local_echo(session_id, room_id, event_id, body, action);
            }
            #[cfg(feature = "chat-client")]
            Message::LoadOlderOmenChatHistory(session_id) => {
                self.load_older_omenchat_history(session_id);
            }
            #[cfg(feature = "chat-client-rns")]
            Message::RequestOmenChatPath(session_id) => {
                return self.request_omenchat_path_task(session_id);
            }
            #[cfg(feature = "chat-client-rns")]
            Message::ReconnectOmenChatSession(session_id) => {
                return self.reconnect_omenchat_session_task(session_id);
            }
            #[cfg(feature = "chat-client-rns")]
            Message::ReconnectOmenChatSessionIfDisconnected(session_id) => {
                return self.reconnect_omenchat_session_if_disconnected_task(session_id);
            }
            #[cfg(feature = "chat-client-rns")]
            Message::OmenChatPathRequestResult {
                session_id,
                destination,
                result,
            } => match result {
                Ok(true) => {
                    self.app.status.task = format!("OMENchat path request queued: {destination}");
                    if self.omenchat_live_transports.contains_key(&session_id) {
                        self.clear_omenchat_reconnect_state(session_id);
                        self.set_omenchat_session_status(
                            session_id,
                            format!(
                                "path request queued for {destination}; live link remains active"
                            ),
                        );
                    } else if self.omenchat_live_opening.contains(&session_id) {
                        self.set_omenchat_session_status(
                            session_id,
                            format!(
                                "path request queued for {destination}; reconnect already pending"
                            ),
                        );
                    } else {
                        self.set_omenchat_session_status(
                            session_id,
                            format!(
                                "path request queued for {destination}; reconnecting after announce wait"
                            ),
                        );
                        return delayed_omenchat_reconnect_if_disconnected_task(session_id);
                    }
                }
                Ok(false) => {
                    self.set_omenchat_session_status(
                        session_id,
                        format!("path request not queued for {destination}"),
                    );
                    self.app.status.task =
                        format!("OMENchat path request not queued: {destination}");
                }
                Err(error) => {
                    self.set_omenchat_session_status(
                        session_id,
                        format!("path request failed: {error}"),
                    );
                    self.app.status.task = format!("OMENchat path request failed: {error}");
                }
            },
            #[cfg(feature = "chat-client-rns")]
            Message::OmenChatLiveOpenResult { descriptor, result } => {
                return self.handle_omenchat_live_open_result(descriptor, result);
            }
            #[cfg(feature = "chat-client-rns")]
            Message::OmenChatLiveReconnectResult {
                session_id,
                generation,
                descriptor,
                result,
            } => {
                return self.handle_omenchat_live_reconnect_result(
                    session_id, generation, descriptor, result,
                );
            }
            Message::CloseConversationPaneTab(conversation_id) => {
                if self.select_conversation_by_id(conversation_id) {
                    let closing_id = self.app.active_conversation().id;
                    let closing_pane =
                        self.find_workspace_pane(&DesktopPane::Conversation(closing_id));
                    self.app.delete_active_conversation();
                    self.close_or_replace_deleted_conversation_pane(closing_pane);
                    self.remove_workspace_panes_for_missing_targets(None, Some(closing_id));
                    self.conversation_body_editors.remove(&closing_id);
                    self.conversation_message_counts.remove(&closing_id);
                    self.conversation_scroll_offsets.remove(&closing_id);
                    self.conversation_scroll_restore_locks.remove(&closing_id);
                    self.app.flush_pending_ui_preferences();
                    self.persist_workspace_panes("workspace panes");
                }
            }
            Message::AddressChanged(value) => {
                self.app.finish_active_browser_field_edit_preserving_value();
                let tab = self.app.active_browser_tab_mut();
                tab.address_input = value;
            }
            Message::BrowserPaneAddressChanged { tab_id, value } => {
                if self.select_browser_tab_by_id(tab_id) {
                    self.app.finish_active_browser_field_edit_preserving_value();
                    self.app.active_browser_tab_mut().address_input = value;
                }
            }
            Message::OpenAddress => {
                let target = self.app.active_browser_tab().address_input.clone();
                if !self.prompt_external_url_if_needed(target, None) {
                    self.app.open_active_browser_address();
                }
            }
            Message::OpenBrowserPaneAddress(tab_id) => {
                if self.select_browser_tab_by_id(tab_id) {
                    let target = self.app.active_browser_tab().address_input.clone();
                    if !self.prompt_external_url_if_needed(target, Some(tab_id)) {
                        self.app.open_active_browser_address();
                    }
                }
            }
            Message::ReloadBrowserPane(tab_id) => {
                if self.select_browser_tab_by_id(tab_id) {
                    self.app.reload_active_browser();
                }
            }
            Message::BrowserPaneBack(tab_id) => {
                if self.select_browser_tab_by_id(tab_id) {
                    self.app.browser_back();
                }
            }
            Message::BrowserPaneForward(tab_id) => {
                if self.select_browser_tab_by_id(tab_id) {
                    self.app.browser_forward();
                }
            }
            Message::BrowserPaneTop(tab_id) => {
                if self.select_browser_tab_by_id(tab_id) {
                    self.app.scroll_browser_tab_to_top(tab_id);
                }
            }
            Message::StopBrowserPaneTask(tab_id) => {
                if self.select_browser_tab_by_id(tab_id) {
                    self.app.cancel_active_browser_load();
                }
            }
            Message::InlineProbeBrowserPane(tab_id) => {
                if self.select_browser_tab_by_id(tab_id) {
                    self.app.probe_active_browser_page_fetch_inline();
                }
            }
            Message::LiveProbeBrowserPane(tab_id) => {
                if self.select_browser_tab_by_id(tab_id) {
                    self.app.probe_active_browser_page_fetch(true);
                }
            }
            Message::WarmBrowserPanePath(tab_id) => {
                if self.select_browser_tab_by_id(tab_id) {
                    self.app.warm_active_browser_path();
                }
            }
            Message::RetryBrowserPaneAfterPath(tab_id) => {
                if self.select_browser_tab_by_id(tab_id) {
                    self.app.retry_active_browser_after_path_discovery();
                }
            }
            Message::BrowserPanePathDiagnostics(tab_id) => {
                if self.select_browser_tab_by_id(tab_id) {
                    self.app.set_diagnostics_target_for_browser_tab(tab_id);
                    self.app.run_active_browser_path_discovery_diagnostics();
                }
            }
            Message::CaptureBrowserPaneRender(tab_id) => {
                if self.select_browser_tab_by_id(tab_id) {
                    self.app.export_active_browser_render_fixture();
                }
            }
            Message::DismissBrowserPaneWarning(tab_id) => {
                if self.select_browser_tab_by_id(tab_id) {
                    self.app.dismiss_active_browser_live_warning();
                }
            }
            Message::DismissBrowserPaneRequest(tab_id) => {
                if self.select_browser_tab_by_id(tab_id) {
                    self.app.dismiss_active_browser_request_preview();
                }
            }
            Message::ToggleBrowserPaneIdentify(tab_id) => {
                if self.select_browser_tab_by_id(tab_id) {
                    self.app.toggle_active_browser_node_identify_on_connect();
                }
            }
            Message::OpenSetupAddress => {
                self.app.switch_section(WorkspaceSection::Browser);
                self.app.open_active_browser_address();
            }
            Message::ReloadBrowser => self.app.reload_active_browser(),
            Message::BrowserBack => self.app.browser_back(),
            Message::BrowserForward => self.app.browser_forward(),
            Message::StopBrowserTask => self.app.cancel_active_browser_load(),
            Message::InlineProbe => {
                self.app.probe_active_browser_page_fetch_inline();
            }
            Message::LiveProbe => {
                self.app.probe_active_browser_page_fetch(true);
            }
            Message::WarmPath => {
                self.app.warm_active_browser_path();
            }
            Message::RetryAfterPath => {
                self.app.retry_active_browser_after_path_discovery();
            }
            Message::PathDiagnostics => {
                self.app.run_active_browser_path_discovery_diagnostics();
            }
            Message::CaptureBrowserRender => {
                self.app.export_active_browser_render_fixture();
            }
            Message::ShowDiagnostics => self.app.switch_section(WorkspaceSection::Diagnostics),
            Message::PreviewManagedConfig => {
                self.app.preview_managed_reticulum_config();
            }
            Message::ExportManagedConfig => {
                self.app.export_managed_reticulum_config();
            }
            Message::PreviewDiagnosticsBundle => {
                self.app.preview_diagnostics_bundle();
            }
            Message::ExportDiagnosticsBundle => {
                self.app.export_diagnostics_bundle();
            }
            Message::PreviewLiveInteropReport => {
                self.app.preview_live_interop_report();
            }
            Message::ExportLiveInteropReport => {
                self.app.export_live_interop_report();
            }
            Message::NativePreflight => {
                self.app.run_native_preflight_report();
            }
            Message::NativeSmokeDryRun => {
                self.app.run_native_network_smoke_test(false);
            }
            Message::NativeSmokeLiveProbe => {
                self.app.run_native_network_smoke_test(true);
            }
            Message::NativeLiveFetchValidate => {
                self.app.run_native_network_live_fetch_validation();
            }
            Message::NativeLxmfSmokeSend => {
                self.app.run_native_lxmf_smoke_send();
            }
            Message::NativeLxmfInterop => {
                self.app.run_native_lxmf_live_interop();
            }
            Message::NativeLxmfPropagationDiagnostics => {
                self.app.run_native_lxmf_propagation_diagnostics();
            }
            Message::SyncPropagationNow => {
                self.app.sync_propagation_messages_now();
            }
            Message::BeginKnownDestinationsPreload => {
                self.app.begin_known_destinations_preload_flow();
            }
            Message::CreateIdentity => {
                self.app.create_settings_managed_identity();
            }
            Message::ActivateManagedIdentity(path) => {
                self.app.activate_managed_identity_path(PathBuf::from(path));
            }
            Message::ActiveIdentityLabelChanged(label) => {
                self.app.set_active_identity_label(label);
            }
            Message::DeleteActiveIdentity => {
                self.identity_delete_confirming = true;
                self.app.status.task =
                    "confirm identity deletion before removing active identity".into();
            }
            Message::ConfirmDeleteActiveIdentity => {
                self.app.delete_active_identity_with_backup();
                self.identity_delete_confirming = false;
            }
            Message::CancelDeleteActiveIdentity => {
                self.identity_delete_confirming = false;
                self.app.status.task = "identity deletion cancelled".into();
            }
            Message::ClearActiveIdentity => {
                self.app.clear_active_identity();
                self.identity_delete_confirming = false;
            }
            Message::AnnounceIdentityNow => {
                self.app.announce_local_lxmf_now();
            }
            Message::CreateTcpClientInterface => {
                self.app.create_tcp_client_interface_profile();
            }
            Message::CreateI2pInterface => {
                self.app.create_i2p_interface_profile();
            }
            Message::CreateRNodeInterface => {
                self.app.create_rnode_interface_profile();
            }
            Message::CreateGatewayPreset(gateway_id) => {
                self.app.create_gateway_interface_profile(&gateway_id);
            }
            Message::SwitchConversation(index) => {
                self.app.select_conversation_tab(index);
                self.ensure_pane_for_active_conversation();
                self.schedule_visible_workspace_scroll_restore(2);
                return self.restore_visible_conversation_scrolls();
            }
            Message::ConversationScrolled {
                conversation_id,
                offset,
            } => {
                if !self
                    .workspace_scroll_pane_is_visible(DesktopPane::Conversation(conversation_id))
                    || self.is_workspace_scroll_restore_settling()
                    || self
                        .conversation_scroll_restore_locks
                        .contains(&conversation_id)
                {
                    return Task::none();
                }
                self.conversation_scroll_offsets
                    .insert(conversation_id, sanitize_scroll_offset(offset));
            }
            Message::JumpConversationToPresent(conversation_id) => {
                self.conversation_scroll_offsets
                    .insert(conversation_id, RelativeOffset { x: 0.0, y: 1.0 });
                return iced::widget::scrollable::snap_to(
                    conversation_scroll_id(conversation_id),
                    RelativeOffset { x: 0.0, y: 1.0 },
                );
            }
            Message::ConversationTitleChanged(value) => {
                self.app.set_active_conversation_draft_title(value);
            }
            Message::ConversationBodyChanged(value) => {
                self.app.set_active_conversation_draft_body(value);
                self.sync_conversation_body_editor(self.app.active_conversation().id);
            }
            Message::ConversationPanePeerChanged {
                conversation_id,
                value,
            } => {
                if self.select_conversation_by_id(conversation_id) {
                    self.app.set_active_conversation_peer_hash(value);
                }
            }
            Message::ConversationPaneTitleChanged {
                conversation_id,
                value,
            } => {
                if self.select_conversation_by_id(conversation_id) {
                    self.app.set_active_conversation_draft_title(value);
                }
            }
            Message::ConversationPaneBodyChanged {
                conversation_id,
                value,
            } => {
                if self.select_conversation_by_id(conversation_id) {
                    self.app.set_active_conversation_draft_body(value);
                    self.sync_conversation_body_editor(conversation_id);
                }
            }
            Message::ConversationPaneBodyEdited {
                conversation_id,
                action,
            } => {
                if self.select_conversation_by_id(conversation_id) {
                    let editor = self.conversation_body_editor_mut(conversation_id);
                    editor.perform(action);
                    let value = conversation_editor_text(editor);
                    self.app.set_active_conversation_draft_body(value);
                }
            }
            Message::PickConversationAttachment(conversation_id) => {
                return Task::perform(
                    async move { pick_conversation_attachment_file() },
                    move |result| Message::ConversationAttachmentPicked {
                        conversation_id,
                        result,
                    },
                );
            }
            Message::ConversationAttachmentPicked {
                conversation_id,
                result,
            } => match result {
                Ok(Some(path)) => {
                    if self.select_conversation_by_id(conversation_id) {
                        self.app.add_active_conversation_attachment(path.clone());
                        self.app.status.task = format!(
                            "attached {}",
                            path.file_name()
                                .and_then(|name| name.to_str())
                                .unwrap_or("file")
                        );
                    }
                }
                Ok(None) => {
                    self.app.status.task = "attachment picker cancelled".into();
                }
                Err(error) => {
                    self.app.status.task = format!("attachment picker failed: {error}");
                }
            },
            Message::RemoveConversationAttachment {
                conversation_id,
                index,
            } => {
                if self.select_conversation_by_id(conversation_id) {
                    self.app.remove_active_conversation_attachment(index);
                }
            }
            Message::OpenConversationAttachment(path) => {
                self.open_local_file(path);
            }
            Message::ToggleConversationPaneDeliveryMode(conversation_id) => {
                if self.select_conversation_by_id(conversation_id) {
                    self.app.toggle_active_conversation_delivery_mode();
                }
            }
            Message::ToggleConversationPaneTicket(conversation_id) => {
                if self.select_conversation_by_id(conversation_id) {
                    self.app.toggle_active_conversation_ticket();
                }
            }
            Message::SendConversationPaneDraft(conversation_id) => {
                if self.select_conversation_by_id(conversation_id) {
                    self.app.send_active_conversation_draft();
                    self.clear_conversation_body_editor(conversation_id);
                }
            }
            Message::PrepareLatestLxmfRetryForConversation(conversation_id) => {
                if self.select_conversation_by_id(conversation_id) {
                    self.app.prepare_latest_lxmf_retry();
                    self.sync_conversation_body_editor(conversation_id);
                }
            }
            Message::SendLatestLxmfRetryForConversation(conversation_id) => {
                if self.select_conversation_by_id(conversation_id) {
                    self.app.send_latest_lxmf_retry();
                    self.sync_conversation_body_editor(conversation_id);
                }
            }
            Message::SelectConversationPaneRow {
                conversation_id,
                key,
            } => {
                if self.select_conversation_by_id(conversation_id) {
                    self.app.select_active_conversation_message(key);
                }
            }
            Message::PrepareLxmfRetryForConversationRow {
                conversation_id,
                key,
            } => {
                if self.select_conversation_by_id(conversation_id) {
                    self.app.prepare_lxmf_retry_by_message_key(&key);
                    self.sync_conversation_body_editor(conversation_id);
                }
            }
            Message::SendLxmfRetryForConversationRow {
                conversation_id,
                key,
            } => {
                if self.select_conversation_by_id(conversation_id) {
                    self.app.send_lxmf_retry_by_message_key(&key);
                    self.sync_conversation_body_editor(conversation_id);
                }
            }
            Message::DismissConversationPaneRow {
                conversation_id,
                key,
            } => {
                if self.select_conversation_by_id(conversation_id) {
                    self.app.dismiss_active_conversation_message(&key);
                }
            }
            Message::CloseConversationPaneDetails { conversation_id } => {
                if self.select_conversation_by_id(conversation_id) {
                    self.app.clear_active_conversation_message_selection();
                }
            }
            Message::SyncPropagationForConversationRow {
                conversation_id,
                key,
            } => {
                if self.select_conversation_by_id(conversation_id) {
                    self.app.sync_propagation_for_message_key(&key);
                }
            }
            Message::InspectConversationPanePeer(conversation_id) => {
                if self.select_conversation_by_id(conversation_id) {
                    self.app.inspect_active_lxmf_peer();
                }
            }
            Message::RequestConversationPanePeerPath(conversation_id) => {
                if self.select_conversation_by_id(conversation_id) {
                    self.app.request_active_lxmf_peer_path();
                }
            }
            Message::ConversationPaneDiagnostics(conversation_id) => {
                if self.select_conversation_by_id(conversation_id) {
                    self.app
                        .set_diagnostics_target_for_conversation(conversation_id);
                    self.app.inspect_active_lxmf_peer();
                    self.app.switch_section(WorkspaceSection::Diagnostics);
                }
            }
            Message::ToggleConversationPaneTrust(conversation_id) => {
                if self.select_conversation_by_id(conversation_id) {
                    self.app.toggle_active_lxmf_peer_trust();
                }
            }
            Message::ToggleConversationDeliveryMode => {
                self.app.toggle_active_conversation_delivery_mode();
            }
            Message::ToggleConversationTicket => {
                self.app.toggle_active_conversation_ticket();
            }
            Message::SendConversationDraft => {
                self.app.send_active_conversation_draft();
            }
            Message::PrepareLatestLxmfRetry => {
                self.app.prepare_latest_lxmf_retry();
            }
            Message::SendLatestLxmfRetry => {
                self.app.send_latest_lxmf_retry();
            }
            Message::SelectConversationRow(key) => {
                self.app.select_active_conversation_message(key);
            }
            Message::PrepareLxmfRetryForRow(key) => {
                self.app.prepare_lxmf_retry_by_message_key(&key);
            }
            Message::SendLxmfRetryForRow(key) => {
                self.app.send_lxmf_retry_by_message_key(&key);
            }
            Message::SyncPropagationForRow(key) => {
                self.app.sync_propagation_for_message_key(&key);
            }
            Message::SyncMessages => {
                self.app.sync_runtime_messages();
            }
            Message::InspectLxmfPeer => {
                self.app.inspect_active_lxmf_peer();
            }
            Message::RequestLxmfPeerPath => {
                self.app.request_active_lxmf_peer_path();
            }
            Message::SwitchDirectoryKind(kind) => {
                self.app.switch_directory_kind(kind);
            }
            Message::SwitchDirectoryScope(scope) => {
                self.app.switch_directory_scope(scope);
            }
            Message::DirectoryFilterChanged(value) => {
                self.app.set_directory_filter(value);
            }
            Message::SelectDirectoryEntry(index) => {
                self.app.select_directory_entry(index);
            }
            Message::OpenDirectoryEntry(index) => {
                if self.app.select_directory_entry(index) {
                    self.app.open_selected_directory_entry();
                    self.ensure_pane_for_active_browser();
                    self.persist_workspace_panes("workspace panes");
                }
            }
            Message::OpenPeerChat(index) => {
                if self.app.select_directory_entry(index) {
                    self.app.message_selected_directory_peer();
                    self.ensure_pane_for_active_conversation();
                    self.persist_workspace_panes("workspace panes");
                }
            }
            #[cfg(feature = "chat-client")]
            Message::OpenDirectoryOmenChat(index) => {
                if self.app.select_directory_entry(index) {
                    if let Some(entry) = self.app.selected_directory_entry() {
                        let target = format!("omenchat://{}", entry.destination_hash);
                        if let Some(task) = self.open_omenchat_link(crate::micron::LinkAction {
                            target,
                            fields: Vec::new(),
                        }) {
                            return task;
                        }
                    }
                }
            }
            Message::InspectDirectoryPeer(index) => {
                if self.app.select_directory_entry(index) {
                    self.app.inspect_selected_directory_peer();
                }
            }
            Message::SaveDirectoryEntry(index) => {
                if self.app.select_directory_entry(index) {
                    self.app.save_selected_directory_entry();
                }
            }
            Message::ToggleDirectoryTrust(index) => {
                if self.app.select_directory_entry(index) {
                    self.app.toggle_selected_directory_trust();
                }
            }
            Message::ToggleDirectoryIdentify(index) => {
                if self.app.select_directory_entry(index) {
                    self.app.toggle_selected_directory_identify_on_connect();
                }
            }
            Message::CycleDirectoryDelivery(index) => {
                if self.app.select_directory_entry(index) {
                    self.app.cycle_selected_directory_preferred_delivery();
                }
            }
            Message::RequestDirectoryPath(index) => {
                if self.app.select_directory_entry(index) {
                    self.app.request_selected_directory_path();
                }
            }
            Message::UseDirectoryPropagation(index) => {
                if self.app.select_directory_entry(index) {
                    self.app.use_selected_directory_propagation_node();
                }
            }
            Message::ClearDirectoryPropagation => {
                self.app.clear_preferred_propagation_node();
            }
            Message::SelectInterfaceProfile(index) => {
                self.app.select_interface_profile(index);
            }
            Message::ToggleInterfaceEnabled(index) => {
                self.app.select_interface_profile(index);
                self.app.toggle_selected_interface_enabled();
            }
            Message::DeleteInterfaceProfile(index) => {
                self.app.select_interface_profile(index);
                self.app.begin_selected_interface_delete_flow();
            }
            Message::ConfirmInterfaceDelete => {
                self.app.confirm_pending_interface_delete();
            }
            Message::CancelInterfaceDelete => {
                self.app.cancel_pending_interface_delete();
            }
            Message::SelectPlugin(index) => {
                self.app.select_plugin(index);
            }
            Message::TogglePlugin(index) => {
                if self.app.select_plugin(index) {
                    self.app.toggle_selected_plugin();
                }
            }
            Message::BeginPluginRemove(index) => {
                if self.app.select_plugin(index) {
                    self.app.begin_selected_plugin_remove_flow();
                }
            }
            Message::ToggleSelectedPlugin => {
                self.app.toggle_selected_plugin();
            }
            Message::BeginPluginInstall => {
                self.app.begin_plugin_install_flow();
            }
            Message::BeginSelectedPluginRemove => {
                self.app.begin_selected_plugin_remove_flow();
            }
            Message::RefreshPlugins => {
                self.app.refresh_plugins_from_registry();
            }
            Message::ShowPluginLogs => {
                self.app.show_plugin_logs();
            }
            Message::InterfaceNameChanged { profile_id, value } => {
                self.app.update_interface_profile_name(&profile_id, value);
            }
            Message::TcpClientHostChanged { profile_id, value } => {
                self.app
                    .update_tcp_client_interface_host(&profile_id, value);
            }
            Message::TcpClientPortChanged { profile_id, value } => {
                self.app
                    .update_tcp_client_interface_port(&profile_id, value);
            }
            Message::TcpClientIfacNetworkChanged { profile_id, value } => {
                self.app
                    .update_tcp_client_interface_ifac_network_name(&profile_id, value);
            }
            Message::TcpClientIfacPassphraseChanged { profile_id, value } => {
                self.app
                    .update_tcp_client_interface_ifac_passphrase(&profile_id, value);
            }
            Message::TcpServerHostChanged { profile_id, value } => {
                self.app
                    .update_tcp_server_interface_host(&profile_id, value);
            }
            Message::TcpServerPortChanged { profile_id, value } => {
                self.app
                    .update_tcp_server_interface_port(&profile_id, value);
            }
            Message::TcpServerIfacNetworkChanged { profile_id, value } => {
                self.app
                    .update_tcp_server_interface_ifac_network_name(&profile_id, value);
            }
            Message::TcpServerIfacPassphraseChanged { profile_id, value } => {
                self.app
                    .update_tcp_server_interface_ifac_passphrase(&profile_id, value);
            }
            Message::ToggleI2pConnectable(index) => {
                self.app.select_interface_profile(index);
                self.app.toggle_selected_interface_connectable();
            }
            Message::I2pPeersChanged { profile_id, value } => {
                self.app.update_i2p_interface_peers(&profile_id, value);
            }
            Message::RNodeDevicePortChanged { profile_id, value } => {
                self.app
                    .update_rnode_interface_device_port(&profile_id, value);
            }
            Message::RNodeFrequencyChanged { profile_id, value } => {
                self.app
                    .update_rnode_interface_frequency(&profile_id, value);
            }
            Message::RNodeBandwidthChanged { profile_id, value } => {
                self.app
                    .update_rnode_interface_bandwidth(&profile_id, value);
            }
            Message::RNodeTxPowerChanged { profile_id, value } => {
                self.app.update_rnode_interface_tx_power(&profile_id, value);
            }
            Message::RNodeSpreadingFactorChanged { profile_id, value } => {
                self.app
                    .update_rnode_interface_spreading_factor(&profile_id, value);
            }
            Message::RNodeCodingRateChanged { profile_id, value } => {
                self.app
                    .update_rnode_interface_coding_rate(&profile_id, value);
            }
            Message::SetTheme(theme) => {
                self.app.set_settings_theme_name(theme);
            }
            Message::SetFontSize(size) => {
                self.app.set_settings_font_size(size);
                set_desktop_font_size(size);
            }
            Message::SelectPreferredExternalBrowser(index) => {
                if let Some(choice) = self.external_browsers.get(index).cloned() {
                    self.app
                        .set_preferred_external_browser_command(Some(choice.command));
                    self.external_browsers = detect_external_browsers(
                        self.app
                            .settings
                            .clearweb
                            .preferred_external_browser_command
                            .as_deref(),
                    );
                }
            }
            Message::ClearPreferredExternalBrowser => {
                self.app.set_preferred_external_browser_command(None);
                self.external_browsers = detect_external_browsers(None);
            }
            Message::ToggleClearwebSocksProxy => {
                self.app.toggle_clearweb_socks_proxy();
            }
            Message::ToggleClearwebRemoteMedia => {
                self.app.toggle_clearweb_remote_media();
            }
            Message::KeyboardModifiersChanged(modifiers) => {
                self.ctrl_down = modifiers.control();
            }
            Message::ToggleAutoSyncAfterPropagationAccept => {
                self.app.toggle_auto_sync_after_propagation_accept();
            }
            Message::SelectNativeBackend => {
                self.app
                    .set_runtime_backend_setting(RuntimeBackendSetting::Reticulum);
            }
            Message::StartNativeRuntime => {
                self.app.start_configured_runtime_nonblocking();
            }
            Message::NativeQuickstart => {
                self.app.run_native_quickstart();
            }
            Message::InterfaceStatsSampled(result) => match result {
                Ok(stats) => {
                    self.app.monitoring_state.last_interface_stats = Some(stats.clone());
                    self.app.status.task = if stats.available {
                        format!("interfaces: {}", stats.interfaces.len())
                    } else {
                        stats
                            .reason
                            .unwrap_or_else(|| "interfaces unavailable".into())
                    };
                }
                Err(error) => {
                    self.app.status.task = format!("interface status failed: {error}");
                }
            },
            Message::ToggleNavigation => {
                self.navigation_open = !self.navigation_open;
            }
            Message::BrowserFieldKey(key) => {
                self.apply_browser_field_key(key);
            }
            Message::SubmitBrowserFieldDraft => {
                self.app.submit_active_input();
            }
            Message::CancelBrowserFieldDraft => {
                self.app.cancel_active_input();
            }
            Message::FocusBrowserItem { reverse } => {
                if self.app.workspace.active_section == WorkspaceSection::Browser {
                    self.app.focus_browser_item_with_viewport(
                        self.app.browser_viewport_width(),
                        self.app.browser_viewport_height(),
                        reverse,
                    );
                }
            }
            Message::ActivateFocusedBrowserItem => {
                if self.app.workspace.active_section == WorkspaceSection::Browser {
                    #[cfg(feature = "chat-client")]
                    if let Some(task) = self.activate_focused_omenchat_link() {
                        return task;
                    }
                    if self.prompt_focused_external_link_if_needed() {
                        return Task::none();
                    }
                    if self.activate_focused_lxmf_link() {
                        return Task::none();
                    }
                    self.app.activate_focused_browser_control();
                }
            }
            Message::ScrollBrowserPage { direction } => {
                if self.app.workspace.active_section == WorkspaceSection::Browser {
                    self.app
                        .scroll_active_browser_page(self.app.browser_viewport_height(), direction);
                }
            }
            Message::Page(PageMessage::Activate {
                row,
                col,
                width,
                action,
            }) => {
                #[cfg(feature = "chat-client")]
                if let Some(task) = self.activate_omenchat_hit_action_if_needed(&action) {
                    return task;
                }
                if self.activate_lxmf_hit_action_if_needed(&action) {
                    return Task::none();
                }
                if self.prompt_external_hit_action_if_needed(&action, None) {
                    return Task::none();
                }
                if !self.app.activate_browser_hit_action(action) {
                    self.app.activate_browser_cell(row, col, width);
                }
            }
            Message::PageForTab {
                tab_id,
                page:
                    PageMessage::Activate {
                        row,
                        col,
                        width,
                        action,
                    },
            } => {
                if self.select_browser_tab_by_id(tab_id) {
                    #[cfg(feature = "chat-client")]
                    if let Some(task) = self.activate_omenchat_hit_action_if_needed(&action) {
                        return task;
                    }
                    if self.activate_lxmf_hit_action_if_needed(&action) {
                        return Task::none();
                    }
                    if !self.prompt_external_hit_action_if_needed(&action, Some(tab_id))
                        && !self.app.activate_browser_hit_action(action)
                    {
                        self.app.activate_browser_cell(row, col, width);
                    }
                }
            }
            Message::Page(PageMessage::Scroll {
                delta,
                width,
                height,
            }) => {
                self.app.set_browser_viewport(width, height);
                if self.ctrl_down {
                    let active = self.app.active_browser_tab().id;
                    let direction = if delta <= 0 { 1 } else { -1 };
                    self.app.zoom_browser_tab(active, direction);
                } else {
                    self.app.scroll_active_browser_lines(delta);
                }
            }
            Message::PageForTab {
                tab_id,
                page:
                    PageMessage::Scroll {
                        delta,
                        width,
                        height,
                    },
            } => {
                self.app.set_browser_tab_viewport(tab_id, width, height);
                if self.ctrl_down {
                    let direction = if delta <= 0 { 1 } else { -1 };
                    self.app.zoom_browser_tab(tab_id, direction);
                } else {
                    self.app.scroll_browser_tab_lines(tab_id, delta);
                }
            }
            Message::Tick => {
                let now = current_epoch_ms();
                self.debug_tick_count = self.debug_tick_count.saturating_add(1);
                if self.debug_last_tick_epoch_ms == 0 {
                    self.debug_last_tick_epoch_ms = now;
                }
                let monitoring_sample_due =
                    section_needs_runtime_interface_sample(self.app.workspace.active_section)
                        && now.saturating_sub(self.monitoring_sample_epoch_ms) >= 1_000;
                if monitoring_sample_due
                    && self.app.workspace.active_section == WorkspaceSection::Monitoring
                {
                    self.monitoring_process_usage = process_resource_usage();
                }
                let interface_stats_task = if monitoring_sample_due {
                    self.monitoring_sample_epoch_ms = now;
                    self.sample_runtime_interface_stats()
                } else {
                    Task::none()
                };
                let partials = self.app.refresh_due_browser_partials(now);
                self.app.flush_due_ui_preferences(now);
                self.app.flush_due_directory_persistence();
                let active_conversation_readable = self.active_conversation_pane_is_visible();
                let internal = self
                    .app
                    .drain_internal_events_with_active_conversation_readable(
                        active_conversation_readable,
                    );
                #[cfg(feature = "chat-client-rns")]
                let omenchat_runtime = self.drain_omenchat_runtime_events();
                #[cfg(feature = "chat-client-rns")]
                let omenchat_recent_sync = self.sync_due_omenchat_recent_history(now);
                #[cfg(feature = "chat-client-rns")]
                let omenchat_heartbeat = self.maintain_omenchat_live_links(now);
                #[cfg(feature = "chat-client-rns")]
                let omenchat_reconnect = self.reconnect_restored_omenchat_sessions_if_ready();
                let browser_tasks = self.app.drain_browser_task_results();
                let message_tasks = self.app.drain_message_task_results();
                let direct_timeouts = self.app.reconcile_due_lxmf_direct_timeouts(now);
                let propagation_timeouts = self.app.reconcile_due_lxmf_propagation_timeouts(now);
                let diagnostics = self.app.drain_diagnostics_task_results();
                if now.saturating_sub(self.debug_last_tick_epoch_ms) >= 5_000 {
                    tracing::debug!(
                        target: "desktop_perf",
                        ticks = self.debug_tick_count,
                        partials,
                        internal,
                        browser_tasks,
                        message_tasks,
                        direct_timeouts,
                        propagation_timeouts,
                        diagnostics,
                        "desktop tick drain sample"
                    );
                    self.debug_tick_count = 0;
                    self.debug_last_tick_epoch_ms = now;
                }
                self.remove_workspace_panes_for_missing_targets(None, None);
                if self.restore_workspace_scroll_locks_release_pending {
                    self.restore_workspace_scroll_locks_release_pending = false;
                    self.conversation_scroll_restore_locks.clear();
                    #[cfg(feature = "chat-client")]
                    self.chat_scroll_bottom_locks.clear();
                }
                if self.restore_workspace_scrolls_pending {
                    self.restore_workspace_scrolls_remaining =
                        self.restore_workspace_scrolls_remaining.saturating_sub(1);
                    self.restore_workspace_scrolls_pending =
                        self.restore_workspace_scrolls_remaining > 0;
                    if !self.restore_workspace_scrolls_pending {
                        self.restore_workspace_scroll_locks_release_pending = true;
                    }
                }
                let bottom_anchor_due = if self.pending_workspace_bottom_anchor_ticks > 0 {
                    self.pending_workspace_bottom_anchor_ticks =
                        self.pending_workspace_bottom_anchor_ticks.saturating_sub(1);
                    self.pending_workspace_bottom_anchor_ticks == 0
                } else {
                    false
                };
                let mut tasks = vec![
                    self.snap_conversations_with_new_messages_to_bottom(),
                    #[cfg(feature = "chat-client")]
                    self.snap_omenchat_with_new_events_to_bottom(),
                    #[cfg(feature = "chat-client-rns")]
                    omenchat_runtime,
                    #[cfg(feature = "chat-client-rns")]
                    omenchat_recent_sync,
                    #[cfg(feature = "chat-client-rns")]
                    omenchat_heartbeat,
                    #[cfg(feature = "chat-client-rns")]
                    omenchat_reconnect,
                    interface_stats_task,
                ];
                if bottom_anchor_due {
                    tasks.push(self.restore_visible_workspace_scrolls());
                }
                return Task::batch(tasks);
            }
            Message::WindowCloseRequested(window_id) => {
                self.shutdown_requested = true;
                self.app.flush_pending_ui_preferences();
                let runtime = self.app.runtime.clone();
                return Task::perform(
                    async move {
                        let _ = runtime.stop_runtime().await;
                        window_id
                    },
                    Message::WindowShutdownComplete,
                );
            }
            Message::WindowShutdownComplete(window_id) => {
                let _ = window_id;
                process::exit(0);
            }
            Message::WorkspacePaneClicked(pane) => {
                self.focus_workspace_pane(pane);
            }
            Message::WorkspacePaneDragged(pane_grid::DragEvent::Dropped { pane, target }) => {
                self.workspace_panes.drop(pane, target);
                self.active_workspace_pane = pane;
                self.persist_workspace_panes("workspace panes");
            }
            Message::WorkspacePaneDragged(pane_grid::DragEvent::Picked { pane })
            | Message::WorkspacePaneDragged(pane_grid::DragEvent::Canceled { pane }) => {
                self.active_workspace_pane = pane;
            }
            Message::WorkspacePaneResized(event) => {
                self.workspace_panes.resize(event.split, event.ratio);
                self.schedule_workspace_panes_persist("workspace panes");
                self.schedule_visible_workspace_bottom_anchor(2);
            }
            Message::WorkspacePaneMaximize(pane) => {
                self.workspace_panes.maximize(pane);
                self.focus_workspace_pane(pane);
                self.schedule_workspace_panes_persist("workspace panes");
            }
            Message::WorkspacePaneRestore => {
                self.workspace_panes.restore();
                self.schedule_workspace_panes_persist("workspace panes");
            }
            Message::WorkspacePaneClose(pane) => {
                self.close_workspace_pane(pane);
                self.persist_workspace_panes("workspace panes");
                return self.anchor_visible_workspace_scrolls_to_bottom_now(2);
            }
            Message::RestoreDesktopPane(kind) => {
                let restore_scroll = self.restore_desktop_pane(kind);
                self.persist_workspace_panes("workspace panes");
                return restore_scroll;
            }
            #[cfg(feature = "chat-client")]
            Message::CloseOmenChatSession(session_id) => {
                self.close_omenchat_session(session_id);
                self.remove_workspace_panes_for_missing_targets(None, None);
                self.persist_workspace_panes("workspace panes");
            }
            Message::OpenExternalLinkWith(index) => {
                self.open_pending_external_link(index);
            }
            Message::CopyExternalLinkUrl => {
                if let Some(prompt) = &self.external_link_prompt {
                    self.app.status.task = "copied external URL to clipboard".into();
                    return iced::clipboard::write(prompt.url.clone());
                }
            }
            Message::CopyActiveIdentityHash => {
                if let Some(identity) = &self.app.runtime_status.active_identity {
                    self.app.status.task = "copied active identity hash to clipboard".into();
                    return iced::clipboard::write(identity.hash_hex.clone());
                }
                self.app.status.task = "no active identity hash to copy".into();
            }
            Message::PromptExternalUrl(url) => {
                self.prompt_external_url_if_needed(url, None);
            }
            #[cfg(feature = "chat-client")]
            Message::OpenCachedOmenChatMedia(path) => {
                self.open_local_file(PathBuf::from(path));
            }
            #[cfg(feature = "chat-client")]
            Message::LoadOmenChatMedia(url) => {
                return self.load_omenchat_media_task(url);
            }
            #[cfg(feature = "chat-client")]
            Message::FetchOmenChatUploadResource {
                session_id,
                resource_id,
            } => {
                self.omenchat_media_cache.insert(
                    omenchat_upload_cache_key(session_id, &resource_id),
                    OmenChatMediaLoadState::Loading {
                        message: "requested upload from server".into(),
                        received: None,
                        total: None,
                    },
                );
                let room = self
                    .chat_client
                    .session(session_id)
                    .map(|session| session.active_room.name.clone())
                    .unwrap_or_else(|| "lobby".into());
                let events = self.handle_omenchat_request(ChatClientRequest::RequestUpload {
                    session_id,
                    room,
                    resource_id: resource_id.clone(),
                });
                self.apply_omenchat_client_events_status(&events);
                if let Some(message) = events.iter().find_map(|event| match event {
                    ChatClientEvent::Error {
                        session_id: Some(error_session_id),
                        message,
                    } if *error_session_id == session_id => Some(message.clone()),
                    _ => None,
                }) {
                    self.omenchat_media_cache.insert(
                        omenchat_upload_cache_key(session_id, &resource_id),
                        OmenChatMediaLoadState::Failed { message },
                    );
                }
            }
            #[cfg(feature = "chat-client")]
            Message::PickOmenChatUpload(session_id) => {
                self.set_omenchat_session_status(session_id, "opening file picker".into());
                return Task::perform(
                    async move {
                        let result = tokio::task::spawn_blocking(pick_omenchat_upload_file)
                            .await
                            .map_err(|error| error.to_string())
                            .and_then(|result| result);
                        (session_id, result)
                    },
                    |(session_id, result)| Message::OmenChatUploadPicked { session_id, result },
                );
            }
            #[cfg(feature = "chat-client")]
            Message::OmenChatUploadPicked { session_id, result } => match result {
                Ok(Some(path)) => match self.send_omenchat_upload_path(session_id, &path) {
                    OmenChatDraftCommandResult::HandledClear => {
                        self.chat_drafts.insert(session_id, String::new());
                    }
                    OmenChatDraftCommandResult::HandledKeep
                    | OmenChatDraftCommandResult::NotCommand => {}
                },
                Ok(None) => {
                    self.set_omenchat_session_status(session_id, "upload picker cancelled".into());
                }
                Err(error) => {
                    self.set_omenchat_session_status(
                        session_id,
                        format!("upload picker failed: {error}"),
                    );
                }
            },
            #[cfg(feature = "chat-client")]
            Message::OmenChatGifFramesLoaded { path, result } => match result {
                Ok(bytes) => match iced_gif::Frames::from_bytes(bytes) {
                    Ok(frames) => {
                        self.omenchat_gif_frames.insert(path.clone(), frames);
                        self.app.status.task = format!("OMENchat animated GIF ready: {path}");
                    }
                    Err(error) => {
                        self.app.status.task =
                            format!("OMENchat GIF animation decode failed for {path}: {error}");
                    }
                },
                Err(error) => {
                    self.app.status.task =
                        format!("OMENchat GIF animation load failed for {path}: {error}");
                }
            },
            #[cfg(feature = "chat-client")]
            Message::OmenChatMediaLoaded { url, result } => match result {
                Ok(file) => {
                    let path = file.path.display().to_string();
                    let animated = cached_media_is_animated_gif(&file.path, &file.content_type);
                    self.omenchat_media_cache.insert(
                        url.clone(),
                        OmenChatMediaLoadState::Cached {
                            path: path.clone(),
                            content_type: file.content_type,
                            animated,
                        },
                    );
                    self.app.status.task = if animated {
                        format!("OMENchat animated GIF cached: {path}")
                    } else {
                        format!("OMENchat media cached: {path}")
                    };
                    if animated {
                        return self.load_omenchat_gif_frames_task(path);
                    }
                }
                Err(error) => {
                    self.omenchat_media_cache.insert(
                        url.clone(),
                        OmenChatMediaLoadState::Failed {
                            message: error.clone(),
                        },
                    );
                    self.app.status.task = format!("OMENchat media load failed: {error}");
                }
            },
            Message::DismissExternalLinkPrompt => {
                self.external_link_prompt = None;
                self.app.status.task = "external URL open cancelled".into();
            }
        }
        Task::none()
    }

    fn prompt_external_hit_action_if_needed(
        &mut self,
        action: &HitAction,
        source_tab: Option<TabId>,
    ) -> bool {
        let HitAction::Link(link) = action else {
            return false;
        };
        self.prompt_external_url_if_needed(link.target.clone(), source_tab)
    }

    fn prompt_external_url_if_needed(&mut self, url: String, source_tab: Option<TabId>) -> bool {
        if !BrowserSession::is_clearweb_url(&url) {
            return false;
        }
        self.external_link_prompt = Some(ExternalLinkPrompt { url, source_tab });
        self.app.status.task = if self.app.settings.clearweb.socks_proxy_enabled {
            let endpoint = self
                .clearweb_proxy_endpoint
                .as_ref()
                .map(|(host, port)| format!("{host}:{port}"))
                .unwrap_or_else(|| {
                    format!(
                        "{}:{} or :9150",
                        self.app.settings.clearweb.socks_proxy_host,
                        self.app.settings.clearweb.socks_proxy_port
                    )
                });
            format!(
                "choose external browser; SOCKS5 {} {}",
                endpoint,
                if self.clearweb_proxy_reachable {
                    "detected"
                } else {
                    "not detected"
                }
            )
        } else {
            "choose an external browser for this URL".into()
        };
        true
    }

    fn prompt_focused_external_link_if_needed(&mut self) -> bool {
        let Some((tab_id, target)) = self
            .app
            .active_browser_tab()
            .focused_link
            .as_ref()
            .map(|link| (self.app.active_browser_tab().id, link.target.clone()))
        else {
            return false;
        };
        self.prompt_external_url_if_needed(target, Some(tab_id))
    }

    fn activate_focused_lxmf_link(&mut self) -> bool {
        let Some(link) = self
            .app
            .active_browser_tab()
            .focused_link
            .as_ref()
            .map(|link| crate::micron::LinkAction {
                target: link.target.clone(),
                fields: link.fields.clone(),
            })
        else {
            return false;
        };
        self.activate_lxmf_link(link)
    }

    fn activate_lxmf_hit_action_if_needed(&mut self, action: &HitAction) -> bool {
        let HitAction::Link(link) = action else {
            return false;
        };
        self.activate_lxmf_link(link.clone())
    }

    fn activate_lxmf_link(&mut self, link: crate::micron::LinkAction) -> bool {
        if !self.app.open_lxmf_peer_link(&link.target) {
            return false;
        }
        self.ensure_pane_for_active_conversation();
        self.persist_workspace_panes("workspace panes");
        true
    }

    #[cfg(feature = "chat-client")]
    fn load_omenchat_media_task(&mut self, url: String) -> Task<Message> {
        let detected_socks_proxy = self
            .clearweb_proxy_endpoint
            .as_ref()
            .map(|(host, port)| (host.as_str(), *port));
        let decision = decide_remote_media(RemoteMediaContext {
            url: &url,
            settings: &self.app.settings.clearweb,
            detected_socks_proxy,
        });

        match decision {
            RemoteMediaDecision::AutoInline {
                transport: RemoteMediaTransport::Reticulum,
                ..
            } => {
                self.omenchat_media_cache.insert(
                    url.clone(),
                    omenchat_media_loading_state("loading over Reticulum/NomadNet"),
                );
                self.app.status.task = "loading OMENchat media over Reticulum/NomadNet".into();
                let runtime = self.app.runtime.clone();
                let media_cache_dir = self.app.paths.cache_dir.join("omenchat-media");
                Task::perform(
                    async move {
                        let result = runtime
                            .download_file(
                                &url,
                                &media_cache_dir,
                                crate::runtime::CancellationToken::new(),
                            )
                            .await
                            .map_err(|error| error.to_string());
                        (url, result)
                    },
                    |(url, result)| Message::OmenChatMediaLoaded { url, result },
                )
            }
            RemoteMediaDecision::AutoInline {
                transport: RemoteMediaTransport::Socks5 { host, port },
                ..
            } => {
                self.omenchat_media_cache.insert(
                    url.clone(),
                    omenchat_media_loading_state(&format!("loading over SOCKS5 {host}:{port}")),
                );
                self.app.status.task = format!("loading OMENchat media over SOCKS5 {host}:{port}");
                let media_cache_dir = self.app.paths.cache_dir.join("omenchat-media");
                Task::perform(
                    async move {
                        let result = fetch_clearweb_media_over_socks(
                            &url,
                            &media_cache_dir,
                            &host,
                            port,
                            Duration::from_secs(30),
                        )
                        .await
                        .map_err(|error| error.to_string());
                        (url, result)
                    },
                    |(url, result)| Message::OmenChatMediaLoaded { url, result },
                )
            }
            RemoteMediaDecision::AutoInline {
                transport: RemoteMediaTransport::ExternalBrowser,
                ..
            } => {
                self.prompt_external_url_if_needed(url, None);
                Task::none()
            }
            RemoteMediaDecision::ManualInline { .. }
            | RemoteMediaDecision::ExternalPrompt { .. } => {
                self.prompt_external_url_if_needed(url, None);
                Task::none()
            }
            RemoteMediaDecision::Unsupported { reason } => {
                self.app.status.task = format!("OMENchat media not loaded: {reason}");
                Task::none()
            }
        }
    }

    #[cfg(feature = "chat-client")]
    fn load_omenchat_gif_frames_task(&mut self, path: String) -> Task<Message> {
        Task::perform(
            async move {
                let result = tokio::fs::read(&path)
                    .await
                    .map_err(|error| error.to_string());
                (path, result)
            },
            |(path, result)| Message::OmenChatGifFramesLoaded { path, result },
        )
    }

    fn open_pending_external_link(&mut self, index: usize) {
        let Some(prompt) = self.external_link_prompt.clone() else {
            return;
        };
        let Some(choice) = self.external_browsers.get(index).cloned() else {
            self.app.status.task = "selected external browser is no longer available".into();
            return;
        };
        match open_external_url_with_choice(&choice, &prompt.url) {
            Ok(_) => {
                self.external_link_prompt = None;
                self.app.status.task =
                    format!("opened external URL in {}: {}", choice.label, prompt.url);
            }
            Err(error) => {
                self.app.status.task =
                    format!("failed to open external URL with {}: {error}", choice.label);
            }
        }
    }

    fn open_local_file(&mut self, path: PathBuf) {
        match Command::new("xdg-open").arg(&path).spawn() {
            Ok(_) => {
                self.app.status.task = format!("opened file: {}", path.display());
            }
            Err(error) => {
                self.app.status.task = format!("failed to open file {}: {error}", path.display());
            }
        }
    }

    #[cfg(feature = "chat-client")]
    fn cache_omenchat_upload_resource(
        &mut self,
        session_id: ChatSessionId,
        resource_id: &str,
        filename: &str,
        content_type: Option<&str>,
        bytes: &[u8],
    ) -> anyhow::Result<String> {
        let cache_dir = self.app.paths.cache_dir.join("omenchat-media");
        std::fs::create_dir_all(&cache_dir)?;
        let path = next_available_download_path(&cache_dir, filename)?;
        std::fs::write(&path, bytes)?;
        let path_label = path.display().to_string();
        let content_type = content_type
            .map(ToOwned::to_owned)
            .or_else(|| omenchat_upload_content_type(filename))
            .unwrap_or_else(|| "application/octet-stream".into());
        let animated = cached_media_is_animated_gif(&path, &content_type);
        if animated {
            self.cache_omenchat_uploaded_gif_frames(&path_label, bytes);
        }
        self.omenchat_media_cache.insert(
            omenchat_upload_cache_key(session_id, resource_id),
            OmenChatMediaLoadState::Cached {
                path: path_label.clone(),
                content_type,
                animated,
            },
        );
        Ok(path_label)
    }

    #[cfg(feature = "chat-client")]
    fn cache_omenchat_upload_source_file(
        &mut self,
        session_id: ChatSessionId,
        resource_id: &str,
        filename: &str,
        source_path: &Path,
    ) -> anyhow::Result<String> {
        let cache_dir = self.app.paths.cache_dir.join("omenchat-media");
        std::fs::create_dir_all(&cache_dir)?;
        let path = next_available_download_path(&cache_dir, filename)?;
        std::fs::copy(source_path, &path)?;
        let content_type = omenchat_upload_content_type(filename)
            .unwrap_or_else(|| "application/octet-stream".into());
        let path_label = path.display().to_string();
        let animated = cached_media_is_animated_gif(&path, &content_type);
        if animated {
            match std::fs::read(&path) {
                Ok(bytes) => self.cache_omenchat_uploaded_gif_frames(&path_label, &bytes),
                Err(error) => tracing::warn!(
                    path = path_label,
                    %error,
                    "failed to read cached OMENchat upload GIF for animation decode"
                ),
            }
        }
        self.omenchat_media_cache.insert(
            omenchat_upload_cache_key(session_id, resource_id),
            OmenChatMediaLoadState::Cached {
                path: path_label.clone(),
                animated,
                content_type,
            },
        );
        Ok(path_label)
    }

    #[cfg(feature = "chat-client")]
    fn cache_omenchat_uploaded_gif_frames(&mut self, path_label: &str, bytes: &[u8]) {
        match iced_gif::Frames::from_bytes(bytes.to_vec()) {
            Ok(frames) => {
                self.omenchat_gif_frames
                    .insert(path_label.to_owned(), frames);
            }
            Err(error) => tracing::warn!(
                path = path_label,
                %error,
                "failed to decode cached OMENchat upload as animated GIF"
            ),
        }
    }

    fn apply_browser_field_key(&mut self, key: BrowserFieldKey) {
        match key {
            BrowserFieldKey::Insert(text) => {
                for ch in text.chars() {
                    self.app.edit_address_char(ch);
                }
            }
            BrowserFieldKey::Backspace => self.app.address_backspace(),
            BrowserFieldKey::Delete => self.app.input_delete(),
            BrowserFieldKey::MoveLeft => {
                self.app.input_move_left();
            }
            BrowserFieldKey::MoveRight => {
                self.app.input_move_right();
            }
            BrowserFieldKey::MoveHome => {
                self.app.input_move_home();
            }
            BrowserFieldKey::MoveEnd => {
                self.app.input_move_end();
            }
        }
    }

    fn focus_workspace_pane(&mut self, pane: pane_grid::Pane) {
        self.active_workspace_pane = pane;
        let Some(kind) = self.workspace_panes.get(pane).cloned() else {
            return;
        };
        match kind {
            DesktopPane::Browser(tab_id) => {
                if let Some(index) = self
                    .app
                    .workspace
                    .browser_tabs
                    .iter()
                    .position(|tab| tab.id == tab_id)
                {
                    self.app.select_browser_tab(index);
                }
            }
            DesktopPane::Conversation(conversation_id) => {
                if let Some(index) = self
                    .app
                    .workspace
                    .conversations
                    .iter()
                    .position(|conversation| conversation.id == conversation_id)
                {
                    self.app.select_conversation_tab(index);
                }
            }
            #[cfg(feature = "chat-client")]
            DesktopPane::OmenChat(session_id) => {
                self.ensure_omenchat_bottom_entry(session_id);
            }
        }
    }

    fn restore_active_conversation_scroll(&self) -> Task<Message> {
        let conversation_id = self.app.active_conversation().id;
        self.restore_conversation_scroll(conversation_id)
    }

    fn restore_visible_conversation_scrolls(&self) -> Task<Message> {
        let tasks = self
            .workspace_panes
            .iter()
            .filter_map(|(_, kind)| match kind {
                DesktopPane::Conversation(conversation_id) => {
                    Some(self.restore_conversation_scroll(*conversation_id))
                }
                DesktopPane::Browser(_) => None,
                #[cfg(feature = "chat-client")]
                DesktopPane::OmenChat(_) => None,
            })
            .collect::<Vec<_>>();
        if tasks.is_empty() {
            self.restore_active_conversation_scroll()
        } else {
            Task::batch(tasks)
        }
    }

    fn restore_visible_workspace_scrolls(&self) -> Task<Message> {
        let mut tasks = vec![self.restore_visible_conversation_scrolls()];
        #[cfg(feature = "chat-client")]
        tasks.push(self.restore_visible_omenchat_scrolls());
        Task::batch(tasks)
    }

    fn is_workspace_scroll_restore_settling(&self) -> bool {
        self.restore_workspace_scrolls_pending
            || self.restore_workspace_scroll_locks_release_pending
    }

    fn workspace_scroll_pane_is_visible(&self, pane: DesktopPane) -> bool {
        matches!(
            self.app.workspace.active_section,
            WorkspaceSection::Browser | WorkspaceSection::Messages
        ) && self.find_workspace_pane(&pane).is_some()
    }

    fn schedule_visible_workspace_scroll_restore(&mut self, ticks: u8) {
        self.restore_workspace_scrolls_pending = true;
        self.restore_workspace_scrolls_remaining =
            self.restore_workspace_scrolls_remaining.max(ticks.max(1));
        self.restore_workspace_scroll_locks_release_pending = false;
        self.conversation_scroll_restore_locks
            .extend(
                self.workspace_panes
                    .iter()
                    .filter_map(|(_, kind)| match kind {
                        DesktopPane::Conversation(conversation_id) => Some(*conversation_id),
                        DesktopPane::Browser(_) => None,
                        #[cfg(feature = "chat-client")]
                        DesktopPane::OmenChat(_) => None,
                    }),
            );
        #[cfg(feature = "chat-client")]
        {
            let keys = self
                .workspace_panes
                .iter()
                .filter_map(|(_, kind)| match kind {
                    DesktopPane::OmenChat(session_id) => {
                        Some(self.omenchat_scroll_key(*session_id))
                    }
                    DesktopPane::Browser(_) | DesktopPane::Conversation(_) => None,
                })
                .collect::<Vec<_>>();
            self.chat_scroll_bottom_locks.extend(keys);
        }
    }

    fn remember_visible_workspace_scroll_bottoms(&mut self) {
        self.conversation_scroll_offsets
            .extend(
                self.workspace_panes
                    .iter()
                    .filter_map(|(_, kind)| match kind {
                        DesktopPane::Conversation(conversation_id) => {
                            Some((*conversation_id, RelativeOffset { x: 0.0, y: 1.0 }))
                        }
                        DesktopPane::Browser(_) => None,
                        #[cfg(feature = "chat-client")]
                        DesktopPane::OmenChat(_) => None,
                    }),
            );
        #[cfg(feature = "chat-client")]
        {
            let keys = self
                .workspace_panes
                .iter()
                .filter_map(|(_, kind)| match kind {
                    DesktopPane::OmenChat(session_id) => {
                        Some(self.omenchat_scroll_key(*session_id))
                    }
                    DesktopPane::Browser(_) | DesktopPane::Conversation(_) => None,
                })
                .collect::<Vec<_>>();
            for key in keys {
                self.chat_scroll_offsets
                    .insert(key, RelativeOffset { x: 0.0, y: 1.0 });
            }
        }
    }

    fn schedule_visible_workspace_bottom_anchor(&mut self, ticks: u8) {
        self.schedule_visible_workspace_scroll_restore(ticks);
        self.remember_visible_workspace_scroll_bottoms();
        self.pending_workspace_bottom_anchor_ticks =
            self.pending_workspace_bottom_anchor_ticks.max(ticks.max(1));
    }

    fn anchor_visible_workspace_scrolls_to_bottom_now(&mut self, ticks: u8) -> Task<Message> {
        self.schedule_visible_workspace_scroll_restore(ticks);
        self.remember_visible_workspace_scroll_bottoms();
        self.pending_workspace_bottom_anchor_ticks = 0;
        self.restore_visible_workspace_scrolls()
    }

    fn conversation_is_viewing_history(&self, conversation_id: u64) -> bool {
        self.conversation_scroll_offsets
            .get(&conversation_id)
            .copied()
            .map(scroll_offset_should_show_history_notice)
            .unwrap_or(false)
    }

    #[cfg(feature = "chat-client")]
    fn omenchat_is_viewing_history(&self, session_id: ChatSessionId, room_id: RoomId) -> bool {
        self.chat_scroll_offsets
            .get(&(session_id, room_id))
            .copied()
            .map(scroll_offset_should_show_history_notice)
            .unwrap_or(false)
    }

    #[cfg(feature = "chat-client")]
    fn restore_visible_omenchat_scrolls(&self) -> Task<Message> {
        let tasks = self
            .workspace_panes
            .iter()
            .filter_map(|(_, kind)| match kind {
                DesktopPane::OmenChat(session_id) => {
                    Some(self.restore_omenchat_scroll(*session_id))
                }
                DesktopPane::Browser(_) | DesktopPane::Conversation(_) => None,
            })
            .collect::<Vec<_>>();
        if tasks.is_empty() {
            Task::none()
        } else {
            Task::batch(tasks)
        }
    }

    #[cfg(feature = "chat-client")]
    fn restore_omenchat_scroll(&self, session_id: ChatSessionId) -> Task<Message> {
        let room_id = self.omenchat_active_room_id(session_id);
        let offset = self
            .chat_scroll_offsets
            .get(&(session_id, room_id))
            .copied()
            .unwrap_or(RelativeOffset { x: 0.0, y: 1.0 });
        iced::widget::scrollable::snap_to(omenchat_scroll_id(session_id, room_id), offset)
    }

    #[cfg(feature = "chat-client")]
    fn omenchat_active_room_id(&self, session_id: ChatSessionId) -> RoomId {
        self.chat_client
            .session(session_id)
            .map(|session| session.active_room.room_id)
            .unwrap_or(1)
    }

    #[cfg(feature = "chat-client")]
    fn omenchat_scroll_key(&self, session_id: ChatSessionId) -> (ChatSessionId, RoomId) {
        (session_id, self.omenchat_active_room_id(session_id))
    }

    #[cfg(feature = "chat-client")]
    fn ensure_omenchat_bottom_entry(&mut self, session_id: ChatSessionId) {
        let key = self.omenchat_scroll_key(session_id);
        self.chat_scroll_offsets
            .entry(key)
            .or_insert(RelativeOffset { x: 0.0, y: 1.0 });
    }

    #[cfg(feature = "chat-client")]
    fn remember_omenchat_bottom(&mut self, session_id: ChatSessionId) {
        let key = self.omenchat_scroll_key(session_id);
        self.chat_scroll_offsets
            .insert(key, RelativeOffset { x: 0.0, y: 1.0 });
    }

    #[cfg(feature = "chat-client")]
    fn lock_omenchat_bottom_until_restore_settles(&mut self, session_id: ChatSessionId) {
        let key = self.omenchat_scroll_key(session_id);
        self.chat_scroll_bottom_locks.insert(key);
        self.chat_scroll_offsets
            .insert(key, RelativeOffset { x: 0.0, y: 1.0 });
    }

    #[cfg(feature = "chat-client")]
    fn lock_omenchat_current_scroll_until_restore_settles(&mut self, session_id: ChatSessionId) {
        let key = self.omenchat_scroll_key(session_id);
        self.chat_scroll_bottom_locks.insert(key);
        self.chat_scroll_offsets
            .entry(key)
            .or_insert(RelativeOffset { x: 0.0, y: 1.0 });
    }

    #[cfg(feature = "chat-client")]
    fn remember_omenchat_bottom_if_missing(&mut self, session_id: ChatSessionId) {
        let key = self.omenchat_scroll_key(session_id);
        self.chat_scroll_offsets
            .entry(key)
            .or_insert(RelativeOffset { x: 0.0, y: 1.0 });
    }

    fn remember_conversation_bottom(&mut self, conversation_id: u64) {
        self.conversation_scroll_offsets
            .insert(conversation_id, RelativeOffset { x: 0.0, y: 1.0 });
    }

    fn restore_conversation_scroll(&self, conversation_id: u64) -> Task<Message> {
        let offset = self
            .conversation_scroll_offsets
            .get(&conversation_id)
            .copied()
            .unwrap_or(RelativeOffset { x: 0.0, y: 1.0 });
        iced::widget::scrollable::snap_to(conversation_scroll_id(conversation_id), offset)
    }

    fn select_browser_tab_by_id(&mut self, tab_id: TabId) -> bool {
        let Some(index) = self
            .app
            .workspace
            .browser_tabs
            .iter()
            .position(|tab| tab.id == tab_id)
        else {
            return false;
        };
        self.app.select_browser_tab(index);
        if let Some(pane) = self.find_workspace_pane(&DesktopPane::Browser(tab_id)) {
            self.active_workspace_pane = pane;
        }
        true
    }

    fn select_conversation_by_id(&mut self, conversation_id: u64) -> bool {
        let Some(index) = self
            .app
            .workspace
            .conversations
            .iter()
            .position(|conversation| conversation.id == conversation_id)
        else {
            return false;
        };
        self.app.select_conversation_tab(index);
        if let Some(pane) = self.find_workspace_pane(&DesktopPane::Conversation(conversation_id)) {
            self.active_workspace_pane = pane;
        }
        self.ensure_conversation_body_editor(conversation_id);
        true
    }

    fn snap_conversations_with_new_messages_to_bottom(&mut self) -> Task<Message> {
        let current_counts = self
            .app
            .workspace
            .conversations
            .iter()
            .map(|conversation| (conversation.id, conversation.thread.messages.len()))
            .collect::<HashMap<_, _>>();
        let tasks = self
            .workspace_panes
            .iter()
            .filter_map(|(_, pane)| match pane {
                DesktopPane::Conversation(conversation_id) => {
                    let previous = self
                        .conversation_message_counts
                        .get(conversation_id)
                        .copied()
                        .unwrap_or(0);
                    let current = current_counts.get(conversation_id).copied().unwrap_or(0);
                    if current <= previous {
                        return None;
                    }
                    let was_following_bottom = self
                        .conversation_scroll_offsets
                        .get(conversation_id)
                        .copied()
                        .map(scroll_offset_is_at_bottom)
                        .unwrap_or(true);
                    if !was_following_bottom {
                        return None;
                    }
                    self.conversation_scroll_offsets
                        .insert(*conversation_id, RelativeOffset { x: 0.0, y: 1.0 });
                    Some(iced::widget::scrollable::snap_to(
                        conversation_scroll_id(*conversation_id),
                        RelativeOffset { x: 0.0, y: 1.0 },
                    ))
                }
                DesktopPane::Browser(_) => None,
                #[cfg(feature = "chat-client")]
                DesktopPane::OmenChat(_) => None,
            })
            .collect::<Vec<_>>();
        self.conversation_message_counts = current_counts;
        if tasks.is_empty() {
            Task::none()
        } else {
            Task::batch(tasks)
        }
    }

    #[cfg(feature = "chat-client")]
    fn snap_omenchat_with_new_events_to_bottom(&mut self) -> Task<Message> {
        let current_counts = omenchat_event_counts_by_room(self.chat_client.sessions());
        let visible_sessions = self
            .workspace_panes
            .iter()
            .filter_map(|(_, pane)| match pane {
                DesktopPane::OmenChat(session_id) => Some(*session_id),
                DesktopPane::Browser(_) | DesktopPane::Conversation(_) => None,
            })
            .collect::<Vec<_>>();
        let tasks = visible_sessions
            .into_iter()
            .filter_map(|session_id| {
                let key = self.omenchat_scroll_key(session_id);
                let previous = self.chat_event_counts.get(&key).copied().unwrap_or(0);
                let current = current_counts.get(&key).copied().unwrap_or(0);
                if current <= previous {
                    return None;
                }
                let was_following_bottom = self
                    .chat_scroll_offsets
                    .get(&key)
                    .copied()
                    .map(scroll_offset_is_at_bottom)
                    .unwrap_or(true);
                if !was_following_bottom {
                    return None;
                }
                self.chat_scroll_offsets
                    .insert(key, RelativeOffset { x: 0.0, y: 1.0 });
                Some(iced::widget::scrollable::snap_to(
                    omenchat_scroll_id(session_id, key.1),
                    RelativeOffset { x: 0.0, y: 1.0 },
                ))
            })
            .collect::<Vec<_>>();
        self.chat_event_counts = current_counts;
        if tasks.is_empty() {
            Task::none()
        } else {
            Task::batch(tasks)
        }
    }

    fn ensure_conversation_body_editor(&mut self, conversation_id: u64) {
        if self
            .conversation_body_editors
            .contains_key(&conversation_id)
        {
            return;
        }
        let body = self
            .app
            .workspace
            .conversations
            .iter()
            .find(|conversation| conversation.id == conversation_id)
            .map(|conversation| conversation.draft_body.as_str())
            .unwrap_or_default();
        self.conversation_body_editors
            .insert(conversation_id, text_editor::Content::with_text(body));
    }

    fn conversation_body_editor_mut(&mut self, conversation_id: u64) -> &mut text_editor::Content {
        self.ensure_conversation_body_editor(conversation_id);
        self.conversation_body_editors
            .get_mut(&conversation_id)
            .expect("conversation body editor was just ensured")
    }

    fn sync_conversation_body_editor(&mut self, conversation_id: u64) {
        let Some(body) = self
            .app
            .workspace
            .conversations
            .iter()
            .find(|conversation| conversation.id == conversation_id)
            .map(|conversation| conversation.draft_body.clone())
        else {
            self.conversation_body_editors.remove(&conversation_id);
            return;
        };
        let needs_replace = self
            .conversation_body_editors
            .get(&conversation_id)
            .is_none_or(|editor| conversation_editor_text(editor) != body);
        if needs_replace {
            self.conversation_body_editors
                .insert(conversation_id, text_editor::Content::with_text(&body));
        }
    }

    fn clear_conversation_body_editor(&mut self, conversation_id: u64) {
        self.conversation_body_editors
            .insert(conversation_id, text_editor::Content::new());
    }

    fn ensure_pane_for_active_browser(&mut self) {
        let tab_id = self.app.active_browser_tab().id;
        if let Some(pane) = self.find_workspace_pane(&DesktopPane::Browser(tab_id)) {
            self.active_workspace_pane = pane;
            return;
        }
        self.split_workspace_from_active(DesktopPane::Browser(tab_id));
    }

    fn ensure_pane_for_active_conversation(&mut self) {
        let conversation_id = self.app.active_conversation().id;
        if let Some(pane) = self.find_workspace_pane(&DesktopPane::Conversation(conversation_id)) {
            self.active_workspace_pane = pane;
            return;
        }
        self.split_workspace_from_active(DesktopPane::Conversation(conversation_id));
    }

    #[cfg(feature = "chat-client")]
    fn ensure_pane_for_omenchat(&mut self, session_id: ChatSessionId) {
        if let Some(pane) = self.find_workspace_pane(&DesktopPane::OmenChat(session_id)) {
            self.active_workspace_pane = pane;
            return;
        }
        self.split_workspace_from_active(DesktopPane::OmenChat(session_id));
    }

    #[cfg(feature = "chat-client")]
    fn place_omenchat_session_preferring_active_blank(&mut self, session_id: ChatSessionId) {
        let active_blank = self
            .workspace_panes
            .get(self.active_workspace_pane)
            .and_then(|kind| match kind {
                DesktopPane::OmenChat(blank_id)
                    if *blank_id != session_id
                        && self.chat_client.session(*blank_id).is_some_and(|session| {
                            is_pending_omenchat_destination(&session.server.destination)
                        }) =>
                {
                    Some(*blank_id)
                }
                _ => None,
            });

        let Some(blank_id) = active_blank else {
            self.ensure_pane_for_omenchat(session_id);
            return;
        };

        let blank_pane = self.active_workspace_pane;
        if let Some(existing_pane) = self.find_workspace_pane(&DesktopPane::OmenChat(session_id)) {
            if self.workspace_panes.len() > 1 {
                self.close_workspace_pane(blank_pane);
            } else if let Some(kind) = self.workspace_panes.get_mut(blank_pane) {
                *kind = DesktopPane::OmenChat(session_id);
            }
            self.active_workspace_pane = existing_pane;
        } else if let Some(kind) = self.workspace_panes.get_mut(blank_pane) {
            *kind = DesktopPane::OmenChat(session_id);
            self.active_workspace_pane = blank_pane;
        } else {
            self.ensure_pane_for_omenchat(session_id);
        }
        self.remove_blank_omenchat_session_state(blank_id);
    }

    #[cfg(feature = "chat-client")]
    fn remove_blank_omenchat_session_state(&mut self, session_id: ChatSessionId) {
        let Some(session) = self.chat_client.session(session_id) else {
            return;
        };
        if !is_pending_omenchat_destination(&session.server.destination) {
            return;
        }
        self.chat_drafts.remove(&session_id);
        self.omenchat_motds.remove(&session_id);
        self.omenchat_upload_quotas.remove(&session_id);
        self.omenchat_upload_max_file_bytes.remove(&session_id);
        self.chat_scroll_offsets
            .retain(|(stored_session_id, _), _| *stored_session_id != session_id);
        self.chat_event_counts
            .retain(|(stored_session_id, _), _| *stored_session_id != session_id);
        self.chat_client.remove_session(session_id);
    }

    fn restore_desktop_pane(&mut self, kind: DesktopPane) -> Task<Message> {
        match kind {
            DesktopPane::Browser(tab_id) => {
                if self.select_browser_tab_by_id(tab_id) {
                    self.ensure_pane_for_active_browser();
                }
                Task::none()
            }
            DesktopPane::Conversation(conversation_id) => {
                if self.select_conversation_by_id(conversation_id) {
                    self.ensure_pane_for_active_conversation();
                    self.remember_conversation_bottom(conversation_id);
                    self.schedule_visible_workspace_scroll_restore(3);
                    return self.restore_conversation_scroll(conversation_id);
                }
                Task::none()
            }
            #[cfg(feature = "chat-client")]
            DesktopPane::OmenChat(session_id) => {
                if self.chat_client.session(session_id).is_some() {
                    self.chat_drafts.entry(session_id).or_default();
                    self.clear_omenchat_active_room_unread(session_id);
                    self.ensure_pane_for_omenchat(session_id);
                    self.lock_omenchat_bottom_until_restore_settles(session_id);
                    self.schedule_visible_workspace_scroll_restore(3);
                    return self.restore_omenchat_scroll(session_id);
                }
                Task::none()
            }
        }
    }

    #[cfg(feature = "chat-client")]
    fn close_omenchat_session(&mut self, session_id: ChatSessionId) {
        let server_id = self
            .chat_client
            .session(session_id)
            .map(|session| session.server.server_id.clone());
        self.chat_drafts.remove(&session_id);
        self.omenchat_motds.remove(&session_id);
        self.chat_scroll_offsets
            .retain(|(stored_session_id, _), _| *stored_session_id != session_id);
        self.chat_event_counts
            .retain(|(stored_session_id, _), _| *stored_session_id != session_id);
        #[cfg(feature = "chat-client-rns")]
        {
            self.omenchat_live_opening.remove(&session_id);
            self.omenchat_live_retry_after.remove(&session_id);
            self.omenchat_live_retry_count.remove(&session_id);
            self.omenchat_live_reconnect_generation.remove(&session_id);
            if let Some(transport) = self.omenchat_live_transports.remove(&session_id) {
                let runtime = self.app.runtime.clone();
                let link_id = transport.link_id;
                tokio::spawn(async move {
                    let _ = runtime.close_omenchat_link(link_id).await;
                });
            }
            self.omenchat_link_sessions
                .retain(|_, stored_session_id| *stored_session_id != session_id);
        }
        self.chat_client.remove_session(session_id);
        if let (Some(store), Some(server_id)) = (self.chat_store.as_mut(), server_id.as_ref()) {
            if let Err(error) = store.delete_server(server_id) {
                tracing::warn!("failed to delete OMENchat server {server_id} from store: {error}");
                self.app.status.task =
                    format!("closed OMENchat session; cache delete failed: {error}");
                return;
            }
        }
        self.app.status.task = "closed OMENchat session".into();
    }

    #[cfg(feature = "chat-client")]
    fn send_omenchat_draft(&mut self, session_id: ChatSessionId) {
        let draft = self
            .chat_drafts
            .get(&session_id)
            .map(|draft| draft.trim().to_owned())
            .unwrap_or_default();
        if draft.is_empty() {
            return;
        }
        match self.handle_omenchat_draft_command(session_id, &draft) {
            OmenChatDraftCommandResult::NotCommand => {}
            OmenChatDraftCommandResult::HandledClear => {
                self.chat_drafts.insert(session_id, String::new());
                return;
            }
            OmenChatDraftCommandResult::HandledKeep => return,
        }
        let events = self.handle_omenchat_request(ChatClientRequest::SendMessage {
            session_id,
            room: self
                .chat_client
                .session(session_id)
                .map(|session| session.active_room.name.clone())
                .unwrap_or_else(|| "lobby".into()),
            body: draft,
        });
        let failed = events
            .iter()
            .any(|event| matches!(event, ChatClientEvent::Error { .. }));
        if !failed {
            self.chat_drafts.insert(session_id, String::new());
        }
        if events.iter().any(|event| {
            matches!(event, ChatClientEvent::EventAppended { event, .. }
                if !is_omenchat_local_echo_event(event))
        }) {
            self.persist_omenchat_session(session_id);
        }
    }

    #[cfg(feature = "chat-client")]
    fn resend_omenchat_local_echo(
        &mut self,
        session_id: ChatSessionId,
        room_id: RoomId,
        event_id: u64,
        body: String,
        action: bool,
    ) {
        let room = self
            .chat_client
            .session(session_id)
            .and_then(|session| {
                session
                    .rooms
                    .iter()
                    .find(|room| room.room_id == room_id)
                    .map(|room| room.name.clone())
                    .or_else(|| Some(session.active_room.name.clone()))
            })
            .unwrap_or_else(|| "lobby".into());
        let request = if action {
            ChatClientRequest::SendAction {
                session_id,
                room,
                body,
            }
        } else {
            ChatClientRequest::SendMessage {
                session_id,
                room,
                body,
            }
        };
        let events = self.handle_omenchat_request(request);
        let replacement_queued = events.iter().any(|event| {
            matches!(event, ChatClientEvent::EventAppended { event, .. }
                if is_omenchat_local_echo_event(event))
        });
        if replacement_queued {
            if let Some(session) = self.chat_client.session_mut(session_id) {
                session
                    .events
                    .retain(|event| !(event.room_id == room_id && event.event_id == event_id));
            }
        } else if events
            .iter()
            .any(|event| matches!(event, ChatClientEvent::Error { .. }))
        {
            self.set_omenchat_session_status(
                session_id,
                "resend did not leave the client; original local echo kept".into(),
            );
        }
        if events.iter().any(|event| {
            matches!(event, ChatClientEvent::EventAppended { event, .. }
                if !is_omenchat_local_echo_event(event))
        }) || replacement_queued
        {
            self.lock_omenchat_bottom_until_restore_settles(session_id);
            self.schedule_visible_workspace_scroll_restore(2);
        }
    }

    #[cfg(feature = "chat-client")]
    fn send_omenchat_upload_path(
        &mut self,
        session_id: ChatSessionId,
        path: &Path,
    ) -> OmenChatDraftCommandResult {
        if path.as_os_str().is_empty() {
            self.set_omenchat_session_status(session_id, "usage: /upload <path>".into());
            return OmenChatDraftCommandResult::HandledKeep;
        }
        let filename = path
            .file_name()
            .and_then(|value| value.to_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| "upload.bin".into());
        let upload_byte_len = match std::fs::metadata(path) {
            Ok(metadata) if metadata.is_file() => metadata.len(),
            Ok(_) => {
                self.set_omenchat_session_status(session_id, "upload path is not a file".into());
                return OmenChatDraftCommandResult::HandledKeep;
            }
            Err(error) => {
                self.set_omenchat_session_status(
                    session_id,
                    format!("upload metadata failed: {error}"),
                );
                return OmenChatDraftCommandResult::HandledKeep;
            }
        };
        if upload_byte_len == 0 {
            self.set_omenchat_session_status(session_id, "upload file is empty".into());
            return OmenChatDraftCommandResult::HandledKeep;
        }
        if let Some(reason) = omenchat_upload_policy_rejection(
            upload_byte_len,
            self.omenchat_session_upload_quota(session_id),
            self.omenchat_session_upload_max_file_bytes(session_id),
        ) {
            self.set_omenchat_session_status(session_id, reason);
            return OmenChatDraftCommandResult::HandledKeep;
        }
        let bytes = match std::fs::read(path) {
            Ok(bytes) if !bytes.is_empty() => bytes,
            Ok(_) => {
                self.set_omenchat_session_status(session_id, "upload file is empty".into());
                return OmenChatDraftCommandResult::HandledKeep;
            }
            Err(error) => {
                self.set_omenchat_session_status(
                    session_id,
                    format!("upload read failed: {error}"),
                );
                return OmenChatDraftCommandResult::HandledKeep;
            }
        };
        let upload_byte_len = bytes.len() as u64;
        if let Some(reason) = omenchat_upload_policy_rejection(
            upload_byte_len,
            self.omenchat_session_upload_quota(session_id),
            self.omenchat_session_upload_max_file_bytes(session_id),
        ) {
            self.set_omenchat_session_status(session_id, reason);
            return OmenChatDraftCommandResult::HandledKeep;
        }
        let content_type = omenchat_upload_content_type(&filename);
        let pending_key = (session_id, filename.clone(), upload_byte_len);
        self.omenchat_pending_upload_sources
            .insert(pending_key.clone(), path.to_path_buf());
        let events = self.handle_omenchat_request(ChatClientRequest::SendUpload {
            session_id,
            room: self
                .chat_client
                .session(session_id)
                .map(|session| session.active_room.name.clone())
                .unwrap_or_else(|| "lobby".into()),
            filename: filename.clone(),
            content_type,
            bytes,
        });
        for event in &events {
            if let ChatClientEvent::UploadAccepted {
                session_id: accepted_session_id,
                resource_id,
                filename: accepted_filename,
                bytes: accepted_bytes,
            } = event
            {
                if *accepted_session_id == session_id
                    && accepted_filename == &filename
                    && *accepted_bytes == upload_byte_len
                {
                    let source_path = self
                        .omenchat_pending_upload_sources
                        .remove(&pending_key)
                        .unwrap_or_else(|| path.to_path_buf());
                    match self.cache_omenchat_upload_source_file(
                        session_id,
                        resource_id,
                        &filename,
                        &source_path,
                    ) {
                        Ok(path) => {
                            self.set_omenchat_session_status(
                                session_id,
                                format!("upload accepted and cached locally: {path}"),
                            );
                        }
                        Err(error) => {
                            self.set_omenchat_session_status(
                                session_id,
                                format!("upload accepted; local cache failed: {error}"),
                            );
                        }
                    }
                }
            }
        }
        if events.iter().any(|event| {
            matches!(
                event,
                ChatClientEvent::UploadRejected {
                    session_id: rejected_session_id,
                    ..
                } if *rejected_session_id == session_id
            )
        }) {
            self.omenchat_pending_upload_sources.remove(&pending_key);
        }
        self.apply_omenchat_client_events_status(&events);
        omenchat_command_result_from_events(&events)
    }

    #[cfg(feature = "chat-client")]
    fn handle_omenchat_draft_command(
        &mut self,
        session_id: ChatSessionId,
        draft: &str,
    ) -> OmenChatDraftCommandResult {
        let Some(command) = parse_client_command(draft) else {
            return OmenChatDraftCommandResult::NotCommand;
        };
        match command {
            ClientCommand::Me(body) => {
                if body.trim().is_empty() {
                    self.set_omenchat_session_status(session_id, "usage: /me <action>".into());
                    return OmenChatDraftCommandResult::HandledKeep;
                }
                let events = self.handle_omenchat_request(ChatClientRequest::SendAction {
                    session_id,
                    room: self
                        .chat_client
                        .session(session_id)
                        .map(|session| session.active_room.name.clone())
                        .unwrap_or_else(|| "lobby".into()),
                    body,
                });
                if events
                    .iter()
                    .any(|event| matches!(event, ChatClientEvent::EventAppended { .. }))
                {
                    self.persist_omenchat_session(session_id);
                }
                self.apply_omenchat_client_events_status(&events);
                omenchat_command_result_from_events(&events)
            }
            ClientCommand::Join(room) => {
                let room = room.trim().trim_start_matches('#').to_owned();
                if room.is_empty() {
                    self.set_omenchat_session_status(session_id, "usage: /join <room>".into());
                    OmenChatDraftCommandResult::HandledKeep
                } else {
                    self.join_omenchat_room(session_id, room);
                    OmenChatDraftCommandResult::HandledClear
                }
            }
            ClientCommand::Rooms => {
                let events =
                    self.handle_omenchat_request(ChatClientRequest::RefreshRooms { session_id });
                if events
                    .iter()
                    .any(|event| matches!(event, ChatClientEvent::RoomsUpdated { .. }))
                {
                    self.persist_omenchat_session(session_id);
                }
                let rooms = self
                    .chat_client
                    .session(session_id)
                    .map(|session| {
                        session
                            .rooms
                            .iter()
                            .map(|room| format!("#{}", room.name))
                            .collect::<Vec<_>>()
                            .join(", ")
                    })
                    .filter(|rooms| !rooms.is_empty())
                    .unwrap_or_else(|| "no rooms advertised yet".into());
                self.set_omenchat_session_status(session_id, format!("rooms: {rooms}"));
                omenchat_command_result_from_events(&events)
            }
            ClientCommand::Who => {
                let users = self
                    .chat_client
                    .session(session_id)
                    .map(|session| {
                        unique_chat_users(&session.users)
                            .into_iter()
                            .map(|user| user.display_label())
                            .collect::<Vec<_>>()
                            .join(", ")
                    })
                    .filter(|users| !users.is_empty())
                    .unwrap_or_else(|| "no users visible yet".into());
                self.set_omenchat_session_status(session_id, format!("users: {users}"));
                OmenChatDraftCommandResult::HandledClear
            }
            ClientCommand::Help => {
                self.app.switch_section(WorkspaceSection::Help);
                self.set_omenchat_session_status(
                    session_id,
                    "opened Help; see OMENchat commands".into(),
                );
                OmenChatDraftCommandResult::HandledClear
            }
            ClientCommand::Notice(body) => {
                if body.trim().is_empty() {
                    self.set_omenchat_session_status(session_id, "usage: /notice <text>".into());
                    return OmenChatDraftCommandResult::HandledKeep;
                }
                let events = self.handle_omenchat_request(ChatClientRequest::SendNotice {
                    session_id,
                    room: self
                        .chat_client
                        .session(session_id)
                        .map(|session| session.active_room.name.clone())
                        .unwrap_or_else(|| "lobby".into()),
                    body,
                });
                if events
                    .iter()
                    .any(|event| matches!(event, ChatClientEvent::EventAppended { .. }))
                {
                    self.persist_omenchat_session(session_id);
                }
                self.apply_omenchat_client_events_status(&events);
                omenchat_command_result_from_events(&events)
            }
            ClientCommand::Upload(path) => {
                self.send_omenchat_upload_path(session_id, Path::new(path.trim()))
            }
            ClientCommand::Topic(topic) => {
                let events =
                    self.handle_omenchat_request(ChatClientRequest::SetTopic { session_id, topic });
                let updated = events
                    .iter()
                    .any(|event| matches!(event, ChatClientEvent::RoomsUpdated { .. }));
                self.apply_omenchat_client_events_status(&events);
                if updated {
                    self.persist_omenchat_session(session_id);
                }
                omenchat_command_result_from_events(&events)
            }
            ClientCommand::CreateRoom { room, topic } => {
                let room = room.trim().trim_start_matches('#').to_owned();
                if room.is_empty() {
                    self.set_omenchat_session_status(
                        session_id,
                        "usage: /create <room> [topic]".into(),
                    );
                    return OmenChatDraftCommandResult::HandledKeep;
                }
                let events = self.handle_omenchat_request(ChatClientRequest::CreateRoom {
                    session_id,
                    room,
                    topic,
                });
                let updated = events
                    .iter()
                    .any(|event| matches!(event, ChatClientEvent::RoomsUpdated { .. }));
                self.apply_omenchat_client_events_status(&events);
                if updated {
                    self.persist_omenchat_session(session_id);
                }
                omenchat_command_result_from_events(&events)
            }
            ClientCommand::Part(room) => {
                let room = room.map(|room| room.trim().trim_start_matches('#').to_owned());
                let events =
                    self.handle_omenchat_request(ChatClientRequest::PartRoom { session_id, room });
                let updated = events
                    .iter()
                    .any(|event| matches!(event, ChatClientEvent::RoomsUpdated { .. }));
                self.apply_omenchat_client_events_status(&events);
                if updated {
                    self.persist_omenchat_session(session_id);
                }
                omenchat_command_result_from_events(&events)
            }
            ClientCommand::Kick(target) => {
                self.send_omenchat_moderation_command(session_id, "kick", target)
            }
            ClientCommand::Ban(target) => {
                self.send_omenchat_moderation_command(session_id, "ban", target)
            }
            ClientCommand::Unban(target) => {
                self.send_omenchat_moderation_command(session_id, "unban", target)
            }
            ClientCommand::Mute(target) => {
                self.send_omenchat_moderation_command(session_id, "mute", target)
            }
            ClientCommand::Unmute(target) => {
                self.send_omenchat_moderation_command(session_id, "unmute", target)
            }
            ClientCommand::Role { target, role } => {
                let target = target.trim();
                let role = role.trim();
                if target.is_empty() || role.is_empty() {
                    self.set_omenchat_session_status(
                        session_id,
                        "usage: /role <user> <standard|trusted|mod|admin>".into(),
                    );
                    OmenChatDraftCommandResult::HandledKeep
                } else {
                    self.send_omenchat_moderation_command(
                        session_id,
                        "role",
                        format!("{target} {role}"),
                    )
                }
            }
            ClientCommand::Unknown(name) => {
                self.set_omenchat_session_status(
                    session_id,
                    format!("unknown command: /{name}; try /help"),
                );
                OmenChatDraftCommandResult::HandledKeep
            }
            ClientCommand::DirectMessage(_) => {
                self.set_omenchat_session_status(
                    session_id,
                    "that OMENchat command is not implemented yet".into(),
                );
                OmenChatDraftCommandResult::HandledKeep
            }
        }
    }

    #[cfg(feature = "chat-client")]
    fn join_omenchat_room(&mut self, session_id: ChatSessionId, room: String) {
        let current = self
            .chat_client
            .session(session_id)
            .map(|session| session.active_room.name == room)
            .unwrap_or(false);
        if current {
            return;
        }
        let events = self.handle_omenchat_request(ChatClientRequest::JoinRoom { session_id, room });
        if events
            .iter()
            .any(|event| matches!(event, ChatClientEvent::RoomJoined { .. }))
        {
            self.restore_cached_omenchat_room_history(session_id);
        }
    }

    #[cfg(feature = "chat-client")]
    fn send_omenchat_moderation_command(
        &mut self,
        session_id: ChatSessionId,
        action: &str,
        target: String,
    ) -> OmenChatDraftCommandResult {
        let target = target.trim().to_owned();
        if target.is_empty() {
            self.set_omenchat_session_status(session_id, format!("usage: /{action} <active user>"));
            return OmenChatDraftCommandResult::HandledKeep;
        }
        let events = self.handle_omenchat_request(ChatClientRequest::ModerateUser {
            session_id,
            action: action.to_owned(),
            target,
        });
        self.apply_omenchat_client_events_status(&events);
        omenchat_command_result_from_events(&events)
    }

    #[cfg(feature = "chat-client")]
    fn load_older_omenchat_history(&mut self, session_id: ChatSessionId) {
        let cached_loaded = if let Some(store) = self.chat_store.as_ref() {
            match self
                .chat_client
                .load_cached_history_before(store, session_id, 50)
            {
                Ok(count) => count,
                Err(error) => {
                    self.set_omenchat_session_status(
                        session_id,
                        format!("cached history load failed: {error}"),
                    );
                    0
                }
            }
        } else {
            0
        };
        if cached_loaded > 0 {
            self.persist_omenchat_session(session_id);
            return;
        }
        #[cfg(feature = "chat-client-rns")]
        if self.omenchat_live_history_request_requires_reconnect(session_id) {
            if cached_loaded == 0 {
                self.set_omenchat_session_status(
                    session_id,
                    "no older cached history; reconnect to request server history".into(),
                );
            }
            return;
        }
        let events = self.handle_omenchat_request(ChatClientRequest::LoadOlder { session_id });
        if events
            .iter()
            .any(|event| matches!(event, ChatClientEvent::HistoryPrepended { .. }))
        {
            self.persist_omenchat_session(session_id);
        }
    }

    #[cfg(all(feature = "chat-client", feature = "chat-client-rns"))]
    fn omenchat_live_history_request_requires_reconnect(&self, session_id: ChatSessionId) -> bool {
        !self.omenchat_live_transports.contains_key(&session_id)
            && self.chat_client.session(session_id).is_some_and(|session| {
                session.server.destination != "mockchatdestination"
                    && session.server.destination.len() >= 32
            })
    }

    #[cfg(feature = "chat-client-rns")]
    fn request_omenchat_path_task(&mut self, session_id: ChatSessionId) -> Task<Message> {
        let Some(destination) = self
            .chat_client
            .session(session_id)
            .map(|session| session.server.destination.clone())
        else {
            self.app.status.task = "cannot request OMENchat path: session is closed".into();
            return Task::none();
        };
        if destination == "mockchatdestination" || destination.len() < 32 {
            self.set_omenchat_session_status(
                session_id,
                "cannot request path for a mock/invalid OMENchat destination".into(),
            );
            return Task::none();
        }
        if !self.app.runtime_status.connected {
            self.set_omenchat_session_status(
                session_id,
                "Reticulum runtime is not connected; request path after startup".into(),
            );
            return Task::none();
        }
        self.set_omenchat_session_status(
            session_id,
            format!("requesting path for OMENchat server {destination}"),
        );
        let runtime = self.app.runtime.clone();
        let request_destination = destination.clone();
        Task::perform(
            async move {
                let result = runtime
                    .request_path(&request_destination, "OMENchat server path request", true)
                    .await
                    .map_err(|error| error.to_string());
                (session_id, destination, result)
            },
            |(session_id, destination, result)| Message::OmenChatPathRequestResult {
                session_id,
                destination,
                result,
            },
        )
    }

    #[cfg(feature = "chat-client-rns")]
    fn reconnect_omenchat_session_task(&mut self, session_id: ChatSessionId) -> Task<Message> {
        if !self.app.runtime_status.connected {
            self.set_omenchat_session_status(
                session_id,
                "Reticulum runtime is not connected; reconnect after startup".into(),
            );
            return Task::none();
        }
        let Some(descriptor) = self.omenchat_descriptor_for_session(session_id) else {
            self.app.status.task = "cannot reconnect OMENchat session: session is closed".into();
            return Task::none();
        };
        if descriptor.server_destination == "mockchatdestination"
            || descriptor.server_destination.len() < 32
        {
            self.set_omenchat_session_status(
                session_id,
                "cannot reconnect a mock/invalid OMENchat destination".into(),
            );
            return Task::none();
        }
        self.disconnect_omenchat_session(
            session_id,
            "manual reconnect requested; closing existing link before reconnect",
        );
        self.omenchat_live_opening.insert(session_id);
        self.omenchat_live_retry_after.remove(&session_id);
        self.omenchat_live_retry_count.remove(&session_id);
        let generation = self.next_omenchat_reconnect_generation(session_id);
        self.set_omenchat_session_status(session_id, "reconnecting live OMENchat link".to_string());
        self.open_live_omenchat_reconnect_task(session_id, generation, descriptor)
    }

    #[cfg(feature = "chat-client-rns")]
    fn reconnect_omenchat_session_if_disconnected_task(
        &mut self,
        session_id: ChatSessionId,
    ) -> Task<Message> {
        if self.omenchat_live_transports.contains_key(&session_id) {
            self.clear_omenchat_reconnect_state(session_id);
            self.set_omenchat_session_status(
                session_id,
                "reconnect skipped: live OMENchat link is already active".into(),
            );
            return Task::none();
        }
        if self.omenchat_live_opening.contains(&session_id) {
            self.set_omenchat_session_status(
                session_id,
                "reconnect skipped: live OMENchat reconnect is already pending".into(),
            );
            return Task::none();
        }
        self.reconnect_omenchat_session_task(session_id)
    }

    #[cfg(feature = "chat-client-rns")]
    fn disconnect_omenchat_session(&mut self, session_id: ChatSessionId, status: &str) {
        let Some(transport) = self.omenchat_live_transports.remove(&session_id) else {
            return;
        };
        let link_id = transport.link_id;
        self.omenchat_live_disconnect_count
            .entry(session_id)
            .and_modify(|count| *count = count.saturating_add(1))
            .or_insert(1);
        self.omenchat_live_last_disconnect_reason
            .insert(session_id, status.to_string());
        self.remove_omenchat_link_session_mappings(session_id);
        self.set_omenchat_session_status(session_id, status.to_string());
        let runtime = self.app.runtime.clone();
        tokio::spawn(async move {
            let _ = runtime.close_omenchat_link(link_id).await;
        });
    }

    #[cfg(feature = "chat-client-rns")]
    fn remove_omenchat_link_session_mappings(&mut self, session_id: ChatSessionId) {
        self.omenchat_link_sessions
            .retain(|_, mapped_session_id| *mapped_session_id != session_id);
    }

    #[cfg(feature = "chat-client-rns")]
    fn register_omenchat_live_transport(
        &mut self,
        session_id: ChatSessionId,
        transport: DesktopOmenChatTransport,
    ) -> Task<Message> {
        let link_id = transport.link_id;
        self.remove_omenchat_link_session_mappings(session_id);
        self.omenchat_link_sessions.insert(link_id, session_id);
        self.omenchat_live_transports.insert(session_id, transport);
        self.omenchat_live_connect_count
            .entry(session_id)
            .and_modify(|count| *count = count.saturating_add(1))
            .or_insert(1);
        self.omenchat_recent_sync_links.remove(&session_id);
        self.omenchat_recent_sync_attempts.remove(&session_id);
        self.clear_omenchat_reconnect_state(session_id);
        if self.omenchat_recent_sync_pending.remove(&session_id) {
            tracing::debug!(
                session_id,
                link_id = %hex_bytes(&link_id),
                "OMENchat recent sync running after pending room join"
            );
            let events = self.sync_recent_omenchat_room_history_if_needed(session_id);
            if omenchat_recent_sync_wants_bottom_restore(&events)
                && self
                    .chat_scroll_offsets
                    .get(&self.omenchat_scroll_key(session_id))
                    .copied()
                    .map(scroll_offset_is_at_bottom)
                    .unwrap_or(false)
            {
                return self.restore_omenchat_scroll(session_id);
            }
        } else {
            tracing::debug!(
                session_id,
                link_id = %hex_bytes(&link_id),
                "OMENchat recent sync scheduled after live transport registration"
            );
            self.schedule_delayed_omenchat_recent_sync(session_id);
        }
        Task::none()
    }

    #[cfg(feature = "chat-client-rns")]
    fn sync_recent_omenchat_room_history(
        &mut self,
        session_id: ChatSessionId,
    ) -> Vec<ChatClientEvent> {
        tracing::debug!(session_id, "OMENchat recent sync request dispatching");
        let active_room_id = self
            .chat_client
            .session(session_id)
            .map(|session| session.active_room.room_id)
            .unwrap_or(1);
        let scroll_key = (session_id, active_room_id);
        let was_following_bottom = self
            .chat_scroll_offsets
            .get(&scroll_key)
            .copied()
            .map(scroll_offset_is_at_bottom)
            .unwrap_or(true);
        if self.omenchat_live_transports.contains_key(&session_id) {
            self.omenchat_recent_sync_due_after.remove(&session_id);
            self.omenchat_recent_sync_pending.remove(&session_id);
        }
        let events = self.handle_omenchat_request(ChatClientRequest::SyncRecent { session_id });
        let accepted = events.iter().any(|event| {
            matches!(
                event,
                ChatClientEvent::HistoryPrepended { .. } | ChatClientEvent::HistorySynced { .. }
            )
        });
        if accepted && was_following_bottom {
            self.chat_scroll_offsets
                .insert(scroll_key, RelativeOffset { x: 0.0, y: 1.0 });
            self.schedule_visible_workspace_scroll_restore(2);
        }
        if !accepted && self.omenchat_live_transports.contains_key(&session_id) {
            self.schedule_retry_omenchat_recent_sync_if_unconfirmed(session_id);
        }
        if events
            .iter()
            .any(|event| matches!(event, ChatClientEvent::Error { .. }))
        {
            tracing::warn!(
                session_id,
                "OMENchat recent sync request produced an error event"
            );
        }
        events
    }

    #[cfg(feature = "chat-client-rns")]
    fn sync_recent_omenchat_room_history_if_needed(
        &mut self,
        session_id: ChatSessionId,
    ) -> Vec<ChatClientEvent> {
        let Some(link_id) = self
            .omenchat_live_transports
            .get(&session_id)
            .map(|transport| transport.link_id)
        else {
            return Vec::new();
        };
        if self
            .omenchat_recent_sync_links
            .get(&session_id)
            .is_some_and(|synced_link_id| *synced_link_id == link_id)
        {
            tracing::debug!(
                session_id,
                link_id = %hex_bytes(&link_id),
                "OMENchat recent sync skipped; link already accepted a sync response"
            );
            return Vec::new();
        }
        self.sync_recent_omenchat_room_history(session_id)
    }

    #[cfg(feature = "chat-client-rns")]
    fn schedule_delayed_omenchat_recent_sync(&mut self, session_id: ChatSessionId) {
        self.omenchat_recent_sync_due_after
            .insert(session_id, current_epoch_ms().saturating_add(1_500));
    }

    #[cfg(feature = "chat-client-rns")]
    fn schedule_retry_omenchat_recent_sync_if_unconfirmed(&mut self, session_id: ChatSessionId) {
        let attempts = self
            .omenchat_recent_sync_attempts
            .entry(session_id)
            .and_modify(|attempts| *attempts = attempts.saturating_add(1))
            .or_insert(1);
        if *attempts >= OMENCHAT_RECENT_SYNC_MAX_ATTEMPTS {
            tracing::debug!(
                session_id,
                attempts = *attempts,
                "OMENchat recent sync stopped waiting for an accepted response"
            );
            return;
        }
        self.omenchat_recent_sync_due_after
            .insert(session_id, current_epoch_ms().saturating_add(3_000));
        tracing::debug!(
            session_id,
            next_attempt = attempts.saturating_add(1),
            "OMENchat recent sync will retry if no accepted response arrives"
        );
    }

    #[cfg(feature = "chat-client-rns")]
    fn schedule_omenchat_recent_sync_after_link_activity(
        &mut self,
        session_id: ChatSessionId,
        now_ms: u64,
    ) {
        let Some(link_id) = self
            .omenchat_live_transports
            .get(&session_id)
            .map(|transport| transport.link_id)
        else {
            return;
        };
        if self
            .omenchat_recent_sync_links
            .get(&session_id)
            .is_some_and(|synced_link_id| *synced_link_id == link_id)
        {
            return;
        }
        if self
            .omenchat_recent_sync_due_after
            .contains_key(&session_id)
        {
            return;
        }
        if self
            .omenchat_recent_sync_attempts
            .get(&session_id)
            .is_some_and(|attempts| *attempts >= OMENCHAT_RECENT_SYNC_MAX_ATTEMPTS)
        {
            return;
        }
        self.omenchat_recent_sync_attempts.remove(&session_id);
        self.omenchat_recent_sync_due_after
            .insert(session_id, now_ms.saturating_add(250));
        tracing::debug!(
            session_id,
            link_id = %hex_bytes(&link_id),
            "OMENchat recent sync re-armed after confirmed link activity"
        );
    }

    #[cfg(feature = "chat-client-rns")]
    fn sync_due_omenchat_recent_history(&mut self, now_ms: u64) -> Task<Message> {
        if self.omenchat_recent_sync_due_after.is_empty() {
            return Task::none();
        }
        let due_sessions = self
            .omenchat_recent_sync_due_after
            .iter()
            .filter_map(|(session_id, due_after)| (now_ms >= *due_after).then_some(*session_id))
            .collect::<Vec<_>>();
        let mut tasks = Vec::new();
        for session_id in due_sessions {
            self.omenchat_recent_sync_due_after.remove(&session_id);
            tracing::debug!(session_id, "OMENchat delayed recent sync is due");
            let events = self.sync_recent_omenchat_room_history_if_needed(session_id);
            if omenchat_recent_sync_wants_bottom_restore(&events)
                && self
                    .chat_scroll_offsets
                    .get(&self.omenchat_scroll_key(session_id))
                    .copied()
                    .map(scroll_offset_is_at_bottom)
                    .unwrap_or(false)
            {
                tasks.push(self.restore_omenchat_scroll(session_id));
            }
        }
        if tasks.is_empty() {
            Task::none()
        } else {
            Task::batch(tasks)
        }
    }

    #[cfg(feature = "chat-client-rns")]
    fn mark_omenchat_recent_sync_complete(&mut self, session_id: ChatSessionId) {
        if let Some(link_id) = self
            .omenchat_live_transports
            .get(&session_id)
            .map(|transport| transport.link_id)
        {
            tracing::debug!(
                session_id,
                link_id = %hex_bytes(&link_id),
                "OMENchat recent sync completed for live link"
            );
            self.omenchat_recent_sync_links.insert(session_id, link_id);
            self.omenchat_recent_sync_due_after.remove(&session_id);
            self.omenchat_recent_sync_pending.remove(&session_id);
            self.omenchat_recent_sync_attempts.remove(&session_id);
        }
    }

    #[cfg(feature = "chat-client-rns")]
    fn omenchat_recent_sync_monitor_label(&self, session_id: ChatSessionId, now_ms: u64) -> String {
        let live_link = self
            .omenchat_live_transports
            .get(&session_id)
            .map(|transport| transport.link_id);
        if let Some(link_id) = live_link {
            if self
                .omenchat_recent_sync_links
                .get(&session_id)
                .is_some_and(|synced_link_id| *synced_link_id == link_id)
            {
                return "history sync: current for live link".into();
            }
        }
        if self.omenchat_recent_sync_pending.contains(&session_id) {
            return "history sync: waiting for live transport".into();
        }
        if let Some(due_after) = self.omenchat_recent_sync_due_after.get(&session_id) {
            let attempts = self
                .omenchat_recent_sync_attempts
                .get(&session_id)
                .copied()
                .unwrap_or(0);
            if now_ms >= *due_after {
                return format!("history sync: due now after {attempts} attempt(s)");
            }
            return format!(
                "history sync: retry in {} after {attempts} attempt(s)",
                compact_elapsed_ms(due_after.saturating_sub(now_ms))
            );
        }
        if let Some(attempts) = self.omenchat_recent_sync_attempts.get(&session_id) {
            if *attempts >= OMENCHAT_RECENT_SYNC_MAX_ATTEMPTS {
                return format!("history sync: stopped after {attempts} attempt(s)");
            }
        }
        if live_link.is_some() {
            "history sync: not yet confirmed".into()
        } else {
            "history sync: offline".into()
        }
    }

    #[cfg(feature = "chat-client-rns")]
    fn clear_omenchat_reconnect_state(&mut self, session_id: ChatSessionId) {
        self.omenchat_live_opening.remove(&session_id);
        self.omenchat_live_retry_after.remove(&session_id);
        self.omenchat_live_retry_count.remove(&session_id);
        self.omenchat_live_reconnect_generation.remove(&session_id);
    }

    #[cfg(feature = "chat-client-rns")]
    fn omenchat_descriptor_for_session(
        &self,
        session_id: ChatSessionId,
    ) -> Option<OmenChatDescriptor> {
        let session = self.chat_client.session(session_id)?;
        Some(OmenChatDescriptor {
            server_destination: session.server.destination.clone(),
            display_name: Some(session.server.display_name.clone()),
            rooms_hint: vec![session.active_room.name.clone()],
            local_display_name: Some(self.local_omenchat_display_name()),
            ..OmenChatDescriptor::default()
        })
    }

    #[cfg(feature = "chat-client")]
    fn handle_omenchat_request(&mut self, request: ChatClientRequest) -> Vec<ChatClientEvent> {
        if let Some(session_id) = request_session_id(&request) {
            if self
                .chat_client
                .session(session_id)
                .is_some_and(|session| is_pending_omenchat_destination(&session.server.destination))
            {
                return vec![ChatClientEvent::Error {
                    session_id: Some(session_id),
                    message: "enter an OMENchat destination and press Open before chatting".into(),
                }];
            }
        }
        #[cfg(feature = "chat-client-rns")]
        if let Some(session_id) = request_session_id(&request) {
            if self.omenchat_live_transports.contains_key(&session_id) {
                return self.handle_live_omenchat_request(request);
            }
            if self.chat_client.session(session_id).is_some_and(|session| {
                session.server.destination != "mockchatdestination"
                    && session.server.destination.len() >= 32
            }) {
                return vec![ChatClientEvent::Error {
                    session_id: Some(session_id),
                    message: "OMENchat is disconnected; use Reconnect before sending".into(),
                }];
            }
        }
        crate::chat::mock::handle_mock_request(&mut self.chat_client, request)
    }

    #[cfg(feature = "chat-client")]
    fn activate_focused_omenchat_link(&mut self) -> Option<Task<Message>> {
        let link = self
            .app
            .active_browser_tab()
            .focused_link
            .as_ref()
            .map(|link| crate::micron::LinkAction {
                target: link.target.clone(),
                fields: link.fields.clone(),
            })?;
        self.open_omenchat_link(link)
    }

    #[cfg(feature = "chat-client")]
    fn activate_omenchat_hit_action_if_needed(
        &mut self,
        action: &HitAction,
    ) -> Option<Task<Message>> {
        let HitAction::Link(link) = action else {
            return None;
        };
        self.open_omenchat_link(link.clone())
    }

    #[cfg(feature = "chat-client")]
    fn open_omenchat_link(&mut self, link: crate::micron::LinkAction) -> Option<Task<Message>> {
        let mut descriptor = OmenChatDescriptor::from_omenchat_link(&link.target)?;
        apply_omenchat_link_fields(&mut descriptor, &link.fields);
        descriptor.local_display_name = Some(self.local_omenchat_display_name());
        if let Some(session_id) = self
            .chat_client
            .sessions()
            .iter()
            .find(|session| session.server.destination == descriptor.server_destination)
            .map(|session| session.session_id)
        {
            self.chat_drafts.entry(session_id).or_default();
            self.ensure_omenchat_bottom_entry(session_id);
            self.place_omenchat_session_preferring_active_blank(session_id);
            self.persist_workspace_panes("workspace panes");
            #[cfg(feature = "chat-client-rns")]
            if self.app.runtime_status.connected
                && !self.omenchat_live_transports.contains_key(&session_id)
                && !self.omenchat_live_opening.contains(&session_id)
                && descriptor.server_destination != "mockchatdestination"
                && descriptor.server_destination.len() >= 32
            {
                self.omenchat_live_opening.insert(session_id);
                self.omenchat_live_retry_after.remove(&session_id);
                self.omenchat_live_retry_count.remove(&session_id);
                let generation = self.next_omenchat_reconnect_generation(session_id);
                self.set_omenchat_session_status(
                    session_id,
                    "opening live OMENchat link".to_string(),
                );
                self.app.status.task = format!(
                    "reconnecting OMENchat session: {}",
                    descriptor.server_destination
                );
                return Some(
                    self.open_live_omenchat_reconnect_task(session_id, generation, descriptor),
                );
            }
            self.app.status.task = format!(
                "restored existing OMENchat session: {}",
                descriptor.server_destination
            );
            return Some(Task::none());
        }
        #[cfg(feature = "chat-client-rns")]
        if self.app.runtime_status.connected
            && descriptor.server_destination != "mockchatdestination"
            && descriptor.server_destination.len() >= 32
        {
            self.app.status.task = format!(
                "opening live OMENchat link: {}",
                descriptor.server_destination
            );
            return Some(self.open_live_omenchat_task(descriptor));
        }
        let events = self.handle_omenchat_request(ChatClientRequest::OpenServer(descriptor));
        let Some(session_id) = events.iter().find_map(|event| match event {
            ChatClientEvent::ServerOpened { session_id, .. } => Some(*session_id),
            _ => None,
        }) else {
            self.app.status.task = "failed to open OMENchat descriptor".into();
            return Some(Task::none());
        };
        self.chat_drafts.entry(session_id).or_default();
        self.remember_omenchat_bottom(session_id);
        self.persist_omenchat_session(session_id);
        self.place_omenchat_session_preferring_active_blank(session_id);
        self.persist_workspace_panes("workspace panes");
        self.app.status.task = "opened OMENchat descriptor".into();
        Some(Task::none())
    }

    #[cfg(feature = "chat-client")]
    fn local_omenchat_display_name(&self) -> String {
        self.app
            .settings
            .active_identity_label
            .as_deref()
            .map(str::trim)
            .filter(|label| !label.is_empty())
            .unwrap_or("OMENbrowser_rs")
            .chars()
            .take(48)
            .collect()
    }

    #[cfg(feature = "chat-client-rns")]
    fn open_live_omenchat_task(&self, descriptor: OmenChatDescriptor) -> Task<Message> {
        let runtime = self.app.runtime.clone();
        let destination_hash = descriptor.server_destination.clone();
        Task::perform(
            async move {
                let result = runtime
                    .open_omenchat_link(
                        &destination_hash,
                        Duration::from_secs(30),
                        crate::runtime::CancellationToken::new(),
                    )
                    .await
                    .map_err(|error| error.to_string());
                (descriptor, result)
            },
            |(descriptor, result)| Message::OmenChatLiveOpenResult { descriptor, result },
        )
    }

    #[cfg(feature = "chat-client-rns")]
    fn open_live_omenchat_reconnect_task(
        &self,
        session_id: ChatSessionId,
        generation: u64,
        descriptor: OmenChatDescriptor,
    ) -> Task<Message> {
        let runtime = self.app.runtime.clone();
        let destination_hash = descriptor.server_destination.clone();
        Task::perform(
            async move {
                let result = runtime
                    .open_omenchat_link(
                        &destination_hash,
                        Duration::from_secs(30),
                        crate::runtime::CancellationToken::new(),
                    )
                    .await
                    .map_err(|error| error.to_string());
                (session_id, generation, descriptor, result)
            },
            |(session_id, generation, descriptor, result)| Message::OmenChatLiveReconnectResult {
                session_id,
                generation,
                descriptor,
                result,
            },
        )
    }

    #[cfg(feature = "chat-client-rns")]
    fn reconnect_restored_omenchat_sessions_if_ready(&mut self) -> Task<Message> {
        if !self.app.runtime_status.connected {
            return Task::none();
        }
        let mut tasks = Vec::new();
        let now = current_epoch_ms();
        let max_auto_attempts = 5u8;
        let candidates = self
            .chat_client
            .sessions()
            .iter()
            .filter(|session| {
                session.server.destination != "mockchatdestination"
                    && session.server.destination.len() >= 32
                    && !self
                        .omenchat_live_transports
                        .contains_key(&session.session_id)
                    && !self.omenchat_live_opening.contains(&session.session_id)
                    && self
                        .omenchat_live_retry_after
                        .get(&session.session_id)
                        .is_none_or(|retry_after| now >= *retry_after)
                    && self
                        .omenchat_live_retry_count
                        .get(&session.session_id)
                        .copied()
                        .unwrap_or(0)
                        < max_auto_attempts
            })
            .map(|session| {
                (
                    session.session_id,
                    OmenChatDescriptor {
                        server_destination: session.server.destination.clone(),
                        display_name: Some(session.server.display_name.clone()),
                        rooms_hint: vec![session.active_room.name.clone()],
                        local_display_name: Some(self.local_omenchat_display_name()),
                        ..OmenChatDescriptor::default()
                    },
                )
            })
            .collect::<Vec<_>>();

        for (session_id, descriptor) in candidates {
            self.omenchat_live_opening.insert(session_id);
            let generation = self.next_omenchat_reconnect_generation(session_id);
            self.set_omenchat_session_status(
                session_id,
                "reconnecting live OMENchat link".to_string(),
            );
            tasks.push(self.open_live_omenchat_reconnect_task(session_id, generation, descriptor));
        }

        if tasks.is_empty() {
            Task::none()
        } else {
            Task::batch(tasks)
        }
    }

    #[cfg(feature = "chat-client-rns")]
    fn handle_omenchat_live_open_result(
        &mut self,
        descriptor: OmenChatDescriptor,
        result: Result<crate::runtime::OmenChatLinkOpened, String>,
    ) -> Task<Message> {
        let opened = match result {
            Ok(opened) => opened,
            Err(error) => {
                let session_id = self.open_omenchat_status_session(
                    descriptor,
                    omenchat_live_open_error_status(&error),
                );
                self.place_omenchat_session_preferring_active_blank(session_id);
                self.persist_workspace_panes("workspace panes");
                self.app.status.task = format!("OMENchat live link failed: {error}");
                return Task::none();
            }
        };
        let mut transport = DesktopOmenChatTransport::new(opened.link_id, current_epoch_ms());
        let events = crate::chat::live::handle_live_request(
            &mut self.chat_client,
            &mut self.omenchat_live_state,
            &mut transport,
            ChatClientRequest::OpenServer(descriptor),
        );
        self.apply_omenchat_client_events_status(&events);
        let Some(session_id) = events.iter().find_map(|event| match event {
            ChatClientEvent::ServerOpened { session_id, .. } => Some(*session_id),
            _ => None,
        }) else {
            self.app.status.task = "OMENchat live session failed to initialize".into();
            return Task::none();
        };
        self.send_omenchat_outgoing_frames(opened.link_id, transport.take_outgoing_frames());
        self.chat_drafts.entry(session_id).or_default();
        self.remember_omenchat_bottom(session_id);
        self.persist_omenchat_session(session_id);
        self.place_omenchat_session_preferring_active_blank(session_id);
        let scroll_task = self.register_omenchat_live_transport(session_id, transport);
        self.persist_workspace_panes("workspace panes");
        self.app.status.task = format!("opened live OMENchat link {}", hex_bytes(&opened.link_id));
        scroll_task
    }

    #[cfg(feature = "chat-client-rns")]
    fn handle_omenchat_live_reconnect_result(
        &mut self,
        session_id: ChatSessionId,
        generation: u64,
        descriptor: OmenChatDescriptor,
        result: Result<crate::runtime::OmenChatLinkOpened, String>,
    ) -> Task<Message> {
        if !self.omenchat_reconnect_generation_is_current(session_id, generation) {
            if let Ok(opened) = result {
                let runtime = self.app.runtime.clone();
                tokio::spawn(async move {
                    let _ = runtime.close_omenchat_link(opened.link_id).await;
                });
            }
            return Task::none();
        }
        self.omenchat_live_opening.remove(&session_id);
        let opened = match result {
            Ok(opened) => opened,
            Err(error) => {
                let attempts = self
                    .omenchat_live_retry_count
                    .entry(session_id)
                    .and_modify(|count| *count = count.saturating_add(1))
                    .or_insert(1);
                self.omenchat_live_retry_after
                    .insert(session_id, current_epoch_ms().saturating_add(15_000));
                let status = if *attempts >= 5 {
                    format!(
                        "{}; automatic reconnect paused after {attempts} attempts, use Reconnect to try again",
                        omenchat_live_open_error_status(&error)
                    )
                } else {
                    format!(
                        "{}; automatic reconnect attempt {attempts}/5",
                        omenchat_live_open_error_status(&error)
                    )
                };
                self.set_omenchat_session_status(session_id, status);
                self.app.status.task = format!("OMENchat live reconnect failed: {error}");
                return Task::none();
            }
        };
        let mut transport = DesktopOmenChatTransport::new(opened.link_id, current_epoch_ms());
        let events = crate::chat::live::reconnect_live_server(
            &mut self.chat_client,
            &mut self.omenchat_live_state,
            &mut transport,
            session_id,
            descriptor,
        );
        self.apply_omenchat_client_events_status(&events);
        self.send_omenchat_outgoing_frames(opened.link_id, transport.take_outgoing_frames());
        self.omenchat_live_retry_after.remove(&session_id);
        self.omenchat_live_retry_count.remove(&session_id);
        self.omenchat_live_reconnect_generation.remove(&session_id);
        self.chat_drafts.entry(session_id).or_default();
        self.remember_omenchat_bottom(session_id);
        self.persist_omenchat_session(session_id);
        self.ensure_pane_for_omenchat(session_id);
        let scroll_task = self.register_omenchat_live_transport(session_id, transport);
        self.persist_workspace_panes("workspace panes");
        self.app.status.task = format!(
            "reconnected live OMENchat link {}",
            hex_bytes(&opened.link_id)
        );
        scroll_task
    }

    #[cfg(feature = "chat-client-rns")]
    fn next_omenchat_reconnect_generation(&mut self, session_id: ChatSessionId) -> u64 {
        let entry = self
            .omenchat_live_reconnect_generation
            .entry(session_id)
            .or_insert(0);
        *entry = entry.saturating_add(1);
        *entry
    }

    #[cfg(feature = "chat-client-rns")]
    fn omenchat_reconnect_generation_is_current(
        &self,
        session_id: ChatSessionId,
        generation: u64,
    ) -> bool {
        self.omenchat_live_reconnect_generation
            .get(&session_id)
            .copied()
            .unwrap_or(0)
            == generation
    }

    #[cfg(feature = "chat-client")]
    fn persist_omenchat_session(&mut self, session_id: ChatSessionId) {
        let Some(store) = self.chat_store.as_mut() else {
            return;
        };
        if let Err(error) = self.chat_client.persist_session(store, session_id) {
            tracing::warn!("failed to persist OMENchat session {session_id}: {error}");
        }
    }

    #[cfg(feature = "chat-client-rns")]
    fn handle_live_omenchat_request(&mut self, request: ChatClientRequest) -> Vec<ChatClientEvent> {
        let Some(session_id) = request_session_id(&request) else {
            return vec![ChatClientEvent::Error {
                session_id: None,
                message: "OMENchat live request missing session id".into(),
            }];
        };
        let Some(transport) = self.omenchat_live_transports.get_mut(&session_id) else {
            return vec![ChatClientEvent::Error {
                session_id: Some(session_id),
                message: "OMENchat is disconnected; use Reconnect before sending".into(),
            }];
        };
        let (link_id, events, outgoing, resources) = {
            let link_id = transport.link_id;
            let events = crate::chat::live::handle_live_request(
                &mut self.chat_client,
                &mut self.omenchat_live_state,
                transport,
                request,
            );
            let outgoing = transport.take_outgoing_frames();
            let resources = transport.take_outgoing_resources();
            (link_id, events, outgoing, resources)
        };
        self.apply_omenchat_client_events_status(&events);
        self.send_omenchat_outgoing_frames(link_id, outgoing);
        self.send_omenchat_outgoing_resources(link_id, resources);
        events
    }

    #[cfg(feature = "chat-client-rns")]
    fn send_omenchat_outgoing_frames(&mut self, link_id: [u8; 16], frames: Vec<Vec<u8>>) {
        if frames.is_empty() {
            return;
        }
        let runtime = self.app.runtime.clone();
        let frame_count = frames.len();
        self.app.status.task = format!("OMENchat sending {frame_count} frame(s)");
        tokio::spawn(async move {
            for frame in frames {
                let byte_len = frame.len();
                let frame_summary = crate::chat::codec::decode_frame(&frame)
                    .map(|decoded| {
                        let body = match &decoded.body {
                            crate::chat::protocol::FrameBody::Empty => "empty".to_string(),
                            crate::chat::protocol::FrameBody::Text(text) => {
                                format!("text:{}", text.len())
                            }
                            crate::chat::protocol::FrameBody::Fields(fields) => {
                                format!("fields:{}", fields.len())
                            }
                        };
                        format!(
                            "{:?} seq={} room={:?} body={}",
                            decoded.op, decoded.seq, decoded.room_id, body
                        )
                    })
                    .unwrap_or_else(|error| format!("decode_error {error}"));
                match runtime.send_omenchat_frame(link_id, frame).await {
                    Ok(()) => {
                        tracing::debug!(
                            link_id = %hex_bytes(&link_id),
                            bytes = byte_len,
                            frame = %frame_summary,
                            "OMENchat sent Link frame"
                        );
                    }
                    Err(error) => {
                        tracing::warn!(
                            link_id = %hex_bytes(&link_id),
                            bytes = byte_len,
                            frame = %frame_summary,
                            error = %error,
                            "OMENchat Link frame send failed"
                        );
                    }
                }
            }
        });
    }

    #[cfg(feature = "chat-client-rns")]
    fn send_omenchat_outgoing_resources(
        &mut self,
        link_id: [u8; 16],
        resources: Vec<(String, Vec<u8>)>,
    ) {
        if resources.is_empty() {
            return;
        }
        let runtime = self.app.runtime.clone();
        let count = resources.len();
        self.app.status.task = format!("OMENchat sending {count} resource(s)");
        tokio::spawn(async move {
            for (resource_id, payload) in resources {
                let byte_len = payload.len();
                match runtime
                    .send_omenchat_resource(link_id, resource_id.clone(), payload)
                    .await
                {
                    Ok(()) => tracing::debug!(
                        link_id = %hex_bytes(&link_id),
                        resource_id,
                        bytes = byte_len,
                        "OMENchat sent Link resource"
                    ),
                    Err(error) => tracing::warn!(
                        link_id = %hex_bytes(&link_id),
                        resource_id,
                        bytes = byte_len,
                        error = %error,
                        "OMENchat Link resource send failed"
                    ),
                }
            }
        });
    }

    #[cfg(feature = "chat-client-rns")]
    fn omenchat_frame_summary(frame: &[u8]) -> String {
        crate::chat::codec::decode_frame(frame)
            .map(|decoded| {
                let body = match &decoded.body {
                    crate::chat::protocol::FrameBody::Empty => "empty".to_string(),
                    crate::chat::protocol::FrameBody::Text(text) => {
                        format!("text:{}", text.len())
                    }
                    crate::chat::protocol::FrameBody::Fields(fields) => {
                        format!("fields:{}", fields.len())
                    }
                };
                format!(
                    "{:?} seq={} room={:?} body={}",
                    decoded.op, decoded.seq, decoded.room_id, body
                )
            })
            .unwrap_or_else(|error| format!("decode_error {error}"))
    }

    #[cfg(feature = "chat-client-rns")]
    fn drain_omenchat_runtime_events(&mut self) -> Task<Message> {
        let now = current_epoch_ms();
        let mut scroll_tasks = Vec::new();
        for closed in self.app.drain_omenchat_link_closed() {
            let Some(session_id) = self.omenchat_link_sessions.remove(&closed.link_id) else {
                continue;
            };
            let closed_link_is_active = self
                .omenchat_live_transports
                .get(&session_id)
                .is_some_and(|transport| transport.link_id == closed.link_id);
            if !closed_link_is_active {
                tracing::debug!(
                    link_id = %hex_bytes(&closed.link_id),
                    session_id,
                    "ignored stale OMENchat link close"
                );
                continue;
            }
            self.omenchat_live_transports.remove(&session_id);
            self.omenchat_live_disconnect_count
                .entry(session_id)
                .and_modify(|count| *count = count.saturating_add(1))
                .or_insert(1);
            let reason = closed.reason.as_deref().unwrap_or("server disconnected");
            self.omenchat_live_last_disconnect_reason
                .insert(session_id, reason.to_string());
            let quick_reconnect =
                omenchat_close_reason_allows_quick_reconnect(closed.reason.as_deref());
            let status = if quick_reconnect {
                self.omenchat_live_retry_after
                    .insert(session_id, current_epoch_ms().saturating_add(500));
                if omenchat_close_reason_is_timeout(closed.reason.as_deref()) {
                    format!("OMENchat link timed out; reconnecting ({reason})")
                } else {
                    format!("OMENchat link closed; reconnecting ({reason})")
                }
            } else {
                self.clear_omenchat_reconnect_state(session_id);
                format!("OMENchat disconnected: {reason}; use Reconnect to open a new link")
            };
            self.set_omenchat_session_status(session_id, status);
            self.persist_omenchat_session(session_id);
            self.app.status.task = format!(
                "OMENchat reconnect pending: {}",
                self.chat_client
                    .session(session_id)
                    .map(|session| session.server.display_name.as_str())
                    .unwrap_or("session")
            );
        }
        for data in self.app.drain_omenchat_link_data() {
            let Some(session_id) = self.omenchat_link_sessions.get(&data.link_id).copied() else {
                continue;
            };
            tracing::debug!(
                session_id,
                link_id = %hex_bytes(&data.link_id),
                bytes = data.frame_bytes.len(),
                frame = %Self::omenchat_frame_summary(&data.frame_bytes),
                "OMENchat received Link frame"
            );
            let received_op = crate::chat::codec::decode_frame(&data.frame_bytes)
                .ok()
                .map(|frame| frame.op);
            let scroll_key = self.omenchat_scroll_key(session_id);
            let was_following_bottom = self
                .chat_scroll_offsets
                .get(&scroll_key)
                .copied()
                .map(scroll_offset_is_at_bottom)
                .unwrap_or(true);
            let Some((events, pending_resources, outgoing, resources)) = self
                .omenchat_live_transports
                .get_mut(&session_id)
                .map(|transport| {
                    transport.push_incoming_frame(data.frame_bytes, now);
                    let events = crate::chat::live::drain_live_events_with_state(
                        &mut self.chat_client,
                        &mut self.omenchat_live_state,
                        transport,
                        Some(session_id),
                    );
                    let pending_resources = transport.pending_resource_offer_count();
                    let outgoing = transport.take_outgoing_frames();
                    let resources = transport.take_outgoing_resources();
                    (events, pending_resources, outgoing, resources)
                })
            else {
                continue;
            };
            if pending_resources > 0 {
                self.set_omenchat_session_status(
                    session_id,
                    format!("waiting for {pending_resources} OMENchat Resource payload(s)"),
                );
            }
            self.apply_omenchat_client_events_status(&events);
            if was_following_bottom && omenchat_recent_sync_wants_bottom_restore(&events) {
                self.chat_scroll_offsets
                    .insert(scroll_key, RelativeOffset { x: 0.0, y: 1.0 });
                scroll_tasks.push(self.restore_omenchat_scroll(session_id));
            }
            self.send_omenchat_outgoing_frames(data.link_id, outgoing);
            self.send_omenchat_outgoing_resources(data.link_id, resources);
            if matches!(received_op, Some(crate::chat::protocol::ChatOp::Pong)) {
                self.schedule_omenchat_recent_sync_after_link_activity(session_id, now);
            }
            if !events.is_empty() {
                self.persist_omenchat_session(session_id);
            }
        }
        for data in self.app.drain_omenchat_resource_data() {
            let link_id = data.link_id;
            let Some(session_id) = self.omenchat_link_sessions.get(&data.link_id).copied() else {
                continue;
            };
            let scroll_key = self.omenchat_scroll_key(session_id);
            let was_following_bottom = self
                .chat_scroll_offsets
                .get(&scroll_key)
                .copied()
                .map(scroll_offset_is_at_bottom)
                .unwrap_or(true);
            if let Some((events, pending_before, pending_after, outgoing, resources)) = self
                .omenchat_live_transports
                .get_mut(&session_id)
                .map(|transport| {
                    let pending_before = transport.pending_resource_offer_count();
                    transport.push_resource(data.metadata, data.data, now);
                    let events = crate::chat::live::drain_live_events_with_state(
                        &mut self.chat_client,
                        &mut self.omenchat_live_state,
                        transport,
                        Some(session_id),
                    );
                    let pending_after = transport.pending_resource_offer_count();
                    let outgoing = transport.take_outgoing_frames();
                    let resources = transport.take_outgoing_resources();
                    (events, pending_before, pending_after, outgoing, resources)
                })
            {
                if pending_before > 0 && pending_after < pending_before {
                    self.set_omenchat_session_status(
                        session_id,
                        "received delayed OMENchat Resource payload".to_string(),
                    );
                }
                if pending_after > 0 {
                    self.set_omenchat_session_status(
                        session_id,
                        format!("waiting for {pending_after} OMENchat Resource payload(s)"),
                    );
                }
                self.apply_omenchat_client_events_status(&events);
                if was_following_bottom && omenchat_recent_sync_wants_bottom_restore(&events) {
                    self.chat_scroll_offsets
                        .insert(scroll_key, RelativeOffset { x: 0.0, y: 1.0 });
                    scroll_tasks.push(self.restore_omenchat_scroll(session_id));
                }
                self.send_omenchat_outgoing_frames(link_id, outgoing);
                self.send_omenchat_outgoing_resources(link_id, resources);
                if !events.is_empty() {
                    self.persist_omenchat_session(session_id);
                }
            }
        }
        if scroll_tasks.is_empty() {
            Task::none()
        } else {
            Task::batch(scroll_tasks)
        }
    }

    #[cfg(feature = "chat-client-rns")]
    fn maintain_omenchat_live_links(&mut self, now: u64) -> Task<Message> {
        let mut stale_sessions = Vec::new();
        let mut outbound = Vec::new();
        let session_ids = self
            .omenchat_live_transports
            .keys()
            .copied()
            .collect::<Vec<_>>();

        for session_id in session_ids {
            let Some(transport) = self.omenchat_live_transports.get_mut(&session_id) else {
                continue;
            };
            let last_activity = transport
                .last_rx_epoch_ms
                .max(transport.last_tx_epoch_ms)
                .max(transport.last_ping_epoch_ms);
            let heartbeat_idle_ms = transport.heartbeat_idle_ms.clamp(
                OMENCHAT_MIN_HEARTBEAT_IDLE_MS,
                OMENCHAT_MAX_HEARTBEAT_IDLE_MS,
            );
            let heartbeat_timeout_ms = OMENCHAT_HEARTBEAT_TIMEOUT_MS.max(
                heartbeat_idle_ms
                    .saturating_mul(3)
                    .min(OMENCHAT_MAX_HEARTBEAT_IDLE_MS),
            );
            if transport.awaiting_pong
                && now.saturating_sub(transport.last_ping_epoch_ms) >= heartbeat_timeout_ms
            {
                stale_sessions.push(session_id);
                continue;
            }
            if now.saturating_sub(last_activity) < heartbeat_idle_ms {
                continue;
            }
            if let Some(event) = crate::chat::live::ping_live_session(
                &mut self.omenchat_live_state,
                transport,
                session_id,
            ) {
                self.apply_omenchat_client_events_status(&[event]);
                stale_sessions.push(session_id);
            } else {
                transport.last_ping_epoch_ms = now;
                transport.awaiting_pong = true;
                let link_id = transport.link_id;
                let frames = transport.take_outgoing_frames();
                if !frames.is_empty() {
                    outbound.push((link_id, frames));
                }
            }
        }

        for session_id in stale_sessions {
            self.disconnect_omenchat_session(
                session_id,
                "OMENchat heartbeat timed out; use Reconnect to open a fresh link",
            );
            self.omenchat_live_retry_after
                .insert(session_id, now.saturating_add(2_000));
        }
        for (link_id, frames) in outbound {
            self.send_omenchat_outgoing_frames(link_id, frames);
        }

        Task::none()
    }

    #[cfg(feature = "chat-client-rns")]
    fn omenchat_reconnect_state_label(&self, session_id: ChatSessionId, now: u64) -> String {
        if self.omenchat_live_transports.contains_key(&session_id) {
            return if self.omenchat_live_opening.contains(&session_id)
                || self.omenchat_live_retry_after.contains_key(&session_id)
                || self
                    .omenchat_live_reconnect_generation
                    .contains_key(&session_id)
            {
                "reconnect: stale state clearing".into()
            } else {
                "reconnect: idle".into()
            };
        }
        if self.omenchat_live_opening.contains(&session_id) {
            return "reconnect: opening".into();
        }
        if let Some(due_after) = self.omenchat_live_retry_after.get(&session_id) {
            let attempts = self
                .omenchat_live_retry_count
                .get(&session_id)
                .copied()
                .unwrap_or_default();
            let wait = compact_elapsed_ms(due_after.saturating_sub(now));
            return format!("reconnect: queued in {wait} (attempt {attempts}/5)");
        }
        "reconnect: manual".into()
    }

    #[cfg(feature = "chat-client")]
    fn open_omenchat_status_session(
        &mut self,
        descriptor: OmenChatDescriptor,
        status: String,
    ) -> ChatSessionId {
        let server_destination = descriptor.server_destination;
        if let Some(session) = self
            .chat_client
            .sessions()
            .iter()
            .find(|session| session.server.destination == server_destination)
        {
            let session_id = session.session_id;
            self.set_omenchat_session_status(session_id, status);
            self.chat_drafts.entry(session_id).or_default();
            return session_id;
        }

        let session_id = self.chat_client.reserve_session_id();
        let server = crate::chat::model::ChatServerSummary {
            server_id: server_destination.clone(),
            destination: server_destination,
            display_name: descriptor
                .display_name
                .unwrap_or_else(|| "OMENchat Server".to_string()),
        };
        let room_name = descriptor
            .rooms_hint
            .first()
            .cloned()
            .unwrap_or_else(|| "lobby".to_string());
        let room = crate::chat::model::ChatRoomSummary {
            server_id: server.server_id.clone(),
            room_id: 1,
            name: room_name,
            topic: None,
            unread: 0,
            joined: false,
        };
        self.chat_client.push_session(crate::chat::ChatSessionView {
            session_id,
            server,
            rooms: vec![room.clone()],
            active_room: room,
            users: Vec::new(),
            events: Vec::new(),
            status,
        });
        self.chat_drafts.entry(session_id).or_default();
        self.persist_omenchat_session(session_id);
        session_id
    }

    #[cfg(feature = "chat-client")]
    fn create_blank_omenchat_session(&mut self) -> ChatSessionId {
        let session_id = self.chat_client.reserve_session_id();
        let server_destination = format!("{OMENCHAT_PENDING_DESTINATION_PREFIX}{session_id}");
        let server = crate::chat::model::ChatServerSummary {
            server_id: server_destination.clone(),
            destination: server_destination,
            display_name: "New Chat".into(),
        };
        let room = crate::chat::model::ChatRoomSummary {
            server_id: server.server_id.clone(),
            room_id: 1,
            name: "lobby".into(),
            topic: Some("Enter an OMENchat destination hash above, then press Open.".into()),
            unread: 0,
            joined: false,
        };
        self.chat_client.push_session(crate::chat::ChatSessionView {
            session_id,
            server,
            rooms: vec![room.clone()],
            active_room: room,
            users: Vec::new(),
            events: Vec::new(),
            status: "enter an OMENchat destination hash, then press Open".into(),
        });
        self.chat_drafts.entry(session_id).or_default();
        self.remember_omenchat_bottom(session_id);
        self.app.status.task = "created blank OMENchat client pane".into();
        session_id
    }

    #[cfg(feature = "chat-client")]
    fn set_omenchat_session_status(&mut self, session_id: ChatSessionId, status: String) {
        if let Some(session) = self.chat_client.session_mut(session_id) {
            session.status = status;
        }
    }

    #[cfg(feature = "chat-client")]
    fn omenchat_session_upload_max_file_bytes(&self, session_id: ChatSessionId) -> Option<u64> {
        self.omenchat_upload_max_file_bytes
            .get(&session_id)
            .copied()
    }

    #[cfg(feature = "chat-client")]
    fn omenchat_session_upload_quota(&self, session_id: ChatSessionId) -> Option<u64> {
        self.omenchat_upload_quotas.get(&session_id).copied()
    }

    #[cfg(feature = "chat-client")]
    fn clear_omenchat_active_room_unread(&mut self, session_id: ChatSessionId) {
        let Some(session) = self.chat_client.session_mut(session_id) else {
            return;
        };
        let room_id = session.active_room.room_id;
        session.active_room.unread = 0;
        for room in &mut session.rooms {
            if room.room_id == room_id {
                room.unread = 0;
            }
        }
        self.persist_omenchat_session(session_id);
    }

    #[cfg(feature = "chat-client")]
    fn mark_hidden_omenchat_room_unread(&mut self, session_id: ChatSessionId, room_id: RoomId) {
        if self
            .find_workspace_pane(&DesktopPane::OmenChat(session_id))
            .is_some()
        {
            return;
        }
        let Some(session) = self.chat_client.session_mut(session_id) else {
            return;
        };
        if session.active_room.room_id != room_id {
            self.persist_omenchat_session(session_id);
            return;
        }
        session.active_room.unread = session.active_room.unread.saturating_add(1);
        if let Some(room) = session
            .rooms
            .iter_mut()
            .find(|room| room.room_id == room_id)
        {
            room.unread = room.unread.saturating_add(1);
        }
        self.persist_omenchat_session(session_id);
    }

    #[cfg(feature = "chat-client")]
    fn restore_cached_omenchat_room_history(&mut self, session_id: ChatSessionId) -> usize {
        let cached_loaded = if let Some(store) = self.chat_store.as_ref() {
            match self
                .chat_client
                .load_cached_room_history(store, session_id, 100)
            {
                Ok(count) => count,
                Err(error) => {
                    self.set_omenchat_session_status(
                        session_id,
                        format!("cached room history load failed: {error}"),
                    );
                    0
                }
            }
        } else {
            0
        };
        if cached_loaded > 0 {
            if self
                .workspace_panes
                .iter()
                .any(|(_, pane)| matches!(pane, DesktopPane::OmenChat(id) if *id == session_id))
            {
                self.lock_omenchat_bottom_until_restore_settles(session_id);
                self.schedule_visible_workspace_scroll_restore(5);
            } else {
                self.remember_omenchat_bottom_if_missing(session_id);
            }
        }
        self.persist_omenchat_session(session_id);
        cached_loaded
    }

    #[cfg(feature = "chat-client")]
    fn apply_omenchat_client_events_status(&mut self, events: &[ChatClientEvent]) {
        for event in events {
            match event {
                ChatClientEvent::RoomJoined { session_id, .. } => {
                    self.restore_cached_omenchat_room_history(*session_id);
                    #[cfg(feature = "chat-client-rns")]
                    if self.omenchat_live_transports.contains_key(session_id) {
                        self.sync_recent_omenchat_room_history_if_needed(*session_id);
                    } else {
                        self.omenchat_recent_sync_pending.insert(*session_id);
                    }
                }
                ChatClientEvent::ServerMotd { session_id, motd } => {
                    let motd = motd.trim();
                    if motd.is_empty() {
                        self.omenchat_motds.remove(session_id);
                    } else {
                        self.omenchat_motds.insert(*session_id, motd.to_owned());
                    }
                }
                ChatClientEvent::ServerPolicy {
                    session_id,
                    upload_quota_bytes,
                    upload_max_file_bytes,
                    ping_interval_seconds,
                } => {
                    self.omenchat_upload_quotas
                        .insert(*session_id, *upload_quota_bytes);
                    self.omenchat_upload_max_file_bytes
                        .insert(*session_id, *upload_max_file_bytes);
                    #[cfg(feature = "chat-client-rns")]
                    if let Some(transport) = self.omenchat_live_transports.get_mut(session_id) {
                        transport.heartbeat_idle_ms =
                            ping_interval_seconds.saturating_mul(1_000).clamp(
                                OMENCHAT_MIN_HEARTBEAT_IDLE_MS,
                                OMENCHAT_MAX_HEARTBEAT_IDLE_MS,
                            );
                    }
                    let quota = if *upload_quota_bytes == 0 {
                        "uploads disabled".into()
                    } else {
                        format!("upload quota {}", human_bytes(*upload_quota_bytes))
                    };
                    self.set_omenchat_session_status(
                        *session_id,
                        format!(
                            "server policy: {quota}; max file {}; ping every {ping_interval_seconds}s",
                            human_bytes(*upload_max_file_bytes)
                        ),
                    );
                }
                ChatClientEvent::UserUpdated { session_id, .. } => {
                    self.persist_omenchat_session(*session_id);
                }
                ChatClientEvent::EventAppended { session_id, event } => {
                    self.mark_hidden_omenchat_room_unread(*session_id, event.room_id);
                    if !is_omenchat_local_echo_event(event) {
                        self.persist_omenchat_session(*session_id);
                    }
                }
                ChatClientEvent::HistoryPrepended { session_id, .. } => {
                    #[cfg(feature = "chat-client-rns")]
                    self.mark_omenchat_recent_sync_complete(*session_id);
                    self.persist_omenchat_session(*session_id);
                }
                ChatClientEvent::HistorySynced { session_id, .. } => {
                    #[cfg(not(feature = "chat-client-rns"))]
                    let _ = session_id;
                    #[cfg(feature = "chat-client-rns")]
                    self.mark_omenchat_recent_sync_complete(*session_id);
                }
                ChatClientEvent::HistorySyncNeeded {
                    session_id,
                    room_id,
                } => {
                    #[cfg(feature = "chat-client-rns")]
                    {
                        self.omenchat_recent_sync_links.remove(session_id);
                        self.omenchat_recent_sync_attempts.remove(session_id);
                        self.omenchat_recent_sync_due_after
                            .insert(*session_id, current_epoch_ms().saturating_add(250));
                        tracing::debug!(
                            session_id = *session_id,
                            room_id = *room_id,
                            "OMENchat live event gap detected; scheduled bounded recent sync"
                        );
                    }
                    #[cfg(not(feature = "chat-client-rns"))]
                    let _ = (session_id, room_id);
                }
                ChatClientEvent::UploadAccepted {
                    session_id,
                    resource_id,
                    filename,
                    bytes,
                } => {
                    let pending_key = (*session_id, filename.clone(), *bytes);
                    if let Some(source_path) =
                        self.omenchat_pending_upload_sources.remove(&pending_key)
                    {
                        match self.cache_omenchat_upload_source_file(
                            *session_id,
                            resource_id,
                            filename,
                            &source_path,
                        ) {
                            Ok(path) => {
                                self.set_omenchat_session_status(
                                    *session_id,
                                    format!("upload accepted and cached locally: {path}"),
                                );
                            }
                            Err(error) => {
                                self.set_omenchat_session_status(
                                    *session_id,
                                    format!("upload accepted; local cache failed: {error}"),
                                );
                            }
                        }
                    } else {
                        self.set_omenchat_session_status(
                            *session_id,
                            format!("upload accepted: {filename} ({})", human_bytes(*bytes)),
                        );
                    }
                }
                ChatClientEvent::UploadRejected { session_id, reason } => {
                    self.omenchat_pending_upload_sources
                        .retain(|(pending_session_id, _, _), _| *pending_session_id != *session_id);
                    self.set_omenchat_session_status(
                        *session_id,
                        format!("upload rejected: {reason}"),
                    );
                }
                ChatClientEvent::UploadResourceAvailable {
                    session_id,
                    resource_id,
                    filename,
                    content_type,
                    bytes,
                } => match self.cache_omenchat_upload_resource(
                    *session_id,
                    resource_id,
                    filename,
                    content_type.as_deref(),
                    bytes,
                ) {
                    Ok(path) => {
                        self.set_omenchat_session_status(
                            *session_id,
                            format!("upload resource cached: {path}"),
                        );
                    }
                    Err(error) => {
                        self.set_omenchat_session_status(
                            *session_id,
                            format!("upload resource cache failed: {error}"),
                        );
                    }
                },
                ChatClientEvent::UploadResourceProgress {
                    session_id,
                    resource_id,
                    filename,
                    received,
                    total,
                } => {
                    self.omenchat_media_cache.insert(
                        omenchat_upload_cache_key(*session_id, resource_id),
                        OmenChatMediaLoadState::Loading {
                            message: format!(
                                "receiving {filename}: {} / {}",
                                human_bytes(*received),
                                human_bytes(*total)
                            ),
                            received: Some(*received),
                            total: Some(*total),
                        },
                    );
                }
                ChatClientEvent::Error {
                    session_id: Some(session_id),
                    message,
                } => {
                    self.set_omenchat_session_status(*session_id, format!("error: {message}"));
                }
                _ => {}
            }
        }
    }

    fn split_workspace_from_active(&mut self, kind: DesktopPane) {
        self.schedule_visible_workspace_scroll_restore(2);
        let target = self
            .workspace_panes
            .get(self.active_workspace_pane)
            .map(|_| self.active_workspace_pane)
            .or_else(|| self.workspace_panes.iter().next().map(|(pane, _)| *pane));
        let Some(target) = target else {
            let (panes, pane) = pane_grid::State::new(kind);
            self.workspace_panes = panes;
            self.active_workspace_pane = pane;
            return;
        };
        if let Some((pane, _)) = self
            .workspace_panes
            .split(pane_grid::Axis::Vertical, target, kind)
        {
            self.active_workspace_pane = pane;
            self.remember_visible_workspace_scroll_bottoms();
        }
    }

    fn find_workspace_pane(&self, kind: &DesktopPane) -> Option<pane_grid::Pane> {
        self.workspace_panes
            .iter()
            .find_map(|(pane, pane_kind)| (pane_kind == kind).then_some(*pane))
    }

    fn close_workspace_pane(&mut self, pane: pane_grid::Pane) {
        if self.workspace_panes.len() <= 1 {
            return;
        }
        self.schedule_visible_workspace_scroll_restore(2);
        if let Some((_, sibling)) = self.workspace_panes.close(pane) {
            self.active_workspace_pane = sibling;
            self.focus_workspace_pane(sibling);
            self.remember_visible_workspace_scroll_bottoms();
        }
    }

    fn close_or_replace_deleted_conversation_pane(&mut self, pane: Option<pane_grid::Pane>) {
        let Some(pane) = pane else {
            return;
        };
        if self.workspace_panes.len() > 1 {
            self.close_workspace_pane(pane);
            return;
        }
        if let Some(kind) = self.workspace_panes.get_mut(pane) {
            *kind = DesktopPane::Browser(self.app.active_browser_tab().id);
            self.active_workspace_pane = pane;
            self.focus_workspace_pane(pane);
        }
    }

    fn persist_workspace_panes(&mut self, label: &str) {
        let panes = self.desktop_workspace_pane_settings();
        let layout = self.desktop_workspace_layout_settings();
        let active = self
            .workspace_panes
            .iter()
            .position(|(pane, _)| *pane == self.active_workspace_pane);
        self.app
            .save_desktop_workspace_layout(panes, active, layout, label);
    }

    fn schedule_workspace_panes_persist(&mut self, label: &str) {
        let panes = self.desktop_workspace_pane_settings();
        let layout = self.desktop_workspace_layout_settings();
        let active = self
            .workspace_panes
            .iter()
            .position(|(pane, _)| *pane == self.active_workspace_pane);
        self.app
            .schedule_desktop_workspace_layout_save(panes, active, layout, label);
    }

    fn desktop_workspace_pane_settings(&self) -> Vec<DesktopWorkspacePaneSettings> {
        self.workspace_panes
            .iter()
            .filter_map(|(_, pane)| self.desktop_pane_to_settings(pane))
            .collect()
    }

    fn desktop_pane_to_settings(&self, pane: &DesktopPane) -> Option<DesktopWorkspacePaneSettings> {
        match pane {
            DesktopPane::Browser(tab_id) => {
                let index = self
                    .app
                    .workspace
                    .browser_tabs
                    .iter()
                    .position(|tab| tab.id == *tab_id)?;
                Some(DesktopWorkspacePaneSettings {
                    kind: DesktopWorkspacePaneKind::Browser,
                    index,
                })
            }
            DesktopPane::Conversation(conversation_id) => {
                let index = self
                    .app
                    .workspace
                    .conversations
                    .iter()
                    .position(|conversation| conversation.id == *conversation_id)?;
                Some(DesktopWorkspacePaneSettings {
                    kind: DesktopWorkspacePaneKind::Conversation,
                    index,
                })
            }
            #[cfg(feature = "chat-client")]
            DesktopPane::OmenChat(session_id) => {
                let index = self
                    .chat_client
                    .sessions()
                    .iter()
                    .position(|session| session.session_id == *session_id)?;
                Some(DesktopWorkspacePaneSettings {
                    kind: DesktopWorkspacePaneKind::OmenChat,
                    index,
                })
            }
        }
    }

    fn desktop_workspace_layout_settings(&self) -> Option<DesktopWorkspaceLayoutNode> {
        desktop_workspace_node_to_settings(self.workspace_panes.layout(), self)
    }

    fn remove_workspace_panes_for_missing_targets(
        &mut self,
        closing_browser_id: Option<TabId>,
        closing_conversation_id: Option<u64>,
    ) {
        let browser_ids = self
            .app
            .workspace
            .browser_tabs
            .iter()
            .map(|tab| tab.id)
            .collect::<std::collections::BTreeSet<_>>();
        let conversation_ids = self
            .app
            .workspace
            .conversations
            .iter()
            .map(|conversation| conversation.id)
            .collect::<std::collections::BTreeSet<_>>();
        let stale = self
            .workspace_panes
            .iter()
            .filter_map(|(pane, kind)| {
                let missing = match kind {
                    DesktopPane::Browser(id) => {
                        Some(*id) == closing_browser_id || !browser_ids.contains(id)
                    }
                    DesktopPane::Conversation(id) => {
                        Some(*id) == closing_conversation_id || !conversation_ids.contains(id)
                    }
                    #[cfg(feature = "chat-client")]
                    DesktopPane::OmenChat(id) => self.chat_client.session(*id).is_none(),
                };
                missing.then_some(*pane)
            })
            .collect::<Vec<_>>();

        for pane in stale {
            if self.workspace_panes.len() <= 1 {
                break;
            }
            self.close_workspace_pane(pane);
        }
    }

    fn view(&self) -> Element<'_, Message> {
        if self.shutdown_requested {
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
            tooltip_icon_button(ICON_STATUS_MENU, "Menu", Message::ToggleNavigation),
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

        let content = match self.app.workspace.active_section {
            WorkspaceSection::Browser | WorkspaceSection::Messages => {
                self.browser_messages_workspace_view()
            }
            WorkspaceSection::Directory => self.directory_view(),
            WorkspaceSection::Identities => self.identities_view(),
            WorkspaceSection::Interfaces => self.interfaces_view(),
            WorkspaceSection::Monitoring => self.monitoring_view(),
            WorkspaceSection::Settings => self.settings_view(),
            WorkspaceSection::Diagnostics => self.diagnostics_view(),
            WorkspaceSection::Logs => self.logs_view(),
            WorkspaceSection::Plugins => self.plugins_view(),
            WorkspaceSection::Help => self.help_view(),
        };

        let content = if let Some(prompt) = &self.external_link_prompt {
            column![self.external_link_prompt_view(prompt), content]
                .spacing(10)
                .into()
        } else {
            content
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
            .height(Length::Fixed(f32::from(ui_size(44))));
        let workspace = if self.navigation_open {
            row![self.navigation_sidebar(), content_card]
                .spacing(DESKTOP_PANEL_PADDING)
                .height(Length::Fill)
        } else {
            row![content_card].height(Length::Fill)
        };

        let shell: Element<'_, Message> = container(
            column![workspace, status_strip]
                .spacing(DESKTOP_PANEL_PADDING)
                .padding(DESKTOP_SHELL_PADDING),
        )
        .style(shell_container_style)
        .width(Length::Fill)
        .height(Length::Fill)
        .into();

        shell
    }

    fn footer_lxmf_unread_counts(&self) -> (u32, u32) {
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
                    omen_button(section.label(), Message::SwitchSection(*section))
                } else {
                    subtle_button(section.label(), Message::SwitchSection(*section))
                };
                nav.push(button)
            });

        container(
            column![
                sections,
                subtle_button("Hide Menu", Message::ToggleNavigation),
            ]
            .spacing(DESKTOP_PANEL_PADDING),
        )
        .style(card_container_style)
        .padding(DESKTOP_PANEL_PADDING)
        .width(Length::Shrink)
        .height(Length::Fill)
        .into()
    }

    fn external_link_prompt_view(&self, prompt: &ExternalLinkPrompt) -> Element<'_, Message> {
        let browsers = self.external_browsers.iter().enumerate().fold(
            row![].spacing(8),
            |row, (index, browser)| {
                let label = if Some(browser.command.as_str())
                    == self
                        .app
                        .settings
                        .clearweb
                        .preferred_external_browser_command
                        .as_deref()
                {
                    format!("{} *", browser.label)
                } else {
                    browser.label.clone()
                };
                row.push(subtle_button_owned(
                    label,
                    Message::OpenExternalLinkWith(index),
                ))
            },
        );
        let browser_commands =
            self.external_browsers
                .iter()
                .fold(column![].spacing(3), |column, browser| {
                    let label = if Some(browser.command.as_str())
                        == self
                            .app
                            .settings
                            .clearweb
                            .preferred_external_browser_command
                            .as_deref()
                    {
                        format!("{} *", browser.label)
                    } else {
                        browser.label.clone()
                    };
                    column.push(
                        text(format!("{label}: {}", browser.command))
                            .size(ui_size(12))
                            .wrapping(Wrapping::WordOrGlyph)
                            .width(Length::Fill),
                    )
                });
        let source = prompt
            .source_tab
            .map(|tab| format!("tab {tab}"))
            .unwrap_or_else(|| "active tab".into());
        let proxy_status = if self.app.settings.clearweb.socks_proxy_enabled {
            if let Some((host, port)) = &self.clearweb_proxy_endpoint {
                format!("SOCKS5 preference: {host}:{port} (proxy detected)")
            } else {
                format!(
                    "SOCKS5 preference: {}:{} or :9150 (proxy not detected)",
                    self.app.settings.clearweb.socks_proxy_host,
                    self.app.settings.clearweb.socks_proxy_port
                )
            }
        } else {
            "SOCKS5 preference: disabled".into()
        };

        container(
            column![
                row![
                    text("Open external URL").size(ui_size(16)),
                    omen_button("Copy URL", Message::CopyExternalLinkUrl),
                    subtle_button("X", Message::DismissExternalLinkPrompt)
                ]
                .spacing(12)
                .wrap(),
                wrapped_text_owned(format!("from {source}: {}", prompt.url), 13),
                wrapped_text_owned(proxy_status, 13),
                wrapped_text_owned(
                    "Tor Browser is handled by Copy URL so an already-running Tor profile is not disturbed. Other detected browsers can be launched below.",
                    13,
                ),
                browsers.wrap(),
                browser_commands,
            ]
            .spacing(8),
        )
        .style(status_container_style)
        .padding(10)
        .width(Length::Fill)
        .into()
    }

    fn startup_status_card(&self) -> Element<'_, Message> {
        let readiness = self.app.native_reticulum_readiness();
        let steps = native_setup_steps(&self.app);
        let completed = steps.iter().filter(|step| step.ready).count();
        let browser_address = self.app.active_browser_tab().address_input.clone();
        let rows = steps
            .into_iter()
            .fold(column![].spacing(3), |column, step| {
                column.push(
                    text(format!(
                        "{}: {} - {}",
                        step.title,
                        if step.ready { "ready" } else { "needs action" },
                        step.detail
                    ))
                    .size(ui_size(13)),
                )
            });
        let title = format!("Startup Status ({completed}/6 ready)");
        let blocker = readiness
            .issues
            .first()
            .map(|issue| format!("blocking live networking: {issue}"))
            .unwrap_or_else(|| "identity/runtime bootstrap has no reported blockers".into());
        let content = column![
            row![
                text(title).size(ui_size(18)),
                text(format!(
                    "compiled={} configured={} backend={:?} connected={}",
                    readiness.compiled,
                    readiness.configured,
                    self.app.settings.runtime_backend,
                    self.app.runtime_status.connected
                ))
                .size(ui_size(14)),
            ]
            .spacing(12),
            text(readiness.summary).size(ui_size(14)),
            text(blocker).size(ui_size(14)),
            rows,
            action_grid(
                vec![
                    omen_button("Retry Startup", Message::StartNativeRuntime),
                    omen_button("Auto Configure", Message::NativeQuickstart),
                    subtle_button("Create Identity", Message::CreateIdentity),
                    subtle_button("Use Native", Message::SelectNativeBackend),
                    subtle_button("Add TCP", Message::CreateTcpClientInterface),
                    subtle_button(
                        "Interfaces",
                        Message::SwitchSection(WorkspaceSection::Interfaces)
                    ),
                ],
                6,
            ),
            setup_tcp_client_editor(&self.app),
            {
                let field_editor_active = self.app.active_browser_field_editor().is_some();
                let input: Element<'_, Message> = if field_editor_active {
                    inert_address_display(browser_address.clone())
                } else {
                    text_input("destination:/path", &browser_address)
                        .on_input(Message::AddressChanged)
                        .on_submit(Message::OpenSetupAddress)
                        .width(Length::Fill)
                        .into()
                };
                row![
                    text("Open live NomadNet").size(ui_size(14)),
                    input,
                    omen_button("Open Address", Message::OpenSetupAddress),
                ]
                .spacing(8)
                .wrap()
            },
            action_grid(
                vec![
                    subtle_button(
                        "Directory",
                        Message::SwitchSection(WorkspaceSection::Directory)
                    ),
                    subtle_button(
                        "Diagnostics",
                        Message::SwitchSection(WorkspaceSection::Diagnostics)
                    ),
                    subtle_button("Preflight", Message::NativePreflight),
                    subtle_button("Live Probe", Message::NativeSmokeLiveProbe),
                    omen_button("Live Fetch", Message::NativeLiveFetchValidate),
                ],
                5,
            ),
        ]
        .spacing(5);

        let styled = container(content)
            .padding(10)
            .width(Length::Fill)
            .style(if readiness.ready {
                status_container_style
            } else {
                warning_container_style
            });
        styled.into()
    }

    fn browser_messages_workspace_view(&self) -> Element<'_, Message> {
        let controls = action_grid(self.workspace_primary_buttons(), 5);
        let hidden_workspace_panes = self.hidden_workspace_pane_buttons();
        let hidden_conversation_panes = self.hidden_conversation_pane_buttons();
        #[cfg(feature = "chat-client")]
        let omenchat_opener = row![
            text_input(
                "omenchat://<destination hash>",
                self.omenchat_server_entry.as_str()
            )
            .size(ui_size(14))
            .padding(8)
            .width(Length::Fill)
            .on_input(Message::OmenChatServerEntryChanged)
            .on_submit(Message::OpenOmenChatServerEntry),
            omen_button("Open", Message::OpenOmenChatServerEntry),
        ]
        .spacing(8);

        let grid = pane_grid(&self.workspace_panes, |pane, kind, is_maximized| {
            let title = self.workspace_pane_title(kind);
            let subtitle = self.workspace_pane_subtitle(kind);
            let focused = pane == self.active_workspace_pane;
            let controls = if is_maximized {
                let row = row![].spacing(6);
                #[cfg(feature = "chat-client-rns")]
                let row = if let DesktopPane::OmenChat(session_id) = kind {
                    row.push(tooltip_icon_button(
                        ICON_OMENCHAT_PATH,
                        "Request path",
                        Message::RequestOmenChatPath(*session_id),
                    ))
                    .push(tooltip_omen_icon_button(
                        ICON_OMENCHAT_RECONNECT,
                        "Reconnect",
                        Message::ReconnectOmenChatSession(*session_id),
                    ))
                } else {
                    row
                };
                row.push(tooltip_icon_button(
                    ICON_WINDOW_MAX,
                    "Restore tiled panes",
                    Message::WorkspacePaneRestore,
                ))
                .wrap()
            } else {
                let row = row![].spacing(6);
                #[cfg(feature = "chat-client-rns")]
                let row = if let DesktopPane::OmenChat(session_id) = kind {
                    row.push(tooltip_icon_button(
                        ICON_OMENCHAT_PATH,
                        "Request path",
                        Message::RequestOmenChatPath(*session_id),
                    ))
                    .push(tooltip_omen_icon_button(
                        ICON_OMENCHAT_RECONNECT,
                        "Reconnect",
                        Message::ReconnectOmenChatSession(*session_id),
                    ))
                } else {
                    row
                };
                let mut row = row
                    .push(tooltip_icon_button(
                        ICON_WINDOW_MAX,
                        "Maximize pane",
                        Message::WorkspacePaneMaximize(pane),
                    ))
                    .push(tooltip_icon_button(
                        ICON_WINDOW_HIDE,
                        "Close pane to restore tabs",
                        Message::WorkspacePaneClose(pane),
                    ));
                row = match kind {
                    DesktopPane::Browser(tab_id) => row.push(tooltip_warning_icon_button(
                        ICON_WINDOW_CLOSE,
                        "Delete browser tab",
                        Message::CloseBrowserPaneTab(*tab_id),
                    )),
                    DesktopPane::Conversation(conversation_id) => {
                        row.push(tooltip_warning_icon_button(
                            ICON_WINDOW_CLOSE,
                            "Delete conversation history",
                            Message::CloseConversationPaneTab(*conversation_id),
                        ))
                    }
                    #[cfg(feature = "chat-client")]
                    DesktopPane::OmenChat(session_id) => row.push(tooltip_warning_icon_button(
                        ICON_WINDOW_CLOSE,
                        "Disconnect and close chat",
                        Message::CloseOmenChatSession(*session_id),
                    )),
                };
                row.wrap()
            };
            let compact_controls = if is_maximized {
                let row = row![].spacing(6);
                #[cfg(feature = "chat-client-rns")]
                let row = if let DesktopPane::OmenChat(session_id) = kind {
                    row.push(tooltip_icon_button(
                        ICON_OMENCHAT_PATH,
                        "Request path",
                        Message::RequestOmenChatPath(*session_id),
                    ))
                    .push(tooltip_omen_icon_button(
                        ICON_OMENCHAT_RECONNECT,
                        "Reconnect",
                        Message::ReconnectOmenChatSession(*session_id),
                    ))
                } else {
                    row
                };
                row.push(tooltip_icon_button(
                    ICON_WINDOW_MAX,
                    "Restore tiled panes",
                    Message::WorkspacePaneRestore,
                ))
                .wrap()
            } else {
                let row = row![].spacing(6);
                #[cfg(feature = "chat-client-rns")]
                let row = if let DesktopPane::OmenChat(session_id) = kind {
                    row.push(tooltip_icon_button(
                        ICON_OMENCHAT_PATH,
                        "Request path",
                        Message::RequestOmenChatPath(*session_id),
                    ))
                    .push(tooltip_omen_icon_button(
                        ICON_OMENCHAT_RECONNECT,
                        "Reconnect",
                        Message::ReconnectOmenChatSession(*session_id),
                    ))
                } else {
                    row
                };
                let mut row = row
                    .push(tooltip_icon_button(
                        ICON_WINDOW_MAX,
                        "Maximize pane",
                        Message::WorkspacePaneMaximize(pane),
                    ))
                    .push(tooltip_icon_button(
                        ICON_WINDOW_HIDE,
                        "Close pane to restore tabs",
                        Message::WorkspacePaneClose(pane),
                    ));
                row = match kind {
                    DesktopPane::Browser(tab_id) => row.push(tooltip_warning_icon_button(
                        ICON_WINDOW_CLOSE,
                        "Delete browser tab",
                        Message::CloseBrowserPaneTab(*tab_id),
                    )),
                    DesktopPane::Conversation(conversation_id) => {
                        row.push(tooltip_warning_icon_button(
                            ICON_WINDOW_CLOSE,
                            "Delete conversation history",
                            Message::CloseConversationPaneTab(*conversation_id),
                        ))
                    }
                    #[cfg(feature = "chat-client")]
                    DesktopPane::OmenChat(session_id) => row.push(tooltip_warning_icon_button(
                        ICON_WINDOW_CLOSE,
                        "Disconnect and close chat",
                        Message::CloseOmenChatSession(*session_id),
                    )),
                };
                row.wrap()
            };
            let title_content = container(
                column![
                    row![
                        text(if focused { "*" } else { " " }).size(ui_size(13)),
                        text(title)
                            .size(ui_size(15))
                            .width(Length::Fill)
                            .wrapping(Wrapping::Word),
                    ]
                    .spacing(6)
                    .width(Length::Fill),
                    text(subtitle.unwrap_or_default())
                        .size(ui_size(12))
                        .width(Length::Fill)
                        .wrapping(Wrapping::Word),
                ]
                .spacing(2)
                .width(Length::Fill),
            )
            .width(Length::Fill)
            .clip(true);
            let title_bar = pane_grid::TitleBar::new(title_content)
                .controls(pane_grid::Controls::dynamic(controls, compact_controls))
                .padding(8)
                .always_show_controls()
                .style(pane_title_container_style);

            pane_grid::Content::new(
                container(self.workspace_pane_body(kind))
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .clip(true),
            )
            .title_bar(title_bar)
            .style(workspace_pane_container_style)
        })
        .spacing(8)
        .on_click(Message::WorkspacePaneClicked)
        .on_drag(Message::WorkspacePaneDragged)
        .on_resize(8, Message::WorkspacePaneResized);

        #[cfg(feature = "chat-client")]
        let content = column![
            controls,
            hidden_workspace_panes,
            hidden_conversation_panes,
            omenchat_opener,
            grid
        ]
        .spacing(8)
        .height(Length::Fill)
        .width(Length::Fill)
        .into();
        #[cfg(not(feature = "chat-client"))]
        let content = column![
            controls,
            hidden_workspace_panes,
            hidden_conversation_panes,
            grid
        ]
        .spacing(8)
        .height(Length::Fill)
        .width(Length::Fill)
        .into();
        content
    }

    fn workspace_primary_buttons(&self) -> Vec<Button<'_, Message>> {
        let controls = vec![
            omen_button("New Browser", Message::NewBrowserTab),
            omen_button("New Conversation", Message::NewConversationPane),
            subtle_button(
                "Directory",
                Message::SwitchSection(WorkspaceSection::Directory),
            ),
        ];
        #[cfg(feature = "chat-client")]
        {
            let mut controls = controls;
            controls.insert(2, omen_button("New Chat", Message::NewOmenChatPane));
            controls
        }
        #[cfg(not(feature = "chat-client"))]
        {
            controls
        }
    }

    fn hidden_workspace_pane_buttons(&self) -> Element<'_, Message> {
        let buttons = self
            .hidden_browser_panes()
            .into_iter()
            .map(|(tab_id, label)| {
                restore_pane_button(
                    ICON_RESTORE_BROWSER,
                    label,
                    Message::RestoreDesktopPane(DesktopPane::Browser(tab_id)),
                    false,
                )
            })
            .chain({
                #[cfg(feature = "chat-client")]
                {
                    self.hidden_omenchat_panes()
                        .into_iter()
                        .map(|(session_id, label, unread)| {
                            restore_pane_button(
                                ICON_RESTORE_CHAT,
                                label,
                                Message::RestoreDesktopPane(DesktopPane::OmenChat(session_id)),
                                unread,
                            )
                        })
                        .collect::<Vec<_>>()
                }
                #[cfg(not(feature = "chat-client"))]
                {
                    Vec::new()
                }
            })
            .collect::<Vec<_>>();
        if buttons.is_empty() {
            return text("").size(ui_size(1)).into();
        }
        action_grid(buttons, 5)
    }

    fn hidden_conversation_pane_buttons(&self) -> Element<'_, Message> {
        let buttons = self
            .hidden_conversation_panes()
            .into_iter()
            .map(|(conversation_id, label, unread)| {
                restore_pane_button(
                    ICON_RESTORE_MESSAGES,
                    label,
                    Message::RestoreDesktopPane(DesktopPane::Conversation(conversation_id)),
                    unread,
                )
            })
            .collect::<Vec<_>>();
        if buttons.is_empty() {
            return text("").size(ui_size(1)).into();
        }
        action_grid(buttons, 5)
    }

    fn workspace_pane_title(&self, kind: &DesktopPane) -> String {
        match kind {
            DesktopPane::Browser(tab_id) => self
                .app
                .workspace
                .browser_tabs
                .iter()
                .find(|tab| tab.id == *tab_id)
                .map(|tab| format!("{} - Browser", compact_label(&tab.title, 32)))
                .unwrap_or_else(|| "closed tab - Browser".into()),
            DesktopPane::Conversation(conversation_id) => self
                .app
                .workspace
                .conversations
                .iter()
                .find(|conversation| conversation.id == *conversation_id)
                .map(|conversation| {
                    format!("{} - Messages", compact_label(&conversation.peer_label, 32))
                })
                .unwrap_or_else(|| "closed conversation - Messages".into()),
            #[cfg(feature = "chat-client")]
            DesktopPane::OmenChat(session_id) => self
                .chat_client
                .session(*session_id)
                .map(|session| {
                    format!(
                        "{} - OMENchat",
                        compact_label(&session.server.display_name, 32)
                    )
                })
                .unwrap_or_else(|| "closed session - OMENchat".into()),
        }
    }

    fn workspace_pane_subtitle(&self, kind: &DesktopPane) -> Option<String> {
        match kind {
            DesktopPane::Browser(_) => None,
            DesktopPane::Conversation(conversation_id) => self
                .app
                .workspace
                .conversations
                .iter()
                .find(|conversation| conversation.id == *conversation_id)
                .and_then(|conversation| {
                    let peer_hash = printable_label(conversation.peer_hash.trim());
                    (!peer_hash.is_empty()).then(|| format!("peer: {peer_hash}"))
                }),
            #[cfg(feature = "chat-client")]
            DesktopPane::OmenChat(session_id) => {
                self.chat_client.session(*session_id).map(|session| {
                    let room = compact_label(&session.active_room.name, 18);
                    let status = compact_label(&session.status, 42);
                    format!(
                        "room: #{} | {} users | {}",
                        room,
                        unique_chat_users(&session.users).len(),
                        status
                    )
                })
            }
        }
    }

    fn workspace_pane_body(&self, kind: &DesktopPane) -> Element<'_, Message> {
        match kind {
            DesktopPane::Browser(tab_id) => self.browser_view_for_tab(*tab_id),
            DesktopPane::Conversation(conversation_id) => {
                self.messages_view_for_conversation(*conversation_id)
            }
            #[cfg(feature = "chat-client")]
            DesktopPane::OmenChat(session_id) => self.omenchat_view_for_session(*session_id),
        }
    }

    fn active_conversation_pane_is_visible(&self) -> bool {
        let Some(conversation) = self
            .app
            .workspace
            .conversations
            .get(self.app.workspace.active_conversation)
        else {
            return false;
        };
        self.find_workspace_pane(&DesktopPane::Conversation(conversation.id))
            .is_some()
    }

    fn hidden_browser_panes(&self) -> Vec<(TabId, String)> {
        self.app
            .workspace
            .browser_tabs
            .iter()
            .filter(|tab| {
                self.find_workspace_pane(&DesktopPane::Browser(tab.id))
                    .is_none()
            })
            .map(|tab| (tab.id, compact_label(&tab.title, 18)))
            .collect()
    }

    fn hidden_conversation_panes(&self) -> Vec<(u64, String, bool)> {
        self.app
            .workspace
            .conversations
            .iter()
            .filter(|conversation| !Self::conversation_is_empty_restore_placeholder(conversation))
            .filter(|conversation| {
                self.find_workspace_pane(&DesktopPane::Conversation(conversation.id))
                    .is_none()
            })
            .map(|conversation| {
                let unread = conversation.thread.unread_count > 0
                    || conversation
                        .thread
                        .messages
                        .iter()
                        .any(|message| message.unread);
                (
                    conversation.id,
                    compact_label(&conversation.peer_label, 18),
                    unread,
                )
            })
            .collect()
    }

    fn conversation_is_empty_restore_placeholder(
        conversation: &crate::messaging::Conversation,
    ) -> bool {
        conversation.peer_hash.trim().is_empty()
            && conversation
                .peer_label
                .trim()
                .eq_ignore_ascii_case("New Conversation")
            && conversation.draft_title.trim().is_empty()
            && conversation.draft_body.trim().is_empty()
            && conversation.attachments.is_empty()
            && conversation.thread.messages.is_empty()
    }

    #[cfg(feature = "chat-client")]
    fn hidden_omenchat_panes(&self) -> Vec<(ChatSessionId, String, bool)> {
        self.chat_client
            .sessions()
            .iter()
            .filter(|session| {
                self.find_workspace_pane(&DesktopPane::OmenChat(session.session_id))
                    .is_none()
            })
            .map(|session| {
                let unread = session.active_room.unread > 0
                    || session.rooms.iter().any(|room| room.unread > 0);
                (
                    session.session_id,
                    compact_label(&session.server.display_name, 18),
                    unread,
                )
            })
            .collect()
    }

    #[cfg(feature = "chat-client")]
    fn omenchat_view_for_session(&self, session_id: ChatSessionId) -> Element<'_, Message> {
        let Some(session) = self.chat_client.session(session_id) else {
            return text("This OMENchat session was closed.")
                .size(ui_size(14))
                .into();
        };

        let room_list = if self.omenchat_rooms_visible {
            let mut rooms = session.rooms.clone();
            if !rooms
                .iter()
                .any(|room| room.room_id == session.active_room.room_id)
            {
                rooms.push(session.active_room.clone());
            }
            rooms.sort_by(|left, right| {
                left.name
                    .cmp(&right.name)
                    .then(left.room_id.cmp(&right.room_id))
            });
            let mut room_column = column![].spacing(8);
            room_column = room_column.push(text("Rooms").size(ui_size(16)));
            for room in rooms {
                let unread = if room.unread > 0 {
                    format!(" ({})", room.unread)
                } else {
                    String::new()
                };
                let label = if room.room_id == session.active_room.room_id {
                    format!("[#{}]", room.name)
                } else {
                    format!("#{}{}", room.name, unread)
                };
                let message = Message::JoinOmenChatRoom {
                    session_id: session.session_id,
                    room: room.name.clone(),
                };
                room_column = room_column.push(if room.unread > 0 {
                    warning_button_owned(label, message)
                } else {
                    subtle_button_owned(label, message)
                });
            }
            room_column
                .push(subtle_button(
                    "Load Older",
                    Message::LoadOlderOmenChatHistory(session.session_id),
                ))
                .width(Length::Shrink)
        } else {
            column![].width(Length::Shrink)
        };

        let mut timeline = column![].spacing(8).width(Length::Fill);
        for group in chat_timeline_groups(session) {
            let header = row![
                text(group.actor).size(ui_size(12)),
                text(chat_event_time_label(group.at_unix)).size(ui_size(11)),
            ]
            .spacing(8)
            .wrap();
            let mut group_content = column![header].spacing(1).width(Length::Fill);
            for body in group.bodies {
                let media_hints = omenchat_media_hints(
                    &body.text,
                    &self.app.settings.clearweb,
                    self.clearweb_proxy_endpoint.as_ref(),
                    self.app.directory_service.trust_level(
                        &session.server.destination,
                        Some(&session.server.display_name),
                    ) == crate::directory::TrustLevel::Trusted,
                    &self.omenchat_media_cache,
                );
                let mut line = text(body.text.clone()).size(ui_size(14));
                if body.is_action {
                    line = line.font(Font {
                        style: FontStyle::Italic,
                        ..desktop_ui_font()
                    });
                }
                if let Some(upload) = body.upload.as_ref() {
                    let key = omenchat_upload_cache_key(upload.session_id, &upload.resource_id);
                    let mut upload_line = row![line].spacing(6).align_y(iced::Alignment::Center);
                    match self.omenchat_media_cache.get(&key) {
                        Some(OmenChatMediaLoadState::Cached { path, .. }) => {
                            upload_line = upload_line.push(inline_icon_button_owned(
                                ICON_OPEN,
                                "Open attachment",
                                Message::OpenCachedOmenChatMedia(path.clone()),
                            ));
                        }
                        Some(OmenChatMediaLoadState::Loading {
                            message,
                            received,
                            total,
                        }) => {
                            upload_line = upload_line.push(
                                text(omenchat_upload_state_label(
                                    &OmenChatMediaLoadState::Loading {
                                        message: message.clone(),
                                        received: *received,
                                        total: *total,
                                    },
                                ))
                                .size(ui_size(11)),
                            );
                        }
                        Some(OmenChatMediaLoadState::Failed { message }) => {
                            upload_line = upload_line.push(
                                text(omenchat_upload_state_label(
                                    &OmenChatMediaLoadState::Failed {
                                        message: message.clone(),
                                    },
                                ))
                                .size(ui_size(11)),
                            );
                            upload_line = upload_line.push(inline_icon_button_owned(
                                ICON_DOWNLOAD,
                                "Retry attachment download",
                                Message::FetchOmenChatUploadResource {
                                    session_id: upload.session_id,
                                    resource_id: upload.resource_id.clone(),
                                },
                            ));
                        }
                        None => {
                            upload_line = upload_line.push(inline_icon_button_owned(
                                ICON_DOWNLOAD,
                                "Download attachment",
                                Message::FetchOmenChatUploadResource {
                                    session_id: upload.session_id,
                                    resource_id: upload.resource_id.clone(),
                                },
                            ));
                        }
                    }
                    group_content = group_content.push(upload_line.wrap());
                } else if let Some(resend) = body.resend {
                    group_content = group_content.push(
                        row![
                            line,
                            inline_icon_button_owned(
                                ICON_OMENCHAT_RECONNECT,
                                "Resend message",
                                Message::ResendOmenChatLocalEcho {
                                    session_id: resend.session_id,
                                    room_id: resend.room_id,
                                    event_id: resend.event_id,
                                    body: resend.body,
                                    action: resend.action,
                                },
                            )
                        ]
                        .spacing(6)
                        .align_y(iced::Alignment::Center)
                        .wrap(),
                    );
                } else {
                    group_content = group_content.push(line);
                }
                for hint in media_hints {
                    let mut hint_row = row![].spacing(8).align_y(iced::Alignment::Center);
                    let mut has_hint_row = false;
                    if !hint.label.is_empty() {
                        hint_row = hint_row.push(text(hint.label).size(ui_size(11)));
                        has_hint_row = true;
                    }
                    if let Some(url) = hint.open_url {
                        hint_row = hint_row.push(inline_icon_button_owned(
                            ICON_OPEN,
                            "Open",
                            Message::PromptExternalUrl(url),
                        ));
                        has_hint_row = true;
                    }
                    if let Some(path) = hint.open_path {
                        hint_row = hint_row.push(inline_icon_button_owned(
                            ICON_OPEN,
                            "Open",
                            Message::OpenCachedOmenChatMedia(path),
                        ));
                        has_hint_row = true;
                    }
                    if let Some(url) = hint.load_url {
                        hint_row = hint_row.push(inline_icon_button_owned(
                            ICON_OPEN,
                            "Load",
                            Message::LoadOmenChatMedia(url),
                        ));
                        has_hint_row = true;
                    }
                    if has_hint_row {
                        group_content = group_content.push(hint_row.wrap());
                    }
                    if let Some(path) = hint.image_path {
                        let media = omenchat_inline_media_element(
                            &path,
                            hint.animated,
                            self.omenchat_gif_frames.get(&path),
                        );
                        group_content = group_content.push(container(media).width(Length::Fill));
                        if let Some(caption) = hint.caption {
                            group_content = group_content
                                .push(text(caption).size(ui_size(11)).width(Length::Fill));
                        }
                    }
                }
                if let Some(upload) = body.upload {
                    let key = omenchat_upload_cache_key(upload.session_id, &upload.resource_id);
                    let upload_state = self.omenchat_media_cache.get(&key).cloned();
                    if let Some(state) = upload_state.as_ref() {
                        if let Some(path) = omenchat_media_state_image_path(state) {
                            let media = omenchat_inline_media_element(
                                &path,
                                omenchat_media_state_is_animated(state),
                                self.omenchat_gif_frames.get(&path),
                            );
                            group_content =
                                group_content.push(container(media).width(Length::Fill));
                        }
                    }
                }
            }
            timeline = timeline.push(container(group_content).padding([2, 8]).width(Length::Fill));
        }

        let mut userlist = column![text("Users").size(ui_size(16))]
            .spacing(6)
            .width(Length::Fixed(82.0));
        for user in unique_chat_users(&session.users) {
            userlist = userlist.push(text(user.display_label()).size(ui_size(13)));
        }

        let draft = self
            .chat_drafts
            .get(&session.session_id)
            .map(String::as_str)
            .unwrap_or_default();
        let session_id = session.session_id;
        let active_room_id = session.active_room.room_id;
        let composer = row![
            tooltip_button(
                button(centered_toolbar_icon(ICON_MENU))
                    .on_press(Message::ToggleOmenChatRooms)
                    .padding(0)
                    .width(Length::Fixed(toolbar_icon_button_side()))
                    .height(Length::Fixed(toolbar_icon_button_side()))
                    .style(subtle_button_style),
                "Rooms"
            ),
            tooltip_button(
                button(centered_toolbar_icon(ICON_ATTACH))
                    .on_press(Message::PickOmenChatUpload(session.session_id))
                    .padding(0)
                    .width(Length::Fixed(toolbar_icon_button_side()))
                    .height(Length::Fixed(toolbar_icon_button_side()))
                    .style(subtle_button_style),
                "Attach file"
            ),
            text_input(&format!("Message #{}", session.active_room.name), draft)
                .size(ui_size(14))
                .padding(8)
                .width(Length::Fill)
                .on_input(move |value| Message::OmenChatDraftChanged { session_id, value })
                .on_submit(Message::SendOmenChatDraft(session.session_id)),
            omen_button("Send", Message::SendOmenChatDraft(session.session_id)),
        ]
        .spacing(8);

        let mut timeline_panel = column![].spacing(8).width(Length::Fill);
        if let Some(motd) = self
            .omenchat_motds
            .get(&session.session_id)
            .map(String::as_str)
            .map(str::trim)
            .filter(|motd| !motd.is_empty())
        {
            timeline_panel = timeline_panel.push(
                container(text(motd).size(ui_size(13)))
                    .padding([6, 8])
                    .width(Length::Fill)
                    .style(status_container_style),
            );
        }
        timeline_panel = timeline_panel.push(
            app_scrollable(timeline)
                .id(omenchat_scroll_id(session.session_id, active_room_id))
                .on_scroll(move |viewport: Viewport| Message::OmenChatScrolled {
                    session_id,
                    room_id: active_room_id,
                    offset: sanitize_scroll_offset(viewport.relative_offset()),
                })
                .height(Length::Fill),
        );
        if self.omenchat_is_viewing_history(session.session_id, active_room_id) {
            timeline_panel = timeline_panel.push(
                container(
                    column![
                        text("You're viewing older messages").size(ui_size(12)),
                        omen_button(
                            "Jump To Present",
                            Message::JumpOmenChatToPresent {
                                session_id: session.session_id,
                                room_id: active_room_id,
                            },
                        )
                    ]
                    .spacing(6)
                    .width(Length::Fill),
                )
                .padding([6, 8])
                .width(Length::Fill)
                .style(status_container_style),
            );
        }

        column![
            row![room_list, timeline_panel, userlist]
                .spacing(10)
                .height(Length::Fill),
            composer
        ]
        .spacing(10)
        .padding(10)
        .height(Length::Fill)
        .into()
    }

    fn browser_view_for_tab(&self, tab_id: TabId) -> Element<'_, Message> {
        let Some((index, tab)) = self
            .app
            .workspace
            .browser_tabs
            .iter()
            .enumerate()
            .find(|(_, tab)| tab.id == tab_id)
        else {
            return text("This browser tab was closed.")
                .size(ui_size(14))
                .into();
        };
        let toolbar = row![
            tooltip_icon_button(ICON_BACK, "Back", Message::BrowserPaneBack(tab_id)),
            tooltip_icon_button(ICON_FORWARD, "Forward", Message::BrowserPaneForward(tab_id)),
            tooltip_icon_button(ICON_RELOAD, "Reload", Message::ReloadBrowserPane(tab_id)),
            tooltip_warning_icon_button(ICON_STOP, "Stop", Message::StopBrowserPaneTask(tab_id)),
            tooltip_omen_icon_button(
                ICON_REQUEST_PATH,
                "Request Path",
                Message::WarmBrowserPanePath(tab_id)
            ),
            tooltip_icon_button(
                IDENTIFY_ICON,
                "Identify",
                Message::ToggleBrowserPaneIdentify(tab_id)
            ),
            tooltip_icon_button(
                ICON_CAPTURE,
                "Capture",
                Message::CaptureBrowserPaneRender(tab_id)
            ),
            tooltip_icon_button(
                ICON_DIAGNOSTICS,
                "Diagnostics",
                Message::BrowserPanePathDiagnostics(tab_id)
            ),
        ]
        .spacing(8)
        .wrap();

        let request_state = self.browser_request_state_view_for_tab(tab);
        let warning = self.browser_live_warning_banner_for_tab(tab_id, tab);
        let active_field_cursor = (index == self.app.workspace.active_browser)
            .then(|| self.app.active_browser_field_editor())
            .flatten();
        let address = self.browser_address_row(tab_id, &tab.address_input);

        let metadata_document = tab.session.current_document.as_ref();
        let viewport_background = metadata_document
            .as_ref()
            .and_then(|document| document.metadata.get("bg"))
            .and_then(|color| color_from_style(Some(color.as_str())));
        let page = self.browser_page_for_tab(tab_id, tab, active_field_cursor);
        let viewport_border = metadata_document
            .as_ref()
            .and_then(|document| document.metadata.get("fg"))
            .and_then(|color| color_from_style(Some(color.as_str())));
        let browser_body = container(page)
            .style(move |theme| {
                browser_viewport_container_style(theme, viewport_background, viewport_border)
            })
            .padding(8)
            .width(Length::Fill)
            .height(Length::Fill);

        container(column![toolbar, address, request_state, warning, browser_body].spacing(6))
            .padding(8)
            .height(Length::Fill)
            .into()
    }

    fn browser_address_row<'a>(
        &self,
        tab_id: TabId,
        address_input: &'a str,
    ) -> Element<'a, Message> {
        let input: Element<'a, Message> = text_input("destination:/path", address_input)
            .on_input(move |value| Message::BrowserPaneAddressChanged { tab_id, value })
            .on_submit(Message::OpenBrowserPaneAddress(tab_id))
            .width(Length::Fill)
            .into();
        row![
            input,
            omen_button("Open", Message::OpenBrowserPaneAddress(tab_id)),
            subtle_button("Top", Message::BrowserPaneTop(tab_id)),
        ]
        .spacing(8)
        .width(Length::Fill)
        .into()
    }

    fn browser_page_for_tab<'a>(
        &'a self,
        tab_id: TabId,
        tab: &'a crate::app::BrowserTab,
        active_field_cursor: Option<BrowserFieldEditor>,
    ) -> Element<'a, Message> {
        let initial_document = self
            .app
            .browser_document_for_tab_width(tab, tab.viewport_width.max(1));
        let row_field_cursor = active_field_cursor.clone();
        nomadnet_page_with_row_renderer(
            NomadNetPageProps {
                document: initial_document.as_ref(),
                rendered_rows: None,
                fallback: tab.current_page.as_ref().map(|page| page.markup.as_str()),
                scroll_offset: tab.scroll.offset,
                zoom_percent: tab.micron_zoom_percent,
                focused_control: tab
                    .focused_control
                    .as_ref()
                    .map(|control| control.name.as_str()),
                focused_link: tab.focused_link.as_ref().map(|link| link.target.as_str()),
                field_cursor: active_field_cursor
                    .as_ref()
                    .map(|editor| (editor.name.as_str(), editor.cursor_byte)),
            },
            move |viewport_width| {
                self.app
                    .browser_rendered_rows_for_tab_width_with_field_cursor(
                        tab,
                        viewport_width,
                        row_field_cursor
                            .as_ref()
                            .map(|editor| (editor.name.as_str(), editor.cursor_byte)),
                    )
            },
            move |page| Message::PageForTab { tab_id, page },
        )
    }

    fn browser_request_state_view_for_tab(
        &self,
        tab: &crate::app::BrowserTab,
    ) -> Element<'_, Message> {
        let Some(preview) = tab.request_preview.as_ref() else {
            return text("").size(ui_size(1)).into();
        };
        if matches!(preview.status, BrowserRequestStatus::Completed) {
            return text("").size(ui_size(1)).into();
        }
        let status_style = match preview.status {
            BrowserRequestStatus::Pending => warning_container_style,
            BrowserRequestStatus::Failed => warning_container_style,
            BrowserRequestStatus::Preview | BrowserRequestStatus::Completed => {
                status_container_style
            }
        };

        let show_path_actions = browser_request_preview_has_path_actions(tab, preview);
        let mut body = column![row![
            text(format!(
                "Request {} -> {}",
                request_status_label(&preview.status),
                preview.target
            ))
            .size(ui_size(14))
            .width(Length::Fill),
            subtle_button("Close", Message::DismissBrowserPaneRequest(tab.id)),
        ]
        .spacing(8)]
        .spacing(3);
        if show_path_actions || !matches!(preview.status, BrowserRequestStatus::Pending) {
            body = body.push(text(request_preview_line(tab, preview)).size(ui_size(12)));
        }
        if show_path_actions {
            let mut actions = vec![
                omen_button("Request Path", Message::WarmBrowserPanePath(tab.id)),
                subtle_button("Diag", Message::BrowserPanePathDiagnostics(tab.id)),
            ];
            if browser_request_preview_retry_ready(tab, preview) {
                actions.insert(
                    1,
                    omen_button("Retry", Message::RetryBrowserPaneAfterPath(tab.id)),
                );
            }
            body = body.push(action_grid(actions, 3));
        }

        container(body)
            .style(status_style)
            .padding(6)
            .width(Length::Fill)
            .into()
    }

    fn browser_live_warning_banner_for_tab(
        &self,
        tab_id: TabId,
        tab: &crate::app::BrowserTab,
    ) -> Element<'_, Message> {
        let Some(warning) = tab.live_warning.as_ref() else {
            return text("").size(ui_size(1)).into();
        };
        let visible = warning
            .visible_page
            .as_deref()
            .unwrap_or("no previous page is visible");
        let actions = action_grid(
            vec![
                subtle_button("Close", Message::DismissBrowserPaneWarning(tab_id)),
                omen_button("Request Path", Message::WarmBrowserPanePath(tab_id)),
                omen_button("Retry", Message::RetryBrowserPaneAfterPath(tab_id)),
                subtle_button("Diag", Message::BrowserPanePathDiagnostics(tab_id)),
            ],
            4,
        );

        container(
            column![
                text("Live load failed; visible page may be stale").size(ui_size(18)),
                text(format!("target: {}", warning.target)).size(ui_size(14)),
                text(format!("visible: {visible}")).size(ui_size(14)),
                text(format!("failure: {}", warning.message)).size(ui_size(14)),
                text(format!("next: {}", warning.next_action)).size(ui_size(14)),
                actions,
            ]
            .spacing(4),
        )
        .style(warning_container_style)
        .padding(10)
        .width(Length::Fill)
        .into()
    }

    fn conversation_delivery_state_line<'a>(
        &self,
        conversation: &crate::messaging::Conversation,
    ) -> Element<'a, Message> {
        if self.app.active_conversation_uses_native_lxmf() {
            text(format!("delivery: {:?}", conversation.delivery_mode))
                .size(ui_size(14))
                .into()
        } else {
            text(format!(
                "delivery: {:?} | ticket: {}",
                conversation.delivery_mode, conversation.include_ticket
            ))
            .size(ui_size(14))
            .into()
        }
    }

    fn messages_view_for_conversation(&self, conversation_id: u64) -> Element<'_, Message> {
        let Some(conversation) = self
            .app
            .workspace
            .conversations
            .iter()
            .find(|conversation| conversation.id == conversation_id)
        else {
            return text("This conversation was closed.")
                .size(ui_size(14))
                .into();
        };
        let messages = conversation
            .thread
            .messages
            .iter()
            .filter(|message| {
                !conversation
                    .dismissed_message_keys
                    .contains(&message_summary_key(message))
            })
            .rev()
            .take(CONVERSATION_VISIBLE_MESSAGES)
            .collect::<Vec<_>>();
        let messages = messages
            .into_iter()
            .rev()
            .fold(column![].spacing(10), |column, message| {
                let key = message_summary_key(message);
                column.push(message_bubble(
                    conversation.id,
                    message,
                    conversation.selected_message_key.as_deref() == Some(key.as_str()),
                ))
            });
        let selected_details = selected_message_details_card(conversation.id, conversation);
        let trust_label = if self.app.lxmf_peer_is_trusted(&conversation.peer_hash) {
            "Untrust"
        } else {
            "Trust"
        };
        let composer = section_card(
            "Write Message",
            column![
                text_input(
                    "LXMF peer destination hash",
                    conversation.peer_hash.as_str()
                )
                .on_input(move |value| Message::ConversationPanePeerChanged {
                    conversation_id,
                    value,
                })
                .width(Length::Fill),
                text_input("subject", &conversation.draft_title)
                    .on_input(move |value| Message::ConversationPaneTitleChanged {
                        conversation_id,
                        value,
                    })
                    .width(Length::Fill),
                self.conversation_body_editors
                    .get(&conversation_id)
                    .map(|editor| {
                        let editor_element: Element<'_, Message> = text_editor(editor)
                            .on_action(move |action| Message::ConversationPaneBodyEdited {
                                conversation_id,
                                action,
                            })
                            .wrapping(Wrapping::WordOrGlyph)
                            .height(Length::Fixed(112.0))
                            .into();
                        editor_element
                    })
                    .unwrap_or_else(|| {
                        text_input("message body", &conversation.draft_body)
                            .on_input(move |value| Message::ConversationPaneBodyChanged {
                                conversation_id,
                                value,
                            })
                            .width(Length::Fill)
                            .into()
                    }),
                conversation_attachment_draft_rows(conversation_id, &conversation.attachments),
                text("Enter inserts a new line. Use Send to deliver the draft.").size(ui_size(12)),
                self.conversation_delivery_state_line(conversation),
                row![
                    tooltip_button(
                        button(centered_toolbar_icon(ICON_ATTACH))
                            .on_press(Message::PickConversationAttachment(conversation_id))
                            .padding(0)
                            .width(Length::Fixed(toolbar_icon_button_side()))
                            .height(Length::Fixed(toolbar_icon_button_side()))
                            .style(subtle_button_style),
                        "Attach file",
                    ),
                    action_grid(
                        vec![
                            subtle_button(
                                "Delivery",
                                Message::ToggleConversationPaneDeliveryMode(conversation_id)
                            ),
                            omen_button(
                                "Send",
                                Message::SendConversationPaneDraft(conversation_id)
                            ),
                            subtle_button(
                                "Path",
                                Message::RequestConversationPanePeerPath(conversation_id)
                            ),
                            subtle_button("Sync Propagation", Message::SyncPropagationNow),
                            subtle_button("Sync", Message::SyncMessages),
                            subtle_button(
                                trust_label,
                                Message::ToggleConversationPaneTrust(conversation_id)
                            ),
                            subtle_button(
                                "Diag",
                                Message::ConversationPaneDiagnostics(conversation_id)
                            ),
                        ],
                        7,
                    ),
                ]
                .spacing(8)
                .wrap(),
            ]
            .spacing(8),
        );

        let message_scroll = app_scrollable(column![messages, selected_details].spacing(12))
            .id(conversation_scroll_id(conversation_id))
            .on_scroll(move |viewport: Viewport| Message::ConversationScrolled {
                conversation_id,
                offset: sanitize_scroll_offset(viewport.relative_offset()),
            })
            .height(Length::Fill)
            .width(Length::Fill);
        let mut message_area = column![message_scroll].spacing(6).height(Length::Fill);
        if self.conversation_is_viewing_history(conversation_id) {
            message_area = message_area.push(
                container(
                    column![
                        text("You're viewing older messages").size(ui_size(12)),
                        omen_button(
                            "Jump To Present",
                            Message::JumpConversationToPresent(conversation_id),
                        )
                    ]
                    .spacing(6)
                    .width(Length::Fill),
                )
                .padding([6, 8])
                .width(Length::Fill)
                .style(status_container_style),
            );
        }

        column![
            message_conversation_header(conversation),
            message_area,
            composer,
        ]
        .spacing(8)
        .padding(8)
        .into()
    }

    fn lxmf_messaging_diagnostics_card(&self) -> Element<'_, Message> {
        let diagnostics = self.app.active_lxmf_messaging_diagnostics();
        let body = diagnostics
            .lines
            .into_iter()
            .fold(column![].spacing(4), |column, line| {
                column.push(wrapped_text_owned(line, 14))
            });
        let style = match diagnostics.severity {
            LxmfMessagingDiagnosticsSeverity::Ready | LxmfMessagingDiagnosticsSeverity::Info => {
                status_container_style
            }
            LxmfMessagingDiagnosticsSeverity::Warning
            | LxmfMessagingDiagnosticsSeverity::Blocked => warning_container_style,
        };
        container(column![text(diagnostics.title).size(ui_size(20)), body].spacing(8))
            .style(style)
            .padding(12)
            .width(Length::Fill)
            .into()
    }

    fn identities_view(&self) -> Element<'_, Message> {
        let active_path = self
            .app
            .settings
            .identity_path
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "none".into());
        let active_label = self
            .app
            .settings
            .active_identity_label
            .clone()
            .unwrap_or_default();
        let active_hash = self
            .app
            .runtime_status
            .active_identity
            .as_ref()
            .map(|identity| identity.hash_hex.clone())
            .unwrap_or_else(|| "not attached".into());
        let active_storage_root = self
            .app
            .settings
            .identity_path
            .as_ref()
            .map(|path| {
                self.app
                    .paths
                    .storage_root_for_identity_path(path)
                    .display()
                    .to_string()
            })
            .unwrap_or_else(|| "none".into());
        let active_reticulum_storage = self
            .app
            .settings
            .identity_path
            .as_ref()
            .map(|path| {
                self.app
                    .paths
                    .scoped_to_identity_path(path)
                    .reticulum_storage_dir
                    .display()
                    .to_string()
            })
            .unwrap_or_else(|| "none".into());
        let managed_profiles = self.app.managed_identity_profiles();
        let has_managed_profiles = !managed_profiles.is_empty();
        let rows = managed_profiles
            .into_iter()
            .fold(column![].spacing(8), |column, profile| {
                let is_active = self
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
                let storage_paths = self.app.paths.with_identity_storage_root(
                    self.app.paths.storage_root_for_identity_profile(&profile),
                );
                column.push(
                    container(
                        column![
                            header.wrap(),
                            wrapped_text_owned(format!("identity: {}", profile.path.display()), 12),
                            wrapped_text_owned(
                                format!(
                                    "storage: {}",
                                    self.app
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
                        wrapped_text_owned(
                            format!("reticulum storage: {active_reticulum_storage}"),
                            12
                        ),
                        action_grid(
                            vec![
                                omen_button("Create Identity", Message::CreateIdentity),
                                subtle_button("Announce Now", Message::AnnounceIdentityNow),
                                subtle_button("Clear Active", Message::ClearActiveIdentity),
                                warning_button("Delete Active", Message::DeleteActiveIdentity),
                            ],
                            4,
                        ),
                        self.identity_delete_confirmation_view(),
                    ]
                    .spacing(8),
                ),
                section_card("Managed Identities", managed),
                section_card(
                    "Paths",
                    column![
                        wrapped_text_owned(format!(
                            "managed identities: {}",
                            self.app.paths.identities_dir.display()
                        ), 14),
                        wrapped_text_owned(format!(
                            "identity storage roots: {}",
                            self.app.paths.identity_storage_dir.display()
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

    fn identity_delete_confirmation_view(&self) -> Element<'_, Message> {
        if !self.identity_delete_confirming {
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

    fn directory_view(&self) -> Element<'_, Message> {
        use crate::directory::DirectoryKind;

        let active_kind = self.app.directory_state.active_kind.clone();
        let active_scope = self.app.directory_state.active_scope.clone();
        let filter = self.app.directory_state.filter.trim();
        let counts = self.app.directory_state.entries.iter().fold(
            (0usize, 0usize, 0usize, 0usize, 0usize),
            |(nodes, peers, propagation, omenchat, trusted), entry| {
                (
                    nodes
                        + usize::from(directory_entry_matches_view(
                            entry,
                            &DirectoryKind::Node,
                            &active_scope,
                            filter,
                        )),
                    peers
                        + usize::from(directory_entry_matches_view(
                            entry,
                            &DirectoryKind::Peer,
                            &active_scope,
                            filter,
                        )),
                    propagation
                        + usize::from(directory_entry_matches_view(
                            entry,
                            &DirectoryKind::Propagation,
                            &active_scope,
                            filter,
                        )),
                    omenchat
                        + usize::from(directory_entry_matches_view(
                            entry,
                            &DirectoryKind::OmenChat,
                            &active_scope,
                            filter,
                        )),
                    trusted + usize::from(entry.trusted),
                )
            },
        );
        let unknown_count = self
            .app
            .directory_state
            .entries
            .iter()
            .filter(|entry| entry.kind == DirectoryKind::Unknown)
            .count();
        let tabs = row![
            directory_tab_button("Nodes", DirectoryKind::Node, &active_kind, counts.0),
            directory_tab_button("Peers", DirectoryKind::Peer, &active_kind, counts.1),
            directory_tab_button("OMENchat", DirectoryKind::OmenChat, &active_kind, counts.3),
            directory_tab_button(
                "Propagation",
                DirectoryKind::Propagation,
                &active_kind,
                counts.2
            ),
        ]
        .spacing(8)
        .wrap();
        let scope_tabs = row![
            directory_scope_button("Live", DirectoryScope::Live, &active_scope),
            directory_scope_button("Saved", DirectoryScope::Saved, &active_scope),
            directory_scope_button("Trusted", DirectoryScope::Trusted, &active_scope),
        ]
        .spacing(8)
        .wrap();
        let mut rows = column![self.directory_group_view(
            directory_kind_title(&active_kind),
            active_kind.clone(),
            active_scope.clone(),
            directory_empty_text(&active_kind),
        )]
        .spacing(12);
        if unknown_count > 0 {
            rows = rows.push(section_card(
                format!("Unknown Announces ({unknown_count})"),
                wrapped_panel_text(
                    "Unknown announces are kept out of Nodes/Peers/Propagation until classified.",
                ),
            ));
        }
        let selected_details = self.directory_selected_details_card();

        app_scrollable(
            column![
                text("Directory").size(ui_size(28)),
                tabs,
                scope_tabs,
                row![
                    text_input(
                        "Search directory by name, destination, kind, delivery...",
                        &self.app.directory_state.filter
                    )
                    .on_input(Message::DirectoryFilterChanged)
                    .width(Length::Fill),
                    subtle_button("Clear", Message::DirectoryFilterChanged(String::new())),
                ]
                .spacing(8)
                .wrap(),
                section_card(
                    "Directory State",
                    column![
                        wrapped_text_owned(format!(
                            "entries={} visible_nodes={} visible_peers={} visible_omenchat={} visible_propagation={} trusted={}",
                            self.app.directory_state.entries.len(),
                            counts.0,
                            counts.1,
                            counts.3,
                            counts.2,
                            counts.4
                        ), 14),
                        wrapped_text_owned(format!(
                            "filter: {}",
                            if self.app.directory_state.filter.is_empty() {
                                "none"
                            } else {
                                self.app.directory_state.filter.as_str()
                            }
                        ), 14),
                        wrapped_text_owned(format!(
                            "preferred propagation: {}",
                            self.app
                                .settings
                                .preferred_propagation_node_hash
                                .as_deref()
                                .unwrap_or("none")
                        ), 14),
                        action_grid(
                            vec![subtle_button(
                                "Clear Propagation",
                                Message::ClearDirectoryPropagation
                            ),],
                            3
                        ),
                    ]
                    .spacing(4),
                ),
                selected_details,
                rows,
            ]
            .spacing(12)
            .width(Length::Fill),
        )
        .height(Length::Fill)
        .into()
    }

    fn directory_group_view(
        &self,
        title: &str,
        kind: crate::directory::DirectoryKind,
        scope: DirectoryScope,
        empty_text: &str,
    ) -> Element<'_, Message> {
        let entries = self
            .app
            .directory_state
            .entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| {
                directory_entry_matches_view(entry, &kind, &scope, &self.app.directory_state.filter)
            })
            .take(DIRECTORY_RENDER_LIMIT)
            .fold(column![].spacing(8), |column, (index, entry)| {
                column.push(self.directory_entry_card(index, entry))
            });
        let count = self
            .app
            .directory_state
            .entries
            .iter()
            .filter(|entry| {
                directory_entry_matches_view(entry, &kind, &scope, &self.app.directory_state.filter)
            })
            .count();

        if count == 0 {
            let empty_message = directory_empty_text_for_scope(
                empty_text,
                &scope,
                &self.app.directory_state.filter,
            );
            section_card(
                format!("{title} (0)"),
                wrapped_text_owned(empty_message, 14),
            )
        } else {
            let body = if count > DIRECTORY_RENDER_LIMIT {
                entries.push(
                    wrapped_text_owned(format!(
                        "Showing first {DIRECTORY_RENDER_LIMIT} of {count}. Use search, saved/trusted scope, or wait for stale live entries to prune."
                    ), 13),
                )
            } else {
                entries
            };
            section_card(format!("{title} ({count})"), body)
        }
    }

    fn directory_entry_card(
        &self,
        index: usize,
        entry: &crate::directory::DirectoryEntry,
    ) -> Element<'static, Message> {
        let marker = if Some(index) == self.app.directory_state.selected {
            "selected"
        } else {
            "entry"
        };
        let primary_action = match entry.kind {
            crate::directory::DirectoryKind::Node => {
                subtle_button("Browse", Message::OpenDirectoryEntry(index))
            }
            crate::directory::DirectoryKind::Peer => {
                subtle_button("Message", Message::OpenPeerChat(index))
            }
            crate::directory::DirectoryKind::Propagation => {
                subtle_button("Use", Message::UseDirectoryPropagation(index))
            }
            #[cfg(feature = "chat-client")]
            crate::directory::DirectoryKind::OmenChat => {
                subtle_button("Open Chat", Message::OpenDirectoryOmenChat(index))
            }
            #[cfg(not(feature = "chat-client"))]
            crate::directory::DirectoryKind::OmenChat => {
                subtle_button("Select", Message::SelectDirectoryEntry(index))
            }
            crate::directory::DirectoryKind::Unknown => {
                subtle_button("Select", Message::SelectDirectoryEntry(index))
            }
        };
        let destination_preview = short_destination_hash(&entry.destination_hash);
        let display_name = entry.display_name.clone();
        let marker_text = if Some(index) == self.app.directory_state.selected {
            "*"
        } else {
            " "
        };

        container(
            row![
                text(marker_text).size(ui_size(14)),
                text(display_name)
                    .size(ui_size(14))
                    .wrapping(Wrapping::WordOrGlyph)
                    .width(Length::FillPortion(3)),
                text(destination_preview)
                    .size(ui_size(13))
                    .wrapping(Wrapping::WordOrGlyph)
                    .width(Length::FillPortion(2)),
                text(relative_time(entry.last_seen))
                    .size(ui_size(13))
                    .width(Length::Shrink),
                subtle_button("Select", Message::SelectDirectoryEntry(index)),
                primary_action,
            ]
            .spacing(8)
            .align_y(iced::Alignment::Center),
        )
        .style(if marker == "selected" {
            status_container_style
        } else {
            card_container_style
        })
        .padding(6)
        .width(Length::Fill)
        .into()
    }

    fn directory_selected_details_card(&self) -> Element<'_, Message> {
        let Some(index) = self.app.directory_state.selected else {
            return section_card(
                "Selected Entry",
                text("Select a directory entry to inspect full destination and relationship details.")
                    .size(ui_size(14)),
            );
        };
        let Some(entry) = self.app.selected_directory_entry() else {
            return section_card(
                "Selected Entry",
                text("Select a directory entry to inspect full destination and relationship details.")
                    .size(ui_size(14)),
            );
        };
        let associated = entry.associated_hash.as_deref().unwrap_or("none");
        let node_associated = entry.node_associated_hash.as_deref().unwrap_or("none");
        let kind_note = directory_selected_kind_note(&entry);
        let state_lines = directory_selected_state_lines(&entry)
            .into_iter()
            .fold(column![].spacing(3), |column, line| {
                column.push(wrapped_text_owned(line, 14))
            });
        let micronplus_warning_lines = self
            .app
            .micronplus_warning_lines_for_directory_entry(&entry)
            .into_iter()
            .fold(column![].spacing(3), |column, line| {
                column.push(wrapped_text_owned(line, 13))
            });
        let selected_primary = directory_selected_primary_actions(index, &entry.kind);
        let trust_action = if entry.trust_level == crate::directory::TrustLevel::Trusted {
            "Untrust"
        } else {
            "Trust"
        };
        let mut management_actions = vec![
            subtle_button(
                if entry.saved { "Remove Saved" } else { "Save" },
                Message::SaveDirectoryEntry(index),
            ),
            subtle_button(trust_action, Message::ToggleDirectoryTrust(index)),
        ];
        if directory_kind_supports_identify_toggle(&entry.kind) {
            management_actions.push(subtle_button(
                if entry.identify_on_connect {
                    "Stop Identify"
                } else {
                    "Identify"
                },
                Message::ToggleDirectoryIdentify(index),
            ));
        }
        if directory_kind_supports_delivery_preference(&entry.kind) {
            management_actions.push(subtle_button(
                "Delivery",
                Message::CycleDirectoryDelivery(index),
            ));
        }
        management_actions.push(subtle_button(
            "Request Path",
            Message::RequestDirectoryPath(index),
        ));
        let selected_management = action_grid(management_actions, 3);

        section_card(
            format!("Selected Entry: {}", entry.display_name),
            column![
                selected_primary,
                selected_management,
                wrapped_text_owned(
                    format!(
                        "primary actions: {}",
                        directory_selected_primary_action_labels(&entry.kind).join(", ")
                    ),
                    13
                ),
                text(format!("{:?}", entry.kind)).size(ui_size(14)),
                wrapped_text_owned(format!("destination: {}", entry.destination_hash), 14),
                wrapped_text_owned(format!("associated: {associated}"), 14),
                wrapped_text_owned(format!("node associated: {node_associated}"), 14),
                state_lines,
                wrapped_text_owned(
                    format!(
                        "last seen: {} ({})",
                        format_epoch_secs(entry.last_seen),
                        relative_time(entry.last_seen)
                    ),
                    14
                ),
                wrapped_text_owned(kind_note, 14),
                wrapped_text_owned(self.app.micronplus_status_for_directory_entry(&entry), 14),
                section_card("MicronPlus Node Warnings", micronplus_warning_lines),
            ]
            .spacing(5),
        )
    }

    fn interfaces_view(&self) -> Element<'_, Message> {
        let selected = self.app.interfaces_state.selected;
        let runtime_interface_stats = self.app.monitoring_state.last_interface_stats.as_ref();
        let profiles = self.app.interfaces_state.profiles.iter().enumerate().fold(
            column![].spacing(8),
            |column, (index, profile)| {
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
                        text(format!("{:?}", profile.kind)).size(ui_size(14)),
                        text(format!("enabled: {}", profile.enabled)).size(ui_size(14)),
                    ]
                    .spacing(8)
                    .wrap(),
                    text(format!("profile id: {}", profile.profile_id))
                        .size(ui_size(14))
                        .wrapping(Wrapping::WordOrGlyph)
                        .width(Length::Fill),
                    text(interface_runtime_state_line(profile, runtime_interface_stats))
                        .size(ui_size(14))
                        .wrapping(Wrapping::WordOrGlyph)
                        .width(Length::Fill),
                    text(interface_runtime_status_label(profile, runtime_interface_stats))
                        .size(ui_size(14))
                        .wrapping(Wrapping::WordOrGlyph)
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
                        .wrapping(Wrapping::WordOrGlyph)
                        .width(Length::Fill),
                ]
                .spacing(5);
                body = match profile.kind {
                    InterfaceKind::TcpClient => {
                        let ifac_network_id = profile.profile_id.clone();
                        let ifac_pass_id = profile.profile_id.clone();
                        body.push(column![
                            text(format!(
                                "TCP gateway: {}:{}",
                                profile.target_host, profile.target_port
                            ))
                            .size(ui_size(14))
                            .wrapping(Wrapping::WordOrGlyph)
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
                            .wrapping(Wrapping::WordOrGlyph)
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
                                    .on_input(move |value| {
                                        Message::TcpClientIfacNetworkChanged {
                                            profile_id: ifac_network_id.clone(),
                                            value,
                                        }
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
                        .spacing(5))
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
                                .wrapping(Wrapping::WordOrGlyph)
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
                                .wrapping(Wrapping::WordOrGlyph)
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
                            .wrapping(Wrapping::WordOrGlyph)
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
                            .wrapping(Wrapping::WordOrGlyph)
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
                            .wrapping(Wrapping::WordOrGlyph)
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
                        column![text("Generic interface: no kind-specific settings are available.")
                            .size(ui_size(14))
                            .wrapping(Wrapping::WordOrGlyph)
                            .width(Length::Fill)]
                        .spacing(5),
                    ),
                };
                column.push(section_card(title, body))
            },
        );
        let preview = self
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
        interface_setup_actions.extend(self.gateway_preset_buttons());
        interface_setup_actions.push(subtle_button(
            "Settings",
            Message::SwitchSection(WorkspaceSection::Settings),
        ));

        let mut content = column![
            text("Interfaces").size(ui_size(28)),
            section_card(
                "Native Runtime Interfaces",
                column![
                    action_grid(interface_setup_actions, 3),
                    action_grid(
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
                    ),
                    text(format!(
                        "profiles={} selected={}",
                        self.app.interfaces_state.profiles.len(),
                        selected
                            .map(|index| index.to_string())
                            .unwrap_or_else(|| "none".into())
                    ))
                    .size(ui_size(14))
                    .wrapping(Wrapping::WordOrGlyph)
                    .width(Length::Fill),
                    text(format!(
                        "last export: {}",
                        self.app
                            .interfaces_state
                            .last_config_export_path
                            .as_ref()
                            .map(|path| path.display().to_string())
                            .unwrap_or_else(|| "none".into())
                    ))
                    .size(ui_size(14))
                    .wrapping(Wrapping::WordOrGlyph)
                    .width(Length::Fill),
                    text(format!(
                        "config path: {}",
                        self.app.interface_service.config_path().display()
                    ))
                    .size(ui_size(13))
                    .wrapping(Wrapping::WordOrGlyph)
                    .width(Length::Fill),
                ]
                .spacing(6),
            ),
        ]
        .spacing(12);
        if let Some(profile) = self.app.pending_interface_delete_profile() {
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
        let preview_lines = interface_config_preview_lines(&preview).into_iter().fold(
            column![].spacing(2),
            |column, line| {
                column.push(
                    text(line)
                        .size(ui_size(13))
                        .wrapping(Wrapping::WordOrGlyph)
                        .width(Length::Fill),
                )
            },
        );
        content = content
            .push(profiles)
            .push(section_card("Generated Config Preview", preview_lines));

        app_scrollable(content.width(Length::Fill))
            .height(Length::Fill)
            .into()
    }

    fn gateway_preset_buttons(&self) -> Vec<Button<'static, Message>> {
        self.app
            .interface_service
            .gateway_presets()
            .unwrap_or_default()
            .into_iter()
            .map(|preset| {
                subtle_button_owned(
                    format!("Add {} Gateway", preset.name),
                    Message::CreateGatewayPreset(preset.id),
                )
            })
            .collect()
    }

    fn diagnostics_view(&self) -> Element<'_, Message> {
        let summary = diagnostics_preview_report_summary(&self.app.diagnostics_state.preview_lines);
        let summary_card = if let Some(summary) = summary {
            section_card(
                format!("Report Summary: {}", summary.report),
                column![
                    wrapped_text_owned(format!("outcome: {}", summary.outcome), 15),
                    wrapped_text_owned(format!("stage: {}", summary.stage), 15),
                    wrapped_text_owned(format!("detail: {}", summary.detail), 14),
                    wrapped_text_owned(format!("next: {}", summary.next_step), 14),
                ]
                .spacing(4),
            )
        } else {
            section_card(
                "Report Summary",
                text("Run or preview a native diagnostic report to see outcome/stage/next-step here.")
                    .size(ui_size(14)),
            )
        };
        let blockers = self
            .native_action_status_lines()
            .into_iter()
            .fold(column![].spacing(3), |column, line| {
                column.push(wrapped_text_owned(line, 14))
            });
        let stage_cards =
            diagnostics_preview_stage_cards(&self.app.diagnostics_state.preview_lines)
                .into_iter()
                .take(12)
                .fold(column![].spacing(8), |column, stage| {
                    column.push(section_card(
                        format!("{}: {}", stage.kind, stage.stage),
                        column![
                            wrapped_text_owned(format!("status: {}", stage.status), 14),
                            wrapped_text_owned(format!("detail: {}", stage.detail), 14),
                            wrapped_text_owned(format!("next: {}", stage.next_step), 14),
                        ]
                        .spacing(3),
                    ))
                });
        let live_fetch_card = if let Some(fetch) =
            diagnostics_preview_live_fetch_card(&self.app.diagnostics_state.preview_lines)
        {
            section_card(
                "Live Fetch Result",
                column![
                    wrapped_text_owned(format!("outcome: {}", fetch.outcome), 14),
                    wrapped_text_owned(format!("stage: {}", fetch.stage_hint), 14),
                    wrapped_text_owned(format!("request backend: {}", fetch.request_backend), 14),
                    wrapped_text_owned(format!("response: {}", fetch.response_size), 14),
                    wrapped_text_owned(format!("detail: {}", fetch.detail), 14),
                    wrapped_text_owned(
                        format!("first failed stage: {}", fetch.first_failed_stage),
                        14
                    ),
                    wrapped_text_owned(format!("next: {}", fetch.next_step), 14),
                ]
                .spacing(3),
            )
        } else {
            section_card(
                "Live Fetch Result",
                wrapped_text_owned(
                    "Run Native Live Fetch to see fetch_page stage, backend, and response metadata here.",
                    14,
                ),
            )
        };
        let lxmf_delivery_card = if let Some(lxmf) =
            diagnostics_preview_lxmf_delivery_card(&self.app.diagnostics_state.preview_lines)
        {
            section_card(
                "LXMF Delivery Result",
                column![
                    wrapped_text_owned(format!("outcome: {}", lxmf.outcome), 14),
                    wrapped_text_owned(format!("send: {}", lxmf.send_state), 14),
                    wrapped_text_owned(format!("proof: {}", lxmf.proof_state), 14),
                    wrapped_text_owned(format!("inbound: {}", lxmf.inbound_state), 14),
                    wrapped_text_owned(format!("events: {}", lxmf.event_counts), 14),
                    wrapped_text_owned(format!("readiness: {}", lxmf.readiness_stage), 14),
                    wrapped_text_owned(format!("detail: {}", lxmf.detail), 14),
                    wrapped_text_owned(format!("next: {}", lxmf.next_step), 14),
                ]
                .spacing(3),
            )
        } else {
            section_card(
                "LXMF Delivery Result",
                wrapped_text_owned(
                    "Run LXMF Interop to see send/proof/inbound evidence here.",
                    14,
                ),
            )
        };
        let propagation_sync_card = if let Some(sync) =
            diagnostics_preview_propagation_sync_card(&self.app.diagnostics_state.preview_lines)
        {
            let event_lines = sync
                .event_lines
                .iter()
                .fold(column![].spacing(2), |column, line| {
                    column.push(wrapped_text_owned(line.clone(), 12))
                });
            section_card(
                "LXMF Propagation Sync",
                column![
                    wrapped_text_owned(format!("outcome: {}", sync.outcome), 14),
                    wrapped_text_owned(format!("selected node: {}", sync.selected_node), 14),
                    wrapped_text_owned(format!("before: {}", sync.before), 14),
                    wrapped_text_owned(format!("after: {}", sync.after), 14),
                    wrapped_text_owned(format!("events: {}", sync.events), 14),
                    section_card("Recent Sync Events", event_lines),
                    wrapped_text_owned(format!("blocker: {}", sync.blocker), 14),
                    wrapped_text_owned(format!("next: {}", sync.next_step), 14),
                ]
                .spacing(3),
            )
        } else {
            section_card(
                "LXMF Propagation Sync",
                wrapped_text_owned(
                    "Run Sync Propagation to see propagation-node /get status, haves/wants, and failures here.",
                    14,
                ),
            )
        };
        let preview = self
            .app
            .diagnostics_state
            .preview_lines
            .iter()
            .take(80)
            .fold(column![].spacing(3), |column, line| {
                column.push(wrapped_text_owned(line.clone(), 13))
            });
        let snapshot = self
            .app
            .diagnostics_state
            .last_snapshot
            .as_ref()
            .cloned()
            .unwrap_or_else(|| "No diagnostics snapshot captured yet.".into());
        let diagnostic_target = column![
            wrapped_text_owned(format!(
                "kind: {}",
                self.app
                    .diagnostics_state
                    .target_kind
                    .as_deref()
                    .unwrap_or("none")
            ), 14),
            wrapped_text_owned(format!(
                "address: {}",
                self.app
                    .diagnostics_state
                    .target_address
                    .as_deref()
                    .unwrap_or("none")
            ), 14),
            wrapped_text_owned(
                "Browser and conversation Diag buttons update this target before running their report.",
                13,
            ),
        ]
        .spacing(3);

        app_scrollable(
            column![
                text("Diagnostics").size(ui_size(28)),
                section_card("Diagnostic Target", diagnostic_target),
                section_card(
                    "Runtime Readiness",
                    column![
                        wrapped_text_owned(
                            format!(
                                "backend: {:?} | connected={} | {}",
                                self.app.runtime_status.backend,
                                self.app.runtime_status.connected,
                                self.app.runtime_status.message
                            ),
                            14
                        ),
                        wrapped_text_owned(format!("task: {}", self.app.status.task), 14),
                        wrapped_text_owned(format!("identity: {}", self.app.status.identity), 14),
                    ]
                    .spacing(4),
                ),
                section_card("Native Action Prerequisites", blockers),
                summary_card,
                live_fetch_card,
                lxmf_delivery_card,
                propagation_sync_card,
                section_card("Report Stages", stage_cards),
                section_card(
                    "Last Export",
                    column![
                        action_grid(
                            vec![
                                omen_button("Native Preflight", Message::NativePreflight),
                                omen_button("Native Dry Smoke", Message::NativeSmokeDryRun),
                                omen_button("Native Live Probe", Message::NativeSmokeLiveProbe),
                                omen_button("Native Live Fetch", Message::NativeLiveFetchValidate),
                                subtle_button("Path Diagnostics", Message::PathDiagnostics),
                                subtle_button(
                                    "Known Destinations",
                                    Message::BeginKnownDestinationsPreload
                                ),
                            ],
                            3,
                        ),
                        action_grid(
                            vec![
                                omen_button("LXMF Smoke Send", Message::NativeLxmfSmokeSend),
                                omen_button("LXMF Interop", Message::NativeLxmfInterop),
                                omen_button(
                                    "Sync Propagation",
                                    Message::NativeLxmfPropagationDiagnostics
                                ),
                                subtle_button(
                                    "Preview Live Report",
                                    Message::PreviewLiveInteropReport
                                ),
                                subtle_button(
                                    "Export Live Report",
                                    Message::ExportLiveInteropReport
                                ),
                                subtle_button("Preview Bundle", Message::PreviewDiagnosticsBundle),
                                subtle_button("Export Bundle", Message::ExportDiagnosticsBundle),
                            ],
                            3
                        ),
                        wrapped_text_owned(
                            format!(
                                "path: {}",
                                self.app
                                    .diagnostics_state
                                    .last_export_path
                                    .as_ref()
                                    .map(|path| path.display().to_string())
                                    .unwrap_or_else(|| "none".into())
                            ),
                            14
                        ),
                        wrapped_text_owned(
                            format!(
                                "summary: {}",
                                self.app
                                    .diagnostics_state
                                    .last_export_summary
                                    .as_deref()
                                    .unwrap_or("none")
                            ),
                            14
                        ),
                        wrapped_text_owned(
                            format!(
                                "preview scroll: {}",
                                self.app.diagnostics_state.preview_scroll
                            ),
                            14
                        ),
                    ]
                    .spacing(4),
                ),
                section_card("Snapshot", wrapped_text_owned(snapshot, 13),),
                section_card("Preview", preview),
            ]
            .spacing(12)
            .width(Length::Fill),
        )
        .height(Length::Fill)
        .into()
    }

    fn monitoring_view(&self) -> Element<'_, Message> {
        let monitoring = &self.app.monitoring_state;
        let runtime = &self.app.runtime_status;
        let resources = self.monitoring_process_usage;
        let uptime_secs = current_epoch_ms()
            .saturating_sub(monitoring.started_epoch_ms)
            .max(1)
            / 1_000;
        let event_rate = monitoring.runtime_events_total as f64 / uptime_secs.max(1) as f64;
        let outbound_messages = self
            .app
            .workspace
            .conversations
            .iter()
            .flat_map(|conversation| conversation.thread.messages.iter())
            .filter(|message| !message.incoming)
            .count();
        let inbound_messages = self
            .app
            .workspace
            .conversations
            .iter()
            .flat_map(|conversation| conversation.thread.messages.iter())
            .filter(|message| message.incoming)
            .count();
        let pending_messages = self
            .app
            .workspace
            .conversations
            .iter()
            .filter(|conversation| conversation.pending_send.is_some())
            .count();
        let directory_entries = self.app.directory_service.list_entries();
        let live_entries = self.app.directory_service.list_live_entries();
        let saved_entries = directory_entries.iter().filter(|entry| entry.saved).count();
        let trusted_entries = directory_entries
            .iter()
            .filter(|entry| entry.trusted)
            .count();

        let traffic_cards = row![
            monitoring_metric_card(
                "RX estimate",
                human_bytes(monitoring.estimated_inbound_bytes),
                format!(
                    "{} announces / {} inbound LXMF",
                    monitoring.announces_received, monitoring.inbound_messages
                ),
            ),
            monitoring_metric_card(
                "TX estimate",
                human_bytes(monitoring.estimated_outbound_bytes),
                format!(
                    "{} page / {} path / {} LXMF",
                    monitoring.outbound_page_requests,
                    monitoring.outbound_path_requests + monitoring.outbound_path_warmups,
                    monitoring.outbound_lxmf_sends
                ),
            ),
            monitoring_metric_card(
                "Runtime events",
                monitoring.runtime_events_total.to_string(),
                format!(
                    "{event_rate:.2}/sec, {} debug",
                    monitoring.runtime_debug_events
                ),
            ),
        ]
        .spacing(8)
        .wrap();

        let network_lines = column![
            wrapped_text_owned(format!(
                "backend: {:?} | connected={} | {}",
                runtime.backend, runtime.connected, runtime.message
            ), 14),
            wrapped_text_owned(
                monitoring_interface_reconnect_line(monitoring.last_interface_stats.as_ref()),
                14,
            ),
            wrapped_text_owned(format!(
                "identity: {}",
                runtime
                    .active_identity
                    .as_ref()
                    .map(|identity| format!("{} / {}", identity.label, identity.hash_hex))
                    .unwrap_or_else(|| "none".into())
            ), 14),
            wrapped_text_owned(format!(
                "path updates: {} | page probes: {} | propagation sync events: {}",
                monitoring.path_updates_received,
                monitoring.page_fetch_probes,
                monitoring.propagation_sync_events
            ), 14),
            wrapped_text_owned(format!(
                "outgoing: pages={} partials={} downloads={} diagnostics={}",
                monitoring.outbound_page_requests,
                monitoring.outbound_partial_refreshes,
                monitoring.outbound_file_downloads,
                monitoring.outbound_diagnostics
            ), 14),
            wrapped_text_owned(format!(
                "outgoing paths/messages: path_requests={} path_warmups={} lxmf_sends={} prop_syncs={}",
                monitoring.outbound_path_requests,
                monitoring.outbound_path_warmups,
                monitoring.outbound_lxmf_sends,
                monitoring.outbound_propagation_syncs
            ), 14),
            wrapped_text_owned(format!(
                "incoming: page_responses={} downloads={} announces={} inbound_lxmf={}",
                monitoring.inbound_page_responses,
                monitoring.inbound_downloads,
                monitoring.announces_received,
                monitoring.inbound_messages
            ), 14),
            wrapped_text_owned(format!(
                "LXMF evidence: {} | outbound status updates: {} | runtime errors: {}",
                monitoring.lxmf_evidence_updates,
                monitoring.outbound_status_updates,
                monitoring.runtime_errors
            ), 14),
        ]
        .spacing(4);
        let attribution_lines = monitoring_runtime_attribution_lines(monitoring, uptime_secs)
            .into_iter()
            .fold(column![].spacing(4), |lines, line| {
                lines.push(wrapped_text_owned(line, 14))
            });

        let directory_lines = column![
            monitoring_meter(
                "live directory",
                live_entries.len(),
                directory_entries.len().max(1)
            ),
            monitoring_meter("saved", saved_entries, directory_entries.len().max(1)),
            monitoring_meter("trusted", trusted_entries, directory_entries.len().max(1)),
            wrapped_text_owned(
                format!(
                    "nodes={} peers={} propagation={} total={}",
                    directory_entries
                        .iter()
                        .filter(|entry| entry.kind == crate::directory::DirectoryKind::Node)
                        .count(),
                    directory_entries
                        .iter()
                        .filter(|entry| entry.kind == crate::directory::DirectoryKind::Peer)
                        .count(),
                    directory_entries
                        .iter()
                        .filter(|entry| entry.kind == crate::directory::DirectoryKind::Propagation)
                        .count(),
                    directory_entries.len()
                ),
                14
            ),
        ]
        .spacing(4);

        let message_lines = column![
            monitoring_meter(
                "incoming share",
                inbound_messages,
                inbound_messages + outbound_messages
            ),
            monitoring_meter(
                "outgoing share",
                outbound_messages,
                inbound_messages + outbound_messages
            ),
            wrapped_text_owned(
                format!(
                    "conversations={} inbound={} outbound={} pending={pending_messages}",
                    self.app.workspace.conversations.len(),
                    inbound_messages,
                    outbound_messages
                ),
                14
            ),
        ]
        .spacing(4);

        let resource_lines = match resources {
            Some(resources) => column![
                monitoring_meter("rss", resources.rss_bytes as usize, 512 * 1024 * 1024),
                wrapped_text_owned(format!("memory: {}", human_bytes(resources.rss_bytes)), 14),
                wrapped_text_owned(
                    format!("process cpu time: {:.2}s", resources.cpu_seconds),
                    14
                ),
            ]
            .spacing(4),
            None => column![wrapped_text_owned(
                "Process resource stats are unavailable on this platform.",
                14
            )]
            .spacing(4),
        };

        let interface_card = if let Some(stats) = &monitoring.last_interface_stats {
            let interface_lines = monitoring_interface_status_lines(stats);
            section_card(
                "Interfaces",
                interface_lines
                    .into_iter()
                    .fold(column![].spacing(4), |column, line| {
                        column.push(wrapped_text_owned(line, 14))
                    }),
            )
        } else {
            section_card(
                "Interfaces",
                wrapped_panel_text("No runtime interface stats have been sampled yet. Run Diagnostics or native startup to populate rnstatus-like interface data."),
            )
        };
        let omenchat_card = self.omenchat_monitoring_card();

        app_scrollable(
            column![
                text("Monitoring").size(ui_size(28)),
                wrapped_panel_text("Runtime traffic and resource pressure for keeping OMENbrowser_rs quiet on Reticulum."),
                traffic_cards,
                row![
                    section_card("Network Runtime", network_lines),
                    section_card("Runtime Attribution", attribution_lines),
                    section_card("Process Resources", resource_lines),
                ]
                .spacing(8)
                .wrap(),
                row![
                    section_card("Directory Noise Surface", directory_lines),
                    section_card("LXMF Message Mix", message_lines),
                ]
                .spacing(8)
                .wrap(),
                interface_card,
                omenchat_card,
            ]
            .spacing(12)
            .width(Length::Fill),
        )
        .height(Length::Fill)
        .into()
    }

    #[cfg(feature = "chat-client-rns")]
    fn omenchat_live_monitor_totals(&self) -> OmenChatLiveMonitorTotals {
        let mut history_sync_waiting: HashSet<ChatSessionId> =
            self.omenchat_recent_sync_pending.iter().copied().collect();
        history_sync_waiting.extend(self.omenchat_recent_sync_due_after.keys().copied());
        let mut totals = OmenChatLiveMonitorTotals {
            sessions: self.chat_client.sessions().len(),
            connected: self.omenchat_live_transports.len(),
            opening: self.omenchat_live_opening.len(),
            reconnect_timers: self.omenchat_live_retry_after.len(),
            history_sync_waiting: history_sync_waiting.len(),
            ..OmenChatLiveMonitorTotals::default()
        };
        for transport in self.omenchat_live_transports.values() {
            totals.pending_resources = totals
                .pending_resources
                .saturating_add(transport.pending_resource_offer_count());
            totals.frames_in = totals.frames_in.saturating_add(transport.frames_in);
            totals.frames_out = totals.frames_out.saturating_add(transport.frames_out);
            totals.bytes_in = totals.bytes_in.saturating_add(transport.bytes_in);
            totals.bytes_out = totals.bytes_out.saturating_add(transport.bytes_out);
            totals.resources_in = totals.resources_in.saturating_add(transport.resources_in);
            totals.resource_bytes_in = totals
                .resource_bytes_in
                .saturating_add(transport.resource_bytes_in);
            totals.upload_fetches_out = totals
                .upload_fetches_out
                .saturating_add(transport.upload_fetches_out);
            totals.upload_resource_offers_in = totals
                .upload_resource_offers_in
                .saturating_add(transport.upload_resource_offers_in);
            totals.upload_inline_chunks_in = totals
                .upload_inline_chunks_in
                .saturating_add(transport.upload_inline_chunks_in);
            totals.upload_inline_bytes_in = totals
                .upload_inline_bytes_in
                .saturating_add(transport.upload_inline_bytes_in);
            totals.upload_resources_in = totals
                .upload_resources_in
                .saturating_add(transport.upload_resources_in);
            totals.upload_resource_bytes_in = totals
                .upload_resource_bytes_in
                .saturating_add(transport.upload_resource_bytes_in);
            if transport.awaiting_pong {
                totals.awaiting_pongs = totals.awaiting_pongs.saturating_add(1);
            }
        }
        totals
    }

    fn omenchat_monitoring_card(&self) -> Element<'_, Message> {
        #[cfg(feature = "chat-client-rns")]
        {
            let now = current_epoch_ms();
            let mut lines = column![].spacing(6);
            if self.chat_client.sessions().is_empty() {
                lines = lines.push(
                    wrapped_text_owned(
                        "No OMENchat sessions are open. Open an omenchat:// destination to monitor live chat traffic.",
                        14,
                    ),
                );
            } else {
                let media_total = self.omenchat_media_cache.len();
                let media_loading = self
                    .omenchat_media_cache
                    .values()
                    .filter(|state| matches!(state, OmenChatMediaLoadState::Loading { .. }))
                    .count();
                let media_cached = self
                    .omenchat_media_cache
                    .values()
                    .filter(|state| matches!(state, OmenChatMediaLoadState::Cached { .. }))
                    .count();
                let media_failed = self
                    .omenchat_media_cache
                    .values()
                    .filter(|state| matches!(state, OmenChatMediaLoadState::Failed { .. }))
                    .count();
                let totals = self.omenchat_live_monitor_totals();
                lines = lines.push(
                    column![
                        wrapped_text_owned(omenchat_monitor_health_line(&totals), 13),
                        wrapped_text_owned(format!(
                            "summary: {} session(s) | {} connected | {} opening | {} reconnect timer(s) | {} history sync wait(s) | {} awaiting pong(s)",
                            totals.sessions,
                            totals.connected,
                            totals.opening,
                            totals.reconnect_timers,
                            totals.history_sync_waiting,
                            totals.awaiting_pongs
                        ), 13),
                        wrapped_text_owned(format!(
                            "traffic total: frames {} in / {} out | wire {} rx / {} tx | resources {} ({}) | pending resources {}",
                            totals.frames_in,
                            totals.frames_out,
                            human_bytes(totals.bytes_in),
                            human_bytes(totals.bytes_out),
                            totals.resources_in,
                            human_bytes(totals.resource_bytes_in),
                            totals.pending_resources
                        ), 13),
                        wrapped_text_owned(format!(
                            "upload total: fetches {} | offers {} | inline chunks {} ({}) | resources {} ({})",
                            totals.upload_fetches_out,
                            totals.upload_resource_offers_in,
                            totals.upload_inline_chunks_in,
                            human_bytes(totals.upload_inline_bytes_in),
                            totals.upload_resources_in,
                            human_bytes(totals.upload_resource_bytes_in)
                        ), 13),
                    ]
                    .spacing(2),
                );
                lines = lines.push(
                    wrapped_text_owned(format!(
                        "media cache: {media_cached} cached / {media_loading} loading / {media_failed} failed ({media_total} tracked)"
                    ), 13),
                );
                for session in self.chat_client.sessions() {
                    let connects = self
                        .omenchat_live_connect_count
                        .get(&session.session_id)
                        .copied()
                        .unwrap_or_default();
                    let disconnects = self
                        .omenchat_live_disconnect_count
                        .get(&session.session_id)
                        .copied()
                        .unwrap_or_default();
                    let retry_attempts = self
                        .omenchat_live_retry_count
                        .get(&session.session_id)
                        .copied()
                        .unwrap_or_default();
                    let reconnect_line =
                        self.omenchat_reconnect_state_label(session.session_id, now);
                    let last_disconnect = self
                        .omenchat_live_last_disconnect_reason
                        .get(&session.session_id)
                        .map(|reason| format!("last_disconnect={reason}"))
                        .unwrap_or_else(|| "last_disconnect=none".into());
                    let transport = self.omenchat_live_transports.get(&session.session_id);
                    let history_sync_line =
                        self.omenchat_recent_sync_monitor_label(session.session_id, now);
                    let attention_line =
                        omenchat_session_attention_line(OmenChatSessionAttention {
                            connected: transport.is_some(),
                            opening: self.omenchat_live_opening.contains(&session.session_id),
                            reconnect_queued: self
                                .omenchat_live_retry_after
                                .contains_key(&session.session_id),
                            awaiting_pong: transport
                                .is_some_and(|transport| transport.awaiting_pong),
                            last_ping_age_ms: transport.and_then(|transport| {
                                (transport.last_ping_epoch_ms > 0)
                                    .then(|| now.saturating_sub(transport.last_ping_epoch_ms))
                            }),
                            heartbeat_idle_ms: transport
                                .map(|transport| transport.heartbeat_idle_ms),
                            pending_resources: transport
                                .map(|transport| transport.pending_resource_offer_count())
                                .unwrap_or_default(),
                            history_sync_label: &history_sync_line,
                        });
                    let link_line = if let Some(transport) = transport {
                        format!(
                            "live link {} | up {} | last rx {} | last tx {} | awaiting_pong={}",
                            short_destination_hash(&hex_bytes(&transport.link_id)),
                            compact_elapsed_ms(
                                now.saturating_sub(transport.connected_since_epoch_ms)
                            ),
                            compact_elapsed_ms(now.saturating_sub(transport.last_rx_epoch_ms)),
                            compact_elapsed_ms(now.saturating_sub(transport.last_tx_epoch_ms)),
                            transport.awaiting_pong
                        )
                    } else if self.omenchat_live_opening.contains(&session.session_id) {
                        "opening/reconnecting live link".into()
                    } else {
                        "no active live link".into()
                    };
                    let traffic_line = if let Some(transport) = transport {
                        format!(
                            "frames {} in / {} out | wire {} rx / {} tx | resources {} ({}) | pending resources {}",
                            transport.frames_in,
                            transport.frames_out,
                            human_bytes(transport.bytes_in),
                            human_bytes(transport.bytes_out),
                            transport.resources_in,
                            human_bytes(transport.resource_bytes_in),
                            transport.pending_resource_offer_count()
                        )
                    } else {
                        "frames 0 in / 0 out | wire 0 B rx / 0 B tx".into()
                    };
                    let mix_line = if let Some(transport) = transport {
                        format!(
                            "mix: history {} in / {} out | room events {} | chat sends {} | userlists {} | resource offers {} | ping {} out / pong {} in",
                            transport.history_frames_in,
                            transport.history_frames_out,
                            transport.room_events_in,
                            transport.chat_frames_out,
                            transport.userlist_frames_in,
                            transport.resource_offers_in,
                            transport.pings_out,
                            transport.pongs_in
                        )
                    } else {
                        "mix: disconnected".into()
                    };
                    let upload_line = if let Some(transport) = transport {
                        format!(
                            "uploads: fetches {} | offers {} | inline chunks {} ({}) | resources {} ({})",
                            transport.upload_fetches_out,
                            transport.upload_resource_offers_in,
                            transport.upload_inline_chunks_in,
                            human_bytes(transport.upload_inline_bytes_in),
                            transport.upload_resources_in,
                            human_bytes(transport.upload_resource_bytes_in)
                        )
                    } else {
                        "uploads: disconnected".into()
                    };
                    let heartbeat_line = if let Some(transport) = transport {
                        let last_pong = if transport.last_pong_epoch_ms > 0 {
                            compact_elapsed_ms(now.saturating_sub(transport.last_pong_epoch_ms))
                        } else {
                            "never".into()
                        };
                        let rtt = transport
                            .last_ping_rtt_ms
                            .map(|rtt| format!("{rtt} ms"))
                            .unwrap_or_else(|| "unknown".into());
                        let last_ping = if transport.last_ping_epoch_ms > 0 {
                            compact_elapsed_ms(now.saturating_sub(transport.last_ping_epoch_ms))
                        } else {
                            "never".into()
                        };
                        let interval = compact_elapsed_ms(transport.heartbeat_idle_ms.clamp(
                            OMENCHAT_MIN_HEARTBEAT_IDLE_MS,
                            OMENCHAT_MAX_HEARTBEAT_IDLE_MS,
                        ));
                        format!(
                            "heartbeat: interval {interval} | last ping {last_ping} ago | last pong {last_pong} ago | RTT {rtt}"
                        )
                    } else {
                        "heartbeat: disconnected".into()
                    };
                    let last_frame_line = if let Some(transport) = transport {
                        format!(
                            "last frames: rx={} / tx={}",
                            transport.last_rx_frame.as_deref().unwrap_or("none"),
                            transport.last_tx_frame.as_deref().unwrap_or("none")
                        )
                    } else {
                        "last frames: none".into()
                    };
                    lines = lines.push(
                        column![
                            wrapped_text_owned(
                                format!(
                                    "{} | {} | room #{} | users {}",
                                    session.server.display_name,
                                    short_destination_hash(&session.server.destination),
                                    session.active_room.name,
                                    session.users.len()
                                ),
                                14
                            ),
                            wrapped_text_owned(
                                format!(
                                "{} | connects={} disconnects={} retry_attempts={} | {} | {} | {}",
                                link_line,
                                connects,
                                disconnects,
                                retry_attempts,
                                reconnect_line,
                                last_disconnect,
                                session.status
                            ),
                                13
                            ),
                            wrapped_text_owned(attention_line, 13),
                            wrapped_text_owned(traffic_line, 13),
                            wrapped_text_owned(mix_line, 13),
                            wrapped_text_owned(upload_line, 13),
                            wrapped_text_owned(heartbeat_line, 13),
                            wrapped_text_owned(history_sync_line, 13),
                            wrapped_text_owned(last_frame_line, 13),
                        ]
                        .spacing(2),
                    );
                }
            }
            section_card("OMENchat Live Links", lines)
        }
        #[cfg(not(feature = "chat-client-rns"))]
        {
            section_card(
                "OMENchat Live Links",
                text("OMENchat live monitoring is unavailable in this build.").size(ui_size(14)),
            )
        }
    }

    fn native_action_status_lines(&self) -> Vec<String> {
        let readiness = self.app.native_reticulum_readiness();
        let native_backend = matches!(
            self.app.settings.runtime_backend,
            RuntimeBackendSetting::Auto | RuntimeBackendSetting::Reticulum
        );
        let identity_ready = self.app.settings.identity_path.is_some();
        let browser_address = self.app.active_browser_tab().address_input.trim();
        let destination_ready = BrowserAddress::parse(browser_address).is_some();
        let peer_ready = is_32_hex_hash(self.app.active_conversation().peer_hash.trim());

        vec![
            action_status_line(
                readiness.compiled,
                "native feature compiled",
                "build with native-network features",
            ),
            action_status_line(
                native_backend,
                "native backend selected",
                "choose Auto or Reticulum backend",
            ),
            action_status_line(
                identity_ready,
                "identity configured",
                "create or attach an identity",
            ),
            action_status_line(
                readiness.configured,
                "native runtime configured",
                "fix interface/config readiness blockers",
            ),
            action_status_line(
                destination_ready,
                "browser destination address ready",
                "enter a destination:path address such as <hash>:/page/index.mu",
            ),
            action_status_line(
                peer_ready,
                "valid LXMF peer selected",
                "open/select a Directory peer with a 32 hex destination hash",
            ),
        ]
    }

    fn logs_view(&self) -> Element<'_, Message> {
        let mut entries = self.app.logs.filtered_entries();
        entries.sort_by_key(|entry| std::cmp::Reverse(entry.epoch_ms));
        let rows =
            entries
                .iter()
                .take(LOG_VISIBLE_ENTRIES)
                .fold(column![].spacing(6), |column, entry| {
                    column.push(section_card(
                        format!(
                            "{:?} / {:?} / {}",
                            entry.severity,
                            entry.source,
                            format_epoch_ms(entry.epoch_ms)
                        ),
                        wrapped_text_owned(entry.message.clone(), 14),
                    ))
                });

        app_scrollable(
            column![
                text("Logs").size(ui_size(28)),
                section_card(
                    "Log Filters",
                    column![
                        wrapped_text_owned(format!(
                            "entries={} visible={} severity={:?} source={:?}",
                            self.app.logs.entries.len(),
                            entries.len().min(LOG_VISIBLE_ENTRIES),
                            self.app.logs.severity_filter,
                            self.app.logs.source_filter
                        ), 14),
                        wrapped_panel_text("Filter controls remain in the TUI/keybinding layer; this desktop panel is a readable log deck."),
                    ]
                    .spacing(4),
                ),
                rows,
            ]
            .spacing(12)
            .width(Length::Fill),
        )
        .height(Length::Fill)
        .into()
    }

    fn plugins_view(&self) -> Element<'_, Message> {
        let warnings = self
            .app
            .plugins_state
            .warnings
            .iter()
            .fold(column![].spacing(4), |column, warning| {
                column.push(wrapped_text_owned(warning.clone(), 14))
            });
        let plugins = self.app.plugins_state.installed.iter().enumerate().fold(
            column![].spacing(8),
            |column, (index, plugin)| {
                let title = if Some(index) == self.app.plugins_state.selected {
                    format!("[selected] {}", plugin.manifest.name)
                } else {
                    plugin.manifest.name.clone()
                };
                column.push(section_card(
                    title,
                    column![
                        row![
                            subtle_button("Select", Message::SelectPlugin(index)),
                            subtle_button("Toggle", Message::TogglePlugin(index)),
                            warning_button("Remove", Message::BeginPluginRemove(index)),
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
                        wrapped_text_owned(
                            format!("entrypoint: {}", plugin.manifest.entrypoint),
                            14
                        ),
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
        let details = self
            .app
            .selected_plugin_detail_lines()
            .into_iter()
            .fold(column![].spacing(3), |column, line| {
                column.push(wrapped_text_owned(line, 13))
            });
        let micronplus_diagnostics = self
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
                                omen_button("Install Trusted Folder", Message::BeginPluginInstall),
                                subtle_button("Toggle Selected", Message::ToggleSelectedPlugin),
                                warning_button(
                                    "Remove Selected",
                                    Message::BeginSelectedPluginRemove,
                                ),
                                subtle_button("Refresh", Message::RefreshPlugins),
                                subtle_button("Open Plugin Logs", Message::ShowPluginLogs),
                            ],
                            5,
                        ),
                        wrapped_text_owned(
                            format!(
                                "installed={} manifests={} selected={}",
                                self.app.plugins_state.installed.len(),
                                self.app.plugins_state.manifests.len(),
                                self.app
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

    fn help_view(&self) -> Element<'_, Message> {
        let browser_help = column![
            wrapped_panel_text("Open NomadNet pages with destination:/path.mu or paste a full destination hash into the address field. Use Request Path when the route/key is unknown, then retry after the path status returns pass."),
            wrapped_panel_text("Back, Forward, Reload, Stop, Identify, Capture, and Diag act on the selected browser pane only. Diag opens diagnostics for that pane's current destination."),
            wrapped_panel_text("Ctrl + mouse wheel zooms only the active Micron viewport. The Top button returns that viewport to the first rendered row."),
            wrapped_panel_text("NomadNet non-.mu file links download through the configured downloads path. HTTP/HTTPS links open through the external browser prompt; use Copy URL for Tor Browser."),
        ]
        .spacing(6);

        let micron_help = column![
            wrapped_panel_text("Micron is rendered as a styled cell grid, so half-block art, true-color headers, links, forms, and focus order are preserved inside the viewport."),
            wrapped_panel_text("Tab and Shift+Tab move focus through links and form fields in the active viewport. Enter activates the focused link or submits the focused form action."),
            wrapped_panel_text("MicronPlus pages can expose live regions and UI controls. Live refreshes are quiet on success and report failures in the browser status/error surface."),
        ]
        .spacing(6);

        let omenchat_commands = column![
            wrapped_text_owned("/me <action> - send an action message", 14),
            wrapped_text_owned("/join <room> - switch to a room", 14),
            wrapped_text_owned("/part [room] - leave a room", 14),
            wrapped_text_owned("/rooms - list rooms advertised by the server", 14),
            wrapped_text_owned("/who - list visible users in the active room", 14),
            wrapped_text_owned("/upload <path> - offer a local file upload to the active room; the attach button opens a native file picker and sends the selected file", 14),
            wrapped_text_owned("/notice <text> - send a room notice; moderator/admin only", 14),
            wrapped_text_owned("/topic <text> - change the active room topic; moderator/admin only", 14),
            wrapped_text_owned("/create-room <room> [topic] - create a room; admin only; /create and /mkroom also work", 14),
            wrapped_text_owned("/kick <user>, /ban <user>, /unban <user> - moderation actions", 14),
            wrapped_text_owned("/mute <user>, /unmute <user> - moderation actions", 14),
            wrapped_text_owned("/role <user> <standard|trusted|mod|admin> - change a user role; admin only", 14),
        ]
        .spacing(4);

        let omenchat_help = column![
            wrapped_panel_text("Open OMENchat with omenchat://<destination hash> from the Browser workspace, a NomadNet link, or the OMENchat quick open field."),
            wrapped_panel_text("Path requests route to the selected OMENchat server. Reconnect restarts the live link and cancels stale reconnect attempts."),
            wrapped_panel_text("Load Older asks the server/client cache for earlier room history. Room history is cached locally per identity and server."),
            wrapped_panel_text("Enter sends the composer draft. The input clears after a successful send."),
            section_card("OMENchat Slash Commands", omenchat_commands),
        ]
        .spacing(8);

        let omenchat_alpha_help = OMENCHAT_ALPHA_TEST_HELP_LINES
            .iter()
            .fold(column![].spacing(6), |column, line| {
                column.push(wrapped_text_owned(*line, 14))
            });

        let omenchat_history_help = OMENCHAT_HISTORY_HELP_LINES
            .iter()
            .fold(column![].spacing(6), |column, line| {
                column.push(wrapped_text_owned(*line, 14))
            });

        let omenchat_media_help = OMENCHAT_MEDIA_HELP_LINES
            .iter()
            .fold(column![].spacing(6), |column, line| {
                column.push(wrapped_text_owned(*line, 14))
            });

        let omenchatd_help = OMENCHATD_OPERATOR_HELP_LINES
            .iter()
            .fold(column![].spacing(6), |column, line| {
                column.push(wrapped_text_owned(*line, 14))
            });

        let lxmf_help = LXMF_HELP_LINES
            .iter()
            .fold(column![].spacing(6), |column, line| {
                column.push(wrapped_text_owned(*line, 14))
            });

        let admin_help = column![
            wrapped_text_owned("Directory remembers selected nodes, peers, and propagation nodes. Trust controls affect defaults and safe interaction choices.", 14),
            wrapped_text_owned("Identities create separate identity material and per-identity storage roots. Delete Active is the only destructive identity action and requires confirmation.", 14),
            wrapped_text_owned("Interfaces edits the active identity's Reticulum config. Diagnostics, Logs, and Monitoring are the places to inspect runtime behavior and traffic.", 14),
            wrapped_text_owned("omenchatd keeps its own server root under ~/.omenchatd by default and should not touch ~/.reticulum, ~/.nomadnetwork, or OMENbrowser_rs identity storage.", 14),
        ]
        .spacing(6);

        app_scrollable(
            column![
                text("Help").size(ui_size(28)),
                section_card("Browser", browser_help),
                section_card("Micron And MicronPlus", micron_help),
                section_card("OMENchat Plugin Client", omenchat_help),
                section_card("OMENchat History Sync", omenchat_history_help),
                section_card("OMENchat Media Privacy And Uploads", omenchat_media_help),
                section_card("OMENchat Alpha Testing", omenchat_alpha_help),
                section_card("omenchatd Operator Notes", omenchatd_help),
                section_card("LXMF Messages", lxmf_help),
                section_card("Directory, Identities, And Admin", admin_help),
            ]
            .spacing(12)
            .width(Length::Fill),
        )
        .height(Length::Fill)
        .into()
    }

    fn settings_view(&self) -> Element<'_, Message> {
        let readiness = self.app.native_reticulum_readiness();
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
        let interface_column = self.app.native_interface_readiness().into_iter().fold(
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
        let interfaces = self.app.interfaces_state.profiles.iter().enumerate().fold(
            column![].spacing(4),
            |column, (index, profile)| {
                let summary = row![
                    subtle_button("Select", Message::SelectInterfaceProfile(index)),
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
                                    .on_input(move |value| Message::TcpClientHostChanged {
                                        profile_id: host_id.clone(),
                                        value,
                                    })
                                    .width(Length::FillPortion(2)),
                                text("port").size(ui_size(14)),
                                text_input("port", &profile.target_port.to_string())
                                    .on_input(move |value| Message::TcpClientPortChanged {
                                        profile_id: port_id.clone(),
                                        value,
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
                                        Message::TcpClientIfacNetworkChanged {
                                            profile_id: ifac_network_id.clone(),
                                            value,
                                        }
                                    })
                                    .width(Length::FillPortion(2)),
                                text_input("passphrase", &profile.passphrase)
                                    .secure(true)
                                    .on_input(move |value| {
                                        Message::TcpClientIfacPassphraseChanged {
                                            profile_id: ifac_pass_id.clone(),
                                            value,
                                        }
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
                                    .on_input(move |value| Message::TcpServerHostChanged {
                                        profile_id: host_id.clone(),
                                        value,
                                    })
                                    .width(Length::FillPortion(2)),
                                text("port").size(ui_size(14)),
                                text_input("port", &profile.target_port.to_string())
                                    .on_input(move |value| Message::TcpServerPortChanged {
                                        profile_id: port_id.clone(),
                                        value,
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
                                        Message::TcpServerIfacNetworkChanged {
                                            profile_id: ifac_network_id.clone(),
                                            value,
                                        }
                                    })
                                    .width(Length::FillPortion(2)),
                                text_input("passphrase", &profile.passphrase)
                                    .secure(true)
                                    .on_input(move |value| {
                                        Message::TcpServerIfacPassphraseChanged {
                                            profile_id: ifac_pass_id.clone(),
                                            value,
                                        }
                                    })
                                    .width(Length::FillPortion(2)),
                            ]
                            .spacing(8)
                            .wrap(),
                        )
                } else {
                    column
                }
            },
        );
        let theme_buttons = DESKTOP_THEME_CHOICES
            .iter()
            .fold(row![].spacing(8), |row, theme| {
                let theme_name = *theme;
                let button = if theme_name == self.app.settings.ui.theme_name {
                    omen_button(theme, Message::SetTheme(theme_name.into()))
                } else {
                    subtle_button(theme, Message::SetTheme(theme_name.into()))
                };
                row.push(button)
            })
            .wrap();
        let themes = column![
            wrapped_text_owned(format!("Theme: {}", self.app.settings.ui.theme_name), 14),
            theme_buttons,
        ]
        .spacing(8);

        let font_size = self.app.settings.ui.font_size.clamp(10, 24);
        let appearance = column![
            themes,
            row![
                wrapped_text_owned(format!("Font size: {font_size}px"), 14),
                subtle_button(
                    "-",
                    Message::SetFontSize(font_size.saturating_sub(1).max(10)),
                ),
                omen_button(
                    "+",
                    Message::SetFontSize(font_size.saturating_add(1).min(24)),
                ),
            ]
            .spacing(8)
            .wrap(),
            wrapped_text_owned("Font size applies on next launch.", 13),
        ]
        .spacing(8);

        let browser_choice_buttons = self.external_browsers.iter().enumerate().fold(
            row![].spacing(8),
            |row, (index, browser)| {
                let selected = Some(browser.command.as_str())
                    == self
                        .app
                        .settings
                        .clearweb
                        .preferred_external_browser_command
                        .as_deref();
                let label = format!("{} ({})", browser.label, browser.command);
                let button = if selected {
                    omen_button_owned(label, Message::SelectPreferredExternalBrowser(index))
                } else {
                    subtle_button_owned(label, Message::SelectPreferredExternalBrowser(index))
                };
                row.push(button)
            },
        );
        let clearweb = &self.app.settings.clearweb;
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
                        Message::ToggleClearwebSocksProxy,
                    ),
                    subtle_button(
                        if clearweb.remote_media_enabled {
                            "Disable Remote Media"
                        } else {
                            "Enable Remote Media"
                        },
                        Message::ToggleClearwebRemoteMedia,
                    ),
                    subtle_button("Clear Browser", Message::ClearPreferredExternalBrowser),
                ]
                .spacing(8)
                .wrap(),
                text(format!(
                    "SOCKS5 proxy: {}:{} | {}",
                    clearweb.socks_proxy_host,
                    clearweb.socks_proxy_port,
                    if self.clearweb_proxy_reachable {
                        "detected"
                    } else {
                        "not detected"
                    }
                ))
                .size(ui_size(14))
                .wrapping(Wrapping::WordOrGlyph)
                .width(Length::Fill),
                wrapped_text_owned(format!(
                    "Tor proxy detection also checks {}:9150 for Tor Browser Bundle; active proxy: {}",
                    clearweb.socks_proxy_host,
                    self.clearweb_proxy_endpoint
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
            omen_button("Create Identity", Message::CreateIdentity),
            omen_button("Add TCP Gateway", Message::CreateTcpClientInterface),
        ];
        native_setup_actions.extend(self.gateway_preset_buttons());
        native_setup_actions.extend([
            omen_button("Select Native Backend", Message::SelectNativeBackend),
            omen_button("Start Native Runtime", Message::StartNativeRuntime),
            omen_button("Full Quickstart", Message::NativeQuickstart),
        ]);
        let native_card = section_card(
            "First Run / Native Setup",
            column![
                action_grid(native_setup_actions, 3),
                action_grid(
                    vec![
                        subtle_button("Preview Config", Message::PreviewManagedConfig),
                        subtle_button("Export Config", Message::ExportManagedConfig),
                        subtle_button("Preflight", Message::NativePreflight),
                        subtle_button("Dry Smoke", Message::NativeSmokeDryRun),
                        subtle_button("Live Probe", Message::NativeSmokeLiveProbe),
                        omen_button("Live Fetch", Message::NativeLiveFetchValidate),
                        subtle_button("Known Destinations", Message::BeginKnownDestinationsPreload),
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
                    format!("backend: {:?}", self.app.settings.runtime_backend),
                    14
                ),
                wrapped_text_owned(
                    format!(
                        "active runtime: {:?} | connected={} | {}",
                        self.app.runtime_status.backend,
                        self.app.runtime_status.connected,
                        self.app.runtime_status.message
                    ),
                    14
                ),
                wrapped_text_owned(
                    format!(
                        "identity: {}",
                        self.app
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
                        self.app
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
                        if self.app.settings.auto_sync_after_propagation_accept {
                            "Disable Auto Sync"
                        } else {
                            "Enable Auto Sync"
                        },
                        Message::ToggleAutoSyncAfterPropagationAccept,
                    ),
                    subtle_button("Sync Now", Message::SyncPropagationNow),
                ]
                .spacing(8)
                .wrap(),
                text(format!(
                    "auto after propagation-node accept: {}",
                    self.app.settings.auto_sync_after_propagation_accept
                ))
                .size(ui_size(14))
                .wrapping(Wrapping::WordOrGlyph)
                .width(Length::Fill),
                text(format!(
                    "throttle interval: {}s | sync limit: {}",
                    self.app.settings.lxmf_sync_interval, self.app.settings.lxmf_sync_limit
                ))
                .size(ui_size(14))
                .wrapping(Wrapping::WordOrGlyph)
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
}

fn app_scrollable<'a>(content: impl Into<Element<'a, Message>>) -> Scrollable<'a, Message> {
    scrollable(scroll_gutter(content))
        .direction(ScrollableDirection::Vertical(compact_scrollbar()))
        .style(themed_scrollable_style)
        .width(Length::Fill)
}

fn scroll_gutter<'a>(content: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    container(content)
        .padding(Padding {
            right: desktop_scroll_gutter_right(),
            ..Padding::default()
        })
        .width(Length::Fill)
        .into()
}

fn desktop_scroll_gutter_right() -> f32 {
    f32::from(DESKTOP_SCROLLBAR_WIDTH + DESKTOP_SCROLLBAR_MARGIN + DESKTOP_SCROLL_GUTTER_EXTRA)
}

fn compact_scrollbar() -> Scrollbar {
    Scrollbar::new()
        .width(DESKTOP_SCROLLBAR_WIDTH)
        .scroller_width(DESKTOP_SCROLLBAR_SCROLLER_WIDTH)
        .margin(DESKTOP_SCROLLBAR_MARGIN)
}

fn omen_button<'a>(label: &'a str, message: Message) -> Button<'a, Message> {
    button(text(label))
        .on_press(message)
        .style(omen_button_style)
}

fn subtle_button<'a>(label: &'a str, message: Message) -> Button<'a, Message> {
    button(text(label))
        .on_press(message)
        .style(subtle_button_style)
}

fn wrapped_panel_text(content: &str) -> Text<'_> {
    text(content)
        .size(ui_size(14))
        .wrapping(Wrapping::WordOrGlyph)
        .width(Length::Fill)
}

fn wrapped_text_owned(content: impl Into<String>, size: u16) -> Text<'static> {
    text(content.into())
        .size(ui_size(size))
        .wrapping(Wrapping::WordOrGlyph)
        .width(Length::Fill)
}

fn tooltip_icon_button<'a>(
    icon: &'a str,
    label: &'static str,
    message: Message,
) -> Element<'a, Message> {
    tooltip_button(
        button(centered_toolbar_icon(icon))
            .on_press(message)
            .padding(0)
            .width(Length::Fixed(toolbar_icon_button_side()))
            .height(Length::Fixed(toolbar_icon_button_side()))
            .style(subtle_button_style),
        label,
    )
}

fn tooltip_omen_icon_button<'a>(
    icon: &'a str,
    label: &'static str,
    message: Message,
) -> Element<'a, Message> {
    tooltip_button(
        button(centered_toolbar_icon(icon))
            .on_press(message)
            .padding(0)
            .width(Length::Fixed(toolbar_icon_button_side()))
            .height(Length::Fixed(toolbar_icon_button_side()))
            .style(omen_button_style),
        label,
    )
}

fn tooltip_warning_icon_button<'a>(
    icon: &'a str,
    label: &'static str,
    message: Message,
) -> Element<'a, Message> {
    tooltip_button(
        button(centered_toolbar_icon(icon))
            .on_press(message)
            .padding(0)
            .width(Length::Fixed(toolbar_icon_button_side()))
            .height(Length::Fixed(toolbar_icon_button_side()))
            .style(warning_button_style),
        label,
    )
}

fn centered_toolbar_icon(icon: &str) -> Element<'_, Message> {
    let side = toolbar_icon_content_side();
    container(
        text(format!("{icon} "))
            .font(nerd_icon_font())
            .size(ui_size(16)),
    )
    .center_x(Length::Fixed(side))
    .center_y(Length::Fixed(side))
    .into()
}

fn toolbar_icon_button_side() -> f32 {
    f32::from(ui_size(30)).max(26.0)
}

fn toolbar_icon_content_side() -> f32 {
    (toolbar_icon_button_side() - 4.0).max(22.0)
}

fn tooltip_button<'a>(button: Button<'a, Message>, label: &'static str) -> Element<'a, Message> {
    tooltip(
        button,
        container(text(label).size(ui_size(12)))
            .padding([4, 8])
            .style(status_container_style),
        tooltip::Position::Top,
    )
    .gap(Pixels(8.0))
    .into()
}

fn inline_icon_button_owned(
    icon: &'static str,
    label: &'static str,
    message: Message,
) -> Element<'static, Message> {
    tooltip_button(
        button(
            text(format!("{icon} "))
                .font(nerd_icon_font())
                .size(ui_size(14)),
        )
        .on_press(message)
        .padding([0, 2])
        .style(inline_icon_button_style),
        label,
    )
}

fn conversation_attachment_draft_rows(
    conversation_id: u64,
    attachments: &[PathBuf],
) -> Element<'static, Message> {
    if attachments.is_empty() {
        return container(text("")).height(Length::Shrink).into();
    }

    let mut rows = column![text("Attachments").size(ui_size(12))]
        .spacing(3)
        .width(Length::Fill);
    for (index, path) in attachments.iter().enumerate() {
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("attachment")
            .to_string();
        let size = std::fs::metadata(path)
            .ok()
            .filter(|metadata| metadata.is_file())
            .map(|metadata| human_bytes(metadata.len()))
            .unwrap_or_else(|| "missing".into());
        rows = rows.push(
            row![
                text(format!("{name} ({size})"))
                    .size(ui_size(12))
                    .width(Length::Fill)
                    .wrapping(Wrapping::WordOrGlyph),
                inline_icon_button_owned(
                    ICON_OPEN,
                    "Open attachment",
                    Message::OpenConversationAttachment(path.clone())
                ),
                inline_icon_button_owned(
                    ICON_WINDOW_CLOSE,
                    "Remove attachment",
                    Message::RemoveConversationAttachment {
                        conversation_id,
                        index,
                    }
                ),
            ]
            .spacing(6)
            .align_y(iced::Alignment::Center)
            .wrap(),
        );
    }

    container(rows)
        .padding([4, 6])
        .style(status_container_style)
        .width(Length::Fill)
        .into()
}

fn conversation_message_attachment_rows<'a>(
    message: &'a crate::messaging::MessageSummary,
) -> Element<'a, Message> {
    if message.attachments.is_empty() {
        return container(text("")).height(Length::Shrink).into();
    }

    let mut rows = column![text("Attachments").size(ui_size(12))]
        .spacing(3)
        .width(Length::Fill);
    for attachment in &message.attachments {
        let mut row = row![text(format!(
            "{} ({})",
            attachment.name,
            human_bytes(attachment.size)
        ))
        .size(ui_size(12))
        .width(Length::Fill)
        .wrapping(Wrapping::WordOrGlyph),]
        .spacing(6)
        .align_y(iced::Alignment::Center);
        if let Some(path) = attachment.path.as_ref() {
            row = row.push(inline_icon_button_owned(
                ICON_OPEN,
                "Open attachment",
                Message::OpenConversationAttachment(path.clone()),
            ));
        }
        rows = rows.push(row.wrap());
    }

    container(rows)
        .padding([4, 6])
        .style(status_container_style)
        .width(Length::Fill)
        .into()
}

fn restore_pane_button(
    icon: &'static str,
    label: String,
    message: Message,
    unread: bool,
) -> Button<'static, Message> {
    let style = if unread {
        warning_button_style
    } else {
        subtle_button_style
    };
    button(
        text(format!("{icon} {label}"))
            .font(nerd_icon_font())
            .size(ui_size(15)),
    )
    .on_press(message)
    .style(style)
}

fn subtle_button_owned(label: String, message: Message) -> Button<'static, Message> {
    button(text(label))
        .on_press(message)
        .style(subtle_button_style)
}

fn warning_button<'a>(label: &'a str, message: Message) -> Button<'a, Message> {
    button(text(label))
        .on_press(message)
        .style(warning_button_style)
}

fn warning_button_owned(label: String, message: Message) -> Button<'static, Message> {
    button(text(label))
        .on_press(message)
        .style(warning_button_style)
}

fn action_grid<'a>(actions: Vec<Button<'a, Message>>, _max_per_row: usize) -> Element<'a, Message> {
    actions
        .into_iter()
        .fold(row![].spacing(8), |row, action| row.push(action))
        .wrap()
        .into()
}

fn conversation_editor_text(editor: &text_editor::Content) -> String {
    editor
        .lines()
        .map(|line| line.to_string())
        .collect::<Vec<_>>()
        .join("\n")
}

fn conversation_scroll_id(conversation_id: u64) -> ScrollableId {
    ScrollableId::new(format!("conversation-scroll-{conversation_id}"))
}

#[cfg(feature = "chat-client")]
fn omenchat_scroll_id(session_id: ChatSessionId, room_id: RoomId) -> ScrollableId {
    ScrollableId::new(format!("omenchat-scroll-{session_id}-{room_id}"))
}

fn sanitize_scroll_offset(offset: RelativeOffset) -> RelativeOffset {
    RelativeOffset {
        x: if offset.x.is_finite() {
            offset.x.clamp(0.0, 1.0)
        } else {
            0.0
        },
        y: if offset.y.is_finite() {
            offset.y.clamp(0.0, 1.0)
        } else {
            1.0
        },
    }
}

fn scroll_offset_is_at_bottom(offset: RelativeOffset) -> bool {
    sanitize_scroll_offset(offset).y >= 0.95
}

fn scroll_offset_should_show_history_notice(offset: RelativeOffset) -> bool {
    sanitize_scroll_offset(offset).y <= 0.88
}

#[cfg(all(feature = "chat-client", feature = "chat-client-rns"))]
fn omenchat_recent_sync_wants_bottom_restore(events: &[ChatClientEvent]) -> bool {
    events.iter().any(|event| {
        matches!(
            event,
            ChatClientEvent::HistoryPrepended { .. } | ChatClientEvent::HistorySynced { .. }
        )
    })
}

fn directory_tab_button(
    label: &'static str,
    kind: crate::directory::DirectoryKind,
    active: &crate::directory::DirectoryKind,
    count: usize,
) -> Button<'static, Message> {
    let title = format!("{label} ({count})");
    if &kind == active {
        omen_button_owned(title, Message::SwitchDirectoryKind(kind))
    } else {
        subtle_button_owned(title, Message::SwitchDirectoryKind(kind))
    }
}

fn directory_scope_button(
    label: &'static str,
    scope: DirectoryScope,
    active: &DirectoryScope,
) -> Button<'static, Message> {
    if &scope == active {
        omen_button_owned(label.to_string(), Message::SwitchDirectoryScope(scope))
    } else {
        subtle_button_owned(label.to_string(), Message::SwitchDirectoryScope(scope))
    }
}

fn omen_button_owned(label: String, message: Message) -> Button<'static, Message> {
    button(text(label))
        .on_press(message)
        .style(omen_button_style)
}

fn directory_kind_title(kind: &crate::directory::DirectoryKind) -> &'static str {
    match kind {
        crate::directory::DirectoryKind::Node => "Nodes",
        crate::directory::DirectoryKind::Peer => "Peers",
        crate::directory::DirectoryKind::Propagation => "Propagation Nodes",
        crate::directory::DirectoryKind::OmenChat => "OMENchat Servers",
        crate::directory::DirectoryKind::Unknown => "Unknown Announces",
    }
}

fn directory_empty_text(kind: &crate::directory::DirectoryKind) -> &'static str {
    match kind {
        crate::directory::DirectoryKind::Node => "No recent NomadNet node announces yet.",
        crate::directory::DirectoryKind::Peer => "No recent LXMF peer announces yet.",
        crate::directory::DirectoryKind::Propagation => {
            "No recent LXMF propagation node announces yet."
        }
        crate::directory::DirectoryKind::OmenChat => "No recent OMENchat server announces yet.",
        crate::directory::DirectoryKind::Unknown => "No unknown announces.",
    }
}

fn directory_empty_text_for_scope(default: &str, scope: &DirectoryScope, filter: &str) -> String {
    if !filter.trim().is_empty() {
        return format!(
            "No directory entries match \"{}\" in this tab.",
            filter.trim()
        );
    }
    match scope {
        DirectoryScope::Live => default.to_string(),
        DirectoryScope::Saved => "No saved entries in this directory tab.".into(),
        DirectoryScope::Trusted => "No trusted entries in this directory tab.".into(),
    }
}

fn short_destination_hash(hash: &str) -> String {
    let trimmed = hash.trim();
    let char_count = trimmed.chars().count();
    if char_count <= 18 {
        return trimmed.to_string();
    }
    let head = trimmed.chars().take(10).collect::<String>();
    let tail = trimmed
        .chars()
        .rev()
        .take(6)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<String>();
    format!("{head}...{tail}")
}

fn directory_selected_kind_note(entry: &crate::directory::DirectoryEntry) -> String {
    match entry.kind {
        crate::directory::DirectoryKind::Node => {
            "node: Browse opens /page/index.mu in a browser tab".into()
        }
        crate::directory::DirectoryKind::Peer => {
            "peer: Message opens an LXMF conversation; Inspect checks identity/path readiness"
                .into()
        }
        crate::directory::DirectoryKind::Propagation => {
            "propagation: Use Propagation selects this node for propagated LXMF sync/send".into()
        }
        crate::directory::DirectoryKind::OmenChat => {
            "omenchat: Open Chat connects to this OMENchat server with a Reticulum Link".into()
        }
        crate::directory::DirectoryKind::Unknown => {
            "unknown: announce is preserved but not classified as node, peer, or propagation".into()
        }
    }
}

fn directory_selected_primary_action_labels(
    kind: &crate::directory::DirectoryKind,
) -> Vec<&'static str> {
    match kind {
        crate::directory::DirectoryKind::Node => vec!["Browse Node"],
        crate::directory::DirectoryKind::Peer => vec!["Message Peer", "Inspect Peer"],
        crate::directory::DirectoryKind::Propagation => vec!["Use Propagation"],
        crate::directory::DirectoryKind::OmenChat => vec!["Open Chat"],
        crate::directory::DirectoryKind::Unknown => vec!["Select"],
    }
}

fn directory_selected_primary_actions(
    index: usize,
    kind: &crate::directory::DirectoryKind,
) -> Element<'static, Message> {
    match kind {
        crate::directory::DirectoryKind::Node => row![omen_button(
            "Browse Node",
            Message::OpenDirectoryEntry(index)
        )]
        .spacing(8)
        .wrap()
        .into(),
        crate::directory::DirectoryKind::Peer => row![
            omen_button("Message Peer", Message::OpenPeerChat(index)),
            subtle_button("Inspect Peer", Message::InspectDirectoryPeer(index)),
        ]
        .spacing(8)
        .wrap()
        .into(),
        crate::directory::DirectoryKind::Propagation => row![omen_button(
            "Use Propagation",
            Message::UseDirectoryPropagation(index)
        )]
        .spacing(8)
        .wrap()
        .into(),
        #[cfg(feature = "chat-client")]
        crate::directory::DirectoryKind::OmenChat => row![omen_button(
            "Open Chat",
            Message::OpenDirectoryOmenChat(index)
        )]
        .spacing(8)
        .wrap()
        .into(),
        #[cfg(not(feature = "chat-client"))]
        crate::directory::DirectoryKind::OmenChat => row![subtle_button(
            "Select",
            Message::SelectDirectoryEntry(index)
        )]
        .spacing(8)
        .wrap()
        .into(),
        crate::directory::DirectoryKind::Unknown => row![subtle_button(
            "Select",
            Message::SelectDirectoryEntry(index)
        )]
        .spacing(8)
        .wrap()
        .into(),
    }
}

fn directory_selected_state_lines(entry: &crate::directory::DirectoryEntry) -> Vec<String> {
    let sort_rank = entry
        .sort_rank
        .map(|rank| rank.to_string())
        .unwrap_or_else(|| "default".into());
    let mut lines = vec![format!(
        "trust: {:?} | trusted={} | saved={}",
        entry.trust_level, entry.trusted, entry.saved
    )];
    match entry.kind {
        crate::directory::DirectoryKind::Node => {
            lines.push(format!(
                "identify on connect: {} | sort rank: {}",
                entry.identify_on_connect, sort_rank
            ));
            lines.push(format!("hosts NomadNet pages: {}", entry.hosts_node));
        }
        crate::directory::DirectoryKind::Peer => {
            let preferred_delivery = entry
                .preferred_delivery
                .as_ref()
                .map(|delivery| format!("{delivery:?}"))
                .unwrap_or_else(|| "default".into());
            lines.push(format!("preferred LXMF delivery: {preferred_delivery}"));
        }
        crate::directory::DirectoryKind::Propagation => {
            lines.push(format!("propagation candidate rank: {sort_rank}"));
        }
        crate::directory::DirectoryKind::OmenChat => {
            lines.push(format!("OMENchat server rank: {sort_rank}"));
        }
        crate::directory::DirectoryKind::Unknown => {
            lines.push(format!("announce sort rank: {sort_rank}"));
        }
    }
    lines
}

fn directory_kind_supports_identify_toggle(kind: &crate::directory::DirectoryKind) -> bool {
    matches!(kind, crate::directory::DirectoryKind::Node)
}

fn directory_kind_supports_delivery_preference(kind: &crate::directory::DirectoryKind) -> bool {
    matches!(kind, crate::directory::DirectoryKind::Peer)
}

fn request_status_label(status: &BrowserRequestStatus) -> &'static str {
    match status {
        BrowserRequestStatus::Preview => "preview",
        BrowserRequestStatus::Pending => "pending",
        BrowserRequestStatus::Completed => "completed",
        BrowserRequestStatus::Failed => "failed",
    }
}

fn request_preview_line(
    tab: &crate::app::BrowserTab,
    preview: &crate::app::BrowserRequestPreview,
) -> String {
    let submission = if preview.fields.is_empty() && preview.request_data.is_empty() {
        None
    } else {
        let field_count = preview
            .request_data
            .keys()
            .filter(|key| key.starts_with("field_"))
            .count()
            .max(
                preview
                    .fields
                    .iter()
                    .filter(|field| !field.contains('='))
                    .count(),
            );
        let variable_count = preview
            .request_data
            .keys()
            .filter(|key| key.starts_with("var_"))
            .count()
            .max(
                preview
                    .fields
                    .iter()
                    .filter(|field| field.contains('='))
                    .count(),
            );
        Some(format!(
            "captured submission: {field_count} field(s), {variable_count} variable(s)"
        ))
    };

    let retry = tab
        .retry_state
        .as_ref()
        .filter(|retry| retry.target == preview.target);
    let action = match (preview.status.clone(), retry) {
        (BrowserRequestStatus::Pending, Some(retry))
            if retry.ready_epoch_ms.is_some()
                && retry.retry_after_epoch_ms <= current_epoch_ms() =>
        {
            "path ready; press Retry if the page does not open automatically".to_string()
        }
        (BrowserRequestStatus::Pending, Some(retry)) if retry.ready_epoch_ms.is_some() => {
            "path ready; waiting briefly before page load".to_string()
        }
        (BrowserRequestStatus::Pending, Some(_)) => {
            "waiting for path evidence before page load".to_string()
        }
        (BrowserRequestStatus::Pending, None) => "request pending".to_string(),
        (BrowserRequestStatus::Failed, Some(retry)) if retry.ready_epoch_ms.is_some() => {
            "request failed; path state is ready for retry".to_string()
        }
        (BrowserRequestStatus::Failed, Some(_)) => {
            "request failed; request path or wait for an announce, then retry".to_string()
        }
        (BrowserRequestStatus::Failed, None) => format!("request failed: {}", preview.detail),
        (BrowserRequestStatus::Preview, _) => preview.detail.clone(),
        (BrowserRequestStatus::Completed, _) => format!("loaded {}", preview.target),
    };

    match submission {
        Some(submission) => format!("{action} | {submission}"),
        None => action,
    }
}

fn browser_request_preview_has_path_actions(
    tab: &crate::app::BrowserTab,
    preview: &crate::app::BrowserRequestPreview,
) -> bool {
    tab.retry_state.as_ref().is_some_and(|retry| {
        retry.target == preview.target
            && (matches!(
                preview.status,
                BrowserRequestStatus::Pending | BrowserRequestStatus::Failed
            ))
            && (retry.reason.contains("auto-load when path is known")
                || retry.ready_epoch_ms.is_some()
                || matches!(preview.status, BrowserRequestStatus::Failed))
    })
}

fn browser_request_preview_retry_ready(
    tab: &crate::app::BrowserTab,
    preview: &crate::app::BrowserRequestPreview,
) -> bool {
    tab.retry_state.as_ref().is_some_and(|retry| {
        retry.target == preview.target
            && retry.ready_epoch_ms.is_some()
            && retry.retry_after_epoch_ms <= current_epoch_ms()
    })
}

fn compact_label(value: &str, max_chars: usize) -> String {
    let value = printable_label(value.trim());
    let count = value.chars().count();
    if count <= max_chars {
        value
    } else {
        let keep = max_chars.saturating_sub(3);
        format!("{}...", value.chars().take(keep).collect::<String>())
    }
}

fn printable_label(value: &str) -> String {
    value.chars().filter(|ch| !ch.is_control()).collect()
}

fn emoji_aware_text<'a>(value: String, size: u16) -> Element<'a, Message> {
    if !value.chars().any(is_emoji_like) {
        return text(value).size(size).into();
    }

    let mut runs: Vec<(bool, String)> = Vec::new();
    for ch in value.chars() {
        let emoji = is_emoji_like(ch);
        if let Some((last_emoji, run)) = runs.last_mut() {
            if *last_emoji == emoji {
                run.push(ch);
                continue;
            }
        }
        runs.push((emoji, ch.to_string()));
    }

    runs.into_iter()
        .fold(row![].spacing(0), |row, (emoji, run)| {
            let mut label = text(run).size(size);
            if emoji {
                label = label.font(emoji_font());
            }
            row.push(label)
        })
        .wrap()
        .into()
}

fn is_emoji_like(ch: char) -> bool {
    matches!(
        ch as u32,
        0x1F000..=0x1FAFF | 0x2600..=0x27BF | 0x2300..=0x23FF
    )
}

fn inert_address_display(value: String) -> Element<'static, Message> {
    container(text(value).size(ui_size(14)))
        .width(Length::Fill)
        .padding([6, 8])
        .style(address_display_container_style)
        .into()
}

fn visible_tab_window(total: usize, active: usize, max_visible: usize) -> (usize, usize) {
    if total == 0 {
        return (0, 0);
    }
    let max_visible = max_visible.max(1).min(total);
    let active = active.min(total - 1);
    let mut start = active.saturating_sub(max_visible / 2);
    if start + max_visible > total {
        start = total - max_visible;
    }
    (start, start + max_visible)
}

fn directory_entry_matches_view(
    entry: &crate::directory::DirectoryEntry,
    kind: &crate::directory::DirectoryKind,
    scope: &DirectoryScope,
    filter: &str,
) -> bool {
    if &entry.kind != kind {
        return false;
    }
    let scope_matches = match scope {
        DirectoryScope::Live => directory_entry_is_recent(entry),
        DirectoryScope::Saved => entry.saved,
        DirectoryScope::Trusted => entry.trusted,
    };
    scope_matches && directory_entry_matches_filter(entry, filter)
}

fn directory_entry_matches_filter(entry: &crate::directory::DirectoryEntry, filter: &str) -> bool {
    let filter = filter.trim().to_lowercase();
    if filter.is_empty() {
        return true;
    }

    let mut haystack = format!(
        "{} {} {:?} {:?} {:?}",
        entry.display_name,
        entry.destination_hash,
        entry.kind,
        entry.trust_level,
        entry.preferred_delivery
    )
    .to_lowercase();
    if let Some(hash) = &entry.associated_hash {
        haystack.push(' ');
        haystack.push_str(&hash.to_lowercase());
    }
    if let Some(hash) = &entry.node_associated_hash {
        haystack.push(' ');
        haystack.push_str(&hash.to_lowercase());
    }
    if entry.saved {
        haystack.push_str(" saved");
    }
    if entry.trusted {
        haystack.push_str(" trusted");
    }
    if entry.identify_on_connect {
        haystack.push_str(" identify");
    }

    filter
        .split_whitespace()
        .all(|token| haystack.contains(token))
}

fn directory_entry_is_recent(entry: &crate::directory::DirectoryEntry) -> bool {
    let now_secs = crate::app::current_epoch_ms() as f64 / 1_000.0;
    entry.last_seen > 0.0 && now_secs - entry.last_seen <= 6.0 * 60.0 * 60.0
}

fn directory_row_action_labels(kind: &crate::directory::DirectoryKind) -> Vec<&'static str> {
    match kind {
        crate::directory::DirectoryKind::Node => vec!["Select", "Browse Node"],
        crate::directory::DirectoryKind::Peer => vec!["Select", "Message Peer"],
        crate::directory::DirectoryKind::Propagation => vec!["Select", "Use Propagation"],
        crate::directory::DirectoryKind::OmenChat => vec!["Select", "Open Chat"],
        crate::directory::DirectoryKind::Unknown => vec!["Select"],
    }
}

fn relative_time(epoch_secs: f64) -> String {
    if epoch_secs <= 0.0 {
        return "never".into();
    }
    let now_secs = crate::app::current_epoch_ms() as f64 / 1_000.0;
    let elapsed = (now_secs - epoch_secs).max(0.0) as u64;
    match elapsed {
        0..=59 => format!("{elapsed}s ago"),
        60..=3_599 => format!("{}m ago", elapsed / 60),
        3_600..=86_399 => format!("{}h ago", elapsed / 3_600),
        _ => format!("{}d ago", elapsed / 86_400),
    }
}

fn format_epoch_secs(epoch_secs: f64) -> String {
    if epoch_secs <= 0.0 {
        return "never".into();
    }
    format_epoch_ms((epoch_secs * 1_000.0) as u64)
}

fn format_epoch_ms(epoch_ms: u64) -> String {
    let total_seconds = (epoch_ms / 1_000) as i64;
    let days = total_seconds.div_euclid(86_400);
    let seconds_of_day = total_seconds.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    let hour = seconds_of_day / 3_600;
    let minute = (seconds_of_day % 3_600) / 60;
    let second = seconds_of_day % 60;
    format!("{year:04}-{month:02}-{day:02} {hour:02}:{minute:02}:{second:02} UTC")
}

fn civil_from_days(days_since_epoch: i64) -> (i32, u32, u32) {
    let z = days_since_epoch + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    let year = year + if month <= 2 { 1 } else { 0 };
    (year as i32, month as u32, day as u32)
}

fn section_card<'a>(
    title: impl Into<String>,
    body: impl Into<Element<'a, Message>>,
) -> Element<'a, Message> {
    container(
        column![
            text(title.into())
                .size(ui_size(20))
                .wrapping(Wrapping::WordOrGlyph)
                .width(Length::Fill),
            body.into()
        ]
        .spacing(10),
    )
    .style(card_container_style)
    .padding(14)
    .width(Length::Fill)
    .into()
}

fn interface_runtime_status_label(
    profile: &crate::interfaces::ReticulumInterfaceProfile,
    stats: Option<&crate::runtime::InterfaceStats>,
) -> String {
    let Some(stats) = stats else {
        if !profile.enabled {
            return "runtime: disabled by profile".into();
        }
        return "runtime: disconnected; waiting for native runtime status".into();
    };
    if !profile.enabled {
        return "runtime: disabled by profile".into();
    }
    if !stats.available {
        return format!(
            "runtime: not running ({})",
            stats
                .reason
                .as_deref()
                .unwrap_or("interface stats unavailable")
        );
    }

    if let Some(sample) = stats
        .samples
        .iter()
        .find(|sample| sample.profile_id == profile.profile_id || sample.name == profile.name)
    {
        match sample.state {
            crate::runtime::network::InterfaceSampleState::Disabled => {
                return "runtime: disabled by profile".into();
            }
            crate::runtime::network::InterfaceSampleState::Unsupported => {
                return "runtime: unsupported".into();
            }
            crate::runtime::network::InterfaceSampleState::Attached => {
                return "runtime: connected".into();
            }
            crate::runtime::network::InterfaceSampleState::Configured
            | crate::runtime::network::InterfaceSampleState::Unknown => {}
        }
        return "runtime: disconnected".into();
    }

    let profile_name = profile.name.to_ascii_lowercase();
    let profile_host = profile.target_host.to_ascii_lowercase();
    let profile_endpoint = if profile.target_host.is_empty() || profile.target_port == 0 {
        String::new()
    } else {
        format!(
            "{}:{}",
            profile.target_host.to_ascii_lowercase(),
            profile.target_port
        )
    };
    let profile_kind = format!("{:?}", profile.kind).to_ascii_lowercase();
    let attached = stats.interfaces.iter().find(|line| {
        let line = line.to_ascii_lowercase();
        line.starts_with("attached ")
            && (line.contains(&profile_name)
                || (!profile_endpoint.is_empty() && line.contains(&profile_endpoint))
                || (!profile_host.is_empty() && line.contains(&profile_host)))
    });
    if let Some(line) = attached {
        let _ = line;
        return "runtime: connected".into();
    }

    let matched_plan = stats.interfaces.iter().find(|line| {
        let line = line.to_ascii_lowercase();
        line.contains(&profile_name)
            || line.contains(&profile_kind) && line.contains(&profile_name)
            || (!profile_host.is_empty() && line.contains(&profile_host))
            || (!profile_endpoint.is_empty() && line.contains(&profile_endpoint))
            || line.contains(&profile.profile_id.to_ascii_lowercase())
    });

    if let Some(_line) = matched_plan {
        "runtime: disconnected".into()
    } else {
        "runtime: disconnected; enabled profile is not attached to the native runtime".into()
    }
}

fn optional_interface_runtime_detail_line<'a>(
    profile: &crate::interfaces::ReticulumInterfaceProfile,
    stats: Option<&crate::runtime::InterfaceStats>,
) -> Element<'a, Message> {
    match interface_runtime_detail_line(profile, stats) {
        Some(line) => text(line)
            .size(ui_size(13))
            .wrapping(Wrapping::WordOrGlyph)
            .width(Length::Fill)
            .into(),
        None => container(column![]).into(),
    }
}

fn interface_runtime_detail_line(
    profile: &crate::interfaces::ReticulumInterfaceProfile,
    stats: Option<&crate::runtime::InterfaceStats>,
) -> Option<String> {
    let stats = stats?;
    if !profile.enabled {
        return None;
    }
    if !stats.available {
        return stats
            .reason
            .as_deref()
            .map(|reason| format!("runtime detail: {reason}"));
    }

    if let Some(sample) = stats
        .samples
        .iter()
        .find(|sample| sample.profile_id == profile.profile_id || sample.name == profile.name)
    {
        return match sample.state {
            crate::runtime::network::InterfaceSampleState::Disabled => None,
            crate::runtime::network::InterfaceSampleState::Unsupported => Some(format!(
                "runtime detail: {}",
                sample
                    .detail
                    .as_deref()
                    .unwrap_or("native startup is not implemented for this interface")
            )),
            crate::runtime::network::InterfaceSampleState::Attached
            | crate::runtime::network::InterfaceSampleState::Configured
            | crate::runtime::network::InterfaceSampleState::Unknown => Some(format!(
                "runtime detail: {}",
                sample
                    .detail
                    .as_deref()
                    .or(sample.endpoint.as_deref())
                    .unwrap_or("configured, but not attached to the native runtime")
            )),
        };
    }

    let profile_name = profile.name.to_ascii_lowercase();
    let profile_host = profile.target_host.to_ascii_lowercase();
    let profile_endpoint = if profile.target_host.is_empty() || profile.target_port == 0 {
        String::new()
    } else {
        format!(
            "{}:{}",
            profile.target_host.to_ascii_lowercase(),
            profile.target_port
        )
    };
    let profile_kind = format!("{:?}", profile.kind).to_ascii_lowercase();
    let attached = stats.interfaces.iter().find(|line| {
        let lower = line.to_ascii_lowercase();
        lower.starts_with("attached ")
            && (lower.contains(&profile_name)
                || (!profile_endpoint.is_empty() && lower.contains(&profile_endpoint))
                || (!profile_host.is_empty() && lower.contains(&profile_host)))
    });
    if let Some(line) = attached {
        return Some(format!("runtime detail: {line}"));
    }

    stats
        .interfaces
        .iter()
        .find(|line| {
            let lower = line.to_ascii_lowercase();
            lower.contains(&profile_name)
                || lower.contains(&profile_kind) && lower.contains(&profile_name)
                || (!profile_host.is_empty() && lower.contains(&profile_host))
                || (!profile_endpoint.is_empty() && lower.contains(&profile_endpoint))
                || lower.contains(&profile.profile_id.to_ascii_lowercase())
        })
        .map(|line| format!("runtime detail: {line}"))
}

fn interface_runtime_state_line(
    profile: &crate::interfaces::ReticulumInterfaceProfile,
    stats: Option<&crate::runtime::InterfaceStats>,
) -> String {
    let endpoint = if profile.target_host.is_empty() || profile.target_port == 0 {
        "no endpoint".to_string()
    } else {
        format!("{}:{}", profile.target_host, profile.target_port)
    };
    let Some(stats) = stats else {
        return format!("state: disconnected | endpoint: {endpoint}");
    };
    if !profile.enabled {
        return format!("state: disabled | endpoint: {endpoint}");
    }
    if !stats.available {
        let reason = stats
            .reason
            .as_deref()
            .unwrap_or("interface stats unavailable");
        return format!("state: runtime unavailable | {reason}");
    }

    if let Some(sample) = stats
        .samples
        .iter()
        .find(|sample| sample.profile_id == profile.profile_id || sample.name == profile.name)
    {
        let endpoint = sample.endpoint.as_deref().unwrap_or(endpoint.as_str());
        let state = interface_sample_state_label(&sample.state);
        return format!("state: {state} | endpoint: {endpoint}");
    }

    let profile_name = profile.name.to_ascii_lowercase();
    let profile_host = profile.target_host.to_ascii_lowercase();
    let profile_endpoint = if profile.target_host.is_empty() || profile.target_port == 0 {
        String::new()
    } else {
        format!(
            "{}:{}",
            profile.target_host.to_ascii_lowercase(),
            profile.target_port
        )
    };
    let attached = stats.interfaces.iter().any(|line| {
        let line = line.to_ascii_lowercase();
        line.starts_with("attached ")
            && (line.contains(&profile_name)
                || (!profile_endpoint.is_empty() && line.contains(&profile_endpoint))
                || (!profile_host.is_empty() && line.contains(&profile_host)))
    });
    if attached {
        return format!(
            "state: {} | endpoint: {endpoint}",
            interface_sample_state_label(&crate::runtime::network::InterfaceSampleState::Attached)
        );
    }

    format!("state: disconnected | endpoint: {endpoint}")
}

fn section_needs_runtime_interface_sample(section: WorkspaceSection) -> bool {
    matches!(
        section,
        WorkspaceSection::Interfaces | WorkspaceSection::Monitoring
    )
}

fn monitoring_interface_status_lines(stats: &crate::runtime::InterfaceStats) -> Vec<String> {
    let mut lines = vec![format!(
        "runtime: {} | {}",
        if stats.available {
            "available"
        } else {
            "unavailable"
        },
        stats
            .reason
            .as_deref()
            .unwrap_or("interface stats available")
    )];

    if !stats.samples.is_empty() {
        lines.extend(stats.samples.iter().map(|sample| {
            let state = interface_sample_state_label(&sample.state);
            let endpoint = sample.endpoint.as_deref().unwrap_or("no endpoint");
            let detail = sample
                .detail
                .as_deref()
                .filter(|detail| !detail.is_empty())
                .unwrap_or("");
            if detail.is_empty() {
                format!(
                    "{} | {} | {} | {}",
                    sample.name, sample.kind, state, endpoint
                )
            } else {
                format!(
                    "{} | {} | {} | {} | {}",
                    sample.name, sample.kind, state, endpoint, detail
                )
            }
        }));
        return lines;
    }

    if stats.interfaces.is_empty() {
        lines.push("interfaces: none reported".into());
    } else {
        lines.extend(
            stats
                .interfaces
                .iter()
                .map(|line| format!("interface: {line}")),
        );
    }
    lines
}

fn interface_sample_state_label(
    state: &crate::runtime::network::InterfaceSampleState,
) -> &'static str {
    match state {
        crate::runtime::network::InterfaceSampleState::Disabled => "disabled",
        crate::runtime::network::InterfaceSampleState::Unsupported => "unsupported",
        crate::runtime::network::InterfaceSampleState::Attached => "connected; auto-retry enabled",
        crate::runtime::network::InterfaceSampleState::Configured => "disconnected",
        crate::runtime::network::InterfaceSampleState::Unknown => "unknown",
    }
}

fn monitoring_interface_reconnect_line(stats: Option<&crate::runtime::InterfaceStats>) -> String {
    let Some(stats) = stats else {
        return "interface reconnect: waiting for native interface status".into();
    };
    if !stats.available {
        return format!(
            "interface reconnect: stats unavailable ({})",
            stats
                .reason
                .as_deref()
                .unwrap_or("runtime has not reported interface stats")
        );
    }
    if stats.interfaces.is_empty() && stats.samples.is_empty() {
        return "interface reconnect: no interfaces reported; configure or enable a gateway".into();
    }

    if stats
        .samples
        .iter()
        .any(|sample| sample.state == crate::runtime::network::InterfaceSampleState::Attached)
    {
        return "interface reconnect: connected; TCP gateways retry automatically after drops"
            .into();
    }
    if stats
        .samples
        .iter()
        .any(|sample| sample.state == crate::runtime::network::InterfaceSampleState::Configured)
    {
        return "interface reconnect: enabled gateway disconnected; restart runtime after interface edits".into();
    }

    let joined = stats.interfaces.join("\n").to_ascii_lowercase();
    if joined.contains("connected=true")
        || joined.contains("connected=yes")
        || joined.contains("connected=connected")
        || joined.contains("connected=online")
    {
        return "interface reconnect: connected; TCP gateways retry automatically after drops"
            .into();
    }
    if joined.contains("connected=false")
        || joined.contains("disconnected")
        || joined.contains("couldn't connect")
        || joined.contains("connection error")
        || joined.contains("connection closed")
    {
        return "interface reconnect: gateway appears offline/retrying; TCP clients retry automatically".into();
    }

    "interface reconnect: interfaces reported; verify connected=true in detailed lines".into()
}

fn monitoring_metric_card<'a>(
    title: &'static str,
    value: String,
    detail: String,
) -> Element<'a, Message> {
    container(
        column![
            text(title).size(ui_size(13)),
            text(value).size(ui_size(22)),
            text(detail).size(ui_size(12)),
        ]
        .spacing(4),
    )
    .style(status_container_style)
    .padding(12)
    .width(Length::FillPortion(1))
    .into()
}

fn monitoring_runtime_attribution_lines(
    monitoring: &crate::app::MonitoringPanelState,
    uptime_secs: u64,
) -> Vec<String> {
    let uptime_minutes = (uptime_secs.max(1) as f64 / 60.0).max(1.0 / 60.0);
    let browser_tx = monitoring
        .outbound_page_requests
        .saturating_add(monitoring.outbound_partial_refreshes)
        .saturating_add(monitoring.outbound_file_downloads);
    let path_tx = monitoring
        .outbound_path_requests
        .saturating_add(monitoring.outbound_path_warmups);
    let lxmf_tx = monitoring
        .outbound_lxmf_sends
        .saturating_add(monitoring.outbound_propagation_syncs);
    let app_tx = monitoring
        .outbound_diagnostics
        .saturating_add(monitoring.outbound_status_updates);
    let page_rx = monitoring
        .inbound_page_responses
        .saturating_add(monitoring.inbound_downloads);
    let discovery_rx = monitoring
        .announces_received
        .saturating_add(monitoring.path_updates_received);
    let lxmf_rx = monitoring
        .inbound_messages
        .saturating_add(monitoring.lxmf_evidence_updates)
        .saturating_add(monitoring.propagation_sync_events);
    let total_tx_ops = browser_tx
        .saturating_add(path_tx)
        .saturating_add(lxmf_tx)
        .saturating_add(app_tx);
    let total_rx_ops = page_rx.saturating_add(discovery_rx).saturating_add(lxmf_rx);
    let tx_classes = [
        ("browser", browser_tx),
        ("path", path_tx),
        ("lxmf", lxmf_tx),
        ("app/status", app_tx),
    ];
    let rx_classes = [
        ("pages/files", page_rx),
        ("discovery", discovery_rx),
        ("lxmf", lxmf_rx),
    ];
    let top_tx = dominant_runtime_class(&tx_classes);
    let top_rx = dominant_runtime_class(&rx_classes);
    let tx_per_min = total_tx_ops as f64 / uptime_minutes;
    let rx_per_min = total_rx_ops as f64 / uptime_minutes;
    let outbound_bytes_per_min = monitoring.estimated_outbound_bytes as f64 / uptime_minutes;
    let inbound_bytes_per_min = monitoring.estimated_inbound_bytes as f64 / uptime_minutes;
    let activity_hint = if total_tx_ops == 0 && total_rx_ops == 0 {
        "activity: idle; no runtime traffic recorded yet".into()
    } else {
        format!(
            "activity: top tx={} ({}) | top rx={} ({}) | {} tx/min, {} rx/min",
            top_tx.0,
            top_tx.1,
            top_rx.0,
            top_rx.1,
            format_rate(tx_per_min),
            format_rate(rx_per_min)
        )
    };
    let mut lines = vec![
        "read this: browser spikes mean page/download traffic; path spikes mean route discovery; lxmf spikes mean message/propagation work".into(),
        format!(
            "tx by class: browser={browser_tx} path={path_tx} lxmf={lxmf_tx} app/status={app_tx}"
        ),
        format!("rx by class: pages/files={page_rx} discovery={discovery_rx} lxmf={lxmf_rx}"),
        activity_hint,
        format!(
            "operation rate: {:.1} tx/min | {:.1} rx/min | {:.1} runtime events/min",
            tx_per_min,
            rx_per_min,
            monitoring.runtime_events_total as f64 / uptime_minutes
        ),
        format!(
            "byte estimate: {} tx / {} rx | rate {} tx/min / {} rx/min",
            human_bytes(monitoring.estimated_outbound_bytes),
            human_bytes(monitoring.estimated_inbound_bytes),
            human_bytes(outbound_bytes_per_min.round() as u64),
            human_bytes(inbound_bytes_per_min.round() as u64)
        ),
    ];
    if monitoring.runtime_errors > 0 {
        lines.push(format!(
            "attention: {} runtime error(s) recorded; inspect Logs and Diagnostics before blaming traffic volume",
            monitoring.runtime_errors
        ));
    }
    let browser_attempts = monitoring
        .outbound_page_requests
        .saturating_add(monitoring.outbound_file_downloads);
    if browser_attempts > 0 {
        lines.push(format!(
            "browser health: {} page/download request(s), {} response/download result(s), {} path operation(s)",
            browser_attempts,
            page_rx,
            path_tx
        ));
    }
    if browser_attempts >= 2 && page_rx == 0 {
        lines.push(
            "attention: browser requests have no page/file responses yet; check Browser live warning, Request Path, then Diagnostics".into(),
        );
    } else if path_tx > browser_attempts.saturating_mul(2).max(3) {
        lines.push(
            "attention: path traffic is high relative to page loads; wait for path pass before retrying repeated links".into(),
        );
    }
    if monitoring.outbound_lxmf_sends > 0 || monitoring.outbound_propagation_syncs > 0 {
        lines.push(format!(
            "LXMF health: sends={} propagation_syncs={} evidence={} inbound={}",
            monitoring.outbound_lxmf_sends,
            monitoring.outbound_propagation_syncs,
            monitoring.lxmf_evidence_updates,
            monitoring.inbound_messages
        ));
    }
    if monitoring.outbound_lxmf_sends > 0 && monitoring.lxmf_evidence_updates == 0 {
        lines.push(
            "attention: LXMF sends have no delivery evidence yet; check selected peer/path and Diagnostics before resending".into(),
        );
    }
    lines
}

fn dominant_runtime_class<'a>(classes: &'a [(&'a str, u64)]) -> (&'a str, u64) {
    classes
        .iter()
        .copied()
        .max_by(|left, right| left.1.cmp(&right.1).then_with(|| right.0.cmp(left.0)))
        .unwrap_or(("none", 0))
}

fn format_rate(value: f64) -> String {
    if value >= 10.0 {
        format!("{value:.0}")
    } else {
        format!("{value:.1}")
    }
}

fn monitoring_meter<'a>(label: &'static str, value: usize, max: usize) -> Element<'a, Message> {
    let max = max.max(1);
    let filled = ((value.min(max) * 18) + max / 2) / max;
    let empty = 18usize.saturating_sub(filled);
    let percent = (value.min(max) * 100) / max;
    text(format!(
        "{label:<16} [{}{}] {:>3}% ({value}/{max})",
        "#".repeat(filled),
        ".".repeat(empty),
        percent
    ))
    .size(ui_size(14))
    .into()
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct ProcessResourceUsage {
    rss_bytes: u64,
    cpu_seconds: f64,
}

fn process_resource_usage() -> Option<ProcessResourceUsage> {
    let page_size = 4096u64;
    let statm = std::fs::read_to_string("/proc/self/statm").ok()?;
    let rss_pages = statm.split_whitespace().nth(1)?.parse::<u64>().ok()?;
    let stat = std::fs::read_to_string("/proc/self/stat").ok()?;
    let close = stat.rfind(')')?;
    let fields = stat
        .get(close + 2..)?
        .split_whitespace()
        .collect::<Vec<_>>();
    let utime = fields.get(11)?.parse::<u64>().ok()?;
    let stime = fields.get(12)?.parse::<u64>().ok()?;
    let ticks_per_second = 100.0;
    Some(ProcessResourceUsage {
        rss_bytes: rss_pages.saturating_mul(page_size),
        cpu_seconds: (utime + stime) as f64 / ticks_per_second,
    })
}

fn human_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} {}", UNITS[unit])
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

#[cfg(feature = "chat-client")]
fn is_omenchat_local_echo_event(event: &ChatEvent) -> bool {
    event.event_id > u64::MAX.saturating_sub(1_000_000)
}

#[cfg(feature = "chat-client")]
fn omenchat_upload_policy_rejection(
    bytes: u64,
    quota: Option<u64>,
    max_file_bytes: Option<u64>,
) -> Option<String> {
    match quota {
        Some(0) => Some("upload blocked: server has uploads disabled".into()),
        _ => match max_file_bytes {
            Some(limit) if bytes > limit => Some(format!(
                "upload blocked: {} exceeds server file limit {}",
                human_bytes(bytes),
                human_bytes(limit)
            )),
            _ => None,
        },
    }
}

fn compact_elapsed_ms(milliseconds: u64) -> String {
    let seconds = milliseconds / 1_000;
    if seconds < 60 {
        format!("{seconds}s")
    } else if seconds < 3_600 {
        format!("{}m {}s", seconds / 60, seconds % 60)
    } else {
        format!("{}h {}m", seconds / 3_600, (seconds % 3_600) / 60)
    }
}

#[cfg(feature = "chat-client-rns")]
fn omenchat_monitor_health_line(totals: &OmenChatLiveMonitorTotals) -> String {
    if totals.sessions == 0 {
        return "health: no OMENchat sessions open".into();
    }
    if totals.reconnect_timers > 0 || totals.opening > 0 {
        return format!(
            "health: reconnect/opening activity visible ({} opening, {} timer(s))",
            totals.opening, totals.reconnect_timers
        );
    }
    if totals.awaiting_pongs > 0 {
        return format!(
            "health: waiting for {} heartbeat pong(s); watch for repeated disconnects",
            totals.awaiting_pongs
        );
    }
    if totals.history_sync_waiting > 0 {
        return format!(
            "health: {} session(s) catching up recent history",
            totals.history_sync_waiting
        );
    }
    if totals.pending_resources > 0 {
        return format!(
            "health: {} pending Resource offer(s); media/history may still be loading",
            totals.pending_resources
        );
    }
    if totals.upload_fetches_out > 0
        || totals.upload_inline_chunks_in > 0
        || totals.upload_resources_in > 0
    {
        return format!(
            "health: media/upload traffic active ({} fetches, {} inline, {} resource bytes)",
            totals.upload_fetches_out,
            totals.upload_inline_chunks_in,
            human_bytes(totals.upload_resource_bytes_in)
        );
    }
    if totals.connected > 0 && totals.frames_in == 0 && totals.frames_out == 0 {
        return "health: connected and quiet; no OMENchat frames yet".into();
    }
    format!(
        "health: ok; {} connected session(s), {} rx / {} tx",
        totals.connected,
        human_bytes(totals.bytes_in),
        human_bytes(totals.bytes_out)
    )
}

#[cfg(feature = "chat-client-rns")]
struct OmenChatSessionAttention<'a> {
    connected: bool,
    opening: bool,
    reconnect_queued: bool,
    awaiting_pong: bool,
    last_ping_age_ms: Option<u64>,
    heartbeat_idle_ms: Option<u64>,
    pending_resources: usize,
    history_sync_label: &'a str,
}

#[cfg(feature = "chat-client-rns")]
fn omenchat_session_attention_line(attention: OmenChatSessionAttention<'_>) -> String {
    if attention.opening {
        return "attention: opening live OMENchat link".into();
    }
    if attention.reconnect_queued {
        return "attention: reconnect queued; waiting for retry timer".into();
    }
    if !attention.connected {
        return "attention: disconnected; use Reconnect after path is known".into();
    }
    if attention.awaiting_pong {
        let last_ping = attention
            .last_ping_age_ms
            .map(compact_elapsed_ms)
            .unwrap_or_else(|| "unknown".into());
        let heartbeat = attention
            .heartbeat_idle_ms
            .unwrap_or(OMENCHAT_HEARTBEAT_IDLE_MS);
        if attention
            .last_ping_age_ms
            .is_some_and(|age| age >= heartbeat.saturating_mul(2))
        {
            return format!("attention: heartbeat pong overdue; last ping {last_ping} ago");
        }
        return format!("attention: waiting for heartbeat pong; last ping {last_ping} ago");
    }
    if attention.history_sync_label.contains("stopped") {
        return "attention: recent history sync stopped; reconnect can retry catch-up".into();
    }
    if attention.history_sync_label.contains("waiting")
        || attention.history_sync_label.contains("retry")
        || attention.history_sync_label.contains("due now")
        || attention.history_sync_label.contains("not yet confirmed")
    {
        return "attention: recent history sync pending".into();
    }
    if attention.pending_resources > 0 {
        let pending_resources = attention.pending_resources;
        return format!(
            "attention: {pending_resources} pending Resource offer(s); media/history may still be loading"
        );
    }
    "attention: live link healthy; no action needed".into()
}

fn message_conversation_header(
    conversation: &crate::messaging::Conversation,
) -> Element<'_, Message> {
    let status = if conversation.pending_send.is_some() {
        "sending"
    } else {
        "ready"
    };
    container(
        row![
            text(format!("delivery: {:?}", conversation.delivery_mode)).size(ui_size(13)),
            text(format!(
                "{} messages | {} unread | {status}",
                conversation.thread.messages.len(),
                conversation.thread.unread_count
            ))
            .size(ui_size(13)),
        ]
        .spacing(10)
        .wrap(),
    )
    .style(status_container_style)
    .padding([6, 10])
    .width(Length::Fill)
    .into()
}

fn message_bubble<'a>(
    conversation_id: u64,
    message: &'a crate::messaging::MessageSummary,
    selected: bool,
) -> Element<'a, Message> {
    let author = if message.incoming { "Peer" } else { "You" };
    let message_key = message_summary_key(message);
    let mut content = column![
        row![
            text(author).size(ui_size(13)),
            text(format_epoch_secs(message.timestamp)).size(ui_size(12)),
        ]
        .spacing(8)
        .wrap(),
        text(message_title_line(message))
            .size(ui_size(14))
            .width(Length::Fill)
            .wrapping(Wrapping::WordOrGlyph),
        text(compact_message_preview(&message.content))
            .size(ui_size(15))
            .width(Length::Fill)
            .wrapping(Wrapping::WordOrGlyph),
    ]
    .spacing(4);

    if let Some(summary) = lxmf_message_compact_status(message) {
        content = content.push(text(summary).size(ui_size(13)));
    }

    if !message.attachments.is_empty() {
        content = content.push(conversation_message_attachment_rows(message));
    }

    let mut actions = vec![subtle_button_owned(
        if selected {
            "Selected".into()
        } else {
            "Details".into()
        },
        Message::SelectConversationPaneRow {
            conversation_id,
            key: message_key.clone(),
        },
    )];
    if selected {
        if desktop_message_is_retry_candidate(message) {
            let retry_key = message_key.clone();
            let labels = desktop_message_retry_labels(message);
            actions.push(subtle_button_owned(
                labels.prepare.into(),
                Message::PrepareLxmfRetryForConversationRow {
                    conversation_id,
                    key: message_key,
                },
            ));
            actions.push(omen_button_owned(
                labels.send.into(),
                Message::SendLxmfRetryForConversationRow {
                    conversation_id,
                    key: retry_key,
                },
            ));
        }
        if let Some(sync_label) = desktop_message_propagation_sync_label(message) {
            actions.push(omen_button_owned(
                sync_label.into(),
                Message::SyncPropagationForConversationRow {
                    conversation_id,
                    key: message_summary_key(message),
                },
            ));
        }
    }
    if message.failed {
        actions.push(subtle_button_owned(
            "Close".into(),
            Message::DismissConversationPaneRow {
                conversation_id,
                key: message_summary_key(message),
            },
        ));
    }
    content = content.push(action_grid(actions, 4));

    let bubble = container(content)
        .style(if selected {
            selected_message_container_style
        } else if message.incoming {
            incoming_message_container_style
        } else if message.failed {
            failed_message_container_style
        } else {
            outgoing_message_container_style
        })
        .padding(12)
        .width(Length::FillPortion(5));

    if message.incoming {
        row![bubble, container(text("")).width(Length::FillPortion(1)),]
            .spacing(8)
            .into()
    } else {
        row![container(text("")).width(Length::FillPortion(1)), bubble,]
            .spacing(8)
            .into()
    }
}

fn selected_message_details_card(
    conversation_id: u64,
    conversation: &crate::messaging::Conversation,
) -> Element<'_, Message> {
    let Some(selected_key) = conversation.selected_message_key.as_deref() else {
        return container(text("")).height(Length::Shrink).into();
    };
    let Some(message) = conversation
        .thread
        .messages
        .iter()
        .find(|message| message_summary_key(message) == selected_key)
    else {
        return container(text("")).height(Length::Shrink).into();
    };

    let header = row![
        text(if message.incoming {
            "Incoming message"
        } else {
            "Outgoing message"
        })
        .size(ui_size(14)),
        text(format_epoch_secs(message.timestamp)).size(ui_size(13)),
        text(format!("transport: {:?}", message.transport_method)).size(ui_size(13)),
        subtle_button(
            "Close",
            Message::CloseConversationPaneDetails { conversation_id }
        ),
    ]
    .spacing(10)
    .wrap();
    let mut header_actions = Vec::new();
    if desktop_message_is_retry_candidate(message) {
        let retry_key = message_summary_key(message);
        let labels = desktop_message_retry_labels(message);
        header_actions.push(subtle_button_owned(
            labels.prepare.into(),
            Message::PrepareLxmfRetryForConversationRow {
                conversation_id,
                key: retry_key.clone(),
            },
        ));
        header_actions.push(omen_button_owned(
            labels.send.into(),
            Message::SendLxmfRetryForConversationRow {
                conversation_id,
                key: retry_key,
            },
        ));
    }
    if let Some(sync_label) = desktop_message_propagation_sync_label(message) {
        header_actions.push(omen_button_owned(
            sync_label.into(),
            Message::SyncPropagationForConversationRow {
                conversation_id,
                key: message_summary_key(message),
            },
        ));
    }

    let mut body = column![
        header,
        action_grid(header_actions, 3),
        text(format!("subject: {}", message_title_line(message))).size(ui_size(13)),
        text(format!(
            "state: delivered={} failed={} unread={}",
            message.delivered, message.failed, message.unread
        ))
        .size(ui_size(13)),
        text(format!(
            "message id: {}",
            message.message_id.as_deref().unwrap_or("<none>")
        ))
        .size(ui_size(13)),
    ]
    .spacing(5);

    for line in lxmf_message_status_lines(message) {
        body = body.push(text(line).size(ui_size(13)));
    }
    if message.fields.is_empty() {
        body = body.push(text("LXMF fields: none recorded").size(ui_size(13)));
    }

    section_card("Message Details", body)
}

fn message_title_line(message: &crate::messaging::MessageSummary) -> String {
    if message.title.trim().is_empty() {
        "(no subject)".into()
    } else {
        message.title.clone()
    }
}

fn compact_message_preview(content: &str) -> String {
    let mut preview = String::new();
    let mut char_count = 0usize;
    let mut truncated = false;

    for (line_index, line) in content.lines().enumerate() {
        if line_index >= CONVERSATION_PREVIEW_LINES {
            truncated = true;
            break;
        }
        if line_index > 0 {
            preview.push('\n');
        }
        for ch in line.chars() {
            if char_count >= CONVERSATION_PREVIEW_CHARS {
                truncated = true;
                break;
            }
            preview.push(ch);
            char_count += 1;
        }
        if truncated {
            break;
        }
    }

    if preview.is_empty() && !content.is_empty() {
        preview = content.chars().take(CONVERSATION_PREVIEW_CHARS).collect();
        truncated = content.chars().count() > CONVERSATION_PREVIEW_CHARS;
    }
    if truncated {
        preview.push_str("...");
    }
    preview
}

fn compact_footer_status(message: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }
    let compact = message.split_whitespace().collect::<Vec<_>>().join(" ");
    if compact.chars().count() <= max_chars {
        return compact;
    }
    if max_chars <= 3 {
        return compact.chars().take(max_chars).collect();
    }
    let mut clipped = compact.chars().take(max_chars - 3).collect::<String>();
    clipped.push_str("...");
    clipped
}

fn compact_identity_status_label(message: &str) -> String {
    message
        .trim()
        .strip_prefix("identity:")
        .map(str::trim)
        .filter(|label| !label.is_empty())
        .unwrap_or_else(|| message.trim())
        .to_owned()
}

fn setup_tcp_client_editor(app: &App) -> Element<'_, Message> {
    if let Some(profile) = setup_tcp_client_profile(app) {
        let host_id = profile.profile_id.clone();
        let port_id = profile.profile_id.clone();
        let ifac_network_id = profile.profile_id.clone();
        let ifac_pass_id = profile.profile_id.clone();
        column![
            row![
                text("TCP gateway").size(ui_size(14)),
                text_input("host", &profile.target_host)
                    .on_input(move |value| Message::TcpClientHostChanged {
                        profile_id: host_id.clone(),
                        value,
                    })
                    .width(Length::FillPortion(2)),
                text_input("port", &profile.target_port.to_string())
                    .on_input(move |value| Message::TcpClientPortChanged {
                        profile_id: port_id.clone(),
                        value,
                    })
                    .width(Length::FillPortion(1)),
            ]
            .spacing(8)
            .wrap(),
            row![
                text("IFAC").size(ui_size(14)),
                text_input("network name", &profile.network_name)
                    .on_input(move |value| Message::TcpClientIfacNetworkChanged {
                        profile_id: ifac_network_id.clone(),
                        value,
                    })
                    .width(Length::FillPortion(2)),
                text_input("passphrase", &profile.passphrase)
                    .secure(true)
                    .on_input(move |value| Message::TcpClientIfacPassphraseChanged {
                        profile_id: ifac_pass_id.clone(),
                        value,
                    })
                    .width(Length::FillPortion(2)),
            ]
            .spacing(8)
            .wrap(),
        ]
        .spacing(6)
        .into()
    } else {
        row![
            text("TCP gateway: none configured").size(ui_size(14)),
            subtle_button("Add TCP Gateway", Message::CreateTcpClientInterface),
            subtle_button(
                "Open Interfaces",
                Message::SwitchSection(WorkspaceSection::Interfaces)
            ),
        ]
        .spacing(8)
        .wrap()
        .into()
    }
}

fn setup_tcp_client_profile(app: &App) -> Option<&crate::interfaces::ReticulumInterfaceProfile> {
    app.interfaces_state
        .profiles
        .iter()
        .find(|profile| profile.kind == InterfaceKind::TcpClient)
}

#[cfg(feature = "chat-client")]
fn apply_omenchat_link_fields(descriptor: &mut OmenChatDescriptor, fields: &[String]) {
    for field in fields {
        let Some((key, value)) = field.split_once('=') else {
            continue;
        };
        match key.trim() {
            "name" | "display_name" => {
                if !value.trim().is_empty() {
                    descriptor.display_name = Some(value.trim().to_owned());
                }
            }
            "lxmf" => {
                if !value.trim().is_empty() {
                    descriptor.server_lxmf_destination = Some(value.trim().to_owned());
                }
            }
            "theme" => {
                if !value.trim().is_empty() {
                    descriptor.theme_hint = Some(value.trim().to_owned());
                }
            }
            "rooms" | "rooms_hint" => {
                descriptor.rooms_hint = value
                    .split(',')
                    .map(str::trim)
                    .filter(|room| !room.is_empty())
                    .map(ToOwned::to_owned)
                    .collect();
            }
            _ => {}
        }
    }
}

#[cfg(feature = "chat-client")]
fn normalize_omenchat_manual_target(input: &str) -> Option<String> {
    let trimmed = input.trim();
    let destination = trimmed
        .strip_prefix("omenchat://")
        .or_else(|| trimmed.strip_prefix("omenchat:"))
        .unwrap_or(trimmed)
        .trim()
        .trim_start_matches('/');
    if destination.len() < 32 || !destination.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return None;
    }
    Some(format!("omenchat://{}", destination.to_ascii_lowercase()))
}

#[cfg(feature = "chat-client")]
fn is_pending_omenchat_destination(destination: &str) -> bool {
    destination.starts_with(OMENCHAT_PENDING_DESTINATION_PREFIX)
}

fn pick_conversation_attachment_file() -> Result<Option<PathBuf>, String> {
    Ok(rfd::FileDialog::new()
        .set_title("Select LXMF attachment")
        .pick_file())
}

#[cfg(feature = "chat-client")]
fn unique_chat_users(users: &[ChatUserSummary]) -> Vec<&ChatUserSummary> {
    let mut seen_ids = HashSet::new();
    let mut seen_names = HashSet::new();
    let mut unique = Vec::new();
    for user in users {
        let normalized_name = user.display_name.trim().to_ascii_lowercase();
        if !seen_ids.insert(user.user_id) && !normalized_name.is_empty() {
            continue;
        }
        if !normalized_name.is_empty() && !seen_names.insert(normalized_name) {
            continue;
        }
        unique.push(user);
    }
    unique
}

#[cfg(feature = "chat-client")]
fn omenchat_command_result_from_events(events: &[ChatClientEvent]) -> OmenChatDraftCommandResult {
    if events
        .iter()
        .any(|event| matches!(event, ChatClientEvent::Error { .. }))
    {
        OmenChatDraftCommandResult::HandledKeep
    } else {
        OmenChatDraftCommandResult::HandledClear
    }
}

#[cfg(feature = "chat-client")]
fn pick_omenchat_upload_file() -> Result<Option<PathBuf>, String> {
    Ok(rfd::FileDialog::new()
        .set_title("Select OMENchat upload")
        .pick_file())
}

#[cfg(feature = "chat-client")]
fn omenchat_upload_content_type(filename: &str) -> Option<String> {
    let ext = std::path::Path::new(filename)
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase())?;
    let content_type = match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "txt" | "log" | "md" => "text/plain",
        "json" => "application/json",
        "pdf" => "application/pdf",
        "zip" => "application/zip",
        _ => "application/octet-stream",
    };
    Some(content_type.into())
}

#[cfg(feature = "chat-client")]
fn chat_event_actor_label(session: &ChatSessionView, event: &ChatEvent) -> String {
    event.actor_display_name.clone().unwrap_or_else(|| {
        event
            .actor_user_id
            .and_then(|actor_id| {
                session
                    .users
                    .iter()
                    .find(|user| user.user_id == actor_id)
                    .map(|user| user.display_name.clone())
            })
            .unwrap_or_else(|| match event.kind {
                ChatEventKind::System { .. } => "system".into(),
                ChatEventKind::Upload { .. } => "upload".into(),
                ChatEventKind::Notice { .. } => "notice".into(),
                _ => "unknown".into(),
            })
    })
}

#[cfg(feature = "chat-client")]
struct ChatTimelineGroup {
    actor_key: String,
    actor: String,
    at_unix: i64,
    last_at_unix: i64,
    bodies: Vec<ChatTimelineBody>,
}

#[cfg(feature = "chat-client")]
struct ChatTimelineBody {
    text: String,
    is_action: bool,
    upload: Option<ChatTimelineUpload>,
    resend: Option<ChatTimelineResend>,
}

#[cfg(feature = "chat-client")]
struct ChatTimelineUpload {
    session_id: ChatSessionId,
    resource_id: String,
}

#[cfg(feature = "chat-client")]
struct ChatTimelineResend {
    session_id: ChatSessionId,
    room_id: RoomId,
    event_id: u64,
    body: String,
    action: bool,
}

#[cfg(feature = "chat-client")]
struct OmenChatMediaHint {
    label: String,
    caption: Option<String>,
    open_url: Option<String>,
    open_path: Option<String>,
    load_url: Option<String>,
    image_path: Option<String>,
    animated: bool,
}

#[cfg(feature = "chat-client")]
fn omenchat_media_hints(
    text: &str,
    settings: &crate::storage::settings::ClearwebPrivacySettings,
    detected_proxy: Option<&(String, u16)>,
    server_trusted: bool,
    cache: &HashMap<String, OmenChatMediaLoadState>,
) -> Vec<OmenChatMediaHint> {
    let detected_socks_proxy = detected_proxy.map(|(host, port)| (host.as_str(), *port));
    extract_link_candidates(text)
        .into_iter()
        .filter_map(|url| {
            if let Some(state) = cache.get(&url) {
                return Some(OmenChatMediaHint {
                    label: omenchat_media_state_label(state),
                    caption: omenchat_media_caption(&url, server_trusted),
                    open_url: None,
                    open_path: omenchat_media_state_open_path(state),
                    load_url: None,
                    image_path: omenchat_media_state_image_path(state),
                    animated: omenchat_media_state_is_animated(state),
                });
            }
            let decision = decide_remote_media(RemoteMediaContext {
                url: &url,
                settings,
                detected_socks_proxy,
            });
            if crate::media::is_clearweb_url(&url)
                && !server_trusted
                && matches!(
                    decision,
                    RemoteMediaDecision::AutoInline {
                        transport: RemoteMediaTransport::Socks5 { .. },
                        ..
                    }
                )
            {
                let label = if let Some((host, port)) = detected_socks_proxy {
                    format!(
                        "media blocked: untrusted OMENchat server; Load uses SOCKS5 {host}:{port}"
                    )
                } else {
                    "media blocked: untrusted OMENchat server".into()
                };
                return Some(OmenChatMediaHint {
                    label,
                    caption: None,
                    open_url: None,
                    open_path: None,
                    load_url: Some(url),
                    image_path: None,
                    animated: false,
                });
            }
            match decision {
                RemoteMediaDecision::AutoInline { transport, .. } => {
                    let load_url = matches!(
                        transport,
                        RemoteMediaTransport::Reticulum | RemoteMediaTransport::Socks5 { .. }
                    )
                    .then(|| url.clone());
                    let open_url = matches!(transport, RemoteMediaTransport::ExternalBrowser)
                        .then(|| url.clone());
                    Some(OmenChatMediaHint {
                        label: format!("media: {}", omenchat_media_transport_label(&transport)),
                        caption: None,
                        open_url,
                        open_path: None,
                        load_url,
                        image_path: None,
                        animated: false,
                    })
                }
                RemoteMediaDecision::ManualInline { reason, .. } => Some(OmenChatMediaHint {
                    label: format!("media blocked: {reason}"),
                    caption: None,
                    open_url: Some(url),
                    open_path: None,
                    load_url: None,
                    image_path: None,
                    animated: false,
                }),
                RemoteMediaDecision::ExternalPrompt { reason, .. } => Some(OmenChatMediaHint {
                    label: reason,
                    caption: None,
                    open_url: Some(url),
                    open_path: None,
                    load_url: None,
                    image_path: None,
                    animated: false,
                }),
                RemoteMediaDecision::Unsupported { .. } => None,
            }
        })
        .collect()
}

#[cfg(feature = "chat-client")]
fn omenchat_media_state_label(state: &OmenChatMediaLoadState) -> String {
    match state {
        OmenChatMediaLoadState::Loading {
            message,
            received,
            total,
        } => match (received, total) {
            (Some(received), Some(total)) if *total > 0 => format!(
                "media loading: {} / {}",
                human_bytes(*received),
                human_bytes(*total)
            ),
            _ => format!("media: {message}"),
        },
        OmenChatMediaLoadState::Cached { .. } => String::new(),
        OmenChatMediaLoadState::Failed { message } => format!("media failed: {message}"),
    }
}

#[cfg(feature = "chat-client")]
fn omenchat_upload_state_label(state: &OmenChatMediaLoadState) -> String {
    match state {
        OmenChatMediaLoadState::Loading {
            message,
            received,
            total,
        } => match (received, total) {
            (Some(received), Some(total)) if *total > 0 => format!(
                "loading: {} / {}",
                human_bytes(*received),
                human_bytes(*total)
            ),
            _ if message.trim().is_empty() => "loading".into(),
            _ if message.trim() == "requested upload from server" => "waiting for server".into(),
            _ => format!("loading: {}", compact_status_message(message.trim(), 72)),
        },
        OmenChatMediaLoadState::Cached { .. } => String::new(),
        OmenChatMediaLoadState::Failed { message } => {
            format!("failed: {}", compact_status_message(message.trim(), 72))
        }
    }
}

#[cfg(feature = "chat-client")]
fn compact_status_message(message: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }
    if message.chars().count() <= max_chars {
        return message.to_owned();
    }
    if max_chars <= 3 {
        return message.chars().take(max_chars).collect();
    }
    let mut compact: String = message.chars().take(max_chars - 3).collect();
    compact.push_str("...");
    compact
}

#[cfg(feature = "chat-client")]
fn omenchat_media_loading_state(message: &str) -> OmenChatMediaLoadState {
    OmenChatMediaLoadState::Loading {
        message: message.to_owned(),
        received: None,
        total: None,
    }
}

#[cfg(feature = "chat-client")]
fn omenchat_media_state_image_path(state: &OmenChatMediaLoadState) -> Option<String> {
    match state {
        OmenChatMediaLoadState::Cached {
            path, content_type, ..
        } if content_type.to_ascii_lowercase().starts_with("image/") => Some(path.clone()),
        _ => None,
    }
}

#[cfg(feature = "chat-client")]
fn omenchat_media_state_open_path(state: &OmenChatMediaLoadState) -> Option<String> {
    match state {
        OmenChatMediaLoadState::Cached { path, animated, .. } if *animated => Some(path.clone()),
        _ => None,
    }
}

#[cfg(feature = "chat-client")]
fn omenchat_media_state_is_animated(state: &OmenChatMediaLoadState) -> bool {
    matches!(state, OmenChatMediaLoadState::Cached { animated: true, .. })
}

#[cfg(feature = "chat-client")]
fn omenchat_media_caption(url: &str, server_trusted: bool) -> Option<String> {
    if crate::media::is_clearweb_url(url) {
        let source = clearweb_media_host(url).unwrap_or("clearweb");
        let trust = if server_trusted { "trusted" } else { "manual" };
        return Some(format!("{trust} clearweb image from {source}"));
    }
    if url.trim().to_ascii_lowercase().starts_with("nomadnet://")
        || BrowserAddress::parse(url).is_some()
    {
        return Some("Reticulum/NomadNet image".into());
    }
    None
}

#[cfg(feature = "chat-client")]
fn clearweb_media_host(url: &str) -> Option<&str> {
    url.split_once("://")
        .map(|(_, rest)| rest)
        .and_then(|rest| rest.split(['/', '?', '#']).next())
        .map(|host| host.trim())
        .filter(|host| !host.is_empty())
}

#[cfg(feature = "chat-client")]
fn omenchat_inline_media_element<'a>(
    path: &str,
    animated: bool,
    frames: Option<&'a iced_gif::Frames>,
) -> Element<'a, Message> {
    let (width, height) = omenchat_inline_media_size(Path::new(path))
        .unwrap_or((OMENCHAT_INLINE_MEDIA_MAX_WIDTH, 240.0));
    if animated {
        if let Some(frames) = frames {
            return iced_gif::Gif::new(frames)
                .width(Length::Fixed(width))
                .height(Length::Fixed(height))
                .content_fit(ContentFit::Contain)
                .into();
        }
    }
    image(path.to_owned())
        .width(Length::Fixed(width))
        .height(Length::Fixed(height))
        .content_fit(ContentFit::Contain)
        .into()
}

#[cfg(feature = "chat-client")]
fn omenchat_inline_media_size(path: &Path) -> Option<(f32, f32)> {
    let bytes = read_media_header_bytes(path, OMENCHAT_INLINE_MEDIA_HEADER_BYTES).ok()?;
    let (width, height) = image_dimensions_from_bytes(&bytes)?;
    Some(scale_media_dimensions(
        width,
        height,
        OMENCHAT_INLINE_MEDIA_MAX_WIDTH,
        OMENCHAT_INLINE_MEDIA_MAX_HEIGHT,
    ))
}

#[cfg(feature = "chat-client")]
fn read_media_header_bytes(path: &Path, max_bytes: usize) -> std::io::Result<Vec<u8>> {
    let mut file = std::fs::File::open(path)?;
    let mut bytes = vec![0; max_bytes.max(1)];
    let read = file.read(&mut bytes)?;
    bytes.truncate(read);
    Ok(bytes)
}

#[cfg(feature = "chat-client")]
fn scale_media_dimensions(width: u32, height: u32, max_width: f32, max_height: f32) -> (f32, f32) {
    let width = width.max(1) as f32;
    let height = height.max(1) as f32;
    let scale = (max_width / width).min(max_height / height).min(1.0);
    ((width * scale).max(1.0), (height * scale).max(1.0))
}

#[cfg(feature = "chat-client")]
fn image_dimensions_from_bytes(bytes: &[u8]) -> Option<(u32, u32)> {
    png_dimensions(bytes)
        .or_else(|| gif_dimensions(bytes))
        .or_else(|| jpeg_dimensions(bytes))
}

#[cfg(feature = "chat-client")]
fn png_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    if bytes.len() < 24 || !bytes.starts_with(b"\x89PNG\r\n\x1a\n") || &bytes[12..16] != b"IHDR" {
        return None;
    }
    Some((
        u32::from_be_bytes(bytes[16..20].try_into().ok()?),
        u32::from_be_bytes(bytes[20..24].try_into().ok()?),
    ))
}

#[cfg(feature = "chat-client")]
fn gif_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    if bytes.len() < 10 || (!bytes.starts_with(b"GIF87a") && !bytes.starts_with(b"GIF89a")) {
        return None;
    }
    Some((
        u16::from_le_bytes(bytes[6..8].try_into().ok()?) as u32,
        u16::from_le_bytes(bytes[8..10].try_into().ok()?) as u32,
    ))
}

#[cfg(feature = "chat-client")]
fn jpeg_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    if bytes.len() < 4 || bytes[0] != 0xFF || bytes[1] != 0xD8 {
        return None;
    }
    let mut offset = 2usize;
    while offset + 4 <= bytes.len() {
        while offset < bytes.len() && bytes[offset] != 0xFF {
            offset += 1;
        }
        while offset < bytes.len() && bytes[offset] == 0xFF {
            offset += 1;
        }
        if offset >= bytes.len() {
            return None;
        }
        let marker = bytes[offset];
        offset += 1;
        if matches!(marker, 0xD8 | 0xD9) {
            continue;
        }
        if offset + 2 > bytes.len() {
            return None;
        }
        let segment_len = u16::from_be_bytes(bytes[offset..offset + 2].try_into().ok()?) as usize;
        if segment_len < 2 || offset + segment_len > bytes.len() {
            return None;
        }
        if matches!(
            marker,
            0xC0 | 0xC1
                | 0xC2
                | 0xC3
                | 0xC5
                | 0xC6
                | 0xC7
                | 0xC9
                | 0xCA
                | 0xCB
                | 0xCD
                | 0xCE
                | 0xCF
        ) {
            if segment_len < 7 {
                return None;
            }
            let height = u16::from_be_bytes(bytes[offset + 3..offset + 5].try_into().ok()?) as u32;
            let width = u16::from_be_bytes(bytes[offset + 5..offset + 7].try_into().ok()?) as u32;
            return Some((width, height));
        }
        offset += segment_len;
    }
    None
}

#[cfg(feature = "chat-client")]
fn omenchat_upload_cache_key(session_id: ChatSessionId, resource_id: &str) -> String {
    format!("upload:{session_id}:{resource_id}")
}

#[cfg(feature = "chat-client")]
fn omenchat_media_transport_label(transport: &RemoteMediaTransport) -> String {
    match transport {
        RemoteMediaTransport::Reticulum => "inline via Reticulum/NomadNet".into(),
        RemoteMediaTransport::Socks5 { host, port } => {
            format!("inline via SOCKS5 {host}:{port}")
        }
        RemoteMediaTransport::ExternalBrowser => "external browser required".into(),
    }
}

#[cfg(feature = "chat-client")]
async fn fetch_clearweb_media_over_socks(
    url: &str,
    media_cache_dir: &Path,
    socks_host: &str,
    socks_port: u16,
    timeout: Duration,
) -> anyhow::Result<DownloadedFile> {
    if !crate::media::is_clearweb_url(url) {
        anyhow::bail!("clearweb media fetch requires http or https URL");
    }

    let proxy = reqwest::Proxy::all(format!("socks5h://{socks_host}:{socks_port}"))?;
    let client = reqwest::Client::builder()
        .proxy(proxy)
        .timeout(timeout)
        .user_agent("OMENbrowser_rs/0.1 OMENchat-media")
        .build()?;
    let response = client.get(url).send().await?.error_for_status()?;
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(|value| value.split(';').next().unwrap_or(value).trim().to_string())
        .unwrap_or_else(|| "application/octet-stream".into());
    if !content_type.to_ascii_lowercase().starts_with("image/") {
        anyhow::bail!("remote media response was not an image: {content_type}");
    }

    let bytes = response.bytes().await?;
    const OMENCHAT_MEDIA_MAX_BYTES: usize = 8 * 1024 * 1024;
    if bytes.len() > OMENCHAT_MEDIA_MAX_BYTES {
        anyhow::bail!("remote image is too large: {} bytes", bytes.len());
    }

    let filename = clearweb_media_cache_filename(url, &content_type);
    let path = next_available_download_path(media_cache_dir, &filename)?;
    tokio::fs::write(&path, bytes.as_ref()).await?;
    Ok(DownloadedFile {
        url: url.to_string(),
        path,
        content_type,
    })
}

#[cfg(feature = "chat-client")]
fn clearweb_media_cache_filename(url: &str, content_type: &str) -> String {
    let path = url
        .split("://")
        .nth(1)
        .and_then(|rest| rest.split(['?', '#']).next())
        .and_then(|rest| rest.rsplit('/').next())
        .filter(|name| !name.trim().is_empty())
        .unwrap_or("omenchat-media");
    let mut filename = path.to_string();
    if !filename.contains('.') {
        filename.push('.');
        filename.push_str(match content_type.to_ascii_lowercase().as_str() {
            "image/png" => "png",
            "image/jpeg" => "jpg",
            "image/gif" => "gif",
            "image/webp" => "webp",
            "image/bmp" => "bmp",
            "image/avif" => "avif",
            _ => "img",
        });
    }
    filename
}

#[cfg(feature = "chat-client")]
fn cached_media_is_animated_gif(path: &Path, content_type: &str) -> bool {
    let content_type_is_gif = content_type.eq_ignore_ascii_case("image/gif");
    let extension_is_gif = path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("gif"));
    if !content_type_is_gif && !extension_is_gif {
        return false;
    }
    read_media_header_bytes(path, OMENCHAT_GIF_ANIMATION_SCAN_BYTES)
        .map(|bytes| gif_image_descriptor_count(&bytes, 2) > 1)
        .unwrap_or(false)
}

#[cfg(feature = "chat-client")]
fn gif_image_descriptor_count(bytes: &[u8], stop_after: usize) -> usize {
    if stop_after == 0 || bytes.len() < 13 {
        return 0;
    }
    if !bytes.starts_with(b"GIF87a") && !bytes.starts_with(b"GIF89a") {
        return 0;
    }

    let packed = bytes[10];
    let global_color_table_len = if packed & 0b1000_0000 != 0 {
        3usize.saturating_mul(1usize << (((packed & 0b0000_0111) as usize) + 1))
    } else {
        0
    };
    let mut offset = 13usize.saturating_add(global_color_table_len);
    let mut frames = 0usize;

    while offset < bytes.len() {
        match bytes[offset] {
            0x2C => {
                frames = frames.saturating_add(1);
                if frames >= stop_after {
                    return frames;
                }
                if offset + 10 > bytes.len() {
                    return frames;
                }
                let image_packed = bytes[offset + 9];
                offset += 10;
                if image_packed & 0b1000_0000 != 0 {
                    let local_color_table_len = 3usize
                        .saturating_mul(1usize << (((image_packed & 0b0000_0111) as usize) + 1));
                    offset = offset.saturating_add(local_color_table_len);
                }
                if offset >= bytes.len() {
                    return frames;
                }
                offset += 1; // LZW minimum code size.
                offset = skip_gif_sub_blocks(bytes, offset);
            }
            0x21 => {
                if offset + 2 > bytes.len() {
                    return frames;
                }
                offset = skip_gif_sub_blocks(bytes, offset + 2);
            }
            0x3B => return frames,
            _ => return frames,
        }
    }

    frames
}

#[cfg(feature = "chat-client")]
fn skip_gif_sub_blocks(bytes: &[u8], mut offset: usize) -> usize {
    while offset < bytes.len() {
        let block_len = bytes[offset] as usize;
        offset += 1;
        if block_len == 0 {
            break;
        }
        offset = offset.saturating_add(block_len);
        if offset > bytes.len() {
            return bytes.len();
        }
    }
    offset
}

#[cfg(feature = "chat-client")]
fn chat_event_actor_key(session: &ChatSessionView, event: &ChatEvent) -> String {
    let prefix = match event.kind {
        ChatEventKind::Action { .. } => "action",
        ChatEventKind::Upload { .. } => "upload",
        _ => "message",
    };
    event
        .actor_user_id
        .map(|actor_id| format!("{prefix}:id:{actor_id}"))
        .unwrap_or_else(|| format!("{prefix}:label:{}", chat_event_actor_label(session, event)))
}

#[cfg(feature = "chat-client")]
fn omenchat_event_counts_by_room(
    sessions: &[ChatSessionView],
) -> HashMap<(ChatSessionId, RoomId), usize> {
    let mut counts = HashMap::new();
    for session in sessions {
        counts
            .entry((session.session_id, session.active_room.room_id))
            .or_insert(0);
        for event in &session.events {
            *counts
                .entry((session.session_id, event.room_id))
                .or_insert(0) += 1;
        }
    }
    counts
}

#[cfg(feature = "chat-client")]
fn chat_event_body(session: &ChatSessionView, event: &ChatEvent) -> ChatTimelineBody {
    match &event.kind {
        ChatEventKind::Action { body } => ChatTimelineBody {
            text: format!("* {} {body}", chat_event_actor_label(session, event)),
            is_action: true,
            upload: None,
            resend: local_echo_resend(session, event, body, true),
        },
        ChatEventKind::Message { body }
        | ChatEventKind::Notice { body }
        | ChatEventKind::System { body } => ChatTimelineBody {
            text: body.clone(),
            is_action: false,
            upload: None,
            resend: match &event.kind {
                ChatEventKind::Message { body } => local_echo_resend(session, event, body, false),
                _ => None,
            },
        },
        ChatEventKind::Upload {
            resource_id,
            filename,
            bytes,
        } => ChatTimelineBody {
            text: format!("uploaded {} ({})", filename, human_bytes(*bytes)),
            is_action: false,
            upload: Some(ChatTimelineUpload {
                session_id: session.session_id,
                resource_id: resource_id.clone(),
            }),
            resend: None,
        },
    }
}

#[cfg(feature = "chat-client")]
fn local_echo_resend(
    session: &ChatSessionView,
    event: &ChatEvent,
    body: &str,
    action: bool,
) -> Option<ChatTimelineResend> {
    if !is_omenchat_local_echo_event(event) {
        return None;
    }
    let now = current_epoch_ms() / 1_000;
    if event.at_unix > 0
        && (now as i64).saturating_sub(event.at_unix) < OMENCHAT_LOCAL_ECHO_RESEND_SECS
    {
        return None;
    }
    Some(ChatTimelineResend {
        session_id: session.session_id,
        room_id: event.room_id,
        event_id: event.event_id,
        body: body.to_owned(),
        action,
    })
}

#[cfg(feature = "chat-client")]
fn chat_event_time_label(at_unix: i64) -> String {
    if at_unix <= 0 {
        String::new()
    } else {
        format_epoch_secs(at_unix as f64)
    }
}

#[cfg(feature = "chat-client")]
fn chat_timeline_groups(session: &ChatSessionView) -> Vec<ChatTimelineGroup> {
    let mut groups: Vec<ChatTimelineGroup> = Vec::new();
    for event in session
        .events
        .iter()
        .filter(|event| event.room_id == session.active_room.room_id)
    {
        let actor_key = chat_event_actor_key(session, event);
        let body = chat_event_body(session, event);
        if let Some(last) = groups.last_mut() {
            if last.actor_key == actor_key
                && chat_events_fit_same_group(last.last_at_unix, event.at_unix)
            {
                last.bodies.push(body);
                last.last_at_unix = event.at_unix;
                continue;
            }
        }
        groups.push(ChatTimelineGroup {
            actor_key,
            actor: chat_event_actor_label(session, event),
            at_unix: event.at_unix,
            last_at_unix: event.at_unix,
            bodies: vec![body],
        });
    }
    groups
}

#[cfg(feature = "chat-client")]
fn chat_events_fit_same_group(previous_at_unix: i64, next_at_unix: i64) -> bool {
    if previous_at_unix <= 0 || next_at_unix <= 0 {
        return true;
    }
    next_at_unix.saturating_sub(previous_at_unix) <= OMENCHAT_MESSAGE_GROUP_GAP_SECS
}

#[cfg(feature = "chat-client")]
fn request_session_id(request: &ChatClientRequest) -> Option<ChatSessionId> {
    match request {
        ChatClientRequest::OpenServer(_) => None,
        ChatClientRequest::JoinRoom { session_id, .. }
        | ChatClientRequest::PartRoom { session_id, .. }
        | ChatClientRequest::SendMessage { session_id, .. }
        | ChatClientRequest::SendAction { session_id, .. }
        | ChatClientRequest::SendNotice { session_id, .. }
        | ChatClientRequest::SendUpload { session_id, .. }
        | ChatClientRequest::RequestUpload { session_id, .. }
        | ChatClientRequest::RefreshRooms { session_id }
        | ChatClientRequest::SetTopic { session_id, .. }
        | ChatClientRequest::CreateRoom { session_id, .. }
        | ChatClientRequest::ModerateUser { session_id, .. }
        | ChatClientRequest::SyncRecent { session_id }
        | ChatClientRequest::LoadOlder { session_id } => Some(*session_id),
    }
}

#[cfg(feature = "chat-client-rns")]
fn delayed_omenchat_reconnect_if_disconnected_task(session_id: ChatSessionId) -> Task<Message> {
    Task::perform(
        async move {
            tokio::time::sleep(Duration::from_millis(OMENCHAT_PATH_RECONNECT_DELAY_MS)).await;
            session_id
        },
        Message::ReconnectOmenChatSessionIfDisconnected,
    )
}

#[cfg(feature = "chat-client-rns")]
fn omenchat_live_open_error_status(error: &str) -> String {
    let lower = error.to_ascii_lowercase();
    if lower.contains("request_path") || lower.contains("path to") && lower.contains("not known") {
        "path missing: request path was queued; wait for the server announce/path, then reconnect"
            .into()
    } else if lower.contains("no known identity key") {
        "path/key missing: request path and wait for the server announce, then reopen this OMENchat link"
            .into()
    } else if lower.contains("timed out") && lower.contains("link") {
        "link establishment timed out: path exists but the server did not complete the Link handshake"
            .into()
    } else if lower.contains("timed out") {
        "server response timed out: Link opened, but the server did not answer before the wait limit"
            .into()
    } else if lower.contains("runtime is not running") || lower.contains("runtime is not started") {
        "Reticulum runtime is not running; start/connect the runtime, then reopen this OMENchat link"
            .into()
    } else {
        format!("live link failed: {error}")
    }
}

#[cfg(feature = "chat-client-rns")]
fn omenchat_close_reason_is_timeout(reason: Option<&str>) -> bool {
    reason
        .map(str::trim)
        .is_some_and(|reason| reason.eq_ignore_ascii_case("timeout"))
}

#[cfg(feature = "chat-client-rns")]
fn omenchat_close_reason_allows_quick_reconnect(reason: Option<&str>) -> bool {
    let Some(reason) = reason.map(str::trim) else {
        return false;
    };
    reason.eq_ignore_ascii_case("timeout")
        || reason.eq_ignore_ascii_case("destinationclosed")
        || reason.eq_ignore_ascii_case("initiatorclosed")
}

#[cfg(feature = "chat-client-rns")]
fn hex_bytes(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn restored_desktop_panes(app: &App, omenchat_session_ids: &[u64]) -> Vec<DesktopPane> {
    let mut panes = app
        .settings
        .ui
        .desktop_workspace_panes
        .iter()
        .filter_map(|saved| match saved.kind {
            DesktopWorkspacePaneKind::Browser => app
                .workspace
                .browser_tabs
                .get(saved.index)
                .map(|tab| DesktopPane::Browser(tab.id)),
            DesktopWorkspacePaneKind::Conversation => app
                .workspace
                .conversations
                .get(saved.index)
                .map(|conversation| DesktopPane::Conversation(conversation.id)),
            DesktopWorkspacePaneKind::OmenChat => {
                #[cfg(feature = "chat-client")]
                {
                    omenchat_session_ids
                        .get(saved.index)
                        .copied()
                        .map(DesktopPane::OmenChat)
                }
                #[cfg(not(feature = "chat-client"))]
                {
                    let _ = omenchat_session_ids;
                    None
                }
            }
        })
        .collect::<Vec<_>>();

    if panes.is_empty() {
        panes.push(DesktopPane::Browser(app.active_browser_tab().id));
        panes.push(DesktopPane::Conversation(app.active_conversation().id));
    }
    panes.dedup();
    panes
}

fn restored_desktop_pane_state(
    app: &App,
    omenchat_session_ids: &[u64],
) -> pane_grid::State<DesktopPane> {
    if let Some(layout) = app.settings.ui.desktop_workspace_layout.as_ref() {
        if let Some(config) =
            desktop_layout_node_to_configuration(layout, app, omenchat_session_ids)
        {
            return pane_grid::State::with_configuration(config);
        }
    }

    let restored_panes = restored_desktop_panes(app, omenchat_session_ids);
    let (mut state, first_pane) = pane_grid::State::new(
        restored_panes
            .first()
            .cloned()
            .unwrap_or_else(|| DesktopPane::Browser(app.active_browser_tab().id)),
    );
    for pane in restored_panes.into_iter().skip(1) {
        let target = desktop_pane_order(state.layout())
            .last()
            .copied()
            .unwrap_or(first_pane);
        let _ = state.split(pane_grid::Axis::Vertical, target, pane);
    }
    state
}

fn desktop_layout_node_to_configuration(
    node: &DesktopWorkspaceLayoutNode,
    app: &App,
    omenchat_session_ids: &[u64],
) -> Option<pane_grid::Configuration<DesktopPane>> {
    match node {
        DesktopWorkspaceLayoutNode::Pane { pane } => {
            desktop_pane_from_settings(pane, app, omenchat_session_ids)
                .map(pane_grid::Configuration::Pane)
        }
        DesktopWorkspaceLayoutNode::Split { axis, ratio, a, b } => {
            let a = desktop_layout_node_to_configuration(a, app, omenchat_session_ids)?;
            let b = desktop_layout_node_to_configuration(b, app, omenchat_session_ids)?;
            Some(pane_grid::Configuration::Split {
                axis: desktop_split_axis_to_iced(*axis),
                ratio: sane_desktop_split_ratio(*ratio),
                a: Box::new(a),
                b: Box::new(b),
            })
        }
    }
}

fn desktop_pane_from_settings(
    pane: &DesktopWorkspacePaneSettings,
    app: &App,
    omenchat_session_ids: &[u64],
) -> Option<DesktopPane> {
    match pane.kind {
        DesktopWorkspacePaneKind::Browser => app
            .workspace
            .browser_tabs
            .get(pane.index)
            .map(|tab| DesktopPane::Browser(tab.id)),
        DesktopWorkspacePaneKind::Conversation => app
            .workspace
            .conversations
            .get(pane.index)
            .map(|conversation| DesktopPane::Conversation(conversation.id)),
        DesktopWorkspacePaneKind::OmenChat => {
            #[cfg(feature = "chat-client")]
            {
                omenchat_session_ids
                    .get(pane.index)
                    .copied()
                    .map(DesktopPane::OmenChat)
            }
            #[cfg(not(feature = "chat-client"))]
            {
                let _ = omenchat_session_ids;
                None
            }
        }
    }
}

fn desktop_workspace_node_to_settings(
    node: &pane_grid::Node,
    desktop: &DesktopApp,
) -> Option<DesktopWorkspaceLayoutNode> {
    match node {
        pane_grid::Node::Pane(pane) => {
            let pane = desktop.workspace_panes.get(*pane)?;
            desktop
                .desktop_pane_to_settings(pane)
                .map(|pane| DesktopWorkspaceLayoutNode::Pane { pane })
        }
        pane_grid::Node::Split {
            axis, ratio, a, b, ..
        } => Some(DesktopWorkspaceLayoutNode::Split {
            axis: desktop_split_axis_from_iced(*axis),
            ratio: sane_desktop_split_ratio(*ratio),
            a: Box::new(desktop_workspace_node_to_settings(a, desktop)?),
            b: Box::new(desktop_workspace_node_to_settings(b, desktop)?),
        }),
    }
}

fn desktop_pane_order(node: &pane_grid::Node) -> Vec<pane_grid::Pane> {
    match node {
        pane_grid::Node::Pane(pane) => vec![*pane],
        pane_grid::Node::Split { a, b, .. } => {
            let mut panes = desktop_pane_order(a);
            panes.extend(desktop_pane_order(b));
            panes
        }
    }
}

#[cfg(feature = "chat-client")]
fn prune_unrestorable_omenchat_servers(store: &mut SqliteChatStore) {
    let Ok(servers) = store.saved_servers() else {
        return;
    };
    for server in servers {
        if is_restorable_server_destination(&server.destination) {
            continue;
        }
        if let Err(error) = store.delete_server(&server.server_id) {
            tracing::warn!(
                "failed to prune unrestorable OMENchat server {}: {error}",
                server.server_id
            );
        }
    }
}

fn sane_desktop_split_ratio(ratio: f32) -> f32 {
    if ratio.is_finite() {
        ratio.clamp(0.05, 0.95)
    } else {
        0.5
    }
}

fn desktop_split_axis_to_iced(axis: DesktopWorkspaceSplitAxis) -> pane_grid::Axis {
    match axis {
        DesktopWorkspaceSplitAxis::Horizontal => pane_grid::Axis::Horizontal,
        DesktopWorkspaceSplitAxis::Vertical => pane_grid::Axis::Vertical,
    }
}

fn desktop_split_axis_from_iced(axis: pane_grid::Axis) -> DesktopWorkspaceSplitAxis {
    match axis {
        pane_grid::Axis::Horizontal => DesktopWorkspaceSplitAxis::Horizontal,
        pane_grid::Axis::Vertical => DesktopWorkspaceSplitAxis::Vertical,
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct NativeSetupStep {
    title: &'static str,
    ready: bool,
    detail: String,
}

fn native_setup_steps(app: &App) -> Vec<NativeSetupStep> {
    let identity_ready = app
        .settings
        .identity_path
        .as_ref()
        .is_some_and(|path| path.is_file());
    let backend_ready = matches!(
        app.settings.runtime_backend,
        RuntimeBackendSetting::Reticulum
    );
    let interface_details = app.native_interface_readiness();
    let native_interface_ready = interface_details
        .iter()
        .any(|detail| detail.enabled && detail.supported && !detail.blocks_native_startup);
    let runtime_ready = app.runtime_status.connected && backend_ready;
    let directory_ready = !app.directory_state.entries.is_empty();
    let live_ready = runtime_ready && app.native_reticulum_readiness().ready;

    vec![
        NativeSetupStep {
            title: "Identity",
            ready: identity_ready,
            detail: app
                .settings
                .active_identity_label
                .clone()
                .or_else(|| {
                    app.settings
                        .identity_path
                        .as_ref()
                        .map(|path| path.display().to_string())
                })
                .unwrap_or_else(|| "create or attach a managed Reticulum identity".into()),
        },
        NativeSetupStep {
            title: "Backend",
            ready: backend_ready,
            detail: format!("selected backend: {:?}", app.settings.runtime_backend),
        },
        NativeSetupStep {
            title: "Interface",
            ready: native_interface_ready,
            detail: if native_interface_ready {
                format!(
                    "{} native-supported profile(s) configured",
                    interface_details
                        .iter()
                        .filter(|detail| detail.enabled
                            && detail.supported
                            && !detail.blocks_native_startup)
                        .count()
                )
            } else {
                "add an enabled TCP gateway profile that native Reticulum can start".into()
            },
        },
        NativeSetupStep {
            title: "Runtime",
            ready: runtime_ready,
            detail: if runtime_ready {
                app.runtime_status.message.clone()
            } else {
                "start the native runtime after identity/backend/interface are ready".into()
            },
        },
        NativeSetupStep {
            title: "Directory",
            ready: directory_ready,
            detail: if directory_ready {
                format!(
                    "{} known directory entrie(s)",
                    app.directory_state.entries.len()
                )
            } else {
                "wait for announces, preload known destinations, or run live probe".into()
            },
        },
        NativeSetupStep {
            title: "Live Test",
            ready: live_ready,
            detail: if live_ready {
                "open a NomadNet destination or run LXMF interop from the app".into()
            } else {
                "use Live Fetch/LXMF Interop after runtime and paths are visible".into()
            },
        },
    ]
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct DiagnosticsReportSummary {
    report: String,
    outcome: String,
    stage: String,
    detail: String,
    next_step: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct DiagnosticsStageCard {
    kind: String,
    stage: String,
    status: String,
    detail: String,
    next_step: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct DiagnosticsLiveFetchCard {
    outcome: String,
    stage_hint: String,
    request_backend: String,
    response_size: String,
    detail: String,
    first_failed_stage: String,
    next_step: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct DiagnosticsLxmfDeliveryCard {
    outcome: String,
    send_state: String,
    proof_state: String,
    inbound_state: String,
    event_counts: String,
    readiness_stage: String,
    detail: String,
    next_step: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct DiagnosticsPropagationSyncCard {
    outcome: String,
    selected_node: String,
    before: String,
    after: String,
    events: String,
    event_lines: Vec<String>,
    blocker: String,
    next_step: String,
}

fn diagnostics_preview_report_summary(lines: &[String]) -> Option<DiagnosticsReportSummary> {
    let value: serde_json::Value = serde_json::from_str(&lines.join("\n")).ok()?;
    let report = string_field(&value, &["report"]).unwrap_or_else(|| "diagnostics".into());
    if let Some(classification) = value.get("classification") {
        return Some(DiagnosticsReportSummary {
            report,
            outcome: string_field(classification, &["outcome"]).unwrap_or_else(|| "unknown".into()),
            stage: string_field(classification, &["stage"]).unwrap_or_else(|| "unknown".into()),
            detail: string_field(classification, &["detail", "reason"])
                .unwrap_or_else(|| "no detail in report".into()),
            next_step: string_field(classification, &["next_step", "next_action"])
                .unwrap_or_else(|| "inspect full report".into()),
        });
    }

    if value.get("ready_to_request").is_some() && value.get("steps").is_some() {
        let ready = value
            .get("ready_to_request")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);
        let failed = value
            .get("steps")
            .and_then(serde_json::Value::as_array)
            .and_then(|steps| {
                steps.iter().find(|step| {
                    !step
                        .get("ok")
                        .and_then(serde_json::Value::as_bool)
                        .unwrap_or(false)
                })
            });
        return Some(DiagnosticsReportSummary {
            report: string_field(&value, &["url"]).unwrap_or(report),
            outcome: if ready {
                "ready".into()
            } else {
                "blocked".into()
            },
            stage: failed
                .and_then(|step| string_field(step, &["stage"]))
                .unwrap_or_else(|| "ready_to_request".into()),
            detail: failed
                .and_then(|step| string_field(step, &["detail"]))
                .unwrap_or_else(|| {
                    if ready {
                        "page fetch prerequisites passed".into()
                    } else {
                        "page fetch prerequisites blocked".into()
                    }
                }),
            next_step: if ready {
                "run a live probe or open the page".into()
            } else {
                "inspect failed probe stage and warm/preload paths as needed".into()
            },
        });
    }

    if let Some(status) = value
        .get("path_warmup")
        .and_then(|path_warmup| string_field(path_warmup, &["status"]))
    {
        return Some(DiagnosticsReportSummary {
            report,
            outcome: status.clone(),
            stage: "path_warmup".into(),
            detail: string_field(&value, &["active_browser_url"])
                .or_else(|| string_field(&value, &["destination_hash"]))
                .unwrap_or_else(|| "path warmup report".into()),
            next_step: if status == "path known" || status == "known" {
                "retry the browser request".into()
            } else {
                "wait for path discovery or preload known_destinations".into()
            },
        });
    }

    None
}

fn diagnostics_preview_live_fetch_card(lines: &[String]) -> Option<DiagnosticsLiveFetchCard> {
    let value: serde_json::Value = serde_json::from_str(&lines.join("\n")).ok()?;
    let live_fetch = value.get("live_fetch")?;
    let ok = live_fetch
        .get("ok")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    let stage_hint = string_field(live_fetch, &["stage_hint"])
        .or_else(|| first_failed_page_probe_stage_from_report(&value))
        .unwrap_or_else(|| "unknown".into());
    let request_backend = live_fetch
        .get("metadata")
        .and_then(|metadata| string_field(metadata, &["native_request_backend"]))
        .unwrap_or_else(|| {
            if ok {
                "missing metadata".into()
            } else {
                "not reached".into()
            }
        });
    let response_size = if ok {
        let bytes = live_fetch
            .get("markup_bytes")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0);
        let lines = live_fetch
            .get("markup_lines")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0);
        format!("{bytes} bytes, {lines} lines")
    } else {
        "no response body".into()
    };
    let detail = if ok {
        let title = string_field(live_fetch, &["title"]).unwrap_or_else(|| "untitled".into());
        let url = string_field(live_fetch, &["url"]).unwrap_or_else(|| "unknown url".into());
        format!("{title} from {url}")
    } else {
        string_field(live_fetch, &["error", "skipped"])
            .or_else(|| {
                value
                    .get("classification")
                    .and_then(|classification| string_field(classification, &["reason", "detail"]))
            })
            .unwrap_or_else(|| "live fetch did not complete".into())
    };
    let first_failed_stage = first_failed_page_probe_stage_from_report(&value)
        .or_else(|| {
            value
                .get("classification")
                .and_then(|classification| string_field(classification, &["stage"]))
        })
        .unwrap_or_else(|| {
            if ok {
                "none".into()
            } else {
                stage_hint.clone()
            }
        });
    let next_step = if ok {
        "open the Browser view and inspect the rendered page".into()
    } else {
        value
            .get("classification")
            .and_then(|classification| string_field(classification, &["next_step", "next_action"]))
            .unwrap_or_else(|| "fix the failed stage, then run Native Live Fetch again".into())
    };

    Some(DiagnosticsLiveFetchCard {
        outcome: if ok { "pass" } else { "blocked" }.into(),
        stage_hint,
        request_backend,
        response_size,
        detail,
        first_failed_stage,
        next_step,
    })
}

fn first_failed_page_probe_stage_from_report(value: &serde_json::Value) -> Option<String> {
    ["live_page_probe", "dry_run_page_probe"]
        .iter()
        .find_map(|section| {
            value
                .get(*section)
                .and_then(|probe| probe.get("report"))
                .and_then(|report| report.get("steps"))
                .and_then(serde_json::Value::as_array)
                .and_then(|steps| {
                    steps.iter().find_map(|step| {
                        let ok = step
                            .get("ok")
                            .and_then(serde_json::Value::as_bool)
                            .unwrap_or(false);
                        (!ok).then(|| string_field(step, &["stage"])).flatten()
                    })
                })
        })
}

fn diagnostics_preview_lxmf_delivery_card(lines: &[String]) -> Option<DiagnosticsLxmfDeliveryCard> {
    let value: serde_json::Value = serde_json::from_str(&lines.join("\n")).ok()?;
    let report = lxmf_interop_report_value(&value)?;
    let classification = report.get("classification");
    let send = report.get("send");
    let wait = report.get("wait");
    let outcome = classification
        .and_then(|value| string_field(value, &["outcome"]))
        .or_else(|| wait.and_then(|value| string_field(value, &["status"])))
        .unwrap_or_else(|| "unknown".into());
    let send_state = send
        .map(lxmf_send_state_line)
        .unwrap_or_else(|| "send: not requested".into());
    let proof_state = wait
        .and_then(|value| string_field(value, &["proof_match_state"]))
        .unwrap_or_else(|| "unknown".into());
    let inbound_state = wait
        .and_then(|value| string_field(value, &["inbound_reply_match_state"]))
        .unwrap_or_else(|| "unknown".into());
    let event_counts = wait
        .map(lxmf_event_counts_line)
        .unwrap_or_else(|| "events unavailable".into());
    let readiness_stage = lxmf_first_failed_readiness_stage(report)
        .unwrap_or_else(|| "ready or not requested".into());
    let detail = classification
        .and_then(|value| string_field(value, &["reason", "detail"]))
        .or_else(|| wait.and_then(|value| string_field(value, &["detail"])))
        .unwrap_or_else(|| "no LXMF delivery detail".into());
    let next_step = classification
        .and_then(|value| string_field(value, &["next_step", "next_action"]))
        .or_else(|| {
            report
                .get("failure_hints")
                .and_then(serde_json::Value::as_array)
                .and_then(|hints| hints.first())
                .and_then(serde_json::Value::as_str)
                .map(ToOwned::to_owned)
        })
        .unwrap_or_else(|| {
            "select an LXMF peer conversation in the app, then run LXMF Interop again".into()
        });

    Some(DiagnosticsLxmfDeliveryCard {
        outcome,
        send_state,
        proof_state,
        inbound_state,
        event_counts,
        readiness_stage,
        detail,
        next_step,
    })
}

fn diagnostics_preview_propagation_sync_card(
    lines: &[String],
) -> Option<DiagnosticsPropagationSyncCard> {
    let value: serde_json::Value = serde_json::from_str(&lines.join("\n")).ok()?;
    if value.get("report").and_then(serde_json::Value::as_str)
        != Some("native_lxmf_propagation_diagnostics")
    {
        return None;
    }
    let sync = value.get("sync");
    let sync_ok = sync
        .and_then(|sync| sync.get("ok"))
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    let selected_node = string_field(&value, &["selected_node"]).unwrap_or_else(|| "none".into());
    let before = propagation_state_line(value.get("before"));
    let after = propagation_state_line(value.get("after"));
    let sync_events = value
        .get("sync_events")
        .and_then(serde_json::Value::as_array)
        .cloned()
        .unwrap_or_default();
    let events = if sync_events.is_empty() {
        "events unavailable".into()
    } else {
        let status_count = sync_events
            .iter()
            .filter(|event| {
                event.get("kind").and_then(serde_json::Value::as_str) == Some("propagation_status")
            })
            .count();
        let debug_count = sync_events
            .iter()
            .filter(|event| event.get("kind").and_then(serde_json::Value::as_str) == Some("debug"))
            .count();
        let message_count = sync_events
            .iter()
            .filter(|event| {
                event.get("kind").and_then(serde_json::Value::as_str) == Some("message_received")
            })
            .count();
        let structured_count = sync_events
            .iter()
            .filter(|event| {
                event.get("kind").and_then(serde_json::Value::as_str) == Some("propagation_sync")
            })
            .count();
        format!(
            "structured={structured_count}, status={status_count}, debug={debug_count}, messages={message_count}, total={}",
            sync_events.len()
        )
    };
    let event_lines = sync_events
        .iter()
        .rev()
        .take(8)
        .map(propagation_sync_event_line)
        .collect::<Vec<_>>();
    let blocker = string_field(&value, &["blocker"]).unwrap_or_else(|| "unknown".into());
    let next_step =
        string_field(&value, &["next_step"]).unwrap_or_else(|| "inspect runtime logs".into());
    let blocked = blocker != "no propagation blocker reported";

    Some(DiagnosticsPropagationSyncCard {
        outcome: if sync_ok && !blocked {
            "complete"
        } else {
            "blocked"
        }
        .into(),
        selected_node,
        before,
        after,
        events,
        event_lines,
        blocker,
        next_step,
    })
}

fn propagation_sync_event_line(event: &serde_json::Value) -> String {
    match event.get("kind").and_then(serde_json::Value::as_str) {
        Some("propagation_status") => {
            let transfer =
                string_field(event, &["transfer_state"]).unwrap_or_else(|| "unknown".into());
            let link = string_field(event, &["link_state"]).unwrap_or_else(|| "unknown".into());
            let path = event
                .get("has_path")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false);
            let app_data = event
                .get("known_app_data")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false);
            format!("status: {link}/{transfer} path={path} app_data={app_data}")
        }
        Some("debug") => string_field(event, &["message"])
            .map(|message| format!("debug: {message}"))
            .unwrap_or_else(|| "debug: <missing message>".into()),
        Some("message_received") => {
            let peer = string_field(event, &["peer_label", "peer_hash"])
                .unwrap_or_else(|| "unknown peer".into());
            let message_id = string_field(event, &["message_id"]).unwrap_or_else(|| "no id".into());
            format!("message: {peer} id={message_id}")
        }
        Some("propagation_sync") => {
            let stage = string_field(event, &["stage"]).unwrap_or_else(|| "unknown".into());
            let status = string_field(event, &["status"]).unwrap_or_else(|| "unknown".into());
            let detail = string_field(event, &["detail"]).unwrap_or_default();
            format!("sync: {stage}/{status} {detail}")
        }
        Some(kind) => {
            let message = string_field(event, &["message"]).unwrap_or_default();
            format!("{kind}: {message}")
        }
        None => "unknown event".into(),
    }
}

fn propagation_state_line(value: Option<&serde_json::Value>) -> String {
    let Some(value) = value else {
        return "unavailable".into();
    };
    if let Some(error) = string_field(value, &["error"]) {
        return format!("error: {error}");
    }
    let link = string_field(value, &["link_state"]).unwrap_or_else(|| "unknown".into());
    let transfer = string_field(value, &["transfer_state"]).unwrap_or_else(|| "unknown".into());
    let has_path = value
        .get("has_path")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    let app_data = value
        .get("known_app_data")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    format!("link={link}, transfer={transfer}, path={has_path}, app_data={app_data}")
}

fn lxmf_interop_report_value(value: &serde_json::Value) -> Option<&serde_json::Value> {
    if value.get("report").and_then(serde_json::Value::as_str) == Some("native_lxmf_live_interop") {
        return Some(value);
    }
    value.get("lxmf_live_interop").filter(|nested| {
        nested.get("report").and_then(serde_json::Value::as_str) == Some("native_lxmf_live_interop")
    })
}

fn lxmf_send_state_line(send: &serde_json::Value) -> String {
    let requested = send
        .get("requested")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    if !requested {
        return "not requested".into();
    }
    let ok = send
        .get("ok")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    let message_id = string_field(send, &["message_id", "packet_hash"])
        .unwrap_or_else(|| "no message id".into());
    let state = string_field(
        send,
        &["native_lxmf_state", "stage_hint", "skipped", "error"],
    )
    .unwrap_or_else(|| {
        if ok {
            "submitted".into()
        } else {
            "failed".into()
        }
    });
    format!(
        "{} | {} | {}",
        if ok { "submitted" } else { "not sent" },
        state,
        message_id
    )
}

fn lxmf_event_counts_line(wait: &serde_json::Value) -> String {
    let inbound = wait
        .get("inbound_messages")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    let delivery = wait
        .get("delivery_updates")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    let proofs = wait
        .get("packet_proofs")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    format!("inbound={inbound}, delivery_updates={delivery}, packet_proofs={proofs}")
}

fn lxmf_first_failed_readiness_stage(report: &serde_json::Value) -> Option<String> {
    report
        .get("readiness_probe")
        .or_else(|| {
            report
                .get("readiness_retry")
                .and_then(|retry| retry.get("followup_probe"))
        })
        .and_then(|probe| probe.get("steps"))
        .and_then(serde_json::Value::as_array)
        .and_then(|steps| {
            steps.iter().find_map(|step| {
                let ok = step
                    .get("ok")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false);
                (!ok)
                    .then(|| string_field(step, &["stage"]))
                    .flatten()
                    .map(|stage| {
                        let detail =
                            string_field(step, &["detail"]).unwrap_or_else(|| "blocked".into());
                        format!("{stage}: {detail}")
                    })
            })
        })
}

fn diagnostics_preview_stage_cards(lines: &[String]) -> Vec<DiagnosticsStageCard> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&lines.join("\n")) else {
        return Vec::new();
    };
    if let Some(stages) = value.get("stages").and_then(serde_json::Value::as_array) {
        return stages
            .iter()
            .filter_map(|stage| {
                Some(DiagnosticsStageCard {
                    kind: "preflight".into(),
                    stage: string_field(stage, &["stage"])?,
                    status: string_field(stage, &["outcome"]).unwrap_or_else(|| "unknown".into()),
                    detail: string_field(stage, &["detail"]).unwrap_or_else(|| "no detail".into()),
                    next_step: string_field(stage, &["next_step"])
                        .unwrap_or_else(|| "inspect report".into()),
                })
            })
            .collect();
    }
    if let Some(verdicts) = value.get("verdicts").and_then(serde_json::Value::as_object) {
        let mut cards = verdicts
            .iter()
            .map(|(stage, verdict)| DiagnosticsStageCard {
                kind: "smoke".into(),
                stage: stage.clone(),
                status: string_field(verdict, &["status"]).unwrap_or_else(|| "unknown".into()),
                detail: string_field(verdict, &["detail"]).unwrap_or_else(|| "no detail".into()),
                next_step: string_field(verdict, &["next_action", "next_step"])
                    .unwrap_or_else(|| "continue".into()),
            })
            .collect::<Vec<_>>();
        cards.sort_by(|left, right| left.stage.cmp(&right.stage));
        return cards;
    }
    if let Some(report) = value
        .get("readiness_probe")
        .or_else(|| value.get("lxmf_delivery_probe"))
        .and_then(|probe| probe.get("report"))
    {
        return lxmf_step_cards(report);
    }
    if value.get("report").and_then(serde_json::Value::as_str) == Some("native_lxmf_live_interop") {
        if let Some(report) = value
            .get("readiness_retry")
            .or_else(|| value.get("readiness_probe"))
            .and_then(|probe| probe.get("report"))
        {
            return lxmf_step_cards(report);
        }
    }
    Vec::new()
}

fn lxmf_step_cards(report: &serde_json::Value) -> Vec<DiagnosticsStageCard> {
    report
        .get("steps")
        .and_then(serde_json::Value::as_array)
        .map(|steps| {
            steps
                .iter()
                .filter_map(|step| {
                    Some(DiagnosticsStageCard {
                        kind: "lxmf".into(),
                        stage: string_field(step, &["stage"])?,
                        status: if step
                            .get("ok")
                            .and_then(serde_json::Value::as_bool)
                            .unwrap_or(false)
                        {
                            "pass".into()
                        } else {
                            "fail".into()
                        },
                        detail: string_field(step, &["detail"])
                            .unwrap_or_else(|| "no detail".into()),
                        next_step: "inspect LXMF readiness and retry when fixed".into(),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

fn string_field(value: &serde_json::Value, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| {
        value.get(*key).and_then(|field| {
            field
                .as_str()
                .map(ToOwned::to_owned)
                .or_else(|| field.as_bool().map(|value| value.to_string()))
                .or_else(|| field.as_u64().map(|value| value.to_string()))
        })
    })
}

fn action_status_line(ok: bool, label: &str, fix: &str) -> String {
    if ok {
        format!("ready: {label}")
    } else {
        format!("blocked: {label}; {fix}")
    }
}

fn is_32_hex_hash(value: &str) -> bool {
    value.len() == 32 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn desktop_interface_detail_lines(
    profile: &crate::interfaces::ReticulumInterfaceProfile,
) -> Vec<String> {
    match profile.kind {
        InterfaceKind::TcpClient => vec![
            format!(
                "TCP gateway: {}:{}",
                profile.target_host, profile.target_port
            ),
            format!(
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
            ),
        ],
        InterfaceKind::TcpServer => vec![
            format!(
                "TCP server listen: {}:{}",
                profile.target_host, profile.target_port
            ),
            format!(
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
            ),
        ],
        InterfaceKind::I2p => vec![
            format!("I2P connectable: {}", profile.connectable),
            format!(
                "I2P peers: {}",
                if profile.peers.is_empty() {
                    "none".into()
                } else {
                    profile.peers.join(", ")
                }
            ),
        ],
        InterfaceKind::RNode => vec![
            format!(
                "RNode device: {}",
                if profile.device_port.is_empty() {
                    "none"
                } else {
                    profile.device_port.as_str()
                }
            ),
            format!(
                "radio: frequency={} bandwidth={} tx_power={} spreading={} coding={}",
                profile.frequency,
                profile.bandwidth,
                profile.tx_power,
                profile.spreading_factor,
                profile.coding_rate
            ),
        ],
        InterfaceKind::Auto | InterfaceKind::Unknown(_) => {
            vec!["Generic interface: no kind-specific settings are available.".into()]
        }
    }
}

fn interface_config_preview_lines(preview: &str) -> Vec<String> {
    if preview.is_empty() {
        return vec![String::new()];
    }
    preview
        .lines()
        .map(|line| {
            if line.is_empty() {
                " ".to_string()
            } else {
                line.to_string()
            }
        })
        .collect()
}

const DESKTOP_THEME_CHOICES: &[&str] = &[
    "default",
    "omen",
    "dark",
    "moonfly",
    "kanagawa",
    "nord",
    "solarized_dark",
    "light",
];

fn theme_from_name(name: &str) -> Theme {
    match name.trim().to_ascii_lowercase().as_str() {
        "default" => Theme::Dark,
        "omen" => omen_desktop_theme(),
        "light" => Theme::Light,
        "nord" => Theme::Nord,
        "solarized_dark" | "solarized-dark" | "solarized dark" => Theme::SolarizedDark,
        "gruvbox_dark" | "gruvbox-dark" | "gruvbox dark" => Theme::GruvboxDark,
        "dracula" => Theme::Dracula,
        "catppuccin" | "mocha" | "catppuccin_mocha" => Theme::CatppuccinMocha,
        "tokyo" | "tokyo_night" => Theme::TokyoNight,
        "kanagawa" | "kanagawa_dragon" => Theme::KanagawaDragon,
        "moonfly" => Theme::Moonfly,
        "nightfly" => Theme::Nightfly,
        "oxocarbon" => Theme::Oxocarbon,
        _ => Theme::Dark,
    }
}

fn omen_desktop_theme() -> Theme {
    Theme::custom(
        "OMEN".into(),
        Palette {
            background: Color::from_rgb8(4, 2, 3),
            text: Color::from_rgb8(232, 222, 220),
            primary: Color::from_rgb8(156, 28, 36),
            success: Color::from_rgb8(166, 84, 70),
            danger: Color::from_rgb8(218, 54, 60),
        },
    )
}

fn omen_application_style(theme: &Theme) -> iced::daemon::Appearance {
    let palette = theme.palette();
    iced::daemon::Appearance {
        background_color: mix_color(palette.background, omen_surface(), 0.55),
        text_color: palette.text,
    }
}

fn shell_container_style(theme: &Theme) -> container::Style {
    container::Style::default()
        .background(mix_color(theme.palette().background, omen_surface(), 0.25))
        .color(theme.palette().text)
}

fn card_container_style(theme: &Theme) -> container::Style {
    container::Style::default()
        .background(mix_color(theme.palette().background, omen_panel(), 0.42))
        .color(theme.palette().text)
        .border(Border {
            color: mix_color(theme.palette().primary, omen_accent(), 0.55),
            width: 1.0,
            radius: 0.0.into(),
        })
        .shadow(Shadow::default())
}

fn status_container_style(theme: &Theme) -> container::Style {
    container::Style::default()
        .background(mix_color(theme.palette().background, omen_status(), 0.55))
        .color(theme.palette().text)
        .border(Border {
            color: mix_color(theme.palette().primary, omen_accent(), 0.45),
            width: 1.0,
            radius: 0.0.into(),
        })
}

fn address_display_container_style(theme: &Theme) -> container::Style {
    container::Style::default()
        .background(mix_color(theme.palette().background, Color::BLACK, 0.75))
        .color(mix_color(theme.palette().text, Color::WHITE, 0.2))
        .border(Border {
            color: mix_color(theme.palette().primary, omen_accent(), 0.3),
            width: 1.0,
            radius: 0.0.into(),
        })
}

fn warning_container_style(theme: &Theme) -> container::Style {
    container::Style::default()
        .background(mix_color(
            theme.palette().background,
            omen_warning_bg(),
            0.7,
        ))
        .color(theme.palette().text)
        .border(Border {
            color: omen_warning(),
            width: 1.5,
            radius: 0.0.into(),
        })
}

fn incoming_message_container_style(theme: &Theme) -> container::Style {
    container::Style::default()
        .background(mix_color(theme.palette().background, omen_panel(), 0.58))
        .color(theme.palette().text)
        .border(Border {
            color: mix_color(theme.palette().primary, omen_accent(), 0.24),
            width: 1.0,
            radius: 0.0.into(),
        })
}

fn outgoing_message_container_style(theme: &Theme) -> container::Style {
    container::Style::default()
        .background(mix_color(theme.palette().primary, omen_accent_deep(), 0.28))
        .color(theme.palette().text)
        .border(Border {
            color: mix_color(theme.palette().primary, omen_accent(), 0.48),
            width: 1.0,
            radius: 0.0.into(),
        })
}

fn failed_message_container_style(theme: &Theme) -> container::Style {
    container::Style::default()
        .background(mix_color(
            theme.palette().background,
            omen_warning_bg(),
            0.58,
        ))
        .color(theme.palette().text)
        .border(Border {
            color: omen_warning(),
            width: 1.0,
            radius: 0.0.into(),
        })
}

fn selected_message_container_style(theme: &Theme) -> container::Style {
    container::Style::default()
        .background(mix_color(theme.palette().primary, omen_accent_deep(), 0.36))
        .color(theme.palette().text)
        .border(Border {
            color: omen_accent(),
            width: 2.0,
            radius: 0.0.into(),
        })
}

fn message_detail_container_style(theme: &Theme) -> container::Style {
    container::Style::default()
        .background(mix_color(theme.palette().background, omen_surface(), 0.7))
        .color(theme.palette().text)
        .border(Border {
            color: mix_color(theme.palette().primary, omen_accent(), 0.16),
            width: 1.0,
            radius: 0.0.into(),
        })
}

fn browser_viewport_container_style(
    theme: &Theme,
    page_background: Option<Color>,
    page_border: Option<Color>,
) -> container::Style {
    container::Style::default()
        .background(page_background.unwrap_or(Color::from_rgb8(4, 8, 10)))
        .border(Border {
            color: page_border
                .unwrap_or_else(|| mix_color(theme.palette().primary, omen_accent(), 0.65)),
            width: 1.0,
            radius: 0.0.into(),
        })
}

fn workspace_pane_container_style(theme: &Theme) -> container::Style {
    container::Style::default()
        .background(mix_color(theme.palette().background, omen_surface(), 0.72))
        .color(theme.palette().text)
        .border(Border {
            color: mix_color(theme.palette().primary, omen_accent(), 0.4),
            width: 1.0,
            radius: 0.0.into(),
        })
}

fn pane_title_container_style(theme: &Theme) -> container::Style {
    container::Style::default()
        .background(mix_color(theme.palette().background, omen_panel(), 0.78))
        .color(theme.palette().text)
        .border(Border {
            color: mix_color(theme.palette().primary, omen_accent(), 0.55),
            width: 1.0,
            radius: 0.0.into(),
        })
}

fn omen_button_style(theme: &Theme, status: button::Status) -> button::Style {
    let base = match status {
        button::Status::Hovered => mix_color(theme.palette().primary, omen_accent(), 0.45),
        button::Status::Pressed => omen_accent_deep(),
        button::Status::Disabled => Color::from_rgb8(48, 55, 59),
        button::Status::Active => mix_color(theme.palette().primary, omen_accent_deep(), 0.25),
    };
    button::Style {
        background: Some(Background::Color(base)),
        text_color: Color::WHITE,
        border: Border {
            color: mix_color(theme.palette().primary, omen_accent(), 0.75),
            width: 1.0,
            radius: 6.0.into(),
        },
        shadow: Shadow::default(),
    }
}

fn subtle_button_style(theme: &Theme, status: button::Status) -> button::Style {
    let base = match status {
        button::Status::Hovered => mix_color(theme.palette().background, omen_panel(), 0.72),
        button::Status::Pressed => mix_color(theme.palette().background, omen_status(), 0.72),
        button::Status::Disabled => Color::from_rgb8(38, 43, 47),
        button::Status::Active => mix_color(theme.palette().background, omen_panel(), 0.46),
    };
    button::Style {
        background: Some(Background::Color(base)),
        text_color: theme.palette().text,
        border: Border {
            color: mix_color(theme.palette().primary, omen_accent(), 0.32),
            width: 1.0,
            radius: 6.0.into(),
        },
        shadow: Shadow::default(),
    }
}

fn warning_button_style(theme: &Theme, status: button::Status) -> button::Style {
    let base = match status {
        button::Status::Hovered => omen_warning(),
        button::Status::Pressed => Color::from_rgb8(123, 48, 28),
        button::Status::Disabled => Color::from_rgb8(58, 42, 37),
        button::Status::Active => mix_color(theme.palette().danger, omen_warning(), 0.55),
    };
    button::Style {
        background: Some(Background::Color(base)),
        text_color: Color::WHITE,
        border: Border {
            color: omen_warning(),
            width: 1.0,
            radius: 6.0.into(),
        },
        shadow: Shadow::default(),
    }
}

fn inline_icon_button_style(theme: &Theme, status: button::Status) -> button::Style {
    let text_color = match status {
        button::Status::Hovered | button::Status::Pressed => theme.palette().primary,
        button::Status::Disabled => {
            mix_color(theme.palette().text, theme.palette().background, 0.55)
        }
        button::Status::Active => theme.palette().text,
    };
    button::Style {
        background: None,
        text_color,
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: 0.0.into(),
        },
        shadow: Shadow::default(),
    }
}

fn themed_scrollable_style(
    theme: &Theme,
    status: ScrollableStatus,
) -> iced::widget::scrollable::Style {
    let palette = theme.palette();
    let rail_background = mix_color(palette.background, omen_surface(), 0.84);
    let base_thumb = mix_color(palette.primary, omen_accent(), 0.48);
    let thumb_color = match status {
        ScrollableStatus::Active => base_thumb,
        ScrollableStatus::Hovered { .. } => mix_color(base_thumb, Color::WHITE, 0.2),
        ScrollableStatus::Dragged { .. } => mix_color(palette.primary, Color::WHITE, 0.34),
    };
    let rail = iced::widget::scrollable::Rail {
        background: Some(Background::Color(rail_background)),
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: 4.0.into(),
        },
        scroller: iced::widget::scrollable::Scroller {
            color: thumb_color,
            border: Border {
                color: mix_color(palette.primary, omen_accent(), 0.28),
                width: 0.5,
                radius: 4.0.into(),
            },
        },
    };

    iced::widget::scrollable::Style {
        container: container::Style::default(),
        vertical_rail: rail,
        horizontal_rail: rail,
        gap: Some(Background::Color(rail_background)),
    }
}

fn omen_surface() -> Color {
    Color::from_rgb8(9, 5, 7)
}

fn omen_panel() -> Color {
    Color::from_rgb8(20, 9, 12)
}

fn omen_status() -> Color {
    Color::from_rgb8(26, 10, 13)
}

fn omen_accent() -> Color {
    Color::from_rgb8(194, 54, 62)
}

fn omen_accent_deep() -> Color {
    Color::from_rgb8(82, 18, 24)
}

fn omen_warning() -> Color {
    Color::from_rgb8(205, 80, 68)
}

fn omen_warning_bg() -> Color {
    Color::from_rgb8(62, 24, 24)
}

fn mix_color(a: Color, b: Color, amount_b: f32) -> Color {
    let amount_b = amount_b.clamp(0.0, 1.0);
    let amount_a = 1.0 - amount_b;
    Color {
        r: a.r * amount_a + b.r * amount_b,
        g: a.g * amount_a + b.g * amount_b,
        b: a.b * amount_a + b.b * amount_b,
        a: a.a * amount_a + b.a * amount_b,
    }
}

fn map_keyboard_modifier_event(
    event: iced::Event,
    _status: event::Status,
    _window: window::Id,
) -> Option<Message> {
    match event {
        iced::Event::Keyboard(keyboard::Event::ModifiersChanged(modifiers)) => {
            Some(Message::KeyboardModifiersChanged(modifiers))
        }
        iced::Event::Keyboard(keyboard::Event::KeyPressed { modifiers, .. })
        | iced::Event::Keyboard(keyboard::Event::KeyReleased { modifiers, .. }) => {
            Some(Message::KeyboardModifiersChanged(modifiers))
        }
        _ => None,
    }
}

fn map_browser_field_keyboard_event(
    event: iced::Event,
    _status: event::Status,
    _window: window::Id,
) -> Option<Message> {
    match event {
        iced::Event::Keyboard(keyboard::Event::ModifiersChanged(modifiers)) => {
            Some(Message::KeyboardModifiersChanged(modifiers))
        }
        iced::Event::Keyboard(keyboard::Event::KeyPressed {
            key,
            modifiers,
            text,
            ..
        }) => map_browser_field_key_event_press(key, modifiers, text.as_deref()),
        iced::Event::Keyboard(keyboard::Event::KeyReleased { modifiers, .. }) => {
            Some(Message::KeyboardModifiersChanged(modifiers))
        }
        _ => None,
    }
}

fn map_browser_field_key_event_press(
    key: keyboard::Key,
    modifiers: keyboard::Modifiers,
    text: Option<&str>,
) -> Option<Message> {
    if modifiers.command() || modifiers.alt() {
        return map_key_press(key, modifiers);
    }
    if let Some(text) =
        text.filter(|text| !text.is_empty() && text.chars().all(|ch| !ch.is_control()))
    {
        return Some(Message::BrowserFieldKey(BrowserFieldKey::Insert(
            text.to_string(),
        )));
    }
    map_browser_field_key_press(key, modifiers)
}

fn map_key_press(key: keyboard::Key, modifiers: keyboard::Modifiers) -> Option<Message> {
    use keyboard::key::Named;
    use keyboard::Key;

    match key.as_ref() {
        Key::Named(Named::PageDown) => Some(Message::ScrollBrowserPage { direction: 1 }),
        Key::Named(Named::PageUp) => Some(Message::ScrollBrowserPage { direction: -1 }),
        Key::Named(Named::F9) => Some(Message::ToggleNavigation),
        Key::Named(Named::Tab) => Some(Message::FocusBrowserItem {
            reverse: modifiers.shift(),
        }),
        Key::Named(Named::Enter) | Key::Named(Named::Space) => {
            Some(Message::ActivateFocusedBrowserItem)
        }
        Key::Named(Named::ArrowLeft) if modifiers.alt() => Some(Message::BrowserBack),
        Key::Named(Named::ArrowRight) if modifiers.alt() => Some(Message::BrowserForward),
        Key::Character("b") if modifiers.command() => Some(Message::ToggleNavigation),
        Key::Character("t") if modifiers.command() => Some(Message::NewBrowserTab),
        Key::Character("w") if modifiers.command() => Some(Message::CloseBrowserTab),
        Key::Character("r") if modifiers.command() => Some(Message::ReloadBrowser),
        Key::Character("l") if modifiers.command() => Some(Message::OpenAddress),
        Key::Character("d") if modifiers.command() => Some(Message::WarmPath),
        Key::Character("x") if modifiers.command() => Some(Message::LiveProbe),
        Key::Character("p") if modifiers.command() => Some(Message::PathDiagnostics),
        Key::Character("i") if modifiers.command() => Some(Message::CreateIdentity),
        Key::Character("g") if modifiers.command() => Some(Message::NativeQuickstart),
        _ => None,
    }
}

fn map_browser_field_key_press(
    key: keyboard::Key,
    modifiers: keyboard::Modifiers,
) -> Option<Message> {
    use keyboard::key::Named;
    use keyboard::Key;

    if modifiers.command() || modifiers.alt() {
        return map_key_press(key, modifiers);
    }

    match key.as_ref() {
        Key::Character(text) => {
            let text = shifted_browser_field_text(text, modifiers);
            if text.chars().all(|ch| !ch.is_control()) {
                Some(Message::BrowserFieldKey(BrowserFieldKey::Insert(text)))
            } else {
                None
            }
        }
        Key::Named(Named::Space) => Some(Message::BrowserFieldKey(BrowserFieldKey::Insert(
            " ".into(),
        ))),
        Key::Named(Named::Backspace) => Some(Message::BrowserFieldKey(BrowserFieldKey::Backspace)),
        Key::Named(Named::Delete) => Some(Message::BrowserFieldKey(BrowserFieldKey::Delete)),
        Key::Named(Named::ArrowLeft) => Some(Message::BrowserFieldKey(BrowserFieldKey::MoveLeft)),
        Key::Named(Named::ArrowRight) => Some(Message::BrowserFieldKey(BrowserFieldKey::MoveRight)),
        Key::Named(Named::Home) => Some(Message::BrowserFieldKey(BrowserFieldKey::MoveHome)),
        Key::Named(Named::End) => Some(Message::BrowserFieldKey(BrowserFieldKey::MoveEnd)),
        Key::Named(Named::Enter) => Some(Message::SubmitBrowserFieldDraft),
        Key::Named(Named::Escape) => Some(Message::CancelBrowserFieldDraft),
        _ => map_key_press(key, modifiers),
    }
}

fn shifted_browser_field_text(text: &str, modifiers: keyboard::Modifiers) -> String {
    if !modifiers.shift() {
        return text.to_string();
    }
    let mut chars = text.chars();
    let Some(ch) = chars.next() else {
        return text.to_string();
    };
    if chars.next().is_some() {
        return text.to_string();
    }
    match ch {
        'a'..='z' => ch.to_ascii_uppercase().to_string(),
        '1' => "!".into(),
        '2' => "@".into(),
        '3' => "#".into(),
        '4' => "$".into(),
        '5' => "%".into(),
        '6' => "^".into(),
        '7' => "&".into(),
        '8' => "*".into(),
        '9' => "(".into(),
        '0' => ")".into(),
        '-' => "_".into(),
        '=' => "+".into(),
        '[' => "{".into(),
        ']' => "}".into(),
        '\\' => "|".into(),
        ';' => ":".into(),
        '\'' => "\"".into(),
        ',' => "<".into(),
        '.' => ">".into(),
        '/' => "?".into(),
        '`' => "~".into(),
        _ => text.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE_RETICULUM_HASH: &str = "00112233445566778899aabbccddeeff";
    const FIXTURE_OMENCHAT_HASH: &str = "ffeeddccbbaa99887766554433221100";
    const FIXTURE_CHAT_SERVER_HASH: &str = FIXTURE_RETICULUM_HASH;
    const FIXTURE_LXMF_PEER_HASH: &str = FIXTURE_RETICULUM_HASH;
    const FIXTURE_BROWSER_NODE_HASH: &str = FIXTURE_RETICULUM_HASH;
    const FIXTURE_PROPAGATION_NODE_HASH: &str = FIXTURE_RETICULUM_HASH;

    fn fixture_browser_node_url() -> String {
        format!("{FIXTURE_BROWSER_NODE_HASH}:/page/index.mu")
    }
    #[cfg(feature = "chat-client")]
    use crate::chat::store::ChatStore;
    use iced::keyboard::key::Named;

    #[test]
    fn footer_status_compaction_stays_single_line() {
        let compact = compact_footer_status(
            "link request timed out after 45s; request cancelled,\nretry when path/link is ready | run Diagnostics X or L for link/request/response report",
            72,
        );

        assert!(!compact.contains('\n'));
        assert!(compact.chars().count() <= 72);
        assert!(compact.ends_with("..."));
    }

    #[test]
    fn identity_footer_label_drops_verbose_prefix() {
        assert_eq!(
            compact_identity_status_label("identity: OMENbrowser_dev"),
            "OMENbrowser_dev"
        );
        assert_eq!(compact_identity_status_label("OMENTest"), "OMENTest");
    }

    #[test]
    fn shared_scroll_gutter_clears_scrollbar_rail() {
        let scrollbar_footprint = DESKTOP_SCROLLBAR_WIDTH + DESKTOP_SCROLLBAR_MARGIN;
        assert_eq!(
            desktop_scroll_gutter_right(),
            f32::from(scrollbar_footprint + DESKTOP_SCROLL_GUTTER_EXTRA)
        );
        assert!(desktop_scroll_gutter_right() >= f32::from(scrollbar_footprint + 10));
        assert!(DESKTOP_SCROLLBAR_SCROLLER_WIDTH <= DESKTOP_SCROLLBAR_WIDTH);
    }

    #[test]
    fn desktop_shell_spacing_keeps_scrollbars_clear_of_panel_borders() {
        let scrollbar_footprint = DESKTOP_SCROLLBAR_WIDTH + DESKTOP_SCROLLBAR_MARGIN;

        assert!(DESKTOP_SCROLL_GUTTER_EXTRA >= DESKTOP_PANEL_PADDING);
        assert!(desktop_scroll_gutter_right() > f32::from(scrollbar_footprint));
        assert!(DESKTOP_SHELL_PADDING >= DESKTOP_PANEL_PADDING);
    }

    #[test]
    fn keyboard_shortcuts_map_browser_focus_and_activation() {
        assert!(matches!(
            map_key_press(
                keyboard::Key::Named(Named::Tab),
                keyboard::Modifiers::empty()
            ),
            Some(Message::FocusBrowserItem { reverse: false })
        ));
        assert!(matches!(
            map_key_press(keyboard::Key::Named(Named::Tab), keyboard::Modifiers::SHIFT),
            Some(Message::FocusBrowserItem { reverse: true })
        ));
        assert!(matches!(
            map_key_press(
                keyboard::Key::Named(Named::Enter),
                keyboard::Modifiers::empty()
            ),
            Some(Message::ActivateFocusedBrowserItem)
        ));
        assert!(matches!(
            map_key_press(
                keyboard::Key::Named(Named::Space),
                keyboard::Modifiers::empty()
            ),
            Some(Message::ActivateFocusedBrowserItem)
        ));
        assert!(matches!(
            map_key_press(
                keyboard::Key::Named(Named::F9),
                keyboard::Modifiers::empty(),
            ),
            Some(Message::ToggleNavigation)
        ));
    }

    #[cfg(feature = "chat-client")]
    #[test]
    fn omenchat_timeline_renders_actions_as_separate_action_lines() {
        let session = ChatSessionView {
            session_id: 1,
            server: crate::chat::ChatServerSummary {
                server_id: "server-a".into(),
                destination: "abcd".into(),
                display_name: "Server A".into(),
            },
            rooms: vec![],
            active_room: crate::chat::ChatRoomSummary {
                server_id: "server-a".into(),
                room_id: 1,
                name: "lobby".into(),
                topic: None,
                unread: 0,
                joined: true,
            },
            users: vec![ChatUserSummary {
                server_id: "server-a".into(),
                user_id: 7,
                display_name: "Alice".into(),
                role_bits: 0,
                status_bits: 0,
                lxmf_available: true,
            }],
            events: vec![
                ChatEvent {
                    server_id: "server-a".into(),
                    room_id: 1,
                    event_id: 1,
                    actor_user_id: Some(7),
                    actor_display_name: None,
                    at_unix: 0,
                    kind: ChatEventKind::Message {
                        body: "hello".into(),
                    },
                },
                ChatEvent {
                    server_id: "server-a".into(),
                    room_id: 1,
                    event_id: 2,
                    actor_user_id: Some(7),
                    actor_display_name: None,
                    at_unix: 1,
                    kind: ChatEventKind::Action {
                        body: "waves".into(),
                    },
                },
            ],
            status: String::new(),
        };

        let groups = chat_timeline_groups(&session);
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].bodies[0].text, "hello");
        assert!(!groups[0].bodies[0].is_action);
        assert_eq!(groups[1].bodies[0].text, "* Alice waves");
        assert!(groups[1].bodies[0].is_action);
    }

    #[cfg(feature = "chat-client")]
    #[test]
    fn omenchat_command_result_keeps_draft_on_server_error() {
        assert_eq!(
            omenchat_command_result_from_events(&[ChatClientEvent::Error {
                session_id: Some(1),
                message: "permission denied: topic changes require moderator or admin role".into(),
            }]),
            OmenChatDraftCommandResult::HandledKeep
        );
        assert_eq!(
            omenchat_command_result_from_events(&[ChatClientEvent::RoomsUpdated {
                session_id: 1,
                rooms: Vec::new(),
            }]),
            OmenChatDraftCommandResult::HandledClear
        );
    }

    #[cfg(feature = "chat-client")]
    #[test]
    fn omenchat_media_hints_offer_reticulum_load_without_clearweb_fetch() {
        let settings = crate::storage::settings::ClearwebPrivacySettings::default();
        let body = format!("pic {FIXTURE_RETICULUM_HASH}:/files/cat.png");
        let hints = omenchat_media_hints(&body, &settings, None, false, &HashMap::new());

        assert_eq!(hints.len(), 1);
        assert!(hints[0].label.contains("Reticulum/NomadNet"));
        assert!(hints[0].load_url.is_some());
        assert!(hints[0].open_url.is_none());
    }

    #[cfg(feature = "chat-client")]
    #[test]
    fn omenchat_media_hints_offer_socks_load_when_remote_media_allowed() {
        let mut settings = crate::storage::settings::ClearwebPrivacySettings::default();
        settings.remote_media_enabled = true;
        let hints = omenchat_media_hints(
            "pic https://example.org/cat.png",
            &settings,
            Some(&("127.0.0.1".to_string(), 9150)),
            true,
            &HashMap::new(),
        );

        assert_eq!(hints.len(), 1);
        assert!(hints[0].label.contains("SOCKS5"));
        assert!(hints[0].open_url.is_none());
        assert!(hints[0].load_url.is_some());
    }

    #[cfg(feature = "chat-client")]
    #[test]
    fn omenchat_media_hints_require_trust_for_clearweb_auto_load() {
        let mut settings = crate::storage::settings::ClearwebPrivacySettings::default();
        settings.remote_media_enabled = true;
        let hints = omenchat_media_hints(
            "pic https://example.org/cat.png",
            &settings,
            Some(&("127.0.0.1".to_string(), 9150)),
            false,
            &HashMap::new(),
        );

        assert_eq!(hints.len(), 1);
        assert!(hints[0].label.contains("untrusted OMENchat server"));
        assert!(hints[0].open_url.is_none());
        assert!(hints[0].load_url.is_some());
    }

    #[cfg(feature = "chat-client")]
    #[test]
    fn omenchat_media_hints_keep_clearweb_images_explicit_without_remote_media() {
        let settings = crate::storage::settings::ClearwebPrivacySettings::default();
        let hints = omenchat_media_hints(
            "pic https://example.org/cat.png",
            &settings,
            Some(&("127.0.0.1".to_string(), 9150)),
            true,
            &HashMap::new(),
        );

        assert_eq!(hints.len(), 1);
        assert!(hints[0].label.contains("disabled"));
        assert!(hints[0].open_url.is_some());
        assert!(hints[0].load_url.is_none());
    }

    #[cfg(feature = "chat-client")]
    #[test]
    fn omenchat_media_hints_report_cached_media() {
        let settings = crate::storage::settings::ClearwebPrivacySettings::default();
        let url = format!("{FIXTURE_RETICULUM_HASH}:/files/cat.png");
        let mut cache = HashMap::new();
        cache.insert(
            url.clone(),
            OmenChatMediaLoadState::Cached {
                path: "/tmp/cat.png".into(),
                content_type: "image/png".into(),
                animated: false,
            },
        );

        let hints = omenchat_media_hints(&url, &settings, None, false, &cache);
        assert_eq!(hints.len(), 1);
        assert!(hints[0].label.is_empty());
        assert!(hints[0].open_url.is_none());
        assert!(hints[0].open_path.is_none());
        assert!(hints[0].load_url.is_none());
        assert_eq!(hints[0].image_path.as_deref(), Some("/tmp/cat.png"));
        assert_eq!(
            hints[0].caption.as_deref(),
            Some("Reticulum/NomadNet image")
        );
    }

    #[cfg(feature = "chat-client")]
    #[test]
    fn omenchat_media_hints_offer_cached_animated_gif_open_button() {
        let settings = crate::storage::settings::ClearwebPrivacySettings::default();
        let url = "https://example.test/loop.gif".to_string();
        let mut cache = HashMap::new();
        cache.insert(
            url.clone(),
            OmenChatMediaLoadState::Cached {
                path: "/tmp/loop.gif".into(),
                content_type: "image/gif".into(),
                animated: true,
            },
        );

        let hints = omenchat_media_hints(&url, &settings, None, false, &cache);
        assert_eq!(hints.len(), 1);
        assert!(hints[0].label.is_empty());
        assert!(hints[0].open_url.is_none());
        assert_eq!(hints[0].open_path.as_deref(), Some("/tmp/loop.gif"));
        assert!(hints[0].load_url.is_none());
        assert_eq!(hints[0].image_path.as_deref(), Some("/tmp/loop.gif"));
    }

    #[cfg(feature = "chat-client")]
    #[test]
    fn omenchat_media_hints_caption_cached_clearweb_source() {
        let settings = crate::storage::settings::ClearwebPrivacySettings::default();
        let url = "https://cdn.example.test/images/cat.png".to_string();
        let mut cache = HashMap::new();
        cache.insert(
            url.clone(),
            OmenChatMediaLoadState::Cached {
                path: "/tmp/cat.png".into(),
                content_type: "image/png".into(),
                animated: false,
            },
        );

        let trusted = omenchat_media_hints(&url, &settings, None, true, &cache);
        let untrusted = omenchat_media_hints(&url, &settings, None, false, &cache);

        assert_eq!(
            trusted[0].caption.as_deref(),
            Some("trusted clearweb image from cdn.example.test")
        );
        assert_eq!(
            untrusted[0].caption.as_deref(),
            Some("manual clearweb image from cdn.example.test")
        );
    }

    #[cfg(feature = "chat-client")]
    #[test]
    fn gif_image_descriptor_count_detects_multiple_frames() {
        let single_frame = [
            b"GIF89a".as_slice(),
            &[1, 0, 1, 0, 0, 0, 0],
            &[0x2C, 0, 0, 0, 0, 1, 0, 1, 0, 0],
            &[2, 1, 0, 0],
            &[0x3B],
        ]
        .concat();
        let animated = [
            b"GIF89a".as_slice(),
            &[1, 0, 1, 0, 0, 0, 0],
            &[0x2C, 0, 0, 0, 0, 1, 0, 1, 0, 0],
            &[2, 1, 0, 0],
            &[0x2C, 0, 0, 0, 0, 1, 0, 1, 0, 0],
            &[2, 1, 0, 0],
            &[0x3B],
        ]
        .concat();

        assert_eq!(gif_image_descriptor_count(&single_frame, 2), 1);
        assert_eq!(gif_image_descriptor_count(&animated, 2), 2);
    }

    #[cfg(feature = "chat-client")]
    #[test]
    fn omenchat_media_dimensions_parse_common_headers() {
        let png = [
            b"\x89PNG\r\n\x1a\n".as_slice(),
            &[0, 0, 0, 13],
            b"IHDR".as_slice(),
            &640u32.to_be_bytes(),
            &480u32.to_be_bytes(),
            &[8, 6, 0, 0, 0],
        ]
        .concat();
        let gif = [b"GIF89a".as_slice(), &[44, 1, 200, 0, 0, 0, 0]].concat();
        let mut jpeg = vec![0xFF, 0xD8, 0xFF, 0xE0, 0, 4, 0, 0, 0xFF, 0xC0, 0, 17, 8];
        jpeg.extend_from_slice(&720u16.to_be_bytes());
        jpeg.extend_from_slice(&1280u16.to_be_bytes());
        jpeg.extend_from_slice(&[3, 1, 0x11, 0, 2, 0x11, 0, 3, 0x11, 0]);

        assert_eq!(image_dimensions_from_bytes(&png), Some((640, 480)));
        assert_eq!(image_dimensions_from_bytes(&gif), Some((300, 200)));
        assert_eq!(image_dimensions_from_bytes(&jpeg), Some((1280, 720)));
    }

    #[cfg(feature = "chat-client")]
    #[test]
    fn omenchat_media_dimensions_scale_large_images_without_upscaling_small_ones() {
        assert_eq!(
            scale_media_dimensions(1040, 720, 520.0, 360.0),
            (520.0, 360.0)
        );
        assert_eq!(
            scale_media_dimensions(200, 100, 520.0, 360.0),
            (200.0, 100.0)
        );
    }

    #[cfg(feature = "chat-client")]
    #[test]
    fn omenchat_upload_state_label_keeps_attachment_status_compact() {
        assert_eq!(
            omenchat_upload_state_label(&OmenChatMediaLoadState::Loading {
                message: "requested upload from server".into(),
                received: None,
                total: None,
            }),
            "waiting for server"
        );
        assert_eq!(
            omenchat_upload_state_label(&OmenChatMediaLoadState::Loading {
                message: "receiving file".into(),
                received: Some(1536),
                total: Some(4096),
            }),
            "loading: 1.5 KiB / 4.0 KiB"
        );
        let label = omenchat_upload_state_label(&OmenChatMediaLoadState::Failed {
            message: "resource transfer failed because the server closed before the resource completed and the retry window expired".into(),
        });
        assert!(label.starts_with("failed: resource transfer failed"));
        assert!(label.ends_with("..."));
        assert!(label.len() < 90);
    }

    #[cfg(feature = "chat-client")]
    #[test]
    fn omenchat_inline_media_size_reads_bounded_header_only() {
        let root =
            std::env::temp_dir().join(format!("omenchat-media-header-{}", current_epoch_ms()));
        std::fs::create_dir_all(&root).expect("temp dir");
        let path = root.join("wide.png");
        let mut png = [
            b"\x89PNG\r\n\x1a\n".as_slice(),
            &[0, 0, 0, 13],
            b"IHDR".as_slice(),
            &1200u32.to_be_bytes(),
            &600u32.to_be_bytes(),
            &[8, 6, 0, 0, 0],
        ]
        .concat();
        png.extend(std::iter::repeat_n(
            0xAA,
            OMENCHAT_INLINE_MEDIA_HEADER_BYTES + 32,
        ));
        std::fs::write(&path, png).expect("write png");

        let bytes = read_media_header_bytes(&path, 32).expect("read header");
        assert_eq!(bytes.len(), 32);
        assert_eq!(image_dimensions_from_bytes(&bytes), Some((1200, 600)));
        assert_eq!(omenchat_inline_media_size(&path), Some((520.0, 260.0)));
        let _ = std::fs::remove_dir_all(root);
    }

    #[cfg(feature = "chat-client")]
    #[test]
    fn cached_media_is_animated_gif_uses_bounded_scan() {
        let root = std::env::temp_dir().join(format!("omenchat-gif-scan-{}", current_epoch_ms()));
        std::fs::create_dir_all(&root).expect("temp dir");
        let path = root.join("animated.gif");
        let mut gif = [
            b"GIF89a".as_slice(),
            &[1, 0, 1, 0, 0, 0, 0],
            &[0x2C, 0, 0, 0, 0, 1, 0, 1, 0, 0],
            &[2, 1, 0, 0],
            &[0x2C, 0, 0, 0, 0, 1, 0, 1, 0, 0],
            &[2, 1, 0, 0],
            &[0x3B],
        ]
        .concat();
        gif.extend(std::iter::repeat_n(
            0xCC,
            OMENCHAT_GIF_ANIMATION_SCAN_BYTES + 1024,
        ));
        std::fs::write(&path, gif).expect("write gif");

        let bounded = read_media_header_bytes(&path, 32).expect("read bounded scan");
        assert_eq!(bounded.len(), 32);
        assert!(cached_media_is_animated_gif(
            &path,
            "application/octet-stream"
        ));
        let _ = std::fs::remove_dir_all(root);
    }

    #[cfg(feature = "chat-client")]
    #[test]
    fn omenchat_timeline_group_preserves_first_event_timestamp() {
        let session = ChatSessionView {
            session_id: 1,
            server: crate::chat::ChatServerSummary {
                server_id: "server-a".into(),
                destination: "abcd".into(),
                display_name: "Server A".into(),
            },
            rooms: vec![],
            active_room: crate::chat::ChatRoomSummary {
                server_id: "server-a".into(),
                room_id: 1,
                name: "lobby".into(),
                topic: None,
                unread: 0,
                joined: true,
            },
            users: vec![ChatUserSummary {
                server_id: "server-a".into(),
                user_id: 7,
                display_name: "Alice".into(),
                role_bits: 0,
                status_bits: 0,
                lxmf_available: true,
            }],
            events: vec![
                ChatEvent {
                    server_id: "server-a".into(),
                    room_id: 1,
                    event_id: 1,
                    actor_user_id: Some(7),
                    actor_display_name: None,
                    at_unix: 1_700_000_000,
                    kind: ChatEventKind::Message {
                        body: "first".into(),
                    },
                },
                ChatEvent {
                    server_id: "server-a".into(),
                    room_id: 1,
                    event_id: 2,
                    actor_user_id: Some(7),
                    actor_display_name: None,
                    at_unix: 1_700_000_060,
                    kind: ChatEventKind::Message {
                        body: "second".into(),
                    },
                },
            ],
            status: String::new(),
        };

        let groups = chat_timeline_groups(&session);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].at_unix, 1_700_000_000);
        assert_eq!(
            chat_event_time_label(groups[0].at_unix),
            "2023-11-14 22:13:20 UTC"
        );
        assert_eq!(groups[0].bodies.len(), 2);
    }

    #[cfg(feature = "chat-client")]
    #[test]
    fn omenchat_timeline_splits_same_actor_after_group_gap() {
        let session = ChatSessionView {
            session_id: 1,
            server: crate::chat::ChatServerSummary {
                server_id: "server-a".into(),
                destination: "abcd".into(),
                display_name: "Server A".into(),
            },
            rooms: vec![],
            active_room: crate::chat::ChatRoomSummary {
                server_id: "server-a".into(),
                room_id: 1,
                name: "lobby".into(),
                topic: None,
                unread: 0,
                joined: true,
            },
            users: vec![ChatUserSummary {
                server_id: "server-a".into(),
                user_id: 7,
                display_name: "Alice".into(),
                role_bits: 0,
                status_bits: 0,
                lxmf_available: true,
            }],
            events: vec![
                ChatEvent {
                    server_id: "server-a".into(),
                    room_id: 1,
                    event_id: 1,
                    actor_user_id: Some(7),
                    actor_display_name: None,
                    at_unix: 1_700_000_000,
                    kind: ChatEventKind::Message {
                        body: "first".into(),
                    },
                },
                ChatEvent {
                    server_id: "server-a".into(),
                    room_id: 1,
                    event_id: 2,
                    actor_user_id: Some(7),
                    actor_display_name: None,
                    at_unix: 1_700_000_240,
                    kind: ChatEventKind::Message {
                        body: "same stack".into(),
                    },
                },
                ChatEvent {
                    server_id: "server-a".into(),
                    room_id: 1,
                    event_id: 3,
                    actor_user_id: Some(7),
                    actor_display_name: None,
                    at_unix: 1_700_000_601,
                    kind: ChatEventKind::Message {
                        body: "new stack".into(),
                    },
                },
            ],
            status: String::new(),
        };

        let groups = chat_timeline_groups(&session);
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].bodies.len(), 2);
        assert_eq!(groups[1].bodies.len(), 1);
    }

    #[cfg(feature = "chat-client")]
    #[test]
    fn omenchat_timeline_only_renders_active_room_events() {
        let session = ChatSessionView {
            session_id: 1,
            server: crate::chat::ChatServerSummary {
                server_id: "server-a".into(),
                destination: "abcd".into(),
                display_name: "Server A".into(),
            },
            rooms: vec![],
            active_room: crate::chat::ChatRoomSummary {
                server_id: "server-a".into(),
                room_id: 2,
                name: "help".into(),
                topic: None,
                unread: 0,
                joined: true,
            },
            users: vec![ChatUserSummary {
                server_id: "server-a".into(),
                user_id: 7,
                display_name: "Alice".into(),
                role_bits: 0,
                status_bits: 0,
                lxmf_available: true,
            }],
            events: vec![
                ChatEvent {
                    server_id: "server-a".into(),
                    room_id: 1,
                    event_id: 1,
                    actor_user_id: Some(7),
                    actor_display_name: None,
                    at_unix: 1,
                    kind: ChatEventKind::Message {
                        body: "lobby only".into(),
                    },
                },
                ChatEvent {
                    server_id: "server-a".into(),
                    room_id: 2,
                    event_id: 2,
                    actor_user_id: Some(7),
                    actor_display_name: None,
                    at_unix: 2,
                    kind: ChatEventKind::Message {
                        body: "help visible".into(),
                    },
                },
            ],
            status: String::new(),
        };

        let groups = chat_timeline_groups(&session);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].bodies[0].text, "help visible");
    }

    #[cfg(feature = "chat-client")]
    #[test]
    fn omenchat_event_counts_are_scoped_by_room() {
        let session = ChatSessionView {
            session_id: 9,
            server: crate::chat::ChatServerSummary {
                server_id: "server-a".into(),
                destination: "abcd".into(),
                display_name: "Server A".into(),
            },
            rooms: vec![],
            active_room: crate::chat::ChatRoomSummary {
                server_id: "server-a".into(),
                room_id: 3,
                name: "empty-active".into(),
                topic: None,
                unread: 0,
                joined: true,
            },
            users: vec![],
            events: vec![
                ChatEvent {
                    server_id: "server-a".into(),
                    room_id: 1,
                    event_id: 1,
                    actor_user_id: None,
                    actor_display_name: Some("Alice".into()),
                    at_unix: 1,
                    kind: ChatEventKind::Message { body: "one".into() },
                },
                ChatEvent {
                    server_id: "server-a".into(),
                    room_id: 1,
                    event_id: 2,
                    actor_user_id: None,
                    actor_display_name: Some("Alice".into()),
                    at_unix: 2,
                    kind: ChatEventKind::Message { body: "two".into() },
                },
                ChatEvent {
                    server_id: "server-a".into(),
                    room_id: 2,
                    event_id: 3,
                    actor_user_id: None,
                    actor_display_name: Some("Bob".into()),
                    at_unix: 3,
                    kind: ChatEventKind::Message {
                        body: "other".into(),
                    },
                },
            ],
            status: String::new(),
        };

        let counts = omenchat_event_counts_by_room(&[session]);
        assert_eq!(counts.get(&(9, 1)), Some(&2));
        assert_eq!(counts.get(&(9, 2)), Some(&1));
        assert_eq!(counts.get(&(9, 3)), Some(&0));
    }

    #[cfg(feature = "chat-client-rns")]
    #[test]
    fn omenchat_load_older_uses_cache_when_live_session_is_disconnected() {
        let root = std::env::temp_dir().join(format!(
            "omenbrowser-rs-desktop-omenchat-load-older-cache-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let paths = crate::config::AppPaths::from_root(root);
        paths.ensure().expect("paths");
        let app = App::new(crate::config::AppConfig {
            paths,
            settings: crate::storage::settings::AppSettings::default(),
        });
        let mut desktop = DesktopApp::new(app);
        let destination = FIXTURE_CHAT_SERVER_HASH;
        let descriptor = OmenChatDescriptor {
            server_destination: destination.into(),
            display_name: Some("Test OMENchat".into()),
            rooms_hint: vec!["lobby".into()],
            local_display_name: Some("tester".into()),
            ..OmenChatDescriptor::default()
        };
        let session_id = desktop.open_omenchat_status_session(descriptor, "disconnected".into());
        if let Some(session) = desktop.chat_client.session_mut(session_id) {
            session.active_room.joined = true;
            session.rooms[0].joined = true;
            session.events.push(ChatEvent {
                server_id: destination.into(),
                room_id: 1,
                event_id: 3,
                actor_user_id: None,
                actor_display_name: Some("Alice".into()),
                at_unix: 3,
                kind: ChatEventKind::Message {
                    body: "newest".into(),
                },
            });
        }
        {
            let store = desktop.chat_store.as_mut().expect("chat store");
            store
                .append_events(vec![
                    ChatEvent {
                        server_id: destination.into(),
                        room_id: 1,
                        event_id: 1,
                        actor_user_id: None,
                        actor_display_name: Some("Alice".into()),
                        at_unix: 1,
                        kind: ChatEventKind::Message {
                            body: "older one".into(),
                        },
                    },
                    ChatEvent {
                        server_id: destination.into(),
                        room_id: 1,
                        event_id: 2,
                        actor_user_id: None,
                        actor_display_name: Some("Alice".into()),
                        at_unix: 2,
                        kind: ChatEventKind::Message {
                            body: "older two".into(),
                        },
                    },
                ])
                .expect("cached events");
        }

        desktop.load_older_omenchat_history(session_id);

        let session = desktop.chat_client.session(session_id).expect("session");
        assert_eq!(
            session
                .events
                .iter()
                .map(|event| event.event_id)
                .collect::<Vec<_>>(),
            vec![1, 2, 3]
        );
        assert_eq!(session.status, "loaded 2 older cached event(s)");
    }

    #[cfg(feature = "chat-client")]
    #[test]
    fn omenchat_load_older_cache_hit_does_not_send_second_request() {
        let root = std::env::temp_dir().join(format!(
            "omenbrowser-rs-desktop-omenchat-load-older-cache-only-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let paths = crate::config::AppPaths::from_root(root);
        paths.ensure().expect("paths");
        let app = App::new(crate::config::AppConfig {
            paths,
            settings: crate::storage::settings::AppSettings::default(),
        });
        let mut desktop = DesktopApp::new(app);
        let destination = "mockchatdestination";
        let descriptor = OmenChatDescriptor {
            server_destination: destination.into(),
            display_name: Some("Mock OMENchat".into()),
            rooms_hint: vec!["lobby".into()],
            local_display_name: Some("tester".into()),
            ..OmenChatDescriptor::default()
        };
        let session_id = desktop.open_omenchat_status_session(descriptor, "connected".into());
        if let Some(session) = desktop.chat_client.session_mut(session_id) {
            session.active_room.joined = true;
            session.rooms[0].joined = true;
            session.events.push(ChatEvent {
                server_id: destination.into(),
                room_id: 1,
                event_id: 3,
                actor_user_id: None,
                actor_display_name: Some("Alice".into()),
                at_unix: 3,
                kind: ChatEventKind::Message {
                    body: "newest".into(),
                },
            });
        }
        {
            let store = desktop.chat_store.as_mut().expect("chat store");
            store
                .append_events(vec![
                    ChatEvent {
                        server_id: destination.into(),
                        room_id: 1,
                        event_id: 1,
                        actor_user_id: None,
                        actor_display_name: Some("Alice".into()),
                        at_unix: 1,
                        kind: ChatEventKind::Message {
                            body: "older one".into(),
                        },
                    },
                    ChatEvent {
                        server_id: destination.into(),
                        room_id: 1,
                        event_id: 2,
                        actor_user_id: None,
                        actor_display_name: Some("Alice".into()),
                        at_unix: 2,
                        kind: ChatEventKind::Message {
                            body: "older two".into(),
                        },
                    },
                ])
                .expect("cached events");
        }

        desktop.load_older_omenchat_history(session_id);

        let session = desktop.chat_client.session(session_id).expect("session");
        assert_eq!(
            session
                .events
                .iter()
                .map(|event| event.event_id)
                .collect::<Vec<_>>(),
            vec![1, 2, 3]
        );
        assert_eq!(session.status, "loaded 2 older cached event(s)");
    }

    #[cfg(feature = "chat-client")]
    #[test]
    fn cached_omenchat_room_restore_preserves_saved_scroll_offset() {
        let root = std::env::temp_dir().join(format!(
            "omenbrowser-rs-desktop-omenchat-preserve-scroll-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let paths = crate::config::AppPaths::from_root(root);
        paths.ensure().expect("paths");
        let app = App::new(crate::config::AppConfig {
            paths,
            settings: crate::storage::settings::AppSettings::default(),
        });
        let mut desktop = DesktopApp::new(app);
        let destination = "mockchatdestination";
        let descriptor = OmenChatDescriptor {
            server_destination: destination.into(),
            display_name: Some("Mock OMENchat".into()),
            rooms_hint: vec!["lobby".into()],
            local_display_name: Some("tester".into()),
            ..OmenChatDescriptor::default()
        };
        let session_id = desktop.open_omenchat_status_session(descriptor, "connected".into());
        let saved_offset = RelativeOffset { x: 0.0, y: 0.35 };
        desktop
            .chat_scroll_offsets
            .insert((session_id, 1), saved_offset);
        {
            let store = desktop.chat_store.as_mut().expect("chat store");
            store
                .append_events(vec![ChatEvent {
                    server_id: destination.into(),
                    room_id: 1,
                    event_id: 1,
                    actor_user_id: None,
                    actor_display_name: Some("Alice".into()),
                    at_unix: 1,
                    kind: ChatEventKind::Message {
                        body: "cached".into(),
                    },
                }])
                .expect("cached event");
        }

        assert_eq!(desktop.restore_cached_omenchat_room_history(session_id), 1);

        assert_eq!(
            desktop.chat_scroll_offsets.get(&(session_id, 1)).copied(),
            Some(saved_offset)
        );
    }

    #[cfg(feature = "chat-client")]
    #[test]
    fn omenchat_scroll_ids_are_room_specific() {
        assert_ne!(omenchat_scroll_id(7, 1), omenchat_scroll_id(7, 2));
        assert_ne!(omenchat_scroll_id(7, 1), omenchat_scroll_id(8, 1));
    }

    #[cfg(feature = "chat-client")]
    #[test]
    fn cached_omenchat_room_restore_defaults_new_room_to_bottom() {
        let root = std::env::temp_dir().join(format!(
            "omenbrowser-rs-desktop-omenchat-default-scroll-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let paths = crate::config::AppPaths::from_root(root);
        paths.ensure().expect("paths");
        let app = App::new(crate::config::AppConfig {
            paths,
            settings: crate::storage::settings::AppSettings::default(),
        });
        let mut desktop = DesktopApp::new(app);
        let destination = "mockchatdestination";
        let descriptor = OmenChatDescriptor {
            server_destination: destination.into(),
            display_name: Some("Mock OMENchat".into()),
            rooms_hint: vec!["lobby".into()],
            local_display_name: Some("tester".into()),
            ..OmenChatDescriptor::default()
        };
        let session_id = desktop.open_omenchat_status_session(descriptor, "connected".into());
        desktop.chat_scroll_offsets.remove(&(session_id, 1));
        {
            let store = desktop.chat_store.as_mut().expect("chat store");
            store
                .append_events(vec![ChatEvent {
                    server_id: destination.into(),
                    room_id: 1,
                    event_id: 1,
                    actor_user_id: None,
                    actor_display_name: Some("Alice".into()),
                    at_unix: 1,
                    kind: ChatEventKind::Message {
                        body: "cached".into(),
                    },
                }])
                .expect("cached event");
        }

        assert_eq!(desktop.restore_cached_omenchat_room_history(session_id), 1);

        assert_eq!(
            desktop.chat_scroll_offsets.get(&(session_id, 1)).copied(),
            Some(RelativeOffset { x: 0.0, y: 1.0 })
        );
    }

    #[cfg(feature = "chat-client")]
    #[test]
    fn cached_omenchat_room_restore_schedules_visible_scroll_retry() {
        let root = std::env::temp_dir().join(format!(
            "omenbrowser-rs-desktop-omenchat-scroll-retry-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let paths = crate::config::AppPaths::from_root(root);
        paths.ensure().expect("paths");
        let app = App::new(crate::config::AppConfig {
            paths,
            settings: crate::storage::settings::AppSettings::default(),
        });
        let mut desktop = DesktopApp::new(app);
        let destination = "mockchatdestination";
        let descriptor = OmenChatDescriptor {
            server_destination: destination.into(),
            display_name: Some("Mock OMENchat".into()),
            rooms_hint: vec!["lobby".into()],
            local_display_name: Some("tester".into()),
            ..OmenChatDescriptor::default()
        };
        let session_id = desktop.open_omenchat_status_session(descriptor, "connected".into());
        desktop.ensure_pane_for_omenchat(session_id);
        desktop.restore_workspace_scrolls_pending = false;
        desktop.restore_workspace_scrolls_remaining = 0;
        {
            let store = desktop.chat_store.as_mut().expect("chat store");
            store
                .append_events(vec![ChatEvent {
                    server_id: destination.into(),
                    room_id: 1,
                    event_id: 1,
                    actor_user_id: None,
                    actor_display_name: Some("Alice".into()),
                    at_unix: 1,
                    kind: ChatEventKind::Message {
                        body: "cached".into(),
                    },
                }])
                .expect("cached event");
        }

        assert_eq!(desktop.restore_cached_omenchat_room_history(session_id), 1);

        assert!(desktop.restore_workspace_scrolls_pending);
        assert!(desktop.restore_workspace_scrolls_remaining >= 5);
        assert_eq!(
            desktop.chat_scroll_offsets.get(&(session_id, 1)).copied(),
            Some(RelativeOffset { x: 0.0, y: 1.0 })
        );
        assert!(desktop.chat_scroll_bottom_locks.contains(&(session_id, 1)));
    }

    #[cfg(feature = "chat-client-rns")]
    #[test]
    fn omenchat_load_older_reports_reconnect_when_disconnected_cache_is_empty() {
        let root = std::env::temp_dir().join(format!(
            "omenbrowser-rs-desktop-omenchat-load-older-empty-cache-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let paths = crate::config::AppPaths::from_root(root);
        paths.ensure().expect("paths");
        let app = App::new(crate::config::AppConfig {
            paths,
            settings: crate::storage::settings::AppSettings::default(),
        });
        let mut desktop = DesktopApp::new(app);
        let descriptor = OmenChatDescriptor {
            server_destination: FIXTURE_CHAT_SERVER_HASH.into(),
            display_name: Some("Test OMENchat".into()),
            rooms_hint: vec!["lobby".into()],
            local_display_name: Some("tester".into()),
            ..OmenChatDescriptor::default()
        };
        let session_id = desktop.open_omenchat_status_session(descriptor, "disconnected".into());

        desktop.load_older_omenchat_history(session_id);

        let session = desktop.chat_client.session(session_id).expect("session");
        assert!(session.events.is_empty());
        assert_eq!(
            session.status,
            "no older cached history; reconnect to request server history"
        );
    }

    #[test]
    fn desktop_browser_titles_put_page_name_before_browser_label_and_strip_controls() {
        let root = std::env::temp_dir().join(format!(
            "omenbrowser-rs-desktop-pane-title-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let paths = crate::config::AppPaths::from_root(root);
        paths.ensure().expect("paths");
        let mut desktop = DesktopApp::new(App::new(crate::config::AppConfig {
            paths,
            settings: crate::storage::settings::AppSettings::default(),
        }));
        let tab_id = desktop.app.active_browser_tab().id;
        desktop.app.active_browser_tab_mut().title = "Node\u{7} Home".into();

        assert_eq!(
            desktop.workspace_pane_title(&DesktopPane::Browser(tab_id)),
            "Node Home - Browser"
        );
    }

    #[test]
    fn focused_clearweb_micron_link_prompts_external_browser() {
        let root = std::env::temp_dir().join(format!(
            "omenbrowser-rs-desktop-clearweb-focused-link-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let paths = crate::config::AppPaths::from_root(root);
        paths.ensure().expect("paths");
        let mut desktop = DesktopApp::new(App::new(crate::config::AppConfig {
            paths,
            settings: crate::storage::settings::AppSettings::default(),
        }));
        let tab_id = desktop.app.active_browser_tab().id;
        desktop.app.active_browser_tab_mut().focused_link = Some(crate::app::FocusedLink {
            target: "https://example.org/news".into(),
            fields: Vec::new(),
            region_index: 0,
        });

        assert!(desktop.prompt_focused_external_link_if_needed());

        let prompt = desktop.external_link_prompt.expect("external prompt");
        assert_eq!(prompt.url, "https://example.org/news");
        assert_eq!(prompt.source_tab, Some(tab_id));
    }

    #[test]
    fn external_browser_choices_do_not_launch_tor_browser() {
        let choices = detect_external_browsers(None);
        let commands = choices
            .iter()
            .map(|choice| choice.command.as_str())
            .collect::<Vec<_>>();

        assert!(
            !commands.iter().any(|command| {
                command.contains("torbrowser-launcher")
                    || command.contains("tor-browser")
                    || command.contains("start-tor-browser")
            }),
            "Tor Browser should use the Copy URL flow, not a launcher button: {commands:?}"
        );
    }

    #[test]
    fn external_browser_choices_keep_one_entry_per_browser_label() {
        let candidates = [
            ("Default browser", "xdg-open"),
            ("Chrome", "google-chrome"),
            ("Chrome", "google-chrome-stable"),
            ("Brave", "brave-browser"),
            ("Brave", "brave"),
        ];

        let choices = detect_external_browsers_from_candidates(None, &candidates, |_| true);

        assert_eq!(
            choices
                .iter()
                .map(|choice| (choice.label.as_str(), choice.command.as_str()))
                .collect::<Vec<_>>(),
            vec![
                ("Default browser", "xdg-open"),
                ("Chrome", "google-chrome"),
                ("Brave", "brave-browser"),
            ]
        );
    }

    #[test]
    fn external_browser_choices_preserve_preferred_duplicate_command() {
        let candidates = [
            ("Chrome", "google-chrome"),
            ("Chrome", "google-chrome-stable"),
            ("Brave", "brave-browser"),
        ];

        let choices = detect_external_browsers_from_candidates(
            Some("google-chrome-stable"),
            &candidates,
            |_| true,
        );

        assert_eq!(
            choices
                .iter()
                .map(|choice| (choice.label.as_str(), choice.command.as_str()))
                .collect::<Vec<_>>(),
            vec![
                ("Chrome", "google-chrome-stable"),
                ("Brave", "brave-browser"),
            ]
        );
    }

    #[test]
    fn standard_external_browser_candidate_uses_url_argument_only() {
        let choice = ExternalBrowserChoice {
            label: "Default browser".into(),
            command: "xdg-open".into(),
            kind: ExternalBrowserKind::Default,
        };

        assert_eq!(
            external_browser_open_candidates(&choice, "https://example.org"),
            vec![("xdg-open".into(), vec!["https://example.org".into()])]
        );
    }

    #[test]
    fn desktop_address_input_events_end_page_field_editor_and_edit_url_bar() {
        let root = std::env::temp_dir().join(format!(
            "omenbrowser-rs-desktop-field-edit-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let paths = crate::config::AppPaths::from_root(root);
        paths.ensure().expect("paths");
        let mut desktop = DesktopApp::new(App::new(crate::config::AppConfig {
            paths,
            settings: crate::storage::settings::AppSettings::default(),
        }));
        let tab_id = desktop.app.active_browser_tab().id;
        let page = crate::browser::BrowserPage {
            url: "mock.node:/profile.mu".into(),
            title: "Profile".into(),
            markup: "`<12|nickname`saved>".into(),
            source: crate::browser::PageSource::Network,
            metadata: std::collections::BTreeMap::new(),
            request_data: None,
        };
        desktop
            .app
            .active_browser_tab_mut()
            .session
            .apply_page(page, true);
        assert!(desktop.app.focus_browser_item_with_viewport(80, 20, false));
        assert!(desktop.app.activate_focused_browser_control());

        let _ = desktop.update(Message::BrowserPaneAddressChanged {
            tab_id,
            value: "mock.node:/other.mu".into(),
        });
        let _ = desktop.update(Message::BrowserFieldKey(BrowserFieldKey::Insert(
            "x".into(),
        )));

        assert!(desktop.app.active_browser_field_editor().is_none());
        assert_eq!(
            desktop.app.active_browser_tab().address_input,
            "mock.node:/other.mu"
        );
        assert_eq!(
            desktop
                .app
                .active_browser_tab()
                .session
                .field_values
                .get("nickname"),
            Some(&"saved".to_string())
        );
    }

    #[test]
    fn new_browser_tab_clears_stale_page_field_editor() {
        let root = std::env::temp_dir().join(format!(
            "omenbrowser-rs-desktop-new-browser-clears-field-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let paths = crate::config::AppPaths::from_root(root);
        paths.ensure().expect("paths");
        let mut desktop = DesktopApp::new(App::new(crate::config::AppConfig {
            paths,
            settings: crate::storage::settings::AppSettings::default(),
        }));
        let page = crate::browser::BrowserPage {
            url: "mock.node:/profile.mu".into(),
            title: "Profile".into(),
            markup: "`<12|nickname`saved>".into(),
            source: crate::browser::PageSource::Network,
            metadata: std::collections::BTreeMap::new(),
            request_data: None,
        };
        desktop
            .app
            .active_browser_tab_mut()
            .session
            .apply_page(page, true);
        assert!(desktop.app.focus_browser_item_with_viewport(80, 20, false));
        assert!(desktop.app.activate_focused_browser_control());

        let _ = desktop.update(Message::NewBrowserTab);

        assert!(desktop.app.active_browser_field_editor().is_none());
        assert_eq!(
            desktop.app.active_browser_tab().address_input,
            desktop.app.settings.default_start_page
        );
    }

    #[test]
    fn browser_field_keyboard_map_edits_only_active_field_text() {
        assert!(matches!(
            map_browser_field_key_press(
                keyboard::Key::Character("a".into()),
                keyboard::Modifiers::empty(),
            ),
            Some(Message::BrowserFieldKey(BrowserFieldKey::Insert(value))) if value == "a"
        ));
        assert!(matches!(
            map_browser_field_key_press(
                keyboard::Key::Named(Named::Backspace),
                keyboard::Modifiers::empty(),
            ),
            Some(Message::BrowserFieldKey(BrowserFieldKey::Backspace))
        ));
        assert!(matches!(
            map_browser_field_key_press(
                keyboard::Key::Named(Named::Enter),
                keyboard::Modifiers::empty(),
            ),
            Some(Message::SubmitBrowserFieldDraft)
        ));
        assert!(matches!(
            map_browser_field_key_press(
                keyboard::Key::Named(Named::Escape),
                keyboard::Modifiers::empty(),
            ),
            Some(Message::CancelBrowserFieldDraft)
        ));
        assert!(matches!(
            map_browser_field_key_press(
                keyboard::Key::Character("t".into()),
                keyboard::Modifiers::COMMAND,
            ),
            Some(Message::NewBrowserTab)
        ));
    }

    #[test]
    fn browser_field_event_listener_routes_captured_text_input_keys() {
        assert!(matches!(
            map_browser_field_keyboard_event(
                iced::Event::Keyboard(keyboard::Event::KeyPressed {
                    key: keyboard::Key::Character("a".into()),
                    modified_key: keyboard::Key::Character("a".into()),
                    physical_key: keyboard::key::Physical::Unidentified(
                        keyboard::key::NativeCode::Unidentified
                    ),
                    location: keyboard::Location::Standard,
                    modifiers: keyboard::Modifiers::empty(),
                    text: None,
                }),
                event::Status::Captured,
                window::Id::unique(),
            ),
            Some(Message::BrowserFieldKey(BrowserFieldKey::Insert(value))) if value == "a"
        ));
    }

    #[test]
    fn browser_field_event_listener_prefers_text_payload_for_insertions() {
        assert!(matches!(
            map_browser_field_keyboard_event(
                iced::Event::Keyboard(keyboard::Event::KeyPressed {
                    key: keyboard::Key::Unidentified,
                    modified_key: keyboard::Key::Unidentified,
                    physical_key: keyboard::key::Physical::Unidentified(
                        keyboard::key::NativeCode::Unidentified
                    ),
                    location: keyboard::Location::Standard,
                    modifiers: keyboard::Modifiers::empty(),
                    text: Some("x".into()),
                }),
                event::Status::Captured,
                window::Id::unique(),
            ),
            Some(Message::BrowserFieldKey(BrowserFieldKey::Insert(value))) if value == "x"
        ));
        assert!(matches!(
            map_browser_field_key_event_press(
                keyboard::Key::Named(Named::Backspace),
                keyboard::Modifiers::empty(),
                None,
            ),
            Some(Message::BrowserFieldKey(BrowserFieldKey::Backspace))
        ));
    }

    #[test]
    fn browser_field_keyboard_map_preserves_shifted_text() {
        assert!(matches!(
            map_browser_field_key_press(
                keyboard::Key::Character("a".into()),
                keyboard::Modifiers::SHIFT,
            ),
            Some(Message::BrowserFieldKey(BrowserFieldKey::Insert(value))) if value == "A"
        ));
        assert!(matches!(
            map_browser_field_key_press(
                keyboard::Key::Character("1".into()),
                keyboard::Modifiers::SHIFT,
            ),
            Some(Message::BrowserFieldKey(BrowserFieldKey::Insert(value))) if value == "!"
        ));
        assert!(matches!(
            map_browser_field_key_press(
                keyboard::Key::Character("/".into()),
                keyboard::Modifiers::SHIFT,
            ),
            Some(Message::BrowserFieldKey(BrowserFieldKey::Insert(value))) if value == "?"
        ));
        assert_eq!(
            shifted_browser_field_text("A", keyboard::Modifiers::SHIFT),
            "A"
        );
    }

    #[test]
    fn desktop_workspace_starts_with_browser_and_message_panes() {
        let root = std::env::temp_dir().join(format!(
            "omenbrowser-rs-desktop-panes-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let paths = crate::config::AppPaths::from_root(root);
        paths.ensure().expect("paths");
        let app = App::new(crate::config::AppConfig {
            paths,
            settings: crate::storage::settings::AppSettings::default(),
        });
        let desktop = DesktopApp::new(app);

        assert_eq!(desktop.workspace_panes.len(), 2);
        assert!(desktop
            .workspace_panes
            .iter()
            .any(|(_, pane)| matches!(pane, DesktopPane::Browser(_))));
        assert!(desktop
            .workspace_panes
            .iter()
            .any(|(_, pane)| matches!(pane, DesktopPane::Conversation(_))));
    }

    #[test]
    fn new_browser_tab_adds_workspace_pane() {
        let root = std::env::temp_dir().join(format!(
            "omenbrowser-rs-desktop-new-pane-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let paths = crate::config::AppPaths::from_root(root);
        paths.ensure().expect("paths");
        let app = App::new(crate::config::AppConfig {
            paths,
            settings: crate::storage::settings::AppSettings::default(),
        });
        let mut desktop = DesktopApp::new(app);
        let initial_panes = desktop.workspace_panes.len();

        let _ = desktop.update(Message::NewBrowserTab);

        assert_eq!(desktop.workspace_panes.len(), initial_panes + 1);
        let active_id = desktop.app.active_browser_tab().id;
        assert!(desktop
            .workspace_panes
            .iter()
            .any(|(_, pane)| *pane == DesktopPane::Browser(active_id)));
    }

    #[test]
    fn browser_pane_address_edit_targets_backing_tab() {
        let root = std::env::temp_dir().join(format!(
            "omenbrowser-rs-desktop-target-tab-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let paths = crate::config::AppPaths::from_root(root);
        paths.ensure().expect("paths");
        let app = App::new(crate::config::AppConfig {
            paths,
            settings: crate::storage::settings::AppSettings::default(),
        });
        let mut desktop = DesktopApp::new(app);
        let first_id = desktop.app.active_browser_tab().id;
        let _ = desktop.update(Message::NewBrowserTab);
        let second_id = desktop.app.active_browser_tab().id;

        let _ = desktop.update(Message::BrowserPaneAddressChanged {
            tab_id: first_id,
            value: "mock.page:/first.mu".into(),
        });

        let first = desktop
            .app
            .workspace
            .browser_tabs
            .iter()
            .find(|tab| tab.id == first_id)
            .expect("first tab");
        let second = desktop
            .app
            .workspace
            .browser_tabs
            .iter()
            .find(|tab| tab.id == second_id)
            .expect("second tab");
        assert_eq!(first.address_input, "mock.page:/first.mu");
        assert_ne!(second.address_input, "mock.page:/first.mu");
        assert_eq!(desktop.app.active_browser_tab().id, first_id);
    }

    #[test]
    fn conversation_pane_composer_targets_backing_conversation() {
        let root = std::env::temp_dir().join(format!(
            "omenbrowser-rs-desktop-target-conversation-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let paths = crate::config::AppPaths::from_root(root);
        paths.ensure().expect("paths");
        let mut app = App::new(crate::config::AppConfig {
            paths,
            settings: crate::storage::settings::AppSettings::default(),
        });
        let first_id = app.active_conversation().id;
        app.new_conversation();
        let second_id = app.active_conversation().id;
        let mut desktop = DesktopApp::new(app);

        let _ = desktop.update(Message::ConversationPaneBodyChanged {
            conversation_id: first_id,
            value: "first body".into(),
        });

        let first = desktop
            .app
            .workspace
            .conversations
            .iter()
            .find(|conversation| conversation.id == first_id)
            .expect("first conversation");
        let second = desktop
            .app
            .workspace
            .conversations
            .iter()
            .find(|conversation| conversation.id == second_id)
            .expect("second conversation");
        assert_eq!(first.draft_body, "first body");
        assert_ne!(second.draft_body, "first body");
        assert_eq!(desktop.app.active_conversation().id, first_id);
    }

    #[test]
    fn desktop_lxmf_micron_link_restores_conversation_pane() {
        let root = std::env::temp_dir().join(format!(
            "omenbrowser-rs-desktop-lxmf-link-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let paths = crate::config::AppPaths::from_root(root);
        paths.ensure().expect("paths");
        let app = App::new(crate::config::AppConfig {
            paths,
            settings: crate::storage::settings::AppSettings::default(),
        });
        let mut desktop = DesktopApp::new(app);

        assert!(desktop.activate_lxmf_link(crate::micron::LinkAction {
            target: format!("lxmf@{FIXTURE_LXMF_PEER_HASH}"),
            fields: Vec::new(),
        }));

        let conversation_id = desktop.app.active_conversation().id;
        assert_eq!(
            desktop.app.active_conversation().peer_hash,
            FIXTURE_LXMF_PEER_HASH
        );
        assert!(desktop
            .workspace_panes
            .iter()
            .any(|(_, pane)| *pane == DesktopPane::Conversation(conversation_id)));
    }

    #[test]
    fn new_conversation_button_adds_tiled_conversation_pane() {
        let root = std::env::temp_dir().join(format!(
            "omenbrowser-rs-desktop-new-conversation-pane-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let paths = crate::config::AppPaths::from_root(root);
        paths.ensure().expect("paths");
        let app = App::new(crate::config::AppConfig {
            paths,
            settings: crate::storage::settings::AppSettings::default(),
        });
        let mut desktop = DesktopApp::new(app);
        let initial_conversations = desktop.app.workspace.conversations.len();
        let initial_panes = desktop.workspace_panes.len();

        let _ = desktop.update(Message::NewConversationPane);

        assert_eq!(
            desktop.app.workspace.conversations.len(),
            initial_conversations + 1
        );
        assert_eq!(desktop.workspace_panes.len(), initial_panes + 1);
        let active_id = desktop.app.active_conversation().id;
        assert!(desktop
            .workspace_panes
            .iter()
            .any(|(_, pane)| *pane == DesktopPane::Conversation(active_id)));
    }

    #[test]
    fn adding_workspace_pane_anchors_existing_chat_scroll_to_bottom() {
        let root = std::env::temp_dir().join(format!(
            "omenbrowser-rs-desktop-add-pane-scroll-lock-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let paths = crate::config::AppPaths::from_root(root);
        paths.ensure().expect("paths");
        let app = App::new(crate::config::AppConfig {
            paths,
            settings: crate::storage::settings::AppSettings::default(),
        });
        let mut desktop = DesktopApp::new(app);
        let conversation_id = desktop.app.active_conversation().id;
        desktop.ensure_pane_for_active_conversation();
        desktop
            .conversation_scroll_offsets
            .insert(conversation_id, RelativeOffset { x: 0.0, y: 0.61 });
        desktop.restore_workspace_scrolls_pending = false;
        desktop.restore_workspace_scrolls_remaining = 0;
        desktop.restore_workspace_scroll_locks_release_pending = false;
        desktop.conversation_scroll_restore_locks.clear();

        let _ = desktop.update(Message::NewConversationPane);
        let _ = desktop.update(Message::ConversationScrolled {
            conversation_id,
            offset: RelativeOffset { x: 0.0, y: 0.0 },
        });

        assert_eq!(
            desktop.conversation_scroll_offsets.get(&conversation_id),
            Some(&RelativeOffset { x: 0.0, y: 1.0 })
        );
    }

    #[test]
    fn closing_workspace_pane_anchors_existing_chat_scroll_to_bottom() {
        let root = std::env::temp_dir().join(format!(
            "omenbrowser-rs-desktop-close-pane-scroll-lock-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let paths = crate::config::AppPaths::from_root(root);
        paths.ensure().expect("paths");
        let app = App::new(crate::config::AppConfig {
            paths,
            settings: crate::storage::settings::AppSettings::default(),
        });
        let mut desktop = DesktopApp::new(app);
        let conversation_id = desktop.app.active_conversation().id;
        desktop.ensure_pane_for_active_conversation();
        let browser_pane = desktop
            .workspace_panes
            .iter()
            .find_map(|(pane, kind)| matches!(kind, DesktopPane::Browser(_)).then_some(*pane))
            .expect("browser pane");
        desktop
            .conversation_scroll_offsets
            .insert(conversation_id, RelativeOffset { x: 0.0, y: 0.74 });
        desktop.restore_workspace_scrolls_pending = false;
        desktop.restore_workspace_scrolls_remaining = 0;
        desktop.restore_workspace_scroll_locks_release_pending = false;
        desktop.conversation_scroll_restore_locks.clear();

        desktop.close_workspace_pane(browser_pane);
        let _ = desktop.update(Message::ConversationScrolled {
            conversation_id,
            offset: RelativeOffset { x: 0.0, y: 0.0 },
        });

        assert_eq!(
            desktop.conversation_scroll_offsets.get(&conversation_id),
            Some(&RelativeOffset { x: 0.0, y: 1.0 })
        );
    }

    #[test]
    fn red_x_delete_conversation_removes_pane_instead_of_retargeting_to_next_thread() {
        let root = std::env::temp_dir().join(format!(
            "omenbrowser-rs-desktop-delete-conversation-pane-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let paths = crate::config::AppPaths::from_root(root);
        paths.ensure().expect("paths");
        let mut app = App::new(crate::config::AppConfig {
            paths,
            settings: crate::storage::settings::AppSettings::default(),
        });
        app.workspace.conversations[0].peer_hash = "peer-one".into();
        app.workspace.conversations[0].peer_label = "Peer One".into();
        let first_id = app.workspace.conversations[0].id;
        app.new_conversation();
        app.workspace.conversations[1].peer_hash = "peer-two".into();
        app.workspace.conversations[1].peer_label = "Peer Two".into();
        let second_id = app.workspace.conversations[1].id;
        let mut desktop = DesktopApp::new(app);

        assert!(desktop
            .workspace_panes
            .iter()
            .any(|(_, pane)| *pane == DesktopPane::Conversation(second_id)));

        let _ = desktop.update(Message::CloseConversationPaneTab(second_id));

        assert!(desktop
            .app
            .workspace
            .conversations
            .iter()
            .any(|conversation| conversation.id == first_id));
        assert!(desktop
            .app
            .workspace
            .conversations
            .iter()
            .all(|conversation| conversation.id != second_id));
        assert!(desktop
            .workspace_panes
            .iter()
            .all(|(_, pane)| !matches!(pane, DesktopPane::Conversation(_))));
        assert!(desktop
            .workspace_panes
            .iter()
            .any(|(_, pane)| matches!(pane, DesktopPane::Browser(_))));
    }

    #[test]
    fn red_x_delete_last_blank_conversation_does_not_leave_restore_tab() {
        let root = std::env::temp_dir().join(format!(
            "omenbrowser-rs-desktop-delete-blank-conversation-pane-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let paths = crate::config::AppPaths::from_root(root);
        paths.ensure().expect("paths");
        let settings_file = paths.settings_file.clone();
        let app = App::new(crate::config::AppConfig {
            paths,
            settings: crate::storage::settings::AppSettings::default(),
        });
        let mut desktop = DesktopApp::new(app);
        let conversation_id = desktop.app.active_conversation().id;

        let _ = desktop.update(Message::CloseConversationPaneTab(conversation_id));

        assert!(desktop.hidden_conversation_panes().is_empty());
        assert!(desktop
            .workspace_panes
            .iter()
            .all(|(_, pane)| !matches!(pane, DesktopPane::Conversation(_))));
        let saved = crate::storage::settings::AppSettings::load_or_default(&settings_file)
            .expect("saved settings");
        assert!(saved.conversation_tabs.is_empty());
    }

    #[cfg(feature = "chat-client-rns")]
    #[test]
    fn omenchat_stale_reconnect_result_does_not_finish_current_attempt() {
        let root = std::env::temp_dir().join(format!(
            "omenbrowser-rs-desktop-omenchat-reconnect-generation-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let paths = crate::config::AppPaths::from_root(root);
        paths.ensure().expect("paths");
        let app = App::new(crate::config::AppConfig {
            paths,
            settings: crate::storage::settings::AppSettings::default(),
        });
        let mut desktop = DesktopApp::new(app);
        let descriptor = OmenChatDescriptor {
            server_destination: FIXTURE_CHAT_SERVER_HASH.into(),
            display_name: Some("Test OMENchat".into()),
            rooms_hint: vec!["lobby".into()],
            local_display_name: Some("tester".into()),
            ..OmenChatDescriptor::default()
        };
        let session_id = desktop.open_omenchat_status_session(descriptor.clone(), "waiting".into());
        let stale_generation = desktop.next_omenchat_reconnect_generation(session_id);
        let current_generation = desktop.next_omenchat_reconnect_generation(session_id);
        desktop.omenchat_live_opening.insert(session_id);

        let _ = desktop.handle_omenchat_live_reconnect_result(
            session_id,
            stale_generation,
            descriptor.clone(),
            Err("old attempt failed".into()),
        );

        assert!(desktop.omenchat_live_opening.contains(&session_id));
        assert_eq!(
            desktop
                .omenchat_live_retry_count
                .get(&session_id)
                .copied()
                .unwrap_or(0),
            0
        );

        let _ = desktop.handle_omenchat_live_reconnect_result(
            session_id,
            current_generation,
            descriptor,
            Err("current attempt failed".into()),
        );

        assert!(!desktop.omenchat_live_opening.contains(&session_id));
        assert_eq!(
            desktop
                .omenchat_live_retry_count
                .get(&session_id)
                .copied()
                .unwrap_or(0),
            1
        );
    }

    #[cfg(feature = "chat-client-rns")]
    #[test]
    fn omenchat_path_result_uses_guarded_delayed_reconnect_status() {
        let root = std::env::temp_dir().join(format!(
            "omenbrowser-rs-desktop-omenchat-path-reconnect-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let paths = crate::config::AppPaths::from_root(root);
        paths.ensure().expect("paths");
        let app = App::new(crate::config::AppConfig {
            paths,
            settings: crate::storage::settings::AppSettings::default(),
        });
        let mut desktop = DesktopApp::new(app);
        let descriptor = OmenChatDescriptor {
            server_destination: FIXTURE_CHAT_SERVER_HASH.into(),
            display_name: Some("Test OMENchat".into()),
            rooms_hint: vec!["lobby".into()],
            local_display_name: Some("tester".into()),
            ..OmenChatDescriptor::default()
        };
        let session_id = desktop.open_omenchat_status_session(descriptor, "waiting".into());
        let destination = FIXTURE_CHAT_SERVER_HASH.to_string();
        desktop.omenchat_live_transports.insert(
            session_id,
            DesktopOmenChatTransport::new([0x31; 16], current_epoch_ms()),
        );

        let _ = desktop.update(Message::OmenChatPathRequestResult {
            session_id,
            destination: destination.clone(),
            result: Ok(true),
        });

        assert!(desktop
            .chat_client
            .session(session_id)
            .expect("session")
            .status
            .contains("live link remains active"));

        desktop.omenchat_live_transports.remove(&session_id);
        desktop.omenchat_live_opening.insert(session_id);
        let _ = desktop.update(Message::OmenChatPathRequestResult {
            session_id,
            destination: destination.clone(),
            result: Ok(true),
        });

        assert!(desktop
            .chat_client
            .session(session_id)
            .expect("session")
            .status
            .contains("reconnect already pending"));

        desktop.omenchat_live_opening.remove(&session_id);
        let _ = desktop.update(Message::OmenChatPathRequestResult {
            session_id,
            destination,
            result: Ok(true),
        });

        assert!(desktop
            .chat_client
            .session(session_id)
            .expect("session")
            .status
            .contains("reconnecting after announce wait"));
    }

    #[cfg(feature = "chat-client-rns")]
    #[test]
    fn omenchat_delayed_reconnect_clears_stale_retry_state_when_link_is_active() {
        let root = std::env::temp_dir().join(format!(
            "omenbrowser-rs-desktop-omenchat-active-reconnect-clear-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let paths = crate::config::AppPaths::from_root(root);
        paths.ensure().expect("paths");
        let app = App::new(crate::config::AppConfig {
            paths,
            settings: crate::storage::settings::AppSettings::default(),
        });
        let mut desktop = DesktopApp::new(app);
        let descriptor = OmenChatDescriptor {
            server_destination: FIXTURE_CHAT_SERVER_HASH.into(),
            display_name: Some("Test OMENchat".into()),
            rooms_hint: vec!["lobby".into()],
            local_display_name: Some("tester".into()),
            ..OmenChatDescriptor::default()
        };
        let session_id = desktop.open_omenchat_status_session(descriptor, "connected".into());
        desktop.omenchat_live_transports.insert(
            session_id,
            DesktopOmenChatTransport::new([0x52; 16], current_epoch_ms()),
        );
        desktop.omenchat_live_opening.insert(session_id);
        desktop.omenchat_live_retry_after.insert(session_id, 123);
        desktop.omenchat_live_retry_count.insert(session_id, 3);
        desktop
            .omenchat_live_reconnect_generation
            .insert(session_id, 9);

        let _ = desktop.update(Message::ReconnectOmenChatSessionIfDisconnected(session_id));

        assert!(desktop.omenchat_live_transports.contains_key(&session_id));
        assert!(!desktop.omenchat_live_opening.contains(&session_id));
        assert!(!desktop.omenchat_live_retry_after.contains_key(&session_id));
        assert!(!desktop.omenchat_live_retry_count.contains_key(&session_id));
        assert!(!desktop
            .omenchat_live_reconnect_generation
            .contains_key(&session_id));
        assert!(desktop
            .chat_client
            .session(session_id)
            .expect("session")
            .status
            .contains("already active"));
    }

    #[cfg(feature = "chat-client-rns")]
    #[tokio::test]
    async fn omenchat_live_monitor_totals_aggregate_sessions_and_transfers() {
        let root = std::env::temp_dir().join(format!(
            "omenbrowser-rs-desktop-omenchat-monitor-totals-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let paths = crate::config::AppPaths::from_root(root);
        paths.ensure().expect("paths");
        let app = App::new(crate::config::AppConfig {
            paths,
            settings: crate::storage::settings::AppSettings::default(),
        });
        let mut desktop = DesktopApp::new(app);
        let session_id = desktop.open_omenchat_status_session(
            OmenChatDescriptor {
                server_destination: FIXTURE_CHAT_SERVER_HASH.into(),
                display_name: Some("Test OMENchat".into()),
                rooms_hint: vec!["lobby".into()],
                local_display_name: Some("tester".into()),
                ..OmenChatDescriptor::default()
            },
            "connected".into(),
        );
        let mut transport = DesktopOmenChatTransport::new([0x73; 16], 1_000);
        transport.frames_in = 4;
        transport.frames_out = 3;
        transport.bytes_in = 1024;
        transport.bytes_out = 512;
        transport.resources_in = 2;
        transport.resource_bytes_in = 2048;
        transport.upload_fetches_out = 1;
        transport.upload_resource_offers_in = 2;
        transport.upload_inline_chunks_in = 3;
        transport.upload_inline_bytes_in = 4096;
        transport.upload_resources_in = 1;
        transport.upload_resource_bytes_in = 8192;
        transport.awaiting_pong = true;
        transport
            .pending_resource_offers
            .entry("upload:test".into())
            .or_default()
            .push_back(vec![1, 2, 3]);
        desktop
            .omenchat_live_transports
            .insert(session_id, transport);
        desktop.omenchat_live_opening.insert(99);
        desktop.omenchat_live_retry_after.insert(100, 2_000);
        desktop.omenchat_recent_sync_pending.insert(session_id);
        desktop
            .omenchat_recent_sync_due_after
            .insert(session_id, 2_500);

        let totals = desktop.omenchat_live_monitor_totals();

        assert_eq!(totals.sessions, 1);
        assert_eq!(totals.connected, 1);
        assert_eq!(totals.opening, 1);
        assert_eq!(totals.reconnect_timers, 1);
        assert_eq!(totals.history_sync_waiting, 1);
        assert_eq!(totals.pending_resources, 1);
        assert_eq!(totals.frames_in, 4);
        assert_eq!(totals.frames_out, 3);
        assert_eq!(totals.bytes_in, 1024);
        assert_eq!(totals.bytes_out, 512);
        assert_eq!(totals.resources_in, 2);
        assert_eq!(totals.resource_bytes_in, 2048);
        assert_eq!(totals.upload_fetches_out, 1);
        assert_eq!(totals.upload_resource_offers_in, 2);
        assert_eq!(totals.upload_inline_chunks_in, 3);
        assert_eq!(totals.upload_inline_bytes_in, 4096);
        assert_eq!(totals.upload_resources_in, 1);
        assert_eq!(totals.upload_resource_bytes_in, 8192);
        assert_eq!(totals.awaiting_pongs, 1);
    }

    #[cfg(feature = "chat-client-rns")]
    #[test]
    fn omenchat_monitor_health_line_prioritizes_actionable_states() {
        assert_eq!(
            omenchat_monitor_health_line(&OmenChatLiveMonitorTotals::default()),
            "health: no OMENchat sessions open"
        );

        let reconnecting = OmenChatLiveMonitorTotals {
            sessions: 1,
            opening: 1,
            reconnect_timers: 2,
            ..OmenChatLiveMonitorTotals::default()
        };
        assert!(omenchat_monitor_health_line(&reconnecting)
            .contains("reconnect/opening activity visible"));

        let waiting_pong = OmenChatLiveMonitorTotals {
            sessions: 1,
            connected: 1,
            awaiting_pongs: 1,
            ..OmenChatLiveMonitorTotals::default()
        };
        assert!(omenchat_monitor_health_line(&waiting_pong).contains("heartbeat pong"));

        let active_upload = OmenChatLiveMonitorTotals {
            sessions: 1,
            connected: 1,
            frames_in: 4,
            bytes_in: 1024,
            bytes_out: 512,
            upload_fetches_out: 1,
            upload_inline_chunks_in: 2,
            upload_resource_bytes_in: 2048,
            ..OmenChatLiveMonitorTotals::default()
        };
        assert!(
            omenchat_monitor_health_line(&active_upload).contains("media/upload traffic active")
        );

        let quiet = OmenChatLiveMonitorTotals {
            sessions: 1,
            connected: 1,
            ..OmenChatLiveMonitorTotals::default()
        };
        assert!(omenchat_monitor_health_line(&quiet).contains("connected and quiet"));
    }

    #[cfg(feature = "chat-client-rns")]
    #[test]
    fn omenchat_session_attention_line_prioritizes_stalls_without_network_actions() {
        let attention = |connected,
                         opening,
                         reconnect_queued,
                         awaiting_pong,
                         last_ping_age_ms,
                         heartbeat_idle_ms,
                         pending_resources,
                         history_sync_label| {
            OmenChatSessionAttention {
                connected,
                opening,
                reconnect_queued,
                awaiting_pong,
                last_ping_age_ms,
                heartbeat_idle_ms,
                pending_resources,
                history_sync_label,
            }
        };

        assert!(omenchat_session_attention_line(attention(
            false,
            false,
            false,
            false,
            None,
            None,
            0,
            "history sync: offline",
        ))
        .contains("disconnected"));

        assert!(omenchat_session_attention_line(attention(
            true,
            false,
            false,
            true,
            Some(82_000),
            Some(40_000),
            0,
            "history sync: current for live link",
        ))
        .contains("heartbeat pong overdue"));

        assert!(omenchat_session_attention_line(attention(
            true,
            false,
            false,
            false,
            None,
            None,
            0,
            "history sync: retry in 3s after 1 attempt(s)",
        ))
        .contains("recent history sync pending"));

        assert!(omenchat_session_attention_line(attention(
            true,
            false,
            false,
            false,
            None,
            None,
            2,
            "history sync: current for live link",
        ))
        .contains("pending Resource offer"));

        assert_eq!(
            omenchat_session_attention_line(attention(
                true,
                false,
                false,
                false,
                None,
                None,
                0,
                "history sync: current for live link",
            )),
            "attention: live link healthy; no action needed"
        );
    }

    #[cfg(feature = "chat-client-rns")]
    #[tokio::test]
    async fn omenchat_recent_sync_monitor_label_reports_retry_and_current() {
        let root = std::env::temp_dir().join(format!(
            "omenbrowser-rs-desktop-omenchat-sync-monitor-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let paths = crate::config::AppPaths::from_root(root);
        paths.ensure().expect("paths");
        let app = App::new(crate::config::AppConfig {
            paths,
            settings: crate::storage::settings::AppSettings::default(),
        });
        let mut desktop = DesktopApp::new(app);
        let session_id = desktop.open_omenchat_status_session(
            OmenChatDescriptor {
                server_destination: FIXTURE_CHAT_SERVER_HASH.into(),
                display_name: Some("Test OMENchat".into()),
                rooms_hint: vec!["lobby".into()],
                local_display_name: Some("tester".into()),
                ..OmenChatDescriptor::default()
            },
            "connected".into(),
        );
        let link_id = [0x72; 16];
        desktop
            .omenchat_live_transports
            .insert(session_id, DesktopOmenChatTransport::new(link_id, 1_000));
        desktop
            .omenchat_recent_sync_due_after
            .insert(session_id, 2_000);
        desktop.omenchat_recent_sync_attempts.insert(session_id, 1);

        let retry = desktop.omenchat_recent_sync_monitor_label(session_id, 1_250);
        assert!(retry.contains("retry in"));
        assert!(retry.contains("1 attempt"));

        desktop.mark_omenchat_recent_sync_complete(session_id);

        assert_eq!(
            desktop.omenchat_recent_sync_monitor_label(session_id, 2_500),
            "history sync: current for live link"
        );
    }

    #[cfg(feature = "chat-client-rns")]
    #[tokio::test]
    async fn omenchat_transport_tracks_ping_pong_rtt_for_monitoring() {
        let mut transport = DesktopOmenChatTransport::new([0x71; 16], 1_000);
        transport.last_ping_epoch_ms = 2_000;
        transport.awaiting_pong = true;

        let pong = crate::chat::protocol::Frame::new(
            crate::chat::protocol::ChatOp::Pong,
            7,
            None,
            crate::chat::protocol::FrameBody::Empty,
        );
        transport.push_incoming_frame(
            crate::chat::codec::encode_frame(&pong).expect("encode pong"),
            2_125,
        );

        assert_eq!(transport.pongs_in, 1);
        assert_eq!(transport.last_pong_epoch_ms, 2_125);
        assert_eq!(transport.last_ping_rtt_ms, Some(125));
        assert!(!transport.awaiting_pong);
    }

    #[cfg(feature = "chat-client-rns")]
    #[tokio::test]
    async fn omenchat_registered_live_transport_syncs_recent_history() {
        let root = std::env::temp_dir().join(format!(
            "omenbrowser-rs-desktop-omenchat-sync-recent-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let paths = crate::config::AppPaths::from_root(root);
        paths.ensure().expect("paths");
        let app = App::new(crate::config::AppConfig {
            paths,
            settings: crate::storage::settings::AppSettings::default(),
        });
        let mut desktop = DesktopApp::new(app);
        let descriptor = OmenChatDescriptor {
            server_destination: FIXTURE_CHAT_SERVER_HASH.into(),
            display_name: Some("Test OMENchat".into()),
            rooms_hint: vec!["lobby".into()],
            local_display_name: Some("tester".into()),
            ..OmenChatDescriptor::default()
        };
        let session_id = desktop.open_omenchat_status_session(descriptor, "connected".into());
        let link_id = [0x61; 16];
        let mut transport = DesktopOmenChatTransport::new(link_id, current_epoch_ms());
        let recent = crate::chat::protocol::Frame::new(
            crate::chat::protocol::ChatOp::HistoryInline,
            9,
            Some(1),
            crate::chat::protocol::batch::compressed_values_body(&[
                crate::chat::protocol::FrameValue::Array(vec![
                    crate::chat::protocol::FrameValue::U64(7),
                    crate::chat::protocol::FrameValue::U64(1),
                    crate::chat::protocol::FrameValue::U64(2),
                    crate::chat::protocol::FrameValue::I64(123),
                    crate::chat::protocol::FrameValue::String("missed while offline".into()),
                    crate::chat::protocol::FrameValue::String("Peer".into()),
                ]),
            ])
            .expect("history body"),
        );
        transport.push_incoming_frame(
            crate::chat::codec::encode_frame(&recent).expect("encode frame"),
            current_epoch_ms(),
        );
        let _ = desktop.register_omenchat_live_transport(session_id, transport);

        let events = desktop.sync_recent_omenchat_room_history(session_id);

        assert!(
            matches!(
                events.as_slice(),
                [ChatClientEvent::HistoryPrepended { events, .. }]
                    if events.iter().map(|event| event.event_id).collect::<Vec<_>>() == vec![7]
            ),
            "events: {events:?}"
        );
        let session = desktop.chat_client.session(session_id).expect("session");
        assert!(session.events.iter().any(|event| {
            event.room_id == 1
                && event.event_id == 7
                && matches!(&event.kind, ChatEventKind::Message { body } if body == "missed while offline")
        }));
        assert_eq!(
            desktop.chat_scroll_offsets.get(&(session_id, 1)),
            Some(&RelativeOffset { x: 0.0, y: 1.0 })
        );
        assert!(desktop.restore_workspace_scrolls_pending);
    }

    #[cfg(feature = "chat-client-rns")]
    #[tokio::test]
    async fn omenchat_recent_sync_preserves_manual_scrollback() {
        let root = std::env::temp_dir().join(format!(
            "omenbrowser-rs-desktop-omenchat-sync-preserve-scrollback-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let paths = crate::config::AppPaths::from_root(root);
        paths.ensure().expect("paths");
        let app = App::new(crate::config::AppConfig {
            paths,
            settings: crate::storage::settings::AppSettings::default(),
        });
        let mut desktop = DesktopApp::new(app);
        let descriptor = OmenChatDescriptor {
            server_destination: FIXTURE_CHAT_SERVER_HASH.into(),
            display_name: Some("Test OMENchat".into()),
            rooms_hint: vec!["lobby".into()],
            local_display_name: Some("tester".into()),
            ..OmenChatDescriptor::default()
        };
        let session_id = desktop.open_omenchat_status_session(descriptor, "connected".into());
        let link_id = [0x66; 16];
        let mut transport = DesktopOmenChatTransport::new(link_id, current_epoch_ms());
        let recent = crate::chat::protocol::Frame::new(
            crate::chat::protocol::ChatOp::HistoryInline,
            13,
            Some(1),
            crate::chat::protocol::batch::compressed_values_body(&[
                crate::chat::protocol::FrameValue::Array(vec![
                    crate::chat::protocol::FrameValue::U64(11),
                    crate::chat::protocol::FrameValue::U64(1),
                    crate::chat::protocol::FrameValue::U64(2),
                    crate::chat::protocol::FrameValue::I64(127),
                    crate::chat::protocol::FrameValue::String("history while reading".into()),
                    crate::chat::protocol::FrameValue::String("Peer".into()),
                ]),
            ])
            .expect("history body"),
        );
        transport.push_incoming_frame(
            crate::chat::codec::encode_frame(&recent).expect("encode frame"),
            current_epoch_ms(),
        );
        desktop
            .chat_scroll_offsets
            .insert((session_id, 1), RelativeOffset { x: 0.0, y: 0.41 });
        let _ = desktop.register_omenchat_live_transport(session_id, transport);

        let events = desktop.sync_recent_omenchat_room_history(session_id);

        assert!(matches!(
            events.as_slice(),
            [ChatClientEvent::HistoryPrepended { .. }]
        ));
        assert_eq!(
            desktop.chat_scroll_offsets.get(&(session_id, 1)),
            Some(&RelativeOffset { x: 0.0, y: 0.41 })
        );
    }

    #[cfg(feature = "chat-client-rns")]
    #[tokio::test]
    async fn omenchat_live_transport_due_sync_catches_restored_room_without_join_event() {
        let root = std::env::temp_dir().join(format!(
            "omenbrowser-rs-desktop-omenchat-due-sync-recent-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let paths = crate::config::AppPaths::from_root(root);
        paths.ensure().expect("paths");
        let app = App::new(crate::config::AppConfig {
            paths,
            settings: crate::storage::settings::AppSettings::default(),
        });
        let mut desktop = DesktopApp::new(app);
        let descriptor = OmenChatDescriptor {
            server_destination: FIXTURE_CHAT_SERVER_HASH.into(),
            display_name: Some("Test OMENchat".into()),
            rooms_hint: vec!["lobby".into()],
            local_display_name: Some("tester".into()),
            ..OmenChatDescriptor::default()
        };
        let session_id = desktop.open_omenchat_status_session(descriptor, "restored".into());
        let link_id = [0x63; 16];
        let mut transport = DesktopOmenChatTransport::new(link_id, current_epoch_ms());
        let recent = crate::chat::protocol::Frame::new(
            crate::chat::protocol::ChatOp::HistoryInline,
            11,
            Some(1),
            crate::chat::protocol::batch::compressed_values_body(&[
                crate::chat::protocol::FrameValue::Array(vec![
                    crate::chat::protocol::FrameValue::U64(9),
                    crate::chat::protocol::FrameValue::U64(1),
                    crate::chat::protocol::FrameValue::U64(2),
                    crate::chat::protocol::FrameValue::I64(125),
                    crate::chat::protocol::FrameValue::String("restored missed event".into()),
                    crate::chat::protocol::FrameValue::String("Peer".into()),
                ]),
            ])
            .expect("history body"),
        );
        transport.push_incoming_frame(
            crate::chat::codec::encode_frame(&recent).expect("encode frame"),
            current_epoch_ms(),
        );

        let _ = desktop.register_omenchat_live_transport(session_id, transport);
        assert!(desktop
            .omenchat_recent_sync_due_after
            .contains_key(&session_id));

        let _ = desktop.sync_due_omenchat_recent_history(current_epoch_ms().saturating_add(2_000));

        assert!(!desktop
            .omenchat_recent_sync_due_after
            .contains_key(&session_id));
        assert_eq!(
            desktop.omenchat_recent_sync_links.get(&session_id),
            Some(&link_id)
        );
        let session = desktop.chat_client.session(session_id).expect("session");
        assert!(session.events.iter().any(|event| {
            event.room_id == 1
                && event.event_id == 9
                && matches!(&event.kind, ChatEventKind::Message { body } if body == "restored missed event")
        }));
    }

    #[cfg(feature = "chat-client-rns")]
    #[tokio::test]
    async fn omenchat_recent_sync_request_alone_does_not_suppress_later_join_sync() {
        let root = std::env::temp_dir().join(format!(
            "omenbrowser-rs-desktop-omenchat-unsatisfied-sync-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let paths = crate::config::AppPaths::from_root(root);
        paths.ensure().expect("paths");
        let app = App::new(crate::config::AppConfig {
            paths,
            settings: crate::storage::settings::AppSettings::default(),
        });
        let mut desktop = DesktopApp::new(app);
        let server_id = FIXTURE_CHAT_SERVER_HASH;
        let descriptor = OmenChatDescriptor {
            server_destination: server_id.into(),
            display_name: Some("Test OMENchat".into()),
            rooms_hint: vec!["lobby".into()],
            local_display_name: Some("tester".into()),
            ..OmenChatDescriptor::default()
        };
        let session_id = desktop.open_omenchat_status_session(descriptor, "restored".into());
        let link_id = [0x64; 16];
        let _ = desktop.register_omenchat_live_transport(
            session_id,
            DesktopOmenChatTransport::new(link_id, current_epoch_ms()),
        );

        let _ = desktop.sync_due_omenchat_recent_history(current_epoch_ms().saturating_add(2_000));

        assert!(!desktop.omenchat_recent_sync_links.contains_key(&session_id));
        assert!(desktop
            .omenchat_recent_sync_due_after
            .contains_key(&session_id));
        assert_eq!(
            desktop
                .omenchat_recent_sync_attempts
                .get(&session_id)
                .copied(),
            Some(1)
        );

        let mut transport = desktop
            .omenchat_live_transports
            .remove(&session_id)
            .expect("transport");
        let recent = crate::chat::protocol::Frame::new(
            crate::chat::protocol::ChatOp::HistoryInline,
            12,
            Some(1),
            crate::chat::protocol::batch::compressed_values_body(&[
                crate::chat::protocol::FrameValue::Array(vec![
                    crate::chat::protocol::FrameValue::U64(10),
                    crate::chat::protocol::FrameValue::U64(1),
                    crate::chat::protocol::FrameValue::U64(2),
                    crate::chat::protocol::FrameValue::I64(126),
                    crate::chat::protocol::FrameValue::String("join-triggered sync".into()),
                    crate::chat::protocol::FrameValue::String("Peer".into()),
                ]),
            ])
            .expect("history body"),
        );
        transport.push_incoming_frame(
            crate::chat::codec::encode_frame(&recent).expect("encode frame"),
            current_epoch_ms(),
        );
        desktop
            .omenchat_live_transports
            .insert(session_id, transport);

        desktop.apply_omenchat_client_events_status(&[ChatClientEvent::RoomJoined {
            session_id,
            room: crate::chat::model::ChatRoomSummary {
                server_id: server_id.into(),
                room_id: 1,
                name: "lobby".into(),
                topic: None,
                unread: 0,
                joined: true,
            },
            users: Vec::new(),
            latest_events: Vec::new(),
        }]);

        assert_eq!(
            desktop.omenchat_recent_sync_links.get(&session_id),
            Some(&link_id)
        );
        let session = desktop.chat_client.session(session_id).expect("session");
        assert!(session.events.iter().any(|event| {
            event.room_id == 1
                && event.event_id == 10
                && matches!(&event.kind, ChatEventKind::Message { body } if body == "join-triggered sync")
        }));
    }

    #[cfg(feature = "chat-client-rns")]
    #[tokio::test]
    async fn omenchat_room_join_before_transport_registers_defers_recent_sync() {
        let root = std::env::temp_dir().join(format!(
            "omenbrowser-rs-desktop-omenchat-deferred-sync-recent-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let paths = crate::config::AppPaths::from_root(root);
        paths.ensure().expect("paths");
        let app = App::new(crate::config::AppConfig {
            paths,
            settings: crate::storage::settings::AppSettings::default(),
        });
        let mut desktop = DesktopApp::new(app);
        let server_id = FIXTURE_CHAT_SERVER_HASH;
        let descriptor = OmenChatDescriptor {
            server_destination: server_id.into(),
            display_name: Some("Test OMENchat".into()),
            rooms_hint: vec!["lobby".into()],
            local_display_name: Some("tester".into()),
            ..OmenChatDescriptor::default()
        };
        let session_id = desktop.open_omenchat_status_session(descriptor, "joining".into());

        desktop.apply_omenchat_client_events_status(&[ChatClientEvent::RoomJoined {
            session_id,
            room: crate::chat::model::ChatRoomSummary {
                server_id: server_id.into(),
                room_id: 1,
                name: "lobby".into(),
                topic: None,
                unread: 0,
                joined: true,
            },
            users: Vec::new(),
            latest_events: Vec::new(),
        }]);
        assert!(desktop.omenchat_recent_sync_pending.contains(&session_id));

        let link_id = [0x62; 16];
        let mut transport = DesktopOmenChatTransport::new(link_id, current_epoch_ms());
        let recent = crate::chat::protocol::Frame::new(
            crate::chat::protocol::ChatOp::HistoryInline,
            10,
            Some(1),
            crate::chat::protocol::batch::compressed_values_body(&[
                crate::chat::protocol::FrameValue::Array(vec![
                    crate::chat::protocol::FrameValue::U64(8),
                    crate::chat::protocol::FrameValue::U64(1),
                    crate::chat::protocol::FrameValue::U64(2),
                    crate::chat::protocol::FrameValue::I64(124),
                    crate::chat::protocol::FrameValue::String("deferred missed event".into()),
                    crate::chat::protocol::FrameValue::String("Peer".into()),
                ]),
            ])
            .expect("history body"),
        );
        transport.push_incoming_frame(
            crate::chat::codec::encode_frame(&recent).expect("encode frame"),
            current_epoch_ms(),
        );

        let _ = desktop.register_omenchat_live_transport(session_id, transport);

        assert!(!desktop.omenchat_recent_sync_pending.contains(&session_id));
        let session = desktop.chat_client.session(session_id).expect("session");
        assert!(session.events.iter().any(|event| {
            event.room_id == 1
                && event.event_id == 8
                && matches!(&event.kind, ChatEventKind::Message { body } if body == "deferred missed event")
        }));
    }

    #[cfg(feature = "chat-client-rns")]
    #[test]
    fn omenchat_timeout_close_marks_session_for_quick_reconnect() {
        let root = std::env::temp_dir().join(format!(
            "omenbrowser-rs-desktop-omenchat-timeout-close-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let paths = crate::config::AppPaths::from_root(root);
        paths.ensure().expect("paths");
        let app = App::new(crate::config::AppConfig {
            paths,
            settings: crate::storage::settings::AppSettings::default(),
        });
        let mut desktop = DesktopApp::new(app);
        let descriptor = OmenChatDescriptor {
            server_destination: FIXTURE_CHAT_SERVER_HASH.into(),
            display_name: Some("Test OMENchat".into()),
            rooms_hint: vec!["lobby".into()],
            local_display_name: Some("tester".into()),
            ..OmenChatDescriptor::default()
        };
        let session_id = desktop.open_omenchat_status_session(descriptor, "connected".into());
        let link_id = [0x42; 16];
        desktop.omenchat_live_transports.insert(
            session_id,
            DesktopOmenChatTransport::new(link_id, current_epoch_ms()),
        );
        desktop.omenchat_link_sessions.insert(link_id, session_id);
        assert!(desktop.app.enqueue_runtime_event(
            crate::runtime::RuntimeBusEvent::OmenChatLinkClosed(
                crate::runtime::OmenChatLinkClosed {
                    link_id,
                    reason: Some("Timeout".into()),
                },
            ),
        ));
        assert_eq!(desktop.app.drain_internal_events(), 1);

        let _ = desktop.drain_omenchat_runtime_events();

        assert!(!desktop.omenchat_live_transports.contains_key(&session_id));
        assert!(desktop.omenchat_live_retry_after.contains_key(&session_id));
        assert_eq!(
            desktop
                .omenchat_live_last_disconnect_reason
                .get(&session_id)
                .map(String::as_str),
            Some("Timeout")
        );
        let status = desktop
            .chat_client
            .session(session_id)
            .expect("session")
            .status
            .clone();
        assert!(status.contains("link timed out"));
        assert!(status.contains("reconnecting"));
    }

    #[cfg(feature = "chat-client-rns")]
    #[test]
    fn omenchat_destination_closed_marks_session_for_quick_reconnect() {
        let root = std::env::temp_dir().join(format!(
            "omenbrowser-rs-desktop-omenchat-destination-closed-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let paths = crate::config::AppPaths::from_root(root);
        paths.ensure().expect("paths");
        let app = App::new(crate::config::AppConfig {
            paths,
            settings: crate::storage::settings::AppSettings::default(),
        });
        let mut desktop = DesktopApp::new(app);
        let descriptor = OmenChatDescriptor {
            server_destination: FIXTURE_CHAT_SERVER_HASH.into(),
            display_name: Some("Test OMENchat".into()),
            rooms_hint: vec!["lobby".into()],
            local_display_name: Some("tester".into()),
            ..OmenChatDescriptor::default()
        };
        let session_id = desktop.open_omenchat_status_session(descriptor, "connected".into());
        let link_id = [0x24; 16];
        desktop.omenchat_live_transports.insert(
            session_id,
            DesktopOmenChatTransport::new(link_id, current_epoch_ms()),
        );
        desktop.omenchat_link_sessions.insert(link_id, session_id);
        assert!(desktop.app.enqueue_runtime_event(
            crate::runtime::RuntimeBusEvent::OmenChatLinkClosed(
                crate::runtime::OmenChatLinkClosed {
                    link_id,
                    reason: Some("DestinationClosed".into()),
                },
            ),
        ));
        assert_eq!(desktop.app.drain_internal_events(), 1);

        let _ = desktop.drain_omenchat_runtime_events();

        assert!(!desktop.omenchat_live_transports.contains_key(&session_id));
        assert!(desktop.omenchat_live_retry_after.contains_key(&session_id));
        let status = desktop
            .chat_client
            .session(session_id)
            .expect("session")
            .status
            .clone();
        assert!(status.contains("link closed"));
        assert!(status.contains("reconnecting"));
    }

    #[cfg(feature = "chat-client-rns")]
    #[test]
    fn omenchat_non_retryable_close_waits_for_manual_reconnect() {
        let root = std::env::temp_dir().join(format!(
            "omenbrowser-rs-desktop-omenchat-non-retry-close-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let paths = crate::config::AppPaths::from_root(root);
        paths.ensure().expect("paths");
        let app = App::new(crate::config::AppConfig {
            paths,
            settings: crate::storage::settings::AppSettings::default(),
        });
        let mut desktop = DesktopApp::new(app);
        let descriptor = OmenChatDescriptor {
            server_destination: FIXTURE_CHAT_SERVER_HASH.into(),
            display_name: Some("Test OMENchat".into()),
            rooms_hint: vec!["lobby".into()],
            local_display_name: Some("tester".into()),
            ..OmenChatDescriptor::default()
        };
        let session_id = desktop.open_omenchat_status_session(descriptor, "connected".into());
        let link_id = [0x66; 16];
        desktop.omenchat_live_transports.insert(
            session_id,
            DesktopOmenChatTransport::new(link_id, current_epoch_ms()),
        );
        desktop.omenchat_link_sessions.insert(link_id, session_id);
        assert!(desktop.app.enqueue_runtime_event(
            crate::runtime::RuntimeBusEvent::OmenChatLinkClosed(
                crate::runtime::OmenChatLinkClosed {
                    link_id,
                    reason: Some("ResourceExhausted".into()),
                },
            ),
        ));
        assert_eq!(desktop.app.drain_internal_events(), 1);

        let _ = desktop.drain_omenchat_runtime_events();

        assert!(!desktop.omenchat_live_transports.contains_key(&session_id));
        assert!(!desktop.omenchat_live_retry_after.contains_key(&session_id));
        assert_eq!(
            desktop.omenchat_reconnect_state_label(session_id, current_epoch_ms()),
            "reconnect: manual"
        );
        let status = desktop
            .chat_client
            .session(session_id)
            .expect("session")
            .status
            .clone();
        assert!(status.contains("use Reconnect"));
    }

    #[cfg(feature = "chat-client-rns")]
    #[test]
    fn omenchat_stale_link_close_does_not_disconnect_active_link() {
        let root = std::env::temp_dir().join(format!(
            "omenbrowser-rs-desktop-omenchat-stale-link-close-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let paths = crate::config::AppPaths::from_root(root);
        paths.ensure().expect("paths");
        let app = App::new(crate::config::AppConfig {
            paths,
            settings: crate::storage::settings::AppSettings::default(),
        });
        let mut desktop = DesktopApp::new(app);
        let descriptor = OmenChatDescriptor {
            server_destination: FIXTURE_CHAT_SERVER_HASH.into(),
            display_name: Some("Test OMENchat".into()),
            rooms_hint: vec!["lobby".into()],
            local_display_name: Some("tester".into()),
            ..OmenChatDescriptor::default()
        };
        let session_id =
            desktop.open_omenchat_status_session(descriptor, "live userlist updated".into());
        let old_link_id = [0x11; 16];
        let active_link_id = [0x22; 16];
        desktop
            .omenchat_link_sessions
            .insert(old_link_id, session_id);
        let _ = desktop.register_omenchat_live_transport(
            session_id,
            DesktopOmenChatTransport::new(active_link_id, current_epoch_ms()),
        );
        desktop
            .omenchat_link_sessions
            .insert(old_link_id, session_id);

        assert!(desktop.app.enqueue_runtime_event(
            crate::runtime::RuntimeBusEvent::OmenChatLinkClosed(
                crate::runtime::OmenChatLinkClosed {
                    link_id: old_link_id,
                    reason: Some("Timeout".into()),
                },
            ),
        ));
        assert_eq!(desktop.app.drain_internal_events(), 1);

        let _ = desktop.drain_omenchat_runtime_events();

        assert_eq!(
            desktop
                .omenchat_live_transports
                .get(&session_id)
                .map(|transport| transport.link_id),
            Some(active_link_id)
        );
        assert!(!desktop.omenchat_live_retry_after.contains_key(&session_id));
        assert_eq!(
            desktop
                .chat_client
                .session(session_id)
                .expect("session")
                .status,
            "live userlist updated"
        );
    }

    #[test]
    fn close_pane_does_not_close_backing_browser_tab() {
        let root = std::env::temp_dir().join(format!(
            "omenbrowser-rs-desktop-close-pane-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let paths = crate::config::AppPaths::from_root(root);
        paths.ensure().expect("paths");
        let app = App::new(crate::config::AppConfig {
            paths,
            settings: crate::storage::settings::AppSettings::default(),
        });
        let mut desktop = DesktopApp::new(app);
        let pane = desktop
            .workspace_panes
            .iter()
            .find_map(|(pane, kind)| matches!(kind, DesktopPane::Browser(_)).then_some(*pane))
            .expect("browser pane");
        let initial_tabs = desktop.app.workspace.browser_tabs.len();

        let _ = desktop.update(Message::WorkspacePaneClose(pane));

        assert_eq!(desktop.app.workspace.browser_tabs.len(), initial_tabs);
        assert!(!desktop
            .workspace_panes
            .iter()
            .any(|(candidate, _)| *candidate == pane));
    }

    #[test]
    fn close_tab_button_closes_backing_browser_tab_and_pane() {
        let root = std::env::temp_dir().join(format!(
            "omenbrowser-rs-desktop-close-tab-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let paths = crate::config::AppPaths::from_root(root);
        paths.ensure().expect("paths");
        let app = App::new(crate::config::AppConfig {
            paths,
            settings: crate::storage::settings::AppSettings::default(),
        });
        let mut desktop = DesktopApp::new(app);
        let _ = desktop.update(Message::NewBrowserTab);
        let closing_id = desktop.app.active_browser_tab().id;
        let initial_tabs = desktop.app.workspace.browser_tabs.len();

        let _ = desktop.update(Message::CloseBrowserPaneTab(closing_id));

        assert_eq!(desktop.app.workspace.browser_tabs.len(), initial_tabs - 1);
        assert!(!desktop
            .workspace_panes
            .iter()
            .any(|(_, pane)| *pane == DesktopPane::Browser(closing_id)));
    }

    #[test]
    fn desktop_workspace_panes_restore_from_settings_order() {
        let root = std::env::temp_dir().join(format!(
            "omenbrowser-rs-desktop-restore-panes-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let paths = crate::config::AppPaths::from_root(root);
        paths.ensure().expect("paths");
        let mut settings = crate::storage::settings::AppSettings::default();
        settings.browser_tabs = vec![
            crate::storage::settings::BrowserTabSettings {
                title: "One".into(),
                address_input: "mock.page:/one.mu".into(),
                current_url: "mock.page:/one.mu".into(),
                ..Default::default()
            },
            crate::storage::settings::BrowserTabSettings {
                title: "Two".into(),
                address_input: "mock.page:/two.mu".into(),
                current_url: "mock.page:/two.mu".into(),
                ..Default::default()
            },
        ];
        settings.ui.desktop_workspace_panes = vec![
            DesktopWorkspacePaneSettings {
                kind: DesktopWorkspacePaneKind::Conversation,
                index: 0,
            },
            DesktopWorkspacePaneSettings {
                kind: DesktopWorkspacePaneKind::Browser,
                index: 1,
            },
        ];
        settings.ui.active_desktop_workspace_pane = Some(1);
        let app = App::new(crate::config::AppConfig { paths, settings });
        let desktop = DesktopApp::new(app);
        let second_tab_id = desktop.app.workspace.browser_tabs[1].id;

        assert!(desktop
            .workspace_panes
            .iter()
            .any(|(_, pane)| *pane == DesktopPane::Browser(second_tab_id)));
        assert_eq!(
            desktop.workspace_panes.get(desktop.active_workspace_pane),
            Some(&DesktopPane::Browser(second_tab_id))
        );
    }

    #[cfg(feature = "chat-client")]
    #[test]
    fn desktop_omenchat_sessions_restore_from_plugin_store() {
        let root = std::env::temp_dir().join(format!(
            "omenbrowser-rs-desktop-restore-omenchat-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let paths = crate::config::AppPaths::from_root(root);
        paths.ensure().expect("paths");

        let store_path = paths
            .identity_storage_root()
            .join("plugins")
            .join(crate::plugins::BUILTIN_OMENCHAT_PLUGIN_ID)
            .join("chat.sqlite");
        let mut store = SqliteChatStore::open(&store_path).expect("chat store");
        let mut client = ChatClient::new();
        let session_id = client.reserve_session_id();
        client.push_session(crate::chat::ChatSessionView {
            session_id,
            server: crate::chat::model::ChatServerSummary {
                server_id: "abcd1234abcd1234abcd1234abcd1234".into(),
                destination: "abcd1234abcd1234abcd1234abcd1234".into(),
                display_name: "Restored OMENchat".into(),
            },
            rooms: vec![crate::chat::model::ChatRoomSummary {
                server_id: "abcd1234abcd1234abcd1234abcd1234".into(),
                room_id: 1,
                name: "lobby".into(),
                topic: None,
                unread: 0,
                joined: true,
            }],
            active_room: crate::chat::model::ChatRoomSummary {
                server_id: "abcd1234abcd1234abcd1234abcd1234".into(),
                room_id: 1,
                name: "lobby".into(),
                topic: None,
                unread: 0,
                joined: true,
            },
            users: Vec::new(),
            events: Vec::new(),
            status: "test".into(),
        });
        client
            .persist_session(&mut store, session_id)
            .expect("persist chat session");

        let mut settings = crate::storage::settings::AppSettings::default();
        settings.ui.desktop_workspace_panes = vec![DesktopWorkspacePaneSettings {
            kind: DesktopWorkspacePaneKind::OmenChat,
            index: 0,
        }];
        settings.ui.desktop_workspace_layout = Some(DesktopWorkspaceLayoutNode::Pane {
            pane: DesktopWorkspacePaneSettings {
                kind: DesktopWorkspacePaneKind::OmenChat,
                index: 0,
            },
        });

        let app = App::new(crate::config::AppConfig { paths, settings });
        let desktop = DesktopApp::new(app);

        let restored = desktop
            .chat_client
            .sessions()
            .iter()
            .find(|session| session.server.display_name == "Restored OMENchat")
            .expect("restored session");
        assert!(desktop
            .workspace_panes
            .iter()
            .any(|(_, pane)| *pane == DesktopPane::OmenChat(restored.session_id)));
    }

    #[cfg(feature = "chat-client")]
    #[test]
    fn desktop_startup_prunes_unrestorable_omenchat_cache_rows() {
        let root = std::env::temp_dir().join(format!(
            "omenbrowser-rs-desktop-prune-omenchat-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let paths = crate::config::AppPaths::from_root(root);
        paths.ensure().expect("paths");

        let store_path = paths
            .identity_storage_root()
            .join("plugins")
            .join(crate::plugins::BUILTIN_OMENCHAT_PLUGIN_ID)
            .join("chat.sqlite");
        let mut store = SqliteChatStore::open(&store_path).expect("chat store");
        for (server_id, destination) in [
            ("mock-server", "mockchatdestination"),
            ("pending-server", "pending-omenchat-1"),
        ] {
            store
                .save_server(crate::chat::model::ChatServerSummary {
                    server_id: server_id.into(),
                    destination: destination.into(),
                    display_name: "Old Dev Chat".into(),
                })
                .expect("save server");
            store
                .save_room(crate::chat::model::ChatRoomSummary {
                    server_id: server_id.into(),
                    room_id: 1,
                    name: "lobby".into(),
                    topic: None,
                    unread: 0,
                    joined: true,
                })
                .expect("save room");
        }
        drop(store);

        let app = App::new(crate::config::AppConfig {
            paths: paths.clone(),
            settings: crate::storage::settings::AppSettings::default(),
        });
        let desktop = DesktopApp::new(app);

        assert!(desktop.chat_client.sessions().is_empty());
        assert!(desktop
            .chat_store
            .as_ref()
            .expect("store")
            .saved_servers()
            .expect("servers")
            .is_empty());
    }

    #[test]
    fn desktop_workspace_layout_uses_stable_generated_split() {
        let root = std::env::temp_dir().join(format!(
            "omenbrowser-rs-desktop-restore-layout-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let paths = crate::config::AppPaths::from_root(root);
        paths.ensure().expect("paths");
        let mut settings = crate::storage::settings::AppSettings::default();
        settings.ui.desktop_workspace_layout = Some(DesktopWorkspaceLayoutNode::Split {
            axis: DesktopWorkspaceSplitAxis::Horizontal,
            ratio: 0.37,
            a: Box::new(DesktopWorkspaceLayoutNode::Pane {
                pane: DesktopWorkspacePaneSettings {
                    kind: DesktopWorkspacePaneKind::Browser,
                    index: 99,
                },
            }),
            b: Box::new(DesktopWorkspaceLayoutNode::Pane {
                pane: DesktopWorkspacePaneSettings {
                    kind: DesktopWorkspacePaneKind::Conversation,
                    index: 99,
                },
            }),
        });
        let app = App::new(crate::config::AppConfig { paths, settings });
        let desktop = DesktopApp::new(app);

        match desktop.workspace_panes.layout() {
            pane_grid::Node::Split { axis, ratio, .. } => {
                assert_eq!(*axis, pane_grid::Axis::Vertical);
                assert!((*ratio - 0.5).abs() < f32::EPSILON);
            }
            pane_grid::Node::Pane(_) => panic!("expected generated split layout"),
        }
    }

    #[test]
    fn desktop_workspace_layout_restores_multi_pane_startup_layout() {
        let root = std::env::temp_dir().join(format!(
            "omenbrowser-rs-desktop-restore-heavy-layout-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let paths = crate::config::AppPaths::from_root(root);
        paths.ensure().expect("paths");
        let mut settings = crate::storage::settings::AppSettings::default();
        settings.browser_tabs = (0..4)
            .map(|index| crate::storage::settings::BrowserTabSettings {
                title: format!("Browser {index}"),
                address_input: format!("mock.page:/tab-{index}.mu"),
                current_url: format!("mock.page:/tab-{index}.mu"),
                ..Default::default()
            })
            .collect();
        settings.ui.desktop_workspace_layout = Some(DesktopWorkspaceLayoutNode::Split {
            axis: DesktopWorkspaceSplitAxis::Vertical,
            ratio: 0.33,
            a: Box::new(DesktopWorkspaceLayoutNode::Pane {
                pane: DesktopWorkspacePaneSettings {
                    kind: DesktopWorkspacePaneKind::Browser,
                    index: 0,
                },
            }),
            b: Box::new(DesktopWorkspaceLayoutNode::Split {
                axis: DesktopWorkspaceSplitAxis::Horizontal,
                ratio: 0.5,
                a: Box::new(DesktopWorkspaceLayoutNode::Pane {
                    pane: DesktopWorkspacePaneSettings {
                        kind: DesktopWorkspacePaneKind::Browser,
                        index: 1,
                    },
                }),
                b: Box::new(DesktopWorkspaceLayoutNode::Split {
                    axis: DesktopWorkspaceSplitAxis::Vertical,
                    ratio: 0.5,
                    a: Box::new(DesktopWorkspaceLayoutNode::Pane {
                        pane: DesktopWorkspacePaneSettings {
                            kind: DesktopWorkspacePaneKind::Browser,
                            index: 2,
                        },
                    }),
                    b: Box::new(DesktopWorkspaceLayoutNode::Pane {
                        pane: DesktopWorkspacePaneSettings {
                            kind: DesktopWorkspacePaneKind::Browser,
                            index: 3,
                        },
                    }),
                }),
            }),
        });
        settings.ui.desktop_workspace_panes = (0..4)
            .map(|index| DesktopWorkspacePaneSettings {
                kind: DesktopWorkspacePaneKind::Browser,
                index,
            })
            .collect();
        let app = App::new(crate::config::AppConfig { paths, settings });
        let desktop = DesktopApp::new(app);

        assert_eq!(desktop.workspace_panes.len(), 4);
        for tab in &desktop.app.workspace.browser_tabs {
            assert!(desktop
                .workspace_panes
                .iter()
                .any(|(_, pane)| *pane == DesktopPane::Browser(tab.id)));
        }
    }

    #[test]
    fn desktop_workspace_layout_persists_current_split_ratio() {
        let root = std::env::temp_dir().join(format!(
            "omenbrowser-rs-desktop-persist-layout-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let paths = crate::config::AppPaths::from_root(root);
        paths.ensure().expect("paths");
        let app = App::new(crate::config::AppConfig {
            paths,
            settings: crate::storage::settings::AppSettings::default(),
        });
        let mut desktop = DesktopApp::new(app);
        let split = *desktop
            .workspace_panes
            .layout()
            .splits()
            .next()
            .expect("initial split");

        let _ = desktop.update(Message::WorkspacePaneResized(pane_grid::ResizeEvent {
            split,
            ratio: 0.64,
        }));

        let Some(DesktopWorkspaceLayoutNode::Split { axis, ratio, .. }) =
            desktop.app.settings.ui.desktop_workspace_layout.as_ref()
        else {
            panic!("expected persisted split layout");
        };
        assert_eq!(*axis, DesktopWorkspaceSplitAxis::Vertical);
        assert!((*ratio - 0.64).abs() < f32::EPSILON);
    }

    #[test]
    fn resizing_workspace_pane_anchors_visible_chat_scroll_to_bottom() {
        let root = std::env::temp_dir().join(format!(
            "omenbrowser-rs-desktop-resize-scroll-bottom-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let paths = crate::config::AppPaths::from_root(root);
        paths.ensure().expect("paths");
        let app = App::new(crate::config::AppConfig {
            paths,
            settings: crate::storage::settings::AppSettings::default(),
        });
        let mut desktop = DesktopApp::new(app);
        let conversation_id = desktop.app.active_conversation().id;
        desktop.ensure_pane_for_active_conversation();
        desktop
            .conversation_scroll_offsets
            .insert(conversation_id, RelativeOffset { x: 0.0, y: 0.38 });
        desktop.restore_workspace_scrolls_pending = false;
        desktop.restore_workspace_scrolls_remaining = 0;
        desktop.restore_workspace_scroll_locks_release_pending = false;
        desktop.conversation_scroll_restore_locks.clear();
        let split = *desktop
            .workspace_panes
            .layout()
            .splits()
            .next()
            .expect("conversation split");

        let _ = desktop.update(Message::WorkspacePaneResized(pane_grid::ResizeEvent {
            split,
            ratio: 0.42,
        }));

        assert_eq!(
            desktop.conversation_scroll_offsets.get(&conversation_id),
            Some(&RelativeOffset { x: 0.0, y: 1.0 })
        );
    }

    #[test]
    fn new_conversation_messages_do_not_force_scroll_when_reading_history() {
        let root = std::env::temp_dir().join(format!(
            "omenbrowser-rs-desktop-conversation-follow-mode-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let paths = crate::config::AppPaths::from_root(root);
        paths.ensure().expect("paths");
        let app = App::new(crate::config::AppConfig {
            paths,
            settings: crate::storage::settings::AppSettings::default(),
        });
        let mut desktop = DesktopApp::new(app);
        let conversation_id = desktop.app.active_conversation().id;
        desktop.ensure_pane_for_active_conversation();
        desktop
            .conversation_message_counts
            .insert(conversation_id, 0);
        desktop
            .conversation_scroll_offsets
            .insert(conversation_id, RelativeOffset { x: 0.0, y: 0.42 });

        desktop
            .app
            .workspace
            .conversations
            .iter_mut()
            .find(|conversation| conversation.id == conversation_id)
            .expect("conversation")
            .push_message(crate::messaging::MessageSummary {
                peer_hash: "peer".into(),
                peer_label: "Peer".into(),
                title: "hello".into(),
                content: "while reading history".into(),
                timestamp: 1.0,
                transport_method: crate::messaging::TransportMethod::Direct,
                delivered: true,
                failed: false,
                incoming: true,
                unread: true,
                message_id: Some("message-1".into()),
                fields: Default::default(),
                attachments: Vec::new(),
            });

        let _ = desktop.snap_conversations_with_new_messages_to_bottom();

        assert_eq!(
            desktop.conversation_scroll_offsets.get(&conversation_id),
            Some(&RelativeOffset { x: 0.0, y: 0.42 })
        );
        assert!(desktop.conversation_is_viewing_history(conversation_id));
    }

    #[test]
    fn new_conversation_messages_follow_bottom_when_already_at_present() {
        let root = std::env::temp_dir().join(format!(
            "omenbrowser-rs-desktop-conversation-follow-bottom-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let paths = crate::config::AppPaths::from_root(root);
        paths.ensure().expect("paths");
        let app = App::new(crate::config::AppConfig {
            paths,
            settings: crate::storage::settings::AppSettings::default(),
        });
        let mut desktop = DesktopApp::new(app);
        let conversation_id = desktop.app.active_conversation().id;
        desktop.ensure_pane_for_active_conversation();
        desktop
            .conversation_message_counts
            .insert(conversation_id, 0);
        desktop
            .conversation_scroll_offsets
            .insert(conversation_id, RelativeOffset { x: 0.0, y: 1.0 });

        desktop
            .app
            .workspace
            .conversations
            .iter_mut()
            .find(|conversation| conversation.id == conversation_id)
            .expect("conversation")
            .push_message(crate::messaging::MessageSummary {
                peer_hash: "peer".into(),
                peer_label: "Peer".into(),
                title: "hello".into(),
                content: "at present".into(),
                timestamp: 1.0,
                transport_method: crate::messaging::TransportMethod::Direct,
                delivered: true,
                failed: false,
                incoming: true,
                unread: true,
                message_id: Some("message-1".into()),
                fields: Default::default(),
                attachments: Vec::new(),
            });

        let _ = desktop.snap_conversations_with_new_messages_to_bottom();

        assert_eq!(
            desktop.conversation_scroll_offsets.get(&conversation_id),
            Some(&RelativeOffset { x: 0.0, y: 1.0 })
        );
        assert!(!desktop.conversation_is_viewing_history(conversation_id));
    }

    #[test]
    fn conversation_history_notice_requires_meaningful_scrollback() {
        let root = std::env::temp_dir().join(format!(
            "omenbrowser-rs-desktop-conversation-history-notice-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let paths = crate::config::AppPaths::from_root(root);
        paths.ensure().expect("paths");
        let app = App::new(crate::config::AppConfig {
            paths,
            settings: crate::storage::settings::AppSettings::default(),
        });
        let mut desktop = DesktopApp::new(app);
        let conversation_id = desktop.app.active_conversation().id;

        desktop
            .conversation_scroll_offsets
            .insert(conversation_id, RelativeOffset { x: 0.0, y: 0.93 });
        assert!(!desktop.conversation_is_viewing_history(conversation_id));

        desktop
            .conversation_scroll_offsets
            .insert(conversation_id, RelativeOffset { x: 0.0, y: 0.50 });
        assert!(desktop.conversation_is_viewing_history(conversation_id));
    }

    #[test]
    fn hidden_browser_pane_can_be_restored_to_tiled_layout() {
        let root = std::env::temp_dir().join(format!(
            "omenbrowser-rs-desktop-hidden-browser-pane-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let paths = crate::config::AppPaths::from_root(root);
        paths.ensure().expect("paths");
        let app = App::new(crate::config::AppConfig {
            paths,
            settings: crate::storage::settings::AppSettings::default(),
        });
        let mut desktop = DesktopApp::new(app);
        desktop.app.new_browser_tab();
        let hidden_id = desktop.app.active_browser_tab().id;
        desktop.ensure_pane_for_active_browser();
        let pane = desktop
            .find_workspace_pane(&DesktopPane::Browser(hidden_id))
            .expect("browser pane");

        desktop.close_workspace_pane(pane);
        assert!(desktop
            .hidden_browser_panes()
            .iter()
            .any(|(tab_id, _)| *tab_id == hidden_id));

        let _ = desktop.restore_desktop_pane(DesktopPane::Browser(hidden_id));
        assert!(desktop
            .find_workspace_pane(&DesktopPane::Browser(hidden_id))
            .is_some());
        assert!(!desktop
            .hidden_browser_panes()
            .iter()
            .any(|(tab_id, _)| *tab_id == hidden_id));
    }

    #[test]
    fn restored_conversation_pane_starts_at_bottom() {
        let root = std::env::temp_dir().join(format!(
            "omenbrowser-rs-desktop-restore-conversation-bottom-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let paths = crate::config::AppPaths::from_root(root);
        paths.ensure().expect("paths");
        let app = App::new(crate::config::AppConfig {
            paths,
            settings: crate::storage::settings::AppSettings::default(),
        });
        let mut desktop = DesktopApp::new(app);
        let conversation_id = desktop.app.active_conversation().id;
        desktop
            .conversation_scroll_offsets
            .insert(conversation_id, RelativeOffset { x: 0.0, y: 0.25 });

        let _ = desktop.restore_desktop_pane(DesktopPane::Conversation(conversation_id));

        assert_eq!(
            desktop.conversation_scroll_offsets.get(&conversation_id),
            Some(&RelativeOffset { x: 0.0, y: 1.0 })
        );
    }

    #[test]
    fn programmatic_conversation_scroll_restore_does_not_persist_top_callback() {
        let root = std::env::temp_dir().join(format!(
            "omenbrowser-rs-desktop-conversation-scroll-lock-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let paths = crate::config::AppPaths::from_root(root);
        paths.ensure().expect("paths");
        let app = App::new(crate::config::AppConfig {
            paths,
            settings: crate::storage::settings::AppSettings::default(),
        });
        let mut desktop = DesktopApp::new(app);
        let conversation_id = desktop.app.active_conversation().id;
        let saved_offset = RelativeOffset { x: 0.0, y: 0.72 };
        desktop
            .conversation_scroll_offsets
            .insert(conversation_id, saved_offset);

        desktop.schedule_visible_workspace_scroll_restore(2);
        let _ = desktop.update(Message::ConversationScrolled {
            conversation_id,
            offset: RelativeOffset { x: 0.0, y: 0.0 },
        });

        assert_eq!(
            desktop.conversation_scroll_offsets.get(&conversation_id),
            Some(&saved_offset)
        );
    }

    #[test]
    fn hidden_workspace_conversation_scroll_callback_does_not_persist_top_offset() {
        let root = std::env::temp_dir().join(format!(
            "omenbrowser-rs-desktop-conversation-hidden-scroll-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let paths = crate::config::AppPaths::from_root(root);
        paths.ensure().expect("paths");
        let app = App::new(crate::config::AppConfig {
            paths,
            settings: crate::storage::settings::AppSettings::default(),
        });
        let mut desktop = DesktopApp::new(app);
        let conversation_id = desktop.app.active_conversation().id;
        desktop.ensure_pane_for_active_conversation();
        let saved_offset = RelativeOffset { x: 0.0, y: 0.82 };
        desktop
            .conversation_scroll_offsets
            .insert(conversation_id, saved_offset);

        let _ = desktop.update(Message::SwitchSection(WorkspaceSection::Logs));
        let _ = desktop.update(Message::ConversationScrolled {
            conversation_id,
            offset: RelativeOffset { x: 0.0, y: 0.0 },
        });

        assert_eq!(
            desktop.conversation_scroll_offsets.get(&conversation_id),
            Some(&saved_offset)
        );
    }

    #[cfg(feature = "chat-client")]
    #[test]
    fn omenchat_pane_subtitle_does_not_duplicate_room_topic() {
        let root = std::env::temp_dir().join(format!(
            "omenbrowser-rs-desktop-omenchat-subtitle-topic-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let paths = crate::config::AppPaths::from_root(root);
        paths.ensure().expect("paths");
        let app = App::new(crate::config::AppConfig {
            paths,
            settings: crate::storage::settings::AppSettings::default(),
        });
        let mut desktop = DesktopApp::new(app);
        let descriptor = OmenChatDescriptor {
            server_destination: FIXTURE_CHAT_SERVER_HASH.into(),
            display_name: Some("Test OMENchat".into()),
            rooms_hint: vec!["lobby".into()],
            local_display_name: Some("tester".into()),
            ..OmenChatDescriptor::default()
        };
        let session_id = desktop.open_omenchat_status_session(descriptor, "connected".into());
        if let Some(session) = desktop.chat_client.session_mut(session_id) {
            session.active_room.topic = Some("Welcome to OMENchat".into());
        }

        let subtitle = desktop
            .workspace_pane_subtitle(&DesktopPane::OmenChat(session_id))
            .expect("subtitle");

        assert!(subtitle.contains("room: #lobby"));
        assert!(!subtitle.contains("Welcome to OMENchat"));
    }

    #[cfg(feature = "chat-client")]
    #[test]
    fn restored_omenchat_pane_starts_at_bottom() {
        let root = std::env::temp_dir().join(format!(
            "omenbrowser-rs-desktop-restore-omenchat-bottom-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let paths = crate::config::AppPaths::from_root(root);
        paths.ensure().expect("paths");
        let app = App::new(crate::config::AppConfig {
            paths,
            settings: crate::storage::settings::AppSettings::default(),
        });
        let mut desktop = DesktopApp::new(app);
        let descriptor = OmenChatDescriptor {
            server_destination: FIXTURE_CHAT_SERVER_HASH.into(),
            display_name: Some("Test OMENchat".into()),
            rooms_hint: vec!["lobby".into()],
            local_display_name: Some("tester".into()),
            ..OmenChatDescriptor::default()
        };
        let session_id = desktop.open_omenchat_status_session(descriptor, "connected".into());
        desktop
            .chat_scroll_offsets
            .insert((session_id, 1), RelativeOffset { x: 0.0, y: 0.25 });

        let _ = desktop.restore_desktop_pane(DesktopPane::OmenChat(session_id));

        assert_eq!(
            desktop.chat_scroll_offsets.get(&(session_id, 1)),
            Some(&RelativeOffset { x: 0.0, y: 1.0 })
        );
    }

    #[cfg(feature = "chat-client")]
    #[test]
    fn new_omenchat_events_do_not_force_scroll_when_reading_history() {
        let root = std::env::temp_dir().join(format!(
            "omenbrowser-rs-desktop-omenchat-follow-mode-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let paths = crate::config::AppPaths::from_root(root);
        paths.ensure().expect("paths");
        let app = App::new(crate::config::AppConfig {
            paths,
            settings: crate::storage::settings::AppSettings::default(),
        });
        let mut desktop = DesktopApp::new(app);
        let descriptor = OmenChatDescriptor {
            server_destination: FIXTURE_CHAT_SERVER_HASH.into(),
            display_name: Some("Test OMENchat".into()),
            rooms_hint: vec!["lobby".into()],
            local_display_name: Some("tester".into()),
            ..OmenChatDescriptor::default()
        };
        let session_id = desktop.open_omenchat_status_session(descriptor, "connected".into());
        let _ = desktop.restore_desktop_pane(DesktopPane::OmenChat(session_id));
        desktop.chat_event_counts.insert((session_id, 1), 0);
        desktop
            .chat_scroll_offsets
            .insert((session_id, 1), RelativeOffset { x: 0.0, y: 0.36 });
        if let Some(session) = desktop.chat_client.session_mut(session_id) {
            session.events.push(ChatEvent {
                server_id: FIXTURE_CHAT_SERVER_HASH.into(),
                room_id: 1,
                event_id: 1,
                actor_user_id: Some(2),
                actor_display_name: Some("Peer".into()),
                at_unix: 1,
                kind: ChatEventKind::Message {
                    body: "while reading history".into(),
                },
            });
        }

        let _ = desktop.snap_omenchat_with_new_events_to_bottom();

        assert_eq!(
            desktop.chat_scroll_offsets.get(&(session_id, 1)),
            Some(&RelativeOffset { x: 0.0, y: 0.36 })
        );
        assert!(desktop.omenchat_is_viewing_history(session_id, 1));
    }

    #[cfg(feature = "chat-client")]
    #[test]
    fn new_omenchat_events_follow_bottom_when_already_at_present() {
        let root = std::env::temp_dir().join(format!(
            "omenbrowser-rs-desktop-omenchat-follow-bottom-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let paths = crate::config::AppPaths::from_root(root);
        paths.ensure().expect("paths");
        let app = App::new(crate::config::AppConfig {
            paths,
            settings: crate::storage::settings::AppSettings::default(),
        });
        let mut desktop = DesktopApp::new(app);
        let descriptor = OmenChatDescriptor {
            server_destination: FIXTURE_CHAT_SERVER_HASH.into(),
            display_name: Some("Test OMENchat".into()),
            rooms_hint: vec!["lobby".into()],
            local_display_name: Some("tester".into()),
            ..OmenChatDescriptor::default()
        };
        let session_id = desktop.open_omenchat_status_session(descriptor, "connected".into());
        let _ = desktop.restore_desktop_pane(DesktopPane::OmenChat(session_id));
        desktop.chat_event_counts.insert((session_id, 1), 0);
        desktop
            .chat_scroll_offsets
            .insert((session_id, 1), RelativeOffset { x: 0.0, y: 1.0 });
        if let Some(session) = desktop.chat_client.session_mut(session_id) {
            session.events.push(ChatEvent {
                server_id: FIXTURE_CHAT_SERVER_HASH.into(),
                room_id: 1,
                event_id: 1,
                actor_user_id: Some(2),
                actor_display_name: Some("Peer".into()),
                at_unix: 1,
                kind: ChatEventKind::Message {
                    body: "at present".into(),
                },
            });
        }

        let _ = desktop.snap_omenchat_with_new_events_to_bottom();

        assert_eq!(
            desktop.chat_scroll_offsets.get(&(session_id, 1)),
            Some(&RelativeOffset { x: 0.0, y: 1.0 })
        );
        assert!(!desktop.omenchat_is_viewing_history(session_id, 1));
    }

    #[cfg(feature = "chat-client")]
    #[test]
    fn omenchat_history_notice_requires_meaningful_scrollback() {
        let root = std::env::temp_dir().join(format!(
            "omenbrowser-rs-desktop-omenchat-history-notice-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let paths = crate::config::AppPaths::from_root(root);
        paths.ensure().expect("paths");
        let app = App::new(crate::config::AppConfig {
            paths,
            settings: crate::storage::settings::AppSettings::default(),
        });
        let mut desktop = DesktopApp::new(app);
        let descriptor = OmenChatDescriptor {
            server_destination: FIXTURE_CHAT_SERVER_HASH.into(),
            display_name: Some("Test OMENchat".into()),
            rooms_hint: vec!["lobby".into()],
            local_display_name: Some("tester".into()),
            ..OmenChatDescriptor::default()
        };
        let session_id = desktop.open_omenchat_status_session(descriptor, "connected".into());

        desktop
            .chat_scroll_offsets
            .insert((session_id, 1), RelativeOffset { x: 0.0, y: 0.93 });
        assert!(!desktop.omenchat_is_viewing_history(session_id, 1));

        desktop
            .chat_scroll_offsets
            .insert((session_id, 1), RelativeOffset { x: 0.0, y: 0.50 });
        assert!(desktop.omenchat_is_viewing_history(session_id, 1));
    }

    #[cfg(feature = "chat-client")]
    #[test]
    fn programmatic_omenchat_scroll_restore_does_not_persist_top_callback() {
        let root = std::env::temp_dir().join(format!(
            "omenbrowser-rs-desktop-omenchat-scroll-lock-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let paths = crate::config::AppPaths::from_root(root);
        paths.ensure().expect("paths");
        let app = App::new(crate::config::AppConfig {
            paths,
            settings: crate::storage::settings::AppSettings::default(),
        });
        let mut desktop = DesktopApp::new(app);
        let descriptor = OmenChatDescriptor {
            server_destination: FIXTURE_CHAT_SERVER_HASH.into(),
            display_name: Some("Test OMENchat".into()),
            rooms_hint: vec!["lobby".into()],
            local_display_name: Some("tester".into()),
            ..OmenChatDescriptor::default()
        };
        let session_id = desktop.open_omenchat_status_session(descriptor, "connected".into());
        desktop.ensure_pane_for_omenchat(session_id);
        let saved_offset = RelativeOffset { x: 0.0, y: 0.64 };
        desktop
            .chat_scroll_offsets
            .insert((session_id, 1), saved_offset);

        desktop.schedule_visible_workspace_scroll_restore(2);
        let _ = desktop.update(Message::OmenChatScrolled {
            session_id,
            room_id: 1,
            offset: RelativeOffset { x: 0.0, y: 0.0 },
        });

        assert_eq!(
            desktop.chat_scroll_offsets.get(&(session_id, 1)).copied(),
            Some(saved_offset)
        );
    }

    #[cfg(feature = "chat-client")]
    #[test]
    fn hidden_workspace_omenchat_scroll_callback_does_not_persist_top_offset() {
        let root = std::env::temp_dir().join(format!(
            "omenbrowser-rs-desktop-omenchat-hidden-scroll-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let paths = crate::config::AppPaths::from_root(root);
        paths.ensure().expect("paths");
        let app = App::new(crate::config::AppConfig {
            paths,
            settings: crate::storage::settings::AppSettings::default(),
        });
        let mut desktop = DesktopApp::new(app);
        let descriptor = OmenChatDescriptor {
            server_destination: FIXTURE_CHAT_SERVER_HASH.into(),
            display_name: Some("Test OMENchat".into()),
            rooms_hint: vec!["lobby".into()],
            local_display_name: Some("tester".into()),
            ..OmenChatDescriptor::default()
        };
        let session_id = desktop.open_omenchat_status_session(descriptor, "connected".into());
        desktop.ensure_pane_for_omenchat(session_id);
        let saved_offset = RelativeOffset { x: 0.0, y: 0.58 };
        desktop
            .chat_scroll_offsets
            .insert((session_id, 1), saved_offset);

        let _ = desktop.update(Message::SwitchSection(WorkspaceSection::Logs));
        let _ = desktop.update(Message::OmenChatScrolled {
            session_id,
            room_id: 1,
            offset: RelativeOffset { x: 0.0, y: 0.0 },
        });

        assert_eq!(
            desktop.chat_scroll_offsets.get(&(session_id, 1)).copied(),
            Some(saved_offset)
        );
    }

    #[cfg(feature = "chat-client")]
    #[test]
    fn omenchat_upload_picker_cancel_does_not_touch_scroll_restore() {
        let root = std::env::temp_dir().join(format!(
            "omenbrowser-rs-desktop-omenchat-upload-no-scroll-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let paths = crate::config::AppPaths::from_root(root);
        paths.ensure().expect("paths");
        let app = App::new(crate::config::AppConfig {
            paths,
            settings: crate::storage::settings::AppSettings::default(),
        });
        let mut desktop = DesktopApp::new(app);
        let descriptor = OmenChatDescriptor {
            server_destination: FIXTURE_CHAT_SERVER_HASH.into(),
            display_name: Some("Test OMENchat".into()),
            rooms_hint: vec!["lobby".into()],
            local_display_name: Some("tester".into()),
            ..OmenChatDescriptor::default()
        };
        let session_id = desktop.open_omenchat_status_session(descriptor, "connected".into());
        desktop.ensure_pane_for_omenchat(session_id);
        let saved_offset = RelativeOffset { x: 0.0, y: 0.42 };
        desktop
            .chat_scroll_offsets
            .insert((session_id, 1), saved_offset);
        desktop.restore_workspace_scrolls_pending = false;
        desktop.restore_workspace_scroll_locks_release_pending = false;
        desktop.chat_scroll_bottom_locks.clear();

        let _ = desktop.update(Message::OmenChatUploadPicked {
            session_id,
            result: Ok(None),
        });

        assert!(!desktop.restore_workspace_scrolls_pending);
        assert!(!desktop.chat_scroll_bottom_locks.contains(&(session_id, 1)));
        assert_eq!(
            desktop.chat_scroll_offsets.get(&(session_id, 1)).copied(),
            Some(saved_offset)
        );
    }

    #[cfg(feature = "chat-client")]
    #[test]
    fn hidden_omenchat_panes_report_unread_state_for_restore_tabs() {
        let root = std::env::temp_dir().join(format!(
            "omenbrowser-rs-desktop-hidden-omenchat-unread-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let paths = crate::config::AppPaths::from_root(root);
        paths.ensure().expect("paths");
        let app = App::new(crate::config::AppConfig {
            paths,
            settings: crate::storage::settings::AppSettings::default(),
        });
        let mut desktop = DesktopApp::new(app);
        let descriptor = OmenChatDescriptor {
            server_destination: FIXTURE_CHAT_SERVER_HASH.into(),
            display_name: Some("Test OMENchat".into()),
            rooms_hint: vec!["lobby".into()],
            local_display_name: Some("tester".into()),
            ..OmenChatDescriptor::default()
        };
        let session_id = desktop.open_omenchat_status_session(descriptor, "connected".into());
        let _ = desktop.restore_desktop_pane(DesktopPane::OmenChat(session_id));
        let pane = desktop
            .find_workspace_pane(&DesktopPane::OmenChat(session_id))
            .expect("omenchat pane");

        desktop.close_workspace_pane(pane);
        if let Some(session) = desktop.chat_client.session_mut(session_id) {
            session.active_room.unread = 2;
            if let Some(room) = session.rooms.first_mut() {
                room.unread = 2;
            }
        }

        assert!(desktop
            .hidden_omenchat_panes()
            .iter()
            .any(|(hidden_id, label, unread)| {
                *hidden_id == session_id && label == "Test OMENchat" && *unread
            }));
    }

    #[cfg(feature = "chat-client")]
    #[test]
    fn hidden_omenchat_event_marks_restore_tab_unread_until_restored() {
        let root = std::env::temp_dir().join(format!(
            "omenbrowser-rs-desktop-hidden-omenchat-event-unread-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let paths = crate::config::AppPaths::from_root(root);
        paths.ensure().expect("paths");
        let app = App::new(crate::config::AppConfig {
            paths,
            settings: crate::storage::settings::AppSettings::default(),
        });
        let mut desktop = DesktopApp::new(app);
        let descriptor = OmenChatDescriptor {
            server_destination: FIXTURE_CHAT_SERVER_HASH.into(),
            display_name: Some("Test OMENchat".into()),
            rooms_hint: vec!["lobby".into()],
            local_display_name: Some("tester".into()),
            ..OmenChatDescriptor::default()
        };
        let session_id = desktop.open_omenchat_status_session(descriptor, "connected".into());
        let _ = desktop.restore_desktop_pane(DesktopPane::OmenChat(session_id));
        let pane = desktop
            .find_workspace_pane(&DesktopPane::OmenChat(session_id))
            .expect("omenchat pane");
        desktop.close_workspace_pane(pane);

        desktop.apply_omenchat_client_events_status(&[ChatClientEvent::EventAppended {
            session_id,
            event: ChatEvent {
                server_id: FIXTURE_CHAT_SERVER_HASH.into(),
                room_id: 1,
                event_id: 2,
                actor_user_id: Some(2),
                actor_display_name: Some("Peer".into()),
                at_unix: 1,
                kind: ChatEventKind::Message {
                    body: "hello".into(),
                },
            },
        }]);

        assert!(desktop
            .hidden_omenchat_panes()
            .iter()
            .any(|(hidden_id, _, unread)| *hidden_id == session_id && *unread));

        let _ = desktop.restore_desktop_pane(DesktopPane::OmenChat(session_id));
        assert!(!desktop
            .hidden_omenchat_panes()
            .iter()
            .any(|(hidden_id, _, unread)| *hidden_id == session_id && *unread));
        let session = desktop.chat_client.session(session_id).expect("session");
        assert_eq!(session.active_room.unread, 0);
        assert_eq!(session.rooms[0].unread, 0);
    }

    #[cfg(feature = "chat-client")]
    #[test]
    fn hidden_omenchat_inactive_room_event_does_not_double_count_unread() {
        let root = std::env::temp_dir().join(format!(
            "omenbrowser-rs-desktop-hidden-omenchat-inactive-unread-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let paths = crate::config::AppPaths::from_root(root);
        paths.ensure().expect("paths");
        let app = App::new(crate::config::AppConfig {
            paths,
            settings: crate::storage::settings::AppSettings::default(),
        });
        let mut desktop = DesktopApp::new(app);
        let descriptor = OmenChatDescriptor {
            server_destination: FIXTURE_CHAT_SERVER_HASH.into(),
            display_name: Some("Test OMENchat".into()),
            rooms_hint: vec!["lobby".into()],
            local_display_name: Some("tester".into()),
            ..OmenChatDescriptor::default()
        };
        let session_id = desktop.open_omenchat_status_session(descriptor, "connected".into());
        if let Some(session) = desktop.chat_client.session_mut(session_id) {
            session.rooms.push(crate::chat::model::ChatRoomSummary {
                server_id: FIXTURE_CHAT_SERVER_HASH.into(),
                room_id: 2,
                name: "help".into(),
                topic: None,
                unread: 1,
                joined: true,
            });
        }
        let _ = desktop.restore_desktop_pane(DesktopPane::OmenChat(session_id));
        let pane = desktop
            .find_workspace_pane(&DesktopPane::OmenChat(session_id))
            .expect("omenchat pane");
        desktop.close_workspace_pane(pane);

        desktop.apply_omenchat_client_events_status(&[ChatClientEvent::EventAppended {
            session_id,
            event: ChatEvent {
                server_id: FIXTURE_CHAT_SERVER_HASH.into(),
                room_id: 2,
                event_id: 3,
                actor_user_id: Some(2),
                actor_display_name: Some("Peer".into()),
                at_unix: 1,
                kind: ChatEventKind::Message {
                    body: "inactive room hello".into(),
                },
            },
        }]);

        let session = desktop.chat_client.session(session_id).expect("session");
        assert_eq!(session.active_room.unread, 0);
        assert_eq!(
            session
                .rooms
                .iter()
                .find(|room| room.room_id == 2)
                .map(|room| room.unread),
            Some(1)
        );
        assert!(desktop
            .hidden_omenchat_panes()
            .iter()
            .any(|(hidden_id, _, unread)| *hidden_id == session_id && *unread));
    }

    #[cfg(feature = "chat-client")]
    #[test]
    fn omenchat_history_prepended_event_persists_room_history() {
        let root = std::env::temp_dir().join(format!(
            "omenbrowser-rs-desktop-omenchat-history-persist-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let paths = crate::config::AppPaths::from_root(root);
        paths.ensure().expect("paths");
        let app = App::new(crate::config::AppConfig {
            paths: paths.clone(),
            settings: crate::storage::settings::AppSettings::default(),
        });
        let mut desktop = DesktopApp::new(app);
        let server_id = FIXTURE_CHAT_SERVER_HASH;
        let descriptor = OmenChatDescriptor {
            server_destination: server_id.into(),
            display_name: Some("Test OMENchat".into()),
            rooms_hint: vec!["lobby".into()],
            local_display_name: Some("tester".into()),
            ..OmenChatDescriptor::default()
        };
        let session_id = desktop.open_omenchat_status_session(descriptor, "connected".into());
        let event = ChatEvent {
            server_id: server_id.into(),
            room_id: 1,
            event_id: 42,
            actor_user_id: Some(2),
            actor_display_name: Some("Peer".into()),
            at_unix: 1,
            kind: ChatEventKind::Message {
                body: "persisted history".into(),
            },
        };
        desktop
            .chat_client
            .prepend_history_events(session_id, vec![event.clone()]);

        desktop.apply_omenchat_client_events_status(&[ChatClientEvent::HistoryPrepended {
            session_id,
            events: vec![event],
        }]);

        let store = desktop.chat_store.as_ref().expect("store");
        let events = store
            .latest_events(&server_id.into(), 1, 10)
            .expect("latest events");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_id, 42);
        assert_eq!(events[0].actor_display_name.as_deref(), Some("Peer"));

        let app = App::new(crate::config::AppConfig {
            paths,
            settings: desktop.app.settings.clone(),
        });
        let restored = DesktopApp::new(app);
        let session = restored
            .chat_client
            .sessions()
            .iter()
            .find(|session| session.server.server_id == server_id)
            .expect("restored session");
        assert!(session
            .events
            .iter()
            .any(|event| event.event_id == 42
                && matches!(&event.kind, ChatEventKind::Message { body } if body == "persisted history")));
    }

    #[test]
    fn hidden_conversation_panes_report_unread_state_for_restore_tabs() {
        let root = std::env::temp_dir().join(format!(
            "omenbrowser-rs-desktop-hidden-conversation-unread-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let paths = crate::config::AppPaths::from_root(root);
        paths.ensure().expect("paths");
        let app = App::new(crate::config::AppConfig {
            paths,
            settings: crate::storage::settings::AppSettings::default(),
        });
        let mut desktop = DesktopApp::new(app);
        desktop.app.workspace.conversations[0].peer_hash = FIXTURE_LXMF_PEER_HASH.into();
        desktop.app.workspace.conversations[0].peer_label = "Peer".into();
        desktop.app.workspace.conversations[0].push_message(crate::messaging::MessageSummary {
            peer_hash: FIXTURE_LXMF_PEER_HASH.into(),
            peer_label: "Peer".into(),
            title: "hello".into(),
            content: "body".into(),
            timestamp: 1.0,
            transport_method: crate::messaging::TransportMethod::Direct,
            delivered: false,
            failed: false,
            incoming: true,
            unread: true,
            message_id: Some("incoming-1".into()),
            fields: std::collections::BTreeMap::new(),
            attachments: Vec::new(),
        });
        let conversation_id = desktop.app.workspace.conversations[0].id;
        let pane = desktop
            .find_workspace_pane(&DesktopPane::Conversation(conversation_id))
            .expect("conversation pane");
        desktop.close_workspace_pane(pane);

        assert!(desktop
            .hidden_conversation_panes()
            .iter()
            .any(|(hidden_id, label, unread)| {
                *hidden_id == conversation_id && label == "Peer" && *unread
            }));
    }

    #[test]
    fn hidden_active_conversation_runtime_message_updates_unread_status() {
        let root = std::env::temp_dir().join(format!(
            "omenbrowser-rs-desktop-hidden-active-conversation-runtime-unread-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let paths = crate::config::AppPaths::from_root(root);
        paths.ensure().expect("paths");
        let app = App::new(crate::config::AppConfig {
            paths,
            settings: crate::storage::settings::AppSettings::default(),
        });
        let mut desktop = DesktopApp::new(app);
        desktop.app.workspace.active_section = WorkspaceSection::Messages;
        desktop.app.workspace.conversations[0].peer_hash = FIXTURE_LXMF_PEER_HASH.into();
        desktop.app.workspace.conversations[0].peer_label = "Peer".into();
        desktop.app.workspace.conversations[0].thread.peer_hash = FIXTURE_LXMF_PEER_HASH.into();
        desktop.app.workspace.conversations[0].thread.peer_label = "Peer".into();
        let conversation_id = desktop.app.workspace.conversations[0].id;
        let pane = desktop
            .find_workspace_pane(&DesktopPane::Conversation(conversation_id))
            .expect("conversation pane");
        desktop.close_workspace_pane(pane);
        assert!(!desktop.active_conversation_pane_is_visible());

        assert!(desktop.app.enqueue_runtime_event(
            crate::runtime::RuntimeBusEvent::MessageReceived(crate::messaging::MessageSummary {
                peer_hash: FIXTURE_LXMF_PEER_HASH.into(),
                peer_label: "Peer".into(),
                title: "hidden hello".into(),
                content: "message while minimized".into(),
                timestamp: 1.0,
                transport_method: crate::messaging::TransportMethod::Direct,
                delivered: true,
                failed: false,
                incoming: true,
                unread: true,
                message_id: Some("hidden-active-inbound-1".into()),
                fields: Default::default(),
                attachments: Vec::new(),
            })
        ));
        let active_conversation_readable = desktop.active_conversation_pane_is_visible();
        assert_eq!(
            desktop
                .app
                .drain_internal_events_with_active_conversation_readable(
                    active_conversation_readable
                ),
            1
        );

        let conversation = desktop
            .app
            .workspace
            .conversations
            .iter()
            .find(|conversation| conversation.id == conversation_id)
            .expect("conversation");
        assert_eq!(conversation.thread.unread_count, 1);
        assert_eq!(desktop.footer_lxmf_unread_counts(), (0, 1));
        assert!(desktop
            .hidden_conversation_panes()
            .iter()
            .any(|(hidden_id, label, unread)| {
                *hidden_id == conversation_id && label == "Peer" && *unread
            }));
    }

    #[cfg(feature = "chat-client")]
    #[test]
    fn close_omenchat_session_deletes_plugin_store_rows() {
        let root = std::env::temp_dir().join(format!(
            "omenbrowser-rs-desktop-delete-omenchat-store-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let paths = crate::config::AppPaths::from_root(root);
        paths.ensure().expect("paths");
        let app = App::new(crate::config::AppConfig {
            paths: paths.clone(),
            settings: crate::storage::settings::AppSettings::default(),
        });
        let mut desktop = DesktopApp::new(app);
        let descriptor = OmenChatDescriptor {
            server_destination: "mockchatdestination".into(),
            display_name: Some("Mock OMENchat".into()),
            rooms_hint: vec!["lobby".into()],
            local_display_name: Some("tester".into()),
            ..OmenChatDescriptor::default()
        };
        let session_id = desktop.open_omenchat_status_session(descriptor, "connected".into());
        let server_id = desktop
            .chat_client
            .session(session_id)
            .expect("session")
            .server
            .server_id
            .clone();
        assert!(desktop
            .chat_store
            .as_ref()
            .expect("store")
            .saved_servers()
            .expect("servers")
            .iter()
            .any(|server| server.server_id == server_id));

        desktop.close_omenchat_session(session_id);
        desktop.remove_workspace_panes_for_missing_targets(None, None);
        desktop.persist_workspace_panes("workspace panes");

        assert!(desktop
            .chat_store
            .as_ref()
            .expect("store")
            .saved_servers()
            .expect("servers")
            .is_empty());

        let app = App::new(crate::config::AppConfig {
            paths,
            settings: desktop.app.settings.clone(),
        });
        let restored = DesktopApp::new(app);
        assert!(restored.chat_client.sessions().is_empty());
        assert!(!restored
            .workspace_panes
            .iter()
            .any(|(_, pane)| matches!(pane, DesktopPane::OmenChat(_))));
    }

    #[cfg(feature = "chat-client-rns")]
    #[tokio::test]
    async fn close_omenchat_session_clears_live_transport_and_retry_state() {
        let root = std::env::temp_dir().join(format!(
            "omenbrowser-rs-desktop-close-omenchat-live-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let paths = crate::config::AppPaths::from_root(root);
        paths.ensure().expect("paths");
        let app = App::new(crate::config::AppConfig {
            paths,
            settings: crate::storage::settings::AppSettings::default(),
        });
        let mut desktop = DesktopApp::new(app);
        let descriptor = OmenChatDescriptor {
            server_destination: FIXTURE_CHAT_SERVER_HASH.into(),
            display_name: Some("Live OMENchat".into()),
            rooms_hint: vec!["lobby".into()],
            local_display_name: Some("tester".into()),
            ..OmenChatDescriptor::default()
        };
        let session_id = desktop.open_omenchat_status_session(descriptor, "connected".into());
        let link_id = [0x42; 16];
        desktop.omenchat_live_opening.insert(session_id);
        desktop.omenchat_live_retry_after.insert(session_id, 123);
        desktop.omenchat_live_retry_count.insert(session_id, 2);
        desktop
            .omenchat_live_reconnect_generation
            .insert(session_id, 4);
        desktop.omenchat_link_sessions.insert(link_id, session_id);
        desktop.omenchat_live_transports.insert(
            session_id,
            DesktopOmenChatTransport::new(link_id, current_epoch_ms()),
        );

        desktop.close_omenchat_session(session_id);

        assert!(desktop.chat_client.session(session_id).is_none());
        assert!(!desktop.omenchat_live_opening.contains(&session_id));
        assert!(!desktop.omenchat_live_retry_after.contains_key(&session_id));
        assert!(!desktop.omenchat_live_retry_count.contains_key(&session_id));
        assert!(!desktop
            .omenchat_live_reconnect_generation
            .contains_key(&session_id));
        assert!(!desktop.omenchat_live_transports.contains_key(&session_id));
        assert!(!desktop.omenchat_link_sessions.contains_key(&link_id));
    }

    #[test]
    fn browser_request_state_line_summarizes_forwarded_form_data_without_values() {
        let root = std::env::temp_dir().join(format!(
            "omenbrowser-rs-desktop-request-preview-line-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let paths = crate::config::AppPaths::from_root(root);
        paths.ensure().expect("paths");
        let mut app = App::new(crate::config::AppConfig {
            paths,
            settings: crate::storage::settings::AppSettings::default(),
        });
        let preview = crate::app::BrowserRequestPreview {
            target: "mock.node:/submit.mu".into(),
            fields: vec!["nickname".into(), "x=1".into()],
            request_data: std::collections::BTreeMap::from([
                ("field_nickname".into(), "mesh friend".into()),
                ("var_x".into(), "1".into()),
            ]),
            status: BrowserRequestStatus::Pending,
            detail: "request queued".into(),
        };
        app.active_browser_tab_mut().request_preview = Some(preview);
        let tab = app.active_browser_tab();
        let preview = tab.request_preview.as_ref().expect("preview");

        let line = request_preview_line(tab, preview);

        assert_eq!(request_status_label(&preview.status), "pending");
        assert!(line.contains("captured submission"));
        assert!(line.contains("1 field(s)"));
        assert!(line.contains("1 variable(s)"));
        assert!(!line.contains("mesh friend"));
        assert!(!line.contains("field_nickname"));
    }

    #[test]
    fn identity_hash_copy_action_reports_status() {
        let root = std::env::temp_dir().join(format!(
            "omenbrowser-rs-desktop-copy-identity-hash-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let paths = crate::config::AppPaths::from_root(root);
        paths.ensure().expect("paths");
        let app = App::new(crate::config::AppConfig {
            paths,
            settings: crate::storage::settings::AppSettings::default(),
        });
        let mut desktop = DesktopApp::new(app);

        let _ = desktop.update(Message::CopyActiveIdentityHash);
        assert_eq!(desktop.app.status.task, "no active identity hash to copy");

        desktop.app.runtime_status.active_identity = Some(crate::identity::IdentityProfile {
            label: "tester".into(),
            path: desktop.app.paths.root.join("identity"),
            hash_hex: "0123456789abcdef0123456789abcdef".into(),
            managed: true,
        });
        let _ = desktop.update(Message::CopyActiveIdentityHash);
        assert_eq!(
            desktop.app.status.task,
            "copied active identity hash to clipboard"
        );
    }

    #[test]
    fn browser_request_preview_path_actions_follow_retry_state() {
        let root = std::env::temp_dir().join(format!(
            "omenbrowser-rs-desktop-request-preview-path-actions-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let paths = crate::config::AppPaths::from_root(root);
        paths.ensure().expect("paths");
        let mut app = App::new(crate::config::AppConfig {
            paths,
            settings: crate::storage::settings::AppSettings::default(),
        });
        let target = fixture_browser_node_url();
        app.active_browser_tab_mut().request_preview = Some(crate::app::BrowserRequestPreview {
            target: target.clone(),
            fields: Vec::new(),
            request_data: std::collections::BTreeMap::new(),
            status: BrowserRequestStatus::Pending,
            detail: "requesting path before page load".into(),
        });
        let tab = app.active_browser_tab();
        let preview = tab.request_preview.as_ref().expect("preview");
        assert!(!browser_request_preview_has_path_actions(&tab, preview));

        app.active_browser_tab_mut()
            .request_preview
            .as_mut()
            .expect("preview")
            .status = BrowserRequestStatus::Failed;
        let tab = app.active_browser_tab();
        let preview = tab.request_preview.as_ref().expect("preview");
        assert!(!browser_request_preview_has_path_actions(&tab, preview));
        app.active_browser_tab_mut()
            .request_preview
            .as_mut()
            .expect("preview")
            .status = BrowserRequestStatus::Pending;

        app.active_browser_tab_mut().retry_state = Some(crate::app::BrowserRetryState {
            target: target.clone(),
            destination_hash: FIXTURE_BROWSER_NODE_HASH.into(),
            reason: "browser navigation path request queued; auto-load when path is known".into(),
            requested_epoch_ms: current_epoch_ms(),
            retry_after_epoch_ms: current_epoch_ms().saturating_add(5_000),
            ready_epoch_ms: None,
            attempts: 0,
        });
        let tab = app.active_browser_tab();
        let preview = tab.request_preview.as_ref().expect("preview");
        assert!(browser_request_preview_has_path_actions(&tab, preview));
        assert!(!browser_request_preview_retry_ready(&tab, preview));

        app.active_browser_tab_mut()
            .request_preview
            .as_mut()
            .expect("preview")
            .status = BrowserRequestStatus::Failed;
        let tab = app.active_browser_tab();
        let preview = tab.request_preview.as_ref().expect("preview");
        assert!(browser_request_preview_has_path_actions(&tab, preview));
        assert!(!browser_request_preview_retry_ready(&tab, preview));
        app.active_browser_tab_mut()
            .request_preview
            .as_mut()
            .expect("preview")
            .status = BrowserRequestStatus::Pending;

        app.active_browser_tab_mut()
            .retry_state
            .as_mut()
            .expect("retry")
            .ready_epoch_ms = Some(current_epoch_ms());
        app.active_browser_tab_mut()
            .retry_state
            .as_mut()
            .expect("retry")
            .retry_after_epoch_ms = current_epoch_ms();
        let tab = app.active_browser_tab();
        let preview = tab.request_preview.as_ref().expect("preview");
        assert!(browser_request_preview_has_path_actions(&tab, preview));
        assert!(browser_request_preview_retry_ready(&tab, preview));

        app.active_browser_tab_mut()
            .retry_state
            .as_mut()
            .expect("retry")
            .reason = "browser path request passed; waiting briefly before page load".into();
        let tab = app.active_browser_tab();
        let preview = tab.request_preview.as_ref().expect("preview");
        assert!(browser_request_preview_has_path_actions(&tab, preview));
    }

    #[test]
    fn keyboard_shortcuts_map_browser_tabs_and_scroll() {
        assert!(matches!(
            map_key_press(
                keyboard::Key::Character("t".into()),
                keyboard::Modifiers::COMMAND
            ),
            Some(Message::NewBrowserTab)
        ));
        assert!(matches!(
            map_key_press(
                keyboard::Key::Named(Named::PageDown),
                keyboard::Modifiers::empty()
            ),
            Some(Message::ScrollBrowserPage { direction: 1 })
        ));
    }

    #[test]
    fn keyboard_shortcuts_map_browser_live_diagnostics_actions() {
        assert!(matches!(
            map_key_press(
                keyboard::Key::Character("r".into()),
                keyboard::Modifiers::COMMAND
            ),
            Some(Message::ReloadBrowser)
        ));
        assert!(map_key_press(
            keyboard::Key::Character("n".into()),
            keyboard::Modifiers::empty()
        )
        .is_none());
        assert!(matches!(
            map_key_press(
                keyboard::Key::Character("x".into()),
                keyboard::Modifiers::COMMAND
            ),
            Some(Message::LiveProbe)
        ));
        assert!(matches!(
            map_key_press(
                keyboard::Key::Character("p".into()),
                keyboard::Modifiers::COMMAND
            ),
            Some(Message::PathDiagnostics)
        ));
    }

    #[test]
    fn desktop_theme_names_map_to_usable_iced_themes() {
        assert_eq!(theme_from_name("default"), Theme::Dark);
        assert_eq!(
            theme_from_name("omen").palette().primary,
            Color::from_rgb8(156, 28, 36)
        );
        assert_eq!(theme_from_name("kanagawa"), Theme::KanagawaDragon);
        assert_eq!(theme_from_name("solarized_dark"), Theme::SolarizedDark);
        assert_eq!(theme_from_name("unknown"), Theme::Dark);
        assert!(DESKTOP_THEME_CHOICES.contains(&"default"));
        assert!(DESKTOP_THEME_CHOICES.contains(&"omen"));
    }

    #[test]
    fn fontconfig_family_selection_prefers_nerd_font_alias() {
        assert_eq!(
            select_nerd_font_family_from_fc_match("Iosevka,Iosevka Nerd Font\n"),
            Some("Iosevka Nerd Font".into())
        );
        assert_eq!(
            select_nerd_font_family_from_fc_match("MesloLGS Nerd Font Mono\n"),
            Some("MesloLGS Nerd Font Mono".into())
        );
        assert_eq!(
            select_nerd_font_family_from_fc_match("Noto Sans Mono\n"),
            Some("Noto Sans Mono".into())
        );
    }

    #[test]
    fn desktop_ui_size_scales_from_user_font_preference() {
        assert_eq!(scaled_ui_size(16, 10), 10);
        assert_eq!(scaled_ui_size(28, 10), 18);
        assert_eq!(scaled_ui_size(16, 24), 24);
        assert_eq!(scaled_ui_size(12, 24), 18);
    }

    #[test]
    fn emoji_detection_covers_common_title_symbols() {
        assert!(is_emoji_like('🐈'));
        assert!(is_emoji_like('☠'));
        assert!(!is_emoji_like('C'));
    }

    #[test]
    fn monitoring_helpers_format_resource_bytes() {
        assert_eq!(human_bytes(512), "512 B");
        assert_eq!(human_bytes(1536), "1.5 KiB");
        assert_eq!(human_bytes(2 * 1024 * 1024), "2.0 MiB");
        assert!(WorkspaceSection::ALL.contains(&WorkspaceSection::Monitoring));
        assert!(WorkspaceSection::ALL.contains(&WorkspaceSection::Help));
    }

    #[test]
    fn runtime_interface_sampling_runs_for_interfaces_and_monitoring() {
        assert!(section_needs_runtime_interface_sample(
            WorkspaceSection::Interfaces
        ));
        assert!(section_needs_runtime_interface_sample(
            WorkspaceSection::Monitoring
        ));
        assert!(!section_needs_runtime_interface_sample(
            WorkspaceSection::Browser
        ));
        assert!(!section_needs_runtime_interface_sample(
            WorkspaceSection::Logs
        ));
    }

    #[test]
    fn monitoring_runtime_attribution_lines_group_runtime_traffic() {
        let monitoring = crate::app::MonitoringPanelState {
            runtime_events_total: 120,
            outbound_page_requests: 3,
            outbound_partial_refreshes: 2,
            outbound_file_downloads: 1,
            outbound_path_requests: 4,
            outbound_path_warmups: 5,
            outbound_lxmf_sends: 6,
            outbound_propagation_syncs: 7,
            outbound_diagnostics: 8,
            outbound_status_updates: 9,
            inbound_page_responses: 10,
            inbound_downloads: 11,
            announces_received: 12,
            path_updates_received: 13,
            inbound_messages: 14,
            lxmf_evidence_updates: 15,
            propagation_sync_events: 16,
            estimated_outbound_bytes: 2048,
            estimated_inbound_bytes: 4096,
            ..crate::app::MonitoringPanelState::default()
        };

        let lines = monitoring_runtime_attribution_lines(&monitoring, 120);

        assert!(lines[0].contains("browser spikes"));
        assert!(lines[1].contains("browser=6 path=9 lxmf=13 app/status=17"));
        assert!(lines[2].contains("pages/files=21 discovery=25 lxmf=45"));
        assert!(lines[3].contains("top tx=app/status (17)"));
        assert!(lines[3].contains("top rx=lxmf (45)"));
        assert!(lines[3].contains("22 tx/min"));
        assert!(lines[3].contains("46 rx/min"));
        assert!(lines[4].contains("22.5 tx/min"));
        assert!(lines[4].contains("45.5 rx/min"));
        assert!(lines[4].contains("60.0 runtime events/min"));
        assert!(lines[5].contains("2.0 KiB tx / 4.0 KiB rx"));
        assert!(lines[5].contains("1.0 KiB tx/min / 2.0 KiB rx/min"));
        assert!(lines
            .iter()
            .any(|line| line.contains("browser health: 4 page/download request(s)")));
        assert!(lines.iter().any(|line| {
            line.contains("LXMF health: sends=6 propagation_syncs=7 evidence=15 inbound=14")
        }));
    }

    #[test]
    fn monitoring_runtime_attribution_lines_explain_idle_state() {
        let lines =
            monitoring_runtime_attribution_lines(&crate::app::MonitoringPanelState::default(), 60);

        assert!(lines
            .iter()
            .any(|line| line.contains("activity: idle; no runtime traffic recorded yet")));
    }

    #[test]
    fn interface_runtime_status_label_reports_sample_visibility() {
        let mut profile =
            crate::interfaces::ReticulumInterfaceProfile::tcp_client("iface_test", "GatewayOne");
        profile.target_host = "10.0.0.7".into();

        assert!(interface_runtime_status_label(&profile, None).contains("disconnected"));
        assert_eq!(
            interface_runtime_state_line(&profile, None),
            "state: disconnected | endpoint: 10.0.0.7:4242"
        );

        profile.enabled = false;
        let running_stats = crate::runtime::InterfaceStats {
            available: true,
            reason: Some("sampled".into()),
            interfaces: vec!["GatewayOne [TcpClient supported enabled]".into()],
            samples: Vec::new(),
        };
        assert!(
            interface_runtime_status_label(&profile, Some(&running_stats))
                .contains("disabled by profile")
        );
        assert_eq!(
            interface_runtime_state_line(&profile, Some(&running_stats)),
            "state: disabled | endpoint: 10.0.0.7:4242"
        );

        profile.enabled = true;
        let configured = interface_runtime_status_label(&profile, Some(&running_stats));
        assert!(configured.contains("disconnected"));
        assert_eq!(
            interface_runtime_detail_line(&profile, Some(&running_stats)).as_deref(),
            Some("runtime detail: GatewayOne [TcpClient supported enabled]")
        );
        assert_eq!(
            interface_runtime_state_line(&profile, Some(&running_stats)),
            "state: disconnected | endpoint: 10.0.0.7:4242"
        );

        let attached_stats = crate::runtime::InterfaceStats {
            available: true,
            reason: Some("sampled".into()),
            interfaces: vec![
                "GatewayOne [TcpClient supported enabled]".into(),
                "attached GatewayOne tcp_client 10.0.0.7:4242 ifac=none".into(),
            ],
            samples: Vec::new(),
        };
        let attached = interface_runtime_status_label(&profile, Some(&attached_stats));
        assert_eq!(attached, "runtime: connected");
        assert_eq!(
            interface_runtime_detail_line(&profile, Some(&attached_stats)).as_deref(),
            Some("runtime detail: attached GatewayOne tcp_client 10.0.0.7:4242 ifac=none")
        );
        assert_eq!(
            interface_runtime_state_line(&profile, Some(&attached_stats)),
            "state: connected; auto-retry enabled | endpoint: 10.0.0.7:4242"
        );

        let structured_attached = crate::runtime::InterfaceStats {
            available: true,
            reason: Some("sampled".into()),
            interfaces: Vec::new(),
            samples: vec![crate::runtime::network::InterfaceSample {
                profile_id: profile.profile_id.clone(),
                name: "GatewayOne".into(),
                kind: "tcp_client".into(),
                state: crate::runtime::network::InterfaceSampleState::Attached,
                enabled: true,
                supported: true,
                attached: true,
                endpoint: Some("10.0.0.7:4242".into()),
                detail: Some("GatewayOne tcp_client 10.0.0.7:4242 ifac=none".into()),
            }],
        };
        let attached = interface_runtime_status_label(&profile, Some(&structured_attached));
        assert_eq!(attached, "runtime: connected");
        assert_eq!(
            interface_runtime_detail_line(&profile, Some(&structured_attached)).as_deref(),
            Some("runtime detail: GatewayOne tcp_client 10.0.0.7:4242 ifac=none")
        );
        assert_eq!(
            interface_runtime_state_line(&profile, Some(&structured_attached)),
            "state: connected; auto-retry enabled | endpoint: 10.0.0.7:4242"
        );

        let missing_stats = crate::runtime::InterfaceStats {
            available: true,
            reason: Some("sampled".into()),
            interfaces: vec!["OtherGateway [TcpClient supported enabled]".into()],
            samples: Vec::new(),
        };
        assert!(
            interface_runtime_status_label(&profile, Some(&missing_stats)).contains("disconnected")
        );

        let stopped_stats = crate::runtime::InterfaceStats {
            available: false,
            reason: Some("runtime stopped".into()),
            interfaces: Vec::new(),
            samples: Vec::new(),
        };
        assert!(
            interface_runtime_status_label(&profile, Some(&stopped_stats)).contains("not running")
        );
    }

    #[test]
    fn monitoring_interface_reconnect_line_summarizes_native_samples() {
        assert!(monitoring_interface_reconnect_line(None).contains("waiting"));

        let unavailable = crate::runtime::InterfaceStats {
            available: false,
            reason: Some("runtime stopped".into()),
            interfaces: Vec::new(),
            samples: Vec::new(),
        };
        assert!(monitoring_interface_reconnect_line(Some(&unavailable)).contains("unavailable"));

        let no_interfaces = crate::runtime::InterfaceStats {
            available: true,
            reason: Some("sampled".into()),
            interfaces: Vec::new(),
            samples: Vec::new(),
        };
        assert!(monitoring_interface_reconnect_line(Some(&no_interfaces)).contains("no interfaces"));

        let connected = crate::runtime::InterfaceStats {
            available: true,
            reason: Some("sampled".into()),
            interfaces: vec!["Gateway [1] TCPClientInterface | connected=true".into()],
            samples: Vec::new(),
        };
        assert!(monitoring_interface_reconnect_line(Some(&connected)).contains("connected"));

        let retrying = crate::runtime::InterfaceStats {
            available: true,
            reason: Some("sampled".into()),
            interfaces: vec!["Gateway [1] TCPClientInterface | connected=false".into()],
            samples: Vec::new(),
        };
        assert!(monitoring_interface_reconnect_line(Some(&retrying)).contains("retrying"));
    }

    #[test]
    fn monitoring_interface_status_lines_prefer_structured_samples() {
        let stats = crate::runtime::InterfaceStats {
            available: true,
            reason: Some("sampled".into()),
            interfaces: vec!["legacy raw line".into()],
            samples: vec![
                crate::runtime::network::InterfaceSample {
                    profile_id: "gw1".into(),
                    name: "GatewayOne".into(),
                    kind: "tcp_client".into(),
                    state: crate::runtime::network::InterfaceSampleState::Attached,
                    enabled: true,
                    supported: true,
                    attached: true,
                    endpoint: Some("10.0.0.7:4242".into()),
                    detail: Some("GatewayOne tcp_client 10.0.0.7:4242 ifac=none".into()),
                },
                crate::runtime::network::InterfaceSample {
                    profile_id: "i2p".into(),
                    name: "I2P".into(),
                    kind: "i2p".into(),
                    state: crate::runtime::network::InterfaceSampleState::Unsupported,
                    enabled: true,
                    supported: false,
                    attached: false,
                    endpoint: None,
                    detail: Some("native interface startup is not implemented".into()),
                },
            ],
        };

        let lines = monitoring_interface_status_lines(&stats);

        assert!(lines.iter().any(|line| line.contains("runtime: available")));
        assert!(lines.iter().any(|line| line
            .contains("GatewayOne | tcp_client | connected; auto-retry enabled | 10.0.0.7:4242")));
        assert!(lines
            .iter()
            .any(|line| line.contains("I2P | i2p | unsupported | no endpoint")));
        assert!(!lines.iter().any(|line| line.contains("legacy raw line")));
    }

    #[test]
    fn monitoring_runtime_attribution_lines_surface_runtime_errors() {
        let monitoring = crate::app::MonitoringPanelState {
            runtime_errors: 2,
            outbound_page_requests: 1,
            ..crate::app::MonitoringPanelState::default()
        };

        let lines = monitoring_runtime_attribution_lines(&monitoring, 60);

        assert!(lines.iter().any(|line| {
            line.contains("attention: 2 runtime error(s)")
                && line.contains("Logs")
                && line.contains("Diagnostics")
        }));
    }

    #[test]
    fn monitoring_runtime_attribution_lines_flag_browser_delivery_gaps() {
        let monitoring = crate::app::MonitoringPanelState {
            outbound_page_requests: 2,
            outbound_path_requests: 5,
            ..crate::app::MonitoringPanelState::default()
        };

        let lines = monitoring_runtime_attribution_lines(&monitoring, 60);

        assert!(lines.iter().any(|line| {
            line.contains("attention: browser requests have no page/file responses yet")
                && line.contains("Request Path")
                && line.contains("Diagnostics")
        }));
    }

    #[test]
    fn monitoring_runtime_attribution_lines_flag_lxmf_evidence_gaps() {
        let monitoring = crate::app::MonitoringPanelState {
            outbound_lxmf_sends: 3,
            lxmf_evidence_updates: 0,
            ..crate::app::MonitoringPanelState::default()
        };

        let lines = monitoring_runtime_attribution_lines(&monitoring, 60);

        assert!(lines.iter().any(|line| {
            line.contains("attention: LXMF sends have no delivery evidence yet")
                && line.contains("selected peer/path")
        }));
    }

    #[cfg(feature = "chat-client")]
    #[test]
    fn omenchat_upload_file_limit_rejects_oversized_local_files() {
        assert_eq!(
            omenchat_upload_policy_rejection(512, Some(50 * 1024 * 1024), None),
            None
        );
        assert_eq!(
            omenchat_upload_policy_rejection(512, Some(50 * 1024 * 1024), Some(512)),
            None
        );
        assert_eq!(
            omenchat_upload_policy_rejection(1, Some(0), Some(512)),
            Some("upload blocked: server has uploads disabled".into())
        );
        assert_eq!(
            omenchat_upload_policy_rejection(1024, Some(50 * 1024 * 1024), Some(512)),
            Some("upload blocked: 1.0 KiB exceeds server file limit 512 B".into())
        );
    }

    #[cfg(feature = "chat-client")]
    #[test]
    fn omenchat_help_documents_alpha_isolation_and_server_storage() {
        let alpha_help = OMENCHAT_ALPHA_TEST_HELP_LINES.join("\n");
        assert!(alpha_help.contains("isolated app root"));
        assert!(alpha_help.contains("--desktop --app-root /tmp/omenbrowser-rs-alpha"));
        assert!(alpha_help.contains("--desktop --app-root /tmp/omenbrowser-rs-alpha-2"));
        assert!(alpha_help.contains("instance_name suffix"));
        assert!(alpha_help.contains("omenchat://<destination hash>"));

        let server_help = OMENCHATD_OPERATOR_HELP_LINES.join("\n");
        assert!(server_help.contains("~/.omenchatd"));
        assert!(server_help.contains("reticulum/storage/pages/index.mu"));
        assert!(server_help.contains("omenchat.node"));
        assert!(server_help.contains("nomadnetwork.node"));
        assert!(server_help.contains("--features live-rns-net -- run"));
        assert!(server_help.contains("--features live-rns-net -- tui"));

        let history_help = OMENCHAT_HISTORY_HELP_LINES.join("\n");
        assert!(history_help.contains("bounded recent room history"));
        assert!(history_help.contains("Load Older"));
        assert!(history_help.contains("server event id"));
        assert!(history_help.contains("HistoryRecent/HistoryBefore"));

        let media_help = OMENCHAT_MEDIA_HELP_LINES.join("\n");
        assert!(media_help.contains("animated GIFs"));
        assert!(media_help.contains("127.0.0.1:9050"));
        assert!(media_help.contains("127.0.0.1:9150"));
        assert!(media_help.contains("512 KiB max per file"));
        assert!(media_help.contains("native file picker"));
    }

    #[test]
    fn lxmf_help_documents_native_ticket_and_receipt_limits() {
        let help = LXMF_HELP_LINES.join("\n");

        assert!(help.contains("direct or propagated"));
        assert!(help.contains("ticket/stamp sending is not implemented"));
        assert!(help.contains("disabled/downgraded before native sends"));
        assert!(help.contains("not the same as a guaranteed peer-side LXMF receipt"));
    }

    #[test]
    fn selected_message_details_card_renders_for_selected_message() {
        let mut conversation = crate::messaging::Conversation::new(1, "peer", "Peer");
        let message = crate::messaging::MessageSummary {
            peer_hash: "peer".into(),
            peer_label: "Peer".into(),
            title: "Subject".into(),
            content: "Body".into(),
            timestamp: 1.0,
            transport_method: crate::messaging::TransportMethod::Direct,
            delivered: false,
            failed: true,
            incoming: false,
            unread: false,
            message_id: Some("packet-1".into()),
            fields: std::collections::BTreeMap::from([(
                "native_lxmf_state".into(),
                "failed".into(),
            )]),
            attachments: Vec::new(),
        };
        conversation.selected_message_key = Some(message_summary_key(&message));
        conversation.push_message(message);

        let _details = selected_message_details_card(conversation.id, &conversation);
    }

    #[test]
    fn diagnostics_summary_extracts_classification_next_step() {
        let lines = serde_json::to_string_pretty(&serde_json::json!({
            "report": "native_network_smoke_test",
            "classification": {
                "outcome": "blocked",
                "stage": "destination_identity",
                "detail": "destination identity is not known",
                "next_step": "preload known_destinations"
            }
        }))
        .expect("json")
        .lines()
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();

        let summary = diagnostics_preview_report_summary(&lines).expect("summary");
        assert_eq!(summary.report, "native_network_smoke_test");
        assert_eq!(summary.outcome, "blocked");
        assert_eq!(summary.stage, "destination_identity");
        assert_eq!(summary.next_step, "preload known_destinations");
    }

    #[test]
    fn diagnostics_summary_extracts_page_probe_failure_stage() {
        let lines = serde_json::to_string_pretty(&serde_json::json!({
            "url": fixture_browser_node_url(),
            "ready_to_request": false,
            "steps": [
                {"stage": "address_parse", "ok": true, "detail": "parsed"},
                {"stage": "path_discovery", "ok": false, "detail": "path unknown"}
            ]
        }))
        .expect("json")
        .lines()
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();

        let summary = diagnostics_preview_report_summary(&lines).expect("summary");
        assert_eq!(summary.outcome, "blocked");
        assert_eq!(summary.stage, "path_discovery");
        assert_eq!(summary.detail, "path unknown");
    }

    #[test]
    fn diagnostics_live_fetch_card_extracts_success_metadata() {
        let lines = serde_json::to_string_pretty(&serde_json::json!({
            "report": "native_network_smoke_test",
            "classification": {
                "outcome": "pass",
                "stage": "live_fetch",
                "next_step": "open browser"
            },
            "live_fetch": {
                "ok": true,
                "stage_hint": "response_decode",
                "url": fixture_browser_node_url(),
                "title": "Node Home",
                "markup_bytes": 128,
                "markup_lines": 6,
                "metadata": {
                    "native_request_backend": "rns-net"
                }
            }
        }))
        .expect("json")
        .lines()
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();

        let card = diagnostics_preview_live_fetch_card(&lines).expect("live fetch card");
        assert_eq!(card.outcome, "pass");
        assert_eq!(card.stage_hint, "response_decode");
        assert_eq!(card.request_backend, "rns-net");
        assert_eq!(card.response_size, "128 bytes, 6 lines");
        assert_eq!(card.first_failed_stage, "live_fetch");
    }

    #[test]
    fn diagnostics_live_fetch_card_extracts_failed_probe_stage() {
        let lines = serde_json::to_string_pretty(&serde_json::json!({
            "report": "native_network_smoke_test",
            "classification": {
                "outcome": "blocked",
                "stage": "path_discovery",
                "next_step": "warm path"
            },
            "live_page_probe": {
                "ok": true,
                "report": {
                    "steps": [
                        {"stage": "address_parse", "ok": true, "detail": "parsed"},
                        {"stage": "path_discovery", "ok": false, "detail": "queued request_path"}
                    ]
                }
            },
            "live_fetch": {
                "ok": false,
                "status": "blocked",
                "error": "live fetch preflight did not report ready_to_request",
                "stage_hint": "path_discovery"
            }
        }))
        .expect("json")
        .lines()
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();

        let card = diagnostics_preview_live_fetch_card(&lines).expect("live fetch card");
        assert_eq!(card.outcome, "blocked");
        assert_eq!(card.request_backend, "not reached");
        assert_eq!(card.response_size, "no response body");
        assert_eq!(card.first_failed_stage, "path_discovery");
        assert_eq!(card.next_step, "warm path");
    }

    #[test]
    fn diagnostics_lxmf_delivery_card_extracts_proof_and_inbound_evidence() {
        let lines = serde_json::to_string_pretty(&serde_json::json!({
            "report": "native_lxmf_live_interop",
            "classification": {
                "outcome": "pass",
                "reason": "explicit send produced matching LXMF/RNS evidence",
                "next_step": "capture report",
                "proof_match_state": "matched_packet_proof",
                "inbound_reply_match_state": "matched_peer_reply"
            },
            "readiness_probe": {
                "ready_to_send": true,
                "steps": [
                    {"stage": "runtime_setup", "ok": true, "detail": "runtime ready"}
                ]
            },
            "send": {
                "requested": true,
                "ok": true,
                "message_id": "packet-1",
                "native_lxmf_state": "submitted_to_rns_net"
            },
            "wait": {
                "status": "observed",
                "proof_match_state": "matched_packet_proof",
                "inbound_reply_match_state": "matched_peer_reply",
                "inbound_messages": 1,
                "delivery_updates": 2,
                "packet_proofs": 1
            }
        }))
        .expect("json")
        .lines()
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();

        let card = diagnostics_preview_lxmf_delivery_card(&lines).expect("lxmf card");
        assert_eq!(card.outcome, "pass");
        assert!(card.send_state.contains("submitted_to_rns_net"));
        assert_eq!(card.proof_state, "matched_packet_proof");
        assert_eq!(card.inbound_state, "matched_peer_reply");
        assert_eq!(
            card.event_counts,
            "inbound=1, delivery_updates=2, packet_proofs=1"
        );
        assert_eq!(card.readiness_stage, "ready or not requested");
    }

    #[test]
    fn diagnostics_lxmf_delivery_card_extracts_nested_blocker() {
        let lines = serde_json::to_string_pretty(&serde_json::json!({
            "report": "native_network_smoke_test",
            "lxmf_live_interop": {
                "report": "native_lxmf_live_interop",
                "classification": {
                    "outcome": "blocked",
                    "reason": "target peer is not ready for direct LXMF send",
                    "next_step": "request peer path"
                },
                "readiness_probe": {
                    "ready_to_send": false,
                    "steps": [
                        {"stage": "path_discovery", "ok": false, "detail": "queued request_path"}
                    ]
                },
                "send": {
                    "requested": true,
                    "ok": false,
                    "skipped": "LXMF delivery probe did not report ready_to_send"
                },
                "wait": {
                    "status": "timeout",
                    "proof_match_state": "no_matching_packet_proof",
                    "inbound_reply_match_state": "no_matching_peer_reply",
                    "inbound_messages": 0,
                    "delivery_updates": 0,
                    "packet_proofs": 0
                }
            }
        }))
        .expect("json")
        .lines()
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();

        let card = diagnostics_preview_lxmf_delivery_card(&lines).expect("lxmf card");
        assert_eq!(card.outcome, "blocked");
        assert!(card.send_state.contains("ready_to_send"));
        assert_eq!(card.readiness_stage, "path_discovery: queued request_path");
        assert_eq!(card.next_step, "request peer path");
    }

    #[test]
    fn diagnostics_propagation_sync_card_extracts_status_and_event_counts() {
        let lines = serde_json::to_string_pretty(&serde_json::json!({
            "report": "native_lxmf_propagation_diagnostics",
            "selected_node": FIXTURE_PROPAGATION_NODE_HASH,
            "sync": {
                "ok": true,
                "error": null
            },
            "before": {
                "has_path": true,
                "known_app_data": true,
                "link_state": "path_known",
                "transfer_state": "idle"
            },
            "after": {
                "has_path": true,
                "known_app_data": true,
                "link_state": "link_established",
                "transfer_state": "complete"
            },
            "sync_events": [
                {"kind": "propagation_sync", "stage": "list_response", "status": "complete", "detail": "received list"},
                {"kind": "propagation_status", "transfer_state": "list_request_sent"},
                {"kind": "propagation_status", "transfer_state": "complete"},
                {"kind": "debug", "message": "native LXMF propagation sync complete"}
            ],
            "blocker": "no propagation blocker reported",
            "next_step": "try propagation sync again or inspect runtime logs"
        }))
        .expect("json")
        .lines()
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();

        let card = diagnostics_preview_propagation_sync_card(&lines).expect("propagation card");

        assert_eq!(card.outcome, "complete");
        assert!(card.before.contains("path=true"));
        assert!(card.after.contains("transfer=complete"));
        assert_eq!(
            card.events,
            "structured=1, status=2, debug=1, messages=0, total=4"
        );
        assert!(card
            .event_lines
            .iter()
            .any(|line| line.contains("native LXMF propagation sync complete")));
        assert_eq!(card.blocker, "no propagation blocker reported");
    }

    #[test]
    fn diagnostics_stage_cards_extract_preflight_and_smoke_stages() {
        let preflight = serde_json::to_string_pretty(&serde_json::json!({
            "report": "native_network_preflight",
            "stages": [
                {
                    "stage": "backend",
                    "outcome": "pass",
                    "detail": "Auto",
                    "next_step": "continue"
                }
            ]
        }))
        .expect("json")
        .lines()
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
        let cards = diagnostics_preview_stage_cards(&preflight);
        assert_eq!(cards[0].kind, "preflight");
        assert_eq!(cards[0].stage, "backend");

        let smoke = serde_json::to_string_pretty(&serde_json::json!({
            "report": "native_network_smoke_test",
            "verdicts": {
                "path_discovery": {
                    "status": "fail",
                    "detail": "path unknown",
                    "next_action": "warm path"
                }
            }
        }))
        .expect("json")
        .lines()
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
        let cards = diagnostics_preview_stage_cards(&smoke);
        assert_eq!(cards[0].kind, "smoke");
        assert_eq!(cards[0].stage, "path_discovery");
        assert_eq!(cards[0].next_step, "warm path");
    }

    #[test]
    fn action_status_line_marks_ready_and_blocked() {
        assert_eq!(
            action_status_line(true, "identity", "create one"),
            "ready: identity"
        );
        assert_eq!(
            action_status_line(false, "identity", "create one"),
            "blocked: identity; create one"
        );
    }

    #[test]
    fn desktop_timestamp_formatting_uses_real_utc_dates() {
        assert_eq!(format_epoch_ms(0), "1970-01-01 00:00:00 UTC");
        assert_eq!(
            format_epoch_ms(1_700_000_000_000),
            "2023-11-14 22:13:20 UTC"
        );
    }

    #[test]
    fn browser_tab_window_keeps_active_tab_visible() {
        assert_eq!(visible_tab_window(0, 0, 6), (0, 0));
        assert_eq!(visible_tab_window(3, 0, 6), (0, 3));
        assert_eq!(visible_tab_window(10, 0, 6), (0, 6));
        assert_eq!(visible_tab_window(10, 5, 6), (2, 8));
        assert_eq!(visible_tab_window(10, 9, 6), (4, 10));
    }

    #[test]
    fn directory_view_scope_filters_live_saved_and_trusted_entries() {
        let now_secs = crate::app::current_epoch_ms() as f64 / 1_000.0;
        let mut live = crate::directory::DirectoryEntry::new(
            "live.node",
            "Live Node",
            crate::directory::DirectoryKind::Node,
        );
        live.last_seen = now_secs;
        let mut stale_saved = crate::directory::DirectoryEntry::new(
            "saved.node",
            "Saved Node",
            crate::directory::DirectoryKind::Node,
        );
        stale_saved.last_seen = now_secs - 7.0 * 60.0 * 60.0;
        stale_saved.saved = true;
        let mut trusted = stale_saved.clone();
        trusted.destination_hash = "trusted.node".into();
        trusted.trusted = true;

        assert!(directory_entry_matches_view(
            &live,
            &crate::directory::DirectoryKind::Node,
            &DirectoryScope::Live,
            ""
        ));
        assert!(!directory_entry_matches_view(
            &stale_saved,
            &crate::directory::DirectoryKind::Node,
            &DirectoryScope::Live,
            ""
        ));
        assert!(directory_entry_matches_view(
            &stale_saved,
            &crate::directory::DirectoryKind::Node,
            &DirectoryScope::Saved,
            ""
        ));
        assert!(directory_entry_matches_view(
            &trusted,
            &crate::directory::DirectoryKind::Node,
            &DirectoryScope::Trusted,
            ""
        ));
    }

    #[test]
    fn directory_view_filter_matches_python_style_directory_fields() {
        let now_secs = crate::app::current_epoch_ms() as f64 / 1_000.0;
        let mut entry = crate::directory::DirectoryEntry::new(
            "abcdef1234567890",
            "Archive Node",
            crate::directory::DirectoryKind::Node,
        );
        entry.last_seen = now_secs;
        entry.associated_hash = Some("peerfeed00112233".into());
        entry.saved = true;

        assert!(directory_entry_matches_view(
            &entry,
            &crate::directory::DirectoryKind::Node,
            &DirectoryScope::Live,
            "archive"
        ));
        assert!(directory_entry_matches_view(
            &entry,
            &crate::directory::DirectoryKind::Node,
            &DirectoryScope::Live,
            "peerfeed"
        ));
        assert!(directory_entry_matches_view(
            &entry,
            &crate::directory::DirectoryKind::Node,
            &DirectoryScope::Saved,
            "saved node"
        ));
        assert!(!directory_entry_matches_view(
            &entry,
            &crate::directory::DirectoryKind::Node,
            &DirectoryScope::Live,
            "lxmf-only"
        ));
    }

    #[test]
    fn directory_row_actions_are_minimal_and_kind_specific() {
        assert_eq!(
            directory_row_action_labels(&crate::directory::DirectoryKind::Node),
            vec!["Select", "Browse Node"]
        );
        assert_eq!(
            directory_row_action_labels(&crate::directory::DirectoryKind::Peer),
            vec!["Select", "Message Peer"]
        );
        assert_eq!(
            directory_row_action_labels(&crate::directory::DirectoryKind::Propagation),
            vec!["Select", "Use Propagation"]
        );
        assert_eq!(
            directory_row_action_labels(&crate::directory::DirectoryKind::OmenChat),
            vec!["Select", "Open Chat"]
        );
        assert_eq!(
            directory_row_action_labels(&crate::directory::DirectoryKind::Unknown),
            vec!["Select"]
        );
    }

    #[test]
    fn directory_selected_details_helpers_summarize_without_losing_full_hash() {
        assert_eq!(short_destination_hash("short.hash"), "short.hash");
        assert_eq!(
            short_destination_hash(FIXTURE_LXMF_PEER_HASH),
            "0011223344...ddeeff"
        );

        let peer = crate::directory::DirectoryEntry::new(
            FIXTURE_LXMF_PEER_HASH,
            "Peer",
            crate::directory::DirectoryKind::Peer,
        );
        assert!(directory_selected_kind_note(&peer).contains("LXMF conversation"));
    }

    #[test]
    fn directory_selected_details_primary_actions_are_kind_specific() {
        assert_eq!(
            directory_selected_primary_action_labels(&crate::directory::DirectoryKind::Node),
            vec!["Browse Node"]
        );
        assert_eq!(
            directory_selected_primary_action_labels(&crate::directory::DirectoryKind::Peer),
            vec!["Message Peer", "Inspect Peer"]
        );
        assert_eq!(
            directory_selected_primary_action_labels(&crate::directory::DirectoryKind::Propagation),
            vec!["Use Propagation"]
        );
        assert_eq!(
            directory_selected_primary_action_labels(&crate::directory::DirectoryKind::OmenChat),
            vec!["Open Chat"]
        );
        assert_eq!(
            directory_selected_primary_action_labels(&crate::directory::DirectoryKind::Unknown),
            vec!["Select"]
        );
    }

    #[test]
    fn directory_selected_management_controls_are_kind_specific() {
        assert!(directory_kind_supports_identify_toggle(
            &crate::directory::DirectoryKind::Node
        ));
        assert!(!directory_kind_supports_identify_toggle(
            &crate::directory::DirectoryKind::OmenChat
        ));
        assert!(!directory_kind_supports_identify_toggle(
            &crate::directory::DirectoryKind::Propagation
        ));
        assert!(directory_kind_supports_delivery_preference(
            &crate::directory::DirectoryKind::Peer
        ));
        assert!(!directory_kind_supports_delivery_preference(
            &crate::directory::DirectoryKind::Node
        ));
        assert!(!directory_kind_supports_delivery_preference(
            &crate::directory::DirectoryKind::OmenChat
        ));
        assert!(!directory_kind_supports_delivery_preference(
            &crate::directory::DirectoryKind::Propagation
        ));
    }

    #[test]
    fn directory_selected_state_lines_are_kind_specific() {
        let mut node = crate::directory::DirectoryEntry::new(
            "node.hash",
            "Node",
            crate::directory::DirectoryKind::Node,
        );
        node.identify_on_connect = true;
        let node_lines = directory_selected_state_lines(&node).join("\n");
        assert!(node_lines.contains("identify on connect: true"));
        assert!(!node_lines.contains("preferred LXMF delivery"));

        let mut peer = crate::directory::DirectoryEntry::new(
            "peer.hash",
            "Peer",
            crate::directory::DirectoryKind::Peer,
        );
        peer.preferred_delivery = Some(crate::directory::PreferredDelivery::Propagated);
        let peer_lines = directory_selected_state_lines(&peer).join("\n");
        assert!(peer_lines.contains("preferred LXMF delivery: Propagated"));
        assert!(!peer_lines.contains("identify on connect"));

        let omenchat = crate::directory::DirectoryEntry::new(
            "chat.hash",
            "Chat",
            crate::directory::DirectoryKind::OmenChat,
        );
        let omenchat_lines = directory_selected_state_lines(&omenchat).join("\n");
        assert!(omenchat_lines.contains("OMENchat server rank"));
        assert!(!omenchat_lines.contains("preferred LXMF delivery"));
        assert!(!omenchat_lines.contains("identify on connect"));
    }

    #[test]
    fn desktop_interface_cards_show_kind_specific_fields() {
        let mut tcp = crate::interfaces::ReticulumInterfaceProfile::tcp_client("tcp", "TCP");
        tcp.network_name = "meshnet".into();
        tcp.passphrase = "secret".into();
        let mut server =
            crate::interfaces::ReticulumInterfaceProfile::tcp_server("server", "Server");
        server.target_host = "127.0.0.1".into();
        server.network_name = "servernet".into();
        server.passphrase = "server secret".into();
        let mut rnode = crate::interfaces::ReticulumInterfaceProfile::rnode("rn", "RNode");
        rnode.device_port = "/dev/ttyUSB0".into();

        assert!(desktop_interface_detail_lines(&tcp)
            .iter()
            .any(|line| line.contains("TCP gateway")));
        assert!(desktop_interface_detail_lines(&tcp)
            .iter()
            .any(|line| line.contains("IFAC: network=meshnet passphrase=configured")));
        assert!(!desktop_interface_detail_lines(&tcp)
            .iter()
            .any(|line| line.contains("radio:")));
        assert!(desktop_interface_detail_lines(&server)
            .iter()
            .any(|line| line.contains("TCP server listen: 127.0.0.1:4242")));
        assert!(desktop_interface_detail_lines(&server)
            .iter()
            .any(|line| line.contains("IFAC: network=servernet passphrase=configured")));
        assert!(desktop_interface_detail_lines(&rnode)
            .iter()
            .any(|line| line.contains("radio: frequency=")));
    }

    #[test]
    fn interface_config_preview_lines_preserve_blank_rows() {
        assert_eq!(interface_config_preview_lines(""), vec!["".to_string()]);
        assert_eq!(
            interface_config_preview_lines("[interfaces]\n\n  enabled = true"),
            vec![
                "[interfaces]".to_string(),
                " ".to_string(),
                "  enabled = true".to_string(),
            ]
        );
    }

    #[test]
    fn native_setup_steps_show_first_run_progress() {
        let root = std::env::temp_dir().join(format!(
            "omenbrowser-rs-desktop-setup-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let paths = crate::config::AppPaths::from_root(root);
        paths.ensure().expect("paths");
        let mut app = App::new(crate::config::AppConfig {
            paths,
            settings: crate::storage::settings::AppSettings::default(),
        });

        let steps = native_setup_steps(&app);
        assert_eq!(steps.len(), 6);
        assert_eq!(steps[0].title, "Identity");
        assert!(!steps[0].ready);
        assert!(!steps[1].ready);

        let identity_path = app.paths.identities_dir.join("setup_identity");
        std::fs::write(&identity_path, b"identity").expect("identity");
        app.settings.identity_path = Some(identity_path);
        app.settings.active_identity_label = Some("Setup Identity".into());
        app.settings.runtime_backend = RuntimeBackendSetting::Reticulum;
        app.create_tcp_client_interface_profile();

        let steps = native_setup_steps(&app);
        assert!(steps[0].ready);
        assert!(steps[1].ready);
        assert!(steps[2].detail.contains("native-supported"));
        assert!(setup_tcp_client_profile(&app).is_some());
    }

    #[cfg(feature = "chat-client-rns")]
    #[test]
    fn omenchat_live_open_errors_have_user_visible_statuses() {
        assert!(omenchat_live_open_error_status("has no known identity key")
            .contains("path/key missing"));
        assert!(omenchat_live_open_error_status(
            "timed out waiting for rns-net link establishment"
        )
        .contains("Link handshake"));
        assert!(
            omenchat_live_open_error_status("native Reticulum runtime is not running")
                .contains("runtime is not running")
        );
    }

    #[cfg(feature = "chat-client")]
    #[test]
    fn omenchat_manual_target_accepts_canonical_legacy_or_raw_hash() {
        let uppercase = FIXTURE_OMENCHAT_HASH.to_ascii_uppercase();
        let canonical = format!("omenchat://{FIXTURE_OMENCHAT_HASH}");
        assert_eq!(
            normalize_omenchat_manual_target(&format!("omenchat://{uppercase}")).as_deref(),
            Some(canonical.as_str())
        );
        assert_eq!(
            normalize_omenchat_manual_target(&format!("omenchat:{uppercase}")).as_deref(),
            Some(canonical.as_str())
        );
        assert_eq!(
            normalize_omenchat_manual_target(FIXTURE_OMENCHAT_HASH).as_deref(),
            Some(canonical.as_str())
        );
        assert!(normalize_omenchat_manual_target("mockchatdestination").is_none());
    }

    #[cfg(feature = "chat-client")]
    #[test]
    fn new_chat_creates_blank_session_instead_of_restoring_existing_chat() {
        let root = std::env::temp_dir().join(format!(
            "omenbrowser-rs-desktop-new-chat-blank-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let paths = crate::config::AppPaths::from_root(root);
        paths.ensure().expect("paths");
        let app = App::new(crate::config::AppConfig {
            paths,
            settings: crate::storage::settings::AppSettings::default(),
        });
        let mut desktop = DesktopApp::new(app);
        let descriptor = OmenChatDescriptor {
            server_destination: FIXTURE_CHAT_SERVER_HASH.into(),
            display_name: Some("Existing Server".into()),
            rooms_hint: vec!["lobby".into()],
            local_display_name: Some("tester".into()),
            ..OmenChatDescriptor::default()
        };
        let existing_id = desktop.open_omenchat_status_session(descriptor, "connected".into());
        desktop.ensure_pane_for_omenchat(existing_id);
        let existing_pane = desktop
            .find_workspace_pane(&DesktopPane::OmenChat(existing_id))
            .expect("existing pane");
        desktop.close_workspace_pane(existing_pane);

        let _ = desktop.update(Message::NewOmenChatPane);

        let sessions = desktop.chat_client.sessions();
        assert_eq!(sessions.len(), 2);
        assert!(sessions
            .iter()
            .any(|session| session.session_id == existing_id
                && session.server.destination == FIXTURE_CHAT_SERVER_HASH));
        let blank = sessions
            .iter()
            .find(|session| session.session_id != existing_id)
            .expect("blank session");
        assert!(is_pending_omenchat_destination(&blank.server.destination));
        assert_eq!(blank.server.display_name, "New Chat");
        assert!(desktop
            .find_workspace_pane(&DesktopPane::OmenChat(blank.session_id))
            .is_some());
        assert!(desktop
            .find_workspace_pane(&DesktopPane::OmenChat(existing_id))
            .is_none());
    }

    #[cfg(feature = "chat-client")]
    #[test]
    fn opening_existing_omenchat_destination_restores_without_duplicate_session() {
        let root = std::env::temp_dir().join(format!(
            "omenbrowser-rs-desktop-open-existing-chat-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let paths = crate::config::AppPaths::from_root(root);
        paths.ensure().expect("paths");
        let app = App::new(crate::config::AppConfig {
            paths,
            settings: crate::storage::settings::AppSettings::default(),
        });
        let mut desktop = DesktopApp::new(app);
        let descriptor = OmenChatDescriptor {
            server_destination: FIXTURE_CHAT_SERVER_HASH.into(),
            display_name: Some("Existing Server".into()),
            rooms_hint: vec!["lobby".into()],
            local_display_name: Some("tester".into()),
            ..OmenChatDescriptor::default()
        };
        let existing_id = desktop.open_omenchat_status_session(descriptor, "connected".into());
        desktop.ensure_pane_for_omenchat(existing_id);
        let existing_pane = desktop
            .find_workspace_pane(&DesktopPane::OmenChat(existing_id))
            .expect("existing pane");
        desktop.close_workspace_pane(existing_pane);

        desktop.omenchat_server_entry = format!("omenchat://{FIXTURE_CHAT_SERVER_HASH}");
        let _ = desktop.update(Message::OpenOmenChatServerEntry);

        assert_eq!(desktop.chat_client.sessions().len(), 1);
        assert!(desktop
            .find_workspace_pane(&DesktopPane::OmenChat(existing_id))
            .is_some());
        assert!(desktop
            .app
            .status
            .task
            .contains("restored existing OMENchat session"));
    }

    #[cfg(feature = "chat-client")]
    #[test]
    fn opening_destination_from_blank_chat_replaces_blank_pane() {
        let root = std::env::temp_dir().join(format!(
            "omenbrowser-rs-desktop-open-from-blank-chat-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let paths = crate::config::AppPaths::from_root(root);
        paths.ensure().expect("paths");
        let app = App::new(crate::config::AppConfig {
            paths,
            settings: crate::storage::settings::AppSettings::default(),
        });
        let mut desktop = DesktopApp::new(app);

        let _ = desktop.update(Message::NewOmenChatPane);
        let blank_id = desktop.chat_client.sessions()[0].session_id;
        assert!(is_pending_omenchat_destination(
            &desktop.chat_client.sessions()[0].server.destination
        ));

        desktop.omenchat_server_entry = FIXTURE_CHAT_SERVER_HASH.into();
        let _ = desktop.update(Message::OpenOmenChatServerEntry);

        let sessions = desktop.chat_client.sessions();
        assert_eq!(sessions.len(), 1);
        assert_ne!(sessions[0].session_id, blank_id);
        assert_eq!(sessions[0].server.destination, FIXTURE_CHAT_SERVER_HASH);
        assert!(!is_pending_omenchat_destination(
            &sessions[0].server.destination
        ));
        assert!(desktop
            .workspace_panes
            .iter()
            .any(|(_, pane)| *pane == DesktopPane::OmenChat(sessions[0].session_id)));
        assert!(desktop
            .workspace_panes
            .iter()
            .all(|(_, pane)| *pane != DesktopPane::OmenChat(blank_id)));
    }

    #[cfg(feature = "chat-client")]
    #[test]
    fn opening_different_omenchat_destinations_creates_separate_sessions() {
        let root = std::env::temp_dir().join(format!(
            "omenbrowser-rs-desktop-open-multiple-chats-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let paths = crate::config::AppPaths::from_root(root);
        paths.ensure().expect("paths");
        let app = App::new(crate::config::AppConfig {
            paths,
            settings: crate::storage::settings::AppSettings::default(),
        });
        let mut desktop = DesktopApp::new(app);

        desktop.omenchat_server_entry = FIXTURE_CHAT_SERVER_HASH.into();
        let _ = desktop.update(Message::OpenOmenChatServerEntry);
        desktop.omenchat_server_entry = FIXTURE_OMENCHAT_HASH.into();
        let _ = desktop.update(Message::OpenOmenChatServerEntry);

        let sessions = desktop.chat_client.sessions();
        assert_eq!(sessions.len(), 2);
        let first = sessions
            .iter()
            .find(|session| session.server.destination == FIXTURE_CHAT_SERVER_HASH)
            .expect("first server session");
        let second = sessions
            .iter()
            .find(|session| session.server.destination == FIXTURE_OMENCHAT_HASH)
            .expect("second server session");
        assert_ne!(first.session_id, second.session_id);
        assert!(desktop
            .workspace_panes
            .iter()
            .any(|(_, pane)| *pane == DesktopPane::OmenChat(first.session_id)));
        assert!(desktop
            .workspace_panes
            .iter()
            .any(|(_, pane)| *pane == DesktopPane::OmenChat(second.session_id)));

        desktop.omenchat_server_entry = format!("omenchat://{FIXTURE_CHAT_SERVER_HASH}");
        let _ = desktop.update(Message::OpenOmenChatServerEntry);

        assert_eq!(desktop.chat_client.sessions().len(), 2);
        assert!(desktop
            .app
            .status
            .task
            .contains("restored existing OMENchat session"));
    }

    #[tokio::test]
    async fn setup_open_address_switches_to_browser_and_uses_active_address() {
        let root = std::env::temp_dir().join(format!(
            "omenbrowser-rs-desktop-open-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let paths = crate::config::AppPaths::from_root(root);
        paths.ensure().expect("paths");
        let app = App::new(crate::config::AppConfig {
            paths,
            settings: crate::storage::settings::AppSettings::default(),
        });
        let mut desktop = DesktopApp::new(app);

        let _ = desktop.update(Message::SwitchSection(WorkspaceSection::Settings));
        let _ = desktop.update(Message::AddressChanged("mock.page:/page/gallery.mu".into()));
        let _ = desktop.update(Message::OpenSetupAddress);

        assert_eq!(
            desktop.app.workspace.active_section,
            WorkspaceSection::Browser
        );
        assert_eq!(
            desktop.app.active_browser_tab().address_input,
            "mock.page:/page/gallery.mu"
        );
    }
}
