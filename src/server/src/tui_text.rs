use crate::config::ServerConfig;
#[cfg(any(test, feature = "live-rns-net"))]
use crate::live::{ActiveLinkSummary, LiveServerStats};
use crate::tui_format::{fit_line_to_width, human_age_duration, human_bytes, human_timestamp};
#[cfg(any(test, feature = "live-rns-net"))]
use crate::tui_format::{human_bytes_per_second, human_duration};

pub(crate) struct ModerationUserText<'a> {
    pub(crate) user_id: i64,
    pub(crate) identity_hex: &'a str,
    pub(crate) display_name: &'a str,
    pub(crate) role_label: &'a str,
    pub(crate) status_label: &'a str,
    pub(crate) lxmf_destination: Option<&'a str>,
    pub(crate) first_seen_at: i64,
    pub(crate) last_seen_at: Option<i64>,
    pub(crate) trusted: bool,
    pub(crate) banned: bool,
    pub(crate) muted: bool,
}

pub(crate) struct UserConsoleRowText<'a> {
    pub(crate) user_id: i64,
    pub(crate) display_name: &'a str,
    pub(crate) role_label: &'a str,
    pub(crate) status_label: &'a str,
    pub(crate) first_seen: &'a str,
    pub(crate) last_seen: &'a str,
    pub(crate) stale_delete: &'a str,
    pub(crate) identity_hex: &'a str,
    pub(crate) lxmf_destination: &'a str,
}

#[cfg(any(test, feature = "live-rns-net"))]
pub(crate) struct ActiveLinkMonitoringText {
    pub(crate) name: String,
    pub(crate) identity: String,
    pub(crate) room: String,
    pub(crate) age: String,
    pub(crate) activity: String,
    pub(crate) frames: u64,
    pub(crate) bytes: String,
    pub(crate) history_requests: u64,
    pub(crate) pings: u64,
    pub(crate) chat_messages: u64,
    pub(crate) commands: u64,
    pub(crate) upload_requests: u64,
    pub(crate) link_id: String,
}

#[cfg(any(test, feature = "live-rns-net"))]
pub(crate) struct ClosedLinkMonitoringText {
    pub(crate) name: String,
    pub(crate) identity: String,
    pub(crate) room: String,
    pub(crate) status: String,
    pub(crate) connected_for: String,
    pub(crate) closed_ago: String,
    pub(crate) link_id: String,
    pub(crate) reason: String,
}

pub(crate) struct SetupLaunchText<'a> {
    pub(crate) ready: usize,
    pub(crate) total: usize,
    pub(crate) first_missing_label: Option<&'a str>,
}

pub(crate) struct SetupAddressesText<'a> {
    pub(crate) public_addresses: &'a str,
    pub(crate) portal_page_file: &'a str,
}

pub(crate) struct SetupNextStepsText<'a> {
    pub(crate) launch_status: &'a str,
    pub(crate) all_ready: bool,
    pub(crate) missing_labels: Vec<&'a str>,
    pub(crate) storage_root: &'a str,
    pub(crate) reticulum_summary: &'a str,
    pub(crate) upload_policy: &'a str,
}

pub(crate) struct SetupChecklistLineText<'a> {
    pub(crate) marker: &'a str,
    pub(crate) label: &'a str,
    pub(crate) detail: &'a str,
    pub(crate) max_width: usize,
}

pub(crate) struct SetupConsoleText<'a> {
    pub(crate) checklist: &'a str,
    pub(crate) addresses: &'a str,
    pub(crate) next_steps: &'a str,
}

pub(crate) struct RoomListLabelText<'a> {
    pub(crate) marker: &'a str,
    pub(crate) room_id: i64,
    pub(crate) name: &'a str,
    pub(crate) topic: Option<&'a str>,
    pub(crate) max_width: usize,
}

pub(crate) struct RoomConsoleRowText<'a> {
    pub(crate) room_id: i64,
    pub(crate) name: &'a str,
    pub(crate) topic: &'a str,
}

pub(crate) struct OverviewOperatorSummaryText<'a> {
    pub(crate) ready: usize,
    pub(crate) total: usize,
    pub(crate) first_missing_label: Option<&'a str>,
    pub(crate) live_line: &'a str,
    pub(crate) interface_summary: &'a str,
    pub(crate) room_count: usize,
    pub(crate) upload_max: String,
    pub(crate) upload_quota: String,
}

pub(crate) struct PortalPanelText<'a> {
    pub(crate) checklist: &'a str,
    pub(crate) destination: &'a str,
    pub(crate) page_state: &'a str,
    pub(crate) motd: &'a str,
}

pub(crate) struct IdentityPanelText<'a> {
    pub(crate) identity_file: &'a str,
    pub(crate) storage_root: &'a str,
    pub(crate) checklist: &'a str,
    pub(crate) destinations: &'a str,
    pub(crate) database_path: &'a str,
    pub(crate) reticulum_path: &'a str,
    pub(crate) reticulum_config_path: &'a str,
}

pub(crate) fn command_help_text() -> String {
    [
        "commands:",
        "  status | refresh        show current config, rooms, addresses, and limits",
        "  setup                   show first-run checklist, join addresses, and upload policy",
        "  rooms                   list rooms",
        "  users                   list known users, moderation ids, and stale-delete state",
        "  add-room <name> [topic] add or unarchive a room",
        "  room-topic <room_id> [topic]",
        "                           set or clear a room topic",
        "  archive-room <room_id>  archive a non-lobby room",
        "  set-name <name>         set server display name",
        "  set-operator <label>    set operator label",
        "  set-motd <message>      set quiet server launch message",
        "  set-announce-interval <minutes>",
        "                           set repeat announce interval, default 360",
        "  set-upload-quota-bytes <bytes|0>",
        "                           set per-identity upload quota; 0 disables uploads",
        "  set-upload-max-file-bytes <bytes>",
        "                           reject individual files above this size",
        "  set-ping-interval <seconds>",
        "                           set client live ping interval, default 30",
        "  set-max-message-bytes <bytes>",
        "                           set per-message payload limit",
        "  set-history-batch-size <count>",
        "                           set history events returned per request",
        "  set-join-backlog-events <count>",
        "                           set events included when a user joins a room",
        "  set-large-batch-threshold-bytes <bytes>",
        "                           set resource threshold for large batches",
        "  set-rate-messages-per-minute <count>",
        "                           set per-user message send rate",
        "  set-rate-commands-per-minute <count>",
        "                           set per-user command rate",
        "  tcp-server <ip:port>    write Local TCP Listener test config",
        "  tcp-client <host:port>  write Connect To Gateway config",
        "  ban-user <id>           block a known user locally",
        "  unban-user <id>         allow a known user again",
        "  mute-user <id>          block room-message sends but allow reading",
        "  unmute-user <id>        allow room-message sends again",
        "  trust-user <id>         mark a known user trusted",
        "  untrust-user <id>       remove trusted mark",
        "  delete-user <id>        delete one inactive stale known-user record older than 24h; run users first",
        "  prune-stale-users       delete all inactive stale known-user records older than 24h",
        "  set-user-role <id> <standard|trusted|mod|admin>",
        "  show-config             print config.toml",
        "  help                    show commands",
        "  quit                    exit",
    ]
    .join("\n")
        + "\n"
}

pub(crate) fn admin_help_text() -> String {
    [
        "First Run Checklist",
        "",
        "1. Setup: choose Connect Gateway for normal RNS or Local Listener for local tests.",
        "2. Start Live, then confirm Monitoring shows a connected interface and fresh traffic.",
        "3. Portal: copy the omenchat:// URI or the NomadNet portal URL for MOTD/rules.",
        "4. Test from a second isolated OMENbrowser_rs app root before inviting users.",
        "5. Check Logs for startup announce, interface failures, protocol errors, and repeated reconnects.",
        "6. Confirm Identity paths stay under this server home, normally ~/.omenchatd.",
        "7. Edit Rooms, Moderation, limits, and reticulum/storage/pages/index.mu before a public launch.",
        "",
        "Storage And Announces",
        "",
        "omenchatd is standalone. It should keep identity, database, logs, Reticulum config, and portal pages inside its own server root.",
        "OMENchat announces as omenchat.node. Operators should not change that service type; it is how OMENbrowser_rs discovers chat servers.",
        "The optional NomadNet portal announces separately as nomadnetwork.node and always serves /page/index.mu. It is for MOTD/rules/portal text, not chat traffic.",
        "Users join chat with omenchat://<destination_hash>. NomadNet links are only the quiet launch/portal path.",
        "The TUI starts the same live Reticulum path as `omenchatd run` when built with live-rns-net.",
        "",
        "Traffic And Upload Policy",
        "",
        "Monitoring is the first place to check server health: active links, recent closes, request mix, traffic, pings, history sync, uploads, and interface state.",
        "The client ping interval is configurable. Keep it low-noise; reduce it only when live testing proves disconnect detection is too slow.",
        "Upload quota is the rotating per-identity cache allowance. Max upload file size is the per-file rejection cap. Default alpha policy is 50 MiB quota and 512 KiB per file.",
        "Set upload quota to 0 to disable uploads. Use Setup/Overview actions or line-console commands for scriptable setup.",
        "",
        "Keyboard",
        "",
        "Tab / Shift+Tab: switch admin panels",
        "Left / Right: switch admin panels",
        "Up / Down: move room/user selection",
        "Up / Down / PageUp / PageDown: scroll this Help panel while Help is selected",
        "Enter: edit the primary setting on the current panel",
        "a: edit server MOTD",
        "g / x: start or stop the live server for this TUI session",
        "n: add a room",
        "t: edit selected room topic",
        "d: archive selected room",
        "o: edit operator label",
        "v: edit announce interval minutes",
        "i: write a Local TCP Listener config",
        "w: write a Connect To Gateway config",
        "c: monitoring",
        "y: audit history",
        "Portal panel: public addresses, MOTD, and NomadNet portal file preview",
        "b: ban/unban selected user in Moderation",
        "k: kick selected user's active links in Moderation",
        "e: mute/unmute selected user in Moderation",
        "u: trust/untrust selected user in Moderation",
        "p: cycle selected user role in Moderation; visible role actions set Standard, Trusted, Moderator, or Admin directly",
        "d: delete selected stale user in Moderation when last seen is older than 24h",
        "Delete Stale Record / Confirm Delete Record only removes an inactive known-user database row; active links block it.",
        "Prune Inactive Stale / Confirm Prune Records removes inactive stale known-user rows older than 24h and skips users with active links.",
        "l: logs",
        "s: save config",
        "q / Ctrl+C: quit",
        "",
        "Permissions",
        "",
        "Admins can create/archive rooms, change roles, unban users, and perform moderator actions.",
        "Moderators can change topics, kick, ban, mute, unmute, trust, and send notices.",
        "User records are keyed by the identified OMENbrowser Reticulum identity, not by transient Link ids.",
        "",
        "OMENchat Slash Commands",
        "",
        "/help: show client command help",
        "/me <action>: send an action-style room message",
        "/rooms: list rooms",
        "/join <room>: join a room",
        "/topic <topic>: change the active room topic; moderator or admin",
        "/create <room> [topic]: create a room; admin only",
        "/notice <message>: post a room notice; moderator or admin",
        "/kick <user>: disconnect a user's active room links; moderator or admin",
        "/ban <user>: ban a user; moderator or admin, but only admins can act on admins",
        "/unban <user>: unban a user; admin only",
        "/mute <user> / /unmute <user>: toggle send permission; moderator or admin",
        "/role <user> <standard|trusted|mod|admin>: change role; admin only",
        "/upload <path>: offer a file upload; rejected if muted, banned, over max file size, or over quota",
        "",
        "Mouse",
        "",
        "Click panel tabs to switch.",
        "Click room/user rows to select them.",
        "Click visible room/user actions to run them.",
        "Mouse wheel in Rooms or Moderation changes selection.",
        "",
        "Limits",
        "",
        "Overview shows the compact dashboard and common launch actions. Setup and line-console commands expose the detailed limit controls.",
        "Line-console commands and `omenchatd config set` expose the same limit controls for scriptable setup.",
        "",
        "The Logs panel tails omenchatd.log from this server home.",
        "The Audit panel filters the same log down to local admin actions.",
        "",
        "This admin UI edits local configuration/database state and can supervise the live server in this terminal.",
    ]
    .join("\n")
}

