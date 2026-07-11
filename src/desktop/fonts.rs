use std::process::Command;
use std::sync::OnceLock;

use iced::Font;

use super::{EMOJI_CHARSET, IDENTIFY_ICON_CHARSET};

static NERD_FONT_FAMILY: OnceLock<Option<&'static str>> = OnceLock::new();
static EMOJI_FONT_FAMILY: OnceLock<Option<&'static str>> = OnceLock::new();

pub(super) const MICRON_VIEWPORT_FONT_BYTES: &[u8] =
    include_bytes!("../../assets/fonts/adwaita/AdwaitaMono-Regular.ttf");

pub(super) fn desktop_ui_font() -> Font {
    Font::MONOSPACE
}

pub(super) fn nerd_icon_font() -> Font {
    detected_nerd_font_family()
        .map(Font::with_name)
        .unwrap_or(Font::MONOSPACE)
}

pub(super) fn emoji_font() -> Font {
    detected_emoji_font_family()
        .map(Font::with_name)
        .unwrap_or(Font::DEFAULT)
}

fn detected_nerd_font_family() -> Option<&'static str> {
    *NERD_FONT_FAMILY.get_or_init(|| {
        detect_system_font_family_for_char(IDENTIFY_ICON_CHARSET)
            .map(|family| Box::leak(family.into_boxed_str()) as &'static str)
    })
}

fn detected_emoji_font_family() -> Option<&'static str> {
    *EMOJI_FONT_FAMILY.get_or_init(|| {
        detect_system_font_family_for_query("emoji", EMOJI_CHARSET)
            .or_else(|| detect_system_font_family_for_query("sans", EMOJI_CHARSET))
            .map(|family| Box::leak(family.into_boxed_str()) as &'static str)
    })
}

fn detect_system_font_family_for_char(charset: &str) -> Option<String> {
    fc_match_families_output("monospace", charset)
        .and_then(|output| select_nerd_font_family_from_fc_match(&output))
}

fn detect_system_font_family_for_query(family: &str, charset: &str) -> Option<String> {
    fc_match_families_output(family, charset)
        .and_then(|output| select_first_font_family_from_fc_match(&output))
}

fn fc_match_families_output(family: &str, charset: &str) -> Option<String> {
    let query = format!("{family}:charset={charset}");
    let output = Command::new("fc-match")
        .arg("--format=%{family}\n")
        .arg(query)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout).ok()
}

pub(super) fn select_first_font_family_from_fc_match(output: &str) -> Option<String> {
    output
        .lines()
        .flat_map(|line| line.split(','))
        .map(str::trim)
        .find(|family| !family.is_empty())
        .map(ToOwned::to_owned)
}

pub(super) fn select_nerd_font_family_from_fc_match(output: &str) -> Option<String> {
    let families = output
        .lines()
        .flat_map(|line| line.split(','))
        .map(str::trim)
        .filter(|family| !family.is_empty());
    let mut first = None;
    for family in families {
        if first.is_none() {
            first = Some(family.to_string());
        }
        if family.to_ascii_lowercase().contains("nerd font") {
            return Some(family.to_string());
        }
    }
    first
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fontconfig_family_selection_prefers_nerd_font_alias() {
        assert_eq!(
            select_nerd_font_family_from_fc_match("Iosevka,Iosevka Nerd Font\n"),
            Some("Iosevka Nerd Font".into())
        );
        assert_eq!(
            select_nerd_font_family_from_fc_match("MesloLGS Nerd Font Mono\n"),
            Some("MesloLGS Nerd Font Mono".into())
        );
        assert_eq!(
            select_nerd_font_family_from_fc_match("Noto Sans Mono\n"),
            Some("Noto Sans Mono".into())
        );
    }
}
