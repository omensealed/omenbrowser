//! Bounded on-disk writer and retention policy for browser structured logs.

use std::{
    collections::VecDeque,
    fs::File,
    io::Write,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc, Arc, Mutex,
    },
    thread::JoinHandle,
    time::{Duration, UNIX_EPOCH},
};

use crate::app::{current_epoch_ms, LogEntry};

pub const STRUCTURED_LOG_MIN_FILE_BYTES: u64 = 4 * 1024;
pub const STRUCTURED_LOG_MAX_FILE_BYTES: u64 = 8 * 1024 * 1024;
pub const STRUCTURED_LOG_MAX_RETAIN_FILES: usize = 16;
pub const STRUCTURED_LOG_PRUNE_SCAN_LIMIT: usize = 4096;
pub const STRUCTURED_LOG_QUEUE_ITEMS: usize = 256;
pub const STRUCTURED_LOG_QUEUE_BYTES: usize = 2 * 1024 * 1024;
const STRUCTURED_LOG_CONTROL_ITEMS: usize = 4;

static ROTATION_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StructuredLogDiskPolicy {
    pub max_file_bytes: u64,
    pub retain_files: usize,
}

impl StructuredLogDiskPolicy {
    pub fn normalized(max_file_bytes: u64, retain_files: usize) -> Self {
        Self {
            max_file_bytes: max_file_bytes
                .clamp(STRUCTURED_LOG_MIN_FILE_BYTES, STRUCTURED_LOG_MAX_FILE_BYTES),
            retain_files: retain_files.clamp(1, STRUCTURED_LOG_MAX_RETAIN_FILES),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StructuredLogDiskStats {
    pub directory_entries_scanned: usize,
    pub directory_scan_truncated: bool,
    pub matching_rotated_files: usize,
    pub removed_files: usize,
    pub removal_failures: usize,
    pub rotations: usize,
    pub write_failures: usize,
    pub unsafe_paths_refused: usize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StructuredLogWorkerMetrics {
    pub queued_items: usize,
    pub queued_bytes: usize,
    pub oldest_age_ms: u64,
    pub dropped_records: u64,
    pub completed_records: u64,
    pub write_failures: u64,
    pub rotations: u64,
    pub removed_files: u64,
    pub removal_failures: u64,
    pub unsafe_paths_refused: u64,
    pub truncated_directory_scans: u64,
}

struct QueueState {
    queued_bytes: usize,
    next_id: u64,
    queued: VecDeque<(u64, u64)>,
    dropped_records: u64,
    completed_records: u64,
    write_failures: u64,
    rotations: u64,
    removed_files: u64,
    removal_failures: u64,
    unsafe_paths_refused: u64,
    truncated_directory_scans: u64,
}

impl QueueState {
    fn new() -> Self {
        Self {
            queued_bytes: 0,
            next_id: 1,
            queued: VecDeque::with_capacity(STRUCTURED_LOG_QUEUE_ITEMS),
            dropped_records: 0,
            completed_records: 0,
            write_failures: 0,
            rotations: 0,
            removed_files: 0,
            removal_failures: 0,
            unsafe_paths_refused: 0,
            truncated_directory_scans: 0,
        }
    }
}

struct QueueBudget {
    state: Mutex<QueueState>,
}

impl QueueBudget {
    fn new() -> Self {
        Self {
            state: Mutex::new(QueueState::new()),
        }
    }

    fn reserve(self: &Arc<Self>, bytes: usize) -> Option<QueuePermit> {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        if bytes > STRUCTURED_LOG_QUEUE_BYTES
            || state.queued.len() >= STRUCTURED_LOG_QUEUE_ITEMS
            || state.queued_bytes.saturating_add(bytes) > STRUCTURED_LOG_QUEUE_BYTES
        {
            state.dropped_records = state.dropped_records.saturating_add(1);
            return None;
        }
        let id = state.next_id;
        state.next_id = state.next_id.wrapping_add(1).max(1);
        state.queued_bytes = state.queued_bytes.saturating_add(bytes);
        state.queued.push_back((id, current_epoch_ms()));
        Some(QueuePermit {
            budget: self.clone(),
            id,
            bytes,
        })
    }

    fn record_send_failure(&self) {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        state.dropped_records = state.dropped_records.saturating_add(1);
    }

    fn record_completion(&self, stats: StructuredLogDiskStats) {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        state.completed_records = state.completed_records.saturating_add(1);
        state.write_failures = state
            .write_failures
            .saturating_add(stats.write_failures as u64);
        state.rotations = state.rotations.saturating_add(stats.rotations as u64);
        state.removed_files = state
            .removed_files
            .saturating_add(stats.removed_files as u64);
        state.removal_failures = state
            .removal_failures
            .saturating_add(stats.removal_failures as u64);
        state.unsafe_paths_refused = state
            .unsafe_paths_refused
            .saturating_add(stats.unsafe_paths_refused as u64);
        state.truncated_directory_scans = state
            .truncated_directory_scans
            .saturating_add(u64::from(stats.directory_scan_truncated));
    }

    fn snapshot(&self) -> StructuredLogWorkerMetrics {
        let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        let oldest_age_ms = state.queued.front().map_or(0, |(_, queued_epoch_ms)| {
            current_epoch_ms().saturating_sub(*queued_epoch_ms)
        });
        StructuredLogWorkerMetrics {
            queued_items: state.queued.len(),
            queued_bytes: state.queued_bytes,
            oldest_age_ms,
            dropped_records: state.dropped_records,
            completed_records: state.completed_records,
            write_failures: state.write_failures,
            rotations: state.rotations,
            removed_files: state.removed_files,
            removal_failures: state.removal_failures,
            unsafe_paths_refused: state.unsafe_paths_refused,
            truncated_directory_scans: state.truncated_directory_scans,
        }
    }
}

struct QueuePermit {
    budget: Arc<QueueBudget>,
    id: u64,
    bytes: usize,
}

impl Drop for QueuePermit {
    fn drop(&mut self) {
        let mut state = self
            .budget
            .state
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if let Some(index) = state.queued.iter().position(|(id, _)| *id == self.id) {
            state.queued.remove(index);
            state.queued_bytes = state.queued_bytes.saturating_sub(self.bytes);
        }
    }
}

struct StructuredLogJob {
    path: PathBuf,
    entry: LogEntry,
    policy: StructuredLogDiskPolicy,
    _permit: QueuePermit,
}

enum WorkerControl {
    Flush(mpsc::SyncSender<()>),
    Shutdown(mpsc::SyncSender<()>),
}

#[derive(Clone)]
pub struct StructuredLogFlushHandle {
    control: mpsc::SyncSender<WorkerControl>,
    budget: Arc<QueueBudget>,
}

impl std::fmt::Debug for StructuredLogFlushHandle {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("StructuredLogFlushHandle")
            .field("metrics", &self.metrics())
            .finish_non_exhaustive()
    }
}

impl StructuredLogFlushHandle {
    pub fn flush(&self, timeout: Duration) -> bool {
        let (ack_tx, ack_rx) = mpsc::sync_channel(1);
        if self.control.try_send(WorkerControl::Flush(ack_tx)).is_err() {
            return false;
        }
        ack_rx.recv_timeout(timeout).is_ok()
    }

    pub fn metrics(&self) -> StructuredLogWorkerMetrics {
        self.budget.snapshot()
    }
}

pub struct StructuredLogWorker {
    records: Option<mpsc::SyncSender<StructuredLogJob>>,
    flush_handle: StructuredLogFlushHandle,
    worker: Option<JoinHandle<()>>,
}

impl std::fmt::Debug for StructuredLogWorker {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("StructuredLogWorker")
            .field("metrics", &self.metrics())
            .finish_non_exhaustive()
    }
}

impl StructuredLogWorker {
    pub fn start() -> std::io::Result<Self> {
        Self::start_with_delay(Duration::ZERO)
    }

    fn start_with_delay(write_delay: Duration) -> std::io::Result<Self> {
        let (records, record_rx) = mpsc::sync_channel(STRUCTURED_LOG_QUEUE_ITEMS);
        let (control, control_rx) = mpsc::sync_channel(STRUCTURED_LOG_CONTROL_ITEMS);
        let budget = Arc::new(QueueBudget::new());
        let worker_budget = budget.clone();
        let worker = std::thread::Builder::new()
            .name("omenbrowser-log-writer".into())
            .spawn(move || run_worker(record_rx, control_rx, worker_budget, write_delay))?;
        Ok(Self {
            records: Some(records),
            flush_handle: StructuredLogFlushHandle { control, budget },
            worker: Some(worker),
        })
    }

    pub fn enqueue(&self, path: &Path, entry: LogEntry, policy: StructuredLogDiskPolicy) -> bool {
        let bytes = queued_job_bytes(path, &entry);
        let Some(permit) = self.flush_handle.budget.reserve(bytes) else {
            return false;
        };
        let Some(records) = &self.records else {
            self.flush_handle.budget.record_send_failure();
            return false;
        };
        if records
            .try_send(StructuredLogJob {
                path: path.to_owned(),
                entry,
                policy,
                _permit: permit,
            })
            .is_err()
        {
            self.flush_handle.budget.record_send_failure();
            return false;
        }
        true
    }

    pub fn flush_handle(&self) -> StructuredLogFlushHandle {
        self.flush_handle.clone()
    }

    pub fn metrics(&self) -> StructuredLogWorkerMetrics {
        self.flush_handle.metrics()
    }

    pub fn shutdown(&mut self, timeout: Duration) -> bool {
        let (ack_tx, ack_rx) = mpsc::sync_channel(1);
        let requested = self
            .flush_handle
            .control
            .try_send(WorkerControl::Shutdown(ack_tx))
            .is_ok();
        let acknowledged = requested && ack_rx.recv_timeout(timeout).is_ok();
        self.records.take();
        if acknowledged {
            if let Some(worker) = self.worker.take() {
                return worker.join().is_ok();
            }
        }
        acknowledged
    }
}

impl Drop for StructuredLogWorker {
    fn drop(&mut self) {
        let _ = self.shutdown(Duration::from_secs(3));
    }
}

fn run_worker(
    records: mpsc::Receiver<StructuredLogJob>,
    control: mpsc::Receiver<WorkerControl>,
    budget: Arc<QueueBudget>,
    write_delay: Duration,
) {
    loop {
        while let Ok(command) = control.try_recv() {
            match command {
                WorkerControl::Flush(ack) => {
                    drain_records(&records, &budget, write_delay);
                    let _ = ack.send(());
                }
                WorkerControl::Shutdown(ack) => {
                    drain_records(&records, &budget, write_delay);
                    let _ = ack.send(());
                    return;
                }
            }
        }
        match records.recv_timeout(Duration::from_millis(10)) {
            Ok(job) => process_job(job, &budget, write_delay),
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                drain_records(&records, &budget, write_delay);
                return;
            }
        }
    }
}

fn drain_records(
    records: &mpsc::Receiver<StructuredLogJob>,
    budget: &QueueBudget,
    write_delay: Duration,
) {
    while let Ok(job) = records.try_recv() {
        process_job(job, budget, write_delay);
    }
}

fn process_job(job: StructuredLogJob, budget: &QueueBudget, write_delay: Duration) {
    if !write_delay.is_zero() {
        std::thread::sleep(write_delay);
    }
    let stats = append_structured_log_entry(&job.path, &job.entry, job.policy);
    budget.record_completion(stats);
}

fn queued_job_bytes(path: &Path, entry: &LogEntry) -> usize {
    std::mem::size_of::<StructuredLogJob>()
        .saturating_add(path.as_os_str().len())
        .saturating_add(entry.message.len())
}

pub fn enforce_structured_log_retention(
    active_path: &Path,
    policy: StructuredLogDiskPolicy,
) -> StructuredLogDiskStats {
    prune_rotated_logs(active_path, policy)
}

pub fn append_structured_log_entry(
    active_path: &Path,
    entry: &LogEntry,
    policy: StructuredLogDiskPolicy,
) -> StructuredLogDiskStats {
    let mut stats = StructuredLogDiskStats::default();
    if !secure_parent(active_path) || !regular_or_missing(active_path) {
        stats.unsafe_paths_refused = 1;
        return stats;
    }
    if crate::private_fs::repair_private_file_if_exists(active_path).is_err() {
        stats.unsafe_paths_refused = 1;
        return stats;
    }

    let Some(encoded) = encode_entry(entry, policy.max_file_bytes as usize) else {
        stats.write_failures = 1;
        return stats;
    };
    let current_bytes = active_path.metadata().map_or(0, |metadata| metadata.len());
    if current_bytes > 0
        && current_bytes.saturating_add(encoded.len() as u64) > policy.max_file_bytes
    {
        let rotated_path = next_rotated_path(active_path, entry.epoch_ms);
        match rotated_path.and_then(|path| std::fs::rename(active_path, path).ok()) {
            Some(()) => {
                stats.rotations = 1;
                merge_stats(&mut stats, prune_rotated_logs(active_path, policy));
            }
            None => {
                stats.write_failures = 1;
                return stats;
            }
        }
    }

    if !regular_or_missing(active_path) {
        stats.unsafe_paths_refused += 1;
        return stats;
    }
    let Ok(mut file) = open_append(active_path) else {
        stats.write_failures += 1;
        return stats;
    };
    if write_encoded_record(&mut file, &encoded).is_err() {
        stats.write_failures += 1;
    }
    stats
}

fn write_encoded_record(writer: &mut impl Write, encoded: &[u8]) -> std::io::Result<()> {
    writer.write_all(encoded)
}

fn secure_parent(path: &Path) -> bool {
    let Some(parent) = path.parent() else {
        return false;
    };
    if crate::private_fs::ensure_private_dir(parent).is_err() {
        return false;
    }
    parent
        .symlink_metadata()
        .is_ok_and(|metadata| metadata.file_type().is_dir())
}

fn regular_or_missing(path: &Path) -> bool {
    match path.symlink_metadata() {
        Ok(metadata) => metadata.file_type().is_file(),
        Err(error) => error.kind() == std::io::ErrorKind::NotFound,
    }
}

fn open_append(path: &Path) -> std::io::Result<File> {
    crate::private_fs::open_private_append(path)
}

fn encode_entry(entry: &LogEntry, byte_limit: usize) -> Option<Vec<u8>> {
    let mut candidate = entry.clone();
    let mut encoded = serde_json::to_vec(&candidate).ok()?;
    encoded.push(b'\n');
    if encoded.len() <= byte_limit {
        return Some(encoded);
    }

    const MARKER: &str = "...<truncated>";
    let boundaries = candidate
        .message
        .char_indices()
        .map(|(index, _)| index)
        .chain(std::iter::once(candidate.message.len()))
        .collect::<Vec<_>>();
    let original = candidate.message.clone();
    let mut low = 0;
    let mut high = boundaries.len();
    let mut best = None;
    while low < high {
        let middle = low + (high - low) / 2;
        let end = boundaries[middle];
        candidate.message = format!("{}{MARKER}", &original[..end]);
        let mut trial = serde_json::to_vec(&candidate).ok()?;
        trial.push(b'\n');
        if trial.len() <= byte_limit {
            best = Some(trial);
            low = middle + 1;
        } else {
            high = middle;
        }
    }
    best
}

fn next_rotated_path(active_path: &Path, epoch_ms: u64) -> Option<PathBuf> {
    let parent = active_path.parent()?;
    for _ in 0..=STRUCTURED_LOG_MAX_RETAIN_FILES {
        let sequence = ROTATION_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let candidate = parent.join(format!("omenbrowser_rs-{epoch_ms}-{sequence}.jsonl"));
        if candidate
            .symlink_metadata()
            .is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound)
        {
            return Some(candidate);
        }
    }
    None
}

