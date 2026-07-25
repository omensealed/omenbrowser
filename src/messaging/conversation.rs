use std::collections::BTreeSet;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::messaging::{
    DeliveryMode, MessageSummary, NativeLxmfReplyTicket, OutboundOperationIdentity,
};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ConversationThread {
    pub peer_hash: String,
    pub peer_label: String,
    pub messages: Vec<MessageSummary>,
    pub unread_count: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lxmf_reply_ticket: Option<NativeLxmfReplyTicket>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MessageSendState {
    pub generation: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PreparedRetryOperation {
    pub identity: OutboundOperationIdentity,
    pub title: String,
    pub body: String,
    pub attachments: Vec<PathBuf>,
    pub delivery_mode: DeliveryMode,
    pub include_ticket: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DirectStampConfirmation {
    pub peer_hash: String,
    pub title: String,
    pub body: String,
    pub attachments: Vec<PathBuf>,
    pub delivery_mode: DeliveryMode,
    pub include_ticket: bool,
    pub advertised_cost: u8,
    pub ask_above: u8,
}

impl DirectStampConfirmation {
    pub fn matches_draft(&self, conversation: &Conversation) -> bool {
        self.peer_hash == conversation.peer_hash
            && self.title == conversation.draft_title
            && self.body == conversation.draft_body
            && self.attachments == conversation.attachments
            && self.delivery_mode == conversation.delivery_mode
            && self.include_ticket == conversation.include_ticket
    }
}

impl PreparedRetryOperation {
    pub fn matches_draft(&self, conversation: &Conversation) -> bool {
        self.title == conversation.draft_title
            && self.body == conversation.draft_body
            && self.attachments == conversation.attachments
            && self.delivery_mode == conversation.delivery_mode
            && self.include_ticket == conversation.include_ticket
    }
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
    pub prepared_retry_operation: Option<PreparedRetryOperation>,
    pub direct_stamp_confirmation: Option<DirectStampConfirmation>,
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
                lxmf_reply_ticket: None,
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
            prepared_retry_operation: None,
            direct_stamp_confirmation: None,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prepared_retry_identity_applies_only_to_the_unchanged_draft() {
        let mut conversation = Conversation::new(1, "peer", "Peer");
        conversation.draft_title = "Title".into();
        conversation.draft_body = "Body".into();
        let prepared = PreparedRetryOperation {
            identity: OutboundOperationIdentity::generate(),
            title: conversation.draft_title.clone(),
            body: conversation.draft_body.clone(),
            attachments: Vec::new(),
            delivery_mode: DeliveryMode::Direct,
            include_ticket: false,
        };

        assert!(prepared.matches_draft(&conversation));
        conversation.draft_body.push_str(" edited");
        assert!(!prepared.matches_draft(&conversation));
    }
}
