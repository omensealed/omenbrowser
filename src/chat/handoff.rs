use serde::{Deserialize, Serialize};

pub const OMENCHAT_INVITE_PROTOCOL: &str = "omenchat.lxmf.invite";
pub const OMENCHAT_INVITE_VERSION: u16 = 1;
pub const OMENCHAT_RESOURCE_METADATA_PREFIX: &str = "omenchat-resource:";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum OmenChatInviteRole {
    #[default]
    Guest,
    Member,
    Mod,
    Admin,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct OmenChatInvitePayload {
    pub protocol: String,
    pub version: u16,
    pub server_destination: String,
    pub room_id: String,
    pub room_display_name: String,
    pub inviter_display_name: String,
    pub inviter_destination: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub invite_token: Option<String>,
    pub room_password_required: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at_unix: Option<u64>,
    pub requested_role: OmenChatInviteRole,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub intro_message: Option<String>,
}

impl OmenChatInvitePayload {
    pub fn new(
        server_destination: impl Into<String>,
        room_id: impl Into<String>,
        room_display_name: impl Into<String>,
        inviter_display_name: impl Into<String>,
        inviter_destination: impl Into<String>,
    ) -> Self {
        Self {
            protocol: OMENCHAT_INVITE_PROTOCOL.into(),
            version: OMENCHAT_INVITE_VERSION,
            server_destination: server_destination.into(),
            room_id: room_id.into(),
            room_display_name: room_display_name.into(),
            inviter_display_name: inviter_display_name.into(),
            inviter_destination: inviter_destination.into(),
            invite_token: None,
            room_password_required: false,
            expires_at_unix: None,
            requested_role: OmenChatInviteRole::Guest,
            intro_message: None,
        }
    }

    pub fn encode(&self) -> Result<Vec<u8>, serde_json::Error> {
        serde_json::to_vec(self)
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, serde_json::Error> {
        serde_json::from_slice(bytes)
    }

    pub fn redacted_for_log(&self) -> Self {
        let mut redacted = self.clone();
        if redacted.invite_token.is_some() {
            redacted.invite_token = Some("[redacted]".into());
        }
        redacted
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OmenChatResourcePurpose {
    HistoryBatch,
    UserListSnapshot,
    MediaUpload,
    MediaDownload,
    RoomExport,
    ServerNotice,
}

impl OmenChatResourcePurpose {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::HistoryBatch => "history_batch",
            Self::UserListSnapshot => "user_list_snapshot",
            Self::MediaUpload => "media_upload",
            Self::MediaDownload => "media_download",
            Self::RoomExport => "room_export",
            Self::ServerNotice => "server_notice",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "history_batch" => Some(Self::HistoryBatch),
            "user_list_snapshot" => Some(Self::UserListSnapshot),
            "media_upload" => Some(Self::MediaUpload),
            "media_download" => Some(Self::MediaDownload),
            "room_export" => Some(Self::RoomExport),
            "server_notice" => Some(Self::ServerNotice),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct OmenChatResourceMetadata {
    pub purpose: OmenChatResourcePurpose,
    pub transfer_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub room_id: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filename: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size_bytes: Option<u64>,
}

impl OmenChatResourceMetadata {
    pub fn encode(&self) -> Result<Vec<u8>, serde_json::Error> {
        let json = serde_json::to_string(self)?;
        Ok(format!("{OMENCHAT_RESOURCE_METADATA_PREFIX}{json}").into_bytes())
    }

    pub fn decode(bytes: &[u8]) -> Result<Option<Self>, serde_json::Error> {
        let Some(metadata) = std::str::from_utf8(bytes)
            .ok()
            .and_then(|value| value.strip_prefix(OMENCHAT_RESOURCE_METADATA_PREFIX))
        else {
            return Ok(None);
        };
        serde_json::from_str(metadata).map(Some)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_omenchat_invite_roundtrip() {
        let mut invite = OmenChatInvitePayload::new(
            "omenchat://0123456789abcdef",
            "lobby",
            "Lobby",
            "OMENbrowser_dev",
            "4d5b6489674775b5423e8a80d9d95409",
        );
        invite.invite_token = Some("secret-token".into());
        invite.room_password_required = true;
        invite.expires_at_unix = Some(1_800_000_000);
        invite.requested_role = OmenChatInviteRole::Member;
        invite.intro_message = Some("join the room".into());

        let encoded = invite.encode().expect("encode invite");
        let decoded = OmenChatInvitePayload::decode(&encoded).expect("decode invite");

        assert_eq!(decoded, invite);
        assert_eq!(
            decoded.redacted_for_log().invite_token.as_deref(),
            Some("[redacted]")
        );
    }

    #[test]
    fn test_omenchat_resource_metadata_roundtrip() {
        let metadata = OmenChatResourceMetadata {
            purpose: OmenChatResourcePurpose::MediaUpload,
            transfer_id: "upload-1".into(),
            room_id: Some(1),
            cursor: None,
            filename: Some("image.gif".into()),
            content_type: Some("image/gif".into()),
            size_bytes: Some(42_000),
        };

        let encoded = metadata.encode().expect("encode metadata");
        let decoded = OmenChatResourceMetadata::decode(&encoded)
            .expect("decode metadata")
            .expect("metadata prefix");

        assert_eq!(decoded, metadata);
        assert_eq!(
            OmenChatResourcePurpose::parse(OmenChatResourcePurpose::RoomExport.as_str()),
            Some(OmenChatResourcePurpose::RoomExport)
        );
        assert_eq!(
            OmenChatResourceMetadata::decode(b"not-omenchat").expect("ignore non-metadata"),
            None
        );
    }
}
