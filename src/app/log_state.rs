use std::path::PathBuf;
use std::time::Duration;

use super::current_epoch_ms;
use crate::structured_log_reader::{
    load_recent_log_entries, PersistedLogLimits, PersistedLogStats,
};
use crate::structured_log_writer::{
    enforce_structured_log_retention, StructuredLogDiskPolicy, StructuredLogDiskStats,
    StructuredLogFlushHandle, StructuredLogWorker, StructuredLogWorkerMetrics,
};

pub(super) const STRUCTURED_LOG_DEFAULT_MAX_BYTES: u64 = 256 * 1024;
pub(super) const STRUCTURED_LOG_MESSAGE_BYTES: usize = 16 * 1024;
pub(super) const STRUCTURED_LOG_MEMORY_ENTRY_LIMIT: usize = 4096;
pub(super) const STRUCTURED_LOG_MEMORY_BYTES: usize = 4 * 1024 * 1024;
pub(super) const STRUCTURED_LOG_STARTUP_ENTRY_LIMIT: usize = 4096;
pub(super) const STRUCTURED_LOG_STARTUP_DIRECTORY_ENTRY_LIMIT: usize = 4096;
pub(super) const STRUCTURED_LOG_STARTUP_FILE_LIMIT: usize = 16;
pub(super) const STRUCTURED_LOG_STARTUP_FILE_BYTES: usize = 512 * 1024;
pub(super) const STRUCTURED_LOG_STARTUP_TOTAL_BYTES: usize = 4 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Default, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub enum LogSeverity {
    Debug,
    #[default]
    Info,
    Warn,
    Error,
}

impl LogSeverity {
    const ALL: [Self; 4] = [Self::Debug, Self::Info, Self::Warn, Self::Error];
}

#[derive(Clone, Copy, Debug, Default, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub enum LogSource {
    #[default]
    App,
    Runtime,
    Directory,
    Messaging,
    Diagnostics,
    Interface,
    Plugin,
}

impl LogSource {
    const ALL: [Self; 7] = [
        Self::App,
        Self::Runtime,
        Self::Directory,
        Self::Messaging,
        Self::Diagnostics,
        Self::Interface,
        Self::Plugin,
    ];
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct LogEntry {
    pub epoch_ms: u64,
    pub severity: LogSeverity,
    pub source: LogSource,
    pub message: String,
}

#[derive(Debug)]
pub struct LogBuffer {
    pub lines: Vec<String>,
    pub entries: Vec<LogEntry>,
    pub severity_filter: Option<LogSeverity>,
    pub source_filter: Option<LogSource>,
    log_file: Option<PathBuf>,
    pub(super) max_file_bytes: u64,
    pub(super) retain_files: usize,
    pub(super) memory_bytes: usize,
    pub(super) startup_load_stats: PersistedLogStats,
    disk_stats: StructuredLogDiskStats,
    worker: Option<StructuredLogWorker>,
}

impl LogBuffer {
    pub(super) fn with_persistence(
        logs_dir: PathBuf,
        max_file_bytes: u64,
        retain_files: usize,
        load_recent_entries: usize,
    ) -> Self {
        let _ = std::fs::create_dir_all(&logs_dir);
        let policy = StructuredLogDiskPolicy::normalized(max_file_bytes, retain_files);
        let log_file = logs_dir.join("omenbrowser_rs.jsonl");
        let worker = StructuredLogWorker::start();
        let worker_start_warning = worker.as_ref().err().map(|error| {
            format!("structured log persistence disabled: writer start failed: {error}")
        });
        let mut buffer = Self {
            log_file: Some(log_file.clone()),
            max_file_bytes: policy.max_file_bytes,
            retain_files: policy.retain_files,
            disk_stats: enforce_structured_log_retention(&log_file, policy),
            worker: worker.ok(),
            ..Self::default()
        };
        buffer.load_recent_entries(load_recent_entries);
        if let Some(warning) = worker_start_warning {
            buffer.push_with_source(LogSeverity::Warn, LogSource::App, warning);
        }
        buffer
    }

    pub(super) fn push(&mut self, severity: LogSeverity, message: impl Into<String>) {
        self.push_with_source(severity, LogSource::App, message);
    }

