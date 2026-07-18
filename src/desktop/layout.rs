use std::sync::atomic::{AtomicU16, Ordering};

use iced::widget::pane_grid;
use iced::widget::scrollable::Scrollbar;

use crate::app::App;
use crate::storage::settings::{
    DesktopWorkspaceLayoutNode, DesktopWorkspacePaneKind, DesktopWorkspacePaneSettings,
    DesktopWorkspaceSplitAxis,
};

use super::{DesktopApp, DesktopPane};

pub(super) const DESKTOP_SCROLLBAR_WIDTH: u16 = 7;
pub(super) const DESKTOP_SCROLLBAR_SCROLLER_WIDTH: u16 = 4;
pub(super) const DESKTOP_SCROLLBAR_MARGIN: u16 = 4;
pub(super) const DESKTOP_SCROLL_GUTTER_EXTRA: u16 = 12;
pub(super) const DESKTOP_SCROLL_OUTER_INSET: u16 = 6;
pub(super) const DESKTOP_PANEL_PADDING: u16 = 12;
pub(super) const DESKTOP_SHELL_PADDING: u16 = 16;

static DESKTOP_FONT_SIZE: AtomicU16 = AtomicU16::new(16);

pub(super) fn set_desktop_font_size(size: u16) {
    DESKTOP_FONT_SIZE.store(size.clamp(10, 24), Ordering::Relaxed);
}

pub(super) fn ui_size(design_size: u16) -> u32 {
    let base = DESKTOP_FONT_SIZE.load(Ordering::Relaxed).clamp(10, 24);
    u32::from(scaled_ui_size(design_size, base))
}

pub(super) fn scaled_ui_size(design_size: u16, base_size: u16) -> u16 {
    let base = base_size.clamp(10, 24);
    let scaled = (u32::from(design_size) * u32::from(base) + 8) / 16;
    scaled.clamp(1, 64) as u16
}

pub(super) fn desktop_scroll_gutter_right() -> f32 {
    f32::from(DESKTOP_SCROLLBAR_WIDTH + DESKTOP_SCROLLBAR_MARGIN + DESKTOP_SCROLL_GUTTER_EXTRA)
}

pub(super) fn compact_scrollbar() -> Scrollbar {
    Scrollbar::new()
        .width(u32::from(DESKTOP_SCROLLBAR_WIDTH))
        .scroller_width(u32::from(DESKTOP_SCROLLBAR_SCROLLER_WIDTH))
        .margin(u32::from(DESKTOP_SCROLLBAR_MARGIN))
}

pub(super) fn restored_desktop_panes(app: &App, omenchat_session_ids: &[u64]) -> Vec<DesktopPane> {
    let mut panes = app
        .settings
        .ui
        .desktop_workspace_panes
        .iter()
        .filter_map(|saved| match saved.kind {
            DesktopWorkspacePaneKind::Browser => app
                .workspace
                .browser_tabs
                .get(saved.index)
                .map(|tab| DesktopPane::Browser(tab.id)),
            DesktopWorkspacePaneKind::Conversation => app
                .workspace
                .conversations
                .get(saved.index)
                .map(|conversation| DesktopPane::Conversation(conversation.id)),
            DesktopWorkspacePaneKind::OmenChat => {
                #[cfg(feature = "chat-client")]
                {
                    omenchat_session_ids
                        .get(saved.index)
                        .copied()
                        .map(DesktopPane::OmenChat)
                }
                #[cfg(not(feature = "chat-client"))]
                {
                    let _ = omenchat_session_ids;
                    None
                }
            }
        })
        .collect::<Vec<_>>();

    if panes.is_empty() {
        panes.push(DesktopPane::Browser(app.active_browser_tab().id));
        panes.push(DesktopPane::Conversation(app.active_conversation().id));
    }
    panes.dedup();
    panes
}

pub(super) fn restored_desktop_pane_state(
    app: &App,
    omenchat_session_ids: &[u64],
) -> pane_grid::State<DesktopPane> {
    if let Some(layout) = app.settings.ui.desktop_workspace_layout.as_ref() {
        if let Some(config) =
            desktop_layout_node_to_configuration(layout, app, omenchat_session_ids)
        {
            return pane_grid::State::with_configuration(config);
        }
    }

    let restored_panes = restored_desktop_panes(app, omenchat_session_ids);
    let (mut state, first_pane) = pane_grid::State::new(
        restored_panes
            .first()
            .cloned()
            .unwrap_or_else(|| DesktopPane::Browser(app.active_browser_tab().id)),
    );
    for pane in restored_panes.into_iter().skip(1) {
        let target = desktop_pane_order(state.layout())
            .last()
            .copied()
            .unwrap_or(first_pane);
        let _ = state.split(pane_grid::Axis::Vertical, target, pane);
    }
    state
}

pub(super) fn desktop_layout_node_to_configuration(
    node: &DesktopWorkspaceLayoutNode,
    app: &App,
    omenchat_session_ids: &[u64],
) -> Option<pane_grid::Configuration<DesktopPane>> {
    match node {
        DesktopWorkspaceLayoutNode::Pane { pane } => {
            desktop_pane_from_settings(pane, app, omenchat_session_ids)
                .map(pane_grid::Configuration::Pane)
        }
        DesktopWorkspaceLayoutNode::Split { axis, ratio, a, b } => {
            let a = desktop_layout_node_to_configuration(a, app, omenchat_session_ids)?;
            let b = desktop_layout_node_to_configuration(b, app, omenchat_session_ids)?;
            Some(pane_grid::Configuration::Split {
                axis: desktop_split_axis_to_iced(*axis),
                ratio: sane_desktop_split_ratio(*ratio),
                a: Box::new(a),
                b: Box::new(b),
            })
        }
    }
}

