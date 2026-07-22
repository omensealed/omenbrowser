use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::Arc;

use iced::widget::scrollable::RelativeOffset;

use crate::chat::protocol::RoomId;
use crate::chat::store::SqliteChatStore;
use crate::chat::{ChatClient, ChatSessionId};
use crate::runtime::CancellationToken;

use super::message::OmenChatMediaLoadState;
use super::omenchat_runtime::omenchat_event_counts_by_room;
#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
use super::omenchat_runtime::DesktopOmenChatTransport;
use super::startup::OmenChatStartupState;
use super::DesktopPane;

#[cfg(feature = "chat-client-gif")]
pub(in crate::desktop) type OmenChatGifFrames = iced_gif::Frames;

#[cfg(not(feature = "chat-client-gif"))]
#[derive(Clone, Debug)]
pub(in crate::desktop) struct OmenChatGifFrames;

#[derive(Clone, Debug)]
pub(in crate::desktop) struct DecodedOmenChatGif {
    pub(in crate::desktop) frames: Arc<OmenChatGifFrames>,
    pub(in crate::desktop) decoded_bytes: u64,
}

#[derive(Clone, Debug)]
pub(in crate::desktop) enum OmenChatMediaCacheSource {
    Bytes(Vec<u8>),
    File(PathBuf),
}

#[derive(Clone, Debug)]
pub(in crate::desktop) struct OmenChatMediaCacheJob {
    pub(in crate::desktop) session_id: ChatSessionId,
    pub(in crate::desktop) cache_key: String,
    pub(in crate::desktop) filename: String,
    pub(in crate::desktop) content_type: String,
    pub(in crate::desktop) source: OmenChatMediaCacheSource,
    pub(in crate::desktop) reserved_bytes: usize,
    pub(in crate::desktop) generation: u64,
    pub(in crate::desktop) cancellation: CancellationToken,
}

#[derive(Clone, Debug)]
pub(in crate::desktop) struct CachedOmenChatMedia {
    pub(in crate::desktop) path: String,
    pub(in crate::desktop) content_type: String,
    pub(in crate::desktop) animated: bool,
    pub(in crate::desktop) decoded_gif: Option<DecodedOmenChatGif>,
    pub(in crate::desktop) evicted_paths: Vec<String>,
}

pub(in crate::desktop) struct OmenChatGifCache {
    entries: HashMap<String, DecodedOmenChatGif>,
    insertion_order: VecDeque<String>,
    decoded_bytes: u64,
}

pub(in crate::desktop) struct OmenChatMediaStateCache {
    entries: HashMap<String, OmenChatMediaLoadState>,
    insertion_order: VecDeque<String>,
    metadata_bytes: usize,
}

impl OmenChatMediaStateCache {
    fn new() -> Self {
        Self {
            entries: HashMap::new(),
            insertion_order: VecDeque::new(),
            metadata_bytes: 0,
        }
    }

    pub(in crate::desktop) fn insert(
        &mut self,
        key: String,
        state: OmenChatMediaLoadState,
    ) -> bool {
        let entry_bytes = media_state_metadata_bytes(&key, &state);
        if entry_bytes > super::OMENCHAT_MEDIA_STATE_MAX_BYTES {
            return false;
        }
        if let Some(previous) = self.entries.remove(&key) {
            self.metadata_bytes = self
                .metadata_bytes
                .saturating_sub(media_state_metadata_bytes(&key, &previous));
            self.insertion_order.retain(|existing| existing != &key);
        }
        while self.entries.len() >= super::OMENCHAT_MEDIA_STATE_MAX_ITEMS
            || self.metadata_bytes.saturating_add(entry_bytes)
                > super::OMENCHAT_MEDIA_STATE_MAX_BYTES
        {
            let Some(oldest) = self.insertion_order.pop_front() else {
                return false;
            };
            if let Some(removed) = self.entries.remove(&oldest) {
                self.metadata_bytes = self
                    .metadata_bytes
                    .saturating_sub(media_state_metadata_bytes(&oldest, &removed));
            }
        }
        self.metadata_bytes = self.metadata_bytes.saturating_add(entry_bytes);
        self.insertion_order.push_back(key.clone());
        self.entries.insert(key, state);
        true
    }

