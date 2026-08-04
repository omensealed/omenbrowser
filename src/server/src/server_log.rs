use std::collections::BTreeMap;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const LOG_QUEUE_ITEMS: usize = 896;
const LOG_QUEUE_BYTES: usize = 768 * 1024;
const LOG_PRIORITY_ITEMS: usize = 128;
const LOG_PRIORITY_BYTES: usize = 256 * 1024;
const LOG_RECORD_BYTES: usize = 16 * 1024;
const LOG_CONTROL_ITEMS: usize = 8;
const LOG_FILE_BYTES: u64 = 8 * 1024 * 1024;
const LOG_BACKUP_FILES: usize = 3;

pub(crate) fn prepare_log_path(path: &Path) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        crate::private_fs::ensure_private_parent_dir(parent)?;
    }
    repair_rotated_logs(path, LOG_BACKUP_FILES)?;
    drop(crate::private_fs::open_private_append(path)?);
    Ok(())
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ServerLogSeverity {
    #[default]
    Info,
    Warning,
    Error,
}

impl ServerLogSeverity {
    fn uses_priority_lane(self) -> bool {
        matches!(self, Self::Warning | Self::Error)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ServerLogMetrics {
    pub queued_items: usize,
    pub queued_bytes: usize,
    pub dropped_records: u64,
    pub write_failures: u64,
    pub oldest_age_ms: u64,
    pub priority_queued_items: usize,
    pub priority_dropped_records: u64,
}

impl ServerLogMetrics {
    pub fn summary(self) -> String {
        format!(
            "logs=items:{} bytes:{} oldest_ms:{} dropped:{} priority_items:{} priority_dropped:{} write_failed:{}",
            self.queued_items,
            self.queued_bytes,
            self.oldest_age_ms,
            self.dropped_records,
            self.priority_queued_items,
            self.priority_dropped_records,
            self.write_failures
        )
    }
}

struct LogBudget {
    max_bytes: usize,
    queued_items: AtomicUsize,
    queued_bytes: AtomicUsize,
    dropped_records: AtomicU64,
    write_failures: AtomicU64,
    oldest_epoch_ms: AtomicU64,
}

impl LogBudget {
    fn new(max_bytes: usize) -> Self {
        Self {
            max_bytes,
            queued_items: AtomicUsize::new(0),
            queued_bytes: AtomicUsize::new(0),
            dropped_records: AtomicU64::new(0),
            write_failures: AtomicU64::new(0),
            oldest_epoch_ms: AtomicU64::new(0),
        }
    }

    fn reserve(self: &Arc<Self>, bytes: usize) -> Option<LogPermit> {
        if bytes > LOG_RECORD_BYTES {
            self.dropped_records.fetch_add(1, Ordering::Relaxed);
            return None;
        }
        let reserved = self
            .queued_bytes
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
                current
                    .checked_add(bytes)
                    .filter(|next| *next <= self.max_bytes)
            })
            .is_ok();
        if !reserved {
            self.dropped_records.fetch_add(1, Ordering::Relaxed);
            return None;
        }
        if self.queued_items.fetch_add(1, Ordering::AcqRel) == 0 {
            self.oldest_epoch_ms
                .store(current_epoch_ms(), Ordering::Release);
        }
        Some(LogPermit {
            budget: self.clone(),
            bytes,
        })
    }

    fn snapshot(&self) -> ServerLogMetrics {
        let queued_items = self.queued_items.load(Ordering::Acquire);
        let oldest = self.oldest_epoch_ms.load(Ordering::Acquire);
        ServerLogMetrics {
            queued_items,
            queued_bytes: self.queued_bytes.load(Ordering::Acquire),
            dropped_records: self.dropped_records.load(Ordering::Relaxed),
            write_failures: self.write_failures.load(Ordering::Relaxed),
            priority_queued_items: 0,
            priority_dropped_records: 0,
            oldest_age_ms: if queued_items == 0 || oldest == 0 {
                0
            } else {
                current_epoch_ms().saturating_sub(oldest)
            },
        }
    }
}

struct LogPermit {
    budget: Arc<LogBudget>,
    bytes: usize,
}

