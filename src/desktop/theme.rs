use iced::theme::Palette;
use iced::widget::scrollable::Status as ScrollableStatus;
use iced::widget::{button, container};
use iced::{Background, Border, Color, Shadow, Theme};

use super::{set_desktop_font_size, DesktopApp, Message, ThemeMessage};

pub(in crate::desktop) const DESKTOP_THEME_CHOICES: &[&str] = &[
    "default",
    "omen",
    "nord",
    "blue",
    "terminal_green",
    "terror",
    "abyss",
    "necropolis",
];

pub(super) fn theme_from_name(name: &str) -> Theme {
    match name.trim().to_ascii_lowercase().as_str() {
        "default" | "dark" => Theme::Dark,
        "omen" => omen_desktop_theme(),
        "nord" => Theme::Nord,
        "blue" | "deep_blue" | "deep-blue" => deep_blue_desktop_theme(),
        "terminal_green" | "terminal-green" | "terminal green" | "solarized_dark"
        | "solarized-dark" | "solarized dark" => terminal_green_desktop_theme(),
        "terror" | "blood" | "blood_rite" | "blood-rite" => terror_desktop_theme(),
        "abyss" | "abyssal" => abyss_desktop_theme(),
        "necropolis" | "bone" => necropolis_desktop_theme(),
        "gruvbox_dark" | "gruvbox-dark" | "gruvbox dark" => Theme::GruvboxDark,
        "dracula" => Theme::Dracula,
        "catppuccin" | "mocha" | "catppuccin_mocha" => Theme::CatppuccinMocha,
        "tokyo" | "tokyo_night" => Theme::TokyoNight,
        "kanagawa" | "kanagawa_dragon" | "moonfly" | "light" => Theme::Dark,
        "nightfly" => Theme::Nightfly,
        "oxocarbon" => Theme::Oxocarbon,
        _ => Theme::Dark,
    }
}

impl DesktopApp {
    pub(super) fn theme(&self) -> Theme {
        theme_from_name(&self.app.settings.ui.theme_name)
    }

    pub(super) fn dispatch_theme_message(
        &mut self,
        message: Message,
    ) -> Result<iced::Task<Message>, Message> {
        match message {
            Message::Theme(ThemeMessage::SetTheme(theme)) => {
                self.update_set_theme(theme);
                Ok(iced::Task::none())
            }
            Message::Theme(ThemeMessage::SetFontSize(size)) => {
                self.update_set_font_size(size);
                Ok(iced::Task::none())
            }
            Message::Theme(ThemeMessage::ToggleReducedMotion) => {
                self.update_toggle_reduced_motion();
                Ok(iced::Task::none())
            }
            Message::Theme(ThemeMessage::ToggleLowPower) => {
                self.update_toggle_low_power();
                Ok(iced::Task::none())
            }
            _ => Err(message),
        }
    }

    pub(super) fn update_set_theme(&mut self, theme: String) {
        self.app.set_settings_theme_name(theme);
    }

    pub(super) fn update_set_font_size(&mut self, size: u16) {
        self.app.set_settings_font_size(size);
        set_desktop_font_size(size);
    }

    pub(super) fn update_toggle_reduced_motion(&mut self) {
        self.app.toggle_settings_reduced_motion();
    }

    pub(super) fn update_toggle_low_power(&mut self) {
        self.app.toggle_settings_low_power_mode();
    }
}

fn omen_desktop_theme() -> Theme {
    Theme::custom(
        "OMEN",
        Palette {
            background: Color::from_rgb8(4, 2, 3),
            text: Color::from_rgb8(232, 222, 220),
            primary: Color::from_rgb8(156, 28, 36),
            success: Color::from_rgb8(166, 84, 70),
            warning: Color::from_rgb8(228, 154, 64),
            danger: Color::from_rgb8(218, 54, 60),
        },
    )
}

