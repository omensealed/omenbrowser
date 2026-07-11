#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
use std::collections::HashSet;

#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
use iced::widget::column;
#[cfg(not(any(feature = "chat-client-rns", feature = "chat-client-rns-clean")))]
use iced::widget::text;
use iced::Element;

#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
use crate::app::current_epoch_ms;
#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
use crate::chat::client::ChatSessionId;
use crate::desktop::{section_card, DesktopApp, Message};

#[cfg(not(any(feature = "chat-client-rns", feature = "chat-client-rns-clean")))]
use crate::desktop::ui_size;

#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
use crate::desktop::{
    compact_elapsed_ms, hex_bytes, human_bytes, short_destination_hash, wrapped_text_owned,
    OmenChatMediaLoadState, OMENCHAT_HEARTBEAT_IDLE_MS, OMENCHAT_MAX_HEARTBEAT_IDLE_MS,
    OMENCHAT_MIN_HEARTBEAT_IDLE_MS,
};

#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(in crate::desktop) struct OmenChatLiveMonitorTotals {
    pub(in crate::desktop) sessions: usize,
    pub(in crate::desktop) connected: usize,
    pub(in crate::desktop) opening: usize,
    pub(in crate::desktop) reconnect_timers: usize,
    pub(in crate::desktop) history_sync_waiting: usize,
    pub(in crate::desktop) pending_resources: usize,
    pub(in crate::desktop) frames_in: u64,
    pub(in crate::desktop) frames_out: u64,
    pub(in crate::desktop) bytes_in: u64,
    pub(in crate::desktop) bytes_out: u64,
    pub(in crate::desktop) resources_in: u64,
    pub(in crate::desktop) resource_bytes_in: u64,
    pub(in crate::desktop) upload_fetches_out: u64,
    pub(in crate::desktop) upload_resource_offers_in: u64,
    pub(in crate::desktop) upload_inline_chunks_in: u64,
    pub(in crate::desktop) upload_inline_bytes_in: u64,
    pub(in crate::desktop) upload_resources_in: u64,
    pub(in crate::desktop) upload_resource_bytes_in: u64,
    pub(in crate::desktop) awaiting_pongs: usize,
}

#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
pub(in crate::desktop) struct OmenChatSessionAttention<'a> {
    pub(in crate::desktop) connected: bool,
    pub(in crate::desktop) opening: bool,
    pub(in crate::desktop) reconnect_queued: bool,
    pub(in crate::desktop) awaiting_pong: bool,
    pub(in crate::desktop) last_ping_age_ms: Option<u64>,
    pub(in crate::desktop) heartbeat_idle_ms: Option<u64>,
    pub(in crate::desktop) pending_resources: usize,
    pub(in crate::desktop) history_sync_label: &'a str,
}

#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
pub(in crate::desktop) fn omenchat_live_monitor_totals(
    desktop: &DesktopApp,
) -> OmenChatLiveMonitorTotals {
    let mut history_sync_waiting: HashSet<ChatSessionId> = desktop
        .omenchat
        .omenchat_recent_sync_pending
        .iter()
        .copied()
        .collect();
    history_sync_waiting.extend(
        desktop
            .omenchat
            .omenchat_recent_sync_due_after
            .keys()
            .copied(),
    );
    let mut totals = OmenChatLiveMonitorTotals {
        sessions: desktop.omenchat.chat_client.sessions().len(),
        connected: desktop.omenchat.omenchat_live_transports.len(),
        opening: desktop.omenchat.omenchat_live_opening.len(),
        reconnect_timers: desktop.omenchat.omenchat_live_retry_after.len(),
        history_sync_waiting: history_sync_waiting.len(),
        ..OmenChatLiveMonitorTotals::default()
    };
    for transport in desktop.omenchat.omenchat_live_transports.values() {
        totals.pending_resources = totals
            .pending_resources
            .saturating_add(transport.pending_resource_offer_count());
        totals.frames_in = totals.frames_in.saturating_add(transport.frames_in);
        totals.frames_out = totals.frames_out.saturating_add(transport.frames_out);
        totals.bytes_in = totals.bytes_in.saturating_add(transport.bytes_in);
        totals.bytes_out = totals.bytes_out.saturating_add(transport.bytes_out);
        totals.resources_in = totals.resources_in.saturating_add(transport.resources_in);
        totals.resource_bytes_in = totals
            .resource_bytes_in
            .saturating_add(transport.resource_bytes_in);
        totals.upload_fetches_out = totals
            .upload_fetches_out
            .saturating_add(transport.upload_fetches_out);
        totals.upload_resource_offers_in = totals
            .upload_resource_offers_in
            .saturating_add(transport.upload_resource_offers_in);
        totals.upload_inline_chunks_in = totals
            .upload_inline_chunks_in
            .saturating_add(transport.upload_inline_chunks_in);
        totals.upload_inline_bytes_in = totals
            .upload_inline_bytes_in
            .saturating_add(transport.upload_inline_bytes_in);
        totals.upload_resources_in = totals
            .upload_resources_in
            .saturating_add(transport.upload_resources_in);
        totals.upload_resource_bytes_in = totals
            .upload_resource_bytes_in
            .saturating_add(transport.upload_resource_bytes_in);
        if transport.awaiting_pong {
            totals.awaiting_pongs = totals.awaiting_pongs.saturating_add(1);
        }
    }
    totals
}

