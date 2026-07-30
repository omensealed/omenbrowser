use std::borrow::Cow;
use std::sync::Mutex;

use iced::{Pixels, Settings};

use crate::app::App;

#[cfg(any(
    feature = "omenchat-slow-mode-qualification",
    feature = "omenchat-room-media-policy-qualification"
))]
use super::Message;
use super::{
    desktop_ui_font, omen_application_style, set_desktop_font_size, DesktopApp,
    MICRON_VIEWPORT_FONT_BYTES,
};

#[cfg(any(
    feature = "omenchat-slow-mode-qualification",
    feature = "omenchat-room-media-policy-qualification"
))]
const QUALIFICATION_OMENCHAT_TARGET_ENV: &str = "OMENBROWSER_QUALIFICATION_OMENCHAT_TARGET";

pub fn run(app: App) -> iced::Result {
    let mut app = app;
    let default_text_size = app.settings.ui.font_size.clamp(10, 24) as f32;
    set_desktop_font_size(app.settings.ui.font_size);
    app.reset_runtime_log_display_for_session();
    app.bootstrap_runtime_on_launch();
    let mut desktop = DesktopApp::new(app);
    let startup_scroll = desktop.anchor_visible_workspace_scrolls_to_bottom_now(2);
    #[cfg(any(
        feature = "omenchat-slow-mode-qualification",
        feature = "omenchat-room-media-policy-qualification"
    ))]
    desktop
        .set_qualification_omenchat_target(std::env::var(QUALIFICATION_OMENCHAT_TARGET_ENV).ok());
    let startup_task = startup_scroll;
    let boot_state = Mutex::new(Some((desktop, startup_task)));
    iced::application(
        move || {
            boot_state
                .lock()
                .expect("desktop boot state lock poisoned")
                .take()
                .expect("desktop application booted more than once")
        },
        DesktopApp::update,
        DesktopApp::view,
    )
    .title("OMENbrowser_rs")
    .settings(Settings {
        default_font: desktop_ui_font(),
        default_text_size: Pixels(default_text_size),
        fonts: vec![Cow::Borrowed(MICRON_VIEWPORT_FONT_BYTES)],
        ..Settings::default()
    })
    .theme(DesktopApp::theme)
    .style(|_, theme| omen_application_style(theme))
    .subscription(DesktopApp::subscription)
    .exit_on_close_request(false)
    .run()
}

#[cfg(any(
    feature = "omenchat-slow-mode-qualification",
    feature = "omenchat-room-media-policy-qualification"
))]
impl DesktopApp {
    fn set_qualification_omenchat_target(&mut self, target: Option<String>) {
        self.qualification_omenchat_target = target;
    }

    pub(super) fn open_qualification_omenchat_if_runtime_ready(&mut self) -> iced::Task<Message> {
        if !self.app.runtime_status.connected {
            return iced::Task::none();
        }
        let Some(target) = self.qualification_omenchat_target.take() else {
            return iced::Task::none();
        };
        self.update_omenchat_server_entry_changed(target);
        self.update_open_omenchat_server_entry()
    }
}

#[cfg(all(
    test,
    any(
        feature = "omenchat-slow-mode-qualification",
        feature = "omenchat-room-media-policy-qualification"
    )
))]
mod tests {
    use super::*;

    #[test]
    fn qualification_target_waits_for_runtime_then_uses_normal_open_path() {
        let root = std::env::temp_dir().join(format!(
            "omenbrowser-qualification-gui-open-{}",
            std::process::id()
        ));
        let app = App::new(crate::config::AppConfig {
            paths: crate::config::AppPaths::from_root(root),
            settings: crate::storage::settings::AppSettings::default(),
        });
        let mut desktop = DesktopApp::new(app);
        desktop.set_qualification_omenchat_target(Some(
            "omenchat://0123456789abcdef0123456789abcdef".into(),
        ));

        let _task = desktop.open_qualification_omenchat_if_runtime_ready();
        assert!(desktop.qualification_omenchat_target.is_some());
        assert!(desktop.omenchat.omenchat_server_entry.is_empty());

        desktop.app.runtime_status.connected = true;
        let _task = desktop.open_qualification_omenchat_if_runtime_ready();
        assert!(desktop.qualification_omenchat_target.is_none());
        assert!(desktop.omenchat.omenchat_server_entry.is_empty());
        assert!(desktop.omenchat.chat_client.sessions().is_empty());
        assert!(desktop
            .app
            .status
            .task
            .contains("opening live OMENchat link"));
    }
}
