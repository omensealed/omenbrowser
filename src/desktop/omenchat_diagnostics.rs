use iced::Task;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::chat::ChatSessionId;

use super::{hex_bytes, DesktopApp, Message};

const OMENCHAT_SESSION_DIAGNOSTICS_MAX_BYTES: usize = 8 * 1024;

impl DesktopApp {
    pub(in crate::desktop) fn omenchat_session_diagnostics_text(
        &self,
        session_id: ChatSessionId,
    ) -> Option<String> {
        let session = self.omenchat.chat_client.session(session_id)?;
        let connection_state = self.omenchat_connection_state(session_id);
        let directory_identity = self
            .app
            .directory_service
            .find(&session.server.destination)
            .and_then(|entry| entry.identity_hash);
        let pending_messages = self
            .omenchat
            .omenchat_live_state
            .pending_local_echo_metrics();
        let pending_uploads = self.omenchat.omenchat_live_state.pending_upload_metrics();
        let inline_downloads = self.omenchat.omenchat_live_state.inline_download_metrics();
        let now_unix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .ok()
            .and_then(|duration| i64::try_from(duration.as_secs()).ok())
            .unwrap_or_default();
        let (recovered_prepared, recovered_uncertain, recovered_past_expiry) = self
            .omenchat
            .omenchat_recovered_mutation_intents
            .iter()
            .filter(|intent| intent.server_destination == session.server.destination)
            .fold((0usize, 0usize, 0usize), |counts, intent| {
                (
                    counts.0
                        + usize::from(
                            intent.state
                                == crate::chat::mutation_intents::OutboundMutationState::Prepared,
                        ),
                    counts.1
                        + usize::from(
                            intent.state
                                == crate::chat::mutation_intents::OutboundMutationState::SentUncertain,
                        ),
                    counts.2 + usize::from(intent.expires_at <= now_unix),
                )
            });
        let intent_worker = self
            .omenchat
            .omenchat_mutation_intent_worker
            .as_ref()
            .map(|worker| worker.metrics());
        let last_disconnect_category = self
            .omenchat
            .omenchat_live_last_disconnect_reason
            .get(&session_id)
            .map(|reason| omenchat_disconnect_category(reason));
        let transport = self
            .omenchat
            .omenchat_live_transports
            .get(&session_id)
            .map(|transport| {
                serde_json::json!({
                    "connected": true,
                    "link_id": hex_bytes(&transport.link_id),
                    "connected_since_epoch_ms": transport.connected_since_epoch_ms,
                    "last_rx_epoch_ms": transport.last_rx_epoch_ms,
                    "last_tx_epoch_ms": transport.last_tx_epoch_ms,
                    "awaiting_pong": transport.awaiting_pong,
                    "last_ping_rtt_ms": transport.last_ping_rtt_ms,
                    "frames_in": transport.frames_in,
                    "frames_out": transport.frames_out,
                    "bytes_in": transport.bytes_in,
                    "bytes_out": transport.bytes_out,
                    "incoming_frame_queue_items": transport.incoming_frames.len(),
                    "incoming_frame_queue_bytes": transport.incoming_frame_bytes,
                    "outgoing_frame_queue_items": transport.outgoing_frames.len(),
                    "outgoing_frame_queue_bytes": transport.outgoing_frame_bytes,
                    "pending_resource_items": transport.pending_resource_offer_count(),
                    "pending_resource_bytes": transport.pending_resource_offer_bytes,
                    "outgoing_resource_items": transport.outgoing_resources.len(),
                    "outgoing_resource_bytes": transport.outgoing_resource_bytes,
                    "rejected_incoming_frames": transport.rejected_incoming_frames,
                    "rejected_outgoing_frames": transport.rejected_outgoing_frames,
                    "rejected_outgoing_resources": transport.rejected_outgoing_resources,
                    "rejected_resources": transport.rejected_resources,
                    "rejected_resource_offers": transport.rejected_resource_offers,
                })
            })
            .unwrap_or_else(|| serde_json::json!({ "connected": false }));
        let report = serde_json::json!({
            "report": "omenchat_session_diagnostics",
            "redacted": true,
            "application_version": env!("CARGO_PKG_VERSION"),
            "session": {
                "id": session_id,
                "server_display_name": session.server.display_name,
                "server_destination": session.server.destination,
                "announce_verified_identity": directory_identity,
                "active_room_id": session.active_room.room_id,
                "active_room_joined": session.active_room.joined,
                "known_room_count": session.rooms.len(),
                "retained_event_count": session.events.len(),
            },
            "connection": {
                "state": connection_state.label(),
                "automatic_retryable": connection_state.retryable(),
                "manual_reconnect_allowed": connection_state.manual_reconnect_allowed(),
                "opening": self.omenchat.omenchat_live_opening.contains(&session_id),
                "reconnect_scheduled": self.omenchat.omenchat_live_retry_after.contains_key(&session_id),
                "retry_attempts": self.omenchat.omenchat_live_retry_count.get(&session_id).copied().unwrap_or_default(),
                "connect_count": self.omenchat.omenchat_live_connect_count.get(&session_id).copied().unwrap_or_default(),
                "disconnect_count": self.omenchat.omenchat_live_disconnect_count.get(&session_id).copied().unwrap_or_default(),
                "last_disconnect_category": last_disconnect_category,
                "recent_history_sync_pending": self.omenchat.omenchat_recent_sync_pending.contains(&session_id),
            },
            "client_bounds": {
                "pending_messages_session": self.omenchat.omenchat_live_state.pending_local_echo_session_items(session_id),
                "pending_messages_global": pending_messages.items,
                "rejected_messages_global": pending_messages.rejected,
                "pending_upload_items": pending_uploads.items,
                "pending_upload_bytes": pending_uploads.bytes,
                "rejected_uploads": pending_uploads.rejected,
                "inline_download_items": inline_downloads.items,
                "inline_download_reserved_bytes": inline_downloads.reserved_bytes,
                "inline_download_retained_bytes": inline_downloads.retained_bytes,
                "inline_download_pending_chunks": inline_downloads.pending_chunks,
                "rejected_inline_downloads": inline_downloads.rejected,
            },
            "durable_mutations": {
                "negotiated_for_session": self.omenchat.omenchat_live_state.durable_mutations_negotiated(session_id),
                "persistence_owner_ready": self.omenchat.omenchat_live_state.durable_mutation_owner_ready(),
                "recovery_state": self.omenchat.omenchat_mutation_recovery_state.label(),
                "recovered_prepared_for_server": recovered_prepared,
                "recovered_uncertain_for_server": recovered_uncertain,
                "recovered_past_expiry_for_server": recovered_past_expiry,
                "other_identity_unresolved_count": self.omenchat.omenchat_other_identity_mutation_intents,
                "worker_queue_items": intent_worker.map(|metrics| metrics.queued),
                "worker_queue_bytes": intent_worker.map(|metrics| metrics.queued_bytes),
                "worker_rejections": intent_worker.map(|metrics| metrics.rejected),
            },
            "transport": transport,
            "omitted": [
                "message bodies",
                "composer drafts",
                "user list",
                "room names",
                "filenames and local paths",
                "credentials and private identity material",
                "free-form status and error text"
            ],
        });
        let text = serde_json::to_string_pretty(&report).ok()?;
        if text.len() > OMENCHAT_SESSION_DIAGNOSTICS_MAX_BYTES {
            return None;
        }
        Some(text)
    }