#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
pub(in crate::desktop) fn omenchat_monitor_health_line(
    totals: &OmenChatLiveMonitorTotals,
) -> String {
    if totals.sessions == 0 {
        return "health: no OMENchat sessions open".into();
    }
    if totals.reconnect_timers > 0 || totals.opening > 0 {
        return format!(
            "health: reconnect/opening activity visible ({} opening, {} timer(s))",
            totals.opening, totals.reconnect_timers
        );
    }
    if totals.connected == 0 {
        return format!(
            "health: disconnected (0/{} session(s) connected); use Reconnect after path is known",
            totals.sessions
        );
    }
    if totals.connected < totals.sessions {
        return format!(
            "health: partial ({}/{} session(s) connected); check disconnected session rows",
            totals.connected, totals.sessions
        );
    }
    if totals.awaiting_pongs > 0 {
        return format!(
            "health: waiting for {} heartbeat pong(s); watch for repeated disconnects",
            totals.awaiting_pongs
        );
    }
    if totals.history_sync_waiting > 0 {
        return format!(
            "health: {} session(s) catching up recent history",
            totals.history_sync_waiting
        );
    }
    if totals.pending_resources > 0 {
        return format!(
            "health: {} pending Resource offer(s); media/history may still be loading",
            totals.pending_resources
        );
    }
    if totals.upload_fetches_out > 0
        || totals.upload_inline_chunks_in > 0
        || totals.upload_resources_in > 0
    {
        return format!(
            "health: media/upload traffic active ({} fetches, {} inline, {} resource bytes)",
            totals.upload_fetches_out,
            totals.upload_inline_chunks_in,
            human_bytes(totals.upload_resource_bytes_in)
        );
    }
    if totals.connected > 0 && totals.frames_in == 0 && totals.frames_out == 0 {
        return "health: connected and quiet; no OMENchat frames yet".into();
    }
    format!(
        "health: ok; {} connected session(s), {} rx / {} tx",
        totals.connected,
        human_bytes(totals.bytes_in),
        human_bytes(totals.bytes_out)
    )
}

#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
pub(in crate::desktop) fn omenchat_session_attention_line(
    attention: OmenChatSessionAttention<'_>,
) -> String {
    if attention.opening {
        return "attention: opening live OMENchat link".into();
    }
    if attention.reconnect_queued {
        return "attention: reconnect queued; waiting for retry timer".into();
    }
    if !attention.connected {
        return "attention: disconnected; use Reconnect after path is known".into();
    }
    if attention.awaiting_pong {
        let last_ping = attention
            .last_ping_age_ms
            .map(compact_elapsed_ms)
            .unwrap_or_else(|| "unknown".into());
        let heartbeat = attention
            .heartbeat_idle_ms
            .unwrap_or(OMENCHAT_HEARTBEAT_IDLE_MS);
        if attention
            .last_ping_age_ms
            .is_some_and(|age| age >= heartbeat.saturating_mul(2))
        {
            return format!("attention: heartbeat pong overdue; last ping {last_ping} ago");
        }
        return format!("attention: waiting for heartbeat pong; last ping {last_ping} ago");
    }
    if attention.history_sync_label.contains("stopped") {
        return "attention: recent history sync stopped; reconnect can retry catch-up".into();
    }
    if attention.history_sync_label.contains("waiting")
        || attention.history_sync_label.contains("retry")
        || attention.history_sync_label.contains("due now")
        || attention.history_sync_label.contains("not yet confirmed")
    {
        return "attention: recent history sync pending".into();
    }
    if attention.pending_resources > 0 {
        let pending_resources = attention.pending_resources;
        return format!(
            "attention: {pending_resources} pending Resource offer(s); media/history may still be loading"
        );
    }
    "attention: live link healthy; no action needed".into()
}

