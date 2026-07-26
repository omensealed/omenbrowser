use iced::Task;

use crate::chat::{ChatSessionId, OmenChatInvitation, OmenChatInvitationError};
use crate::directory::DirectoryKind;
use crate::micron::LinkAction;

#[cfg(feature = "desktop-qr")]
use super::omenchat_desktop_state::OmenChatInvitationQr;
use super::{DesktopApp, Message};

impl DesktopApp {
    pub(in crate::desktop) fn preview_omenchat_invitation(
        &mut self,
        value: &str,
    ) -> Result<(), OmenChatInvitationError> {
        self.omenchat.omenchat_invitation_room = None;
        self.omenchat
            .omenchat_invitation_preview
            .replace_from_uri(value, &self.app.directory_state.entries)
    }

    pub(in crate::desktop) fn preview_or_open_omenchat_link(
        &mut self,
        link: LinkAction,
    ) -> Option<Task<Message>> {
        if link.target.starts_with("omenchat://") && link.target.contains('?') {
            match self.preview_omenchat_invitation(&link.target) {
                Ok(()) => {
                    self.app.status.task =
                        "review the OMENchat invitation; no connection has been opened".into();
                }
                Err(error) => {
                    self.app.status.task = format!("invalid OMENchat invitation: {error}");
                }
            }
            return Some(Task::none());
        }
        self.open_omenchat_link(link)
    }

    pub(in crate::desktop) fn omenchat_invitation_uri(
        &self,
        session_id: ChatSessionId,
    ) -> Option<String> {
        let session = self.omenchat.chat_client.session(session_id)?;
        let mut invitation = OmenChatInvitation::new(&session.server.destination).ok()?;
        if session.active_room.joined {
            invitation.room_id = Some(session.active_room.room_id);
        }
        let display_name = session.server.display_name.trim();
        if !display_name.is_empty() {
            invitation.display_label = Some(display_name.to_owned());
        }
        invitation.claimed_identity_hash =
            self.unambiguous_omenchat_directory_identity(&session.server.destination);
        invitation.canonical_uri().ok()
    }

    pub(in crate::desktop) fn update_copy_omenchat_invitation(
        &mut self,
        session_id: ChatSessionId,
    ) -> Task<Message> {
        let Some(uri) = self.omenchat_invitation_uri_for_copy(session_id) else {
            self.app.status.task =
                "could not create a bounded OMENchat invitation for this session".into();
            return Task::none();
        };
        self.app.status.task = "copied OMENchat invitation to clipboard".into();
        iced::clipboard::write(uri)
    }

    fn omenchat_invitation_uri_for_copy(&self, session_id: ChatSessionId) -> Option<String> {
        #[cfg(feature = "desktop-qr")]
        if let Some(qr) = self
            .omenchat
            .omenchat_invitation_qr
            .as_ref()
            .filter(|qr| qr.session_id == session_id)
        {
            return Some(qr.uri.clone());
        }
        self.omenchat_invitation_uri(session_id)
    }

    #[cfg(feature = "desktop-qr")]
    pub(in crate::desktop) fn update_toggle_omenchat_invitation_qr(
        &mut self,
        session_id: ChatSessionId,
    ) {
        if self
            .omenchat
            .omenchat_invitation_qr
            .as_ref()
            .is_some_and(|qr| qr.session_id == session_id)
        {
            self.omenchat.omenchat_invitation_qr = None;
            self.app.status.task = "closed OMENchat invitation QR".into();
            return;
        }
        let Some(uri) = self.omenchat_invitation_uri(session_id) else {
            self.app.status.task =
                "could not create a bounded OMENchat invitation QR for this session".into();
            return;
        };
        let Ok(data) = iced::widget::qr_code::Data::new(uri.as_bytes()) else {
            self.app.status.task = "OMENchat invitation exceeds the supported QR capacity".into();
            return;
        };
        self.omenchat.omenchat_invitation_qr = Some(OmenChatInvitationQr {
            session_id,
            uri,
            data,
        });
        self.app.status.task = "showing OMENchat invitation QR".into();
    }

    #[cfg(feature = "desktop-qr")]
    pub(in crate::desktop) fn clear_omenchat_invitation_qr_for_session(
        &mut self,
        session_id: ChatSessionId,
    ) {
        if self
            .omenchat
            .omenchat_invitation_qr
            .as_ref()
            .is_some_and(|qr| qr.session_id == session_id)
        {
            self.omenchat.omenchat_invitation_qr = None;
        }
    }

    fn unambiguous_omenchat_directory_identity(&self, server_destination: &str) -> Option<String> {
        let mut selected: Option<String> = None;
        for entry in self.app.directory_state.entries.iter().filter(|entry| {
            entry.kind == DirectoryKind::OmenChat
                && entry
                    .destination_hash
                    .eq_ignore_ascii_case(server_destination)
        }) {
            let Some(identity) = entry.identity_hash.as_deref() else {
                continue;
            };
            if identity.len() != crate::directory::DIRECTORY_IDENTITY_HASH_BYTES
                || !identity.bytes().all(|byte| byte.is_ascii_hexdigit())
            {
                return None;
            }
            let identity = identity.to_ascii_lowercase();
            if selected
                .as_ref()
                .is_some_and(|known| !known.eq_ignore_ascii_case(&identity))
            {
                return None;
            }
            selected = Some(identity);
        }
        selected
    }
}

