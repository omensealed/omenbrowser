use anyhow::Context;
#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
use std::collections::{BTreeMap, VecDeque};
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use omenbrowser_rs::app::{App, SmokePathWarmup};
use omenbrowser_rs::browser::BrowserAddress;
#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
use omenbrowser_rs::chat::rns::ChatLinkTransport;
use omenbrowser_rs::cli_network::TcpClientOverride;
use omenbrowser_rs::cli_overrides::SmokeOverrides;
use omenbrowser_rs::cli_redaction::{
    redact_bundle_log_message, redacted_argv, redacted_override_snapshot, redacted_path_hint,
};
use omenbrowser_rs::cli_report_logs::{
    redacted_recent_persisted_logs, REPORT_LOG_DIRECTORY_ENTRY_LIMIT, REPORT_LOG_ENTRY_LIMIT,
    REPORT_LOG_FILE_BYTES, REPORT_LOG_FILE_LIMIT, REPORT_LOG_TOTAL_BYTES,
};
use omenbrowser_rs::cli_values::{parse_lxmf_delivery_mode, parse_runtime_backend};
use omenbrowser_rs::config::{AppConfig, AppPaths};
#[cfg(feature = "desktop-ui")]
use omenbrowser_rs::desktop;
use omenbrowser_rs::interfaces::ReticulumInterfaceProfile;
#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
use omenbrowser_rs::runtime::{CancellationToken, RuntimeBusEvent};
use omenbrowser_rs::storage::settings::RuntimeBackendSetting;
#[cfg(feature = "tui")]
use omenbrowser_rs::ui;

fn main() -> anyhow::Result<()> {
    let runtime = omenbrowser_rs::runtime::bootstrap::build_app_runtime()
        .context("failed to start OMENbrowser async runtime")?;
    runtime.block_on(async_main())
}

async fn async_main() -> anyhow::Result<()> {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "omenbrowser_rs=info,reticulum_rs_transport=warn".into()),
        )
        .with_writer(std::io::stderr)
        .try_init();

    let cli = CliCommand::parse(std::env::args().skip(1))?;
    if matches!(cli, CliCommand::Help) {
        print_help();
        return Ok(());
    }
    if matches!(cli, CliCommand::Version) {
        print_version();
        return Ok(());
    }

    match cli {
        #[cfg(feature = "desktop-ui")]
        CliCommand::Desktop { app_root } => {
            let config = load_config_for_smoke(app_root)
                .context("failed to load application configuration")?;
            let app = App::new(config);
            tokio::task::block_in_place(move || desktop::run(app)).context("desktop UI failed")
        }
        #[cfg(not(feature = "desktop-ui"))]
        CliCommand::Desktop { .. } => {
            anyhow::bail!("desktop UI unavailable: build with feature desktop-ui")
        }
        #[cfg(feature = "tui")]
        CliCommand::Tui { app_root } => {
            let config = load_config_for_smoke(app_root)
                .context("failed to load application configuration")?;
            let app = App::new(config);
            ui::run(app).await.context("terminal UI failed")
        }
        #[cfg(not(feature = "tui"))]
        CliCommand::Tui { .. } => anyhow::bail!("terminal UI unavailable: build with feature tui"),
        CliCommand::NativeSmoke {
            destination,
            live,
            fetch,
            lxmf_smoke_peer,
            lxmf_smoke_delivery_mode,
            lxmf_smoke_propagation_node,
            lxmf_include_ticket,
            lxmf_interop_wait_secs,
            warmup,
            output,
            stdout,
            suggest_shell,
            bundle_report,
            overrides,
        } => {
            run_native_smoke_command(NativeSmokeCommandInput {
                destination,
                live,
                fetch,
                lxmf_smoke_peer,
                lxmf_smoke_delivery_mode,
                lxmf_smoke_propagation_node,
                lxmf_include_ticket,
                lxmf_interop_wait_secs,
                warmup,
                output,
                stdout,
                suggest_shell,
                bundle_report,
                overrides: *overrides,
            })
            .await
        }
        CliCommand::NativePreflight {
            destination,
            lxmf_peer,
            preflight_wait_ms,
            output,
            stdout,
            suggest_shell,
            bundle_report,
            overrides,
        } => {
            run_native_preflight_command(NativePreflightCommandInput {
                destination,
                lxmf_peer,
                preflight_wait_ms,
                output,
                stdout,
                suggest_shell,
                bundle_report,
                overrides: *overrides,
            })
            .await
        }
        CliCommand::NativeStartup {
            output,
            stdout,
            suggest_shell,
            bundle_report,
            overrides,
        } => {
            run_native_startup_command(NativeStartupCommandInput {
                output,
                stdout,
                suggest_shell,
                bundle_report,
                overrides: *overrides,
            })
            .await
        }
        CliCommand::LxmfInterop {
            peer_hash,
            lxmf_smoke_delivery_mode,
            lxmf_smoke_propagation_node,
            lxmf_include_ticket,
            wait_secs,
            output,
            stdout,
            suggest_shell,
            bundle_report,
            overrides,
        } => {
            run_lxmf_interop_command(LxmfInteropCommandInput {
                peer_hash,
                lxmf_smoke_delivery_mode,
                lxmf_smoke_propagation_node,
                lxmf_include_ticket,
                wait_secs,
                output,
                stdout,
                suggest_shell,
                bundle_report,
                overrides: *overrides,
            })
            .await
        }
        CliCommand::LxmfPropagationSync {
            lxmf_smoke_propagation_node,
            sync_limit,
            output,
            stdout,
            suggest_shell,
            bundle_report,
            overrides,
        } => {
            run_lxmf_propagation_sync_command(LxmfPropagationSyncCommandInput {
                lxmf_smoke_propagation_node,
                sync_limit,
                output,
                stdout,
                suggest_shell,
                bundle_report,
                overrides: *overrides,
            })
            .await
        }
        CliCommand::OmenChatSmoke {
            destination,
            room,
            message,
            reaction_smoke,
            revision_smoke,
            upload_file,
            fetch_upload_filename,
            fetch_upload_bytes,
            reconnect_ready_file,
            reconnect_wait_secs,
            link_timeout_secs,
            response_wait_secs,
            warmup,
            output,
            stdout,
            overrides,
        } => {
            run_omenchat_smoke_command(OmenChatSmokeCommandInput {
                destination,
                room,
                message,
                reaction_smoke,
                revision_smoke,
                upload_file,
                fetch_upload_filename,
                fetch_upload_bytes,
                reconnect_ready_file,
                reconnect_wait_secs,
                link_timeout_secs,
                response_wait_secs,
                warmup,
                output,
                stdout,
                overrides: *overrides,
            })
            .await
        }
        CliCommand::NativeLiveSequence {
            destination,
            lxmf_smoke_peer,
            lxmf_smoke_delivery_mode,
            lxmf_smoke_propagation_node,
            lxmf_include_ticket,
            lxmf_interop_wait_secs,
            warmup,
            preflight_wait_ms,
            output,
            stdout,
            suggest_shell,
            bundle_report,
            overrides,
        } => {
            run_native_live_sequence_command(NativeLiveSequenceCommandInput {
                destination,
                lxmf_smoke_peer,
                lxmf_smoke_delivery_mode,
                lxmf_smoke_propagation_node,
                lxmf_include_ticket,
                lxmf_interop_wait_secs,
                warmup,
                preflight_wait_ms,
                output,
                stdout,
                suggest_shell,
                bundle_report,
                overrides: *overrides,
            })
            .await
        }
        CliCommand::GenerateNativeIdentity {
            label,
            output,
            stdout,
            overrides,
        } => run_generate_native_identity_command(GenerateNativeIdentityCommandInput {
            label,
            output,
            stdout,
            overrides: *overrides,
        }),
        CliCommand::Help | CliCommand::Version => unreachable!("handled before config load"),
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum CliCommand {
    Desktop {
        app_root: Option<PathBuf>,
    },
    Tui {
        app_root: Option<PathBuf>,
    },
    Help,
    Version,
    NativeSmoke {
        destination: String,
        live: bool,
        fetch: bool,
        lxmf_smoke_peer: Option<String>,
        lxmf_smoke_delivery_mode: omenbrowser_rs::messaging::DeliveryMode,
        lxmf_smoke_propagation_node: Option<String>,
        lxmf_include_ticket: bool,
        lxmf_interop_wait_secs: Option<u64>,
        warmup: Option<SmokePathWarmup>,
        output: Option<PathBuf>,
        stdout: bool,
        suggest_shell: bool,
        bundle_report: Option<PathBuf>,
        overrides: Box<SmokeOverrides>,
    },
    NativePreflight {
        destination: String,
        lxmf_peer: Option<String>,
        preflight_wait_ms: u64,
        output: Option<PathBuf>,
        stdout: bool,
        suggest_shell: bool,
        bundle_report: Option<PathBuf>,
        overrides: Box<SmokeOverrides>,
    },
    NativeStartup {
        output: Option<PathBuf>,
        stdout: bool,
        suggest_shell: bool,
        bundle_report: Option<PathBuf>,
        overrides: Box<SmokeOverrides>,
    },
    NativeLiveSequence {
        destination: String,
        lxmf_smoke_peer: Option<String>,
        lxmf_smoke_delivery_mode: omenbrowser_rs::messaging::DeliveryMode,
        lxmf_smoke_propagation_node: Option<String>,
        lxmf_include_ticket: bool,
        lxmf_interop_wait_secs: Option<u64>,
        warmup: SmokePathWarmup,
        preflight_wait_ms: u64,
        output: Option<PathBuf>,
        stdout: bool,
        suggest_shell: bool,
        bundle_report: Option<PathBuf>,
        overrides: Box<SmokeOverrides>,
    },
    LxmfInterop {
        peer_hash: Option<String>,
        lxmf_smoke_delivery_mode: omenbrowser_rs::messaging::DeliveryMode,
        lxmf_smoke_propagation_node: Option<String>,
        lxmf_include_ticket: bool,
        wait_secs: u64,
        output: Option<PathBuf>,
        stdout: bool,
        suggest_shell: bool,
        bundle_report: Option<PathBuf>,
        overrides: Box<SmokeOverrides>,
    },
    LxmfPropagationSync {
        lxmf_smoke_propagation_node: Option<String>,
        sync_limit: Option<u32>,
        output: Option<PathBuf>,
        stdout: bool,
        suggest_shell: bool,
        bundle_report: Option<PathBuf>,
        overrides: Box<SmokeOverrides>,
    },
    OmenChatSmoke {
        destination: String,
        room: String,
        message: String,
        reaction_smoke: bool,
        revision_smoke: bool,
        upload_file: Option<PathBuf>,
        fetch_upload_filename: Option<String>,
        fetch_upload_bytes: Option<u64>,
        reconnect_ready_file: Option<PathBuf>,
        reconnect_wait_secs: u64,
        link_timeout_secs: u64,
        response_wait_secs: u64,
        warmup: Option<SmokePathWarmup>,
        output: Option<PathBuf>,
        stdout: bool,
        overrides: Box<SmokeOverrides>,
    },
    GenerateNativeIdentity {
        label: String,
        output: Option<PathBuf>,
        stdout: bool,
        overrides: Box<SmokeOverrides>,
    },
}

struct NativeSmokeCommandInput {
    destination: String,
    live: bool,
    fetch: bool,
    lxmf_smoke_peer: Option<String>,
    lxmf_smoke_delivery_mode: omenbrowser_rs::messaging::DeliveryMode,
    lxmf_smoke_propagation_node: Option<String>,
    lxmf_include_ticket: bool,
    lxmf_interop_wait_secs: Option<u64>,
    warmup: Option<SmokePathWarmup>,
    output: Option<PathBuf>,
    stdout: bool,
    suggest_shell: bool,
    bundle_report: Option<PathBuf>,
    overrides: SmokeOverrides,
}

struct LxmfInteropCommandInput {
    peer_hash: Option<String>,
    lxmf_smoke_delivery_mode: omenbrowser_rs::messaging::DeliveryMode,
    lxmf_smoke_propagation_node: Option<String>,
    lxmf_include_ticket: bool,
    wait_secs: u64,
    output: Option<PathBuf>,
    stdout: bool,
    suggest_shell: bool,
    bundle_report: Option<PathBuf>,
    overrides: SmokeOverrides,
}

struct LxmfPropagationSyncCommandInput {
    lxmf_smoke_propagation_node: Option<String>,
    sync_limit: Option<u32>,
    output: Option<PathBuf>,
    stdout: bool,
    suggest_shell: bool,
    bundle_report: Option<PathBuf>,
    overrides: SmokeOverrides,
}

#[allow(dead_code)]
struct OmenChatSmokeCommandInput {
    destination: String,
    room: String,
    message: String,
    reaction_smoke: bool,
    revision_smoke: bool,
    upload_file: Option<PathBuf>,
    fetch_upload_filename: Option<String>,
    fetch_upload_bytes: Option<u64>,
    reconnect_ready_file: Option<PathBuf>,
    reconnect_wait_secs: u64,
    link_timeout_secs: u64,
    response_wait_secs: u64,
    warmup: Option<SmokePathWarmup>,
    output: Option<PathBuf>,
    stdout: bool,
    overrides: SmokeOverrides,
}

struct NativePreflightCommandInput {
    destination: String,
    lxmf_peer: Option<String>,
    preflight_wait_ms: u64,
    output: Option<PathBuf>,
    stdout: bool,
    suggest_shell: bool,
    bundle_report: Option<PathBuf>,
    overrides: SmokeOverrides,
}

struct NativeStartupCommandInput {
    output: Option<PathBuf>,
    stdout: bool,
    suggest_shell: bool,
    bundle_report: Option<PathBuf>,
    overrides: SmokeOverrides,
}

struct NativeLiveSequenceCommandInput {
    destination: String,
    lxmf_smoke_peer: Option<String>,
    lxmf_smoke_delivery_mode: omenbrowser_rs::messaging::DeliveryMode,
    lxmf_smoke_propagation_node: Option<String>,
    lxmf_include_ticket: bool,
    lxmf_interop_wait_secs: Option<u64>,
    warmup: SmokePathWarmup,
    preflight_wait_ms: u64,
    output: Option<PathBuf>,
    stdout: bool,
    suggest_shell: bool,
    bundle_report: Option<PathBuf>,
    overrides: SmokeOverrides,
}

struct GenerateNativeIdentityCommandInput {
    label: String,
    output: Option<PathBuf>,
    stdout: bool,
    overrides: SmokeOverrides,
}

fn default_frontend_command() -> CliCommand {
    frontend_command(omenbrowser_rs::cli_frontend::default_frontend(), None)
}

fn frontend_command(
    frontend: omenbrowser_rs::cli_frontend::Frontend,
    app_root: Option<PathBuf>,
) -> CliCommand {
    match frontend {
        omenbrowser_rs::cli_frontend::Frontend::Desktop => CliCommand::Desktop { app_root },
        omenbrowser_rs::cli_frontend::Frontend::Tui => CliCommand::Tui { app_root },
    }
}

impl CliCommand {
    fn parse(args: impl IntoIterator<Item = String>) -> anyhow::Result<Self> {
        let args = omenbrowser_rs::cli_secret::resolve_passphrase_args(args.into_iter().collect())?;
        let mut args = args.into_iter().peekable();
        if args.peek().is_none() {
            return Ok(default_frontend_command());
        }

        let mut command = None;
        let mut frontend = None;
        let mut native_validate_destination = None;
        let mut native_live_sequence_destination = None;
        let mut preflight_destination = None;
        let mut native_startup = false;
        let mut omenchat_smoke_destination = None;
        let mut omenchat_room = "lobby".to_string();
        let mut omenchat_message = "OMENchat smoke test from OMENbrowser_rs".to_string();
        let mut omenchat_reaction_smoke = false;
        let mut omenchat_revision_smoke = false;
        let mut omenchat_upload_file = None;
        let mut omenchat_fetch_upload_filename = None;
        let mut omenchat_fetch_upload_bytes = None;
        let mut omenchat_reconnect_ready_file = None;
        let mut omenchat_reconnect_wait_secs = 60;
        let mut omenchat_link_timeout_secs = 15;
        let mut omenchat_response_wait_secs = 10;
        let mut generate_native_identity_label = None;
        let mut live = false;
        let mut fetch = false;
        let mut lxmf_smoke_peer = None;
        let mut lxmf_smoke_delivery_mode = omenbrowser_rs::messaging::DeliveryMode::Direct;
        let mut lxmf_smoke_propagation_node = std::env::var("TEST_PROPAGATION")
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty() && value != "PROPAGATION_NODE_ADDRESS");
        let mut lxmf_include_ticket = false;
        let mut lxmf_interop_wait_secs = None;
        let mut lxmf_sync_propagation = false;
        let mut lxmf_sync_limit = None;
        let mut warmup = None;
        let mut preflight_wait_ms = 250;
        let mut output = None;
        let mut stdout = false;
        let mut suggest_shell = false;
        let mut bundle_report = None;
        let mut overrides = SmokeOverrides::default();

        while let Some(arg) = args.next() {
            if let Some(simple) = omenbrowser_rs::cli_frontend::classify_argument(&arg) {
                match simple {
                    omenbrowser_rs::cli_frontend::SimpleCommand::Help => return Ok(Self::Help),
                    omenbrowser_rs::cli_frontend::SimpleCommand::Version => {
                        return Ok(Self::Version);
                    }
                    omenbrowser_rs::cli_frontend::SimpleCommand::Frontend(selected) => {
                        frontend = Some(selected);
                    }
                }
                continue;
            }
            match arg.as_str() {
                "--native-smoke" | "--smoke-test" => {
                    let destination = args.next().ok_or_else(|| {
                        anyhow::anyhow!("{arg} requires a destination:path value")
                    })?;
                    command = Some(destination);
                }
                "--native-validate" | "--native-live-validate" => {
                    let destination = args.next().ok_or_else(|| {
                        anyhow::anyhow!("{arg} requires a destination:path value")
                    })?;
                    native_validate_destination = Some(destination);
                }
                "--native-live-sequence" | "--native-sequence" => {
                    let destination = args.next().ok_or_else(|| {
                        anyhow::anyhow!("{arg} requires a destination:path value")
                    })?;
                    native_live_sequence_destination = Some(destination);
                }
                "--native-preflight" | "--preflight" => {
                    let destination = args.next().ok_or_else(|| {
                        anyhow::anyhow!("{arg} requires a destination:path value")
                    })?;
                    preflight_destination = Some(destination);
                }
                "--native-startup" | "--native-runtime-startup" => {
                    native_startup = true;
                }
                "--omenchat-smoke" | "--omenchat-live-smoke" => {
                    let destination = args.next().ok_or_else(|| {
                        anyhow::anyhow!("{arg} requires an OMENchat destination hash")
                    })?;
                    omenchat_smoke_destination = Some(destination);
                }
                "--omenchat-room" | "--room" => {
                    omenchat_room = args
                        .next()
                        .ok_or_else(|| anyhow::anyhow!("{arg} requires a room name"))?;
                }
                "--omenchat-message" | "--message" => {
                    omenchat_message = args
                        .next()
                        .ok_or_else(|| anyhow::anyhow!("{arg} requires a message body"))?;
                }
                "--omenchat-reaction-smoke" => {
                    omenchat_reaction_smoke = true;
                }
                "--omenchat-revision-smoke" => {
                    omenchat_revision_smoke = true;
                }
                "--omenchat-upload-file" | "--upload-file" => {
                    let value = args
                        .next()
                        .ok_or_else(|| anyhow::anyhow!("{arg} requires a file path"))?;
                    omenchat_upload_file = Some(PathBuf::from(value));
                }
                "--omenchat-fetch-upload" | "--fetch-upload" => {
                    omenchat_fetch_upload_filename = Some(
                        args.next()
                            .ok_or_else(|| anyhow::anyhow!("{arg} requires a filename"))?,
                    );
                }
                "--omenchat-fetch-upload-bytes" | "--fetch-upload-bytes" => {
                    let value = args
                        .next()
                        .ok_or_else(|| anyhow::anyhow!("{arg} requires a byte count"))?;
                    omenchat_fetch_upload_bytes =
                        Some(value.parse::<u64>().with_context(|| {
                            format!("invalid OMENchat fetch upload byte count in {value}")
                        })?);
                }
                "--omenchat-reconnect-ready-file" => {
                    let value = args
                        .next()
                        .ok_or_else(|| anyhow::anyhow!("{arg} requires a file path"))?;
                    omenchat_reconnect_ready_file = Some(PathBuf::from(value));
                }
                "--omenchat-reconnect-wait" => {
                    let value = args
                        .next()
                        .ok_or_else(|| anyhow::anyhow!("{arg} requires a seconds value"))?;
                    omenchat_reconnect_wait_secs = value.parse::<u64>().with_context(|| {
                        format!("invalid OMENchat reconnect wait seconds in {value}")
                    })?;
                    if omenchat_reconnect_wait_secs == 0 {
                        anyhow::bail!("OMENchat reconnect wait seconds must be positive");
                    }
                }
                "--omenchat-link-timeout" | "--link-timeout" => {
                    let value = args
                        .next()
                        .ok_or_else(|| anyhow::anyhow!("{arg} requires a seconds value"))?;
                    omenchat_link_timeout_secs = value.parse::<u64>().with_context(|| {
                        format!("invalid OMENchat link timeout seconds in {value}")
                    })?;
                }
                "--omenchat-response-wait" | "--response-wait" => {
                    let value = args
                        .next()
                        .ok_or_else(|| anyhow::anyhow!("{arg} requires a seconds value"))?;
                    omenchat_response_wait_secs = value.parse::<u64>().with_context(|| {
                        format!("invalid OMENchat response wait seconds in {value}")
                    })?;
                }
                "--generate-native-identity" | "--create-native-identity" => {
                    let label = args
                        .next()
                        .ok_or_else(|| anyhow::anyhow!("{arg} requires an identity label"))?;
                    generate_native_identity_label = Some(label);
                }
                "--live" => live = true,
                "--fetch-page" | "--live-fetch" => {
                    fetch = true;
                    live = true;
                }
                "--send-lxmf-smoke" | "--lxmf-smoke-send" => {
                    let peer_hash = args.next().ok_or_else(|| {
                        anyhow::anyhow!("{arg} requires an LXMF peer destination hash")
                    })?;
                    lxmf_smoke_peer = Some(peer_hash);
                }
                "--lxmf-smoke-method" | "--lxmf-delivery" => {
                    let value = args
                        .next()
                        .ok_or_else(|| anyhow::anyhow!("{arg} requires direct or propagated"))?;
                    lxmf_smoke_delivery_mode = parse_lxmf_delivery_mode(&value)?;
                }
                "--propagation-node" | "--lxmf-propagation-node" => {
                    let hash = args
                        .next()
                        .ok_or_else(|| anyhow::anyhow!("{arg} requires a propagation node hash"))?;
                    lxmf_smoke_propagation_node = Some(hash);
                }
                "--lxmf-include-ticket" | "--include-ticket" => {
                    lxmf_include_ticket = true;
                }
                "--lxmf-interop" | "--lxmf-live-interop" => {
                    lxmf_interop_wait_secs = Some(10);
                }
                "--lxmf-sync-propagation" | "--sync-lxmf-propagation" => {
                    lxmf_sync_propagation = true;
                }
                "--lxmf-sync-limit" | "--sync-limit" => {
                    let value = args
                        .next()
                        .ok_or_else(|| anyhow::anyhow!("{arg} requires a message limit"))?;
                    lxmf_sync_limit = Some(
                        value
                            .parse::<u32>()
                            .with_context(|| format!("invalid LXMF sync limit in {value}"))?,
                    );
                    lxmf_sync_propagation = true;
                }
                "--lxmf-wait" | "--lxmf-wait-secs" => {
                    let value = args
                        .next()
                        .ok_or_else(|| anyhow::anyhow!("{arg} requires a seconds value"))?;
                    lxmf_interop_wait_secs = Some(
                        value
                            .parse::<u64>()
                            .with_context(|| format!("invalid LXMF wait seconds in {value}"))?,
                    );
                }
                "--warm-path" | "--request-path" => {
                    warmup = Some(SmokePathWarmup { wait_secs: 5 });
                }
                "--path-wait" | "--warm-path-wait" => {
                    let value = args
                        .next()
                        .ok_or_else(|| anyhow::anyhow!("{arg} requires a seconds value"))?;
                    let wait_secs = value
                        .parse::<u64>()
                        .with_context(|| format!("invalid path wait seconds in {value}"))?;
                    warmup = Some(SmokePathWarmup { wait_secs });
                }
                "--preflight-wait" | "--preflight-wait-ms" => {
                    let value = args
                        .next()
                        .ok_or_else(|| anyhow::anyhow!("{arg} requires a milliseconds value"))?;
                    preflight_wait_ms = value.parse::<u64>().with_context(|| {
                        format!("invalid preflight wait milliseconds in {value}")
                    })?;
                }
                "--stdout" => stdout = true,
                "--suggest-shell" => suggest_shell = true,
                "--backend" => {
                    let backend = args.next().ok_or_else(|| {
                        anyhow::anyhow!("{arg} requires auto, mock, or reticulum")
                    })?;
                    overrides.set_runtime_backend(parse_runtime_backend(&backend)?);
                }
                "--identity" | "--identity-path" => {
                    let path = args
                        .next()
                        .ok_or_else(|| anyhow::anyhow!("{arg} requires a file path"))?;
                    overrides.set_identity_path(PathBuf::from(path));
                }
                "--reticulum-config" | "--reticulum-config-path" => {
                    let path = args
                        .next()
                        .ok_or_else(|| anyhow::anyhow!("{arg} requires a directory path"))?;
                    overrides.set_reticulum_config_path(PathBuf::from(path));
                }
                "--known-destinations" | "--known-destinations-path" => {
                    let path = args
                        .next()
                        .ok_or_else(|| anyhow::anyhow!("{arg} requires a file path"))?;
                    overrides.set_known_destinations_path(PathBuf::from(path));
                }
                "--generate-known-destinations-fixture" | "--write-known-destinations-fixture" => {
                    let path = args
                        .next()
                        .ok_or_else(|| anyhow::anyhow!("{arg} requires a file path"))?;
                    overrides.set_known_destinations_fixture_path(PathBuf::from(path));
                }
                "--tcp-client" => {
                    let endpoint = args
                        .next()
                        .ok_or_else(|| anyhow::anyhow!("{arg} requires host:port"))?;
                    let mut parsed_tcp = TcpClientOverride::parse_endpoint(&endpoint)?;
                    if let Some(existing) = overrides.take_tcp_client() {
                        parsed_tcp.inherit_credentials(existing);
                    }
                    overrides.set_tcp_client(parsed_tcp);
                }
                "--network-name" => {
                    let name = args
                        .next()
                        .ok_or_else(|| anyhow::anyhow!("{arg} requires a value"))?;
                    let tcp = overrides.tcp_client_mut_or_insert_empty();
                    tcp.set_network_name(name);
                }
                "--passphrase" => {
                    let passphrase = args
                        .next()
                        .ok_or_else(|| anyhow::anyhow!("{arg} requires a value"))?;
                    let tcp = overrides.tcp_client_mut_or_insert_empty();
                    tcp.set_passphrase(passphrase);
                }
                "--app-root" => {
                    let path = args
                        .next()
                        .ok_or_else(|| anyhow::anyhow!("{arg} requires a directory path"))?;
                    overrides.set_app_root(PathBuf::from(path));
                }
                "--output" | "-o" => {
                    let path = args
                        .next()
                        .ok_or_else(|| anyhow::anyhow!("{arg} requires a file path"))?;
                    output = Some(PathBuf::from(path));
                }
                "--bundle-report" => {
                    let path = args
                        .next()
                        .ok_or_else(|| anyhow::anyhow!("{arg} requires a directory path"))?;
                    bundle_report = Some(PathBuf::from(path));
                }
                other => {
                    return Err(anyhow::anyhow!(
                        "unknown argument {other}; use --help for usage"
                    ));
                }
            }
        }

        let command_count = usize::from(command.is_some())
            + usize::from(native_validate_destination.is_some())
            + usize::from(native_live_sequence_destination.is_some())
            + usize::from(preflight_destination.is_some())
            + usize::from(native_startup)
            + usize::from(frontend.is_some())
            + usize::from(omenchat_smoke_destination.is_some())
            + usize::from(generate_native_identity_label.is_some())
            + usize::from(lxmf_sync_propagation)
            + usize::from(
                lxmf_interop_wait_secs.is_some()
                    && command.is_none()
                    && native_validate_destination.is_none()
                    && native_live_sequence_destination.is_none()
                    && omenchat_smoke_destination.is_none(),
            );
        if command_count > 1 {
            return Err(anyhow::anyhow!(
                "native CLI modes are separate commands; choose one command"
            ));
        }

        if let Some(frontend) = frontend {
            Ok(frontend_command(frontend, overrides.take_app_root()))
        } else if let Some(label) = generate_native_identity_label {
            Ok(Self::GenerateNativeIdentity {
                label,
                output,
                stdout,
                overrides: Box::new(overrides),
            })
        } else if let Some(destination) = omenchat_smoke_destination {
            overrides.ensure_runtime_backend(RuntimeBackendSetting::Reticulum);
            Ok(Self::OmenChatSmoke {
                destination,
                room: omenchat_room,
                message: omenchat_message,
                reaction_smoke: omenchat_reaction_smoke,
                revision_smoke: omenchat_revision_smoke,
                upload_file: omenchat_upload_file,
                fetch_upload_filename: omenchat_fetch_upload_filename,
                fetch_upload_bytes: omenchat_fetch_upload_bytes,
                reconnect_ready_file: omenchat_reconnect_ready_file,
                reconnect_wait_secs: omenchat_reconnect_wait_secs,
                link_timeout_secs: omenchat_link_timeout_secs,
                response_wait_secs: omenchat_response_wait_secs,
                warmup,
                output,
                stdout,
                overrides: Box::new(overrides),
            })
        } else if let Some(destination) = native_live_sequence_destination {
            overrides.ensure_runtime_backend(RuntimeBackendSetting::Reticulum);
            Ok(Self::NativeLiveSequence {
                destination,
                lxmf_smoke_peer,
                lxmf_smoke_delivery_mode,
                lxmf_smoke_propagation_node,
                lxmf_include_ticket,
                lxmf_interop_wait_secs,
                warmup: warmup.unwrap_or(SmokePathWarmup { wait_secs: 10 }),
                preflight_wait_ms,
                output,
                stdout,
                suggest_shell,
                bundle_report,
                overrides: Box::new(overrides),
            })
        } else if let Some(destination) = preflight_destination {
            Ok(Self::NativePreflight {
                destination,
                lxmf_peer: lxmf_smoke_peer,
                preflight_wait_ms,
                output,
                stdout,
                suggest_shell,
                bundle_report,
                overrides: Box::new(overrides),
            })
        } else if native_startup {
            Ok(Self::NativeStartup {
                output,
                stdout,
                suggest_shell,
                bundle_report,
                overrides: Box::new(overrides),
            })
        } else if lxmf_sync_propagation {
            overrides.ensure_runtime_backend(RuntimeBackendSetting::Reticulum);
            Ok(Self::LxmfPropagationSync {
                lxmf_smoke_propagation_node,
                sync_limit: lxmf_sync_limit,
                output,
                stdout,
                suggest_shell,
                bundle_report,
                overrides: Box::new(overrides),
            })
        } else if let Some(destination) = native_validate_destination {
            overrides.ensure_runtime_backend(RuntimeBackendSetting::Reticulum);
            Ok(Self::NativeSmoke {
                destination,
                live: true,
                fetch: true,
                lxmf_smoke_peer,
                lxmf_smoke_delivery_mode,
                lxmf_smoke_propagation_node,
                lxmf_include_ticket,
                lxmf_interop_wait_secs,
                warmup: warmup.or(Some(SmokePathWarmup { wait_secs: 10 })),
                output,
                stdout,
                suggest_shell,
                bundle_report,
                overrides: Box::new(overrides),
            })
        } else if let Some(destination) = command {
            Ok(Self::NativeSmoke {
                destination,
                live,
                fetch,
                lxmf_smoke_peer,
                lxmf_smoke_delivery_mode,
                lxmf_smoke_propagation_node,
                lxmf_include_ticket,
                lxmf_interop_wait_secs,
                warmup,
                output,
                stdout,
                suggest_shell,
                bundle_report,
                overrides: Box::new(overrides),
            })
        } else if let Some(wait_secs) = lxmf_interop_wait_secs {
            Ok(Self::LxmfInterop {
                peer_hash: lxmf_smoke_peer,
                lxmf_smoke_delivery_mode,
                lxmf_smoke_propagation_node,
                lxmf_include_ticket,
                wait_secs,
                output,
                stdout,
                suggest_shell,
                bundle_report,
                overrides: Box::new(overrides),
            })
        } else {
            Err(anyhow::anyhow!(
                "no command specified; use --help for usage"
            ))
        }
    }
}

