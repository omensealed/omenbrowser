use iced::widget::text::Wrapping;
use iced::widget::{column, container, row, text, text_input, Button};
use iced::{Element, Length};

use crate::app::DirectoryScope;
use crate::directory::{DirectoryEntry, DirectoryKind, TrustLevel};

use super::super::{
    action_grid, app_scrollable, card_container_style, format_epoch_secs, omen_button,
    omen_button_owned, relative_time, section_card, status_container_style, subtle_button,
    subtle_button_owned, ui_size, wrapped_panel_text, wrapped_text_owned, DesktopApp,
    DiagnosticsMessage, DirectoryMessage, Message, DIRECTORY_RENDER_LIMIT,
};
use super::directory_model::{
    directory_empty_text, directory_empty_text_for_scope, directory_entry_matches_view,
    directory_kind_supports_delivery_preference, directory_kind_supports_identify_toggle,
    directory_kind_title, directory_selected_kind_note, directory_selected_primary_action_labels,
    directory_selected_state_lines, propagation_node_state_lines, short_destination_hash,
};

pub(in crate::desktop) fn directory_tab_button(
    label: &'static str,
    kind: DirectoryKind,
    active: &DirectoryKind,
    count: usize,
) -> Button<'static, Message> {
    let title = format!("{label} ({count})");
    if &kind == active {
        omen_button_owned(
            title,
            Message::Directory(DirectoryMessage::SwitchKind(kind)),
        )
    } else {
        subtle_button_owned(
            title,
            Message::Directory(DirectoryMessage::SwitchKind(kind)),
        )
    }
}

pub(in crate::desktop) fn directory_scope_button(
    label: &'static str,
    scope: DirectoryScope,
    active: &DirectoryScope,
) -> Button<'static, Message> {
    if &scope == active {
        omen_button_owned(
            label.to_string(),
            Message::Directory(DirectoryMessage::SwitchScope(scope)),
        )
    } else {
        subtle_button_owned(
            label.to_string(),
            Message::Directory(DirectoryMessage::SwitchScope(scope)),
        )
    }
}

pub(in crate::desktop) fn directory_selected_primary_actions(
    index: usize,
    kind: &DirectoryKind,
) -> Element<'static, Message> {
    match kind {
        DirectoryKind::Node => row![omen_button(
            "Browse Node",
            Message::Directory(DirectoryMessage::OpenEntry(index))
        )]
        .spacing(8)
        .wrap()
        .into(),
        DirectoryKind::Peer => row![
            omen_button(
                "Message Peer",
                Message::Directory(DirectoryMessage::OpenPeerChat(index))
            ),
            subtle_button(
                "Inspect Peer",
                Message::Directory(DirectoryMessage::InspectPeer(index))
            ),
        ]
        .spacing(8)
        .wrap()
        .into(),
        DirectoryKind::Propagation => row![
            omen_button(
                "Use Propagation",
                Message::Directory(DirectoryMessage::UsePropagation(index))
            ),
            subtle_button(
                "Refresh Node",
                Message::Directory(DirectoryMessage::RefreshPropagation(index))
            ),
            subtle_button(
                "Cancel Refresh",
                Message::Directory(DirectoryMessage::CancelPropagationRefresh)
            ),
            subtle_button(
                "Sync Now",
                Message::Diagnostics(DiagnosticsMessage::SyncPropagationNow)
            ),
        ]
        .spacing(8)
        .wrap()
        .into(),
        #[cfg(feature = "chat-client")]
        DirectoryKind::OmenChat => row![omen_button(
            "Open Chat",
            Message::Directory(DirectoryMessage::OpenOmenChat(index))
        )]
        .spacing(8)
        .wrap()
        .into(),
        #[cfg(not(feature = "chat-client"))]
        DirectoryKind::OmenChat => row![subtle_button(
            "Select",
            Message::Directory(DirectoryMessage::SelectEntry(index))
        )]
        .spacing(8)
        .wrap()
        .into(),
        DirectoryKind::Unknown => row![subtle_button(
            "Select",
            Message::Directory(DirectoryMessage::SelectEntry(index))
        )]
        .spacing(8)
        .wrap()
        .into(),
    }
}