#[cfg(all(test, feature = "chat-client"))]
mod tests {
    use super::*;
    use crate::app::App;
    use crate::chat::OmenChatDescriptor;
    use crate::directory::DirectoryEntry;
    use crate::micron::render::HitAction;

    const DESTINATION: &str = "00112233445566778899aabbccddeeff";
    const IDENTITY: &str = "ffeeddccbbaa99887766554433221100";

    fn desktop_with_session(name: &str) -> (DesktopApp, ChatSessionId) {
        let root = std::env::temp_dir().join(format!("{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let paths = crate::config::AppPaths::from_root(root);
        paths.ensure().expect("paths");
        let mut desktop = DesktopApp::new(App::new(crate::config::AppConfig {
            paths,
            settings: crate::storage::settings::AppSettings::default(),
        }));
        let session_id = desktop.open_omenchat_status_session(
            OmenChatDescriptor {
                server_destination: DESTINATION.into(),
                display_name: Some("Field Ops / East".into()),
                rooms_hint: vec!["lobby".into()],
                ..OmenChatDescriptor::default()
            },
            "connected".into(),
        );
        let session = desktop
            .omenchat
            .chat_client
            .session_mut(session_id)
            .expect("session");
        session.active_room.room_id = 7;
        session.active_room.joined = true;
        (desktop, session_id)
    }

    #[test]
    fn generated_invitation_uses_joined_room_label_and_unambiguous_identity() {
        let (mut desktop, session_id) =
            desktop_with_session("omenbrowser-rs-copy-omenchat-invitation");
        let mut entry = DirectoryEntry::new(DESTINATION, "Field Ops", DirectoryKind::OmenChat);
        entry.identity_hash = Some(IDENTITY.to_ascii_uppercase());
        desktop.app.directory_state.entries.push(entry);

        assert_eq!(
            desktop
                .omenchat_invitation_uri(session_id)
                .expect("invitation"),
            format!(
                "omenchat://{DESTINATION}?invite=1&room=7&label=Field%20Ops%20%2F%20East&identity={IDENTITY}"
            )
        );
    }

    #[test]
    fn generated_invitation_omits_unjoined_room_and_ambiguous_identity() {
        let (mut desktop, session_id) =
            desktop_with_session("omenbrowser-rs-copy-omenchat-ambiguous-invitation");
        desktop
            .omenchat
            .chat_client
            .session_mut(session_id)
            .expect("session")
            .active_room
            .joined = false;
        for identity in [IDENTITY, "11111111111111111111111111111111"] {
            let mut entry = DirectoryEntry::new(DESTINATION, "Field Ops", DirectoryKind::OmenChat);
            entry.identity_hash = Some(identity.into());
            desktop.app.directory_state.entries.push(entry);
        }

        let uri = desktop
            .omenchat_invitation_uri(session_id)
            .expect("invitation");
        assert_eq!(
            uri,
            format!("omenchat://{DESTINATION}?invite=1&label=Field%20Ops%20%2F%20East")
        );
        assert!(!uri.contains("room="));
        assert!(!uri.contains("identity="));
    }

    #[test]
    fn copy_invitation_action_reports_success_or_missing_session_without_state_mutation() {
        let (mut desktop, session_id) =
            desktop_with_session("omenbrowser-rs-copy-omenchat-invitation-action");
        let sessions_before = desktop.omenchat.chat_client.sessions().to_vec();

        let _ = desktop.update_copy_omenchat_invitation(session_id);
        assert_eq!(
            desktop.app.status.task,
            "copied OMENchat invitation to clipboard"
        );
        assert_eq!(desktop.omenchat.chat_client.sessions(), sessions_before);

        let _ = desktop.update_copy_omenchat_invitation(u64::MAX);
        assert!(desktop.app.status.task.contains("could not create"));
        assert_eq!(desktop.omenchat.chat_client.sessions(), sessions_before);
    }

    #[cfg(feature = "desktop-qr")]
    #[test]
    fn qr_owner_uses_canonical_uri_toggles_and_rejects_missing_session() {
        let (mut desktop, session_id) =
            desktop_with_session("omenbrowser-rs-omenchat-invitation-qr");
        let expected = desktop
            .omenchat_invitation_uri(session_id)
            .expect("canonical invitation");

        desktop.update_toggle_omenchat_invitation_qr(session_id);
        let qr = desktop
            .omenchat
            .omenchat_invitation_qr
            .as_ref()
            .expect("QR owner");
        assert_eq!(qr.session_id, session_id);
        assert_eq!(qr.uri, expected);
        assert!(qr.uri.len() <= crate::chat::OMENCHAT_INVITATION_MAX_BYTES);
        desktop
            .omenchat
            .chat_client
            .session_mut(session_id)
            .expect("session")
            .server
            .display_name = "Changed after QR render".into();
        assert_eq!(
            desktop
                .omenchat_invitation_uri_for_copy(session_id)
                .expect("displayed QR copy"),
            expected,
            "clipboard text must remain byte-identical to the displayed QR"
        );

        desktop.update_toggle_omenchat_invitation_qr(session_id);
        assert!(desktop.omenchat.omenchat_invitation_qr.is_none());

        desktop.update_toggle_omenchat_invitation_qr(u64::MAX);
        assert!(desktop.omenchat.omenchat_invitation_qr.is_none());
        assert!(desktop.app.status.task.contains("could not create"));
    }

    #[cfg(feature = "desktop-qr")]
    #[test]
    fn qr_owner_is_single_item_and_clears_with_its_session() {
        let (mut desktop, first_session) =
            desktop_with_session("omenbrowser-rs-omenchat-invitation-qr-owner");
        let second_session = desktop.open_omenchat_status_session(
            OmenChatDescriptor {
                server_destination: "11111111111111111111111111111111".into(),
                display_name: Some("Second Server".into()),
                rooms_hint: vec!["lobby".into()],
                ..OmenChatDescriptor::default()
            },
            "connected".into(),
        );

        desktop.update_toggle_omenchat_invitation_qr(first_session);
        desktop.update_toggle_omenchat_invitation_qr(second_session);
        assert_eq!(
            desktop
                .omenchat
                .omenchat_invitation_qr
                .as_ref()
                .map(|qr| qr.session_id),
            Some(second_session)
        );

        desktop.clear_omenchat_invitation_qr_for_session(first_session);
        assert!(desktop.omenchat.omenchat_invitation_qr.is_some());
        desktop.close_omenchat_session(second_session);
        assert!(desktop.omenchat.omenchat_invitation_qr.is_none());
    }

    #[test]
    fn enhanced_micron_link_enters_confirmation_without_opening_or_trusting() {
        let (mut desktop, _) =
            desktop_with_session("omenbrowser-rs-enhanced-omenchat-micron-invitation");
        desktop.omenchat.chat_client = crate::chat::ChatClient::new();
        let sessions_before = desktop.omenchat.chat_client.sessions().len();
        let directory_before = desktop.app.directory_state.entries.clone();
        let action = HitAction::Link(LinkAction {
            target: format!(
                "omenchat://{DESTINATION}?invite=1&room=7&label=Field%20Ops&identity={IDENTITY}"
            ),
            fields: vec!["ignored-form-field=not-an-invitation-field".into()],
        });

        assert!(desktop
            .activate_omenchat_hit_action_if_needed(&action)
            .is_some());

        let preview = desktop
            .omenchat
            .omenchat_invitation_preview
            .pending()
            .expect("preview");
        assert_eq!(preview.invitation.room_id, Some(7));
        assert_eq!(
            desktop.omenchat.chat_client.sessions().len(),
            sessions_before
        );
        assert_eq!(desktop.app.directory_state.entries, directory_before);
        assert!(desktop.app.status.task.contains("no connection"));
    }

    #[test]
    fn malformed_enhanced_micron_link_is_handled_without_opening() {
        let (mut desktop, _) =
            desktop_with_session("omenbrowser-rs-invalid-omenchat-micron-invitation");
        desktop.omenchat.chat_client = crate::chat::ChatClient::new();
        let action = HitAction::Link(LinkAction {
            target: format!("omenchat://{DESTINATION}?invite=1&secret=credential"),
            fields: Vec::new(),
        });

        assert!(desktop
            .activate_omenchat_hit_action_if_needed(&action)
            .is_some());
        assert!(desktop
            .omenchat
            .omenchat_invitation_preview
            .pending()
            .is_none());
        assert!(desktop.omenchat.chat_client.sessions().is_empty());
        assert!(desktop.app.status.task.contains("invalid"));
    }

    #[cfg(feature = "mock-runtime")]
    #[test]
    fn plain_micron_omenchat_link_retains_existing_open_behavior() {
        let (mut desktop, _) = desktop_with_session("omenbrowser-rs-plain-omenchat-micron-link");
        desktop.omenchat.chat_client = crate::chat::ChatClient::new();
        desktop.app.runtime_status.connected = false;
        let action = HitAction::Link(LinkAction {
            target: format!("omenchat://{DESTINATION}"),
            fields: Vec::new(),
        });

        assert!(desktop
            .activate_omenchat_hit_action_if_needed(&action)
            .is_some());
        assert!(desktop
            .omenchat
            .chat_client
            .sessions()
            .iter()
            .any(|session| session.server.destination == DESTINATION));
        assert!(desktop
            .omenchat
            .omenchat_invitation_preview
            .pending()
            .is_none());
    }
}
