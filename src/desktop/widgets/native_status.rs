use crate::app::App;
use crate::browser::BrowserAddress;
use crate::desktop::DesktopApp;
use crate::storage::settings::RuntimeBackendSetting;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::desktop) struct NativeSetupStep {
    pub(in crate::desktop) title: &'static str,
    pub(in crate::desktop) ready: bool,
    pub(in crate::desktop) detail: String,
}

pub(in crate::desktop) fn native_action_status_lines(desktop: &DesktopApp) -> Vec<String> {
    let readiness = desktop.app.native_reticulum_readiness();
    let native_backend = matches!(
        desktop.app.settings.runtime_backend,
        RuntimeBackendSetting::Auto | RuntimeBackendSetting::Reticulum
    );
    let identity_ready = desktop.app.settings.identity_path.is_some();
    let browser_address = desktop.app.active_browser_tab().address_input.trim();
    let destination_ready = BrowserAddress::parse(browser_address).is_some();
    let peer_ready = is_32_hex_hash(desktop.app.active_conversation().peer_hash.trim());

    vec![
        action_status_line(
            readiness.compiled,
            "native feature compiled",
            "build with native-network features",
        ),
        action_status_line(
            native_backend,
            "native backend selected",
            "choose Auto or Reticulum backend",
        ),
        action_status_line(
            identity_ready,
            "identity configured",
            "create or attach an identity",
        ),
        action_status_line(
            readiness.configured,
            "native runtime configured",
            "fix interface/config readiness blockers",
        ),
        action_status_line(
            destination_ready,
            "browser destination address ready",
            "enter a destination:path address such as <hash>:/page/index.mu",
        ),
        action_status_line(
            peer_ready,
            "valid LXMF peer selected",
            "open/select a Directory peer with a 32 hex destination hash",
        ),
    ]
}

pub(in crate::desktop) fn native_setup_steps(app: &App) -> Vec<NativeSetupStep> {
    let identity_ready = app
        .settings
        .identity_path
        .as_ref()
        .is_some_and(|path| path.is_file());
    let backend_ready = matches!(
        app.settings.runtime_backend,
        RuntimeBackendSetting::Reticulum
    );
    let interface_details = app.native_interface_readiness();
    let native_interface_ready = interface_details
        .iter()
        .any(|detail| detail.enabled && detail.supported && !detail.blocks_native_startup);
    let runtime_ready = app.runtime_status.connected && backend_ready;
    let directory_ready = !app.directory_state.entries.is_empty();
    let live_ready = runtime_ready && app.native_reticulum_readiness().ready;

    vec![
        NativeSetupStep {
            title: "Identity",
            ready: identity_ready,
            detail: app
                .settings
                .active_identity_label
                .clone()
                .or_else(|| {
                    app.settings
                        .identity_path
                        .as_ref()
                        .map(|path| path.display().to_string())
                })
                .unwrap_or_else(|| "create or attach a managed Reticulum identity".into()),
        },
        NativeSetupStep {
            title: "Backend",
            ready: backend_ready,
            detail: format!("selected backend: {:?}", app.settings.runtime_backend),
        },
        NativeSetupStep {
            title: "Interface",
            ready: native_interface_ready,
            detail: if native_interface_ready {
                format!(
                    "{} native-supported profile(s) configured",
                    interface_details
                        .iter()
                        .filter(|detail| detail.enabled
                            && detail.supported
                            && !detail.blocks_native_startup)
                        .count()
                )
            } else {
                "add an enabled TCP gateway profile that native Reticulum can start".into()
            },
        },
        NativeSetupStep {
            title: "Runtime",
            ready: runtime_ready,
            detail: if runtime_ready {
                app.runtime_status.message.clone()
            } else {
                "start the native runtime after identity/backend/interface are ready".into()
            },
        },
        NativeSetupStep {
            title: "Directory",
            ready: directory_ready,
            detail: if directory_ready {
                format!(
                    "{} known directory entrie(s)",
                    app.directory_state.entries.len()
                )
            } else {
                "wait for announces, preload known destinations, or run live probe".into()
            },
        },
        NativeSetupStep {
            title: "Live Test",
            ready: live_ready,
            detail: if live_ready {
                "open a NomadNet destination or run LXMF interop from the app".into()
            } else {
                "use Live Fetch/LXMF Interop after runtime and paths are visible".into()
            },
        },
    ]
}

pub(in crate::desktop) fn action_status_line(ok: bool, label: &str, fix: &str) -> String {
    if ok {
        format!("ready: {label}")
    } else {
        format!("blocked: {label}; {fix}")
    }
}

pub(in crate::desktop) fn is_32_hex_hash(value: &str) -> bool {
    value.len() == 32 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::desktop::widgets::interface_config::setup_tcp_client_profile;

    #[test]
    fn action_status_line_marks_ready_and_blocked() {
        assert_eq!(
            action_status_line(true, "identity", "create one"),
            "ready: identity"
        );
        assert_eq!(
            action_status_line(false, "identity", "create one"),
            "blocked: identity; create one"
        );
    }

    #[test]
    fn native_setup_steps_show_first_run_progress() {
        let root = std::env::temp_dir().join(format!(
            "omenbrowser-rs-desktop-setup-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let paths = crate::config::AppPaths::from_root(root);
        paths.ensure().expect("paths");
        let mut app = App::new(crate::config::AppConfig {
            paths,
            settings: crate::storage::settings::AppSettings::default(),
        });

        let steps = native_setup_steps(&app);
        assert_eq!(steps.len(), 6);
        assert_eq!(steps[0].title, "Identity");
        assert!(!steps[0].ready);
        assert!(!steps[1].ready);

        let identity_path = app.paths.identities_dir.join("setup_identity");
        std::fs::write(&identity_path, b"identity").expect("identity");
        app.settings.identity_path = Some(identity_path);
        app.settings.active_identity_label = Some("Setup Identity".into());
        app.settings.runtime_backend = RuntimeBackendSetting::Reticulum;
        app.create_tcp_client_interface_profile();

        let steps = native_setup_steps(&app);
        assert!(steps[0].ready);
        assert!(steps[1].ready);
        assert!(steps[2].detail.contains("native-supported"));
        assert!(setup_tcp_client_profile(&app).is_some());
    }
}
