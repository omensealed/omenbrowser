#[cfg(feature = "live-rns-net")]
compile_error!("live-rns-net was removed from omenchatd; use --features live-reticulum");

pub mod config;
pub mod error;
pub mod live;
pub mod protocol;
#[cfg(feature = "live-reticulum")]
pub mod reticulum_live;
pub mod session;
pub mod store;
pub mod transport;
pub mod tui;
mod tui_format;
mod tui_layout;
mod tui_text;
pub mod upload;

use error::ServerResult;
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CliCommand {
    Init(ServerOptions),
    Run(ServerOptions),
    Tui(ServerOptions),
    Status(ServerOptions),
    Doctor(ServerOptions),
    ConfigShow(ServerOptions),
    ConfigSet(ServerOptions, ConfigSetOptions),
    RoomsList(ServerOptions),
    RoomsAdd(ServerOptions, RoomAddOptions),
    RoomsSetTopic(ServerOptions, RoomTopicOptions),
    RoomsArchive(ServerOptions, RoomSelectOptions),
    InterfacesTcpServer(ServerOptions, TcpServerOverride),
    InterfacesTcpClient(ServerOptions, TcpClientOverride),
    Help,
    Version,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ServerOptions {
    pub home: Option<PathBuf>,
    pub tcp_server: Option<TcpServerOverride>,
    pub tcp_client: Option<TcpClientOverride>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ConfigSetOptions {
    pub name: Option<String>,
    pub operator_label: Option<String>,
    pub motd: Option<String>,
    pub announce_interval_minutes: Option<u64>,
    pub upload_quota_bytes: Option<u64>,
    pub upload_max_file_bytes: Option<u64>,
    pub ping_interval_seconds: Option<u64>,
    pub max_message_bytes: Option<usize>,
    pub history_batch_size: Option<usize>,
    pub join_backlog_events: Option<usize>,
    pub large_batch_threshold_bytes: Option<usize>,
    pub rate_messages_per_minute: Option<usize>,
    pub rate_commands_per_minute: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RoomAddOptions {
    pub name: String,
    pub topic: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RoomTopicOptions {
    pub room_id: i64,
    pub topic: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RoomSelectOptions {
    pub room_id: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TcpServerOverride {
    pub listen_ip: String,
    pub listen_port: u16,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TcpClientOverride {
    pub target_host: String,
    pub target_port: u16,
    pub network_name: Option<String>,
    pub passphrase: Option<String>,
}

impl CliCommand {
    pub fn parse(args: impl IntoIterator<Item = String>) -> Self {
        let mut args = args.into_iter();
        let Some(command) = args.next() else {
            return Self::Help;
        };
        match command.as_str() {
            "init" => Self::Init(parse_options(args)),
            "run" => Self::Run(parse_options(args)),
            "tui" => Self::Tui(parse_options(args)),
            "status" => Self::Status(parse_options(args)),
            "doctor" => Self::Doctor(parse_options(args)),
            "config" => parse_config_command(args),
            "rooms" => parse_rooms_command(args),
            "interfaces" => parse_interfaces_command(args),
            "-h" | "--help" | "help" => Self::Help,
            "-V" | "--version" | "version" => Self::Version,
            _ => Self::Help,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Omenchatd;

impl Omenchatd {
    pub fn run(&self, command: CliCommand) -> ServerResult<()> {
        match command {
            CliCommand::Init(options) => {
                let config = config_from_options(&options)?;
                config::init_files(&config)?;
                if let Some(tcp_server) = options.tcp_server.as_ref() {
                    config::write_reticulum_tcp_server_config(&config, tcp_server)?;
                }
                if let Some(tcp_client) = options.tcp_client.as_ref() {
                    config::write_reticulum_tcp_client_config(&config, tcp_client)?;
                }
                println!("initialized omenchatd");
                println!("config: {}", config.config_path.display());
                println!("database: {}", config.database_path.display());
                println!("identity: {}", config.identity_path.display());
                println!("reticulum: {}", config.reticulum_config_path.display());
                Ok(())
            }
            CliCommand::Run(options) => {
                let config = config_from_options(&options)?;
                config::init_files(&config)?;
                if let Some(tcp_server) = options.tcp_server.as_ref() {
                    config::write_reticulum_tcp_server_config(&config, tcp_server)?;
                }
                if let Some(tcp_client) = options.tcp_client.as_ref() {
                    config::write_reticulum_tcp_client_config(&config, tcp_client)?;
                }
                #[cfg(feature = "live-reticulum")]
                {
                    reticulum_live::run_live_server(config)
                }
                #[cfg(all(not(feature = "live-reticulum"), all(feature = "live-rns-net", any())))]
                {
                    rns_net_live::run_live_server(config)
                }
                #[cfg(all(
                    not(feature = "live-reticulum"),
                    not(all(feature = "live-rns-net", any()))
                ))]
                {
                    let _ = config;
                    println!(
                        "omenchatd run: rebuild with --features live-reticulum to enable native Reticulum transport"
                    );
                    Ok(())
                }
            }
            CliCommand::Tui(options) => {
                let config = config_from_options(&options)?;
                tui::run_admin_console(config)
            }
            CliCommand::Status(options) => {
                let config = config_from_options(&options)?;
                config::init_files(&config)?;
                print!("{}", config::render_status(&config));
                Ok(())
            }
            CliCommand::Doctor(options) => {
                let config = config_from_options(&options)?;
                print!("{}", render_doctor_report(&config));
                Ok(())
            }
            CliCommand::ConfigShow(options) => {
                let config = config_from_options(&options)?;
                config::init_files(&config)?;
                print!("{}", config.render_toml());
                Ok(())
            }
            CliCommand::ConfigSet(options, patch) => {
                let mut config = config_from_options(&options)?;
                config::init_files(&config)?;
                apply_config_limit_patch(&mut config, &patch);
                if let Some(name) = patch.name {
                    config.name = name;
                }
                if let Some(operator_label) = patch.operator_label {
                    config.operator_label = operator_label;
                }
                if let Some(motd) = patch.motd {
                    config.motd = motd;
                }
                if let Some(minutes) = patch.announce_interval_minutes {
                    config.announce_interval_minutes = minutes.max(1);
                }
                if let Some(bytes) = patch.upload_quota_bytes {
                    config.upload_quota_bytes = bytes.min(10 * 1024 * 1024 * 1024);
                }
                if let Some(bytes) = patch.upload_max_file_bytes {
                    config.upload_max_file_bytes = bytes.clamp(1, 10 * 1024 * 1024);
                }
                if let Some(seconds) = patch.ping_interval_seconds {
                    config.ping_interval_seconds = seconds.clamp(5, 600);
                }
                config.save()?;
                println!("updated {}", config.config_path.display());
                Ok(())
            }
            CliCommand::RoomsList(options) => {
                let config = config_from_options(&options)?;
                for (room_id, name, topic) in config::list_rooms(&config)? {
                    println!(
                        "#{name}\troom_id={room_id}\ttopic={topic}",
                        topic = topic.unwrap_or_default()
                    );
                }
                Ok(())
            }
            CliCommand::RoomsAdd(options, room) => {
                let config = config_from_options(&options)?;
                config::add_room(&config, &room.name, room.topic.as_deref())?;
                println!("room ready: #{}", room.name.trim().trim_start_matches('#'));
                Ok(())
            }
            CliCommand::RoomsSetTopic(options, room) => {
                let config = config_from_options(&options)?;
                config::update_room_topic(&config, room.room_id, room.topic.as_deref())?;
                println!("room topic updated: id={}", room.room_id);
                Ok(())
            }
            CliCommand::RoomsArchive(options, room) => {
                let config = config_from_options(&options)?;
                config::archive_room(&config, room.room_id)?;
                println!("room archived: id={}", room.room_id);
                Ok(())
            }
            CliCommand::InterfacesTcpServer(options, tcp_server) => {
                let config = config_from_options(&options)?;
                config::init_files(&config)?;
                config::write_reticulum_tcp_server_config(&config, &tcp_server)?;
                println!("updated {}", config.reticulum_config_file().display());
                Ok(())
            }
            CliCommand::InterfacesTcpClient(options, tcp_client) => {
                let config = config_from_options(&options)?;
                config::init_files(&config)?;
                config::write_reticulum_tcp_client_config(&config, &tcp_client)?;
                println!("updated {}", config.reticulum_config_file().display());
                Ok(())
            }
            CliCommand::Help => {
                print_help();
                Ok(())
            }
            CliCommand::Version => {
                print_version();
                Ok(())
            }
        }
    }
}

fn parse_config_command(args: impl IntoIterator<Item = String>) -> CliCommand {
    let mut args = args.into_iter();
    let Some(command) = args.next() else {
        return CliCommand::Help;
    };
    match command.as_str() {
        "show" => CliCommand::ConfigShow(parse_options(args)),
        "set" => {
            let (options, patch) = parse_config_set_options(args);
            CliCommand::ConfigSet(options, patch)
        }
        _ => CliCommand::Help,
    }
}

fn parse_rooms_command(args: impl IntoIterator<Item = String>) -> CliCommand {
    let mut args = args.into_iter();
    let Some(command) = args.next() else {
        return CliCommand::Help;
    };
    match command.as_str() {
        "list" => CliCommand::RoomsList(parse_options(args)),
        "add" => {
            let (options, room) = parse_room_add_options(args);
            match room {
                Some(room) => CliCommand::RoomsAdd(options, room),
                None => CliCommand::Help,
            }
        }
        "topic" => {
            let (options, room) = parse_room_topic_options(args);
            match room {
                Some(room) => CliCommand::RoomsSetTopic(options, room),
                None => CliCommand::Help,
            }
        }
        "archive" => {
            let (options, room) = parse_room_select_options(args);
            match room {
                Some(room) => CliCommand::RoomsArchive(options, room),
                None => CliCommand::Help,
            }
        }
        _ => CliCommand::Help,
    }
}

fn parse_interfaces_command(args: impl IntoIterator<Item = String>) -> CliCommand {
    let mut args = args.into_iter();
    let Some(command) = args.next() else {
        return CliCommand::Help;
    };
    match command.as_str() {
        "tcp-server" => {
            let Some(value) = args.next() else {
                return CliCommand::Help;
            };
            let Some(tcp_server) = parse_tcp_server_override(&value) else {
                return CliCommand::Help;
            };
            CliCommand::InterfacesTcpServer(parse_options(args), tcp_server)
        }
        "tcp-client" => {
            let Some(value) = args.next() else {
                return CliCommand::Help;
            };
            let (options, ifac) = parse_options_with_ifac(args);
            let Some(mut tcp_client) = parse_tcp_client_override(&value) else {
                return CliCommand::Help;
            };
            apply_ifac_options(&mut tcp_client, ifac);
            CliCommand::InterfacesTcpClient(options, tcp_client)
        }
        _ => CliCommand::Help,
    }
}

fn apply_config_limit_patch(config: &mut config::ServerConfig, patch: &ConfigSetOptions) {
    if let Some(bytes) = patch.max_message_bytes {
        config.limits.max_message_bytes = bytes.clamp(1, 262_144);
    }
    if let Some(size) = patch.history_batch_size {
        config.limits.history_batch_size = size.clamp(1, 500);
    }
    if let Some(size) = patch.join_backlog_events {
        config.limits.join_backlog_events = size.clamp(0, 500);
    }
    if let Some(bytes) = patch.large_batch_threshold_bytes {
        config.limits.large_batch_threshold_bytes = bytes.clamp(256, 1_048_576);
    }
    if let Some(rate) = patch.rate_messages_per_minute {
        config.limits.rate_messages_per_minute = rate.min(600);
    }
    if let Some(rate) = patch.rate_commands_per_minute {
        config.limits.rate_commands_per_minute = rate.min(600);
    }
}

fn parse_options(args: impl IntoIterator<Item = String>) -> ServerOptions {
    parse_options_with_ifac(args).0
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct IfacOptions {
    network_name: Option<String>,
    passphrase: Option<String>,
}

fn parse_options_with_ifac(args: impl IntoIterator<Item = String>) -> (ServerOptions, IfacOptions) {
    let mut options = ServerOptions::default();
    let mut ifac = IfacOptions::default();
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--home" => {
                if let Some(path) = args.next() {
                    options.home = Some(PathBuf::from(path));
                }
            }
            "--tcp-server" => {
                if let Some(value) = args.next() {
                    options.tcp_server = parse_tcp_server_override(&value);
                }
            }
            "--tcp-client" => {
                if let Some(value) = args.next() {
                    options.tcp_client = parse_tcp_client_override(&value);
                }
            }
            "--network-name" | "--ifac-network" | "--ifac-network-name" => {
                ifac.network_name = args.next().filter(|value| !value.trim().is_empty());
            }
            "--passphrase" | "--ifac-passphrase" => {
                ifac.passphrase = args.next().filter(|value| !value.trim().is_empty());
            }
            _ => {}
        }
    }
    if let Some(tcp_client) = options.tcp_client.as_mut() {
        apply_ifac_options(tcp_client, ifac.clone());
    }
    (options, ifac)
}

fn apply_ifac_options(tcp_client: &mut TcpClientOverride, ifac: IfacOptions) {
    if ifac.network_name.is_some() {
        tcp_client.network_name = ifac.network_name;
    }
    if ifac.passphrase.is_some() {
        tcp_client.passphrase = ifac.passphrase;
    }
}

fn parse_config_set_options(
    args: impl IntoIterator<Item = String>,
) -> (ServerOptions, ConfigSetOptions) {
    let mut options = ServerOptions::default();
    let mut patch = ConfigSetOptions::default();
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--home" => {
                if let Some(path) = args.next() {
                    options.home = Some(PathBuf::from(path));
                }
            }
            "--name" => patch.name = args.next(),
            "--operator-label" => patch.operator_label = args.next(),
            "--motd" => patch.motd = args.next(),
            "--chat-aspect" | "--nomadnet-page" | "--nomadnet-page-path" => {
                let _ = args.next();
            }
            "--announce-interval" | "--announce-interval-minutes" => {
                patch.announce_interval_minutes = args.next().and_then(|value| value.parse().ok());
            }
            "--upload-quota-bytes" => {
                patch.upload_quota_bytes = args.next().and_then(|value| value.parse().ok());
            }
            "--upload-max-file-bytes" => {
                patch.upload_max_file_bytes = args.next().and_then(|value| value.parse().ok());
            }
            "--ping-interval" | "--ping-interval-seconds" => {
                patch.ping_interval_seconds = args.next().and_then(|value| value.parse().ok());
            }
            "--max-message-bytes" => {
                patch.max_message_bytes = args.next().and_then(|value| value.parse().ok());
            }
            "--history-batch-size" => {
                patch.history_batch_size = args.next().and_then(|value| value.parse().ok());
            }
            "--join-backlog-events" => {
                patch.join_backlog_events = args.next().and_then(|value| value.parse().ok());
            }
            "--large-batch-threshold-bytes" => {
                patch.large_batch_threshold_bytes =
                    args.next().and_then(|value| value.parse().ok());
            }
            "--rate-messages-per-minute" => {
                patch.rate_messages_per_minute = args.next().and_then(|value| value.parse().ok());
            }
            "--rate-commands-per-minute" => {
                patch.rate_commands_per_minute = args.next().and_then(|value| value.parse().ok());
            }
            _ => {}
        }
    }
    (options, patch)
}

fn parse_room_add_options(
    args: impl IntoIterator<Item = String>,
) -> (ServerOptions, Option<RoomAddOptions>) {
    let mut options = ServerOptions::default();
    let mut name = None;
    let mut topic = None;
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--home" => {
                if let Some(path) = args.next() {
                    options.home = Some(PathBuf::from(path));
                }
            }
            "--topic" => topic = args.next(),
            value if name.is_none() => name = Some(value.to_string()),
            _ => {}
        }
    }
    (options, name.map(|name| RoomAddOptions { name, topic }))
}

