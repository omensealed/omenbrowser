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
