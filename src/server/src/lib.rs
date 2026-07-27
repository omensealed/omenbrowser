#[cfg(feature = "live-rns-net")]
compile_error!("live-rns-net was removed from omenchatd; use --features live-reticulum");

pub mod admin_db;
pub mod config;
pub mod database_recovery;
pub mod error;
pub mod live;
pub mod protocol;
#[cfg(feature = "live-reticulum")]
pub mod reticulum_live;
pub mod server_log;
pub mod session;
pub mod store;
pub mod transport;
#[cfg(feature = "tui")]
pub mod tui;
mod tui_format;
#[cfg(feature = "tui")]
mod tui_layout;
#[cfg(feature = "tui")]
mod tui_text;
pub mod upload;

use error::ServerResult;
use std::io::Read;
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CliCommand {
    Init(ServerOptions),
    Run(ServerOptions),
    Tui(ServerOptions),
    Status(ServerOptions),
    StatusJson(ServerOptions),
    Doctor(ServerOptions),
    DoctorJson(ServerOptions),
    UploadsRepairLedger(ServerOptions),
    DatabaseRestoreMigrationBackup(ServerOptions, DatabaseRestoreOptions),
    DatabaseExportSchemaFour(ServerOptions, DatabaseExportOptions),
    DatabaseExportSchemaFive(ServerOptions, DatabaseExportOptions),
    DatabaseExportSchemaSix(ServerOptions, DatabaseExportOptions),
    DatabaseExportSchemaSeven(ServerOptions, DatabaseExportOptions),
    DatabaseExportSchemaEight(ServerOptions, DatabaseExportOptions),
    DatabaseExportSchemaNine(ServerOptions, DatabaseExportOptions),
    DatabaseExportSchemaTen(ServerOptions, DatabaseExportOptions),
    DatabaseAdvanceHistoryUsage(ServerOptions, DatabaseHistoryUsageOptions),
    ConfigShow(ServerOptions),
    ConfigSet(ServerOptions, ConfigSetOptions),
    RoomsList(ServerOptions),
    RoomsListJson(ServerOptions),
    RoomsAdd(ServerOptions, RoomAddOptions),
    RoomsSetTopic(ServerOptions, RoomTopicOptions),
    RoomsSetPolicy(ServerOptions, RoomPolicyOptions),
    RoomsArchive(ServerOptions, RoomSelectOptions),
    UsersListJson(ServerOptions),
    UsersSetRole(ServerOptions, UserRoleOptions),
    InterfacesList(ServerOptions),
    InterfacesTcpServer(ServerOptions, TcpServerOverride),
    InterfacesTcpClient(ServerOptions, TcpClientOverride),
    InterfacesDeleteTcpClient(ServerOptions, TcpClientOverride),
    Invalid(String),
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
pub struct DatabaseRestoreOptions {
    pub backup: PathBuf,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DatabaseExportOptions {
    pub destination: PathBuf,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DatabaseHistoryUsageOptions {
    pub room_id: u32,
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
pub struct RoomPolicyOptions {
    pub room_id: i64,
    pub announcement_only: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RoomSelectOptions {
    pub room_id: i64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AdministrativeUserRole {
    Standard,
    Trusted,
    Moderator,
    Administrator,
}

impl AdministrativeUserRole {
    fn bits(self) -> u64 {
        match self {
            Self::Standard => 0,
            Self::Trusted => 1,
            Self::Moderator => 1 | (1 << 1),
            Self::Administrator => 1 | (1 << 1) | (1 << 2),
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Standard => "standard",
            Self::Trusted => "trusted",
            Self::Moderator => "moderator",
            Self::Administrator => "administrator",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UserRoleOptions {
    pub user_id: i64,
    pub role: AdministrativeUserRole,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TcpServerOverride {
    pub listen_ip: String,
    pub listen_port: u16,
}

#[derive(Clone, PartialEq, Eq)]
pub struct TcpClientOverride {
    pub target_host: String,
    pub target_port: u16,
    pub network_name: Option<String>,
    pub passphrase: Option<String>,
}

impl std::fmt::Debug for TcpClientOverride {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TcpClientOverride")
            .field("target_host", &self.target_host)
            .field("target_port", &self.target_port)
            .field("network_name", &self.network_name)
            .field(
                "passphrase",
                &self.passphrase.as_ref().map(|_| "<redacted>"),
            )
            .finish()
    }
}

impl CliCommand {
    pub fn parse(args: impl IntoIterator<Item = String>) -> Self {
        let args = match resolve_passphrase_args(args.into_iter().collect()) {
            Ok(args) => args,
            Err(error) => return Self::Invalid(error),
        };
        let mut args = args.into_iter();
        let Some(command) = args.next() else {
            return Self::Help;
        };
        match command.as_str() {
            "init" => Self::Init(parse_options(args)),
            "run" => Self::Run(parse_options(args)),
            "tui" => Self::Tui(parse_options(args)),
            "status" => {
                let (options, json) = parse_machine_output_options(args);
                if json {
                    Self::StatusJson(options)
                } else {
                    Self::Status(options)
                }
            }
            "doctor" => {
                let (options, json) = parse_machine_output_options(args);
                if json {
                    Self::DoctorJson(options)
                } else {
                    Self::Doctor(options)
                }
            }
            "uploads" => parse_uploads_command(args),
            "database" => parse_database_command(args),
            "config" => parse_config_command(args),
            "rooms" => parse_rooms_command(args),
            "users" => parse_users_command(args),
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
                #[cfg(feature = "tui")]
                {
                    tui::run_admin_console(config)
                }
                #[cfg(not(feature = "tui"))]
                {
                    let _ = config;
                    Err(crate::error::ServerError::Message(
                        "omenchatd tui is unavailable in this headless build; rebuild with --features server-full or tui".into(),
                    ))
                }
            }
            CliCommand::Status(options) => {
                let config = config_from_options(&options)?;
                config::init_files(&config)?;
                print!("{}", config::render_status(&config));
                Ok(())
            }
            CliCommand::StatusJson(options) => {
                let config = config_from_options(&options)?;
                config::init_files(&config)?;
                println!("{}", render_status_json(&config)?);
                Ok(())
            }
            CliCommand::Doctor(options) => {
                let config = config_from_options(&options)?;
                print!("{}", render_doctor_report(&config));
                Ok(())
            }
            CliCommand::DoctorJson(options) => {
                let config = config_from_options(&options)?;
                println!("{}", render_doctor_json(&config)?);
                Ok(())
            }
            CliCommand::UploadsRepairLedger(options) => {
                let config = config_from_options(&options)?;
                print!("{}", repair_upload_ledger(&config)?);
                Ok(())
            }
            CliCommand::DatabaseRestoreMigrationBackup(options, restore) => {
                let config = config_from_options(&options)?;
                let report = crate::database_recovery::restore_migration_backup(
                    &config.database_path,
                    &restore.backup,
                )?;
                println!(
                    "restored omenchatd database from schema v{} migration backup",
                    report.source_version
                );
                println!("database: {}", config.database_path.display());
                println!(
                    "preserved previous database: {}",
                    report.preserved_database.display()
                );
                println!(
                    "Run `omenchatd doctor --home {}` before restarting the server.",
                    config.root_dir().display()
                );
                Ok(())
            }
            CliCommand::DatabaseExportSchemaFour(options, export) => {
                let config = config_from_options(&options)?;
                let report = crate::database_recovery::export_schema_four_copy(
                    &config.database_path,
                    &export.destination,
                )?;
                println!(
                    "exported omenchatd schema-4 compatible copy from schema v{}",
                    report.source_version
                );
                println!("source database: {}", config.database_path.display());
                println!("schema-4 copy: {}", report.destination.display());
                println!(
                    "Reaction state is intentionally absent; the active database was not modified."
                );
                Ok(())
            }
            CliCommand::DatabaseExportSchemaFive(options, export) => {
                let config = config_from_options(&options)?;
                let report = crate::database_recovery::export_schema_five_copy(
                    &config.database_path,
                    &export.destination,
                )?;
                println!(
                    "exported omenchatd schema-5 compatible copy from schema v{}",
                    report.source_version
                );
                println!("source database: {}", config.database_path.display());
                println!("schema-5 copy: {}", report.destination.display());
                println!(
                    "Message revision state is intentionally absent; reaction state is preserved; the active database was not modified."
                );
                Ok(())
            }
            CliCommand::DatabaseExportSchemaSix(options, export) => {
                let config = config_from_options(&options)?;
                let report = crate::database_recovery::export_schema_six_copy(
                    &config.database_path,
                    &export.destination,
                )?;
                println!(
                    "exported omenchatd schema-6 compatible copy from schema v{}",
                    report.source_version
                );
                println!("source database: {}", config.database_path.display());
                println!("schema-6 copy: {}", report.destination.display());
                println!(
                    "Room event sequence metadata is intentionally absent; message revisions, reactions, and ordinary history are preserved; the active database was not modified."
                );
                Ok(())
            }
            CliCommand::DatabaseExportSchemaSeven(options, export) => {
                let config = config_from_options(&options)?;
                let report = crate::database_recovery::export_schema_seven_copy(
                    &config.database_path,
                    &export.destination,
                )?;
                println!(
                    "exported omenchatd schema-7 compatible copy from schema v{}",
                    report.source_version
                );
                println!("source database: {}", config.database_path.display());
                println!("schema-7 copy: {}", report.destination.display());
                println!(
                    "Room history usage metadata is intentionally absent; event sequences, message revisions, reactions, and ordinary history are preserved; the active database was not modified."
                );
                Ok(())
            }
            CliCommand::DatabaseExportSchemaEight(options, export) => {
                let config = config_from_options(&options)?;
                let report = crate::database_recovery::export_schema_eight_copy(
                    &config.database_path,
                    &export.destination,
                )?;
                println!(
                    "exported omenchatd schema-8 compatible copy from schema v{}",
                    report.source_version
                );
                println!("source database: {}", config.database_path.display());
                println!("schema-8 copy: {}", report.destination.display());
                println!(
                    "Pin state is intentionally absent; history usage, event sequences, message revisions, reactions, and ordinary history are preserved; the active database was not modified."
                );
                Ok(())
            }
            CliCommand::DatabaseExportSchemaNine(options, export) => {
                let config = config_from_options(&options)?;
                let report = crate::database_recovery::export_schema_nine_copy(
                    &config.database_path,
                    &export.destination,
                )?;
                println!(
                    "exported omenchatd schema-9 compatible copy from schema v{}",
                    report.source_version
                );
                println!("source database: {}", config.database_path.display());
                println!("schema-9 copy: {}", report.destination.display());
                println!(
                    "Moderation-audit history is intentionally absent; pin state and all earlier schema layers are preserved; the active database was not modified."
                );
                Ok(())
            }
            CliCommand::DatabaseExportSchemaTen(options, export) => {
                let config = config_from_options(&options)?;
                let report = crate::database_recovery::export_schema_ten_copy(
                    &config.database_path,
                    &export.destination,
                )?;
                println!(
                    "exported omenchatd schema-10 compatible copy from schema v{}",
                    report.source_version
                );
                println!("source database: {}", config.database_path.display());
                println!("schema-10 copy: {}", report.destination.display());
                println!(
                    "Announcement-room policy is intentionally absent; moderation-audit history and all earlier schema layers are preserved; the active database was not modified."
                );
                Ok(())
            }
            CliCommand::DatabaseAdvanceHistoryUsage(options, history) => {
                let config = config_from_options(&options)?;
                if !config.database_path.is_file() {
                    return Err(crate::error::ServerError::Message(
                        "room history usage maintenance refused: database file is missing; run `omenchatd init` only when creating a new server home"
                            .into(),
                    ));
                }
                let database =
                    admin_db::AdminDatabase::open_existing_for_maintenance(&config.database_path)?;
                let usage = database.advance_room_history_usage(history.room_id)?;
                println!(
                    "room {} history usage: events={} bytes={} cursor={} target={} complete={}",
                    history.room_id,
                    usage.event_count,
                    usage.retained_bytes,
                    usage.backfill_through_event_id,
                    usage.backfill_target_event_id,
                    usage.backfill_complete
                );
                if usage.backfill_complete {
                    println!("History usage accounting is complete. No history was deleted.");
                } else {
                    println!(
                        "One bounded accounting batch completed. Stop omenchatd and repeat this command for room {} before enabling retention.",
                        history.room_id
                    );
                }
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
                config::init_files(&config)?;
                let database = admin_db::AdminDatabase::open(&config.database_path)?;
                for room in database.list_rooms()? {
                    println!(
                        "#{name}\troom_id={room_id}\tpolicy={policy}\trevision={revision}\ttopic={topic}",
                        name = room.name,
                        room_id = room.room_id,
                        policy = if room.policy_bits == 0 {
                            "ordinary"
                        } else {
                            "announcement"
                        },
                        revision = room.room_revision,
                        topic = room.topic.unwrap_or_default()
                    );
                }
                Ok(())
            }
            CliCommand::RoomsListJson(options) => {
                let config = config_from_options(&options)?;
                config::init_files(&config)?;
                let database = admin_db::AdminDatabase::open(&config.database_path)?;
                let rooms = database
                    .list_rooms()?
                    .into_iter()
                    .map(|room| {
                        serde_json::json!({
                            "room_id": room.room_id,
                            "name": room.name,
                            "topic": room.topic,
                            "policy": if room.policy_bits == 0 {
                                "ordinary"
                            } else {
                                "announcement"
                            },
                            "policy_bits": room.policy_bits,
                            "room_revision": room.room_revision,
                        })
                    })
                    .collect::<Vec<_>>();
                println!(
                    "{}",
                    serde_json::to_string_pretty(&serde_json::json!({
                        "schema_version": 1,
                        "rooms": rooms,
                    }))
                    .map_err(|error| {
                        crate::error::ServerError::Message(format!(
                            "room status JSON encoding failed: {error}"
                        ))
                    })?
                );
                Ok(())
            }
            CliCommand::RoomsAdd(options, room) => {
                let config = config_from_options(&options)?;
                config::init_files(&config)?;
                let database = admin_db::AdminDatabase::open(&config.database_path)?;
                database.create_room(room.name.clone(), room.topic)?;
                println!("room ready: #{}", room.name.trim().trim_start_matches('#'));
                Ok(())
            }
            CliCommand::RoomsSetTopic(options, room) => {
                let config = config_from_options(&options)?;
                config::init_files(&config)?;
                let database = admin_db::AdminDatabase::open(&config.database_path)?;
                let room_id = u32::try_from(room.room_id)
                    .map_err(|_| error::ServerError::Message("room not found".into()))?;
                database.update_room_topic(room_id, room.topic)?;
                println!("room topic updated: id={}", room.room_id);
                Ok(())
            }
            CliCommand::RoomsSetPolicy(options, room) => {
                let config = config_from_options(&options)?;
                if !config.database_path.is_file() {
                    return Err(error::ServerError::Message(
                        "room policy update refused: database file is missing; initialize the server home first"
                            .into(),
                    ));
                }
                let database =
                    admin_db::AdminDatabase::open_existing_for_maintenance(&config.database_path)?;
                let room_id = u32::try_from(room.room_id)
                    .map_err(|_| error::ServerError::Message("room not found".into()))?;
                let updated =
                    database.set_room_announcement_policy(room_id, room.announcement_only)?;
                println!(
                    "room policy updated: id={} policy={} revision={}",
                    updated.room_id,
                    if updated.policy_bits == 0 {
                        "ordinary"
                    } else {
                        "announcement"
                    },
                    updated.room_revision
                );
                Ok(())
            }
            CliCommand::RoomsArchive(options, room) => {
                let config = config_from_options(&options)?;
                config::init_files(&config)?;
                let database = admin_db::AdminDatabase::open(&config.database_path)?;
                let room_id = u32::try_from(room.room_id)
                    .map_err(|_| error::ServerError::Message("room not found".into()))?;
                database.archive_room(room_id)?;
                println!("room archived: id={}", room.room_id);
                Ok(())
            }
            CliCommand::UsersListJson(options) => {
                let config = config_from_options(&options)?;
                if !config.database_path.is_file() {
                    return Err(error::ServerError::Message(
                        "user listing refused: database file is missing; initialize the server home first"
                            .into(),
                    ));
                }
                let database = admin_db::AdminDatabase::open_read_only(&config.database_path)?;
                let users = database
                    .list_users()?
                    .into_iter()
                    .map(|user| {
                        serde_json::json!({
                            "user_id": user.user.user_id,
                            "display_name": user.user.display_name,
                            "role_bits": user.user.role_bits,
                            "status_bits": user.user.status_bits,
                            "first_seen_at": user.first_seen_at,
                            "last_seen_at": user.last_seen_at,
                        })
                    })
                    .collect::<Vec<_>>();
                println!(
                    "{}",
                    serde_json::to_string_pretty(&serde_json::json!({
                        "schema_version": 1,
                        "users": users,
                    }))
                    .map_err(|error| {
                        crate::error::ServerError::Message(format!(
                            "user status JSON encoding failed: {error}"
                        ))
                    })?
                );
                Ok(())
            }
            CliCommand::UsersSetRole(options, user) => {
                let config = config_from_options(&options)?;
                if !config.database_path.is_file() {
                    return Err(error::ServerError::Message(
                        "user role update refused: database file is missing; initialize the server home first"
                            .into(),
                    ));
                }
                let database =
                    admin_db::AdminDatabase::open_existing_for_maintenance(&config.database_path)?;
                let user_id = u32::try_from(user.user_id)
                    .map_err(|_| error::ServerError::Message("user was not found".into()))?;
                let updated = database.set_user_role_bits(user_id, user.role.bits())?;
                println!(
                    "user role updated: id={} role={}",
                    updated.user_id,
                    user.role.label()
                );
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
                let name = config::add_reticulum_tcp_client_config(&config, &tcp_client)?;
                println!(
                    "added {name}: {}:{}",
                    tcp_client.target_host, tcp_client.target_port
                );
                Ok(())
            }
            CliCommand::InterfacesList(options) => {
                let config = config_from_options(&options)?;
                config::init_files(&config)?;
                let interfaces = config::list_reticulum_tcp_client_configs(&config)?;
                if interfaces.is_empty() {
                    println!("no TCP client interfaces configured");
                } else {
                    for interface in interfaces {
                        println!(
                            "{}\t{}:{}\tifac={}",
                            interface.name,
                            interface.target_host,
                            interface.target_port,
                            if interface.ifac_configured {
                                "configured"
                            } else {
                                "none"
                            }
                        );
                    }
                }
                Ok(())
            }
            CliCommand::InterfacesDeleteTcpClient(options, tcp_client) => {
                let config = config_from_options(&options)?;
                config::init_files(&config)?;
                let removed = config::delete_reticulum_tcp_client_config(
                    &config,
                    &tcp_client.target_host,
                    tcp_client.target_port,
                )?;
                println!(
                    "removed {removed} TCP client interface(s) for {}:{}",
                    tcp_client.target_host, tcp_client.target_port
                );
                Ok(())
            }
            CliCommand::Invalid(error) => Err(crate::error::ServerError::Message(error)),
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

fn parse_uploads_command(args: impl IntoIterator<Item = String>) -> CliCommand {
    let mut args = args.into_iter();
    let Some(command) = args.next() else {
        return CliCommand::Help;
    };
    if command != "repair-ledger" {
        return CliCommand::Help;
    }
    let mut confirmed = false;
    let options = args
        .filter(|arg| {
            if arg == "--confirm" {
                confirmed = true;
                false
            } else {
                true
            }
        })
        .collect::<Vec<_>>();
    if !confirmed {
        return CliCommand::Invalid(
            "upload ledger repair requires --confirm and must be run while omenchatd is stopped"
                .into(),
        );
    }
    CliCommand::UploadsRepairLedger(parse_options(options))
}

fn parse_database_command(args: impl IntoIterator<Item = String>) -> CliCommand {
    let mut args = args.into_iter();
    let Some(command) = args.next() else {
        return CliCommand::Help;
    };
    let mut confirmed = false;
    let mut path = None;
    let mut room_id = None;
    let mut options = ServerOptions::default();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--confirm" => confirmed = true,
            "--from" if command == "restore-migration-backup" => {
                path = args.next().map(PathBuf::from)
            }
            "--to"
                if matches!(
                    command.as_str(),
                    "export-schema4-copy"
                        | "export-schema5-copy"
                        | "export-schema6-copy"
                        | "export-schema7-copy"
                        | "export-schema8-copy"
                        | "export-schema9-copy"
                        | "export-schema10-copy"
                ) =>
            {
                path = args.next().map(PathBuf::from)
            }
            "--room-id" if command == "advance-history-usage" => {
                let Some(value) = args.next() else {
                    return CliCommand::Invalid(
                        "history usage maintenance requires --room-id <positive-id>".into(),
                    );
                };
                room_id = match value.parse::<u32>() {
                    Ok(value) if value > 0 => Some(value),
                    _ => {
                        return CliCommand::Invalid(
                            "history usage maintenance requires --room-id <positive-id>".into(),
                        );
                    }
                };
            }
            "--home" => options.home = args.next().map(PathBuf::from),
            other => {
                return CliCommand::Invalid(format!(
                    "unknown database maintenance option: {other}"
                ));
            }
        }
    }
    if !confirmed {
        return CliCommand::Invalid(
            "database maintenance requires --confirm and must be run while omenchatd is stopped"
                .into(),
        );
    }
    match (command.as_str(), path) {
        ("restore-migration-backup", Some(backup)) => {
            CliCommand::DatabaseRestoreMigrationBackup(options, DatabaseRestoreOptions { backup })
        }
        ("restore-migration-backup", None) => CliCommand::Invalid(
            "database restore requires --from <generated-migration-backup>".into(),
        ),
        ("export-schema4-copy", Some(destination)) => {
            CliCommand::DatabaseExportSchemaFour(options, DatabaseExportOptions { destination })
        }
        ("export-schema4-copy", None) => {
            CliCommand::Invalid("schema-4 export requires --to <new-database-path>".into())
        }
        ("export-schema5-copy", Some(destination)) => {
            CliCommand::DatabaseExportSchemaFive(options, DatabaseExportOptions { destination })
        }
        ("export-schema5-copy", None) => {
            CliCommand::Invalid("schema-5 export requires --to <new-database-path>".into())
        }
        ("export-schema6-copy", Some(destination)) => {
            CliCommand::DatabaseExportSchemaSix(options, DatabaseExportOptions { destination })
        }
        ("export-schema6-copy", None) => {
            CliCommand::Invalid("schema-6 export requires --to <new-database-path>".into())
        }
        ("export-schema7-copy", Some(destination)) => {
            CliCommand::DatabaseExportSchemaSeven(options, DatabaseExportOptions { destination })
        }
        ("export-schema7-copy", None) => {
            CliCommand::Invalid("schema-7 export requires --to <new-database-path>".into())
        }
        ("export-schema8-copy", Some(destination)) => {
            CliCommand::DatabaseExportSchemaEight(options, DatabaseExportOptions { destination })
        }
        ("export-schema8-copy", None) => {
            CliCommand::Invalid("schema-8 export requires --to <new-database-path>".into())
        }
        ("export-schema9-copy", Some(destination)) => {
            CliCommand::DatabaseExportSchemaNine(options, DatabaseExportOptions { destination })
        }
        ("export-schema9-copy", None) => {
            CliCommand::Invalid("schema-9 export requires --to <new-database-path>".into())
        }
        ("export-schema10-copy", Some(destination)) => {
            CliCommand::DatabaseExportSchemaTen(options, DatabaseExportOptions { destination })
        }
        ("export-schema10-copy", None) => {
            CliCommand::Invalid("schema-10 export requires --to <new-database-path>".into())
        }
        ("advance-history-usage", _) => match room_id {
            Some(room_id) => CliCommand::DatabaseAdvanceHistoryUsage(
                options,
                DatabaseHistoryUsageOptions { room_id },
            ),
            None => CliCommand::Invalid(
                "history usage maintenance requires --room-id <positive-id>".into(),
            ),
        },
        _ => CliCommand::Help,
    }
}

fn parse_rooms_command(args: impl IntoIterator<Item = String>) -> CliCommand {
    let mut args = args.into_iter();
    let Some(command) = args.next() else {
        return CliCommand::Help;
    };
    match command.as_str() {
        "list" => {
            let (options, json) = parse_machine_output_options(args);
            if json {
                CliCommand::RoomsListJson(options)
            } else {
                CliCommand::RoomsList(options)
            }
        }
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
        "policy" => {
            let (options, room) = parse_room_policy_options(args);
            match room {
                Some(room) => CliCommand::RoomsSetPolicy(options, room),
                None => CliCommand::Invalid(
                    "room policy requires <room_id> ordinary|announcement --confirm and must be run while omenchatd is stopped"
                        .into(),
                ),
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

fn parse_users_command(args: impl IntoIterator<Item = String>) -> CliCommand {
    let mut args = args.into_iter();
    let Some(command) = args.next() else {
        return CliCommand::Help;
    };
    match command.as_str() {
        "list" => {
            let (options, json) = parse_machine_output_options(args);
            if json {
                CliCommand::UsersListJson(options)
            } else {
                CliCommand::Invalid("user listing requires --json".into())
            }
        }
        "role" => {
            let (options, user) = parse_user_role_options(args);
            match user {
                Some(user) => CliCommand::UsersSetRole(options, user),
                None => CliCommand::Invalid(
                    "user role requires <user_id> standard|trusted|moderator|administrator --confirm and must be run while omenchatd is stopped"
                        .into(),
                ),
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
        "list" => CliCommand::InterfacesList(parse_options(args)),
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
        "delete-tcp-client" => {
            let Some(value) = args.next() else {
                return CliCommand::Help;
            };
            let Some(tcp_client) = parse_tcp_client_override(&value) else {
                return CliCommand::Help;
            };
            CliCommand::InterfacesDeleteTcpClient(parse_options(args), tcp_client)
        }
        "delete" => {
            if args.next().as_deref() != Some("tcp-client") {
                return CliCommand::Help;
            }
            let Some(value) = args.next() else {
                return CliCommand::Help;
            };
            let Some(tcp_client) = parse_tcp_client_override(&value) else {
                return CliCommand::Help;
            };
            CliCommand::InterfacesDeleteTcpClient(parse_options(args), tcp_client)
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
        config.limits.large_batch_threshold_bytes = bytes.clamp(1, 1_048_576);
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

fn parse_machine_output_options(args: impl IntoIterator<Item = String>) -> (ServerOptions, bool) {
    let args = args.into_iter().collect::<Vec<_>>();
    let json = args.iter().any(|arg| arg == "--json");
    (parse_options(args), json)
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

fn parse_room_policy_options(
    args: impl IntoIterator<Item = String>,
) -> (ServerOptions, Option<RoomPolicyOptions>) {
    let mut options = ServerOptions::default();
    let mut room_id = None;
    let mut policy = None;
    let mut confirmed = false;
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--home" => options.home = args.next().map(PathBuf::from),
            "--confirm" => confirmed = true,
            "ordinary" if policy.is_none() => policy = Some(false),
            "announcement" if policy.is_none() => policy = Some(true),
            value if room_id.is_none() => {
                room_id = value.parse::<i64>().ok().filter(|value| *value > 0)
            }
            _ => return (options, None),
        }
    }
    (
        options,
        confirmed
            .then_some(())
            .and(room_id)
            .zip(policy)
            .map(|(room_id, announcement_only)| RoomPolicyOptions {
                room_id,
                announcement_only,
            }),
    )
}

fn parse_user_role_options(
    args: impl IntoIterator<Item = String>,
) -> (ServerOptions, Option<UserRoleOptions>) {
    let mut options = ServerOptions::default();
    let mut user_id = None;
    let mut role = None;
    let mut confirmed = false;
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--home" => options.home = args.next().map(PathBuf::from),
            "--confirm" => confirmed = true,
            "standard" if role.is_none() => role = Some(AdministrativeUserRole::Standard),
            "trusted" if role.is_none() => role = Some(AdministrativeUserRole::Trusted),
            "moderator" if role.is_none() => role = Some(AdministrativeUserRole::Moderator),
            "administrator" if role.is_none() => role = Some(AdministrativeUserRole::Administrator),
            value if user_id.is_none() => {
                user_id = value.parse::<i64>().ok().filter(|value| *value > 0)
            }
            _ => return (options, None),
        }
    }
    (
        options,
        confirmed
            .then_some(())
            .and(user_id)
            .zip(role)
            .map(|(user_id, role)| UserRoleOptions { user_id, role }),
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

fn resolve_passphrase_args(args: Vec<String>) -> Result<Vec<String>, String> {
    let mut resolved = Vec::with_capacity(args.len());
    let mut args = args.into_iter();
    let mut source_seen = false;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--passphrase" | "--ifac-passphrase" => {
                ensure_single_passphrase_source(&mut source_seen)?;
                let value = args
                    .next()
                    .ok_or_else(|| format!("{arg} requires a value"))?;
                eprintln!(
                    "warning: --passphrase exposes secrets in process listings; use --passphrase-file, --passphrase-stdin, or --passphrase-prompt"
                );
                resolved.extend(["--passphrase".into(), validate_passphrase(value)?]);
            }
            "--passphrase-file" | "--ifac-passphrase-file" => {
                ensure_single_passphrase_source(&mut source_seen)?;
                let path = args
                    .next()
                    .ok_or_else(|| format!("{arg} requires a path"))?;
                let value = read_passphrase_file(std::path::Path::new(&path))?;
                resolved.extend(["--passphrase".into(), value]);
            }
            "--passphrase-stdin" | "--ifac-passphrase-stdin" => {
                ensure_single_passphrase_source(&mut source_seen)?;
                let value = read_passphrase_from_reader(std::io::stdin().lock())?;
                resolved.extend(["--passphrase".into(), value]);
            }
            "--passphrase-prompt" | "--ifac-passphrase-prompt" => {
                ensure_single_passphrase_source(&mut source_seen)?;
                let value = rpassword::prompt_password("IFAC passphrase: ")
                    .map_err(|error| format!("failed to read hidden IFAC passphrase: {error}"))?;
                resolved.extend(["--passphrase".into(), validate_passphrase(value)?]);
            }
            _ => resolved.push(arg),
        }
    }
    Ok(resolved)
}

fn ensure_single_passphrase_source(seen: &mut bool) -> Result<(), String> {
    if std::mem::replace(seen, true) {
        return Err("choose exactly one passphrase source".into());
    }
    Ok(())
}

fn read_passphrase_file(path: &std::path::Path) -> Result<String, String> {
    let metadata = std::fs::symlink_metadata(path)
        .map_err(|error| format!("failed to inspect passphrase file: {error}"))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err("passphrase file must be a regular non-symlink file".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o077 != 0 {
            return Err("passphrase file permissions must not allow group or other access".into());
        }
    }
    let file = std::fs::File::open(path)
        .map_err(|error| format!("failed to open passphrase file: {error}"))?;
    read_passphrase_from_reader(file)
}

fn read_passphrase_from_reader(reader: impl Read) -> Result<String, String> {
    let mut bytes = Vec::new();
    reader
        .take(4097)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("failed to read passphrase: {error}"))?;
    if bytes.len() > 4096 {
        return Err("passphrase input exceeds 4096 bytes".into());
    }
    let value =
        String::from_utf8(bytes).map_err(|_| "passphrase input is not valid UTF-8".to_string())?;
    validate_passphrase(value.trim_end_matches(['\r', '\n']).to_owned())
}

fn validate_passphrase(value: String) -> Result<String, String> {
    if value.is_empty() {
        return Err("passphrase must not be empty".into());
    }
    if value.contains('\0') {
        return Err("passphrase must not contain NUL bytes".into());
    }
    Ok(value)
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
    println!("  init [--home <path>] [--tcp-server <listen_ip:port>] [--tcp-client <host:port>] [--network-name <name>] [--passphrase-file <path>|--passphrase-stdin|--passphrase-prompt]");
    println!("  run [--home <path>] [--tcp-server <listen_ip:port>] [--tcp-client <host:port>] [--network-name <name>] [--passphrase-file <path>|--passphrase-stdin|--passphrase-prompt]");
    println!("  status [--home <path>] [--json]");
    println!("  doctor [--home <path>] [--json]");
    println!("  uploads repair-ledger --confirm [--home <path>]  # server must be stopped");
    println!("  database restore-migration-backup --from <path> --confirm [--home <path>]  # server must be stopped");
    println!("  database export-schema10-copy --to <new-path> --confirm [--home <path>]  # server must be stopped");
    println!("  database export-schema9-copy --to <new-path> --confirm [--home <path>]  # server must be stopped");
    println!("  database export-schema8-copy --to <new-path> --confirm [--home <path>]  # server must be stopped");
    println!("  database export-schema7-copy --to <new-path> --confirm [--home <path>]  # server must be stopped");
    println!("  database export-schema6-copy --to <new-path> --confirm [--home <path>]  # server must be stopped");
    println!("  database export-schema5-copy --to <new-path> --confirm [--home <path>]  # server must be stopped");
    println!("  database export-schema4-copy --to <new-path> --confirm [--home <path>]  # server must be stopped");
    println!("  database advance-history-usage --room-id <id> --confirm [--home <path>]  # one metadata-only batch; server must be stopped");
    println!("  config show [--home <path>]");
    println!(
        "  config set [--home <path>] [--name <name>] [--operator-label <label>] [--motd <text>] [--announce-interval <minutes>]"
    );
    println!("             [--max-message-bytes <bytes>] [--history-batch-size <count>] [--join-backlog-events <count>]");
    println!("             [--large-batch-threshold-bytes <bytes>] [--rate-messages-per-minute <count>] [--rate-commands-per-minute <count>]");
    println!("  rooms list [--home <path>] [--json]");
    println!("  rooms add <name> [--topic <topic>] [--home <path>]");
    println!("  rooms topic <room_id> [--topic <topic>] [--home <path>]");
    println!("  rooms policy <room_id> ordinary|announcement --confirm [--home <path>]  # server must be stopped");
    println!("  rooms archive <room_id> [--home <path>]");
    println!("  users list --json [--home <path>]");
    println!("  users role <user_id> standard|trusted|moderator|administrator --confirm [--home <path>]  # server must be stopped");
    println!("  interfaces list [--home <path>]");
    println!("  interfaces tcp-server <listen_ip:port> [--home <path>]");
    println!("  interfaces tcp-client <host:port> [--home <path>] [--network-name <name>] [--passphrase-file <path>|--passphrase-stdin|--passphrase-prompt]  # add without replacing existing clients");
    println!("  interfaces delete tcp-client <host:port> [--home <path>]");
    println!("  --passphrase <pass> is deprecated because argv may be visible to other processes");
    if cfg!(feature = "tui") {
        println!("  tui [--home <path>]");
    } else {
        println!("  tui [unavailable in this headless build]");
    }
}

fn print_version() {
    println!(
        "omenchatd {} features={}",
        env!("CARGO_PKG_VERSION"),
        compiled_feature_summary()
    );
}

fn compiled_feature_summary() -> String {
    [
        ("server-headless", cfg!(feature = "server-headless")),
        ("server-full", cfg!(feature = "server-full")),
        ("live-reticulum", cfg!(feature = "live-reticulum")),
        ("tui", cfg!(feature = "tui")),
    ]
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

    fn machine_label(self) -> &'static str {
        match self {
            Self::Pass => "pass",
            Self::Warn => "warn",
            Self::Fail => "fail",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct DoctorCheck {
    level: DoctorLevel,
    name: &'static str,
    detail: String,
}

fn json_string(value: &serde_json::Value) -> ServerResult<String> {
    serde_json::to_string_pretty(value)
        .map_err(|error| crate::error::ServerError::Message(format!("JSON output failed: {error}")))
}

fn runtime_mode_label() -> &'static str {
    if cfg!(feature = "live-reticulum") {
        "independent-in-process-reticulum"
    } else {
        "transport-disabled"
    }
}

fn render_status_json(config: &config::ServerConfig) -> ServerResult<String> {
    let room_result = config::list_rooms(config);
    let room_count = room_result.as_ref().map(Vec::len).ok();
    let history_accounting = store::OmenchatStore::open_read_only(&config.database_path)
        .and_then(|store| store.room_history_maintenance_status(256))
        .map(|status| {
            serde_json::json!({
                "state": "available",
                "inspected_rooms": status.inspected_rooms,
                "more_rooms": status.more_rooms,
                "complete_ledgers": status.complete_ledgers,
                "incomplete_ledgers": status.incomplete_ledgers,
                "missing_ledgers": status.missing_ledgers,
                "accounted_events": status.accounted_events,
                "accounted_bytes": status.accounted_bytes,
            })
        })
        .unwrap_or_else(|_| serde_json::json!({ "state": "unavailable" }));
    let public_addresses = config::render_public_addresses(config)
        .lines()
        .filter(|line| {
            [
                "identity hash: ",
                "destination: ",
                "client uri: ",
                "nomadnet portal: ",
                "portal url: ",
            ]
            .iter()
            .any(|prefix| line.starts_with(prefix))
                && !line.contains("unavailable")
        })
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let interface = interface_check(&config.reticulum_config_file());
    json_string(&serde_json::json!({
        "schema_version": 1,
        "application": {
            "name": "omenchatd",
            "version": env!("CARGO_PKG_VERSION"),
        },
        "dependency_train": {
            "reticulum_rs": "0.9.6",
            "lxmf": null,
        },
        "runtime": {
            "mode": runtime_mode_label(),
            "shared_instance": false,
            "live_metrics_available": false,
        },
        "services": {
            "chat": "omenchat.node",
            "portal": "nomadnetwork.node:/page/index.mu",
            "public_addresses": public_addresses,
        },
        "storage": {
            "config_present": config.config_path.is_file(),
            "identity_present": config.identity_path.is_file(),
            "database_present": config.database_path.is_file(),
            "reticulum_config_present": config.reticulum_config_file().is_file(),
            "reticulum_storage_present": config.reticulum_storage_path().is_dir(),
            "portal_present": config.nomadnet_index_page_path().is_file(),
        },
        "interfaces": {
            "level": interface.level.machine_label(),
        },
        "rooms": {
            "catalog": if room_result.is_ok() { "ok" } else { "error" },
            "count": room_count,
        },
        "history_retention": {
            "enabled": config.history_retention.enabled,
            "admission_compaction_enabled": config.history_retention.enabled,
            "runtime_activity_observable": false,
            "max_age_days": config.history_retention.max_age_days,
            "max_events_per_room": config.history_retention.max_events_per_room,
            "max_bytes_per_room": config.history_retention.max_bytes_per_room,
            "accounting": history_accounting,
        },
        "limits": {
            "max_message_bytes": config.limits.max_message_bytes,
            "history_batch_size": config.limits.history_batch_size,
            "join_backlog_events": config.limits.join_backlog_events,
            "large_batch_threshold_bytes": config.limits.large_batch_threshold_bytes,
            "rate_messages_per_minute": config.limits.rate_messages_per_minute,
            "rate_commands_per_minute": config.limits.rate_commands_per_minute,
            "upload_quota_bytes": config.upload_quota_bytes,
            "upload_max_file_bytes": config.upload_max_file_bytes,
        },
        "redaction": "private paths, credentials, private identity material, operator label, MOTD, and free-form errors omitted",
    }))
}

fn render_doctor_json(config: &config::ServerConfig) -> ServerResult<String> {
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
    let checks = checks
        .into_iter()
        .map(|check| {
            serde_json::json!({
                "name": check.name,
                "level": check.level.machine_label(),
            })
        })
        .collect::<Vec<_>>();
    json_string(&serde_json::json!({
        "schema_version": 1,
        "application": {
            "name": "omenchatd",
            "version": env!("CARGO_PKG_VERSION"),
        },
        "outcome": outcome,
        "fail_count": fail_count,
        "warn_count": warn_count,
        "checks": checks,
        "redaction": "check details and private paths omitted",
    }))
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
    checks.push(upload_ledger_check(config));
    checks
}

fn upload_ledger_check(config: &config::ServerConfig) -> DoctorCheck {
    if !config.database_path.is_file() {
        return DoctorCheck {
            level: DoctorLevel::Warn,
            name: "upload ledger",
            detail: "database is unavailable; upload reconciliation was not run".into(),
        };
    }
    let database = match crate::admin_db::AdminDatabase::open_read_only(&config.database_path) {
        Ok(database) => database,
        Err(error) => {
            return DoctorCheck {
                level: DoctorLevel::Fail,
                name: "upload ledger",
                detail: format!("read-only database inspection failed: {error}"),
            };
        }
    };
    let report = match database.inspect_upload_ledgers(config.upload_cache_path()) {
        Ok(report) => report,
        Err(error) => {
            return DoctorCheck {
                level: DoctorLevel::Fail,
                name: "upload ledger",
                detail: format!("identity reconciliation failed: {error}"),
            };
        }
    };
    let detail = format!(
        "tracked={} files/{} disk={} files/{} missing={} mismatched={} orphan={} unsafe={}",
        report.tracked_files,
        crate::tui_format::human_bytes(report.tracked_bytes),
        report.disk_files,
        crate::tui_format::human_bytes(report.disk_bytes),
        report.missing,
        report.mismatched,
        report.orphans,
        report.unsafe_paths,
    );
    DoctorCheck {
        level: if report.unsafe_paths > 0 {
            DoctorLevel::Fail
        } else if report.missing > 0 || report.mismatched > 0 || report.orphans > 0 {
            DoctorLevel::Warn
        } else {
            DoctorLevel::Pass
        },
        name: "upload ledger",
        detail,
    }
}

fn repair_upload_ledger(config: &config::ServerConfig) -> ServerResult<String> {
    if !config.database_path.is_file() {
        return Err(crate::error::ServerError::Message(
            "upload ledger repair refused: database file is missing; run `omenchatd init` only when creating a new server home"
                .into(),
        ));
    }
    let database =
        crate::admin_db::AdminDatabase::open_existing_for_maintenance(&config.database_path)?;
    let repair = database.repair_upload_ledgers(config.upload_cache_path())?;
    Ok(format!(
        "omenchatd upload ledger repair: removed_missing={} removed_unsafe={} preserved_orphans={}\nNo files were deleted. Run `omenchatd doctor --home {}` to verify the repaired ledger.\n",
        repair.removed_missing,
        repair.removed_unsafe,
        repair.preserved_orphans,
        config.root_dir().display()
    ))
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
    fn cli_parses_machine_readable_status_and_doctor_modes() {
        let home = PathBuf::from("/tmp/omenchatd-machine-status");
        assert_eq!(
            CliCommand::parse([
                "status".to_string(),
                "--json".to_string(),
                "--home".to_string(),
                home.display().to_string(),
            ]),
            CliCommand::StatusJson(ServerOptions {
                home: Some(home.clone()),
                ..ServerOptions::default()
            })
        );
        assert_eq!(
            CliCommand::parse([
                "doctor".to_string(),
                "--home".to_string(),
                home.display().to_string(),
                "--json".to_string(),
            ]),
            CliCommand::DoctorJson(ServerOptions {
                home: Some(home),
                ..ServerOptions::default()
            })
        );
    }

    #[test]
    fn machine_readable_status_and_doctor_are_valid_and_redacted() {
        let root = std::env::temp_dir().join(format!(
            "omenchatd-machine-report-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        let mut config = config::ServerConfig::for_root(root.clone());
        config.operator_label = "operator-secret".into();
        config.motd = "motd-secret".into();
        config::init_files(&config).expect("isolated server root");

        let status = render_status_json(&config).expect("status json");
        let status_value: serde_json::Value = serde_json::from_str(&status).expect("valid status");
        assert_eq!(status_value["schema_version"], 1);
        assert_eq!(
            status_value["application"]["version"],
            env!("CARGO_PKG_VERSION")
        );
        assert_eq!(status_value["dependency_train"]["reticulum_rs"], "0.9.6");
        assert_eq!(status_value["runtime"]["mode"], runtime_mode_label());
        assert_eq!(status_value["history_retention"]["enabled"], false);
        assert_eq!(
            status_value["history_retention"]["admission_compaction_enabled"],
            false
        );
        assert_eq!(
            status_value["history_retention"]["runtime_activity_observable"],
            false
        );
        assert_eq!(
            status_value["history_retention"]["accounting"]["state"],
            "available"
        );
        assert_eq!(
            status_value["history_retention"]["accounting"]["missing_ledgers"],
            1
        );

        let doctor = render_doctor_json(&config).expect("doctor json");
        let doctor_value: serde_json::Value = serde_json::from_str(&doctor).expect("valid doctor");
        assert!(doctor_value["checks"]
            .as_array()
            .is_some_and(|checks| !checks.is_empty()));
        for report in [&status, &doctor] {
            assert!(!report.contains(root.to_string_lossy().as_ref()));
            assert!(!report.contains("operator-secret"));
            assert!(!report.contains("motd-secret"));
            assert!(!report.contains("passphrase"));
        }

        let _ = std::fs::remove_dir_all(root);
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
    fn cli_parses_release_runbook_server_commands() {
        let home = PathBuf::from("/tmp/omenchatd-test");

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
                "Release OMENchat".to_string(),
                "--operator-label".to_string(),
                "release-admin".to_string(),
                "--motd".to_string(),
                "Release launch message".to_string(),
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
                    name: Some("Release OMENchat".into()),
                    operator_label: Some("release-admin".into()),
                    motd: Some("Release launch message".into()),
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
        assert_eq!(
            CliCommand::parse([
                "rooms".to_string(),
                "list".to_string(),
                "--json".to_string(),
                "--home".to_string(),
                "/tmp/omenchatd-admin".to_string(),
            ]),
            CliCommand::RoomsListJson(ServerOptions {
                home: Some(PathBuf::from("/tmp/omenchatd-admin")),
                tcp_server: None,
                tcp_client: None,
            })
        );

        assert!(matches!(
            CliCommand::parse([
                "rooms".to_string(),
                "policy".to_string(),
                "7".to_string(),
                "announcement".to_string(),
            ]),
            CliCommand::Invalid(message) if message.contains("--confirm")
        ));
        assert!(matches!(
            CliCommand::parse([
                "rooms".to_string(),
                "policy".to_string(),
                "7".to_string(),
                "unsupported".to_string(),
                "--confirm".to_string(),
            ]),
            CliCommand::Invalid(message) if message.contains("ordinary|announcement")
        ));
        assert_eq!(
            CliCommand::parse([
                "rooms".to_string(),
                "policy".to_string(),
                "7".to_string(),
                "announcement".to_string(),
                "--confirm".to_string(),
                "--home".to_string(),
                "/tmp/omenchatd-admin".to_string(),
            ]),
            CliCommand::RoomsSetPolicy(
                ServerOptions {
                    home: Some(PathBuf::from("/tmp/omenchatd-admin")),
                    tcp_server: None,
                    tcp_client: None,
                },
                RoomPolicyOptions {
                    room_id: 7,
                    announcement_only: true,
                }
            )
        );
        assert_eq!(
            CliCommand::parse([
                "users".to_string(),
                "list".to_string(),
                "--json".to_string(),
                "--home".to_string(),
                "/tmp/omenchatd-admin".to_string(),
            ]),
            CliCommand::UsersListJson(ServerOptions {
                home: Some(PathBuf::from("/tmp/omenchatd-admin")),
                tcp_server: None,
                tcp_client: None,
            })
        );
        assert_eq!(
            CliCommand::parse([
                "users".to_string(),
                "role".to_string(),
                "12".to_string(),
                "moderator".to_string(),
                "--confirm".to_string(),
                "--home".to_string(),
                "/tmp/omenchatd-admin".to_string(),
            ]),
            CliCommand::UsersSetRole(
                ServerOptions {
                    home: Some(PathBuf::from("/tmp/omenchatd-admin")),
                    tcp_server: None,
                    tcp_client: None,
                },
                UserRoleOptions {
                    user_id: 12,
                    role: AdministrativeUserRole::Moderator,
                }
            )
        );
        assert!(matches!(
            CliCommand::parse([
                "users".to_string(),
                "role".to_string(),
                "12".to_string(),
                "moderator".to_string(),
            ]),
            CliCommand::Invalid(message) if message.contains("--confirm")
        ));
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
    fn cli_parses_interface_list_and_tcp_client_delete_commands() {
        let home = PathBuf::from("/tmp/omenchatd-admin");
        assert_eq!(
            CliCommand::parse([
                "interfaces".into(),
                "list".into(),
                "--home".into(),
                home.display().to_string(),
            ]),
            CliCommand::InterfacesList(ServerOptions {
                home: Some(home.clone()),
                tcp_server: None,
                tcp_client: None,
            })
        );
        assert_eq!(
            CliCommand::parse([
                "interfaces".into(),
                "delete".into(),
                "tcp-client".into(),
                "gateway.example:42420".into(),
                "--home".into(),
                home.display().to_string(),
            ]),
            CliCommand::InterfacesDeleteTcpClient(
                ServerOptions {
                    home: Some(home),
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
    fn cli_requires_confirmation_for_upload_ledger_repair() {
        assert!(matches!(
            CliCommand::parse(["uploads".to_string(), "repair-ledger".to_string()]),
            CliCommand::Invalid(message) if message.contains("--confirm")
        ));
        assert_eq!(
            CliCommand::parse([
                "uploads".to_string(),
                "repair-ledger".to_string(),
                "--home".to_string(),
                "/tmp/omenchatd-repair".to_string(),
                "--confirm".to_string(),
            ]),
            CliCommand::UploadsRepairLedger(ServerOptions {
                home: Some(PathBuf::from("/tmp/omenchatd-repair")),
                tcp_server: None,
                tcp_client: None,
            })
        );
    }

    #[test]
    fn cli_requires_explicit_source_and_confirmation_for_database_restore() {
        assert!(matches!(
            CliCommand::parse([
                "database".to_string(),
                "restore-migration-backup".to_string(),
                "--from".to_string(),
                "/tmp/backup.sqlite".to_string(),
            ]),
            CliCommand::Invalid(message) if message.contains("--confirm")
        ));
        assert!(matches!(
            CliCommand::parse([
                "database".to_string(),
                "restore-migration-backup".to_string(),
                "--confirm".to_string(),
            ]),
            CliCommand::Invalid(message) if message.contains("--from")
        ));
        assert_eq!(
            CliCommand::parse([
                "database".to_string(),
                "restore-migration-backup".to_string(),
                "--from".to_string(),
                "/tmp/omenchat.sqlite.pre-v2-from-v1.bak".to_string(),
                "--confirm".to_string(),
                "--home".to_string(),
                "/tmp/omenchatd-restore".to_string(),
            ]),
            CliCommand::DatabaseRestoreMigrationBackup(
                ServerOptions {
                    home: Some(PathBuf::from("/tmp/omenchatd-restore")),
                    tcp_server: None,
                    tcp_client: None,
                },
                DatabaseRestoreOptions {
                    backup: PathBuf::from("/tmp/omenchat.sqlite.pre-v2-from-v1.bak"),
                }
            )
        );
    }

    #[test]
    fn cli_requires_new_destination_and_confirmation_for_schema_four_export() {
        assert!(matches!(
            CliCommand::parse([
                "database".to_string(),
                "export-schema4-copy".to_string(),
                "--to".to_string(),
                "/tmp/omenchat-schema4.sqlite".to_string(),
            ]),
            CliCommand::Invalid(message) if message.contains("--confirm")
        ));
        assert!(matches!(
            CliCommand::parse([
                "database".to_string(),
                "export-schema4-copy".to_string(),
                "--confirm".to_string(),
            ]),
            CliCommand::Invalid(message) if message.contains("--to")
        ));
        assert_eq!(
            CliCommand::parse([
                "database".to_string(),
                "export-schema4-copy".to_string(),
                "--to".to_string(),
                "/tmp/omenchat-schema4.sqlite".to_string(),
                "--confirm".to_string(),
                "--home".to_string(),
                "/tmp/omenchatd-export".to_string(),
            ]),
            CliCommand::DatabaseExportSchemaFour(
                ServerOptions {
                    home: Some(PathBuf::from("/tmp/omenchatd-export")),
                    tcp_server: None,
                    tcp_client: None,
                },
                DatabaseExportOptions {
                    destination: PathBuf::from("/tmp/omenchat-schema4.sqlite"),
                }
            )
        );
    }

    #[test]
    fn cli_requires_new_destination_and_confirmation_for_schema_five_export() {
        assert!(matches!(
            CliCommand::parse([
                "database".to_string(),
                "export-schema5-copy".to_string(),
                "--to".to_string(),
                "/tmp/omenchat-schema5.sqlite".to_string(),
            ]),
            CliCommand::Invalid(message) if message.contains("--confirm")
        ));
        assert!(matches!(
            CliCommand::parse([
                "database".to_string(),
                "export-schema5-copy".to_string(),
                "--confirm".to_string(),
            ]),
            CliCommand::Invalid(message) if message.contains("--to")
        ));
        assert_eq!(
            CliCommand::parse([
                "database".to_string(),
                "export-schema5-copy".to_string(),
                "--to".to_string(),
                "/tmp/omenchat-schema5.sqlite".to_string(),
                "--confirm".to_string(),
                "--home".to_string(),
                "/tmp/omenchatd-export".to_string(),
            ]),
            CliCommand::DatabaseExportSchemaFive(
                ServerOptions {
                    home: Some(PathBuf::from("/tmp/omenchatd-export")),
                    tcp_server: None,
                    tcp_client: None,
                },
                DatabaseExportOptions {
                    destination: PathBuf::from("/tmp/omenchat-schema5.sqlite"),
                }
            )
        );
    }

    #[test]
    fn cli_requires_new_destination_and_confirmation_for_schema_six_export() {
        assert!(matches!(
            CliCommand::parse([
                "database".to_string(),
                "export-schema6-copy".to_string(),
                "--to".to_string(),
                "/tmp/omenchat-schema6.sqlite".to_string(),
            ]),
            CliCommand::Invalid(message) if message.contains("--confirm")
        ));
        assert!(matches!(
            CliCommand::parse([
                "database".to_string(),
                "export-schema6-copy".to_string(),
                "--confirm".to_string(),
            ]),
            CliCommand::Invalid(message) if message.contains("--to")
        ));
        assert_eq!(
            CliCommand::parse([
                "database".to_string(),
                "export-schema6-copy".to_string(),
                "--to".to_string(),
                "/tmp/omenchat-schema6.sqlite".to_string(),
                "--confirm".to_string(),
                "--home".to_string(),
                "/tmp/omenchatd-export".to_string(),
            ]),
            CliCommand::DatabaseExportSchemaSix(
                ServerOptions {
                    home: Some(PathBuf::from("/tmp/omenchatd-export")),
                    tcp_server: None,
                    tcp_client: None,
                },
                DatabaseExportOptions {
                    destination: PathBuf::from("/tmp/omenchat-schema6.sqlite"),
                }
            )
        );
    }

    #[test]
    fn cli_requires_new_destination_and_confirmation_for_schema_seven_export() {
        assert!(matches!(
            CliCommand::parse([
                "database".to_string(),
                "export-schema7-copy".to_string(),
                "--to".to_string(),
                "/tmp/omenchat-schema7.sqlite".to_string(),
            ]),
            CliCommand::Invalid(message) if message.contains("--confirm")
        ));
        assert!(matches!(
            CliCommand::parse([
                "database".to_string(),
                "export-schema7-copy".to_string(),
                "--confirm".to_string(),
            ]),
            CliCommand::Invalid(message) if message.contains("--to")
        ));
        assert_eq!(
            CliCommand::parse([
                "database".to_string(),
                "export-schema7-copy".to_string(),
                "--to".to_string(),
                "/tmp/omenchat-schema7.sqlite".to_string(),
                "--confirm".to_string(),
                "--home".to_string(),
                "/tmp/omenchatd-export".to_string(),
            ]),
            CliCommand::DatabaseExportSchemaSeven(
                ServerOptions {
                    home: Some(PathBuf::from("/tmp/omenchatd-export")),
                    tcp_server: None,
                    tcp_client: None,
                },
                DatabaseExportOptions {
                    destination: PathBuf::from("/tmp/omenchat-schema7.sqlite"),
                }
            )
        );
    }

    #[test]
    fn cli_requires_new_destination_and_confirmation_for_schema_eight_export() {
        assert!(matches!(
            CliCommand::parse([
                "database".to_string(),
                "export-schema8-copy".to_string(),
                "--to".to_string(),
                "/tmp/omenchat-schema8.sqlite".to_string(),
            ]),
            CliCommand::Invalid(message) if message.contains("--confirm")
        ));
        assert!(matches!(
            CliCommand::parse([
                "database".to_string(),
                "export-schema8-copy".to_string(),
                "--confirm".to_string(),
            ]),
            CliCommand::Invalid(message) if message.contains("--to")
        ));
        assert_eq!(
            CliCommand::parse([
                "database".to_string(),
                "export-schema8-copy".to_string(),
                "--to".to_string(),
                "/tmp/omenchat-schema8.sqlite".to_string(),
                "--confirm".to_string(),
                "--home".to_string(),
                "/tmp/omenchatd-export".to_string(),
            ]),
            CliCommand::DatabaseExportSchemaEight(
                ServerOptions {
                    home: Some(PathBuf::from("/tmp/omenchatd-export")),
                    tcp_server: None,
                    tcp_client: None,
                },
                DatabaseExportOptions {
                    destination: PathBuf::from("/tmp/omenchat-schema8.sqlite"),
                }
            )
        );
    }

    #[test]
    fn cli_requires_new_destination_and_confirmation_for_schema_nine_export() {
        assert!(matches!(
            CliCommand::parse([
                "database".to_string(),
                "export-schema9-copy".to_string(),
                "--to".to_string(),
                "/tmp/omenchat-schema9.sqlite".to_string(),
            ]),
            CliCommand::Invalid(message) if message.contains("--confirm")
        ));
        assert!(matches!(
            CliCommand::parse([
                "database".to_string(),
                "export-schema9-copy".to_string(),
                "--confirm".to_string(),
            ]),
            CliCommand::Invalid(message) if message.contains("--to")
        ));
        assert_eq!(
            CliCommand::parse([
                "database".to_string(),
                "export-schema9-copy".to_string(),
                "--to".to_string(),
                "/tmp/omenchat-schema9.sqlite".to_string(),
                "--confirm".to_string(),
                "--home".to_string(),
                "/tmp/omenchatd-export".to_string(),
            ]),
            CliCommand::DatabaseExportSchemaNine(
                ServerOptions {
                    home: Some(PathBuf::from("/tmp/omenchatd-export")),
                    tcp_server: None,
                    tcp_client: None,
                },
                DatabaseExportOptions {
                    destination: PathBuf::from("/tmp/omenchat-schema9.sqlite"),
                }
            )
        );
    }

    #[test]
    fn cli_requires_new_destination_and_confirmation_for_schema_ten_export() {
        assert!(matches!(
            CliCommand::parse([
                "database".to_string(),
                "export-schema10-copy".to_string(),
                "--to".to_string(),
                "/tmp/omenchat-schema10.sqlite".to_string(),
            ]),
            CliCommand::Invalid(message) if message.contains("--confirm")
        ));
        assert!(matches!(
            CliCommand::parse([
                "database".to_string(),
                "export-schema10-copy".to_string(),
                "--confirm".to_string(),
            ]),
            CliCommand::Invalid(message) if message.contains("--to")
        ));
        assert_eq!(
            CliCommand::parse([
                "database".to_string(),
                "export-schema10-copy".to_string(),
                "--to".to_string(),
                "/tmp/omenchat-schema10.sqlite".to_string(),
                "--confirm".to_string(),
                "--home".to_string(),
                "/tmp/omenchatd-export".to_string(),
            ]),
            CliCommand::DatabaseExportSchemaTen(
                ServerOptions {
                    home: Some(PathBuf::from("/tmp/omenchatd-export")),
                    tcp_server: None,
                    tcp_client: None,
                },
                DatabaseExportOptions {
                    destination: PathBuf::from("/tmp/omenchat-schema10.sqlite"),
                }
            )
        );
    }

    #[test]
    fn cli_requires_room_and_confirmation_for_history_usage_maintenance() {
        assert!(matches!(
            CliCommand::parse([
                "database".to_string(),
                "advance-history-usage".to_string(),
                "--room-id".to_string(),
                "7".to_string(),
            ]),
            CliCommand::Invalid(message) if message.contains("--confirm")
        ));
        assert!(matches!(
            CliCommand::parse([
                "database".to_string(),
                "advance-history-usage".to_string(),
                "--confirm".to_string(),
            ]),
            CliCommand::Invalid(message) if message.contains("--room-id")
        ));
        assert!(matches!(
            CliCommand::parse([
                "database".to_string(),
                "advance-history-usage".to_string(),
                "--room-id".to_string(),
                "0".to_string(),
                "--confirm".to_string(),
            ]),
            CliCommand::Invalid(message) if message.contains("positive-id")
        ));
        assert_eq!(
            CliCommand::parse([
                "database".to_string(),
                "advance-history-usage".to_string(),
                "--room-id".to_string(),
                "7".to_string(),
                "--confirm".to_string(),
                "--home".to_string(),
                "/tmp/omenchatd-history-usage".to_string(),
            ]),
            CliCommand::DatabaseAdvanceHistoryUsage(
                ServerOptions {
                    home: Some(PathBuf::from("/tmp/omenchatd-history-usage")),
                    tcp_server: None,
                    tcp_client: None,
                },
                DatabaseHistoryUsageOptions { room_id: 7 },
            )
        );
    }

    #[test]
    fn cli_history_usage_maintenance_advances_one_bounded_batch_per_invocation() {
        let root = std::env::temp_dir().join(format!(
            "omenchatd-cli-history-usage-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        let config = config::ServerConfig::for_root(root.clone());
        config::init_files(&config).expect("initialize isolated home");
        let store = crate::store::OmenchatStore::open(&config.database_path)
            .expect("migrate current database");
        drop(store);
        let missing = Omenchatd
            .run(CliCommand::DatabaseAdvanceHistoryUsage(
                ServerOptions {
                    home: Some(root.clone()),
                    ..ServerOptions::default()
                },
                DatabaseHistoryUsageOptions { room_id: 99 },
            ))
            .expect_err("unknown room must fail")
            .to_string();
        assert!(missing.contains("room 99 was not found"), "{missing}");
        let store = crate::store::OmenchatStore::open(&config.database_path)
            .expect("reopen current database");
        for event_id in 1..=300 {
            store
                .append_event(
                    1,
                    None,
                    crate::store::ServerRoomEventKind::Message {
                        body: format!("legacy-{event_id}"),
                    },
                )
                .expect("seed history");
        }
        drop(store);
        let fixture = rusqlite::Connection::open(&config.database_path).expect("fixture database");
        fixture
            .execute(
                "UPDATE room_history_usage
                 SET event_count = 0, retained_bytes = 0,
                     backfill_through_event_id = 0,
                     backfill_target_event_id = 300,
                     backfill_complete = 0
                 WHERE room_id = 1",
                [],
            )
            .expect("reset usage fixture");
        drop(fixture);

        let command = || {
            CliCommand::DatabaseAdvanceHistoryUsage(
                ServerOptions {
                    home: Some(root.clone()),
                    ..ServerOptions::default()
                },
                DatabaseHistoryUsageOptions { room_id: 1 },
            )
        };
        Omenchatd
            .run(command())
            .expect("first bounded maintenance batch");
        let store =
            crate::store::OmenchatStore::open_existing_for_maintenance(&config.database_path)
                .expect("inspect first batch");
        let first = store
            .room_history_usage(1)
            .expect("usage")
            .expect("usage row");
        assert_eq!(first.event_count, 256);
        assert_eq!(first.backfill_through_event_id, 256);
        assert!(!first.backfill_complete);
        drop(store);

        Omenchatd
            .run(command())
            .expect("final bounded maintenance batch");
        let store =
            crate::store::OmenchatStore::open_existing_for_maintenance(&config.database_path)
                .expect("inspect final batch");
        let complete = store
            .room_history_usage(1)
            .expect("usage")
            .expect("usage row");
        assert_eq!(complete.event_count, 300);
        assert_eq!(complete.backfill_through_event_id, 300);
        assert!(complete.backfill_complete);
        assert_eq!(
            store
                .latest_events(1, 400)
                .expect("preserved history")
                .len(),
            300
        );
        drop(store);
        std::fs::remove_dir_all(root).expect("remove isolated history usage home");
    }

    #[test]
    fn cli_database_restore_uses_only_the_selected_isolated_home() {
        let root = std::env::temp_dir().join(format!(
            "omenchatd-cli-database-restore-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        let config = config::ServerConfig::for_root(root.clone());
        config::init_files(&config).expect("initialize isolated home");
        let current = crate::store::OmenchatStore::open(&config.database_path)
            .expect("open current database");
        current
            .ensure_room("current-cli", None)
            .expect("current marker");
        drop(current);

        let backup = crate::store::migration_backup_path(&config.database_path, 1);
        let source = rusqlite::Connection::open(&backup).expect("restore source");
        source
            .execute_batch(include_str!("../migrations/001_init.sql"))
            .expect("source schema");
        source
            .execute(
                "INSERT INTO rooms(name, created_at) VALUES ('restored-cli', 1)",
                [],
            )
            .expect("source marker");
        source
            .pragma_update(None, "user_version", 1)
            .expect("source version");
        drop(source);

        Omenchatd
            .run(CliCommand::DatabaseRestoreMigrationBackup(
                ServerOptions {
                    home: Some(root.clone()),
                    ..ServerOptions::default()
                },
                DatabaseRestoreOptions {
                    backup: backup.clone(),
                },
            ))
            .expect("CLI restore");

        let restored =
            crate::store::OmenchatStore::open_existing_for_maintenance(&config.database_path)
                .expect("restored database");
        assert!(restored
            .room_by_name("restored-cli")
            .expect("restored marker")
            .is_some());
        assert!(restored
            .room_by_name("current-cli")
            .expect("current marker absent")
            .is_none());
        drop(restored);
        assert!(backup.is_file(), "selected source remains intact");
        std::fs::remove_dir_all(root).expect("remove isolated CLI restore home");
    }

    #[test]
    fn cli_schema_four_export_uses_only_the_selected_isolated_home() {
        let root = std::env::temp_dir().join(format!(
            "omenchatd-cli-database-export-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        let config = config::ServerConfig::for_root(root.clone());
        config::init_files(&config).expect("initialize isolated home");
        let current = crate::store::OmenchatStore::open(&config.database_path)
            .expect("open current database");
        current
            .ensure_room("exported-cli", None)
            .expect("current marker");
        drop(current);
        let destination = root.join("operator-schema4.sqlite");

        Omenchatd
            .run(CliCommand::DatabaseExportSchemaFour(
                ServerOptions {
                    home: Some(root.clone()),
                    ..ServerOptions::default()
                },
                DatabaseExportOptions {
                    destination: destination.clone(),
                },
            ))
            .expect("CLI schema four export");

        let exported = rusqlite::Connection::open_with_flags(
            &destination,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .expect("exported database");
        assert_eq!(
            exported
                .pragma_query_value::<i64, _>(None, "user_version", |row| row.get(0))
                .expect("exported version"),
            4
        );
        assert_eq!(
            exported
                .query_row(
                    "SELECT COUNT(*) FROM rooms WHERE name = 'exported-cli'",
                    [],
                    |row| row.get::<_, i64>(0)
                )
                .expect("exported marker"),
            1
        );
        drop(exported);

        let active =
            crate::store::OmenchatStore::open_existing_for_maintenance(&config.database_path)
                .expect("active current database");
        assert!(active
            .room_by_name("exported-cli")
            .expect("active marker")
            .is_some());
        drop(active);
        std::fs::remove_dir_all(root).expect("remove isolated CLI export home");
    }

    #[test]
    fn cli_room_mutations_use_the_initialized_administrative_database_path() {
        let root = std::env::temp_dir().join(format!(
            "omenchatd-cli-admin-db-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        let options = ServerOptions {
            home: Some(root.clone()),
            ..ServerOptions::default()
        };
        Omenchatd
            .run(CliCommand::RoomsAdd(
                options.clone(),
                RoomAddOptions {
                    name: "ops".into(),
                    topic: Some("Operations".into()),
                },
            ))
            .expect("add room through administrative database");
        let config = config::ServerConfig::for_root(root.clone());
        let room = config::list_rooms(&config)
            .expect("list rooms")
            .into_iter()
            .find(|(_, name, _)| name == "ops")
            .expect("ops room");
        Omenchatd
            .run(CliCommand::RoomsSetTopic(
                options.clone(),
                RoomTopicOptions {
                    room_id: room.0,
                    topic: Some("Incidents".into()),
                },
            ))
            .expect("update room through administrative database");
        assert_eq!(
            config::list_rooms(&config)
                .expect("updated rooms")
                .into_iter()
                .find(|(_, name, _)| name == "ops")
                .and_then(|(_, _, topic)| topic),
            Some("Incidents".into())
        );
        Omenchatd
            .run(CliCommand::RoomsSetPolicy(
                options.clone(),
                RoomPolicyOptions {
                    room_id: room.0,
                    announcement_only: true,
                },
            ))
            .expect("update room policy through administrative database");
        let database =
            crate::store::OmenchatStore::open_existing_for_maintenance(&config.database_path)
                .expect("open current policy database");
        let policy_room = database
            .room_by_id(room.0 as u32)
            .expect("policy room lookup")
            .expect("policy room");
        assert_eq!(
            policy_room.policy_bits,
            crate::protocol::ROOM_POLICY_ANNOUNCEMENT
        );
        assert_eq!(policy_room.room_revision, 2);
        drop(database);
        Omenchatd
            .run(CliCommand::RoomsArchive(
                options,
                RoomSelectOptions { room_id: room.0 },
            ))
            .expect("archive room through administrative database");
        assert!(!config::list_rooms(&config)
            .expect("rooms after archive")
            .iter()
            .any(|(_, name, _)| name == "ops"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn headless_user_role_maintenance_uses_the_selected_existing_database() {
        let root = std::env::temp_dir().join(format!(
            "omenchatd-cli-user-role-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        let config = config::ServerConfig::for_root(root.clone());
        config::init_files(&config).expect("initialize isolated home");
        let store =
            crate::store::OmenchatStore::open(&config.database_path).expect("initialize database");
        let user = store
            .ensure_user(&[9; 16], "Moderator fixture", None)
            .expect("seed user");
        drop(store);
        let options = ServerOptions {
            home: Some(root.clone()),
            ..ServerOptions::default()
        };

        Omenchatd
            .run(CliCommand::UsersListJson(options.clone()))
            .expect("headless user listing");
        Omenchatd
            .run(CliCommand::UsersSetRole(
                options,
                UserRoleOptions {
                    user_id: i64::from(user.user_id),
                    role: AdministrativeUserRole::Moderator,
                },
            ))
            .expect("headless role update");

        let store =
            crate::store::OmenchatStore::open_existing_for_maintenance(&config.database_path)
                .expect("inspect role update");
        assert_eq!(
            store
                .user_by_identity(&[9; 16])
                .expect("user lookup")
                .expect("updated user")
                .role_bits,
            AdministrativeUserRole::Moderator.bits()
        );
        drop(store);
        std::fs::remove_dir_all(root).expect("remove isolated user role home");
    }

    #[test]
    fn cli_parses_version_command() {
        assert_eq!(
            CliCommand::parse(["--version".to_string()]),
            CliCommand::Version
        );
        let features = compiled_feature_summary();
        assert!(features.contains("server-headless:"));
        assert!(features.contains("server-full:"));
        assert!(features.contains("live-reticulum:"));
        assert!(features.contains("tui:"));
        assert!(!compiled_feature_summary().contains("live-rns-net:"));
    }

    #[cfg(not(feature = "tui"))]
    #[test]
    fn headless_build_rejects_tui_without_touching_a_server_home() {
        let root =
            std::env::temp_dir().join(format!("omenchatd-headless-tui-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let error = Omenchatd
            .run(CliCommand::Tui(ServerOptions {
                home: Some(root.clone()),
                ..ServerOptions::default()
            }))
            .expect_err("headless TUI must fail");
        assert!(error.to_string().contains("headless build"));
        assert!(!root.exists());
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
    fn doctor_reports_upload_ledger_discrepancies_without_repairing() {
        let root = std::env::temp_dir().join(format!(
            "omenchatd-doctor-upload-ledger-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        let config = config::ServerConfig::for_root(root.clone());
        config::init_files(&config).expect("init");
        let identity = b"doctor-upload-user";
        let identity_dir = crate::upload::upload_identity_dir(&config, identity);
        std::fs::create_dir_all(&identity_dir).expect("identity dir");
        let missing = identity_dir.join("missing.bin");
        let orphan = identity_dir.join("orphan.bin");
        std::fs::write(&orphan, b"orphan").expect("orphan");
        let connection = rusqlite::Connection::open(&config.database_path).expect("database");
        connection
            .execute(
                "INSERT INTO users(user_id, rns_identity_hash, display_name, first_seen_at, last_seen_at)
                 VALUES (41, ?1, 'Doctor Upload', 0, 0)",
                [identity.as_slice()],
            )
            .expect("user");
        connection
            .execute(
                "INSERT INTO upload_files(resource_id, room_id, actor_user_id, filename, byte_len, path, created_at)
                 VALUES ('missing-resource', 1, 41, 'missing.bin', 7, ?1, 0)",
                [missing.display().to_string()],
            )
            .expect("missing row");
        drop(connection);

        let warning = render_doctor_report(&config);
        assert!(warning.contains("[WARN] upload ledger:"));
        assert!(warning.contains("missing=1 mismatched=0 orphan=1 unsafe=0"));
        assert!(orphan.exists());

        let outside = root.join("outside.bin");
        std::fs::write(&outside, b"outside").expect("outside");
        let connection = rusqlite::Connection::open(&config.database_path).expect("database");
        connection
            .execute(
                "INSERT INTO upload_files(resource_id, room_id, actor_user_id, filename, byte_len, path, created_at)
                 VALUES ('unsafe-resource', 1, 41, 'outside.bin', 7, ?1, 1)",
                [outside.display().to_string()],
            )
            .expect("unsafe row");
        drop(connection);

        let failure = render_doctor_report(&config);
        assert!(failure.contains("omenchatd doctor: fail"));
        assert!(failure.contains("[FAIL] upload ledger:"));
        assert!(failure.contains("missing=1 mismatched=0 orphan=1 unsafe=1"));
        assert!(orphan.exists());
        assert!(outside.exists());
        let connection = rusqlite::Connection::open(&config.database_path).expect("database");
        let rows: i64 = connection
            .query_row("SELECT COUNT(*) FROM upload_files", [], |row| row.get(0))
            .expect("upload row count");
        assert_eq!(rows, 2);

        drop(connection);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn explicit_upload_ledger_repair_preserves_all_files() {
        let root = std::env::temp_dir().join(format!(
            "omenchatd-upload-repair-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        let config = config::ServerConfig::for_root(root.clone());
        config::init_files(&config).expect("init");
        drop(
            crate::store::OmenchatStore::open(&config.database_path)
                .expect("initialize current schema"),
        );
        let identity = b"repair-upload-user";
        let identity_dir = crate::upload::upload_identity_dir(&config, identity);
        std::fs::create_dir_all(&identity_dir).expect("identity dir");
        let missing = identity_dir.join("missing.bin");
        let orphan = identity_dir.join("orphan.bin");
        let outside = root.join("outside.bin");
        std::fs::write(&orphan, b"orphan").expect("orphan");
        std::fs::write(&outside, b"outside").expect("outside");
        let connection = rusqlite::Connection::open(&config.database_path).expect("database");
        connection
            .execute(
                "INSERT INTO users(user_id, rns_identity_hash, display_name, first_seen_at, last_seen_at)
                 VALUES (42, ?1, 'Repair Upload', 0, 0)",
                [identity.as_slice()],
            )
            .expect("user");
        for (resource_id, path) in [
            ("missing-resource", &missing),
            ("unsafe-resource", &outside),
        ] {
            connection
                .execute(
                    "INSERT INTO upload_files(resource_id, room_id, actor_user_id, filename, byte_len, path, created_at)
                     VALUES (?1, 1, 42, 'file.bin', 7, ?2, 0)",
                    (resource_id, path.display().to_string()),
                )
                .expect("upload row");
        }
        drop(connection);

        let report = repair_upload_ledger(&config).expect("explicit repair");
        assert!(report.contains("removed_missing=1"));
        assert!(report.contains("removed_unsafe=1"));
        assert!(report.contains("preserved_orphans=1"));
        assert!(orphan.exists());
        assert!(outside.exists());
        let connection = rusqlite::Connection::open(&config.database_path).expect("database");
        let rows: i64 = connection
            .query_row("SELECT COUNT(*) FROM upload_files", [], |row| row.get(0))
            .expect("row count");
        assert_eq!(rows, 0);
        drop(connection);

        let repeated = repair_upload_ledger(&config).expect("idempotent repair");
        assert!(repeated.contains("removed_missing=0"));
        assert!(repeated.contains("removed_unsafe=0"));
        assert!(repeated.contains("preserved_orphans=1"));
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

    #[test]
    fn safe_passphrase_file_is_resolved_for_tcp_client_command() {
        let path =
            std::env::temp_dir().join(format!("omenchatd-passphrase-{}", std::process::id()));
        std::fs::write(&path, b"server-secret\n").expect("passphrase file");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
                .expect("permissions");
        }

        let command = CliCommand::parse([
            "interfaces".into(),
            "tcp-client".into(),
            "gateway.example:42420".into(),
            "--passphrase-file".into(),
            path.display().to_string(),
        ]);
        let CliCommand::InterfacesTcpClient(_, tcp) = command else {
            panic!("expected TCP client command");
        };
        assert_eq!(tcp.passphrase.as_deref(), Some("server-secret"));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn passphrase_sources_are_exclusive_and_invalid_input_blocks_execution() {
        let command = CliCommand::parse([
            "run".into(),
            "--passphrase".into(),
            "one".into(),
            "--passphrase-stdin".into(),
        ]);
        assert!(matches!(command, CliCommand::Invalid(_)));
        assert!(Omenchatd.run(command).is_err());
        assert!(read_passphrase_from_reader(std::io::Cursor::new(vec![b'x'; 4097])).is_err());
        assert!(read_passphrase_from_reader(std::io::Cursor::new(b"\n")).is_err());
    }

    #[test]
    fn tcp_override_debug_redacts_passphrase() {
        let value = TcpClientOverride {
            target_host: "gateway.example".into(),
            target_port: 42420,
            network_name: Some("private".into()),
            passphrase: Some("server-debug-secret".into()),
        };
        let debug = format!("{value:?}");
        assert!(!debug.contains("server-debug-secret"));
        assert!(debug.contains("<redacted>"));
    }
}
