use super::*;
use crate::app::{current_epoch_ms, App};
use crate::chat::{ChatSessionId, OmenChatDescriptor};
use crate::desktop::DesktopOmenChatTransport;
use crate::runtime::{OmenChatLinkClosed, RuntimeBusEvent};

const FIXTURE_CHAT_SERVER_HASH: &str = "00112233445566778899aabbccddeeff";

fn desktop_with_temp_root(name: &str) -> DesktopApp {
    let root = std::env::temp_dir().join(format!("{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let paths = crate::config::AppPaths::from_root(root);
    paths.ensure().expect("paths");
    let app = App::new(crate::config::AppConfig {
        paths,
        settings: crate::storage::settings::AppSettings::default(),
    });
    DesktopApp::new(app)
}

fn test_descriptor() -> OmenChatDescriptor {
    OmenChatDescriptor {
        server_destination: FIXTURE_CHAT_SERVER_HASH.into(),
        display_name: Some("Test OMENchat".into()),
        rooms_hint: vec!["lobby".into()],
        local_display_name: Some("tester".into()),
        ..OmenChatDescriptor::default()
    }
}

fn open_connected_session(
    desktop: &mut DesktopApp,
    link_id: [u8; 16],
    status: &str,
) -> ChatSessionId {
    let session_id = desktop.open_omenchat_status_session(test_descriptor(), status.into());
    desktop.omenchat.omenchat_live_transports.insert(
        session_id,
        DesktopOmenChatTransport::new(link_id, current_epoch_ms()),
    );
    desktop
        .omenchat
        .omenchat_link_sessions
        .insert(link_id, session_id);
    session_id
}

fn enqueue_close(desktop: &mut DesktopApp, link_id: [u8; 16], reason: &str) {
    assert!(desktop
        .app
        .enqueue_runtime_event(RuntimeBusEvent::OmenChatLinkClosed(OmenChatLinkClosed {
            link_id,
            reason: Some(reason.into()),
        })));
    assert_eq!(desktop.app.drain_internal_events(), 1);
}

#[test]
fn omenchat_timeout_close_marks_session_for_quick_reconnect() {
    let mut desktop = desktop_with_temp_root("omenbrowser-rs-desktop-omenchat-timeout-close");
    let link_id = [0x42; 16];
    let session_id = open_connected_session(&mut desktop, link_id, "connected");
    enqueue_close(&mut desktop, link_id, "Timeout");

    let _ = desktop.drain_omenchat_runtime_events();

    assert!(!desktop
        .omenchat
        .omenchat_live_transports
        .contains_key(&session_id));
    assert!(desktop
        .omenchat
        .omenchat_live_retry_after
        .contains_key(&session_id));
    assert_eq!(
        desktop
            .omenchat
            .omenchat_live_last_disconnect_reason
            .get(&session_id)
            .map(String::as_str),
        Some("Timeout")
    );
    let status = desktop
        .omenchat
        .chat_client
        .session(session_id)
        .expect("session")
        .status
        .clone();
    assert!(status.contains("link timed out"));
    assert!(status.contains("reconnecting"));
}

#[test]
fn omenchat_destination_closed_marks_session_for_quick_reconnect() {
    let mut desktop = desktop_with_temp_root("omenbrowser-rs-desktop-omenchat-destination-closed");
    let link_id = [0x24; 16];
    let session_id = open_connected_session(&mut desktop, link_id, "connected");
    enqueue_close(&mut desktop, link_id, "DestinationClosed");

    let _ = desktop.drain_omenchat_runtime_events();

    assert!(!desktop
        .omenchat
        .omenchat_live_transports
        .contains_key(&session_id));
    assert!(desktop
        .omenchat
        .omenchat_live_retry_after
        .contains_key(&session_id));
    let status = desktop
        .omenchat
        .chat_client
        .session(session_id)
        .expect("session")
        .status
        .clone();
    assert!(status.contains("link closed"));
    assert!(status.contains("reconnecting"));
}

#[test]
fn omenchat_non_retryable_close_waits_for_manual_reconnect() {
    let mut desktop = desktop_with_temp_root("omenbrowser-rs-desktop-omenchat-non-retry-close");
    let link_id = [0x66; 16];
    let session_id = open_connected_session(&mut desktop, link_id, "connected");
    enqueue_close(&mut desktop, link_id, "ResourceExhausted");

    let _ = desktop.drain_omenchat_runtime_events();

    assert!(!desktop
        .omenchat
        .omenchat_live_transports
        .contains_key(&session_id));
    assert!(!desktop
        .omenchat
        .omenchat_live_retry_after
        .contains_key(&session_id));
    assert_eq!(
        desktop.omenchat_reconnect_state_label(session_id, current_epoch_ms()),
        "reconnect: manual"
    );
    let status = desktop
        .omenchat
        .chat_client
        .session(session_id)
        .expect("session")
        .status
        .clone();
    assert!(status.contains("use Reconnect"));
}

#[test]
fn omenchat_stale_link_close_does_not_disconnect_active_link() {
    let mut desktop = desktop_with_temp_root("omenbrowser-rs-desktop-omenchat-stale-link-close");
    let old_link_id = [0x11; 16];
    let active_link_id = [0x22; 16];
    let session_id =
        desktop.open_omenchat_status_session(test_descriptor(), "live userlist updated".into());
    desktop
        .omenchat
        .omenchat_link_sessions
        .insert(old_link_id, session_id);
    let _ = desktop.register_omenchat_live_transport(
        session_id,
        DesktopOmenChatTransport::new(active_link_id, current_epoch_ms()),
    );
    desktop
        .omenchat
        .omenchat_link_sessions
        .insert(old_link_id, session_id);
    enqueue_close(&mut desktop, old_link_id, "Timeout");

    let _ = desktop.drain_omenchat_runtime_events();

    assert_eq!(
        desktop
            .omenchat
            .omenchat_live_transports
            .get(&session_id)
            .map(|transport| transport.link_id),
        Some(active_link_id)
    );
    assert!(!desktop
        .omenchat
        .omenchat_live_retry_after
        .contains_key(&session_id));
    assert_eq!(
        desktop
            .omenchat
            .chat_client
            .session(session_id)
            .expect("session")
            .status,
        "live userlist updated"
    );
}
