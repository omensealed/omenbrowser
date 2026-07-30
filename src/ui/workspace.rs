use ratatui::layout::{Alignment as TuiAlignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap};
use ratatui::Frame;

use crate::app::{App, BrowserOverlayMode, SettingsAction};
use crate::browser::PageSource;
use crate::input::{InputTarget, InterfaceEditField};
use crate::interfaces::InterfaceKind;
use crate::micron::render::{document_to_lines_with_focus_and_cursor, rendered_rows_to_lines};
use crate::plugins::BUILTIN_MICRONPLUS_PLUGIN_ID;
use crate::storage::settings::RuntimeBackendSetting;
use crate::ui::{operations, status, tabs};
pub use crate::workspace::{FocusArea, WorkspaceSection};

pub fn render(frame: &mut Frame, app: &App) {
    let root = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(10),
            Constraint::Length(2),
        ])
        .split(frame.area());

    render_header(frame, root[0], app);
    render_body(frame, root[1], app);
    render_footer(frame, root[2], app);

    if app.workspace.show_help {
        render_help(frame, centered_rect(74, 60, frame.area()), app);
    }
}

fn render_header(frame: &mut Frame, area: Rect, app: &App) {
    let nav = WorkspaceSection::ALL
        .iter()
        .map(|section| {
            let style = if *section == app.workspace.active_section {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Gray)
            };
            Span::styled(format!(" {} ", section.label()), style)
        })
        .collect::<Vec<_>>();
    let title = Paragraph::new(Line::from(nav))
        .block(
            Block::default()
                .title(" OMENbrowser_rs ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan)),
        )
        .alignment(TuiAlignment::Center);
    frame.render_widget(title, area);
}

fn render_body(frame: &mut Frame, area: Rect, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(28), Constraint::Min(20)])
        .split(area);
    render_sidebar(frame, chunks[0], app);
    render_workspace(frame, chunks[1], app);
}

fn render_sidebar(frame: &mut Frame, area: Rect, app: &App) {
    let mut items = WorkspaceSection::ALL
        .iter()
        .map(|section| ListItem::new(section.label()))
        .collect::<Vec<_>>();
    items.push(ListItem::new(" "));
    items.extend(
        app.workspace
            .browser_tabs
            .iter()
            .map(|tab| ListItem::new(format!("B:{} {}", tab.id, tab.title))),
    );
    items.extend(app.workspace.conversations.iter().map(|conversation| {
        ListItem::new(format!("M:{} {}", conversation.id, conversation.peer_label))
    }));

    let focus_style = if app.workspace.focus == FocusArea::Sidebar {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let list = List::new(items).block(
        Block::default()
            .title(" Navigation ")
            .borders(Borders::ALL)
            .border_style(focus_style),
    );
    frame.render_widget(list, area);
}

fn render_workspace(frame: &mut Frame, area: Rect, app: &App) {
    match app.workspace.active_section {
        WorkspaceSection::Browser => render_browser(frame, area, app),
        WorkspaceSection::Messages => render_messages(frame, area, app),
        WorkspaceSection::Directory => render_directory(frame, area, app),
        WorkspaceSection::Identities => {
            render_placeholder(frame, area, app.workspace.active_section)
        }
        WorkspaceSection::Interfaces => render_interfaces(frame, area, app),
        WorkspaceSection::Monitoring => {
            render_placeholder(frame, area, app.workspace.active_section)
        }
        WorkspaceSection::NetworkDoctor => operations::render(frame, area, app),
        WorkspaceSection::Settings => render_settings(frame, area, app),
        WorkspaceSection::Diagnostics => render_diagnostics(frame, area, app),
        WorkspaceSection::Logs => render_logs(frame, area, app),
        WorkspaceSection::Plugins => render_plugins(frame, area, app),
        WorkspaceSection::Help => render_placeholder(frame, area, app.workspace.active_section),
    }
}

fn render_browser(frame: &mut Frame, area: Rect, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Length(5),
            Constraint::Length(3),
            Constraint::Length(4),
            Constraint::Min(5),
        ])
        .split(area);
    frame.render_widget(
        Paragraph::new(tabs::browser_tab_line(
            &app.workspace.browser_tabs,
            app.workspace.active_browser,
        )),
        chunks[0],
    );
    render_native_setup_strip(frame, chunks[1], app);

    let tab = app.active_browser_tab();
    let focus = if app.workspace.focus == FocusArea::Command {
        "FOCUSED"
    } else {
        "Ctrl-L to edit"
    };
    let loading = tab
        .loading
        .as_ref()
        .map(|loading| format!(" | loading {}", loading.target))
        .unwrap_or_default();
    let transfer = tab
        .transfer_status
        .as_ref()
        .map(|status| format!(" | {status}"))
        .unwrap_or_default();
    let path_warmup = tab
        .path_warmup
        .as_ref()
        .map(|warmup| format!(" | warming path {}", warmup.target))
        .unwrap_or_default();
    let retry = tab
        .retry_state
        .as_ref()
        .map(|retry| {
            let state = if retry.ready_epoch_ms.is_some() {
                "retry ready"
            } else {
                "retry pending"
            };
            format!(" | {state}: {}", retry.target)
        })
        .unwrap_or_default();
    let partials = if tab.partials.specs.is_empty() {
        "partials: none".to_string()
    } else {
        format!(
            "partials: {} specs, {} pending",
            tab.partials.specs.len(),
            tab.partials.pending
        )
    };
    let control = tab
        .focused_control
        .as_ref()
        .map(|control| format!(" | focused control: {}", control.name))
        .or_else(|| {
            tab.focused_link
                .as_ref()
                .map(|link| format!(" | focused link: {}", link.target))
        })
        .unwrap_or_default();
    let probe = tab
        .probe_summary
        .as_ref()
        .map(|probe| {
            format!(
                " | probe {}: {}{} - {}",
                probe.mode,
                probe.status,
                if probe.ready_to_request { " ready" } else { "" },
                probe.detail
            )
        })
        .unwrap_or_default();
    let micronplus = app.micronplus_status_for_active_page();
    let address_text = app
        .input
        .active
        .as_ref()
        .and_then(|active| match active.target {
            InputTarget::BrowserAddress { tab_id } if tab_id == tab.id => {
                Some(active.buffer.display_with_cursor())
            }
            _ => None,
        })
        .unwrap_or_else(|| tab.address_input.clone());
    let address = Paragraph::new(format!(
        "Address ({focus}): {}{}{}{}{}\nControls: Alt-Left Back | Alt-Right Forward | Ctrl-R Reload | R Retry | Ctrl-F Partials | Ctrl-D Download | N Probe | D Path | PgUp/PgDn Scroll | Tab Focus | Enter/Space Activate | o overlays | O expand | Esc Stop | {partials} | {micronplus}{control}{probe}",
        address_text,
        loading,
        transfer,
        path_warmup,
        retry
    ))
    .block(Block::default().borders(Borders::ALL).title(" Browser "));
    frame.render_widget(address, chunks[2]);
    render_browser_result_strip(frame, chunks[3], app);

    let width = chunks[4].width.saturating_sub(2).max(1) as usize;
    let field_cursor = app
        .input
        .active
        .as_ref()
        .and_then(|active| match &active.target {
            InputTarget::BrowserField { tab_id, name } if *tab_id == tab.id => {
                Some((name.as_str(), active.buffer.cursor()))
            }
            _ => None,
        });
    let content_lines = app
        .browser_rendered_rows_for_tab_width_with_field_cursor(tab, width, field_cursor)
        .map(|rows| {
            rendered_rows_to_lines(
                rows,
                tab.focused_control
                    .as_ref()
                    .map(|control| control.name.as_str()),
                tab.focused_link.as_ref().map(|link| link.target.as_str()),
            )
        })
        .or_else(|| {
            tab.session.current_document.as_ref().map(|document| {
                document_to_lines_with_focus_and_cursor(
                    document,
                    width,
                    tab.focused_control
                        .as_ref()
                        .map(|control| control.name.as_str()),
                    tab.focused_link.as_ref().map(|link| link.target.as_str()),
                    field_cursor,
                )
            })
        })
        .unwrap_or_else(|| vec![Line::from("No page loaded")]);
    let content = Paragraph::new(content_lines)
        .block(Block::default().borders(Borders::ALL).title(" Micron "))
        .scroll((tab.scroll.offset.min(u16::MAX as usize) as u16, 0))
        .wrap(Wrap { trim: false });
    frame.render_widget(content, chunks[4]);
    let overlay_mode = app.workspace.browser_overlay_mode;
    if overlay_mode.shows_status() && chunks[4].height >= 4 && chunks[4].width >= 20 {
        let status_overlay = app.browser_status_overlay();
        let max_height = if overlay_mode == BrowserOverlayMode::Expanded {
            chunks[4].height
        } else {
            chunks[4].height.min(11)
        };
        let status_height = (status_overlay.lines.len() as u16 + 2).min(max_height);
        let status_width = if overlay_mode == BrowserOverlayMode::Expanded {
            chunks[4].width
        } else {
            chunks[4].width.min(76)
        };
        let status_area = Rect {
            x: chunks[4].x,
            y: chunks[4].y,
            width: status_width,
            height: status_height,
        };
        frame.render_widget(Clear, status_area);
        frame.render_widget(
            Paragraph::new(
                status_overlay
                    .lines
                    .into_iter()
                    .map(Line::from)
                    .collect::<Vec<_>>(),
            )
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(status_overlay.title)
                    .border_style(Style::default().fg(Color::Blue)),
            )
            .wrap(Wrap { trim: false }),
            status_area,
        );
    }
    if overlay_mode.shows_focus() {
        if chunks[4].height < 3 || chunks[4].width < 20 {
            return;
        }
        let Some(preview) = app.browser_focus_preview() else {
            return;
        };
        let height = (preview.lines.len() as u16 + 2).min(chunks[4].height.min(8));
        let width = chunks[4].width.min(72);
        let area = Rect {
            x: chunks[4].x + chunks[4].width.saturating_sub(width),
            y: chunks[4].y + chunks[4].height.saturating_sub(height),
            width,
            height,
        };
        frame.render_widget(Clear, area);
        frame.render_widget(
            Paragraph::new(
                preview
                    .lines
                    .into_iter()
                    .map(Line::from)
                    .collect::<Vec<_>>(),
            )
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(preview.title)
                    .border_style(Style::default().fg(Color::Yellow)),
            )
            .wrap(Wrap { trim: false }),
            area,
        );
    }
}

