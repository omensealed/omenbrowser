//! Bounded persisted-log collection for diagnostic report bundles.

use std::path::Path;

use crate::{
    cli_overrides::SmokeOverrides,
    cli_redaction::redact_bundle_log_message,
    structured_log_reader::{load_recent_log_entries, PersistedLogLimits},
};

pub const REPORT_LOG_ENTRY_LIMIT: usize = 50;
pub const REPORT_LOG_FILE_LIMIT: usize = 8;
pub const REPORT_LOG_FILE_BYTES: usize = 512 * 1024;
pub const REPORT_LOG_TOTAL_BYTES: usize = 2 * 1024 * 1024;
pub const REPORT_LOG_DIRECTORY_ENTRY_LIMIT: usize = 4096;

#[derive(Debug)]
pub struct RecentPersistedLogs {
    pub entries: Vec<serde_json::Value>,
    pub directory_entries_scanned: usize,
    pub directory_scan_truncated: bool,
    pub matching_files: usize,
    pub selected_files: usize,
    pub files_read: usize,
    pub bytes_read: usize,
    pub truncated_files: usize,
    pub read_failures: usize,
}

type CollectionLimits = PersistedLogLimits;

const REPORT_LIMITS: CollectionLimits = CollectionLimits {
    entry_limit: REPORT_LOG_ENTRY_LIMIT,
    directory_entry_limit: REPORT_LOG_DIRECTORY_ENTRY_LIMIT,
    file_limit: REPORT_LOG_FILE_LIMIT,
    file_bytes: REPORT_LOG_FILE_BYTES,
    total_bytes: REPORT_LOG_TOTAL_BYTES,
};

pub fn redacted_recent_persisted_logs(
    logs_dir: &Path,
    overrides: &SmokeOverrides,
    identity_path: Option<&Path>,
) -> RecentPersistedLogs {
    collect_with_limits(logs_dir, overrides, identity_path, REPORT_LIMITS)
}

