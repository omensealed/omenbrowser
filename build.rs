use std::{env, fs, path::Path, process::Command};

fn main() {
    println!("cargo:rerun-if-env-changed=OMENBROWSER_GIT_COMMIT");
    println!("cargo:rerun-if-env-changed=GITHUB_SHA");
    track_git_head();

    let git_commit = env::var("OMENBROWSER_GIT_COMMIT")
        .ok()
        .and_then(normalize_commit)
        .or_else(|| env::var("GITHUB_SHA").ok().and_then(normalize_commit))
        .or_else(commit_from_checkout)
        .unwrap_or_else(|| "unknown".to_string());
    let target = env::var("TARGET").unwrap_or_else(|_| "unknown".to_string());

    println!("cargo:rustc-env=OMENBROWSER_BUILD_GIT_COMMIT={git_commit}");
    println!("cargo:rustc-env=OMENBROWSER_BUILD_TARGET={target}");
}

fn normalize_commit(value: String) -> Option<String> {
    let value = value.trim();
    (value.len() >= 7 && value.len() <= 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .then(|| value.to_ascii_lowercase())
}

fn commit_from_checkout() -> Option<String> {
    let output = Command::new("git")
        .args(["rev-parse", "--verify", "HEAD"])
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
        .and_then(normalize_commit)
}

fn track_git_head() {
    let git_dir = Path::new(".git");
    let head = git_dir.join("HEAD");
    if !head.is_file() {
        return;
    }
    println!("cargo:rerun-if-changed={}", head.display());

    let Ok(contents) = fs::read_to_string(&head) else {
        return;
    };
    let Some(reference) = contents.trim().strip_prefix("ref: ") else {
        return;
    };
    let reference = git_dir.join(reference);
    if reference.is_file() {
        println!("cargo:rerun-if-changed={}", reference.display());
    }
}
