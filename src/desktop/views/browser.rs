use iced::widget::{column, container, row, text, text_input};
use iced::{Element, Length};

use crate::app::BrowserRequestStatus;

use super::super::*;

pub(in crate::desktop) fn request_status_label(status: &BrowserRequestStatus) -> &'static str {
    match status {
        BrowserRequestStatus::Preview => "preview",
        BrowserRequestStatus::Pending => "pending",
        BrowserRequestStatus::Completed => "completed",
        BrowserRequestStatus::Failed => "failed",
    }
}

pub(in crate::desktop) fn request_preview_line(
    tab: &crate::app::BrowserTab,
    preview: &crate::app::BrowserRequestPreview,
) -> String {
    let submission = if preview.fields.is_empty() && preview.request_data.is_empty() {
        None
    } else {
        let field_count = preview
            .request_data
            .keys()
            .filter(|key| key.starts_with("field_"))
            .count()
            .max(
                preview
                    .fields
                    .iter()
                    .filter(|field| !field.contains('='))
                    .count(),
            );
        let variable_count = preview
            .request_data
            .keys()
            .filter(|key| key.starts_with("var_"))
            .count()
            .max(
                preview
                    .fields
                    .iter()
                    .filter(|field| field.contains('='))
                    .count(),
            );
        Some(format!(
            "captured submission: {field_count} field(s), {variable_count} variable(s)"
        ))
    };

    let retry = tab
        .retry_state
        .as_ref()
        .filter(|retry| retry.target == preview.target);
    let action = match (preview.status.clone(), retry) {
        (BrowserRequestStatus::Pending, Some(retry))
            if retry.ready_epoch_ms.is_some()
                && retry.retry_after_epoch_ms <= current_epoch_ms() =>
        {
            "path ready; press Retry if the page does not open automatically".to_string()
        }
        (BrowserRequestStatus::Pending, Some(retry)) if retry.ready_epoch_ms.is_some() => {
            "path ready; waiting briefly before page load".to_string()
        }
        (BrowserRequestStatus::Pending, Some(_)) => {
            "waiting for path evidence before page load".to_string()
        }
        (BrowserRequestStatus::Pending, None) => "request pending".to_string(),
        (BrowserRequestStatus::Failed, Some(retry)) if retry.ready_epoch_ms.is_some() => {
            "request failed; path state is ready for retry".to_string()
        }
        (BrowserRequestStatus::Failed, Some(_)) => {
            "request failed; request path or wait for an announce, then retry".to_string()
        }
        (BrowserRequestStatus::Failed, None) => format!("request failed: {}", preview.detail),
        (BrowserRequestStatus::Preview, _) => preview.detail.clone(),
        (BrowserRequestStatus::Completed, _) => format!("loaded {}", preview.target),
    };

    match submission {
        Some(submission) => format!("{action} | {submission}"),
        None => action,
    }
}

pub(in crate::desktop) fn browser_request_preview_has_path_actions(
    tab: &crate::app::BrowserTab,
    preview: &crate::app::BrowserRequestPreview,
) -> bool {
    tab.retry_state.as_ref().is_some_and(|retry| {
        retry.target == preview.target
            && (matches!(
                preview.status,
                BrowserRequestStatus::Pending | BrowserRequestStatus::Failed
            ))
            && (retry.reason.contains("auto-load when path is known")
                || retry.ready_epoch_ms.is_some()
                || matches!(preview.status, BrowserRequestStatus::Failed))
    })
}

pub(in crate::desktop) fn browser_request_preview_retry_ready(
    tab: &crate::app::BrowserTab,
    preview: &crate::app::BrowserRequestPreview,
) -> bool {
    tab.retry_state.as_ref().is_some_and(|retry| {
        retry.target == preview.target
            && retry.ready_epoch_ms.is_some()
            && retry.retry_after_epoch_ms <= current_epoch_ms()
    })
}