fn parse_room_topic_options(
    args: impl IntoIterator<Item = String>,
) -> (ServerOptions, Option<RoomTopicOptions>) {
    let mut options = ServerOptions::default();
    let mut room_id = None;
    let mut topic = None;
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--home" => {
                if let Some(path) = args.next() {
                    options.home = Some(PathBuf::from(path));
                }
            }
            "--topic" => topic = args.next(),
            value if room_id.is_none() => room_id = value.parse::<i64>().ok(),
            _ => {}
        }
    }
    (
        options,
        room_id.map(|room_id| RoomTopicOptions { room_id, topic }),
    )
}

fn parse_room_select_options(
    args: impl IntoIterator<Item = String>,
) -> (ServerOptions, Option<RoomSelectOptions>) {
    let mut options = ServerOptions::default();
    let mut room_id = None;
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--home" => {
                if let Some(path) = args.next() {
                    options.home = Some(PathBuf::from(path));
                }
            }
            value if room_id.is_none() => room_id = value.parse::<i64>().ok(),
            _ => {}
        }
    }
    (
        options,
        room_id.map(|room_id| RoomSelectOptions { room_id }),
    )
}

fn parse_tcp_server_override(value: &str) -> Option<TcpServerOverride> {
    let (listen_ip, port) = value.rsplit_once(':')?;
    let listen_port = port.parse::<u16>().ok()?;
    if listen_ip.trim().is_empty() {
        return None;
    }
    Some(TcpServerOverride {
        listen_ip: listen_ip.trim().to_string(),
        listen_port,
    })
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

fn config_from_options(options: &ServerOptions) -> ServerResult<config::ServerConfig> {
    let root = options
        .home
        .clone()
        .or_else(|| std::env::var_os("OMENCHATD_HOME").map(PathBuf::from))
        .unwrap_or_else(default_home);
    config::ServerConfig::load_or_default(root)
}

fn default_home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".omenchatd")
}