    pub(in crate::desktop) fn get(&self, key: &str) -> Option<&OmenChatMediaLoadState> {
        self.entries.get(key)
    }

    pub(in crate::desktop) fn remove(&mut self, key: &str) -> Option<OmenChatMediaLoadState> {
        let removed = self.entries.remove(key)?;
        self.metadata_bytes = self
            .metadata_bytes
            .saturating_sub(media_state_metadata_bytes(key, &removed));
        self.insertion_order.retain(|existing| existing != key);
        Some(removed)
    }

    pub(in crate::desktop) fn len(&self) -> usize {
        self.entries.len()
    }

    pub(in crate::desktop) fn values(&self) -> impl Iterator<Item = &OmenChatMediaLoadState> {
        self.entries.values()
    }

    pub(in crate::desktop) fn metadata_bytes(&self) -> usize {
        self.metadata_bytes
    }

    pub(in crate::desktop) fn remove_cached_paths(&mut self, paths: &[String]) {
        let keys = self
            .entries
            .iter()
            .filter_map(|(key, state)| match state {
                OmenChatMediaLoadState::Cached { path, .. } if paths.contains(path) => {
                    Some(key.clone())
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        for key in keys {
            self.remove(&key);
        }
    }
}

impl std::ops::Deref for OmenChatMediaStateCache {
    type Target = HashMap<String, OmenChatMediaLoadState>;

    fn deref(&self) -> &Self::Target {
        &self.entries
    }
}

fn media_state_metadata_bytes(key: &str, state: &OmenChatMediaLoadState) -> usize {
    let state_bytes = match state {
        OmenChatMediaLoadState::Loading { message, .. }
        | OmenChatMediaLoadState::Failed { message } => message.len(),
        OmenChatMediaLoadState::Cached {
            path, content_type, ..
        } => path.len().saturating_add(content_type.len()),
    };
    key.len().saturating_add(state_bytes)
}

impl OmenChatGifCache {
    pub(in crate::desktop) fn new() -> Self {
        Self {
            entries: HashMap::new(),
            insertion_order: VecDeque::new(),
            decoded_bytes: 0,
        }
    }

    pub(in crate::desktop) fn insert(&mut self, path: String, decoded: DecodedOmenChatGif) -> bool {
        if decoded.decoded_bytes > super::OMENCHAT_GIF_DECODED_MAX_BYTES {
            return false;
        }
        if let Some(previous) = self.entries.remove(&path) {
            self.decoded_bytes = self.decoded_bytes.saturating_sub(previous.decoded_bytes);
            self.insertion_order.retain(|key| key != &path);
        }
        while self.entries.len() >= super::OMENCHAT_GIF_CACHE_MAX_ITEMS
            || self.decoded_bytes.saturating_add(decoded.decoded_bytes)
                > super::OMENCHAT_GIF_DECODED_MAX_BYTES
        {
            let Some(oldest) = self.insertion_order.pop_front() else {
                return false;
            };
            if let Some(removed) = self.entries.remove(&oldest) {
                self.decoded_bytes = self.decoded_bytes.saturating_sub(removed.decoded_bytes);
            }
        }
        self.decoded_bytes = self.decoded_bytes.saturating_add(decoded.decoded_bytes);
        self.insertion_order.push_back(path.clone());
        self.entries.insert(path, decoded);
        true
    }

    pub(in crate::desktop) fn get(&self, path: &str) -> Option<&OmenChatGifFrames> {
        self.entries.get(path).map(|entry| entry.frames.as_ref())
    }

    pub(in crate::desktop) fn len(&self) -> usize {
        self.entries.len()
    }

    pub(in crate::desktop) fn decoded_bytes(&self) -> u64 {
        self.decoded_bytes
    }
}

#[cfg(test)]
// The cache tests sit beside the private cache types they exercise; the state
// type follows so production ownership fields remain grouped with its impls.
#[allow(clippy::items_after_test_module)]
mod tests {
    use super::*;

    #[cfg(feature = "chat-client-gif")]
    fn one_pixel_frames() -> Arc<OmenChatGifFrames> {
        let bytes = vec![
            0x47, 0x49, 0x46, 0x38, 0x39, 0x61, 0x01, 0x00, 0x01, 0x00, 0x80, 0x00, 0x00, 0x00,
            0x00, 0x00, 0xff, 0xff, 0xff, 0x2c, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x01, 0x00,
            0x00, 0x02, 0x01, 0x4c, 0x00, 0x3b,
        ];
        Arc::new(iced_gif::Frames::from_bytes(bytes).expect("one pixel GIF"))
    }

    #[test]
    #[cfg(feature = "chat-client-gif")]
    fn gif_cache_enforces_item_and_decoded_byte_budgets() {
        let frames = one_pixel_frames();
        let mut cache = OmenChatGifCache::new();
        for index in 0..=super::super::OMENCHAT_GIF_CACHE_MAX_ITEMS {
            assert!(cache.insert(
                format!("item-{index}"),
                DecodedOmenChatGif {
                    frames: frames.clone(),
                    decoded_bytes: 4,
                },
            ));
        }
        assert_eq!(cache.len(), super::super::OMENCHAT_GIF_CACHE_MAX_ITEMS);
        assert!(cache.get("item-0").is_none());

        let mut cache = OmenChatGifCache::new();
        let forty_mib = 40 * 1024 * 1024;
        assert!(cache.insert(
            "first".into(),
            DecodedOmenChatGif {
                frames: frames.clone(),
                decoded_bytes: forty_mib,
            },
        ));
        assert!(cache.insert(
            "second".into(),
            DecodedOmenChatGif {
                frames,
                decoded_bytes: forty_mib,
            },
        ));
        assert!(cache.get("first").is_none());
        assert!(cache.get("second").is_some());
        assert_eq!(cache.decoded_bytes(), forty_mib);
    }

    #[test]
    fn media_cache_job_queue_enforces_item_and_byte_reservations() {
        let startup = OmenChatStartupState {
            chat_client: ChatClient::new(),
            chat_store: None,
            session_ids: Vec::new(),
            #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
            client_instance_id: None,
            #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
            authenticated_identity_hash: None,
            #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
            mutation_intent_worker: None,
        };
        let panes = iced::widget::pane_grid::State::new(DesktopPane::Browser(1)).0;
        let mut state = OmenChatDesktopState::from_startup(startup, &panes);
        let job = |index| OmenChatMediaCacheJob {
            session_id: 1,
            cache_key: format!("job-{index}"),
            filename: "file.bin".into(),
            content_type: "application/octet-stream".into(),
            source: OmenChatMediaCacheSource::File(PathBuf::from("isolated-source")),
            reserved_bytes: super::super::OMENCHAT_GIF_ENCODED_MAX_BYTES,
            generation: 0,
            cancellation: CancellationToken::new(),
        };
        assert!(state.enqueue_media_cache_job(job(1)));
        assert!(state.enqueue_media_cache_job(job(2)));
        assert!(!state.enqueue_media_cache_job(job(3)));
        assert_eq!(
            state.pending_media_cache_bytes,
            super::super::OMENCHAT_MEDIA_JOB_MAX_BYTES
        );
        assert_eq!(state.take_media_cache_jobs().len(), 2);
        assert_eq!(state.pending_media_cache_bytes, 0);

        let mut first = job(7);
        first.reserved_bytes = 1;
        let first_cancellation = first.cancellation.clone();
        assert!(state.enqueue_media_cache_job(first));
        let first = state.take_media_cache_jobs().pop().expect("first job");
        let mut replacement = job(7);
        replacement.reserved_bytes = 1;
        assert!(state.enqueue_media_cache_job(replacement));
        assert!(first_cancellation.is_cancelled());
        let replacement = state
            .take_media_cache_jobs()
            .pop()
            .expect("replacement job");
        assert!(!state.accept_media_cache_completion(1, &first.cache_key, first.generation));
        assert!(state.accept_media_cache_completion(
            1,
            &replacement.cache_key,
            replacement.generation
        ));

        let mut cancelled = job(8);
        cancelled.reserved_bytes = 3;
        let session_cancellation = cancelled.cancellation.clone();
        assert!(state.enqueue_media_cache_job(cancelled));
        let mut cancelled = state.cancel_media_cache_jobs_for_session(1);
        cancelled.sort();
        assert_eq!(cancelled, vec!["job-1", "job-2", "job-8"]);
        assert!(state.pending_media_cache_jobs.is_empty());
        assert_eq!(state.pending_media_cache_bytes, 0);
        assert!(session_cancellation.is_cancelled());
    }

    #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
    #[test]
    fn durable_capability_is_not_advertised_without_its_persistence_owner() {
        let startup = OmenChatStartupState {
            chat_client: ChatClient::new(),
            chat_store: None,
            session_ids: Vec::new(),
            client_instance_id: Some(crate::chat::protocol::ClientInstanceId::new([7; 16])),
            authenticated_identity_hash: Some(vec![8; 16]),
            mutation_intent_worker: None,
        };
        let panes = iced::widget::pane_grid::State::new(DesktopPane::Browser(1)).0;

        let state = OmenChatDesktopState::from_startup(startup, &panes);

        assert!(state.omenchat_live_state.client_instance_id().is_some());
        assert!(!state.omenchat_live_state.durable_mutation_owner_ready());
    }

    #[test]
    fn media_state_cache_enforces_item_and_metadata_byte_budgets() {
        let mut cache = OmenChatMediaStateCache::new();
        for index in 0..=super::super::OMENCHAT_MEDIA_STATE_MAX_ITEMS {
            assert!(cache.insert(
                format!("item-{index}"),
                OmenChatMediaLoadState::Failed {
                    message: "failed".into(),
                },
            ));
        }
        assert_eq!(cache.len(), super::super::OMENCHAT_MEDIA_STATE_MAX_ITEMS);
        assert!(cache.get("item-0").is_none());

        let large = "x".repeat(super::super::OMENCHAT_MEDIA_STATE_MAX_BYTES / 2);
        let mut cache = OmenChatMediaStateCache::new();
        assert!(cache.insert(
            "first".into(),
            OmenChatMediaLoadState::Failed {
                message: large.clone(),
            },
        ));
        assert!(cache.insert(
            "second".into(),
            OmenChatMediaLoadState::Failed { message: large },
        ));
        assert!(cache.get("first").is_none());
        assert!(cache.get("second").is_some());
        assert!(cache.metadata_bytes() <= super::super::OMENCHAT_MEDIA_STATE_MAX_BYTES);

        assert!(!cache.insert(
            "oversize".into(),
            OmenChatMediaLoadState::Failed {
                message: "x".repeat(super::super::OMENCHAT_MEDIA_STATE_MAX_BYTES + 1),
            },
        ));
    }
}

#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::desktop) enum OmenChatMutationRecoveryState {
    Unavailable,
    Pending,
    InFlight,
    Loaded,
    Failed,
}

#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
impl OmenChatMutationRecoveryState {
    pub(in crate::desktop) fn label(self) -> &'static str {
        match self {
            Self::Unavailable => "unavailable",
            Self::Pending => "pending",
            Self::InFlight => "in-flight",
            Self::Loaded => "loaded",
            Self::Failed => "failed",
        }
    }
}

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
    pub(in crate::desktop) omenchat_media_cache: OmenChatMediaStateCache,
    pub(in crate::desktop) omenchat_gif_frames: OmenChatGifCache,
    pub(in crate::desktop) pending_media_cache_jobs: VecDeque<OmenChatMediaCacheJob>,
    pub(in crate::desktop) pending_media_cache_bytes: usize,
    pub(in crate::desktop) media_cache_generation: u64,
    pub(in crate::desktop) active_media_cache_jobs:
        HashMap<String, (ChatSessionId, u64, CancellationToken)>,
    pub(in crate::desktop) omenchat_server_entry: String,
    pub(in crate::desktop) omenchat_rooms_visible: bool,
    pub(in crate::desktop) omenchat_pending_upload_sources:
        HashMap<(ChatSessionId, String, u64), PathBuf>,
    #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
    pub(in crate::desktop) omenchat_live_state: crate::chat::live::LiveChatClientState,
    #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
    pub(in crate::desktop) omenchat_authenticated_identity_hash: Option<Vec<u8>>,
    #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
    pub(in crate::desktop) omenchat_mutation_intent_worker:
        Option<crate::chat::mutation_intent_worker::MutationIntentWorker>,
    #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
    pub(in crate::desktop) omenchat_mutation_recovery_state: OmenChatMutationRecoveryState,
    #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
    pub(in crate::desktop) omenchat_recovered_mutation_intents:
        Vec<crate::chat::mutation_intents::OutboundMutationIntent>,
    #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
    pub(in crate::desktop) omenchat_other_identity_mutation_intents: usize,
    #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
    pub(in crate::desktop) omenchat_live_transports:
        HashMap<ChatSessionId, DesktopOmenChatTransport>,
    #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
    pub(in crate::desktop) omenchat_link_sessions: HashMap<[u8; 16], ChatSessionId>,
    #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
    pub(in crate::desktop) omenchat_live_opening: HashSet<ChatSessionId>,
    #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
    pub(in crate::desktop) omenchat_live_open_cancellations:
        HashMap<ChatSessionId, CancellationToken>,
    #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
    pub(in crate::desktop) omenchat_live_retry_after: HashMap<ChatSessionId, u64>,
    #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
    pub(in crate::desktop) omenchat_live_retry_count: HashMap<ChatSessionId, u8>,
    #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
    pub(in crate::desktop) omenchat_live_stable_after: HashMap<ChatSessionId, u64>,
    #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
    pub(in crate::desktop) omenchat_live_connect_count: HashMap<ChatSessionId, u64>,
    #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
    pub(in crate::desktop) omenchat_live_disconnect_count: HashMap<ChatSessionId, u64>,
    #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
    pub(in crate::desktop) omenchat_live_last_disconnect_reason: HashMap<ChatSessionId, String>,
    #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
    pub(in crate::desktop) omenchat_connection_states:
        HashMap<ChatSessionId, crate::chat::ChatConnectionState>,
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

