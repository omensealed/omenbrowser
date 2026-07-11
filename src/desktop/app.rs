use std::borrow::Cow;
use std::sync::Mutex;

use iced::{Pixels, Settings};

use crate::app::App;

use super::{
    desktop_ui_font, omen_application_style, set_desktop_font_size, DesktopApp,
    MICRON_VIEWPORT_FONT_BYTES,
};

pub fn run(app: App) -> iced::Result {
    let mut app = app;
    let default_text_size = app.settings.ui.font_size.clamp(10, 24) as f32;
    set_desktop_font_size(app.settings.ui.font_size);
    app.reset_runtime_log_display_for_session();
    app.bootstrap_runtime_on_launch();
    let mut desktop = DesktopApp::new(app);
    let startup_scroll = desktop.anchor_visible_workspace_scrolls_to_bottom_now(2);
    let boot_state = Mutex::new(Some((desktop, startup_scroll)));
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