fn prune_rotated_logs(
    active_path: &Path,
    policy: StructuredLogDiskPolicy,
) -> StructuredLogDiskStats {
    let mut stats = StructuredLogDiskStats::default();
    let Some(parent) = active_path.parent() else {
        return stats;
    };
    let Ok(entries) = std::fs::read_dir(parent) else {
        return stats;
    };
    let mut rotated = Vec::with_capacity(policy.retain_files.saturating_add(1));
    for entry in entries {
        if stats.directory_entries_scanned == STRUCTURED_LOG_PRUNE_SCAN_LIMIT {
            stats.directory_scan_truncated = true;
            break;
        }
        stats.directory_entries_scanned += 1;
        let Ok(entry) = entry else {
            continue;
        };
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        if !(name.starts_with("omenbrowser_rs-") && name.ends_with(".jsonl")) {
            continue;
        }
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if !file_type.is_file() {
            continue;
        }
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        if crate::private_fs::repair_private_file(&entry.path()).is_err() {
            stats.unsafe_paths_refused += 1;
            continue;
        }
        stats.matching_rotated_files += 1;
        rotated.push((metadata.modified().unwrap_or(UNIX_EPOCH), entry.path()));
    }
    rotated.sort_by(|left, right| left.0.cmp(&right.0).then_with(|| left.1.cmp(&right.1)));
    let remove_count = rotated.len().saturating_sub(policy.retain_files);
    for (_, path) in rotated.into_iter().take(remove_count) {
        match std::fs::remove_file(path) {
            Ok(()) => stats.removed_files += 1,
            Err(_) => stats.removal_failures += 1,
        }
    }
    stats
}