    pub(super) fn push_with_source(
        &mut self,
        severity: LogSeverity,
        source: LogSource,
        message: impl Into<String>,
    ) {
        let message = bounded_log_message(message.into());
        self.lines.push(message.clone());
        let entry = LogEntry {
            epoch_ms: current_epoch_ms(),
            severity,
            source,
            message,
        };
        self.persist_entry(&entry);
        self.memory_bytes = self
            .memory_bytes
            .saturating_add(log_entry_memory_bytes(&entry));
        self.entries.push(entry);
        self.enforce_memory_budget();
    }

    pub(super) fn clear_display(&mut self) {
        self.lines.clear();
        self.entries.clear();
        self.memory_bytes = 0;
        self.severity_filter = None;
        self.source_filter = None;
    }

    pub(crate) fn filtered_entries(&self) -> Vec<&LogEntry> {
        self.entries
            .iter()
            .filter(|entry| {
                self.severity_filter
                    .is_none_or(|severity| entry.severity == severity)
                    && self
                        .source_filter
                        .is_none_or(|source| entry.source == source)
            })
            .collect()
    }

    pub(super) fn cycle_severity_filter(&mut self) -> Option<LogSeverity> {
        self.severity_filter = match self.severity_filter {
            None => Some(LogSeverity::Debug),
            Some(current) => LogSeverity::ALL
                .iter()
                .position(|candidate| *candidate == current)
                .and_then(|index| LogSeverity::ALL.get(index + 1).copied()),
        };
        self.severity_filter
    }

    pub(super) fn cycle_source_filter(&mut self) -> Option<LogSource> {
        self.source_filter = match self.source_filter {
            None => Some(LogSource::App),
            Some(current) => LogSource::ALL
                .iter()
                .position(|candidate| *candidate == current)
                .and_then(|index| LogSource::ALL.get(index + 1).copied()),
        };
        self.source_filter
    }

    fn persist_entry(&mut self, entry: &LogEntry) {
        let Some(path) = &self.log_file else {
            return;
        };
        let policy = StructuredLogDiskPolicy::normalized(self.max_file_bytes, self.retain_files);
        if let Some(worker) = &self.worker {
            let _ = worker.enqueue(path, entry.clone(), policy);
        } else {
            let stats = StructuredLogDiskStats {
                write_failures: 1,
                ..StructuredLogDiskStats::default()
            };
            self.record_disk_stats(stats);
        }
    }

    pub(super) fn flush(&self, timeout: Duration) -> bool {
        self.worker
            .as_ref()
            .is_none_or(|worker| worker.flush_handle().flush(timeout))
    }

    pub(super) fn flush_handle(&self) -> Option<StructuredLogFlushHandle> {
        self.worker.as_ref().map(StructuredLogWorker::flush_handle)
    }

    pub(super) fn worker_metrics(&self) -> StructuredLogWorkerMetrics {
        let mut metrics = self.worker.as_ref().map_or_else(
            StructuredLogWorkerMetrics::default,
            StructuredLogWorker::metrics,
        );
        metrics.write_failures = metrics
            .write_failures
            .saturating_add(self.disk_stats.write_failures as u64);
        metrics.rotations = metrics
            .rotations
            .saturating_add(self.disk_stats.rotations as u64);
        metrics.removed_files = metrics
            .removed_files
            .saturating_add(self.disk_stats.removed_files as u64);
        metrics.removal_failures = metrics
            .removal_failures
            .saturating_add(self.disk_stats.removal_failures as u64);
        metrics.unsafe_paths_refused = metrics
            .unsafe_paths_refused
            .saturating_add(self.disk_stats.unsafe_paths_refused as u64);
        metrics.truncated_directory_scans = metrics
            .truncated_directory_scans
            .saturating_add(u64::from(self.disk_stats.directory_scan_truncated));
        metrics
    }

