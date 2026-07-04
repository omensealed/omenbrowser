pub(crate) fn fit_line_to_width(value: &str, max_width: usize) -> String {
    if max_width == 0 {
        return String::new();
    }
    let count = value.chars().count();
    if count <= max_width {
        return value.to_string();
    }
    if max_width <= 3 {
        return ".".repeat(max_width);
    }
    let keep = max_width.saturating_sub(3);
    format!("{}...", value.chars().take(keep).collect::<String>())
}

pub(crate) fn current_unix_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}

pub(crate) fn human_timestamp(unix_secs: i64) -> String {
    let now = current_unix_secs();
    let age = now.saturating_sub(unix_secs);
    format!("{} UTC ({})", unix_to_utc_string(unix_secs), human_age(age))
}

pub(crate) fn human_system_time_local(time: std::time::SystemTime) -> String {
    let unix_secs = time
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0);
    format!("{} UTC", unix_to_utc_string(unix_secs))
}

pub(crate) fn human_age(seconds: i64) -> String {
    let seconds = seconds.max(0);
    let days = seconds / 86_400;
    let hours = (seconds % 86_400) / 3_600;
    let minutes = (seconds % 3_600) / 60;
    if days > 0 {
        format!("{days}d {hours}h ago")
    } else if hours > 0 {
        format!("{hours}h {minutes}m ago")
    } else if minutes > 0 {
        format!("{minutes}m ago")
    } else {
        format!("{seconds}s ago")
    }
}

pub(crate) fn human_age_duration(seconds: i64) -> String {
    let seconds = seconds.max(0);
    let days = seconds / 86_400;
    let hours = (seconds % 86_400) / 3_600;
    let minutes = (seconds % 3_600) / 60;
    if days > 0 {
        format!("{days}d {hours}h")
    } else if hours > 0 {
        format!("{hours}h {minutes}m")
    } else if minutes > 0 {
        format!("{minutes}m")
    } else {
        format!("{seconds}s")
    }
}

pub(crate) fn unix_to_utc_string(unix_secs: i64) -> String {
    let days = unix_secs.div_euclid(86_400);
    let seconds_of_day = unix_secs.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    let hour = seconds_of_day / 3_600;
    let minute = (seconds_of_day % 3_600) / 60;
    let second = seconds_of_day % 60;
    format!("{year:04}-{month:02}-{day:02} {hour:02}:{minute:02}:{second:02}")
}

fn civil_from_days(days_since_epoch: i64) -> (i64, u32, u32) {
    let z = days_since_epoch + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    let year = y + if month <= 2 { 1 } else { 0 };
    (year, month as u32, day as u32)
}

pub(crate) fn human_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else if value >= 10.0 {
        format!("{value:.1} {}", UNITS[unit])
    } else {
        format!("{value:.2} {}", UNITS[unit])
    }
}

#[cfg(any(test, feature = "live-reticulum", all(feature = "live-rns-net", any())))]
pub(crate) fn human_bytes_per_second(bytes: u64, elapsed_secs: f64) -> String {
    human_bytes((bytes as f64 / elapsed_secs).round() as u64)
}

#[cfg(any(test, feature = "live-reticulum", all(feature = "live-rns-net", any())))]
pub(crate) fn human_duration(seconds: f64) -> String {
    if seconds >= 60.0 {
        format!("{:.1}m", seconds / 60.0)
    } else {
        format!("{seconds:.1}s")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_fitting_preserves_short_text_and_truncates_long_text() {
        assert_eq!(fit_line_to_width("short", 12), "short");
        assert_eq!(fit_line_to_width("abcdef", 0), "");
        assert_eq!(fit_line_to_width("abcdef", 2), "..");
        assert_eq!(fit_line_to_width("abcdef", 5), "ab...");
    }

    #[test]
    fn human_formatters_use_readable_bytes_and_dates() {
        assert_eq!(human_bytes(999), "999 B");
        assert_eq!(human_bytes(2048), "2.00 KiB");
        assert_eq!(unix_to_utc_string(0), "1970-01-01 00:00:00");
        assert_eq!(unix_to_utc_string(1_700_000_000), "2023-11-14 22:13:20");
    }

    #[test]
    fn human_age_formatters_keep_operator_text_compact() {
        assert_eq!(human_age(42), "42s ago");
        assert_eq!(human_age(180), "3m ago");
        assert_eq!(human_age(7_500), "2h 5m ago");
        assert_eq!(human_age(180_000), "2d 2h ago");
        assert_eq!(human_age_duration(42), "42s");
        assert_eq!(human_age_duration(180), "3m");
        assert_eq!(human_age_duration(7_500), "2h 5m");
        assert_eq!(human_age_duration(180_000), "2d 2h");
    }

    #[cfg(any(test, feature = "live-reticulum", all(feature = "live-rns-net", any())))]
    #[test]
    fn rate_formatters_are_human_readable() {
        assert_eq!(human_bytes_per_second(2048, 2.0), "1.00 KiB");
        assert_eq!(human_duration(2.5), "2.5s");
        assert_eq!(human_duration(90.0), "1.5m");
    }
}
