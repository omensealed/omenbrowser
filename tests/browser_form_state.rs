use std::collections::BTreeMap;
use std::path::PathBuf;

use omenbrowser_rs::storage::form_state::{
    BrowserFormStateStore, BROWSER_FORM_STATE_CORRUPT_BACKUP_MAX_FILES,
    BROWSER_FORM_STATE_MAX_BYTES, BROWSER_FORM_STATE_MAX_FIELDS_PER_PAGE,
    BROWSER_FORM_STATE_MAX_FIELD_VALUE_BYTES, BROWSER_FORM_STATE_MAX_PAGES,
};

fn temp_file(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "omenbrowser-rs-form-state-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir.join("browser_form_state.json")
}

fn corrupt_backups(path: &std::path::Path) -> Vec<PathBuf> {
    let prefix = format!(
        "{}.corrupt.",
        path.file_name()
            .expect("form-state filename")
            .to_string_lossy()
    );
    let mut backups = std::fs::read_dir(path.parent().expect("parent"))
        .expect("read parent")
        .filter_map(Result::ok)
        .filter(|entry| {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            name.starts_with(&prefix) && name.ends_with(".bak")
        })
        .map(|entry| entry.path())
        .collect::<Vec<_>>();
    backups.sort();
    backups
}

#[test]
fn missing_form_state_loads_empty() {
    let store =
        BrowserFormStateStore::load_or_default(temp_file("missing")).expect("load form state");

    assert!(store.fields_for("mock.node:/").is_none());
}

#[test]
fn form_state_round_trips_by_page_url() {
    let path = temp_file("round-trip");
    let mut store = BrowserFormStateStore::load_or_default(path.clone()).expect("load form state");
    store
        .set_fields(
            "mock.node:/form.mu",
            BTreeMap::from([("nickname".into(), "mesh".into())]),
        )
        .expect("save form state");
    assert_eq!(
        std::fs::read_dir(path.parent().expect("form-state parent"))
            .expect("temporary listing")
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().ends_with(".tmp"))
            .count(),
        0
    );

    let store = BrowserFormStateStore::load_or_default(path.clone()).expect("reload form state");

    assert_eq!(
        store
            .fields_for("mock.node:/form.mu")
            .and_then(|fields| fields.get("nickname"))
            .map(String::as_str),
        Some("mesh")
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&path)
                .expect("form-state metadata")
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
}

#[test]
fn corrupt_form_state_is_backed_up_exactly_without_rewrite() {
    let path = temp_file("corrupt");
    let raw = b"{not json";
    std::fs::write(&path, raw).expect("write corrupt state");

    let store = BrowserFormStateStore::load_or_default(path.clone()).expect("load corrupt");

    assert!(store.fields_for("mock.node:/form.mu").is_none());
    assert_eq!(std::fs::read(&path).expect("read source"), raw);
    let backups = corrupt_backups(&path);
    assert_eq!(backups.len(), 1);
    assert_eq!(std::fs::read(&backups[0]).expect("read backup"), raw);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&backups[0])
                .expect("backup metadata")
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
}

#[test]
fn legacy_form_state_shape_loads_as_current_page_state() {
    let path = temp_file("legacy");
    std::fs::write(
        &path,
        r#"{"pages":{"mock.node:/form.mu":{"nickname":"mesh"}}}"#,
    )
    .expect("write legacy state");

    let store = BrowserFormStateStore::load_or_default(path).expect("load legacy");

    assert_eq!(
        store
            .fields_for("mock.node:/form.mu")
            .and_then(|fields| fields.get("nickname"))
            .map(String::as_str),
        Some("mesh")
    );
}

