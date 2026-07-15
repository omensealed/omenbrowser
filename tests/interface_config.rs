use std::path::PathBuf;

use omenbrowser_rs::interfaces::{
    render_config, InterfaceConfigService, InterfaceKind, ReticulumInterfaceProfile,
    GATEWAY_PRESETS_MAX_BYTES, INTERFACE_PROFILES_MAX_BYTES, INTERFACE_PROFILES_MAX_ITEMS,
    RETICULUM_CONFIG_MAX_BYTES,
};

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "omenbrowser-rs-interfaces-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

#[test]
fn interface_profile_defaults_match_python_model() {
    let profile = ReticulumInterfaceProfile::tcp_client("tcp", "TCP");

    assert_eq!(profile.target_port, 4242);
    assert_eq!(profile.frequency, 867_200_000);
    assert_eq!(profile.bandwidth, 125_000);
    assert_eq!(profile.spreading_factor, 8);
    assert_eq!(profile.coding_rate, 5);
}

#[test]
fn renders_tcp_i2p_and_rnode_profiles() {
    let mut tcp = ReticulumInterfaceProfile::tcp_client("tcp", "TCP");
    tcp.target_host = "node.example".into();
    tcp.network_name = "meshnet".into();
    tcp.passphrase = "secret phrase".into();
    let mut i2p = ReticulumInterfaceProfile::i2p("i2p", "I2P");
    i2p.peers = vec!["peer.b32.i2p".into()];
    let mut rnode = ReticulumInterfaceProfile::rnode("rn", "RNode");
    rnode.device_port = "/dev/ttyUSB0".into();

    let rendered = render_config(&[tcp, i2p, rnode]);

    assert!(rendered.contains("type = TCPClientInterface"));
    assert!(rendered.contains("target_host = node.example"));
    assert!(rendered.contains("network_name = meshnet"));
    assert!(rendered.contains("passphrase = secret phrase"));
    assert!(rendered.contains("type = I2PInterface"));
    assert!(rendered.contains("peers = peer.b32.i2p"));
    assert!(rendered.contains("type = RNodeInterface"));
    assert!(rendered.contains("port = /dev/ttyUSB0"));
}

#[test]
fn renders_tcp_server_listen_ip_and_ifac() {
    let mut server = ReticulumInterfaceProfile::tcp_server("server", "Server");
    server.target_host = "127.0.0.1".into();
    server.target_port = 4243;
    server.network_name = "meshnet".into();
    server.passphrase = "secret phrase".into();

    let rendered = render_config(&[server]);

    assert!(rendered.contains("type = TCPServerInterface"));
    assert!(rendered.contains("listen_ip = 127.0.0.1"));
    assert!(rendered.contains("listen_port = 4243"));
    assert!(rendered.contains("network_name = meshnet"));
    assert!(rendered.contains("passphrase = secret phrase"));
}

#[test]
fn service_persists_toggles_and_applies_config() {
    let root = temp_dir("service");
    let mut service = InterfaceConfigService::new(
        root.join("interfaces.json"),
        root.join("reticulum"),
        root.join("gateways.json"),
    )
    .expect("service");
    let profile = service.create(InterfaceKind::I2p).expect("create");

    let toggled = service
        .toggle_connectable(&profile.profile_id)
        .expect("toggle")
        .expect("profile");
    let config_path = service.apply().expect("apply");
    let reloaded = InterfaceConfigService::new(
        root.join("interfaces.json"),
        root.join("reticulum"),
        root.join("gateways.json"),
    )
    .expect("reload");

    assert!(!toggled.connectable);
    assert!(config_path.exists());
    assert!(reloaded.get(&profile.profile_id).is_some());
}

#[test]
fn service_create_uses_unique_profile_ids() {
    let root = temp_dir("unique-ids");
    let mut service = InterfaceConfigService::new(
        root.join("interfaces.json"),
        root.join("reticulum"),
        root.join("gateways.json"),
    )
    .expect("service");

    let first = service.create(InterfaceKind::TcpClient).expect("first");
    let second = service.create(InterfaceKind::TcpClient).expect("second");

    assert_ne!(first.profile_id, second.profile_id);
    assert_eq!(
        service
            .list_profiles()
            .iter()
            .filter(|profile| profile.profile_id == first.profile_id)
            .count(),
        1
    );
}

#[test]
fn gateway_profile_uses_presets() {
    let root = temp_dir("gateway");
    let mut service = InterfaceConfigService::new(
        root.join("interfaces.json"),
        root.join("reticulum"),
        root.join("gateways.json"),
    )
    .expect("service");

    let profile = service
        .create_gateway_profile("rmap")
        .expect("gateway")
        .expect("profile");

    assert_eq!(profile.kind, InterfaceKind::TcpClient);
    assert_eq!(profile.target_host, "rmap.world");
}

