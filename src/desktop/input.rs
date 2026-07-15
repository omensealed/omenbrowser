use iced::{event, keyboard, window};

use super::{
    BrowserFieldKey, BrowserMessage, DesktopApp, IdentityMessage, Message, RuntimeMessage,
    ShellMessage,
};

impl DesktopApp {
    pub(super) fn apply_browser_field_key(&mut self, key: BrowserFieldKey) {
        match key {
            BrowserFieldKey::Insert(text) => {
                self.app.edit_address_text(&text);
            }
            BrowserFieldKey::Backspace => self.app.address_backspace(),
            BrowserFieldKey::Delete => self.app.input_delete(),
            BrowserFieldKey::MoveLeft => {
                self.app.input_move_left();
            }
            BrowserFieldKey::MoveRight => {
                self.app.input_move_right();
            }
            BrowserFieldKey::MoveHome => {
                self.app.input_move_home();
            }
            BrowserFieldKey::MoveEnd => {
                self.app.input_move_end();
            }
        }
    }
}

pub(super) fn map_keyboard_modifier_event(
    event: iced::Event,
    _status: event::Status,
    _window: window::Id,
) -> Option<Message> {
    match event {
        iced::Event::Keyboard(keyboard::Event::ModifiersChanged(modifiers)) => Some(
            Message::Shell(ShellMessage::KeyboardModifiersChanged(modifiers)),
        ),
        iced::Event::Keyboard(keyboard::Event::KeyPressed { modifiers, .. })
        | iced::Event::Keyboard(keyboard::Event::KeyReleased { modifiers, .. }) => Some(
            Message::Shell(ShellMessage::KeyboardModifiersChanged(modifiers)),
        ),
        _ => None,
    }
}

pub(super) fn map_browser_field_keyboard_event(
    event: iced::Event,
    _status: event::Status,
    _window: window::Id,
) -> Option<Message> {
    match event {
        iced::Event::Keyboard(keyboard::Event::ModifiersChanged(modifiers)) => Some(
            Message::Shell(ShellMessage::KeyboardModifiersChanged(modifiers)),
        ),
        iced::Event::Keyboard(keyboard::Event::KeyPressed {
            key,
            modifiers,
            text,
            ..
        }) => map_browser_field_key_event_press(key, modifiers, text.as_deref()),
        iced::Event::Keyboard(keyboard::Event::KeyReleased { modifiers, .. }) => Some(
            Message::Shell(ShellMessage::KeyboardModifiersChanged(modifiers)),
        ),
        _ => None,
    }
}

pub(super) fn map_browser_field_key_event_press(
    key: keyboard::Key,
    modifiers: keyboard::Modifiers,
    text: Option<&str>,
) -> Option<Message> {
    if modifiers.command() || modifiers.alt() {
        return map_key_press(key, modifiers);
    }
    if let Some(text) =
        text.filter(|text| !text.is_empty() && text.chars().all(|ch| !ch.is_control()))
    {
        return Some(Message::Browser(BrowserMessage::FieldKey(
            BrowserFieldKey::Insert(text.to_string()),
        )));
    }
    map_browser_field_key_press(key, modifiers)
}

