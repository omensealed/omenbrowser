use iced::Task;

use crate::chat::{ChatSessionId, OmenChatInvitation};
use crate::directory::DirectoryKind;

use super::{DesktopApp, Message};

impl DesktopApp {
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
        let Some(uri) = self.omenchat_invitation_uri(session_id) else {
            self.app.status.task =
                "could not create a bounded OMENchat invitation for this session".into();
            return Task::none();
        };
        self.app.status.task = "copied OMENchat invitation to clipboard".into();
        iced::clipboard::write(uri)
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
}