#[test]
fn gateway_profile_reads_user_interface_gateways_file() {
    let root = temp_dir("interface-gateway-presets");
    std::fs::write(
        root.join("interface_gateways.json"),
        r#"{
  "gateways": [
    { "id": "chi_no", "name": "CHI-NO", "host": "rns.chicagonomad.net", "port": 4242 },
    { "id": "lwh", "name": "LWH", "host": "lazyworkhorse.net", "port": 4242 }
  ]
}"#,
    )
    .expect("write user presets");
    let mut service = InterfaceConfigService::new(
        root.join("interfaces.json"),
        root.join("reticulum"),
        root.join("interface_gateways.json"),
    )
    .expect("service");

    let presets = service.gateway_presets().expect("presets");
    assert_eq!(presets.len(), 2);
    assert!(presets.iter().any(|preset| preset.id == "chi_no"));

    let profile = service
        .create_gateway_profile("lwh")
        .expect("gateway")
        .expect("profile");

    assert_eq!(profile.name, "LWH");
    assert_eq!(profile.target_host, "lazyworkhorse.net");
    assert_eq!(profile.target_port, 4242);
}

#[test]
fn gateway_presets_migrate_legacy_gateways_file() {
    let root = temp_dir("legacy-gateway-presets");
    std::fs::write(
        root.join("gateways.json"),
        r#"{
  "gateways": [
    { "id": "custom", "name": "Custom", "host": "gateway.example", "port": 42420 }
  ]
}"#,
    )
    .expect("write legacy presets");
    let service = InterfaceConfigService::new(
        root.join("interfaces.json"),
        root.join("reticulum"),
        root.join("interface_gateways.json"),
    )
    .expect("service");

    let presets = service.gateway_presets().expect("presets");
    assert_eq!(presets.len(), 1);
    assert_eq!(presets[0].id, "custom");
    assert!(root.join("interface_gateways.json").exists());
    assert!(root.join("gateways.json").exists());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(root.join("interface_gateways.json"))
                .expect("migrated preset metadata")
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
}

#[test]
fn oversized_gateway_preset_file_is_rejected_without_mutation() {
    let root = temp_dir("oversized-gateways");
    let path = root.join("gateways.json");
    std::fs::File::create(&path)
        .and_then(|file| file.set_len(GATEWAY_PRESETS_MAX_BYTES + 1))
        .expect("write oversized gateways");

    let error = InterfaceConfigService::new(
        root.join("interfaces.json"),
        root.join("reticulum"),
        path.clone(),
    )
    .expect_err("reject oversized gateways");

    assert!(error.to_string().contains("1048576 byte limit"));
    assert_eq!(
        std::fs::metadata(path).expect("gateway metadata").len(),
        GATEWAY_PRESETS_MAX_BYTES + 1
    );
}

#[test]
fn render_config_disambiguates_duplicate_section_names() {
    let first = ReticulumInterfaceProfile::tcp_client("first", "Gateway");
    let second = ReticulumInterfaceProfile::tcp_client("second", "Gateway");

    let rendered = render_config(&[first, second]);

    assert!(rendered.contains("[[Gateway]]"));
    assert!(rendered.contains("[[Gateway second]]"));
}

#[test]
fn apply_preserves_existing_network_identity_and_all_profiles() {
    let root = temp_dir("preserve-identity");
    let mut service = InterfaceConfigService::new(
        root.join("interfaces.json"),
        root.join("reticulum"),
        root.join("gateways.json"),
    )
    .expect("service");
    let config_path = root.join("reticulum").join("config");
    std::fs::write(
        &config_path,
        "# existing\n[reticulum]\nnetwork_identity = /tmp/omen/identity\n\n[interfaces]\n",
    )
    .expect("seed existing config");

    service
        .create_gateway_profile("rmap")
        .expect("rmap")
        .expect("rmap profile");
    service
        .create_gateway_profile("wns")
        .expect("wns")
        .expect("wns profile");
    service.apply().expect("apply");
    let rendered = std::fs::read_to_string(config_path).expect("rendered config");

    assert!(rendered.contains("network_identity = /tmp/omen/identity"));
    assert!(rendered.contains("[[RMAP]]"));
    assert!(rendered.contains("[[WNS]]"));
    assert!(rendered.contains("[[Default Interface]]"));
}

