use std::path::{Path, PathBuf};

use crate::error::AppResult;

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
