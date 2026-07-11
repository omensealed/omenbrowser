pub(in crate::desktop) struct DesktopUiState {
    pub(in crate::desktop) navigation_open: bool,
    pub(in crate::desktop) identity_delete_confirming: bool,
    pub(in crate::desktop) ctrl_down: bool,
    pub(in crate::desktop) shutdown_requested: bool,
}

impl Default for DesktopUiState {
    fn default() -> Self {
        Self {
            navigation_open: true,
            identity_delete_confirming: false,
            ctrl_down: false,
            shutdown_requested: false,
        }
    }
}