fn render_native_setup_strip(frame: &mut Frame, area: Rect, app: &App) {
    let identity_state = app
        .settings
        .identity_path
        .as_ref()
        .filter(|path| path.is_file())
        .map(|path| {
            format!(
                "ok: {} ({})",
                app.settings
                    .active_identity_label
                    .as_deref()
                    .unwrap_or("identity"),
                path.display()
            )
        })
        .unwrap_or_else(|| "missing".into());
    let enabled_tcp_clients = app
        .interface_service
        .list_profiles()
        .iter()
        .filter(|profile| profile.enabled && profile.kind == InterfaceKind::TcpClient)
        .count();
    let runtime_state = if app.runtime_startup_pending() {
        "starting"
    } else if app.runtime_status.connected {
        "connected"
    } else {
        "stopped"
    };
    let lines = vec![
        Line::from(vec![
            Span::styled(
                "Setup: ",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("[I] create identity  "),
            Span::raw("[G] setup/start native  "),
            Span::raw("[F4] interfaces/gateways  "),
            Span::raw("[F6] diagnostics"),
        ]),
        Line::from(format!(
            "backend={:?} | runtime={runtime_state} | identity={identity_state}",
            app.runtime_status.backend
        )),
        Line::from(format!(
            "gateways: {enabled_tcp_clients} enabled TCP gateway(s) | current task: {}",
            app.status.task
        )),
    ];
    frame.render_widget(
        Paragraph::new(lines)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Native Reticulum Setup "),
            )
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn render_browser_result_strip(frame: &mut Frame, area: Rect, app: &App) {
    let tab = app.active_browser_tab();
    let (source, url) = tab
        .current_page
        .as_ref()
        .map(|page| (page_source_label(&page.source), page.url.as_str()))
        .unwrap_or(("none", "none"));
    let load_state = tab
        .loading
        .as_ref()
        .map(|loading| format!("loading {}", loading.target))
        .unwrap_or_else(|| "idle".into());
    let transfer = tab
        .transfer_status
        .as_deref()
        .map(|status| format!(" | {status}"))
        .unwrap_or_default();
    let probe = tab
        .probe_summary
        .as_ref()
        .map(|probe| format!("probe={} {} - {}", probe.mode, probe.status, probe.detail))
        .unwrap_or_else(|| "probe=none".into());
    let style = if app.status.task.contains("failed")
        || app.status.task.contains("cannot")
        || app.status.task.contains("mock runtime cannot")
        || app.status.task.contains("not known")
        || app.status.task.contains("blocked")
    {
        Style::default().fg(Color::Red)
    } else if source == "live network" {
        Style::default().fg(Color::Green)
    } else if source == "mock" {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
    };
    let lines = vec![
        Line::from(format!(
            "result={source} | state={load_state}{transfer} | url={url}"
        )),
        Line::from(format!("status: {}", app.status.task)),
        Line::from(probe),
    ];
    frame.render_widget(
        Paragraph::new(lines).style(style).block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Browser Result "),
        ),
        area,
    );
}

fn page_source_label(source: &PageSource) -> &'static str {
    match source {
        PageSource::Cache => "cache",
        PageSource::Network => "live network",
        PageSource::Mock => "mock",
    }
}

fn render_messages(frame: &mut Frame, area: Rect, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Min(5),
            Constraint::Length(5),
        ])
        .split(area);
    frame.render_widget(
        Paragraph::new(tabs::conversation_tab_line(
            &app.workspace.conversations,
            app.workspace.active_conversation,
        )),
        chunks[0],
    );

    let conversation = app.active_conversation();
    let composer_controls = conversation
        .direct_stamp_confirmation
        .as_ref()
        .map(|confirmation| {
            format!(
                "STAMP COST {} > {}: Ctrl-A Confirm | Ctrl-X Cancel | no message sent",
                confirmation.advertised_cost, confirmation.ask_above
            )
        })
        .unwrap_or_else(|| {
            "Ctrl-Y Title | Ctrl-E Body | Ctrl-P Mode | Ctrl-U Ticket | Ctrl-S Send | Ctrl-G Sync | Enter Commit | Esc Cancel".into()
        });
    let body = if conversation.thread.messages.is_empty() {
        "No messages yet in this mock conversation.".to_string()
    } else {
        conversation
            .thread
            .messages
            .iter()
            .map(|message| format!("{}: {}", message.peer_label, message.content))
            .collect::<Vec<_>>()
            .join("\n")
    };
    frame.render_widget(
        Paragraph::new(body).block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Conversation "),
        ),
        chunks[1],
    );
    frame.render_widget(
        Paragraph::new(format!(
            "Title: {}\nBody: {}\nMode: {:?} | Ticket: {} | Pending: {}\n{}",
            composer_title(app),
            composer_body(app),
            conversation.delivery_mode,
            conversation.include_ticket,
            conversation.pending_send.is_some(),
            composer_controls
        ))
        .block(Block::default().borders(Borders::ALL).title(" Composer ")),
        chunks[2],
    );
}

fn composer_title(app: &App) -> String {
    let conversation = app.active_conversation();
    app.input
        .active
        .as_ref()
        .and_then(|active| match active.target {
            InputTarget::MessageTitle { conversation_id } if conversation_id == conversation.id => {
                Some(active.buffer.display_with_cursor())
            }
            _ => None,
        })
        .unwrap_or_else(|| conversation.draft_title.clone())
}

fn composer_body(app: &App) -> String {
    let conversation = app.active_conversation();
    app.input
        .active
        .as_ref()
        .and_then(|active| match active.target {
            InputTarget::MessageBody { conversation_id } if conversation_id == conversation.id => {
                Some(active.buffer.display_with_cursor())
            }
            _ => None,
        })
        .unwrap_or_else(|| conversation.draft_body.clone())
}

