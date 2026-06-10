use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};

use omenbrowser_rs::app::{
    App, BrowserLiveWarning, BrowserProbeSummary, BrowserRequestPreview, BrowserRequestStatus,
    BrowserRetryState, BrowserTaskResult, InternalAppEvent, LoadState, LogSeverity,
};
use omenbrowser_rs::browser::BrowserPage;
use omenbrowser_rs::config::{AppConfig, AppPaths};
use omenbrowser_rs::micron::render::HitAction;
use omenbrowser_rs::micron::LinkAction;
use omenbrowser_rs::runtime::{CancellationToken, PathEvent, RuntimeBusEvent};
use omenbrowser_rs::storage::settings::{AppSettings, RuntimeBackendSetting};
use omenbrowser_rs::workspace::WorkspaceSection;

const FIXTURE_NODE_HASH: &str = "00112233445566778899aabbccddeeff";

fn test_config(name: &str) -> AppConfig {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "omenbrowser-rs-browser-path-retry-{name}-{}-{nonce}",
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

fn fixture_node_url() -> String {
    format!("{FIXTURE_NODE_HASH}:/page/index.mu")
}

fn current_epoch_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time")
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

fn install_stale_browser_request_state(app: &mut App, target: String) {
    let now = current_epoch_ms();
    app.active_browser_tab_mut().request_preview = Some(BrowserRequestPreview {
        target: target.clone(),
        fields: vec!["message".into()],
        request_data: BTreeMap::from([("field_message".into(), "stale".into())]),
        status: BrowserRequestStatus::Failed,
        detail: "old failed request".into(),
    });
    app.active_browser_tab_mut().retry_state = Some(BrowserRetryState {
        target,
        destination_hash: FIXTURE_NODE_HASH.into(),
        reason: "old retry should not affect new navigation".into(),
        requested_epoch_ms: now,
        retry_after_epoch_ms: now,
        ready_epoch_ms: Some(now),
        attempts: 1,
    });
}

#[test]
fn browser_task_timeout_uses_actionable_retry_warning() {
    let mut app = App::new(test_config("browser-task-timeout-warning"));
    app.switch_section(WorkspaceSection::Browser);
    app.runtime_status.connected = true;
    let tab_id = app.active_browser_tab().id;
    let target = fixture_node_url();
    app.active_browser_tab_mut().loading = Some(LoadState {
        generation: 13,
        target: target.clone(),
        submitted_fields: false,
        cancel: CancellationToken::new(),
    });

    assert!(app.apply_browser_task_result(BrowserTaskResult::Error {
        tab_id,
        generation: 13,
        message:
            "link request timed out after 45s; request cancelled, retry when path/link is ready"
                .into(),
    }));

    let summary = app
        .active_browser_tab()
        .probe_summary
        .as_ref()
        .expect("probe summary");
    assert_eq!(summary.mode, "native-load");
    assert_eq!(summary.status, "response wait failed");
    assert!(summary.detail.contains("response"));
    assert!(summary.detail.contains("timed out after 45s"));
    let warning = app
        .active_browser_live_warning()
        .expect("live warning for task timeout");
    assert_eq!(warning.target, target);
    assert!(warning.next_action.contains("Diagnostics X"));
    assert_eq!(
        app.status.task,
        "link request timed out after 45s; press Retry when path/link is ready"
    );
    let retry = app
        .active_browser_tab()
        .retry_state
        .as_ref()
        .expect("manual retry state");
    assert_eq!(retry.target, target);
    assert!(retry.ready_epoch_ms.is_some());
    assert_eq!(retry.retry_after_epoch_ms, retry.ready_epoch_ms.unwrap());
    assert!(retry.reason.contains("press Retry"));
    assert!(!app.status.task.contains("retrying once"));
    assert!(app.logs.lines.iter().any(|line| {
        line.contains("browser-load page probe step")
            && line.contains("response failed")
            && line.contains("request cancelled")
    }));
}

#[tokio::test]
async fn same_destination_timeout_refreshes_path_for_retry() {
    let mut app = App::new(test_config("same-destination-timeout-path-refresh"));
    app.switch_section(WorkspaceSection::Browser);
    app.runtime_status.connected = true;
    let current = fixture_node_url();
    let target = format!("{FIXTURE_NODE_HASH}:/page/next.mu");
    let tab_id = app.active_browser_tab().id;
    let mut session = app.active_browser_tab().session.clone();
    let page = BrowserPage::mock_home(&current);
    session.restore_page(page.clone(), vec![page.url.clone()], 0);
    app.active_browser_tab_mut().session = session;
    app.active_browser_tab_mut().loading = Some(LoadState {
        generation: 17,
        target: target.clone(),
        submitted_fields: false,
        cancel: CancellationToken::new(),
    });

    assert!(app.apply_browser_task_result(BrowserTaskResult::Error {
        tab_id,
        generation: 17,
        message:
            "link request timed out after 45s; request cancelled, retry when path/link is ready"
                .into(),
    }));

    assert_eq!(app.monitoring_state.outbound_path_requests, 1);
    let retry = app
        .active_browser_tab()
        .retry_state
        .as_ref()
        .expect("retry state");
    assert_eq!(retry.target, target);
    assert_eq!(retry.destination_hash, FIXTURE_NODE_HASH);
    assert!(retry.ready_epoch_ms.is_some());
}

#[test]
fn cancelling_browser_load_marks_matching_preview_failed() {
    let mut app = App::new(test_config("cancel-load-preview"));
    let target = fixture_node_url();
    app.active_browser_tab_mut().loading = Some(LoadState {
        generation: 21,
        target: target.clone(),
        submitted_fields: false,
        cancel: CancellationToken::new(),
    });
    app.active_browser_tab_mut().request_preview = Some(BrowserRequestPreview {
        target: target.clone(),
        fields: Vec::new(),
        request_data: BTreeMap::new(),
        status: BrowserRequestStatus::Pending,
        detail: "request queued".into(),
    });

    app.cancel_active_browser_load();

    assert!(app.active_browser_tab().loading.is_none());
    let preview = app
        .active_browser_tab()
        .request_preview
        .as_ref()
        .expect("preview");
    assert_eq!(preview.status, BrowserRequestStatus::Failed);
    assert_eq!(preview.detail, "request cancelled by user");
}

#[tokio::test]
async fn direct_browser_open_clears_stale_pending_request_preview() {
    let mut app = App::new(test_config("open-clears-stale-preview"));
    let now = current_epoch_ms();
    app.active_browser_tab_mut().request_preview = Some(BrowserRequestPreview {
        target: fixture_node_url(),
        fields: vec!["message".into()],
        request_data: BTreeMap::from([("field_message".into(), "stale".into())]),
        status: BrowserRequestStatus::Pending,
        detail: "old request pending".into(),
    });
    app.active_browser_tab_mut().retry_state = Some(BrowserRetryState {
        target: fixture_node_url(),
        destination_hash: FIXTURE_NODE_HASH.into(),
        reason: "old retry should not affect new direct open".into(),
        requested_epoch_ms: now,
        retry_after_epoch_ms: now,
        ready_epoch_ms: Some(now),
        attempts: 1,
    });

    app.open_active_browser_target("mock.node:/page/gallery.mu".into());

    assert!(app.active_browser_tab().request_preview.is_none());
    assert!(app.active_browser_tab().retry_state.is_none());
    assert!(app.active_browser_tab().loading.is_some());
}

#[tokio::test]
async fn browser_download_clears_stale_request_state() {
    let mut app = App::new(test_config("download-clears-stale-request-state"));
    let stale_target = fixture_node_url();
    let download_target = "mock.node:/files/readme.txt".to_string();
    install_stale_browser_request_state(&mut app, stale_target);

    app.open_active_browser_target(download_target.clone());

    assert!(app.active_browser_tab().request_preview.is_none());
    assert!(app.active_browser_tab().retry_state.is_none());
    let loading = app.active_browser_tab().loading.as_ref().expect("loading");
    assert_eq!(loading.target, format!("download {download_target}"));
}

#[tokio::test]
async fn browser_reload_clears_stale_request_state() {
    let mut app = App::new(test_config("reload-clears-stale-request-state"));
    let page = BrowserPage::mock_home("mock.node:/page/index.mu");
    app.active_browser_tab_mut()
        .session
        .restore_page(page.clone(), vec![page.url.clone()], 0);
    install_stale_browser_request_state(&mut app, "mock.node:/page/old-submit.mu".into());

    app.reload_active_browser();

    assert!(app.active_browser_tab().request_preview.is_none());
    assert!(app.active_browser_tab().retry_state.is_none());
    let loading = app.active_browser_tab().loading.as_ref().expect("loading");
    assert_eq!(loading.target, "mock.node:/page/index.mu");
}

#[tokio::test]
async fn browser_history_navigation_clears_stale_request_state() {
    let mut app = App::new(test_config("history-clears-stale-request-state"));
    let first = BrowserPage::mock_home("mock.node:/page/one.mu");
    let second = BrowserPage::mock_home("mock.node:/page/two.mu");
    app.active_browser_tab_mut().session.restore_page(
        second,
        vec![first.url.clone(), "mock.node:/page/two.mu".into()],
        1,
    );
    install_stale_browser_request_state(&mut app, "mock.node:/page/old-submit.mu".into());

    app.browser_back();

    assert!(app.active_browser_tab().request_preview.is_none());
    assert!(app.active_browser_tab().retry_state.is_none());
    let loading = app.active_browser_tab().loading.as_ref().expect("loading");
    assert_eq!(loading.target, "history back");

    install_stale_browser_request_state(&mut app, "mock.node:/page/another-old-submit.mu".into());

    app.browser_forward();

    assert!(app.active_browser_tab().request_preview.is_none());
    assert!(app.active_browser_tab().retry_state.is_none());
    let loading = app.active_browser_tab().loading.as_ref().expect("loading");
    assert_eq!(loading.target, "history forward");
}

#[tokio::test]
async fn browser_retry_replays_captured_request_data_after_failed_preview() {
    let mut app = App::new(test_config("browser-retry-captured-failed-preview"));
    app.switch_section(WorkspaceSection::Browser);
    app.runtime_status.connected = true;
    let target = "mock.node:/page/micronplus-feed.mu".to_string();
    let now = current_epoch_ms();
    app.active_browser_tab_mut()
        .session
        .set_field_value("message", "edited after failure");
    app.active_browser_tab_mut().retry_state = Some(BrowserRetryState {
        target: target.clone(),
        destination_hash: "mock.node".into(),
        reason: "browser request timed out and was cancelled; press Retry to rebuild the page link"
            .into(),
        requested_epoch_ms: now,
        retry_after_epoch_ms: now,
        ready_epoch_ms: Some(now),
        attempts: 0,
    });
    app.active_browser_tab_mut().request_preview = Some(BrowserRequestPreview {
        target: target.clone(),
        fields: vec!["message".into(), "mode=chat".into()],
        request_data: BTreeMap::from([
            ("field_message".into(), "submitted before failure".into()),
            ("var_mode".into(), "chat".into()),
        ]),
        status: BrowserRequestStatus::Failed,
        detail: "request timed out".into(),
    });

    assert!(app.retry_active_browser_after_path_discovery());
    assert!(app.wait_for_browser_task_result().await);

    let request_data = app
        .active_browser_tab()
        .current_page
        .as_ref()
        .and_then(|page| page.request_data.as_ref())
        .expect("request data");
    assert_eq!(
        request_data.get("field_message").map(String::as_str),
        Some("submitted before failure")
    );
    assert_eq!(
        request_data.get("var_mode").map(String::as_str),
        Some("chat")
    );
}

#[tokio::test]
async fn browser_retry_ignores_completed_request_preview_for_same_target() {
    let mut app = App::new(test_config("browser-retry-ignore-completed-preview"));
    app.switch_section(WorkspaceSection::Browser);
    app.runtime_status.connected = true;
    let target = "mock.node:/page/gallery.mu".to_string();
    let now = current_epoch_ms();
    app.active_browser_tab_mut().retry_state = Some(BrowserRetryState {
        target: target.clone(),
        destination_hash: "mock.node".into(),
        reason: "path discovery reports a known route; retry now".into(),
        requested_epoch_ms: now,
        retry_after_epoch_ms: now,
        ready_epoch_ms: Some(now),
        attempts: 0,
    });
    app.active_browser_tab_mut().request_preview = Some(BrowserRequestPreview {
        target: target.clone(),
        fields: vec!["message".into()],
        request_data: BTreeMap::from([("field_message".into(), "old completed form".into())]),
        status: BrowserRequestStatus::Completed,
        detail: "loaded mock.node:/page/gallery.mu".into(),
    });

    assert!(app.retry_active_browser_after_path_discovery());
    assert!(app.wait_for_browser_task_result().await);

    let page = app
        .active_browser_tab()
        .current_page
        .as_ref()
        .expect("page");
    assert_eq!(page.url, target);
    assert!(
        page.request_data.is_none(),
        "completed previews must not replay stale form payloads"
    );
}

#[tokio::test]
async fn clicked_link_starts_with_fresh_request_state_for_same_target() {
    let mut app = App::new(test_config("clicked-link-fresh-request-state"));
    app.switch_section(WorkspaceSection::Browser);
    app.runtime_status.connected = true;
    let current = fixture_node_url();
    let target = format!("{FIXTURE_NODE_HASH}:/page/next.mu");
    let page = BrowserPage::mock_home(&current);
    app.active_browser_tab_mut()
        .session
        .restore_page(page.clone(), vec![page.url.clone()], 0);
    let now = current_epoch_ms();
    app.active_browser_tab_mut().retry_state = Some(BrowserRetryState {
        target: target.clone(),
        destination_hash: FIXTURE_NODE_HASH.into(),
        reason: "old failed click should not affect a fresh clicked link".into(),
        requested_epoch_ms: now,
        retry_after_epoch_ms: now,
        ready_epoch_ms: Some(now),
        attempts: 1,
    });
    app.active_browser_tab_mut().request_preview = Some(BrowserRequestPreview {
        target: target.clone(),
        fields: vec!["message".into()],
        request_data: BTreeMap::from([("field_message".into(), "old payload".into())]),
        status: BrowserRequestStatus::Failed,
        detail: "old failed request".into(),
    });

    assert!(app.activate_browser_hit_action(HitAction::Link(LinkAction {
        target: target.clone(),
        fields: Vec::new(),
    })));

    assert!(app.active_browser_tab().loading.is_some());
    assert!(app.active_browser_tab().retry_state.is_none());
    let preview = app
        .active_browser_tab()
        .request_preview
        .as_ref()
        .expect("fresh link preview");
    assert_eq!(preview.target, target);
    assert_eq!(preview.status, BrowserRequestStatus::Pending);
    assert!(preview.fields.is_empty());
    assert!(preview.request_data.is_empty());
}

#[test]
fn non_native_browser_load_failure_does_not_attach_probe_trace() {
    let mut app = App::new(test_config("non-native-load-error"));
    let tab_id = app.active_browser_tab().id;
    app.active_browser_tab_mut().loading = Some(LoadState {
        generation: 11,
        target: "mock.node:/".into(),
        submitted_fields: false,
        cancel: CancellationToken::new(),
    });

    assert!(app.apply_browser_task_result(BrowserTaskResult::Error {
        tab_id,
        generation: 11,
        message: "mock page is missing".into(),
    }));

    assert_eq!(app.status.task, "mock page is missing");
    assert!(app.active_browser_tab().probe_summary.is_none());
    assert!(app.active_browser_live_warning().is_none());
    assert!(!app
        .logs
        .lines
        .iter()
        .any(|line| line.contains("browser-load page probe step")));
}

#[test]
fn failed_submitted_browser_request_does_not_auto_resubmit() {
    let mut app = App::new(test_config("submitted-request-no-auto-resubmit"));
    app.switch_section(WorkspaceSection::Browser);
    app.runtime_status.connected = true;
    let tab_id = app.active_browser_tab().id;
    let target = format!("{FIXTURE_NODE_HASH}:/page/myblog.mu");
    app.active_browser_tab_mut().loading = Some(LoadState {
        generation: 42,
        target: target.clone(),
        submitted_fields: true,
        cancel: CancellationToken::new(),
    });
    app.active_browser_tab_mut().request_preview = Some(BrowserRequestPreview {
        target: target.clone(),
        fields: vec!["title".into(), "body".into()],
        request_data: BTreeMap::from([
            ("field_title".into(), "post title".into()),
            ("field_body".into(), "post body".into()),
        ]),
        status: BrowserRequestStatus::Pending,
        detail: "request queued".into(),
    });

    assert!(app.apply_browser_task_result(BrowserTaskResult::Error {
        tab_id,
        generation: 42,
        message: format!("native Reticulum page fetch failed for {FIXTURE_NODE_HASH} during response wait: timed out waiting for rns-net page response"),
    }));

    let tab = app.active_browser_tab();
    assert!(tab.loading.is_none());
    let preview = tab.request_preview.as_ref().expect("failed preview");
    assert_eq!(preview.status, BrowserRequestStatus::Failed);
    assert_eq!(
        preview.request_data.get("field_body").map(String::as_str),
        Some("post body")
    );
    let retry = tab.retry_state.as_ref().expect("manual retry state");
    assert_eq!(retry.target, target);
    assert_eq!(retry.attempts, 0);
    assert!(retry.ready_epoch_ms.is_some());
    assert!(retry.reason.contains("press Retry"));
    assert!(!retry.reason.contains("automatic"));
    assert!(!app.status.task.contains("retrying once"));
}

#[test]
fn successful_browser_load_clears_live_warning() {
    let mut app = App::new(test_config("live-warning-clear"));
    let tab_id = app.active_browser_tab().id;
    app.active_browser_tab_mut().live_warning = Some(BrowserLiveWarning {
        target: fixture_node_url(),
        visible_page: Some("mock.node:/ (mock)".into()),
        message: "previous failure".into(),
        next_action: "retry".into(),
    });
    app.active_browser_tab_mut().loading = Some(LoadState {
        generation: 3,
        target: "mock.node:/page/gallery.mu".into(),
        submitted_fields: false,
        cancel: CancellationToken::new(),
    });
    let mut session = app.active_browser_tab().session.clone();
    let page = BrowserPage::mock_home("mock.node:/page/gallery.mu");
    session.restore_page(page.clone(), vec![page.url.clone()], 0);

    assert!(app.apply_browser_task_result(BrowserTaskResult::Page {
        tab_id,
        generation: 3,
        session: Box::new(session),
        page,
    }));

    assert!(app.active_browser_live_warning().is_none());
}

#[tokio::test]
async fn browser_open_defers_fetch_until_path_is_ready() {
    let mut app = App::new(test_config("browser-open-path-gate"));
    app.runtime_status.connected = true;
    let destination = FIXTURE_NODE_HASH;
    let target = fixture_node_url();

    app.open_active_browser_target(target.clone());

    assert!(app.active_browser_tab().loading.is_none());
    assert_eq!(app.monitoring_state.outbound_page_requests, 0);
    let preview = app
        .active_browser_tab()
        .request_preview
        .as_ref()
        .expect("pending preview");
    assert_eq!(preview.status, BrowserRequestStatus::Pending);
    assert_eq!(preview.target, target);
    assert!(preview.detail.contains("requesting path before page load"));
    let retry = app
        .active_browser_tab()
        .retry_state
        .as_ref()
        .expect("path-gated retry");
    assert_eq!(retry.target, target);
    assert_eq!(retry.destination_hash, destination);
    assert!(retry.reason.contains("auto-load when path is known"));

    assert!(
        app.handle_internal_event(InternalAppEvent::Runtime(RuntimeBusEvent::PathUpdated(
            PathEvent {
                destination_hash: destination.into(),
                known: true,
                hops: Some(1),
            }
        )))
    );
    assert!(app
        .active_browser_tab()
        .retry_state
        .as_ref()
        .is_some_and(|state| state.ready_epoch_ms.is_some()));
    app.active_browser_tab_mut()
        .retry_state
        .as_mut()
        .expect("retry")
        .retry_after_epoch_ms = current_epoch_ms();
    assert!(
        app.handle_internal_event(InternalAppEvent::StartupBrowserPathReady {
            destination: destination.into(),
            target: None,
            requested_epoch_ms: None,
        })
    );

    let loading = app.active_browser_tab().loading.as_ref().expect("loading");
    assert_eq!(loading.target, target);
    assert_eq!(app.monitoring_state.outbound_page_requests, 1);
    let autoload_log = app
        .logs
        .entries
        .iter()
        .find(|entry| {
            entry
                .message
                .contains("browser path-ready auto-load queued")
        })
        .expect("path-ready auto-load log");
    assert_eq!(autoload_log.severity, LogSeverity::Debug);
}

#[test]
fn browser_path_request_timeout_marks_pending_preview_failed() {
    let mut app = App::new(test_config("browser-path-timeout"));
    let destination = FIXTURE_NODE_HASH;
    let target = fixture_node_url();
    let tab_id = app.active_browser_tab().id;
    let now = current_epoch_ms();
    app.active_browser_tab_mut().retry_state = Some(BrowserRetryState {
        target: target.clone(),
        destination_hash: destination.into(),
        reason: "browser navigation path request queued; auto-load when path is known".into(),
        requested_epoch_ms: now,
        retry_after_epoch_ms: now.saturating_add(5_000),
        ready_epoch_ms: None,
        attempts: 0,
    });
    app.active_browser_tab_mut().request_preview = Some(BrowserRequestPreview {
        target: target.clone(),
        fields: Vec::new(),
        request_data: BTreeMap::new(),
        status: BrowserRequestStatus::Pending,
        detail: "requesting path before page load".into(),
    });

    assert!(
        app.handle_internal_event(InternalAppEvent::BrowserPathRequestTimedOut {
            tab_id,
            destination: destination.into(),
            target,
            requested_epoch_ms: Some(now),
        })
    );

    let preview = app
        .active_browser_tab()
        .request_preview
        .as_ref()
        .expect("preview");
    assert_eq!(preview.status, BrowserRequestStatus::Failed);
    assert!(preview.detail.contains("did not produce ready path"));
}

#[test]
fn stale_browser_path_request_timeout_does_not_fail_newer_retry() {
    let mut app = App::new(test_config("browser-stale-path-timeout"));
    let destination = FIXTURE_NODE_HASH;
    let target = fixture_node_url();
    let tab_id = app.active_browser_tab().id;
    let old_requested = current_epoch_ms();
    let new_requested = old_requested.saturating_add(5_000);
    app.active_browser_tab_mut().retry_state = Some(BrowserRetryState {
        target: target.clone(),
        destination_hash: destination.into(),
        reason: "browser navigation path request queued; auto-load when path is known".into(),
        requested_epoch_ms: new_requested,
        retry_after_epoch_ms: new_requested.saturating_add(5_000),
        ready_epoch_ms: None,
        attempts: 0,
    });
    app.active_browser_tab_mut().request_preview = Some(BrowserRequestPreview {
        target: target.clone(),
        fields: Vec::new(),
        request_data: BTreeMap::new(),
        status: BrowserRequestStatus::Pending,
        detail: "newer request is still waiting for path".into(),
    });

    assert!(
        !app.handle_internal_event(InternalAppEvent::BrowserPathRequestTimedOut {
            tab_id,
            destination: destination.into(),
            target,
            requested_epoch_ms: Some(old_requested),
        })
    );

    let preview = app
        .active_browser_tab()
        .request_preview
        .as_ref()
        .expect("preview");
    assert_eq!(preview.status, BrowserRequestStatus::Pending);
    assert_eq!(preview.detail, "newer request is still waiting for path");
}

#[tokio::test]
async fn stale_browser_path_ready_event_does_not_autoload_newer_retry() {
    let mut app = App::new(test_config("browser-stale-path-ready"));
    let destination = FIXTURE_NODE_HASH;
    let target = fixture_node_url();
    let old_requested = current_epoch_ms();
    let new_requested = old_requested.saturating_add(5_000);
    app.active_browser_tab_mut().retry_state = Some(BrowserRetryState {
        target: target.clone(),
        destination_hash: destination.into(),
        reason: "browser path request passed; waiting briefly before page load".into(),
        requested_epoch_ms: new_requested,
        retry_after_epoch_ms: current_epoch_ms(),
        ready_epoch_ms: Some(current_epoch_ms()),
        attempts: 0,
    });

    assert!(
        !app.handle_internal_event(InternalAppEvent::StartupBrowserPathReady {
            destination: destination.into(),
            target: Some(target.clone()),
            requested_epoch_ms: Some(old_requested),
        })
    );
    assert!(app.active_browser_tab().loading.is_none());

    assert!(
        app.handle_internal_event(InternalAppEvent::StartupBrowserPathReady {
            destination: destination.into(),
            target: Some(target.clone()),
            requested_epoch_ms: Some(new_requested),
        })
    );
    let loading = app.active_browser_tab().loading.as_ref().expect("loading");
    assert_eq!(loading.target, target);
}

#[tokio::test]
async fn browser_link_path_ready_does_not_plain_load_without_request_snapshot() {
    let mut app = App::new(test_config("browser-link-ready-missing-snapshot"));
    let destination = FIXTURE_NODE_HASH;
    let target = fixture_node_url();
    let now = current_epoch_ms();
    app.active_browser_tab_mut().retry_state = Some(BrowserRetryState {
        target: target.clone(),
        destination_hash: destination.into(),
        reason: "browser link path request queued; auto-load when path is known".into(),
        requested_epoch_ms: now,
        retry_after_epoch_ms: now,
        ready_epoch_ms: Some(now),
        attempts: 0,
    });

    assert!(
        !app.handle_internal_event(InternalAppEvent::StartupBrowserPathReady {
            destination: destination.into(),
            target: Some(target),
            requested_epoch_ms: Some(now),
        })
    );

    assert!(app.active_browser_tab().loading.is_none());
    assert!(app
        .status
        .task
        .contains("deferred link request snapshot is no longer available"));
    assert!(app.active_browser_tab().request_preview.is_none());
    assert!(app.active_browser_tab().retry_state.is_none());
}

#[tokio::test]
async fn browser_retry_uses_ready_retry_state_without_probe_summary() {
    let mut app = App::new(test_config("browser-retry-ready-state"));
    app.switch_section(WorkspaceSection::Browser);
    app.runtime_status.connected = true;
    let destination = FIXTURE_NODE_HASH;
    let target = fixture_node_url();
    let now = current_epoch_ms();
    app.active_browser_tab_mut().address_input = target.clone();
    app.active_browser_tab_mut().probe_summary = None;
    app.active_browser_tab_mut().retry_state = Some(BrowserRetryState {
        target: target.clone(),
        destination_hash: destination.into(),
        reason: "browser path request passed; waiting briefly before page load".into(),
        requested_epoch_ms: now,
        retry_after_epoch_ms: now,
        ready_epoch_ms: Some(now),
        attempts: 0,
    });

    assert!(app.retry_active_browser_after_path_discovery());

    let loading = app.active_browser_tab().loading.as_ref().expect("loading");
    assert_eq!(loading.target, target);
    assert_eq!(
        app.active_browser_tab()
            .retry_state
            .as_ref()
            .map(|retry| retry.attempts),
        Some(1)
    );
}

#[tokio::test]
async fn browser_open_with_ready_retry_state_does_not_requeue_path() {
    let mut app = App::new(test_config("browser-open-ready-state"));
    app.runtime_status.connected = true;
    let destination = FIXTURE_NODE_HASH;
    let target = fixture_node_url();
    let now = current_epoch_ms();
    app.active_browser_tab_mut().retry_state = Some(BrowserRetryState {
        target: target.clone(),
        destination_hash: destination.into(),
        reason: "browser path request passed; waiting briefly before page load".into(),
        requested_epoch_ms: now.saturating_sub(2_000),
        retry_after_epoch_ms: now,
        ready_epoch_ms: Some(now.saturating_sub(1_000)),
        attempts: 0,
    });

    app.open_active_browser_target(target.clone());

    let loading = app.active_browser_tab().loading.as_ref().expect("loading");
    assert_eq!(loading.target, target);
    assert_eq!(
        app.active_browser_tab()
            .retry_state
            .as_ref()
            .and_then(|state| state.ready_epoch_ms),
        Some(now.saturating_sub(1_000))
    );
}

#[tokio::test]
async fn request_path_for_pending_clicked_link_uses_preview_target() {
    let mut app = App::new(test_config("browser-link-path-request-target"));
    app.runtime_status.connected = true;
    let destination = FIXTURE_NODE_HASH;
    let target = fixture_node_url();
    let now = current_epoch_ms();
    app.active_browser_tab_mut().address_input = "mock.node:/page/index.mu".into();
    app.active_browser_tab_mut().retry_state = Some(BrowserRetryState {
        target: target.clone(),
        destination_hash: destination.into(),
        reason: "browser link path request queued; auto-load when path is known".into(),
        requested_epoch_ms: now,
        retry_after_epoch_ms: now.saturating_add(5_000),
        ready_epoch_ms: None,
        attempts: 0,
    });
    app.active_browser_tab_mut().request_preview = Some(BrowserRequestPreview {
        target: target.clone(),
        fields: vec!["message".into()],
        request_data: BTreeMap::from([("field_message".into(), "hello".into())]),
        status: BrowserRequestStatus::Pending,
        detail: "requesting path before link request".into(),
    });

    assert!(app.warm_active_browser_path());

    let warmup = app
        .active_browser_tab()
        .path_warmup
        .as_ref()
        .expect("path warmup");
    assert_eq!(warmup.target, target);
    let summary = app
        .active_browser_tab()
        .probe_summary
        .as_ref()
        .expect("probe summary");
    assert_eq!(summary.url, target);
}

#[tokio::test]
async fn browser_retry_after_path_discovery_reuses_cancelable_page_load() {
    let mut app = App::new(test_config("browser-retry-after-path"));
    app.switch_section(WorkspaceSection::Browser);
    app.active_browser_tab_mut().address_input = "mock.node:/page/gallery.mu".into();
    app.active_browser_tab_mut().probe_summary = Some(BrowserProbeSummary {
        url: "mock.node:/page/gallery.mu".into(),
        mode: "path-discovery".into(),
        ready_to_request: true,
        status: "path known".into(),
        detail: "path discovery reports a known route".into(),
    });

    assert!(app.retry_active_browser_after_path_discovery());
    assert!(app.active_browser_tab().loading.is_some());
    assert!(app.status.task.contains("retrying browser load"));
    assert!(app.wait_for_browser_task_result().await);

    let tab = app.active_browser_tab();
    assert!(tab.loading.is_none());
    assert_eq!(tab.title, "Micron Gallery");
    assert_eq!(tab.address_input, "mock.node:/page/gallery.mu");
    assert!(app.status.task.starts_with("mock page:"));
}

#[test]
fn browser_retry_requires_path_or_probe_ready_state() {
    let mut app = App::new(test_config("browser-retry-guard"));
    app.switch_section(WorkspaceSection::Browser);
    assert!(!app.retry_active_browser_after_path_discovery());
    assert!(app.status.task.contains("no browser probe"));

    app.active_browser_tab_mut().probe_summary = Some(BrowserProbeSummary {
        url: fixture_node_url(),
        mode: "path-discovery".into(),
        ready_to_request: false,
        status: "identity unknown".into(),
        detail: "destination identity not known".into(),
    });
    assert!(!app.retry_active_browser_after_path_discovery());
    assert!(app.status.task.contains("retry blocked"));
    assert!(app.active_browser_tab().loading.is_none());
}