pub(crate) fn reticulum_interface_summary(
    contents: Option<&str>,
    path: &std::path::Path,
) -> String {
    let Some(contents) = contents else {
        return format!("missing Reticulum config: {}", path.display());
    };
    let lower = contents.to_ascii_lowercase();
    if lower.contains("tcpclientinterface") {
        let host = config_value(contents, "target_host").unwrap_or("unknown-host");
        let port = config_value(contents, "target_port").unwrap_or("unknown-port");
        return format!(
            "TCP gateway client -> {host}:{port}; config {}",
            path.display()
        );
    }
    if lower.contains("tcpserverinterface") {
        let host = config_value(contents, "listen_ip").unwrap_or("unknown-listen-ip");
        let port = config_value(contents, "listen_port").unwrap_or("unknown-port");
        return format!(
            "local TCP server listener -> {host}:{port}; config {}",
            path.display()
        );
    }
    let enabled = lower.contains("enabled = yes") || lower.contains("interface_enabled = true");
    if lower.contains("type") && enabled {
        return format!("custom enabled interface config: {}", path.display());
    }
    format!(
        "no enabled interface yet; configure TCP Gateway for wider network or Local TCP Server for local tests; config {}",
        path.display()
    )
}

pub(crate) fn interface_operator_summary_text(contents: &str, path: &std::path::Path) -> String {
    let mode = reticulum_interface_summary(Some(contents), path);
    let lower = contents.to_ascii_lowercase();
    let ifac = interface_ifac_summary(contents);
    let next = if lower.contains("tcpclientinterface") {
        "Start Live, then verify Monitoring shows the gateway connected"
    } else if lower.contains("tcpserverinterface") {
        "Start Live, then verify Monitoring shows incoming clients"
    } else if lower.contains("type")
        && (lower.contains("enabled = yes") || lower.contains("interface_enabled = true"))
    {
        "Start Live and verify Monitoring before publishing addresses"
    } else {
        "choose Connect Gateway for normal use, or Local TCP Listener for local/direct tests"
    };
    [
        "interface setup:".to_string(),
        format!("  mode: {mode}"),
        format!("  IFAC: {ifac}"),
        format!("  next: {next}"),
        "  storage: config stays inside this server root".to_string(),
        "  identity: interface edits do not overwrite the server identity".to_string(),
    ]
    .join("\n")
}

fn interface_ifac_summary(contents: &str) -> String {
    let network_name = config_value(contents, "network_name")
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let passphrase = config_value(contents, "passphrase")
        .map(str::trim)
        .filter(|value| !value.is_empty());
    match (network_name, passphrase) {
        (Some(name), Some(_)) => format!("network_name={name}; passphrase set"),
        (Some(name), None) => format!("network_name={name}; passphrase missing"),
        (None, Some(_)) => "passphrase set; network_name missing".into(),
        (None, None) => "not configured".into(),
    }
}

fn config_value<'a>(contents: &'a str, key: &str) -> Option<&'a str> {
    contents.lines().find_map(|line| {
        let line = line.trim();
        let (line_key, value) = line.split_once('=')?;
        (line_key.trim() == key)
            .then(|| value.trim().trim_matches('"'))
            .filter(|value| !value.is_empty())
    })
}

pub(crate) fn audit_summary_text(tail: &str) -> String {
    let mut config_changes = 0usize;
    let mut room_changes = 0usize;
    let mut moderation_changes = 0usize;
    let mut stale_user_changes = 0usize;
    let mut interface_changes = 0usize;
    for line in tail.lines() {
        let lower = line.to_ascii_lowercase();
        if lower.contains("tcpclientinterface")
            || lower.contains("tcpserverinterface")
            || lower.contains("reticulum")
        {
            interface_changes += 1;
        } else if lower.contains("room") {
            room_changes += 1;
        } else if lower.contains("stale user")
            || lower.contains("deleted stale")
            || lower.contains("pruned stale")
        {
            stale_user_changes += 1;
        } else if lower.contains("banned user")
            || lower.contains("unbanned user")
            || lower.contains("muted user")
            || lower.contains("unmuted user")
            || lower.contains("trusted user")
            || lower.contains("untrusted user")
            || lower.contains("kicked active links")
            || lower.contains("set user role")
        {
            moderation_changes += 1;
        } else if lower.contains("server")
            || lower.contains("operator")
            || lower.contains("motd")
            || lower.contains("announce interval")
            || lower.contains("upload")
            || lower.contains("ping interval")
            || lower.contains("message")
            || lower.contains("history")
            || lower.contains("rate")
            || lower.contains("threshold")
            || lower.contains("config")
        {
            config_changes += 1;
        }
    }
    let total = tail.lines().filter(|line| !line.trim().is_empty()).count();
    if total == 0 {
        return "summary: no admin actions in this view".into();
    }
    format!(
        "summary: {total} action(s) | config {config_changes} | interfaces {interface_changes} | rooms {room_changes} | moderation {moderation_changes} | stale cleanup {stale_user_changes}"
    )
}

pub(crate) fn server_limits_text(config: &ServerConfig) -> String {
    let limits = &config.limits;
    [
        format!(
            "message size: {}",
            human_bytes(limits.max_message_bytes as u64)
        ),
        format!("history batch: {} event(s)", limits.history_batch_size),
        format!("join backlog: {} event(s)", limits.join_backlog_events),
        format!(
            "large batch threshold: {}",
            human_bytes(limits.large_batch_threshold_bytes as u64)
        ),
        upload_policy_hint(config),
        format!("ping interval: {}s", config.ping_interval_seconds),
        format!("message rate: {} / minute", limits.rate_messages_per_minute),
        format!("command rate: {} / minute", limits.rate_commands_per_minute),
        "edit: line console or config set".into(),
    ]
    .join("\n")
}

pub(crate) fn upload_policy_hint(config: &ServerConfig) -> String {
    if config.upload_quota_bytes == 0 {
        "uploads: disabled; upload offers are rejected".into()
    } else {
        format!(
            "uploads: max file {}; quota {} per identity",
            human_bytes(config.upload_max_file_bytes),
            human_bytes(config.upload_quota_bytes)
        )
    }
}

pub(crate) fn upload_quota_update_text(bytes: u64) -> String {
    if bytes == 0 {
        "upload quota updated: uploads disabled".into()
    } else {
        format!("upload quota updated: {}", human_bytes(bytes))
    }
}

pub(crate) fn upload_max_file_update_text(bytes: u64) -> String {
    format!("upload max file updated: {}", human_bytes(bytes))
}

pub(crate) fn ping_interval_update_text(seconds: u64) -> String {
    format!("ping interval updated: {seconds} second(s)")
}

pub(crate) fn announce_interval_update_text(minutes: u64) -> String {
    format!("announce interval updated: {minutes} minute(s)")
}

pub(crate) fn max_message_bytes_update_text(bytes: usize) -> String {
    format!("max message size updated: {}", human_bytes(bytes as u64))
}

pub(crate) fn history_batch_size_update_text(count: usize) -> String {
    format!("history batch size updated: {count}")
}

pub(crate) fn join_backlog_events_update_text(count: usize) -> String {
    format!("join backlog events updated: {count}")
}

pub(crate) fn large_batch_threshold_update_text(bytes: usize) -> String {
    format!(
        "large batch threshold updated: {}",
        human_bytes(bytes as u64)
    )
}

pub(crate) fn message_rate_update_text(count: usize) -> String {
    format!("message rate updated: {count} per minute")
}

pub(crate) fn command_rate_update_text(count: usize) -> String {
    format!("command rate updated: {count} per minute")
}

pub(crate) fn room_ready_update_text(name: &str) -> String {
    format!("room ready: #{}", name.trim().trim_start_matches('#'))
}

pub(crate) fn room_topic_update_text(room_id: i64) -> String {
    format!("room topic updated: id={room_id}")
}

pub(crate) fn room_archived_update_text(room_id: i64) -> String {
    format!("room archived: id={room_id}")
}

pub(crate) fn server_name_update_text() -> &'static str {
    "server name updated"
}

pub(crate) fn operator_label_update_text() -> &'static str {
    "operator label updated"
}

pub(crate) fn motd_update_text() -> &'static str {
    "MOTD updated"
}

pub(crate) fn user_banned_update_text(user_id: i64) -> String {
    format!("user banned: id={user_id}")
}

pub(crate) fn user_unbanned_update_text(user_id: i64) -> String {
    format!("user unbanned: id={user_id}")
}

pub(crate) fn user_muted_update_text(user_id: i64) -> String {
    format!("user muted: id={user_id}")
}

pub(crate) fn user_unmuted_update_text(user_id: i64) -> String {
    format!("user unmuted: id={user_id}")
}

pub(crate) fn user_trusted_update_text(user_id: i64) -> String {
    format!("user trusted: id={user_id}")
}

pub(crate) fn user_untrusted_update_text(user_id: i64) -> String {
    format!("user untrusted: id={user_id}")
}

pub(crate) fn user_role_update_text(user_id: i64, role_label: &str) -> String {
    format!("user role updated: id={user_id} role={role_label}")
}