fn deep_blue_desktop_theme() -> Theme {
    Theme::custom(
        "OMEN Blue",
        Palette {
            background: Color::from_rgb8(0, 10, 24),
            text: Color::from_rgb8(218, 242, 255),
            primary: Color::from_rgb8(24, 168, 232),
            success: Color::from_rgb8(58, 214, 184),
            warning: Color::from_rgb8(255, 196, 90),
            danger: Color::from_rgb8(255, 92, 92),
        },
    )
}

fn terminal_green_desktop_theme() -> Theme {
    Theme::custom(
        "Terminal Green",
        Palette {
            background: Color::from_rgb8(1, 9, 5),
            text: Color::from_rgb8(178, 238, 188),
            primary: Color::from_rgb8(42, 176, 86),
            success: Color::from_rgb8(64, 214, 116),
            warning: Color::from_rgb8(210, 210, 82),
            danger: Color::from_rgb8(232, 76, 64),
        },
    )
}

fn terror_desktop_theme() -> Theme {
    Theme::custom(
        "Terror",
        Palette {
            background: Color::from_rgb8(4, 0, 10),
            text: Color::from_rgb8(237, 226, 255),
            primary: Color::from_rgb8(122, 48, 226),
            success: Color::from_rgb8(168, 232, 54),
            warning: Color::from_rgb8(246, 196, 64),
            danger: Color::from_rgb8(255, 112, 28),
        },
    )
}

fn abyss_desktop_theme() -> Theme {
    Theme::custom(
        "Abyss",
        Palette {
            background: Color::from_rgb8(1, 3, 10),
            text: Color::from_rgb8(218, 226, 238),
            primary: Color::from_rgb8(76, 64, 190),
            success: Color::from_rgb8(54, 196, 170),
            warning: Color::from_rgb8(198, 176, 92),
            danger: Color::from_rgb8(221, 62, 122),
        },
    )
}

fn necropolis_desktop_theme() -> Theme {
    Theme::custom(
        "Necropolis",
        Palette {
            background: Color::from_rgb8(5, 5, 6),
            text: Color::from_rgb8(224, 219, 204),
            primary: Color::from_rgb8(128, 155, 116),
            success: Color::from_rgb8(156, 190, 128),
            warning: Color::from_rgb8(190, 172, 94),
            danger: Color::from_rgb8(194, 62, 72),
        },
    )
}

pub(super) fn omen_application_style(theme: &Theme) -> iced::theme::Style {
    let palette = theme.palette();
    iced::theme::Style {
        background_color: mix_color(palette.background, omen_surface(), 0.55),
        text_color: palette.text,
    }
}

pub(super) fn shell_container_style(theme: &Theme) -> container::Style {
    container::Style::default()
        .background(mix_color(theme.palette().background, omen_surface(), 0.25))
        .color(theme.palette().text)
}

pub(super) fn card_container_style(theme: &Theme) -> container::Style {
    container::Style::default()
        .background(mix_color(theme.palette().background, omen_panel(), 0.42))
        .color(theme.palette().text)
        .border(Border {
            color: mix_color(theme.palette().primary, omen_accent(), 0.55),
            width: 1.0,
            radius: 0.0.into(),
        })
        .shadow(Shadow::default())
}

pub(super) fn status_container_style(theme: &Theme) -> container::Style {
    container::Style::default()
        .background(mix_color(theme.palette().background, omen_status(), 0.55))
        .color(theme.palette().text)
        .border(Border {
            color: mix_color(theme.palette().primary, omen_accent(), 0.45),
            width: 1.0,
            radius: 0.0.into(),
        })
}

pub(super) fn address_display_container_style(theme: &Theme) -> container::Style {
    container::Style::default()
        .background(mix_color(theme.palette().background, Color::BLACK, 0.75))
        .color(mix_color(theme.palette().text, Color::WHITE, 0.2))
        .border(Border {
            color: mix_color(theme.palette().primary, omen_accent(), 0.3),
            width: 1.0,
            radius: 0.0.into(),
        })
}

pub(super) fn warning_container_style(theme: &Theme) -> container::Style {
    container::Style::default()
        .background(mix_color(
            theme.palette().background,
            omen_warning_bg(),
            0.7,
        ))
        .color(theme.palette().text)
        .border(Border {
            color: omen_warning(),
            width: 1.5,
            radius: 0.0.into(),
        })
}