pub(super) fn map_key_press(key: keyboard::Key, modifiers: keyboard::Modifiers) -> Option<Message> {
    use keyboard::key::Named;
    use keyboard::Key;

    match key.as_ref() {
        Key::Named(Named::PageDown) => Some(Message::Browser(BrowserMessage::ScrollPage {
            direction: 1,
        })),
        Key::Named(Named::PageUp) => Some(Message::Browser(BrowserMessage::ScrollPage {
            direction: -1,
        })),
        Key::Named(Named::F9) => Some(Message::Shell(ShellMessage::ToggleNavigation)),
        Key::Named(Named::Tab) => Some(Message::Browser(BrowserMessage::FocusItem {
            reverse: modifiers.shift(),
        })),
        Key::Named(Named::Enter) | Key::Named(Named::Space) => {
            Some(Message::Browser(BrowserMessage::ActivateFocusedItem))
        }
        Key::Named(Named::ArrowLeft) if modifiers.alt() => {
            Some(Message::Browser(BrowserMessage::Back))
        }
        Key::Named(Named::ArrowRight) if modifiers.alt() => {
            Some(Message::Browser(BrowserMessage::Forward))
        }
        Key::Character("b") if modifiers.command() => {
            Some(Message::Shell(ShellMessage::ToggleNavigation))
        }
        Key::Character("+") | Key::Character("=") if modifiers.command() => {
            Some(Message::Browser(BrowserMessage::Zoom { direction: 1 }))
        }
        Key::Character("-") | Key::Character("_") if modifiers.command() => {
            Some(Message::Browser(BrowserMessage::Zoom { direction: -1 }))
        }
        Key::Character("t") if modifiers.command() => {
            Some(Message::Browser(BrowserMessage::NewTab))
        }
        Key::Character("w") if modifiers.command() => {
            Some(Message::Browser(BrowserMessage::CloseTab))
        }
        Key::Character("r") if modifiers.command() => {
            Some(Message::Browser(BrowserMessage::Reload))
        }
        Key::Character("l") if modifiers.command() => {
            Some(Message::Browser(BrowserMessage::OpenAddress))
        }
        Key::Character("d") if modifiers.command() => {
            Some(Message::Browser(BrowserMessage::WarmPath))
        }
        Key::Character("x") if modifiers.command() => {
            Some(Message::Browser(BrowserMessage::LiveProbe))
        }
        Key::Character("p") if modifiers.command() => {
            Some(Message::Browser(BrowserMessage::PathDiagnostics))
        }
        Key::Character("i") if modifiers.command() => {
            Some(Message::Identity(IdentityMessage::Create))
        }
        Key::Character("g") if modifiers.command() => {
            Some(Message::Runtime(RuntimeMessage::NativeQuickstart))
        }
        _ => None,
    }
}

pub(super) fn map_browser_field_key_press(
    key: keyboard::Key,
    modifiers: keyboard::Modifiers,
) -> Option<Message> {
    use keyboard::key::Named;
    use keyboard::Key;

    if modifiers.command() || modifiers.alt() {
        return map_key_press(key, modifiers);
    }

    match key.as_ref() {
        Key::Character(text) => {
            let text = shifted_browser_field_text(text, modifiers);
            if text.chars().all(|ch| !ch.is_control()) {
                Some(Message::Browser(BrowserMessage::FieldKey(
                    BrowserFieldKey::Insert(text),
                )))
            } else {
                None
            }
        }
        Key::Named(Named::Space) => Some(Message::Browser(BrowserMessage::FieldKey(
            BrowserFieldKey::Insert(" ".into()),
        ))),
        Key::Named(Named::Backspace) => Some(Message::Browser(BrowserMessage::FieldKey(
            BrowserFieldKey::Backspace,
        ))),
        Key::Named(Named::Delete) => Some(Message::Browser(BrowserMessage::FieldKey(
            BrowserFieldKey::Delete,
        ))),
        Key::Named(Named::ArrowLeft) => Some(Message::Browser(BrowserMessage::FieldKey(
            BrowserFieldKey::MoveLeft,
        ))),
        Key::Named(Named::ArrowRight) => Some(Message::Browser(BrowserMessage::FieldKey(
            BrowserFieldKey::MoveRight,
        ))),
        Key::Named(Named::Home) => Some(Message::Browser(BrowserMessage::FieldKey(
            BrowserFieldKey::MoveHome,
        ))),
        Key::Named(Named::End) => Some(Message::Browser(BrowserMessage::FieldKey(
            BrowserFieldKey::MoveEnd,
        ))),
        Key::Named(Named::Enter) => Some(Message::Browser(BrowserMessage::SubmitFieldDraft)),
        Key::Named(Named::Escape) => Some(Message::Browser(BrowserMessage::CancelFieldDraft)),
        _ => map_key_press(key, modifiers),
    }
}