#[test]
fn managed_apply_generates_unique_stable_instance_name() {
    let first_root = temp_dir("instance-first");
    let second_root = temp_dir("instance-second");
    let first = InterfaceConfigService::new(
        first_root.join("interfaces.json"),
        first_root.join("reticulum"),
        first_root.join("gateways.json"),
    )
    .expect("first service");
    let second = InterfaceConfigService::new(
        second_root.join("interfaces.json"),
        second_root.join("reticulum"),
        second_root.join("gateways.json"),
    )
    .expect("second service");

    let first_before = std::fs::read_to_string(first.config_path()).expect("first config");
    let second_before = std::fs::read_to_string(second.config_path()).expect("second config");
    let first_name = reticulum_config_value(&first_before, "instance_name").expect("first name");
    let second_name = reticulum_config_value(&second_before, "instance_name").expect("second name");

    first.apply().expect("first reapply");
    let first_after = std::fs::read_to_string(first.config_path()).expect("first config reapplied");

    assert!(first_name.starts_with("omenbrowser_rs_"));
    assert_eq!(first_name.len(), "omenbrowser_rs_".len() + 6);
    assert!(first_name["omenbrowser_rs_".len()..]
        .chars()
        .all(|ch| ch.is_ascii_hexdigit()));
    assert_ne!(first_name, second_name);
    assert_eq!(
        Some(first_name),
        reticulum_config_value(&first_after, "instance_name")
    );
}

#[test]
fn managed_apply_preserves_custom_instance_name() {
    let root = temp_dir("preserve-instance");
    let service = InterfaceConfigService::new(
        root.join("interfaces.json"),
        root.join("reticulum"),
        root.join("gateways.json"),
    )
    .expect("service");
    let config_path = root.join("reticulum").join("config");
    std::fs::write(
        &config_path,
        "# existing\n[reticulum]\ninstance_name = custom_browser_instance\nnetwork_identity = /tmp/omen/identity\n\n[interfaces]\n",
    )
    .expect("seed existing config");

    service.apply().expect("apply");
    let rendered = std::fs::read_to_string(config_path).expect("rendered config");

    assert_eq!(
        reticulum_config_value(&rendered, "instance_name").as_deref(),
        Some("custom_browser_instance")
    );
    assert!(rendered.contains("network_identity = /tmp/omen/identity"));
}

#[test]
fn exact_profile_file_limit_is_accepted() {
    let root = temp_dir("exact-profile-limit");
    let profile = ReticulumInterfaceProfile::auto("exact", "Exact");
    let mut raw = serde_json::to_vec(&serde_json::json!({"profiles": [profile]}))
        .expect("serialize profiles");
    raw.resize(INTERFACE_PROFILES_MAX_BYTES as usize, b' ');
    std::fs::write(root.join("interfaces.json"), raw).expect("write exact profiles");

    let service = InterfaceConfigService::new(
        root.join("interfaces.json"),
        root.join("reticulum"),
        root.join("gateways.json"),
    )
    .expect("load exact profiles");

    assert!(service.get("exact").is_some());
}

#[test]
fn oversized_profile_file_is_rejected_without_mutation() {
    let root = temp_dir("oversized-profiles");
    let path = root.join("interfaces.json");
    std::fs::File::create(&path)
        .and_then(|file| file.set_len(INTERFACE_PROFILES_MAX_BYTES + 1))
        .expect("write oversized profiles");

    let error = InterfaceConfigService::new(
        path.clone(),
        root.join("reticulum"),
        root.join("gateways.json"),
    )
    .expect_err("reject oversized profiles");

    assert!(error.to_string().contains("2097152 byte limit"));
    assert_eq!(
        std::fs::metadata(path).expect("profile metadata").len(),
        INTERFACE_PROFILES_MAX_BYTES + 1
    );
}

#[test]
fn excessive_profile_count_is_rejected_without_rewrite() {
    let root = temp_dir("profile-count");
    let path = root.join("interfaces.json");
    let profiles = (0..=INTERFACE_PROFILES_MAX_ITEMS)
        .map(|index| ReticulumInterfaceProfile::auto(format!("profile-{index}"), "Auto"))
        .collect::<Vec<_>>();
    let raw = serde_json::to_vec(&serde_json::json!({"profiles": profiles}))
        .expect("serialize excessive profiles");
    std::fs::write(&path, &raw).expect("write excessive profiles");

    let error = InterfaceConfigService::new(
        path.clone(),
        root.join("reticulum"),
        root.join("gateways.json"),
    )
    .expect_err("reject excessive profiles");

    assert!(error.to_string().contains("64 item limit"));
    assert_eq!(std::fs::read(path).expect("read source"), raw);
}

#[cfg(unix)]
#[test]
fn profile_symlink_is_rejected_without_touching_referent() {
    use std::os::unix::fs::symlink;

    let root = temp_dir("profile-symlink");
    let path = root.join("interfaces.json");
    let referent = root.join("referent.json");
    let raw = b"{\"profiles\":[]}";
    std::fs::write(&referent, raw).expect("write referent");
    symlink(&referent, &path).expect("create symlink");

    let error = InterfaceConfigService::new(
        path.clone(),
        root.join("reticulum"),
        root.join("gateways.json"),
    )
    .expect_err("reject profile symlink");

    assert!(error.to_string().contains("regular file"));
    assert!(std::fs::symlink_metadata(path)
        .expect("symlink metadata")
        .file_type()
        .is_symlink());
    assert_eq!(std::fs::read(referent).expect("read referent"), raw);
}

