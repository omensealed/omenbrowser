use std::sync::Arc;

use crate::config::AppPaths;
use crate::error::{AppError, AppResult};
use crate::runtime::network::{MockNetworkRuntime, NetworkRuntime, RuntimeBackendName};
use crate::storage::settings::{AppSettings, RuntimeBackendSetting};

#[derive(Clone)]
pub struct RuntimeFactoryDecision {
    pub runtime: Arc<dyn NetworkRuntime>,
    pub backend: RuntimeBackendName,
    pub warning: Option<String>,
}

impl std::fmt::Debug for RuntimeFactoryDecision {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RuntimeFactoryDecision")
            .field("backend", &self.backend)
            .field("warning", &self.warning)
            .finish_non_exhaustive()
    }
}

pub fn build_runtime(
    settings: &AppSettings,
    paths: &AppPaths,
) -> AppResult<RuntimeFactoryDecision> {
    match settings.runtime_backend {
        RuntimeBackendSetting::Mock => Ok(mock_decision(None)),
        RuntimeBackendSetting::Auto => build_auto_runtime(settings, paths),
        RuntimeBackendSetting::Reticulum => build_native_reticulum(settings, paths),
        RuntimeBackendSetting::Bridge => Err(AppError::Unsupported(
            "bridge runtime is not implemented; use mock or native-reticulum".into(),
        )),
    }
}

#[cfg(feature = "native-reticulum")]
fn build_auto_runtime(
    settings: &AppSettings,
    paths: &AppPaths,
) -> AppResult<RuntimeFactoryDecision> {
    let mut decision = build_native_reticulum(settings, paths)?;
    decision.warning = Some(
        "auto runtime selected native Reticulum; use Settings -> Mock for offline demo pages"
            .into(),
    );
    Ok(decision)
}

#[cfg(not(feature = "native-reticulum"))]
fn build_auto_runtime(
    _settings: &AppSettings,
    _paths: &AppPaths,
) -> AppResult<RuntimeFactoryDecision> {
    Ok(mock_decision(Some(
        "auto runtime selected mock backend because native-reticulum is not compiled".into(),
    )))
}

fn mock_decision(warning: Option<String>) -> RuntimeFactoryDecision {
    RuntimeFactoryDecision {
        runtime: Arc::new(MockNetworkRuntime::default()),
        backend: RuntimeBackendName::Mock,
        warning,
    }
}

#[cfg(feature = "native-reticulum")]
fn build_native_reticulum(
    settings: &AppSettings,
    paths: &AppPaths,
) -> AppResult<RuntimeFactoryDecision> {
    let runtime = crate::runtime::native::NativeNetworkRuntime::new(
        crate::runtime::native::NativeRuntimeConfig::from_settings_and_paths(settings, paths),
    );
    Ok(RuntimeFactoryDecision {
        runtime: Arc::new(runtime),
        backend: RuntimeBackendName::Reticulum,
        warning: Some(
            "native Reticulum runtime selected; live page/LXMF behavior depends on compiled native-network features and configured interfaces"
                .into(),
        ),
    })
}

#[cfg(not(feature = "native-reticulum"))]
fn build_native_reticulum(
    _settings: &AppSettings,
    _paths: &AppPaths,
) -> AppResult<RuntimeFactoryDecision> {
    Err(AppError::Unsupported(
        "runtime_backend=reticulum requested, but feature native-reticulum is not compiled".into(),
    ))
}
