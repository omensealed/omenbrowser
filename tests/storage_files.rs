use std::path::PathBuf;

use omenbrowser_rs::storage::files::{
    atomic_replace, atomic_write_new, atomic_write_new_bounded, next_available_download_path,
    sanitize_filename,
};

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "omenbrowser-rs-storage-files-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

#[tokio::test]
async fn bounded_atomic_download_writer_commits_parallel_files() {
    let dir = temp_dir("bounded-atomic-download");
    let writes = (0..8)
        .map(|index| {
            let path = dir.join(format!("download-{index}.bin"));
            tokio::spawn(
                async move { atomic_write_new_bounded(path, vec![index as u8; 1024]).await },
            )
        })
        .collect::<Vec<_>>();
    for write in writes {
        write
            .await
            .expect("bounded writer task")
            .expect("bounded atomic write");
    }
    for index in 0..8 {
        assert_eq!(
            std::fs::read(dir.join(format!("download-{index}.bin"))).expect("written file"),
            vec![index as u8; 1024]
        );
    }
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

#[test]
fn atomic_download_write_commits_complete_bytes_and_preserves_existing_files() {
    let dir = temp_dir("atomic-download");
    let path = dir.join("download.bin");
    atomic_write_new(&path, b"complete").expect("atomic write");
    assert_eq!(std::fs::read(&path).expect("committed bytes"), b"complete");
    assert_eq!(
        std::fs::read_dir(&dir)
            .expect("temporary listing")
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().ends_with(".tmp"))
            .count(),
        0
    );

    let error = atomic_write_new(&path, b"replacement").expect_err("existing file refusal");
    assert!(error.to_string().contains("already exists"));
    assert_eq!(std::fs::read(path).expect("preserved bytes"), b"complete");
}

#[test]
fn atomic_replace_replaces_existing_file_without_source_residue() {
    let dir = temp_dir("atomic-replace");
    let source = dir.join("replacement.tmp");
    let destination = dir.join("state.json");
    std::fs::write(&source, b"new").expect("replacement fixture");
    std::fs::write(&destination, b"old").expect("existing fixture");

    atomic_replace(&source, &destination).expect("atomic replacement");

    assert_eq!(
        std::fs::read(destination).expect("replacement bytes"),
        b"new"
    );
    assert!(!source.exists());
}
