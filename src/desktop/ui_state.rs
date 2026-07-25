#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(in crate::desktop) enum ShutdownPhase {
    #[default]
    Running,
    ShutdownRequested,
    Draining,
    Closed,
}

impl ShutdownPhase {
    pub(in crate::desktop) fn is_running(self) -> bool {
        self == Self::Running
    }

    pub(in crate::desktop) fn request(&mut self) -> bool {
        if !self.is_running() {
            return false;
        }
        *self = Self::ShutdownRequested;
        true
    }

    pub(in crate::desktop) fn begin_draining(&mut self) -> bool {
        if *self != Self::ShutdownRequested {
            return false;
        }
        *self = Self::Draining;
        true
    }

    pub(in crate::desktop) fn finish(&mut self) -> bool {
        if *self != Self::Draining {
            return false;
        }
        *self = Self::Closed;
        true
    }
}

pub(in crate::desktop) struct DesktopUiState {
    pub(in crate::desktop) navigation_open: bool,
    pub(in crate::desktop) identity_delete_confirming: bool,
    pub(in crate::desktop) command_palette_open: bool,
    pub(in crate::desktop) command_palette_query: String,
    pub(in crate::desktop) ctrl_down: bool,
    pub(in crate::desktop) shutdown_phase: ShutdownPhase,
}

impl Default for DesktopUiState {
    fn default() -> Self {
        Self {
            navigation_open: true,
            identity_delete_confirming: false,
            command_palette_open: false,
            command_palette_query: String::new(),
            ctrl_down: false,
            shutdown_phase: ShutdownPhase::Running,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ShutdownPhase;

    #[test]
    fn shutdown_phase_allows_only_the_ordered_lifecycle() {
        let mut phase = ShutdownPhase::Running;
        assert!(!phase.begin_draining());
        assert!(phase.request());
        assert!(!phase.request());
        assert!(phase.begin_draining());
        assert!(!phase.begin_draining());
        assert!(phase.finish());
        assert!(!phase.finish());
        assert_eq!(phase, ShutdownPhase::Closed);
    }
}
