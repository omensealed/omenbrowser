use crate::interfaces::{InterfaceKind, ReticulumInterfaceProfile};
use crate::runtime::network::InterfaceSampleState;
use crate::workspace::WorkspaceSection;

pub(in crate::desktop) fn interface_runtime_status_label(
    profile: &ReticulumInterfaceProfile,
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
            InterfaceSampleState::Disabled => {
                return "runtime: disabled by profile".into();
            }
            InterfaceSampleState::Unsupported => {
                return "runtime: unsupported".into();
            }
            InterfaceSampleState::Attached => {
                return "runtime: connected".into();
            }
            InterfaceSampleState::Configured | InterfaceSampleState::Unknown => {}
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
    if attached.is_some() {
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

    if matched_plan.is_some() {
        "runtime: disconnected".into()
    } else {
        "runtime: disconnected; enabled profile is not attached to the native runtime".into()
    }
}

pub(in crate::desktop) fn interface_runtime_detail_line(
    profile: &ReticulumInterfaceProfile,
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
            .map(|reason| format!("detail: {reason}"));
    }

    if let Some(sample) = stats
        .samples
        .iter()
        .find(|sample| sample.profile_id == profile.profile_id || sample.name == profile.name)
    {
        return match sample.state {
            InterfaceSampleState::Disabled => None,
            InterfaceSampleState::Unsupported => Some(format!(
                "detail: {}",
                sample
                    .detail
                    .as_deref()
                    .unwrap_or("native startup is not implemented for this interface")
            )),
            InterfaceSampleState::Attached
            | InterfaceSampleState::Configured
            | InterfaceSampleState::Unknown => Some(format!(
                "detail: {}",
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
        return Some(format!("detail: {line}"));
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
        .map(|line| format!("detail: {line}"))
}

pub(in crate::desktop) fn interface_runtime_state_line(
    profile: &ReticulumInterfaceProfile,
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
            interface_sample_state_label(&InterfaceSampleState::Attached)
        );
    }

    format!("state: disconnected | endpoint: {endpoint}")
}

pub(in crate::desktop) fn section_needs_runtime_interface_sample(
    section: WorkspaceSection,
) -> bool {
    matches!(
        section,
        WorkspaceSection::Interfaces
            | WorkspaceSection::Monitoring
            | WorkspaceSection::NetworkDoctor
    )
}

pub(in crate::desktop) fn monitoring_interface_status_lines(
    stats: &crate::runtime::InterfaceStats,
) -> Vec<String> {
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
    lines.push(monitoring_interface_health_line(stats));

    if !stats.samples.is_empty() {
        let connected = stats
            .samples
            .iter()
            .filter(|sample| sample.state == InterfaceSampleState::Attached)
            .count();
        let retrying = stats
            .samples
            .iter()
            .filter(|sample| sample.state == InterfaceSampleState::Configured)
            .count();
        let disabled = stats
            .samples
            .iter()
            .filter(|sample| sample.state == InterfaceSampleState::Disabled)
            .count();
        let unsupported = stats
            .samples
            .iter()
            .filter(|sample| sample.state == InterfaceSampleState::Unsupported)
            .count();
        lines.push(format!(
            "interfaces: connected={connected} retrying={retrying} disabled={disabled} unsupported={unsupported}"
        ));
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

pub(in crate::desktop) fn monitoring_interface_health_line(
    stats: &crate::runtime::InterfaceStats,
) -> String {
    if !stats.available {
        return "health: runtime unavailable".into();
    }
    if !stats.samples.is_empty() {
        let total = stats.samples.len();
        let connected = stats
            .samples
            .iter()
            .filter(|sample| sample.state == InterfaceSampleState::Attached)
            .count();
        let retrying = stats
            .samples
            .iter()
            .filter(|sample| sample.state == InterfaceSampleState::Configured)
            .count();
        if connected > 0 {
            return format!("health: online ({connected}/{total} connected)");
        }
        if retrying > 0 {
            return format!(
                "health: retrying ({retrying}/{total} enabled gateway(s) disconnected)"
            );
        }
        return format!("health: offline ({total} interface sample(s), none connected)");
    }

    if stats.interfaces.is_empty() {
        return "health: no interfaces reported".into();
    }
    let joined = stats.interfaces.join("\n").to_ascii_lowercase();
    if joined.contains("connected=true")
        || joined.contains("connected=yes")
        || joined.contains("connected=connected")
        || joined.contains("connected=online")
        || joined.contains("attached ")
    {
        return "health: online".into();
    }
    if joined.contains("connected=false")
        || joined.contains("disconnected")
        || joined.contains("retry")
        || joined.contains("connection error")
        || joined.contains("connection closed")
    {
        return "health: retrying/offline".into();
    }
    "health: reported".into()
}

pub(in crate::desktop) fn interface_sample_state_label(
    state: &InterfaceSampleState,
) -> &'static str {
    match state {
        InterfaceSampleState::Disabled => "disabled",
        InterfaceSampleState::Unsupported => "unsupported",
        InterfaceSampleState::Attached => "connected; auto-retry",
        InterfaceSampleState::Configured => "disconnected",
        InterfaceSampleState::Unknown => "unknown",
    }
}

pub(in crate::desktop) fn monitoring_interface_reconnect_line(
    stats: Option<&crate::runtime::InterfaceStats>,
) -> String {
    let Some(stats) = stats else {
        return "interface reconnect: waiting for native status".into();
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
        .any(|sample| sample.state == InterfaceSampleState::Attached)
    {
        return "interface reconnect: connected; TCP gateways auto-retry after drops".into();
    }
    if stats
        .samples
        .iter()
        .any(|sample| sample.state == InterfaceSampleState::Configured)
    {
        return "interface reconnect: enabled gateway disconnected; restart after interface edits"
            .into();
    }

    let joined = stats.interfaces.join("\n").to_ascii_lowercase();
    if joined.contains("connected=true")
        || joined.contains("connected=yes")
        || joined.contains("connected=connected")
        || joined.contains("connected=online")
    {
        return "interface reconnect: connected; TCP gateways auto-retry after drops".into();
    }
    if joined.contains("connected=false")
        || joined.contains("disconnected")
        || joined.contains("couldn't connect")
        || joined.contains("connection error")
        || joined.contains("connection closed")
    {
        return "interface reconnect: gateway offline/retrying; TCP clients auto-retry".into();
    }

    "interface reconnect: interfaces reported; verify connected=true in detailed lines".into()
}

pub(in crate::desktop) fn interface_kind_display_label(kind: &InterfaceKind) -> String {
    match kind {
        InterfaceKind::Auto => "kind: auto".into(),
        InterfaceKind::TcpClient => "kind: TCP gateway".into(),
        InterfaceKind::TcpServer => "kind: TCP listener".into(),
        InterfaceKind::I2p => "kind: I2P".into(),
        InterfaceKind::RNode => "kind: RNode/LoRa".into(),
        InterfaceKind::Unknown(kind) => format!("kind: {kind}"),
    }
}

pub(in crate::desktop) fn interface_restart_recommendation_line(
    profiles: &[ReticulumInterfaceProfile],
    stats: Option<&crate::runtime::InterfaceStats>,
) -> Option<String> {
    let stats = stats?;
    if !stats.available {
        return None;
    }

    let stale_enabled = profiles
        .iter()
        .filter(|profile| profile.enabled)
        .any(|profile| {
            interface_runtime_status_label(profile, Some(stats)) != "runtime: connected"
        });

    if stale_enabled {
        Some(
            "restart recommended: enabled interface changes are not active in the running native runtime"
                .into(),
        )
    } else {
        None
    }
}

pub(in crate::desktop) fn desktop_interface_detail_lines(
    profile: &ReticulumInterfaceProfile,
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

pub(in crate::desktop) fn interface_config_preview_lines(preview: &str) -> Vec<String> {
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

pub(in crate::desktop) fn interface_config_summary_lines(
    profiles: &[ReticulumInterfaceProfile],
) -> Vec<String> {
    let total = profiles.len();
    let enabled = profiles.iter().filter(|profile| profile.enabled).count();
    let disabled = total.saturating_sub(enabled);
    let tcp_gateways = profiles
        .iter()
        .filter(|profile| profile.kind == InterfaceKind::TcpClient)
        .count();
    let rnodes = profiles
        .iter()
        .filter(|profile| profile.kind == InterfaceKind::RNode)
        .count();
    let i2p = profiles
        .iter()
        .filter(|profile| profile.kind == InterfaceKind::I2p)
        .count();
    vec![
        format!("summary: {enabled}/{total} enabled, {disabled} disabled"),
        format!("types: TCP gateways={tcp_gateways} RNode/LoRa={rnodes} I2P={i2p}"),
    ]
}