pub(super) fn incoming_message_container_style(theme: &Theme) -> container::Style {
    container::Style::default()
        .background(mix_color(theme.palette().background, omen_panel(), 0.58))
        .color(theme.palette().text)
        .border(Border {
            color: mix_color(theme.palette().primary, omen_accent(), 0.24),
            width: 1.0,
            radius: 0.0.into(),
        })
}

pub(super) fn outgoing_message_container_style(theme: &Theme) -> container::Style {
    container::Style::default()
        .background(mix_color(theme.palette().primary, omen_accent_deep(), 0.28))
        .color(theme.palette().text)
        .border(Border {
            color: mix_color(theme.palette().primary, omen_accent(), 0.48),
            width: 1.0,
            radius: 0.0.into(),
        })
}

pub(super) fn failed_message_container_style(theme: &Theme) -> container::Style {
    container::Style::default()
        .background(mix_color(
            theme.palette().background,
            omen_warning_bg(),
            0.58,
        ))
        .color(theme.palette().text)
        .border(Border {
            color: omen_warning(),
            width: 1.0,
            radius: 0.0.into(),
        })
}

pub(super) fn selected_message_container_style(theme: &Theme) -> container::Style {
    container::Style::default()
        .background(mix_color(theme.palette().primary, omen_accent_deep(), 0.36))
        .color(theme.palette().text)
        .border(Border {
            color: omen_accent(),
            width: 2.0,
            radius: 0.0.into(),
        })
}

pub(super) fn message_detail_container_style(theme: &Theme) -> container::Style {
    container::Style::default()
        .background(mix_color(theme.palette().background, omen_surface(), 0.7))
        .color(theme.palette().text)
        .border(Border {
            color: mix_color(theme.palette().primary, omen_accent(), 0.16),
            width: 1.0,
            radius: 0.0.into(),
        })
}

pub(super) fn browser_viewport_container_style(
    theme: &Theme,
    page_background: Option<Color>,
    page_border: Option<Color>,
) -> container::Style {
    container::Style::default()
        .background(page_background.unwrap_or(Color::from_rgb8(4, 8, 10)))
        .border(Border {
            color: page_border
                .unwrap_or_else(|| mix_color(theme.palette().primary, omen_accent(), 0.65)),
            width: 1.0,
            radius: 0.0.into(),
        })
}

pub(super) fn workspace_pane_container_style(theme: &Theme) -> container::Style {
    container::Style::default()
        .background(mix_color(theme.palette().background, omen_surface(), 0.72))
        .color(theme.palette().text)
        .border(Border {
            color: mix_color(theme.palette().primary, omen_accent(), 0.4),
            width: 1.0,
            radius: 0.0.into(),
        })
}

pub(super) fn pane_title_container_style(theme: &Theme) -> container::Style {
    container::Style::default()
        .background(mix_color(theme.palette().background, omen_panel(), 0.78))
        .color(theme.palette().text)
        .border(Border {
            color: mix_color(theme.palette().primary, omen_accent(), 0.55),
            width: 1.0,
            radius: 0.0.into(),
        })
}

pub(super) fn omen_button_style(theme: &Theme, status: button::Status) -> button::Style {
    let base = match status {
        button::Status::Hovered => mix_color(theme.palette().primary, omen_accent(), 0.45),
        button::Status::Pressed => omen_accent_deep(),
        button::Status::Disabled => Color::from_rgb8(48, 55, 59),
        button::Status::Active => mix_color(theme.palette().primary, omen_accent_deep(), 0.25),
    };
    button::Style {
        background: Some(Background::Color(base)),
        text_color: Color::WHITE,
        border: Border {
            color: mix_color(theme.palette().primary, omen_accent(), 0.75),
            width: 1.0,
            radius: 6.0.into(),
        },
        shadow: Shadow::default(),
        snap: false,
    }
}

