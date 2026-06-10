use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TransportMethod {
    Direct,
    Propagated,
    Unknown(String),
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DeliveryMode {
    Direct,
    Propagated,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DeliveryState {
    Incoming,
    Pending,
    Delivered,
    Failed,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct AttachmentSummary {
    pub name: String,
    pub size: u64,
    pub path: Option<PathBuf>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct MessageSummary {
    pub peer_hash: String,
    pub peer_label: String,
    pub title: String,
    pub content: String,
    pub timestamp: f64,
    pub transport_method: TransportMethod,
    pub delivered: bool,
    pub failed: bool,
    pub incoming: bool,
    pub unread: bool,
    pub message_id: Option<String>,
    pub fields: std::collections::BTreeMap<String, String>,
    pub attachments: Vec<AttachmentSummary>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct MessageEnvelope {
    pub peer_hash: String,
    pub title: String,
    pub body: String,
    pub delivery_mode: DeliveryMode,
    pub include_ticket: bool,
    pub attachments: Vec<PathBuf>,
}
