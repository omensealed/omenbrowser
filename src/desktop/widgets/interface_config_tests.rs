use super::super::interface_status::*;
use crate::interfaces::{InterfaceKind, ReticulumInterfaceProfile};
use crate::runtime::network::InterfaceSample;
use crate::runtime::network::InterfaceSampleState;
use crate::runtime::InterfaceStats;
use crate::workspace::WorkspaceSection;

fn sample(
    profile_id: impl Into<String>,
    name: impl Into<String>,
    state: InterfaceSampleState,
) -> InterfaceSample {
    let attached = matches!(state, InterfaceSampleState::Attached);
    InterfaceSample {
        profile_id: profile_id.into(),
        name: name.into(),
        kind: "tcp_client".into(),
        state,
        enabled: true,
        supported: true,
        attached,
        endpoint: Some("10.0.0.7:4242".into()),
        detail: None,
    }
}

#[test]
fn runtime_interface_sampling_runs_for_interfaces_and_monitoring() {
    assert!(section_needs_runtime_interface_sample(
        WorkspaceSection::Interfaces
    ));
    assert!(section_needs_runtime_interface_sample(
        WorkspaceSection::Monitoring
    ));
    assert!(section_needs_runtime_interface_sample(
        WorkspaceSection::NetworkDoctor
    ));
    assert!(!section_needs_runtime_interface_sample(
        WorkspaceSection::Browser
    ));
    assert!(!section_needs_runtime_interface_sample(
        WorkspaceSection::Logs
    ));
}

#[test]
fn interface_runtime_status_label_reports_sample_visibility() {
    let mut profile = ReticulumInterfaceProfile::tcp_client("iface_test", "GatewayOne");
    profile.target_host = "10.0.0.7".into();

    assert!(interface_runtime_status_label(&profile, None).contains("disconnected"));
    assert_eq!(
        interface_runtime_state_line(&profile, None),
        "state: disconnected | endpoint: 10.0.0.7:4242"
    );

    profile.enabled = false;
    let running_stats = InterfaceStats {
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
        Some("detail: GatewayOne [TcpClient supported enabled]")
    );
    assert_eq!(
        interface_runtime_state_line(&profile, Some(&running_stats)),
        "state: disconnected | endpoint: 10.0.0.7:4242"
    );

    let attached_stats = InterfaceStats {
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
        Some("detail: attached GatewayOne tcp_client 10.0.0.7:4242 ifac=none")
    );
    assert_eq!(
        interface_runtime_state_line(&profile, Some(&attached_stats)),
        "state: connected; auto-retry | endpoint: 10.0.0.7:4242"
    );

    let structured_attached = InterfaceStats {
        available: true,
        reason: Some("sampled".into()),
        interfaces: Vec::new(),
        samples: vec![InterfaceSample {
            detail: Some("GatewayOne tcp_client 10.0.0.7:4242 ifac=none".into()),
            ..sample(
                profile.profile_id.clone(),
                "GatewayOne",
                InterfaceSampleState::Attached,
            )
        }],
    };
    let attached = interface_runtime_status_label(&profile, Some(&structured_attached));
    assert_eq!(attached, "runtime: connected");
    assert_eq!(
        interface_runtime_detail_line(&profile, Some(&structured_attached)).as_deref(),
        Some("detail: GatewayOne tcp_client 10.0.0.7:4242 ifac=none")
    );
    assert_eq!(
        interface_runtime_state_line(&profile, Some(&structured_attached)),
        "state: connected; auto-retry | endpoint: 10.0.0.7:4242"
    );

    let missing_stats = InterfaceStats {
        available: true,
        reason: Some("sampled".into()),
        interfaces: vec!["OtherGateway [TcpClient supported enabled]".into()],
        samples: Vec::new(),
    };
    assert!(
        interface_runtime_status_label(&profile, Some(&missing_stats)).contains("disconnected")
    );

    let stopped_stats = InterfaceStats {
        available: false,
        reason: Some("runtime stopped".into()),
        interfaces: Vec::new(),
        samples: Vec::new(),
    };
    assert!(interface_runtime_status_label(&profile, Some(&stopped_stats)).contains("not running"));
}

#[test]
fn interface_kind_display_labels_are_user_facing() {
    assert_eq!(
        interface_kind_display_label(&InterfaceKind::TcpClient),
        "kind: TCP gateway"
    );
    assert_eq!(
        interface_kind_display_label(&InterfaceKind::TcpServer),
        "kind: TCP listener"
    );
    assert_eq!(
        interface_kind_display_label(&InterfaceKind::RNode),
        "kind: RNode/LoRa"
    );
    assert_eq!(
        interface_kind_display_label(&InterfaceKind::Unknown("custom".into())),
        "kind: custom"
    );
}