async fn run_native_smoke_command(input: NativeSmokeCommandInput) -> anyhow::Result<()> {
    let NativeSmokeCommandInput {
        destination,
        live,
        fetch,
        lxmf_smoke_peer,
        lxmf_smoke_delivery_mode,
        lxmf_smoke_propagation_node,
        lxmf_include_ticket,
        lxmf_interop_wait_secs,
        warmup,
        output,
        stdout,
        suggest_shell,
        bundle_report,
        overrides,
    } = input;
    let mut config = load_config_for_smoke(overrides.app_root().cloned())
        .context("failed to load smoke command app configuration")?;
    let mut known_destinations_path = overrides.known_destinations_path().cloned();
    if let Some(path) = overrides.known_destinations_fixture_path().cloned() {
        generate_known_destinations_fixture_for_smoke(&path, &destination)?;
        if known_destinations_path.is_none() {
            known_destinations_path = Some(path);
        }
    }
    let interface_override = apply_smoke_overrides(&mut config, overrides.clone());
    let default_output = output.is_none() && !stdout && bundle_report.is_none();
    let diagnostics_dir = config.paths.diagnostics_dir.clone();
    let logs_dir = config.paths.logs_dir.clone();
    let identity_path = config.settings.identity_path.clone();
    let mut app = App::try_new(config).context("failed to initialize application services")?;
    app.start_runtime_for_smoke_test_with_interfaces(interface_override)
        .await
        .context("failed to start configured runtime for smoke report")?;
    let known_destinations_preload = if let Some(path) = known_destinations_path.clone() {
        Some(
            app.preload_known_destinations_for_smoke_test(&path)
                .await
                .with_context(|| {
                    format!(
                        "failed to preload known destinations from {}",
                        path.display()
                    )
                })?,
        )
    } else {
        None
    };
    let mut report = app
        .native_network_smoke_test_report_for_url_with_fetch_options(
            destination.clone(),
            live,
            fetch,
            warmup,
            known_destinations_preload,
        )
        .await
        .context("failed to collect native-network smoke report")?;
    if let Some(peer_hash) = lxmf_smoke_peer {
        let lxmf_report = app
            .native_lxmf_smoke_send_report_for_peer(
                peer_hash.clone(),
                lxmf_smoke_delivery_mode.clone(),
                lxmf_smoke_propagation_node.clone(),
                lxmf_include_ticket,
            )
            .await
            .context("failed to collect native LXMF smoke-send report")?;
        if let Some(object) = report.as_object_mut() {
            object.insert("explicit_lxmf_smoke_send".into(), lxmf_report);
        }
        if let Some(wait_secs) = lxmf_interop_wait_secs {
            let interop_report = app
                .native_lxmf_live_interop_report(
                    Some(peer_hash),
                    wait_secs,
                    lxmf_smoke_delivery_mode.clone(),
                    lxmf_smoke_propagation_node.clone(),
                    lxmf_include_ticket,
                )
                .await
                .context("failed to collect native LXMF live interop report")?;
            if let Some(object) = report.as_object_mut() {
                object.insert("lxmf_live_interop".into(), interop_report);
            }
        }
    } else if let Some(wait_secs) = lxmf_interop_wait_secs {
        let interop_report = app
            .native_lxmf_live_interop_report(
                None,
                wait_secs,
                lxmf_smoke_delivery_mode.clone(),
                lxmf_smoke_propagation_node.clone(),
                lxmf_include_ticket,
            )
            .await
            .context("failed to collect native LXMF live interop report")?;
        if let Some(object) = report.as_object_mut() {
            object.insert("lxmf_live_interop".into(), interop_report);
        }
    }
    add_native_smoke_suggested_commands(
        &mut report,
        &destination,
        known_destinations_path.as_ref(),
    );
    let content =
        serde_json::to_string_pretty(&report).context("failed to render smoke report JSON")?;

    let summary = render_report_summary_with_options(&report, suggest_shell);

    if stdout {
        eprintln!("{summary}");
        println!("{content}");
    } else if suggest_shell {
        eprintln!("{summary}");
    }

    if let Some(path) =
        output.or_else(|| default_output.then(|| default_smoke_report_path(&diagnostics_dir)))
    {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).with_context(|| {
                format!("failed to create output directory {}", parent.display())
            })?;
        }
        std::fs::write(&path, content.as_bytes())
            .with_context(|| format!("failed to write smoke report {}", path.display()))?;
        if stdout {
            eprintln!("{}", path.display());
        } else {
            println!("{}", path.display());
        }
    }

    if let Some(root) = bundle_report {
        let bundle_dir = write_report_bundle(ReportBundleInput {
            root: &root,
            prefix: "native-network-smoke",
            command_kind: "native_smoke",
            report: &report,
            summary: &summary,
            overrides: &overrides,
            logs_dir: &logs_dir,
            identity_path: identity_path.as_ref(),
        })
        .context("failed to write native smoke bundle report")?;
        if stdout {
            eprintln!("{}", bundle_dir.display());
        } else {
            println!("{}", bundle_dir.display());
        }
    }

    Ok(())
}

#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
#[derive(Clone, Debug, Default)]
struct OmenChatSmokeTransport {
    incoming_frames: VecDeque<Vec<u8>>,
    resources: BTreeMap<String, Vec<u8>>,
    pending_resource_offers: BTreeMap<String, VecDeque<Vec<u8>>>,
    outgoing_frames: Vec<Vec<u8>>,
    outgoing_resources: Vec<(String, Vec<u8>)>,
}

#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
impl OmenChatSmokeTransport {
    fn push_incoming_frame(&mut self, frame: Vec<u8>) {
        self.incoming_frames.push_back(frame);
    }

    fn push_resource(&mut self, metadata: Option<Vec<u8>>, data: Vec<u8>) {
        let Some(resource_id) =
            omenbrowser_rs::chat::rns::resource_id_from_metadata(metadata.as_deref())
        else {
            return;
        };
        self.resources.insert(resource_id.clone(), data);
        if let Some(mut offers) = self.pending_resource_offers.remove(&resource_id) {
            while let Some(frame) = offers.pop_back() {
                self.incoming_frames.push_front(frame);
            }
        }
    }

    fn take_outgoing_frames(&mut self) -> Vec<Vec<u8>> {
        std::mem::take(&mut self.outgoing_frames)
    }

    fn take_outgoing_resources(&mut self) -> Vec<(String, Vec<u8>)> {
        std::mem::take(&mut self.outgoing_resources)
    }
}

#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
impl ChatLinkTransport for OmenChatSmokeTransport {
    fn send_frame(&mut self, frame_bytes: Vec<u8>) -> anyhow::Result<()> {
        self.outgoing_frames.push(frame_bytes);
        Ok(())
    }

    fn send_resource(&mut self, resource_id: &str, payload: Vec<u8>) -> anyhow::Result<()> {
        self.outgoing_resources
            .push((resource_id.to_owned(), payload));
        Ok(())
    }

    fn recv_frame(&mut self) -> anyhow::Result<Option<Vec<u8>>> {
        Ok(self.incoming_frames.pop_front())
    }

    fn fetch_resource(&mut self, resource_id: &str) -> anyhow::Result<Option<Vec<u8>>> {
        Ok(self.resources.get(resource_id).cloned())
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

#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
async fn run_omenchat_smoke_command(input: OmenChatSmokeCommandInput) -> anyhow::Result<()> {
    use omenbrowser_rs::chat::{
        ChatClient, ChatClientEvent, ChatClientRequest, OmenChatDescriptor,
    };

    let OmenChatSmokeCommandInput {
        destination,
        room,
        message,
        reaction_smoke,
        revision_smoke,
        upload_file,
        fetch_upload_filename,
        fetch_upload_bytes,
        reconnect_ready_file,
        reconnect_wait_secs,
        link_timeout_secs,
        response_wait_secs,
        warmup,
        output,
        stdout,
        overrides,
    } = input;
    parse_16_byte_hex_hash(&destination)?;

    let mut config = load_config_for_smoke(overrides.app_root().cloned())
        .context("failed to load OMENchat smoke app configuration")?;
    let known_destinations_path = overrides.known_destinations_path().cloned();
    let interface_override = apply_smoke_overrides(&mut config, overrides.clone());
    let default_output = output.is_none() && !stdout;
    let diagnostics_dir = config.paths.diagnostics_dir.clone();
    let mut app = App::try_new(config).context("failed to initialize application services")?;
    let mut runtime_events = app
        .runtime
        .subscribe_events()
        .ok_or_else(|| anyhow::anyhow!("configured runtime does not expose runtime events"))?;

    let mut stages = Vec::new();
    app.start_runtime_for_smoke_test_with_interfaces(interface_override)
        .await
        .context("failed to start runtime for OMENchat smoke")?;
    stages.push(serde_json::json!({
        "stage": "runtime_start",
        "ok": true,
        "status": app.runtime_status,
    }));

    if let Some(path) = known_destinations_path.clone() {
        let loaded = app
            .preload_known_destinations_for_smoke_test(&path)
            .await
            .with_context(|| {
                format!(
                    "failed to preload known destinations from {}",
                    path.display()
                )
            })?;
        stages.push(serde_json::json!({
            "stage": "known_destinations_preload",
            "ok": true,
            "path": path,
            "loaded": loaded,
        }));
    }

    if let Some(warmup) = warmup {
        let requested = app
            .runtime
            .request_path(&destination, "omenchat_smoke", true)
            .await;
        stages.push(serde_json::json!({
            "stage": "path_request",
            "ok": requested.is_ok(),
            "queued": requested.as_ref().ok().copied(),
            "error": requested.err().map(|error| error.to_string()),
            "wait_secs": warmup.wait_secs,
        }));
        let warmup_events = collect_runtime_trace(
            &mut runtime_events,
            Duration::from_secs(warmup.wait_secs),
            Some(&destination),
        )
        .await;
        stages.push(serde_json::json!({
            "stage": "path_wait",
            "ok": true,
            "events": warmup_events,
        }));
    }

    let opened = match app
        .runtime
        .open_omenchat_link(
            &destination,
            Duration::from_secs(link_timeout_secs),
            CancellationToken::new(),
        )
        .await
    {
        Ok(opened) => {
            stages.push(serde_json::json!({
                "stage": "link_open",
                "ok": true,
                "link_id": hex_bytes(&opened.link_id),
                "rtt_millis": opened.rtt_millis,
            }));
            opened
        }
        Err(error) => {
            stages.push(serde_json::json!({
                "stage": "link_open",
                "ok": false,
                "error": error.to_string(),
            }));
            let report = omenchat_smoke_report(
                false,
                "link_open",
                &destination,
                &room,
                &message,
                stages,
                None,
            );
            write_omenchat_smoke_report(report, output, stdout, default_output, &diagnostics_dir)?;
            let _ = app.runtime.stop_runtime().await;
            return Ok(());
        }
    };

    let mut client = ChatClient::new();
    let mut live_state = omenbrowser_rs::chat::live::LiveChatClientState::default();
    let client_instance_store =
        omenbrowser_rs::chat::client_instance::ClientInstanceIdStore::for_identity_storage_root(
            app.paths.identity_storage_root(),
        );
    let client_instance_id = client_instance_store.load_or_create().with_context(|| {
        format!(
            "failed to initialize isolated OMENchat smoke client instance at {}",
            client_instance_store.path().display()
        )
    })?;
    live_state.set_client_instance_id(Some(client_instance_id));
    let mut transport = OmenChatSmokeTransport::default();
    let descriptor = OmenChatDescriptor {
        server_destination: destination.clone(),
        display_name: Some("OMENchat smoke".into()),
        local_display_name: Some("OMENbrowser_rs smoke".into()),
        rooms_hint: vec![room.clone()],
        ..OmenChatDescriptor::default()
    };
    let open_events = omenbrowser_rs::chat::live::handle_live_request(
        &mut client,
        &mut live_state,
        &mut transport,
        ChatClientRequest::OpenServer(descriptor.clone()),
    );
    let session_id = open_events.iter().find_map(|event| match event {
        ChatClientEvent::ServerOpened { session_id, .. } => Some(*session_id),
        _ => None,
    });
    stages.push(serde_json::json!({
        "stage": "session_open_frames",
        "ok": session_id.is_some(),
        "events": open_events.iter().map(format_chat_event).collect::<Vec<_>>(),
    }));
    send_omenchat_smoke_outgoing(&*app.runtime, opened.link_id, &mut transport).await?;

    let Some(session_id) = session_id else {
        let report = omenchat_smoke_report(
            false,
            "session_open",
            &destination,
            &room,
            &message,
            stages,
            None,
        );
        write_omenchat_smoke_report(report, output, stdout, default_output, &diagnostics_dir)?;
        let _ = app.runtime.close_omenchat_link(opened.link_id).await;
        let _ = app.runtime.stop_runtime().await;
        return Ok(());
    };

    let join_events = wait_for_omenchat_condition(
        &*app.runtime,
        &mut runtime_events,
        &mut client,
        &mut live_state,
        &mut transport,
        OmenChatWaitOptions {
            link_id: opened.link_id,
            session_id,
            wait: Duration::from_secs(response_wait_secs),
        },
        |client| {
            client
                .session(session_id)
                .is_some_and(|session| session.active_room.joined)
        },
    )
    .await;
    send_omenchat_smoke_outgoing(&*app.runtime, opened.link_id, &mut transport).await?;
    let joined = client
        .session(session_id)
        .is_some_and(|session| session.active_room.joined);
    stages.push(serde_json::json!({
        "stage": "join_wait",
        "ok": joined,
        "events": join_events,
    }));
    stages.push(serde_json::json!({
        "stage": "capability_observation",
        "ok": true,
        "durable_mutations_negotiated": live_state.durable_mutations_negotiated(session_id),
        "durable_notice_ack_negotiated": live_state
            .durable_notice_ack_negotiated(session_id),
        "reply_mentions_negotiated": live_state.reply_mentions_negotiated(session_id),
        "reactions_negotiated": live_state.reactions_negotiated(session_id),
        "message_revisions_negotiated": live_state.message_revisions_negotiated(session_id),
        "local_user_id_bound": live_state.local_user_id(session_id).is_some(),
    }));

    if joined {
        let send_events = omenbrowser_rs::chat::live::handle_live_request(
            &mut client,
            &mut live_state,
            &mut transport,
            ChatClientRequest::SendMessage {
                session_id,
                room: room.clone(),
                body: message.clone(),
            },
        );
        stages.push(serde_json::json!({
            "stage": "message_send_frame",
            "ok": !send_events.iter().any(|event| matches!(event, ChatClientEvent::Error { .. })),
            "events": send_events.iter().map(format_chat_event).collect::<Vec<_>>(),
        }));
        send_omenchat_smoke_outgoing(&*app.runtime, opened.link_id, &mut transport).await?;
    }

    let message_events = if joined {
        wait_for_omenchat_condition(
            &*app.runtime,
            &mut runtime_events,
            &mut client,
            &mut live_state,
            &mut transport,
            OmenChatWaitOptions {
                link_id: opened.link_id,
                session_id,
                wait: Duration::from_secs(response_wait_secs),
            },
            |client| omenchat_session_contains_message(client, session_id, &message),
        )
        .await
    } else {
        Vec::new()
    };
    let message_seen = omenchat_session_contains_message(&client, session_id, &message);
    stages.push(serde_json::json!({
        "stage": "message_echo_wait",
        "ok": message_seen,
        "events": message_events,
    }));

    let mut reaction_ok = true;
    let mutation_identity_storage_root = app.paths.identity_storage_root();
    let mutation_identity_hash = if reaction_smoke || revision_smoke {
        app.runtime_status
            .active_identity
            .as_ref()
            .map(|identity| parse_16_byte_hex_hash(&identity.hash_hex))
            .transpose()?
    } else {
        None
    };
    if joined && message_seen && reaction_smoke {
        let target_event_id = omenchat_session_message_event_id(&client, session_id, &message)
            .context("OMENchat reaction smoke target message was not retained")?;
        let authenticated_identity_hash =
            mutation_identity_hash.context("OMENchat reaction smoke has no active identity")?;
        let room_id = client
            .session(session_id)
            .map(|session| session.active_room.room_id)
            .unwrap_or(1);
        let (passed, reaction_stages) = run_omenchat_reaction_smoke(
            &*app.runtime,
            &mut runtime_events,
            &mut client,
            &mut live_state,
            &mut transport,
            OmenChatReactionSmokeOptions {
                link_id: opened.link_id,
                session_id,
                room_id,
                target_event_id,
                server_destination: &destination,
                identity_storage_root: &mutation_identity_storage_root,
                authenticated_identity_hash,
                wait: Duration::from_secs(response_wait_secs),
            },
        )
        .await?;
        reaction_ok = passed;
        stages.extend(reaction_stages);
    }

    let mut revision_ok = true;
    if joined && message_seen && revision_smoke {
        let target_event_id = omenchat_session_message_event_id(&client, session_id, &message)
            .context("OMENchat revision smoke target message was not retained")?;
        let authenticated_identity_hash =
            mutation_identity_hash.context("OMENchat revision smoke has no active identity")?;
        let room_id = client
            .session(session_id)
            .map(|session| session.active_room.room_id)
            .unwrap_or(1);
        let (passed, revision_stages) = run_omenchat_revision_smoke(
            &*app.runtime,
            &mut runtime_events,
            &mut client,
            &mut live_state,
            &mut transport,
            OmenChatRevisionSmokeOptions {
                link_id: opened.link_id,
                session_id,
                room_id,
                target_event_id,
                server_destination: &destination,
                identity_storage_root: &mutation_identity_storage_root,
                authenticated_identity_hash,
                wait: Duration::from_secs(response_wait_secs),
            },
        )
        .await?;
        revision_ok = passed;
        stages.extend(revision_stages);
    }

    let mut upload_ok = true;
    if joined && message_seen {
        if let Some(upload_file) = upload_file {
            let upload_bytes = std::fs::read(&upload_file).with_context(|| {
                format!(
                    "failed to read OMENchat smoke upload file {}",
                    upload_file.display()
                )
            })?;
            let upload_filename = upload_file
                .file_name()
                .and_then(|name| name.to_str())
                .filter(|name| !name.trim().is_empty())
                .unwrap_or("omenchat-smoke-upload.bin")
                .to_owned();
            let upload_len = upload_bytes.len() as u64;
            let send_upload_events = omenbrowser_rs::chat::live::handle_live_request(
                &mut client,
                &mut live_state,
                &mut transport,
                ChatClientRequest::SendUpload {
                    session_id,
                    room: room.clone(),
                    filename: upload_filename.clone(),
                    content_type: Some("application/octet-stream".into()),
                    bytes: upload_bytes.clone(),
                },
            );
            stages.push(serde_json::json!({
                "stage": "upload_offer_frame",
                "ok": !send_upload_events.iter().any(|event| matches!(event, ChatClientEvent::Error { .. })),
                "filename": upload_filename.clone(),
                "bytes": upload_len,
                "events": send_upload_events.iter().map(format_chat_event).collect::<Vec<_>>(),
            }));
            send_omenchat_smoke_outgoing(&*app.runtime, opened.link_id, &mut transport).await?;

            let upload_complete_events = wait_for_omenchat_condition(
                &*app.runtime,
                &mut runtime_events,
                &mut client,
                &mut live_state,
                &mut transport,
                OmenChatWaitOptions {
                    link_id: opened.link_id,
                    session_id,
                    wait: Duration::from_secs(response_wait_secs),
                },
                |client| {
                    omenchat_session_upload_resource_id(
                        client,
                        session_id,
                        &upload_filename,
                        Some(upload_len),
                    )
                    .is_some()
                },
            )
            .await;
            let upload_resource_id = omenchat_session_upload_resource_id(
                &client,
                session_id,
                &upload_filename,
                Some(upload_len),
            );
            let upload_completed = upload_resource_id.is_some()
                && omenchat_smoke_events_contain_decoded_event(
                    &upload_complete_events,
                    "upload_completed",
                );
            stages.push(serde_json::json!({
                "stage": "upload_complete_wait",
                "ok": upload_completed,
                "resource_id": upload_resource_id.clone(),
                "events": upload_complete_events,
            }));

            if let Some(resource_id) = upload_resource_id {
                let (upload_resource_available, fetch_stages) =
                    run_omenchat_smoke_upload_fetch(OmenchatSmokeUploadFetch {
                        runtime: &*app.runtime,
                        runtime_events: &mut runtime_events,
                        link_id: opened.link_id,
                        client: &mut client,
                        live_state: &mut live_state,
                        transport: &mut transport,
                        session_id,
                        room: &room,
                        resource_id,
                        filename: &upload_filename,
                        bytes: Some(upload_len),
                        wait: Duration::from_secs(response_wait_secs),
                    })
                    .await?;
                stages.extend(fetch_stages);
                upload_ok = upload_completed && upload_resource_available;
            } else {
                upload_ok = false;
            }
        }
    }

    if joined && message_seen {
        if let Some(fetch_filename) = fetch_upload_filename {
            let existing_resource_id = omenchat_session_upload_resource_id(
                &client,
                session_id,
                &fetch_filename,
                fetch_upload_bytes,
            );
            stages.push(serde_json::json!({
                "stage": "existing_upload_lookup",
                "ok": existing_resource_id.is_some(),
                "filename": fetch_filename.clone(),
                "bytes": fetch_upload_bytes,
                "resource_id": existing_resource_id.clone(),
            }));
            if let Some(resource_id) = existing_resource_id {
                let (fetched_existing_upload, fetch_stages) =
                    run_omenchat_smoke_upload_fetch(OmenchatSmokeUploadFetch {
                        runtime: &*app.runtime,
                        runtime_events: &mut runtime_events,
                        link_id: opened.link_id,
                        client: &mut client,
                        live_state: &mut live_state,
                        transport: &mut transport,
                        session_id,
                        room: &room,
                        resource_id,
                        filename: &fetch_filename,
                        bytes: fetch_upload_bytes,
                        wait: Duration::from_secs(response_wait_secs),
                    })
                    .await?;
                stages.extend(fetch_stages);
                upload_ok = upload_ok && fetched_existing_upload;
            } else {
                upload_ok = false;
            }
        }
    }

    let mut active_link_id = opened.link_id;
    let mut reconnect_ok = true;
    if joined && message_seen {
        if let Some(ready_file) = reconnect_ready_file.as_deref() {
            let (passed, reconnected_link, reconnect_stages) =
                run_omenchat_continuous_reconnect_smoke(OmenChatContinuousReconnectSmoke {
                    runtime: &*app.runtime,
                    runtime_events: &mut runtime_events,
                    client: &mut client,
                    live_state: &mut live_state,
                    transport: &mut transport,
                    descriptor,
                    old_link_id: opened.link_id,
                    session_id,
                    room: &room,
                    message: &message,
                    ready_file,
                    wait: Duration::from_secs(reconnect_wait_secs),
                    link_timeout: Duration::from_secs(link_timeout_secs),
                    response_wait: Duration::from_secs(response_wait_secs),
                    reaction_smoke,
                    revision_smoke,
                    server_destination: &destination,
                    identity_storage_root: &mutation_identity_storage_root,
                    authenticated_identity_hash: mutation_identity_hash,
                })
                .await?;
            reconnect_ok = passed;
            stages.extend(reconnect_stages);
            if let Some(link_id) = reconnected_link {
                active_link_id = link_id;
            }
        }
    }

    let session_summary = client.session(session_id).map(|session| {
        serde_json::json!({
            "session_id": session.session_id,
            "server": {
                "server_id": session.server.server_id.clone(),
                "destination": session.server.destination.clone(),
                "display_name": session.server.display_name.clone(),
            },
            "room": {
                "server_id": session.active_room.server_id.clone(),
                "room_id": session.active_room.room_id,
                "name": session.active_room.name.clone(),
                "unread": session.active_room.unread,
                "joined": session.active_room.joined,
            },
            "user_count": session.users.len(),
            "event_count": session.events.len(),
            "status": session.status.clone(),
        })
    });
    let outcome = joined && message_seen && reaction_ok && revision_ok && upload_ok && reconnect_ok;
    let failed_stage = if !joined {
        "join_wait"
    } else if !message_seen {
        "message_echo_wait"
    } else if !reaction_ok {
        "reaction_smoke"
    } else if !revision_ok {
        "revision_smoke"
    } else if !upload_ok {
        "upload_fetch_wait"
    } else if !reconnect_ok {
        "continuous_reconnect"
    } else {
        "complete"
    };
    let report = omenchat_smoke_report(
        outcome,
        failed_stage,
        &destination,
        &room,
        &message,
        stages,
        session_summary,
    );
    write_omenchat_smoke_report(report, output, stdout, default_output, &diagnostics_dir)?;
    let _ = app.runtime.close_omenchat_link(active_link_id).await;
    let _ = app.runtime.stop_runtime().await;
    Ok(())
}

#[cfg(not(any(feature = "chat-client-rns", feature = "chat-client-rns-clean")))]
async fn run_omenchat_smoke_command(_input: OmenChatSmokeCommandInput) -> anyhow::Result<()> {
    anyhow::bail!("OMENchat smoke requires --features chat-client-rns or chat-client-rns-clean")
}

#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
async fn send_omenchat_smoke_outgoing(
    runtime: &dyn omenbrowser_rs::runtime::NetworkRuntime,
    link_id: [u8; 16],
    transport: &mut OmenChatSmokeTransport,
) -> anyhow::Result<()> {
    for frame in transport.take_outgoing_frames() {
        runtime
            .send_omenchat_frame(link_id, frame)
            .await
            .context("failed to send OMENchat smoke frame")?;
    }
    for (resource_id, payload) in transport.take_outgoing_resources() {
        runtime
            .send_omenchat_resource(link_id, resource_id, payload)
            .await
            .context("failed to send OMENchat smoke resource")?;
    }
    Ok(())
}

#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
#[derive(Clone, Copy)]
struct OmenChatWaitOptions {
    link_id: [u8; 16],
    session_id: omenbrowser_rs::chat::ChatSessionId,
    wait: Duration,
}

#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
async fn wait_for_omenchat_condition(
    runtime: &dyn omenbrowser_rs::runtime::NetworkRuntime,
    runtime_events: &mut tokio::sync::broadcast::Receiver<RuntimeBusEvent>,
    client: &mut omenbrowser_rs::chat::ChatClient,
    live_state: &mut omenbrowser_rs::chat::live::LiveChatClientState,
    transport: &mut OmenChatSmokeTransport,
    options: OmenChatWaitOptions,
    condition: impl Fn(&omenbrowser_rs::chat::ChatClient) -> bool,
) -> Vec<serde_json::Value> {
    let OmenChatWaitOptions {
        link_id,
        session_id,
        wait,
    } = options;
    let deadline = tokio::time::Instant::now() + wait;
    let mut events = Vec::new();
    while tokio::time::Instant::now() < deadline && !condition(client) {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        let received = tokio::time::timeout(remaining, runtime_events.recv()).await;
        let event = match received {
            Ok(Ok(event)) => event,
            Ok(Err(tokio::sync::broadcast::error::RecvError::Lagged(count))) => {
                events.push(serde_json::json!({
                    "event": "lagged",
                    "count": count,
                }));
                continue;
            }
            Ok(Err(tokio::sync::broadcast::error::RecvError::Closed)) => {
                events.push(serde_json::json!({"event": "closed"}));
                break;
            }
            Err(_) => {
                events.push(serde_json::json!({"event": "timeout"}));
                break;
            }
        };
        match event {
            RuntimeBusEvent::OmenChatLinkData(data) if data.link_id == link_id => {
                let bytes = data.frame_bytes.len();
                transport.push_incoming_frame(data.frame_bytes);
                let decoded = omenbrowser_rs::chat::live::drain_live_events_with_state(
                    client,
                    live_state,
                    transport,
                    Some(session_id),
                );
                events.push(serde_json::json!({
                    "event": "link_data",
                    "bytes": bytes,
                    "decoded": decoded.iter().map(format_chat_event).collect::<Vec<_>>(),
                }));
                if let Err(error) = send_omenchat_smoke_outgoing(runtime, link_id, transport).await
                {
                    events.push(serde_json::json!({
                        "event": "flush_error",
                        "error": error.to_string(),
                    }));
                    break;
                }
            }
            RuntimeBusEvent::OmenChatResourceData(data) if data.link_id == link_id => {
                let bytes = data.data.len();
                let metadata_len = data.metadata.as_ref().map_or(0, Vec::len);
                transport.push_resource(data.metadata, data.data);
                let decoded = omenbrowser_rs::chat::live::drain_live_events_with_state(
                    client,
                    live_state,
                    transport,
                    Some(session_id),
                );
                events.push(serde_json::json!({
                    "event": "resource_data",
                    "bytes": bytes,
                    "metadata_len": metadata_len,
                    "decoded": decoded.iter().map(format_chat_event).collect::<Vec<_>>(),
                }));
                if let Err(error) = send_omenchat_smoke_outgoing(runtime, link_id, transport).await
                {
                    events.push(serde_json::json!({
                        "event": "flush_error",
                        "error": error.to_string(),
                    }));
                    break;
                }
            }
            RuntimeBusEvent::Debug(message) => {
                events.push(serde_json::json!({"event": "debug", "message": message}));
            }
            RuntimeBusEvent::Error(message) => {
                events.push(serde_json::json!({"event": "error", "message": message}));
            }
            RuntimeBusEvent::PathUpdated(path) => {
                events.push(serde_json::json!({"event": "path", "value": path}));
            }
            RuntimeBusEvent::Announce(announce) => {
                events.push(serde_json::json!({"event": "announce", "value": announce}));
            }
            _ => {}
        }
    }
    events
}

#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
struct OmenChatContinuousReconnectSmoke<'a> {
    runtime: &'a dyn omenbrowser_rs::runtime::NetworkRuntime,
    runtime_events: &'a mut tokio::sync::broadcast::Receiver<RuntimeBusEvent>,
    client: &'a mut omenbrowser_rs::chat::ChatClient,
    live_state: &'a mut omenbrowser_rs::chat::live::LiveChatClientState,
    transport: &'a mut OmenChatSmokeTransport,
    descriptor: omenbrowser_rs::chat::OmenChatDescriptor,
    old_link_id: [u8; 16],
    session_id: omenbrowser_rs::chat::ChatSessionId,
    room: &'a str,
    message: &'a str,
    ready_file: &'a std::path::Path,
    wait: Duration,
    link_timeout: Duration,
    response_wait: Duration,
    reaction_smoke: bool,
    revision_smoke: bool,
    server_destination: &'a str,
    identity_storage_root: &'a std::path::Path,
    authenticated_identity_hash: Option<[u8; 16]>,
}

#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
async fn run_omenchat_continuous_reconnect_smoke(
    input: OmenChatContinuousReconnectSmoke<'_>,
) -> anyhow::Result<(bool, Option<[u8; 16]>, Vec<serde_json::Value>)> {
    use omenbrowser_rs::chat::{ChatClientEvent, ChatClientRequest};

    create_omenchat_reconnect_ready_file(input.ready_file)?;
    let deadline = tokio::time::Instant::now() + input.wait;
    let mut stages = Vec::new();
    let mut close_seen = false;
    while tokio::time::Instant::now() < deadline && !close_seen {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        match tokio::time::timeout(remaining, input.runtime_events.recv()).await {
            Ok(Ok(RuntimeBusEvent::OmenChatLinkClosed(closed)))
                if closed.link_id == input.old_link_id =>
            {
                close_seen = true;
                stages.push(serde_json::json!({
                    "stage": "continuous_link_close_wait",
                    "ok": true,
                    "reason": closed.reason,
                }));
            }
            Ok(Ok(_)) | Ok(Err(tokio::sync::broadcast::error::RecvError::Lagged(_))) => {}
            Ok(Err(tokio::sync::broadcast::error::RecvError::Closed)) | Err(_) => break,
        }
    }
    if !close_seen {
        stages.push(serde_json::json!({
            "stage": "continuous_link_close_wait",
            "ok": false,
            "reason": "old link did not close before the bounded deadline",
        }));
        return Ok((false, None, stages));
    }

    let mut opened = None;
    let mut attempts = 0u8;
    while tokio::time::Instant::now() < deadline && attempts < 6 {
        attempts = attempts.saturating_add(1);
        let _ = input
            .runtime
            .request_path(
                &input.descriptor.server_destination,
                "omenchat_continuous_reconnect_smoke",
                true,
            )
            .await;
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        let attempt_timeout = input.link_timeout.min(remaining);
        if attempt_timeout.is_zero() {
            break;
        }
        match input
            .runtime
            .open_omenchat_link(
                &input.descriptor.server_destination,
                attempt_timeout,
                CancellationToken::new(),
            )
            .await
        {
            Ok(link) => {
                stages.push(serde_json::json!({
                    "stage": "continuous_link_reopen",
                    "ok": true,
                    "attempt": attempts,
                    "link_changed": link.link_id != input.old_link_id,
                }));
                opened = Some(link);
                break;
            }
            Err(error) => {
                stages.push(serde_json::json!({
                    "stage": "continuous_link_reopen_attempt",
                    "ok": false,
                    "attempt": attempts,
                    "error": error.to_string(),
                }));
                let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
                if remaining.is_zero() {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(500).min(remaining)).await;
            }
        }
    }
    let Some(opened) = opened else {
        stages.push(serde_json::json!({
            "stage": "continuous_link_reopen",
            "ok": false,
            "attempts": attempts,
        }));
        return Ok((false, None, stages));
    };

    *input.transport = OmenChatSmokeTransport::default();
    let reconnect_events = omenbrowser_rs::chat::live::reconnect_live_server(
        input.client,
        input.live_state,
        input.transport,
        input.session_id,
        input.descriptor,
    );
    let reconnect_started = !reconnect_events
        .iter()
        .any(|event| matches!(event, ChatClientEvent::Error { .. }));
    stages.push(serde_json::json!({
        "stage": "continuous_session_reconnect",
        "ok": reconnect_started,
        "events": reconnect_events.iter().map(format_chat_event).collect::<Vec<_>>(),
    }));
    send_omenchat_smoke_outgoing(input.runtime, opened.link_id, input.transport).await?;

    let reconnect_message = format!("{} (after continuous reconnect)", input.message);
    let send_events = omenbrowser_rs::chat::live::handle_live_request(
        input.client,
        input.live_state,
        input.transport,
        ChatClientRequest::SendMessage {
            session_id: input.session_id,
            room: input.room.to_owned(),
            body: reconnect_message.clone(),
        },
    );
    let send_ok = !send_events
        .iter()
        .any(|event| matches!(event, ChatClientEvent::Error { .. }));
    stages.push(serde_json::json!({
        "stage": "continuous_message_send",
        "ok": send_ok,
        "events": send_events.iter().map(format_chat_event).collect::<Vec<_>>(),
    }));
    send_omenchat_smoke_outgoing(input.runtime, opened.link_id, input.transport).await?;

    let message_events = wait_for_omenchat_condition(
        input.runtime,
        input.runtime_events,
        input.client,
        input.live_state,
        input.transport,
        OmenChatWaitOptions {
            link_id: opened.link_id,
            session_id: input.session_id,
            wait: input.response_wait,
        },
        |client| omenchat_session_contains_message(client, input.session_id, &reconnect_message),
    )
    .await;
    let message_seen =
        omenchat_session_contains_message(input.client, input.session_id, &reconnect_message);
    stages.push(serde_json::json!({
        "stage": "continuous_message_echo_wait",
        "ok": message_seen,
        "events": message_events,
    }));
    let mut reaction_ok = true;
    if input.reaction_smoke && message_seen {
        let target_event_id =
            omenchat_session_message_event_id(input.client, input.session_id, &reconnect_message)
                .context("continuous reconnect reaction target message was not retained")?;
        let room_id = input
            .client
            .session(input.session_id)
            .map(|session| session.active_room.room_id)
            .unwrap_or(1);
        let Some(authenticated_identity_hash) = input.authenticated_identity_hash else {
            stages.push(serde_json::json!({
                "stage": "continuous_reaction_identity",
                "ok": false,
                "reason": "active identity was unavailable",
            }));
            return Ok((false, Some(opened.link_id), stages));
        };
        let (passed, reaction_stages) = run_omenchat_reaction_smoke(
            input.runtime,
            input.runtime_events,
            input.client,
            input.live_state,
            input.transport,
            OmenChatReactionSmokeOptions {
                link_id: opened.link_id,
                session_id: input.session_id,
                room_id,
                target_event_id,
                server_destination: input.server_destination,
                identity_storage_root: input.identity_storage_root,
                authenticated_identity_hash,
                wait: input.response_wait,
            },
        )
        .await?;
        reaction_ok = passed;
        stages.extend(reaction_stages.into_iter().map(|mut stage| {
            if let Some(name) = stage
                .get("stage")
                .and_then(serde_json::Value::as_str)
                .map(ToOwned::to_owned)
            {
                stage["stage"] = serde_json::Value::String(format!("continuous_{name}"));
            }
            stage
        }));
    }
    let mut revision_ok = true;
    if input.revision_smoke && message_seen {
        let target_event_id =
            omenchat_session_message_event_id(input.client, input.session_id, &reconnect_message)
                .context("continuous reconnect revision target message was not retained")?;
        let room_id = input
            .client
            .session(input.session_id)
            .map(|session| session.active_room.room_id)
            .unwrap_or(1);
        let Some(authenticated_identity_hash) = input.authenticated_identity_hash else {
            stages.push(serde_json::json!({
                "stage": "continuous_revision_identity",
                "ok": false,
                "reason": "active identity was unavailable",
            }));
            return Ok((false, Some(opened.link_id), stages));
        };
        let (passed, revision_stages) = run_omenchat_revision_smoke(
            input.runtime,
            input.runtime_events,
            input.client,
            input.live_state,
            input.transport,
            OmenChatRevisionSmokeOptions {
                link_id: opened.link_id,
                session_id: input.session_id,
                room_id,
                target_event_id,
                server_destination: input.server_destination,
                identity_storage_root: input.identity_storage_root,
                authenticated_identity_hash,
                wait: input.response_wait,
            },
        )
        .await?;
        revision_ok = passed;
        stages.extend(revision_stages.into_iter().map(|mut stage| {
            if let Some(name) = stage
                .get("stage")
                .and_then(serde_json::Value::as_str)
                .map(ToOwned::to_owned)
            {
                stage["stage"] = serde_json::Value::String(format!("continuous_{name}"));
            }
            stage
        }));
    }
    Ok((
        reconnect_started
            && send_ok
            && message_seen
            && reaction_ok
            && revision_ok
            && opened.link_id != input.old_link_id,
        Some(opened.link_id),
        stages,
    ))
}

#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
struct OmenChatReactionSmokeOptions<'a> {
    link_id: [u8; 16],
    session_id: omenbrowser_rs::chat::ChatSessionId,
    room_id: u32,
    target_event_id: u64,
    server_destination: &'a str,
    identity_storage_root: &'a std::path::Path,
    authenticated_identity_hash: [u8; 16],
    wait: Duration,
}