#[test]
fn prune_expired_removes_old_page_entries() {
    let path = temp_file("prune");
    let mut store = BrowserFormStateStore::load_or_default(path.clone()).expect("load form state");
    store
        .set_fields_at(
            "mock.node:/old.mu",
            BTreeMap::from([("name".into(), "old".into())]),
            1_000,
        )
        .expect("save old");
    store
        .set_fields_at(
            "mock.node:/fresh.mu",
            BTreeMap::from([("name".into(), "fresh".into())]),
            4_000,
        )
        .expect("save fresh");

    assert_eq!(store.prune_expired(5_000, 2).expect("prune"), 1);
    let store = BrowserFormStateStore::load_or_default(path).expect("reload");

    assert!(store.fields_for("mock.node:/old.mu").is_none());
    assert_eq!(
        store
            .fields_for("mock.node:/fresh.mu")
            .and_then(|fields| fields.get("name"))
            .map(String::as_str),
        Some("fresh")
    );
}

#[test]
fn remove_page_matching_and_clear_forget_entries() {
    let path = temp_file("forget");
    let mut store = BrowserFormStateStore::load_or_default(path.clone()).expect("load form state");
    store
        .set_fields(
            "mock.node:/one.mu",
            BTreeMap::from([("name".into(), "one".into())]),
        )
        .expect("save one");
    store
        .set_fields(
            "other.node:/two.mu",
            BTreeMap::from([("name".into(), "two".into())]),
        )
        .expect("save two");

    assert_eq!(store.page_count(), 2);
    assert_eq!(
        store
            .remove_pages_matching(|url| url.starts_with("mock.node:"))
            .expect("remove matching"),
        1
    );
    assert!(store.fields_for("mock.node:/one.mu").is_none());
    assert!(store.fields_for("other.node:/two.mu").is_some());
    assert!(store
        .remove_page("other.node:/two.mu")
        .expect("remove page"));
    assert_eq!(store.page_count(), 0);

    let store = BrowserFormStateStore::load_or_default(path).expect("reload");
    assert_eq!(store.page_count(), 0);
}

#[test]
fn oversized_form_state_file_is_rejected_before_read() {
    let path = temp_file("oversized-file");
    std::fs::File::create(&path)
        .and_then(|file| file.set_len(BROWSER_FORM_STATE_MAX_BYTES + 1))
        .expect("oversized sparse form state");

    let error = BrowserFormStateStore::load_or_default(path.clone())
        .expect_err("reject oversized form state");

    assert!(error.to_string().contains("4194304 byte limit"));
    assert_eq!(
        std::fs::metadata(&path).expect("source metadata").len(),
        BROWSER_FORM_STATE_MAX_BYTES + 1
    );
    assert!(corrupt_backups(&path).is_empty());
}

#[test]
fn exact_byte_limit_with_json_whitespace_is_accepted() {
    let path = temp_file("exact-limit");
    let mut raw = b"{\"pages\":{}}".to_vec();
    raw.resize(BROWSER_FORM_STATE_MAX_BYTES as usize, b' ');
    std::fs::write(&path, raw).expect("write exact-limit state");

    let store =
        BrowserFormStateStore::load_or_default(path.clone()).expect("load exact-limit state");

    assert_eq!(store.page_count(), 0);
    assert!(corrupt_backups(&path).is_empty());
}

#[test]
fn directory_form_state_is_rejected_without_backup() {
    let path = temp_file("directory");
    std::fs::create_dir(&path).expect("create directory state");

    let error =
        BrowserFormStateStore::load_or_default(path.clone()).expect_err("reject directory state");

    assert!(error.to_string().contains("regular file"));
    assert!(path.is_dir());
    assert!(corrupt_backups(&path).is_empty());
}

#[cfg(unix)]
#[test]
fn symlink_form_state_is_rejected_without_touching_target() {
    use std::os::unix::fs::symlink;

    let path = temp_file("symlink");
    let target = path.parent().expect("parent").join("target.json");
    std::fs::write(&target, b"target bytes").expect("write target");
    symlink(&target, &path).expect("create symlink");

    let error =
        BrowserFormStateStore::load_or_default(path.clone()).expect_err("reject symlink state");

    assert!(error.to_string().contains("regular file"));
    assert!(std::fs::symlink_metadata(&path)
        .expect("symlink metadata")
        .file_type()
        .is_symlink());
    assert_eq!(std::fs::read(target).expect("read target"), b"target bytes");
    assert!(corrupt_backups(&path).is_empty());
}

