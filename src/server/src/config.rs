use std::path::PathBuf;
use std::time::SystemTime;

use crate::error::{ServerError, ServerResult};
use crate::session::SessionLimits;
use crate::tui_format::human_bytes;
use crate::{TcpClientOverride, TcpServerOverride};

const PLACEHOLDER_IDENTITY: &[u8] =
    b"OMENCHATD_IDENTITY_PLACEHOLDER\nreplace-with-native-reticulum-identity\n";
pub const OMENCHAT_DESTINATION_ASPECT: &str = "node";
pub const NOMADNET_PORTAL_PATH: &str = "/page/index.mu";
pub const DEFAULT_UPLOAD_QUOTA_BYTES: u64 = 50 * 1024 * 1024;
pub const DEFAULT_UPLOAD_MAX_FILE_BYTES: u64 = 512 * 1024;
pub const DEFAULT_PING_INTERVAL_SECONDS: u64 = 30;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServerConfig {
    pub name: String,
    pub operator_label: String,
    pub config_path: PathBuf,
    pub identity_path: PathBuf,
    pub database_path: PathBuf,
    pub reticulum_config_path: PathBuf,
    pub chat_aspect: String,
    pub nomadnet_page_path: String,
    pub motd: String,
    pub announce_interval_minutes: u64,
    pub limits: ServerLimitsConfig,
    pub upload_quota_bytes: u64,
    pub upload_max_file_bytes: u64,
    pub ping_interval_seconds: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServerLimitsConfig {
    pub max_message_bytes: usize,
    pub history_batch_size: usize,
    pub join_backlog_events: usize,
    pub large_batch_threshold_bytes: usize,
    pub rate_messages_per_minute: usize,
    pub rate_commands_per_minute: usize,
}

impl Default for ServerLimitsConfig {
    fn default() -> Self {
        Self {
            max_message_bytes: 2048,
            history_batch_size: 50,
            join_backlog_events: 50,
            large_batch_threshold_bytes: 4096,
            rate_messages_per_minute: 20,
            rate_commands_per_minute: 12,
        }
    }
}

impl From<&ServerLimitsConfig> for SessionLimits {
    fn from(limits: &ServerLimitsConfig) -> Self {
        Self {
            history_batch_size: limits.history_batch_size,
            join_backlog_events: limits.join_backlog_events,
            large_batch_threshold_bytes: limits.large_batch_threshold_bytes,
            max_message_bytes: limits.max_message_bytes,
            rate_messages_per_minute: limits.rate_messages_per_minute,
            rate_commands_per_minute: limits.rate_commands_per_minute,
            upload_quota_bytes: DEFAULT_UPLOAD_QUOTA_BYTES,
            upload_max_file_bytes: DEFAULT_UPLOAD_MAX_FILE_BYTES,
            upload_cache_root: None,
            ping_interval_seconds: DEFAULT_PING_INTERVAL_SECONDS,
        }
    }
}

impl From<&ServerConfig> for SessionLimits {
    fn from(config: &ServerConfig) -> Self {
        let mut limits = SessionLimits::from(&config.limits);
        limits.upload_quota_bytes = config.upload_quota_bytes;
        limits.upload_max_file_bytes = config.upload_max_file_bytes;
        limits.upload_cache_root = Some(config.upload_cache_path());
        limits.ping_interval_seconds = config.ping_interval_seconds.clamp(5, 600);
        limits
    }
}

impl Default for ServerConfig {
    fn default() -> Self {
        if let Some(root) = std::env::var_os("OMENCHATD_HOME").map(PathBuf::from) {
            return Self::for_root(root);
        }
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));
        Self::for_root(home.join(".omenchatd"))
    }
}

impl ServerConfig {
    pub fn for_root(root: PathBuf) -> Self {
        Self {
            name: "OMENchat Node".into(),
            operator_label: "node-admin".into(),
            config_path: root.join("config.toml"),
            identity_path: root.join("identity"),
            database_path: root.join("omenchat.sqlite"),
            reticulum_config_path: root.join("reticulum"),
            chat_aspect: OMENCHAT_DESTINATION_ASPECT.into(),
            nomadnet_page_path: NOMADNET_PORTAL_PATH.into(),
            motd: "Welcome to OMENchat".into(),
            announce_interval_minutes: 360,
            limits: ServerLimitsConfig::default(),
            upload_quota_bytes: DEFAULT_UPLOAD_QUOTA_BYTES,
            upload_max_file_bytes: DEFAULT_UPLOAD_MAX_FILE_BYTES,
            ping_interval_seconds: DEFAULT_PING_INTERVAL_SECONDS,
        }
    }

    pub fn root_dir(&self) -> PathBuf {
        self.config_path
            .parent()
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."))
    }

    pub fn log_path(&self) -> PathBuf {
        self.root_dir().join("omenchatd.log")
    }