#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
fn prepare_omenchat_smoke_reaction(
    store: &omenbrowser_rs::chat::mutation_intents::MutationIntentStore,
    options: &OmenChatReactionSmokeOptions<'_>,
    client_instance_id: omenbrowser_rs::chat::protocol::ClientInstanceId,
    action: omenbrowser_rs::chat::protocol::ReactionAction,
) -> anyhow::Result<omenbrowser_rs::chat::mutation_intents::OutboundMutationIntent> {
    use omenbrowser_rs::chat::mutation_intents::{
        IntentTransition, OutboundMutationState, PrepareOutboundMutation,
    };
    use omenbrowser_rs::chat::protocol::{ChatOp, ReactionRequest, ReactionToken};

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or_default();
    let body = ReactionRequest {
        target_event_id: options.target_event_id,
        token: ReactionToken::Heart,
        action,
    }
    .into_frame_body()
    .context("encode OMENchat smoke reaction")?;
    let prepared = store.persist_prepared(PrepareOutboundMutation {
        server_destination: options.server_destination,
        authenticated_identity_hash: &options.authenticated_identity_hash,
        client_instance_id,
        op: ChatOp::RoomReaction,
        room_id: Some(options.room_id),
        body,
        created_at: now,
        expires_at: now.saturating_add(60 * 60),
        correlation_id: Some("release-reaction-smoke"),
    })?;
    match store.transition(
        prepared.mutation_id,
        OutboundMutationState::Prepared,
        OutboundMutationState::SentUncertain,
    )? {
        IntentTransition::Updated(intent) => Ok(intent),
        other => anyhow::bail!("OMENchat smoke reaction transition failed: {other:?}"),
    }
}

#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
async fn send_omenchat_smoke_reaction(
    runtime: &dyn omenbrowser_rs::runtime::NetworkRuntime,
    client: &mut omenbrowser_rs::chat::ChatClient,
    live_state: &mut omenbrowser_rs::chat::live::LiveChatClientState,
    transport: &mut OmenChatSmokeTransport,
    options: &OmenChatReactionSmokeOptions<'_>,
    intent: &omenbrowser_rs::chat::mutation_intents::OutboundMutationIntent,
) -> anyhow::Result<Vec<serde_json::Value>> {
    let events = omenbrowser_rs::chat::live::send_uncertain_durable_reaction(
        client,
        live_state,
        transport,
        options.session_id,
        intent,
    );
    if events
        .iter()
        .any(|event| matches!(event, omenbrowser_rs::chat::ChatClientEvent::Error { .. }))
    {
        anyhow::bail!("OMENchat smoke reaction was rejected before transmission");
    }
    send_omenchat_smoke_outgoing(runtime, options.link_id, transport).await?;
    Ok(events.iter().map(format_chat_event).collect())
}

#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
async fn discard_omenchat_reaction_ack(
    runtime_events: &mut tokio::sync::broadcast::Receiver<RuntimeBusEvent>,
    link_id: [u8; 16],
    wait: Duration,
) -> anyhow::Result<serde_json::Value> {
    let deadline = tokio::time::Instant::now() + wait;
    while tokio::time::Instant::now() < deadline {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        match tokio::time::timeout(remaining, runtime_events.recv()).await {
            Ok(Ok(RuntimeBusEvent::OmenChatLinkData(data))) if data.link_id == link_id => {
                let frame = omenbrowser_rs::chat::codec::decode_frame(&data.frame_bytes)
                    .context("decode deliberately discarded OMENchat reaction response")?;
                if frame.op == omenbrowser_rs::chat::protocol::ChatOp::ReactionAck {
                    return Ok(serde_json::json!({
                        "stage": "reaction_lost_ack",
                        "ok": true,
                        "bytes": data.frame_bytes.len(),
                        "sequence": frame.seq,
                    }));
                }
            }
            Ok(Ok(_)) | Ok(Err(tokio::sync::broadcast::error::RecvError::Lagged(_))) => {}
            Ok(Err(tokio::sync::broadcast::error::RecvError::Closed)) | Err(_) => break,
        }
    }
    anyhow::bail!("OMENchat smoke did not observe the acknowledgement selected for loss")
}

#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
async fn run_omenchat_reaction_smoke(
    runtime: &dyn omenbrowser_rs::runtime::NetworkRuntime,
    runtime_events: &mut tokio::sync::broadcast::Receiver<RuntimeBusEvent>,
    client: &mut omenbrowser_rs::chat::ChatClient,
    live_state: &mut omenbrowser_rs::chat::live::LiveChatClientState,
    transport: &mut OmenChatSmokeTransport,
    options: OmenChatReactionSmokeOptions<'_>,
) -> anyhow::Result<(bool, Vec<serde_json::Value>)> {
    use omenbrowser_rs::chat::mutation_intents::{MutationIntentStore, OutboundMutationState};
    use omenbrowser_rs::chat::protocol::{ReactionAction, ReactionToken};
    use omenbrowser_rs::chat::ChatClientRequest;

    let mut stages = Vec::new();
    let negotiated = live_state.durable_mutations_negotiated(options.session_id)
        && live_state.reactions_negotiated(options.session_id);
    stages.push(serde_json::json!({
        "stage": "reaction_capability",
        "ok": negotiated,
    }));
    let Some(client_instance_id) = live_state.client_instance_id() else {
        return Ok((false, stages));
    };
    if !negotiated {
        return Ok((false, stages));
    }
    let store = MutationIntentStore::open_for_identity_storage_root(options.identity_storage_root)
        .context("open isolated OMENchat smoke mutation store")?;

    let add =
        prepare_omenchat_smoke_reaction(&store, &options, client_instance_id, ReactionAction::Add)?;
    let sent = send_omenchat_smoke_reaction(runtime, client, live_state, transport, &options, &add)
        .await?;
    stages.push(serde_json::json!({
        "stage": "reaction_add_send",
        "ok": true,
        "events": sent,
    }));
    stages
        .push(discard_omenchat_reaction_ack(runtime_events, options.link_id, options.wait).await?);

    let replayed =
        send_omenchat_smoke_reaction(runtime, client, live_state, transport, &options, &add)
            .await?;
    let replay_events = wait_for_omenchat_condition(
        runtime,
        runtime_events,
        client,
        live_state,
        transport,
        OmenChatWaitOptions {
            link_id: options.link_id,
            session_id: options.session_id,
            wait: options.wait,
        },
        |client| {
            client
                .session(options.session_id)
                .is_some_and(|session| session.status == "reaction accepted by server")
        },
    )
    .await;
    let replay_acknowledged = client
        .session(options.session_id)
        .is_some_and(|session| session.status == "reaction accepted by server");
    stages.push(serde_json::json!({
        "stage": "reaction_exact_replay",
        "ok": replay_acknowledged,
        "send_events": replayed,
        "events": replay_events,
    }));
    if replay_acknowledged {
        let _ = store.transition(
            add.mutation_id,
            OutboundMutationState::SentUncertain,
            OutboundMutationState::Acknowledged,
        )?;
    }

    let sync_events = omenbrowser_rs::chat::live::handle_live_request(
        client,
        live_state,
        transport,
        ChatClientRequest::SyncRecent {
            session_id: options.session_id,
        },
    );
    send_omenchat_smoke_outgoing(runtime, options.link_id, transport).await?;
    let snapshot_events = wait_for_omenchat_condition(
        runtime,
        runtime_events,
        client,
        live_state,
        transport,
        OmenChatWaitOptions {
            link_id: options.link_id,
            session_id: options.session_id,
            wait: options.wait,
        },
        |client| {
            client
                .reactions_for_targets(
                    options.session_id,
                    options.room_id,
                    &[options.target_event_id],
                )
                .iter()
                .any(|reaction| reaction.token == ReactionToken::Heart)
        },
    )
    .await;
    let resource_snapshot = snapshot_events.iter().any(|event| {
        event.get("event").and_then(serde_json::Value::as_str) == Some("resource_data")
            && omenchat_smoke_events_contain_decoded_event(
                std::slice::from_ref(event),
                "reaction_snapshot_applied",
            )
    });
    stages.push(serde_json::json!({
        "stage": "reaction_resource_snapshot",
        "ok": resource_snapshot,
        "request_events": sync_events.iter().map(format_chat_event).collect::<Vec<_>>(),
        "events": snapshot_events,
    }));

    let no_op =
        prepare_omenchat_smoke_reaction(&store, &options, client_instance_id, ReactionAction::Add)?;
    let _ = send_omenchat_smoke_reaction(runtime, client, live_state, transport, &options, &no_op)
        .await?;
    let no_op_events = wait_for_omenchat_condition(
        runtime,
        runtime_events,
        client,
        live_state,
        transport,
        OmenChatWaitOptions {
            link_id: options.link_id,
            session_id: options.session_id,
            wait: options.wait,
        },
        |client| {
            client.session(options.session_id).is_some_and(|session| {
                session.status == "reaction already matched the requested state"
            })
        },
    )
    .await;
    let no_op_ok = client
        .session(options.session_id)
        .is_some_and(|session| session.status == "reaction already matched the requested state");
    if no_op_ok {
        let _ = store.transition(
            no_op.mutation_id,
            OutboundMutationState::SentUncertain,
            OutboundMutationState::Acknowledged,
        )?;
    }
    stages.push(serde_json::json!({
        "stage": "reaction_noop_add",
        "ok": no_op_ok,
        "events": no_op_events,
    }));

    let remove = prepare_omenchat_smoke_reaction(
        &store,
        &options,
        client_instance_id,
        ReactionAction::Remove,
    )?;
    let _ = send_omenchat_smoke_reaction(runtime, client, live_state, transport, &options, &remove)
        .await?;
    let remove_events = wait_for_omenchat_condition(
        runtime,
        runtime_events,
        client,
        live_state,
        transport,
        OmenChatWaitOptions {
            link_id: options.link_id,
            session_id: options.session_id,
            wait: options.wait,
        },
        |client| {
            client
                .session(options.session_id)
                .is_some_and(|session| session.status == "reaction accepted by server")
        },
    )
    .await;
    let remove_ok = client
        .session(options.session_id)
        .is_some_and(|session| session.status == "reaction accepted by server");
    if remove_ok {
        let _ = store.transition(
            remove.mutation_id,
            OutboundMutationState::SentUncertain,
            OutboundMutationState::Acknowledged,
        )?;
    }
    stages.push(serde_json::json!({
        "stage": "reaction_remove",
        "ok": remove_ok,
        "events": remove_events,
    }));

    let remove_sync_events = omenbrowser_rs::chat::live::handle_live_request(
        client,
        live_state,
        transport,
        ChatClientRequest::SyncRecent {
            session_id: options.session_id,
        },
    );
    send_omenchat_smoke_outgoing(runtime, options.link_id, transport).await?;
    let remove_snapshot_events = wait_for_omenchat_condition(
        runtime,
        runtime_events,
        client,
        live_state,
        transport,
        OmenChatWaitOptions {
            link_id: options.link_id,
            session_id: options.session_id,
            wait: options.wait,
        },
        |client| {
            client.session(options.session_id).is_some_and(|session| {
                session.status != "requested recent room history"
                    && client
                        .reactions_for_targets(
                            options.session_id,
                            options.room_id,
                            &[options.target_event_id],
                        )
                        .iter()
                        .all(|reaction| reaction.token != ReactionToken::Heart)
            })
        },
    )
    .await;
    let remove_snapshot_ok = client
        .reactions_for_targets(
            options.session_id,
            options.room_id,
            &[options.target_event_id],
        )
        .iter()
        .all(|reaction| reaction.token != ReactionToken::Heart);
    stages.push(serde_json::json!({
        "stage": "reaction_remove_snapshot",
        "ok": remove_snapshot_ok,
        "request_events": remove_sync_events.iter().map(format_chat_event).collect::<Vec<_>>(),
        "events": remove_snapshot_events,
    }));

    let recovered = store.recover_nonterminal()?;
    let persistence_ok = recovered.is_empty();
    stages.push(serde_json::json!({
        "stage": "reaction_intent_persistence",
        "ok": persistence_ok,
        "nonterminal_count": recovered.len(),
    }));
    Ok((
        replay_acknowledged
            && resource_snapshot
            && no_op_ok
            && remove_ok
            && remove_snapshot_ok
            && persistence_ok,
        stages,
    ))
}

#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
struct OmenChatRevisionSmokeOptions<'a> {
    link_id: [u8; 16],
    session_id: omenbrowser_rs::chat::ChatSessionId,
    room_id: u32,
    target_event_id: u64,
    server_destination: &'a str,
    identity_storage_root: &'a std::path::Path,
    authenticated_identity_hash: [u8; 16],
    wait: Duration,
}

#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
fn prepare_omenchat_smoke_revision(
    store: &omenbrowser_rs::chat::mutation_intents::MutationIntentStore,
    options: &OmenChatRevisionSmokeOptions<'_>,
    client_instance_id: omenbrowser_rs::chat::protocol::ClientInstanceId,
    action: omenbrowser_rs::chat::protocol::MessageRevisionAction,
    replacement: Option<String>,
) -> anyhow::Result<omenbrowser_rs::chat::mutation_intents::OutboundMutationIntent> {
    use omenbrowser_rs::chat::mutation_intents::{
        IntentTransition, OutboundMutationState, PrepareOutboundMutation,
    };
    use omenbrowser_rs::chat::protocol::{ChatOp, MessageRevisionRequest};

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or_default();
    let body = MessageRevisionRequest {
        target_event_id: options.target_event_id,
        action,
        replacement,
    }
    .into_frame_body()
    .context("encode OMENchat smoke message revision")?;
    let prepared = store.persist_prepared(PrepareOutboundMutation {
        server_destination: options.server_destination,
        authenticated_identity_hash: &options.authenticated_identity_hash,
        client_instance_id,
        op: ChatOp::RoomMessageRevision,
        room_id: Some(options.room_id),
        body,
        created_at: now,
        expires_at: now.saturating_add(60 * 60),
        correlation_id: Some("release-message-revision-smoke"),
    })?;
    match store.transition(
        prepared.mutation_id,
        OutboundMutationState::Prepared,
        OutboundMutationState::SentUncertain,
    )? {
        IntentTransition::Updated(intent) => Ok(intent),
        other => anyhow::bail!("OMENchat smoke message revision transition failed: {other:?}"),
    }
}

