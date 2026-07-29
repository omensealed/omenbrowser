use std::collections::HashSet;

use crate::chat::{
    ChatClientEvent, ChatEvent, ChatEventKind, ChatSessionView, ChatUserSummary, OmenChatDescriptor,
};

use super::super::OmenChatDraftCommandResult;
use super::super::OMENCHAT_PENDING_DESTINATION_PREFIX;
use super::human_bytes;

pub(in crate::desktop) fn is_omenchat_local_echo_event(event: &ChatEvent) -> bool {
    event.event_id > u64::MAX.saturating_sub(1_000_000)
}

pub(in crate::desktop) fn omenchat_upload_policy_rejection(
    bytes: u64,
    quota: Option<u64>,
    max_file_bytes: Option<u64>,
) -> Option<String> {
    match quota {
        Some(0) => Some("upload blocked: server has uploads disabled".into()),
        _ => match max_file_bytes {
            Some(0) => Some("upload blocked: uploads are disabled in this room".into()),
            Some(limit) if bytes > limit => Some(format!(
                "upload blocked: {} exceeds server file limit {}",
                human_bytes(bytes),
                human_bytes(limit)
            )),
            _ => None,
        },
    }
}

pub(in crate::desktop) fn apply_omenchat_link_fields(
    descriptor: &mut OmenChatDescriptor,
    fields: &[String],
) -> bool {
    descriptor.apply_link_fields(fields)
}

pub(in crate::desktop) fn normalize_omenchat_manual_target(input: &str) -> Option<String> {
    let trimmed = input.trim();
    let destination = trimmed
        .strip_prefix("omenchat://")
        .or_else(|| trimmed.strip_prefix("omenchat:"))
        .unwrap_or(trimmed)
        .trim()
        .trim_start_matches('/');
    if destination.len() < 32 || !destination.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return None;
    }
    Some(format!("omenchat://{}", destination.to_ascii_lowercase()))
}

pub(in crate::desktop) fn is_pending_omenchat_destination(destination: &str) -> bool {
    destination.starts_with(OMENCHAT_PENDING_DESTINATION_PREFIX)
}

pub(in crate::desktop) fn omenchat_upload_content_type(filename: &str) -> Option<String> {
    let ext = std::path::Path::new(filename)
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase())?;
    let content_type = match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "txt" | "log" | "md" => "text/plain",
        "json" => "application/json",
        "pdf" => "application/pdf",
        "zip" => "application/zip",
        _ => "application/octet-stream",
    };
    Some(content_type.into())
}

pub(in crate::desktop) fn unique_chat_users(users: &[ChatUserSummary]) -> Vec<&ChatUserSummary> {
    let mut seen_ids = HashSet::new();
    let mut seen_names = HashSet::new();
    let mut unique = Vec::new();
    for user in users {
        let normalized_name = user.display_name.trim().to_ascii_lowercase();
        if !seen_ids.insert(user.user_id) && !normalized_name.is_empty() {
            continue;
        }
        if !normalized_name.is_empty() && !seen_names.insert(normalized_name) {
            continue;
        }
        unique.push(user);
    }
    unique
}

pub(in crate::desktop) fn omenchat_command_result_from_events(
    events: &[ChatClientEvent],
) -> OmenChatDraftCommandResult {
    if events
        .iter()
        .any(|event| matches!(event, ChatClientEvent::Error { .. }))
    {
        OmenChatDraftCommandResult::HandledKeep
    } else {
        OmenChatDraftCommandResult::HandledClear
    }
}

pub(in crate::desktop) fn chat_event_actor_label(
    session: &ChatSessionView,
    event: &ChatEvent,
) -> String {
    event.actor_display_name.clone().unwrap_or_else(|| {
        event
            .actor_user_id
            .and_then(|actor_id| {
                session
                    .users
                    .iter()
                    .find(|user| user.user_id == actor_id)
                    .map(|user| user.display_name.clone())
            })
            .unwrap_or_else(|| match event.kind {
                ChatEventKind::System { .. } => "system".into(),
                ChatEventKind::Upload { .. } => "upload".into(),
                ChatEventKind::Notice { .. } => "notice".into(),
                _ => "unknown".into(),
            })
    })
}

#[cfg(all(test, feature = "chat-client"))]
mod tests {
    use super::*;
    use crate::chat::ChatClientEvent;

    #[test]
    fn omenchat_command_result_keeps_draft_on_server_error() {
        assert_eq!(
            omenchat_command_result_from_events(&[ChatClientEvent::Error {
                session_id: Some(1),
                message: "permission denied: topic changes require moderator or admin role".into(),
            }]),
            OmenChatDraftCommandResult::HandledKeep
        );
        assert_eq!(
            omenchat_command_result_from_events(&[ChatClientEvent::RoomsUpdated {
                session_id: 1,
                rooms: Vec::new(),
            }]),
            OmenChatDraftCommandResult::HandledClear
        );
    }

    #[test]
    fn omenchat_manual_target_accepts_canonical_legacy_or_raw_hash() {
        let destination = "ffeeddccbbaa99887766554433221100";
        let uppercase = destination.to_ascii_uppercase();
        let canonical = format!("omenchat://{destination}");
        assert_eq!(
            normalize_omenchat_manual_target(&format!("omenchat://{uppercase}")).as_deref(),
            Some(canonical.as_str())
        );
        assert_eq!(
            normalize_omenchat_manual_target(&format!("omenchat:{uppercase}")).as_deref(),
            Some(canonical.as_str())
        );
        assert_eq!(
            normalize_omenchat_manual_target(destination).as_deref(),
            Some(canonical.as_str())
        );
        assert!(normalize_omenchat_manual_target("mockchatdestination").is_none());
    }

    #[test]
    fn omenchat_upload_file_limit_rejects_oversized_local_files() {
        assert_eq!(
            omenchat_upload_policy_rejection(512, Some(50 * 1024 * 1024), None),
            None
        );
        assert_eq!(
            omenchat_upload_policy_rejection(512, Some(50 * 1024 * 1024), Some(512)),
            None
        );
        assert_eq!(
            omenchat_upload_policy_rejection(1, Some(0), Some(512)),
            Some("upload blocked: server has uploads disabled".into())
        );
        assert_eq!(
            omenchat_upload_policy_rejection(1, Some(50 * 1024 * 1024), Some(0)),
            Some("upload blocked: uploads are disabled in this room".into())
        );
        assert_eq!(
            omenchat_upload_policy_rejection(1024, Some(50 * 1024 * 1024), Some(512)),
            Some("upload blocked: 1.0 KiB exceeds server file limit 512 B".into())
        );
    }
}