pub(in crate::desktop) fn omenchat_monitoring_card(desktop: &DesktopApp) -> Element<'_, Message> {
    #[cfg(not(any(feature = "chat-client-rns", feature = "chat-client-rns-clean")))]
    let _ = desktop;

    #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
    {
        let now = current_epoch_ms();
        let mut lines = column![].spacing(6);
        if desktop.omenchat.chat_client.sessions().is_empty() {
            lines = lines.push(wrapped_text_owned(
                "No OMENchat sessions are open. Open an omenchat:// destination to monitor live chat traffic.",
                14,
            ));
        } else {
            let media_total = desktop.omenchat.omenchat_media_cache.len();
            let media_loading = desktop
                .omenchat
                .omenchat_media_cache
                .values()
                .filter(|state| matches!(state, OmenChatMediaLoadState::Loading { .. }))
                .count();
            let media_cached = desktop
                .omenchat
                .omenchat_media_cache
                .values()
                .filter(|state| matches!(state, OmenChatMediaLoadState::Cached { .. }))
                .count();
            let media_failed = desktop
                .omenchat
                .omenchat_media_cache
                .values()
                .filter(|state| matches!(state, OmenChatMediaLoadState::Failed { .. }))
                .count();
            let totals = desktop.omenchat_live_monitor_totals();
            lines = lines.push(
                column![
                    wrapped_text_owned(omenchat_monitor_health_line(&totals), 13),
                    wrapped_text_owned(format!(
                        "summary: {} session(s) | {} connected | {} opening | {} reconnect timer(s) | {} history sync wait(s) | {} awaiting pong(s)",
                        totals.sessions,
                        totals.connected,
                        totals.opening,
                        totals.reconnect_timers,
                        totals.history_sync_waiting,
                        totals.awaiting_pongs
                    ), 13),
                    wrapped_text_owned(format!(
                        "traffic total: frames {} in / {} out | wire {} rx / {} tx | resources {} ({}) | pending resources {}",
                        totals.frames_in,
                        totals.frames_out,
                        human_bytes(totals.bytes_in),
                        human_bytes(totals.bytes_out),
                        totals.resources_in,
                        human_bytes(totals.resource_bytes_in),
                        totals.pending_resources
                    ), 13),
                    wrapped_text_owned(format!(
                        "upload total: fetches {} | offers {} | inline chunks {} ({}) | resources {} ({})",
                        totals.upload_fetches_out,
                        totals.upload_resource_offers_in,
                        totals.upload_inline_chunks_in,
                        human_bytes(totals.upload_inline_bytes_in),
                        totals.upload_resources_in,
                        human_bytes(totals.upload_resource_bytes_in)
                    ), 13),
                ]
                .spacing(2),
            );
            lines = lines.push(wrapped_text_owned(
                format!(
                    "media cache: {media_cached} cached / {media_loading} loading / {media_failed} failed ({media_total} tracked)"
                ),
                13,
            ));
            for session in desktop.omenchat.chat_client.sessions() {
                let connects = desktop
                    .omenchat
                    .omenchat_live_connect_count
                    .get(&session.session_id)
                    .copied()
                    .unwrap_or_default();
                let disconnects = desktop
                    .omenchat
                    .omenchat_live_disconnect_count
                    .get(&session.session_id)
                    .copied()
                    .unwrap_or_default();
                let retry_attempts = desktop
                    .omenchat
                    .omenchat_live_retry_count
                    .get(&session.session_id)
                    .copied()
                    .unwrap_or_default();
                let reconnect_line =
                    desktop.omenchat_reconnect_state_label(session.session_id, now);
                let last_disconnect = desktop
                    .omenchat
                    .omenchat_live_last_disconnect_reason
                    .get(&session.session_id)
                    .map(|reason| format!("last disconnect: {reason}"))
                    .unwrap_or_else(|| "last disconnect: none".into());
                let transport = desktop
                    .omenchat
                    .omenchat_live_transports
                    .get(&session.session_id);
                let history_sync_line =
                    desktop.omenchat_recent_sync_monitor_label(session.session_id, now);
                let attention_line = omenchat_session_attention_line(OmenChatSessionAttention {
                    connected: transport.is_some(),
                    opening: desktop
                        .omenchat
                        .omenchat_live_opening
                        .contains(&session.session_id),
                    reconnect_queued: desktop
                        .omenchat
                        .omenchat_live_retry_after
                        .contains_key(&session.session_id),
                    awaiting_pong: transport.is_some_and(|transport| transport.awaiting_pong),
                    last_ping_age_ms: transport.and_then(|transport| {
                        (transport.last_ping_epoch_ms > 0)
                            .then(|| now.saturating_sub(transport.last_ping_epoch_ms))
                    }),
                    heartbeat_idle_ms: transport.map(|transport| transport.heartbeat_idle_ms),
                    pending_resources: transport
                        .map(|transport| transport.pending_resource_offer_count())
                        .unwrap_or_default(),
                    history_sync_label: &history_sync_line,
                });
                let link_line = if let Some(transport) = transport {
                    format!(
                        "link {} | up {} | rx {} ago | tx {} ago | pong wait={}",
                        short_destination_hash(&hex_bytes(&transport.link_id)),
                        compact_elapsed_ms(now.saturating_sub(transport.connected_since_epoch_ms)),
                        compact_elapsed_ms(now.saturating_sub(transport.last_rx_epoch_ms)),
                        compact_elapsed_ms(now.saturating_sub(transport.last_tx_epoch_ms)),
                        transport.awaiting_pong
                    )
                } else if desktop
                    .omenchat
                    .omenchat_live_opening
                    .contains(&session.session_id)
                {
                    "opening/reconnecting live link".into()
                } else {
                    "no active live link".into()
                };
                let traffic_line = if let Some(transport) = transport {
                    format!(
                        "traffic: frames {} in / {} out | wire {} rx / {} tx | resources {} ({}) | pending {}",
                        transport.frames_in,
                        transport.frames_out,
                        human_bytes(transport.bytes_in),
                        human_bytes(transport.bytes_out),
                        transport.resources_in,
                        human_bytes(transport.resource_bytes_in),
                        transport.pending_resource_offer_count()
                    )
                } else {
                    "traffic: disconnected".into()
                };
                let mix_line = if let Some(transport) = transport {
                    format!(
                        "frames: history {} in / {} out | room {} | chat sends {} | userlists {} | offers {} | ping {} / pong {}",
                        transport.history_frames_in,
                        transport.history_frames_out,
                        transport.room_events_in,
                        transport.chat_frames_out,
                        transport.userlist_frames_in,
                        transport.resource_offers_in,
                        transport.pings_out,
                        transport.pongs_in
                    )
                } else {
                    "frames: disconnected".into()
                };
                let upload_line = if let Some(transport) = transport {
                    format!(
                        "uploads: fetches {} | offers {} | inline chunks {} ({}) | resources {} ({})",
                        transport.upload_fetches_out,
                        transport.upload_resource_offers_in,
                        transport.upload_inline_chunks_in,
                        human_bytes(transport.upload_inline_bytes_in),
                        transport.upload_resources_in,
                        human_bytes(transport.upload_resource_bytes_in)
                    )
                } else {
                    "uploads: disconnected".into()
                };
                let heartbeat_line = if let Some(transport) = transport {
                    let last_pong = if transport.last_pong_epoch_ms > 0 {
                        compact_elapsed_ms(now.saturating_sub(transport.last_pong_epoch_ms))
                    } else {
                        "never".into()
                    };
                    let rtt = transport
                        .last_ping_rtt_ms
                        .map(|rtt| format!("{rtt} ms"))
                        .unwrap_or_else(|| "unknown".into());
                    let last_ping = if transport.last_ping_epoch_ms > 0 {
                        compact_elapsed_ms(now.saturating_sub(transport.last_ping_epoch_ms))
                    } else {
                        "never".into()
                    };
                    let interval = compact_elapsed_ms(transport.heartbeat_idle_ms.clamp(
                        OMENCHAT_MIN_HEARTBEAT_IDLE_MS,
                        OMENCHAT_MAX_HEARTBEAT_IDLE_MS,
                    ));
                    format!(
                        "heartbeat: interval {interval} | last ping {last_ping} ago | last pong {last_pong} ago | RTT {rtt}"
                    )
                } else {
                    "heartbeat: disconnected".into()
                };
                let last_frame_line = if let Some(transport) = transport {
                    format!(
                        "last frames: rx={} / tx={}",
                        transport.last_rx_frame.as_deref().unwrap_or("none"),
                        transport.last_tx_frame.as_deref().unwrap_or("none")
                    )
                } else {
                    "last frames: none".into()
                };
                lines = lines.push(
                    column![
                        wrapped_text_owned(
                            format!(
                                "{} | {} | room #{} | users {}",
                                session.server.display_name,
                                short_destination_hash(&session.server.destination),
                                session.active_room.name,
                                session.users.len()
                            ),
                            14
                        ),
                        wrapped_text_owned(
                            format!(
                                "{} | connects {} | disconnects {} | retries {} | {} | {} | {}",
                                link_line,
                                connects,
                                disconnects,
                                retry_attempts,
                                reconnect_line,
                                last_disconnect,
                                session.status
                            ),
                            13
                        ),
                        wrapped_text_owned(attention_line, 13),
                        wrapped_text_owned(traffic_line, 13),
                        wrapped_text_owned(mix_line, 13),
                        wrapped_text_owned(upload_line, 13),
                        wrapped_text_owned(heartbeat_line, 13),
                        wrapped_text_owned(history_sync_line, 13),
                        wrapped_text_owned(last_frame_line, 13),
                    ]
                    .spacing(2),
                );
            }
        }
        section_card("OMENchat Live Links", lines)
    }
    #[cfg(not(any(feature = "chat-client-rns", feature = "chat-client-rns-clean")))]
    {
        section_card(
            "OMENchat Live Links",
            text("OMENchat live monitoring is unavailable in this build.").size(ui_size(14)),
        )
    }
}