#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
async fn send_omenchat_smoke_revision(
    runtime: &dyn omenbrowser_rs::runtime::NetworkRuntime,
    client: &mut omenbrowser_rs::chat::ChatClient,
    live_state: &mut omenbrowser_rs::chat::live::LiveChatClientState,
    transport: &mut OmenChatSmokeTransport,
    options: &OmenChatRevisionSmokeOptions<'_>,
    intent: &omenbrowser_rs::chat::mutation_intents::OutboundMutationIntent,
) -> anyhow::Result<Vec<serde_json::Value>> {
    let events = omenbrowser_rs::chat::live::send_uncertain_durable_message_revision(
        client,
        live_state,
        transport,
        options.session_id,
        intent,
    );
    if events
        .iter()
        .any(|event| matches!(event, omenbrowser_rs::chat::ChatClientEvent::Error { .. }))
    {
        anyhow::bail!("OMENchat smoke message revision was rejected before transmission");
    }
    send_omenchat_smoke_outgoing(runtime, options.link_id, transport).await?;
    Ok(events.iter().map(format_chat_event).collect())
}

#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
async fn discard_omenchat_revision_ack(
    runtime_events: &mut tokio::sync::broadcast::Receiver<RuntimeBusEvent>,
    link_id: [u8; 16],
    wait: Duration,
) -> anyhow::Result<serde_json::Value> {
    let deadline = tokio::time::Instant::now() + wait;
    while tokio::time::Instant::now() < deadline {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        match tokio::time::timeout(remaining, runtime_events.recv()).await {
            Ok(Ok(RuntimeBusEvent::OmenChatLinkData(data))) if data.link_id == link_id => {
                let frame = omenbrowser_rs::chat::codec::decode_frame(&data.frame_bytes)
                    .context("decode deliberately discarded OMENchat revision response")?;
                if frame.op == omenbrowser_rs::chat::protocol::ChatOp::MessageRevisionAck {
                    return Ok(serde_json::json!({
                        "stage": "revision_lost_ack",
                        "ok": true,
                        "bytes": data.frame_bytes.len(),
                        "sequence": frame.seq,
                    }));
                }
            }
            Ok(Ok(_)) | Ok(Err(tokio::sync::broadcast::error::RecvError::Lagged(_))) => {}
            Ok(Err(tokio::sync::broadcast::error::RecvError::Closed)) | Err(_) => break,
        }
    }
    anyhow::bail!("OMENchat smoke did not observe the revision acknowledgement selected for loss")
}

#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
async fn run_omenchat_revision_smoke(
    runtime: &dyn omenbrowser_rs::runtime::NetworkRuntime,
    runtime_events: &mut tokio::sync::broadcast::Receiver<RuntimeBusEvent>,
    client: &mut omenbrowser_rs::chat::ChatClient,
    live_state: &mut omenbrowser_rs::chat::live::LiveChatClientState,
    transport: &mut OmenChatSmokeTransport,
    options: OmenChatRevisionSmokeOptions<'_>,
) -> anyhow::Result<(bool, Vec<serde_json::Value>)> {
    use omenbrowser_rs::chat::mutation_intents::{MutationIntentStore, OutboundMutationState};
    use omenbrowser_rs::chat::protocol::MessageRevisionAction;
    use omenbrowser_rs::chat::ChatClientRequest;

    let mut stages = Vec::new();
    let negotiated = live_state.durable_mutations_negotiated(options.session_id)
        && live_state.message_revisions_negotiated(options.session_id);
    stages.push(serde_json::json!({
        "stage": "revision_capability",
        "ok": negotiated,
    }));
    let Some(client_instance_id) = live_state.client_instance_id() else {
        return Ok((false, stages));
    };
    if !negotiated {
        return Ok((false, stages));
    }
    let store = MutationIntentStore::open_for_identity_storage_root(options.identity_storage_root)
        .context("open isolated OMENchat revision smoke mutation store")?;

    let corrected_body = "OMENchat smoke corrected".to_owned();
    let correction = prepare_omenchat_smoke_revision(
        &store,
        &options,
        client_instance_id,
        MessageRevisionAction::Correct,
        Some(corrected_body.clone()),
    )?;
    let sent = send_omenchat_smoke_revision(
        runtime,
        client,
        live_state,
        transport,
        &options,
        &correction,
    )
    .await?;
    stages.push(serde_json::json!({
        "stage": "revision_correction_send",
        "ok": true,
        "events": sent,
    }));
    stages
        .push(discard_omenchat_revision_ack(runtime_events, options.link_id, options.wait).await?);

    let replayed = send_omenchat_smoke_revision(
        runtime,
        client,
        live_state,
        transport,
        &options,
        &correction,
    )
    .await?;
    let replay_events = wait_for_omenchat_condition(
        runtime,
        runtime_events,
        client,
        live_state,
        transport,
        OmenChatWaitOptions {
            link_id: options.link_id,
            session_id: options.session_id,
            wait: options.wait,
        },
        |client| {
            client.session(options.session_id).is_some_and(|session| {
                session.status == "message revision accepted by server; awaiting room event"
            })
        },
    )
    .await;
    let correction_acknowledged = client.session(options.session_id).is_some_and(|session| {
        session.status == "message revision accepted by server; awaiting room event"
    });
    stages.push(serde_json::json!({
        "stage": "revision_exact_replay",
        "ok": correction_acknowledged,
        "send_events": replayed,
        "events": replay_events,
    }));
    if correction_acknowledged {
        let _ = store.transition(
            correction.mutation_id,
            OutboundMutationState::SentUncertain,
            OutboundMutationState::Acknowledged,
        )?;
    }

    let sync_events = omenbrowser_rs::chat::live::handle_live_request(
        client,
        live_state,
        transport,
        ChatClientRequest::SyncRecent {
            session_id: options.session_id,
        },
    );
    send_omenchat_smoke_outgoing(runtime, options.link_id, transport).await?;
    let snapshot_events = wait_for_omenchat_condition(
        runtime,
        runtime_events,
        client,
        live_state,
        transport,
        OmenChatWaitOptions {
            link_id: options.link_id,
            session_id: options.session_id,
            wait: options.wait,
        },
        |client| {
            client
                .message_revision_for_target(
                    options.session_id,
                    options.room_id,
                    options.target_event_id,
                )
                .is_some_and(|revision| {
                    revision.action == MessageRevisionAction::Correct
                        && revision.replacement_body.as_deref() == Some(corrected_body.as_str())
                })
                && client.message_revision_snapshot_complete(
                    options.session_id,
                    options.room_id,
                    options.target_event_id,
                )
        },
    )
    .await;
    let correction_resource_snapshot = snapshot_events.iter().any(|event| {
        event.get("event").and_then(serde_json::Value::as_str) == Some("resource_data")
            && omenchat_smoke_events_contain_decoded_event(
                std::slice::from_ref(event),
                "message_revision_snapshot_applied",
            )
    }) && client
        .message_revision_for_target(options.session_id, options.room_id, options.target_event_id)
        .is_some_and(|revision| {
            revision.action == MessageRevisionAction::Correct
                && revision.replacement_body.as_deref() == Some(corrected_body.as_str())
        });
    stages.push(serde_json::json!({
        "stage": "revision_correction_resource_snapshot",
        "ok": correction_resource_snapshot,
        "request_events": sync_events.iter().map(format_chat_event).collect::<Vec<_>>(),
        "events": snapshot_events,
    }));

    let tombstone = prepare_omenchat_smoke_revision(
        &store,
        &options,
        client_instance_id,
        MessageRevisionAction::Tombstone,
        None,
    )?;
    if let Some(session) = client.session_mut(options.session_id) {
        session.status = "awaiting message tombstone acknowledgement".into();
    }
    let _ =
        send_omenchat_smoke_revision(runtime, client, live_state, transport, &options, &tombstone)
            .await?;
    let tombstone_events = wait_for_omenchat_condition(
        runtime,
        runtime_events,
        client,
        live_state,
        transport,
        OmenChatWaitOptions {
            link_id: options.link_id,
            session_id: options.session_id,
            wait: options.wait,
        },
        |client| {
            client.session(options.session_id).is_some_and(|session| {
                session.status == "message revision accepted by server; awaiting room event"
            })
        },
    )
    .await;
    let tombstone_acknowledged = client.session(options.session_id).is_some_and(|session| {
        session.status == "message revision accepted by server; awaiting room event"
    });
    if tombstone_acknowledged {
        let _ = store.transition(
            tombstone.mutation_id,
            OutboundMutationState::SentUncertain,
            OutboundMutationState::Acknowledged,
        )?;
    }
    stages.push(serde_json::json!({
        "stage": "revision_tombstone",
        "ok": tombstone_acknowledged,
        "events": tombstone_events,
    }));

    let tombstone_sync_events = omenbrowser_rs::chat::live::handle_live_request(
        client,
        live_state,
        transport,
        ChatClientRequest::SyncRecent {
            session_id: options.session_id,
        },
    );
    send_omenchat_smoke_outgoing(runtime, options.link_id, transport).await?;
    let tombstone_snapshot_events = wait_for_omenchat_condition(
        runtime,
        runtime_events,
        client,
        live_state,
        transport,
        OmenChatWaitOptions {
            link_id: options.link_id,
            session_id: options.session_id,
            wait: options.wait,
        },
        |client| {
            client
                .message_revision_for_target(
                    options.session_id,
                    options.room_id,
                    options.target_event_id,
                )
                .is_some_and(|revision| revision.action == MessageRevisionAction::Tombstone)
                && client.message_revision_snapshot_complete(
                    options.session_id,
                    options.room_id,
                    options.target_event_id,
                )
        },
    )
    .await;
    let tombstone_resource_snapshot = tombstone_snapshot_events.iter().any(|event| {
        event.get("event").and_then(serde_json::Value::as_str) == Some("resource_data")
            && omenchat_smoke_events_contain_decoded_event(
                std::slice::from_ref(event),
                "message_revision_snapshot_applied",
            )
    }) && client
        .message_revision_for_target(options.session_id, options.room_id, options.target_event_id)
        .is_some_and(|revision| revision.action == MessageRevisionAction::Tombstone);
    stages.push(serde_json::json!({
        "stage": "revision_tombstone_resource_snapshot",
        "ok": tombstone_resource_snapshot,
        "request_events": tombstone_sync_events.iter().map(format_chat_event).collect::<Vec<_>>(),
        "events": tombstone_snapshot_events,
    }));

    let recovered = store.recover_nonterminal()?;
    let persistence_ok = recovered.is_empty();
    stages.push(serde_json::json!({
        "stage": "revision_intent_persistence",
        "ok": persistence_ok,
        "nonterminal_count": recovered.len(),
    }));
    Ok((
        correction_acknowledged
            && correction_resource_snapshot
            && tombstone_acknowledged
            && tombstone_resource_snapshot
            && persistence_ok,
        stages,
    ))
}

#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
fn create_omenchat_reconnect_ready_file(path: &std::path::Path) -> anyhow::Result<()> {
    use std::io::Write;

    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .ok_or_else(|| anyhow::anyhow!("OMENchat reconnect marker needs an existing parent"))?;
    if !parent.is_dir() {
        anyhow::bail!("OMENchat reconnect marker parent is not a directory");
    }
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .context("failed to create OMENchat reconnect ready marker")?;
    file.write_all(b"ready\n")
        .context("failed to write OMENchat reconnect ready marker")?;
    file.sync_all()
        .context("failed to sync OMENchat reconnect ready marker")
}

#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
struct OmenchatSmokeUploadFetch<'a> {
    runtime: &'a dyn omenbrowser_rs::runtime::NetworkRuntime,
    runtime_events: &'a mut tokio::sync::broadcast::Receiver<RuntimeBusEvent>,
    link_id: [u8; 16],
    client: &'a mut omenbrowser_rs::chat::ChatClient,
    live_state: &'a mut omenbrowser_rs::chat::live::LiveChatClientState,
    transport: &'a mut OmenChatSmokeTransport,
    session_id: omenbrowser_rs::chat::ChatSessionId,
    room: &'a str,
    resource_id: String,
    filename: &'a str,
    bytes: Option<u64>,
    wait: Duration,
}

#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
async fn run_omenchat_smoke_upload_fetch(
    input: OmenchatSmokeUploadFetch<'_>,
) -> anyhow::Result<(bool, Vec<serde_json::Value>)> {
    let request_upload_events = omenbrowser_rs::chat::live::handle_live_request(
        input.client,
        input.live_state,
        input.transport,
        omenbrowser_rs::chat::ChatClientRequest::RequestUpload {
            session_id: input.session_id,
            room: input.room.to_owned(),
            resource_id: input.resource_id.clone(),
        },
    );
    let mut stages = vec![serde_json::json!({
        "stage": "upload_fetch_frame",
        "ok": !request_upload_events.iter().any(|event| matches!(event, omenbrowser_rs::chat::ChatClientEvent::Error { .. })),
        "resource_id": input.resource_id,
        "filename": input.filename,
        "bytes": input.bytes,
        "events": request_upload_events.iter().map(format_chat_event).collect::<Vec<_>>(),
    })];
    send_omenchat_smoke_outgoing(input.runtime, input.link_id, input.transport).await?;

    let upload_fetch_events = wait_for_omenchat_condition(
        input.runtime,
        input.runtime_events,
        input.client,
        input.live_state,
        input.transport,
        OmenChatWaitOptions {
            link_id: input.link_id,
            session_id: input.session_id,
            wait: input.wait,
        },
        |client| {
            omenchat_session_upload_resource_received(client, input.session_id, input.filename)
        },
    )
    .await;
    let upload_resource_available =
        omenchat_session_upload_resource_received(input.client, input.session_id, input.filename)
            && omenchat_smoke_events_contain_decoded_event(
                &upload_fetch_events,
                "upload_resource_available",
            );
    stages.push(serde_json::json!({
        "stage": "upload_fetch_wait",
        "ok": upload_resource_available,
        "filename": input.filename,
        "bytes": input.bytes,
        "events": upload_fetch_events,
    }));
    Ok((upload_resource_available, stages))
}

#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
async fn collect_runtime_trace(
    runtime_events: &mut tokio::sync::broadcast::Receiver<RuntimeBusEvent>,
    wait: Duration,
    destination_filter: Option<&str>,
) -> Vec<serde_json::Value> {
    let deadline = tokio::time::Instant::now() + wait;
    let mut events = Vec::new();
    while tokio::time::Instant::now() < deadline {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        let received = tokio::time::timeout(remaining, runtime_events.recv()).await;
        let event = match received {
            Ok(Ok(event)) => event,
            Ok(Err(tokio::sync::broadcast::error::RecvError::Lagged(count))) => {
                events.push(serde_json::json!({"event": "lagged", "count": count}));
                continue;
            }
            Ok(Err(tokio::sync::broadcast::error::RecvError::Closed)) => break,
            Err(_) => break,
        };
        match event {
            RuntimeBusEvent::PathUpdated(path)
                if destination_filter.is_none_or(|destination| {
                    path.destination_hash.eq_ignore_ascii_case(destination)
                }) =>
            {
                let known = path.known;
                events.push(serde_json::json!({"event": "path", "value": path}));
                if known {
                    break;
                }
            }
            RuntimeBusEvent::Announce(announce)
                if destination_filter.is_none_or(|destination| {
                    announce.destination_hash.eq_ignore_ascii_case(destination)
                }) =>
            {
                events.push(serde_json::json!({"event": "announce", "value": announce}));
            }
            RuntimeBusEvent::Debug(message) => {
                events.push(serde_json::json!({"event": "debug", "message": message}));
            }
            RuntimeBusEvent::Error(message) => {
                events.push(serde_json::json!({"event": "error", "message": message}));
            }
            _ => {}
        }
    }
    events
}

#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
fn omenchat_session_contains_message(
    client: &omenbrowser_rs::chat::ChatClient,
    session_id: omenbrowser_rs::chat::ChatSessionId,
    message: &str,
) -> bool {
    client.session(session_id).is_some_and(|session| {
        session.events.iter().any(|event| {
            if event.event_id > u64::MAX.saturating_sub(1_000_000) {
                return false;
            }
            matches!(
                &event.kind,
                omenbrowser_rs::chat::ChatEventKind::Message { body }
                    | omenbrowser_rs::chat::ChatEventKind::Action { body }
                    | omenbrowser_rs::chat::ChatEventKind::Notice { body }
                    | omenbrowser_rs::chat::ChatEventKind::System { body }
                    if body == message
            )
        })
    })
}

#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
fn omenchat_session_message_event_id(
    client: &omenbrowser_rs::chat::ChatClient,
    session_id: omenbrowser_rs::chat::ChatSessionId,
    message: &str,
) -> Option<u64> {
    client.session(session_id).and_then(|session| {
        session.events.iter().rev().find_map(|event| {
            if event.event_id > u64::MAX.saturating_sub(1_000_000) {
                return None;
            }
            matches!(
                &event.kind,
                omenbrowser_rs::chat::ChatEventKind::Message { body }
                    | omenbrowser_rs::chat::ChatEventKind::Action { body }
                    | omenbrowser_rs::chat::ChatEventKind::Notice { body }
                    | omenbrowser_rs::chat::ChatEventKind::System { body }
                    if body == message
            )
            .then_some(event.event_id)
        })
    })
}

#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
fn omenchat_session_upload_resource_id(
    client: &omenbrowser_rs::chat::ChatClient,
    session_id: omenbrowser_rs::chat::ChatSessionId,
    filename: &str,
    bytes: Option<u64>,
) -> Option<String> {
    client.session(session_id).and_then(|session| {
        session
            .events
            .iter()
            .rev()
            .find_map(|event| match &event.kind {
                omenbrowser_rs::chat::ChatEventKind::Upload {
                    resource_id,
                    filename: event_filename,
                    bytes: event_bytes,
                } if event_filename == filename
                    && bytes.is_none_or(|bytes| *event_bytes == bytes) =>
                {
                    Some(resource_id.clone())
                }
                _ => None,
            })
    })
}

#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
fn omenchat_session_upload_resource_received(
    client: &omenbrowser_rs::chat::ChatClient,
    session_id: omenbrowser_rs::chat::ChatSessionId,
    filename: &str,
) -> bool {
    client.session(session_id).is_some_and(|session| {
        session.status.starts_with("upload resource received:") && session.status.contains(filename)
    })
}

#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
fn omenchat_smoke_events_contain_decoded_event(
    events: &[serde_json::Value],
    event_name: &str,
) -> bool {
    events.iter().any(|entry| {
        entry
            .get("decoded")
            .and_then(serde_json::Value::as_array)
            .is_some_and(|decoded| {
                decoded.iter().any(|event| {
                    event.get("event").and_then(serde_json::Value::as_str) == Some(event_name)
                })
            })
    })
}

#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
fn format_chat_event(event: &omenbrowser_rs::chat::ChatClientEvent) -> serde_json::Value {
    match event {
        omenbrowser_rs::chat::ChatClientEvent::ServerOpened { session_id, server } => {
            serde_json::json!({
                "event": "server_opened",
                "session_id": session_id,
                "server": {
                    "server_id": server.server_id.clone(),
                    "destination": server.destination.clone(),
                    "display_name": server.display_name.clone(),
                }
            })
        }
        omenbrowser_rs::chat::ChatClientEvent::RoomJoined {
            session_id,
            room,
            users,
            latest_events,
        } => serde_json::json!({
            "event": "room_joined",
            "session_id": session_id,
            "room": {
                "server_id": room.server_id.clone(),
                "room_id": room.room_id,
                "name": room.name.clone(),
                "unread": room.unread,
                "joined": room.joined,
            },
            "users": users.len(),
            "latest_events": latest_events.len(),
        }),
        omenbrowser_rs::chat::ChatClientEvent::EventAppended { session_id, event } => {
            serde_json::json!({
                "event": "event_appended",
                "session_id": session_id,
                "chat_event": format_chat_timeline_event(event),
            })
        }
        omenbrowser_rs::chat::ChatClientEvent::DurableMutationAcknowledged {
            session_id,
            mutation_id,
        } => serde_json::json!({
            "event": "durable_mutation_acknowledged",
            "session_id": session_id,
            "mutation_id": mutation_id
                .as_bytes()
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>(),
        }),
        omenbrowser_rs::chat::ChatClientEvent::DurableMutationTerminal {
            session_id,
            mutation_id,
            state,
        } => serde_json::json!({
            "event": "durable_mutation_terminal",
            "session_id": session_id,
            "mutation_id": mutation_id
                .as_bytes()
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>(),
            "state": match state {
                omenbrowser_rs::chat::DurableMutationTerminalState::Conflict => "conflict",
                omenbrowser_rs::chat::DurableMutationTerminalState::Expired => "expired",
            },
        }),
        omenbrowser_rs::chat::ChatClientEvent::HistoryPrepended { session_id, events } => {
            serde_json::json!({"event": "history_prepended", "session_id": session_id, "events": events.len()})
        }
        omenbrowser_rs::chat::ChatClientEvent::HistorySynced {
            session_id,
            room_id,
        } => {
            serde_json::json!({"event": "history_synced", "session_id": session_id, "room_id": room_id})
        }
        omenbrowser_rs::chat::ChatClientEvent::HistorySyncNeeded {
            session_id,
            room_id,
        } => {
            serde_json::json!({"event": "history_sync_needed", "session_id": session_id, "room_id": room_id})
        }
        omenbrowser_rs::chat::ChatClientEvent::ReactionDeltaApplied {
            session_id,
            room_id,
            event,
        } => serde_json::json!({
            "event": "reaction_delta_applied",
            "session_id": session_id,
            "room_id": room_id,
            "target_event_id": event.target_event_id,
            "actor_user_id": event.actor_user_id,
            "reaction": event.token.as_str(),
            "action": event.action as u8,
        }),
        omenbrowser_rs::chat::ChatClientEvent::ReactionSnapshotApplied {
            session_id,
            room_id,
            snapshot,
        } => serde_json::json!({
            "event": "reaction_snapshot_applied",
            "session_id": session_id,
            "room_id": room_id,
            "targets": snapshot.target_event_ids.len(),
            "entries": snapshot.entries.len(),
        }),
        omenbrowser_rs::chat::ChatClientEvent::MessageRevisionDeltaApplied {
            session_id,
            room_id,
            event,
        } => serde_json::json!({
            "event": "message_revision_delta_applied",
            "session_id": session_id,
            "room_id": room_id,
            "target_event_id": event.target_event_id,
            "revision_event_id": event.revision_event_id,
            "action": event.action as u8,
            "revision_number": event.revision_number,
        }),
        omenbrowser_rs::chat::ChatClientEvent::MessageRevisionSnapshotApplied {
            session_id,
            room_id,
            snapshot,
        } => serde_json::json!({
            "event": "message_revision_snapshot_applied",
            "session_id": session_id,
            "room_id": room_id,
            "targets": snapshot.target_event_ids.len(),
            "entries": snapshot.entries.len(),
        }),
        omenbrowser_rs::chat::ChatClientEvent::RoomsUpdated { session_id, rooms } => {
            serde_json::json!({"event": "rooms_updated", "session_id": session_id, "rooms": rooms.len()})
        }
        omenbrowser_rs::chat::ChatClientEvent::ServerMotd { session_id, motd } => {
            serde_json::json!({"event": "server_motd", "session_id": session_id, "motd": motd})
        }
        omenbrowser_rs::chat::ChatClientEvent::ServerPolicy {
            session_id,
            upload_quota_bytes,
            upload_max_file_bytes,
            ping_interval_seconds,
        } => {
            serde_json::json!({
                "event": "server_policy",
                "session_id": session_id,
                "upload_quota_bytes": upload_quota_bytes,
                "upload_max_file_bytes": upload_max_file_bytes,
                "ping_interval_seconds": ping_interval_seconds,
            })
        }
        omenbrowser_rs::chat::ChatClientEvent::UserUpdated { session_id, user } => {
            serde_json::json!({
                "event": "user_updated",
                "session_id": session_id,
                "user": {
                    "server_id": user.server_id.clone(),
                    "user_id": user.user_id,
                    "display_name": user.display_name.clone(),
                    "role_bits": user.role_bits,
                    "status_bits": user.status_bits,
                    "lxmf_available": user.lxmf_available,
                }
            })
        }
        omenbrowser_rs::chat::ChatClientEvent::LocalUserBound {
            session_id,
            user_id,
        } => serde_json::json!({
            "event": "local_user_bound",
            "session_id": session_id,
            "user_id": user_id,
        }),
        omenbrowser_rs::chat::ChatClientEvent::UploadAccepted {
            session_id,
            resource_id,
            filename,
            bytes,
        } => {
            serde_json::json!({
                "event": "upload_accepted",
                "session_id": session_id,
                "resource_id": resource_id,
                "filename": filename,
                "bytes": bytes,
            })
        }
        omenbrowser_rs::chat::ChatClientEvent::UploadRejected { session_id, reason } => {
            serde_json::json!({
                "event": "upload_rejected",
                "session_id": session_id,
                "reason": reason,
            })
        }
        omenbrowser_rs::chat::ChatClientEvent::UploadCompleted {
            session_id,
            resource_id,
            filename,
            bytes,
        } => {
            serde_json::json!({
                "event": "upload_completed",
                "session_id": session_id,
                "resource_id": resource_id,
                "filename": filename,
                "bytes": bytes,
            })
        }
        omenbrowser_rs::chat::ChatClientEvent::UploadResourceAvailable {
            session_id,
            resource_id,
            filename,
            content_type,
            bytes,
        } => {
            serde_json::json!({
                "event": "upload_resource_available",
                "session_id": session_id,
                "resource_id": resource_id,
                "filename": filename,
                "content_type": content_type,
                "bytes": bytes.len(),
            })
        }
        omenbrowser_rs::chat::ChatClientEvent::UploadResourceProgress {
            session_id,
            resource_id,
            filename,
            received,
            total,
        } => {
            serde_json::json!({
                "event": "upload_resource_progress",
                "session_id": session_id,
                "resource_id": resource_id,
                "filename": filename,
                "received": received,
                "total": total,
            })
        }
        omenbrowser_rs::chat::ChatClientEvent::Error {
            session_id,
            message,
        } => serde_json::json!({"event": "error", "session_id": session_id, "message": message}),
    }
}

#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
fn format_chat_timeline_event(event: &omenbrowser_rs::chat::ChatEvent) -> serde_json::Value {
    let (kind, body) = match &event.kind {
        omenbrowser_rs::chat::ChatEventKind::Message { body }
        | omenbrowser_rs::chat::ChatEventKind::RichMessage { body, .. } => {
            ("message", body.as_str())
        }
        omenbrowser_rs::chat::ChatEventKind::Action { body } => ("action", body.as_str()),
        omenbrowser_rs::chat::ChatEventKind::Notice { body } => ("notice", body.as_str()),
        omenbrowser_rs::chat::ChatEventKind::System { body } => ("system", body.as_str()),
        omenbrowser_rs::chat::ChatEventKind::Upload { filename, .. } => {
            ("upload", filename.as_str())
        }
    };
    let mut value = serde_json::json!({
        "server_id": event.server_id.clone(),
        "room_id": event.room_id,
        "event_id": event.event_id,
        "actor_user_id": event.actor_user_id,
        "at_unix": event.at_unix,
        "kind": kind,
        "body": body,
    });
    if let omenbrowser_rs::chat::ChatEventKind::Upload {
        resource_id,
        filename,
        bytes,
    } = &event.kind
    {
        value["resource_id"] = serde_json::json!(resource_id);
        value["filename"] = serde_json::json!(filename);
        value["bytes"] = serde_json::json!(bytes);
    }
    if let omenbrowser_rs::chat::ChatEventKind::RichMessage { metadata, .. } = &event.kind {
        value["reply_to_event_id"] = serde_json::json!(metadata.reply_to_event_id);
        value["mentioned_user_ids"] = serde_json::json!(metadata.mentioned_user_ids);
    }
    value
}

