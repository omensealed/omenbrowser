use thiserror::Error;

use crate::messaging::{
    Conversation, ConversationThread, DeliveryState, MessageStore, MessageSummary,
};

#[cfg(feature = "chat-client")]
use crate::chat::store::{
    SqliteChatStore, StoredChatHistoryEvent, CHAT_HISTORY_SEARCH_READ_MAX_BYTES,
};
#[cfg(feature = "chat-client")]
use crate::chat::{ChatEvent, ChatEventKind, ChatSessionView};

pub const LOCAL_HISTORY_SEARCH_QUERY_MAX_BYTES: usize = 256;
pub const LOCAL_HISTORY_SEARCH_TERM_MAX_ITEMS: usize = 8;
pub const LOCAL_HISTORY_SEARCH_SCAN_MAX_ITEMS: usize = 8_192;
pub const LOCAL_HISTORY_SEARCH_RESULT_MAX_ITEMS: usize = 128;
pub const LOCAL_HISTORY_SEARCH_TEXT_MAX_BYTES: usize = 256;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LocalHistorySourceFilter {
    #[default]
    All,
    Lxmf,
    OmenChat,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LocalHistorySearchQuery {
    pub text: String,
    pub sender: Option<String>,
    pub room: Option<String>,
    pub after_unix: Option<i64>,
    pub before_unix: Option<i64>,
    pub attachment_only: bool,
    pub delivery: Option<DeliveryState>,
    pub source: LocalHistorySourceFilter,
}

impl LocalHistorySearchQuery {
    pub fn validate(&self) -> Result<(), LocalHistorySearchError> {
        for value in [
            Some(self.text.as_str()),
            self.sender.as_deref(),
            self.room.as_deref(),
        ]
        .into_iter()
        .flatten()
        {
            if value.len() > LOCAL_HISTORY_SEARCH_QUERY_MAX_BYTES
                || value.chars().any(char::is_control)
            {
                return Err(LocalHistorySearchError::InvalidQuery);
            }
        }
        if self.text.split_whitespace().count() > LOCAL_HISTORY_SEARCH_TERM_MAX_ITEMS
            || matches!(
                (self.after_unix, self.before_unix),
                (Some(after), Some(before)) if after > before
            )
        {
            return Err(LocalHistorySearchError::InvalidQuery);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug)]
pub enum LocalHistorySearchInput<'a> {
    Lxmf(&'a Conversation),
    #[cfg(feature = "chat-client")]
    OmenChat(&'a ChatSessionView),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LocalHistorySource {
    Lxmf,
    OmenChat,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LocalHistoryResultKey {
    Lxmf {
        conversation_id: u64,
        message_index: usize,
    },
    #[cfg(feature = "chat-client")]
    OmenChat {
        session_id: u64,
        room_id: u32,
        event_id: u64,
    },
    LxmfStored {
        peer_key: String,
        message_index: usize,
        message_key: String,
    },
    #[cfg(feature = "chat-client")]
    OmenChatStored {
        server_key: String,
        room_id: u32,
        event_id: u64,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalHistorySearchResult {
    pub source: LocalHistorySource,
    pub key: LocalHistoryResultKey,
    pub sender: String,
    pub context: String,
    pub excerpt: String,
    pub attachment_name: Option<String>,
    pub at_unix: i64,
    pub delivery: Option<DeliveryState>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LocalHistorySearchPage {
    pub results: Vec<LocalHistorySearchResult>,
    pub scanned_items: usize,
    pub scan_limit_reached: bool,
    pub result_limit_reached: bool,
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum LocalHistorySearchError {
    #[error("local history search query exceeds its bounds")]
    InvalidQuery,
    #[error("local history search storage failed: {0}")]
    Storage(String),
}

pub fn search_local_history<'a>(
    inputs: impl IntoIterator<Item = LocalHistorySearchInput<'a>>,
    query: &LocalHistorySearchQuery,
) -> Result<LocalHistorySearchPage, LocalHistorySearchError> {
    query.validate()?;
    let terms = query.text.split_whitespace().collect::<Vec<_>>();
    let mut search = BoundedSearch {
        query,
        terms: &terms,
        page: LocalHistorySearchPage::default(),
        scan_max_items: LOCAL_HISTORY_SEARCH_SCAN_MAX_ITEMS,
    };
    for input in inputs {
        match input {
            LocalHistorySearchInput::Lxmf(conversation) => {
                if !matches!(
                    query.source,
                    LocalHistorySourceFilter::All | LocalHistorySourceFilter::Lxmf
                ) {
                    continue;
                }
                for (message_index, message) in
                    conversation.thread.messages.iter().enumerate().rev()
                {
                    if !search.admit_scan() {
                        break;
                    }
                    search.consider_lxmf(conversation, message_index, message);
                }
            }
            #[cfg(feature = "chat-client")]
            LocalHistorySearchInput::OmenChat(session) => {
                if !matches!(
                    query.source,
                    LocalHistorySourceFilter::All | LocalHistorySourceFilter::OmenChat
                ) {
                    continue;
                }
                for event in session.events.iter().rev() {
                    if !search.admit_scan() {
                        break;
                    }
                    search.consider_omenchat(session, event);
                }
            }
        }
        if search.page.scan_limit_reached {
            break;
        }
    }
    search.page.results.sort_by(|left, right| {
        right
            .at_unix
            .cmp(&left.at_unix)
            .then_with(|| left.context.cmp(&right.context))
            .then_with(|| left.excerpt.cmp(&right.excerpt))
    });
    Ok(search.page)
}

pub fn search_persisted_local_history(
    message_store: &MessageStore,
    #[cfg(feature = "chat-client")] omenchat_store_path: Option<&std::path::Path>,
    query: &LocalHistorySearchQuery,
) -> Result<LocalHistorySearchPage, LocalHistorySearchError> {
    query.validate()?;
    let terms = query.text.split_whitespace().collect::<Vec<_>>();
    let per_source_limit = if query.source == LocalHistorySourceFilter::All {
        LOCAL_HISTORY_SEARCH_SCAN_MAX_ITEMS / 2
    } else {
        LOCAL_HISTORY_SEARCH_SCAN_MAX_ITEMS
    };
    let mut search = BoundedSearch {
        query,
        terms: &terms,
        page: LocalHistorySearchPage::default(),
        scan_max_items: per_source_limit,
    };

    if matches!(
        query.source,
        LocalHistorySourceFilter::All | LocalHistorySourceFilter::Lxmf
    ) {
        let threads = message_store
            .list_threads_read_only()
            .map_err(|error| LocalHistorySearchError::Storage(error.to_string()))?;
        search.consider_stored_lxmf_threads(&threads);
    }

    #[cfg(feature = "chat-client")]
    if matches!(
        query.source,
        LocalHistorySourceFilter::All | LocalHistorySourceFilter::OmenChat
    ) {
        if let Some(path) = omenchat_store_path {
            search.scan_max_items = search
                .page
                .scanned_items
                .saturating_add(per_source_limit)
                .min(LOCAL_HISTORY_SEARCH_SCAN_MAX_ITEMS);
            let store = SqliteChatStore::open_read_only(path)
                .map_err(|error| LocalHistorySearchError::Storage(error.to_string()))?;
            let stored = store
                .latest_history_search_events(per_source_limit, CHAT_HISTORY_SEARCH_READ_MAX_BYTES)
                .map_err(|error| LocalHistorySearchError::Storage(error.to_string()))?;
            search.consider_stored_omenchat_events(&stored);
            if stored.len() == per_source_limit {
                search.page.scan_limit_reached = true;
            }
        }
    }

    search.finish();
    Ok(search.page)
}

struct BoundedSearch<'a, 'q> {
    query: &'q LocalHistorySearchQuery,
    terms: &'a [&'q str],
    page: LocalHistorySearchPage,
    scan_max_items: usize,
}

impl BoundedSearch<'_, '_> {
    fn admit_scan(&mut self) -> bool {
        if self.page.scanned_items >= self.scan_max_items {
            self.page.scan_limit_reached = true;
            return false;
        }
        self.page.scanned_items = self.page.scanned_items.saturating_add(1);
        true
    }

    fn consider_stored_lxmf_threads(&mut self, threads: &[ConversationThread]) {
        for thread in threads {
            for (message_index, message) in thread.messages.iter().enumerate().rev() {
                if !self.admit_scan() {
                    return;
                }
                self.consider_lxmf_message(
                    &thread.peer_label,
                    LocalHistoryResultKey::LxmfStored {
                        peer_key: thread.peer_hash.clone(),
                        message_index,
                        message_key: crate::app::message_summary_key(message),
                    },
                    message,
                );
            }
        }
    }

    fn consider_lxmf(
        &mut self,
        conversation: &Conversation,
        message_index: usize,
        message: &MessageSummary,
    ) {
        self.consider_lxmf_message(
            &conversation.peer_label,
            LocalHistoryResultKey::Lxmf {
                conversation_id: conversation.id,
                message_index,
            },
            message,
        );
    }

    fn consider_lxmf_message(
        &mut self,
        peer_label: &str,
        key: LocalHistoryResultKey,
        message: &MessageSummary,
    ) {
        let at_unix = finite_timestamp(message.timestamp);
        let delivery = message_delivery(message);
        let sender = if message.incoming {
            message.peer_label.as_str()
        } else {
            "You"
        };
        let attachment_name = message
            .attachments
            .iter()
            .find(|attachment| {
                self.terms
                    .iter()
                    .all(|term| contains_ascii_case_insensitive(&attachment.name, term))
            })
            .or_else(|| message.attachments.first())
            .map(|attachment| {
                bounded_search_text(&attachment.name, LOCAL_HISTORY_SEARCH_TEXT_MAX_BYTES)
            });
        let searchable = [
            peer_label,
            message.peer_label.as_str(),
            message.title.as_str(),
            message.content.as_str(),
        ];
        let attachment_fields = message
            .attachments
            .iter()
            .map(|attachment| attachment.name.as_str())
            .collect::<Vec<_>>();
        if !self.matches_common(
            at_unix,
            &searchable,
            &attachment_fields,
            Some(sender),
            None,
            Some(&delivery),
        ) {
            return;
        }
        let excerpt = if !message.content.trim().is_empty() {
            &message.content
        } else if !message.title.trim().is_empty() {
            &message.title
        } else {
            attachment_name.as_deref().unwrap_or("Attachment")
        };
        self.push(LocalHistorySearchResult {
            source: LocalHistorySource::Lxmf,
            key,
            sender: bounded_search_text(sender, LOCAL_HISTORY_SEARCH_TEXT_MAX_BYTES),
            context: bounded_search_text(peer_label, LOCAL_HISTORY_SEARCH_TEXT_MAX_BYTES),
            excerpt: bounded_search_text(excerpt, LOCAL_HISTORY_SEARCH_TEXT_MAX_BYTES),
            attachment_name,
            at_unix,
            delivery: Some(delivery),
        });
    }

    #[cfg(feature = "chat-client")]
    fn consider_stored_omenchat_events(&mut self, stored: &[StoredChatHistoryEvent]) {
        for stored in stored {
            if !self.admit_scan() {
                return;
            }
            self.consider_omenchat_event(
                &stored.server_display_name,
                &stored.room_name,
                LocalHistoryResultKey::OmenChatStored {
                    server_key: stored.event.server_id.clone(),
                    room_id: stored.event.room_id,
                    event_id: stored.event.event_id,
                },
                &stored.event,
            );
        }
    }

    #[cfg(feature = "chat-client")]
    fn consider_omenchat(&mut self, session: &ChatSessionView, event: &ChatEvent) {
        let room = session
            .rooms
            .iter()
            .find(|room| room.room_id == event.room_id)
            .unwrap_or(&session.active_room);
        self.consider_omenchat_event(
            &session.server.display_name,
            &room.name,
            LocalHistoryResultKey::OmenChat {
                session_id: session.session_id,
                room_id: event.room_id,
                event_id: event.event_id,
            },
            event,
        );
    }

    #[cfg(feature = "chat-client")]
    fn consider_omenchat_event(
        &mut self,
        server_display_name: &str,
        room_name: &str,
        key: LocalHistoryResultKey,
        event: &ChatEvent,
    ) {
        let sender = event.actor_display_name.as_deref().unwrap_or("System");
        let (body, attachment) = match &event.kind {
            ChatEventKind::Message { body }
            | ChatEventKind::RichMessage { body, .. }
            | ChatEventKind::Action { body }
            | ChatEventKind::Notice { body }
            | ChatEventKind::System { body } => (body.as_str(), None),
            ChatEventKind::Upload { filename, .. } => (filename.as_str(), Some(filename.as_str())),
        };
        let searchable = [server_display_name, room_name, sender, body];
        let attachment_fields = attachment.into_iter().collect::<Vec<_>>();
        if !self.matches_common(
            event.at_unix,
            &searchable,
            &attachment_fields,
            Some(sender),
            Some(room_name),
            None,
        ) {
            return;
        }
        self.push(LocalHistorySearchResult {
            source: LocalHistorySource::OmenChat,
            key,
            sender: bounded_search_text(sender, LOCAL_HISTORY_SEARCH_TEXT_MAX_BYTES),
            context: bounded_search_text(
                &format!("#{room_name} · {server_display_name}"),
                LOCAL_HISTORY_SEARCH_TEXT_MAX_BYTES,
            ),
            excerpt: bounded_search_text(body, LOCAL_HISTORY_SEARCH_TEXT_MAX_BYTES),
            attachment_name: attachment
                .map(|name| bounded_search_text(name, LOCAL_HISTORY_SEARCH_TEXT_MAX_BYTES)),
            at_unix: event.at_unix,
            delivery: None,
        });
    }

    fn finish(&mut self) {
        self.page.results.sort_by(|left, right| {
            right
                .at_unix
                .cmp(&left.at_unix)
                .then_with(|| left.context.cmp(&right.context))
                .then_with(|| left.excerpt.cmp(&right.excerpt))
        });
    }

    fn matches_common(
        &self,
        at_unix: i64,
        searchable: &[&str],
        attachments: &[&str],
        sender: Option<&str>,
        room: Option<&str>,
        delivery: Option<&DeliveryState>,
    ) -> bool {
        if self.query.after_unix.is_some_and(|after| at_unix < after)
            || self
                .query
                .before_unix
                .is_some_and(|before| at_unix > before)
            || self.query.attachment_only && attachments.is_empty()
            || self
                .query
                .delivery
                .as_ref()
                .is_some_and(|expected| delivery != Some(expected))
            || self.query.sender.as_deref().is_some_and(|needle| {
                !sender.is_some_and(|value| contains_ascii_case_insensitive(value, needle))
            })
            || self.query.room.as_deref().is_some_and(|needle| {
                !room.is_some_and(|value| contains_ascii_case_insensitive(value, needle))
            })
        {
            return false;
        }
        self.terms.iter().all(|term| {
            searchable
                .iter()
                .chain(attachments)
                .any(|value| contains_ascii_case_insensitive(value, term))
        })
    }

    fn push(&mut self, result: LocalHistorySearchResult) {
        self.page.results.push(result);
        if self.page.results.len() > LOCAL_HISTORY_SEARCH_RESULT_MAX_ITEMS {
            self.page.result_limit_reached = true;
            self.page.results.sort_by(|left, right| {
                right
                    .at_unix
                    .cmp(&left.at_unix)
                    .then_with(|| left.context.cmp(&right.context))
                    .then_with(|| left.excerpt.cmp(&right.excerpt))
            });
            self.page
                .results
                .truncate(LOCAL_HISTORY_SEARCH_RESULT_MAX_ITEMS);
        }
    }
}

fn message_delivery(message: &MessageSummary) -> DeliveryState {
    if message.incoming {
        DeliveryState::Incoming
    } else if message.failed {
        DeliveryState::Failed
    } else if message.delivered {
        DeliveryState::Delivered
    } else {
        DeliveryState::Pending
    }
}

fn finite_timestamp(timestamp: f64) -> i64 {
    if timestamp.is_finite() {
        timestamp as i64
    } else {
        0
    }
}

fn contains_ascii_case_insensitive(value: &str, needle: &str) -> bool {
    if needle.is_empty() {
        return true;
    }
    value.as_bytes().windows(needle.len()).any(|window| {
        window
            .iter()
            .zip(needle.as_bytes())
            .all(|(left, right)| left.eq_ignore_ascii_case(right))
    })
}

fn bounded_search_text(value: &str, max_bytes: usize) -> String {
    if value.len() <= max_bytes {
        return value.to_owned();
    }
    if max_bytes < '…'.len_utf8() {
        return String::new();
    }
    let mut end = max_bytes - '…'.len_utf8();
    while !value.is_char_boundary(end) {
        end = end.saturating_sub(1);
    }
    format!("{}…", &value[..end])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::messaging::{AttachmentSummary, DeliveryMode};
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    fn message(timestamp: f64, title: &str, content: &str) -> MessageSummary {
        MessageSummary {
            peer_hash: "opaque-peer-hash".into(),
            peer_label: "Alice".into(),
            title: title.into(),
            content: content.into(),
            timestamp,
            transport_method: crate::messaging::TransportMethod::Direct,
            delivered: true,
            failed: false,
            incoming: true,
            unread: false,
            message_id: Some("opaque-message-id".into()),
            fields: BTreeMap::from([("secret".into(), "opaque-private-field".into())]),
            attachments: Vec::new(),
        }
    }

    fn conversation(id: u64, messages: Vec<MessageSummary>) -> Conversation {
        let mut conversation = Conversation::new(id, "opaque-peer-hash", "Alice");
        conversation.delivery_mode = DeliveryMode::Direct;
        conversation.thread.messages = messages;
        conversation
    }

    #[test]
    fn lxmf_search_matches_public_fields_and_excludes_opaque_metadata() {
        let mut attachment_message = message(20.0, "Field report", "Map is attached");
        attachment_message.attachments.push(AttachmentSummary {
            name: "ridge-map.png".into(),
            size: 10,
            path: Some(PathBuf::from("/private/path/ridge-map.png")),
        });
        let conversation = conversation(
            7,
            vec![message(10.0, "Earlier", "quiet"), attachment_message],
        );

        let page = search_local_history(
            [LocalHistorySearchInput::Lxmf(&conversation)],
            &LocalHistorySearchQuery {
                text: "RIDGE map".into(),
                sender: Some("ali".into()),
                attachment_only: true,
                delivery: Some(DeliveryState::Incoming),
                ..LocalHistorySearchQuery::default()
            },
        )
        .expect("search");
        assert_eq!(page.results.len(), 1);
        assert_eq!(
            page.results[0].attachment_name.as_deref(),
            Some("ridge-map.png")
        );
        assert_eq!(page.results[0].at_unix, 20);

        for opaque in [
            "opaque-peer-hash",
            "opaque-message-id",
            "opaque-private-field",
            "/private/path",
        ] {
            let page = search_local_history(
                [LocalHistorySearchInput::Lxmf(&conversation)],
                &LocalHistorySearchQuery {
                    text: opaque.into(),
                    ..LocalHistorySearchQuery::default()
                },
            )
            .expect("opaque search");
            assert!(page.results.is_empty(), "{opaque} must not be searchable");
        }
    }

    #[test]
    fn lxmf_outbound_sender_and_source_filter_are_truthful() {
        let mut outgoing = message(30.0, "Status", "Leaving now");
        outgoing.incoming = false;
        outgoing.delivered = false;
        let conversation = conversation(8, vec![outgoing]);
        let page = search_local_history(
            [LocalHistorySearchInput::Lxmf(&conversation)],
            &LocalHistorySearchQuery {
                sender: Some("you".into()),
                delivery: Some(DeliveryState::Pending),
                source: LocalHistorySourceFilter::Lxmf,
                ..LocalHistorySearchQuery::default()
            },
        )
        .expect("outbound search");
        assert_eq!(page.results.len(), 1);
        assert_eq!(page.results[0].sender, "You");

        let excluded = search_local_history(
            [LocalHistorySearchInput::Lxmf(&conversation)],
            &LocalHistorySearchQuery {
                source: LocalHistorySourceFilter::OmenChat,
                ..LocalHistorySearchQuery::default()
            },
        )
        .expect("source filter");
        assert!(excluded.results.is_empty());
        assert_eq!(excluded.scanned_items, 0);
    }

    #[cfg(feature = "chat-client")]
    #[test]
    fn omenchat_search_matches_sender_room_date_and_attachment_without_ids() {
        use crate::chat::{
            ChatEvent, ChatEventKind, ChatRoomSummary, ChatServerSummary, ChatSessionView,
        };

        let session = ChatSessionView {
            session_id: 9,
            server: ChatServerSummary {
                server_id: "opaque-server-id".into(),
                destination: "opaque-destination".into(),
                display_name: "Field Server".into(),
            },
            rooms: Vec::new(),
            active_room: ChatRoomSummary {
                server_id: "opaque-server-id".into(),
                room_id: 2,
                name: "maps".into(),
                topic: None,
                unread: 0,
                joined: true,
            },
            users: Vec::new(),
            events: vec![ChatEvent {
                server_id: "opaque-server-id".into(),
                room_id: 2,
                event_id: 44,
                actor_user_id: Some(5),
                actor_display_name: Some("Bob".into()),
                at_unix: 50,
                kind: ChatEventKind::Upload {
                    resource_id: "opaque-resource-id".into(),
                    filename: "route.gpx".into(),
                    bytes: 20,
                },
            }],
            status: String::new(),
        };
        let page = search_local_history(
            [LocalHistorySearchInput::OmenChat(&session)],
            &LocalHistorySearchQuery {
                text: "route".into(),
                sender: Some("BOB".into()),
                room: Some("map".into()),
                after_unix: Some(40),
                before_unix: Some(60),
                attachment_only: true,
                source: LocalHistorySourceFilter::OmenChat,
                ..LocalHistorySearchQuery::default()
            },
        )
        .expect("search");
        assert_eq!(page.results.len(), 1);
        assert_eq!(
            page.results[0].attachment_name.as_deref(),
            Some("route.gpx")
        );

        for opaque in [
            "opaque-server-id",
            "opaque-destination",
            "opaque-resource-id",
        ] {
            let page = search_local_history(
                [LocalHistorySearchInput::OmenChat(&session)],
                &LocalHistorySearchQuery {
                    text: opaque.into(),
                    ..LocalHistorySearchQuery::default()
                },
            )
            .expect("opaque search");
            assert!(page.results.is_empty(), "{opaque} must not be searchable");
        }
    }

    #[cfg(feature = "chat-client")]
    #[test]
    fn persisted_search_combines_bounded_stores_with_opaque_routing_keys() {
        use crate::chat::store::{ChatStore, SqliteChatStore};
        use crate::chat::{ChatEvent, ChatEventKind, ChatRoomSummary, ChatServerSummary};

        let root = std::env::temp_dir().join(format!(
            "omenbrowser-persisted-history-search-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system time")
                .as_nanos()
        ));
        let messages = root.join("messages");
        let chat_path = root.join("chat.sqlite");
        let message_store = MessageStore::new(messages).expect("message store");
        message_store
            .append(message(10.0, "LXMF", "shared search phrase"))
            .expect("LXMF message");
        {
            let mut chat = SqliteChatStore::open(&chat_path).expect("chat store");
            chat.save_server(ChatServerSummary {
                server_id: "opaque-server-key".into(),
                destination: "opaque-destination".into(),
                display_name: "Field Server".into(),
            })
            .expect("server");
            chat.save_room(ChatRoomSummary {
                server_id: "opaque-server-key".into(),
                room_id: 2,
                name: "maps".into(),
                topic: None,
                unread: 0,
                joined: true,
            })
            .expect("room");
            chat.append_events(vec![ChatEvent {
                server_id: "opaque-server-key".into(),
                room_id: 2,
                event_id: 44,
                actor_user_id: Some(5),
                actor_display_name: Some("Bob".into()),
                at_unix: 20,
                kind: ChatEventKind::Message {
                    body: "shared search phrase".into(),
                },
            }])
            .expect("event");
        }

        let page = search_persisted_local_history(
            &message_store,
            Some(&chat_path),
            &LocalHistorySearchQuery {
                text: "shared phrase".into(),
                ..LocalHistorySearchQuery::default()
            },
        )
        .expect("persisted search");
        assert_eq!(page.results.len(), 2);
        assert_eq!(page.results[0].source, LocalHistorySource::OmenChat);
        assert!(matches!(
            &page.results[0].key,
            LocalHistoryResultKey::OmenChatStored {
                server_key,
                room_id: 2,
                event_id: 44
            } if server_key == "opaque-server-key"
        ));
        assert!(matches!(
            &page.results[1].key,
            LocalHistoryResultKey::LxmfStored {
                peer_key,
                message_index: 0,
                message_key
            } if peer_key == "opaque-peer-hash"
                && message_key == "opaque-message-id"
        ));
        assert!(page.results.iter().all(|result| {
            !result.context.contains("opaque")
                && !result.excerpt.contains("opaque")
                && !result.sender.contains("opaque")
        }));
        drop(message_store);
        std::fs::remove_dir_all(root).expect("remove search root");
    }

    #[test]
    fn query_result_and_scan_work_are_bounded() {
        let too_many_terms = LocalHistorySearchQuery {
            text: "1 2 3 4 5 6 7 8 9".into(),
            ..LocalHistorySearchQuery::default()
        };
        assert_eq!(
            too_many_terms.validate(),
            Err(LocalHistorySearchError::InvalidQuery)
        );
        assert_eq!(
            LocalHistorySearchQuery {
                text: "line\nbreak".into(),
                ..LocalHistorySearchQuery::default()
            }
            .validate(),
            Err(LocalHistorySearchError::InvalidQuery)
        );

        let conversations = (0..3)
            .map(|conversation_id| {
                conversation(
                    conversation_id,
                    (0..3_000)
                        .map(|index| message(index as f64, "match", "match"))
                        .collect(),
                )
            })
            .collect::<Vec<_>>();
        let page = search_local_history(
            conversations.iter().map(LocalHistorySearchInput::Lxmf),
            &LocalHistorySearchQuery {
                text: "match".into(),
                ..LocalHistorySearchQuery::default()
            },
        )
        .expect("bounded search");
        assert_eq!(page.scanned_items, LOCAL_HISTORY_SEARCH_SCAN_MAX_ITEMS);
        assert!(page.scan_limit_reached);
        assert!(page.result_limit_reached);
        assert_eq!(page.results.len(), LOCAL_HISTORY_SEARCH_RESULT_MAX_ITEMS);
        assert!(page
            .results
            .windows(2)
            .all(|window| window[0].at_unix >= window[1].at_unix));
    }

    #[test]
    #[ignore = "opt-in 64 MiB local-history search measurement"]
    fn measure_maximum_bounded_lxmf_search() {
        use std::time::Instant;

        const CONVERSATION_COUNT: usize = 2;
        const MESSAGE_TEXT_BYTES: usize = 8 * 1024;
        let conversations = (0..CONVERSATION_COUNT)
            .map(|conversation_id| {
                conversation(
                    conversation_id as u64,
                    (0..crate::messaging::store::MESSAGE_STORE_THREAD_MAX_MESSAGES)
                        .map(|index| {
                            let marker = if index % 32 == 0 {
                                " bounded-search-hit "
                            } else {
                                " "
                            };
                            let mut body = "x".repeat(MESSAGE_TEXT_BYTES - marker.len());
                            body.push_str(marker);
                            message(index as f64, "measurement", &body)
                        })
                        .collect(),
                )
            })
            .collect::<Vec<_>>();
        let retained_text_bytes = conversations
            .iter()
            .flat_map(|conversation| conversation.thread.messages.iter())
            .map(|message| message.content.len())
            .sum::<usize>();
        assert_eq!(
            conversations
                .iter()
                .map(|conversation| conversation.thread.messages.len())
                .sum::<usize>(),
            LOCAL_HISTORY_SEARCH_SCAN_MAX_ITEMS
        );
        assert_eq!(
            retained_text_bytes,
            LOCAL_HISTORY_SEARCH_SCAN_MAX_ITEMS * MESSAGE_TEXT_BYTES
        );

        let miss_started = Instant::now();
        let miss = search_local_history(
            conversations.iter().map(LocalHistorySearchInput::Lxmf),
            &LocalHistorySearchQuery {
                text: "definitely-absent-search-term".into(),
                ..LocalHistorySearchQuery::default()
            },
        )
        .expect("bounded miss");
        let miss_elapsed = miss_started.elapsed();

        let hit_started = Instant::now();
        let hit = search_local_history(
            conversations.iter().map(LocalHistorySearchInput::Lxmf),
            &LocalHistorySearchQuery {
                text: "bounded-search-hit".into(),
                ..LocalHistorySearchQuery::default()
            },
        )
        .expect("bounded hit");
        let hit_elapsed = hit_started.elapsed();

        assert_eq!(miss.scanned_items, LOCAL_HISTORY_SEARCH_SCAN_MAX_ITEMS);
        assert!(miss.results.is_empty());
        assert_eq!(hit.scanned_items, LOCAL_HISTORY_SEARCH_SCAN_MAX_ITEMS);
        assert_eq!(hit.results.len(), LOCAL_HISTORY_SEARCH_RESULT_MAX_ITEMS);
        assert!(hit.result_limit_reached);
        println!(
            "local_history_search_measurement retained_text_bytes={retained_text_bytes} \
             scanned_items={} miss_micros={} capped_hit_micros={} results={}",
            miss.scanned_items,
            miss_elapsed.as_micros(),
            hit_elapsed.as_micros(),
            hit.results.len()
        );
    }

    #[test]
    fn excerpts_preserve_utf8_and_hard_byte_bounds() {
        let conversation = conversation(
            1,
            vec![message(
                1.0,
                "snow",
                &"☃".repeat(LOCAL_HISTORY_SEARCH_TEXT_MAX_BYTES),
            )],
        );
        let page = search_local_history(
            [LocalHistorySearchInput::Lxmf(&conversation)],
            &LocalHistorySearchQuery {
                text: "snow".into(),
                ..LocalHistorySearchQuery::default()
            },
        )
        .expect("search");
        assert_eq!(page.results.len(), 1);
        assert!(page.results[0].excerpt.len() <= LOCAL_HISTORY_SEARCH_TEXT_MAX_BYTES);
        assert!(page.results[0].excerpt.ends_with('…'));
    }
}