pub(in crate::desktop) fn directory_view(desktop: &DesktopApp) -> Element<'_, Message> {
    let propagation_inventory = desktop.app.propagation_node_inventory();
    let active_kind = desktop.app.directory_state.active_kind.clone();
    let active_scope = desktop.app.directory_state.active_scope.clone();
    let filter = desktop.app.directory_state.filter.trim();
    let counts = desktop.app.directory_state.entries.iter().fold(
        (0usize, 0usize, 0usize, 0usize, 0usize),
        |(nodes, peers, propagation, omenchat, trusted), entry| {
            (
                nodes
                    + usize::from(directory_entry_matches_view(
                        entry,
                        &DirectoryKind::Node,
                        &active_scope,
                        filter,
                    )),
                peers
                    + usize::from(directory_entry_matches_view(
                        entry,
                        &DirectoryKind::Peer,
                        &active_scope,
                        filter,
                    )),
                propagation
                    + usize::from(directory_entry_matches_view(
                        entry,
                        &DirectoryKind::Propagation,
                        &active_scope,
                        filter,
                    )),
                omenchat
                    + usize::from(directory_entry_matches_view(
                        entry,
                        &DirectoryKind::OmenChat,
                        &active_scope,
                        filter,
                    )),
                trusted + usize::from(entry.trusted),
            )
        },
    );
    let unknown_count = desktop
        .app
        .directory_state
        .entries
        .iter()
        .filter(|entry| entry.kind == DirectoryKind::Unknown)
        .count();
    let tabs = row![
        directory_tab_button("Nodes", DirectoryKind::Node, &active_kind, counts.0),
        directory_tab_button("Peers", DirectoryKind::Peer, &active_kind, counts.1),
        directory_tab_button("OMENchat", DirectoryKind::OmenChat, &active_kind, counts.3),
        directory_tab_button(
            "Propagation",
            DirectoryKind::Propagation,
            &active_kind,
            counts.2
        ),
    ]
    .spacing(8)
    .wrap();
    let scope_tabs = row![
        directory_scope_button("Live", DirectoryScope::Live, &active_scope),
        directory_scope_button("Saved", DirectoryScope::Saved, &active_scope),
        directory_scope_button("Trusted", DirectoryScope::Trusted, &active_scope),
    ]
    .spacing(8)
    .wrap();
    let mut rows = column![directory_group_view(
        desktop,
        directory_kind_title(&active_kind),
        active_kind.clone(),
        active_scope.clone(),
        directory_empty_text(&active_kind),
    )]
    .spacing(12);
    if unknown_count > 0 {
        rows = rows.push(section_card(
            format!("Unknown Announces ({unknown_count})"),
            wrapped_panel_text(
                "Unknown announces are kept out of Nodes/Peers/Propagation until classified.",
            ),
        ));
    }
    let selected_details = directory_selected_details_card(desktop);

    app_scrollable(
        column![
            text("Directory").size(ui_size(28)),
            tabs,
            scope_tabs,
            row![
                text_input(
                    "Search directory by name, destination, kind, delivery...",
                    &desktop.app.directory_state.filter
                )
                .on_input(|value| Message::Directory(DirectoryMessage::FilterChanged(value)))
                .width(Length::Fill),
                subtle_button(
                    "Clear",
                    Message::Directory(DirectoryMessage::FilterChanged(String::new()))
                ),
            ]
            .spacing(8)
            .wrap(),
            section_card(
                "Directory State",
                column![
                    wrapped_text_owned(format!(
                        "entries={} visible_nodes={} visible_peers={} visible_omenchat={} visible_propagation={} trusted={}",
                        desktop.app.directory_state.entries.len(),
                        counts.0,
                        counts.1,
                        counts.3,
                        counts.2,
                        counts.4
                    ), 14),
                    wrapped_text_owned(format!(
                        "filter: {}",
                        if desktop.app.directory_state.filter.is_empty() {
                            "none"
                        } else {
                            desktop.app.directory_state.filter.as_str()
                        }
                    ), 14),
                    wrapped_text_owned(format!(
                        "preferred propagation: {}",
                        desktop
                            .app
                            .settings
                            .preferred_propagation_node_hash
                            .as_deref()
                            .unwrap_or("none")
                    ), 14),
                    wrapped_text_owned(format!(
                        "propagation inventory: retained={}/{} bytes={} truncated={}",
                        propagation_inventory.nodes.len(),
                        propagation_inventory.total_candidates,
                        propagation_inventory.retained_bytes,
                        propagation_inventory.truncated
                    ), 14),
                    wrapped_text_owned(format!(
                        "propagation refresh: {:?} | {}",
                        desktop.app.directory_state.propagation_refresh.outcome,
                        if desktop.app.directory_state.propagation_refresh.detail.is_empty() {
                            "idle"
                        } else {
                            desktop.app.directory_state.propagation_refresh.detail.as_str()
                        }
                    ), 14),
                    action_grid(
                        vec![subtle_button(
                            "Clear Propagation",
                            Message::Directory(DirectoryMessage::ClearPropagation)
                        ),],
                        3
                    ),
                ]
                .spacing(4),
            ),
            selected_details,
            rows,
        ]
        .spacing(12)
        .width(Length::Fill),
    )
    .height(Length::Fill)
    .into()
}