fn render_directory(frame: &mut Frame, area: Rect, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(8), Constraint::Length(7)])
        .split(area);
    let propagation_inventory = app.propagation_node_inventory();
    let rows = app
        .directory_state
        .entries
        .iter()
        .enumerate()
        .map(|(index, entry)| {
            let marker = if app.directory_state.selected == Some(index) {
                ">"
            } else {
                " "
            };
            let micronplus = app.micronplus_status_for_directory_entry(entry);
            let propagation_health = propagation_inventory
                .nodes
                .iter()
                .find(|node| {
                    node.destination_hash
                        .eq_ignore_ascii_case(&entry.destination_hash)
                })
                .map(|node| {
                    format!(
                        " | selection={:?} freshness={:?} age={} path={:?} evidence={:?} cost={} compatibility={:?}",
                        node.selection,
                        node.freshness,
                        node.announce_age_seconds
                            .map(|age| format!("{age}s"))
                            .unwrap_or_else(|| "unknown".into()),
                        node.path_state,
                        node.evidence,
                        node.advertised_stamp_cost
                            .map(|cost| cost.to_string())
                            .unwrap_or_else(|| "unknown".into()),
                        node.compatibility
                    )
                })
                .unwrap_or_default();
            let item = ListItem::new(format!(
                "{marker} [open] [save] [trust] {} | {:?} | saved={} trusted={} | {:?} | {} | {}{}",
                entry.destination_hash,
                entry.kind,
                entry.saved,
                entry.trusted,
                entry.trust_level,
                micronplus,
                entry.display_name,
                propagation_health
            ));
            if app.directory_state.selected == Some(index) {
                item.style(Style::default().fg(Color::Black).bg(Color::Cyan))
            } else {
                item
            }
        })
        .collect::<Vec<_>>();
    let title = format!(
        " Directory | filter: {} | entries: {} | propagation={}/{} bytes={} truncated={} refresh={:?} | Enter open | d path | s save | t trust | k ticket | r refresh | x cancel | p use | g sync ",
        app.directory_state.filter,
        app.directory_state.entries.len(),
        propagation_inventory.nodes.len(),
        propagation_inventory.total_candidates,
        propagation_inventory.retained_bytes,
        propagation_inventory.truncated,
        app.directory_state.propagation_refresh.outcome
    );
    frame.render_widget(
        List::new(rows).block(Block::default().borders(Borders::ALL).title(title)),
        chunks[0],
    );

    let detail = app
        .selected_directory_entry()
        .map(|entry| {
            let mut lines = vec![
                Line::from(format!("{} | {:?}", entry.display_name, entry.kind)),
                Line::from(format!(
                    "destination={} saved={} trusted={} trust={:?}",
                    entry.destination_hash, entry.saved, entry.trusted, entry.trust_level
                )),
                Line::from(app.micronplus_status_for_directory_entry(&entry)),
            ];
            lines.extend(
                app.micronplus_warning_lines_for_directory_entry(&entry)
                    .into_iter()
                    .map(Line::from),
            );
            if entry.kind == crate::directory::DirectoryKind::Peer {
                lines.push(Line::from(format!(
                    "reply ticket default={}",
                    match entry.offer_reply_ticket {
                        Some(true) => "offer",
                        Some(false) => "do not offer",
                        None => "default (off)",
                    }
                )));
            }
            if let Some(node) = propagation_inventory.nodes.iter().find(|node| {
                node.destination_hash
                    .eq_ignore_ascii_case(&entry.destination_hash)
            }) {
                lines.push(Line::from(format!(
                    "propagation selection={:?} freshness={:?} age={} path={:?} evidence={:?} compatibility={:?}",
                    node.selection,
                    node.freshness,
                    node.announce_age_seconds
                        .map(|age| format!("{age}s"))
                        .unwrap_or_else(|| "unknown".into()),
                    node.path_state,
                    node.evidence,
                    node.compatibility
                )));
                lines.push(Line::from(format!(
                    "identity={} name_authenticated={} stamp_cost={}",
                    node.identity_hash.as_deref().unwrap_or("unknown"),
                    node.display_name_authenticated,
                    node.advertised_stamp_cost
                        .map(|cost| cost.to_string())
                        .unwrap_or_else(|| "unknown".into())
                )));
                lines.push(Line::from(format!(
                    "refresh={:?} observed={:?} cooldown_snapshot={} sync={:?}",
                    node.refresh,
                    node.refresh_observed_epoch_ms,
                    node.refresh_cooldown_remaining_seconds
                        .map(|seconds| format!("{seconds}s"))
                        .unwrap_or_else(|| "ready".into()),
                    node.sync
                )));
                lines.push(Line::from(format!(
                    "last_sync={:?} last_successful_sync={:?}",
                    node.last_sync_epoch_ms, node.last_successful_sync_epoch_ms
                )));
                if let Some(error) = &node.last_sync_error {
                    lines.push(Line::from(format!("last_sync_error={error}")));
                }
            }
            lines
        })
        .unwrap_or_else(|| vec![Line::from("No directory entry selected")]);
    frame.render_widget(
        Paragraph::new(detail)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Directory Detail | MicronPlus node warnings "),
            )
            .wrap(Wrap { trim: false }),
        chunks[1],
    );
}

fn render_interfaces(frame: &mut Frame, area: Rect, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);
    let readiness = app.native_interface_readiness();
    let blockers = readiness
        .iter()
        .filter(|detail| detail.blocks_native_startup)
        .count();
    let rows = app
        .interfaces_state
        .profiles
        .iter()
        .zip(readiness.iter())
        .enumerate()
        .map(|(index, (profile, readiness))| {
            let marker = if app.interfaces_state.selected == Some(index) {
                ">"
            } else {
                " "
            };
            let item = ListItem::new(format!(
                "{marker} [toggle] [delete] {} | {:?} | enabled={} | {}:{} | native={}{}",
                profile.name,
                profile.kind,
                profile.enabled,
                profile.target_host,
                profile.target_port,
                if readiness.supported {
                    "supported"
                } else {
                    "not-wired"
                },
                if readiness.blocks_native_startup {
                    " BLOCKS"
                } else {
                    ""
                }
            ));
            if app.interfaces_state.selected == Some(index) {
                item.style(Style::default().fg(Color::Black).bg(Color::Cyan))
            } else {
                item
            }
        })
        .collect::<Vec<_>>();
    let title = format!(
        " Interfaces | native blockers: {blockers} | G native setup | 1 RMAP | 2 WNS | a TCP gateway | i I2P | v RNode | n name | x delete | e toggle | h/p TCP | P preview | E export "
    );
    frame.render_widget(
        List::new(rows).block(Block::default().borders(Borders::ALL).title(title)),
        chunks[0],
    );

    let detail = selected_interface_detail(app, &readiness);
    frame.render_widget(
        Paragraph::new(detail)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Interface Detail "),
            )
            .wrap(Wrap { trim: false }),
        chunks[1],
    );
}