pub(in crate::desktop) fn browser_view_for_tab(
    desktop: &DesktopApp,
    tab_id: TabId,
) -> Element<'_, Message> {
    let Some((index, tab)) = desktop
        .app
        .workspace
        .browser_tabs
        .iter()
        .enumerate()
        .find(|(_, tab)| tab.id == tab_id)
    else {
        return text("This browser tab was closed.")
            .size(ui_size(14))
            .into();
    };
    let toolbar = row![
        tooltip_icon_button(ICON_BACK, "Back", Message::BrowserPaneBack(tab_id)),
        tooltip_icon_button(ICON_FORWARD, "Forward", Message::BrowserPaneForward(tab_id)),
        tooltip_icon_button(ICON_RELOAD, "Reload", Message::ReloadBrowserPane(tab_id)),
        tooltip_warning_icon_button(ICON_STOP, "Stop", Message::StopBrowserPaneTask(tab_id)),
        tooltip_omen_icon_button(
            ICON_REQUEST_PATH,
            "Request Path",
            Message::WarmBrowserPanePath(tab_id)
        ),
        tooltip_icon_button(
            IDENTIFY_ICON,
            "Identify",
            Message::ToggleBrowserPaneIdentify(tab_id)
        ),
        tooltip_icon_button(
            ICON_CAPTURE,
            "Capture",
            Message::CaptureBrowserPaneRender(tab_id)
        ),
        tooltip_icon_button(
            ICON_DIAGNOSTICS,
            "Diagnostics",
            Message::BrowserPanePathDiagnostics(tab_id)
        ),
    ]
    .spacing(8)
    .wrap();

    let request_state = browser_request_state_view_for_tab(tab);
    let warning = browser_live_warning_banner_for_tab(tab_id, tab);
    let active_field_cursor = (index == desktop.app.workspace.active_browser)
        .then(|| desktop.app.active_browser_field_editor())
        .flatten();
    let address = browser_address_row(tab_id, &tab.address_input);

    let metadata_document = tab.session.current_document.as_ref();
    let viewport_background = metadata_document
        .as_ref()
        .and_then(|document| document.metadata.get("bg"))
        .and_then(|color| color_from_style(Some(color.as_str())));
    let page = browser_page_for_tab(desktop, tab_id, tab, active_field_cursor);
    let viewport_border = metadata_document
        .as_ref()
        .and_then(|document| document.metadata.get("fg"))
        .and_then(|color| color_from_style(Some(color.as_str())));
    let browser_body = container(page)
        .style(move |theme| {
            browser_viewport_container_style(theme, viewport_background, viewport_border)
        })
        .padding(8)
        .width(Length::Fill)
        .height(Length::Fill);

    container(column![toolbar, address, request_state, warning, browser_body].spacing(6))
        .padding(8)
        .height(Length::Fill)
        .into()
}

fn browser_address_row<'a>(tab_id: TabId, address_input: &'a str) -> Element<'a, Message> {
    let input: Element<'a, Message> = text_input("destination:/path", address_input)
        .on_input(move |value| Message::BrowserPaneAddressChanged { tab_id, value })
        .on_submit(Message::OpenBrowserPaneAddress(tab_id))
        .width(Length::Fill)
        .into();
    row![
        input,
        omen_button("Open", Message::OpenBrowserPaneAddress(tab_id)),
        subtle_button("Top", Message::BrowserPaneTop(tab_id)),
    ]
    .spacing(8)
    .width(Length::Fill)
    .into()
}

fn browser_page_for_tab<'a>(
    desktop: &'a DesktopApp,
    tab_id: TabId,
    tab: &'a crate::app::BrowserTab,
    active_field_cursor: Option<BrowserFieldEditor>,
) -> Element<'a, Message> {
    let initial_document = desktop
        .app
        .browser_document_for_tab_width(tab, tab.viewport_width.max(1));
    let row_field_cursor = active_field_cursor.clone();
    nomadnet_page_with_row_renderer(
        NomadNetPageProps {
            document: initial_document.as_ref(),
            rendered_rows: None,
            fallback: tab.current_page.as_ref().map(|page| page.markup.as_str()),
            scroll_offset: tab.scroll.offset,
            zoom_percent: tab.micron_zoom_percent,
            focused_control: tab
                .focused_control
                .as_ref()
                .map(|control| control.name.as_str()),
            focused_link: tab.focused_link.as_ref().map(|link| link.target.as_str()),
            field_cursor: active_field_cursor
                .as_ref()
                .map(|editor| (editor.name.as_str(), editor.cursor_byte)),
        },
        move |viewport_width| {
            desktop
                .app
                .browser_rendered_rows_for_tab_width_with_field_cursor(
                    tab,
                    viewport_width,
                    row_field_cursor
                        .as_ref()
                        .map(|editor| (editor.name.as_str(), editor.cursor_byte)),
                )
        },
        move |page| Message::PageForTab { tab_id, page },
    )
}

