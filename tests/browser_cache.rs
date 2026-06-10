use std::collections::BTreeMap;
use std::path::PathBuf;

use omenbrowser_rs::browser::cache::{cache_ttl_for_markup, PageCache, DEFAULT_CACHE_SECONDS};

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "omenbrowser-rs-cache-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

#[test]
fn parses_cache_ttl_from_markup() {
    assert_eq!(cache_ttl_for_markup("#!c=60\nbody"), 60);
    assert_eq!(
        cache_ttl_for_markup("#!fg=ccc\n#!bg=000\n#!c=1440\nbody"),
        1440
    );
    assert_eq!(
        cache_ttl_for_markup("#!fg=ccc\nbody\n#!c=0"),
        DEFAULT_CACHE_SECONDS
    );
    assert_eq!(cache_ttl_for_markup("#c=0\nbody"), 0);
    assert_eq!(cache_ttl_for_markup("#!c=bad\nbody"), DEFAULT_CACHE_SECONDS);
}

#[test]
fn cache_store_load_delete_round_trip() {
    let cache = PageCache::new(temp_dir("round-trip")).expect("create cache");

    cache
        .store("mock.node:/", "markup", 60, "Title", BTreeMap::new())
        .expect("store cache");
    let record = cache
        .load("mock.node:/")
        .expect("load cache")
        .expect("record");
    assert_eq!(record.markup, "markup");

    cache.delete("mock.node:/").expect("delete cache");
    assert!(cache.load("mock.node:/").expect("load cache").is_none());
}
