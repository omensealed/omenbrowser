use std::time::{SystemTime, UNIX_EPOCH};

use omenbrowser_rs::app::{App, SmokeKnownDestinationsPreload, SmokePathWarmup};
use omenbrowser_rs::config::{AppConfig, AppPaths};
use omenbrowser_rs::messaging::DeliveryMode;
use omenbrowser_rs::storage::settings::{AppSettings, RuntimeBackendSetting};

const FIXTURE_NODE_HASH: &str = "00112233445566778899aabbccddeeff";

fn test_config(name: &str) -> AppConfig {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "omenbrowser-rs-native-smoke-{name}-{}-{nonce}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    let settings = AppSettings {
        runtime_backend: RuntimeBackendSetting::Mock,
        ..AppSettings::default()
    };
    AppConfig {
        paths: AppPaths::from_root(root),
        settings,
    }
}

#[tokio::test]
async fn native_network_smoke_report_can_request_path_warmup() {
    let app = App::new(test_config("native-smoke-warm-path"));

    let report = app
        .native_network_smoke_test_report_for_url_with_warmup(
            "mock.node:/",
            false,
            Some(SmokePathWarmup { wait_secs: 0 }),
        )
        .await
        .expect("smoke report");

    assert_eq!(
        report.get("report").and_then(serde_json::Value::as_str),
        Some("native_network_smoke_test")
    );
    assert_eq!(
        report
            .get("path_warmup")
            .and_then(|value| value.get("requested"))
            .and_then(serde_json::Value::as_bool),
        Some(true)
    );
    assert_eq!(
        report
            .get("path_warmup")
            .and_then(|value| value.get("request_path"))
            .and_then(|value| value.get("ok"))
            .and_then(serde_json::Value::as_bool),
        Some(true)
    );
    assert_eq!(
        report
            .get("verdicts")
            .and_then(|value| value.get("path_warmup"))
            .and_then(|value| value.get("status"))
            .and_then(serde_json::Value::as_str),
        Some("skip")
    );
    assert!(!report
        .get("path_warmup")
        .and_then(|value| value.get("wait"))
        .and_then(|value| value.get("attempts"))
        .and_then(serde_json::Value::as_array)
        .expect("path warmup attempts")
        .is_empty());
}

#[tokio::test]
async fn smoke_path_warmup_attempts_include_inspection_and_dry_probe() {
    let app = App::new(test_config("native-smoke-warm-path-attempts"));

    let report = app
        .native_network_smoke_test_report_for_url_with_warmup(
            "mock.node:/",
            false,
            Some(SmokePathWarmup { wait_secs: 0 }),
        )
        .await
        .expect("smoke report");
    let first_attempt = report
        .get("path_warmup")
        .and_then(|value| value.get("wait"))
        .and_then(|value| value.get("attempts"))
        .and_then(serde_json::Value::as_array)
        .and_then(|attempts| attempts.first())
        .expect("first attempt");

    assert_eq!(
        first_attempt
            .get("inspection")
            .and_then(|value| value.get("ok"))
            .and_then(serde_json::Value::as_bool),
        Some(true)
    );
    assert_eq!(
        first_attempt
            .get("dry_run_page_probe")
            .and_then(|value| value.get("ok"))
            .and_then(serde_json::Value::as_bool),
        Some(true)
    );
}

#[tokio::test]
async fn native_network_smoke_report_includes_known_destinations_preload_summary() {
    let app = App::new(test_config("native-smoke-known-destinations"));

    let report = app
        .native_network_smoke_test_report_for_url_with_options(
            "mock.node:/",
            false,
            None,
            Some(SmokeKnownDestinationsPreload {
                source_hint: "known_destinations".into(),
                loaded: 2,
            }),
        )
        .await
        .expect("smoke report");

    assert_eq!(
        report
            .get("known_destinations_preload")
            .and_then(|value| value.get("requested"))
            .and_then(serde_json::Value::as_bool),
        Some(true)
    );
    assert_eq!(
        report
            .get("known_destinations_preload")
            .and_then(|value| value.get("loaded"))
            .and_then(serde_json::Value::as_u64),
        Some(2)
    );
    assert!(!serde_json::to_string(&report)
        .expect("json")
        .contains("/tmp/"));
}