pub(super) fn shifted_browser_field_text(text: &str, modifiers: keyboard::Modifiers) -> String {
    if !modifiers.shift() {
        return text.to_string();
    }
    let mut chars = text.chars();
    let Some(ch) = chars.next() else {
        return text.to_string();
    };
    if chars.next().is_some() {
        return text.to_string();
    }
    match ch {
        'a'..='z' => ch.to_ascii_uppercase().to_string(),
        '1' => "!".into(),
        '2' => "@".into(),
        '3' => "#".into(),
        '4' => "$".into(),
        '5' => "%".into(),
        '6' => "^".into(),
        '7' => "&".into(),
        '8' => "*".into(),
        '9' => "(".into(),
        '0' => ")".into(),
        '-' => "_".into(),
        '=' => "+".into(),
        '[' => "{".into(),
        ']' => "}".into(),
        '\\' => "|".into(),
        ';' => ":".into(),
        '\'' => "\"".into(),
        ',' => "<".into(),
        '.' => ">".into(),
        '/' => "?".into(),
        '`' => "~".into(),
        _ => text.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    use crate::app::App;
    use crate::browser::{BrowserPage, PageSource};
    use iced::keyboard::key::Named;

    fn desktop_with_temp_root(name: &str) -> DesktopApp {
        let root = std::env::temp_dir().join(format!("{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let paths = crate::config::AppPaths::from_root(root);
        paths.ensure().expect("paths");
        DesktopApp::new(App::new(crate::config::AppConfig {
            paths,
            settings: crate::storage::settings::AppSettings::default(),
        }))
    }

    fn apply_profile_field_page(desktop: &mut DesktopApp) {
        let page = BrowserPage {
            url: "mock.node:/profile.mu".into(),
            title: "Profile".into(),
            markup: "`<12|nickname`saved>".into(),
            source: PageSource::Network,
            metadata: BTreeMap::new(),
            request_data: None,
        };
        desktop
            .app
            .active_browser_tab_mut()
            .session
            .apply_page(page, true);
        assert!(desktop.app.focus_browser_item_with_viewport(80, 20, false));
        assert!(desktop.app.activate_focused_browser_control());
    }

    #[test]
    fn desktop_address_input_events_end_page_field_editor_and_edit_url_bar() {
        let mut desktop = desktop_with_temp_root("omenbrowser-rs-desktop-field-edit");
        let tab_id = desktop.app.active_browser_tab().id;
        apply_profile_field_page(&mut desktop);

        let _ = desktop.update(Message::Browser(BrowserMessage::PaneAddressChanged {
            tab_id,
            value: "mock.node:/other.mu".into(),
        }));
        let _ = desktop.update(Message::Browser(BrowserMessage::FieldKey(
            BrowserFieldKey::Insert("x".into()),
        )));

        assert!(desktop.app.active_browser_field_editor().is_none());
        assert_eq!(
            desktop.app.active_browser_tab().address_input,
            "mock.node:/other.mu"
        );
        assert_eq!(
            desktop
                .app
                .active_browser_tab()
                .session
                .field_values
                .get("nickname"),
            Some(&"saved".to_string())
        );
    }

    #[test]
    fn new_browser_tab_clears_stale_page_field_editor() {
        let mut desktop = desktop_with_temp_root("omenbrowser-rs-desktop-new-browser-clears-field");
        apply_profile_field_page(&mut desktop);

        let _ = desktop.update(Message::Browser(BrowserMessage::NewTab));

        assert!(desktop.app.active_browser_field_editor().is_none());
        assert_eq!(
            desktop.app.active_browser_tab().address_input,
            desktop.app.settings.default_start_page
        );
    }

    #[test]
    fn browser_field_insert_is_utf8_atomic_at_the_value_limit() {
        let mut desktop = desktop_with_temp_root("omenbrowser-rs-desktop-field-value-limit");
        apply_profile_field_page(&mut desktop);
        let limit = crate::micron::parser::MICRON_CONTROL_VALUE_MAX_BYTES;
        let initial = "a".repeat(limit - 1);
        assert!(desktop.app.set_active_browser_field_draft(initial.clone()));

        let _ = desktop.update(Message::Browser(BrowserMessage::FieldKey(
            BrowserFieldKey::Insert("é".into()),
        )));

        let editor = desktop
            .app
            .active_browser_field_editor()
            .expect("field editor remains active");
        assert_eq!(editor.value, initial);
        assert_eq!(
            desktop
                .app
                .active_browser_tab()
                .session
                .field_values
                .get("nickname")
                .map(String::as_str),
            Some(editor.value.as_str())
        );
        assert_eq!(
            desktop.app.status.task,
            "page field limit reached; input was not changed"
        );

        let _ = desktop.update(Message::Browser(BrowserMessage::FieldKey(
            BrowserFieldKey::Insert("b".into()),
        )));
        assert_eq!(
            desktop
                .app
                .active_browser_field_editor()
                .expect("field editor")
                .value
                .len(),
            limit
        );
    }

    #[test]
    fn browser_field_draft_preserves_ui_and_session_at_aggregate_limit() {
        let mut desktop = desktop_with_temp_root("omenbrowser-rs-desktop-field-aggregate-limit");
        apply_profile_field_page(&mut desktop);
        let value = "v".repeat(crate::micron::parser::MICRON_CONTROL_VALUE_MAX_BYTES);
        for index in 0..63 {
            assert!(desktop
                .app
                .active_browser_tab_mut()
                .session
                .set_field_value(format!("f{index}"), value.clone()));
        }

        assert!(!desktop.app.set_active_browser_field_draft(value));
        assert_eq!(
            desktop
                .app
                .active_browser_field_editor()
                .expect("field editor")
                .value,
            "saved"
        );
        assert_eq!(
            desktop
                .app
                .active_browser_tab()
                .session
                .field_values
                .get("nickname")
                .map(String::as_str),
            Some("saved")
        );
    }

    #[test]
    fn keyboard_shortcuts_map_browser_focus_and_activation() {
        assert!(matches!(
            map_key_press(
                keyboard::Key::Named(Named::Tab),
                keyboard::Modifiers::empty()
            ),
            Some(Message::Browser(BrowserMessage::FocusItem {
                reverse: false
            }))
        ));
        assert!(matches!(
            map_key_press(keyboard::Key::Named(Named::Tab), keyboard::Modifiers::SHIFT),
            Some(Message::Browser(BrowserMessage::FocusItem {
                reverse: true
            }))
        ));
        assert!(matches!(
            map_key_press(
                keyboard::Key::Named(Named::Enter),
                keyboard::Modifiers::empty()
            ),
            Some(Message::Browser(BrowserMessage::ActivateFocusedItem))
        ));
        assert!(matches!(
            map_key_press(
                keyboard::Key::Named(Named::Space),
                keyboard::Modifiers::empty()
            ),
            Some(Message::Browser(BrowserMessage::ActivateFocusedItem))
        ));
        assert!(matches!(
            map_key_press(
                keyboard::Key::Named(Named::F9),
                keyboard::Modifiers::empty(),
            ),
            Some(Message::Shell(ShellMessage::ToggleNavigation))
        ));
    }

    #[test]
    fn keyboard_shortcuts_map_browser_tabs_and_scroll() {
        assert!(matches!(
            map_key_press(
                keyboard::Key::Character("t".into()),
                keyboard::Modifiers::COMMAND
            ),
            Some(Message::Browser(BrowserMessage::NewTab))
        ));
        assert!(matches!(
            map_key_press(
                keyboard::Key::Named(Named::PageDown),
                keyboard::Modifiers::empty()
            ),
            Some(Message::Browser(BrowserMessage::ScrollPage {
                direction: 1
            }))
        ));
        assert!(matches!(
            map_key_press(
                keyboard::Key::Character("+".into()),
                keyboard::Modifiers::COMMAND
            ),
            Some(Message::Browser(BrowserMessage::Zoom { direction: 1 }))
        ));
        assert!(matches!(
            map_key_press(
                keyboard::Key::Character("=".into()),
                keyboard::Modifiers::COMMAND
            ),
            Some(Message::Browser(BrowserMessage::Zoom { direction: 1 }))
        ));
        assert!(matches!(
            map_key_press(
                keyboard::Key::Character("-".into()),
                keyboard::Modifiers::COMMAND
            ),
            Some(Message::Browser(BrowserMessage::Zoom { direction: -1 }))
        ));
    }

    #[test]
    fn keyboard_shortcuts_map_browser_live_diagnostics_actions() {
        assert!(matches!(
            map_key_press(
                keyboard::Key::Character("r".into()),
                keyboard::Modifiers::COMMAND
            ),
            Some(Message::Browser(BrowserMessage::Reload))
        ));
        assert!(map_key_press(
            keyboard::Key::Character("n".into()),
            keyboard::Modifiers::empty()
        )
        .is_none());
        assert!(matches!(
            map_key_press(
                keyboard::Key::Character("x".into()),
                keyboard::Modifiers::COMMAND
            ),
            Some(Message::Browser(BrowserMessage::LiveProbe))
        ));
        assert!(matches!(
            map_key_press(
                keyboard::Key::Character("p".into()),
                keyboard::Modifiers::COMMAND
            ),
            Some(Message::Browser(BrowserMessage::PathDiagnostics))
        ));
    }

    #[test]
    fn browser_field_keyboard_map_edits_only_active_field_text() {
        assert!(matches!(
            map_browser_field_key_press(
                keyboard::Key::Character("a".into()),
                keyboard::Modifiers::empty(),
            ),
            Some(Message::Browser(BrowserMessage::FieldKey(BrowserFieldKey::Insert(value)))) if value == "a"
        ));
        assert!(matches!(
            map_browser_field_key_press(
                keyboard::Key::Named(Named::Backspace),
                keyboard::Modifiers::empty(),
            ),
            Some(Message::Browser(BrowserMessage::FieldKey(
                BrowserFieldKey::Backspace
            )))
        ));
        assert!(matches!(
            map_browser_field_key_press(
                keyboard::Key::Named(Named::Enter),
                keyboard::Modifiers::empty(),
            ),
            Some(Message::Browser(BrowserMessage::SubmitFieldDraft))
        ));
        assert!(matches!(
            map_browser_field_key_press(
                keyboard::Key::Named(Named::Escape),
                keyboard::Modifiers::empty(),
            ),
            Some(Message::Browser(BrowserMessage::CancelFieldDraft))
        ));
        assert!(matches!(
            map_browser_field_key_press(
                keyboard::Key::Character("t".into()),
                keyboard::Modifiers::COMMAND,
            ),
            Some(Message::Browser(BrowserMessage::NewTab))
        ));
    }

    #[test]
    fn browser_field_event_listener_routes_captured_text_input_keys() {
        assert!(matches!(
            map_browser_field_keyboard_event(
                iced::Event::Keyboard(keyboard::Event::KeyPressed {
                    key: keyboard::Key::Character("a".into()),
                    modified_key: keyboard::Key::Character("a".into()),
                    physical_key: keyboard::key::Physical::Unidentified(
                        keyboard::key::NativeCode::Unidentified
                    ),
                    location: keyboard::Location::Standard,
                    modifiers: keyboard::Modifiers::empty(),
                    text: None,
                    repeat: false,
                }),
                event::Status::Captured,
                window::Id::unique(),
            ),
            Some(Message::Browser(BrowserMessage::FieldKey(BrowserFieldKey::Insert(value)))) if value == "a"
        ));
    }

    #[test]
    fn browser_field_event_listener_prefers_text_payload_for_insertions() {
        assert!(matches!(
            map_browser_field_keyboard_event(
                iced::Event::Keyboard(keyboard::Event::KeyPressed {
                    key: keyboard::Key::Unidentified,
                    modified_key: keyboard::Key::Unidentified,
                    physical_key: keyboard::key::Physical::Unidentified(
                        keyboard::key::NativeCode::Unidentified
                    ),
                    location: keyboard::Location::Standard,
                    modifiers: keyboard::Modifiers::empty(),
                    text: Some("x".into()),
                    repeat: false,
                }),
                event::Status::Captured,
                window::Id::unique(),
            ),
            Some(Message::Browser(BrowserMessage::FieldKey(BrowserFieldKey::Insert(value)))) if value == "x"
        ));
        assert!(matches!(
            map_browser_field_key_event_press(
                keyboard::Key::Named(Named::Backspace),
                keyboard::Modifiers::empty(),
                None,
            ),
            Some(Message::Browser(BrowserMessage::FieldKey(
                BrowserFieldKey::Backspace
            )))
        ));
    }

    #[test]
    fn browser_field_keyboard_map_preserves_shifted_text() {
        assert!(matches!(
            map_browser_field_key_press(
                keyboard::Key::Character("a".into()),
                keyboard::Modifiers::SHIFT,
            ),
            Some(Message::Browser(BrowserMessage::FieldKey(BrowserFieldKey::Insert(value)))) if value == "A"
        ));
        assert!(matches!(
            map_browser_field_key_press(
                keyboard::Key::Character("1".into()),
                keyboard::Modifiers::SHIFT,
            ),
            Some(Message::Browser(BrowserMessage::FieldKey(BrowserFieldKey::Insert(value)))) if value == "!"
        ));
        assert!(matches!(
            map_browser_field_key_press(
                keyboard::Key::Character("/".into()),
                keyboard::Modifiers::SHIFT,
            ),
            Some(Message::Browser(BrowserMessage::FieldKey(BrowserFieldKey::Insert(value)))) if value == "?"
        ));
        assert_eq!(
            shifted_browser_field_text("A", keyboard::Modifiers::SHIFT),
            "A"
        );
    }
}
