use iced::widget::pane_grid;

use super::DesktopPane;

pub(in crate::desktop) struct DesktopWorkspaceState {
    pub(in crate::desktop) workspace_panes: pane_grid::State<DesktopPane>,
    pub(in crate::desktop) active_workspace_pane: pane_grid::Pane,
    pub(in crate::desktop) restore_workspace_scrolls_pending: bool,
    pub(in crate::desktop) restore_workspace_scrolls_remaining: u8,
    pub(in crate::desktop) restore_workspace_scroll_locks_release_pending: bool,
    pub(in crate::desktop) pending_workspace_bottom_anchor_ticks: u8,
}

impl DesktopWorkspaceState {
    pub(in crate::desktop) fn from_startup(
        workspace_panes: pane_grid::State<DesktopPane>,
        active_workspace_pane: pane_grid::Pane,
        restore_workspace_scrolls_pending: bool,
    ) -> Self {
        Self {
            workspace_panes,
            active_workspace_pane,
            restore_workspace_scrolls_pending,
            restore_workspace_scrolls_remaining: if restore_workspace_scrolls_pending {
                5
            } else {
                0
            },
            restore_workspace_scroll_locks_release_pending: false,
            pending_workspace_bottom_anchor_ticks: if restore_workspace_scrolls_pending {
                2
            } else {
                0
            },
        }
    }
}