    pub fn render_toml(&self) -> String {
        format!(
            r#"[server]
name = "{name}"
operator_label = "{operator}"
motd = "{motd}"
identity_path = "{identity}"
database_path = "{database}"
reticulum_config_path = "{reticulum}"
announce_interval_minutes = {announce_interval_minutes}
ping_interval_seconds = {ping_interval_seconds}

[destinations]
# Fixed public service names. These are not operator-editable:
#   OMENchat live chat: omenchat.node
#   NomadNet portal:    nomadnetwork.node
nomadnet_enabled = true
lxmf_enabled = true

[limits]
max_rooms_per_session = 16
max_message_bytes = {max_message_bytes}
history_batch_size = {history_batch_size}
join_backlog_events = {join_backlog_events}
large_batch_threshold_bytes = {large_batch_threshold_bytes}
upload_quota_bytes = {upload_quota_bytes}
upload_max_file_bytes = {upload_max_file_bytes}
max_userlist_inline_users = 96
rate_messages_per_minute = {rate_messages_per_minute}
rate_commands_per_minute = {rate_commands_per_minute}

[compression]
history = "bzip2"
userlist = "bzip2"
compression_level = 6

[policy]
allow_public_rooms = true
allow_contact_exchange = true
contact_visibility_default = "on_request"
require_invite_for_private_rooms = true
allow_typing_indicators = false
allow_read_receipts = false
"#,
            name = self.name,
            operator = self.operator_label,
            motd = escape_toml_string(&self.motd),
            identity = self.identity_path.display(),
            database = self.database_path.display(),
            reticulum = self.reticulum_config_path.display(),
            announce_interval_minutes = self.announce_interval_minutes,
            upload_quota_bytes = self.upload_quota_bytes,
            upload_max_file_bytes = self.upload_max_file_bytes,
            ping_interval_seconds = self.ping_interval_seconds,
            max_message_bytes = self.limits.max_message_bytes,
            history_batch_size = self.limits.history_batch_size,
            join_backlog_events = self.limits.join_backlog_events,
            large_batch_threshold_bytes = self.limits.large_batch_threshold_bytes,
            rate_messages_per_minute = self.limits.rate_messages_per_minute,
            rate_commands_per_minute = self.limits.rate_commands_per_minute,
        )
    }

    pub fn load_or_default(root: PathBuf) -> ServerResult<Self> {
        let mut config = Self::for_root(root);
        if !config.config_path.exists() {
            return Ok(config);
        }
        let contents = std::fs::read_to_string(&config.config_path)?;
        let mut saw_upload_max_file_bytes = false;
        for line in contents.lines() {
            let Some((key, value)) = parse_toml_assignment(line) else {
                continue;
            };
            match key {
                "name" => config.name = value,
                "operator_label" => config.operator_label = value,
                "motd" => config.motd = value,
                "identity_path" => config.identity_path = PathBuf::from(value),
                "database_path" => config.database_path = PathBuf::from(value),
                "reticulum_config_path" => config.reticulum_config_path = PathBuf::from(value),
                "announce_interval_minutes" => {
                    if let Ok(minutes) = value.parse::<u64>() {
                        config.announce_interval_minutes = minutes.max(1);
                    }
                }
                "upload_quota_bytes" => {
                    if let Ok(bytes) = value.parse::<u64>() {
                        config.upload_quota_bytes = bytes.min(10 * 1024 * 1024 * 1024);
                    }
                }
                "upload_max_file_bytes" => {
                    saw_upload_max_file_bytes = true;
                    if let Ok(bytes) = value.parse::<u64>() {
                        config.upload_max_file_bytes = bytes.clamp(1, 10 * 1024 * 1024);
                    }
                }
                "ping_interval_seconds" => {
                    if let Ok(seconds) = value.parse::<u64>() {
                        config.ping_interval_seconds = seconds.clamp(5, 600);
                    }
                }
                "chat_aspect" | "nomadnet_page_path" => {}
                "max_message_bytes" => {
                    if let Ok(bytes) = value.parse::<usize>() {
                        config.limits.max_message_bytes = bytes.clamp(1, 262_144);
                    }
                }
                "history_batch_size" => {
                    if let Ok(size) = value.parse::<usize>() {
                        config.limits.history_batch_size = size.clamp(1, 500);
                    }
                }
                "join_backlog_events" => {
                    if let Ok(size) = value.parse::<usize>() {
                        config.limits.join_backlog_events = size.clamp(0, 500);
                    }
                }
                "large_batch_threshold_bytes" => {
                    if let Ok(bytes) = value.parse::<usize>() {
                        config.limits.large_batch_threshold_bytes = bytes.clamp(256, 1_048_576);
                    }
                }
                "rate_messages_per_minute" => {
                    if let Ok(rate) = value.parse::<usize>() {
                        config.limits.rate_messages_per_minute = rate.min(600);
                    }
                }
                "rate_commands_per_minute" => {
                    if let Ok(rate) = value.parse::<usize>() {
                        config.limits.rate_commands_per_minute = rate.min(600);
                    }
                }
                _ => {}
            }
        }
        if !saw_upload_max_file_bytes && config.upload_quota_bytes == DEFAULT_UPLOAD_MAX_FILE_BYTES
        {
            config.upload_quota_bytes = DEFAULT_UPLOAD_QUOTA_BYTES;
        }
        Ok(config)
    }

    pub fn save(&self) -> ServerResult<()> {
        std::fs::create_dir_all(self.root_dir())?;
        std::fs::write(&self.config_path, self.render_toml())?;
        Ok(())
    }

    pub fn reticulum_config_file(&self) -> PathBuf {
        self.reticulum_config_path.join("config")
    }

    pub fn reticulum_storage_path(&self) -> PathBuf {
        self.reticulum_config_path.join("storage")
    }

    pub fn nomadnet_pages_path(&self) -> PathBuf {
        self.reticulum_storage_path().join("pages")
    }

    pub fn upload_cache_path(&self) -> PathBuf {
        self.root_dir().join("uploads")
    }

    pub fn nomadnet_index_page_path(&self) -> PathBuf {
        self.nomadnet_pages_path().join("index.mu")
    }
}