fn selected_interface_detail(
    app: &App,
    readiness: &[crate::app::NativeInterfaceReadiness],
) -> Vec<Line<'static>> {
    let Some(index) = app.interfaces_state.selected else {
        return vec![Line::from("No interface selected")];
    };
    let Some(profile) = app.interfaces_state.profiles.get(index) else {
        return vec![Line::from("Selected interface is unavailable")];
    };
    let Some(readiness) = readiness.get(index) else {
        return vec![Line::from("Native readiness detail is unavailable")];
    };

    let name = active_interface_value(app, &profile.profile_id, InterfaceEditField::ProfileName)
        .unwrap_or_else(|| profile.name.clone());
    let host = active_interface_value(app, &profile.profile_id, InterfaceEditField::TcpHost)
        .unwrap_or_else(|| profile.target_host.clone());
    let port = active_interface_value(app, &profile.profile_id, InterfaceEditField::TcpPort)
        .unwrap_or_else(|| profile.target_port.to_string());
    let ifac_network = active_interface_value(
        app,
        &profile.profile_id,
        InterfaceEditField::TcpIfacNetworkName,
    )
    .unwrap_or_else(|| profile.network_name.clone());
    let passphrase_edit_active = app.input.active.as_ref().is_some_and(|active| {
        matches!(
            &active.target,
            InputTarget::InterfaceField {
                profile_id,
                field: InterfaceEditField::TcpIfacPassphrase,
            } if profile_id == &profile.profile_id
        )
    });
    let ifac_passphrase =
        masked_passphrase_status(passphrase_edit_active, !profile.passphrase.is_empty());
    let peers = active_interface_value(app, &profile.profile_id, InterfaceEditField::I2pPeers)
        .unwrap_or_else(|| profile.peers.join(", "));
    let device_port = active_interface_value(
        app,
        &profile.profile_id,
        InterfaceEditField::RNodeDevicePort,
    )
    .unwrap_or_else(|| profile.device_port.clone());
    let frequency =
        active_interface_value(app, &profile.profile_id, InterfaceEditField::RNodeFrequency)
            .unwrap_or_else(|| profile.frequency.to_string());
    let bandwidth =
        active_interface_value(app, &profile.profile_id, InterfaceEditField::RNodeBandwidth)
            .unwrap_or_else(|| profile.bandwidth.to_string());
    let tx_power =
        active_interface_value(app, &profile.profile_id, InterfaceEditField::RNodeTxPower)
            .unwrap_or_else(|| profile.tx_power.to_string());
    let spreading_factor = active_interface_value(
        app,
        &profile.profile_id,
        InterfaceEditField::RNodeSpreadingFactor,
    )
    .unwrap_or_else(|| profile.spreading_factor.to_string());
    let coding_rate = active_interface_value(
        app,
        &profile.profile_id,
        InterfaceEditField::RNodeCodingRate,
    )
    .unwrap_or_else(|| profile.coding_rate.to_string());

    let mut lines = vec![
        Line::from(format!("name: {name}")),
        Line::from(format!("profile id: {}", profile.profile_id)),
        Line::from(format!(
            "kind: {:?} | enabled: {}",
            profile.kind, profile.enabled
        )),
        Line::from(
            "scope: controls write managed config; changes activate on next runtime start/restart (live mutation not negotiated)",
        ),
        Line::from(
            "create: [gateway] custom TCP | [1] RMAP gateway | [2] WNS gateway | [i2p] I2P | [rnode] RNode",
        ),
        Line::from("selected: [rename] name | [toggle] enabled | [delete] remove"),
        Line::from(format!(
            "tcp: [host] edit | [port] edit | value: {host}:{port}"
        )),
        Line::from(format!(
            "tcp IFAC: [ifac-name] edit | [ifac-pass] edit | network={} passphrase={}",
            if ifac_network.is_empty() {
                "not set"
            } else {
                ifac_network.as_str()
            },
            ifac_passphrase
        )),
        Line::from(format!(
            "i2p: [connect] toggle | [peers] edit | connectable={} peers={peers}",
            profile.connectable
        )),
        Line::from(format!("native supported: {}", readiness.supported)),
        Line::from(format!(
            "blocks native startup: {}",
            readiness.blocks_native_startup
        )),
        Line::from(format!("reason: {}", readiness.reason)),
        Line::from(format!(
            "rnode: [device] edit | [freq] edit | [bw] edit | [tx] edit | [sf] edit | [cr] edit | values: {device_port} {frequency} {bandwidth} {tx_power} {spreading_factor} {coding_rate}"
        )),
        {
            let warning = if readiness.warnings.is_empty() {
                "none".to_string()
            } else {
                readiness.warnings.join(" | ")
            };
            Line::from(format!("warnings: {warning}"))
        },
        Line::from("config: [preview] generated | [export] diagnostics copy"),
    ];

    if let Some(path) = &app.interfaces_state.last_config_export_path {
        lines.push(Line::from(format!("config export: {}", path.display())));
    } else {
        lines.push(Line::from("config export: none"));
    }

    if let Some(preview) = &app.interfaces_state.config_preview {
        lines.push(Line::from(format!(
            "preview: {} lines",
            preview.lines().count()
        )));
        lines.extend(
            preview
                .lines()
                .take(8)
                .map(|line| Line::from(format!("  {line}"))),
        );
    } else {
        lines.push(Line::from("preview: none"));
    }

    lines
}

fn active_interface_value(
    app: &App,
    profile_id: &str,
    field: InterfaceEditField,
) -> Option<String> {
    let active = app.input.active.as_ref()?;
    match &active.target {
        InputTarget::InterfaceField {
            profile_id: active_profile_id,
            field: active_field,
        } if active_profile_id == profile_id && *active_field == field => {
            Some(active.buffer.display_with_cursor())
        }
        _ => None,
    }
}

fn masked_passphrase_status(edit_active: bool, configured: bool) -> &'static str {
    if edit_active {
        "editing (hidden)"
    } else if configured {
        "configured"
    } else {
        "not set"
    }
}