fn collect_with_limits(
    logs_dir: &Path,
    overrides: &SmokeOverrides,
    identity_path: Option<&Path>,
    limits: CollectionLimits,
) -> RecentPersistedLogs {
    let loaded = load_recent_log_entries(logs_dir, limits);
    let stats = loaded.stats;
    let entries = loaded
        .entries
        .into_iter()
        .map(|entry| {
            serde_json::json!({
                "epoch_ms": entry.epoch_ms,
                "severity": format!("{:?}", entry.severity),
                "source": format!("{:?}", entry.source),
                "message": redact_bundle_log_message(&entry.message, overrides, identity_path),
            })
        })
        .collect();
    RecentPersistedLogs {
        entries,
        directory_entries_scanned: stats.directory_entries_scanned,
        directory_scan_truncated: stats.directory_scan_truncated,
        matching_files: stats.matching_files,
        selected_files: stats.selected_files,
        files_read: stats.files_read,
        bytes_read: stats.bytes_read,
        truncated_files: stats.truncated_files,
        read_failures: stats.read_failures,
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs::File,
        io::{Seek, SeekFrom, Write},
        path::PathBuf,
        sync::atomic::{AtomicU64, Ordering},
    };

    use super::*;
    use crate::app::{LogEntry, LogSeverity, LogSource};

    static NEXT_ROOT: AtomicU64 = AtomicU64::new(0);

    fn isolated_root(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "omen-cli-report-logs-{label}-{}-{}",
            std::process::id(),
            NEXT_ROOT.fetch_add(1, Ordering::Relaxed)
        ))
    }

    fn encoded_entry(epoch_ms: u64, message: &str) -> String {
        serde_json::to_string(&LogEntry {
            epoch_ms,
            severity: LogSeverity::Warn,
            source: LogSource::Runtime,
            message: message.into(),
        })
        .expect("log entry")
    }

    #[test]
    fn collector_keeps_only_the_newest_entry_budget() {
        let root = isolated_root("recent");
        std::fs::create_dir_all(&root).expect("logs root");
        let raw = (0..12)
            .map(|epoch| encoded_entry(epoch, &format!("entry-{epoch}")))
            .collect::<Vec<_>>()
            .join("\n");
        std::fs::write(root.join("omenbrowser_rs.jsonl"), raw).expect("seed active log");
        let limits = CollectionLimits {
            entry_limit: 4,
            directory_entry_limit: 8,
            file_limit: 2,
            file_bytes: 4096,
            total_bytes: 4096,
        };

        let report = collect_with_limits(&root, &SmokeOverrides::default(), None, limits);
        let epochs = report
            .entries
            .iter()
            .filter_map(|entry| entry.get("epoch_ms")?.as_u64())
            .collect::<Vec<_>>();
        assert_eq!(epochs, vec![8, 9, 10, 11]);
        assert_eq!(report.files_read, 1);
        assert!(report.bytes_read <= limits.total_bytes);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn collector_bounds_scan_files_and_bytes_while_retaining_tail() {
        let root = isolated_root("bounds");
        std::fs::create_dir_all(&root).expect("logs root");
        for index in 0..6 {
            let name = if index == 0 {
                "omenbrowser_rs.jsonl".to_string()
            } else {
                format!("omenbrowser_rs-{index}.jsonl")
            };
            let raw = format!(
                "{}\n{}\n",
                "x".repeat(180),
                encoded_entry(100 + index, &format!("tail-{index}"))
            );
            std::fs::write(root.join(name), raw).expect("seed oversized log");
        }
        std::fs::write(root.join("not-a-log"), b"ignored").expect("seed unrelated file");
        let limits = CollectionLimits {
            entry_limit: 8,
            directory_entry_limit: 4,
            file_limit: 3,
            file_bytes: 128,
            total_bytes: 256,
        };

        let report = collect_with_limits(&root, &SmokeOverrides::default(), None, limits);
        assert!(report.directory_entries_scanned <= limits.directory_entry_limit);
        assert!(report.directory_scan_truncated);
        assert!(report.selected_files <= limits.file_limit);
        assert!(report.files_read <= 2);
        assert!(report.bytes_read <= limits.total_bytes);
        assert!(report.truncated_files <= report.files_read);
        assert!(report
            .entries
            .iter()
            .any(
                |entry| entry.get("message").and_then(serde_json::Value::as_str) == Some("tail-0")
            ));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn production_collector_enforces_declared_file_and_total_byte_caps() {
        let root = isolated_root("production-bounds");
        std::fs::create_dir_all(&root).expect("logs root");
        for index in 0..10 {
            let name = if index == 0 {
                "omenbrowser_rs.jsonl".to_string()
            } else {
                format!("omenbrowser_rs-{index}.jsonl")
            };
            let mut file = File::create(root.join(name)).expect("create sparse log");
            file.set_len((REPORT_LOG_FILE_BYTES + 128) as u64)
                .expect("size sparse log");
            file.seek(SeekFrom::End(-256)).expect("seek log tail");
            writeln!(file, "discarded-partial").expect("partial line");
            writeln!(
                file,
                "{}",
                encoded_entry(100 + index, &format!("tail-{index}"))
            )
            .expect("tail entry");
        }

        let report = redacted_recent_persisted_logs(&root, &SmokeOverrides::default(), None);
        assert_eq!(report.selected_files, REPORT_LOG_FILE_LIMIT);
        assert_eq!(report.bytes_read, REPORT_LOG_TOTAL_BYTES);
        assert_eq!(
            report.files_read,
            REPORT_LOG_TOTAL_BYTES / REPORT_LOG_FILE_BYTES
        );
        assert_eq!(report.truncated_files, report.files_read);
        assert!(report.entries.len() <= REPORT_LOG_ENTRY_LIMIT);
        assert!(report
            .entries
            .iter()
            .any(
                |entry| entry.get("message").and_then(serde_json::Value::as_str) == Some("tail-0")
            ));
        let _ = std::fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn collector_does_not_follow_matching_symlinks() {
        use std::os::unix::fs::symlink;

        let root = isolated_root("symlink");
        std::fs::create_dir_all(&root).expect("logs root");
        let outside = root.with_extension("outside");
        std::fs::write(&outside, encoded_entry(9, "must-not-load")).expect("outside log");
        symlink(&outside, root.join("omenbrowser_rs-9.jsonl")).expect("matching symlink");

        let report = redacted_recent_persisted_logs(&root, &SmokeOverrides::default(), None);
        assert!(report.entries.is_empty());
        assert_eq!(report.matching_files, 0);
        let _ = std::fs::remove_dir_all(root);
        let _ = std::fs::remove_file(outside);
    }
}