fn parse_toml_assignment(line: &str) -> Option<(&str, String)> {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with('[') {
        return None;
    }
    let (key, raw_value) = trimmed.split_once('=')?;
    let key = key.trim();
    let raw_value = raw_value.trim();
    let value = raw_value
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .unwrap_or(raw_value)
        .trim()
        .to_string();
    if key.is_empty() || value.is_empty() {
        return None;
    }
    Some((key, value))
}

fn escape_toml_string(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

pub fn init_files(config: &ServerConfig) -> ServerResult<()> {
    std::fs::create_dir_all(config.root_dir())?;
    if let Some(parent) = config.identity_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    if let Some(parent) = config.database_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::create_dir_all(&config.reticulum_config_path)?;
    std::fs::create_dir_all(config.nomadnet_pages_path())?;

    write_if_missing(&config.config_path, config.render_toml().as_bytes())?;
    write_if_missing(
        &config.reticulum_config_file(),
        render_reticulum_base_config(config).as_bytes(),
    )?;
    touch_log_file(config)?;
    write_identity_if_missing(&config.identity_path)?;

    let connection = rusqlite::Connection::open(&config.database_path)?;
    connection.execute_batch(include_str!("../migrations/001_init.sql"))?;
    connection.execute(
        "INSERT OR IGNORE INTO server_config(key, value) VALUES (?1, ?2)",
        ("schema_version", "1"),
    )?;
    connection.execute(
        "INSERT OR IGNORE INTO rooms(room_id, name, topic, created_at) VALUES (1, 'lobby', 'Default OMENchat lobby', 0)",
        [],
    )?;
    Ok(())
}

fn touch_log_file(config: &ServerConfig) -> ServerResult<()> {
    let path = config.log_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    Ok(())
}

pub fn add_room(config: &ServerConfig, name: &str, topic: Option<&str>) -> ServerResult<()> {
    init_files(config)?;
    let room_name = normalize_room_name(name);
    if room_name.is_empty() {
        return Err(ServerError::Message(
            "room name must contain at least one ASCII letter, digit, '_' or '-'".into(),
        ));
    }
    let connection = rusqlite::Connection::open(&config.database_path)?;
    connection.execute(
        "INSERT INTO rooms(name, topic, created_at) VALUES (?1, ?2, 0)
         ON CONFLICT(name) DO UPDATE SET
           topic = COALESCE(excluded.topic, rooms.topic),
           archived = 0",
        (&room_name, topic),
    )?;
    Ok(())
}

pub fn list_rooms(config: &ServerConfig) -> ServerResult<Vec<(i64, String, Option<String>)>> {
    init_files(config)?;
    let connection = rusqlite::Connection::open(&config.database_path)?;
    let mut statement = connection
        .prepare("SELECT room_id, name, topic FROM rooms WHERE archived = 0 ORDER BY name")?;
    let rows = statement.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, Option<String>>(2)?,
        ))
    })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

pub fn nomadnet_portal_paths(config: &ServerConfig) -> Vec<String> {
    let mut paths = Vec::new();
    for candidate in [
        NOMADNET_PORTAL_PATH,
        config.nomadnet_page_path.as_str(),
        "/omenchat.mu",
        "/",
    ] {
        let path = normalize_nomadnet_page_path(candidate);
        if !paths.contains(&path) {
            paths.push(path);
        }
    }
    paths
}

pub fn render_nomadnet_portal(
    config: &ServerConfig,
    omenchat_destination_hash: &str,
) -> ServerResult<String> {
    let rooms = list_rooms(config)?;
    let mut page = String::new();
    page.push_str("#!c=60\n#!bg=000000\n#!fg=dddddd\n\n");
    page.push_str("`c");
    page.push_str(&escape_micron_text(&config.name));
    page.push_str("\n\n");
    if !config.motd.trim().is_empty() {
        page.push_str(&escape_micron_text(config.motd.trim()));
        page.push_str("\n\n");
    }
    page.push_str("`[Open OMENchat`omenchat://");
    page.push_str(omenchat_destination_hash);
    page.push_str("]\n\n");
    page.push_str("Server address:\n");
    page.push_str("omenchat://");
    page.push_str(omenchat_destination_hash);
    page.push_str("\n\n");
    page.push_str("Rooms:\n");
    for (_, name, topic) in rooms {
        page.push_str("- #");
        page.push_str(&escape_micron_text(&name));
        if let Some(topic) = topic.filter(|topic| !topic.trim().is_empty()) {
            page.push_str(" - ");
            page.push_str(&escape_micron_text(topic.trim()));
        }
        page.push('\n');
    }
    page.push_str("\nThis NomadNet portal is quiet. Chat traffic uses the OMENchat link above.\n");
    Ok(page)
}

pub fn ensure_nomadnet_portal(
    config: &ServerConfig,
    omenchat_destination_hash: &str,
) -> ServerResult<PathBuf> {
    let path = config.nomadnet_index_page_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    if !path.exists() {
        let page = render_nomadnet_portal(config, omenchat_destination_hash)?;
        std::fs::write(&path, page)?;
    }
    Ok(path)
}

fn normalize_nomadnet_page_path(path: &str) -> String {
    let path = path.trim();
    if path.is_empty() {
        return NOMADNET_PORTAL_PATH.into();
    }
    if path.starts_with('/') {
        path.to_owned()
    } else {
        format!("/{path}")
    }
}

