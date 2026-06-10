use std::collections::BTreeMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::browser::cache::PageCache;
use crate::config::AppPaths;
use crate::directory::DirectoryService;
use crate::error::AppResult;
use crate::identity::IdentityProfile;
use crate::interfaces::ReticulumInterfaceProfile;
use crate::messaging::MessageStore;
use crate::plugins::PluginManifest;
use crate::runtime::{
    InterfaceStats, NetworkRuntime, NetworkSnapshot, PropagationStatus, RuntimeStatus,
};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct DiagnosticsSnapshot {
    pub app_version: String,
    pub platform: PlatformInfo,
    pub runtime: RuntimeDiagnostics,
    pub paths: PathDiagnostics,
    pub active_identity: Option<IdentityDiagnostics>,
    pub reticulum: ReticulumPathDiagnostics,
    pub interface_stats: InterfaceStats,
    pub propagation_status: PropagationStatus,
    pub network: NetworkSnapshot,
    pub counts: DiagnosticsCounts,
    pub plugins: Vec<PluginManifest>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlatformInfo {
    pub os: String,
    pub arch: String,
    pub family: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuntimeDiagnostics {
    pub backend: String,
    pub connected: bool,
    pub message: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct PathDiagnostics {
    pub root: String,
    pub settings_file: String,
    pub messages_dir: String,
    pub cache_dir: String,
    pub downloads_dir: String,
    pub plugins_dir: String,
    pub logs_dir: String,
    pub diagnostics_dir: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReticulumPathDiagnostics {
    pub config_dir: String,
    pub storage_dir: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct IdentityDiagnostics {
    pub label: String,
    pub hash_short: String,
    pub path_hint: String,
    pub managed: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiagnosticsCounts {
    pub message_threads: usize,
    pub directory_entries: usize,
    pub live_directory_entries: usize,
    pub cache_files: usize,
    pub interface_profiles: usize,
    pub browser_tabs: usize,
    pub conversation_tabs: usize,
}

#[derive(Clone)]
pub struct DiagnosticsService {
    paths: AppPaths,
    runtime: Arc<dyn NetworkRuntime>,
}

impl DiagnosticsService {
    pub fn new(paths: AppPaths, runtime: Arc<dyn NetworkRuntime>) -> Self {
        Self { paths, runtime }
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn snapshot(
        &self,
        message_store: Option<&MessageStore>,
        directory: Option<&DirectoryService>,
        cache: Option<&PageCache>,
        interfaces: &[ReticulumInterfaceProfile],
        plugins: &[PluginManifest],
        browser_tabs: usize,
        conversation_tabs: usize,
    ) -> AppResult<DiagnosticsSnapshot> {
        let runtime_status = self.runtime.status().await;
        let interface_stats = self.runtime.interface_stats().await?;
        let network = self.runtime.network_snapshot().await?;
        let propagation_status = self.runtime.propagation_status().await?;
        Ok(DiagnosticsSnapshot {
            app_version: env!("CARGO_PKG_VERSION").into(),
            platform: PlatformInfo {
                os: std::env::consts::OS.into(),
                arch: std::env::consts::ARCH.into(),
                family: std::env::consts::FAMILY.into(),
            },
            runtime: runtime_diagnostics(&runtime_status),
            paths: PathDiagnostics::from(&self.paths),
            active_identity: runtime_status
                .active_identity
                .as_ref()
                .map(identity_diagnostics),
            reticulum: ReticulumPathDiagnostics {
                config_dir: self.paths.reticulum_config_dir.display().to_string(),
                storage_dir: self.paths.reticulum_storage_dir.display().to_string(),
            },
            interface_stats,
            propagation_status,
            network,
            counts: DiagnosticsCounts {
                message_threads: message_store
                    .map(|store| store.list_threads().map(|threads| threads.len()))
                    .transpose()?
                    .unwrap_or_default(),
                directory_entries: directory
                    .map(|directory| directory.list_entries().len())
                    .unwrap_or_default(),
                live_directory_entries: directory
                    .map(|directory| directory.list_live_entries().len())
                    .unwrap_or_default(),
                cache_files: cache
                    .map(count_cache_files)
                    .transpose()?
                    .unwrap_or_default(),
                interface_profiles: interfaces.len(),
                browser_tabs,
                conversation_tabs,
            },
            plugins: plugins.to_vec(),
        })
    }

    pub fn redacted_export(snapshot: &DiagnosticsSnapshot) -> BTreeMap<String, serde_json::Value> {
        let mut value = serde_json::to_value(snapshot)
            .expect("diagnostics snapshot serializes")
            .as_object()
            .cloned()
            .unwrap_or_default();
        if let Some(identity) = value
            .get_mut("active_identity")
            .and_then(|identity| identity.as_object_mut())
        {
            identity.insert(
                "path_hint".into(),
                serde_json::Value::String("<redacted>".into()),
            );
        }
        value.into_iter().collect()
    }
}

impl From<&AppPaths> for PathDiagnostics {
    fn from(paths: &AppPaths) -> Self {
        Self {
            root: paths.root.display().to_string(),
            settings_file: paths.settings_file.display().to_string(),
            messages_dir: paths.messages_dir.display().to_string(),
            cache_dir: paths.cache_dir.display().to_string(),
            downloads_dir: paths.downloads_dir.display().to_string(),
            plugins_dir: paths.plugins_dir.display().to_string(),
            logs_dir: paths.logs_dir.display().to_string(),
            diagnostics_dir: paths.diagnostics_dir.display().to_string(),
        }
    }
}

fn runtime_diagnostics(status: &RuntimeStatus) -> RuntimeDiagnostics {
    RuntimeDiagnostics {
        backend: format!("{:?}", status.backend),
        connected: status.connected,
        message: status.message.clone(),
    }
}

fn identity_diagnostics(identity: &IdentityProfile) -> IdentityDiagnostics {
    IdentityDiagnostics {
        label: identity.label.clone(),
        hash_short: identity.hash_hex.chars().take(12).collect(),
        path_hint: identity
            .path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("<identity>")
            .into(),
        managed: identity.managed,
    }
}

fn count_cache_files(cache: &PageCache) -> AppResult<usize> {
    Ok(std::fs::read_dir(cache.cache_dir())?
        .filter_map(Result::ok)
        .filter(|entry| entry.path().is_file())
        .count())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::browser::cache::PageCache;
    use crate::directory::{DirectoryKind, DirectoryService};
    use crate::messaging::MessageStore;
    use crate::runtime::{MockNetworkRuntime, NetworkRuntime};

    use super::*;

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "omenbrowser-rs-diagnostics-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    #[tokio::test]
    async fn diagnostics_snapshot_contains_counts_and_redacts_identity_path_hint() {
        let root = temp_dir("snapshot");
        let paths = AppPaths::from_root(root.clone());
        paths.ensure().expect("paths");
        let runtime = Arc::new(MockNetworkRuntime::default());
        runtime
            .attach_identity(IdentityProfile {
                label: "Test".into(),
                path: root.join("secret_identity"),
                hash_hex: "abcdef1234567890".into(),
                managed: true,
            })
            .await
            .expect("attach");
        let message_store = MessageStore::new(paths.messages_dir.clone()).expect("messages");
        let mut directory = DirectoryService::new(paths.directory_file.clone()).expect("directory");
        directory
            .ingest_announce("mock.node", "Mock Node", DirectoryKind::Node, None, None)
            .expect("announce");
        let cache = PageCache::new(paths.cache_dir.clone()).expect("cache");
        cache
            .store("key", "markup", 60, "title", BTreeMap::new())
            .expect("store cache");
        let service = DiagnosticsService::new(paths, runtime);

        let snapshot = service
            .snapshot(
                Some(&message_store),
                Some(&directory),
                Some(&cache),
                &[],
                &[],
                2,
                3,
            )
            .await
            .expect("snapshot");
        let redacted = DiagnosticsService::redacted_export(&snapshot);

        assert_eq!(snapshot.counts.directory_entries, 1);
        assert_eq!(snapshot.counts.cache_files, 1);
        assert_eq!(snapshot.counts.browser_tabs, 2);
        assert_eq!(
            redacted
                .get("active_identity")
                .and_then(|value| value.get("path_hint"))
                .and_then(|value| value.as_str()),
            Some("<redacted>")
        );
    }
}
