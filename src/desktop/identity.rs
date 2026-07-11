use iced::Task;
use std::path::PathBuf;

use super::{DesktopApp, Message};

impl DesktopApp {
    pub(super) fn dispatch_identity_message(
        &mut self,
        message: Message,
    ) -> Result<Task<Message>, Message> {
        match message {
            Message::CreateIdentity => {
                self.update_create_identity();
                Ok(Task::none())
            }
            Message::ActivateManagedIdentity(path) => {
                self.update_activate_managed_identity(path);
                Ok(Task::none())
            }
            Message::ActiveIdentityLabelChanged(label) => {
                self.update_active_identity_label_changed(label);
                Ok(Task::none())
            }
            Message::DeleteActiveIdentity => {
                self.update_delete_active_identity();
                Ok(Task::none())
            }
            Message::ConfirmDeleteActiveIdentity => {
                self.update_confirm_delete_active_identity();
                Ok(Task::none())
            }
            Message::CancelDeleteActiveIdentity => {
                self.update_cancel_delete_active_identity();
                Ok(Task::none())
            }
            Message::ClearActiveIdentity => {
                self.update_clear_active_identity();
                Ok(Task::none())
            }
            Message::AnnounceIdentityNow => {
                self.update_announce_identity_now();
                Ok(Task::none())
            }
            Message::CopyActiveIdentityHash => Ok(self.update_copy_active_identity_hash()),
            _ => Err(message),
        }
    }

    pub(super) fn update_create_identity(&mut self) {
        self.app.create_settings_managed_identity();
    }

    pub(super) fn update_activate_managed_identity(&mut self, path: String) {
        self.app.activate_managed_identity_path(PathBuf::from(path));
    }

    pub(super) fn update_active_identity_label_changed(&mut self, label: String) {
        self.app.set_active_identity_label(label);
    }

    pub(super) fn update_delete_active_identity(&mut self) {
        self.ui.identity_delete_confirming = true;
        self.app.status.task = "confirm identity deletion before removing active identity".into();
    }

    pub(super) fn update_confirm_delete_active_identity(&mut self) {
        self.app.delete_active_identity_with_backup();
        self.ui.identity_delete_confirming = false;
    }

    pub(super) fn update_cancel_delete_active_identity(&mut self) {
        self.ui.identity_delete_confirming = false;
        self.app.status.task = "identity deletion cancelled".into();
    }

    pub(super) fn update_clear_active_identity(&mut self) {
        self.app.clear_active_identity();
        self.ui.identity_delete_confirming = false;
    }

    pub(super) fn update_announce_identity_now(&mut self) {
        self.app.announce_local_lxmf_now();
    }

    pub(super) fn update_copy_active_identity_hash(&mut self) -> Task<Message> {
        if let Some(identity) = &self.app.runtime_status.active_identity {
            self.app.status.task = "copied active identity hash to clipboard".into();
            return iced::clipboard::write(identity.hash_hex.clone());
        }
        self.app.status.task = "no active identity hash to copy".into();
        Task::none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::App;
    use crate::identity::IdentityProfile;

    #[test]
    fn identity_hash_copy_action_reports_status() {
        let root = std::env::temp_dir().join(format!(
            "omenbrowser-rs-desktop-copy-identity-hash-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let paths = crate::config::AppPaths::from_root(root);
        paths.ensure().expect("paths");
        let app = App::new(crate::config::AppConfig {
            paths,
            settings: crate::storage::settings::AppSettings::default(),
        });
        let mut desktop = DesktopApp::new(app);

        let _ = desktop.update(Message::CopyActiveIdentityHash);
        assert_eq!(desktop.app.status.task, "no active identity hash to copy");

        desktop.app.runtime_status.active_identity = Some(IdentityProfile {
            label: "tester".into(),
            path: desktop.app.paths.root.join("identity"),
            hash_hex: "0123456789abcdef0123456789abcdef".into(),
            managed: true,
        });
        let _ = desktop.update(Message::CopyActiveIdentityHash);
        assert_eq!(
            desktop.app.status.task,
            "copied active identity hash to clipboard"
        );
    }
}