fn escape_micron_text(value: &str) -> String {
    value
        .chars()
        .map(|ch| match ch {
            '`' | '[' | ']' => ' ',
            _ => ch,
        })
        .collect()
}

pub fn update_room_topic(
    config: &ServerConfig,
    room_id: i64,
    topic: Option<&str>,
) -> ServerResult<()> {
    init_files(config)?;
    let connection = rusqlite::Connection::open(&config.database_path)?;
    let changed = connection.execute(
        "UPDATE rooms
         SET topic = ?1, room_revision = room_revision + 1
         WHERE room_id = ?2 AND archived = 0",
        (topic, room_id),
    )?;
    if changed == 0 {
        return Err(ServerError::Message("room not found".into()));
    }
    Ok(())
}

pub fn archive_room(config: &ServerConfig, room_id: i64) -> ServerResult<()> {
    init_files(config)?;
    if room_id == 1 {
        return Err(ServerError::Message(
            "the lobby room cannot be archived".into(),
        ));
    }
    let connection = rusqlite::Connection::open(&config.database_path)?;
    let changed = connection.execute(
        "UPDATE rooms
         SET archived = 1, room_revision = room_revision + 1
         WHERE room_id = ?1 AND archived = 0",
        [room_id],
    )?;
    if changed == 0 {
        return Err(ServerError::Message("room not found".into()));
    }
    Ok(())
}

fn normalize_room_name(name: &str) -> String {
    name.trim()
        .trim_start_matches('#')
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || *ch == '_' || *ch == '-')
        .take(48)
        .collect::<String>()
}

pub fn write_reticulum_tcp_server_config(
    config: &ServerConfig,
    tcp_server: &TcpServerOverride,
) -> ServerResult<()> {
    std::fs::create_dir_all(&config.reticulum_config_path)?;
    let config_path = config.reticulum_config_file();
    let rendered = render_reticulum_tcp_server_config(config, tcp_server);
    std::fs::write(config_path, rendered)?;
    Ok(())
}

pub fn write_reticulum_tcp_client_config(
    config: &ServerConfig,
    tcp_client: &TcpClientOverride,
) -> ServerResult<()> {
    std::fs::create_dir_all(&config.reticulum_config_path)?;
    let config_path = config.reticulum_config_file();
    let rendered = render_reticulum_tcp_client_config(config, tcp_client);
    std::fs::write(config_path, rendered)?;
    Ok(())
}

fn render_reticulum_base_config(config: &ServerConfig) -> String {
    format!(
        r#"[reticulum]
enable_transport = No
share_instance = No
panic_on_interface_error = Yes
network_identity = {identity}

[logging]
loglevel = 4

[interfaces]
  # Add an interface with:
  #   omenchatd interfaces tcp-client <gateway_host:port> --home <server-home>
  # or edit this file directly.
"#,
        identity = config.identity_path.display(),
    )
}

fn render_reticulum_tcp_server_config(
    config: &ServerConfig,
    tcp_server: &TcpServerOverride,
) -> String {
    format!(
        r#"[reticulum]
enable_transport = No
share_instance = No
panic_on_interface_error = Yes
network_identity = {identity}

[logging]
loglevel = 4

[interfaces]
  [[OMENchat TCP Server]]
    type = TCPServerInterface
    enabled = Yes
    interface_enabled = true
    listen_ip = {listen_ip}
    listen_port = {listen_port}
"#,
        identity = config.identity_path.display(),
        listen_ip = tcp_server.listen_ip,
        listen_port = tcp_server.listen_port
    )
}

fn render_reticulum_tcp_client_config(
    config: &ServerConfig,
    tcp_client: &TcpClientOverride,
) -> String {
    let mut rendered = format!(
        r#"[reticulum]
enable_transport = No
share_instance = No
panic_on_interface_error = Yes
network_identity = {identity}

[logging]
loglevel = 4

[interfaces]
  [[OMENchat TCP Client]]
    type = TCPClientInterface
    enabled = Yes
    interface_enabled = true
    target_host = {target_host}
    target_port = {target_port}
"#,
        identity = config.identity_path.display(),
        target_host = tcp_client.target_host,
        target_port = tcp_client.target_port
    );
    if let Some(network_name) = tcp_client
        .network_name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        rendered.push_str(&format!("    network_name = {network_name}\n"));
    }
    if let Some(passphrase) = tcp_client
        .passphrase
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        rendered.push_str(&format!("    passphrase = {passphrase}\n"));
    }
    rendered
}

pub fn render_status(config: &ServerConfig) -> String {
    let rooms = list_rooms(config).unwrap_or_default();
    let reticulum_config_file = config.reticulum_config_file();
    let destination_status = render_destination_status(config);
    let portal_file_status = render_portal_file_status(config);
    let limits = render_limits_status(&config.limits);
    format!(
        "name: {name}\noperator: {operator}\nmotd: {motd}\nidentity: {identity}\n{destination_status}database: {database}\nreticulum dir: {reticulum_dir}\nreticulum config: {reticulum_config}\nchat service: omenchat.{aspect} (fixed)\nportal service: nomadnetwork.node path={nomadnet_page} (fixed)\n{portal_file_status}announce interval: {announce_interval} minute(s)\nping interval: {ping_interval} second(s)\nupload quota: {upload_quota}\nupload max file: {upload_max_file}\nupload cache: {upload_cache}\nrooms: {rooms}\n{limits}",
        name = config.name,
        operator = config.operator_label,
        motd = if config.motd.trim().is_empty() {
            "(none)"
        } else {
            config.motd.trim()
        },
        identity = config.identity_path.display(),
        destination_status = destination_status,
        database = config.database_path.display(),
        reticulum_dir = config.reticulum_config_path.display(),
        reticulum_config = reticulum_config_file.display(),
        aspect = OMENCHAT_DESTINATION_ASPECT,
        nomadnet_page = NOMADNET_PORTAL_PATH,
        portal_file_status = portal_file_status,
        announce_interval = config.announce_interval_minutes,
        ping_interval = config.ping_interval_seconds,
        upload_quota = if config.upload_quota_bytes == 0 {
            "disabled".into()
        } else {
            human_bytes(config.upload_quota_bytes)
        },
        upload_max_file = human_bytes(config.upload_max_file_bytes),
        upload_cache = config.upload_cache_path().display(),
        rooms = rooms.len(),
        limits = limits,
    )
}