#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
fn omenchat_smoke_report(
    ok: bool,
    stage: &str,
    destination: &str,
    room: &str,
    message: &str,
    stages: Vec<serde_json::Value>,
    session: Option<serde_json::Value>,
) -> serde_json::Value {
    serde_json::json!({
        "report": "omenchat_smoke",
        "classification": {
            "outcome": if ok { "pass" } else { "fail" },
            "stage": stage,
            "reason": if ok {
                "OMENchat Link opened, room joined, and message echo was observed"
            } else {
                "OMENchat smoke did not complete all required stages"
            },
            "next_step": if ok {
                "test from the desktop OMENchat pane"
            } else {
                "inspect stages; common blockers are missing destination key/path, no server announce, or Link response timeout"
            },
        },
        "destination": destination,
        "room": room,
        "message": message,
        "stages": stages,
        "session": session,
    })
}

#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
fn write_omenchat_smoke_report(
    report: serde_json::Value,
    output: Option<PathBuf>,
    stdout: bool,
    default_output: bool,
    diagnostics_dir: &std::path::Path,
) -> anyhow::Result<()> {
    let content =
        serde_json::to_string_pretty(&report).context("failed to render OMENchat smoke JSON")?;
    let outcome = report
        .pointer("/classification/outcome")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("unknown");
    let stage = report
        .pointer("/classification/stage")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("unknown");
    if stdout {
        eprintln!("OMENchat smoke: {outcome} at {stage}");
        println!("{content}");
    }
    if let Some(path) = output
        .or_else(|| default_output.then(|| default_omenchat_smoke_report_path(diagnostics_dir)))
    {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).with_context(|| {
                format!("failed to create output directory {}", parent.display())
            })?;
        }
        std::fs::write(&path, content.as_bytes())
            .with_context(|| format!("failed to write OMENchat smoke report {}", path.display()))?;
        if stdout {
            eprintln!("{}", path.display());
        } else {
            println!("{}", path.display());
        }
    }
    Ok(())
}

#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
fn default_omenchat_smoke_report_path(diagnostics_dir: &std::path::Path) -> PathBuf {
    let epoch = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    diagnostics_dir.join(format!("omenchat-smoke-{epoch}.json"))
}

#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
fn hex_bytes(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

async fn run_lxmf_interop_command(input: LxmfInteropCommandInput) -> anyhow::Result<()> {
    let LxmfInteropCommandInput {
        peer_hash,
        lxmf_smoke_delivery_mode,
        lxmf_smoke_propagation_node,
        lxmf_include_ticket,
        wait_secs,
        output,
        stdout,
        suggest_shell,
        bundle_report,
        overrides,
    } = input;
    let mut config = load_config_for_smoke(overrides.app_root().cloned())
        .context("failed to load LXMF interop app configuration")?;
    let interface_override = apply_smoke_overrides(&mut config, overrides.clone());
    let default_output = output.is_none() && !stdout && bundle_report.is_none();
    let diagnostics_dir = config.paths.diagnostics_dir.clone();
    let logs_dir = config.paths.logs_dir.clone();
    let identity_path = config.settings.identity_path.clone();
    let mut app = App::try_new(config).context("failed to initialize application services")?;
    app.start_runtime_for_smoke_test_with_interfaces(interface_override)
        .await
        .context("failed to start configured runtime for LXMF interop")?;
    let mut report = app
        .native_lxmf_live_interop_report(
            peer_hash,
            wait_secs,
            lxmf_smoke_delivery_mode,
            lxmf_smoke_propagation_node,
            lxmf_include_ticket,
        )
        .await
        .context("failed to collect native LXMF live interop report")?;
    add_lxmf_interop_suggested_commands(&mut report, wait_secs);
    let content =
        serde_json::to_string_pretty(&report).context("failed to render LXMF report JSON")?;

    let summary = render_report_summary_with_options(&report, suggest_shell);

    if stdout {
        eprintln!("{summary}");
        println!("{content}");
    } else if suggest_shell {
        eprintln!("{summary}");
    }

    if let Some(path) = output
        .or_else(|| default_output.then(|| default_lxmf_interop_report_path(&diagnostics_dir)))
    {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).with_context(|| {
                format!("failed to create output directory {}", parent.display())
            })?;
        }
        std::fs::write(&path, content.as_bytes())
            .with_context(|| format!("failed to write LXMF report {}", path.display()))?;
        if stdout {
            eprintln!("{}", path.display());
        } else {
            println!("{}", path.display());
        }
    }

    if let Some(root) = bundle_report {
        let bundle_dir = write_report_bundle(ReportBundleInput {
            root: &root,
            prefix: "native-lxmf-interop",
            command_kind: "lxmf_interop",
            report: &report,
            summary: &summary,
            overrides: &overrides,
            logs_dir: &logs_dir,
            identity_path: identity_path.as_ref(),
        })
        .context("failed to write LXMF interop bundle report")?;
        if stdout {
            eprintln!("{}", bundle_dir.display());
        } else {
            println!("{}", bundle_dir.display());
        }
    }

    Ok(())
}

async fn run_lxmf_propagation_sync_command(
    input: LxmfPropagationSyncCommandInput,
) -> anyhow::Result<()> {
    let LxmfPropagationSyncCommandInput {
        lxmf_smoke_propagation_node,
        sync_limit,
        output,
        stdout,
        suggest_shell,
        bundle_report,
        overrides,
    } = input;
    let mut config = load_config_for_smoke(overrides.app_root().cloned())
        .context("failed to load LXMF propagation sync app configuration")?;
    let selected_node = lxmf_smoke_propagation_node
        .clone()
        .or_else(|| config.settings.preferred_propagation_node_hash.clone());
    if let Some(node) = selected_node.clone() {
        config.settings.preferred_propagation_node_hash = Some(node);
    }
    let interface_override = apply_smoke_overrides(&mut config, overrides.clone());
    let default_output = output.is_none() && !stdout && bundle_report.is_none();
    let diagnostics_dir = config.paths.diagnostics_dir.clone();
    let logs_dir = config.paths.logs_dir.clone();
    let identity_path = config.settings.identity_path.clone();
    let mut app = App::try_new(config).context("failed to initialize application services")?;
    app.start_runtime_for_smoke_test_with_interfaces(interface_override)
        .await
        .context("failed to start configured runtime for LXMF propagation sync")?;
    let report = app
        .native_lxmf_propagation_diagnostics_report(selected_node, sync_limit)
        .await;
    let content = serde_json::to_string_pretty(&report)
        .context("failed to render LXMF propagation sync report JSON")?;
    let summary = render_report_summary_with_options(&report, suggest_shell);

    if stdout {
        eprintln!("{summary}");
        println!("{content}");
    } else if suggest_shell {
        eprintln!("{summary}");
    }

    if let Some(path) = output.or_else(|| {
        default_output.then(|| default_lxmf_propagation_sync_report_path(&diagnostics_dir))
    }) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).with_context(|| {
                format!("failed to create output directory {}", parent.display())
            })?;
        }
        std::fs::write(&path, content.as_bytes()).with_context(|| {
            format!(
                "failed to write LXMF propagation sync report {}",
                path.display()
            )
        })?;
        if stdout {
            eprintln!("{}", path.display());
        } else {
            println!("{}", path.display());
        }
    }

    if let Some(root) = bundle_report {
        let bundle_dir = write_report_bundle(ReportBundleInput {
            root: &root,
            prefix: "native-lxmf-propagation-sync",
            command_kind: "lxmf_propagation_sync",
            report: &report,
            summary: &summary,
            overrides: &overrides,
            logs_dir: &logs_dir,
            identity_path: identity_path.as_ref(),
        })
        .context("failed to write LXMF propagation sync bundle report")?;
        if stdout {
            eprintln!("{}", bundle_dir.display());
        } else {
            println!("{}", bundle_dir.display());
        }
    }

    Ok(())
}

fn run_generate_native_identity_command(
    input: GenerateNativeIdentityCommandInput,
) -> anyhow::Result<()> {
    let GenerateNativeIdentityCommandInput {
        label,
        output,
        stdout,
        mut overrides,
    } = input;
    let mut config = load_config_for_smoke(overrides.app_root().cloned())
        .context("failed to load native identity app configuration")?;
    if let Some(backend) = overrides.take_runtime_backend() {
        config.settings.runtime_backend = backend;
    }
    if let Some(reticulum_config_path) = overrides.take_reticulum_config_path() {
        config.settings.reticulum_config_path = Some(reticulum_config_path);
    }

    let profile = create_native_identity_profile(&config, &label)
        .context("failed to create native Reticulum identity")?;
    omenbrowser_rs::identity::IdentityManager::activate_profile(&mut config.settings, &profile);
    config
        .settings
        .save(&config.paths.settings_file)
        .context("failed to save activated native identity settings")?;

    let report = serde_json::json!({
        "report": "native_identity_created",
        "native_reticulum_compiled": cfg!(feature = "native-reticulum"),
        "identity": {
            "label": profile.label,
            "path": profile.path,
            "hash_hex": profile.hash_hex,
            "managed": profile.managed,
        },
        "settings_file": config.paths.settings_file,
        "reticulum_config_path": config
            .settings
            .reticulum_config_path
            .as_ref()
            .unwrap_or(&config.paths.reticulum_config_dir),
        "next_step": "run --native-startup --backend reticulum --tcp-client <host:port>",
    });
    let content =
        serde_json::to_string_pretty(&report).context("failed to render identity report JSON")?;

    let wrote_output = output.is_some();
    if let Some(path) = output.as_ref() {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).with_context(|| {
                format!("failed to create output directory {}", parent.display())
            })?;
        }
        std::fs::write(path, content.as_bytes())
            .with_context(|| format!("failed to write identity report {}", path.display()))?;
        if stdout {
            eprintln!("{}", path.display());
        } else {
            println!("{}", path.display());
        }
    }

    if stdout || !wrote_output {
        println!("{content}");
    }

    Ok(())
}

#[cfg(feature = "native-reticulum")]
fn create_native_identity_profile(
    config: &AppConfig,
    label: &str,
) -> anyhow::Result<omenbrowser_rs::identity::IdentityProfile> {
    let manager = omenbrowser_rs::identity::IdentityManager::new(
        config.paths.identities_dir.clone(),
        config.paths.identity_backups_dir.clone(),
    );
    let provider = omenbrowser_rs::runtime::native::identity::NativeReticulumIdentityProvider;
    manager
        .create_managed_identity_with_provider(label, &provider)
        .map_err(Into::into)
}

#[cfg(not(feature = "native-reticulum"))]
fn create_native_identity_profile(
    _config: &AppConfig,
    _label: &str,
) -> anyhow::Result<omenbrowser_rs::identity::IdentityProfile> {
    Err(anyhow::anyhow!(
        "native identity generation requires --features native-reticulum or native-network"
    ))
}

async fn run_native_preflight_command(input: NativePreflightCommandInput) -> anyhow::Result<()> {
    let NativePreflightCommandInput {
        destination,
        lxmf_peer,
        preflight_wait_ms,
        output,
        stdout,
        suggest_shell,
        bundle_report,
        overrides,
    } = input;
    let mut config = load_config_for_smoke(overrides.app_root().cloned())
        .context("failed to load native preflight app configuration")?;
    let known_destinations_path = overrides
        .known_destinations_fixture_path()
        .cloned()
        .or_else(|| overrides.known_destinations_path().cloned());
    if let Some(path) = overrides.known_destinations_fixture_path().cloned() {
        generate_known_destinations_fixture_for_smoke(&path, &destination)?;
    }
    let interface_override = apply_smoke_overrides(&mut config, overrides.clone());
    let default_output = output.is_none() && !stdout && bundle_report.is_none();
    let diagnostics_dir = config.paths.diagnostics_dir.clone();
    let logs_dir = config.paths.logs_dir.clone();
    let identity_path = config.settings.identity_path.clone();
    let mut app = App::try_new(config).context("failed to initialize application services")?;
    let transport_startup = collect_transport_startup_preflight(
        &mut app,
        interface_override.clone(),
        Duration::from_millis(preflight_wait_ms),
    )
    .await;
    let report = native_preflight_report(
        &app,
        &destination,
        lxmf_peer.as_deref(),
        known_destinations_path.as_ref(),
        interface_override.as_ref(),
        Some(transport_startup),
    );
    let content =
        serde_json::to_string_pretty(&report).context("failed to render preflight report JSON")?;
    let summary = render_report_summary_with_options(&report, suggest_shell);

    if stdout {
        eprintln!("{summary}");
        println!("{content}");
    } else if suggest_shell {
        eprintln!("{summary}");
    }

    if let Some(path) =
        output.or_else(|| default_output.then(|| default_preflight_report_path(&diagnostics_dir)))
    {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).with_context(|| {
                format!("failed to create output directory {}", parent.display())
            })?;
        }
        std::fs::write(&path, content.as_bytes())
            .with_context(|| format!("failed to write preflight report {}", path.display()))?;
        if stdout {
            eprintln!("{}", path.display());
        } else {
            println!("{}", path.display());
        }
    }

    if let Some(root) = bundle_report {
        let bundle_dir = write_report_bundle(ReportBundleInput {
            root: &root,
            prefix: "native-network-preflight",
            command_kind: "native_preflight",
            report: &report,
            summary: &summary,
            overrides: &overrides,
            logs_dir: &logs_dir,
            identity_path: identity_path.as_ref(),
        })
        .context("failed to write native preflight bundle report")?;
        if stdout {
            eprintln!("{}", bundle_dir.display());
        } else {
            println!("{}", bundle_dir.display());
        }
    }

    Ok(())
}

async fn run_native_startup_command(input: NativeStartupCommandInput) -> anyhow::Result<()> {
    let NativeStartupCommandInput {
        output,
        stdout,
        suggest_shell,
        bundle_report,
        overrides,
    } = input;
    let mut config = load_config_for_smoke(overrides.app_root().cloned())
        .context("failed to load native startup app configuration")?;
    let interface_override = apply_smoke_overrides(&mut config, overrides.clone());
    let default_output = output.is_none() && !stdout && bundle_report.is_none();
    let diagnostics_dir = config.paths.diagnostics_dir.clone();
    let logs_dir = config.paths.logs_dir.clone();
    let identity_path = config.settings.identity_path.clone();
    let mut app = App::try_new(config).context("failed to initialize application services")?;
    let report = native_startup_report(&mut app, interface_override).await;
    let content =
        serde_json::to_string_pretty(&report).context("failed to render startup report JSON")?;
    let summary = render_report_summary_with_options(&report, suggest_shell);

    if stdout {
        eprintln!("{summary}");
        println!("{content}");
    } else if suggest_shell {
        eprintln!("{summary}");
    }

    if let Some(path) =
        output.or_else(|| default_output.then(|| default_startup_report_path(&diagnostics_dir)))
    {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).with_context(|| {
                format!("failed to create output directory {}", parent.display())
            })?;
        }
        std::fs::write(&path, content.as_bytes())
            .with_context(|| format!("failed to write startup report {}", path.display()))?;
        if stdout {
            eprintln!("{}", path.display());
        } else {
            println!("{}", path.display());
        }
    }

    if let Some(root) = bundle_report {
        let bundle_dir = write_report_bundle(ReportBundleInput {
            root: &root,
            prefix: "native-runtime-startup",
            command_kind: "native_startup",
            report: &report,
            summary: &summary,
            overrides: &overrides,
            logs_dir: &logs_dir,
            identity_path: identity_path.as_ref(),
        })
        .context("failed to write native startup bundle report")?;
        if stdout {
            eprintln!("{}", bundle_dir.display());
        } else {
            println!("{}", bundle_dir.display());
        }
    }

    Ok(())
}

async fn run_native_live_sequence_command(
    input: NativeLiveSequenceCommandInput,
) -> anyhow::Result<()> {
    let NativeLiveSequenceCommandInput {
        destination,
        lxmf_smoke_peer,
        lxmf_smoke_delivery_mode,
        lxmf_smoke_propagation_node,
        lxmf_include_ticket,
        lxmf_interop_wait_secs,
        warmup,
        preflight_wait_ms,
        output,
        stdout,
        suggest_shell,
        bundle_report,
        overrides,
    } = input;
    let mut config = load_config_for_smoke(overrides.app_root().cloned())
        .context("failed to load native live sequence app configuration")?;
    let mut known_destinations_path = overrides
        .known_destinations_fixture_path()
        .cloned()
        .or_else(|| overrides.known_destinations_path().cloned());
    if let Some(path) = overrides.known_destinations_fixture_path().cloned() {
        generate_known_destinations_fixture_for_smoke(&path, &destination)?;
        if known_destinations_path.is_none() {
            known_destinations_path = Some(path);
        }
    }
    let interface_override = apply_smoke_overrides(&mut config, overrides.clone());
    let default_output = output.is_none() && !stdout && bundle_report.is_none();
    let diagnostics_dir = config.paths.diagnostics_dir.clone();
    let logs_dir = config.paths.logs_dir.clone();
    let identity_path = config.settings.identity_path.clone();

    let mut startup_app =
        App::try_new(config.clone()).context("failed to initialize startup app services")?;
    let startup = native_startup_report(&mut startup_app, interface_override.clone()).await;

    let mut preflight_app =
        App::try_new(config.clone()).context("failed to initialize preflight app services")?;
    let transport_startup = collect_transport_startup_preflight(
        &mut preflight_app,
        interface_override.clone(),
        Duration::from_millis(preflight_wait_ms),
    )
    .await;
    let preflight = native_preflight_report(
        &preflight_app,
        &destination,
        lxmf_smoke_peer.as_deref(),
        known_destinations_path.as_ref(),
        interface_override.as_ref(),
        Some(transport_startup),
    );

    let mut smoke_app =
        App::try_new(config).context("failed to initialize live validation app services")?;
    let smoke = match smoke_app
        .start_runtime_for_smoke_test_with_interfaces(interface_override)
        .await
    {
        Ok(()) => {
            let mut preload_error = None;
            let preload_for_report = match known_destinations_path.clone() {
                Some(path) => match smoke_app
                    .preload_known_destinations_for_smoke_test(&path)
                    .await
                {
                    Ok(value) => Some(value),
                    Err(error) => {
                        preload_error = Some(format!(
                            "failed to preload known destinations from {}: {error}",
                            path.display()
                        ));
                        None
                    }
                },
                None => None,
            };
            let mut report = smoke_app
                .native_network_smoke_test_report_for_url_with_fetch_options(
                    destination.clone(),
                    true,
                    true,
                    Some(warmup),
                    preload_for_report,
                )
                .await
                .context("failed to collect native live validation smoke report")?;
            if let Some(error) = preload_error {
                if let Some(object) = report.as_object_mut() {
                    object.insert(
                        "known_destinations_preload_error".into(),
                        serde_json::json!({
                            "ok": false,
                            "error": error,
                        }),
                    );
                }
            }
            if let Some(peer_hash) = lxmf_smoke_peer.clone() {
                let lxmf_report = smoke_app
                    .native_lxmf_smoke_send_report_for_peer(
                        peer_hash.clone(),
                        lxmf_smoke_delivery_mode.clone(),
                        lxmf_smoke_propagation_node.clone(),
                        lxmf_include_ticket,
                    )
                    .await
                    .context("failed to collect native LXMF smoke-send report")?;
                if let Some(object) = report.as_object_mut() {
                    object.insert("explicit_lxmf_smoke_send".into(), lxmf_report);
                }
                if let Some(wait_secs) = lxmf_interop_wait_secs {
                    let interop_report = smoke_app
                        .native_lxmf_live_interop_report(
                            Some(peer_hash),
                            wait_secs,
                            lxmf_smoke_delivery_mode.clone(),
                            lxmf_smoke_propagation_node.clone(),
                            lxmf_include_ticket,
                        )
                        .await
                        .context("failed to collect native LXMF live interop report")?;
                    if let Some(object) = report.as_object_mut() {
                        object.insert("lxmf_live_interop".into(), interop_report);
                    }
                }
            } else if let Some(wait_secs) = lxmf_interop_wait_secs {
                let interop_report = smoke_app
                    .native_lxmf_live_interop_report(
                        None,
                        wait_secs,
                        lxmf_smoke_delivery_mode.clone(),
                        lxmf_smoke_propagation_node.clone(),
                        lxmf_include_ticket,
                    )
                    .await
                    .context("failed to collect native LXMF live interop report")?;
                if let Some(object) = report.as_object_mut() {
                    object.insert("lxmf_live_interop".into(), interop_report);
                }
            }
            add_native_smoke_suggested_commands(
                &mut report,
                &destination,
                known_destinations_path.as_ref(),
            );
            report
        }
        Err(error) => serde_json::json!({
            "report": "native_network_smoke_test",
            "classification": {
                "outcome": "blocked",
                "stage": "start_runtime",
                "reason": error.to_string(),
                "next_step": "fix startup/preflight failures before live validation",
            },
        }),
    };
    let _ = smoke_app.runtime.stop_runtime().await;

    let report = native_live_sequence_report(startup, preflight, smoke);
    let content = serde_json::to_string_pretty(&report)
        .context("failed to render native live sequence report JSON")?;
    let summary = render_report_summary_with_options(&report, suggest_shell);

    if stdout {
        eprintln!("{summary}");
        println!("{content}");
    } else if suggest_shell {
        eprintln!("{summary}");
    }

    if let Some(path) = output
        .or_else(|| default_output.then(|| default_live_sequence_report_path(&diagnostics_dir)))
    {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).with_context(|| {
                format!("failed to create output directory {}", parent.display())
            })?;
        }
        std::fs::write(&path, content.as_bytes())
            .with_context(|| format!("failed to write live sequence report {}", path.display()))?;
        if stdout {
            eprintln!("{}", path.display());
        } else {
            println!("{}", path.display());
        }
    }

    if let Some(root) = bundle_report {
        let bundle_dir = write_report_bundle(ReportBundleInput {
            root: &root,
            prefix: "native-live-sequence",
            command_kind: "native_live_sequence",
            report: &report,
            summary: &summary,
            overrides: &overrides,
            logs_dir: &logs_dir,
            identity_path: identity_path.as_ref(),
        })
        .context("failed to write native live sequence bundle report")?;
        if stdout {
            eprintln!("{}", bundle_dir.display());
        } else {
            println!("{}", bundle_dir.display());
        }
    }

    Ok(())
}

fn native_live_sequence_report(
    startup: serde_json::Value,
    preflight: serde_json::Value,
    validation: serde_json::Value,
) -> serde_json::Value {
    let classification = native_live_sequence_classification(&startup, &preflight, &validation);
    let suggested_commands = native_live_sequence_suggested_commands(&preflight, &validation);
    let failure_focus =
        native_live_sequence_failure_focus(&startup, &preflight, &validation, &classification);
    serde_json::json!({
        "schema_version": "omenbrowser_rs.native_live_sequence.v1",
        "report": "native_live_sequence",
        "classification": classification,
        "failure_focus": failure_focus,
        "suggested_commands": suggested_commands,
        "startup": startup,
        "preflight": preflight,
        "validation": validation,
    })
}

fn native_live_sequence_classification(
    startup: &serde_json::Value,
    preflight: &serde_json::Value,
    validation: &serde_json::Value,
) -> serde_json::Value {
    for (section, report) in [
        ("startup", startup),
        ("preflight", preflight),
        ("validation", validation),
    ] {
        let Some(classification) = report.get("classification") else {
            continue;
        };
        let outcome = classification
            .get("outcome")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("unknown");
        if outcome != "pass" {
            return serde_json::json!({
                "outcome": outcome,
                "stage": format!(
                    "{section}:{}",
                    classification
                        .get("stage")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or("unknown")
                ),
                "reason": classification
                    .get("reason")
                    .or_else(|| classification.get("detail"))
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("inspect nested report"),
                "next_step": classification
                    .get("next_step")
                    .or_else(|| classification.get("next_action"))
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("inspect nested report"),
            });
        }
    }
    serde_json::json!({
        "outcome": "pass",
        "stage": "validation",
        "reason": "startup, preflight, and live validation passed",
        "next_step": "run the TUI with the same identity/interface settings and browse the destination",
    })
}

fn native_live_sequence_failure_focus(
    startup: &serde_json::Value,
    preflight: &serde_json::Value,
    validation: &serde_json::Value,
    classification: &serde_json::Value,
) -> serde_json::Value {
    if classification
        .get("outcome")
        .and_then(serde_json::Value::as_str)
        == Some("pass")
    {
        return serde_json::json!({
            "status": "none",
            "detail": "sequence passed",
        });
    }

    let stage = classification
        .get("stage")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("unknown");
    let (section, nested_stage) = stage.split_once(':').unwrap_or(("unknown", stage));

    match section {
        "startup" => native_live_sequence_startup_focus(startup, nested_stage, classification),
        "preflight" => {
            native_live_sequence_preflight_focus(preflight, nested_stage, classification)
        }
        "validation" => {
            native_live_sequence_validation_focus(validation, nested_stage, classification)
        }
        _ => serde_json::json!({
            "status": "unknown",
            "section": section,
            "stage": nested_stage,
            "classification": classification,
        }),
    }
}

fn native_live_sequence_startup_focus(
    startup: &serde_json::Value,
    stage: &str,
    classification: &serde_json::Value,
) -> serde_json::Value {
    serde_json::json!({
        "status": "focused",
        "section": "startup",
        "stage": stage,
        "classification": classification,
        "start": startup.get("start").cloned().unwrap_or(serde_json::Value::Null),
        "runtime_status_after": startup
            .get("runtime_status_after")
            .cloned()
            .unwrap_or(serde_json::Value::Null),
        "interface_stats": startup
            .get("interface_stats")
            .cloned()
            .unwrap_or(serde_json::Value::Null),
        "stop": startup.get("stop").cloned().unwrap_or(serde_json::Value::Null),
    })
}

fn native_live_sequence_preflight_focus(
    preflight: &serde_json::Value,
    stage: &str,
    classification: &serde_json::Value,
) -> serde_json::Value {
    let stage_report = preflight
        .get("stages")
        .and_then(serde_json::Value::as_array)
        .and_then(|stages| {
            stages.iter().find(|candidate| {
                candidate.get("stage").and_then(serde_json::Value::as_str) == Some(stage)
            })
        })
        .cloned()
        .unwrap_or(serde_json::Value::Null);

    serde_json::json!({
        "status": "focused",
        "section": "preflight",
        "stage": stage,
        "classification": classification,
        "stage_report": stage_report,
    })
}

fn native_live_sequence_validation_focus(
    validation: &serde_json::Value,
    stage: &str,
    classification: &serde_json::Value,
) -> serde_json::Value {
    let verdict = validation
        .get("verdicts")
        .and_then(|verdicts| verdicts.get(stage))
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    let live_stage = validation
        .get("live_stage_subreport")
        .and_then(|subreport| subreport.get("stages"))
        .and_then(serde_json::Value::as_array)
        .and_then(|stages| {
            stages.iter().find(|candidate| {
                candidate.get("stage").and_then(serde_json::Value::as_str) == Some(stage)
            })
        })
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    let dry_step = probe_step_for_stage(validation.get("dry_run_page_probe"), stage);
    let live_step = probe_step_for_stage(validation.get("live_page_probe"), stage);

    serde_json::json!({
        "status": "focused",
        "section": "validation",
        "stage": stage,
        "classification": classification,
        "verdict": verdict,
        "dry_run_probe_step": dry_step,
        "live_probe_step": live_step,
        "live_stage": live_stage,
        "live_fetch": validation.get("live_fetch").cloned().unwrap_or(serde_json::Value::Null),
        "live_fetch_readiness_retry": validation
            .get("live_fetch_readiness_retry")
            .cloned()
            .unwrap_or(serde_json::Value::Null),
        "path_warmup": validation.get("path_warmup").cloned().unwrap_or(serde_json::Value::Null),
        "known_destinations_preload": validation
            .get("known_destinations_preload")
            .cloned()
            .unwrap_or(serde_json::Value::Null),
        "known_destinations_preload_error": validation
            .get("known_destinations_preload_error")
            .cloned()
            .unwrap_or(serde_json::Value::Null),
    })
}

fn probe_step_for_stage(probe: Option<&serde_json::Value>, stage: &str) -> serde_json::Value {
    probe
        .and_then(|probe| probe.get("steps"))
        .and_then(serde_json::Value::as_array)
        .and_then(|steps| {
            steps
                .iter()
                .find(|step| step.get("stage").and_then(serde_json::Value::as_str) == Some(stage))
        })
        .cloned()
        .unwrap_or(serde_json::Value::Null)
}