fn browser_request_state_view_for_tab(tab: &crate::app::BrowserTab) -> Element<'_, Message> {
    let Some(preview) = tab.request_preview.as_ref() else {
        return text("").size(ui_size(1)).into();
    };
    if matches!(preview.status, BrowserRequestStatus::Completed) {
        return text("").size(ui_size(1)).into();
    }
    let status_style = match preview.status {
        BrowserRequestStatus::Pending => warning_container_style,
        BrowserRequestStatus::Failed => warning_container_style,
        BrowserRequestStatus::Preview | BrowserRequestStatus::Completed => status_container_style,
    };

    let show_path_actions = browser_request_preview_has_path_actions(tab, preview);
    let mut body = column![row![
        safe_timeline_text(
            format!(
                "Request {} -> {}",
                request_status_label(&preview.status),
                preview.target
            ),
            14
        ),
        subtle_button("Close", Message::DismissBrowserPaneRequest(tab.id)),
    ]
    .spacing(8)]
    .spacing(3);
    if show_path_actions || !matches!(preview.status, BrowserRequestStatus::Pending) {
        body = body.push(safe_timeline_text(request_preview_line(tab, preview), 12));
    }
    if show_path_actions {
        let mut actions = vec![
            omen_button("Request Path", Message::WarmBrowserPanePath(tab.id)),
            subtle_button("Diag", Message::BrowserPanePathDiagnostics(tab.id)),
        ];
        if browser_request_preview_retry_ready(tab, preview) {
            actions.insert(
                1,
                omen_button("Retry", Message::RetryBrowserPaneAfterPath(tab.id)),
            );
        }
        body = body.push(action_grid(actions, 3));
    }

    container(body)
        .style(status_style)
        .padding(6)
        .width(Length::Fill)
        .into()
}