fn directory_group_view(
    desktop: &DesktopApp,
    title: &str,
    kind: DirectoryKind,
    scope: DirectoryScope,
    empty_text: &str,
) -> Element<'static, Message> {
    let entries = desktop
        .app
        .directory_state
        .entries
        .iter()
        .enumerate()
        .filter(|(_, entry)| {
            directory_entry_matches_view(entry, &kind, &scope, &desktop.app.directory_state.filter)
        })
        .take(DIRECTORY_RENDER_LIMIT)
        .fold(column![].spacing(8), |column, (index, entry)| {
            column.push(directory_entry_card(desktop, index, entry))
        });
    let count = desktop
        .app
        .directory_state
        .entries
        .iter()
        .filter(|entry| {
            directory_entry_matches_view(entry, &kind, &scope, &desktop.app.directory_state.filter)
        })
        .count();

    if count == 0 {
        let empty_message =
            directory_empty_text_for_scope(empty_text, &scope, &desktop.app.directory_state.filter);
        section_card(
            format!("{title} (0)"),
            wrapped_text_owned(empty_message, 14),
        )
    } else {
        let body = if count > DIRECTORY_RENDER_LIMIT {
            entries.push(
                wrapped_text_owned(format!(
                    "Showing first {DIRECTORY_RENDER_LIMIT} of {count}. Use search, saved/trusted scope, or wait for stale live entries to prune."
                ), 13),
            )
        } else {
            entries
        };
        section_card(format!("{title} ({count})"), body)
    }
}

fn directory_entry_card(
    desktop: &DesktopApp,
    index: usize,
    entry: &DirectoryEntry,
) -> Element<'static, Message> {
    let marker = if Some(index) == desktop.app.directory_state.selected {
        "selected"
    } else {
        "entry"
    };
    let primary_action = match entry.kind {
        DirectoryKind::Node => subtle_button(
            "Browse",
            Message::Directory(DirectoryMessage::OpenEntry(index)),
        ),
        DirectoryKind::Peer => subtle_button(
            "Message",
            Message::Directory(DirectoryMessage::OpenPeerChat(index)),
        ),
        DirectoryKind::Propagation => subtle_button(
            "Use",
            Message::Directory(DirectoryMessage::UsePropagation(index)),
        ),
        #[cfg(feature = "chat-client")]
        DirectoryKind::OmenChat => subtle_button(
            "Open Chat",
            Message::Directory(DirectoryMessage::OpenOmenChat(index)),
        ),
        #[cfg(not(feature = "chat-client"))]
        DirectoryKind::OmenChat => subtle_button(
            "Select",
            Message::Directory(DirectoryMessage::SelectEntry(index)),
        ),
        DirectoryKind::Unknown => subtle_button(
            "Select",
            Message::Directory(DirectoryMessage::SelectEntry(index)),
        ),
    };
    let destination_preview = short_destination_hash(&entry.destination_hash);
    let display_name = entry.display_name.clone();
    let marker_text = if Some(index) == desktop.app.directory_state.selected {
        "*"
    } else {
        " "
    };

    container(
        row![
            text(marker_text).size(ui_size(14)),
            text(display_name)
                .size(ui_size(14))
                .wrapping(Wrapping::WordOrGlyph)
                .width(Length::FillPortion(3)),
            text(destination_preview)
                .size(ui_size(13))
                .wrapping(Wrapping::WordOrGlyph)
                .width(Length::FillPortion(2)),
            text(relative_time(entry.last_seen))
                .size(ui_size(13))
                .width(Length::Shrink),
            subtle_button(
                "Select",
                Message::Directory(DirectoryMessage::SelectEntry(index))
            ),
            primary_action,
        ]
        .spacing(8)
        .align_y(iced::Alignment::Center),
    )
    .style(if marker == "selected" {
        status_container_style
    } else {
        card_container_style
    })
    .padding(6)
    .width(Length::Fill)
    .into()
}