#[cfg(all(
    test,
    any(feature = "chat-client-rns", feature = "chat-client-rns-clean")
))]
mod tests {
    use super::*;
    use crate::app::App;
    use crate::chat::OmenChatDescriptor;
    use crate::desktop::DesktopOmenChatTransport;

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

    #[tokio::test]
    async fn omenchat_live_monitor_totals_aggregate_sessions_and_transfers() {
        let mut desktop = desktop_with_temp_root("omenbrowser-rs-desktop-omenchat-monitor-totals");
        let session_id =
            desktop.open_omenchat_status_session(test_descriptor(), "connected".into());
        let mut transport = DesktopOmenChatTransport::new([0x73; 16], 1_000);
        transport.frames_in = 4;
        transport.frames_out = 3;
        transport.bytes_in = 1024;
        transport.bytes_out = 512;
        transport.resources_in = 2;
        transport.resource_bytes_in = 2048;
        transport.upload_fetches_out = 1;
        transport.upload_resource_offers_in = 2;
        transport.upload_inline_chunks_in = 3;
        transport.upload_inline_bytes_in = 4096;
        transport.upload_resources_in = 1;
        transport.upload_resource_bytes_in = 8192;
        transport.awaiting_pong = true;
        transport
            .pending_resource_offers
            .entry("upload:test".into())
            .or_default()
            .push_back(vec![1, 2, 3]);
        desktop
            .omenchat
            .omenchat_live_transports
            .insert(session_id, transport);
        desktop.omenchat.omenchat_live_opening.insert(99);
        desktop
            .omenchat
            .omenchat_live_retry_after
            .insert(100, 2_000);
        desktop
            .omenchat
            .omenchat_recent_sync_pending
            .insert(session_id);
        desktop
            .omenchat
            .omenchat_recent_sync_due_after
            .insert(session_id, 2_500);

        let totals = desktop.omenchat_live_monitor_totals();

        assert_eq!(totals.sessions, 1);
        assert_eq!(totals.connected, 1);
        assert_eq!(totals.opening, 1);
        assert_eq!(totals.reconnect_timers, 1);
        assert_eq!(totals.history_sync_waiting, 1);
        assert_eq!(totals.pending_resources, 1);
        assert_eq!(totals.frames_in, 4);
        assert_eq!(totals.frames_out, 3);
        assert_eq!(totals.bytes_in, 1024);
        assert_eq!(totals.bytes_out, 512);
        assert_eq!(totals.resources_in, 2);
        assert_eq!(totals.resource_bytes_in, 2048);
        assert_eq!(totals.upload_fetches_out, 1);
        assert_eq!(totals.upload_resource_offers_in, 2);
        assert_eq!(totals.upload_inline_chunks_in, 3);
        assert_eq!(totals.upload_inline_bytes_in, 4096);
        assert_eq!(totals.upload_resources_in, 1);
        assert_eq!(totals.upload_resource_bytes_in, 8192);
        assert_eq!(totals.awaiting_pongs, 1);
    }