    pub(in crate::desktop) fn update_copy_omenchat_session_diagnostics(
        &mut self,
        session_id: ChatSessionId,
    ) -> Task<Message> {
        let Some(text) = self.omenchat_session_diagnostics_text(session_id) else {
            self.app.status.task = "could not create bounded OMENchat session diagnostics".into();
            return Task::none();
        };
        self.app.status.task = "copied redacted OMENchat session diagnostics".into();
        iced::clipboard::write(text)
    }
}

fn omenchat_disconnect_category(reason: &str) -> &'static str {
    let reason = reason.to_ascii_lowercase();
    if reason.contains("heartbeat") || reason.contains("pong") {
        "heartbeat-timeout"
    } else if reason.contains("timeout") {
        "transport-timeout"
    } else if reason.contains("destinationclosed") || reason.contains("destination closed") {
        "destination-closed"
    } else if reason.contains("resourceexhausted") || reason.contains("resource exhausted") {
        "resource-exhausted"
    } else if reason.contains("manual") || reason.contains("user") {
        "local-request"
    } else if reason.contains("cancel") {
        "cancelled"
    } else {
        "other-redacted"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::App;
    use crate::chat::OmenChatDescriptor;
    use crate::desktop::OmenChatMessage;

    fn desktop_with_session(name: &str) -> (DesktopApp, ChatSessionId) {
        let root = std::env::temp_dir().join(format!("{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let paths = crate::config::AppPaths::from_root(root);
        paths.ensure().expect("paths");
        let app = App::new(crate::config::AppConfig {
            paths,
            settings: crate::storage::settings::AppSettings::default(),
        });
        let mut desktop = DesktopApp::new(app);
        let session_id = desktop.open_omenchat_status_session(
            OmenChatDescriptor {
                server_destination: "00112233445566778899aabbccddeeff".into(),
                display_name: Some("Diagnostic Server".into()),
                rooms_hint: vec!["private-room-name".into()],
                ..OmenChatDescriptor::default()
            },
            "status includes /private/path and secret message body".into(),
        );
        desktop.omenchat.chat_drafts.insert(
            session_id,
            "private composer draft must not be exported".into(),
        );
        desktop
            .omenchat
            .omenchat_live_last_disconnect_reason
            .insert(
                session_id,
                "failure at /private/path token=secret message body".into(),
            );
        (desktop, session_id)
    }

    #[test]
    fn omenchat_session_diagnostics_are_bounded_structured_and_redacted() {
        let (desktop, session_id) =
            desktop_with_session("omenbrowser-rs-omenchat-copy-diagnostics");
        let text = desktop
            .omenchat_session_diagnostics_text(session_id)
            .expect("diagnostics");
        let report: serde_json::Value = serde_json::from_str(&text).expect("json");

        assert!(text.len() <= OMENCHAT_SESSION_DIAGNOSTICS_MAX_BYTES);
        assert_eq!(report["report"], "omenchat_session_diagnostics");
        assert_eq!(report["redacted"], true);
        assert_eq!(
            report["connection"]["last_disconnect_category"],
            "other-redacted"
        );
        assert_eq!(report["transport"]["connected"], false);
        assert!(text.contains("00112233445566778899aabbccddeeff"));
        for secret in [
            "/private/path",
            "token=secret",
            "message body",
            "private composer draft",
            "private-room-name",
        ] {
            assert!(!text.contains(secret), "diagnostics leaked {secret}");
        }
    }

    #[test]
    fn copy_omenchat_session_diagnostics_reports_clipboard_action_or_closed_session() {
        let (mut desktop, session_id) =
            desktop_with_session("omenbrowser-rs-omenchat-copy-diagnostics-action");
        let _ = desktop.update(Message::OmenChat(OmenChatMessage::CopySessionDiagnostics(
            session_id,
        )));
        assert_eq!(
            desktop.app.status.task,
            "copied redacted OMENchat session diagnostics"
        );

        desktop.close_omenchat_session(session_id);
        let _ = desktop.update(Message::OmenChat(OmenChatMessage::CopySessionDiagnostics(
            session_id,
        )));
        assert_eq!(
            desktop.app.status.task,
            "could not create bounded OMENchat session diagnostics"
        );
    }
}
