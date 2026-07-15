use super::widgets::ProcessResourceUsage;

#[derive(Default)]
pub(in crate::desktop) struct DesktopMonitoringState {
    pub(in crate::desktop) sample_epoch_ms: u64,
    pub(in crate::desktop) process_usage: Option<ProcessResourceUsage>,
}