fn directory_selected_details_card(desktop: &DesktopApp) -> Element<'_, Message> {
    let Some(index) = desktop.app.directory_state.selected else {
        return section_card(
            "Selected Entry",
            text("Select a directory entry to inspect full destination and relationship details.")
                .size(ui_size(14)),
        );
    };
    let Some(entry) = desktop.app.selected_directory_entry() else {
        return section_card(
            "Selected Entry",
            text("Select a directory entry to inspect full destination and relationship details.")
                .size(ui_size(14)),
        );
    };
    let associated = entry.associated_hash.as_deref().unwrap_or("none");
    let node_associated = entry.node_associated_hash.as_deref().unwrap_or("none");
    let kind_note = directory_selected_kind_note(&entry);
    let state_lines = directory_selected_state_lines(&entry)
        .into_iter()
        .fold(column![].spacing(3), |column, line| {
            column.push(wrapped_text_owned(line, 14))
        });
    let propagation_state_lines = desktop
        .app
        .propagation_node_inventory()
        .nodes
        .iter()
        .find(|node| {
            node.destination_hash
                .eq_ignore_ascii_case(&entry.destination_hash)
        })
        .map(propagation_node_state_lines)
        .unwrap_or_default()
        .into_iter()
        .fold(column![].spacing(3), |column, line| {
            column.push(wrapped_text_owned(line, 14))
        });
    let micronplus_warning_lines = desktop
        .app
        .micronplus_warning_lines_for_directory_entry(&entry)
        .into_iter()
        .fold(column![].spacing(3), |column, line| {
            column.push(wrapped_text_owned(line, 13))
        });
    let selected_primary = directory_selected_primary_actions(index, &entry.kind);
    let trust_action = if entry.trust_level == TrustLevel::Trusted {
        "Untrust"
    } else {
        "Trust"
    };
    let mut management_actions = vec![
        subtle_button(
            if entry.saved { "Remove Saved" } else { "Save" },
            Message::Directory(DirectoryMessage::SaveEntry(index)),
        ),
        subtle_button(
            trust_action,
            Message::Directory(DirectoryMessage::ToggleTrust(index)),
        ),
    ];
    if directory_kind_supports_identify_toggle(&entry.kind) {
        management_actions.push(subtle_button(
            if entry.identify_on_connect {
                "Stop Identify"
            } else {
                "Identify"
            },
            Message::Directory(DirectoryMessage::ToggleIdentify(index)),
        ));
    }
    if directory_kind_supports_delivery_preference(&entry.kind) {
        management_actions.push(subtle_button(
            "Delivery",
            Message::Directory(DirectoryMessage::CycleDelivery(index)),
        ));
        management_actions.push(subtle_button(
            "Fallback",
            Message::Directory(DirectoryMessage::CycleFallback(index)),
        ));
        management_actions.push(subtle_button(
            "Stamp Limit",
            Message::Directory(DirectoryMessage::CycleDirectStampLimit(index)),
        ));
        management_actions.push(subtle_button(
            "Stamp Ask",
            Message::Directory(DirectoryMessage::CycleDirectStampConfirmation(index)),
        ));
    }
    if entry.kind != DirectoryKind::Propagation {
        management_actions.push(subtle_button(
            "Request Path",
            Message::Directory(DirectoryMessage::RequestPath(index)),
        ));
    }
    let selected_management = action_grid(management_actions, 3);

    section_card(
        format!("Selected Entry: {}", entry.display_name),
        column![
            selected_primary,
            selected_management,
            wrapped_text_owned(
                format!(
                    "primary actions: {}",
                    directory_selected_primary_action_labels(&entry.kind).join(", ")
                ),
                13
            ),
            text(format!("{:?}", entry.kind)).size(ui_size(14)),
            wrapped_text_owned(format!("destination: {}", entry.destination_hash), 14),
            wrapped_text_owned(format!("associated: {associated}"), 14),
            wrapped_text_owned(format!("node associated: {node_associated}"), 14),
            state_lines,
            propagation_state_lines,
            wrapped_text_owned(
                format!(
                    "last seen: {} ({})",
                    format_epoch_secs(entry.last_seen),
                    relative_time(entry.last_seen)
                ),
                14
            ),
            wrapped_text_owned(kind_note, 14),
            wrapped_text_owned(
                desktop.app.micronplus_status_for_directory_entry(&entry),
                14
            ),
            section_card("MicronPlus Node Warnings", micronplus_warning_lines),
        ]
        .spacing(5),
    )
}