pub(crate) fn selected_room_text(room_id: i64, name: &str, topic: Option<&str>) -> String {
    let topic = topic
        .filter(|topic| !topic.trim().is_empty())
        .unwrap_or("(no topic)");
    let archive = if room_id == 1 {
        "protected; #lobby stays available"
    } else {
        "admin archive available after confirmation"
    };
    [
        format!("room: #{name}"),
        format!("topic: {topic}"),
        format!("archive: {archive}"),
        "topic: mod/admin".to_string(),
        "create/archive: admin".to_string(),
        String::new(),
        format!("id: {room_id}"),
        "active rooms appear in /rooms and client sidebars".to_string(),
        String::new(),
        "select: click row, Up/Down, or mouse wheel".to_string(),
    ]
    .join("\n")
}

pub(crate) fn room_list_label_text(room: &RoomListLabelText<'_>) -> String {
    let topic = room.topic.unwrap_or_default().trim();
    let label = if topic.is_empty() {
        format!("{} #{} id={}", room.marker, room.name, room.room_id)
    } else {
        format!(
            "{} #{} id={} - {}",
            room.marker, room.name, room.room_id, topic
        )
    };
    fit_line_to_width(&label, room.max_width)
}

pub(crate) fn room_console_row_text(room: &RoomConsoleRowText<'_>) -> String {
    format!(
        "  #{:<20} id={:<4} {}\n",
        room.name, room.room_id, room.topic
    )
}

pub(crate) fn room_action_guide_text(
    selected: Option<(i64, &str, Option<&str>)>,
    pending_archive_room_id: Option<i64>,
) -> String {
    let Some((room_id, name, topic)) = selected else {
        return "Select a room or Add Room.\n\nAdmin: create/archive rooms.\nMod/Admin: edit topics.\n#lobby cannot be archived.".into();
    };
    let topic_state = if topic.map(|topic| !topic.trim().is_empty()).unwrap_or(false) {
        "Edit Topic changes the topic shown in clients."
    } else {
        "Edit Topic sets the missing topic."
    };
    let archive_state = if room_id == 1 {
        "Archive disabled: #lobby is required."
    } else if pending_archive_room_id == Some(room_id) {
        "Confirm Archive hides the room; history stays stored."
    } else {
        "Archive Room asks for one confirmation click."
    };

    [
        format!("Selected: #{name}"),
        "Add Room creates or restores a public room.".to_string(),
        topic_state.to_string(),
        archive_state.to_string(),
        "Permissions: admin create/archive; mod/admin topic.".to_string(),
    ]
    .join("\n")
}

pub(crate) fn moderation_selected_user_text(
    user: &ModerationUserText<'_>,
    active_links: usize,
    stale_delete_when_inactive: &str,
) -> String {
    let send_state = if user.banned {
        "blocked: banned users cannot open sessions, join rooms, send messages, or fetch history"
    } else if user.muted {
        "limited: muted users can join/read but cannot send room messages"
    } else {
        "allowed: user can join, read, and send messages"
    };
    let link_state = if active_links > 0 {
        format!("{active_links} active link(s); Kick Active Links disconnects current sessions")
    } else {
        "no active links".to_string()
    };
    let delete_state = if active_links > 0 {
        "blocked while active links exist; kick or ban first if cleanup is urgent".to_string()
    } else {
        stale_delete_when_inactive.to_string()
    };

    format!(
        "user: {name}\nid: {id}\nrns identity: {identity}\nlxmf: {lxmf}\n\naccess: {send_state}\nlinks: {link_state}\nstale delete: {delete_state}\n\nrole: {role}\nstatus: {status}\ntrusted media: {trusted}\n\nfirst seen: {first_seen}\nlast seen: {last_seen}\n\nClick a user row to select it.",
        name = user.display_name,
        id = user.user_id,
        identity = user.identity_hex,
        lxmf = user.lxmf_destination.unwrap_or("(none)"),
        role = user.role_label,
        status = user.status_label,
        trusted = if user.trusted { "yes" } else { "no" },
        first_seen = human_timestamp(user.first_seen_at),
        last_seen = user
            .last_seen_at
            .map(human_timestamp)
            .unwrap_or_else(|| "never".into()),
    )
}

pub(crate) fn moderation_action_guide_text(
    user: Option<&ModerationUserText<'_>>,
    active_links: usize,
    pending_delete_user_id: Option<i64>,
    pending_prune_stale_users: bool,
) -> String {
    let Some(user) = user else {
        return "Select a user to moderate.\n\nUsers appear after they connect to this server."
            .into();
    };
    let ban = if user.banned {
        "Unban allows future sessions; it does not reconnect them."
    } else if active_links > 0 {
        "Ban blocks future sessions and closes active links."
    } else {
        "Ban blocks future sessions for this identity."
    };
    let mute = if user.muted {
        "Unmute restores sending; reading was still allowed."
    } else {
        "Mute keeps read access but blocks sends/actions."
    };
    let trust = if user.trusted {
        "Untrust removes trusted-media handling."
    } else {
        "Trust enables trusted-media handling."
    };
    let role = match user.role_label {
        "admin" => "Role: admin manages roles, rooms, and moderation.",
        "mod" => "Role: moderator can topic/kick/ban/mute/trust/notice.",
        "trusted" => "Role: trusted member with trusted-media handling.",
        _ => "Role: standard can read/send unless muted or banned.",
    };
    let delete = if active_links > 0 {
        "Delete stale record blocked while active."
    } else if pending_delete_user_id == Some(user.user_id) {
        "Confirm Delete removes only this known-user record."
    } else {
        "Delete stale record needs 24h inactive and no links."
    };
    let prune = if pending_prune_stale_users {
        "Confirm Prune removes all inactive stale records."
    } else {
        "Prune removes all inactive records older than 24h."
    };
    [
        ban.to_string(),
        format!("Kick closes current links only; active={active_links}"),
        mute.to_string(),
        trust.to_string(),
        role.to_string(),
        delete.to_string(),
        prune.to_string(),
    ]
    .join("\n")
}

pub(crate) fn moderation_user_list_label(
    marker: &str,
    user: &ModerationUserText<'_>,
    stale_age_secs: i64,
    stale_delete_min_age_secs: i64,
    active_links: usize,
    max_width: usize,
) -> String {
    let stale = if stale_age_secs >= stale_delete_min_age_secs {
        "delete ok".to_string()
    } else {
        format!(
            "delete in {}",
            human_age_duration(stale_delete_min_age_secs.saturating_sub(stale_age_secs))
        )
    };
    let label = format!(
        "{marker} {:<22} {:<8} {:<8} active={:<2} {:<13} last seen {}",
        user.display_name,
        user.status_label,
        user.role_label,
        active_links,
        stale,
        user.last_seen_at
            .map(human_timestamp)
            .unwrap_or_else(|| "never".into())
    );
    fit_line_to_width(&label, max_width)
}

pub(crate) fn user_console_row_text(user: &UserConsoleRowText<'_>) -> String {
    format!(
        "  id={:<4} {:<20} role={:<8} status={:<8} first={} last={} stale_delete=\"{}\"\n       identity={} lxmf={}\n",
        user.user_id,
        user.display_name,
        user.role_label,
        user.status_label,
        user.first_seen,
        user.last_seen,
        user.stale_delete,
        user.identity_hex,
        user.lxmf_destination,
    )
}

#[cfg(any(test, feature = "live-rns-net"))]
pub(crate) fn upload_transfer_summary(stats: &LiveServerStats) -> String {
    if stats.upload_offers_in == 0
        && stats.upload_fetches_in == 0
        && stats.upload_resources_in == 0
        && stats.upload_inline_chunks_out == 0
        && stats.upload_resource_offers_out == 0
    {
        return "idle".into();
    }
    format!(
        "offer {} | fetch {} | stored {} ({}) | inline {} ({}) | resource offers {}",
        stats.upload_offers_in,
        stats.upload_fetches_in,
        stats.upload_resources_in,
        human_bytes(stats.upload_resource_bytes_in),
        stats.upload_inline_chunks_out,
        human_bytes(stats.upload_inline_bytes_out),
        stats.upload_resource_offers_out,
    )
}

#[cfg(any(test, feature = "live-rns-net"))]
pub(crate) fn interface_health_label(interface_lines: &[String]) -> String {
    if interface_lines.is_empty() {
        return "waiting for Reticulum interface stats".into();
    }
    let joined = interface_lines.join("\n").to_ascii_lowercase();
    if joined.contains("unavailable") || joined.contains("query failed") {
        return "stats unavailable; check runtime logs".into();
    }
    if joined.contains("interfaces: 0") {
        return "no interfaces visible; configure a gateway or local TCP server".into();
    }
    if joined.contains("connected=true")
        || joined.contains("connected=yes")
        || joined.contains("connected=connected")
        || joined.contains("connected=online")
    {
        return "connected; server can publish and receive".into();
    }
    "disconnected; watchdog will rebuild runtime after repeated samples".into()
}

#[cfg(any(test, feature = "live-rns-net"))]
pub(crate) fn closed_link_status_label(reason: &str) -> &'static str {
    let reason = reason.to_ascii_lowercase();
    if reason.contains("duplicate identity") || reason.contains("replaced") {
        "normal reconnect"
    } else if reason.contains("kick") || reason.contains("ban") || reason.contains("moderation") {
        "moderation close"
    } else if reason.contains("timeout") {
        "timeout; watch if repeated"
    } else if reason.contains("initiatorclosed")
        || reason.contains("initiator closed")
        || reason.contains("destinationclosed")
        || reason.contains("destination closed")
        || reason.contains("peer closed")
    {
        "peer/runtime closed"
    } else if reason.trim().is_empty() || reason == "unspecified" {
        "unknown close"
    } else {
        "check logs"
    }
}

#[cfg(any(test, feature = "live-rns-net"))]
pub(crate) fn closed_link_churn_summary(recent_close_reasons: &[&str]) -> String {
    if recent_close_reasons.is_empty() {
        return "recent close summary: none".into();
    }

    let mut normal = 0usize;
    let mut moderation = 0usize;
    let mut peer_runtime = 0usize;
    let mut timeout = 0usize;
    let mut investigate = 0usize;
    for reason in recent_close_reasons {
        match closed_link_status_label(reason) {
            "normal reconnect" => normal += 1,
            "moderation close" => moderation += 1,
            "peer/runtime closed" => peer_runtime += 1,
            "timeout; watch if repeated" => timeout += 1,
            _ => investigate += 1,
        }
    }

    let mut parts = Vec::new();
    if normal > 0 {
        parts.push(format!("normal reconnect {normal}"));
    }
    if peer_runtime > 0 {
        parts.push(format!("peer/runtime {peer_runtime}"));
    }
    if moderation > 0 {
        parts.push(format!("moderation {moderation}"));
    }
    if timeout > 0 {
        parts.push(format!("timeout {timeout}"));
    }
    if investigate > 0 {
        parts.push(format!("investigate {investigate}"));
    }
    format!(
        "recent close summary: {} total | {}",
        recent_close_reasons.len(),
        parts.join(" | ")
    )
}

