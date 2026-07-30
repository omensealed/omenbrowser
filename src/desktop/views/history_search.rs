use iced::widget::{column, container, row, text, text_input};
use iced::{Element, Length};

use crate::history_search::LocalHistorySourceFilter;

use super::super::*;

const VISIBLE_SEARCH_RESULTS: usize = 12;

fn source_label(source: LocalHistorySourceFilter) -> &'static str {
    match source {
        LocalHistorySourceFilter::All => "All",
        LocalHistorySourceFilter::Lxmf => "LXMF",
        LocalHistorySourceFilter::OmenChat => "OMENchat",
    }
}

fn result_summary(page: &crate::history_search::LocalHistorySearchPage) -> String {
    let mut summary = format!(
        "{} result(s) · {} item(s) examined",
        page.results.len(),
        page.scanned_items
    );
    if page.scan_limit_reached {
        summary.push_str(" · scan limit reached");
    }
    if page.result_limit_reached {
        summary.push_str(" · result limit reached");
    }
    summary
}

pub(in crate::desktop) fn local_history_search_view(desktop: &DesktopApp) -> Element<'_, Message> {
    let source = source_label(desktop.history_search.source);
    let submit_label = if desktop.history_search.is_active() {
        if desktop.history_search.has_pending() {
            "Replace Queued"
        } else {
            "Queue Latest"
        }
    } else {
        "Search"
    };
    let controls = row![
        text_input(
            "Search local message history",
            &desktop.history_search.draft
        )
        .on_input(
            |value| Message::HistorySearch(Box::new(HistorySearchMessage::QueryChanged(value)))
        )
        .on_submit(Message::HistorySearch(Box::new(
            HistorySearchMessage::SubmitCurrent
        )))
        .padding(7)
        .width(Length::Fill),
        subtle_button_owned(
            format!("Source: {source}"),
            Message::HistorySearch(Box::new(HistorySearchMessage::CycleSource)),
        ),
        omen_button(
            submit_label,
            Message::HistorySearch(Box::new(HistorySearchMessage::SubmitCurrent)),
        ),
    ]
    .spacing(8);

    let mut body = column![controls].spacing(6);
    if let Some(error) = &desktop.history_search.error {
        body = body.push(
            text(format!("Search failed: {error}"))
                .size(ui_size(12))
                .color(iced::Color::from_rgb8(230, 70, 76)),
        );
    } else if let Some(page) = &desktop.history_search.result {
        body = body.push(text(result_summary(page)).size(ui_size(12)));
        let rows = page.results.iter().take(VISIBLE_SEARCH_RESULTS).fold(
            column![].spacing(4),
            |rows, result| {
                rows.push(
                    container(
                        row![
                            column![
                                text(format!("{} · {}", result.sender, result.context))
                                    .size(ui_size(12)),
                                text(&result.excerpt).size(ui_size(12)),
                            ]
                            .spacing(2)
                            .width(Length::Fill),
                            subtle_button(
                                "Open",
                                Message::HistorySearch(Box::new(HistorySearchMessage::Jump(
                                    result.key.clone()
                                ))),
                            ),
                        ]
                        .spacing(8),
                    )
                    .padding([4, 6])
                    .style(status_container_style),
                )
            },
        );
        body = body.push(
            app_scrollable(rows)
                .height(Length::Fixed(180.0))
                .width(Length::Fill),
        );
        if page.results.len() > VISIBLE_SEARCH_RESULTS {
            body = body.push(
                text(format!(
                    "Showing newest {VISIBLE_SEARCH_RESULTS} of {} retained results",
                    page.results.len()
                ))
                .size(ui_size(12)),
            );
        }
    } else {
        body = body
            .push(text("Explicit search only; typing does not scan storage.").size(ui_size(12)));
    }
    section_card("Local History Search", body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presentation_labels_sources_and_every_limit_truthfully() {
        assert_eq!(source_label(LocalHistorySourceFilter::All), "All");
        assert_eq!(source_label(LocalHistorySourceFilter::Lxmf), "LXMF");
        assert_eq!(source_label(LocalHistorySourceFilter::OmenChat), "OMENchat");
        let page = crate::history_search::LocalHistorySearchPage {
            scanned_items: 8_192,
            scan_limit_reached: true,
            result_limit_reached: true,
            ..crate::history_search::LocalHistorySearchPage::default()
        };
        assert_eq!(
            result_summary(&page),
            "0 result(s) · 8192 item(s) examined · scan limit reached · result limit reached"
        );
    }
}