fn merge_stats(target: &mut StructuredLogDiskStats, other: StructuredLogDiskStats) {
    target.directory_entries_scanned = target
        .directory_entries_scanned
        .saturating_add(other.directory_entries_scanned);
    target.directory_scan_truncated |= other.directory_scan_truncated;
    target.matching_rotated_files = target
        .matching_rotated_files
        .saturating_add(other.matching_rotated_files);
    target.removed_files = target.removed_files.saturating_add(other.removed_files);
    target.removal_failures = target
        .removal_failures
        .saturating_add(other.removal_failures);
    target.rotations = target.rotations.saturating_add(other.rotations);
    target.write_failures = target.write_failures.saturating_add(other.write_failures);
    target.unsafe_paths_refused = target
        .unsafe_paths_refused
        .saturating_add(other.unsafe_paths_refused);
}

#[cfg(test)]
mod tests {
    use std::fs::OpenOptions;
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::*;
    use crate::app::{LogSeverity, LogSource};

    static TEST_SEQUENCE: AtomicU64 = AtomicU64::new(0);

    fn isolated_dir(label: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "omen-structured-log-writer-{label}-{}-{}",
            std::process::id(),
            TEST_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("isolated log root");
        path
    }

    fn entry(epoch_ms: u64, message: impl Into<String>) -> LogEntry {
        LogEntry {
            epoch_ms,
            severity: LogSeverity::Info,
            source: LogSource::App,
            message: message.into(),
        }
    }

