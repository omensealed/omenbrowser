use std::collections::BTreeMap;

use omenbrowser_rs::messaging::conversation::Conversation;
use omenbrowser_rs::messaging::{MessageSummary, TransportMethod};

#[test]
fn conversation_tracks_messages_and_unread_count() {
    let mut conversation = Conversation::new(1, "peer", "Peer");
    conversation.push_message(MessageSummary {
        peer_hash: "peer".into(),
        peer_label: "Peer".into(),
        title: "hello".into(),
        content: "body".into(),
        timestamp: 1.0,
        transport_method: TransportMethod::Direct,
        delivered: false,
        failed: false,
        incoming: true,
        unread: true,
        message_id: Some("m1".into()),
        fields: BTreeMap::new(),
        attachments: Vec::new(),
    });

    assert_eq!(conversation.thread.messages.len(), 1);
    assert_eq!(conversation.thread.unread_count, 1);
}
