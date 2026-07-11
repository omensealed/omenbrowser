use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use iced::widget::scrollable::RelativeOffset;

use crate::chat::protocol::RoomId;
use crate::chat::store::SqliteChatStore;
use crate::chat::{ChatClient, ChatSessionId};

use super::message::OmenChatMediaLoadState;
use super::omenchat_runtime::omenchat_event_counts_by_room;
#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
use super::omenchat_runtime::DesktopOmenChatTransport;
use super::startup::OmenChatStartupState;
use super::DesktopPane;

pub(in crate::desktop) struct OmenChatDesktopState {
    pub(in crate::desktop) chat_client: ChatClient,
    pub(in crate::desktop) chat_store: Option<SqliteChatStore>,
    pub(in crate::desktop) chat_drafts: HashMap<ChatSessionId, String>,
    pub(in crate::desktop) chat_event_counts: HashMap<(ChatSessionId, RoomId), usize>,
    pub(in crate::desktop) chat_scroll_offsets: HashMap<(ChatSessionId, RoomId), RelativeOffset>,
    pub(in crate::desktop) chat_scroll_bottom_locks: HashSet<(ChatSessionId, RoomId)>,
    pub(in crate::desktop) omenchat_motds: HashMap<ChatSessionId, String>,
    pub(in crate::desktop) omenchat_upload_quotas: HashMap<ChatSessionId, u64>,
    pub(in crate::desktop) omenchat_upload_max_file_bytes: HashMap<ChatSessionId, u64>,
    pub(in crate::desktop) omenchat_media_cache: HashMap<String, OmenChatMediaLoadState>,
    pub(in crate::desktop) omenchat_gif_frames: HashMap<String, iced_gif::Frames>,
    pub(in crate::desktop) omenchat_server_entry: String,
    pub(in crate::desktop) omenchat_rooms_visible: bool,
    pub(in crate::desktop) omenchat_pending_upload_sources:
        HashMap<(ChatSessionId, String, u64), PathBuf>,
    #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
    pub(in crate::desktop) omenchat_live_state: crate::chat::live::LiveChatClientState,
    #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
    pub(in crate::desktop) omenchat_live_transports:
        HashMap<ChatSessionId, DesktopOmenChatTransport>,
    #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
    pub(in crate::desktop) omenchat_link_sessions: HashMap<[u8; 16], ChatSessionId>,
    #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
    pub(in crate::desktop) omenchat_live_opening: HashSet<ChatSessionId>,
    #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
    pub(in crate::desktop) omenchat_live_retry_after: HashMap<ChatSessionId, u64>,
    #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
    pub(in crate::desktop) omenchat_live_retry_count: HashMap<ChatSessionId, u8>,
    #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
    pub(in crate::desktop) omenchat_live_connect_count: HashMap<ChatSessionId, u64>,
    #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
    pub(in crate::desktop) omenchat_live_disconnect_count: HashMap<ChatSessionId, u64>,
    #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
    pub(in crate::desktop) omenchat_live_last_disconnect_reason: HashMap<ChatSessionId, String>,
    #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
    pub(in crate::desktop) omenchat_recent_sync_pending: HashSet<ChatSessionId>,
    #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
    pub(in crate::desktop) omenchat_recent_sync_links: HashMap<ChatSessionId, [u8; 16]>,
    #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
    pub(in crate::desktop) omenchat_recent_sync_due_after: HashMap<ChatSessionId, u64>,
    #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
    pub(in crate::desktop) omenchat_recent_sync_attempts: HashMap<ChatSessionId, u8>,
    #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
    pub(in crate::desktop) omenchat_live_reconnect_generation: HashMap<ChatSessionId, u64>,
}

impl OmenChatDesktopState {
    pub(in crate::desktop) fn from_startup(
        startup: OmenChatStartupState,
        workspace_panes: &iced::widget::pane_grid::State<DesktopPane>,
    ) -> Self {
        let chat_event_counts = omenchat_event_counts_by_room(startup.chat_client.sessions());
        let chat_scroll_offsets = workspace_panes
            .iter()
            .filter_map(|(_, pane)| match pane {
                DesktopPane::OmenChat(session_id) => {
                    let room_id = startup
                        .chat_client
                        .session(*session_id)
                        .map(|session| session.active_room.room_id)
                        .unwrap_or(1);
                    Some(((*session_id, room_id), RelativeOffset { x: 0.0, y: 1.0 }))
                }
                DesktopPane::Browser(_) | DesktopPane::Conversation(_) => None,
            })
            .collect::<HashMap<_, _>>();
        let chat_scroll_bottom_locks = chat_scroll_offsets.keys().copied().collect::<HashSet<_>>();

        Self {
            chat_client: startup.chat_client,
            chat_store: startup.chat_store,
            chat_drafts: HashMap::new(),
            chat_event_counts,
            chat_scroll_offsets,
            chat_scroll_bottom_locks,
            omenchat_motds: HashMap::new(),
            omenchat_upload_quotas: HashMap::new(),
            omenchat_upload_max_file_bytes: HashMap::new(),
            omenchat_media_cache: HashMap::new(),
            omenchat_gif_frames: HashMap::new(),
            omenchat_server_entry: String::new(),
            omenchat_rooms_visible: true,
            omenchat_pending_upload_sources: HashMap::new(),
            #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
            omenchat_live_state: crate::chat::live::LiveChatClientState::default(),
            #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
            omenchat_live_transports: HashMap::new(),
            #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
            omenchat_link_sessions: HashMap::new(),
            #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
            omenchat_live_opening: HashSet::new(),
            #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
            omenchat_live_retry_after: HashMap::new(),
            #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
            omenchat_live_retry_count: HashMap::new(),
            #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
            omenchat_live_connect_count: HashMap::new(),
            #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
            omenchat_live_disconnect_count: HashMap::new(),
            #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
            omenchat_live_last_disconnect_reason: HashMap::new(),
            #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
            omenchat_recent_sync_pending: HashSet::new(),
            #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
            omenchat_recent_sync_links: HashMap::new(),
            #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
            omenchat_recent_sync_due_after: HashMap::new(),
            #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
            omenchat_recent_sync_attempts: HashMap::new(),
            #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
            omenchat_live_reconnect_generation: HashMap::new(),
        }
    }
}
