use std::path::PathBuf;

use omenbrowser_rs::config::AppPaths;
use omenbrowser_rs::runtime::native::config::{NativeRuntimeConfig, NativeRuntimeMode};
use omenbrowser_rs::storage::settings::{AppSettings, ReticulumInstanceMode};

#[test]
fn native_config_uses_app_paths_without_secret_material() {
    let paths = AppPaths::from_root(PathBuf::from("/tmp/omen-native"));
    let config = NativeRuntimeConfig::from_paths(&paths);

    assert_eq!(config.reticulum_config_dir, paths.reticulum_config_dir);
    assert_eq!(config.reticulum_storage_dir, paths.reticulum_storage_dir);
    assert_eq!(config.attachments_dir, paths.attachments_dir);
    assert!(config.identity_path.is_none());
    assert_eq!(config.instance_mode, NativeRuntimeMode::Managed);
    assert!(config.announce_on_start);
    assert_eq!(config.request_timeout_secs, 30);
    assert!(config.native_lxmf_sdk_rpc_endpoint.is_none());
}

#[test]
fn native_config_maps_settings_and_redacts_identity_path_in_debug() {
    let paths = AppPaths::from_root(PathBuf::from("/tmp/omen-native"));
    let settings = AppSettings {
        reticulum_config_path: Some(PathBuf::from("/tmp/custom-reticulum")),
        identity_path: Some(PathBuf::from("/tmp/secret/default_identity")),
        reticulum_instance_mode: ReticulumInstanceMode::External,
        ..AppSettings::default()
    };
    let config = NativeRuntimeConfig::from_settings_and_paths(&settings, &paths);

    assert_eq!(
        config.reticulum_config_dir,
        PathBuf::from("/tmp/custom-reticulum")
    );
    assert_eq!(config.identity_path, settings.identity_path);
    assert_eq!(config.instance_mode, NativeRuntimeMode::External);
    assert_eq!(config.identity_hint().as_deref(), Some("default_identity"));
    let debug = format!("{config:?}");
    assert!(debug.contains("default_identity"));
    assert!(!debug.contains("/tmp/secret"));
}

#[test]
fn native_config_maps_and_redacts_lxmf_sdk_rpc_endpoint() {
    let paths = AppPaths::from_root(PathBuf::from("/tmp/omen-native"));
    let settings = AppSettings {
        native_lxmf_sdk_rpc_endpoint: Some("  tcp://127.0.0.1:37428/rpc  ".into()),
        ..AppSettings::default()
    };

    let config = NativeRuntimeConfig::from_settings_and_paths(&settings, &paths);

    assert_eq!(
        config.native_lxmf_sdk_rpc_endpoint.as_deref(),
        Some("tcp://127.0.0.1:37428/rpc")
    );
    let debug = format!("{config:?}");
    assert!(debug.contains("native_lxmf_sdk_rpc_endpoint"));
    assert!(debug.contains("<configured>"));
    assert!(!debug.contains("127.0.0.1:37428"));
}

#[test]
fn native_config_treats_blank_lxmf_sdk_rpc_endpoint_as_missing() {
    let paths = AppPaths::from_root(PathBuf::from("/tmp/omen-native"));
    let settings = AppSettings {
        native_lxmf_sdk_rpc_endpoint: Some("   ".into()),
        ..AppSettings::default()
    };

    let config = NativeRuntimeConfig::from_settings_and_paths(&settings, &paths);

    assert!(config.native_lxmf_sdk_rpc_endpoint.is_none());
}