#[tokio::test]
async fn smoke_known_destinations_preload_requires_existing_file() {
    let app = App::new(test_config("native-smoke-known-destinations-missing"));

    let error = app
        .preload_known_destinations_for_smoke_test(&app.paths.root.join("missing"))
        .await
        .expect_err("missing file");

    assert!(error.to_string().contains("known destinations file"));
}

#[tokio::test]
async fn native_lxmf_smoke_send_report_skips_send_when_not_ready() {
    let app = App::new(test_config("native-lxmf-smoke-send-skip"));

    let report = app
        .native_lxmf_smoke_send_report_for_peer(
            FIXTURE_NODE_HASH,
            DeliveryMode::Direct,
            None,
            false,
        )
        .await
        .expect("smoke send report");

    assert_eq!(
        report.get("report").and_then(serde_json::Value::as_str),
        Some("native_lxmf_smoke_send")
    );
    assert!(report.get("attempt").is_some());
    assert!(report
        .get("app_actions")
        .and_then(|value| value.get("send"))
        .and_then(serde_json::Value::as_str)
        .is_some_and(|action| action.contains("Diagnostics")));
    assert!(
        report
            .get("command_examples")
            .and_then(|value| value.get("explicit_lxmf_smoke_send"))
            .and_then(serde_json::Value::as_str)
            .is_some_and(
                |command| command.contains(&format!("--send-lxmf-smoke {FIXTURE_NODE_HASH}"))
            )
    );
    assert_eq!(
        report
            .get("send")
            .and_then(|value| value.get("ok"))
            .and_then(serde_json::Value::as_bool),
        Some(false)
    );
    assert!(report
        .get("send")
        .and_then(|value| value.get("skipped"))
        .and_then(serde_json::Value::as_str)
        .is_some_and(|detail| detail.contains("ready_to_send")));
}

#[tokio::test]
async fn native_lxmf_live_interop_report_announces_and_waits_without_peer_send() {
    let app = App::new(test_config("native-lxmf-live-interop"));

    let report = app
        .native_lxmf_live_interop_report(None, 0, DeliveryMode::Direct, None, false)
        .await
        .expect("interop report");

    assert_eq!(
        report.get("report").and_then(serde_json::Value::as_str),
        Some("native_lxmf_live_interop")
    );
    assert_eq!(
        report
            .get("send")
            .and_then(|value| value.get("requested"))
            .and_then(serde_json::Value::as_bool),
        Some(false)
    );
    assert_eq!(
        report
            .get("wait")
            .and_then(|value| value.get("status"))
            .and_then(serde_json::Value::as_str),
        Some("skip")
    );
    assert!(report.get("local_announce").is_some());
    assert!(report.get("local").is_some());
    assert!(report.get("failure_hints").is_some());
    assert_eq!(
        report
            .get("readiness_retry")
            .and_then(|value| value.get("status"))
            .and_then(serde_json::Value::as_str),
        Some("skip")
    );
    assert_eq!(
        report
            .get("classification")
            .and_then(|value| value.get("outcome"))
            .and_then(serde_json::Value::as_str),
        Some("blocked")
    );
    assert!(report
        .get("classification")
        .and_then(|value| value.get("next_step"))
        .and_then(serde_json::Value::as_str)
        .is_some_and(|next_step| next_step.contains("native Reticulum")));
    assert!(report
        .get("app_actions")
        .and_then(|value| value.get("receive_only"))
        .and_then(serde_json::Value::as_str)
        .is_some_and(|action| action.contains("Diagnostics")));
    assert!(report
        .get("command_examples")
        .and_then(|value| value.get("developer_only"))
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false));
    assert!(report
        .get("app_actions")
        .and_then(|value| value.get("send_and_wait"))
        .and_then(serde_json::Value::as_str)
        .is_some_and(|action| action.contains("active peer conversation")));
    assert_eq!(
        report
            .get("wait")
            .and_then(|value| value.get("proof_match_state"))
            .and_then(serde_json::Value::as_str),
        Some("no_sent_packet")
    );
}
