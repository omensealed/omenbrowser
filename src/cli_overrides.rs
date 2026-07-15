//! Private command-local overrides for the browser compatibility CLI.

use std::path::PathBuf;

use crate::{cli_network::TcpClientOverride, storage::settings::RuntimeBackendSetting};

#[derive(Clone, Default, PartialEq, Eq)]
pub struct SmokeOverrides {
    runtime_backend: Option<RuntimeBackendSetting>,
    identity_path: Option<PathBuf>,
    reticulum_config_path: Option<PathBuf>,
    known_destinations_path: Option<PathBuf>,
    known_destinations_fixture_path: Option<PathBuf>,
    tcp_client: Option<TcpClientOverride>,
    app_root: Option<PathBuf>,
}

impl SmokeOverrides {
    pub fn runtime_backend(&self) -> Option<&RuntimeBackendSetting> {
        self.runtime_backend.as_ref()
    }

    pub fn set_runtime_backend(&mut self, value: RuntimeBackendSetting) {
        self.runtime_backend = Some(value);
    }

    pub fn ensure_runtime_backend(&mut self, value: RuntimeBackendSetting) {
        self.runtime_backend.get_or_insert(value);
    }

    pub fn take_runtime_backend(&mut self) -> Option<RuntimeBackendSetting> {
        self.runtime_backend.take()
    }

    pub fn identity_path(&self) -> Option<&PathBuf> {
        self.identity_path.as_ref()
    }

    pub fn set_identity_path(&mut self, value: PathBuf) {
        self.identity_path = Some(value);
    }

    pub fn take_identity_path(&mut self) -> Option<PathBuf> {
        self.identity_path.take()
    }

    pub fn reticulum_config_path(&self) -> Option<&PathBuf> {
        self.reticulum_config_path.as_ref()
    }

    pub fn set_reticulum_config_path(&mut self, value: PathBuf) {
        self.reticulum_config_path = Some(value);
    }

    pub fn take_reticulum_config_path(&mut self) -> Option<PathBuf> {
        self.reticulum_config_path.take()
    }

    pub fn known_destinations_path(&self) -> Option<&PathBuf> {
        self.known_destinations_path.as_ref()
    }

    pub fn set_known_destinations_path(&mut self, value: PathBuf) {
        self.known_destinations_path = Some(value);
    }

    pub fn known_destinations_fixture_path(&self) -> Option<&PathBuf> {
        self.known_destinations_fixture_path.as_ref()
    }

    pub fn set_known_destinations_fixture_path(&mut self, value: PathBuf) {
        self.known_destinations_fixture_path = Some(value);
    }

    pub fn tcp_client(&self) -> Option<&TcpClientOverride> {
        self.tcp_client.as_ref()
    }

    pub fn set_tcp_client(&mut self, value: TcpClientOverride) {
        self.tcp_client = Some(value);
    }

    pub fn take_tcp_client(&mut self) -> Option<TcpClientOverride> {
        self.tcp_client.take()
    }

    pub fn tcp_client_mut_or_insert_empty(&mut self) -> &mut TcpClientOverride {
        self.tcp_client.get_or_insert_with(TcpClientOverride::empty)
    }

    pub fn app_root(&self) -> Option<&PathBuf> {
        self.app_root.as_ref()
    }

    pub fn set_app_root(&mut self, value: PathBuf) {
        self.app_root = Some(value);
    }

    pub fn take_app_root(&mut self) -> Option<PathBuf> {
        self.app_root.take()
    }

    pub fn with_runtime_backend(mut self, value: RuntimeBackendSetting) -> Self {
        self.set_runtime_backend(value);
        self
    }

    pub fn with_identity_path(mut self, value: impl Into<PathBuf>) -> Self {
        self.set_identity_path(value.into());
        self
    }

    pub fn with_reticulum_config_path(mut self, value: impl Into<PathBuf>) -> Self {
        self.set_reticulum_config_path(value.into());
        self
    }

    pub fn with_known_destinations_path(mut self, value: impl Into<PathBuf>) -> Self {
        self.set_known_destinations_path(value.into());
        self
    }

    pub fn with_known_destinations_fixture_path(mut self, value: impl Into<PathBuf>) -> Self {
        self.set_known_destinations_fixture_path(value.into());
        self
    }

    pub fn with_tcp_client(mut self, value: TcpClientOverride) -> Self {
        self.set_tcp_client(value);
        self
    }

    pub fn with_app_root(mut self, value: impl Into<PathBuf>) -> Self {
        self.set_app_root(value.into());
        self
    }
}

impl std::fmt::Debug for SmokeOverrides {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        fn path_state(value: &Option<PathBuf>) -> Option<&'static str> {
            value.as_ref().map(|_| "<redacted-path>")
        }

        f.debug_struct("SmokeOverrides")
            .field("runtime_backend", &self.runtime_backend)
            .field("identity_path", &path_state(&self.identity_path))
            .field(
                "reticulum_config_path",
                &path_state(&self.reticulum_config_path),
            )
            .field(
                "known_destinations_path",
                &path_state(&self.known_destinations_path),
            )
            .field(
                "known_destinations_fixture_path",
                &path_state(&self.known_destinations_fixture_path),
            )
            .field("tcp_client", &self.tcp_client)
            .field("app_root", &path_state(&self.app_root))
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_backend_is_set_only_when_absent() {
        let mut overrides = SmokeOverrides::default();
        overrides.ensure_runtime_backend(RuntimeBackendSetting::Reticulum);
        overrides.ensure_runtime_backend(RuntimeBackendSetting::Mock);
        assert_eq!(
            overrides.runtime_backend(),
            Some(&RuntimeBackendSetting::Reticulum)
        );
    }

    #[test]
    fn debug_redacts_paths_and_nested_tcp_credentials() {
        let overrides = SmokeOverrides::default()
            .with_identity_path("/private/identity")
            .with_app_root("/private/app-root")
            .with_tcp_client(TcpClientOverride::new(
                "gateway.example",
                4242,
                Some("private-network".into()),
                Some("unique-cli-secret".into()),
            ));
        let debug = format!("{overrides:?}");
        assert!(!debug.contains("/private/identity"));
        assert!(!debug.contains("/private/app-root"));
        assert!(!debug.contains("private-network"));
        assert!(!debug.contains("unique-cli-secret"));
        assert!(debug.contains("<redacted-path>"));
        assert!(debug.contains("<redacted>"));
    }

    #[test]
    fn consuming_accessors_transfer_owned_values_once() {
        let mut overrides = SmokeOverrides::default()
            .with_identity_path("identity")
            .with_tcp_client(TcpClientOverride::parse_endpoint("localhost:4242").unwrap());
        assert_eq!(
            overrides.take_identity_path(),
            Some(PathBuf::from("identity"))
        );
        assert!(overrides.take_identity_path().is_none());
        assert_eq!(
            overrides.take_tcp_client().map(|tcp| tcp.into_parts()),
            Some(("localhost".into(), 4242, None, None))
        );
        assert!(overrides.take_tcp_client().is_none());
    }
}
