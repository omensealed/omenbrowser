use iced::widget::{row, text};
use iced::Element;

use super::super::{emoji_font, Message};

pub(in crate::desktop) fn compact_label(value: &str, max_chars: usize) -> String {
    let value = printable_label(value.trim());
    let count = value.chars().count();
    if count <= max_chars {
        value
    } else {
        let keep = max_chars.saturating_sub(3);
        format!("{}...", value.chars().take(keep).collect::<String>())
    }
}

pub(in crate::desktop) fn printable_label(value: &str) -> String {
    value.chars().filter(|ch| !ch.is_control()).collect()
}

pub(in crate::desktop) fn emoji_aware_text<'a>(value: String, size: u16) -> Element<'a, Message> {
    let size = u32::from(size);
    if !value.chars().any(is_emoji_like) {
        return text(value).size(size).into();
    }

    let mut runs: Vec<(bool, String)> = Vec::new();
    for ch in value.chars() {
        let emoji = is_emoji_like(ch);
        if let Some((last_emoji, run)) = runs.last_mut() {
            if *last_emoji == emoji {
                run.push(ch);
                continue;
            }
        }
        runs.push((emoji, ch.to_string()));
    }

    runs.into_iter()
        .fold(row![].spacing(0), |row, (emoji, run)| {
            let mut label = text(run).size(size);
            if emoji {
                label = label.font(emoji_font());
            }
            row.push(label)
        })
        .wrap()
        .into()
}

pub(in crate::desktop) fn is_emoji_like(ch: char) -> bool {
    matches!(
        ch as u32,
        0x1F000..=0x1FAFF | 0x2600..=0x27BF | 0x2300..=0x23FF
    )
}

pub(in crate::desktop) fn visible_tab_window(
    total: usize,
    active: usize,
    max_visible: usize,
) -> (usize, usize) {
    if total == 0 {
        return (0, 0);
    }
    let max_visible = max_visible.max(1).min(total);
    let active = active.min(total - 1);
    let mut start = active.saturating_sub(max_visible / 2);
    if start + max_visible > total {
        start = total - max_visible;
    }
    (start, start + max_visible)
}

pub(in crate::desktop) fn relative_time(epoch_secs: f64) -> String {
    if epoch_secs <= 0.0 {
        return "never".into();
    }
    let now_secs = crate::app::current_epoch_ms() as f64 / 1_000.0;
    let elapsed = (now_secs - epoch_secs).max(0.0) as u64;
    match elapsed {
        0..=59 => format!("{elapsed}s ago"),
        60..=3_599 => format!("{}m ago", elapsed / 60),
        3_600..=86_399 => format!("{}h ago", elapsed / 3_600),
        _ => format!("{}d ago", elapsed / 86_400),
    }
}

pub(in crate::desktop) fn format_epoch_secs(epoch_secs: f64) -> String {
    if epoch_secs <= 0.0 {
        return "never".into();
    }
    format_epoch_ms((epoch_secs * 1_000.0) as u64)
}

pub(in crate::desktop) fn format_epoch_ms(epoch_ms: u64) -> String {
    let total_seconds = (epoch_ms / 1_000) as i64;
    let days = total_seconds.div_euclid(86_400);
    let seconds_of_day = total_seconds.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    let hour = seconds_of_day / 3_600;
    let minute = (seconds_of_day % 3_600) / 60;
    let second = seconds_of_day % 60;
    format!("{year:04}-{month:02}-{day:02} {hour:02}:{minute:02}:{second:02} UTC")
}

pub(in crate::desktop) fn human_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} {}", UNITS[unit])
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

pub(in crate::desktop) fn compact_elapsed_ms(milliseconds: u64) -> String {
    let seconds = milliseconds / 1_000;
    if seconds < 60 {
        format!("{seconds}s")
    } else if seconds < 3_600 {
        format!("{}m {}s", seconds / 60, seconds % 60)
    } else {
        format!("{}h {}m", seconds / 3_600, (seconds % 3_600) / 60)
    }
}

pub(in crate::desktop) fn compact_footer_status(message: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }
    let compact = message.split_whitespace().collect::<Vec<_>>().join(" ");
    if compact.chars().count() <= max_chars {
        return compact;
    }
    if max_chars <= 3 {
        return compact.chars().take(max_chars).collect();
    }
    let mut clipped = compact.chars().take(max_chars - 3).collect::<String>();
    clipped.push_str("...");
    clipped
}

pub(in crate::desktop) fn compact_identity_status_label(message: &str) -> String {
    message
        .trim()
        .strip_prefix("identity:")
        .map(str::trim)
        .filter(|label| !label.is_empty())
        .unwrap_or_else(|| message.trim())
        .to_owned()
}

fn civil_from_days(days_since_epoch: i64) -> (i32, u32, u32) {
    let z = days_since_epoch + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    let year = year + if month <= 2 { 1 } else { 0 };
    (year as i32, month as u32, day as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn footer_status_compaction_stays_single_line() {
        let compact = compact_footer_status(
            "link request timed out after 45s; request cancelled,\nretry when path/link is ready | run Diagnostics X or L for link/request/response report",
            72,
        );

        assert!(!compact.contains('\n'));
        assert!(compact.chars().count() <= 72);
        assert!(compact.ends_with("..."));
    }

    #[test]
    fn identity_footer_label_drops_verbose_prefix() {
        assert_eq!(
            compact_identity_status_label("identity: OMENbrowser_dev"),
            "OMENbrowser_dev"
        );
        assert_eq!(compact_identity_status_label("OMENTest"), "OMENTest");
    }

    #[test]
    fn emoji_detection_covers_common_title_symbols() {
        assert!(is_emoji_like('🐈'));
        assert!(is_emoji_like('☠'));
        assert!(!is_emoji_like('C'));
    }

    #[test]
    fn human_bytes_formats_resource_sizes() {
        assert_eq!(human_bytes(512), "512 B");
        assert_eq!(human_bytes(1536), "1.5 KiB");
        assert_eq!(human_bytes(2 * 1024 * 1024), "2.0 MiB");
        assert!(crate::workspace::WorkspaceSection::ALL
            .contains(&crate::workspace::WorkspaceSection::Monitoring));
        assert!(crate::workspace::WorkspaceSection::ALL
            .contains(&crate::workspace::WorkspaceSection::NetworkDoctor));
        assert!(crate::workspace::WorkspaceSection::ALL
            .contains(&crate::workspace::WorkspaceSection::Help));
    }

    #[test]
    fn desktop_timestamp_formatting_uses_real_utc_dates() {
        assert_eq!(format_epoch_ms(0), "1970-01-01 00:00:00 UTC");
        assert_eq!(
            format_epoch_ms(1_700_000_000_000),
            "2023-11-14 22:13:20 UTC"
        );
    }

    #[test]
    fn browser_tab_window_keeps_active_tab_visible() {
        assert_eq!(visible_tab_window(0, 0, 6), (0, 0));
        assert_eq!(visible_tab_window(3, 0, 6), (0, 3));
        assert_eq!(visible_tab_window(10, 0, 6), (0, 6));
        assert_eq!(visible_tab_window(10, 5, 6), (2, 8));
        assert_eq!(visible_tab_window(10, 9, 6), (4, 10));
    }
}
