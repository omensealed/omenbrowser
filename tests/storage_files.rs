use std::path::PathBuf;

use omenbrowser_rs::storage::files::{next_available_download_path, sanitize_filename};

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "omenbrowser-rs-storage-files-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

#[test]
fn sanitize_filename_replaces_unsafe_characters_and_empty_names() {
    assert_eq!(sanitize_filename("../bad:name?.txt"), "_bad_name_.txt");
    assert_eq!(sanitize_filename("..."), "download.bin");
}

#[test]
fn next_available_download_path_never_overwrites_existing_file() {
    let dir = temp_dir("download-path");
    std::fs::write(dir.join("file.txt"), b"old").expect("seed file");

    let next = next_available_download_path(&dir, "file.txt").expect("next path");

    assert_eq!(
        next.file_name().and_then(|name| name.to_str()),
        Some("file-1.txt")
    );
}