fn native_live_sequence_suggested_commands(
    preflight: &serde_json::Value,
    validation: &serde_json::Value,
) -> Vec<serde_json::Value> {
    let mut suggestions = Vec::new();
    append_report_suggested_commands(&mut suggestions, preflight);
    append_report_suggested_commands(&mut suggestions, validation);
    dedupe_suggested_commands(suggestions)
}

fn append_report_suggested_commands(
    suggestions: &mut Vec<serde_json::Value>,
    report: &serde_json::Value,
) {
    let Some(commands) = report
        .get("suggested_commands")
        .and_then(serde_json::Value::as_array)
    else {
        return;
    };
    suggestions.extend(commands.iter().cloned());
}

fn dedupe_suggested_commands(commands: Vec<serde_json::Value>) -> Vec<serde_json::Value> {
    let mut seen = std::collections::BTreeSet::new();
    let mut output = Vec::new();
    for command in commands {
        let key = command
            .get("argv")
            .and_then(serde_json::Value::as_array)
            .map(|argv| {
                argv.iter()
                    .filter_map(serde_json::Value::as_str)
                    .collect::<Vec<_>>()
                    .join("\0")
            })
            .unwrap_or_else(|| command.to_string());
        if seen.insert(key) {
            output.push(command);
        }
    }
    output
}

fn load_config_for_smoke(app_root: Option<PathBuf>) -> anyhow::Result<AppConfig> {
    if let Some(root) = app_root {
        let paths = AppPaths::from_root(root);
        paths.ensure()?;
        let settings =
            omenbrowser_rs::storage::settings::AppSettings::load_or_default(&paths.settings_file)?;
        Ok(AppConfig { paths, settings })
    } else {
        AppConfig::load().map_err(Into::into)
    }
}

fn apply_smoke_overrides(
    config: &mut AppConfig,
    mut overrides: SmokeOverrides,
) -> Option<Vec<ReticulumInterfaceProfile>> {
    if let Some(backend) = overrides.take_runtime_backend() {
        config.settings.runtime_backend = backend;
    }
    if let Some(identity_path) = overrides.take_identity_path() {
        config.settings.identity_path = Some(identity_path);
    }
    if let Some(reticulum_config_path) = overrides.take_reticulum_config_path() {
        config.settings.reticulum_config_path = Some(reticulum_config_path);
    }
    overrides.take_tcp_client().map(|tcp| {
        let (host, port, network_name, passphrase) = tcp.into_parts();
        let mut profile = ReticulumInterfaceProfile::tcp_client("cli-tcp-client", "CLI TCP Client");
        profile.target_host = host;
        profile.target_port = port;
        if let Some(network_name) = network_name {
            profile.network_name = network_name;
        }
        if let Some(passphrase) = passphrase {
            profile.passphrase = passphrase;
        }
        profile.enabled = true;
        vec![profile]
    })
}

fn generate_known_destinations_fixture_for_smoke(
    path: &std::path::Path,
    destination_url: &str,
) -> anyhow::Result<()> {
    let address = BrowserAddress::parse(destination_url)
        .ok_or_else(|| anyhow::anyhow!("fixture generation requires destination:path input"))?;
    let destination = parse_16_byte_hex_hash(&address.destination)?;
    write_known_destinations_fixture(path, destination)
}

#[cfg(all(feature = "native-rns-net", any()))]
fn write_known_destinations_fixture(
    path: &std::path::Path,
    destination: [u8; 16],
) -> anyhow::Result<()> {
    omenbrowser_rs::runtime::native::rns_net::write_known_destinations_fixture(path, destination)
        .map_err(omenbrowser_rs::error::AppError::from)
        .map_err(Into::into)
}

#[cfg(not(all(feature = "native-rns-net", any())))]
fn write_known_destinations_fixture(
    path: &std::path::Path,
    destination: [u8; 16],
) -> anyhow::Result<()> {
    let _ = (path, destination);
    Err(anyhow::anyhow!(
        "known_destinations fixture generation is not available in the clean Reticulum 0.9 build"
    ))
}

fn parse_16_byte_hex_hash(value: &str) -> anyhow::Result<[u8; 16]> {
    if value.len() != 32 {
        return Err(anyhow::anyhow!(
            "destination hash must be 32 hexadecimal characters"
        ));
    }
    let mut output = [0u8; 16];
    for (index, chunk) in value.as_bytes().chunks_exact(2).enumerate() {
        let text = std::str::from_utf8(chunk)?;
        output[index] = u8::from_str_radix(text, 16)
            .with_context(|| format!("invalid destination hash hex byte {text}"))?;
    }
    Ok(output)
}

fn default_smoke_report_path(diagnostics_dir: &std::path::Path) -> PathBuf {
    let epoch = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default();
    diagnostics_dir.join(format!("native-network-smoke-{epoch}.json"))
}

fn default_lxmf_interop_report_path(diagnostics_dir: &std::path::Path) -> PathBuf {
    let epoch = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default();
    diagnostics_dir.join(format!("native-lxmf-interop-{epoch}.json"))
}

fn default_lxmf_propagation_sync_report_path(diagnostics_dir: &std::path::Path) -> PathBuf {
    let epoch = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default();
    diagnostics_dir.join(format!("native-lxmf-propagation-sync-{epoch}.json"))
}

fn default_preflight_report_path(diagnostics_dir: &std::path::Path) -> PathBuf {
    let epoch = current_epoch_millis();
    diagnostics_dir.join(format!("native-network-preflight-{epoch}.json"))
}

fn default_startup_report_path(diagnostics_dir: &std::path::Path) -> PathBuf {
    let epoch = current_epoch_millis();
    diagnostics_dir.join(format!("native-runtime-startup-{epoch}.json"))
}

fn default_live_sequence_report_path(diagnostics_dir: &std::path::Path) -> PathBuf {
    let epoch = current_epoch_millis();
    diagnostics_dir.join(format!("native-live-sequence-{epoch}.json"))
}

async fn native_startup_report(
    app: &mut App,
    interface_override: Option<Vec<ReticulumInterfaceProfile>>,
) -> serde_json::Value {
    let status_before = app.runtime.status().await;
    let start_result = app
        .start_runtime_for_smoke_test_with_interfaces(interface_override)
        .await;
    let status_after = app.runtime.status().await;
    let interface_stats = report_result(app.runtime.interface_stats().await);
    let network_snapshot = report_result(app.runtime.network_snapshot().await);
    let stop_result = app.runtime.stop_runtime().await;
    let stopped_status = app.runtime.status().await;
    let mut report = serde_json::json!({
        "report": "native_runtime_startup",
        "runtime_status_before": status_before,
        "start": match start_result {
            Ok(()) => serde_json::json!({"ok": true}),
            Err(error) => serde_json::json!({"ok": false, "error": error.to_string()}),
        },
        "runtime_status_after": status_after,
        "interface_stats": interface_stats,
        "network_snapshot": network_snapshot,
        "stop": match stop_result {
            Ok(()) => serde_json::json!({"ok": true}),
            Err(error) => serde_json::json!({"ok": false, "error": error.to_string()}),
        },
        "runtime_status_stopped": stopped_status,
    });
    let classification = native_startup_classification(&report);
    if let Some(object) = report.as_object_mut() {
        object.insert("classification".into(), classification);
    }
    report
}

fn report_result<T: serde::Serialize>(
    result: Result<T, omenbrowser_rs::error::AppError>,
) -> serde_json::Value {
    match result {
        Ok(value) => serde_json::json!({"ok": true, "value": value}),
        Err(error) => serde_json::json!({"ok": false, "error": error.to_string()}),
    }
}