pub(super) fn subtle_button_style(theme: &Theme, status: button::Status) -> button::Style {
    let base = match status {
        button::Status::Hovered => mix_color(theme.palette().background, omen_panel(), 0.72),
        button::Status::Pressed => mix_color(theme.palette().background, omen_status(), 0.72),
        button::Status::Disabled => Color::from_rgb8(38, 43, 47),
        button::Status::Active => mix_color(theme.palette().background, omen_panel(), 0.46),
    };
    button::Style {
        background: Some(Background::Color(base)),
        text_color: theme.palette().text,
        border: Border {
            color: mix_color(theme.palette().primary, omen_accent(), 0.32),
            width: 1.0,
            radius: 6.0.into(),
        },
        shadow: Shadow::default(),
        snap: false,
    }
}

pub(super) fn toggle_button_style(
    theme: &Theme,
    status: button::Status,
    selected: bool,
) -> button::Style {
    let mut style = subtle_button_style(theme, status);
    if selected {
        style.border.color = mix_color(theme.palette().primary, omen_accent(), 0.75);
        style.border.width = 2.0;
    }
    style
}

pub(super) fn warning_button_style(theme: &Theme, status: button::Status) -> button::Style {
    let base = match status {
        button::Status::Hovered => omen_warning(),
        button::Status::Pressed => Color::from_rgb8(123, 48, 28),
        button::Status::Disabled => Color::from_rgb8(58, 42, 37),
        button::Status::Active => mix_color(theme.palette().danger, omen_warning(), 0.55),
    };
    button::Style {
        background: Some(Background::Color(base)),
        text_color: Color::WHITE,
        border: Border {
            color: omen_warning(),
            width: 1.0,
            radius: 6.0.into(),
        },
        shadow: Shadow::default(),
        snap: false,
    }
}

pub(super) fn inline_icon_button_style(theme: &Theme, status: button::Status) -> button::Style {
    let text_color = match status {
        button::Status::Hovered | button::Status::Pressed => theme.palette().primary,
        button::Status::Disabled => {
            mix_color(theme.palette().text, theme.palette().background, 0.55)
        }
        button::Status::Active => theme.palette().text,
    };
    button::Style {
        background: None,
        text_color,
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: 0.0.into(),
        },
        shadow: Shadow::default(),
        snap: false,
    }
}

pub(super) fn themed_scrollable_style(
    theme: &Theme,
    status: ScrollableStatus,
) -> iced::widget::scrollable::Style {
    let palette = theme.palette();
    let rail_background = mix_color(palette.background, omen_surface(), 0.84);
    let base_thumb = mix_color(palette.primary, omen_accent(), 0.48);
    let thumb_color = match status {
        ScrollableStatus::Active { .. } => base_thumb,
        ScrollableStatus::Hovered { .. } => mix_color(base_thumb, Color::WHITE, 0.2),
        ScrollableStatus::Dragged { .. } => mix_color(palette.primary, Color::WHITE, 0.34),
    };
    let rail = iced::widget::scrollable::Rail {
        background: Some(Background::Color(rail_background)),
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: 4.0.into(),
        },
        scroller: iced::widget::scrollable::Scroller {
            background: Background::Color(thumb_color),
            border: Border {
                color: mix_color(palette.primary, omen_accent(), 0.28),
                width: 0.5,
                radius: 4.0.into(),
            },
        },
    };

    iced::widget::scrollable::Style {
        container: container::Style::default(),
        vertical_rail: rail,
        horizontal_rail: rail,
        gap: Some(Background::Color(rail_background)),
        auto_scroll: iced::widget::scrollable::AutoScroll {
            background: Background::Color(mix_color(rail_background, Color::BLACK, 0.2)),
            border: Border {
                color: mix_color(palette.primary, omen_accent(), 0.28),
                width: 1.0,
                radius: 9999.0.into(),
            },
            shadow: Shadow::default(),
            icon: palette.text,
        },
    }
}

fn omen_surface() -> Color {
    Color::from_rgb8(9, 5, 7)
}

