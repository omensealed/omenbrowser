//! Pure diagnostic redaction for the browser compatibility CLI.

use std::path::Path;

use crate::cli_overrides::SmokeOverrides;

pub fn redacted_argv(argv: Vec<String>) -> Vec<String> {
    let mut redacted = Vec::with_capacity(argv.len());
    let mut redact_next = None;
    for arg in argv {
        if let Some(replacement) = redact_next.take() {
            redacted.push(replacement);
            continue;
        }
        redact_next = match arg.as_str() {
            "--passphrase" => Some("<redacted-secret>".into()),
            "--passphrase-file" => Some("<redacted-path>".into()),
            "--identity"
            | "--identity-path"
            | "--reticulum-config"
            | "--reticulum-config-path"
            | "--known-destinations"
            | "--known-destinations-path"
            | "--generate-known-destinations-fixture"
            | "--write-known-destinations-fixture"
            | "--app-root"
            | "--output"
            | "-o"
            | "--bundle-report" => Some("<redacted-path>".into()),
            _ => None,
        };
        redacted.push(arg);
    }
    redacted
}

pub fn redacted_override_snapshot(overrides: &SmokeOverrides) -> serde_json::Value {
    serde_json::json!({
        "runtime_backend": overrides.runtime_backend().map(|backend| format!("{backend:?}")),
        "identity_path": overrides.identity_path().map(|path| redacted_path_hint(path)),
        "reticulum_config_path": overrides.reticulum_config_path().map(|path| redacted_path_hint(path)),
        "known_destinations_path": overrides.known_destinations_path().map(|path| redacted_path_hint(path)),
        "known_destinations_fixture_path": overrides.known_destinations_fixture_path().map(|path| redacted_path_hint(path)),
        "app_root": overrides.app_root().map(|path| redacted_path_hint(path)),
        "tcp_client": overrides.tcp_client().map(|tcp| serde_json::json!({
            "host": tcp.host(),
            "port": tcp.port(),
            "network_name": tcp.network_name().map(|_| "<redacted>"),
            "passphrase": tcp.passphrase().map(|_| "<redacted>"),
        })),
    })
}

pub fn redacted_path_hint(path: &Path) -> serde_json::Value {
    serde_json::json!({
        "redacted": true,
        "file_name": path.file_name().and_then(|name| name.to_str()).unwrap_or("<none>"),
        "is_absolute": path.is_absolute(),
    })
}

pub fn redact_bundle_log_message(
    message: &str,
    overrides: &SmokeOverrides,
    identity_path: Option<&Path>,
) -> String {
    let lower = message.to_ascii_lowercase();
    let mut redacted: String = if lower.contains("message body") || lower.contains("draft body") {
        "<redacted message body log>".into()
    } else {
        message.into()
    };
    for path in protected_paths(overrides, identity_path) {
        let text = path.display().to_string();
        if !text.is_empty() {
            redacted = redacted.replace(&text, "<redacted-path>");
        }
    }
    if let Some(passphrase) = overrides
        .tcp_client()
        .and_then(crate::cli_network::TcpClientOverride::passphrase)
        .filter(|value| !value.is_empty())
    {
        redacted = redacted.replace(passphrase, "<redacted-secret>");
    }
    if redacted.chars().count() > 240 {
        let truncated = redacted.chars().take(240).collect::<String>();
        format!("{truncated}...")
    } else {
        redacted
    }
}