    fn record_disk_stats(&mut self, stats: StructuredLogDiskStats) {
        // Persistence failures remain non-fatal and must not recursively log themselves.
        self.disk_stats.directory_entries_scanned = self
            .disk_stats
            .directory_entries_scanned
            .saturating_add(stats.directory_entries_scanned);
        self.disk_stats.directory_scan_truncated |= stats.directory_scan_truncated;
        self.disk_stats.matching_rotated_files = stats.matching_rotated_files;
        self.disk_stats.removed_files = self
            .disk_stats
            .removed_files
            .saturating_add(stats.removed_files);
        self.disk_stats.removal_failures = self
            .disk_stats
            .removal_failures
            .saturating_add(stats.removal_failures);
        self.disk_stats.rotations = self.disk_stats.rotations.saturating_add(stats.rotations);
        self.disk_stats.write_failures = self
            .disk_stats
            .write_failures
            .saturating_add(stats.write_failures);
        self.disk_stats.unsafe_paths_refused = self
            .disk_stats
            .unsafe_paths_refused
            .saturating_add(stats.unsafe_paths_refused);
    }

    fn load_recent_entries(&mut self, limit: usize) {
        if limit == 0 {
            self.lines.clear();
            self.entries.clear();
            self.memory_bytes = 0;
            self.startup_load_stats = PersistedLogStats::default();
            return;
        }
        let Some(path) = &self.log_file else {
            return;
        };
        let Some(logs_dir) = path.parent() else {
            return;
        };
        let effective_limit = limit.min(STRUCTURED_LOG_STARTUP_ENTRY_LIMIT);
        let loaded = load_recent_log_entries(
            logs_dir,
            PersistedLogLimits {
                entry_limit: effective_limit,
                directory_entry_limit: STRUCTURED_LOG_STARTUP_DIRECTORY_ENTRY_LIMIT,
                file_limit: self
                    .retain_files
                    .saturating_add(1)
                    .clamp(1, STRUCTURED_LOG_STARTUP_FILE_LIMIT),
                file_bytes: STRUCTURED_LOG_STARTUP_FILE_BYTES,
                total_bytes: STRUCTURED_LOG_STARTUP_TOTAL_BYTES,
            },
        );
        self.startup_load_stats = loaded.stats;
        self.entries = loaded
            .entries
            .into_iter()
            .map(|mut entry| {
                entry.message = bounded_log_message(entry.message);
                entry
            })
            .collect();
        self.lines = self
            .entries
            .iter()
            .map(|entry| entry.message.clone())
            .collect();
        self.memory_bytes = self.entries.iter().map(log_entry_memory_bytes).sum();
        self.enforce_memory_budget();
        if limit > effective_limit {
            self.push_with_source(
                LogSeverity::Warn,
                LogSource::App,
                format!(
                    "structured log startup entries capped at {effective_limit} (requested {limit})"
                ),
            );
        }
    }

