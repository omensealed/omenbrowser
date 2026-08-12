use anyhow::Context;

mod omenchat_smoke;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use omenbrowser_rs::app::{App, SmokePathWarmup};
use omenbrowser_rs::browser::BrowserAddress;
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
        #[cfg(feature = "chat-client")]
        CliCommand::LxmfInvitationSmoke {
            peer_hash,
            server_destination,
            wait_secs,
            output,
            stdout,
            overrides,
        } => {
            run_lxmf_invitation_smoke_command(LxmfInvitationSmokeCommandInput {
                peer_hash,
                server_destination,
                wait_secs,
                output,
                stdout,
                overrides: *overrides,
            })
            .await
        }
        #[cfg(not(feature = "chat-client"))]
        CliCommand::LxmfInvitationSmoke { .. } => {
            anyhow::bail!("LXMF invitation smoke unavailable: build with feature chat-client")
        }
        CliCommand::LxmfInvitationCapabilityProbe {
            peer_hash,
            cancel_after_ms,
            output,
            stdout,
            overrides,
        } => {
            run_lxmf_invitation_capability_probe_command(
                peer_hash,
                cancel_after_ms,
                output,
                stdout,
                *overrides,
            )
            .await
        }
        CliCommand::LxmfTopicCapabilityProbe {
            output,
            stdout,
            overrides,
        } => run_lxmf_topic_capability_probe_command(output, stdout, *overrides).await,
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
            local_display_name,
            announcement_rejection_smoke,
            announcement_upload_rejection_smoke,
            room_media_policy_upload_rejection_smoke,
            slow_mode_rejection_smoke,
            slow_mode_delta_seconds,
            reaction_smoke,
            revision_smoke,
            pin_smoke,
            moderation_audit_smoke,
            moderation_audit_target,
            moderation_audit_expect_record,
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
            omenchat_smoke::run(OmenChatSmokeCommandInput {
                destination,
                room,
                message,
                local_display_name,
                announcement_rejection_smoke,
                announcement_upload_rejection_smoke,
                room_media_policy_upload_rejection_smoke,
                slow_mode_rejection_smoke,
                slow_mode_delta_seconds,
                reaction_smoke,
                revision_smoke,
                pin_smoke,
                moderation_audit_smoke,
                moderation_audit_target,
                moderation_audit_expect_record,
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
    LxmfInvitationSmoke {
        peer_hash: Option<String>,
        server_destination: String,
        wait_secs: u64,
        output: Option<PathBuf>,
        stdout: bool,
        overrides: Box<SmokeOverrides>,
    },
    LxmfInvitationCapabilityProbe {
        peer_hash: String,
        cancel_after_ms: Option<u64>,
        output: Option<PathBuf>,
        stdout: bool,
        overrides: Box<SmokeOverrides>,
    },
    LxmfTopicCapabilityProbe {
        output: Option<PathBuf>,
        stdout: bool,
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
        local_display_name: String,
        announcement_rejection_smoke: bool,
        announcement_upload_rejection_smoke: bool,
        room_media_policy_upload_rejection_smoke: bool,
        slow_mode_rejection_smoke: bool,
        slow_mode_delta_seconds: Option<u32>,
        reaction_smoke: bool,
        revision_smoke: bool,
        pin_smoke: bool,
        moderation_audit_smoke: bool,
        moderation_audit_target: Option<String>,
        moderation_audit_expect_record: bool,
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

#[cfg(feature = "chat-client")]
struct LxmfInvitationSmokeCommandInput {
    peer_hash: Option<String>,
    server_destination: String,
    wait_secs: u64,
    output: Option<PathBuf>,
    stdout: bool,
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
    local_display_name: String,
    announcement_rejection_smoke: bool,
    announcement_upload_rejection_smoke: bool,
    room_media_policy_upload_rejection_smoke: bool,
    slow_mode_rejection_smoke: bool,
    slow_mode_delta_seconds: Option<u32>,
    reaction_smoke: bool,
    revision_smoke: bool,
    pin_smoke: bool,
    moderation_audit_smoke: bool,
    moderation_audit_target: Option<String>,
    moderation_audit_expect_record: bool,
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
        let mut omenchat_local_display_name = "OMENbrowser_rs smoke".to_string();
        let mut omenchat_announcement_rejection_smoke = false;
        let mut omenchat_announcement_upload_rejection_smoke = false;
        let mut omenchat_room_media_policy_upload_rejection_smoke = false;
        let mut omenchat_slow_mode_rejection_smoke = false;
        let mut omenchat_slow_mode_delta_seconds = None;
        let mut omenchat_reaction_smoke = false;
        let mut omenchat_revision_smoke = false;
        let mut omenchat_pin_smoke = false;
        let mut omenchat_moderation_audit_smoke = false;
        let mut omenchat_moderation_audit_target = None;
        let mut omenchat_moderation_audit_expect_record = false;
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
        let mut lxmf_invitation_smoke_server = None;
        let mut lxmf_invitation_capability_peer = None;
        let mut lxmf_invitation_capability_cancel_after_ms = None;
        let mut lxmf_topic_capability_probe = false;
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
                "--omenchat-local-display-name" => {
                    omenchat_local_display_name = args.next().ok_or_else(|| {
                        anyhow::anyhow!("{arg} requires an OMENchat display name")
                    })?;
                }
                "--omenchat-announcement-rejection-smoke" => {
                    omenchat_announcement_rejection_smoke = true;
                }
                "--omenchat-announcement-upload-rejection-smoke" => {
                    omenchat_announcement_upload_rejection_smoke = true;
                }
                "--omenchat-room-media-policy-upload-rejection-smoke" => {
                    omenchat_room_media_policy_upload_rejection_smoke = true;
                }
                "--omenchat-slow-mode-rejection-smoke" => {
                    omenchat_slow_mode_rejection_smoke = true;
                }
                "--omenchat-slow-mode-delta-smoke" => {
                    let value = args
                        .next()
                        .ok_or_else(|| anyhow::anyhow!("{arg} requires seconds"))?;
                    omenchat_slow_mode_delta_seconds = Some(value.parse().map_err(|_| {
                        anyhow::anyhow!("{arg} requires an integer number of seconds")
                    })?);
                }
                "--omenchat-reaction-smoke" => {
                    omenchat_reaction_smoke = true;
                }
                "--omenchat-revision-smoke" => {
                    omenchat_revision_smoke = true;
                }
                "--omenchat-pin-smoke" => {
                    omenchat_pin_smoke = true;
                }
                "--omenchat-moderation-audit-smoke" => {
                    omenchat_moderation_audit_smoke = true;
                }
                "--omenchat-moderation-audit-target" => {
                    omenchat_moderation_audit_target = Some(args.next().ok_or_else(|| {
                        anyhow::anyhow!("{arg} requires an active user display name")
                    })?);
                }
                "--omenchat-moderation-audit-expect-record" => {
                    omenchat_moderation_audit_expect_record = true;
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
                "--lxmf-invitation-smoke" => {
                    lxmf_invitation_smoke_server = Some(args.next().ok_or_else(|| {
                        anyhow::anyhow!("{arg} requires an OMENchat server destination hash")
                    })?);
                    if lxmf_interop_wait_secs.is_none() {
                        lxmf_interop_wait_secs = Some(30);
                    }
                }
                "--lxmf-invitation-capability-probe" => {
                    lxmf_invitation_capability_peer = Some(args.next().ok_or_else(|| {
                        anyhow::anyhow!("{arg} requires an LXMF peer destination hash")
                    })?);
                }
                "--lxmf-invitation-capability-cancel-after-ms" => {
                    let value = args
                        .next()
                        .ok_or_else(|| anyhow::anyhow!("{arg} requires a millisecond value"))?;
                    let delay = value.parse::<u64>().with_context(|| {
                        format!("invalid LXMF invitation capability cancellation delay in {value}")
                    })?;
                    if delay > omenbrowser_rs::runtime::LXMF_INVITATION_CAPABILITY_PROBE_DEADLINE_MS
                    {
                        anyhow::bail!(
                            "LXMF invitation capability cancellation delay exceeds the probe deadline"
                        );
                    }
                    lxmf_invitation_capability_cancel_after_ms = Some(delay);
                }
                "--lxmf-topic-capability-probe" => {
                    lxmf_topic_capability_probe = true;
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
            + usize::from(lxmf_invitation_smoke_server.is_some())
            + usize::from(
                lxmf_interop_wait_secs.is_some()
                    && lxmf_invitation_smoke_server.is_none()
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
            if omenchat_slow_mode_delta_seconds.is_some_and(|seconds| {
                !(1..=omenchat_protocol::ROOM_SLOW_MODE_MAX_SECONDS).contains(&seconds)
            }) {
                return Err(anyhow::anyhow!(
                    "--omenchat-slow-mode-delta-smoke seconds are outside protocol bounds"
                ));
            }
            if omenchat_announcement_rejection_smoke && omenchat_announcement_upload_rejection_smoke
            {
                return Err(anyhow::anyhow!(
                    "choose only one OMENchat announcement rejection smoke mode"
                ));
            }
            if omenchat_room_media_policy_upload_rejection_smoke
                && (omenchat_announcement_rejection_smoke
                    || omenchat_announcement_upload_rejection_smoke
                    || omenchat_slow_mode_rejection_smoke
                    || omenchat_slow_mode_delta_seconds.is_some()
                    || omenchat_moderation_audit_smoke
                    || omenchat_reaction_smoke
                    || omenchat_revision_smoke
                    || omenchat_pin_smoke
                    || omenchat_upload_file.is_none()
                    || omenchat_fetch_upload_filename.is_some()
                    || omenchat_reconnect_ready_file.is_some())
            {
                return Err(anyhow::anyhow!(
                    "--omenchat-room-media-policy-upload-rejection-smoke requires one upload file and is an isolated qualification case"
                ));
            }
            if omenchat_slow_mode_rejection_smoke
                && (omenchat_announcement_rejection_smoke
                    || omenchat_announcement_upload_rejection_smoke
                    || omenchat_reaction_smoke
                    || omenchat_revision_smoke
                    || omenchat_pin_smoke
                    || omenchat_upload_file.is_some()
                    || omenchat_fetch_upload_filename.is_some()
                    || omenchat_reconnect_ready_file.is_some())
            {
                return Err(anyhow::anyhow!(
                    "--omenchat-slow-mode-rejection-smoke is an isolated qualification case"
                ));
            }
            if omenchat_slow_mode_delta_seconds.is_some()
                && (omenchat_slow_mode_rejection_smoke
                    || omenchat_announcement_rejection_smoke
                    || omenchat_announcement_upload_rejection_smoke
                    || omenchat_reaction_smoke
                    || omenchat_revision_smoke
                    || omenchat_pin_smoke
                    || omenchat_upload_file.is_some()
                    || omenchat_fetch_upload_filename.is_some()
                    || omenchat_reconnect_ready_file.is_some())
            {
                return Err(anyhow::anyhow!(
                    "--omenchat-slow-mode-delta-smoke is an isolated qualification case"
                ));
            }
            if omenchat_moderation_audit_smoke
                && (omenchat_slow_mode_rejection_smoke
                    || omenchat_slow_mode_delta_seconds.is_some()
                    || omenchat_announcement_rejection_smoke
                    || omenchat_announcement_upload_rejection_smoke
                    || omenchat_reaction_smoke
                    || omenchat_revision_smoke
                    || omenchat_pin_smoke
                    || omenchat_upload_file.is_some()
                    || omenchat_fetch_upload_filename.is_some()
                    || omenchat_reconnect_ready_file.is_some())
            {
                return Err(anyhow::anyhow!(
                    "--omenchat-moderation-audit-smoke is an isolated qualification case"
                ));
            }
            if omenchat_moderation_audit_target.is_some() && !omenchat_moderation_audit_smoke {
                return Err(anyhow::anyhow!(
                    "--omenchat-moderation-audit-target requires --omenchat-moderation-audit-smoke"
                ));
            }
            if omenchat_moderation_audit_expect_record && !omenchat_moderation_audit_smoke {
                return Err(anyhow::anyhow!(
                    "--omenchat-moderation-audit-expect-record requires --omenchat-moderation-audit-smoke"
                ));
            }
            if omenchat_announcement_rejection_smoke
                && (omenchat_reaction_smoke
                    || omenchat_revision_smoke
                    || omenchat_pin_smoke
                    || omenchat_upload_file.is_some()
                    || omenchat_fetch_upload_filename.is_some()
                    || omenchat_reconnect_ready_file.is_some())
            {
                return Err(anyhow::anyhow!(
                    "--omenchat-announcement-rejection-smoke is an isolated authorization case"
                ));
            }
            if omenchat_announcement_upload_rejection_smoke
                && (omenchat_reaction_smoke
                    || omenchat_revision_smoke
                    || omenchat_pin_smoke
                    || omenchat_upload_file.is_none()
                    || omenchat_fetch_upload_filename.is_some()
                    || omenchat_reconnect_ready_file.is_some())
            {
                return Err(anyhow::anyhow!(
                    "--omenchat-announcement-upload-rejection-smoke requires one upload file and is an isolated authorization case"
                ));
            }
            overrides.ensure_runtime_backend(RuntimeBackendSetting::Reticulum);
            Ok(Self::OmenChatSmoke {
                destination,
                room: omenchat_room,
                message: omenchat_message,
                local_display_name: omenchat_local_display_name,
                announcement_rejection_smoke: omenchat_announcement_rejection_smoke,
                announcement_upload_rejection_smoke: omenchat_announcement_upload_rejection_smoke,
                room_media_policy_upload_rejection_smoke:
                    omenchat_room_media_policy_upload_rejection_smoke,
                slow_mode_rejection_smoke: omenchat_slow_mode_rejection_smoke,
                slow_mode_delta_seconds: omenchat_slow_mode_delta_seconds,
                reaction_smoke: omenchat_reaction_smoke,
                revision_smoke: omenchat_revision_smoke,
                pin_smoke: omenchat_pin_smoke,
                moderation_audit_smoke: omenchat_moderation_audit_smoke,
                moderation_audit_target: omenchat_moderation_audit_target,
                moderation_audit_expect_record: omenchat_moderation_audit_expect_record,
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
        } else if lxmf_topic_capability_probe {
            Ok(Self::LxmfTopicCapabilityProbe {
                output,
                stdout,
                overrides: Box::new(overrides),
            })
        } else if lxmf_invitation_capability_cancel_after_ms.is_some()
            && lxmf_invitation_capability_peer.is_none()
        {
            Err(anyhow::anyhow!(
                "--lxmf-invitation-capability-cancel-after-ms requires --lxmf-invitation-capability-probe"
            ))
        } else if let Some(peer_hash) = lxmf_invitation_capability_peer {
            if peer_hash.len() != 32
                || !peer_hash
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            {
                return Err(anyhow::anyhow!(
                    "LXMF invitation capability peer must be 32 lowercase hexadecimal characters"
                ));
            }
            overrides.ensure_runtime_backend(RuntimeBackendSetting::Reticulum);
            Ok(Self::LxmfInvitationCapabilityProbe {
                peer_hash,
                cancel_after_ms: lxmf_invitation_capability_cancel_after_ms,
                output,
                stdout,
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
        } else if let Some(server_destination) = lxmf_invitation_smoke_server {
            let wait_secs = lxmf_interop_wait_secs.unwrap_or(30);
            if !(1..=300).contains(&wait_secs) {
                return Err(anyhow::anyhow!(
                    "--lxmf-wait for invitation smoke must be between 1 and 300 seconds"
                ));
            }
            if !matches!(
                lxmf_smoke_delivery_mode,
                omenbrowser_rs::messaging::DeliveryMode::Direct
            ) || lxmf_smoke_propagation_node.is_some()
                || lxmf_include_ticket
            {
                return Err(anyhow::anyhow!(
                    "LXMF invitation smoke currently supports direct tokenless delivery without ticket or propagation options"
                ));
            }
            overrides.ensure_runtime_backend(RuntimeBackendSetting::Reticulum);
            Ok(Self::LxmfInvitationSmoke {
                peer_hash: lxmf_smoke_peer,
                server_destination,
                wait_secs,
                output,
                stdout,
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

#[cfg(feature = "chat-client")]
async fn run_lxmf_invitation_smoke_command(
    input: LxmfInvitationSmokeCommandInput,
) -> anyhow::Result<()> {
    let LxmfInvitationSmokeCommandInput {
        peer_hash,
        server_destination,
        wait_secs,
        output,
        stdout,
        overrides,
    } = input;
    let mut config = load_config_for_smoke(overrides.app_root().cloned())
        .context("failed to load LXMF invitation smoke application configuration")?;
    let interface_override = apply_smoke_overrides(&mut config, overrides);
    let diagnostics_dir = config.paths.diagnostics_dir.clone();
    let default_output = output.is_none() && !stdout;
    let mut app = App::try_new(config).context("failed to initialize application services")?;
    app.start_runtime_for_smoke_test_with_interfaces(interface_override)
        .await
        .context("failed to start configured runtime for LXMF invitation smoke")?;
    let report = app
        .native_lxmf_invitation_live_report(peer_hash, server_destination, wait_secs)
        .await
        .context("failed to collect native LXMF invitation smoke report")?;
    let content = serde_json::to_string_pretty(&report)
        .context("failed to render LXMF invitation smoke JSON")?;
    if stdout {
        println!("{content}");
    }
    if let Some(path) = output
        .or_else(|| default_output.then(|| default_lxmf_invitation_report_path(&diagnostics_dir)))
    {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).with_context(|| {
                format!("failed to create output directory {}", parent.display())
            })?;
        }
        std::fs::write(&path, content.as_bytes()).with_context(|| {
            format!("failed to write LXMF invitation report {}", path.display())
        })?;
        if stdout {
            eprintln!("{}", path.display());
        } else {
            println!("{}", path.display());
        }
    }
    Ok(())
}

async fn run_lxmf_invitation_capability_probe_command(
    peer_hash: String,
    cancel_after_ms: Option<u64>,
    output: Option<PathBuf>,
    stdout: bool,
    overrides: SmokeOverrides,
) -> anyhow::Result<()> {
    let mut config = load_config_for_smoke(overrides.app_root().cloned())
        .context("failed to load LXMF invitation capability probe configuration")?;
    let interface_override = apply_smoke_overrides(&mut config, overrides);
    let diagnostics_dir = config.paths.diagnostics_dir.clone();
    let default_output = output.is_none() && !stdout;
    let mut app = App::try_new(config).context("failed to initialize application services")?;
    app.start_runtime_for_smoke_test_with_interfaces(interface_override)
        .await
        .context("failed to start runtime for LXMF invitation capability probe")?;
    let announced = app.runtime.announce_identity().await;
    let cancel = omenbrowser_rs::runtime::CancellationToken::new();
    let started = std::time::Instant::now();
    let probe_future = app
        .runtime
        .probe_lxmf_invitation_capability(&peer_hash, cancel.clone());
    tokio::pin!(probe_future);
    let probe = if let Some(delay_ms) = cancel_after_ms {
        tokio::select! {
            biased;
            () = tokio::time::sleep(std::time::Duration::from_millis(delay_ms)) => {
                cancel.cancel();
                probe_future.await
            },
            result = &mut probe_future => result,
        }
    } else {
        probe_future.await
    };
    let elapsed_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
    let stop = app.runtime.stop_runtime().await;
    let (outcome, supported, error_category) = match probe {
        Ok(omenbrowser_rs::runtime::InvitationCapabilityProbeOutcome::Supported) => {
            ("supported", true, None)
        }
        Ok(omenbrowser_rs::runtime::InvitationCapabilityProbeOutcome::Unsupported) => {
            ("unsupported", false, None)
        }
        Ok(omenbrowser_rs::runtime::InvitationCapabilityProbeOutcome::Conflict) => {
            ("conflict", false, None)
        }
        Ok(omenbrowser_rs::runtime::InvitationCapabilityProbeOutcome::Unknown) => {
            ("unknown", false, None)
        }
        Err(error) => {
            let category = if error.to_string().contains("timed out")
                || error.to_string().contains("unavailable")
            {
                "unavailable_or_timeout"
            } else if error.to_string().contains("cancel") {
                "cancelled"
            } else if error.to_string().contains("identity")
                || error.to_string().contains("correlation")
            {
                "conflict"
            } else {
                "probe_failed"
            };
            ("unknown", false, Some(category))
        }
    };
    let report = serde_json::json!({
        "report": "native_lxmf_invitation_capability_probe",
        "peer_destination_redacted": true,
        "announce_attempted": true,
        "announce_ok": announced.is_ok(),
        "outcome": outcome,
        "supported": supported,
        "error_category": error_category,
        "cancellation_requested": cancel_after_ms.is_some(),
        "cancel_after_ms": cancel_after_ms,
        "elapsed_ms": elapsed_ms,
        "deadline_ms": omenbrowser_rs::runtime::LXMF_INVITATION_CAPABILITY_PROBE_DEADLINE_MS,
        "automatic_retries": 0,
        "invitation_sent": false,
        "shutdown_ok": stop.is_ok(),
    });
    let content = serde_json::to_string_pretty(&report)
        .context("failed to render LXMF invitation capability probe JSON")?;
    if stdout {
        println!("{content}");
    }
    if let Some(path) = output.or_else(|| {
        default_output.then(|| {
            diagnostics_dir.join(format!(
                "native-lxmf-invitation-capability-{}.json",
                current_epoch_millis()
            ))
        })
    }) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).with_context(|| {
                format!("failed to create output directory {}", parent.display())
            })?;
        }
        std::fs::write(&path, content.as_bytes()).with_context(|| {
            format!(
                "failed to write LXMF invitation capability report {}",
                path.display()
            )
        })?;
        if stdout {
            eprintln!("{}", path.display());
        } else {
            println!("{}", path.display());
        }
    }
    Ok(())
}

async fn run_lxmf_topic_capability_probe_command(
    output: Option<PathBuf>,
    stdout: bool,
    overrides: SmokeOverrides,
) -> anyhow::Result<()> {
    #[cfg(not(feature = "native-lxmf-sdk"))]
    {
        let _ = (output, stdout, overrides);
        anyhow::bail!(
            "LXMF topic capability probe unavailable: build with feature native-lxmf-sdk"
        );
    }

    #[cfg(feature = "native-lxmf-sdk")]
    {
        let config = load_config_for_smoke(overrides.app_root().cloned())
            .context("failed to load LXMF topic capability probe configuration")?;
        let diagnostics_dir = config.paths.diagnostics_dir.clone();
        let endpoint = config
            .settings
            .native_lxmf_sdk_rpc_endpoint
            .as_deref()
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "LXMF topic capability probe requires a configured local SDK/RPC endpoint"
                )
            })?;
        let sender =
            omenbrowser_rs::runtime::native_lxmf::client::RpcNativeLxmfSdkSender::new(endpoint);
        let started = std::time::Instant::now();
        let probe = sender
            .probe_topic_capabilities()
            .await
            .context("LXMF topic capability negotiation did not complete")?;
        let elapsed_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
        let readiness = match probe.capabilities.receive_readiness {
            omenbrowser_rs::runtime::lxmf_topics::LxmfTopicReceiveReadiness::ProductAdapterMissing => "product_adapter_missing",
            omenbrowser_rs::runtime::lxmf_topics::LxmfTopicReceiveReadiness::CapabilityAbsent => "capability_absent",
            omenbrowser_rs::runtime::lxmf_topics::LxmfTopicReceiveReadiness::RecoveryUnproven => "recovery_unproven",
            omenbrowser_rs::runtime::lxmf_topics::LxmfTopicReceiveReadiness::TopicEventContractUnproven => "topic_event_contract_unproven",
            omenbrowser_rs::runtime::lxmf_topics::LxmfTopicReceiveReadiness::PublisherAuthenticationUnproven => "publisher_authentication_unproven",
            omenbrowser_rs::runtime::lxmf_topics::LxmfTopicReceiveReadiness::EligibleForReceiveAdapter => "eligible_for_receive_adapter",
        };
        let report = serde_json::json!({
            "report": "external_lxmf_topic_capability_probe",
            "endpoint": probe.endpoint,
            "endpoint_redacted": true,
            "active_contract_version": probe.active_contract_version,
            "topics": probe.capabilities.topics,
            "subscriptions": probe.capabilities.subscriptions,
            "fanout": probe.capabilities.fanout,
            "cursor_replay": probe.capabilities.cursor_replay,
            "async_events": probe.capabilities.async_events,
            "topic_event_contract_proven": probe.capabilities.topic_event_contract_proven,
            "authenticated_publisher_events": probe.capabilities.authenticated_publisher_events,
            "cursor_gap_recovery_proven": probe.capabilities.cursor_gap_recovery_proven,
            "receive_readiness": readiness,
            "upstream_publish_capability": probe.capabilities.may_publish(),
            "publish_adapter_active": false,
            "receive_adapter_active": false,
            "subscribe_calls": 0,
            "publish_calls": 0,
            "automatic_retries": 0,
            "daemon_shutdown_requested": false,
            "deadline_ms": omenbrowser_rs::runtime::native_lxmf::client::NATIVE_LXMF_TOPIC_CAPABILITY_PROBE_DEADLINE_MS,
            "elapsed_ms": elapsed_ms,
        });
        let content = serde_json::to_string_pretty(&report)
            .context("failed to render LXMF topic capability report JSON")?;
        if stdout {
            println!("{content}");
        }
        let default_output = output.is_none() && !stdout;
        if let Some(path) = output.or_else(|| {
            default_output.then(|| {
                diagnostics_dir.join(format!(
                    "external-lxmf-topic-capability-{}.json",
                    current_epoch_millis()
                ))
            })
        }) {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).with_context(|| {
                    format!("failed to create output directory {}", parent.display())
                })?;
            }
            std::fs::write(&path, content.as_bytes()).with_context(|| {
                format!(
                    "failed to write LXMF topic capability report {}",
                    path.display()
                )
            })?;
            if stdout {
                eprintln!("{}", path.display());
            } else {
                println!("{}", path.display());
            }
        }
        Ok(())
    }
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

#[cfg(feature = "chat-client")]
fn default_lxmf_invitation_report_path(diagnostics_dir: &std::path::Path) -> PathBuf {
    diagnostics_dir.join(format!(
        "native-lxmf-invitation-{}.json",
        current_epoch_millis()
    ))
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
        "upstream_software_parity": upstream_software_parity_diagnostics(),
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

#[cfg(feature = "native-lxmf-sdk")]
fn upstream_software_parity_diagnostics() -> serde_json::Value {
    serde_json::json!({
        "available": true,
        "source": "official registry lxmf-sdk 0.9.9",
        "interpretation": "advisory implementation capability inventory; not live interoperability proof",
        "orientation": lxmf_sdk::current_software_parity_orientation(),
    })
}

#[cfg(not(feature = "native-lxmf-sdk"))]
fn upstream_software_parity_diagnostics() -> serde_json::Value {
    serde_json::json!({
        "available": false,
        "interpretation": "lxmf-sdk parity inventory is not compiled in this product profile; it would be advisory capability metadata, not live interoperability proof",
    })
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
            "--omenchat-pin-smoke".to_string(),
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
                local_display_name: "OMENbrowser_rs smoke".into(),
                announcement_rejection_smoke: false,
                announcement_upload_rejection_smoke: false,
                room_media_policy_upload_rejection_smoke: false,
                slow_mode_rejection_smoke: false,
                slow_mode_delta_seconds: None,
                reaction_smoke: true,
                revision_smoke: true,
                pin_smoke: true,
                moderation_audit_smoke: false,
                moderation_audit_target: None,
                moderation_audit_expect_record: false,
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
    fn cli_parses_isolated_moderation_audit_smoke() {
        let parsed = CliCommand::parse([
            "--omenchat-smoke".to_string(),
            FIXTURE_DESTINATION_HASH.to_string(),
            "--omenchat-moderation-audit-smoke".to_string(),
            "--omenchat-moderation-audit-target".to_string(),
            "Target User".to_string(),
            "--omenchat-moderation-audit-expect-record".to_string(),
            "--omenchat-local-display-name".to_string(),
            "Audit Moderator".to_string(),
        ])
        .expect("parse");

        assert!(matches!(
            parsed,
            CliCommand::OmenChatSmoke {
                moderation_audit_smoke: true,
                moderation_audit_target: Some(target),
                moderation_audit_expect_record: true,
                local_display_name,
                reaction_smoke: false,
                revision_smoke: false,
                pin_smoke: false,
                ..
            } if target == "Target User" && local_display_name == "Audit Moderator"
        ));
        assert!(CliCommand::parse([
            "--omenchat-smoke".to_string(),
            FIXTURE_DESTINATION_HASH.to_string(),
            "--omenchat-moderation-audit-smoke".to_string(),
            "--omenchat-pin-smoke".to_string(),
        ])
        .is_err());
        assert!(CliCommand::parse([
            "--omenchat-smoke".to_string(),
            FIXTURE_DESTINATION_HASH.to_string(),
            "--omenchat-moderation-audit-expect-record".to_string(),
        ])
        .is_err());
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

    #[test]
    fn cli_keeps_announcement_rejection_smoke_isolated() {
        let parsed = CliCommand::parse([
            "--omenchat-smoke".to_string(),
            FIXTURE_DESTINATION_HASH.to_string(),
            "--omenchat-announcement-rejection-smoke".to_string(),
        ])
        .expect("parse announcement rejection smoke");
        assert!(matches!(
            parsed,
            CliCommand::OmenChatSmoke {
                announcement_rejection_smoke: true,
                ..
            }
        ));

        let error = CliCommand::parse([
            "--omenchat-smoke".to_string(),
            FIXTURE_DESTINATION_HASH.to_string(),
            "--omenchat-announcement-rejection-smoke".to_string(),
            "--omenchat-reaction-smoke".to_string(),
        ])
        .expect_err("mixed authorization and mutation smoke must fail");
        assert!(error
            .to_string()
            .contains("is an isolated authorization case"));
    }

    #[test]
    fn cli_keeps_slow_mode_rejection_smoke_isolated() {
        let parsed = CliCommand::parse([
            "--omenchat-smoke".to_string(),
            FIXTURE_DESTINATION_HASH.to_string(),
            "--omenchat-slow-mode-rejection-smoke".to_string(),
        ])
        .expect("parse slow-mode rejection smoke");
        assert!(matches!(
            parsed,
            CliCommand::OmenChatSmoke {
                slow_mode_rejection_smoke: true,
                ..
            }
        ));

        let error = CliCommand::parse([
            "--omenchat-smoke".to_string(),
            FIXTURE_DESTINATION_HASH.to_string(),
            "--omenchat-slow-mode-rejection-smoke".to_string(),
            "--omenchat-reaction-smoke".to_string(),
        ])
        .expect_err("mixed slow-mode and reaction smoke must fail");
        assert!(error.to_string().contains("isolated qualification case"));

        let delta = CliCommand::parse([
            "--omenchat-smoke".to_string(),
            FIXTURE_DESTINATION_HASH.to_string(),
            "--omenchat-slow-mode-delta-smoke".to_string(),
            "30".to_string(),
        ])
        .expect("parse slow-mode delta smoke");
        assert!(matches!(
            delta,
            CliCommand::OmenChatSmoke {
                slow_mode_delta_seconds: Some(30),
                ..
            }
        ));
        let mixed = CliCommand::parse([
            "--omenchat-smoke".to_string(),
            FIXTURE_DESTINATION_HASH.to_string(),
            "--omenchat-slow-mode-delta-smoke".to_string(),
            "30".to_string(),
            "--omenchat-slow-mode-rejection-smoke".to_string(),
        ])
        .expect_err("mixed slow-mode qualification cases must fail");
        assert!(mixed.to_string().contains("isolated qualification case"));
    }

    #[test]
    fn cli_requires_one_upload_for_announcement_upload_rejection() {
        let parsed = CliCommand::parse([
            "--omenchat-smoke".to_string(),
            FIXTURE_DESTINATION_HASH.to_string(),
            "--omenchat-announcement-upload-rejection-smoke".to_string(),
            "--omenchat-upload-file".to_string(),
            "/tmp/rejected.bin".to_string(),
        ])
        .expect("parse announcement upload rejection");
        assert!(matches!(
            parsed,
            CliCommand::OmenChatSmoke {
                announcement_upload_rejection_smoke: true,
                ..
            }
        ));

        let error = CliCommand::parse([
            "--omenchat-smoke".to_string(),
            FIXTURE_DESTINATION_HASH.to_string(),
            "--omenchat-announcement-upload-rejection-smoke".to_string(),
        ])
        .expect_err("missing upload must fail");
        assert!(error.to_string().contains("requires one upload file"));
    }

    #[test]
    fn cli_keeps_room_media_policy_upload_rejection_isolated() {
        let parsed = CliCommand::parse([
            "--omenchat-smoke".to_string(),
            FIXTURE_DESTINATION_HASH.to_string(),
            "--omenchat-room-media-policy-upload-rejection-smoke".to_string(),
            "--omenchat-upload-file".to_string(),
            "/tmp/rejected.bin".to_string(),
        ])
        .expect("parse room media-policy upload rejection");
        assert!(matches!(
            parsed,
            CliCommand::OmenChatSmoke {
                room_media_policy_upload_rejection_smoke: true,
                ..
            }
        ));

        for extra in [
            "--omenchat-announcement-upload-rejection-smoke",
            "--omenchat-reaction-smoke",
        ] {
            let mut args = vec![
                "--omenchat-smoke".to_string(),
                FIXTURE_DESTINATION_HASH.to_string(),
                "--omenchat-room-media-policy-upload-rejection-smoke".to_string(),
                "--omenchat-upload-file".to_string(),
                "/tmp/rejected.bin".to_string(),
            ];
            args.push(extra.to_string());
            let error = CliCommand::parse(args).expect_err("invalid mixed qualification must fail");
            assert!(error.to_string().contains("isolated qualification case"));
        }

        let missing_upload = CliCommand::parse([
            "--omenchat-smoke".to_string(),
            FIXTURE_DESTINATION_HASH.to_string(),
            "--omenchat-room-media-policy-upload-rejection-smoke".to_string(),
        ])
        .expect_err("missing upload must fail");
        assert!(missing_upload
            .to_string()
            .contains("requires one upload file"));
    }

    #[test]
    fn upstream_parity_diagnostics_are_advisory_not_live_proof() {
        let diagnostics = upstream_software_parity_diagnostics();
        let interpretation = diagnostics["interpretation"]
            .as_str()
            .expect("parity interpretation");
        assert!(interpretation.contains("not live interoperability proof"));

        #[cfg(feature = "native-lxmf-sdk")]
        {
            assert_eq!(diagnostics["available"], true);
            assert_eq!(diagnostics["orientation"]["advisory"], true);
            assert!(diagnostics["orientation"]["overall"]["inventory"]["total"]
                .as_u64()
                .is_some_and(|total| total > 0));
            assert_eq!(diagnostics["source"], "official registry lxmf-sdk 0.9.9");
        }

        #[cfg(not(feature = "native-lxmf-sdk"))]
        assert_eq!(diagnostics["available"], false);
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
    fn cli_parses_bounded_lxmf_invitation_sender_and_receiver_modes() {
        let receiver = CliCommand::parse([
            "--lxmf-invitation-smoke".to_string(),
            FIXTURE_DESTINATION_HASH.to_string(),
            "--lxmf-wait".to_string(),
            "12".to_string(),
            "--stdout".to_string(),
        ])
        .expect("receive-only parse");
        assert_eq!(
            receiver,
            CliCommand::LxmfInvitationSmoke {
                peer_hash: None,
                server_destination: FIXTURE_DESTINATION_HASH.into(),
                wait_secs: 12,
                output: None,
                stdout: true,
                overrides: Box::new(
                    SmokeOverrides::default()
                        .with_runtime_backend(RuntimeBackendSetting::Reticulum),
                ),
            }
        );

        let sender = CliCommand::parse([
            "--lxmf-invitation-smoke".to_string(),
            FIXTURE_DESTINATION_HASH.to_string(),
            "--send-lxmf-smoke".to_string(),
            FIXTURE_LXMF_PEER_HASH.to_string(),
        ])
        .expect("sender parse");
        assert!(matches!(
            sender,
            CliCommand::LxmfInvitationSmoke {
                peer_hash: Some(peer),
                wait_secs: 30,
                ..
            } if peer == FIXTURE_LXMF_PEER_HASH
        ));

        assert!(CliCommand::parse([
            "--lxmf-invitation-smoke".to_string(),
            FIXTURE_DESTINATION_HASH.to_string(),
            "--lxmf-wait".to_string(),
            "301".to_string(),
        ])
        .is_err());
    }

    #[test]
    fn cli_parses_redacted_lxmf_invitation_capability_probe() {
        let parsed = CliCommand::parse([
            "--lxmf-invitation-capability-probe".to_string(),
            FIXTURE_LXMF_PEER_HASH.to_string(),
            "--lxmf-invitation-capability-cancel-after-ms".to_string(),
            "0".to_string(),
            "--backend".to_string(),
            "reticulum".to_string(),
            "--stdout".to_string(),
        ])
        .expect("capability probe parse");

        assert!(matches!(
            parsed,
            CliCommand::LxmfInvitationCapabilityProbe {
                peer_hash,
                cancel_after_ms: Some(0),
                stdout: true,
                ..
            } if peer_hash == FIXTURE_LXMF_PEER_HASH
        ));
        assert!(CliCommand::parse([
            "--lxmf-invitation-capability-probe".to_string(),
            "ABCDEF".to_string(),
        ])
        .is_err());
        assert!(CliCommand::parse([
            "--lxmf-invitation-capability-probe".to_string(),
            FIXTURE_LXMF_PEER_HASH.to_string(),
            "--lxmf-invitation-capability-cancel-after-ms".to_string(),
            "15001".to_string(),
        ])
        .is_err());
        assert!(CliCommand::parse([
            "--lxmf-invitation-capability-cancel-after-ms".to_string(),
            "0".to_string(),
        ])
        .is_err());
    }

    #[test]
    fn cli_parses_external_lxmf_topic_capability_probe() {
        let parsed = CliCommand::parse([
            "--lxmf-topic-capability-probe".to_string(),
            "--app-root".to_string(),
            "/tmp/isolated-topic-probe".to_string(),
            "--stdout".to_string(),
        ])
        .expect("topic capability probe parse");

        assert!(matches!(
            parsed,
            CliCommand::LxmfTopicCapabilityProbe {
                stdout: true,
                overrides,
                ..
            } if overrides.app_root().is_some_and(|path| path == std::path::Path::new("/tmp/isolated-topic-probe"))
        ));
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