#[test]
fn failed_profile_save_restores_previous_in_memory_profiles() {
    let root = temp_dir("profile-rollback");
    let profiles_path = root.join("interfaces.json");
    let mut service = InterfaceConfigService::new(
        profiles_path.clone(),
        root.join("reticulum"),
        root.join("gateways.json"),
    )
    .expect("service");
    let previous = service.list_profiles().to_vec();
    std::fs::remove_file(&profiles_path).expect("remove profiles file");
    std::fs::create_dir(&profiles_path).expect("replace target with directory");

    let error = service
        .create(InterfaceKind::TcpClient)
        .expect_err("reject unsafe profile target");

    assert!(error.to_string().contains("regular file"));
    assert_eq!(service.list_profiles(), previous);
}

#[test]
fn unsafe_control_character_update_is_rejected_without_mutation() {
    let root = temp_dir("control-injection");
    let mut service = InterfaceConfigService::new(
        root.join("interfaces.json"),
        root.join("reticulum"),
        root.join("gateways.json"),
    )
    .expect("service");
    let mut profile = service.list_profiles()[0].clone();
    let profile_id = profile.profile_id.clone();
    let previous = profile.clone();
    profile.name = "Injected\n[[Hostile]]".into();

    let error = service
        .update(profile)
        .expect_err("reject control injection");

    assert!(error.to_string().contains("unsafe control character"));
    assert_eq!(service.get(&profile_id), Some(&previous));
}

#[cfg(unix)]
#[test]
fn gateway_preset_symlink_is_rejected_without_touching_referent() {
    use std::os::unix::fs::symlink;

    let root = temp_dir("gateway-symlink");
    let path = root.join("gateways.json");
    let referent = root.join("gateway-referent.json");
    let raw = b"{\"gateways\":[]}";
    std::fs::write(&referent, raw).expect("write referent");
    symlink(&referent, &path).expect("create symlink");

    let error = InterfaceConfigService::new(
        root.join("interfaces.json"),
        root.join("reticulum"),
        path.clone(),
    )
    .expect_err("reject gateway symlink");

    assert!(error.to_string().contains("regular file"));
    assert!(std::fs::symlink_metadata(path)
        .expect("symlink metadata")
        .file_type()
        .is_symlink());
    assert_eq!(std::fs::read(referent).expect("read referent"), raw);
}

#[cfg(unix)]
#[test]
fn apply_rejects_config_symlink_without_touching_referent() {
    use std::os::unix::fs::symlink;

    let root = temp_dir("config-symlink");
    let service = InterfaceConfigService::new(
        root.join("interfaces.json"),
        root.join("reticulum"),
        root.join("gateways.json"),
    )
    .expect("service");
    let config = service.config_path().clone();
    let referent = root.join("config-referent");
    std::fs::remove_file(&config).expect("remove config");
    std::fs::write(&referent, b"referent secret").expect("write referent");
    symlink(&referent, &config).expect("create config symlink");

    let error = service.apply().expect_err("reject config symlink");

    assert!(error.to_string().contains("regular file"));
    assert_eq!(
        std::fs::read(referent).expect("read referent"),
        b"referent secret"
    );
}

#[test]
fn apply_rejects_oversized_existing_config_without_rewrite() {
    let root = temp_dir("oversized-config");
    let service = InterfaceConfigService::new(
        root.join("interfaces.json"),
        root.join("reticulum"),
        root.join("gateways.json"),
    )
    .expect("service");
    let config = service.config_path().clone();
    std::fs::File::create(&config)
        .and_then(|file| file.set_len(RETICULUM_CONFIG_MAX_BYTES + 1))
        .expect("write oversized config");

    let error = service.apply().expect_err("reject oversized config");

    assert!(error.to_string().contains("1048576 byte limit"));
    assert_eq!(
        std::fs::metadata(config).expect("config metadata").len(),
        RETICULUM_CONFIG_MAX_BYTES + 1
    );
}

fn reticulum_config_value(config: &str, expected_key: &str) -> Option<String> {
    let mut in_reticulum = false;
    for line in config.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            in_reticulum = trimmed == "[reticulum]";
            continue;
        }
        if !in_reticulum {
            continue;
        }
        let Some((key, value)) = trimmed.split_once('=') else {
            continue;
        };
        if key.trim() == expected_key {
            return Some(value.trim().to_string());
        }
    }
    None
}
