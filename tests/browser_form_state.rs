use std::collections::BTreeMap;
use std::path::PathBuf;

use omenbrowser_rs::storage::form_state::BrowserFormStateStore;

fn temp_file(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "omenbrowser-rs-form-state-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir.join("browser_form_state.json")
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

    let store = BrowserFormStateStore::load_or_default(path).expect("reload form state");

    assert_eq!(
        store
            .fields_for("mock.node:/form.mu")
            .and_then(|fields| fields.get("nickname"))
            .map(String::as_str),
        Some("mesh")
    );
}

#[test]
fn corrupt_form_state_falls_back_empty_without_rewrite() {
    let path = temp_file("corrupt");
    std::fs::write(&path, "{not json").expect("write corrupt state");

    let store = BrowserFormStateStore::load_or_default(path).expect("load corrupt");

    assert!(store.fields_for("mock.node:/form.mu").is_none());
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