    #[test]
    fn policy_normalizes_legacy_extremes() {
        assert_eq!(
            StructuredLogDiskPolicy::normalized(0, 0),
            StructuredLogDiskPolicy {
                max_file_bytes: STRUCTURED_LOG_MIN_FILE_BYTES,
                retain_files: 1,
            }
        );
        assert_eq!(
            StructuredLogDiskPolicy::normalized(u64::MAX, usize::MAX),
            StructuredLogDiskPolicy {
                max_file_bytes: STRUCTURED_LOG_MAX_FILE_BYTES,
                retain_files: STRUCTURED_LOG_MAX_RETAIN_FILES,
            }
        );
    }

    #[test]
    fn rotation_retention_and_encoded_records_stay_bounded() {
        let root = isolated_dir("retention");
        let active = root.join("omenbrowser_rs.jsonl");
        let policy = StructuredLogDiskPolicy::normalized(STRUCTURED_LOG_MIN_FILE_BYTES, 3);
        let mut rotations = 0;
        for index in 0..20 {
            let stats = append_structured_log_entry(
                &active,
                &entry(index, format!("{index}:{}", "x".repeat(12 * 1024))),
                policy,
            );
            assert_eq!(stats.write_failures, 0);
            rotations += stats.rotations;
            assert!(active.metadata().expect("active metadata").len() <= policy.max_file_bytes);
        }
        let rotated = std::fs::read_dir(&root)
            .expect("log directory")
            .filter_map(Result::ok)
            .filter(|entry| {
                entry.file_type().is_ok_and(|kind| kind.is_file())
                    && entry.file_name().to_str().is_some_and(|name| {
                        name.starts_with("omenbrowser_rs-") && name.ends_with(".jsonl")
                    })
            })
            .collect::<Vec<_>>();
        assert!(rotations > 0);
        assert!(rotated.len() <= policy.retain_files);
        assert!(rotated.iter().all(|entry| entry
            .metadata()
            .is_ok_and(|metadata| metadata.len() <= policy.max_file_bytes)));
        let persisted = std::fs::read_to_string(&active).expect("active log");
        assert!(persisted.contains("...<truncated>"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&active)
                    .expect("active metadata")
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
            for entry in rotated {
                assert_eq!(
                    entry
                        .metadata()
                        .expect("rotated metadata")
                        .permissions()
                        .mode()
                        & 0o777,
                    0o600
                );
            }
        }
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn retention_scan_is_bounded() {
        let root = isolated_dir("scan-bound");
        let active = root.join("omenbrowser_rs.jsonl");
        for index in 0..STRUCTURED_LOG_PRUNE_SCAN_LIMIT + 20 {
            File::create(root.join(format!("unrelated-{index}"))).expect("unrelated file");
        }
        let stats =
            enforce_structured_log_retention(&active, StructuredLogDiskPolicy::normalized(4096, 4));
        assert_eq!(
            stats.directory_entries_scanned,
            STRUCTURED_LOG_PRUNE_SCAN_LIMIT
        );
        assert!(stats.directory_scan_truncated);
        let _ = std::fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn retention_repairs_existing_active_and_rotated_modes_without_changing_content() {
        use std::os::unix::fs::PermissionsExt;

        let root = isolated_dir("permission-repair");
        let active = root.join("omenbrowser_rs.jsonl");
        let rotated = root.join("omenbrowser_rs-1.jsonl");
        std::fs::write(&active, b"active-preserved\n").expect("active");
        std::fs::write(&rotated, b"rotated-preserved\n").expect("rotated");
        for path in [&active, &rotated] {
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o644))
                .expect("permissive mode");
        }

        let repair =
            enforce_structured_log_retention(&active, StructuredLogDiskPolicy::normalized(4096, 4));
        assert_eq!(repair.unsafe_paths_refused, 0);
        let stats = append_structured_log_entry(
            &active,
            &entry(2, "new record"),
            StructuredLogDiskPolicy::normalized(4096, 4),
        );

        assert_eq!(stats.write_failures, 0);
        assert!(std::fs::read_to_string(&active)
            .expect("active content")
            .starts_with("active-preserved\n"));
        assert_eq!(
            std::fs::read_to_string(&rotated).expect("rotated content"),
            "rotated-preserved\n"
        );
        for path in [&active, &rotated] {
            assert_eq!(
                std::fs::metadata(path)
                    .expect("metadata")
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
        std::fs::remove_dir_all(root).expect("cleanup");
    }

    #[cfg(unix)]
    #[test]
    fn active_file_and_rotated_symlinks_are_refused() {
        use std::os::unix::fs::symlink;

        let root = isolated_dir("symlink");
        let outside = root.with_extension("outside");
        std::fs::write(&outside, "outside").expect("outside file");
        let active = root.join("omenbrowser_rs.jsonl");
        symlink(&outside, &active).expect("active symlink");
        let stats = append_structured_log_entry(
            &active,
            &entry(1, "must not escape"),
            StructuredLogDiskPolicy::normalized(4096, 1),
        );
        assert_eq!(stats.unsafe_paths_refused, 1);
        assert_eq!(
            std::fs::read_to_string(&outside).expect("outside file"),
            "outside"
        );

        std::fs::remove_file(&active).expect("remove active link");
        let rotated_link = root.join("omenbrowser_rs-1.jsonl");
        symlink(&outside, &rotated_link).expect("rotated symlink");
        let prune =
            enforce_structured_log_retention(&active, StructuredLogDiskPolicy::normalized(4096, 1));
        assert_eq!(prune.matching_rotated_files, 0);
        assert!(rotated_link.is_symlink());
        let _ = std::fs::remove_dir_all(root);
        let _ = std::fs::remove_file(outside);
    }

    #[test]
    fn background_writer_is_item_byte_and_latency_bounded_under_slow_io() {
        let root = isolated_dir("worker-overload");
        let active = root.join("omenbrowser_rs.jsonl");
        let mut worker = StructuredLogWorker::start_with_delay(Duration::from_millis(2))
            .expect("background writer");
        let policy = StructuredLogDiskPolicy::normalized(STRUCTURED_LOG_MAX_FILE_BYTES, 4);
        let started = std::time::Instant::now();
        for index in 0..1000 {
            let _ = worker.enqueue(
                &active,
                entry(index, format!("{index}:{}", "x".repeat(12 * 1024))),
                policy,
            );
        }
        let admission_elapsed = started.elapsed();
        std::thread::sleep(Duration::from_millis(3));
        let saturated = worker.metrics();
        assert!(admission_elapsed < Duration::from_millis(250));
        assert!(saturated.queued_items <= STRUCTURED_LOG_QUEUE_ITEMS);
        assert!(saturated.queued_bytes <= STRUCTURED_LOG_QUEUE_BYTES);
        assert!(saturated.dropped_records > 0);
        assert!(saturated.oldest_age_ms > 0);

        assert!(worker.flush_handle().flush(Duration::from_secs(5)));
        let drained = worker.metrics();
        assert_eq!(drained.queued_items, 0);
        assert_eq!(drained.queued_bytes, 0);
        assert!(drained.completed_records > 0);
        assert_eq!(drained.write_failures, 0);
        println!(
            "STRUCTURED_LOG_WORKER_SUMMARY submitted=1000 admission_us={} service_delay_ms=2 peak_items={} peak_bytes={} dropped={} completed={} drained_items={} drained_bytes={}",
            admission_elapsed.as_micros(),
            saturated.queued_items,
            saturated.queued_bytes,
            saturated.dropped_records,
            drained.completed_records,
            drained.queued_items,
            drained.queued_bytes
        );
        assert!(worker.shutdown(Duration::from_secs(1)));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn background_flush_persists_every_admitted_record_in_order() {
        let root = isolated_dir("worker-flush");
        let active = root.join("omenbrowser_rs.jsonl");
        let mut worker = StructuredLogWorker::start().expect("background writer");
        let policy = StructuredLogDiskPolicy::normalized(64 * 1024, 2);
        for index in 0..20 {
            assert!(worker.enqueue(&active, entry(index, format!("entry-{index}")), policy));
        }
        assert!(worker.flush_handle().flush(Duration::from_secs(2)));
        let persisted = std::fs::read_to_string(&active).expect("flushed active log");
        let entries = persisted
            .lines()
            .map(|line| serde_json::from_str::<LogEntry>(line).expect("JSONL entry"))
            .collect::<Vec<_>>();
        assert_eq!(entries.len(), 20);
        assert!(entries
            .iter()
            .enumerate()
            .all(|(index, entry)| entry.message == format!("entry-{index}")));
        assert!(worker.shutdown(Duration::from_secs(1)));
        assert_eq!(worker.metrics().queued_items, 0);
        let _ = std::fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn background_writer_counts_refused_symlink_without_retaining_permit() {
        use std::os::unix::fs::symlink;

        let root = isolated_dir("worker-symlink");
        let outside = root.with_extension("worker-outside");
        std::fs::write(&outside, "outside").expect("outside file");
        let active = root.join("omenbrowser_rs.jsonl");
        symlink(&outside, &active).expect("active symlink");
        let mut worker = StructuredLogWorker::start().expect("background writer");
        assert!(worker.enqueue(
            &active,
            entry(1, "refused"),
            StructuredLogDiskPolicy::normalized(4096, 1),
        ));
        assert!(worker.flush_handle().flush(Duration::from_secs(2)));
        let metrics = worker.metrics();
        assert_eq!(metrics.queued_items, 0);
        assert_eq!(metrics.queued_bytes, 0);
        assert_eq!(metrics.unsafe_paths_refused, 1);
        assert_eq!(
            std::fs::read_to_string(&outside).expect("outside file"),
            "outside"
        );
        assert!(worker.shutdown(Duration::from_secs(1)));
        let _ = std::fs::remove_dir_all(root);
        let _ = std::fs::remove_file(outside);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn kernel_enospc_is_counted_and_releases_the_queue_permit() {
        let encoded = encode_entry(
            &entry(1, "isolated ENOSPC qualification"),
            STRUCTURED_LOG_MIN_FILE_BYTES as usize,
        )
        .expect("encoded log record");
        let budget = Arc::new(QueueBudget::new());
        let permit = budget.reserve(encoded.len()).expect("queue permit");
        let mut full = OpenOptions::new()
            .write(true)
            .open("/dev/full")
            .expect("Linux /dev/full");

        let error = write_encoded_record(&mut full, &encoded).expect_err("ENOSPC expected");
        assert_eq!(error.raw_os_error(), Some(28));
        budget.record_completion(StructuredLogDiskStats {
            write_failures: 1,
            ..StructuredLogDiskStats::default()
        });
        drop(permit);

        let metrics = budget.snapshot();
        assert_eq!(metrics.queued_items, 0);
        assert_eq!(metrics.queued_bytes, 0);
        assert_eq!(metrics.completed_records, 1);
        assert_eq!(metrics.write_failures, 1);
    }
}