fn render_settings(frame: &mut Frame, area: Rect, app: &App) {
    let native_readiness = app.native_reticulum_readiness();
    let runtime_confirm = app
        .input
        .active
        .as_ref()
        .and_then(|active| match &active.target {
            InputTarget::RuntimeBackendConfirm { backend } => Some(format!(
                "confirm backend {:?}: submit {}",
                backend,
                active.buffer.display_with_cursor()
            )),
            _ => None,
        });
    let enabled_tcp_clients = app
        .interface_service
        .list_profiles()
        .iter()
        .filter(|profile| profile.enabled && profile.kind == InterfaceKind::TcpClient)
        .count();
    let runtime_state = if app.runtime_startup_pending() {
        "starting"
    } else if app.runtime_status.connected {
        "connected"
    } else {
        "stopped"
    };
    let mut content = vec![
        Line::from("FIRST RUN / NATIVE SETUP"),
        Line::from("[I] create managed identity | [G] auto setup + start | [F4] add/edit gateways | [3] choose Reticulum backend"),
        Line::from(format!(
            "setup state: backend={:?} runtime={runtime_state} identity={} tcp_gateways={enabled_tcp_clients}",
            app.runtime_status.backend,
            app.settings
                .active_identity_label
                .as_deref()
                .unwrap_or("none")
        )),
        Line::from(""),
        settings_action_line(
            app,
            0,
            format!(
                "[edit theme] theme: {}",
                settings_input_value(
                    app,
                    InputTarget::SettingsThemeName,
                    app.settings.ui.theme_name.clone(),
                )
            ),
        ),
        settings_action_line(
            app,
            1,
            format!(
                "[edit home] home: {}",
                settings_input_value(
                    app,
                    InputTarget::SettingsDefaultStartPage,
                    app.settings.default_start_page.clone(),
                )
            ),
        ),
        settings_action_line(
            app,
            settings_action_index(SettingsAction::ToggleReducedMotion),
            format!(
                "[reduced motion] {} | desktop animated previews={} | TUI animation=none",
                app.settings.ui.reduce_motion,
                if cfg!(feature = "chat-client-gif") {
                    if app.settings.ui.reduce_motion {
                        "static frame"
                    } else {
                        "enabled"
                    }
                } else {
                    "not compiled"
                }
            ),
        ),
        Line::from(format!(
            "runtime backend: {:?}{}",
            app.settings.runtime_backend,
            if app.settings.restart_required {
                " | restart required"
            } else {
                ""
            }
        )),
        Line::from(format!(
            "native Reticulum readiness: compiled={} configured={} ready={} | {}",
            native_readiness.compiled,
            native_readiness.configured,
            native_readiness.ready,
            native_readiness.summary
        )),
        Line::from(runtime_confirm.unwrap_or_else(|| {
            "native Reticulum guard: selecting while not ready requires typed confirmation".into()
        })),
        runtime_backend_action_line(app, RuntimeBackendSetting::Auto),
        runtime_backend_action_line(app, RuntimeBackendSetting::Mock),
        runtime_backend_action_line(app, RuntimeBackendSetting::Reticulum),
        runtime_backend_action_line(app, RuntimeBackendSetting::Bridge),
        settings_action_line(
            app,
            settings_action_index(SettingsAction::CycleReticulumMode),
            format!(
                "[reticulum mode] {:?} | external/shared: deferred | native feature: {}",
                app.settings.reticulum_instance_mode,
                if cfg!(feature = "native-reticulum") {
                    "available"
                } else {
                    "not compiled"
                }
            ),
        ),
        settings_action_line(
            app,
            settings_action_index(SettingsAction::RestartToApply),
            if app.settings.restart_required {
                format!(
                    "[restart to apply] pending: {}",
                    app.restart_pending_summary().join(" | ")
                )
            } else {
                "[restart to apply] no pending restart-required settings".into()
            },
        ),
        settings_action_line(
            app,
            settings_action_index(SettingsAction::ToggleAnnounceOnStart),
            format!("[announce on start] {}", app.settings.announce_on_start),
        ),
        settings_action_line(
            app,
            settings_action_index(SettingsAction::TogglePeriodicLxmfSync),
            format!("[periodic LXMF sync] {}", app.settings.periodic_lxmf_sync),
        ),
        settings_action_line(
            app,
            settings_action_index(SettingsAction::ToggleAutoSyncAfterPropagationAccept),
            format!(
                "[auto sync after propagation accept] {}",
                app.settings.auto_sync_after_propagation_accept
            ),
        ),
        settings_action_line(
            app,
            settings_action_index(SettingsAction::EditLxmfSyncInterval),
            format!(
                "[edit sync interval] {}s",
                settings_input_value(
                    app,
                    InputTarget::SettingsLxmfSyncIntervalSecs,
                    app.settings.lxmf_sync_interval.to_string(),
                )
            ),
        ),
        settings_action_line(
            app,
            settings_action_index(SettingsAction::EditLxmfSyncLimit),
            format!(
                "[edit sync limit] {}",
                settings_input_value(
                    app,
                    InputTarget::SettingsLxmfSyncLimit,
                    app.settings.lxmf_sync_limit.to_string(),
                )
            ),
        ),
        settings_action_line(
            app,
            settings_action_index(SettingsAction::EditPreferredPropagation),
            format!(
                "[edit propagation] {}",
                settings_input_value(
                    app,
                    InputTarget::SettingsPreferredPropagationNode,
                    app.settings
                        .preferred_propagation_node_hash
                        .clone()
                        .unwrap_or_else(|| "none".into()),
                )
            ),
        ),
        settings_action_line(
            app,
            settings_action_index(SettingsAction::TogglePluginRemoteContent),
            format!(
                "[plugin remote content] {}",
                app.settings.plugins.remote_content_enabled
            ),
        ),
        settings_action_line(
            app,
            settings_action_index(SettingsAction::CreateManagedIdentity),
            format!(
                "[create managed identity] {}",
                if cfg!(feature = "native-reticulum") {
                    "native Reticulum identity"
                } else {
                    "mock identity (native-reticulum not compiled)"
                }
            ),
        ),
        settings_action_line(
            app,
            settings_action_index(SettingsAction::EditIdentityPath),
            format!(
                "[attach existing identity path] {}",
                settings_input_value(
                    app,
                    InputTarget::SettingsIdentityPath,
                    app.settings
                        .identity_path
                        .as_ref()
                        .map(|path| path.display().to_string())
                        .unwrap_or_else(|| "none".into()),
                )
            ),
        ),
        settings_action_line(
            app,
            settings_action_index(SettingsAction::EditReticulumConfigPath),
            format!(
                "[reticulum config path] {}",
                settings_input_value(
                    app,
                    InputTarget::SettingsReticulumConfigPath,
                    app.settings
                        .reticulum_config_path
                        .as_ref()
                        .map(|path| path.display().to_string())
                        .unwrap_or_else(|| "managed".into()),
                )
            ),
        ),
        settings_action_line(
            app,
            settings_action_index(SettingsAction::EditFormMaxAge),
            format!(
                "[edit max age] browser form-state max age: {}s | pages: {}",
                settings_input_value(
                    app,
                    InputTarget::SettingsBrowserFormMaxAgeSecs,
                    app.settings.browser_form_state.max_age_secs.to_string(),
                ),
                app.browser_form_state.page_count()
            ),
        ),
        settings_action_line(
            app,
            settings_action_index(SettingsAction::EditLogMaxBytes),
            format!(
                "[edit log bytes] structured log max file bytes: {}",
                settings_input_value(
                    app,
                    InputTarget::SettingsLogMaxBytes,
                    app.settings.logs.max_file_bytes.to_string(),
                )
            ),
        ),
        settings_action_line(
            app,
            settings_action_index(SettingsAction::EditLogRetainFiles),
            format!(
                "[edit log retain] structured log retained files: {}",
                settings_input_value(
                    app,
                    InputTarget::SettingsLogRetainFiles,
                    app.settings.logs.retain_files.to_string(),
                )
            ),
        ),
        settings_action_line(
            app,
            settings_action_index(SettingsAction::EditLogLoadRecentEntries),
            format!(
                "[edit log load] structured log startup entries: {}",
                settings_input_value(
                    app,
                    InputTarget::SettingsLogLoadRecentEntries,
                    app.settings.logs.load_recent_entries.to_string(),
                )
            ),
        ),
        settings_action_line(
            app,
            settings_action_index(SettingsAction::ToggleForms),
            format!(
                "[toggle forms] browser form-state: {}",
                if app.settings.browser_form_state.enabled {
                    "enabled"
                } else {
                    "disabled"
                }
            ),
        ),
        settings_action_line(
            app,
            settings_action_index(SettingsAction::SensitivePolicy),
            format!(
                "[sensitive policy] sensitive form fields: {:?}",
                app.settings.browser_form_state.sensitive_fields
            ),
        ),
        settings_action_line(
            app,
            settings_action_index(SettingsAction::ForgetPage),
            "[forget page] forget active page form values".into(),
        ),
        settings_action_line(
            app,
            settings_action_index(SettingsAction::ForgetNode),
            "[forget node] forget active node form values".into(),
        ),
        settings_action_line(
            app,
            settings_action_index(SettingsAction::ForgetAll),
            "[forget all] forget all saved form values".into(),
        ),
        Line::from(format!(
            "identity: {}",
            app.settings
                .active_identity_label
                .as_deref()
                .unwrap_or("none")
        )),
        Line::from(format!("data root: {}", app.paths.root.display())),
    ];
    content.insert(
        SettingsAction::ALL.len(),
        Line::from("Use Up/Down to select, Enter to activate, Esc to cancel edits."),
    );
    frame.render_widget(
        Paragraph::new(content).block(Block::default().borders(Borders::ALL).title(
            " Settings | I create identity | G native setup | Up/Down select | Enter activate ",
        )),
        area,
    );
}

fn settings_action_line(app: &App, index: usize, text: String) -> Line<'static> {
    let marker = if app.settings_state.selected == index {
        ">"
    } else {
        " "
    };
    let style = if app.settings_state.selected == index {
        Style::default().fg(Color::Black).bg(Color::Cyan)
    } else {
        Style::default()
    };
    let action = SettingsAction::ALL[index];
    Line::from(Span::styled(format!("{marker} {text}"), style)).patch_style(match action {
        SettingsAction::ForgetAll => Style::default().fg(Color::Yellow),
        SettingsAction::SelectRuntimeReticulum if !cfg!(feature = "native-reticulum") => {
            Style::default().fg(Color::DarkGray)
        }
        SettingsAction::SelectRuntimeBridge => Style::default().fg(Color::DarkGray),
        _ => Style::default(),
    })
}

fn runtime_backend_action_line(app: &App, backend: RuntimeBackendSetting) -> Line<'static> {
    let availability = app
        .runtime_backend_availability()
        .into_iter()
        .find(|candidate| candidate.backend == backend)
        .expect("backend has availability");
    let selected = app.settings.runtime_backend == backend;
    let state = if availability.available {
        "available"
    } else {
        "unavailable"
    };
    settings_action_line(
        app,
        settings_action_index(settings_action_for_backend(backend)),
        format!(
            "[select backend] {} {:?}: {state} ({})",
            if selected { "*" } else { " " },
            backend,
            availability.reason
        ),
    )
}

fn settings_action_for_backend(backend: RuntimeBackendSetting) -> SettingsAction {
    match backend {
        RuntimeBackendSetting::Auto => SettingsAction::SelectRuntimeAuto,
        RuntimeBackendSetting::Mock => SettingsAction::SelectRuntimeMock,
        RuntimeBackendSetting::Reticulum => SettingsAction::SelectRuntimeReticulum,
        RuntimeBackendSetting::Bridge => SettingsAction::SelectRuntimeBridge,
    }
}

fn settings_action_index(action: SettingsAction) -> usize {
    SettingsAction::ALL
        .iter()
        .position(|candidate| *candidate == action)
        .expect("settings action is in SettingsAction::ALL")
}

fn settings_input_value(app: &App, target: InputTarget, fallback: String) -> String {
    app.input
        .active
        .as_ref()
        .and_then(|active| (active.target == target).then(|| active.buffer.display_with_cursor()))
        .unwrap_or(fallback)
}

fn diagnostics_known_destinations_input(app: &App) -> String {
    app.input
        .active
        .as_ref()
        .and_then(|active| {
            (active.target == InputTarget::DiagnosticsKnownDestinationsPath)
                .then(|| active.buffer.display_with_cursor())
        })
        .unwrap_or_else(|| "import Reticulum storage/known_destinations and recheck path".into())
}