        #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
        let omenchat_connection_states = startup
            .chat_client
            .sessions()
            .iter()
            .map(|session| {
                (
                    session.session_id,
                    crate::chat::ChatConnectionState::Disconnected,
                )
            })
            .collect();

        #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
        let omenchat_live_state = {
            let mut state = crate::chat::live::LiveChatClientState::default();
            state.set_client_instance_id(startup.client_instance_id);
            state.set_durable_mutation_owner_ready(
                startup.mutation_intent_worker.is_some()
                    && startup.authenticated_identity_hash.is_some(),
            );
            state
        };
        #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
        let omenchat_mutation_recovery_state = if startup.mutation_intent_worker.is_some() {
            OmenChatMutationRecoveryState::Pending
        } else {
            OmenChatMutationRecoveryState::Unavailable
        };

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
            omenchat_media_cache: OmenChatMediaStateCache::new(),
            omenchat_gif_frames: OmenChatGifCache::new(),
            pending_media_cache_jobs: VecDeque::new(),
            pending_media_cache_bytes: 0,
            media_cache_generation: 0,
            active_media_cache_jobs: HashMap::new(),
            omenchat_server_entry: String::new(),
            omenchat_rooms_visible: true,
            omenchat_pending_upload_sources: HashMap::new(),
            #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
            omenchat_live_state,
            #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
            omenchat_authenticated_identity_hash: startup.authenticated_identity_hash,
            #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
            omenchat_mutation_intent_worker: startup.mutation_intent_worker,
            #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
            omenchat_mutation_recovery_state,
            #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
            omenchat_recovered_mutation_intents: Vec::new(),
            #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
            omenchat_other_identity_mutation_intents: 0,
            #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
            omenchat_live_transports: HashMap::new(),
            #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
            omenchat_link_sessions: HashMap::new(),
            #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
            omenchat_live_opening: HashSet::new(),
            #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
            omenchat_live_open_cancellations: HashMap::new(),
            #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
            omenchat_live_retry_after: HashMap::new(),
            #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
            omenchat_live_retry_count: HashMap::new(),
            #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
            omenchat_live_stable_after: HashMap::new(),
            #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
            omenchat_live_connect_count: HashMap::new(),
            #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
            omenchat_live_disconnect_count: HashMap::new(),
            #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
            omenchat_live_last_disconnect_reason: HashMap::new(),
            #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
            omenchat_connection_states,
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

impl OmenChatDesktopState {
    pub(in crate::desktop) fn enqueue_media_cache_job(
        &mut self,
        mut job: OmenChatMediaCacheJob,
    ) -> bool {
        let Some(next_bytes) = self
            .pending_media_cache_bytes
            .checked_add(job.reserved_bytes)
        else {
            return false;
        };
        if self.pending_media_cache_jobs.len() >= super::OMENCHAT_MEDIA_JOB_MAX_ITEMS
            || next_bytes > super::OMENCHAT_MEDIA_JOB_MAX_BYTES
        {
            return false;
        }
        self.pending_media_cache_bytes = next_bytes;
        self.media_cache_generation = self.media_cache_generation.wrapping_add(1).max(1);
        job.generation = self.media_cache_generation;
        if let Some((_, _, cancellation)) = self.active_media_cache_jobs.insert(
            job.cache_key.clone(),
            (job.session_id, job.generation, job.cancellation.clone()),
        ) {
            cancellation.cancel();
        }
        self.pending_media_cache_jobs.push_back(job);
        true
    }

