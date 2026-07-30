use thiserror::Error;

use crate::directory::{DirectoryEntry, DirectoryKind};

use super::model::CHAT_SERVER_DISPLAY_MAX_BYTES;

pub const OMENCHAT_INVITATION_MAX_BYTES: usize = 2 * 1024;
pub const OMENCHAT_INVITATION_DESTINATION_HEX_BYTES: usize = 32;
pub const OMENCHAT_INVITATION_IDENTITY_HEX_BYTES: usize = 32;
const OMENCHAT_INVITATION_MAX_FIELDS: usize = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OmenChatInvitationKind {
    LegacyLaunch,
    Enhanced,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OmenChatInvitation {
    pub kind: OmenChatInvitationKind,
    pub server_destination: String,
    pub room_id: Option<u32>,
    pub display_label: Option<String>,
    pub claimed_identity_hash: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OmenChatInvitationIdentityEvidence {
    NotProvided,
    Unverified,
    VerifiedMatch { trusted: bool },
    Conflict,
}

impl OmenChatInvitationIdentityEvidence {
    pub fn label(self) -> &'static str {
        match self {
            Self::NotProvided => "no identity fingerprint supplied",
            Self::Unverified => "identity fingerprint unverified",
            Self::VerifiedMatch { trusted: true } => "verified match · trusted directory entry",
            Self::VerifiedMatch { trusted: false } => "verified match · not trusted",
            Self::Conflict => "identity fingerprint conflicts with directory evidence",
        }
    }

    pub fn allows_confirmation(self) -> bool {
        !matches!(self, Self::Conflict)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OmenChatInvitationPreview {
    pub invitation: OmenChatInvitation,
    pub identity_evidence: OmenChatInvitationIdentityEvidence,
}

impl OmenChatInvitationPreview {
    pub fn new(invitation: OmenChatInvitation, directory: &[DirectoryEntry]) -> Self {
        let identity_evidence = assess_identity_evidence(&invitation, directory);
        Self {
            invitation,
            identity_evidence,
        }
    }

    pub fn allows_confirmation(&self) -> bool {
        self.identity_evidence.allows_confirmation()
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OmenChatInvitationPreviewOwner {
    pending: Option<OmenChatInvitationPreview>,
}

impl OmenChatInvitationPreviewOwner {
    pub fn pending(&self) -> Option<&OmenChatInvitationPreview> {
        self.pending.as_ref()
    }

    pub fn replace_from_uri(
        &mut self,
        value: &str,
        directory: &[DirectoryEntry],
    ) -> Result<(), OmenChatInvitationError> {
        let invitation = OmenChatInvitation::parse(value)?;
        self.pending = Some(OmenChatInvitationPreview::new(invitation, directory));
        Ok(())
    }

    pub fn cancel(&mut self) -> bool {
        self.pending.take().is_some()
    }

    pub fn take_confirmable(&mut self) -> Option<OmenChatInvitation> {
        if !self.pending.as_ref()?.allows_confirmation() {
            return None;
        }
        self.pending.take().map(|preview| preview.invitation)
    }
}

#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub enum OmenChatInvitationError {
    #[error("OMENchat invitation exceeds its byte limit")]
    TooLarge,
    #[error("OMENchat invitation has an invalid scheme or destination")]
    InvalidDestination,
    #[error("OMENchat invitation query is malformed")]
    InvalidQuery,
    #[error("OMENchat invitation contains an unsupported or duplicate field")]
    InvalidField,
    #[error("OMENchat invitation room is invalid")]
    InvalidRoom,
    #[error("OMENchat invitation label is invalid")]
    InvalidLabel,
    #[error("OMENchat invitation identity fingerprint is invalid")]
    InvalidIdentity,
}

impl OmenChatInvitation {
    pub fn new(server_destination: impl AsRef<str>) -> Result<Self, OmenChatInvitationError> {
        let server_destination = normalized_hex(
            server_destination.as_ref(),
            OMENCHAT_INVITATION_DESTINATION_HEX_BYTES,
        )
        .ok_or(OmenChatInvitationError::InvalidDestination)?;
        Ok(Self {
            kind: OmenChatInvitationKind::Enhanced,
            server_destination,
            room_id: None,
            display_label: None,
            claimed_identity_hash: None,
        })
    }

    pub fn parse(value: &str) -> Result<Self, OmenChatInvitationError> {
        if value.len() > OMENCHAT_INVITATION_MAX_BYTES {
            return Err(OmenChatInvitationError::TooLarge);
        }
        if value.chars().any(char::is_control) || value.contains('#') {
            return Err(OmenChatInvitationError::InvalidQuery);
        }
        let body = value
            .strip_prefix("omenchat://")
            .ok_or(OmenChatInvitationError::InvalidDestination)?;
        let (destination, query) = match body.split_once('?') {
            Some((destination, query)) => (destination, Some(query)),
            None => (body, None),
        };
        let server_destination =
            normalized_hex(destination, OMENCHAT_INVITATION_DESTINATION_HEX_BYTES)
                .ok_or(OmenChatInvitationError::InvalidDestination)?;
        let Some(query) = query else {
            return Ok(Self {
                kind: OmenChatInvitationKind::LegacyLaunch,
                server_destination,
                room_id: None,
                display_label: None,
                claimed_identity_hash: None,
            });
        };
        if query.is_empty() {
            return Err(OmenChatInvitationError::InvalidQuery);
        }

        let mut invite_version = None;
        let mut room_id = None;
        let mut display_label = None;
        let mut claimed_identity_hash = None;
        let mut field_count = 0usize;
        for field in query.split('&') {
            field_count = field_count.saturating_add(1);
            if field_count > OMENCHAT_INVITATION_MAX_FIELDS || field.is_empty() {
                return Err(OmenChatInvitationError::InvalidField);
            }
            let (key, value) = field
                .split_once('=')
                .ok_or(OmenChatInvitationError::InvalidField)?;
            match key {
                "invite" if invite_version.is_none() && value == "1" => {
                    invite_version = Some(1);
                }
                "room" if room_id.is_none() => {
                    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
                        return Err(OmenChatInvitationError::InvalidRoom);
                    }
                    room_id = Some(
                        value
                            .parse()
                            .map_err(|_| OmenChatInvitationError::InvalidRoom)?,
                    );
                }
                "label" if display_label.is_none() => {
                    let label = percent_decode_label(value)?;
                    display_label = Some(label);
                }
                "identity" if claimed_identity_hash.is_none() => {
                    claimed_identity_hash = Some(
                        normalized_hex(value, OMENCHAT_INVITATION_IDENTITY_HEX_BYTES)
                            .ok_or(OmenChatInvitationError::InvalidIdentity)?,
                    );
                }
                _ => return Err(OmenChatInvitationError::InvalidField),
            }
        }
        if invite_version != Some(1) {
            return Err(OmenChatInvitationError::InvalidQuery);
        }
        Ok(Self {
            kind: OmenChatInvitationKind::Enhanced,
            server_destination,
            room_id,
            display_label,
            claimed_identity_hash,
        })
    }

    pub fn canonical_uri(&self) -> Result<String, OmenChatInvitationError> {
        let destination = normalized_hex(
            &self.server_destination,
            OMENCHAT_INVITATION_DESTINATION_HEX_BYTES,
        )
        .ok_or(OmenChatInvitationError::InvalidDestination)?;
        if self.kind == OmenChatInvitationKind::LegacyLaunch {
            return Ok(format!("omenchat://{destination}"));
        }

        let mut uri = format!("omenchat://{destination}?invite=1");
        if let Some(room_id) = self.room_id {
            uri.push_str("&room=");
            uri.push_str(&room_id.to_string());
        }
        if let Some(label) = &self.display_label {
            validate_label(label)?;
            uri.push_str("&label=");
            percent_encode_label(&mut uri, label);
        }
        if let Some(identity_hash) = &self.claimed_identity_hash {
            let identity_hash =
                normalized_hex(identity_hash, OMENCHAT_INVITATION_IDENTITY_HEX_BYTES)
                    .ok_or(OmenChatInvitationError::InvalidIdentity)?;
            uri.push_str("&identity=");
            uri.push_str(&identity_hash);
        }
        if uri.len() > OMENCHAT_INVITATION_MAX_BYTES {
            return Err(OmenChatInvitationError::TooLarge);
        }
        Ok(uri)
    }
}

fn normalized_hex(value: &str, exact_bytes: usize) -> Option<String> {
    (value.len() == exact_bytes && value.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .then(|| value.to_ascii_lowercase())
}

fn validate_label(label: &str) -> Result<(), OmenChatInvitationError> {
    if label.is_empty()
        || label.len() > CHAT_SERVER_DISPLAY_MAX_BYTES
        || label.chars().any(char::is_control)
    {
        return Err(OmenChatInvitationError::InvalidLabel);
    }
    Ok(())
}

fn percent_encode_label(output: &mut String, label: &str) {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    for byte in label.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            output.push(char::from(byte));
        } else {
            output.push('%');
            output.push(char::from(HEX[usize::from(byte >> 4)]));
            output.push(char::from(HEX[usize::from(byte & 0x0f)]));
        }
    }
}

fn percent_decode_label(value: &str) -> Result<String, OmenChatInvitationError> {
    if value.is_empty() || value.contains('+') {
        return Err(OmenChatInvitationError::InvalidLabel);
    }
    let input = value.as_bytes();
    let mut decoded = Vec::with_capacity(input.len().min(CHAT_SERVER_DISPLAY_MAX_BYTES));
    let mut index = 0usize;
    while index < input.len() {
        if decoded.len() > CHAT_SERVER_DISPLAY_MAX_BYTES {
            return Err(OmenChatInvitationError::InvalidLabel);
        }
        match input[index] {
            b'%' => {
                let high = *input
                    .get(index + 1)
                    .ok_or(OmenChatInvitationError::InvalidLabel)?;
                let low = *input
                    .get(index + 2)
                    .ok_or(OmenChatInvitationError::InvalidLabel)?;
                decoded.push(
                    decode_hex(high)
                        .and_then(|high| decode_hex(low).map(|low| high << 4 | low))
                        .ok_or(OmenChatInvitationError::InvalidLabel)?,
                );
                index += 3;
            }
            byte => {
                decoded.push(byte);
                index += 1;
            }
        }
    }
    if decoded.len() > CHAT_SERVER_DISPLAY_MAX_BYTES {
        return Err(OmenChatInvitationError::InvalidLabel);
    }
    let label = String::from_utf8(decoded).map_err(|_| OmenChatInvitationError::InvalidLabel)?;
    validate_label(&label)?;
    Ok(label)
}

fn decode_hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn assess_identity_evidence(
    invitation: &OmenChatInvitation,
    directory: &[DirectoryEntry],
) -> OmenChatInvitationIdentityEvidence {
    let Some(claimed) = invitation.claimed_identity_hash.as_deref() else {
        return OmenChatInvitationIdentityEvidence::NotProvided;
    };
    let mut matching_identity_seen = false;
    let mut matching_identity_trusted = false;
    for entry in directory.iter().filter(|entry| {
        entry.kind == DirectoryKind::OmenChat
            && entry
                .destination_hash
                .eq_ignore_ascii_case(&invitation.server_destination)
    }) {
        let Some(known) = entry.identity_hash.as_deref() else {
            continue;
        };
        if !known.eq_ignore_ascii_case(claimed) {
            return OmenChatInvitationIdentityEvidence::Conflict;
        }
        matching_identity_seen = true;
        matching_identity_trusted |= entry.trusted;
    }
    if matching_identity_seen {
        OmenChatInvitationIdentityEvidence::VerifiedMatch {
            trusted: matching_identity_trusted,
        }
    } else {
        OmenChatInvitationIdentityEvidence::Unverified
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DESTINATION: &str = "00112233445566778899aabbccddeeff";
    const IDENTITY: &str = "ffeeddccbbaa99887766554433221100";

    #[test]
    fn plain_launch_uri_remains_compatible() {
        let invite =
            OmenChatInvitation::parse(&format!("omenchat://{DESTINATION}")).expect("plain launch");
        assert_eq!(invite.kind, OmenChatInvitationKind::LegacyLaunch);
        assert_eq!(
            invite.canonical_uri().expect("canonical"),
            format!("omenchat://{DESTINATION}")
        );
    }

    #[test]
    fn enhanced_invitation_round_trips_in_fixed_order() {
        let mut invite = OmenChatInvitation::new(DESTINATION.to_uppercase()).expect("invite");
        invite.room_id = Some(u32::MAX);
        invite.display_label = Some("Field Ops / ☃".into());
        invite.claimed_identity_hash = Some(IDENTITY.into());
        let uri = invite.canonical_uri().expect("canonical");
        assert_eq!(
            uri,
            format!(
                "omenchat://{DESTINATION}?invite=1&room=4294967295&label=Field%20Ops%20%2F%20%E2%98%83&identity={IDENTITY}"
            )
        );
        assert_eq!(OmenChatInvitation::parse(&uri), Ok(invite));
    }

    #[test]
    fn complete_input_is_bounded_before_parsing() {
        assert_eq!(
            OmenChatInvitation::parse(&format!(
                "omenchat://{DESTINATION}?invite=1&label={}",
                "x".repeat(OMENCHAT_INVITATION_MAX_BYTES)
            )),
            Err(OmenChatInvitationError::TooLarge)
        );
    }

    #[test]
    fn destination_and_identity_are_exact_lowercase_hex() {
        let normalized = OmenChatInvitation::parse(&format!(
            "omenchat://{}?invite=1&identity={}",
            DESTINATION.to_uppercase(),
            IDENTITY.to_uppercase()
        ))
        .expect("uppercase hexadecimal invite");
        assert_eq!(normalized.server_destination, DESTINATION);
        assert_eq!(normalized.claimed_identity_hash.as_deref(), Some(IDENTITY));
        for invalid in [
            "11",
            "00112233445566778899aabbccddeef",
            "00112233445566778899aabbccddeeff00",
            "00112233445566778899aabbccddeefg",
        ] {
            assert_eq!(
                OmenChatInvitation::parse(&format!("omenchat://{invalid}")),
                Err(OmenChatInvitationError::InvalidDestination)
            );
        }
        assert_eq!(
            OmenChatInvitation::parse(&format!("omenchat://{DESTINATION}?invite=1&identity=11")),
            Err(OmenChatInvitationError::InvalidIdentity)
        );
    }

    #[test]
    fn query_requires_version_and_rejects_unknown_duplicate_or_trailing_fields() {
        for query in [
            "room=1",
            "invite=2",
            "invite=1&invite=1",
            "invite=1&token=secret",
            "invite=1&",
            "invite=1#fragment",
        ] {
            assert!(
                OmenChatInvitation::parse(&format!("omenchat://{DESTINATION}?{query}")).is_err()
            );
        }
    }

    #[test]
    fn room_accepts_u32_boundaries_and_rejects_overflow_or_non_decimal() {
        for room in ["0", "4294967295"] {
            assert!(OmenChatInvitation::parse(&format!(
                "omenchat://{DESTINATION}?invite=1&room={room}"
            ))
            .is_ok());
        }
        for room in ["", "-1", "+1", "4294967296", "1.0"] {
            assert_eq!(
                OmenChatInvitation::parse(&format!(
                    "omenchat://{DESTINATION}?invite=1&room={room}"
                )),
                Err(OmenChatInvitationError::InvalidRoom)
            );
        }
    }

    #[test]
    fn label_decode_rejects_malformed_utf8_controls_plus_and_next_byte() {
        for label in ["", "%", "%0", "%GG", "%FF", "%00", "a+b"] {
            assert_eq!(
                OmenChatInvitation::parse(&format!(
                    "omenchat://{DESTINATION}?invite=1&label={label}"
                )),
                Err(OmenChatInvitationError::InvalidLabel)
            );
        }
        let exact = "x".repeat(CHAT_SERVER_DISPLAY_MAX_BYTES);
        assert!(OmenChatInvitation::parse(&format!(
            "omenchat://{DESTINATION}?invite=1&label={exact}"
        ))
        .is_ok());
        let next = "x".repeat(CHAT_SERVER_DISPLAY_MAX_BYTES + 1);
        assert_eq!(
            OmenChatInvitation::parse(&format!("omenchat://{DESTINATION}?invite=1&label={next}")),
            Err(OmenChatInvitationError::InvalidLabel)
        );
    }

    #[test]
    fn authority_tricks_and_noncanonical_schemes_are_rejected() {
        for uri in [
            format!("OMENCHAT://{DESTINATION}"),
            format!("omenchat://user@{DESTINATION}"),
            format!("omenchat://{DESTINATION}:80"),
            format!("omenchat://{DESTINATION}/room"),
            format!("omenchat:{DESTINATION}"),
        ] {
            assert!(OmenChatInvitation::parse(&uri).is_err());
        }
    }

    #[test]
    fn serializer_cannot_emit_secret_or_unsupported_fields() {
        let invite = OmenChatInvitation::new(DESTINATION).expect("invite");
        let uri = invite.canonical_uri().expect("canonical");
        for forbidden in [
            "token",
            "password",
            "role",
            "ifac",
            "ticket",
            "identity_key",
        ] {
            assert!(!uri.contains(forbidden));
        }
    }

    fn directory_entry(identity: Option<&str>, trusted: bool) -> DirectoryEntry {
        let mut entry = DirectoryEntry::new(DESTINATION, "Server", DirectoryKind::OmenChat);
        entry.identity_hash = identity.map(str::to_owned);
        entry.trusted = trusted;
        entry
    }

    fn invitation_with_identity(identity: Option<&str>) -> OmenChatInvitation {
        let mut invitation = OmenChatInvitation::new(DESTINATION).expect("invitation");
        invitation.claimed_identity_hash = identity.map(str::to_owned);
        invitation
    }

    #[test]
    fn preview_distinguishes_absent_unverified_verified_and_conflicting_identity() {
        assert_eq!(
            OmenChatInvitationPreview::new(invitation_with_identity(None), &[]).identity_evidence,
            OmenChatInvitationIdentityEvidence::NotProvided
        );
        assert_eq!(
            OmenChatInvitationPreview::new(invitation_with_identity(Some(IDENTITY)), &[])
                .identity_evidence,
            OmenChatInvitationIdentityEvidence::Unverified
        );
        assert_eq!(
            OmenChatInvitationPreview::new(
                invitation_with_identity(Some(IDENTITY)),
                &[directory_entry(Some(IDENTITY), true)]
            )
            .identity_evidence,
            OmenChatInvitationIdentityEvidence::VerifiedMatch { trusted: true }
        );
        let conflict = OmenChatInvitationPreview::new(
            invitation_with_identity(Some(IDENTITY)),
            &[directory_entry(
                Some("11111111111111111111111111111111"),
                true,
            )],
        );
        assert_eq!(
            conflict.identity_evidence,
            OmenChatInvitationIdentityEvidence::Conflict
        );
        assert!(!conflict.allows_confirmation());
    }

    #[test]
    fn conflicting_duplicate_directory_evidence_wins_over_a_match() {
        let preview = OmenChatInvitationPreview::new(
            invitation_with_identity(Some(IDENTITY)),
            &[
                directory_entry(Some(IDENTITY), true),
                directory_entry(Some("11111111111111111111111111111111"), false),
            ],
        );
        assert_eq!(
            preview.identity_evidence,
            OmenChatInvitationIdentityEvidence::Conflict
        );
    }

    #[test]
    fn preview_owner_retains_one_item_replaces_only_after_valid_parse_and_cancels() {
        let mut owner = OmenChatInvitationPreviewOwner::default();
        let first = format!("omenchat://{DESTINATION}?invite=1&room=1");
        owner.replace_from_uri(&first, &[]).expect("first preview");
        assert_eq!(
            owner
                .pending()
                .and_then(|preview| preview.invitation.room_id),
            Some(1)
        );

        assert!(owner.replace_from_uri("hostile input", &[]).is_err());
        assert_eq!(
            owner
                .pending()
                .and_then(|preview| preview.invitation.room_id),
            Some(1),
            "invalid input must not discard the current explicit preview"
        );

        let second = format!("omenchat://{DESTINATION}?invite=1&room=2");
        owner.replace_from_uri(&second, &[]).expect("replacement");
        assert_eq!(
            owner
                .take_confirmable()
                .and_then(|invitation| invitation.room_id),
            Some(2)
        );
        assert!(owner.pending().is_none());
        assert!(!owner.cancel());
    }

    #[test]
    fn conflict_cannot_be_taken_for_confirmation_or_discarded_implicitly() {
        let mut owner = OmenChatInvitationPreviewOwner::default();
        let uri = format!("omenchat://{DESTINATION}?invite=1&identity={IDENTITY}");
        owner
            .replace_from_uri(
                &uri,
                &[directory_entry(
                    Some("11111111111111111111111111111111"),
                    false,
                )],
            )
            .expect("conflicting preview");
        assert!(owner.take_confirmable().is_none());
        assert!(owner.pending().is_some());
        assert!(owner.cancel());
        assert!(owner.pending().is_none());
    }
}
