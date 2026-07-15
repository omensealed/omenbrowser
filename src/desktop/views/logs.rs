use iced::widget::{column, text};
use iced::{Element, Length};

use super::super::{
    app_scrollable, format_epoch_ms, section_card, ui_size, wrapped_panel_text, wrapped_text_owned,
    DesktopApp, Message, LOG_VISIBLE_ENTRIES,
};

pub(in crate::desktop) fn logs_view(desktop: &DesktopApp) -> Element<'_, Message> {
    let metrics = desktop.app.structured_log_worker_metrics();
    let mut entries = desktop.app.logs.filtered_entries();
    entries.sort_by_key(|entry| std::cmp::Reverse(entry.epoch_ms));
    let rows =
        entries
            .iter()
            .take(LOG_VISIBLE_ENTRIES)
            .fold(column![].spacing(6), |column, entry| {
                column.push(section_card(
                    format!(
                        "{:?} / {:?} / {}",
                        entry.severity,
                        entry.source,
                        format_epoch_ms(entry.epoch_ms)
                    ),
                    wrapped_text_owned(entry.message.clone(), 14),
                ))
            });

    app_scrollable(
        column![
            text("Logs").size(ui_size(28)),
            section_card(
                "Log Filters",
                column![
                    wrapped_text_owned(
                        format!(
                            "entries={} visible={} severity={:?} source={:?}",
                            desktop.app.logs.entries.len(),
                            entries.len().min(LOG_VISIBLE_ENTRIES),
                            desktop.app.logs.severity_filter,
                            desktop.app.logs.source_filter
                        ),
                        14
                    ),
                    wrapped_text_owned(
                        format!(
                            "writer queue: items={} bytes={} oldest_ms={} dropped={} completed={}",
                            metrics.queued_items,
                            metrics.queued_bytes,
                            metrics.oldest_age_ms,
                            metrics.dropped_records,
                            metrics.completed_records
                        ),
                        14
                    ),
                    wrapped_text_owned(
                        format!(
                            "writer disk: failures={} rotations={} removed={} removal_failures={} unsafe_refused={} truncated_scans={}",
                            metrics.write_failures,
                            metrics.rotations,
                            metrics.removed_files,
                            metrics.removal_failures,
                            metrics.unsafe_paths_refused,
                            metrics.truncated_directory_scans
                        ),
                        14
                    ),
                    wrapped_panel_text("Filter controls remain in the TUI/keybinding layer; this desktop panel is a readable log deck."),
                ]
                .spacing(4),
            ),
            rows,
        ]
        .spacing(12)
        .width(Length::Fill),
    )
    .height(Length::Fill)
    .into()
}
