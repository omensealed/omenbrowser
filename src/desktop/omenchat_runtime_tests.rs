use super::*;
use crate::chat::{ChatEvent, ChatEventKind, ChatRoomSummary, ChatServerSummary};

#[test]
fn omenchat_event_counts_are_scoped_by_room() {
    let session = ChatSessionView {
        session_id: 9,
        server: ChatServerSummary {
            server_id: "server-a".into(),
            destination: "abcd".into(),
            display_name: "Server A".into(),
        },
        rooms: Vec::new(),
        active_room: ChatRoomSummary {
            server_id: "server-a".into(),
            room_id: 3,
            name: "empty-active".into(),
            topic: None,
            unread: 0,
            joined: true,
        },
        users: Vec::new(),
        events: vec![
            ChatEvent {
                server_id: "server-a".into(),
                room_id: 1,
                event_id: 1,
                actor_user_id: None,
                actor_display_name: Some("Alice".into()),
                at_unix: 1,
                kind: ChatEventKind::Message { body: "one".into() },
            },
            ChatEvent {
                server_id: "server-a".into(),
                room_id: 1,
                event_id: 2,
                actor_user_id: None,
                actor_display_name: Some("Alice".into()),
                at_unix: 2,
                kind: ChatEventKind::Message { body: "two".into() },
            },
            ChatEvent {
                server_id: "server-a".into(),
                room_id: 2,
                event_id: 3,
                actor_user_id: None,
                actor_display_name: Some("Bob".into()),
                at_unix: 3,
                kind: ChatEventKind::Message {
                    body: "other".into(),
                },
            },
        ],
        status: String::new(),
    };

    let counts = omenchat_event_counts_by_room(&[session]);
    assert_eq!(counts.get(&(9, 1)), Some(&2));
    assert_eq!(counts.get(&(9, 2)), Some(&1));
    assert_eq!(counts.get(&(9, 3)), Some(&0));
}
