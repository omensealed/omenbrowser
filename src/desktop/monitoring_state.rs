use super::widgets::ProcessResourceUsage;

pub(in crate::desktop) struct DesktopMonitoringState {
    pub(in crate::desktop) sample_epoch_ms: u64,
    pub(in crate::desktop) process_usage: Option<ProcessResourceUsage>,
    pub(in crate::desktop) debug_tick_count: u64,
    pub(in crate::desktop) debug_last_tick_epoch_ms: u64,
}

impl Default for DesktopMonitoringState {
    fn default() -> Self {
        Self {
            sample_epoch_ms: 0,
            process_usage: None,
            debug_tick_count: 0,
            debug_last_tick_epoch_ms: 0,
        }
    }
}