fn render_diagnostics(frame: &mut Frame, area: Rect, app: &App) {
    let native_readiness = app.native_reticulum_readiness();
    let log_metrics = app.structured_log_worker_metrics();
    let mut content = vec![
        Line::from("[preview] redacted JSON | [export] write JSON | [clear] preview/export state"),
        Line::from("[probe dry-run] active browser address | [probe live] active browser address"),
        Line::from("[interop preview] live report | [interop export] live report"),
        Line::from(
            "[smoke dry-run] native sequence | [smoke live] includes live probe | [live fetch] probe + fetch_page",
        ),
        Line::from(
            "[path discovery] request path and inspect active browser destination | [lxmf peer] auto-select Directory peer | [lxmf smoke send] peer | [lxmf interop] wait",
        ),
        Line::from(format!(
            "[preload known_destinations] {}",
            diagnostics_known_destinations_input(app)
        )),
        Line::from(format!("runtime: {:?}", app.runtime_status.backend)),
        Line::from(format!("connected: {}", app.runtime_status.connected)),
        Line::from(format!("message: {}", app.runtime_status.message)),
        Line::from(app.runtime_lifecycle_diagnostics_line()),
        Line::from(app.runtime_capabilities_diagnostics_line()),
        Line::from(app.runtime_ownership_diagnostics_line()),
        Line::from(app.interface_diagnostics_line()),
        Line::from(app.path_network_diagnostics_line()),
        Line::from(format!(
            "native Reticulum: compiled={} configured={} ready={}",
            native_readiness.compiled, native_readiness.configured, native_readiness.ready
        )),
        Line::from(format!(
            "native Reticulum detail: {}",
            native_readiness.summary
        )),
        Line::from(format!(
            "identity: {}",
            app.runtime_status
                .active_identity
                .as_ref()
                .map(|identity| identity.label.as_str())
                .unwrap_or("none")
        )),
        Line::from(format!(
            "reticulum config: {}",
            app.paths.reticulum_config_dir.display()
        )),
        Line::from(format!(
            "reticulum storage: {}",
            app.paths.reticulum_storage_dir.display()
        )),
        Line::from(app.native_lxmf_sdk_rpc_probe_line()),
        Line::from(format!(
            "structured log queue: items={} bytes={} oldest_ms={} dropped={} completed={}",
            log_metrics.queued_items,
            log_metrics.queued_bytes,
            log_metrics.oldest_age_ms,
            log_metrics.dropped_records,
            log_metrics.completed_records
        )),
        Line::from(format!(
            "structured log disk: write_failures={} rotations={} removed={} removal_failures={} unsafe_refused={} truncated_scans={}",
            log_metrics.write_failures,
            log_metrics.rotations,
            log_metrics.removed_files,
            log_metrics.removal_failures,
            log_metrics.unsafe_paths_refused,
            log_metrics.truncated_directory_scans
        )),
        Line::from(
            app.diagnostics_state
                .last_snapshot
                .clone()
                .unwrap_or_else(|| "no diagnostics snapshot".into()),
        ),
    ];
    if let Some(path) = &app.diagnostics_state.last_export_path {
        content.push(Line::from(format!(
            "last diagnostics export: {}",
            path.display()
        )));
    } else {
        content.push(Line::from("last diagnostics export: none"));
    }
    if let Some(summary) = &app.diagnostics_state.last_export_summary {
        content.push(Line::from(format!("last export summary: {summary}")));
    }
    if app.diagnostics_export_pending() {
        content.push(Line::from(
            "export state: collecting async diagnostics snapshot",
        ));
    } else {
        content.push(Line::from("export state: idle"));
    }
    if let Some(summary) = diagnostics_live_fetch_summary(&app.diagnostics_state.preview_lines) {
        content.push(Line::from("live fetch result:"));
        content.push(Line::from(format!("  outcome: {}", summary.outcome)));
        content.push(Line::from(format!("  stage: {}", summary.stage_hint)));
        content.push(Line::from(format!(
            "  request backend: {}",
            summary.request_backend
        )));
        content.push(Line::from(format!("  response: {}", summary.response_size)));
        content.push(Line::from(format!("  detail: {}", summary.detail)));
        content.push(Line::from(format!(
            "  first failed stage: {}",
            summary.first_failed_stage
        )));
        content.push(Line::from(format!("  next: {}", summary.next_step)));
    }
    if let Some(summary) = diagnostics_lxmf_delivery_summary(&app.diagnostics_state.preview_lines) {
        content.push(Line::from("lxmf delivery result:"));
        content.push(Line::from(format!("  outcome: {}", summary.outcome)));
        content.push(Line::from(format!("  send: {}", summary.send_state)));
        content.push(Line::from(format!("  proof: {}", summary.proof_state)));
        content.push(Line::from(format!("  inbound: {}", summary.inbound_state)));
        content.push(Line::from(format!("  events: {}", summary.event_counts)));
        content.push(Line::from(format!(
            "  readiness: {}",
            summary.readiness_stage
        )));
        content.push(Line::from(format!("  detail: {}", summary.detail)));
        content.push(Line::from(format!("  next: {}", summary.next_step)));
    }
    let lxmf_peer_candidates = app.lxmf_peer_candidates();
    if lxmf_peer_candidates.is_empty() {
        content.push(Line::from(
            "lxmf peer candidates: none; wait for Directory peer announces or preload known_destinations",
        ));
    } else {
        content.push(Line::from("lxmf peer candidates:"));
        for (index, peer) in lxmf_peer_candidates.iter().take(8).enumerate() {
            let marker = if peer.active_conversation {
                "active"
            } else if peer.selected {
                "selected"
            } else {
                "candidate"
            };
            content.push(Line::from(format!(
                "  {index}: {marker} {} {} trusted={} saved={} last_seen={:.0}",
                peer.display_name, peer.destination_hash, peer.trusted, peer.saved, peer.last_seen
            )));
        }
    }
    if app.diagnostics_state.preview_lines.is_empty() {
        content.push(Line::from("preview: none"));
    } else {
        let total = app.diagnostics_state.preview_lines.len();
        let offset = app
            .diagnostics_state
            .preview_scroll
            .min(total.saturating_sub(1));
        content.push(Line::from(format!(
            "preview: {total} lines | scroll={offset}"
        )));
        content.extend(
            app.diagnostics_state
                .preview_lines
                .iter()
                .skip(offset)
                .map(|line| Line::from(line.clone())),
        );
    }
    frame.render_widget(
        Paragraph::new(content)
            .block(Block::default().borders(Borders::ALL).title(
                " Diagnostics | G native setup | P/E/C bundle | N/X probe | I/O interop | S/L/V smoke/fetch | D path | A peer | M send | Y LXMF wait | K known ",
            ))
            .wrap(Wrap { trim: false }),
        area,
    );
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct TuiLiveFetchSummary {
    outcome: String,
    stage_hint: String,
    request_backend: String,
    response_size: String,
    detail: String,
    first_failed_stage: String,
    next_step: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct TuiLxmfDeliverySummary {
    outcome: String,
    send_state: String,
    proof_state: String,
    inbound_state: String,
    event_counts: String,
    readiness_stage: String,
    detail: String,
    next_step: String,
}

fn diagnostics_live_fetch_summary(lines: &[String]) -> Option<TuiLiveFetchSummary> {
    let value: serde_json::Value = serde_json::from_str(&lines.join("\n")).ok()?;
    let live_fetch = value.get("live_fetch")?;
    let ok = live_fetch
        .get("ok")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    let stage_hint = json_string_field(live_fetch, &["stage_hint"])
        .or_else(|| first_failed_page_probe_stage(&value))
        .unwrap_or_else(|| "unknown".into());
    let request_backend = live_fetch
        .get("metadata")
        .and_then(|metadata| {
            let backend = json_string_field(metadata, &["native_request_backend"])?;
            Some(
                match json_string_field(metadata, &["native_request_primitive"]) {
                    Some(primitive) => format!("{backend}/{primitive}"),
                    None => backend,
                },
            )
        })
        .unwrap_or_else(|| {
            if ok {
                "missing metadata".into()
            } else {
                "not reached".into()
            }
        });
    let response_size = if ok {
        let bytes = live_fetch
            .get("markup_bytes")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0);
        let lines = live_fetch
            .get("markup_lines")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0);
        format!("{bytes} bytes, {lines} lines")
    } else {
        "no response body".into()
    };
    let detail = if ok {
        let title = json_string_field(live_fetch, &["title"]).unwrap_or_else(|| "untitled".into());
        let url = json_string_field(live_fetch, &["url"]).unwrap_or_else(|| "unknown url".into());
        format!("{title} from {url}")
    } else {
        json_string_field(live_fetch, &["error", "skipped"])
            .or_else(|| {
                value.get("classification").and_then(|classification| {
                    json_string_field(classification, &["reason", "detail"])
                })
            })
            .unwrap_or_else(|| "live fetch did not complete".into())
    };
    let first_failed_stage = first_failed_page_probe_stage(&value)
        .or_else(|| {
            value
                .get("classification")
                .and_then(|classification| json_string_field(classification, &["stage"]))
        })
        .unwrap_or_else(|| {
            if ok {
                "none".into()
            } else {
                stage_hint.clone()
            }
        });
    let next_step = if ok {
        "open the Browser view and inspect the rendered page".into()
    } else {
        value
            .get("classification")
            .and_then(|classification| {
                json_string_field(classification, &["next_step", "next_action"])
            })
            .unwrap_or_else(|| "fix the failed stage, then run Native Live Fetch again".into())
    };

    Some(TuiLiveFetchSummary {
        outcome: if ok { "pass" } else { "blocked" }.into(),
        stage_hint,
        request_backend,
        response_size,
        detail,
        first_failed_stage,
        next_step,
    })
}