#[cfg(any(test, feature = "live-rns-net"))]
pub(crate) fn active_link_activity_label(
    link: &ActiveLinkSummary,
    now_unix: i64,
) -> Option<String> {
    if link.connected_at_unix <= 0 {
        return None;
    }
    let age_secs = now_unix.saturating_sub(link.connected_at_unix).max(1) as f64;
    let age_minutes = (age_secs / 60.0).max(1.0 / 60.0);
    let frames_per_min = link.traffic.frames_in as f64 / age_minutes;
    let history_per_min = link.traffic.history_requests as f64 / age_minutes;
    let ping_per_min = link.traffic.pings as f64 / age_minutes;
    let upload_per_min = link.traffic.upload_requests as f64 / age_minutes;
    let mut flags = Vec::new();
    if frames_per_min >= 120.0 {
        flags.push("high frames");
    }
    if history_per_min >= 20.0 {
        flags.push("high history");
    }
    if ping_per_min >= 30.0 {
        flags.push("high ping");
    }
    if upload_per_min >= 10.0 {
        flags.push("high upload");
    }
    let flags = if flags.is_empty() {
        "normal".into()
    } else {
        flags.join(", ")
    };
    Some(format!(
        "inbound rate: {:.1} frames/min, {:.1} KiB/min | {flags}",
        frames_per_min,
        (link.traffic.bytes_in as f64 / 1024.0) / age_minutes
    ))
}

#[cfg(any(test, feature = "live-rns-net"))]
pub(crate) fn traffic_delta_text(
    previous: &LiveServerStats,
    current: &LiveServerStats,
    elapsed_secs: f64,
) -> String {
    let elapsed_secs = elapsed_secs.max(0.001);
    let bytes_in = current.bytes_in.saturating_sub(previous.bytes_in);
    let bytes_out = current.bytes_out.saturating_sub(previous.bytes_out);
    let resource_bytes = current
        .resource_bytes_out
        .saturating_sub(previous.resource_bytes_out);
    let frames_in = current.frames_in.saturating_sub(previous.frames_in);
    let frames_out = current.frames_out.saturating_sub(previous.frames_out);
    let chat = current
        .chat_messages_in
        .saturating_sub(previous.chat_messages_in);
    let history = current
        .history_requests_in
        .saturating_sub(previous.history_requests_in);
    let pings = current.pings_in.saturating_sub(previous.pings_in);
    let upload_fetches = current
        .upload_fetches_in
        .saturating_sub(previous.upload_fetches_in);
    let upload_inline_bytes = current
        .upload_inline_bytes_out
        .saturating_sub(previous.upload_inline_bytes_out);
    let room = current
        .room_navigation_in
        .saturating_sub(previous.room_navigation_in);
    let session = current
        .session_requests_in
        .saturating_sub(previous.session_requests_in);
    let problems = current
        .ignored_packets
        .saturating_sub(previous.ignored_packets)
        .saturating_add(
            current
                .unknown_link_packets
                .saturating_sub(previous.unknown_link_packets),
        )
        .saturating_add(
            current
                .protocol_errors
                .saturating_sub(previous.protocol_errors),
        );
    format!(
        "{} sample | rate rx {}/s ({}) / tx {}/s ({}) / resources {}/s ({})\nframes: {} in / {} out | requests: session {} room {} chat {} history {} ping {} | uploads: fetch {} inline {} | problems: {}",
        human_duration(elapsed_secs),
        human_bytes_per_second(bytes_in, elapsed_secs),
        human_bytes(bytes_in),
        human_bytes_per_second(bytes_out, elapsed_secs),
        human_bytes(bytes_out),
        human_bytes_per_second(resource_bytes, elapsed_secs),
        human_bytes(resource_bytes),
        frames_in,
        frames_out,
        session,
        room,
        chat,
        history,
        pings,
        upload_fetches,
        human_bytes(upload_inline_bytes),
        problems
    )
}

#[cfg(any(test, feature = "live-rns-net"))]
pub(crate) fn server_health_label(
    stats: &LiveServerStats,
    recent_close_reasons: &[&str],
) -> String {
    let problem_count = stats
        .ignored_packets
        .saturating_add(stats.unknown_link_packets)
        .saturating_add(stats.protocol_errors);
    if problem_count > 0 {
        return format!("server health: check logs; {problem_count} problem counter(s)");
    }
    let noteworthy_closes = recent_close_reasons
        .iter()
        .filter(|reason| {
            matches!(
                closed_link_status_label(reason),
                "timeout; watch if repeated" | "unknown close" | "check logs"
            )
        })
        .count();
    if noteworthy_closes > 0 {
        return format!("server health: watch link churn; {noteworthy_closes} notable close(s)");
    }
    if stats.active_links == 0 && stats.frames_in == 0 && stats.frames_out == 0 {
        return "server health: idle; waiting for clients".into();
    }
    if !recent_close_reasons.is_empty() {
        return "server health: ok; recent closes look explainable".into();
    }
    "server health: ok".into()
}

#[cfg(any(test, feature = "live-rns-net"))]
pub(crate) fn monitoring_operator_summary_text(
    stats: Option<&LiveServerStats>,
    interface_lines: &[String],
    recent_stats: &str,
    recent_close_reasons: &[&str],
) -> String {
    let Some(stats) = stats else {
        return "operator summary:\n  state: stopped\n  next: press g or Start Live Server"
            .to_string();
    };

    let problem_count = stats
        .ignored_packets
        .saturating_add(stats.unknown_link_packets)
        .saturating_add(stats.protocol_errors);
    let interface = interface_health_label(interface_lines);
    let traffic = if stats.frames_in == 0 && stats.frames_out == 0 {
        "traffic: idle; no OMENchat frames yet".to_string()
    } else {
        format!(
            "traffic: {} frame(s) in / {} out, {} received / {} sent",
            stats.frames_in,
            stats.frames_out,
            human_bytes(stats.bytes_in),
            human_bytes(stats.bytes_out)
        )
    };
    let recent = recent_stats
        .lines()
        .next()
        .filter(|line| !line.trim().is_empty())
        .unwrap_or("waiting for next sample");
    let health = if problem_count == 0 {
        "health: ok; no protocol/problem counters".to_string()
    } else {
        format!("health: check problem counters ({problem_count} total)")
    };

    [
        "monitoring:".to_string(),
        format!(
            "  clients active={} opened={} closed={} | interface: {interface}",
            stats.active_links, stats.links_opened, stats.links_closed
        ),
        format!("  {traffic}"),
        format!(
            "  requests session={} room={} chat={} history={} ping={} command={}",
            stats.session_requests_in,
            stats.room_navigation_in,
            stats.chat_messages_in,
            stats.history_requests_in,
            stats.pings_in,
            stats.commands_in
        ),
        format!("  uploads {}", upload_transfer_summary(stats)),
        format!("  {}", server_health_label(stats, recent_close_reasons)),
        format!("  {health} | recent: {recent}"),
    ]
    .join("\n")
}

#[cfg(any(test, feature = "live-rns-net"))]
pub(crate) fn active_link_monitoring_line(link: &ActiveLinkMonitoringText) -> String {
    format!(
        "  {name} [{identity}] {room} | age {age} | {activity} | rx {frames} frame(s), {bytes} | req h={history} p={ping} chat={chat} cmd={command} up={upload} | link {link_id}",
        name = link.name,
        identity = link.identity,
        room = link.room,
        age = link.age,
        activity = link.activity,
        frames = link.frames,
        bytes = link.bytes,
        history = link.history_requests,
        ping = link.pings,
        chat = link.chat_messages,
        command = link.commands,
        upload = link.upload_requests,
        link_id = link.link_id,
    )
}

#[cfg(any(test, feature = "live-rns-net"))]
pub(crate) fn closed_link_monitoring_line(link: &ClosedLinkMonitoringText) -> String {
    format!(
        "  {name} [{identity}] {room} | {status} | connected {connected_for} | closed {closed_ago} ago | reason {reason} | link {link_id}",
        name = link.name,
        identity = link.identity,
        room = link.room,
        status = link.status,
        connected_for = link.connected_for,
        closed_ago = link.closed_ago,
        link_id = link.link_id,
        reason = link.reason,
    )
}

pub(crate) fn setup_advice_for_label(label: &str) -> &'static str {
    match label {
        "server name" => "Edit Server Name so users recognize the server.",
        "operator" => "Edit Operator Label for the admin/contact field.",
        "identity" => {
            "Run init/status or start the live server once to create a native server identity."
        }
        "database" => "Run init or status to create omenchat.sqlite under this server home.",
        "reticulum" => {
            "Use Connect To Gateway for normal wider-network hosting, or Local TCP Listener for local/direct testing."
        }
        "lobby room" => "Run init/status to repair the required #lobby room.",
        "announce interval" => "Set announce interval above zero; default 360 minutes is fine.",
        _ => "Review this checklist item before publishing the server.",
    }
}

pub(crate) fn setup_action_for_label(label: &str) -> &'static str {
    match label {
        "server name" => "Use: Edit Server Name.",
        "operator" => "Use: Edit Operator Label.",
        "identity" => "Use: Start Live Server or run init/status.",
        "database" => "Use: Start Live Server or run init/status.",
        "reticulum" => {
            "Use: Connect To Gateway for normal hosting, or Local TCP Listener for local tests."
        }
        "lobby room" => "Use: Start Live Server or run init/status.",
        "announce interval" => "Use: Edit Announce Interval.",
        _ => "Use: review the matching Setup Action.",
    }
}

pub(crate) fn setup_launch_status_text(launch: &SetupLaunchText<'_>) -> String {
    if launch.ready == launch.total {
        return [
            "Launch status: READY for live testing".to_string(),
            "Next action: start the live server, verify Monitoring shows a connected interface, then share the omenchat:// URI or NomadNet portal URL.".to_string(),
        ]
        .join("\n");
    }
    let next = launch
        .first_missing_label
        .map(|label| format!("{label} - {}", setup_advice_for_label(label)))
        .unwrap_or_else(|| "review setup checklist".into());
    [
        format!(
            "Launch status: NEEDS SETUP ({}/{} ready)",
            launch.ready, launch.total
        ),
        format!("Next action: {next}"),
    ]
    .join("\n")
}

pub(crate) fn setup_addresses_text(addresses: &SetupAddressesText<'_>) -> String {
    let mut text = String::from(
        "Share after Monitoring shows connected:\n  OMENchat invite: client uri\n  MOTD/rules page: portal url\n\n",
    );
    text.push_str(addresses.public_addresses);
    if !text.ends_with('\n') {
        text.push('\n');
    }
    text.push_str(&format!(
        "portal page file: {}\n",
        addresses.portal_page_file
    ));
    text.push_str("chat URI format: omenchat://<destination_hash>");
    text
}