fn print_help() {
    println!("omenchatd commands:");
    println!("  --version");
    println!("  init [--home <path>] [--tcp-server <listen_ip:port>] [--tcp-client <host:port>] [--network-name <name>] [--passphrase <pass>]");
    println!("  run [--home <path>] [--tcp-server <listen_ip:port>] [--tcp-client <host:port>] [--network-name <name>] [--passphrase <pass>]");
    println!("  status [--home <path>]");
    println!("  doctor [--home <path>]");
    println!("  config show [--home <path>]");
    println!(
        "  config set [--home <path>] [--name <name>] [--operator-label <label>] [--motd <text>] [--announce-interval <minutes>]"
    );
    println!("             [--max-message-bytes <bytes>] [--history-batch-size <count>] [--join-backlog-events <count>]");
    println!("             [--large-batch-threshold-bytes <bytes>] [--rate-messages-per-minute <count>] [--rate-commands-per-minute <count>]");
    println!("  rooms list [--home <path>]");
    println!("  rooms add <name> [--topic <topic>] [--home <path>]");
    println!("  rooms topic <room_id> [--topic <topic>] [--home <path>]");
    println!("  rooms archive <room_id> [--home <path>]");
    println!("  interfaces tcp-server <listen_ip:port> [--home <path>]");
    println!("  interfaces tcp-client <host:port> [--home <path>] [--network-name <name>] [--passphrase <pass>]");
    println!("  tui [--home <path>]");
}