fn omen_panel() -> Color {
    Color::from_rgb8(20, 9, 12)
}

fn omen_status() -> Color {
    Color::from_rgb8(26, 10, 13)
}

fn omen_accent() -> Color {
    Color::from_rgb8(194, 54, 62)
}

fn omen_accent_deep() -> Color {
    Color::from_rgb8(82, 18, 24)
}

fn omen_warning() -> Color {
    Color::from_rgb8(205, 80, 68)
}

fn omen_warning_bg() -> Color {
    Color::from_rgb8(62, 24, 24)
}

fn mix_color(a: Color, b: Color, amount_b: f32) -> Color {
    let amount_b = amount_b.clamp(0.0, 1.0);
    let amount_a = 1.0 - amount_b;
    Color {
        r: a.r * amount_a + b.r * amount_b,
        g: a.g * amount_a + b.g * amount_b,
        b: a.b * amount_a + b.b * amount_b,
        a: a.a * amount_a + b.a * amount_b,
    }
}

#[cfg(test)]
mod tests {
    use iced::{Color, Theme};

    use super::*;

    #[test]
    fn desktop_theme_names_map_to_usable_iced_themes() {
        assert_eq!(theme_from_name("default"), Theme::Dark);
        assert_eq!(
            theme_from_name("omen").palette().primary,
            Color::from_rgb8(156, 28, 36)
        );
        assert_eq!(
            theme_from_name("blue").palette().primary,
            Color::from_rgb8(24, 168, 232)
        );
        assert_eq!(
            theme_from_name("terminal_green").palette().primary,
            Color::from_rgb8(42, 176, 86)
        );
        assert_eq!(
            theme_from_name("solarized_dark").palette().primary,
            Color::from_rgb8(42, 176, 86)
        );
        assert_eq!(
            theme_from_name("terror").palette().primary,
            Color::from_rgb8(122, 48, 226)
        );
        assert_eq!(
            theme_from_name("blood").palette().primary,
            Color::from_rgb8(122, 48, 226)
        );
        assert_eq!(
            theme_from_name("abyss").palette().primary,
            Color::from_rgb8(76, 64, 190)
        );
        assert_eq!(
            theme_from_name("necropolis").palette().primary,
            Color::from_rgb8(128, 155, 116)
        );
        assert_eq!(theme_from_name("kanagawa"), Theme::Dark);
        assert_eq!(theme_from_name("moonfly"), Theme::Dark);
        assert_eq!(theme_from_name("unknown"), Theme::Dark);
        assert!(DESKTOP_THEME_CHOICES.contains(&"default"));
        assert!(DESKTOP_THEME_CHOICES.contains(&"omen"));
        assert!(DESKTOP_THEME_CHOICES.contains(&"blue"));
        assert!(DESKTOP_THEME_CHOICES.contains(&"terminal_green"));
        assert!(DESKTOP_THEME_CHOICES.contains(&"terror"));
        assert!(DESKTOP_THEME_CHOICES.contains(&"abyss"));
        assert!(DESKTOP_THEME_CHOICES.contains(&"necropolis"));
        assert!(!DESKTOP_THEME_CHOICES.contains(&"dark"));
        assert!(!DESKTOP_THEME_CHOICES.contains(&"moonfly"));
        assert!(!DESKTOP_THEME_CHOICES.contains(&"kanagawa"));
        assert!(!DESKTOP_THEME_CHOICES.contains(&"light"));
        assert!(!DESKTOP_THEME_CHOICES.contains(&"blood"));
    }

    #[test]
    fn selected_toggle_uses_a_stronger_border_without_changing_layout_policy() {
        let theme = omen_desktop_theme();
        let inactive = toggle_button_style(&theme, button::Status::Active, false);
        let active = toggle_button_style(&theme, button::Status::Active, true);
        assert_eq!(inactive.border.width, 1.0);
        assert_eq!(active.border.width, 2.0);
        assert_ne!(inactive.border.color, active.border.color);
        assert_eq!(inactive.border.radius, active.border.radius);
    }
}
