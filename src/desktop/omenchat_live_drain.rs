use iced::widget::scrollable::RelativeOffset;
use iced::Task;

use crate::app::current_epoch_ms;

use super::{
    hex_bytes, omenchat_close_reason_allows_quick_reconnect, omenchat_close_reason_is_timeout,
    omenchat_recent_sync_wants_bottom_restore, scroll_offset_is_at_bottom, DesktopApp, Message,
};

impl DesktopApp {
    pub(in crate::desktop) fn drain_omenchat_runtime_events(&mut self) -> Task<Message> {
        let now = current_epoch_ms();
        let mut scroll_tasks = Vec::new();
        for closed in self.app.drain_omenchat_link_closed() {
            let Some(session_id) = self.omenchat.omenchat_link_sessions.remove(&closed.link_id)
            else {
                continue;
            };
            let closed_link_is_active = self
                .omenchat
                .omenchat_live_transports
                .get(&session_id)
                .is_some_and(|transport| transport.link_id == closed.link_id);
            if !closed_link_is_active {
                tracing::debug!(
                    link_id = %hex_bytes(&closed.link_id),
                    session_id,
                    "ignored stale OMENchat link close"
                );
                continue;
            }
            self.omenchat.omenchat_live_transports.remove(&session_id);
            self.omenchat
                .omenchat_live_disconnect_count
                .entry(session_id)
                .and_modify(|count| *count = count.saturating_add(1))
                .or_insert(1);
            let reason = closed.reason.as_deref().unwrap_or("server disconnected");
            self.omenchat
                .omenchat_live_last_disconnect_reason
                .insert(session_id, reason.to_string());
            let quick_reconnect =
                omenchat_close_reason_allows_quick_reconnect(closed.reason.as_deref());
            let status = if quick_reconnect {
                let (attempt, retry_after) = self.schedule_omenchat_reconnect(session_id, now);
                let retry_status = if retry_after.is_some() {
                    format!("reconnecting with backoff (attempt {attempt})")
                } else {
                    format!("automatic reconnect paused after {attempt} attempts")
                };
                if omenchat_close_reason_is_timeout(closed.reason.as_deref()) {
                    format!("OMENchat link timed out; {retry_status} ({reason})")
                } else {
                    format!("OMENchat link closed; {retry_status} ({reason})")
                }
            } else {
                self.clear_omenchat_reconnect_state(session_id);
                format!("OMENchat disconnected: {reason}; use Reconnect to open a new link")
            };
            self.set_omenchat_connection_state(
                session_id,
                if quick_reconnect
                    && self
                        .omenchat
                        .omenchat_live_retry_after
                        .contains_key(&session_id)
                {
                    crate::chat::ChatConnectionState::Reconnecting
                } else if quick_reconnect {
                    crate::chat::ChatConnectionState::Failed { retryable: true }
                } else {
                    crate::chat::ChatConnectionState::Disconnected
                },
            );
            self.set_omenchat_session_status(session_id, status);
            self.persist_omenchat_session(session_id);
            self.app.status.task = format!(
                "OMENchat reconnect pending: {}",
                self.omenchat
                    .chat_client
                    .session(session_id)
                    .map(|session| session.server.display_name.as_str())
                    .unwrap_or("session")
            );
        }
        for terminal in self.app.drain_omenchat_resource_terminals() {
            let Some(peer) = terminal.peer.as_deref() else {
                continue;
            };
            let Some(session_id) = self.omenchat.omenchat_live_transports.iter().find_map(
                |(session_id, transport)| {
                    (hex_bytes(&transport.link_id) == peer).then_some(*session_id)
                },
            ) else {
                continue;
            };
            let released = self
                .omenchat
                .omenchat_live_transports
                .get_mut(&session_id)
                .map_or(0, |transport| transport.clear_pending_resource_offers());
            if released == 0 {
                continue;
            }
            let state = match terminal.state {
                crate::runtime::ResourceLifecycleState::Failed => "failed",
                crate::runtime::ResourceLifecycleState::Cancelled => "was cancelled",
                _ => continue,
            };
            let status = if matches!(
                (&terminal.state, terminal.reason.as_deref()),
                (
                    crate::runtime::ResourceLifecycleState::Failed,
                    Some("retry_limit_exhausted")
                )
            ) {
                format!(
                    "OMENchat transfer failed on the current route; attachment was not committed; upstream 0.9.9 routed Resource retransmission limits may apply; released {released} pending offer(s); no automatic retry occurred"
                )
            } else {
                format!(
                    "OMENchat inbound Resource {state}; released {released} pending offer(s); no automatic retry occurred"
                )
            };
            self.set_omenchat_session_status(session_id, status);
            self.persist_omenchat_session(session_id);
        }
        for data in self.app.drain_omenchat_link_data() {
            let Some(session_id) = self
                .omenchat
                .omenchat_link_sessions
                .get(&data.link_id)
                .copied()
            else {
                continue;
            };
            let frame_summary = Self::omenchat_frame_summary(&data.frame_bytes);
            if !Self::omenchat_frame_summary_is_heartbeat(&frame_summary) {
                tracing::debug!(
                    session_id,
                    link_id = %hex_bytes(&data.link_id),
                    bytes = data.frame_bytes.len(),
                    frame = %frame_summary,
                    "OMENchat received Link frame"
                );
            }
            let received_op = crate::chat::codec::decode_frame(&data.frame_bytes)
                .ok()
                .map(|frame| frame.op);
            let scroll_key = self.omenchat_scroll_key(session_id);
            let was_following_bottom = self
                .omenchat
                .chat_scroll_offsets
                .get(&scroll_key)
                .copied()
                .map(scroll_offset_is_at_bottom)
                .unwrap_or(true);
            let Some((admitted, events, pending_resources, outgoing, resources)) = self
                .omenchat
                .omenchat_live_transports
                .get_mut(&session_id)
                .map(|transport| {
                    let admitted = transport.push_incoming_frame(data.frame_bytes, now);
                    let events = crate::chat::live::drain_live_events_with_state(
                        &mut self.omenchat.chat_client,
                        &mut self.omenchat.omenchat_live_state,
                        transport,
                        Some(session_id),
                    );
                    let pending_resources = transport.pending_resource_offer_count();
                    let outgoing = transport.take_outgoing_frames();
                    let resources = transport.take_outgoing_resources();
                    (admitted, events, pending_resources, outgoing, resources)
                })
            else {
                continue;
            };
            if !admitted {
                self.set_omenchat_session_status(
                    session_id,
                    "rejected OMENchat frame outside the per-link receive queue budget".into(),
                );
            }
            if pending_resources > 0 {
                self.set_omenchat_session_status(
                    session_id,
                    format!("waiting for {pending_resources} OMENchat Resource payload(s)"),
                );
            }
            self.apply_omenchat_client_events_status(&events);
            scroll_tasks.extend(self.omenchat_mutation_persistence_tasks(&events));
            if was_following_bottom && omenchat_recent_sync_wants_bottom_restore(&events) {
                self.omenchat
                    .chat_scroll_offsets
                    .insert(scroll_key, RelativeOffset { x: 0.0, y: 1.0 });
                scroll_tasks.push(self.restore_omenchat_scroll(session_id));
            }
            self.send_omenchat_outgoing_frames(data.link_id, outgoing);
            self.send_omenchat_outgoing_resources(data.link_id, resources);
            if matches!(received_op, Some(crate::chat::protocol::ChatOp::Pong)) {
                self.schedule_omenchat_recent_sync_after_link_activity(session_id, now);
            }
            if !events.is_empty() {
                self.persist_omenchat_session(session_id);
            }
        }
        for data in self.app.drain_omenchat_resource_data() {
            let link_id = data.link_id;
            let Some(session_id) = self
                .omenchat
                .omenchat_link_sessions
                .get(&data.link_id)
                .copied()
            else {
                continue;
            };
            let scroll_key = self.omenchat_scroll_key(session_id);
            let was_following_bottom = self
                .omenchat
                .chat_scroll_offsets
                .get(&scroll_key)
                .copied()
                .map(scroll_offset_is_at_bottom)
                .unwrap_or(true);
            if let Some((events, accepted, pending_before, pending_after, outgoing, resources)) =
                self.omenchat
                    .omenchat_live_transports
                    .get_mut(&session_id)
                    .map(|transport| {
                        let pending_before = transport.pending_resource_offer_count();
                        let accepted = transport.push_resource(data.metadata, data.data, now);
                        let events = crate::chat::live::drain_live_events_with_state(
                            &mut self.omenchat.chat_client,
                            &mut self.omenchat.omenchat_live_state,
                            transport,
                            Some(session_id),
                        );
                        let pending_after = transport.pending_resource_offer_count();
                        let outgoing = transport.take_outgoing_frames();
                        let resources = transport.take_outgoing_resources();
                        (
                            events,
                            accepted,
                            pending_before,
                            pending_after,
                            outgoing,
                            resources,
                        )
                    })
            {
                if !accepted {
                    self.set_omenchat_session_status(
                        session_id,
                        "rejected OMENchat Resource outside bounded lifecycle policy".into(),
                    );
                }
                if pending_before > 0 && pending_after < pending_before {
                    self.set_omenchat_session_status(
                        session_id,
                        "received delayed OMENchat Resource payload".to_string(),
                    );
                }
                if pending_after > 0 {
                    self.set_omenchat_session_status(
                        session_id,
                        format!("waiting for {pending_after} OMENchat Resource payload(s)"),
                    );
                }
                self.apply_omenchat_client_events_status(&events);
                scroll_tasks.extend(self.omenchat_mutation_persistence_tasks(&events));
                if was_following_bottom && omenchat_recent_sync_wants_bottom_restore(&events) {
                    self.omenchat
                        .chat_scroll_offsets
                        .insert(scroll_key, RelativeOffset { x: 0.0, y: 1.0 });
                    scroll_tasks.push(self.restore_omenchat_scroll(session_id));
                }
                self.send_omenchat_outgoing_frames(link_id, outgoing);
                self.send_omenchat_outgoing_resources(link_id, resources);
                if !events.is_empty() {
                    self.persist_omenchat_session(session_id);
                }
            }
        }
        if scroll_tasks.is_empty() {
            Task::none()
        } else {
            Task::batch(scroll_tasks)
        }
    }
}