fn diagnostics_lxmf_delivery_summary(lines: &[String]) -> Option<TuiLxmfDeliverySummary> {
    let value: serde_json::Value = serde_json::from_str(&lines.join("\n")).ok()?;
    let report = tui_lxmf_interop_report_value(&value)?;
    let classification = report.get("classification");
    let send = report.get("send");
    let wait = report.get("wait");
    let outcome = classification
        .and_then(|value| json_string_field(value, &["outcome"]))
        .or_else(|| wait.and_then(|value| json_string_field(value, &["status"])))
        .unwrap_or_else(|| "unknown".into());
    let send_state = send
        .map(tui_lxmf_send_state_line)
        .unwrap_or_else(|| "send: not requested".into());
    let proof_state = wait
        .and_then(|value| json_string_field(value, &["proof_match_state"]))
        .unwrap_or_else(|| "unknown".into());
    let inbound_state = wait
        .and_then(|value| json_string_field(value, &["inbound_reply_match_state"]))
        .unwrap_or_else(|| "unknown".into());
    let event_counts = wait
        .map(tui_lxmf_event_counts_line)
        .unwrap_or_else(|| "events unavailable".into());
    let readiness_stage = tui_lxmf_first_failed_readiness_stage(report)
        .unwrap_or_else(|| "ready or not requested".into());
    let detail = classification
        .and_then(|value| json_string_field(value, &["reason", "detail"]))
        .or_else(|| wait.and_then(|value| json_string_field(value, &["detail"])))
        .unwrap_or_else(|| "no LXMF delivery detail".into());
    let next_step = classification
        .and_then(|value| json_string_field(value, &["next_step", "next_action"]))
        .or_else(|| {
            report
                .get("failure_hints")
                .and_then(serde_json::Value::as_array)
                .and_then(|hints| hints.first())
                .and_then(serde_json::Value::as_str)
                .map(ToOwned::to_owned)
        })
        .unwrap_or_else(|| {
            "select an LXMF peer conversation in the app, then run LXMF Interop again".into()
        });

    Some(TuiLxmfDeliverySummary {
        outcome,
        send_state,
        proof_state,
        inbound_state,
        event_counts,
        readiness_stage,
        detail,
        next_step,
    })
}

fn tui_lxmf_interop_report_value(value: &serde_json::Value) -> Option<&serde_json::Value> {
    if value.get("report").and_then(serde_json::Value::as_str) == Some("native_lxmf_live_interop") {
        return Some(value);
    }
    value.get("lxmf_live_interop").filter(|nested| {
        nested.get("report").and_then(serde_json::Value::as_str) == Some("native_lxmf_live_interop")
    })
}

fn tui_lxmf_send_state_line(send: &serde_json::Value) -> String {
    let requested = send
        .get("requested")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    if !requested {
        return "not requested".into();
    }
    let ok = send
        .get("ok")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    let message_id = json_string_field(send, &["message_id", "packet_hash"])
        .unwrap_or_else(|| "no message id".into());
    let state = json_string_field(
        send,
        &["native_lxmf_state", "stage_hint", "skipped", "error"],
    )
    .unwrap_or_else(|| {
        if ok {
            "submitted".into()
        } else {
            "failed".into()
        }
    });
    format!(
        "{} | {} | {}",
        if ok { "submitted" } else { "not sent" },
        state,
        message_id
    )
}

fn tui_lxmf_event_counts_line(wait: &serde_json::Value) -> String {
    let inbound = wait
        .get("inbound_messages")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    let delivery = wait
        .get("delivery_updates")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    let proofs = wait
        .get("packet_proofs")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    format!("inbound={inbound}, delivery_updates={delivery}, packet_proofs={proofs}")
}

fn tui_lxmf_first_failed_readiness_stage(report: &serde_json::Value) -> Option<String> {
    report
        .get("readiness_probe")
        .and_then(|probe| probe.get("steps"))
        .and_then(serde_json::Value::as_array)
        .and_then(|steps| {
            steps.iter().find_map(|step| {
                let ok = step
                    .get("ok")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false);
                (!ok)
                    .then(|| json_string_field(step, &["stage"]))
                    .flatten()
                    .map(|stage| {
                        let detail = json_string_field(step, &["detail"])
                            .unwrap_or_else(|| "blocked".into());
                        format!("{stage}: {detail}")
                    })
            })
        })
}

fn first_failed_page_probe_stage(value: &serde_json::Value) -> Option<String> {
    ["live_page_probe", "dry_run_page_probe"]
        .iter()
        .find_map(|section| {
            value
                .get(*section)
                .and_then(|probe| probe.get("report"))
                .and_then(|report| report.get("steps"))
                .and_then(serde_json::Value::as_array)
                .and_then(|steps| {
                    steps.iter().find_map(|step| {
                        let ok = step
                            .get("ok")
                            .and_then(serde_json::Value::as_bool)
                            .unwrap_or(false);
                        (!ok).then(|| json_string_field(step, &["stage"])).flatten()
                    })
                })
        })
}

fn json_string_field(value: &serde_json::Value, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| {
        value.get(*key).and_then(|field| {
            field
                .as_str()
                .map(ToOwned::to_owned)
                .or_else(|| field.as_bool().map(|value| value.to_string()))
                .or_else(|| field.as_u64().map(|value| value.to_string()))
        })
    })
}

fn render_logs(frame: &mut Frame, area: Rect, app: &App) {
    let metrics = app.structured_log_worker_metrics();
    let worker_status = Line::from(format!(
        "writer: queued={}/{} bytes oldest_ms={} dropped={} completed={} write_failures={} unsafe_refused={}",
        metrics.queued_items,
        metrics.queued_bytes,
        metrics.oldest_age_ms,
        metrics.dropped_records,
        metrics.completed_records,
        metrics.write_failures,
        metrics.unsafe_paths_refused
    ));
    let content = if app.logs.entries.is_empty() {
        std::iter::once(worker_status)
            .chain(app.logs.lines.iter().cloned().map(Line::from))
            .collect::<Vec<_>>()
    } else {
        let filtered = app.logs.filtered_entries();
        std::iter::once(worker_status)
            .chain(std::iter::once(Line::from(format!(
                "filters: severity={} source={} | f severity | s source",
                app.logs
                    .severity_filter
                    .map(|severity| format!("{severity:?}"))
                    .unwrap_or_else(|| "all".into()),
                app.logs
                    .source_filter
                    .map(|source| format!("{source:?}"))
                    .unwrap_or_else(|| "all".into())
            ))))
            .chain(filtered.into_iter().map(|entry| {
                Line::from(format!(
                    "{} {:?} {:?} {}",
                    entry.epoch_ms, entry.severity, entry.source, entry.message
                ))
            }))
            .collect::<Vec<_>>()
    };
    frame.render_widget(
        Paragraph::new(content).block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Logs | f severity | s source "),
        ),
        area,
    );
}

fn render_plugins(frame: &mut Frame, area: Rect, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(6), Constraint::Length(9)])
        .split(area);
    let rows = app
        .plugins_state
        .manifests
        .iter()
        .enumerate()
        .map(|(index, manifest)| {
            let enabled = app
                .settings
                .plugins
                .enabled_plugin_ids
                .iter()
                .any(|plugin_id| plugin_id == &manifest.plugin_id);
            let marker = if app.plugins_state.selected == Some(index) {
                ">"
            } else {
                " "
            };
            let item = ListItem::new(format!(
                "{marker} [toggle] [{}] {} {} | perms={} | {}",
                if enabled { "on " } else { "off" },
                manifest.name,
                manifest.version,
                manifest.permissions.len(),
                if manifest.plugin_id == BUILTIN_MICRONPLUS_PLUGIN_ID {
                    app.micronplus_status_for_active_page()
                } else {
                    manifest.description.clone()
                }
            ));
            if app.plugins_state.selected == Some(index) {
                item.style(Style::default().fg(Color::Black).bg(Color::Cyan))
            } else {
                item
            }
        })
        .collect::<Vec<_>>();
    frame.render_widget(
        List::new(rows).block(Block::default().borders(Borders::ALL).title(
            " Plugins | click [toggle] | Enter/e toggle | i install | x remove | r refresh | l logs ",
        )),
        chunks[0],
    );
    let mut detail = app
        .selected_plugin_detail_lines()
        .into_iter()
        .map(Line::from)
        .collect::<Vec<_>>();
    detail.push(Line::from(""));
    detail.push(Line::from("MicronPlus active page diagnostics:"));
    detail.extend(
        app.active_micronplus_diagnostic_lines()
            .into_iter()
            .map(|line| Line::from(format!("  {line}"))),
    );
    if !app.plugins_state.warnings.is_empty() {
        detail.push(Line::from(format!(
            "warnings: {}",
            app.plugins_state.warnings.join(" | ")
        )));
    }
    frame.render_widget(
        Paragraph::new(detail)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Plugin Detail | manifest-only, no execution "),
            )
            .wrap(Wrap { trim: false }),
        chunks[1],
    );
}