fn browser_live_warning_banner_for_tab(
    tab_id: TabId,
    tab: &crate::app::BrowserTab,
) -> Element<'_, Message> {
    let Some(warning) = tab.live_warning.as_ref() else {
        return text("").size(ui_size(1)).into();
    };
    let visible = warning
        .visible_page
        .as_deref()
        .unwrap_or("no previous page is visible");
    let actions = action_grid(
        vec![
            subtle_button("Close", Message::DismissBrowserPaneWarning(tab_id)),
            omen_button("Request Path", Message::WarmBrowserPanePath(tab_id)),
            omen_button("Retry", Message::RetryBrowserPaneAfterPath(tab_id)),
            subtle_button("Diag", Message::BrowserPanePathDiagnostics(tab_id)),
        ],
        4,
    );

    container(
        column![
            safe_timeline_text("Live load failed; visible page may be stale", 18),
            safe_timeline_text(format!("target: {}", warning.target), 14),
            safe_timeline_text(format!("visible: {visible}"), 14),
            safe_timeline_text(format!("failure: {}", warning.message), 14),
            safe_timeline_text(format!("next: {}", warning.next_action), 14),
            actions,
        ]
        .spacing(4),
    )
    .style(warning_container_style)
    .padding(10)
    .width(Length::Fill)
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{App, BrowserRequestPreview, BrowserRetryState};

    const FIXTURE_BROWSER_NODE_HASH: &str = "00112233445566778899aabbccddeeff";

    fn app_with_temp_root(name: &str) -> App {
        let root = std::env::temp_dir().join(format!("{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let paths = crate::config::AppPaths::from_root(root);
        paths.ensure().expect("paths");
        App::new(crate::config::AppConfig {
            paths,
            settings: crate::storage::settings::AppSettings::default(),
        })
    }

    fn fixture_browser_node_url() -> String {
        format!("{FIXTURE_BROWSER_NODE_HASH}:/page/index.mu")
    }

    #[test]
    fn browser_request_state_line_summarizes_forwarded_form_data_without_values() {
        let mut app = app_with_temp_root("omenbrowser-rs-desktop-request-preview-line");
        let preview = BrowserRequestPreview {
            target: "mock.node:/submit.mu".into(),
            fields: vec!["nickname".into(), "x=1".into()],
            request_data: std::collections::BTreeMap::from([
                ("field_nickname".into(), "mesh friend".into()),
                ("var_x".into(), "1".into()),
            ]),
            status: BrowserRequestStatus::Pending,
            detail: "request queued".into(),
        };
        app.active_browser_tab_mut().request_preview = Some(preview);
        let tab = app.active_browser_tab();
        let preview = tab.request_preview.as_ref().expect("preview");

        let line = request_preview_line(tab, preview);

        assert_eq!(request_status_label(&preview.status), "pending");
        assert!(line.contains("captured submission"));
        assert!(line.contains("1 field(s)"));
        assert!(line.contains("1 variable(s)"));
        assert!(!line.contains("mesh friend"));
        assert!(!line.contains("field_nickname"));
    }

    #[test]
    fn browser_request_preview_path_actions_follow_retry_state() {
        let mut app = app_with_temp_root("omenbrowser-rs-desktop-request-preview-path-actions");
        let target = fixture_browser_node_url();
        app.active_browser_tab_mut().request_preview = Some(BrowserRequestPreview {
            target: target.clone(),
            fields: Vec::new(),
            request_data: std::collections::BTreeMap::new(),
            status: BrowserRequestStatus::Pending,
            detail: "requesting path before page load".into(),
        });
        let tab = app.active_browser_tab();
        let preview = tab.request_preview.as_ref().expect("preview");
        assert!(!browser_request_preview_has_path_actions(tab, preview));

        app.active_browser_tab_mut()
            .request_preview
            .as_mut()
            .expect("preview")
            .status = BrowserRequestStatus::Failed;
        let tab = app.active_browser_tab();
        let preview = tab.request_preview.as_ref().expect("preview");
        assert!(!browser_request_preview_has_path_actions(tab, preview));
        app.active_browser_tab_mut()
            .request_preview
            .as_mut()
            .expect("preview")
            .status = BrowserRequestStatus::Pending;

        app.active_browser_tab_mut().retry_state = Some(BrowserRetryState {
            target: target.clone(),
            destination_hash: FIXTURE_BROWSER_NODE_HASH.into(),
            reason: "browser navigation path request queued; auto-load when path is known".into(),
            requested_epoch_ms: current_epoch_ms(),
            retry_after_epoch_ms: current_epoch_ms().saturating_add(5_000),
            ready_epoch_ms: None,
            attempts: 0,
        });
        let tab = app.active_browser_tab();
        let preview = tab.request_preview.as_ref().expect("preview");
        assert!(browser_request_preview_has_path_actions(tab, preview));
        assert!(!browser_request_preview_retry_ready(tab, preview));

        app.active_browser_tab_mut()
            .request_preview
            .as_mut()
            .expect("preview")
            .status = BrowserRequestStatus::Failed;
        let tab = app.active_browser_tab();
        let preview = tab.request_preview.as_ref().expect("preview");
        assert!(browser_request_preview_has_path_actions(tab, preview));
        assert!(!browser_request_preview_retry_ready(tab, preview));
        app.active_browser_tab_mut()
            .request_preview
            .as_mut()
            .expect("preview")
            .status = BrowserRequestStatus::Pending;

        app.active_browser_tab_mut()
            .retry_state
            .as_mut()
            .expect("retry")
            .ready_epoch_ms = Some(current_epoch_ms());
        app.active_browser_tab_mut()
            .retry_state
            .as_mut()
            .expect("retry")
            .retry_after_epoch_ms = current_epoch_ms();
        let tab = app.active_browser_tab();
        let preview = tab.request_preview.as_ref().expect("preview");
        assert!(browser_request_preview_has_path_actions(tab, preview));
        assert!(browser_request_preview_retry_ready(tab, preview));

        app.active_browser_tab_mut()
            .retry_state
            .as_mut()
            .expect("retry")
            .reason = "browser path request passed; waiting briefly before page load".into();
        let tab = app.active_browser_tab();
        let preview = tab.request_preview.as_ref().expect("preview");
        assert!(browser_request_preview_has_path_actions(tab, preview));
    }
}