fn native_startup_classification(report: &serde_json::Value) -> serde_json::Value {
    let start_ok = report
        .get("start")
        .and_then(|value| value.get("ok"))
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    if !start_ok {
        let reason = report
            .get("start")
            .and_then(|value| value.get("error"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or("native runtime startup failed");
        return serde_json::json!({
            "outcome": "blocked",
            "stage": "start_runtime",
            "reason": reason,
            "next_step": "fix identity/interface/backend settings, then rerun --native-startup",
        });
    }
    let connected = report
        .get("runtime_status_after")
        .and_then(|value| value.get("connected"))
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    if !connected {
        return serde_json::json!({
            "outcome": "blocked",
            "stage": "runtime_status",
            "reason": "runtime start returned ok but status is not connected",
            "next_step": "inspect runtime_status_after and interface_stats",
        });
    }
    let stats_ok = report
        .get("interface_stats")
        .and_then(|value| value.get("ok"))
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    if !stats_ok {
        return serde_json::json!({
            "outcome": "partial",
            "stage": "interface_stats",
            "reason": "runtime started but interface stats failed",
            "next_step": "inspect interface_stats.error before attempting live fetch",
        });
    }
    serde_json::json!({
        "outcome": "pass",
        "stage": "runtime_status",
        "reason": "native runtime started and status/interface data was collected",
        "next_step": "run --native-preflight, then --native-smoke --live --fetch-page with a real destination",
    })
}

fn native_preflight_report(
    app: &App,
    destination: &str,
    lxmf_peer: Option<&str>,
    known_destinations_path: Option<&PathBuf>,
    interface_override: Option<&Vec<ReticulumInterfaceProfile>>,
    transport_startup: Option<serde_json::Value>,
) -> serde_json::Value {
    let native_readiness = app.native_reticulum_readiness();
    let mut stages = Vec::new();

    stages.push(preflight_stage(
        "backend",
        true,
        matches!(
            app.settings.runtime_backend,
            RuntimeBackendSetting::Reticulum | RuntimeBackendSetting::Auto
        ),
        format!("{:?}", app.settings.runtime_backend),
        "use --backend reticulum for live native Reticulum validation",
    ));

    stages.push(preflight_stage(
        "native_reticulum_readiness",
        native_readiness.ready,
        native_readiness.configured,
        &native_readiness.summary,
        "fix native readiness issues before running NomadNet fetch or LXMF delivery",
    ));

    stages.push(preflight_path_stage(
        "identity_path",
        app.settings.identity_path.as_ref(),
        true,
        "attach or create an identity before live native networking",
    ));

    let reticulum_config_path = app
        .settings
        .reticulum_config_path
        .as_ref()
        .unwrap_or(&app.paths.reticulum_config_dir);
    stages.push(preflight_path_stage(
        "reticulum_config_path",
        Some(reticulum_config_path),
        false,
        "provide an existing Reticulum config directory or allow the managed config path",
    ));

    let parsed_address = BrowserAddress::parse(destination);
    stages.push(preflight_stage(
        "nomadnet_address",
        parsed_address.is_some(),
        false,
        parsed_address
            .as_ref()
            .map(|address| format!("destination={} path={}", address.destination, address.path))
            .unwrap_or_else(|| "destination could not be parsed".into()),
        "use a destination:path address such as 00112233445566778899aabbccddeeff:/",
    ));

    let destination_hash_valid = parsed_address
        .as_ref()
        .is_some_and(|address| parse_16_byte_hex_hash(&address.destination).is_ok());
    let parsed_destination_hash = parsed_address
        .as_ref()
        .and_then(|address| parse_16_byte_hex_hash(&address.destination).ok());
    stages.push(preflight_stage(
        "nomadnet_destination_hash",
        destination_hash_valid,
        false,
        if destination_hash_valid {
            String::from("32 hex character destination hash")
        } else {
            String::from("destination hash must be exactly 32 hexadecimal characters")
        },
        "copy the 16-byte NomadNet destination hash, not a full Reticulum address blob",
    ));

    stages.push(preflight_known_destinations_stage(
        known_destinations_path,
        parsed_destination_hash,
    ));

    if let Some(peer) = lxmf_peer {
        stages.push(preflight_stage(
            "lxmf_peer_hash",
            parse_16_byte_hex_hash(peer).is_ok(),
            false,
            if parse_16_byte_hex_hash(peer).is_ok() {
                String::from("32 hex character LXMF delivery hash")
            } else {
                String::from("LXMF peer hash must be exactly 32 hexadecimal characters")
            },
            "copy the peer lxmf.delivery destination hash before send/wait tests",
        ));
    }

    if let Some(profiles) = interface_override {
        stages.push(preflight_stage(
            "cli_interface_override",
            !profiles.is_empty(),
            false,
            format!("{} CLI interface override(s)", profiles.len()),
            "provide --tcp-client host:port for direct TCP peer testing",
        ));
    }

    if let Some(transport_startup) = transport_startup {
        stages.push(transport_startup);
    }

    let suggestions =
        preflight_suggested_commands(destination, lxmf_peer, known_destinations_path, &stages);
    let classification = classify_preflight(&stages);
    serde_json::json!({
        "schema_version": "omenbrowser_rs.native_preflight.v1",
        "report": "native_network_preflight",
        "destination": destination,
        "lxmf_peer": lxmf_peer,
        "classification": classification,
        "suggested_commands": suggestions,
        "native_reticulum_readiness": {
            "compiled": native_readiness.compiled,
            "configured": native_readiness.configured,
            "ready": native_readiness.ready,
            "summary": native_readiness.summary,
            "issues": native_readiness.issues,
        },
        "interface_readiness": app.native_interface_readiness().into_iter().map(|detail| serde_json::json!({
            "profile_id": detail.profile_id,
            "name": detail.name,
            "kind": detail.kind,
            "enabled": detail.enabled,
            "supported": detail.supported,
            "blocks_native_startup": detail.blocks_native_startup,
            "reason": detail.reason,
            "warnings": detail.warnings,
        })).collect::<Vec<_>>(),
        "stages": stages,
    })
}

fn preflight_suggested_commands(
    destination: &str,
    lxmf_peer: Option<&str>,
    known_destinations_path: Option<&PathBuf>,
    stages: &[serde_json::Value],
) -> Vec<serde_json::Value> {
    let mut suggestions = Vec::new();
    for stage in stages {
        let outcome = stage
            .get("outcome")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("unknown");
        if outcome == "pass" {
            continue;
        }
        let stage_name = stage
            .get("stage")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("unknown");
        if let Some(command) =
            suggested_command_for_stage(stage_name, destination, lxmf_peer, known_destinations_path)
        {
            suggestions.push(command);
        }
    }
    if suggestions.is_empty() {
        suggestions.push(suggested_native_smoke_command(
            "live_fetch",
            destination,
            known_destinations_path,
            true,
        ));
        if let Some(peer) = lxmf_peer {
            suggestions.push(suggested_lxmf_interop_command(peer));
        }
    }
    suggestions
}

fn suggested_command_for_stage(
    stage: &str,
    destination: &str,
    lxmf_peer: Option<&str>,
    known_destinations_path: Option<&PathBuf>,
) -> Option<serde_json::Value> {
    match stage {
        "backend"
        | "native_reticulum_readiness"
        | "identity_path"
        | "reticulum_config_path"
        | "cli_interface_override"
        | "transport_startup" => Some(suggested_preflight_command(
            destination,
            lxmf_peer,
            known_destinations_path,
        )),
        "known_destinations" => Some(suggested_native_smoke_command(
            "preload_known_destinations_and_dry_probe",
            destination,
            known_destinations_path,
            false,
        )),
        "nomadnet_address" | "nomadnet_destination_hash" => Some(serde_json::json!({
            "purpose": "fix_nomadnet_destination",
            "argv": [
                "cargo", "run", "--features", "native-network", "--",
                "--native-preflight", "<32-hex-destination>:/",
                "--backend", "reticulum",
                "--identity", "<identity-file>",
                "--tcp-client", "<host:port>",
                "--stdout",
            ],
        })),
        "lxmf_peer_hash" => Some(serde_json::json!({
            "purpose": "fix_lxmf_peer",
            "argv": [
                "cargo", "run", "--features", "native-network", "--",
                "--native-preflight", destination,
                "--send-lxmf-smoke", "<32-hex-lxmf-peer-destination>",
                "--backend", "reticulum",
                "--identity", "<identity-file>",
                "--tcp-client", "<host:port>",
                "--stdout",
            ],
        })),
        _ => None,
    }
}

fn suggested_preflight_command(
    destination: &str,
    lxmf_peer: Option<&str>,
    known_destinations_path: Option<&PathBuf>,
) -> serde_json::Value {
    let mut argv = vec![
        "cargo".to_string(),
        "run".to_string(),
        "--features".to_string(),
        "native-network".to_string(),
        "--".to_string(),
        "--native-preflight".to_string(),
        destination.to_string(),
        "--backend".to_string(),
        "reticulum".to_string(),
        "--identity".to_string(),
        "<identity-file>".to_string(),
        "--tcp-client".to_string(),
        "<host:port>".to_string(),
    ];
    if let Some(path) = known_destinations_path {
        argv.extend([
            "--known-destinations".to_string(),
            redacted_path_placeholder(path),
        ]);
    }
    if let Some(peer) = lxmf_peer {
        argv.extend(["--send-lxmf-smoke".to_string(), peer.to_string()]);
    }
    argv.push("--stdout".to_string());
    serde_json::json!({
        "purpose": "rerun_native_preflight",
        "argv": argv,
    })
}

fn suggested_native_smoke_command(
    purpose: &str,
    destination: &str,
    known_destinations_path: Option<&PathBuf>,
    live_fetch: bool,
) -> serde_json::Value {
    let mut argv = vec![
        "cargo".to_string(),
        "run".to_string(),
        "--features".to_string(),
        "native-network".to_string(),
        "--".to_string(),
        "--native-smoke".to_string(),
        destination.to_string(),
        "--backend".to_string(),
        "reticulum".to_string(),
        "--identity".to_string(),
        "<identity-file>".to_string(),
        "--tcp-client".to_string(),
        "<host:port>".to_string(),
    ];
    if let Some(path) = known_destinations_path {
        argv.extend([
            "--known-destinations".to_string(),
            redacted_path_placeholder(path),
        ]);
    }
    argv.extend([
        "--warm-path".to_string(),
        "--path-wait".to_string(),
        "10".to_string(),
    ]);
    if live_fetch {
        argv.extend(["--live".to_string(), "--fetch-page".to_string()]);
    }
    argv.push("--stdout".to_string());
    serde_json::json!({
        "purpose": purpose,
        "argv": argv,
    })
}

fn suggested_lxmf_interop_command(peer: &str) -> serde_json::Value {
    serde_json::json!({
        "purpose": "lxmf_interop_send_and_wait",
        "argv": [
            "cargo", "run", "--features", "native-network", "--",
            "--lxmf-interop",
            "--send-lxmf-smoke", peer,
            "--lxmf-wait", "10",
            "--backend", "reticulum",
            "--identity", "<identity-file>",
            "--tcp-client", "<host:port>",
            "--stdout",
        ],
    })
}

fn redacted_path_placeholder(path: &std::path::Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(|name| format!("<path:{name}>"))
        .unwrap_or_else(|| "<path>".into())
}

fn add_native_smoke_suggested_commands(
    report: &mut serde_json::Value,
    destination: &str,
    known_destinations_path: Option<&PathBuf>,
) {
    let suggestions = native_smoke_suggested_commands(report, destination, known_destinations_path);
    if let Some(object) = report.as_object_mut() {
        object.insert(
            "suggested_commands".into(),
            serde_json::Value::Array(suggestions),
        );
    }
}

fn native_smoke_suggested_commands(
    report: &serde_json::Value,
    destination: &str,
    known_destinations_path: Option<&PathBuf>,
) -> Vec<serde_json::Value> {
    let mut suggestions = Vec::new();
    let stage = report
        .get("classification")
        .and_then(|classification| classification.get("stage"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or("unknown");
    let outcome = report
        .get("classification")
        .and_then(|classification| classification.get("outcome"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or("unknown");

    match stage {
        "config" | "runtime_startup" | "address_parse" => {
            suggestions.push(suggested_preflight_command(
                destination,
                None,
                known_destinations_path,
            ));
        }
        "destination_identity" => {
            suggestions.push(suggested_preflight_command(
                destination,
                None,
                known_destinations_path,
            ));
            suggestions.push(suggested_native_smoke_command(
                "preload_known_destinations_and_dry_probe",
                destination,
                known_destinations_path,
                false,
            ));
        }
        "path_discovery" | "destination_inspection" => {
            suggestions.push(suggested_native_smoke_command(
                "warm_path_and_dry_probe",
                destination,
                known_destinations_path,
                false,
            ));
        }
        "live_fetch_preflight"
        | "link_setup"
        | "request_send"
        | "response_wait"
        | "response_decode" => {
            suggestions.push(suggested_native_smoke_command(
                "live_fetch_with_bundle",
                destination,
                known_destinations_path,
                true,
            ));
        }
        "dry_run_page_probe" | "live_probe" if outcome == "pass" => {
            suggestions.push(suggested_native_smoke_command(
                "live_fetch",
                destination,
                known_destinations_path,
                true,
            ));
        }
        "live_fetch" if outcome == "pass" => {
            suggestions.push(suggested_native_smoke_command(
                "repeat_live_fetch",
                destination,
                known_destinations_path,
                true,
            ));
        }
        _ => suggestions.push(suggested_preflight_command(
            destination,
            None,
            known_destinations_path,
        )),
    }

    if let Some(lxmf) = report.get("lxmf_live_interop") {
        suggestions.extend(lxmf_interop_suggested_commands_from_report(lxmf, 10));
    }
    suggestions
}

fn add_lxmf_interop_suggested_commands(report: &mut serde_json::Value, wait_secs: u64) {
    let suggestions = lxmf_interop_suggested_commands_from_report(report, wait_secs);
    if let Some(object) = report.as_object_mut() {
        object.insert(
            "suggested_commands".into(),
            serde_json::Value::Array(suggestions),
        );
    }
}

fn lxmf_interop_suggested_commands_from_report(
    report: &serde_json::Value,
    wait_secs: u64,
) -> Vec<serde_json::Value> {
    let peer = report
        .get("peer_hash")
        .and_then(serde_json::Value::as_str)
        .or_else(|| {
            report
                .get("send")
                .and_then(|send| send.get("peer_hash"))
                .and_then(serde_json::Value::as_str)
        });
    let outcome = report
        .get("classification")
        .and_then(|classification| classification.get("outcome"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or("unknown");
    let wait_status = report
        .get("classification")
        .and_then(|classification| classification.get("wait_status"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or("unknown");

    let mut suggestions = Vec::new();
    if let Some(peer) = peer {
        suggestions.push(suggested_lxmf_interop_command_with_wait(
            "lxmf_interop_send_and_wait",
            peer,
            wait_secs.max(10),
        ));
        if matches!(outcome, "timeout") || wait_status == "timeout" {
            suggestions.push(suggested_lxmf_interop_command_with_wait(
                "lxmf_interop_retry_longer_wait",
                peer,
                wait_secs.saturating_mul(2).max(20),
            ));
        }
    } else {
        suggestions.push(serde_json::json!({
            "purpose": "lxmf_receive_only_wait",
            "argv": [
                "cargo", "run", "--features", "native-network", "--",
                "--lxmf-interop",
                "--lxmf-wait", wait_secs.max(10).to_string(),
                "--backend", "reticulum",
                "--identity", "<identity-file>",
                "--tcp-client", "<host:port>",
                "--stdout",
            ],
        }));
        suggestions.push(serde_json::json!({
            "purpose": "lxmf_explicit_peer_send",
            "argv": [
                "cargo", "run", "--features", "native-network", "--",
                "--lxmf-interop",
                "--send-lxmf-smoke", "<32-hex-lxmf-peer-destination>",
                "--lxmf-wait", wait_secs.max(10).to_string(),
                "--backend", "reticulum",
                "--identity", "<identity-file>",
                "--tcp-client", "<host:port>",
                "--stdout",
            ],
        }));
    }
    suggestions
}

fn suggested_lxmf_interop_command_with_wait(
    purpose: &str,
    peer: &str,
    wait_secs: u64,
) -> serde_json::Value {
    serde_json::json!({
        "purpose": purpose,
        "argv": [
            "cargo", "run", "--features", "native-network", "--",
            "--lxmf-interop",
            "--send-lxmf-smoke", peer,
            "--lxmf-wait", wait_secs.to_string(),
            "--backend", "reticulum",
            "--identity", "<identity-file>",
            "--tcp-client", "<host:port>",
            "--stdout",
        ],
    })
}

async fn collect_transport_startup_preflight(
    app: &mut App,
    interface_override: Option<Vec<ReticulumInterfaceProfile>>,
    wait_for: Duration,
) -> serde_json::Value {
    let mut events = app.runtime.subscribe_events();
    let startup = app
        .start_runtime_for_smoke_test_with_interfaces(interface_override)
        .await;
    let status = app.runtime.status().await;
    let interface_stats = app.runtime.interface_stats().await;
    let observed_events = collect_preflight_runtime_events(events.as_mut(), wait_for).await;
    let shutdown = app.runtime.stop_runtime().await;

    let startup_ok = startup.is_ok();
    let shutdown_ok = shutdown.is_ok();
    let interface_available = interface_stats
        .as_ref()
        .map(|stats| stats.available)
        .unwrap_or(false);
    let backend_is_reticulum = matches!(
        status.backend,
        omenbrowser_rs::runtime::RuntimeBackendName::Reticulum
    );
    let status_connected = status.connected;
    let native_expectation_met = if backend_is_reticulum {
        status_connected && interface_available
    } else {
        startup_ok && shutdown_ok
    };
    let expectation = if backend_is_reticulum {
        "reticulum_transport"
    } else {
        "non_native_backend"
    };
    let outcome = if startup_ok && shutdown_ok && native_expectation_met {
        "pass"
    } else if startup_ok && shutdown_ok {
        "blocked"
    } else {
        "fail"
    };
    let detail = if let Err(error) = &startup {
        format!("runtime startup failed: {error}")
    } else if let Err(error) = &shutdown {
        format!("runtime shutdown failed: {error}")
    } else if backend_is_reticulum && !status_connected {
        "native Reticulum runtime started, but status did not report connected".into()
    } else if backend_is_reticulum && !interface_available {
        "native Reticulum runtime started, but interface stats did not report an available transport".into()
    } else if !backend_is_reticulum {
        "non-native backend started and stopped; use --backend reticulum for native transport validation".into()
    } else {
        "runtime started, reported available interface stats, and stopped cleanly".into()
    };
    let next_step = match outcome {
        "pass" => "continue to --native-smoke --live --fetch-page or --lxmf-interop",
        "blocked" => "inspect transport_startup.status and interface_stats before live traffic",
        _ => "fix runtime startup/shutdown failure before live traffic",
    };

    serde_json::json!({
        "stage": "transport_startup",
        "outcome": outcome,
        "detail": detail,
        "next_step": next_step,
        "startup_ok": startup_ok,
        "shutdown_ok": shutdown_ok,
        "wait_ms": wait_for.as_millis(),
        "expectation": expectation,
        "backend_is_reticulum": backend_is_reticulum,
        "status_connected": status_connected,
        "interface_available": interface_available,
        "event_subscription": events.is_some(),
        "observed_events": observed_events,
        "status": {
            "connected": status.connected,
            "backend": format!("{:?}", status.backend),
            "active_identity": status.active_identity.as_ref().map(|identity| serde_json::json!({
                "label": identity.label,
                "hash_hex": identity.hash_hex,
                "path": redacted_path_hint(&identity.path),
            })),
            "message": redact_bundle_log_message(&status.message, &SmokeOverrides::default(), None),
        },
        "interface_stats": interface_stats
            .map(|stats| serde_json::json!({
                "available": stats.available,
                "reason": stats.reason,
                "interfaces": stats.interfaces,
            }))
            .unwrap_or_else(|error| serde_json::json!({
                "available": false,
                "error": error.to_string(),
            })),
    })
}

async fn collect_preflight_runtime_events(
    receiver: Option<
        &mut tokio::sync::broadcast::Receiver<omenbrowser_rs::runtime::RuntimeBusEvent>,
    >,
    wait_for: Duration,
) -> Vec<serde_json::Value> {
    let Some(receiver) = receiver else {
        return Vec::new();
    };
    let deadline = tokio::time::Instant::now() + wait_for;
    let mut events = Vec::new();
    while events.len() < 16 {
        let now = tokio::time::Instant::now();
        if now >= deadline {
            break;
        }
        match tokio::time::timeout(deadline - now, receiver.recv()).await {
            Ok(Ok(event)) => events.push(preflight_runtime_event_summary(event)),
            Ok(Err(tokio::sync::broadcast::error::RecvError::Lagged(count))) => {
                events.push(serde_json::json!({
                    "event": "event_lagged",
                    "count": count,
                }));
            }
            Ok(Err(tokio::sync::broadcast::error::RecvError::Closed)) | Err(_) => break,
        }
    }
    events
}

fn preflight_runtime_event_summary(
    event: omenbrowser_rs::runtime::RuntimeBusEvent,
) -> serde_json::Value {
    match event {
        omenbrowser_rs::runtime::RuntimeBusEvent::StatusChanged(status) => serde_json::json!({
            "event": "status_changed",
            "connected": status.connected,
            "backend": format!("{:?}", status.backend),
            "message": status.message,
        }),
        omenbrowser_rs::runtime::RuntimeBusEvent::InterfaceStats(stats) => serde_json::json!({
            "event": "interface_stats",
            "available": stats.available,
            "reason": stats.reason,
            "interfaces": stats.interfaces,
            "samples": stats.samples,
        }),
        omenbrowser_rs::runtime::RuntimeBusEvent::Announce(payload) => serde_json::json!({
            "event": "announce",
            "destination_hash": payload.destination_hash,
            "kind": format!("{:?}", payload.kind),
            "display_name": payload.display_name,
        }),
        omenbrowser_rs::runtime::RuntimeBusEvent::PathUpdated(path) => serde_json::json!({
            "event": "path_updated",
            "destination_hash": path.destination_hash,
            "known": path.known,
            "hops": path.hops,
        }),
        omenbrowser_rs::runtime::RuntimeBusEvent::LxmfDeliveryEvidence(evidence) => {
            serde_json::json!({
                "event": "lxmf_delivery_evidence",
                "peer_hash": evidence.peer_hash,
                "message_id": evidence.message_id,
                "kind": format!("{:?}", evidence.kind),
                "detail": evidence.detail,
                "rtt": evidence.rtt,
                "observed_at": evidence.observed_at,
            })
        }
        omenbrowser_rs::runtime::RuntimeBusEvent::Debug(message) => serde_json::json!({
            "event": "debug",
            "message": message,
        }),
        omenbrowser_rs::runtime::RuntimeBusEvent::Error(message) => serde_json::json!({
            "event": "error",
            "message": message,
        }),
        other => serde_json::json!({
            "event": "other",
            "kind": format!("{other:?}"),
        }),
    }
}

fn preflight_stage(
    stage: &str,
    passed: bool,
    blocked: bool,
    detail: impl Into<String>,
    next_step: &str,
) -> serde_json::Value {
    let outcome = if passed {
        "pass"
    } else if blocked {
        "blocked"
    } else {
        "fail"
    };
    serde_json::json!({
        "stage": stage,
        "outcome": outcome,
        "detail": detail.into(),
        "next_step": if passed { "continue" } else { next_step },
    })
}

fn preflight_path_stage(
    stage: &str,
    path: Option<&PathBuf>,
    must_be_file: bool,
    next_step: &str,
) -> serde_json::Value {
    let Some(path) = path else {
        return preflight_stage(stage, false, true, "path is not configured", next_step);
    };
    let exists = if must_be_file {
        path.is_file()
    } else {
        path.is_dir()
    };
    preflight_stage(
        stage,
        exists,
        !exists,
        serde_json::json!({
            "path": redacted_path_hint(path),
            "kind": if must_be_file { "file" } else { "directory" },
            "exists": exists,
        })
        .to_string(),
        next_step,
    )
}

fn preflight_known_destinations_stage(
    path: Option<&PathBuf>,
    destination_hash: Option<[u8; 16]>,
) -> serde_json::Value {
    let Some(path) = path else {
        return preflight_stage(
            "known_destinations",
            true,
            false,
            "not provided; live announce/path discovery must supply identities",
            "continue",
        );
    };
    if !path.is_file() {
        return preflight_stage(
            "known_destinations",
            false,
            false,
            serde_json::json!({
                "path": redacted_path_hint(path),
                "exists": false,
                "semantic_check": "not_run",
            })
            .to_string(),
            "provide an existing Python/RNS-compatible known_destinations file",
        );
    }
    let semantic = semantic_known_destinations_check(path, destination_hash);
    preflight_stage(
        "known_destinations",
        semantic
            .get("ok")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false),
        false,
        serde_json::json!({
            "path": redacted_path_hint(path),
            "exists": true,
            "semantic": semantic,
        })
        .to_string(),
        "provide a loadable Python/RNS-compatible known_destinations file containing the target destination identity",
    )
}

#[cfg(all(feature = "native-rns-net", any()))]
fn semantic_known_destinations_check(
    path: &std::path::Path,
    destination_hash: Option<[u8; 16]>,
) -> serde_json::Value {
    match omenbrowser_rs::runtime::native::rns_net::RnsNetDestinationKeyStore::load_known_destinations_file(
        path,
    ) {
        Ok(store) => {
            let destination_present = destination_hash
                .as_ref()
                .map(|hash| store.signing_public_key(hash).is_some());
            serde_json::json!({
                "available": true,
                "ok": !store.is_empty() && destination_present.unwrap_or(true),
                "loaded": store.len(),
                "destination_present": destination_present,
            })
        }
        Err(error) => serde_json::json!({
            "available": true,
            "ok": false,
            "error": format!("{error:?}"),
        }),
    }
}

#[cfg(not(all(feature = "native-rns-net", any())))]
fn semantic_known_destinations_check(
    _path: &std::path::Path,
    _destination_hash: Option<[u8; 16]>,
) -> serde_json::Value {
    serde_json::json!({
        "available": false,
        "ok": true,
        "detail": "semantic known_destinations parsing is not available in the clean Reticulum 0.9 build",
    })
}

fn classify_preflight(stages: &[serde_json::Value]) -> serde_json::Value {
    if let Some(stage) = stages
        .iter()
        .find(|stage| stage.get("outcome").and_then(serde_json::Value::as_str) == Some("fail"))
    {
        return preflight_classification("fail", stage);
    }
    if let Some(stage) = stages
        .iter()
        .find(|stage| stage.get("outcome").and_then(serde_json::Value::as_str) == Some("blocked"))
    {
        return preflight_classification("blocked", stage);
    }
    serde_json::json!({
        "outcome": "pass",
        "stage": "ready",
        "reason": "all preflight stages passed",
        "next_step": "run --native-smoke with --live --fetch-page or --lxmf-interop",
    })
}

fn preflight_classification(outcome: &str, stage: &serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "outcome": outcome,
        "stage": stage.get("stage").and_then(serde_json::Value::as_str).unwrap_or("unknown"),
        "reason": stage.get("detail").and_then(serde_json::Value::as_str).unwrap_or("preflight stage failed"),
        "next_step": stage.get("next_step").and_then(serde_json::Value::as_str).unwrap_or("inspect preflight stages"),
    })
}

struct ReportBundleInput<'a> {
    root: &'a std::path::Path,
    prefix: &'a str,
    command_kind: &'a str,
    report: &'a serde_json::Value,
    summary: &'a str,
    overrides: &'a SmokeOverrides,
    logs_dir: &'a std::path::Path,
    identity_path: Option<&'a PathBuf>,
}

fn write_report_bundle(input: ReportBundleInput<'_>) -> anyhow::Result<PathBuf> {
    let ReportBundleInput {
        root,
        prefix,
        command_kind,
        report,
        summary,
        overrides,
        logs_dir,
        identity_path,
    } = input;
    let epoch = current_epoch_millis();
    let bundle_dir = root.join(format!("{prefix}-{epoch}"));
    std::fs::create_dir_all(&bundle_dir)
        .with_context(|| format!("failed to create bundle directory {}", bundle_dir.display()))?;

    let report_json =
        serde_json::to_string_pretty(report).context("failed to render bundle report JSON")?;
    std::fs::write(bundle_dir.join("report.json"), report_json.as_bytes())
        .with_context(|| "failed to write bundle report.json")?;
    std::fs::write(bundle_dir.join("summary.txt"), summary.as_bytes())
        .with_context(|| "failed to write bundle summary.txt")?;

    let bundle_manifest = serde_json::json!({
        "schema_version": "omenbrowser_rs.cli_report_bundle.v1",
        "command_kind": command_kind,
        "created_epoch_ms": epoch,
        "files": [
            "report.json",
            "summary.txt",
            "command.json",
            "environment.json",
            "logs.json",
        ],
    });
    let bundle_manifest_json = serde_json::to_string_pretty(&bundle_manifest)
        .context("failed to render bundle manifest")?;
    std::fs::write(
        bundle_dir.join("bundle.json"),
        bundle_manifest_json.as_bytes(),
    )
    .with_context(|| "failed to write bundle bundle.json")?;

    let command = serde_json::json!({
        "schema_version": "omenbrowser_rs.cli_command.v1",
        "command_kind": command_kind,
        "argv": redacted_argv(std::env::args().collect::<Vec<_>>()),
        "overrides": redacted_override_snapshot(overrides),
        "created_epoch_ms": epoch,
    });
    let command_json =
        serde_json::to_string_pretty(&command).context("failed to render command metadata")?;
    std::fs::write(bundle_dir.join("command.json"), command_json.as_bytes())
        .with_context(|| "failed to write bundle command.json")?;

    let environment = redacted_environment_snapshot();
    let environment_json = serde_json::to_string_pretty(&environment)
        .context("failed to render environment metadata")?;
    std::fs::write(
        bundle_dir.join("environment.json"),
        environment_json.as_bytes(),
    )
    .with_context(|| "failed to write bundle environment.json")?;

    let logs = redacted_recent_persisted_logs(
        logs_dir,
        overrides,
        identity_path.map(|path| path.as_path()),
    );
    let logs_json = serde_json::to_string_pretty(&serde_json::json!({
        "schema_version": "omenbrowser_rs.cli_recent_logs.v1",
        "source": "logs/omenbrowser_rs*.jsonl",
        "limit": REPORT_LOG_ENTRY_LIMIT,
        "limits": {
            "directory_entries": REPORT_LOG_DIRECTORY_ENTRY_LIMIT,
            "files": REPORT_LOG_FILE_LIMIT,
            "bytes_per_file": REPORT_LOG_FILE_BYTES,
            "total_bytes": REPORT_LOG_TOTAL_BYTES,
        },
        "collection": {
            "directory_entries_scanned": logs.directory_entries_scanned,
            "directory_scan_truncated": logs.directory_scan_truncated,
            "matching_files": logs.matching_files,
            "selected_files": logs.selected_files,
            "files_read": logs.files_read,
            "bytes_read": logs.bytes_read,
            "truncated_files": logs.truncated_files,
            "read_failures": logs.read_failures,
        },
        "entries": logs.entries,
    }))
    .context("failed to render bundle logs")?;
    std::fs::write(bundle_dir.join("logs.json"), logs_json.as_bytes())
        .with_context(|| "failed to write bundle logs.json")?;

    Ok(bundle_dir)
}

fn current_epoch_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default()
}

fn redacted_environment_snapshot() -> serde_json::Value {
    serde_json::json!({
        "schema_version": "omenbrowser_rs.cli_environment.v1",
        "app": {
            "name": env!("CARGO_PKG_NAME"),
            "version": env!("CARGO_PKG_VERSION"),
        },
        "target": {
            "os": std::env::consts::OS,
            "arch": std::env::consts::ARCH,
            "family": std::env::consts::FAMILY,
        },
        "features": {
            "native_reticulum": cfg!(feature = "native-reticulum"),
            "native_lxmf": cfg!(feature = "native-lxmf"),
            "native_network": cfg!(feature = "native-network"),
            "mock_runtime": cfg!(feature = "mock-runtime"),
        },
        "env": {
            "RUST_LOG_set": std::env::var_os("RUST_LOG").is_some(),
            "TERM": std::env::var("TERM").ok(),
        },
    })
}

fn render_report_summary(report: &serde_json::Value) -> String {
    let mut lines = Vec::new();
    match report.get("report").and_then(serde_json::Value::as_str) {
        Some("native_network_smoke_test") => {
            lines.extend(render_classification_summary(
                "NomadNet native smoke",
                report.get("classification"),
            ));
            if let Some(lxmf) = report.get("lxmf_live_interop") {
                lines.extend(render_classification_summary(
                    "LXMF live interop",
                    lxmf.get("classification"),
                ));
            }
            if let Some(lxmf) = report.get("explicit_lxmf_smoke_send") {
                lines.extend(render_lxmf_smoke_send_summary(lxmf));
            }
        }
        Some("native_lxmf_live_interop") => {
            lines.extend(render_classification_summary(
                "LXMF live interop",
                report.get("classification"),
            ));
        }
        Some("native_network_preflight") => {
            lines.extend(render_classification_summary(
                "Native network preflight",
                report.get("classification"),
            ));
        }
        Some("native_runtime_startup") => {
            lines.extend(render_classification_summary(
                "Native runtime startup",
                report.get("classification"),
            ));
        }
        Some("native_live_sequence") => {
            lines.extend(render_classification_summary(
                "Native live sequence",
                report.get("classification"),
            ));
            if let Some(startup) = report.get("startup") {
                lines.extend(render_classification_summary(
                    "Startup",
                    startup.get("classification"),
                ));
            }
            if let Some(preflight) = report.get("preflight") {
                lines.extend(render_classification_summary(
                    "Preflight",
                    preflight.get("classification"),
                ));
            }
            if let Some(validation) = report.get("validation") {
                lines.extend(render_classification_summary(
                    "Validation",
                    validation.get("classification"),
                ));
            }
        }
        Some(other) => {
            lines.push(format!("{other}: summary unavailable"));
        }
        None => lines.push("OMENbrowser_rs report: summary unavailable".into()),
    }
    lines.push("JSON follows on stdout.".into());
    lines.join("\n")
}

fn render_report_summary_with_options(report: &serde_json::Value, suggest_shell: bool) -> String {
    let mut summary = render_report_summary(report);
    if suggest_shell {
        let shell_suggestions = render_shell_suggestions(report);
        if !shell_suggestions.is_empty() {
            summary.push_str("\n\nsuggested shell commands:\n");
            summary.push_str(&shell_suggestions);
        }
    }
    summary
}

fn render_shell_suggestions(report: &serde_json::Value) -> String {
    let Some(suggestions) = report
        .get("suggested_commands")
        .and_then(serde_json::Value::as_array)
    else {
        return String::new();
    };

    suggestions
        .iter()
        .filter_map(|suggestion| {
            let argv = suggestion.get("argv")?.as_array()?;
            let command = argv
                .iter()
                .map(serde_json::Value::as_str)
                .collect::<Option<Vec<_>>>()?
                .into_iter()
                .map(shell_escape_arg)
                .collect::<Vec<_>>()
                .join(" ");
            let purpose = suggestion
                .get("purpose")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("suggested command");
            Some(format!("- {purpose}: {command}"))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn shell_escape_arg(arg: &str) -> String {
    if !arg.is_empty()
        && arg
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_./:=+@%,".contains(c))
    {
        arg.to_string()
    } else {
        format!("'{}'", arg.replace('\'', "'\\''"))
    }
}

fn render_classification_summary(
    label: &str,
    classification: Option<&serde_json::Value>,
) -> Vec<String> {
    let Some(classification) = classification else {
        return vec![format!("{label}: classification unavailable")];
    };
    let outcome = classification
        .get("outcome")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("unknown");
    let stage = classification
        .get("stage")
        .and_then(serde_json::Value::as_str)
        .or_else(|| {
            classification
                .get("wait_status")
                .and_then(serde_json::Value::as_str)
        })
        .or_else(|| {
            classification
                .get("proof_match_state")
                .and_then(serde_json::Value::as_str)
        })
        .unwrap_or("unknown");
    let reason = classification
        .get("reason")
        .or_else(|| classification.get("detail"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or("no reason provided");
    let next_step = classification
        .get("next_step")
        .or_else(|| classification.get("next_action"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or("inspect the JSON report");

    vec![
        format!("{label}: {outcome}"),
        format!("stage: {stage}"),
        format!("reason: {reason}"),
        format!("next: {next_step}"),
    ]
}

fn render_lxmf_smoke_send_summary(report: &serde_json::Value) -> Vec<String> {
    let send = report.get("send");
    let ok = send
        .and_then(|value| value.get("ok"))
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    let stage = send
        .and_then(|value| value.get("stage_hint"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or(if ok { "submitted" } else { "not_sent" });
    let detail = send
        .and_then(|value| value.get("error").or_else(|| value.get("skipped")))
        .and_then(serde_json::Value::as_str)
        .unwrap_or(if ok {
            "LXMF smoke packet submitted"
        } else {
            "LXMF smoke send did not complete"
        });
    vec![
        format!(
            "LXMF explicit smoke send: {}",
            if ok { "pass" } else { "blocked" }
        ),
        format!("stage: {stage}"),
        format!("reason: {detail}"),
        "next: run --lxmf-interop with --send-lxmf-smoke to wait for proof or inbound evidence"
            .into(),
    ]
}

fn print_help() {
    print!("{}", omenbrowser_rs::cli_help::help_text());
}

fn print_version() {
    println!("{}", omenbrowser_rs::product_identity::version_line());
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE_DESTINATION_HASH: &str = "00112233445566778899aabbccddeeff";
    const FIXTURE_DESTINATION_URL: &str = "00112233445566778899aabbccddeeff:/";
    const FIXTURE_LXMF_PEER_HASH: &str = "ffeeddccbbaa99887766554433221100";

    #[test]
    fn cli_defaults_to_compiled_frontend_without_args() {
        assert_eq!(
            CliCommand::parse(Vec::<String>::new()).expect("parse"),
            default_frontend_command()
        );
    }

    #[test]
    fn cli_parses_version_command() {
        for argument in ["--version", "-V", "version"] {
            assert_eq!(
                CliCommand::parse([argument.to_string()]).expect("parse"),
                CliCommand::Version
            );
        }
        let features = omenbrowser_rs::product_identity::compiled_feature_summary();
        assert!(features.contains("desktop-product:"));
        assert!(features.contains("desktop-dev:"));
        assert!(features.contains("desktop-test:"));
        assert!(features.contains("mock-runtime:"));
        assert!(features.contains("chat-client-reticulum:"));
        assert!(features.contains("chat-client-rns:"));
        assert!(features.contains("chat-client-rns-clean:"));
        assert!(!features.contains("chat-client-rns-legacy:"));
        assert!(!features.contains("native-rns-net:"));
    }

    #[test]
    fn cli_parses_help_command_and_alias() {
        for argument in ["--help", "-h"] {
            assert_eq!(
                CliCommand::parse([argument.to_string()]).expect("parse"),
                CliCommand::Help
            );
        }
        assert!(CliCommand::parse(["help".to_string()]).is_err());
    }

    #[test]
    fn cli_parses_explicit_frontend_selection() {
        for argument in ["--desktop", "--iced"] {
            assert_eq!(
                CliCommand::parse([argument.to_string()]).expect("parse"),
                CliCommand::Desktop { app_root: None }
            );
        }
        for argument in ["--tui", "--terminal"] {
            assert_eq!(
                CliCommand::parse([argument.to_string()]).expect("parse"),
                CliCommand::Tui { app_root: None }
            );
        }
    }

    #[test]
    fn cli_parses_frontend_app_root_for_isolated_runs() {
        assert_eq!(
            CliCommand::parse([
                "--desktop".to_string(),
                "--app-root".to_string(),
                "/tmp/omenbrowser-test".to_string(),
            ])
            .expect("parse"),
            CliCommand::Desktop {
                app_root: Some(PathBuf::from("/tmp/omenbrowser-test")),
            }
        );
        assert_eq!(
            CliCommand::parse([
                "--tui".to_string(),
                "--app-root".to_string(),
                "/tmp/omenbrowser-test".to_string(),
            ])
            .expect("parse"),
            CliCommand::Tui {
                app_root: Some(PathBuf::from("/tmp/omenbrowser-test")),
            }
        );
    }

    #[test]
    fn cli_resolves_owner_only_passphrase_file_before_command_parsing() {
        let root = std::env::temp_dir().join(format!(
            "omenbrowser-cli-passphrase-integration-{}-{}",
            std::process::id(),
            current_epoch_millis()
        ));
        std::fs::create_dir(&root).expect("create isolated root");
        let path = root.join("passphrase");
        std::fs::write(&path, b"integration-secret\n").expect("write passphrase");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
                .expect("permissions");
        }

        let parsed = CliCommand::parse([
            "--native-preflight".to_string(),
            FIXTURE_DESTINATION_URL.to_string(),
            "--passphrase-file".to_string(),
            path.display().to_string(),
            "--network-name".to_string(),
            "private-integration".to_string(),
            "--tcp-client".to_string(),
            "127.0.0.1:4242".to_string(),
        ])
        .expect("parse safe passphrase source");
        let CliCommand::NativePreflight { overrides, .. } = parsed else {
            panic!("expected native preflight command");
        };
        assert_eq!(
            overrides
                .tcp_client()
                .and_then(TcpClientOverride::passphrase),
            Some("integration-secret")
        );
        let tcp = overrides.tcp_client().expect("TCP override");
        assert_eq!(tcp.host(), "127.0.0.1");
        assert_eq!(tcp.port(), 4242);
        assert_eq!(tcp.network_name(), Some("private-integration"));
        std::fs::remove_dir_all(root).expect("remove isolated root");
    }

    #[test]
    fn cli_parses_omenchat_smoke_command() {
        let parsed = CliCommand::parse([
            "--omenchat-smoke".to_string(),
            FIXTURE_DESTINATION_HASH.to_string(),
            "--omenchat-room".to_string(),
            "lobby".to_string(),
            "--omenchat-message".to_string(),
            "hello smoke".to_string(),
            "--omenchat-reaction-smoke".to_string(),
            "--omenchat-revision-smoke".to_string(),
            "--path-wait".to_string(),
            "3".to_string(),
            "--tcp-client".to_string(),
            "127.0.0.1:4242".to_string(),
            "--network-name".to_string(),
            "private_ret".to_string(),
            "--passphrase".to_string(),
            "secret".to_string(),
            "--stdout".to_string(),
        ])
        .expect("parse");

        assert_eq!(
            parsed,
            CliCommand::OmenChatSmoke {
                destination: FIXTURE_DESTINATION_HASH.into(),
                room: "lobby".into(),
                message: "hello smoke".into(),
                reaction_smoke: true,
                revision_smoke: true,
                upload_file: None,
                fetch_upload_filename: None,
                fetch_upload_bytes: None,
                reconnect_ready_file: None,
                reconnect_wait_secs: 60,
                link_timeout_secs: 15,
                response_wait_secs: 10,
                warmup: Some(SmokePathWarmup { wait_secs: 3 }),
                output: None,
                stdout: true,
                overrides: Box::new(
                    SmokeOverrides::default()
                        .with_runtime_backend(RuntimeBackendSetting::Reticulum)
                        .with_tcp_client(TcpClientOverride::new(
                            "127.0.0.1",
                            4242,
                            Some("private_ret".into()),
                            Some("secret".into()),
                        )),
                ),
            }
        );
    }

    #[test]
    fn cli_parses_omenchat_continuous_reconnect_control() {
        let parsed = CliCommand::parse([
            "--omenchat-smoke".to_string(),
            FIXTURE_DESTINATION_HASH.to_string(),
            "--omenchat-reconnect-ready-file".to_string(),
            "/tmp/omenchat-reconnect-ready".to_string(),
            "--omenchat-reconnect-wait".to_string(),
            "45".to_string(),
        ])
        .expect("parse reconnect smoke control");

        let CliCommand::OmenChatSmoke {
            reconnect_ready_file,
            reconnect_wait_secs,
            ..
        } = parsed
        else {
            panic!("expected OMENchat smoke command");
        };
        assert_eq!(
            reconnect_ready_file,
            Some(PathBuf::from("/tmp/omenchat-reconnect-ready"))
        );
        assert_eq!(reconnect_wait_secs, 45);
    }

    #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
    #[test]
    fn omenchat_reconnect_ready_marker_is_create_new_and_isolated() {
        let root = std::env::temp_dir().join(format!(
            "omenbrowser-reconnect-marker-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir(&root).expect("create isolated marker root");
        let marker = root.join("ready");
        create_omenchat_reconnect_ready_file(&marker).expect("create ready marker");
        assert_eq!(std::fs::read(&marker).expect("read marker"), b"ready\n");
        assert!(create_omenchat_reconnect_ready_file(&marker).is_err());
        std::fs::remove_dir_all(root).expect("remove isolated marker root");
    }

    #[test]
    fn native_live_sequence_report_promotes_nested_suggestions() {
        let startup = serde_json::json!({
            "report": "native_runtime_startup",
            "classification": {
                "outcome": "pass",
                "stage": "runtime_status",
                "reason": "started",
                "next_step": "preflight",
            }
        });
        let preflight = serde_json::json!({
            "report": "native_network_preflight",
            "classification": {
                "outcome": "blocked",
                "stage": "known_destinations",
                "reason": "missing key",
                "next_step": "preload",
            },
            "suggested_commands": [
                {
                    "purpose": "preload",
                    "argv": ["cargo", "run", "--", "--native-smoke", "dest:/"]
                }
            ]
        });
        let validation = serde_json::json!({
            "report": "native_network_smoke_test",
            "classification": {
                "outcome": "blocked",
                "stage": "destination_identity",
                "reason": "missing identity",
                "next_step": "preload",
            },
            "suggested_commands": [
                {
                    "purpose": "preload",
                    "argv": ["cargo", "run", "--", "--native-smoke", "dest:/"]
                },
                {
                    "purpose": "sequence",
                    "argv": ["cargo", "run", "--", "--native-live-sequence", "dest:/"]
                }
            ]
        });

        let report = native_live_sequence_report(startup, preflight, validation);
        let suggestions = report
            .get("suggested_commands")
            .and_then(serde_json::Value::as_array)
            .expect("suggestions");
        let focus = report.get("failure_focus").expect("failure focus");

        assert_eq!(suggestions.len(), 2);
        assert_eq!(
            suggestions[0]
                .get("purpose")
                .and_then(serde_json::Value::as_str),
            Some("preload")
        );
        assert_eq!(
            suggestions[1]
                .get("purpose")
                .and_then(serde_json::Value::as_str),
            Some("sequence")
        );
        assert_eq!(
            focus.get("section").and_then(serde_json::Value::as_str),
            Some("preflight")
        );
        assert_eq!(
            focus.get("stage").and_then(serde_json::Value::as_str),
            Some("known_destinations")
        );
    }

    #[test]
    fn native_live_sequence_failure_focus_extracts_validation_probe_steps() {
        let startup = serde_json::json!({
            "report": "native_runtime_startup",
            "classification": {
                "outcome": "pass",
                "stage": "runtime_status",
                "reason": "started",
                "next_step": "preflight",
            }
        });
        let preflight = serde_json::json!({
            "report": "native_network_preflight",
            "classification": {
                "outcome": "pass",
                "stage": "ready",
                "reason": "ready",
                "next_step": "validate",
            }
        });
        let validation = serde_json::json!({
            "report": "native_network_smoke_test",
            "classification": {
                "outcome": "timeout",
                "stage": "response_wait",
                "reason": "live request timed out",
                "next_step": "verify remote node",
            },
            "verdicts": {
                "response_wait": {
                    "status": "fail",
                    "detail": "response timeout",
                    "next_action": "verify remote node",
                }
            },
            "live_page_probe": {
                "steps": [
                    {
                        "stage": "response_wait",
                        "ok": false,
                        "detail": "timed out waiting for response",
                        "trace": {
                            "request_id": "abc123"
                        }
                    }
                ]
            },
            "live_fetch": {
                "ok": false,
                "error": "timed out"
            }
        });

        let report = native_live_sequence_report(startup, preflight, validation);
        let focus = report.get("failure_focus").expect("failure focus");

        assert_eq!(
            focus.get("section").and_then(serde_json::Value::as_str),
            Some("validation")
        );
        assert_eq!(
            focus.get("stage").and_then(serde_json::Value::as_str),
            Some("response_wait")
        );
        assert_eq!(
            focus
                .get("live_probe_step")
                .and_then(|step| step.get("trace"))
                .and_then(|trace| trace.get("request_id"))
                .and_then(serde_json::Value::as_str),
            Some("abc123")
        );
    }

    #[test]
    fn report_summary_can_include_shell_escaped_suggestions() {
        let report = serde_json::json!({
            "report": "native_network_preflight",
            "classification": {
                "outcome": "blocked",
                "stage": "identity_path",
                "reason": "path is not configured",
                "next_step": "attach identity",
            },
            "suggested_commands": [
                {
                    "purpose": "run live probe",
                    "argv": [
                        "cargo",
                        "run",
                        "--features",
                        "native-network",
                        "--",
                        "--native-smoke",
                        "<destination:path>",
                        "--identity",
                        "it's private",
                        "--stdout"
                    ]
                }
            ]
        });

        let summary = render_report_summary_with_options(&report, true);

        assert!(summary.contains("suggested shell commands:"));
        assert!(summary.contains("- run live probe: cargo run --features native-network -- --native-smoke '<destination:path>' --identity 'it'\\''s private' --stdout"));
    }

    #[test]
    fn cli_parses_native_smoke_command() {
        let parsed = CliCommand::parse([
            "--native-smoke".to_string(),
            FIXTURE_DESTINATION_URL.to_string(),
            "--live".to_string(),
            "--backend".to_string(),
            "reticulum".to_string(),
            "--app-root".to_string(),
            "/tmp/omen-app".to_string(),
            "--identity".to_string(),
            "/tmp/identity".to_string(),
            "--reticulum-config".to_string(),
            "/tmp/rns".to_string(),
            "--known-destinations".to_string(),
            "/tmp/known_destinations".to_string(),
            "--generate-known-destinations-fixture".to_string(),
            "/tmp/fixture_known_destinations".to_string(),
            "--tcp-client".to_string(),
            "127.0.0.1:4242".to_string(),
            "--output".to_string(),
            "/tmp/report.json".to_string(),
        ])
        .expect("parse");
        assert_eq!(
            parsed,
            CliCommand::NativeSmoke {
                destination: FIXTURE_DESTINATION_URL.into(),
                live: true,
                fetch: false,
                lxmf_smoke_peer: None,
                lxmf_smoke_delivery_mode: omenbrowser_rs::messaging::DeliveryMode::Direct,
                lxmf_smoke_propagation_node: None,
                lxmf_include_ticket: false,
                lxmf_interop_wait_secs: None,
                warmup: None,
                output: Some(PathBuf::from("/tmp/report.json")),
                stdout: false,
                suggest_shell: false,
                bundle_report: None,
                overrides: Box::new(
                    SmokeOverrides::default()
                        .with_runtime_backend(RuntimeBackendSetting::Reticulum)
                        .with_app_root("/tmp/omen-app")
                        .with_identity_path("/tmp/identity")
                        .with_reticulum_config_path("/tmp/rns")
                        .with_known_destinations_path("/tmp/known_destinations")
                        .with_known_destinations_fixture_path("/tmp/fixture_known_destinations",)
                        .with_tcp_client(TcpClientOverride::new("127.0.0.1", 4242, None, None,)),
                ),
            }
        );
    }

    #[test]
    fn cli_delegates_typed_backend_and_delivery_values() {
        let parsed = CliCommand::parse([
            "--native-smoke".to_string(),
            FIXTURE_DESTINATION_URL.to_string(),
            "--backend".to_string(),
            "native-reticulum".to_string(),
            "--lxmf-delivery".to_string(),
            " PROP ".to_string(),
        ])
        .expect("parse compatibility aliases");

        let CliCommand::NativeSmoke {
            lxmf_smoke_delivery_mode,
            overrides,
            ..
        } = parsed
        else {
            panic!("expected native smoke command");
        };
        assert_eq!(
            lxmf_smoke_delivery_mode,
            omenbrowser_rs::messaging::DeliveryMode::Propagated
        );
        assert_eq!(
            overrides.runtime_backend(),
            Some(&RuntimeBackendSetting::Reticulum)
        );

        assert_eq!(
            CliCommand::parse([
                "--native-smoke".to_string(),
                FIXTURE_DESTINATION_URL.to_string(),
                "--backend".to_string(),
                "RETICULUM".to_string(),
            ])
            .expect_err("backend remains case-sensitive")
            .to_string(),
            "invalid backend RETICULUM; expected auto, mock, or reticulum"
        );
        assert_eq!(
            CliCommand::parse([
                "--native-smoke".to_string(),
                FIXTURE_DESTINATION_URL.to_string(),
                "--lxmf-smoke-method".to_string(),
                " Unknown ".to_string(),
            ])
            .expect_err("invalid delivery value")
            .to_string(),
            "invalid LXMF smoke delivery mode unknown; expected direct or propagated"
        );
    }

    #[test]
    fn cli_parses_native_live_sequence_command() {
        let parsed = CliCommand::parse([
            "--native-live-sequence".to_string(),
            FIXTURE_DESTINATION_URL.to_string(),
            "--known-destinations".to_string(),
            "/tmp/known_destinations".to_string(),
            "--path-wait".to_string(),
            "7".to_string(),
            "--preflight-wait".to_string(),
            "500".to_string(),
            "--send-lxmf-smoke".to_string(),
            FIXTURE_LXMF_PEER_HASH.to_string(),
            "--lxmf-wait".to_string(),
            "15".to_string(),
            "--tcp-client".to_string(),
            "127.0.0.1:4242".to_string(),
            "--stdout".to_string(),
        ])
        .expect("parse");

        assert_eq!(
            parsed,
            CliCommand::NativeLiveSequence {
                destination: FIXTURE_DESTINATION_URL.into(),
                lxmf_smoke_peer: Some(FIXTURE_LXMF_PEER_HASH.into()),
                lxmf_smoke_delivery_mode: omenbrowser_rs::messaging::DeliveryMode::Direct,
                lxmf_smoke_propagation_node: None,
                lxmf_include_ticket: false,
                lxmf_interop_wait_secs: Some(15),
                warmup: SmokePathWarmup { wait_secs: 7 },
                preflight_wait_ms: 500,
                output: None,
                stdout: true,
                suggest_shell: false,
                bundle_report: None,
                overrides: Box::new(
                    SmokeOverrides::default()
                        .with_runtime_backend(RuntimeBackendSetting::Reticulum)
                        .with_known_destinations_path("/tmp/known_destinations")
                        .with_tcp_client(TcpClientOverride::new("127.0.0.1", 4242, None, None,)),
                ),
            }
        );
    }

    #[test]
    fn cli_parses_native_smoke_path_warmup() {
        let parsed = CliCommand::parse([
            "--native-smoke".to_string(),
            FIXTURE_DESTINATION_URL.to_string(),
            "--warm-path".to_string(),
            "--path-wait".to_string(),
            "0".to_string(),
            "--stdout".to_string(),
        ])
        .expect("parse");
        assert_eq!(
            parsed,
            CliCommand::NativeSmoke {
                destination: FIXTURE_DESTINATION_URL.into(),
                live: false,
                fetch: false,
                lxmf_smoke_peer: None,
                lxmf_smoke_delivery_mode: omenbrowser_rs::messaging::DeliveryMode::Direct,
                lxmf_smoke_propagation_node: None,
                lxmf_include_ticket: false,
                lxmf_interop_wait_secs: None,
                warmup: Some(SmokePathWarmup { wait_secs: 0 }),
                output: None,
                stdout: true,
                suggest_shell: false,
                bundle_report: None,
                overrides: Box::new(SmokeOverrides::default()),
            }
        );
    }

    #[test]
    fn cli_parses_native_smoke_live_fetch_as_live_probe_plus_fetch() {
        let parsed = CliCommand::parse([
            "--native-smoke".to_string(),
            FIXTURE_DESTINATION_URL.to_string(),
            "--fetch-page".to_string(),
            "--stdout".to_string(),
        ])
        .expect("parse");

        assert_eq!(
            parsed,
            CliCommand::NativeSmoke {
                destination: FIXTURE_DESTINATION_URL.into(),
                live: true,
                fetch: true,
                lxmf_smoke_peer: None,
                lxmf_smoke_delivery_mode: omenbrowser_rs::messaging::DeliveryMode::Direct,
                lxmf_smoke_propagation_node: None,
                lxmf_include_ticket: false,
                lxmf_interop_wait_secs: None,
                warmup: None,
                output: None,
                stdout: true,
                suggest_shell: false,
                bundle_report: None,
                overrides: Box::new(SmokeOverrides::default()),
            }
        );
    }

    #[test]
    fn cli_native_validate_defaults_to_live_reticulum_fetch_with_warmup() {
        let parsed = CliCommand::parse([
            "--native-validate".to_string(),
            FIXTURE_DESTINATION_URL.to_string(),
            "--stdout".to_string(),
        ])
        .expect("parse");

        assert_eq!(
            parsed,
            CliCommand::NativeSmoke {
                destination: FIXTURE_DESTINATION_URL.into(),
                live: true,
                fetch: true,
                lxmf_smoke_peer: None,
                lxmf_smoke_delivery_mode: omenbrowser_rs::messaging::DeliveryMode::Direct,
                lxmf_smoke_propagation_node: None,
                lxmf_include_ticket: false,
                lxmf_interop_wait_secs: None,
                warmup: Some(SmokePathWarmup { wait_secs: 10 }),
                output: None,
                stdout: true,
                suggest_shell: false,
                bundle_report: None,
                overrides: Box::new(
                    SmokeOverrides::default()
                        .with_runtime_backend(RuntimeBackendSetting::Reticulum),
                ),
            }
        );
    }

    #[test]
    fn cli_parses_explicit_lxmf_smoke_send_peer() {
        let parsed = CliCommand::parse([
            "--native-smoke".to_string(),
            FIXTURE_DESTINATION_URL.to_string(),
            "--send-lxmf-smoke".to_string(),
            FIXTURE_LXMF_PEER_HASH.to_string(),
            "--lxmf-include-ticket".to_string(),
            "--stdout".to_string(),
        ])
        .expect("parse");

        assert_eq!(
            parsed,
            CliCommand::NativeSmoke {
                destination: FIXTURE_DESTINATION_URL.into(),
                live: false,
                fetch: false,
                lxmf_smoke_peer: Some(FIXTURE_LXMF_PEER_HASH.into()),
                lxmf_smoke_delivery_mode: omenbrowser_rs::messaging::DeliveryMode::Direct,
                lxmf_smoke_propagation_node: None,
                lxmf_include_ticket: true,
                lxmf_interop_wait_secs: None,
                warmup: None,
                output: None,
                stdout: true,
                suggest_shell: false,
                bundle_report: None,
                overrides: Box::new(SmokeOverrides::default()),
            }
        );
    }

    #[test]
    fn cli_parses_lxmf_interop_wait() {
        let parsed = CliCommand::parse([
            "--native-smoke".to_string(),
            FIXTURE_DESTINATION_URL.to_string(),
            "--send-lxmf-smoke".to_string(),
            FIXTURE_LXMF_PEER_HASH.to_string(),
            "--lxmf-wait".to_string(),
            "3".to_string(),
            "--stdout".to_string(),
        ])
        .expect("parse");

        assert_eq!(
            parsed,
            CliCommand::NativeSmoke {
                destination: FIXTURE_DESTINATION_URL.into(),
                live: false,
                fetch: false,
                lxmf_smoke_peer: Some(FIXTURE_LXMF_PEER_HASH.into()),
                lxmf_smoke_delivery_mode: omenbrowser_rs::messaging::DeliveryMode::Direct,
                lxmf_smoke_propagation_node: None,
                lxmf_include_ticket: false,
                lxmf_interop_wait_secs: Some(3),
                warmup: None,
                output: None,
                stdout: true,
                suggest_shell: false,
                bundle_report: None,
                overrides: Box::new(SmokeOverrides::default()),
            }
        );
    }

    #[test]
    fn cli_parses_dedicated_lxmf_interop_command() {
        let parsed = CliCommand::parse([
            "--lxmf-interop".to_string(),
            "--send-lxmf-smoke".to_string(),
            FIXTURE_LXMF_PEER_HASH.to_string(),
            "--backend".to_string(),
            "reticulum".to_string(),
            "--stdout".to_string(),
        ])
        .expect("parse");

        assert_eq!(
            parsed,
            CliCommand::LxmfInterop {
                peer_hash: Some(FIXTURE_LXMF_PEER_HASH.into()),
                lxmf_smoke_delivery_mode: omenbrowser_rs::messaging::DeliveryMode::Direct,
                lxmf_smoke_propagation_node: None,
                lxmf_include_ticket: false,
                wait_secs: 10,
                output: None,
                stdout: true,
                suggest_shell: false,
                bundle_report: None,
                overrides: Box::new(
                    SmokeOverrides::default()
                        .with_runtime_backend(RuntimeBackendSetting::Reticulum),
                ),
            }
        );
    }

    #[test]
    fn cli_parses_native_startup_command() {
        let parsed = CliCommand::parse([
            "--native-startup".to_string(),
            "--backend".to_string(),
            "reticulum".to_string(),
            "--identity".to_string(),
            "/tmp/identity".to_string(),
            "--tcp-client".to_string(),
            "127.0.0.1:4242".to_string(),
            "--stdout".to_string(),
        ])
        .expect("parse");

        assert_eq!(
            parsed,
            CliCommand::NativeStartup {
                output: None,
                stdout: true,
                suggest_shell: false,
                bundle_report: None,
                overrides: Box::new(
                    SmokeOverrides::default()
                        .with_runtime_backend(RuntimeBackendSetting::Reticulum)
                        .with_identity_path("/tmp/identity")
                        .with_tcp_client(TcpClientOverride::new("127.0.0.1", 4242, None, None,)),
                ),
            }
        );
    }

    #[test]
    fn cli_parses_generate_native_identity_command() {
        let parsed = CliCommand::parse([
            "--generate-native-identity".to_string(),
            "Live Identity".to_string(),
            "--app-root".to_string(),
            "/tmp/omen-app".to_string(),
            "--reticulum-config".to_string(),
            "/tmp/omen-rns".to_string(),
            "--output".to_string(),
            "/tmp/identity-report.json".to_string(),
        ])
        .expect("parse");

        assert_eq!(
            parsed,
            CliCommand::GenerateNativeIdentity {
                label: "Live Identity".into(),
                output: Some(PathBuf::from("/tmp/identity-report.json")),
                stdout: false,
                overrides: Box::new(
                    SmokeOverrides::default()
                        .with_app_root("/tmp/omen-app")
                        .with_reticulum_config_path("/tmp/omen-rns"),
                ),
            }
        );
    }

    #[cfg(feature = "native-reticulum")]
    #[test]
    fn generate_native_identity_command_creates_and_activates_identity() {
        let root = std::env::temp_dir().join(format!(
            "omenbrowser-rs-cli-native-identity-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let output = root.join("identity-report.json");

        run_generate_native_identity_command(GenerateNativeIdentityCommandInput {
            label: "CLI Native".into(),
            output: Some(output.clone()),
            stdout: false,
            overrides: SmokeOverrides::default()
                .with_app_root(root.clone())
                .with_runtime_backend(RuntimeBackendSetting::Reticulum),
        })
        .expect("generate identity");

        let paths = AppPaths::from_root(root);
        let settings =
            omenbrowser_rs::storage::settings::AppSettings::load_or_default(&paths.settings_file)
                .expect("load settings");
        let identity_path = settings.identity_path.expect("identity path");
        let summary =
            omenbrowser_rs::runtime::native::identity::load_private_identity_file(&identity_path)
                .expect("load native identity");
        assert_eq!(settings.runtime_backend, RuntimeBackendSetting::Reticulum);
        assert_eq!(
            settings.active_identity_label.as_deref(),
            Some("CLI Native")
        );
        assert_eq!(
            summary.byte_len,
            omenbrowser_rs::runtime::native::identity::native_private_identity_len()
        );
        assert!(output.exists());
    }

    #[test]
    fn cli_parses_native_preflight_command() {
        let parsed = CliCommand::parse([
            "--native-preflight".to_string(),
            FIXTURE_DESTINATION_URL.to_string(),
            "--send-lxmf-smoke".to_string(),
            FIXTURE_LXMF_PEER_HASH.to_string(),
            "--stdout".to_string(),
            "--suggest-shell".to_string(),
            "--preflight-wait".to_string(),
            "750".to_string(),
            "--bundle-report".to_string(),
            "/tmp/preflight-bundles".to_string(),
        ])
        .expect("parse");

        assert_eq!(
            parsed,
            CliCommand::NativePreflight {
                destination: FIXTURE_DESTINATION_URL.into(),
                lxmf_peer: Some(FIXTURE_LXMF_PEER_HASH.into()),
                preflight_wait_ms: 750,
                output: None,
                stdout: true,
                suggest_shell: true,
                bundle_report: Some(PathBuf::from("/tmp/preflight-bundles")),
                overrides: Box::new(SmokeOverrides::default()),
            }
        );
    }

    #[test]
    fn native_preflight_classification_prefers_fail_before_blocked() {
        let stages = vec![
            preflight_stage(
                "identity_path",
                false,
                true,
                "missing identity",
                "attach identity",
            ),
            preflight_stage(
                "nomadnet_address",
                false,
                false,
                "bad address",
                "fix address",
            ),
        ];
        let classification = classify_preflight(&stages);

        assert_eq!(
            classification
                .get("outcome")
                .and_then(serde_json::Value::as_str),
            Some("fail")
        );
        assert_eq!(
            classification
                .get("stage")
                .and_then(serde_json::Value::as_str),
            Some("nomadnet_address")
        );
    }

    #[test]
    fn native_preflight_suggestions_include_stage_specific_commands() {
        let known = PathBuf::from("/tmp/private/known_destinations");
        let stages = vec![
            preflight_stage(
                "known_destinations",
                false,
                false,
                "destination missing",
                "provide known destinations",
            ),
            preflight_stage("lxmf_peer_hash", false, false, "bad peer", "fix peer"),
        ];

        let suggestions = preflight_suggested_commands(
            FIXTURE_DESTINATION_URL,
            Some("bad-peer"),
            Some(&known),
            &stages,
        );

        assert!(suggestions.iter().any(|suggestion| {
            suggestion
                .get("purpose")
                .and_then(serde_json::Value::as_str)
                == Some("preload_known_destinations_and_dry_probe")
        }));
        assert!(suggestions.iter().any(|suggestion| {
            suggestion
                .get("purpose")
                .and_then(serde_json::Value::as_str)
                == Some("fix_lxmf_peer")
        }));
        let text = serde_json::to_string(&suggestions).expect("suggestions");
        assert!(text.contains("<path:known_destinations>"));
        assert!(!text.contains("/tmp/private/known_destinations"));
    }

    #[test]
    fn native_preflight_suggestions_include_live_next_steps_when_ready() {
        let stages = vec![preflight_stage(
            "transport_startup",
            true,
            false,
            "ready",
            "continue",
        )];

        let suggestions = preflight_suggested_commands(
            FIXTURE_DESTINATION_URL,
            Some(FIXTURE_LXMF_PEER_HASH),
            None,
            &stages,
        );

        assert!(suggestions.iter().any(|suggestion| {
            suggestion
                .get("purpose")
                .and_then(serde_json::Value::as_str)
                == Some("live_fetch")
        }));
        assert!(suggestions.iter().any(|suggestion| {
            suggestion
                .get("purpose")
                .and_then(serde_json::Value::as_str)
                == Some("lxmf_interop_send_and_wait")
        }));
    }

    #[test]
    fn native_smoke_suggestions_follow_classification_stage() {
        let known = PathBuf::from("/tmp/private/known_destinations");
        let report = serde_json::json!({
            "report": "native_network_smoke_test",
            "classification": {
                "outcome": "blocked",
                "stage": "destination_identity",
                "reason": "identity missing",
                "next_step": "preload known_destinations",
            }
        });

        let suggestions =
            native_smoke_suggested_commands(&report, FIXTURE_DESTINATION_URL, Some(&known));

        assert!(suggestions.iter().any(|suggestion| {
            suggestion
                .get("purpose")
                .and_then(serde_json::Value::as_str)
                == Some("preload_known_destinations_and_dry_probe")
        }));
        let text = serde_json::to_string(&suggestions).expect("suggestions");
        assert!(text.contains("<path:known_destinations>"));
        assert!(!text.contains("/tmp/private/known_destinations"));
    }

    #[test]
    fn lxmf_interop_suggestions_retry_timeouts_with_longer_wait() {
        let report = serde_json::json!({
            "report": "native_lxmf_live_interop",
            "peer_hash": FIXTURE_LXMF_PEER_HASH,
            "classification": {
                "outcome": "timeout",
                "wait_status": "timeout",
                "next_step": "increase wait",
            }
        });

        let suggestions = lxmf_interop_suggested_commands_from_report(&report, 10);

        assert!(suggestions.iter().any(|suggestion| {
            suggestion
                .get("purpose")
                .and_then(serde_json::Value::as_str)
                == Some("lxmf_interop_retry_longer_wait")
        }));
        let text = serde_json::to_string(&suggestions).expect("suggestions");
        assert!(text.contains(FIXTURE_LXMF_PEER_HASH));
        assert!(text.contains("\"20\""));
    }

    #[cfg(feature = "native-reticulum")]
    #[tokio::test]
    async fn native_preflight_transport_startup_stage_reports_shutdown() {
        let root = std::env::temp_dir().join(format!(
            "omen-preflight-transport-main-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let paths = AppPaths::from_root(root.clone());
        paths.ensure().expect("paths");
        let config = AppConfig {
            paths,
            settings: omenbrowser_rs::storage::settings::AppSettings::default(),
        };
        let mut app = App::new(config);

        let stage =
            collect_transport_startup_preflight(&mut app, None, Duration::from_millis(0)).await;

        assert_eq!(
            stage.get("stage").and_then(serde_json::Value::as_str),
            Some("transport_startup")
        );
        assert_eq!(
            stage.get("startup_ok").and_then(serde_json::Value::as_bool),
            Some(true)
        );
        assert_eq!(
            stage
                .get("shutdown_ok")
                .and_then(serde_json::Value::as_bool),
            Some(true)
        );
        assert!(stage.get("status").is_some());
        assert!(stage.get("interface_stats").is_some());
        assert_eq!(
            stage.get("wait_ms").and_then(serde_json::Value::as_u64),
            Some(0)
        );
        assert_eq!(
            stage.get("expectation").and_then(serde_json::Value::as_str),
            Some("reticulum_transport")
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn known_destinations_preflight_default_build_checks_file_without_semantic_parse() {
        let dir = std::env::temp_dir().join(format!(
            "omen-preflight-known-default-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("dir");
        let path = dir.join("known_destinations");
        std::fs::write(&path, b"not parsed in default build").expect("known destinations");

        let stage = preflight_known_destinations_stage(Some(&path), Some([0x11; 16]));

        assert_eq!(
            stage.get("stage").and_then(serde_json::Value::as_str),
            Some("known_destinations")
        );
        #[cfg(not(all(feature = "native-rns-net", any())))]
        assert_eq!(
            stage.get("outcome").and_then(serde_json::Value::as_str),
            Some("pass")
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn cli_parses_bundle_report_root() {
        let parsed = CliCommand::parse([
            "--native-smoke".to_string(),
            FIXTURE_DESTINATION_URL.to_string(),
            "--bundle-report".to_string(),
            "/tmp/omen-bundles".to_string(),
        ])
        .expect("parse");

        assert_eq!(
            parsed,
            CliCommand::NativeSmoke {
                destination: FIXTURE_DESTINATION_URL.into(),
                live: false,
                fetch: false,
                lxmf_smoke_peer: None,
                lxmf_smoke_delivery_mode: omenbrowser_rs::messaging::DeliveryMode::Direct,
                lxmf_smoke_propagation_node: None,
                lxmf_include_ticket: false,
                lxmf_interop_wait_secs: None,
                warmup: None,
                output: None,
                stdout: false,
                suggest_shell: false,
                bundle_report: Some(PathBuf::from("/tmp/omen-bundles")),
                overrides: Box::new(SmokeOverrides::default()),
            }
        );
    }

    #[test]
    fn tcp_override_debug_redacts_passphrase() {
        let value = TcpClientOverride::new(
            "gateway.example",
            42420,
            Some("private".into()),
            Some("debug-secret-value".into()),
        );
        let debug = format!("{value:?}");
        assert!(!debug.contains("debug-secret-value"));
        assert!(debug.contains("<redacted>"));
    }

    #[test]
    fn report_bundle_writes_expected_redacted_files() {
        let dir =
            std::env::temp_dir().join(format!("omen-report-bundle-main-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let logs_dir = dir.join("logs");
        std::fs::create_dir_all(&logs_dir).expect("logs dir");
        let log_entry = omenbrowser_rs::app::LogEntry {
            epoch_ms: 1,
            severity: omenbrowser_rs::app::LogSeverity::Warn,
            source: omenbrowser_rs::app::LogSource::Runtime,
            message: "runtime saw /tmp/private/identity and message body: secret".into(),
        };
        std::fs::write(
            logs_dir.join("omenbrowser_rs.jsonl"),
            format!(
                "{}\n",
                serde_json::to_string(&log_entry).expect("log entry json")
            ),
        )
        .expect("seed logs");
        let report = serde_json::json!({
            "report": "native_network_smoke_test",
            "classification": {
                "outcome": "blocked",
                "stage": "destination_identity",
                "reason": "missing identity",
                "next_step": "preload known_destinations",
            }
        });
        let overrides = SmokeOverrides::default()
            .with_identity_path("/tmp/private/identity")
            .with_reticulum_config_path("/tmp/private/rns");

        let bundle_dir = write_report_bundle(ReportBundleInput {
            root: &dir,
            prefix: "native-network-smoke",
            command_kind: "native_smoke",
            report: &report,
            summary: "summary text",
            overrides: &overrides,
            logs_dir: &logs_dir,
            identity_path: overrides.identity_path(),
        })
        .expect("write bundle");

        assert!(bundle_dir.join("bundle.json").is_file());
        assert!(bundle_dir.join("report.json").is_file());
        assert!(bundle_dir.join("summary.txt").is_file());
        assert!(bundle_dir.join("command.json").is_file());
        assert!(bundle_dir.join("environment.json").is_file());
        assert!(bundle_dir.join("logs.json").is_file());
        let manifest = std::fs::read_to_string(bundle_dir.join("bundle.json")).expect("manifest");
        assert!(manifest.contains("omenbrowser_rs.cli_report_bundle.v1"));
        let command = std::fs::read_to_string(bundle_dir.join("command.json")).expect("command");
        assert!(command.contains("native_smoke"));
        assert!(command.contains("omenbrowser_rs.cli_command.v1"));
        assert!(command.contains("\"file_name\": \"identity\""));
        assert!(!command.contains("/tmp/private/identity"));
        assert!(!command.contains("/tmp/private/rns"));
        let logs = std::fs::read_to_string(bundle_dir.join("logs.json")).expect("logs");
        assert!(logs.contains("omenbrowser_rs.cli_recent_logs.v1"));
        assert!(logs.contains("<redacted message body log>"));
        assert!(!logs.contains("/tmp/private/identity"));
        let logs_json: serde_json::Value = serde_json::from_str(&logs).expect("logs JSON");
        assert_eq!(
            logs_json.pointer("/limits/directory_entries"),
            Some(&serde_json::json!(REPORT_LOG_DIRECTORY_ENTRY_LIMIT))
        );
        assert_eq!(
            logs_json.pointer("/limits/files"),
            Some(&serde_json::json!(REPORT_LOG_FILE_LIMIT))
        );
        assert_eq!(
            logs_json.pointer("/limits/bytes_per_file"),
            Some(&serde_json::json!(REPORT_LOG_FILE_BYTES))
        );
        assert_eq!(
            logs_json.pointer("/limits/total_bytes"),
            Some(&serde_json::json!(REPORT_LOG_TOTAL_BYTES))
        );
        assert_eq!(
            logs_json.pointer("/collection/files_read"),
            Some(&serde_json::json!(1))
        );
        assert!(logs_json
            .pointer("/collection/bytes_read")
            .and_then(serde_json::Value::as_u64)
            .is_some_and(|bytes| bytes <= REPORT_LOG_TOTAL_BYTES as u64));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(all(feature = "native-rns-net", any()))]
    #[test]
    fn known_destinations_preflight_semantic_check_finds_destination() {
        let dir = std::env::temp_dir().join(format!(
            "omen-preflight-known-native-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("known_destinations");
        let destination = [
            0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xaa, 0xbb, 0xcc, 0xdd,
            0xee, 0xff,
        ];
        write_known_destinations_fixture(&path, destination).expect("fixture");

        let stage = preflight_known_destinations_stage(Some(&path), Some(destination));

        assert_eq!(
            stage.get("outcome").and_then(serde_json::Value::as_str),
            Some("pass")
        );
        let detail = stage
            .get("detail")
            .and_then(serde_json::Value::as_str)
            .expect("detail");
        assert!(detail.contains("\"loaded\":1"));
        assert!(detail.contains("\"destination_present\":true"));

        let _ = std::fs::remove_dir_all(&dir);
    }
}
