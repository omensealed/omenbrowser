use omenbrowser_rs::config::AppPaths;
use omenbrowser_rs::runtime::adapter::build_runtime;
use omenbrowser_rs::runtime::network::RuntimeBackendName;
use omenbrowser_rs::storage::settings::{AppSettings, RuntimeBackendSetting};

fn paths(name: &str) -> AppPaths {
    AppPaths::from_root(std::env::temp_dir().join(format!(
        "omenbrowser-rs-runtime-factory-{name}-{}",
        std::process::id()
    )))
}

#[test]
fn auto_runtime_prefers_native_when_compiled_else_mock() {
    let settings = AppSettings::default();
    let decision = build_runtime(&settings, &paths("auto")).expect("runtime");

    #[cfg(feature = "native-reticulum")]
    assert_eq!(decision.backend, RuntimeBackendName::Reticulum);
    #[cfg(not(feature = "native-reticulum"))]
    assert_eq!(decision.backend, RuntimeBackendName::Mock);
    assert!(decision.warning.is_some());
}

#[test]
fn explicit_mock_runtime_is_available() {
    let settings = AppSettings {
        runtime_backend: RuntimeBackendSetting::Mock,
        ..AppSettings::default()
    };
    let decision = build_runtime(&settings, &paths("mock")).expect("runtime");

    assert_eq!(decision.backend, RuntimeBackendName::Mock);
    assert!(decision.warning.is_none());
}

#[cfg(not(feature = "native-reticulum"))]
#[test]
fn native_requested_without_feature_is_clear_error() {
    let settings = AppSettings {
        runtime_backend: RuntimeBackendSetting::Reticulum,
        ..AppSettings::default()
    };
    let error = build_runtime(&settings, &paths("native-missing")).expect_err("error");

    assert!(error
        .to_string()
        .contains("feature native-reticulum is not compiled"));
}

#[cfg(feature = "native-reticulum")]
#[test]
fn native_requested_with_feature_builds_identity_config_adapter() {
    let settings = AppSettings {
        runtime_backend: RuntimeBackendSetting::Reticulum,
        ..AppSettings::default()
    };
    let decision = build_runtime(&settings, &paths("native-present")).expect("runtime");

    assert_eq!(decision.backend, RuntimeBackendName::Reticulum);
    assert!(decision
        .warning
        .as_deref()
        .unwrap_or_default()
        .contains("native Reticulum runtime selected"));
}
