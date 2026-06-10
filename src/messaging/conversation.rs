use std::collections::BTreeSet;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::messaging::{DeliveryMode, MessageSummary};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ConversationThread {
    pub peer_hash: String,
    pub peer_label: String,
    pub messages: Vec<MessageSummary>,
    pub unread_count: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MessageSendState {
    pub generation: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Conversation {
    pub id: u64,
    pub peer_hash: String,
    pub peer_label: String,
    pub thread: ConversationThread,
    pub draft_title: String,
    pub draft_body: String,
    pub attachments: Vec<PathBuf>,
    pub delivery_mode: DeliveryMode,
    pub include_ticket: bool,
    pub unread_at_open: u32,
    pub pending_send: Option<MessageSendState>,
    pub selected_message_key: Option<String>,
    pub dismissed_message_keys: BTreeSet<String>,
}

pub type ConversationTab = Conversation;

impl Conversation {
    pub fn new(id: u64, peer_hash: impl Into<String>, peer_label: impl Into<String>) -> Self {
        let peer_hash = peer_hash.into();
        let peer_label = peer_label.into();
        Self {
            id,
            thread: ConversationThread {
                peer_hash: peer_hash.clone(),
                peer_label: peer_label.clone(),
                messages: Vec::new(),
                unread_count: 0,
            },
            peer_hash,
            peer_label,
            draft_title: String::new(),
            draft_body: String::new(),
            attachments: Vec::new(),
            delivery_mode: DeliveryMode::Direct,
            include_ticket: false,
            unread_at_open: 0,
            pending_send: None,
            selected_message_key: None,
            dismissed_message_keys: BTreeSet::new(),
        }
    }

    pub fn push_message(&mut self, message: MessageSummary) {
        if message.unread {
            self.thread.unread_count += 1;
        }
        self.thread.messages.push(message);
    }
}
