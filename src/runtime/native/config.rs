use std::path::PathBuf;

use crate::config::AppPaths;
use crate::storage::settings::{AppSettings, ReticulumInstanceMode};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NativeRuntimeMode {
    Managed,
    External,
}

#[derive(Clone, PartialEq, Eq)]
pub struct NativeRuntimeConfig {
    pub reticulum_config_dir: PathBuf,
    pub reticulum_storage_dir: PathBuf,
    pub attachments_dir: PathBuf,
    pub identity_path: Option<PathBuf>,
    pub instance_mode: NativeRuntimeMode,
    pub announce_on_start: bool,
    pub request_timeout_secs: u64,
    pub native_lxmf_sdk_rpc_endpoint: Option<String>,
}

impl NativeRuntimeConfig {
    pub fn from_paths(paths: &AppPaths) -> Self {
        Self::from_settings_and_paths(&AppSettings::default(), paths)
    }

    pub fn from_settings_and_paths(settings: &AppSettings, paths: &AppPaths) -> Self {
        Self {
            reticulum_config_dir: settings
                .reticulum_config_path
                .clone()
                .unwrap_or_else(|| paths.reticulum_config_dir.clone()),
            reticulum_storage_dir: paths.reticulum_storage_dir.clone(),
            attachments_dir: paths.attachments_dir.clone(),
            identity_path: settings.identity_path.clone(),
            instance_mode: match &settings.reticulum_instance_mode {
                ReticulumInstanceMode::Managed => NativeRuntimeMode::Managed,
                ReticulumInstanceMode::External => NativeRuntimeMode::External,
            },
            announce_on_start: settings.announce_on_start,
            request_timeout_secs: 30,
            native_lxmf_sdk_rpc_endpoint: settings
                .native_lxmf_sdk_rpc_endpoint
                .as_deref()
                .map(str::trim)
                .filter(|endpoint| !endpoint.is_empty())
                .map(str::to_string),
        }
    }

    pub fn identity_hint(&self) -> Option<String> {
        self.identity_path
            .as_ref()
            .and_then(|path| path.file_name())
            .and_then(|name| name.to_str())
            .map(str::to_string)
            .or_else(|| {
                self.identity_path
                    .as_ref()
                    .map(|_| "<identity-path>".into())
            })
    }
}

impl std::fmt::Debug for NativeRuntimeConfig {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("NativeRuntimeConfig")
            .field("reticulum_config_dir", &self.reticulum_config_dir)
            .field("reticulum_storage_dir", &self.reticulum_storage_dir)
            .field("attachments_dir", &self.attachments_dir)
            .field("identity_path", &self.identity_hint())
            .field("instance_mode", &self.instance_mode)
            .field("announce_on_start", &self.announce_on_start)
            .field("request_timeout_secs", &self.request_timeout_secs)
            .field(
                "native_lxmf_sdk_rpc_endpoint",
                &self
                    .native_lxmf_sdk_rpc_endpoint
                    .as_ref()
                    .map(|_| "<configured>"),
            )
            .finish()
    }
}