pub fn render_public_addresses(config: &ServerConfig) -> String {
    render_destination_status(config)
}

fn render_portal_file_status(config: &ServerConfig) -> String {
    let path = config.nomadnet_index_page_path();
    match std::fs::metadata(&path) {
        Ok(metadata) => format!(
            "nomadnet page file: {path} ({size}, modified {modified})\n",
            path = path.display(),
            size = human_bytes(metadata.len()),
            modified = metadata
                .modified()
                .map(human_system_time)
                .unwrap_or_else(|_| "unknown".into()),
        ),
        Err(_) => format!("nomadnet page file: {} (missing)\n", path.display()),
    }
}

fn render_limits_status(limits: &ServerLimitsConfig) -> String {
    format!(
        "limits:\n  max message: {max_message_bytes} ({max_message_human})\n  history batch: {history_batch_size} event(s)\n  join backlog: {join_backlog_events} event(s)\n  large batch threshold: {large_batch_threshold_bytes} ({large_batch_human})\n  message rate: {rate_messages_per_minute} / minute\n  command rate: {rate_commands_per_minute} / minute\n",
        max_message_bytes = limits.max_message_bytes,
        max_message_human = human_bytes(limits.max_message_bytes as u64),
        history_batch_size = limits.history_batch_size,
        join_backlog_events = limits.join_backlog_events,
        large_batch_threshold_bytes = limits.large_batch_threshold_bytes,
        large_batch_human = human_bytes(limits.large_batch_threshold_bytes as u64),
        rate_messages_per_minute = limits.rate_messages_per_minute,
        rate_commands_per_minute = limits.rate_commands_per_minute,
    )
}

fn human_system_time(value: SystemTime) -> String {
    let Ok(age) = SystemTime::now().duration_since(value) else {
        return "in the future".into();
    };
    let seconds = age.as_secs();
    if seconds < 5 {
        "just now".into()
    } else if seconds < 60 {
        format!("{seconds} seconds ago")
    } else if seconds < 3_600 {
        format!("{} minutes ago", seconds / 60)
    } else if seconds < 86_400 {
        format!("{} hours ago", seconds / 3_600)
    } else {
        format!("{} days ago", seconds / 86_400)
    }
}

#[cfg(feature = "live-reticulum")]
fn render_destination_status(config: &ServerConfig) -> String {
    crate::reticulum_live::configured_destination_status(config)
        .unwrap_or_else(|error| format!("destination: unavailable ({error})\n"))
}

#[cfg(all(not(feature = "live-reticulum"), all(feature = "live-rns-net", any())))]
fn render_destination_status(config: &ServerConfig) -> String {
    crate::rns_net_live::configured_destination_status(config)
        .unwrap_or_else(|error| format!("destination: unavailable ({error})\n"))
}

#[cfg(all(
    not(feature = "live-reticulum"),
    not(all(feature = "live-rns-net", any()))
))]
fn render_destination_status(_config: &ServerConfig) -> String {
    "destination: unavailable (rebuild with --features live-reticulum)\n".into()
}

