use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, LazyLock};

use crate::error::AppResult;

static DOWNLOAD_TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(1);
static DOWNLOAD_WRITE_PERMITS: LazyLock<Arc<tokio::sync::Semaphore>> =
    LazyLock::new(|| Arc::new(tokio::sync::Semaphore::new(2)));

pub fn sanitize_filename(filename: &str) -> String {
    let sanitized = filename
        .chars()
        .map(|ch| match ch {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            ch if ch.is_control() => '_',
            ch => ch,
        })
        .collect::<String>()
        .trim()
        .trim_matches('.')
        .to_string();

    if sanitized.is_empty() {
        "download.bin".into()
    } else {
        sanitized
    }
}

pub fn next_available_download_path(downloads_dir: &Path, filename: &str) -> AppResult<PathBuf> {
    std::fs::create_dir_all(downloads_dir)?;
    let filename = sanitize_filename(filename);
    let candidate = downloads_dir.join(&filename);
    if !candidate.exists() {
        return Ok(candidate);
    }

    let path = Path::new(&filename);
    let stem = path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("download");
    let extension = path.extension().and_then(|value| value.to_str());

    for counter in 1_u32.. {
        let numbered = match extension {
            Some(extension) if !extension.is_empty() => format!("{stem}-{counter}.{extension}"),
            _ => format!("{stem}-{counter}"),
        };
        let candidate = downloads_dir.join(numbered);
        if !candidate.exists() {
            return Ok(candidate);
        }
    }

    unreachable!("unbounded counter should always return before overflow in practical use")
}

pub fn atomic_write_new(path: &Path, bytes: &[u8]) -> AppResult<()> {
    if path.exists() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            "download destination already exists",
        )
        .into());
    }
    let parent = path.parent().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "download destination has no parent",
        )
    })?;
    std::fs::create_dir_all(parent)?;
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "download destination has no safe file name",
            )
        })?;
    let sequence = DOWNLOAD_TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let temporary = parent.join(format!(
        ".{file_name}.{}.{}.tmp",
        std::process::id(),
        sequence
    ));
    let result = (|| -> std::io::Result<()> {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        file.write_all(bytes)?;
        file.flush()?;
        file.sync_all()?;
        drop(file);
        if path.exists() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::AlreadyExists,
                "download destination appeared before commit",
            ));
        }
        std::fs::rename(&temporary, path)?;
        #[cfg(unix)]
        std::fs::File::open(parent)?.sync_all()?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(temporary);
    }
    result.map_err(Into::into)
}

#[cfg(not(windows))]
pub fn atomic_replace(source: &Path, destination: &Path) -> std::io::Result<()> {
    std::fs::rename(source, destination)
}

#[cfg(windows)]
pub fn atomic_replace(source: &Path, destination: &Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn MoveFileExW(
            existing_file_name: *const u16,
            new_file_name: *const u16,
            flags: u32,
        ) -> i32;
    }

    const MOVEFILE_REPLACE_EXISTING: u32 = 0x1;
    const MOVEFILE_WRITE_THROUGH: u32 = 0x8;
    let source = source
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let destination = destination
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    // SAFETY: both pointers reference NUL-terminated UTF-16 buffers that remain
    // alive for the duration of the synchronous Win32 call.
    let replaced = unsafe {
        MoveFileExW(
            source.as_ptr(),
            destination.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if replaced == 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

pub async fn atomic_write_new_bounded(path: PathBuf, bytes: Vec<u8>) -> AppResult<()> {
    let permit = DOWNLOAD_WRITE_PERMITS
        .clone()
        .acquire_owned()
        .await
        .map_err(|error| crate::error::AppError::Runtime(error.to_string()))?;
    tokio::task::spawn_blocking(move || {
        let _permit = permit;
        atomic_write_new(&path, &bytes)
    })
    .await
    .map_err(|error| crate::error::AppError::Runtime(error.to_string()))?
}