#[allow(dead_code)]
fn render_placeholder(frame: &mut Frame, area: Rect, section: WorkspaceSection) {
    frame.render_widget(
        Paragraph::new(format!(
            "{} panel boundary is reserved for the next porting pass.",
            section.label()
        ))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(section.label()),
        ),
        area,
    );
}

fn render_footer(frame: &mut Frame, area: Rect, app: &App) {
    let help = Line::from(vec![
        Span::raw(" q/Ctrl-c quit | Tab focus | mouse navigate | Ctrl-t browser tab | Ctrl-n conversation | ? contextual shortcuts "),
    ]);
    let lines = vec![status::status_line(&app.status), help];
    frame.render_widget(Paragraph::new(lines), area);
}

fn shortcut_help_lines(section: WorkspaceSection) -> Vec<Line<'static>> {
    let mut lines = vec![
        Line::styled(
            format!("{} shortcuts", section.label()),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Line::from("q or Ctrl-c exits | ? closes shortcuts | Tab cycles focus"),
        Line::from("F1-F8 switches primary workspaces | mouse navigates visible controls"),
    ];
    lines.extend(match section {
        WorkspaceSection::Browser => vec![
            Line::from("Ctrl-l edits address | Enter opens | Ctrl-r reloads | Ctrl-d downloads"),
            Line::from("Ctrl-t creates a tab | Ctrl-w closes | Left/Right switches tabs"),
            Line::from("Tab/Shift-Tab moves page control focus | Enter/Space activates"),
            Line::from("PgUp/PgDn scrolls | Ctrl-j/Ctrl-k scrolls one line"),
            Line::from("Ctrl-f refreshes partials | D discovers path | R retries after discovery"),
            Line::from("o cycles status overlay | O expands overlay | N runs inline page probe"),
        ],
        WorkspaceSection::Messages => vec![
            Line::from("Ctrl-n creates a conversation"),
            Line::from("Ctrl-y edits title | Ctrl-e edits body | Ctrl-s sends"),
            Line::from("Ctrl-p toggles direct/propagated | Ctrl-u toggles reply ticket"),
            Line::from("Ctrl-a confirms a direct stamp | Ctrl-x cancels confirmation"),
            Line::from("Ctrl-g syncs runtime messages"),
        ],
        WorkspaceSection::NetworkDoctor => vec![
            Line::from("/ edits bounded Operations search | f cycles filter | c clears search"),
            Line::from("Up/Down or j/k selects one of the eight visible operation rows"),
            Line::from("Enter or v opens the redacted, bounded diagnostic"),
            Line::from("Copy/select view: terminal mouse selects text; Esc or q returns"),
            Line::from("Copy/select view: PgUp/PgDn or j/k scrolls"),
        ],
        WorkspaceSection::Directory => vec![
            Line::from("Up/Down selects an entry | Enter opens its primary action"),
            Line::from("d requests selected paths | s saves | t cycles trust"),
            Line::from("r refreshes propagation evidence | x cancels | p selects node"),
            Line::from("g syncs propagation messages | k cycles peer reply-ticket default"),
        ],
        WorkspaceSection::Interfaces => vec![
            Line::from("Up/Down selects an interface | e enables/disables | x deletes"),
            Line::from("a adds TCP | 1/2 adds RMap/WNS preset | i adds I2P | v adds RNode"),
            Line::from("n edits name | h/p edits TCP host/port | r edits I2P peers"),
            Line::from("P previews config | E exports config | G runs native quickstart"),
        ],
        WorkspaceSection::Settings => vec![
            Line::from("Up/Down selects a setting | Enter edits or activates"),
            Line::from("Reduced motion controls desktop animated previews; TUI stays static"),
            Line::from("I creates a managed identity | i attaches an identity path"),
            Line::from("G runs native Reticulum quickstart"),
        ],
        WorkspaceSection::Diagnostics => vec![
            Line::from("P previews diagnostics | E exports | C clears preview"),
            Line::from("N/X probes page fetch | I/O previews/exports interop"),
            Line::from("S/L runs dry/live native smoke | G runs quickstart"),
        ],
        WorkspaceSection::Logs => vec![Line::from("f cycles severity | s cycles source")],
        WorkspaceSection::Plugins => vec![Line::from(
            "Up/Down selects a plugin | Enter activates the selected action",
        )],
        WorkspaceSection::Identities | WorkspaceSection::Monitoring | WorkspaceSection::Help => {
            vec![Line::from(
                "This workspace currently has no additional section-specific shortcuts.",
            )]
        }
    });
    lines
}

fn render_help(frame: &mut Frame, area: Rect, app: &App) {
    frame.render_widget(Clear, area);
    let help = Paragraph::new(shortcut_help_lines(app.workspace.active_section))
        .block(
            Block::default()
                .title(" Shortcuts ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan)),
        )
        .wrap(Wrap { trim: false });
    frame.render_widget(help, area);
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(vertical[1])[1]
}

#[cfg(test)]
mod tests {
    use super::{
        diagnostics_live_fetch_summary, masked_passphrase_status, shortcut_help_lines,
        WorkspaceSection,
    };

    fn help_text(section: WorkspaceSection) -> String {
        shortcut_help_lines(section)
            .into_iter()
            .flat_map(|line| line.spans)
            .map(|span| span.content.into_owned())
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn shortcut_overlay_is_scoped_to_the_active_workspace() {
        let operations = help_text(WorkspaceSection::NetworkDoctor);
        assert!(operations.contains("/ edits bounded Operations search"));
        assert!(operations.contains("Enter or v opens the redacted, bounded diagnostic"));
        assert!(operations.contains("terminal mouse selects text"));
        assert!(!operations.contains("Ctrl-l edits address"));

        let browser = help_text(WorkspaceSection::Browser);
        assert!(browser.contains("Ctrl-l edits address"));
        assert!(browser.contains("D discovers path"));
        assert!(!browser.contains("Operations search"));

        let directory = help_text(WorkspaceSection::Directory);
        assert!(directory.contains("d requests selected paths"));
        assert!(directory.contains("r refreshes propagation evidence"));
        assert!(directory.contains("x cancels"));

        let settings = help_text(WorkspaceSection::Settings);
        assert!(settings.contains("Reduced motion controls desktop animated previews"));
    }

    #[test]
    fn passphrase_status_never_renders_the_active_secret() {
        assert_eq!(masked_passphrase_status(true, true), "editing (hidden)");
        assert_eq!(masked_passphrase_status(true, false), "editing (hidden)");
        assert_eq!(masked_passphrase_status(false, true), "configured");
        assert_eq!(masked_passphrase_status(false, false), "not set");
    }

    #[test]
    fn live_fetch_summary_names_request_resource_compatibility_primitive() {
        let lines = serde_json::to_string_pretty(&serde_json::json!({
            "live_fetch": {
                "ok": true,
                "stage_hint": "response_decode",
                "url": "00112233445566778899aabbccddeeff:/page/index.mu",
                "title": "Node Home",
                "markup_bytes": 32,
                "markup_lines": 2,
                "metadata": {
                    "native_request_backend": "reticulum-transport",
                    "native_request_primitive": "request-resource"
                }
            }
        }))
        .expect("live fetch preview json")
        .lines()
        .map(str::to_owned)
        .collect::<Vec<_>>();

        let summary = diagnostics_live_fetch_summary(&lines).expect("live fetch summary");
        assert_eq!(
            summary.request_backend,
            "reticulum-transport/request-resource"
        );
    }
}