    #[test]
    fn omenchat_monitor_health_line_prioritizes_actionable_states() {
        assert_eq!(
            omenchat_monitor_health_line(&OmenChatLiveMonitorTotals::default()),
            "health: no OMENchat sessions open"
        );

        let reconnecting = OmenChatLiveMonitorTotals {
            sessions: 1,
            opening: 1,
            reconnect_timers: 2,
            ..OmenChatLiveMonitorTotals::default()
        };
        assert!(omenchat_monitor_health_line(&reconnecting)
            .contains("reconnect/opening activity visible"));

        let disconnected = OmenChatLiveMonitorTotals {
            sessions: 2,
            connected: 0,
            ..OmenChatLiveMonitorTotals::default()
        };
        assert!(omenchat_monitor_health_line(&disconnected).contains("disconnected"));

        let partial = OmenChatLiveMonitorTotals {
            sessions: 2,
            connected: 1,
            ..OmenChatLiveMonitorTotals::default()
        };
        assert!(omenchat_monitor_health_line(&partial).contains("partial"));

        let waiting_pong = OmenChatLiveMonitorTotals {
            sessions: 1,
            connected: 1,
            awaiting_pongs: 1,
            ..OmenChatLiveMonitorTotals::default()
        };
        assert!(omenchat_monitor_health_line(&waiting_pong).contains("heartbeat pong"));

        let active_upload = OmenChatLiveMonitorTotals {
            sessions: 1,
            connected: 1,
            frames_in: 4,
            bytes_in: 1024,
            bytes_out: 512,
            upload_fetches_out: 1,
            upload_inline_chunks_in: 2,
            upload_resource_bytes_in: 2048,
            ..OmenChatLiveMonitorTotals::default()
        };
        assert!(
            omenchat_monitor_health_line(&active_upload).contains("media/upload traffic active")
        );

        let quiet = OmenChatLiveMonitorTotals {
            sessions: 1,
            connected: 1,
            ..OmenChatLiveMonitorTotals::default()
        };
        assert!(omenchat_monitor_health_line(&quiet).contains("connected and quiet"));
    }