#[test]
fn desktop_interface_cards_show_kind_specific_fields() {
    let mut tcp = ReticulumInterfaceProfile::tcp_client("tcp", "TCP");
    tcp.network_name = "meshnet".into();
    tcp.passphrase = "secret".into();
    let mut server = ReticulumInterfaceProfile::tcp_server("server", "Server");
    server.target_host = "127.0.0.1".into();
    server.network_name = "servernet".into();
    server.passphrase = "server secret".into();
    let mut rnode = ReticulumInterfaceProfile::rnode("rn", "RNode");
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
fn interface_config_summary_lines_count_enabled_profiles_by_kind() {
    let tcp = ReticulumInterfaceProfile::tcp_client("tcp", "Gateway");
    let mut rnode = ReticulumInterfaceProfile::rnode("rnode", "Radio");
    rnode.enabled = false;
    let i2p = ReticulumInterfaceProfile::i2p("i2p", "I2P");

    let lines = interface_config_summary_lines(&[tcp, rnode, i2p]);

    assert_eq!(lines[0], "summary: 2/3 enabled, 1 disabled");
    assert_eq!(lines[1], "types: TCP gateways=1 RNode/LoRa=1 I2P=1");
}

#[test]
fn interface_restart_recommendation_only_flags_stale_runtime_samples() {
    let mut profile = ReticulumInterfaceProfile::tcp_client("iface_test", "GatewayOne");
    profile.target_host = "10.0.0.7".into();

    assert!(interface_restart_recommendation_line(&[profile.clone()], None).is_none());

    let attached_stats = InterfaceStats {
        available: true,
        reason: Some("sampled".into()),
        interfaces: Vec::new(),
        samples: vec![sample(
            profile.profile_id.clone(),
            profile.name.clone(),
            InterfaceSampleState::Attached,
        )],
    };
    assert!(
        interface_restart_recommendation_line(&[profile.clone()], Some(&attached_stats)).is_none()
    );

    let stale_stats = InterfaceStats {
        available: true,
        reason: Some("sampled".into()),
        interfaces: vec!["GatewayOne [TcpClient supported enabled]".into()],
        samples: Vec::new(),
    };
    let warning = interface_restart_recommendation_line(&[profile.clone()], Some(&stale_stats))
        .expect("stale runtime/config warning");
    assert!(warning.contains("restart the runtime"));

    profile.enabled = false;
    assert!(interface_restart_recommendation_line(&[profile], Some(&stale_stats)).is_none());
}

#[test]
fn monitoring_interface_reconnect_line_summarizes_native_samples() {
    assert!(monitoring_interface_reconnect_line(None).contains("waiting"));

    let unavailable = InterfaceStats {
        available: false,
        reason: Some("runtime stopped".into()),
        interfaces: Vec::new(),
        samples: Vec::new(),
    };
    assert!(monitoring_interface_reconnect_line(Some(&unavailable)).contains("unavailable"));

    let no_interfaces = InterfaceStats {
        available: true,
        reason: Some("sampled".into()),
        interfaces: Vec::new(),
        samples: Vec::new(),
    };
    assert!(monitoring_interface_reconnect_line(Some(&no_interfaces)).contains("no interfaces"));

    let connected = InterfaceStats {
        available: true,
        reason: Some("sampled".into()),
        interfaces: vec!["Gateway [1] TCPClientInterface | connected=true".into()],
        samples: Vec::new(),
    };
    assert!(monitoring_interface_reconnect_line(Some(&connected)).contains("connected"));

    let retrying = InterfaceStats {
        available: true,
        reason: Some("sampled".into()),
        interfaces: vec!["Gateway [1] TCPClientInterface | connected=false".into()],
        samples: Vec::new(),
    };
    assert!(monitoring_interface_reconnect_line(Some(&retrying)).contains("retrying"));
}

#[test]
fn monitoring_interface_status_lines_prefer_structured_samples() {
    let stats = InterfaceStats {
        available: true,
        reason: Some("sampled".into()),
        interfaces: vec!["legacy raw line".into()],
        samples: vec![
            InterfaceSample {
                detail: Some("GatewayOne tcp_client 10.0.0.7:4242 ifac=none".into()),
                ..sample("gw1", "GatewayOne", InterfaceSampleState::Attached)
            },
            InterfaceSample {
                profile_id: "i2p".into(),
                name: "I2P".into(),
                kind: "i2p".into(),
                state: InterfaceSampleState::Unsupported,
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
    assert!(lines
        .iter()
        .any(|line| line.contains("health: online (1/2 connected)")));
    assert!(lines.iter().any(|line| {
        line.contains("interfaces: connected=1 retrying=0 disabled=0 unsupported=1")
    }));
    assert!(lines.iter().any(|line| {
        line.contains("GatewayOne | tcp_client | connected; auto-retry | 10.0.0.7:4242")
    }));
    assert!(lines
        .iter()
        .any(|line| line.contains("I2P | i2p | unsupported | no endpoint")));
    assert!(!lines.iter().any(|line| line.contains("legacy raw line")));
}

#[test]
fn monitoring_interface_health_line_summarizes_disconnected_samples() {
    let retrying = InterfaceStats {
        available: true,
        reason: Some("sampled".into()),
        interfaces: Vec::new(),
        samples: vec![sample(
            "gw1",
            "GatewayOne",
            InterfaceSampleState::Configured,
        )],
    };
    assert_eq!(
        monitoring_interface_health_line(&retrying),
        "health: retrying (1/1 enabled gateway(s) disconnected)"
    );

    let unavailable = InterfaceStats {
        available: false,
        reason: Some("runtime stopped".into()),
        interfaces: Vec::new(),
        samples: Vec::new(),
    };
    assert_eq!(
        monitoring_interface_health_line(&unavailable),
        "health: runtime unavailable"
    );
}