    fn enforce_memory_budget(&mut self) {
        let mut remove_count = 0;
        let mut retained_bytes = self.memory_bytes;
        while self.entries.len().saturating_sub(remove_count) > STRUCTURED_LOG_MEMORY_ENTRY_LIMIT
            || retained_bytes > STRUCTURED_LOG_MEMORY_BYTES
        {
            let Some(entry) = self.entries.get(remove_count) else {
                break;
            };
            retained_bytes = retained_bytes.saturating_sub(log_entry_memory_bytes(entry));
            remove_count += 1;
        }
        if remove_count > 0 {
            self.entries.drain(..remove_count);
            self.lines.drain(..remove_count.min(self.lines.len()));
        }
        self.memory_bytes = retained_bytes;
    }
}

impl Default for LogBuffer {
    fn default() -> Self {
        Self {
            lines: Vec::new(),
            entries: Vec::new(),
            severity_filter: None,
            source_filter: None,
            log_file: None,
            max_file_bytes: STRUCTURED_LOG_DEFAULT_MAX_BYTES,
            retain_files: 4,
            memory_bytes: 0,
            startup_load_stats: PersistedLogStats::default(),
            disk_stats: StructuredLogDiskStats::default(),
            worker: None,
        }
    }
}

fn bounded_log_message(message: String) -> String {
    const MARKER: &str = "...<truncated>";
    if message.len() <= STRUCTURED_LOG_MESSAGE_BYTES {
        return message.as_str().to_owned();
    }
    let mut end = STRUCTURED_LOG_MESSAGE_BYTES.saturating_sub(MARKER.len());
    while !message.is_char_boundary(end) {
        end = end.saturating_sub(1);
    }
    let mut bounded = String::with_capacity(end.saturating_add(MARKER.len()));
    bounded.push_str(&message[..end]);
    bounded.push_str(MARKER);
    bounded
}

fn log_entry_memory_bytes(entry: &LogEntry) -> usize {
    entry.message.len().saturating_mul(2).saturating_add(128)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn structured_log_memory_is_item_byte_and_message_bounded() {
        let mut logs = LogBuffer::default();
        for index in 0..500 {
            logs.push_with_source(
                LogSeverity::Info,
                LogSource::Runtime,
                format!("{index}:{}", "é".repeat(STRUCTURED_LOG_MESSAGE_BYTES)),
            );
        }

        assert!(logs.entries.len() <= STRUCTURED_LOG_MEMORY_ENTRY_LIMIT);
        assert_eq!(logs.lines.len(), logs.entries.len());
        assert!(logs.memory_bytes <= STRUCTURED_LOG_MEMORY_BYTES);
        assert!(logs
            .entries
            .iter()
            .all(|entry| entry.message.len() <= STRUCTURED_LOG_MESSAGE_BYTES));
        assert!(logs
            .entries
            .iter()
            .all(|entry| entry.message.capacity() <= STRUCTURED_LOG_MESSAGE_BYTES));
        assert!(logs
            .lines
            .iter()
            .all(|line| line.capacity() <= STRUCTURED_LOG_MESSAGE_BYTES));
        assert!(logs
            .entries
            .last()
            .is_some_and(|entry| entry.message.starts_with("499:")));
        assert!(logs
            .entries
            .last()
            .is_some_and(|entry| entry.message.ends_with("...<truncated>")));

        let mut short_with_excess_capacity = String::with_capacity(1024 * 1024);
        short_with_excess_capacity.push_str("short");
        logs.push(LogSeverity::Info, short_with_excess_capacity);
        assert_eq!(logs.entries.last().expect("short entry").message, "short");
        assert!(
            logs.entries.last().expect("short entry").message.capacity()
                <= STRUCTURED_LOG_MESSAGE_BYTES
        );

        let mut item_limited = LogBuffer::default();
        for index in 0..STRUCTURED_LOG_MEMORY_ENTRY_LIMIT + 7 {
            item_limited.push(LogSeverity::Info, format!("item-{index}"));
        }
        assert_eq!(
            item_limited.entries.len(),
            STRUCTURED_LOG_MEMORY_ENTRY_LIMIT
        );
        assert_eq!(item_limited.lines.len(), item_limited.entries.len());
        assert!(item_limited.memory_bytes <= STRUCTURED_LOG_MEMORY_BYTES);
        assert_eq!(item_limited.entries[0].message, "item-7");
    }

    #[test]
    fn filters_cycle_in_stable_display_order() {
        let mut logs = LogBuffer::default();
        assert_eq!(logs.cycle_severity_filter(), Some(LogSeverity::Debug));
        assert_eq!(logs.cycle_severity_filter(), Some(LogSeverity::Info));
        assert_eq!(logs.cycle_severity_filter(), Some(LogSeverity::Warn));
        assert_eq!(logs.cycle_severity_filter(), Some(LogSeverity::Error));
        assert_eq!(logs.cycle_severity_filter(), None);

        assert_eq!(logs.cycle_source_filter(), Some(LogSource::App));
        assert_eq!(logs.cycle_source_filter(), Some(LogSource::Runtime));
        assert_eq!(logs.cycle_source_filter(), Some(LogSource::Directory));
        assert_eq!(logs.cycle_source_filter(), Some(LogSource::Messaging));
        assert_eq!(logs.cycle_source_filter(), Some(LogSource::Diagnostics));
        assert_eq!(logs.cycle_source_filter(), Some(LogSource::Interface));
        assert_eq!(logs.cycle_source_filter(), Some(LogSource::Plugin));
        assert_eq!(logs.cycle_source_filter(), None);
    }
}