    pub(in crate::desktop) fn take_media_cache_jobs(&mut self) -> Vec<OmenChatMediaCacheJob> {
        self.pending_media_cache_bytes = 0;
        self.pending_media_cache_jobs.drain(..).collect()
    }

    pub(in crate::desktop) fn accept_media_cache_completion(
        &mut self,
        session_id: ChatSessionId,
        cache_key: &str,
        generation: u64,
    ) -> bool {
        if !matches!(
            self.active_media_cache_jobs.get(cache_key),
            Some((active_session, active_generation, _))
                if *active_session == session_id && *active_generation == generation
        ) {
            return false;
        }
        self.active_media_cache_jobs.remove(cache_key);
        true
    }

    pub(in crate::desktop) fn cancel_media_cache_jobs_for_session(
        &mut self,
        session_id: ChatSessionId,
    ) -> Vec<String> {
        let cancelled = self
            .active_media_cache_jobs
            .iter()
            .filter_map(|(key, (job_session_id, _, cancellation))| {
                if *job_session_id == session_id {
                    cancellation.cancel();
                }
                (*job_session_id == session_id).then_some(key.clone())
            })
            .collect::<Vec<_>>();
        self.pending_media_cache_jobs
            .retain(|job| job.session_id != session_id);
        self.pending_media_cache_bytes = self
            .pending_media_cache_jobs
            .iter()
            .fold(0usize, |total, job| {
                total.saturating_add(job.reserved_bytes)
            });
        self.active_media_cache_jobs
            .retain(|_, (job_session_id, _, _)| *job_session_id != session_id);
        cancelled
    }
}