fn print_version() {
    println!(
        "omenchatd {} features={}",
        env!("CARGO_PKG_VERSION"),
        compiled_feature_summary()
    );
}

fn compiled_feature_summary() -> String {
    [("live-reticulum", cfg!(feature = "live-reticulum"))]
        .into_iter()
        .map(|(name, enabled)| format!("{name}:{}", if enabled { "on" } else { "off" }))
        .collect::<Vec<_>>()
        .join(",")
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DoctorLevel {
    Pass,
    Warn,
    Fail,
}

impl DoctorLevel {
    fn label(self) -> &'static str {
        match self {
            Self::Pass => "PASS",
            Self::Warn => "WARN",
            Self::Fail => "FAIL",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct DoctorCheck {
    level: DoctorLevel,
    name: &'static str,
    detail: String,
}

fn render_doctor_report(config: &config::ServerConfig) -> String {
    let checks = doctor_checks(config);
    let fail_count = checks
        .iter()
        .filter(|check| check.level == DoctorLevel::Fail)
        .count();
    let warn_count = checks
        .iter()
        .filter(|check| check.level == DoctorLevel::Warn)
        .count();
    let outcome = if fail_count > 0 {
        "fail"
    } else if warn_count > 0 {
        "warn"
    } else {
        "pass"
    };
    let mut report = format!(
        "omenchatd doctor: {outcome}\nhome: {}\n",
        config.root_dir().display()
    );
    for check in checks {
        report.push_str(&format!(
            "[{}] {}: {}\n",
            check.level.label(),
            check.name,
            check.detail
        ));
    }
    report
}

fn doctor_checks(config: &config::ServerConfig) -> Vec<DoctorCheck> {
    let mut checks = Vec::new();
    checks.push(path_check(
        "config",
        &config.config_path,
        config.config_path.is_file(),
        "run `omenchatd init --home <path>`",
    ));
    checks.push(identity_check(&config.identity_path));
    checks.push(path_check(
        "database",
        &config.database_path,
        config.database_path.is_file(),
        "database file is missing",
    ));
    let reticulum_config = config.reticulum_config_file();
    checks.push(path_check(
        "reticulum config",
        &reticulum_config,
        reticulum_config.is_file(),
        "server-owned Reticulum config is missing",
    ));
    checks.push(path_check(
        "reticulum storage",
        &config.reticulum_storage_path(),
        config.reticulum_storage_path().is_dir(),
        "server-owned Reticulum storage directory is missing",
    ));
    checks.push(nomadnet_portal_check(config));
    checks.push(room_check(config));
    checks.push(interface_check(&reticulum_config));
    checks.push(limit_check(config));
    checks
}

fn path_check(name: &'static str, path: &std::path::Path, ok: bool, advice: &str) -> DoctorCheck {
    if ok {
        DoctorCheck {
            level: DoctorLevel::Pass,
            name,
            detail: path.display().to_string(),
        }
    } else {
        DoctorCheck {
            level: DoctorLevel::Fail,
            name,
            detail: format!("{} ({advice})", path.display()),
        }
    }
}

fn identity_check(path: &std::path::Path) -> DoctorCheck {
    let Ok(bytes) = std::fs::read(path) else {
        return DoctorCheck {
            level: DoctorLevel::Fail,
            name: "identity",
            detail: format!("{} (identity file is missing)", path.display()),
        };
    };
    if bytes.is_empty() {
        return DoctorCheck {
            level: DoctorLevel::Fail,
            name: "identity",
            detail: format!("{} (identity file is empty)", path.display()),
        };
    }
    if String::from_utf8_lossy(&bytes).contains("PLACEHOLDER") {
        return DoctorCheck {
            level: DoctorLevel::Warn,
            name: "identity",
            detail: format!(
                "{} (placeholder identity; rebuild/run with live-reticulum before public hosting)",
                path.display()
            ),
        };
    }
    DoctorCheck {
        level: DoctorLevel::Pass,
        name: "identity",
        detail: path.display().to_string(),
    }
}

fn nomadnet_portal_check(config: &config::ServerConfig) -> DoctorCheck {
    let path = config.nomadnet_index_page_path();
    if path.is_file() {
        DoctorCheck {
            level: DoctorLevel::Pass,
            name: "nomadnet portal",
            detail: path.display().to_string(),
        }
    } else {
        DoctorCheck {
            level: DoctorLevel::Warn,
            name: "nomadnet portal",
            detail: format!(
                "{} (portal page will be created when the live destination hash is available)",
                path.display()
            ),
        }
    }
}

fn room_check(config: &config::ServerConfig) -> DoctorCheck {
    match config::list_rooms(config) {
        Ok(rooms) if rooms.iter().any(|(_, name, _)| name == "lobby") => DoctorCheck {
            level: DoctorLevel::Pass,
            name: "rooms",
            detail: format!("{} active room(s), #lobby present", rooms.len()),
        },
        Ok(rooms) => DoctorCheck {
            level: DoctorLevel::Warn,
            name: "rooms",
            detail: format!("{} active room(s), #lobby missing", rooms.len()),
        },
        Err(error) => DoctorCheck {
            level: DoctorLevel::Fail,
            name: "rooms",
            detail: error.to_string(),
        },
    }
}

fn interface_check(reticulum_config: &std::path::Path) -> DoctorCheck {
    let Ok(contents) = std::fs::read_to_string(reticulum_config) else {
        return DoctorCheck {
            level: DoctorLevel::Fail,
            name: "interfaces",
            detail: "Reticulum config could not be read".into(),
        };
    };
    let active_config = contents
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .collect::<Vec<_>>()
        .join("\n");
    let has_enabled = active_config.contains("enabled = Yes")
        || active_config.contains("interface_enabled = true");
    let has_tcp_client = active_config.contains("TCPClientInterface");
    let has_tcp_server = active_config.contains("TCPServerInterface");
    if has_enabled && (has_tcp_client || has_tcp_server) {
        DoctorCheck {
            level: DoctorLevel::Pass,
            name: "interfaces",
            detail: if has_tcp_client {
                "enabled TCPClientInterface found".into()
            } else {
                "enabled TCPServerInterface found".into()
            },
        }
    } else {
        DoctorCheck {
            level: DoctorLevel::Warn,
            name: "interfaces",
            detail: "no enabled TCP client/server interface found; configure a gateway before public hosting".into(),
        }
    }
}

fn limit_check(config: &config::ServerConfig) -> DoctorCheck {
    let limits = &config.limits;
    if limits.history_batch_size == 0 || limits.max_message_bytes == 0 {
        DoctorCheck {
            level: DoctorLevel::Fail,
            name: "limits",
            detail: "history batch and max message bytes must be non-zero".into(),
        }
    } else if limits.max_message_bytes > 262_144 {
        DoctorCheck {
            level: DoctorLevel::Warn,
            name: "limits",
            detail: format!(
                "max message size is high: {} bytes",
                limits.max_message_bytes
            ),
        }
    } else {
        DoctorCheck {
            level: DoctorLevel::Pass,
            name: "limits",
            detail: format!(
                "max_message_bytes={} history_batch_size={} join_backlog_events={}",
                limits.max_message_bytes, limits.history_batch_size, limits.join_backlog_events
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    #[test]
    fn init_creates_standalone_files() {
        let root = std::env::temp_dir().join(format!(
            "omenchatd-init-test-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        let config = config::ServerConfig::for_root(root.clone());

        config::init_files(&config).expect("init files");

        assert!(config.config_path.exists());
        assert!(config.identity_path.exists());
        assert!(config.database_path.exists());

        let connection = rusqlite::Connection::open(&config.database_path).expect("open db");
        let room_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM rooms WHERE name = 'lobby'",
                [],
                |row| row.get(0),
            )
            .expect("query lobby");
        assert_eq!(room_count, 1);

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn cli_parses_run_home_and_tcp_server_override() {
        let command = CliCommand::parse([
            "run".to_string(),
            "--home".to_string(),
            "/tmp/omenchatd-test".to_string(),
            "--tcp-server".to_string(),
            "127.0.0.1:42420".to_string(),
        ]);

        assert_eq!(
            command,
            CliCommand::Run(ServerOptions {
                home: Some(PathBuf::from("/tmp/omenchatd-test")),
                tcp_server: Some(TcpServerOverride {
                    listen_ip: "127.0.0.1".into(),
                    listen_port: 42420
                }),
                tcp_client: None,
            })
        );
    }

    #[test]
    fn cli_parses_run_home_and_tcp_client_override() {
        let command = CliCommand::parse([
            "run".to_string(),
            "--home".to_string(),
            "/tmp/omenchatd-test".to_string(),
            "--tcp-client".to_string(),
            "gateway.example:42420".to_string(),
        ]);

        assert_eq!(
            command,
            CliCommand::Run(ServerOptions {
                home: Some(PathBuf::from("/tmp/omenchatd-test")),
                tcp_server: None,
                tcp_client: Some(TcpClientOverride {
                    target_host: "gateway.example".into(),
                    target_port: 42420,
                    network_name: None,
                    passphrase: None,
                }),
            })
        );
    }

    #[test]
    fn cli_parses_alpha_runbook_server_commands() {
        let home = PathBuf::from("/tmp/omenchatd-alpha");

        assert_eq!(
            CliCommand::parse([
                "init".to_string(),
                "--home".to_string(),
                home.display().to_string(),
            ]),
            CliCommand::Init(ServerOptions {
                home: Some(home.clone()),
                tcp_server: None,
                tcp_client: None,
            })
        );
        assert_eq!(
            CliCommand::parse([
                "init".to_string(),
                "--home".to_string(),
                home.display().to_string(),
                "--tcp-server".to_string(),
                "127.0.0.1:42420".to_string(),
            ]),
            CliCommand::Init(ServerOptions {
                home: Some(home.clone()),
                tcp_server: Some(TcpServerOverride {
                    listen_ip: "127.0.0.1".into(),
                    listen_port: 42420,
                }),
                tcp_client: None,
            })
        );
        assert_eq!(
            CliCommand::parse([
                "init".to_string(),
                "--home".to_string(),
                home.display().to_string(),
                "--tcp-client".to_string(),
                "gateway.example:42420".to_string(),
            ]),
            CliCommand::Init(ServerOptions {
                home: Some(home.clone()),
                tcp_server: None,
                tcp_client: Some(TcpClientOverride {
                    target_host: "gateway.example".into(),
                    target_port: 42420,
                    network_name: None,
                    passphrase: None,
                }),
            })
        );
        assert_eq!(
            CliCommand::parse([
                "run".to_string(),
                "--home".to_string(),
                home.display().to_string(),
            ]),
            CliCommand::Run(ServerOptions {
                home: Some(home.clone()),
                tcp_server: None,
                tcp_client: None,
            })
        );
        assert_eq!(
            CliCommand::parse([
                "status".to_string(),
                "--home".to_string(),
                home.display().to_string(),
            ]),
            CliCommand::Status(ServerOptions {
                home: Some(home.clone()),
                tcp_server: None,
                tcp_client: None,
            })
        );
        assert_eq!(
            CliCommand::parse([
                "config".to_string(),
                "set".to_string(),
                "--home".to_string(),
                home.display().to_string(),
                "--name".to_string(),
                "Alpha OMENchat".to_string(),
                "--operator-label".to_string(),
                "alpha-admin".to_string(),
                "--motd".to_string(),
                "Alpha launch message".to_string(),
                "--announce-interval".to_string(),
                "360".to_string(),
                "--max-message-bytes".to_string(),
                "2048".to_string(),
                "--history-batch-size".to_string(),
                "50".to_string(),
                "--join-backlog-events".to_string(),
                "50".to_string(),
                "--large-batch-threshold-bytes".to_string(),
                "4096".to_string(),
                "--rate-messages-per-minute".to_string(),
                "20".to_string(),
                "--rate-commands-per-minute".to_string(),
                "12".to_string(),
            ]),
            CliCommand::ConfigSet(
                ServerOptions {
                    home: Some(home.clone()),
                    tcp_server: None,
                    tcp_client: None,
                },
                ConfigSetOptions {
                    name: Some("Alpha OMENchat".into()),
                    operator_label: Some("alpha-admin".into()),
                    motd: Some("Alpha launch message".into()),
                    announce_interval_minutes: Some(360),
                    upload_quota_bytes: None,
                    upload_max_file_bytes: None,
                    ping_interval_seconds: None,
                    max_message_bytes: Some(2048),
                    history_batch_size: Some(50),
                    join_backlog_events: Some(50),
                    large_batch_threshold_bytes: Some(4096),
                    rate_messages_per_minute: Some(20),
                    rate_commands_per_minute: Some(12),
                }
            )
        );
        assert_eq!(
            CliCommand::parse([
                "rooms".to_string(),
                "add".to_string(),
                "#help".to_string(),
                "--home".to_string(),
                home.display().to_string(),
                "--topic".to_string(),
                "Ask OMEN related questions".to_string(),
            ]),
            CliCommand::RoomsAdd(
                ServerOptions {
                    home: Some(home.clone()),
                    tcp_server: None,
                    tcp_client: None,
                },
                RoomAddOptions {
                    name: "#help".into(),
                    topic: Some("Ask OMEN related questions".into()),
                }
            )
        );
        assert_eq!(
            CliCommand::parse([
                "tui".to_string(),
                "--home".to_string(),
                home.display().to_string(),
            ]),
            CliCommand::Tui(ServerOptions {
                home: Some(home),
                tcp_server: None,
                tcp_client: None,
            })
        );
    }

    #[test]
    fn cli_parses_admin_config_and_room_commands() {
        let config_command = CliCommand::parse([
            "config".to_string(),
            "set".to_string(),
            "--home".to_string(),
            "/tmp/omenchatd-admin".to_string(),
            "--name".to_string(),
            "Field Chat".to_string(),
            "--operator-label".to_string(),
            "field-admin".to_string(),
            "--motd".to_string(),
            "Field chat ready".to_string(),
            "--announce-interval".to_string(),
            "45".to_string(),
            "--max-message-bytes".to_string(),
            "4096".to_string(),
            "--history-batch-size".to_string(),
            "25".to_string(),
            "--join-backlog-events".to_string(),
            "12".to_string(),
            "--large-batch-threshold-bytes".to_string(),
            "8192".to_string(),
            "--rate-messages-per-minute".to_string(),
            "33".to_string(),
            "--rate-commands-per-minute".to_string(),
            "17".to_string(),
        ]);

        assert_eq!(
            config_command,
            CliCommand::ConfigSet(
                ServerOptions {
                    home: Some(PathBuf::from("/tmp/omenchatd-admin")),
                    tcp_server: None,
                    tcp_client: None,
                },
                ConfigSetOptions {
                    name: Some("Field Chat".into()),
                    operator_label: Some("field-admin".into()),
                    motd: Some("Field chat ready".into()),
                    announce_interval_minutes: Some(45),
                    upload_quota_bytes: None,
                    upload_max_file_bytes: None,
                    ping_interval_seconds: None,
                    max_message_bytes: Some(4096),
                    history_batch_size: Some(25),
                    join_backlog_events: Some(12),
                    large_batch_threshold_bytes: Some(8192),
                    rate_messages_per_minute: Some(33),
                    rate_commands_per_minute: Some(17),
                }
            )
        );

        let room_command = CliCommand::parse([
            "rooms".to_string(),
            "add".to_string(),
            "#field".to_string(),
            "--topic".to_string(),
            "Field room".to_string(),
        ]);

        assert_eq!(
            room_command,
            CliCommand::RoomsAdd(
                ServerOptions::default(),
                RoomAddOptions {
                    name: "#field".into(),
                    topic: Some("Field room".into()),
                }
            )
        );
    }

    #[test]
    fn cli_parses_interface_tcp_server_command() {
        let command = CliCommand::parse([
            "interfaces".to_string(),
            "tcp-server".to_string(),
            "0.0.0.0:42420".to_string(),
            "--home".to_string(),
            "/tmp/omenchatd-admin".to_string(),
        ]);

        assert_eq!(
            command,
            CliCommand::InterfacesTcpServer(
                ServerOptions {
                    home: Some(PathBuf::from("/tmp/omenchatd-admin")),
                    tcp_server: None,
                    tcp_client: None,
                },
                TcpServerOverride {
                    listen_ip: "0.0.0.0".into(),
                    listen_port: 42420,
                }
            )
        );
    }

    #[test]
    fn cli_parses_interface_tcp_client_command() {
        let command = CliCommand::parse([
            "interfaces".to_string(),
            "tcp-client".to_string(),
            "gateway.example:42420".to_string(),
            "--home".to_string(),
            "/tmp/omenchatd-admin".to_string(),
        ]);

        assert_eq!(
            command,
            CliCommand::InterfacesTcpClient(
                ServerOptions {
                    home: Some(PathBuf::from("/tmp/omenchatd-admin")),
                    tcp_server: None,
                    tcp_client: None,
                },
                TcpClientOverride {
                    target_host: "gateway.example".into(),
                    target_port: 42420,
                    network_name: None,
                    passphrase: None,
                }
            )
        );
    }

    #[test]
    fn cli_parses_interface_tcp_client_ifac_fields() {
        let command = CliCommand::parse([
            "interfaces".to_string(),
            "tcp-client".to_string(),
            "gateway.example:42420".to_string(),
            "--home".to_string(),
            "/tmp/omenchatd-admin".to_string(),
            "--network-name".to_string(),
            "private_ret".to_string(),
            "--passphrase".to_string(),
            "test-passphrase".to_string(),
        ]);

        assert_eq!(
            command,
            CliCommand::InterfacesTcpClient(
                ServerOptions {
                    home: Some(PathBuf::from("/tmp/omenchatd-admin")),
                    tcp_server: None,
                    tcp_client: None,
                },
                TcpClientOverride {
                    target_host: "gateway.example".into(),
                    target_port: 42420,
                    network_name: Some("private_ret".into()),
                    passphrase: Some("test-passphrase".into()),
                }
            )
        );
    }

    #[test]
    fn cli_parses_doctor_command() {
        let command = CliCommand::parse([
            "doctor".to_string(),
            "--home".to_string(),
            "/tmp/omenchatd-doctor".to_string(),
        ]);

        assert_eq!(
            command,
            CliCommand::Doctor(ServerOptions {
                home: Some(PathBuf::from("/tmp/omenchatd-doctor")),
                tcp_server: None,
                tcp_client: None,
            })
        );
    }

    #[test]
    fn cli_parses_version_command() {
        assert_eq!(
            CliCommand::parse(["--version".to_string()]),
            CliCommand::Version
        );
        assert!(compiled_feature_summary().contains("live-reticulum:"));
        assert!(!compiled_feature_summary().contains("live-rns-net:"));
    }

    #[test]
    fn doctor_reports_initialized_home_and_missing_gateway_warning() {
        let root = std::env::temp_dir().join(format!(
            "omenchatd-doctor-init-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        let config = config::ServerConfig::for_root(root.clone());
        config::init_files(&config).expect("init");

        let report = render_doctor_report(&config);

        assert!(report.contains("omenchatd doctor: warn"));
        assert!(report.contains("[PASS] config:"));
        assert!(report.contains("[PASS] database:"));
        assert!(report.contains("[WARN] nomadnet portal:"));
        assert!(report.contains("[WARN] interfaces:"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn doctor_reports_configured_tcp_client_interface() {
        let root = std::env::temp_dir().join(format!(
            "omenchatd-doctor-tcp-client-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        let config = config::ServerConfig::for_root(root.clone());
        config::init_files(&config).expect("init");
        config::write_reticulum_tcp_client_config(
            &config,
            &TcpClientOverride {
                target_host: "gateway.example".into(),
                target_port: 42420,
                network_name: None,
                passphrase: None,
            },
        )
        .expect("tcp client");

        let report = render_doctor_report(&config);

        assert!(report.contains("[PASS] interfaces: enabled TCPClientInterface found"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn config_set_persists_announce_interval_and_limits() {
        let root = std::env::temp_dir().join(format!(
            "omenchatd-config-set-limits-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let command = CliCommand::ConfigSet(
            ServerOptions {
                home: Some(root.clone()),
                tcp_server: None,
                tcp_client: None,
            },
            ConfigSetOptions {
                announce_interval_minutes: Some(22),
                upload_quota_bytes: Some(12_345_678),
                upload_max_file_bytes: Some(654_321),
                ping_interval_seconds: Some(45),
                max_message_bytes: Some(4096),
                history_batch_size: Some(25),
                join_backlog_events: Some(12),
                large_batch_threshold_bytes: Some(8192),
                rate_messages_per_minute: Some(33),
                rate_commands_per_minute: Some(17),
                ..ConfigSetOptions::default()
            },
        );

        Omenchatd.run(command).expect("config set");

        let config = config::ServerConfig::load_or_default(root.clone()).expect("load");
        assert_eq!(config.announce_interval_minutes, 22);
        assert_eq!(config.upload_quota_bytes, 12_345_678);
        assert_eq!(config.upload_max_file_bytes, 654_321);
        assert_eq!(config.ping_interval_seconds, 45);
        assert_eq!(config.limits.max_message_bytes, 4096);
        assert_eq!(config.limits.history_batch_size, 25);
        assert_eq!(config.limits.join_backlog_events, 12);
        assert_eq!(config.limits.large_batch_threshold_bytes, 8192);
        assert_eq!(config.limits.rate_messages_per_minute, 33);
        assert_eq!(config.limits.rate_commands_per_minute, 17);
        let _ = std::fs::remove_dir_all(root);
    }
}