fn protected_paths<'a>(
    overrides: &'a SmokeOverrides,
    identity_path: Option<&'a Path>,
) -> impl Iterator<Item = &'a Path> {
    [
        identity_path,
        overrides.identity_path().map(|path| path.as_path()),
        overrides.reticulum_config_path().map(|path| path.as_path()),
        overrides
            .known_destinations_path()
            .map(|path| path.as_path()),
        overrides
            .known_destinations_fixture_path()
            .map(|path| path.as_path()),
        overrides.app_root().map(|path| path.as_path()),
    ]
    .into_iter()
    .flatten()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{cli_network::TcpClientOverride, storage::settings::RuntimeBackendSetting};

    fn absolute_private_identity_path() -> std::path::PathBuf {
        #[cfg(windows)]
        {
            std::path::PathBuf::from(r"C:\private\identity")
        }
        #[cfg(not(windows))]
        {
            std::path::PathBuf::from("/private/identity")
        }
    }

    #[test]
    fn argv_redaction_preserves_exact_compatibility_shape() {
        assert_eq!(
            redacted_argv(vec![
                "omenbrowser_rs".into(),
                "--identity".into(),
                "/tmp/private/identity".into(),
                "--reticulum-config".into(),
                "/tmp/private/rns".into(),
                "--tcp-client".into(),
                "127.0.0.1:4242".into(),
                "--passphrase".into(),
                "do-not-leak".into(),
                "--passphrase-file".into(),
                "/tmp/private/passphrase".into(),
                "--bundle-report".into(),
                "/tmp/private/bundles".into(),
            ]),
            vec![
                "omenbrowser_rs",
                "--identity",
                "<redacted-path>",
                "--reticulum-config",
                "<redacted-path>",
                "--tcp-client",
                "127.0.0.1:4242",
                "--passphrase",
                "<redacted-secret>",
                "--passphrase-file",
                "<redacted-path>",
                "--bundle-report",
                "<redacted-path>",
            ]
        );
    }

    #[test]
    fn path_hint_preserves_metadata_without_the_private_path() {
        let absolute_path = absolute_private_identity_path();
        assert_eq!(
            redacted_path_hint(&absolute_path),
            serde_json::json!({
                "redacted": true,
                "file_name": "identity",
                "is_absolute": true,
            })
        );
        assert_eq!(
            redacted_path_hint(Path::new("relative/config")),
            serde_json::json!({
                "redacted": true,
                "file_name": "config",
                "is_absolute": false,
            })
        );
    }

    #[test]
    fn override_snapshot_preserves_schema_and_redacts_credentials() {
        let private_identity_path = absolute_private_identity_path();
        let overrides = SmokeOverrides::default()
            .with_runtime_backend(RuntimeBackendSetting::Reticulum)
            .with_identity_path(&private_identity_path)
            .with_tcp_client(TcpClientOverride::new(
                "gateway.example",
                4242,
                Some("private-network".into()),
                Some("unique-secret".into()),
            ));
        let snapshot = redacted_override_snapshot(&overrides);
        assert_eq!(
            snapshot,
            serde_json::json!({
                "runtime_backend": "Reticulum",
                "identity_path": {
                    "redacted": true,
                    "file_name": "identity",
                    "is_absolute": true,
                },
                "reticulum_config_path": null,
                "known_destinations_path": null,
                "known_destinations_fixture_path": null,
                "app_root": null,
                "tcp_client": {
                    "host": "gateway.example",
                    "port": 4242,
                    "network_name": "<redacted>",
                    "passphrase": "<redacted>",
                },
            })
        );
        let rendered = snapshot.to_string();
        assert!(!rendered.contains(&private_identity_path.display().to_string()));
        assert!(!rendered.contains("private-network"));
        assert!(!rendered.contains("unique-secret"));
    }

    #[test]
    fn log_sanitizer_suppresses_message_bodies_before_other_processing() {
        let overrides = SmokeOverrides::default().with_identity_path("/private/identity");
        assert_eq!(
            redact_bundle_log_message("Runtime DRAFT BODY at /private/identity", &overrides, None,),
            "<redacted message body log>"
        );
    }

    #[test]
    fn log_sanitizer_redacts_all_paths_and_active_passphrase() {
        let overrides = SmokeOverrides::default()
            .with_identity_path("/private/managed-identity")
            .with_reticulum_config_path("/private/reticulum")
            .with_known_destinations_path("/private/known")
            .with_known_destinations_fixture_path("/private/fixture")
            .with_app_root("/private/app")
            .with_tcp_client(TcpClientOverride::new(
                "gateway.example",
                4242,
                None,
                Some("unique-secret".into()),
            ));
        let message = "external=/external/identity managed=/private/managed-identity reticulum=/private/reticulum known=/private/known fixture=/private/fixture app=/private/app pass=unique-secret";
        let redacted =
            redact_bundle_log_message(message, &overrides, Some(Path::new("/external/identity")));
        for protected in [
            "/external/identity",
            "/private/managed-identity",
            "/private/reticulum",
            "/private/known",
            "/private/fixture",
            "/private/app",
            "unique-secret",
        ] {
            assert!(!redacted.contains(protected));
        }
        assert_eq!(redacted.matches("<redacted-path>").count(), 6);
        assert!(redacted.contains("<redacted-secret>"));
    }

    #[test]
    fn log_sanitizer_preserves_character_based_truncation_contract() {
        let input = "é".repeat(241);
        let redacted = redact_bundle_log_message(&input, &SmokeOverrides::default(), None);
        assert_eq!(redacted, format!("{}...", "é".repeat(240)));
        assert_eq!(redacted.chars().count(), 243);
    }
}