fn write_if_missing(path: &PathBuf, bytes: &[u8]) -> ServerResult<()> {
    match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
    {
        Ok(mut file) => {
            use std::io::Write;
            file.write_all(bytes)?;
            Ok(())
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => Ok(()),
        Err(error) => Err(error.into()),
    }
}

#[cfg(all(feature = "live-rns-net", any()))]
fn write_identity_if_missing(path: &PathBuf) -> ServerResult<()> {
    use crate::error::ServerError;

    if path.exists() {
        let existing = std::fs::read(path)?;
        if existing.len() == 64 {
            return Ok(());
        }
        if existing == PLACEHOLDER_IDENTITY {
            let backup_path = path.with_extension("placeholder.bak");
            if !backup_path.exists() {
                std::fs::copy(path, &backup_path)?;
            }
        } else {
            return Err(ServerError::Message(format!(
                "refusing to replace invalid OMENchat identity at {}; expected 64 bytes, got {}",
                path.display(),
                existing.len()
            )));
        }
    }

    let identity = rns_crypto::identity::Identity::new(&mut rns_crypto::OsRng);
    rns_net::storage::save_identity(&identity, path)?;
    Ok(())
}

#[cfg(not(all(feature = "live-rns-net", any())))]
fn write_identity_if_missing(path: &PathBuf) -> ServerResult<()> {
    write_if_missing(path, PLACEHOLDER_IDENTITY)
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    const FIXTURE_OMENCHAT_HASH: &str = "00112233445566778899aabbccddeeff";
    const REPLACEMENT_OMENCHAT_HASH: &str = "ffffffffffffffffffffffffffffffff";

    fn temp_root(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "omenchatd-config-{label}-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ))
    }

    #[test]
    fn init_preserves_existing_identity_file() {
        let root = temp_root("preserve-identity");
        let config = ServerConfig::for_root(root.clone());
        std::fs::create_dir_all(config.root_dir()).expect("root");
        std::fs::write(&config.identity_path, [7u8; 64]).expect("identity");

        init_files(&config).expect("init");

        assert_eq!(
            std::fs::read(&config.identity_path).expect("read"),
            [7u8; 64]
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn init_creates_empty_log_file() {
        let root = temp_root("log-file");
        let config = ServerConfig::for_root(root.clone());

        init_files(&config).expect("init");

        assert!(config.log_path().is_file());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn init_creates_editable_baseline_reticulum_config() {
        let root = temp_root("baseline-reticulum");
        let config = ServerConfig::for_root(root.clone());

        init_files(&config).expect("init");

        let rendered =
            std::fs::read_to_string(config.reticulum_config_path.join("config")).expect("read");
        assert!(rendered.contains("share_instance = No"));
        assert!(rendered.contains(&format!(
            "network_identity = {}",
            config.identity_path.display()
        )));
        assert!(rendered.contains("[interfaces]"));
        assert!(rendered.contains("omenchatd interfaces tcp-client"));
        assert!(!rendered.contains(".reticulum"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[cfg(all(feature = "live-rns-net", any()))]
    #[test]
    fn live_init_creates_real_rns_identity() {
        let root = temp_root("live-identity");
        let config = ServerConfig::for_root(root.clone());

        init_files(&config).expect("init");

        let identity_bytes = std::fs::read(&config.identity_path).expect("read identity");
        assert_eq!(identity_bytes.len(), 64);
        let _ = rns_net::storage::load_identity(&config.identity_path).expect("load identity");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn tcp_server_override_writes_isolated_reticulum_config() {
        let root = temp_root("tcp-server-reticulum");
        let config = ServerConfig::for_root(root.clone());
        init_files(&config).expect("init");

        write_reticulum_tcp_server_config(
            &config,
            &TcpServerOverride {
                listen_ip: "127.0.0.1".into(),
                listen_port: 42420,
            },
        )
        .expect("write tcp config");

        let rendered =
            std::fs::read_to_string(config.reticulum_config_path.join("config")).expect("read");
        assert!(rendered.contains("share_instance = No"));
        assert!(rendered.contains(&format!(
            "network_identity = {}",
            config.identity_path.display()
        )));
        assert!(rendered.contains("type = TCPServerInterface"));
        assert!(rendered.contains("listen_ip = 127.0.0.1"));
        assert!(rendered.contains("listen_port = 42420"));
        assert!(!rendered.contains(".reticulum"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn tcp_client_override_writes_isolated_reticulum_config() {
        let root = temp_root("tcp-client-reticulum");
        let config = ServerConfig::for_root(root.clone());
        init_files(&config).expect("init");

        write_reticulum_tcp_client_config(
            &config,
            &TcpClientOverride {
                target_host: "gateway.example".into(),
                target_port: 42420,
                network_name: None,
                passphrase: None,
            },
        )
        .expect("write tcp config");

        let rendered =
            std::fs::read_to_string(config.reticulum_config_path.join("config")).expect("read");
        assert!(rendered.contains("share_instance = No"));
        assert!(rendered.contains(&format!(
            "network_identity = {}",
            config.identity_path.display()
        )));
        assert!(rendered.contains("type = TCPClientInterface"));
        assert!(rendered.contains("target_host = gateway.example"));
        assert!(rendered.contains("target_port = 42420"));
        assert!(!rendered.contains(".reticulum"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn tcp_client_override_writes_ifac_fields_when_provided() {
        let root = temp_root("tcp-client-reticulum-ifac");
        let config = ServerConfig::for_root(root.clone());
        init_files(&config).expect("init");

        write_reticulum_tcp_client_config(
            &config,
            &TcpClientOverride {
                target_host: "gateway.example".into(),
                target_port: 42420,
                network_name: Some("private_ret".into()),
                passphrase: Some("test-passphrase".into()),
            },
        )
        .expect("write tcp config");

        let rendered =
            std::fs::read_to_string(config.reticulum_config_path.join("config")).expect("read");
        assert!(rendered.contains("type = TCPClientInterface"));
        assert!(rendered.contains("target_host = gateway.example"));
        assert!(rendered.contains("target_port = 42420"));
        assert!(rendered.contains("network_name = private_ret"));
        assert!(rendered.contains("passphrase = test-passphrase"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn load_or_default_reads_saved_operator_config() {
        let root = temp_root("load-config");
        let mut config = ServerConfig::for_root(root.clone());
        init_files(&config).expect("init");
        config.name = "Test Chat".into();
        config.operator_label = "test-admin".into();
        config.motd = "Field notices go here".into();
        config.announce_interval_minutes = 720;
        config.upload_quota_bytes = 123_456;
        config.upload_max_file_bytes = 12_345;
        config.ping_interval_seconds = 45;
        config.limits.max_message_bytes = 4096;
        config.limits.history_batch_size = 25;
        config.limits.join_backlog_events = 12;
        config.limits.large_batch_threshold_bytes = 8192;
        config.limits.rate_messages_per_minute = 33;
        config.limits.rate_commands_per_minute = 17;
        config.save().expect("save");

        let loaded = ServerConfig::load_or_default(root.clone()).expect("load");

        assert_eq!(loaded.name, "Test Chat");
        assert_eq!(loaded.operator_label, "test-admin");
        assert_eq!(loaded.motd, "Field notices go here");
        assert_eq!(loaded.chat_aspect, OMENCHAT_DESTINATION_ASPECT);
        assert_eq!(loaded.nomadnet_page_path, NOMADNET_PORTAL_PATH);
        assert_eq!(loaded.announce_interval_minutes, 720);
        assert_eq!(loaded.upload_quota_bytes, 123_456);
        assert_eq!(loaded.upload_max_file_bytes, 12_345);
        assert_eq!(loaded.ping_interval_seconds, 45);
        assert_eq!(loaded.limits.max_message_bytes, 4096);
        assert_eq!(loaded.limits.history_batch_size, 25);
        assert_eq!(loaded.limits.join_backlog_events, 12);
        assert_eq!(loaded.limits.large_batch_threshold_bytes, 8192);
        assert_eq!(loaded.limits.rate_messages_per_minute, 33);
        assert_eq!(loaded.limits.rate_commands_per_minute, 17);
        assert_eq!(loaded.identity_path, config.identity_path);
        assert_eq!(loaded.reticulum_config_path, config.reticulum_config_path);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn rendered_config_does_not_expose_fixed_destination_knobs() {
        let root = temp_root("render-fixed-destinations");
        let config = ServerConfig::for_root(root.clone());

        let rendered = config.render_toml();

        assert!(rendered.contains("Fixed public service names"));
        assert!(rendered.contains("OMENchat live chat: omenchat.node"));
        assert!(rendered.contains("NomadNet portal:    nomadnetwork.node"));
        assert!(!rendered.contains("chat_aspect ="));
        assert!(!rendered.contains("nomadnet_page_path ="));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn default_upload_policy_separates_quota_and_file_limit() {
        let root = temp_root("default-upload-policy");
        let config = ServerConfig::for_root(root.clone());
        let limits = SessionLimits::from(&config);

        assert_eq!(config.upload_quota_bytes, 50 * 1024 * 1024);
        assert_eq!(config.upload_max_file_bytes, 512 * 1024);
        assert_eq!(limits.upload_quota_bytes, 50 * 1024 * 1024);
        assert_eq!(limits.upload_max_file_bytes, 512 * 1024);
        assert_eq!(
            SessionLimits::default().upload_quota_bytes,
            50 * 1024 * 1024
        );
        assert_eq!(SessionLimits::default().upload_max_file_bytes, 512 * 1024);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn load_or_default_recovers_mistaken_alpha_upload_quota_default() {
        let root = temp_root("recover-alpha-upload-quota");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("root");
        std::fs::write(
            root.join("config.toml"),
            "name = \"OMENchat Server\"\nupload_quota_bytes = 524288\n",
        )
        .expect("write config");

        let loaded = ServerConfig::load_or_default(root.clone()).expect("load");

        assert_eq!(loaded.upload_quota_bytes, DEFAULT_UPLOAD_QUOTA_BYTES);
        assert_eq!(loaded.upload_max_file_bytes, DEFAULT_UPLOAD_MAX_FILE_BYTES);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn load_or_default_preserves_explicit_small_quota_when_max_file_is_present() {
        let root = temp_root("preserve-explicit-small-upload-quota");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("root");
        std::fs::write(
            root.join("config.toml"),
            "name = \"OMENchat Server\"\nupload_quota_bytes = 524288\nupload_max_file_bytes = 131072\n",
        )
        .expect("write config");

        let loaded = ServerConfig::load_or_default(root.clone()).expect("load");

        assert_eq!(loaded.upload_quota_bytes, DEFAULT_UPLOAD_MAX_FILE_BYTES);
        assert_eq!(loaded.upload_max_file_bytes, 128 * 1024);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn nomadnet_portal_renders_omenchat_uri_motd_and_rooms() {
        let root = temp_root("nomadnet-portal");
        let mut config = ServerConfig::for_root(root.clone());
        config.name = "Field Chat".into();
        config.motd = "Quiet launch notice".into();
        init_files(&config).expect("init");
        add_room(&config, "ops", Some("Field operations")).expect("room");

        let page = render_nomadnet_portal(&config, FIXTURE_OMENCHAT_HASH).expect("portal");

        assert!(page.contains("Field Chat"));
        assert!(page.contains("Quiet launch notice"));
        assert!(page.contains(&format!(
            "`[Open OMENchat`omenchat://{FIXTURE_OMENCHAT_HASH}]"
        )));
        assert!(page.contains("#ops - Field operations"));
        assert!(nomadnet_portal_paths(&config).contains(&NOMADNET_PORTAL_PATH.to_string()));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn nomadnet_portal_is_created_in_server_reticulum_pages_storage_without_overwriting() {
        let root = temp_root("nomadnet-portal-file");
        let mut config = ServerConfig::for_root(root.clone());
        config.name = "Stored Portal".into();
        init_files(&config).expect("init");

        let path = ensure_nomadnet_portal(&config, FIXTURE_OMENCHAT_HASH).expect("write");
        let page = std::fs::read_to_string(&path).expect("read");

        assert_eq!(
            path,
            config
                .reticulum_config_path
                .join("storage")
                .join("pages")
                .join("index.mu")
        );
        assert!(page.contains("Stored Portal"));
        assert!(page.contains(&format!("omenchat://{FIXTURE_OMENCHAT_HASH}")));

        std::fs::write(&path, "custom operator page").expect("custom page");
        let same_path = ensure_nomadnet_portal(&config, REPLACEMENT_OMENCHAT_HASH).expect("ensure");
        assert_eq!(same_path, path);
        assert_eq!(
            std::fs::read_to_string(&path).expect("read custom"),
            "custom operator page"
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn status_reports_operator_nomadnet_page_file() {
        let root = temp_root("nomadnet-portal-status");
        let config = ServerConfig::for_root(root.clone());
        init_files(&config).expect("init");
        ensure_nomadnet_portal(&config, FIXTURE_OMENCHAT_HASH).expect("portal");

        let status = render_status(&config);

        assert!(status.contains("chat service: omenchat.node (fixed)"));
        assert!(status.contains("portal service: nomadnetwork.node path=/page/index.mu (fixed)"));
        assert!(status.contains(&format!(
            "nomadnet page file: {}",
            config.nomadnet_index_page_path().display()
        )));
        assert!(!status.contains("nomadnet page file: (missing)"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[cfg(all(feature = "live-rns-net", any()))]
    #[test]
    fn status_reports_live_destination_hash() {
        let root = temp_root("status-live-destination");
        let config = ServerConfig::for_root(root.clone());
        init_files(&config).expect("init");

        let status = render_status(&config);

        assert!(status.contains("identity hash: "));
        assert!(status.contains("destination: omenchat.node ("));
        assert!(status.contains("client uri: omenchat://"));
        assert!(status.contains("nomadnet portal: nomadnetwork.node ("));
        assert!(status.contains("portal url: "));
        assert!(status.contains(":/page/index.mu"));
        assert!(!status.contains("destination: unavailable"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn status_reports_server_limits() {
        let root = temp_root("status-limits");
        let mut config = ServerConfig::for_root(root.clone());
        config.limits.max_message_bytes = 4096;
        config.limits.history_batch_size = 25;
        config.limits.join_backlog_events = 12;
        config.limits.large_batch_threshold_bytes = 8192;
        config.limits.rate_messages_per_minute = 33;
        config.limits.rate_commands_per_minute = 17;
        init_files(&config).expect("init");

        let status = render_status(&config);

        assert!(status.contains("limits:"));
        assert!(status.contains("max message: 4096 (4.00 KiB)"));
        assert!(status.contains("history batch: 25 event(s)"));
        assert!(status.contains("join backlog: 12 event(s)"));
        assert!(status.contains("large batch threshold: 8192 (8.00 KiB)"));
        assert!(status.contains("message rate: 33 / minute"));
        assert!(status.contains("command rate: 17 / minute"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[cfg(not(any(all(feature = "live-rns-net", any()), feature = "live-reticulum")))]
    #[test]
    fn status_reports_live_destination_requires_feature() {
        let root = temp_root("status-live-destination-unavailable");
        let config = ServerConfig::for_root(root.clone());
        init_files(&config).expect("init");

        let status = render_status(&config);

        assert!(status.contains("destination: unavailable"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[cfg(feature = "live-reticulum")]
    #[test]
    fn status_reports_live_destination_with_reticulum_feature() {
        let root = temp_root("status-live-destination-reticulum");
        let config = ServerConfig::for_root(root.clone());
        init_files(&config).expect("init");

        let status = render_status(&config);

        assert!(status.contains("destination: omenchat.node"));
        assert!(status.contains("client uri: omenchat://"));
        assert!(status.contains("nomadnet portal: nomadnetwork.node"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn server_limits_convert_to_session_limits() {
        let config = ServerLimitsConfig {
            max_message_bytes: 333,
            history_batch_size: 44,
            join_backlog_events: 22,
            large_batch_threshold_bytes: 555,
            rate_messages_per_minute: 66,
            rate_commands_per_minute: 11,
        };

        let limits = SessionLimits::from(&config);

        assert_eq!(limits.max_message_bytes, 333);
        assert_eq!(limits.history_batch_size, 44);
        assert_eq!(limits.join_backlog_events, 22);
        assert_eq!(limits.large_batch_threshold_bytes, 555);
        assert_eq!(limits.rate_messages_per_minute, 66);
        assert_eq!(limits.rate_commands_per_minute, 11);
    }

    #[test]
    fn add_room_normalizes_and_lists_rooms() {
        let root = temp_root("rooms");
        let config = ServerConfig::for_root(root.clone());

        add_room(&config, "#field-ops", Some("Field operations")).expect("add room");

        let rooms = list_rooms(&config).expect("rooms");
        assert!(rooms
            .iter()
            .any(|(_, name, topic)| name == "field-ops"
                && topic.as_deref() == Some("Field operations")));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn room_topic_and_archive_update_room_catalog() {
        let root = temp_root("room-admin");
        let config = ServerConfig::for_root(root.clone());

        add_room(&config, "ops", Some("Old topic")).expect("add room");
        let room_id = list_rooms(&config)
            .expect("rooms")
            .into_iter()
            .find(|(_, name, _)| name == "ops")
            .map(|(room_id, _, _)| room_id)
            .expect("ops room");

        update_room_topic(&config, room_id, Some("New topic")).expect("topic");
        assert!(list_rooms(&config)
            .expect("rooms")
            .iter()
            .any(|(id, _, topic)| *id == room_id && topic.as_deref() == Some("New topic")));

        archive_room(&config, room_id).expect("archive");
        assert!(!list_rooms(&config)
            .expect("rooms")
            .iter()
            .any(|(_, name, _)| name == "ops"));
        let _ = std::fs::remove_dir_all(root);
    }
}