    #[test]
    fn omenchat_session_attention_line_prioritizes_stalls_without_network_actions() {
        let attention = |connected,
                         opening,
                         reconnect_queued,
                         awaiting_pong,
                         last_ping_age_ms,
                         heartbeat_idle_ms,
                         pending_resources,
                         history_sync_label| {
            OmenChatSessionAttention {
                connected,
                opening,
                reconnect_queued,
                awaiting_pong,
                last_ping_age_ms,
                heartbeat_idle_ms,
                pending_resources,
                history_sync_label,
            }
        };

        assert!(omenchat_session_attention_line(attention(
            false,
            false,
            false,
            false,
            None,
            None,
            0,
            "history sync: offline",
        ))
        .contains("disconnected"));

        assert!(omenchat_session_attention_line(attention(
            true,
            false,
            false,
            true,
            Some(82_000),
            Some(40_000),
            0,
            "history sync: current for live link",
        ))
        .contains("heartbeat pong overdue"));

        assert!(omenchat_session_attention_line(attention(
            true,
            false,
            false,
            false,
            None,
            None,
            0,
            "history sync: retry in 3s after 1 attempt(s)",
        ))
        .contains("recent history sync pending"));

        assert!(omenchat_session_attention_line(attention(
            true,
            false,
            false,
            false,
            None,
            None,
            2,
            "history sync: current for live link",
        ))
        .contains("pending Resource offer"));

        assert_eq!(
            omenchat_session_attention_line(attention(
                true,
                false,
                false,
                false,
                None,
                None,
                0,
                "history sync: current for live link",
            )),
            "attention: live link healthy; no action needed"
        );
    }
}
