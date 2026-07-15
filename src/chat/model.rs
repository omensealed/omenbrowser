use super::protocol::{EventId, RoomId, ServerId, UserId};

pub const CHAT_CLIENT_MAX_SESSIONS: usize = 64;
pub const CHAT_SESSION_MAX_ROOMS: usize = 256;
pub const CHAT_SESSION_MAX_ROOM_BYTES: usize = 512 * 1024;
pub const CHAT_SESSION_MAX_USERS: usize = 1_024;
pub const CHAT_SESSION_MAX_USER_BYTES: usize = 1024 * 1024;
pub const CHAT_SERVER_DISPLAY_MAX_BYTES: usize = 256;
pub const CHAT_SERVER_ID_MAX_BYTES: usize = 4 * 1024;
pub const CHAT_SERVER_DESTINATION_MAX_BYTES: usize = 4 * 1024;
pub const CHAT_ROOM_NAME_MAX_BYTES: usize = 64;
pub const CHAT_ROOM_TOPIC_MAX_BYTES: usize = 4 * 1024;
pub const CHAT_USER_DISPLAY_MAX_BYTES: usize = 256;
pub const CHAT_ACTOR_DISPLAY_MAX_BYTES: usize = 256;
pub const CHAT_MOTD_MAX_BYTES: usize = 16 * 1024;
pub const CHAT_STATUS_MAX_BYTES: usize = 4 * 1024;
pub const CHAT_RESOURCE_ID_MAX_BYTES: usize = 4 * 1024;
pub const CHAT_UPLOAD_FILENAME_MAX_BYTES: usize = 4 * 1024;
pub const CHAT_CONTENT_TYPE_MAX_BYTES: usize = 1_024;

pub fn bounded_chat_text(value: &str, max_bytes: usize) -> String {
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
    let mut bounded = String::with_capacity(max_bytes);
    bounded.push_str(&value[..end]);
    bounded.push('…');
    bounded
}

pub fn chat_text_fits(value: &str, max_bytes: usize) -> bool {
    value.len() <= max_bytes
}

pub const CHAT_STATUS_BANNED: u32 = 1;
pub const CHAT_STATUS_MUTED: u32 = 1 << 1;
pub const CHAT_ROLE_TRUSTED: u64 = 1;
pub const CHAT_ROLE_MODERATOR: u64 = 1 << 1;
pub const CHAT_ROLE_ADMIN: u64 = 1 << 2;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChatServerSummary {
    pub server_id: ServerId,
    pub destination: String,
    pub display_name: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChatRoomSummary {
    pub server_id: ServerId,
    pub room_id: RoomId,
    pub name: String,
    pub topic: Option<String>,
    pub unread: u32,
    pub joined: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChatUserSummary {
    pub server_id: ServerId,
    pub user_id: UserId,
    pub display_name: String,
    pub role_bits: u64,
    pub status_bits: u32,
    pub lxmf_available: bool,
}

impl ChatUserSummary {
    pub fn role_label(&self) -> &'static str {
        if self.role_bits & CHAT_ROLE_ADMIN != 0 {
            "admin"
        } else if self.role_bits & CHAT_ROLE_MODERATOR != 0 {
            "mod"
        } else if self.role_bits & CHAT_ROLE_TRUSTED != 0 {
            "trusted"
        } else {
            "member"
        }
    }

    pub fn status_label(&self) -> Option<&'static str> {
        if self.status_bits & CHAT_STATUS_BANNED != 0 {
            Some("banned")
        } else if self.status_bits & CHAT_STATUS_MUTED != 0 {
            Some("muted")
        } else {
            None
        }
    }

    pub fn display_label(&self) -> String {
        let mut label = self.display_name.clone();
        let role = self.role_label();
        if role != "member" {
            label.push_str(" [");
            label.push_str(role);
            label.push(']');
        }
        if let Some(status) = self.status_label() {
            label.push_str(" (");
            label.push_str(status);
            label.push(')');
        }
        label
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChatEvent {
    pub server_id: ServerId,
    pub room_id: RoomId,
    pub event_id: EventId,
    pub actor_user_id: Option<UserId>,
    pub actor_display_name: Option<String>,
    pub at_unix: i64,
    pub kind: ChatEventKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChatEventKind {
    Message {
        body: String,
    },
    Action {
        body: String,
    },
    Notice {
        body: String,
    },
    System {
        body: String,
    },
    Upload {
        resource_id: String,
        filename: String,
        bytes: u64,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn user(role_bits: u64, status_bits: u32) -> ChatUserSummary {
        ChatUserSummary {
            server_id: "server".into(),
            user_id: 1,
            display_name: "Alice".into(),
            role_bits,
            status_bits,
            lxmf_available: false,
        }
    }

    #[test]
    fn bounded_chat_text_preserves_utf8_and_exact_byte_ceiling() {
        assert_eq!(bounded_chat_text("hello", 5), "hello");
        let bounded = bounded_chat_text(&"☃".repeat(10), 10);
        assert!(bounded.is_char_boundary(bounded.len()));
        assert!(bounded.len() <= 10);
        assert!(bounded.ends_with('…'));
        assert_eq!(bounded_chat_text("hello", 2), "");
    }

    #[test]
    fn user_display_labels_include_roles_and_moderation_status() {
        assert_eq!(user(0, 0).display_label(), "Alice");
        assert_eq!(
            user(CHAT_ROLE_TRUSTED, 0).display_label(),
            "Alice [trusted]"
        );
        assert_eq!(
            user(CHAT_ROLE_TRUSTED | CHAT_ROLE_MODERATOR, CHAT_STATUS_MUTED).display_label(),
            "Alice [mod] (muted)"
        );
        assert_eq!(
            user(
                CHAT_ROLE_TRUSTED | CHAT_ROLE_MODERATOR | CHAT_ROLE_ADMIN,
                CHAT_STATUS_BANNED
            )
            .display_label(),
            "Alice [admin] (banned)"
        );
    }
}