pub(crate) fn setup_next_steps_text(setup: &SetupNextStepsText<'_>) -> String {
    let mut steps = Vec::new();
    steps.push(setup.launch_status.to_string());
    steps.push(String::new());
    if setup.all_ready {
        steps.push("1. Start Live or press g.".to_string());
        steps
            .push("2. Open Monitoring and verify at least one interface is connected.".to_string());
        steps.push("3. Announce Now if you do not want to wait for the interval.".to_string());
        steps.push(
            "4. Share the omenchat:// invite or NomadNet portal URL from Portal.".to_string(),
        );
        steps.push("5. Test with a second isolated OMENbrowser_rs app root.".to_string());
        steps.push(format!("Limits: {}.", setup.upload_policy));
        steps.push(format!("Network: {}", setup.reticulum_summary));
        return steps.join("\n");
    }

    steps.push("Fix first, then Start Live:".to_string());
    for label in &setup.missing_labels {
        let advice = setup_advice_for_label(label);
        let action = setup_action_for_label(label);
        steps.push(format!("- {label}: {action} {advice}"));
    }
    steps.push(String::new());
    steps.push("Storage: server files stay under this omenchatd home.".to_string());
    steps.push(format!("home: {}", setup.storage_root));
    steps.push(String::new());
    steps.push(format!("Network: {}", setup.reticulum_summary));
    steps.push(String::new());
    steps.push(
        "Addresses: OMENchat announces as omenchat.node; share omenchat:// for chat or the NomadNet portal URL for MOTD/rules."
            .to_string(),
    );
    steps.push(String::new());
    steps.push(format!("Limits: {}.", setup.upload_policy));
    steps.join("\n")
}

pub(crate) fn setup_checklist_line_text(item: &SetupChecklistLineText<'_>) -> String {
    let marker_width = item.marker.chars().count();
    let detail_width = item.max_width.saturating_sub(marker_width);
    let detail = fit_line_to_width(&format!(" {} - {}", item.label, item.detail), detail_width);
    format!("{}{}", item.marker, detail)
}

pub(crate) fn setup_console_text(setup: &SetupConsoleText<'_>) -> String {
    let mut text = String::from("setup:\n");
    push_indented_block(&mut text, setup.checklist);
    text.push('\n');
    text.push_str("addresses:\n");
    push_indented_block(&mut text, setup.addresses);
    text.push('\n');
    text.push_str("next steps:\n");
    push_indented_block(&mut text, setup.next_steps);
    text
}

fn push_indented_block(text: &mut String, block: &str) {
    for line in block.lines() {
        if line.is_empty() {
            text.push('\n');
        } else {
            text.push_str("  ");
            text.push_str(line);
            text.push('\n');
        }
    }
}

pub(crate) fn overview_operator_summary_text(summary: &OverviewOperatorSummaryText<'_>) -> String {
    let launch = if summary.ready == summary.total {
        "ready for live testing".to_string()
    } else {
        format!("needs setup ({}/{} ready)", summary.ready, summary.total)
    };
    let next = summary
        .first_missing_label
        .map(|label| format!("{label}: {}", setup_action_for_label(label)))
        .unwrap_or_else(|| "verify Monitoring, then copy invite/portal from Portal".into());
    [
        "overview:".to_string(),
        format!("  launch: {launch}"),
        format!("  live: {}", summary.live_line),
        format!("  network: {}", summary.interface_summary),
        format!("  rooms: {} active", summary.room_count),
        format!(
            "  uploads: max {}, quota {}",
            summary.upload_max, summary.upload_quota
        ),
        "  share: Portal tab has the omenchat:// invite and NomadNet URL".to_string(),
        format!("  next: {next}"),
    ]
    .join("\n")
}

pub(crate) fn portal_panel_text(portal: &PortalPanelText<'_>) -> String {
    format!(
        "share:\n  chat invite: omenchat:// URI\n  portal page: NomadNet /page/index.mu URL\n\nuse portal for: MOTD, rules, help, launch links\nchat traffic: stays on OMENchat\nedit file: reticulum/storage/pages/index.mu\nserved path: /page/index.mu\n\n{checklist}\n\naddresses:\n{destination}\npage file: {page_state}\nMOTD: {motd}",
        checklist = portal.checklist,
        destination = portal.destination,
        page_state = portal.page_state,
        motd = portal.motd,
    )
}

