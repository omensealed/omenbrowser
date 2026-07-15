//! Shared bounded reader for OMENbrowser structured log files.

use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use crate::app::LogEntry;

#[derive(Clone, Copy, Debug)]
pub struct PersistedLogLimits {
    pub entry_limit: usize,
    pub directory_entry_limit: usize,
    pub file_limit: usize,
    pub file_bytes: usize,
    pub total_bytes: usize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PersistedLogStats {
    pub directory_entries_scanned: usize,
    pub directory_scan_truncated: bool,
    pub matching_files: usize,
    pub selected_files: usize,
    pub files_read: usize,
    pub bytes_read: usize,
    pub truncated_files: usize,
    pub read_failures: usize,
}

#[derive(Debug)]
pub struct RecentLogEntries {
    pub entries: Vec<LogEntry>,
    pub stats: PersistedLogStats,
}

struct Candidate {
    path: PathBuf,
    modified: SystemTime,
    active: bool,
}

pub fn load_recent_log_entries(logs_dir: &Path, limits: PersistedLogLimits) -> RecentLogEntries {
    let mut stats = PersistedLogStats::default();
    if limits.entry_limit == 0 || limits.file_limit == 0 || limits.total_bytes == 0 {
        return RecentLogEntries {
            entries: Vec::new(),
            stats,
        };
    }

    let active_path = logs_dir.join("omenbrowser_rs.jsonl");
    let mut candidates = Vec::with_capacity(limits.file_limit);
    if let Some(candidate) = regular_candidate(active_path, true) {
        stats.matching_files += 1;
        candidates.push(candidate);
    }

    if let Ok(read_dir) = std::fs::read_dir(logs_dir) {
        for entry in read_dir {
            if stats.directory_entries_scanned == limits.directory_entry_limit {
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
            let Some(candidate) = regular_candidate(entry.path(), false) else {
                continue;
            };
            stats.matching_files += 1;
            retain_candidate(&mut candidates, candidate, limits.file_limit);
        }
    }

    candidates.sort_by(|left, right| {
        right
            .active
            .cmp(&left.active)
            .then_with(|| right.modified.cmp(&left.modified))
            .then_with(|| right.path.cmp(&left.path))
    });
    stats.selected_files = candidates.len();

    let mut recent_entries = Vec::with_capacity(limits.entry_limit);
    for candidate in candidates {
        let remaining = limits.total_bytes.saturating_sub(stats.bytes_read);
        if remaining == 0 {
            break;
        }
        let read_limit = limits.file_bytes.min(remaining);
        match read_log_tail(&candidate.path, read_limit) {
            Ok(read) => {
                stats.files_read += 1;
                stats.bytes_read += read.bytes.len();
                stats.truncated_files += usize::from(read.truncated);
                let lines = if read.truncated {
                    read.bytes
                        .iter()
                        .position(|byte| *byte == b'\n')
                        .map_or(&[][..], |index| &read.bytes[index + 1..])
                } else {
                    read.bytes.as_slice()
                };
                for line in lines.split(|byte| *byte == b'\n') {
                    if let Ok(entry) = serde_json::from_slice::<LogEntry>(line) {
                        retain_recent_entry(&mut recent_entries, entry, limits.entry_limit);
                    }
                }
            }
            Err(()) => stats.read_failures += 1,
        }
    }

    recent_entries.sort_by_key(|entry| entry.epoch_ms);
    RecentLogEntries {
        entries: recent_entries,
        stats,
    }
}

fn regular_candidate(path: PathBuf, active: bool) -> Option<Candidate> {
    let metadata = path.symlink_metadata().ok()?;
    if !metadata.file_type().is_file() {
        return None;
    }
    Some(Candidate {
        path,
        modified: metadata.modified().unwrap_or(UNIX_EPOCH),
        active,
    })
}

fn retain_candidate(candidates: &mut Vec<Candidate>, candidate: Candidate, limit: usize) {
    if candidates.len() < limit {
        candidates.push(candidate);
        return;
    }
    let Some((oldest_index, oldest)) = candidates
        .iter()
        .enumerate()
        .filter(|(_, candidate)| !candidate.active)
        .min_by(|(_, left), (_, right)| {
            left.modified
                .cmp(&right.modified)
                .then_with(|| left.path.cmp(&right.path))
        })
    else {
        return;
    };
    if (candidate.modified, &candidate.path) > (oldest.modified, &oldest.path) {
        candidates[oldest_index] = candidate;
    }
}

struct TailRead {
    bytes: Vec<u8>,
    truncated: bool,
}

fn read_log_tail(path: &Path, limit: usize) -> Result<TailRead, ()> {
    let mut file = File::open(path).map_err(|_| ())?;
    let metadata = file.metadata().map_err(|_| ())?;
    if !metadata.is_file() {
        return Err(());
    }
    let file_len = metadata.len();
    let read_len = file_len.min(limit as u64);
    let start = file_len.saturating_sub(read_len);
    file.seek(SeekFrom::Start(start)).map_err(|_| ())?;
    let mut bytes = Vec::with_capacity(read_len as usize);
    file.take(read_len)
        .read_to_end(&mut bytes)
        .map_err(|_| ())?;
    Ok(TailRead {
        bytes,
        truncated: start > 0,
    })
}

fn retain_recent_entry(entries: &mut Vec<LogEntry>, entry: LogEntry, limit: usize) {
    if entries.len() < limit {
        entries.push(entry);
        return;
    }
    let Some((oldest_index, oldest)) = entries
        .iter()
        .enumerate()
        .min_by_key(|(_, entry)| entry.epoch_ms)
    else {
        return;
    };
    if entry.epoch_ms >= oldest.epoch_ms {
        entries[oldest_index] = entry;
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::*;
    use crate::app::{LogSeverity, LogSource};

    static NEXT_ROOT: AtomicU64 = AtomicU64::new(0);

    fn isolated_root(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "omen-structured-log-reader-{label}-{}-{}",
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
    fn reader_keeps_newest_entries_with_bounded_scan_and_tail_bytes() {
        let root = isolated_root("bounds");
        std::fs::create_dir_all(&root).expect("logs root");
        let raw = (0..12)
            .map(|epoch| encoded_entry(epoch, &format!("entry-{epoch}")))
            .collect::<Vec<_>>()
            .join("\n");
        std::fs::write(root.join("omenbrowser_rs.jsonl"), raw).expect("seed active log");
        let limits = PersistedLogLimits {
            entry_limit: 4,
            directory_entry_limit: 8,
            file_limit: 2,
            file_bytes: 4096,
            total_bytes: 4096,
        };

        let loaded = load_recent_log_entries(&root, limits);
        assert_eq!(
            loaded
                .entries
                .iter()
                .map(|entry| entry.epoch_ms)
                .collect::<Vec<_>>(),
            vec![8, 9, 10, 11]
        );
        assert_eq!(loaded.stats.files_read, 1);
        assert!(loaded.stats.bytes_read <= limits.total_bytes);
        let _ = std::fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn reader_does_not_follow_matching_symlinks() {
        use std::os::unix::fs::symlink;

        let root = isolated_root("symlink");
        std::fs::create_dir_all(&root).expect("logs root");
        let outside = root.with_extension("outside");
        std::fs::write(&outside, encoded_entry(9, "must-not-load")).expect("outside log");
        symlink(&outside, root.join("omenbrowser_rs-9.jsonl")).expect("matching symlink");

        let loaded = load_recent_log_entries(
            &root,
            PersistedLogLimits {
                entry_limit: 4,
                directory_entry_limit: 8,
                file_limit: 2,
                file_bytes: 4096,
                total_bytes: 4096,
            },
        );
        assert!(loaded.entries.is_empty());
        assert_eq!(loaded.stats.matching_files, 0);
        let _ = std::fs::remove_dir_all(root);
        let _ = std::fs::remove_file(outside);
    }
}
