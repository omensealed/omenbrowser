use super::*;
use crate::app::{current_epoch_ms, App};
use crate::chat::{ChatSessionId, OmenChatDescriptor};
use crate::chat::rns::ChatLinkTransport;
use crate::desktop::{hex_bytes, DesktopOmenChatTransport};
use crate::runtime::{
    OmenChatLinkClosed, ResourceLifecycleEvent, ResourceLifecycleState, RuntimeBusEvent,
};

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

#[cfg(feature = "omenchat-moderation-audit")]
fn authorize_moderation_audit(desktop: &mut DesktopApp, session_id: ChatSessionId) {
    let server_id = desktop
        .omenchat
        .chat_client
        .session(session_id)
        .expect("session")
        .server
        .server_id
        .clone();
    let session = desktop
        .omenchat
        .chat_client
        .session_mut(session_id)
        .expect("session");
    session.active_room.joined = true;
    session.users = vec![crate::chat::ChatUserSummary {
        server_id,
        user_id: 7,
        display_name: "Moderator".into(),
        role_bits: crate::chat::CHAT_ROLE_MODERATOR,
        status_bits: 0,
        lxmf_available: false,
    }];
    assert!(desktop
        .omenchat
        .chat_client
        .bind_local_user_id(session_id, 7));
    desktop
        .omenchat
        .omenchat_live_state
        .set_moderation_audit_negotiated_for_test(session_id, true);
    desktop.set_omenchat_connection_state(session_id, crate::chat::ChatConnectionState::Joined);
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

fn enqueue_inbound_resource_terminal(
    desktop: &mut DesktopApp,
    link_id: [u8; 16],
    state: ResourceLifecycleState,
) {
    assert!(desktop
        .app
        .enqueue_runtime_event(RuntimeBusEvent::ResourceLifecycle(
            ResourceLifecycleEvent {
                transfer_id: "resource-hash".into(),
                state,
                bytes: None,
                reason: Some("test terminal".into()),
                operation_id: None,
                source: Some("omenchat".into()),
                purpose: Some("omenchat-resource".into()),
                direction: Some("inbound".into()),
                peer: Some(hex_bytes(&link_id)),
            },
        )));
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
    assert_eq!(
        desktop.omenchat_connection_state(session_id),
        crate::chat::ChatConnectionState::Reconnecting
    );
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
    assert_eq!(
        desktop.omenchat_connection_state(session_id),
        crate::chat::ChatConnectionState::Disconnected
    );
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

#[test]
fn omenchat_inbound_resource_failure_releases_pending_offers_but_keeps_link() {
    let mut desktop = desktop_with_temp_root("omenbrowser-rs-omenchat-resource-terminal");
    let link_id = [0x81; 16];
    let session_id = open_connected_session(&mut desktop, link_id, "waiting for resource");
    let transport = desktop
        .omenchat
        .omenchat_live_transports
        .get_mut(&session_id)
        .expect("live transport");
    transport
        .defer_resource_offer("history:one", vec![0x01, 0x02])
        .expect("pending history offer");
    transport
        .defer_resource_offer("userlist:two", vec![0x03])
        .expect("pending user-list offer");

    enqueue_inbound_resource_terminal(&mut desktop, link_id, ResourceLifecycleState::Failed);
    let _ = desktop.drain_omenchat_runtime_events();

    let transport = desktop
        .omenchat
        .omenchat_live_transports
        .get(&session_id)
        .expect("failure must not close a healthy link");
    assert_eq!(transport.pending_resource_offer_count(), 0);
    assert_eq!(transport.pending_resource_offer_bytes, 0);
    assert_eq!(transport.link_id, link_id);
    let status = &desktop
        .omenchat
        .chat_client
        .session(session_id)
        .expect("session")
        .status;
    assert!(status.contains("failed"));
    assert!(status.contains("released 2 pending offer(s)"));
    assert!(status.contains("retry history or reconnect"));
}

#[test]
fn omenchat_inbound_resource_cancellation_releases_pending_offer_but_keeps_link() {
    let mut desktop = desktop_with_temp_root("omenbrowser-rs-omenchat-resource-cancelled");
    let link_id = [0x82; 16];
    let session_id = open_connected_session(&mut desktop, link_id, "waiting for resource");
    desktop
        .omenchat
        .omenchat_live_transports
        .get_mut(&session_id)
        .expect("live transport")
        .defer_resource_offer("history:cancelled", vec![0x01])
        .expect("pending history offer");

    enqueue_inbound_resource_terminal(
        &mut desktop,
        link_id,
        ResourceLifecycleState::Cancelled,
    );
    let _ = desktop.drain_omenchat_runtime_events();

    let transport = desktop
        .omenchat
        .omenchat_live_transports
        .get(&session_id)
        .expect("cancellation must not close a healthy link");
    assert_eq!(transport.pending_resource_offer_count(), 0);
    assert_eq!(transport.pending_resource_offer_bytes, 0);
    let status = &desktop
        .omenchat
        .chat_client
        .session(session_id)
        .expect("session")
        .status;
    assert!(status.contains("was cancelled"));
    assert!(status.contains("released 1 pending offer(s)"));
}

#[cfg(feature = "omenchat-moderation-audit")]
#[tokio::test]
async fn moderation_audit_refresh_is_single_flight_and_end_marks_page_current() {
    let mut desktop = desktop_with_temp_root("omenbrowser-rs-moderation-audit-refresh");
    let link_id = [0x91; 16];
    let session_id = open_connected_session(&mut desktop, link_id, "connected");
    authorize_moderation_audit(&mut desktop, session_id);

    desktop.refresh_omenchat_moderation_audit(session_id);

    assert!(matches!(
        desktop
            .omenchat
            .omenchat_moderation_audit_requests
            .get(&session_id),
        Some(super::super::omenchat_desktop_state::OmenChatModerationAuditRequest {
            room_id: 1,
            owner_user_id: 7,
            state: crate::chat::ChatModerationAuditRequestState::Receiving,
        })
    ));
    desktop.refresh_omenchat_moderation_audit(session_id);
    assert_eq!(
        desktop
            .omenchat
            .chat_client
            .session(session_id)
            .expect("session")
            .status,
        "moderation audit request already receiving"
    );

    desktop.apply_omenchat_client_events_status(&[crate::chat::ChatClientEvent::ModerationAuditEnd {
        session_id,
        room_id: 1,
    }]);
    assert!(matches!(
        desktop
            .omenchat
            .omenchat_moderation_audit_requests
            .get(&session_id),
        Some(super::super::omenchat_desktop_state::OmenChatModerationAuditRequest {
            room_id: 1,
            owner_user_id: 7,
            state: crate::chat::ChatModerationAuditRequestState::Complete,
        })
    ));
    assert!(desktop
        .omenchat
        .chat_client
        .session(session_id)
        .expect("session")
        .status
        .contains("current: 0 record(s)"));
}

#[cfg(feature = "omenchat-moderation-audit")]
#[test]
fn moderation_audit_evidence_is_cleared_when_link_authority_is_lost() {
    let mut desktop = desktop_with_temp_root("omenbrowser-rs-moderation-audit-authority-loss");
    let session_id = open_connected_session(&mut desktop, [0x92; 16], "connected");
    authorize_moderation_audit(&mut desktop, session_id);
    desktop.omenchat.omenchat_moderation_audit_requests.insert(
        session_id,
        super::super::omenchat_desktop_state::OmenChatModerationAuditRequest {
            room_id: 1,
            owner_user_id: 7,
            state: crate::chat::ChatModerationAuditRequestState::Complete,
        },
    );

    desktop.set_omenchat_connection_state(
        session_id,
        crate::chat::ChatConnectionState::Reconnecting,
    );

    assert!(!desktop
        .omenchat
        .omenchat_moderation_audit_requests
        .contains_key(&session_id));
}

#[cfg(feature = "omenchat-moderation-audit")]
#[test]
fn moderation_audit_evidence_is_cleared_on_immediate_local_role_loss() {
    let mut desktop = desktop_with_temp_root("omenbrowser-rs-moderation-audit-role-loss");
    let session_id = open_connected_session(&mut desktop, [0x93; 16], "connected");
    authorize_moderation_audit(&mut desktop, session_id);
    desktop
        .omenchat
        .chat_client
        .replace_moderation_audit_page(
            session_id,
            1,
            crate::chat::protocol::ModerationAuditPage {
                records: vec![crate::chat::protocol::ModerationAuditRecord {
                    audit_id: 1,
                    room_id: 1,
                    actor_user_id: 7,
                    actor_display_name_at_action: "Moderator".into(),
                    target_user_id: Some(8),
                    target_display_name_at_action: Some("Member".into()),
                    action: crate::chat::protocol::ModerationAuditAction::Kick,
                    committed_at_unix: 1,
                    result_role_bits: None,
                    result_status_bits: None,
                }],
            },
        )
        .expect("bounded page");
    desktop.omenchat.omenchat_moderation_audit_requests.insert(
        session_id,
        super::super::omenchat_desktop_state::OmenChatModerationAuditRequest {
            room_id: 1,
            owner_user_id: 7,
            state: crate::chat::ChatModerationAuditRequestState::Complete,
        },
    );
    let demoted_user = crate::chat::ChatUserSummary {
        role_bits: 0,
        ..desktop
            .omenchat
            .chat_client
            .session(session_id)
            .expect("session")
            .users[0]
            .clone()
    };
    desktop
        .omenchat
        .chat_client
        .session_mut(session_id)
        .expect("session")
        .users[0] = demoted_user.clone();

    desktop.apply_omenchat_client_events_status(&[crate::chat::ChatClientEvent::UserUpdated {
        session_id,
        user: demoted_user,
    }]);

    assert!(desktop
        .omenchat
        .chat_client
        .moderation_audit_page(session_id, 1)
        .is_none());
    assert!(!desktop
        .omenchat
        .omenchat_moderation_audit_requests
        .contains_key(&session_id));
}

#[cfg(feature = "omenchat-moderation-audit")]
#[test]
fn moderation_audit_evidence_is_identity_scoped() {
    let mut desktop = desktop_with_temp_root("omenbrowser-rs-moderation-audit-identity-change");
    let session_id = open_connected_session(&mut desktop, [0x94; 16], "connected");
    authorize_moderation_audit(&mut desktop, session_id);
    desktop.omenchat.omenchat_moderation_audit_requests.insert(
        session_id,
        super::super::omenchat_desktop_state::OmenChatModerationAuditRequest {
            room_id: 1,
            owner_user_id: 7,
            state: crate::chat::ChatModerationAuditRequestState::Complete,
        },
    );

    desktop.apply_omenchat_client_events_status(&[crate::chat::ChatClientEvent::LocalUserBound {
        session_id,
        user_id: 9,
    }]);

    assert_eq!(
        desktop.omenchat.chat_client.local_user_id(session_id),
        Some(9)
    );
    assert!(!desktop
        .omenchat
        .omenchat_moderation_audit_requests
        .contains_key(&session_id));
}