#[test]
fn corrupt_backup_retention_is_bounded_and_ignores_legacy_names() {
    let path = temp_file("backup-retention");
    let parent = path.parent().expect("parent");
    std::fs::write(&path, b"invalid").expect("write invalid state");
    let legacy = parent.join("browser_form_state.corrupt-legacy");
    std::fs::write(&legacy, b"legacy").expect("write legacy backup");

    for _ in 0..BROWSER_FORM_STATE_CORRUPT_BACKUP_MAX_FILES + 3 {
        let store =
            BrowserFormStateStore::load_or_default(path.clone()).expect("load invalid state");
        assert_eq!(store.page_count(), 0);
    }

    assert_eq!(
        corrupt_backups(&path).len(),
        BROWSER_FORM_STATE_CORRUPT_BACKUP_MAX_FILES
    );
    assert_eq!(
        std::fs::read(legacy).expect("read legacy backup"),
        b"legacy"
    );
}

#[test]
fn failed_remove_restores_prior_in_memory_state() {
    let path = temp_file("remove-rollback");
    let mut store = BrowserFormStateStore::load_or_default(path.clone()).expect("load state");
    store
        .set_fields(
            "mock.node:/form.mu",
            BTreeMap::from([("nickname".into(), "mesh".into())]),
        )
        .expect("save state");
    std::fs::remove_file(&path).expect("remove state file");
    std::fs::create_dir(&path).expect("replace target with directory");

    let error = store
        .remove_page("mock.node:/form.mu")
        .expect_err("reject persistence target");

    assert!(error.to_string().contains("regular file"));
    assert_eq!(
        store
            .fields_for("mock.node:/form.mu")
            .and_then(|fields| fields.get("nickname"))
            .map(String::as_str),
        Some("mesh")
    );
}

#[test]
fn rejected_page_does_not_replace_previous_form_state() {
    let path = temp_file("rejected-page");
    let mut store = BrowserFormStateStore::load_or_default(path).expect("form state");
    store
        .set_fields(
            "mock.node:/form.mu",
            BTreeMap::from([("name".into(), "previous".into())]),
        )
        .expect("initial state");

    let too_many = (0..=BROWSER_FORM_STATE_MAX_FIELDS_PER_PAGE)
        .map(|index| (format!("field-{index}"), "value".to_string()))
        .collect::<BTreeMap<_, _>>();
    assert!(store.set_fields("mock.node:/form.mu", too_many).is_err());
    assert!(store
        .set_fields(
            "mock.node:/form.mu",
            BTreeMap::from([(
                "name".into(),
                "x".repeat(BROWSER_FORM_STATE_MAX_FIELD_VALUE_BYTES + 1),
            )]),
        )
        .is_err());
    assert_eq!(
        store
            .fields_for("mock.node:/form.mu")
            .and_then(|fields| fields.get("name"))
            .map(String::as_str),
        Some("previous")
    );
}

#[test]
fn loaded_form_state_keeps_newest_pages_within_item_limit() {
    let path = temp_file("page-limit");
    let pages = (0..=BROWSER_FORM_STATE_MAX_PAGES)
        .map(|index| {
            (
                format!("mock.node:/{index}.mu"),
                serde_json::json!({
                    "updated_epoch_ms": index,
                    "fields": {"name": index.to_string()}
                }),
            )
        })
        .collect::<serde_json::Map<_, _>>();
    std::fs::write(
        &path,
        serde_json::to_vec(&serde_json::json!({"pages": pages})).expect("fixture JSON"),
    )
    .expect("page-limit fixture");

    let store = BrowserFormStateStore::load_or_default(path).expect("bounded state");

    assert_eq!(store.page_count(), BROWSER_FORM_STATE_MAX_PAGES);
    assert!(store.fields_for("mock.node:/0.mu").is_none());
    assert!(store
        .fields_for(&format!("mock.node:/{BROWSER_FORM_STATE_MAX_PAGES}.mu"))
        .is_some());
}