impl Drop for LogPermit {
    fn drop(&mut self) {
        self.budget
            .queued_bytes
            .fetch_sub(self.bytes, Ordering::AcqRel);
        if self.budget.queued_items.fetch_sub(1, Ordering::AcqRel) == 1 {
            self.budget.oldest_epoch_ms.store(0, Ordering::Release);
        }
    }
}

struct LogRecord {
    path: PathBuf,
    line: String,
    _permit: LogPermit,
}

enum LogControl {
    Flush(mpsc::SyncSender<()>),
}

struct ServerLogWriter {
    records: mpsc::SyncSender<LogRecord>,
    priority_records: mpsc::SyncSender<LogRecord>,
    control: mpsc::SyncSender<LogControl>,
    budget: Arc<LogBudget>,
    priority_budget: Arc<LogBudget>,
}

struct OpenLogFile {
    writer: BufWriter<File>,
    bytes: u64,
}

impl ServerLogWriter {
    fn start() -> Self {
        Self::start_with_write_delay(Duration::ZERO).0
    }

    fn start_with_write_delay(write_delay: Duration) -> (Self, std::thread::JoinHandle<()>) {
        let (records, record_rx) = mpsc::sync_channel(LOG_QUEUE_ITEMS);
        let (priority_records, priority_rx) = mpsc::sync_channel(LOG_PRIORITY_ITEMS);
        let (control, control_rx) = mpsc::sync_channel(LOG_CONTROL_ITEMS);
        let budget = Arc::new(LogBudget::new(LOG_QUEUE_BYTES));
        let priority_budget = Arc::new(LogBudget::new(LOG_PRIORITY_BYTES));
        let worker_budget = budget.clone();
        let worker_priority_budget = priority_budget.clone();
        let worker = std::thread::Builder::new()
            .name("omenchatd-log-writer".into())
            .spawn(move || {
                run_writer(
                    record_rx,
                    priority_rx,
                    control_rx,
                    worker_budget,
                    worker_priority_budget,
                    write_delay,
                )
            })
            .expect("spawn omenchatd log writer");
        (
            Self {
                records,
                priority_records,
                control,
                budget,
                priority_budget,
            },
            worker,
        )
    }

    fn enqueue(&self, path: &Path, severity: ServerLogSeverity, message: &str) {
        let line = bounded_line(message);
        let bytes = line.len().saturating_add(24);
        let priority = severity.uses_priority_lane();
        let budget = if priority {
            &self.priority_budget
        } else {
            &self.budget
        };
        let Some(permit) = budget.reserve(bytes) else {
            return;
        };
        let sender = if priority {
            &self.priority_records
        } else {
            &self.records
        };
        if sender
            .try_send(LogRecord {
                path: path.to_owned(),
                line,
                _permit: permit,
            })
            .is_err()
        {
            budget.dropped_records.fetch_add(1, Ordering::Relaxed);
        }
    }

    fn flush(&self, timeout: Duration) -> bool {
        let (ack_tx, ack_rx) = mpsc::sync_channel(1);
        if self.control.try_send(LogControl::Flush(ack_tx)).is_err() {
            return false;
        }
        ack_rx.recv_timeout(timeout).is_ok()
    }
}