fn desktop_pane_from_settings(
    pane: &DesktopWorkspacePaneSettings,
    app: &App,
    omenchat_session_ids: &[u64],
) -> Option<DesktopPane> {
    match pane.kind {
        DesktopWorkspacePaneKind::Browser => app
            .workspace
            .browser_tabs
            .get(pane.index)
            .map(|tab| DesktopPane::Browser(tab.id)),
        DesktopWorkspacePaneKind::Conversation => app
            .workspace
            .conversations
            .get(pane.index)
            .map(|conversation| DesktopPane::Conversation(conversation.id)),
        DesktopWorkspacePaneKind::OmenChat => {
            #[cfg(feature = "chat-client")]
            {
                omenchat_session_ids
                    .get(pane.index)
                    .copied()
                    .map(DesktopPane::OmenChat)
            }
            #[cfg(not(feature = "chat-client"))]
            {
                let _ = omenchat_session_ids;
                None
            }
        }
    }
}

pub(in crate::desktop) fn desktop_workspace_node_to_settings(
    node: &pane_grid::Node,
    desktop: &DesktopApp,
) -> Option<DesktopWorkspaceLayoutNode> {
    match node {
        pane_grid::Node::Pane(pane) => {
            let pane = desktop.workspace.workspace_panes.get(*pane)?;
            desktop
                .desktop_pane_to_settings(pane)
                .map(|pane| DesktopWorkspaceLayoutNode::Pane { pane })
        }
        pane_grid::Node::Split {
            axis, ratio, a, b, ..
        } => Some(DesktopWorkspaceLayoutNode::Split {
            axis: desktop_split_axis_from_iced(*axis),
            ratio: sane_desktop_split_ratio(*ratio),
            a: Box::new(desktop_workspace_node_to_settings(a, desktop)?),
            b: Box::new(desktop_workspace_node_to_settings(b, desktop)?),
        }),
    }
}

pub(super) fn desktop_pane_order(node: &pane_grid::Node) -> Vec<pane_grid::Pane> {
    match node {
        pane_grid::Node::Pane(pane) => vec![*pane],
        pane_grid::Node::Split { a, b, .. } => {
            let mut panes = desktop_pane_order(a);
            panes.extend(desktop_pane_order(b));
            panes
        }
    }
}

pub(super) fn sane_desktop_split_ratio(ratio: f32) -> f32 {
    if ratio.is_finite() {
        ratio.clamp(0.05, 0.95)
    } else {
        0.5
    }
}

pub(super) fn desktop_split_axis_to_iced(axis: DesktopWorkspaceSplitAxis) -> pane_grid::Axis {
    match axis {
        DesktopWorkspaceSplitAxis::Horizontal => pane_grid::Axis::Horizontal,
        DesktopWorkspaceSplitAxis::Vertical => pane_grid::Axis::Vertical,
    }
}

pub(super) fn desktop_split_axis_from_iced(axis: pane_grid::Axis) -> DesktopWorkspaceSplitAxis {
    match axis {
        pane_grid::Axis::Horizontal => DesktopWorkspaceSplitAxis::Horizontal,
        pane_grid::Axis::Vertical => DesktopWorkspaceSplitAxis::Vertical,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desktop_ui_size_scales_from_user_font_preference() {
        assert_eq!(scaled_ui_size(16, 10), 10);
        assert_eq!(scaled_ui_size(28, 10), 18);
        assert_eq!(scaled_ui_size(16, 24), 24);
        assert_eq!(scaled_ui_size(12, 24), 18);
    }

    #[test]
    fn shared_scroll_gutter_clears_scrollbar_rail() {
        let scrollbar_footprint = DESKTOP_SCROLLBAR_WIDTH + DESKTOP_SCROLLBAR_MARGIN;
        assert_eq!(
            desktop_scroll_gutter_right(),
            f32::from(scrollbar_footprint + DESKTOP_SCROLL_GUTTER_EXTRA)
        );
        assert!(desktop_scroll_gutter_right() >= f32::from(scrollbar_footprint + 10));
        const {
            assert!(DESKTOP_SCROLLBAR_SCROLLER_WIDTH <= DESKTOP_SCROLLBAR_WIDTH);
        }
    }

    #[test]
    fn desktop_shell_spacing_keeps_scrollbars_clear_of_panel_borders() {
        let scrollbar_footprint = DESKTOP_SCROLLBAR_WIDTH + DESKTOP_SCROLLBAR_MARGIN;

        const {
            assert!(DESKTOP_SCROLL_GUTTER_EXTRA >= DESKTOP_PANEL_PADDING);
            assert!(DESKTOP_SCROLL_OUTER_INSET >= DESKTOP_SCROLLBAR_SCROLLER_WIDTH);
            assert!(DESKTOP_SHELL_PADDING >= DESKTOP_PANEL_PADDING);
        }
        assert!(desktop_scroll_gutter_right() > f32::from(scrollbar_footprint));
    }
}