pub(crate) fn identity_panel_text(identity: &IdentityPanelText<'_>) -> String {
    format!(
        "identity:\n  file: {identity_file}\n  backup: copy this file before public testing\n  storage root: {storage_root}\n  isolation: standalone omenchatd storage\n  safety: never overwrite identity material\n\n{checklist}\n\ndestinations:\n{destinations}\n\npaths:\n  identity: {identity_file}\n  database: {database_path}\n  reticulum: {reticulum_path}\n  reticulum config: {reticulum_config_path}\n\nstorage rule: omenchatd owns this root and does not use ~/.reticulum, ~/.nomadnetwork, or ~/.lxmd unless configured explicitly.",
        identity_file = identity.identity_file,
        storage_root = identity.storage_root,
        checklist = identity.checklist,
        destinations = identity.destinations,
        database_path = identity.database_path,
        reticulum_path = identity.reticulum_path,
        reticulum_config_path = identity.reticulum_config_path,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_console_help_lists_upload_policy_commands() {
        let text = command_help_text();

        assert!(text.contains("show current config, rooms, addresses, and limits"));
        assert!(text.contains("show first-run checklist, join addresses, and upload policy"));
        assert!(text.contains("list known users, moderation ids, and stale-delete state"));
        assert!(text.contains(
            "delete one inactive stale known-user record older than 24h; run users first"
        ));
        assert!(text.contains("prune-stale-users"));
        assert!(text.contains("delete all inactive stale known-user records older than 24h"));
        assert!(text.contains("set-upload-quota-bytes <bytes|0>"));
        assert!(text.contains("set per-identity upload quota"));
        assert!(text.contains("set-upload-max-file-bytes <bytes>"));
        assert!(text.contains("reject individual files above this size"));
    }

    #[test]
    fn admin_help_text_documents_dashboard_limit_edits() {
        let text = admin_help_text();

        assert!(text.contains("First Run Checklist"));
        assert!(text.contains("choose Connect Gateway for normal RNS"));
        assert!(text.contains("confirm Monitoring shows a connected interface"));
        assert!(text.contains("Test from a second isolated OMENbrowser_rs app root"));
        assert!(text.contains("repeated reconnects"));
        assert!(text.contains("normally ~/.omenchatd"));
        assert!(text.contains("reticulum/storage/pages/index.mu"));
        assert!(text.contains("OMENchat announces as omenchat.node"));
        assert!(text.contains("Operators should not change that service type"));
        assert!(text.contains("Users join chat with omenchat://<destination_hash>"));
        assert!(text.contains("nomadnetwork.node"));
        assert!(text.contains("Traffic And Upload Policy"));
        assert!(text.contains("active links, recent closes, request mix"));
        assert!(text.contains("The client ping interval is configurable"));
        assert!(text.contains("50 MiB quota and 512 KiB per file"));
        assert!(text.contains("Set upload quota to 0 to disable uploads"));
        assert!(text.contains("Use Setup/Overview actions"));
        assert!(text.contains("g / x: start or stop the live server"));
        assert!(text.contains("Connect To Gateway config"));
        assert!(text.contains("w: write a Connect To Gateway"));
        assert!(text.contains("PageUp / PageDown"));
        assert!(text.contains("Admins can create/archive rooms"));
        assert!(text.contains("Moderators can change topics"));
        assert!(text.contains("Reticulum identity, not by transient Link ids"));
        assert!(
            text.contains("Confirm Delete Record only removes an inactive known-user database row")
        );
        assert!(text.contains(
            "Prune Inactive Stale / Confirm Prune Records removes inactive stale known-user rows"
        ));
        assert!(text.contains("Overview shows the compact dashboard"));
        assert!(text.contains("Setup and line-console commands expose the detailed limit controls"));
        assert!(text.contains("omenchatd config set"));
        assert!(text.contains("OMENchat Slash Commands"));
        assert!(text.contains("/me <action>: send an action-style room message"));
        assert!(text.contains("/topic <topic>: change the active room topic; moderator or admin"));
        assert!(text.contains("/create <room> [topic]: create a room; admin only"));
        assert!(text.contains("/role <user> <standard|trusted|mod|admin>: change role; admin only"));
        assert!(text.contains("/upload <path>: offer a file upload"));
    }

    #[test]
    fn audit_summary_counts_admin_action_categories() {
        let text = "admin updated server name\nadmin archived room id=2\nadmin console banned user id=7\nadmin pruned stale user id=8\nadmin wrote TCPClientInterface target=gateway.example:42420\n";

        assert_eq!(
            audit_summary_text(text),
            "summary: 5 action(s) | config 1 | interfaces 1 | rooms 1 | moderation 1 | stale cleanup 1"
        );
        assert_eq!(
            audit_summary_text(""),
            "summary: no admin actions in this view"
        );
    }

    #[test]
    fn interface_operator_summary_explains_gateway_and_local_modes() {
        let path = std::path::Path::new("/tmp/omenchatd/reticulum/config");
        let gateway = interface_operator_summary_text(
            "[interfaces]\n[[interfaces.gateway]]\ntype = TCPClientInterface\ntarget_host = example.net\ntarget_port = 42420\ninterface_enabled = true\nnetwork_name = private_ret\npassphrase = test-passphrase\n",
            path,
        );
        assert!(gateway.contains("interface setup"));
        assert!(gateway.contains("TCP gateway client -> example.net:42420"));
        assert!(gateway.contains("IFAC: network_name=private_ret; passphrase set"));
        assert!(!gateway.contains("test-passphrase"));
        assert!(gateway.contains("verify Monitoring shows the gateway connected"));

        let local = interface_operator_summary_text(
            "[interfaces]\n[[interfaces.local]]\ntype = TCPServerInterface\nlisten_ip = 127.0.0.1\nlisten_port = 42420\ninterface_enabled = true\n",
            path,
        );
        assert!(local.contains("local TCP server listener -> 127.0.0.1:42420"));
        assert!(local.contains("verify Monitoring shows incoming clients"));
        assert!(local.contains("IFAC: not configured"));

        let empty = interface_operator_summary_text("", path);
        assert!(empty.contains("Connect Gateway"));
        assert!(empty.contains("interface edits do not overwrite the server identity"));
    }

    #[test]
    fn overview_limits_text_reports_operator_tunable_limits() {
        let mut config =
            ServerConfig::for_root(std::env::temp_dir().join("omenchatd-tui-text-limits-text"));
        config.limits.max_message_bytes = 4096;
        config.limits.history_batch_size = 25;
        config.limits.join_backlog_events = 12;
        config.limits.large_batch_threshold_bytes = 8192;
        config.limits.rate_messages_per_minute = 33;
        config.limits.rate_commands_per_minute = 17;
        config.upload_quota_bytes = 50 * 1024 * 1024;
        config.upload_max_file_bytes = 512 * 1024;

        let text = server_limits_text(&config);

        assert!(text.contains("message size: 4.00 KiB"));
        assert!(text.contains("history batch: 25 event(s)"));
        assert!(text.contains("join backlog: 12 event(s)"));
        assert!(text.contains("large batch threshold: 8.00 KiB"));
        assert!(text.contains("uploads: max file 512.0 KiB; quota 50.0 MiB per identity"));
        assert!(text.contains("message rate: 33 / minute"));
        assert!(text.contains("command rate: 17 / minute"));
    }

    #[test]
    fn upload_policy_hint_reports_disabled_uploads() {
        let mut config = ServerConfig::for_root(
            std::env::temp_dir().join("omenchatd-tui-text-upload-policy-disabled"),
        );
        config.upload_quota_bytes = 0;

        let text = upload_policy_hint(&config);

        assert_eq!(text, "uploads: disabled; upload offers are rejected");
    }

    #[test]
    fn upload_quota_update_text_reports_disabled_and_human_bytes() {
        assert_eq!(
            upload_quota_update_text(0),
            "upload quota updated: uploads disabled"
        );
        assert_eq!(
            upload_quota_update_text(512 * 1024),
            "upload quota updated: 512.0 KiB"
        );
    }

    #[test]
    fn upload_max_file_update_text_reports_human_bytes() {
        assert_eq!(
            upload_max_file_update_text(512 * 1024),
            "upload max file updated: 512.0 KiB"
        );
    }

    #[test]
    fn ping_interval_update_text_reports_seconds() {
        assert_eq!(
            ping_interval_update_text(30),
            "ping interval updated: 30 second(s)"
        );
    }

    #[test]
    fn announce_interval_update_text_reports_minutes() {
        assert_eq!(
            announce_interval_update_text(360),
            "announce interval updated: 360 minute(s)"
        );
    }

    #[test]
    fn max_message_bytes_update_text_reports_human_limit() {
        assert_eq!(
            max_message_bytes_update_text(4096),
            "max message size updated: 4.00 KiB"
        );
    }

    #[test]
    fn history_batch_size_update_text_reports_count() {
        assert_eq!(
            history_batch_size_update_text(50),
            "history batch size updated: 50"
        );
    }

    #[test]
    fn join_backlog_events_update_text_reports_count() {
        assert_eq!(
            join_backlog_events_update_text(25),
            "join backlog events updated: 25"
        );
    }

    #[test]
    fn large_batch_threshold_update_text_reports_human_bytes() {
        assert_eq!(
            large_batch_threshold_update_text(8192),
            "large batch threshold updated: 8.00 KiB"
        );
    }

    #[test]
    fn rate_update_text_reports_per_minute_counts() {
        assert_eq!(
            message_rate_update_text(60),
            "message rate updated: 60 per minute"
        );
        assert_eq!(
            command_rate_update_text(30),
            "command rate updated: 30 per minute"
        );
    }

    #[test]
    fn line_console_room_update_text_normalizes_room_names_and_ids() {
        assert_eq!(room_ready_update_text("#ops"), "room ready: #ops");
        assert_eq!(room_ready_update_text("  #help  "), "room ready: #help");
        assert_eq!(room_topic_update_text(7), "room topic updated: id=7");
        assert_eq!(room_archived_update_text(9), "room archived: id=9");
    }

    #[test]
    fn line_console_server_metadata_update_text_is_stable() {
        assert_eq!(server_name_update_text(), "server name updated");
        assert_eq!(operator_label_update_text(), "operator label updated");
        assert_eq!(motd_update_text(), "MOTD updated");
    }

    #[test]
    fn line_console_user_moderation_update_text_names_action_and_user_id() {
        assert_eq!(user_banned_update_text(12), "user banned: id=12");
        assert_eq!(user_unbanned_update_text(12), "user unbanned: id=12");
        assert_eq!(user_muted_update_text(13), "user muted: id=13");
        assert_eq!(user_unmuted_update_text(13), "user unmuted: id=13");
        assert_eq!(user_trusted_update_text(14), "user trusted: id=14");
        assert_eq!(user_untrusted_update_text(14), "user untrusted: id=14");
        assert_eq!(
            user_role_update_text(15, "moderator"),
            "user role updated: id=15 role=moderator"
        );
    }

    #[test]
    fn room_console_row_text_formats_line_console_room_rows() {
        let row = room_console_row_text(&RoomConsoleRowText {
            room_id: 42,
            name: "operations",
            topic: "Ops bridge",
        });

        assert_eq!(row, "  #operations           id=42   Ops bridge\n");
    }

    #[test]
    fn selected_room_text_explains_lobby_and_archive_policy() {
        let lobby = selected_room_text(1, "lobby", Some("Default OMENchat lobby"));
        assert!(lobby.contains("room: #lobby"));
        assert!(lobby.contains("topic: Default OMENchat lobby"));
        assert!(lobby.contains("topic: mod/admin"));
        assert!(lobby.contains("protected; #lobby stays available"));

        let ops = selected_room_text(2, "ops", None);
        assert!(ops.contains("topic: (no topic)"));
        assert!(ops.contains("admin archive available after confirmation"));
        assert!(ops.contains("active rooms appear in /rooms"));
    }

    #[test]
    fn room_action_guide_explains_selected_room_actions() {
        let empty = room_action_guide_text(None, None);
        assert!(empty.contains("Select a room or Add Room"));
        assert!(empty.contains("Admin: create/archive rooms"));

        let lobby =
            room_action_guide_text(Some((1, "lobby", Some("Default OMENchat lobby"))), None);
        assert!(lobby.contains("Selected: #lobby"));
        assert!(lobby.contains("Edit Topic changes the topic"));
        assert!(lobby.contains("Archive disabled: #lobby"));
        assert!(lobby.contains("admin create/archive; mod/admin topic"));

        let room = room_action_guide_text(Some((2, "ops", None)), None);
        assert!(room.contains("Edit Topic sets the missing topic"));
        assert!(room.contains("asks for one confirmation click"));

        let confirm = room_action_guide_text(Some((2, "ops", Some("Ops room"))), Some(2));
        assert!(confirm.contains("Confirm Archive hides the room"));
        assert!(confirm.contains("history stays stored"));
    }

    #[test]
    fn room_list_label_text_formats_topic_and_fits_width() {
        let label = room_list_label_text(&RoomListLabelText {
            marker: ">",
            room_id: 42,
            name: "long-room-name",
            topic: Some("A very long topic that would otherwise overflow the room list"),
            max_width: 24,
        });

        assert!(label.chars().count() <= 24);
        assert!(label.starts_with("> #long-room-name"));
        assert!(label.ends_with("..."));

        let no_topic = room_list_label_text(&RoomListLabelText {
            marker: " ",
            room_id: 1,
            name: "lobby",
            topic: Some("   "),
            max_width: 80,
        });
        assert_eq!(no_topic, "  #lobby id=1");
    }

    #[test]
    fn moderation_selected_user_summary_explains_active_link_delete_block() {
        let user = ModerationUserText {
            user_id: 11,
            identity_hex: "70656572",
            display_name: "Peer",
            role_label: "trusted",
            status_label: "allowed",
            lxmf_destination: Some("lxmf-peer"),
            first_seen_at: 1,
            last_seen_at: Some(2),
            trusted: true,
            banned: false,
            muted: false,
        };

        let text = moderation_selected_user_text(&user, 2, "eligible; last seen 1d");

        assert!(text.contains("user: Peer"));
        assert!(text.contains("access: allowed"));
        assert!(text.contains("allowed: user can join, read, and send messages"));
        assert!(text.contains("2 active link(s)"));
        assert!(text.contains("blocked while active links exist"));
        assert!(text.contains("trusted media: yes"));
    }

    #[test]
    fn moderation_selected_user_summary_explains_send_blocks() {
        let mut user = ModerationUserText {
            user_id: 12,
            identity_hex: "70656572",
            display_name: "Peer",
            role_label: "standard",
            status_label: "muted",
            lxmf_destination: None,
            first_seen_at: 1,
            last_seen_at: Some(2),
            trusted: false,
            banned: false,
            muted: true,
        };

        let muted = moderation_selected_user_text(&user, 0, "available in 1h");
        assert!(muted.contains("limited: muted users can join/read"));
        assert!(muted.contains("no active links"));

        user.banned = true;
        user.muted = false;
        user.status_label = "banned";
        let banned = moderation_selected_user_text(&user, 0, "available in 1h");
        assert!(banned.contains("blocked: banned users cannot open sessions"));
    }

    #[test]
    fn moderation_action_guide_explains_selected_user_actions() {
        let mut user = ModerationUserText {
            user_id: 12,
            identity_hex: "70656572",
            display_name: "Peer",
            role_label: "mod",
            status_label: "allowed",
            lxmf_destination: None,
            first_seen_at: 1,
            last_seen_at: Some(2),
            trusted: true,
            banned: false,
            muted: false,
        };

        let text = moderation_action_guide_text(Some(&user), 2, None, false);
        assert!(text.contains("Ban blocks future sessions and closes active links"));
        assert!(text.contains("Kick closes current links only"));
        assert!(text.contains("Mute keeps read access"));
        assert!(text.contains("Untrust removes trusted-media handling"));
        assert!(text.contains("Role: moderator"));
        assert!(text.contains("Delete stale record blocked while active"));

        user.banned = true;
        user.muted = true;
        user.status_label = "banned";
        let text = moderation_action_guide_text(Some(&user), 0, Some(user.user_id), true);
        assert!(text.contains("Unban allows future sessions"));
        assert!(text.contains("Unmute restores sending"));
        assert!(text.contains("Confirm Delete removes only this known-user record"));
        assert!(text.contains("Confirm Prune removes all inactive stale"));

        let empty = moderation_action_guide_text(None, 0, None, false);
        assert!(empty.contains("Select a user to moderate"));
    }

    #[test]
    fn moderation_user_list_label_surfaces_stale_delete_state() {
        let mut user = ModerationUserText {
            user_id: 7,
            identity_hex: "70656572",
            display_name: "Peer",
            role_label: "mod",
            status_label: "muted",
            lxmf_destination: None,
            first_seen_at: 1,
            last_seen_at: Some(2),
            trusted: true,
            banned: false,
            muted: true,
        };

        let label = moderation_user_list_label(">", &user, 90_000, 86_400, 2, 160);

        assert!(label.contains("> Peer"));
        assert!(label.contains("muted"));
        assert!(label.contains("mod"));
        assert!(label.contains("active=2"));
        assert!(label.contains("delete ok"));
        assert!(label.contains("last seen"));

        user.last_seen_at = Some(100_000);
        let label = moderation_user_list_label(" ", &user, 0, 86_400, 0, 160);

        assert!(label.contains("delete in"));
        assert!(label.contains("active=0"));
    }

    #[test]
    fn user_console_row_text_formats_line_console_user_rows() {
        let text = user_console_row_text(&UserConsoleRowText {
            user_id: 12,
            display_name: "Old Peer",
            role_label: "mod",
            status_label: "muted",
            first_seen: "2026-05-20 12:00:00 UTC",
            last_seen: "2026-05-21 12:00:00 UTC",
            stale_delete: "eligible; last seen 1d",
            identity_hex: "70656572",
            lxmf_destination: "lxmf-old",
        });

        assert!(text.contains("id=12"));
        assert!(text.contains("Old Peer"));
        assert!(text.contains("role=mod"));
        assert!(text.contains("status=muted"));
        assert!(text.contains("first=2026-05-20"));
        assert!(text.contains("last=2026-05-21"));
        assert!(text.contains("stale_delete=\"eligible; last seen 1d\""));
        assert!(text.contains("identity=70656572 lxmf=lxmf-old"));
    }

    #[cfg(any(test, feature = "live-rns-net"))]
    #[test]
    fn upload_transfer_summary_reports_idle_and_activity() {
        assert_eq!(upload_transfer_summary(&LiveServerStats::default()), "idle");
        let stats = LiveServerStats {
            upload_offers_in: 2,
            upload_fetches_in: 3,
            upload_resources_in: 1,
            upload_resource_bytes_in: 2048,
            upload_inline_chunks_out: 4,
            upload_inline_bytes_out: 4096,
            upload_resource_offers_out: 5,
            ..LiveServerStats::default()
        };

        let text = upload_transfer_summary(&stats);

        assert!(text.contains("offer 2"));
        assert!(text.contains("fetch 3"));
        assert!(text.contains("stored 1 (2.00 KiB)"));
        assert!(text.contains("inline 4 (4.00 KiB)"));
        assert!(text.contains("resource offers 5"));
    }

    #[cfg(any(test, feature = "live-rns-net"))]
    #[test]
    fn interface_health_label_classifies_operator_states() {
        assert_eq!(
            interface_health_label(&[]),
            "waiting for Reticulum interface stats"
        );
        assert_eq!(
            interface_health_label(&["interfaces: 0 | transport: false".into()]),
            "no interfaces visible; configure a gateway or local TCP server"
        );
        assert_eq!(
            interface_health_label(&["Gateway | connected=true | rx=1 KiB".into()]),
            "connected; server can publish and receive"
        );
        assert_eq!(
            interface_health_label(&["query failed: timeout".into()]),
            "stats unavailable; check runtime logs"
        );
        assert_eq!(
            interface_health_label(&["Gateway | connected=false".into()]),
            "disconnected; watchdog will rebuild runtime after repeated samples"
        );
    }

    #[cfg(any(test, feature = "live-rns-net"))]
    #[test]
    fn closed_link_status_label_classifies_operator_reasons() {
        assert_eq!(
            closed_link_status_label("duplicate identity link replaced"),
            "normal reconnect"
        );
        assert_eq!(
            closed_link_status_label("Timeout"),
            "timeout; watch if repeated"
        );
        assert_eq!(
            closed_link_status_label("DestinationClosed"),
            "peer/runtime closed"
        );
        assert_eq!(
            closed_link_status_label("admin kick active link"),
            "moderation close"
        );
        assert_eq!(closed_link_status_label("unspecified"), "unknown close");
        assert_eq!(closed_link_status_label("decode failed"), "check logs");
    }

    #[cfg(any(test, feature = "live-rns-net"))]
    #[test]
    fn closed_link_churn_summary_groups_operator_reasons() {
        assert_eq!(
            closed_link_churn_summary(&[]),
            "recent close summary: none"
        );

        let summary = closed_link_churn_summary(&[
            "duplicate identity link replaced",
            "DestinationClosed",
            "admin kick active link",
            "Timeout",
            "decode failed",
        ]);

        assert!(summary.contains("5 total"));
        assert!(summary.contains("normal reconnect 1"));
        assert!(summary.contains("peer/runtime 1"));
        assert!(summary.contains("moderation 1"));
        assert!(summary.contains("timeout 1"));
        assert!(summary.contains("investigate 1"));
    }

    #[cfg(any(test, feature = "live-rns-net"))]
    #[test]
    fn active_link_activity_label_flags_noisy_clients() {
        let link = ActiveLinkSummary {
            display_name: "tester".into(),
            identity_hash: vec![1, 2, 3, 4],
            link_id: [0x55; 16],
            room_id: Some(1),
            connected_at_unix: 1_000,
            traffic: crate::live::LinkTrafficSummary {
                frames_in: 180,
                bytes_in: 60 * 1024,
                history_requests: 25,
                pings: 40,
                upload_requests: 12,
                ..crate::live::LinkTrafficSummary::default()
            },
        };

        let label = active_link_activity_label(&link, 1_060).expect("label");

        assert!(label.contains("180.0 frames/min"));
        assert!(label.contains("60.0 KiB/min"));
        assert!(label.contains("high frames"));
        assert!(label.contains("high history"));
        assert!(label.contains("high ping"));
        assert!(label.contains("high upload"));
    }

    #[cfg(any(test, feature = "live-rns-net"))]
    #[test]
    fn traffic_delta_text_reports_recent_activity_mix() {
        let previous = LiveServerStats {
            bytes_in: 1024,
            bytes_out: 2048,
            frames_in: 2,
            frames_out: 4,
            chat_messages_in: 1,
            pings_in: 5,
            ..LiveServerStats::default()
        };
        let current = LiveServerStats {
            bytes_in: 3072,
            bytes_out: 4096,
            resource_bytes_out: 1024,
            frames_in: 5,
            frames_out: 8,
            session_requests_in: 1,
            room_navigation_in: 2,
            chat_messages_in: 4,
            history_requests_in: 1,
            pings_in: 8,
            protocol_errors: 1,
            ..LiveServerStats::default()
        };

        let text = traffic_delta_text(&previous, &current, 2.0);

        assert!(text.contains("2.0s sample"));
        assert!(text.contains("rate rx 1.00 KiB/s (2.00 KiB)"));
        assert!(text.contains("tx 1.00 KiB/s (2.00 KiB)"));
        assert!(text.contains("resources 512 B/s (1.00 KiB)"));
        assert!(text.contains(
            "requests: session 1 room 2 chat 3 history 1 ping 3 | uploads: fetch 0 inline 0 B | problems: 1"
        ));
    }

    #[cfg(any(test, feature = "live-rns-net"))]
    #[test]
    fn server_health_label_summarizes_recent_link_churn() {
        let stats = LiveServerStats {
            active_links: 1,
            frames_in: 4,
            frames_out: 4,
            ..LiveServerStats::default()
        };

        assert_eq!(
            server_health_label(&stats, &["duplicate identity link replaced"]),
            "server health: ok; recent closes look explainable"
        );
        assert_eq!(
            server_health_label(&stats, &["Timeout"]),
            "server health: watch link churn; 1 notable close(s)"
        );

        let idle = LiveServerStats::default();
        assert_eq!(
            server_health_label(&idle, &[]),
            "server health: idle; waiting for clients"
        );

        let problematic = LiveServerStats {
            ignored_packets: 1,
            unknown_link_packets: 2,
            protocol_errors: 3,
            ..LiveServerStats::default()
        };
        assert_eq!(
            server_health_label(&problematic, &[]),
            "server health: check logs; 6 problem counter(s)"
        );
    }

    #[cfg(any(test, feature = "live-rns-net"))]
    #[test]
    fn monitoring_operator_summary_reports_stopped_and_live_health() {
        let stopped = monitoring_operator_summary_text(None, &[], "waiting for next sample", &[]);
        assert!(stopped.contains("state: stopped"));
        assert!(stopped.contains("Start Live Server"));

        let stats = LiveServerStats {
            active_links: 2,
            links_opened: 3,
            links_closed: 1,
            frames_in: 10,
            frames_out: 12,
            bytes_in: 2048,
            bytes_out: 4096,
            upload_offers_in: 1,
            upload_fetches_in: 2,
            upload_inline_chunks_out: 3,
            upload_inline_bytes_out: 1536,
            upload_resources_in: 1,
            upload_resource_bytes_in: 512,
            upload_resource_offers_out: 4,
            ..LiveServerStats::default()
        };
        let text = monitoring_operator_summary_text(
            Some(&stats),
            &[
                "interfaces: 1 | transport: false | received: 2.00 KiB | sent: 4.00 KiB".into(),
                "Gateway [1] TCPClientInterface | connected=true | rx=2.00 KiB in 2 pkt | tx=4.00 KiB in 4 pkt | ifac=none".into(),
            ],
            "5.0s sample | rate rx 1 B/s (5 B)",
            &[],
        );

        assert!(text.contains("server health: ok"));
        assert!(text.contains("monitoring:"));
        assert!(text.contains(
            "clients active=2 opened=3 closed=1 | interface: connected; server can publish and receive"
        ));
        assert!(text.contains("server can publish and receive"));
        assert!(text.contains("traffic: 10 frame(s) in / 12 out"));
        assert!(text.contains("requests session=0 room=0 chat=0 history=0 ping=0 command=0"));
        assert!(text.contains(
            "uploads offer 1 | fetch 2 | stored 1 (512 B) | inline 3 (1.50 KiB) | resource offers 4"
        ));
        assert!(text.contains("health: ok"));
    }

    #[cfg(any(test, feature = "live-rns-net"))]
    #[test]
    fn monitoring_operator_summary_flags_problem_counters() {
        let stats = LiveServerStats {
            ignored_packets: 1,
            unknown_link_packets: 2,
            protocol_errors: 3,
            ..LiveServerStats::default()
        };
        let text = monitoring_operator_summary_text(
            Some(&stats),
            &["interfaces: 0 | transport: false | received: 0 B | sent: 0 B".into()],
            "",
            &[],
        );

        assert!(text.contains("server health: check logs; 6 problem counter(s)"));
        assert!(text.contains("interface: no interfaces visible"));
        assert!(text.contains("health: check problem counters (6 total)"));
    }

    #[cfg(any(test, feature = "live-rns-net"))]
    #[test]
    fn active_link_monitoring_line_groups_identity_rate_and_requests() {
        let line = active_link_monitoring_line(&ActiveLinkMonitoringText {
            name: "Alice".into(),
            identity: "70656572".into(),
            room: "#lobby".into(),
            age: "1m".into(),
            activity: "inbound rate: 12.0 frames/min, 4.0 KiB/min | normal".into(),
            frames: 12,
            bytes: "4.00 KiB".into(),
            history_requests: 2,
            pings: 3,
            chat_messages: 4,
            commands: 5,
            upload_requests: 6,
            link_id: "aaaaaaaa".into(),
        });

        assert!(line.contains("Alice [70656572] #lobby"));
        assert!(line.contains("age 1m"));
        assert!(line.contains("inbound rate:"));
        assert!(line.contains("rx 12 frame(s), 4.00 KiB"));
        assert!(line.contains("req h=2 p=3 chat=4 cmd=5 up=6"));
        assert!(line.contains("link aaaaaaaa"));
    }

    #[cfg(any(test, feature = "live-rns-net"))]
    #[test]
    fn closed_link_monitoring_line_groups_status_and_reason() {
        let line = closed_link_monitoring_line(&ClosedLinkMonitoringText {
            name: "Alice".into(),
            identity: "70656572".into(),
            room: "#lobby".into(),
            status: "normal reconnect".into(),
            connected_for: "1m".into(),
            closed_ago: "1m".into(),
            link_id: "bbbbbbbb".into(),
            reason: "duplicate identity link replaced".into(),
        });

        assert!(line.contains("Alice [70656572] #lobby"));
        assert!(line.contains("normal reconnect"));
        assert!(line.contains("connected 1m"));
        assert!(line.contains("closed 1m ago"));
        assert!(line.contains("link bbbbbbbb"));
        assert!(line.contains("reason duplicate identity link replaced"));
    }

    #[test]
    fn setup_label_helpers_name_operator_actions() {
        assert!(setup_advice_for_label("server name").contains("Edit Server Name"));
        assert_eq!(
            setup_action_for_label("reticulum"),
            "Use: Connect To Gateway for normal hosting, or Local TCP Listener for local tests."
        );
        assert!(setup_advice_for_label("unknown").contains("Review"));
        assert!(setup_action_for_label("unknown").contains("review"));
    }

    #[test]
    fn setup_launch_status_text_reports_ready_and_next_missing() {
        let ready = setup_launch_status_text(&SetupLaunchText {
            ready: 8,
            total: 8,
            first_missing_label: None,
        });
        assert!(ready.contains("READY for live testing"));
        assert!(ready.contains("share the omenchat:// URI"));

        let missing = setup_launch_status_text(&SetupLaunchText {
            ready: 5,
            total: 8,
            first_missing_label: Some("reticulum"),
        });
        assert!(missing.contains("NEEDS SETUP (5/8 ready)"));
        assert!(missing.contains("reticulum - Use Connect To Gateway"));

        let fallback = setup_launch_status_text(&SetupLaunchText {
            ready: 0,
            total: 8,
            first_missing_label: None,
        });
        assert!(fallback.contains("review setup checklist"));
    }

    #[test]
    fn setup_addresses_text_keeps_join_and_portal_guidance() {
        let text = setup_addresses_text(&SetupAddressesText {
            public_addresses: "destination: omenchat.node (abc123)\nclient uri: omenchat://abc123\nportal url: def456:/page/index.mu\n",
            portal_page_file: "/tmp/omenchatd/reticulum/storage/pages/index.mu",
        });

        assert!(text.contains("Share after Monitoring shows connected:"));
        assert!(text.contains("OMENchat invite: client uri"));
        assert!(text.contains("MOTD/rules page: portal url"));
        assert!(text.contains("destination: omenchat.node (abc123)"));
        assert!(text.contains("client uri: omenchat://abc123"));
        assert!(text.contains("portal page file: /tmp/omenchatd/reticulum/storage/pages/index.mu"));
        assert!(text.contains("chat URI format: omenchat://<destination_hash>"));
    }

    #[test]
    fn setup_addresses_text_inserts_newline_before_portal_file() {
        let text = setup_addresses_text(&SetupAddressesText {
            public_addresses: "destination: unavailable",
            portal_page_file: "/tmp/index.mu",
        });

        assert!(text.contains("destination: unavailable\nportal page file: /tmp/index.mu"));
    }

    #[test]
    fn setup_next_steps_text_formats_ready_launch_steps() {
        let text = setup_next_steps_text(&SetupNextStepsText {
            launch_status: "Launch status: READY for live testing",
            all_ready: true,
            missing_labels: Vec::new(),
            storage_root: "/tmp/omenchatd",
            reticulum_summary: "TCP gateway client -> gateway.example:42420",
            upload_policy: "uploads: max file 512.0 KiB; quota 50.0 MiB per identity",
        });

        assert!(text.contains("Launch status: READY for live testing"));
        assert!(text.contains("1. Start Live or press g."));
        assert!(text.contains("3. Announce Now"));
        assert!(text.contains("4. Share the omenchat:// invite or NomadNet portal URL"));
        assert!(text.contains("Limits: uploads: max file 512.0 KiB"));
        assert!(text.contains("Network: TCP gateway client -> gateway.example:42420"));
        assert!(!text.contains("Fix first, then Start Live:"));
    }

    #[test]
    fn setup_next_steps_text_formats_missing_items_and_storage_rule() {
        let text = setup_next_steps_text(&SetupNextStepsText {
            launch_status: "Launch status: NEEDS SETUP (6/8 ready)",
            all_ready: false,
            missing_labels: vec!["reticulum", "operator"],
            storage_root: "/tmp/omenchatd",
            reticulum_summary: "no active interface configured",
            upload_policy: "uploads: disabled; upload offers are rejected",
        });

        assert!(text.contains("Launch status: NEEDS SETUP (6/8 ready)"));
        assert!(text.contains("Fix first, then Start Live:"));
        assert!(text.contains("- reticulum: Use: Connect To Gateway"));
        assert!(text.contains("- operator: Use: Edit Operator Label"));
        assert!(text.contains("Storage: server files stay under this omenchatd home."));
        assert!(text.contains("home: /tmp/omenchatd"));
        assert!(text.contains("share omenchat:// for chat"));
        assert!(text.contains("Limits: uploads: disabled; upload offers are rejected."));
    }

    #[test]
    fn setup_checklist_line_text_fits_marker_label_and_detail() {
        let line = setup_checklist_line_text(&SetupChecklistLineText {
            marker: "[x]",
            label: "reticulum",
            detail: "TCP gateway client -> very-long-gateway.example:42420",
            max_width: 28,
        });

        assert!(line.chars().count() <= 28);
        assert!(line.starts_with("[x] reticulum"));
        assert!(line.ends_with("..."));

        let tiny = setup_checklist_line_text(&SetupChecklistLineText {
            marker: "[ ]",
            label: "identity",
            detail: "missing identity",
            max_width: 2,
        });
        assert_eq!(tiny, "[ ]");
    }

    #[test]
    fn setup_console_text_formats_line_console_sections() {
        let text = setup_console_text(&SetupConsoleText {
            checklist: "[x] database           ready\n[ ] reticulum          missing\n",
            addresses: "client uri: omenchat://abc\nportal url: def:/page/index.mu\n",
            next_steps:
                "Launch status: NEEDS SETUP\n\nWork these in order:\n- reticulum: connect\n",
        });

        assert!(text.starts_with("setup:\n  [x] database"));
        assert!(text.contains("\naddresses:\n  client uri: omenchat://abc"));
        assert!(text.contains("  portal url: def:/page/index.mu"));
        assert!(
            text.contains("\nnext steps:\n  Launch status: NEEDS SETUP\n\n  Work these in order:")
        );
        assert!(text.contains("  - reticulum: connect\n"));
    }

    #[test]
    fn overview_operator_summary_text_formats_snapshot_without_config_reads() {
        let text = overview_operator_summary_text(&OverviewOperatorSummaryText {
            ready: 5,
            total: 8,
            first_missing_label: Some("reticulum"),
            live_line: "runtime: live server running",
            interface_summary: "TCP gateway client -> example.net:42420",
            room_count: 2,
            upload_max: "512.0 KiB".into(),
            upload_quota: "50.0 MiB".into(),
        });

        assert!(text.contains("overview:"));
        assert!(text.contains("launch: needs setup (5/8 ready)"));
        assert!(text.contains("live: runtime: live server running"));
        assert!(text.contains("network: TCP gateway client"));
        assert!(text.contains("rooms: 2 active"));
        assert!(text.contains("uploads: max 512.0 KiB, quota 50.0 MiB"));
        assert!(text.contains("next: reticulum: Use: Connect To Gateway"));

        let ready = overview_operator_summary_text(&OverviewOperatorSummaryText {
            ready: 8,
            total: 8,
            first_missing_label: None,
            live_line: "runtime: stopped",
            interface_summary: "no active interface configured",
            room_count: 1,
            upload_max: "512.0 KiB".into(),
            upload_quota: "disabled".into(),
        });
        assert!(ready.contains("ready for live testing"));
        assert!(ready.contains("next: verify Monitoring, then copy invite/portal from Portal"));
    }

    #[test]
    fn portal_panel_text_formats_public_address_guidance() {
        let text = portal_panel_text(&PortalPanelText {
            checklist:
                "portal readiness:\n  page: portal page exists\n  motd: MOTD is set\n  publish: verify Monitoring",
            destination: "destination: omenchat.node abc123\nnomadnet portal: def456",
            page_state: "/tmp/omenchatd/reticulum/storage/pages/index.mu (1.00 KiB, modified now)",
            motd: "Read the rules",
        });

        assert!(text.contains("share:"));
        assert!(text.contains("chat invite: omenchat:// URI"));
        assert!(text.contains("portal page: NomadNet /page/index.mu URL"));
        assert!(text.contains("use portal for: MOTD, rules, help, launch links"));
        assert!(text.contains("edit file: reticulum/storage/pages/index.mu"));
        assert!(text.contains("served path: /page/index.mu"));
        assert!(text.contains("portal readiness:"));
        assert!(text.contains("destination: omenchat.node abc123"));
        assert!(text.contains("page file: /tmp/omenchatd/reticulum/storage/pages/index.mu"));
        assert!(text.contains("MOTD: Read the rules"));
    }

    #[test]
    fn identity_panel_text_formats_safety_paths_and_destinations() {
        let text = identity_panel_text(&IdentityPanelText {
            identity_file: "/tmp/omenchatd/identity",
            storage_root: "/tmp/omenchatd",
            checklist: "identity safety:\n  state: identity exists\n  backup now: copy /tmp/omenchatd/identity",
            destinations: "  identity hash: abc123\n  destination: omenchat.node (def456)",
            database_path: "/tmp/omenchatd/omenchat.sqlite",
            reticulum_path: "/tmp/omenchatd/reticulum",
            reticulum_config_path: "/tmp/omenchatd/reticulum/config",
        });

        assert!(text.contains("identity:"));
        assert!(text.contains("file: /tmp/omenchatd/identity"));
        assert!(text.contains("backup: copy this file before public testing"));
        assert!(text.contains("storage root: /tmp/omenchatd"));
        assert!(text.contains("isolation: standalone omenchatd storage"));
        assert!(text.contains("safety: never overwrite identity material"));
        assert!(text.contains("identity safety:"));
        assert!(text.contains("destinations:"));
        assert!(text.contains("destination: omenchat.node (def456)"));
        assert!(text.contains("database: /tmp/omenchatd/omenchat.sqlite"));
        assert!(text.contains("reticulum config: /tmp/omenchatd/reticulum/config"));
        assert!(text.contains("does not use ~/.reticulum"));
    }
}
