use iced::widget::{container, row, text, Column};
use iced::{Alignment, Element, Length};

use super::{compact_elapsed_ms, ui_size, wrapped_text_owned, Message};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::desktop) struct RecentActivityDisplayRow {
    pub age: String,
    pub left: String,
    pub middle: String,
    pub right: String,
}

pub(in crate::desktop) trait RecentActivityRow {
    fn epoch_ms(&self) -> u64;

    fn columns(&self) -> (&str, &str, &str);

    fn display_line(&self) -> String {
        let (left, middle, right) = self.columns();
        format!("{left} | {middle} | {right}")
    }

    fn display_line_at(&self, now_epoch_ms: u64) -> String {
        let row = self.display_row_at(now_epoch_ms);
        format!(
            "{} | {} | {} | {}",
            row.age, row.left, row.middle, row.right
        )
    }

    fn display_row_at(&self, now_epoch_ms: u64) -> RecentActivityDisplayRow {
        let (left, middle, right) = self.columns();
        RecentActivityDisplayRow {
            age: format!(
                "{} ago",
                compact_elapsed_ms(now_epoch_ms.saturating_sub(self.epoch_ms()))
            ),
            left: left.to_string(),
            middle: middle.to_string(),
            right: right.to_string(),
        }
    }
}

pub(in crate::desktop) fn recent_activity_column<T>(
    title: &'static str,
    headers: (&'static str, &'static str, &'static str),
    empty_text: &'static str,
    rows: Vec<T>,
    now_epoch_ms: u64,
) -> Column<'static, Message>
where
    T: RecentActivityRow,
{
    let display_rows = recent_activity_display_rows(rows, now_epoch_ms);
    let column = Column::new()
        .push(wrapped_text_owned(title, 13))
        .push(recent_activity_header_row(headers))
        .spacing(4);
    if display_rows.is_empty() {
        return column.push(wrapped_text_owned(empty_text, 13));
    }
    display_rows
        .into_iter()
        .fold(column, |column, row| column.push(recent_activity_row(row)))
}

pub(in crate::desktop) fn recent_activity_display_rows<T>(
    rows: Vec<T>,
    now_epoch_ms: u64,
) -> Vec<RecentActivityDisplayRow>
where
    T: RecentActivityRow,
{
    rows.into_iter()
        .take(4)
        .map(|row| row.display_row_at(now_epoch_ms))
        .collect()
}

pub(in crate::desktop) fn recent_activity_lines<T>(
    title: &'static str,
    headers: (&'static str, &'static str, &'static str),
    empty_text: &'static str,
    rows: Vec<T>,
    now_epoch_ms: u64,
) -> Vec<String>
where
    T: RecentActivityRow,
{
    let mut lines = vec![
        title.to_string(),
        format!("age | {} | {} | {}", headers.0, headers.1, headers.2),
    ];
    if rows.is_empty() {
        lines.push(empty_text.to_string());
        return lines;
    }
    lines.extend(
        recent_activity_display_rows(rows, now_epoch_ms)
            .into_iter()
            .map(|row| {
                format!(
                    "{} | {} | {} | {}",
                    row.age, row.left, row.middle, row.right
                )
            }),
    );
    lines
}

fn recent_activity_header_row(
    headers: (&'static str, &'static str, &'static str),
) -> Element<'static, Message> {
    recent_activity_row_content(
        "age".to_string(),
        headers.0.to_string(),
        headers.1.to_string(),
        headers.2.to_string(),
        12,
    )
}

fn recent_activity_row(row: RecentActivityDisplayRow) -> Element<'static, Message> {
    recent_activity_row_content(row.age, row.left, row.middle, row.right, 13)
}

fn recent_activity_row_content(
    age: String,
    left: String,
    middle: String,
    right: String,
    size: u16,
) -> Element<'static, Message> {
    container(
        row![
            text(age).size(ui_size(size)).width(Length::Fixed(72.0)),
            text(left).size(ui_size(size)).width(Length::FillPortion(2)),
            text(middle)
                .size(ui_size(size))
                .width(Length::FillPortion(1)),
            text(right)
                .size(ui_size(size))
                .width(Length::FillPortion(3)),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    )
    .width(Length::Fill)
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone)]
    struct TestRow {
        left: &'static str,
        middle: &'static str,
        right: &'static str,
    }

    impl RecentActivityRow for TestRow {
        fn epoch_ms(&self) -> u64 {
            1_000
        }

        fn columns(&self) -> (&str, &str, &str) {
            (self.left, self.middle, self.right)
        }
    }

    #[test]
    fn recent_activity_lines_keep_headers_and_empty_text() {
        let empty = recent_activity_lines::<TestRow>(
            "recent activity",
            ("target", "state", "detail"),
            "no rows",
            Vec::new(),
            2_000,
        );
        assert_eq!(
            empty,
            vec![
                "recent activity".to_string(),
                "age | target | state | detail".to_string(),
                "no rows".to_string(),
            ]
        );
    }

    #[test]
    fn recent_activity_lines_are_bounded() {
        let rows = vec![
            TestRow {
                left: "a",
                middle: "open",
                right: "one",
            },
            TestRow {
                left: "b",
                middle: "open",
                right: "two",
            },
            TestRow {
                left: "c",
                middle: "open",
                right: "three",
            },
            TestRow {
                left: "d",
                middle: "open",
                right: "four",
            },
            TestRow {
                left: "e",
                middle: "open",
                right: "five",
            },
        ];

        let lines = recent_activity_lines(
            "recent activity",
            ("target", "state", "detail"),
            "no rows",
            rows,
            3_000,
        );

        assert_eq!(lines.len(), 6);
        assert_eq!(lines[2], "2s ago | a | open | one");
        assert_eq!(lines[5], "2s ago | d | open | four");
    }

    #[test]
    fn recent_activity_lines_saturate_future_timestamps() {
        let lines = recent_activity_lines(
            "recent activity",
            ("target", "state", "detail"),
            "no rows",
            vec![TestRow {
                left: "future",
                middle: "open",
                right: "clock skew",
            }],
            500,
        );

        assert_eq!(lines[2], "0s ago | future | open | clock skew");
    }

    #[test]
    fn recent_activity_display_rows_keep_structured_columns() {
        let rows = recent_activity_display_rows(
            vec![TestRow {
                left: "path",
                middle: "known",
                right: "1 hop",
            }],
            61_000,
        );

        assert_eq!(
            rows,
            vec![RecentActivityDisplayRow {
                age: "1m 0s ago".to_string(),
                left: "path".to_string(),
                middle: "known".to_string(),
                right: "1 hop".to_string(),
            }]
        );
    }
}