fn run_writer(
    records: mpsc::Receiver<LogRecord>,
    priority_records: mpsc::Receiver<LogRecord>,
    control: mpsc::Receiver<LogControl>,
    budget: Arc<LogBudget>,
    priority_budget: Arc<LogBudget>,
    write_delay: Duration,
) {
    let mut files: BTreeMap<PathBuf, OpenLogFile> = BTreeMap::new();
    let mut last_flush = Instant::now();
    loop {
        while let Ok(command) = control.try_recv() {
            match command {
                LogControl::Flush(ack) => {
                    while let Ok(record) = priority_records.try_recv() {
                        if !write_record_with_delay(&mut files, record, write_delay) {
                            priority_budget
                                .write_failures
                                .fetch_add(1, Ordering::Relaxed);
                        }
                    }
                    while let Ok(record) = records.try_recv() {
                        if !write_record_with_delay(&mut files, record, write_delay) {
                            budget.write_failures.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                    flush_files(&mut files);
                    last_flush = Instant::now();
                    let _ = ack.send(());
                }
            }
        }
        while let Ok(record) = priority_records.try_recv() {
            if !write_record_with_delay(&mut files, record, write_delay) {
                priority_budget
                    .write_failures
                    .fetch_add(1, Ordering::Relaxed);
            }
        }
        match records.recv_timeout(Duration::from_millis(10)) {
            Ok(record) => {
                if !write_record_with_delay(&mut files, record, write_delay) {
                    budget.write_failures.fetch_add(1, Ordering::Relaxed);
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if last_flush.elapsed() >= Duration::from_millis(250) {
                    flush_files(&mut files);
                    last_flush = Instant::now();
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                flush_files(&mut files);
                break;
            }
        }
    }
}

fn write_record(files: &mut BTreeMap<PathBuf, OpenLogFile>, record: LogRecord) -> bool {
    write_log_line(
        files,
        &record.path,
        &record.line,
        LOG_FILE_BYTES,
        LOG_BACKUP_FILES,
    )
}

fn write_record_with_delay(
    files: &mut BTreeMap<PathBuf, OpenLogFile>,
    record: LogRecord,
    write_delay: Duration,
) -> bool {
    if !write_delay.is_zero() {
        std::thread::sleep(write_delay);
    }
    write_record(files, record)
}

fn write_log_line(
    files: &mut BTreeMap<PathBuf, OpenLogFile>,
    path: &Path,
    line: &str,
    max_file_bytes: u64,
    backup_files: usize,
) -> bool {
    let encoded = format!("{} {line}\n", current_unix_seconds());
    let encoded_bytes = encoded.len() as u64;
    let should_rotate = files
        .get(path)
        .map(|file| file.bytes > 0 && file.bytes.saturating_add(encoded_bytes) > max_file_bytes)
        .unwrap_or_else(|| {
            std::fs::metadata(path)
                .map(|metadata| {
                    metadata.len() > 0
                        && metadata.len().saturating_add(encoded_bytes) > max_file_bytes
                })
                .unwrap_or(false)
        });
    if should_rotate {
        if let Some(mut open) = files.remove(path) {
            let _ = open.writer.flush();
        }
        if rotate_logs(path, backup_files).is_err() {
            return false;
        }
    }
    if !files.contains_key(path) {
        if let Some(parent) = path.parent() {
            if crate::private_fs::ensure_private_dir(parent).is_err() {
                return false;
            }
        }
        if repair_rotated_logs(path, backup_files).is_err() {
            return false;
        }
        let Ok(file) = crate::private_fs::open_private_append(path) else {
            return false;
        };
        let bytes = file.metadata().map(|metadata| metadata.len()).unwrap_or(0);
        files.insert(
            path.to_owned(),
            OpenLogFile {
                writer: BufWriter::new(file),
                bytes,
            },
        );
    }
    let Some(file) = files.get_mut(path) else {
        return false;
    };
    if file.writer.write_all(encoded.as_bytes()).is_err() {
        return false;
    }
    file.bytes = file.bytes.saturating_add(encoded_bytes);
    true
}

fn rotate_logs(path: &Path, backup_files: usize) -> std::io::Result<()> {
    if backup_files == 0 {
        match std::fs::remove_file(path) {
            Ok(()) => return Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error),
        }
    }
    let oldest = rotated_log_path(path, backup_files);
    match std::fs::remove_file(oldest) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    for index in (1..backup_files).rev() {
        let source = rotated_log_path(path, index);
        let destination = rotated_log_path(path, index + 1);
        match std::fs::rename(source, destination) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
    }
    let rotated = rotated_log_path(path, 1);
    match std::fs::rename(path, &rotated) {
        Ok(()) => crate::private_fs::repair_private_file(&rotated),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

fn repair_rotated_logs(path: &Path, backup_files: usize) -> std::io::Result<()> {
    for index in 1..=backup_files {
        crate::private_fs::repair_private_file_if_exists(&rotated_log_path(path, index))?;
    }
    Ok(())
}

fn rotated_log_path(path: &Path, index: usize) -> PathBuf {
    let filename = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("omenchatd.log");
    path.with_file_name(format!("{filename}.{index}"))
}

fn flush_files(files: &mut BTreeMap<PathBuf, OpenLogFile>) {
    for file in files.values_mut() {
        let _ = file.writer.flush();
    }
}

fn bounded_line(message: &str) -> String {
    if message.len() <= LOG_RECORD_BYTES {
        return message.to_owned();
    }
    let mut end = LOG_RECORD_BYTES.saturating_sub(15);
    while !message.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}...[truncated]", &message[..end])
}

fn current_epoch_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

fn current_unix_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or_default()
}

fn writer() -> &'static ServerLogWriter {
    static WRITER: OnceLock<ServerLogWriter> = OnceLock::new();
    WRITER.get_or_init(ServerLogWriter::start)
}

pub fn append(path: &Path, message: &str) {
    append_with_severity(path, ServerLogSeverity::Info, message);
}

pub fn append_with_severity(path: &Path, severity: ServerLogSeverity, message: &str) {
    writer().enqueue(path, severity, message);
}

pub fn metrics() -> ServerLogMetrics {
    let writer = writer();
    let normal = writer.budget.snapshot();
    let priority = writer.priority_budget.snapshot();
    ServerLogMetrics {
        queued_items: normal.queued_items.saturating_add(priority.queued_items),
        queued_bytes: normal.queued_bytes.saturating_add(priority.queued_bytes),
        dropped_records: normal
            .dropped_records
            .saturating_add(priority.dropped_records),
        write_failures: normal
            .write_failures
            .saturating_add(priority.write_failures),
        oldest_age_ms: normal.oldest_age_ms.max(priority.oldest_age_ms),
        priority_queued_items: priority.queued_items,
        priority_dropped_records: priority.dropped_records,
    }
}

pub fn flush(timeout: Duration) -> bool {
    writer().flush(timeout)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn byte_budget_rejects_without_exceeding_limit() {
        let budget = Arc::new(LogBudget::new(LOG_QUEUE_BYTES));
        let permits = (0..LOG_QUEUE_BYTES / LOG_RECORD_BYTES)
            .map(|_| budget.reserve(LOG_RECORD_BYTES).expect("budget record"))
            .collect::<Vec<_>>();
        assert!(budget.reserve(1).is_none());
        assert_eq!(budget.snapshot().queued_bytes, LOG_QUEUE_BYTES);
        assert_eq!(budget.snapshot().dropped_records, 1);
        drop(permits);
        assert_eq!(budget.snapshot().queued_bytes, 0);
    }

    #[test]
    fn bounded_line_preserves_utf8_and_caps_records() {
        let message = "☃".repeat(LOG_RECORD_BYTES);
        let bounded = bounded_line(&message);
        assert!(bounded.len() <= LOG_RECORD_BYTES);
        assert!(bounded.ends_with("...[truncated]"));
    }

    #[test]
    fn priority_warning_survives_saturated_normal_lane() {
        let (records, _record_rx) = mpsc::sync_channel(LOG_QUEUE_ITEMS);
        let (priority_records, priority_rx) = mpsc::sync_channel(LOG_PRIORITY_ITEMS);
        let (control, _control_rx) = mpsc::sync_channel(LOG_CONTROL_ITEMS);
        let writer = ServerLogWriter {
            records,
            priority_records,
            control,
            budget: Arc::new(LogBudget::new(LOG_QUEUE_BYTES)),
            priority_budget: Arc::new(LogBudget::new(LOG_PRIORITY_BYTES)),
        };
        let path = Path::new("/isolated/omenchatd.log");
        for _ in 0..LOG_QUEUE_ITEMS {
            writer.enqueue(path, ServerLogSeverity::Info, "routine frame sent");
        }
        writer.enqueue(path, ServerLogSeverity::Info, "routine frame sent");
        assert_eq!(writer.budget.snapshot().dropped_records, 1);

        writer.enqueue(
            path,
            ServerLogSeverity::Warning,
            "resource send needs operator attention",
        );
        let priority = priority_rx.try_recv().expect("priority record");
        assert!(priority.line.contains("operator attention"));
        assert_eq!(writer.priority_budget.snapshot().dropped_records, 0);
    }

    #[test]
    fn typed_severity_alone_selects_the_priority_lane() {
        let (records, record_rx) = mpsc::sync_channel(4);
        let (priority_records, priority_rx) = mpsc::sync_channel(4);
        let (control, _control_rx) = mpsc::sync_channel(1);
        let writer = ServerLogWriter {
            records,
            priority_records,
            control,
            budget: Arc::new(LogBudget::new(4 * LOG_RECORD_BYTES)),
            priority_budget: Arc::new(LogBudget::new(4 * LOG_RECORD_BYTES)),
        };
        let path = Path::new("/isolated/typed-severity.log");

        writer.enqueue(
            path,
            ServerLogSeverity::Info,
            "text says failed error overloaded timed out lagged queue stopped missing request_id",
        );
        writer.enqueue(path, ServerLogSeverity::Error, "neutral typed error record");

        assert!(record_rx
            .try_recv()
            .expect("info record")
            .line
            .contains("failed"));
        assert!(priority_rx
            .try_recv()
            .expect("typed error record")
            .line
            .contains("neutral"));
    }

    #[test]
    fn typed_severity_preserves_the_timestamp_text_file_format() {
        let root = std::env::temp_dir().join(format!(
            "omenchatd-log-severity-format-{}-{}",
            std::process::id(),
            current_epoch_ms()
        ));
        let path = root.join("omenchatd.log");
        let (writer, worker) = ServerLogWriter::start_with_write_delay(Duration::ZERO);
        writer.enqueue(
            &path,
            ServerLogSeverity::Warning,
            "typed severity format marker",
        );
        assert!(writer.flush(Duration::from_secs(1)));
        drop(writer);
        worker.join().expect("join format writer");

        let line = std::fs::read_to_string(&path).expect("typed severity log");
        let (timestamp, message) = line.trim_end().split_once(' ').expect("timestamp and text");
        assert!(timestamp.parse::<i64>().is_ok());
        assert_eq!(message, "typed severity format marker");
        std::fs::remove_dir_all(root).expect("remove severity format root");
    }

    #[test]
    fn slow_consumer_keeps_callback_admission_bounded_and_nonblocking() {
        const SAMPLES: usize = 5_000;
        let (records, record_rx) = mpsc::sync_channel(16);
        let (priority_records, _priority_rx) = mpsc::sync_channel(4);
        let (control, _control_rx) = mpsc::sync_channel(1);
        let budget = Arc::new(LogBudget::new(64 * 1024));
        let writer = ServerLogWriter {
            records,
            priority_records,
            control,
            budget: budget.clone(),
            priority_budget: Arc::new(LogBudget::new(16 * 1024)),
        };
        let consumer = std::thread::spawn(move || {
            let mut consumed = 0usize;
            while let Ok(_record) = record_rx.recv() {
                std::thread::sleep(Duration::from_millis(2));
                consumed += 1;
            }
            consumed
        });

        let path = Path::new("/isolated/slow-writer.log");
        let mut samples = Vec::with_capacity(SAMPLES);
        let mut peak_queued = 0usize;
        for index in 0..SAMPLES {
            let started = Instant::now();
            writer.enqueue(
                path,
                ServerLogSeverity::Info,
                &format!("routine frame sent sample={index}"),
            );
            samples.push(started.elapsed().as_nanos() as u64);
            peak_queued = peak_queued.max(budget.snapshot().queued_items);
        }
        drop(writer);
        let consumed = consumer.join().expect("slow consumer");
        samples.sort_unstable();
        let median = samples[SAMPLES / 2];
        let p95 = samples[SAMPLES * 95 / 100];
        let max = samples[SAMPLES - 1];
        let metrics = budget.snapshot();
        eprintln!(
            "server_log_slow_consumer samples={SAMPLES} consumed={consumed} dropped={} peak_queued={peak_queued} median_ns={median} p95_ns={p95} max_ns={max}",
            metrics.dropped_records
        );

        assert!(metrics.dropped_records > 0);
        assert!(
            peak_queued <= 17,
            "bounded queue plus one in-flight record retained too many records"
        );
        assert!(
            p95 < 5_000_000,
            "p95 admission must stay below one slow write"
        );
        assert_eq!(metrics.queued_items, 0);
        assert_eq!(metrics.queued_bytes, 0);
    }

    #[test]
    fn writer_flushes_isolated_log() {
        let root = std::env::temp_dir().join(format!(
            "omenchatd-log-test-{}-{}",
            std::process::id(),
            current_epoch_ms()
        ));
        let path = root.join("server.log");
        append(&path, "isolated flush marker");
        assert!(flush(Duration::from_secs(1)));
        let contents = std::fs::read_to_string(&path).expect("flushed log");
        assert!(contents.contains("isolated flush marker"));
        std::fs::remove_dir_all(root).expect("remove isolated log root");
    }

    #[test]
    fn writer_reports_file_open_failures() {
        let root = std::env::temp_dir().join(format!(
            "omenchatd-log-failure-{}-{}",
            std::process::id(),
            current_epoch_ms()
        ));
        std::fs::create_dir_all(&root).expect("failure directory");
        let before = metrics().write_failures;
        append(&root, "cannot append to a directory");
        assert!(flush(Duration::from_secs(1)));
        assert!(metrics().write_failures > before);
        std::fs::remove_dir_all(root).expect("remove failure directory");
    }

    #[cfg(unix)]
    #[test]
    fn writer_repairs_existing_active_and_rotated_modes_without_changing_content() {
        use std::os::unix::fs::PermissionsExt;

        let root = std::env::temp_dir().join(format!(
            "omenchatd-log-permission-repair-{}-{}",
            std::process::id(),
            current_epoch_ms()
        ));
        std::fs::create_dir_all(&root).expect("root");
        let path = root.join("omenchatd.log");
        let rotated = rotated_log_path(&path, 1);
        std::fs::write(&path, b"active-preserved\n").expect("active");
        std::fs::write(&rotated, b"rotated-preserved\n").expect("rotated");
        for retained in [&path, &rotated] {
            std::fs::set_permissions(retained, std::fs::Permissions::from_mode(0o644))
                .expect("permissive mode");
        }
        let mut files = BTreeMap::new();
        assert!(write_log_line(&mut files, &path, "new record", 4096, 4));
        flush_files(&mut files);
        drop(files);

        assert!(std::fs::read_to_string(&path)
            .expect("active content")
            .starts_with("active-preserved\n"));
        assert_eq!(
            std::fs::read_to_string(&rotated).expect("rotated content"),
            "rotated-preserved\n"
        );
        for retained in [&path, &rotated] {
            assert_eq!(
                std::fs::metadata(retained)
                    .expect("metadata")
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
        std::fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn rotation_caps_backup_count_and_preserves_newest_log() {
        let root = std::env::temp_dir().join(format!(
            "omenchatd-log-rotation-{}-{}",
            std::process::id(),
            current_epoch_ms()
        ));
        let path = root.join("omenchatd.log");
        let mut files = BTreeMap::new();
        for index in 0..12 {
            assert!(write_log_line(
                &mut files,
                &path,
                &format!("rotation-marker-{index}-{}", "x".repeat(32)),
                128,
                2,
            ));
        }
        flush_files(&mut files);
        drop(files);

        assert!(path.exists());
        assert!(rotated_log_path(&path, 1).exists());
        assert!(rotated_log_path(&path, 2).exists());
        assert!(!rotated_log_path(&path, 3).exists());
        assert!(std::fs::read_to_string(&path)
            .expect("current log")
            .contains("rotation-marker-11"));
        for retained in [
            path.clone(),
            rotated_log_path(&path, 1),
            rotated_log_path(&path, 2),
        ] {
            assert!(std::fs::metadata(retained).expect("retained log").len() <= 128);
        }

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            for retained in [
                path.clone(),
                rotated_log_path(&path, 1),
                rotated_log_path(&path, 2),
            ] {
                assert_eq!(
                    std::fs::metadata(retained)
                        .expect("retained log metadata")
                        .permissions()
                        .mode()
                        & 0o777,
                    0o600
                );
            }
        }

        std::fs::remove_dir_all(root).expect("remove rotation root");
    }

    #[cfg(target_os = "linux")]
    #[test]
    #[ignore = "explicit 60-second native-filesystem/slow-writer soak; run through scripts/measure-omenchatd-logging.sh"]
    fn bounded_logger_stays_nonblocking_and_retained_under_slow_filesystem_soak() {
        const CYCLES: usize = 3;
        const BURST: usize = 64;
        const WRITE_DELAY: Duration = Duration::from_millis(2);

        let duration_secs = std::env::var("OMENCHATD_LOG_SOAK_SECONDS")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(60);
        assert!((3..=600).contains(&duration_secs));
        let root = std::env::temp_dir().join(format!(
            "omenchatd-log-soak-{}-{}",
            std::process::id(),
            current_epoch_ms()
        ));
        std::fs::create_dir_all(&root).expect("isolated log soak root");
        let rss_before = linux_rss_bytes().expect("Linux VmRSS");
        let fd_before = linux_fd_count().expect("Linux fd count");
        let routine = format!(
            "routine frame sent request_id=soak {}",
            "x".repeat(12 * 1024)
        );
        let started = Instant::now();
        let mut admission_ns = Vec::new();
        let mut submitted = 0_u64;
        let mut dropped = 0_u64;
        let mut priority_dropped = 0_u64;
        let mut write_failures = 0_u64;
        let mut peak_items = 0_usize;
        let mut peak_bytes = 0_usize;
        let mut peak_oldest_ms = 0_u64;
        let mut retained_files = 0_usize;
        let mut retained_bytes = 0_u64;
        let mut rotated_cycles = 0_usize;

        for cycle in 0..CYCLES {
            let cycle_seconds =
                duration_secs / CYCLES as u64 + u64::from(cycle < duration_secs as usize % CYCLES);
            let path = root.join(format!("omenchatd-{cycle}.log"));
            let (writer, worker) = ServerLogWriter::start_with_write_delay(WRITE_DELAY);
            let deadline = Instant::now() + Duration::from_secs(cycle_seconds);
            let mut burst_index = 0_u64;
            while Instant::now() < deadline {
                let burst_started = Instant::now();
                for _ in 0..BURST {
                    let admission_started = Instant::now();
                    writer.enqueue(&path, ServerLogSeverity::Info, &routine);
                    admission_ns.push(admission_started.elapsed().as_nanos() as u64);
                    submitted = submitted.saturating_add(1);
                }
                if burst_index % 10 == 0 {
                    let admission_started = Instant::now();
                    writer.enqueue(
                        &path,
                        ServerLogSeverity::Warning,
                        &format!("resource send failed request_id=priority-{cycle}-{burst_index}"),
                    );
                    admission_ns.push(admission_started.elapsed().as_nanos() as u64);
                    submitted = submitted.saturating_add(1);
                }
                burst_index = burst_index.saturating_add(1);
                let metrics = writer_snapshot(&writer);
                peak_items = peak_items.max(metrics.queued_items);
                peak_bytes = peak_bytes.max(metrics.queued_bytes);
                peak_oldest_ms = peak_oldest_ms.max(metrics.oldest_age_ms);
                let remaining = Duration::from_millis(10).saturating_sub(burst_started.elapsed());
                if !remaining.is_zero() {
                    std::thread::sleep(remaining);
                }
            }
            assert!(
                writer.flush(Duration::from_secs(15)),
                "slow logger must drain on explicit flush"
            );
            let metrics = writer_snapshot(&writer);
            dropped = dropped.saturating_add(metrics.dropped_records);
            priority_dropped = priority_dropped.saturating_add(metrics.priority_dropped_records);
            write_failures = write_failures.saturating_add(metrics.write_failures);
            assert_eq!(metrics.queued_items, 0);
            assert_eq!(metrics.queued_bytes, 0);
            drop(writer);
            worker.join().expect("join slow log writer");

            let mut cycle_bytes = 0_u64;
            let mut cycle_files = 0_usize;
            for index in 0..=LOG_BACKUP_FILES {
                let retained = if index == 0 {
                    path.clone()
                } else {
                    rotated_log_path(&path, index)
                };
                if let Ok(metadata) = std::fs::metadata(&retained) {
                    assert!(metadata.len() <= LOG_FILE_BYTES);
                    cycle_bytes = cycle_bytes.saturating_add(metadata.len());
                    cycle_files = cycle_files.saturating_add(1);
                }
            }
            assert!(!rotated_log_path(&path, LOG_BACKUP_FILES + 1).exists());
            assert!(cycle_files <= LOG_BACKUP_FILES + 1);
            assert!(
                cycle_bytes <= LOG_FILE_BYTES * (LOG_BACKUP_FILES as u64 + 1),
                "log retention exceeded the production cap"
            );
            rotated_cycles += usize::from(rotated_log_path(&path, 1).exists());
            retained_files = retained_files.saturating_add(cycle_files);
            retained_bytes = retained_bytes.saturating_add(cycle_bytes);
        }

        admission_ns.sort_unstable();
        let median_ns = admission_ns[admission_ns.len() / 2];
        let p95_ns = admission_ns[admission_ns.len() * 95 / 100];
        let max_ns = admission_ns[admission_ns.len() - 1];
        let rss_after = linux_rss_bytes().expect("Linux VmRSS after soak");
        let fd_after = linux_fd_count().expect("Linux fd count after soak");
        let rss_delta = rss_after.saturating_sub(rss_before);
        println!(
            "LOG_SOAK_SUMMARY duration_s={} cycles={CYCLES} submitted={submitted} dropped={dropped} priority_dropped={priority_dropped} write_failures={write_failures} peak_items={peak_items} peak_bytes={peak_bytes} peak_oldest_ms={peak_oldest_ms} admission_median_ns={median_ns} admission_p95_ns={p95_ns} admission_max_ns={max_ns} rss_before={rss_before} rss_after={rss_after} rss_delta={rss_delta} fd_before={fd_before} fd_after={fd_after} retained_files={retained_files} retained_bytes={retained_bytes} rotated_cycles={rotated_cycles} elapsed_ms={}",
            duration_secs,
            started.elapsed().as_millis()
        );

        assert!(
            dropped > submitted / 2,
            "soak must sustain overload pressure"
        );
        assert_eq!(priority_dropped, 0);
        assert_eq!(write_failures, 0);
        assert!(peak_items <= LOG_QUEUE_ITEMS + LOG_PRIORITY_ITEMS);
        assert!(peak_bytes <= LOG_QUEUE_BYTES + LOG_PRIORITY_BYTES);
        assert!(p95_ns < 2_000_000, "p95 admission stalled on slow writes");
        assert!(
            rss_delta <= 64 * 1024 * 1024,
            "logger RSS grew unexpectedly"
        );
        assert!(fd_after <= fd_before.saturating_add(2));
        if duration_secs >= 12 {
            assert!(rotated_cycles > 0, "release soak must exercise rotation");
        }

        std::fs::remove_dir_all(root).expect("remove isolated log soak root");
    }

    #[cfg(target_os = "linux")]
    fn writer_snapshot(writer: &ServerLogWriter) -> ServerLogMetrics {
        let normal = writer.budget.snapshot();
        let priority = writer.priority_budget.snapshot();
        ServerLogMetrics {
            queued_items: normal.queued_items.saturating_add(priority.queued_items),
            queued_bytes: normal.queued_bytes.saturating_add(priority.queued_bytes),
            dropped_records: normal
                .dropped_records
                .saturating_add(priority.dropped_records),
            write_failures: normal
                .write_failures
                .saturating_add(priority.write_failures),
            oldest_age_ms: normal.oldest_age_ms.max(priority.oldest_age_ms),
            priority_queued_items: priority.queued_items,
            priority_dropped_records: priority.dropped_records,
        }
    }

    #[cfg(target_os = "linux")]
    fn linux_rss_bytes() -> Option<u64> {
        std::fs::read_to_string("/proc/self/status")
            .ok()?
            .lines()
            .find_map(|line| {
                line.strip_prefix("VmRSS:")?
                    .split_whitespace()
                    .next()?
                    .parse::<u64>()
                    .ok()
            })
            .map(|kib| kib.saturating_mul(1024))
    }

    #[cfg(target_os = "linux")]
    fn linux_fd_count() -> Option<usize> {
        std::fs::read_dir("/proc/self/fd").ok()?.count().into()
    }
}
