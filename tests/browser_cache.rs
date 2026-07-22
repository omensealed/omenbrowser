use std::collections::BTreeMap;
use std::ops::Deref;
use std::path::{Path, PathBuf};

use omenbrowser_rs::browser::cache::{
    cache_ttl_for_markup, PageCache, DEFAULT_CACHE_SECONDS, PAGE_CACHE_MAX_BYTES,
    PAGE_CACHE_MAX_ITEMS, PAGE_CACHE_MAX_RECORD_BYTES,
};

struct TestDirectory(PathBuf);

impl Deref for TestDirectory {
    type Target = Path;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl AsRef<Path> for TestDirectory {
    fn as_ref(&self) -> &Path {
        &self.0
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn temp_dir(name: &str) -> TestDirectory {
    let dir = std::env::temp_dir().join(format!(
        "omenbrowser-rs-cache-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create temp dir");
    TestDirectory(dir)
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
    let root = temp_dir("round-trip");
    let cache = PageCache::new(root.to_path_buf()).expect("create cache");

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

#[test]
fn cache_lookup_is_deterministic_and_item_budget_evicts_oldest() {
    let root = temp_dir("indexed-budget");
    let cache = PageCache::new(root.to_path_buf()).expect("create cache");
    for index in 0..=PAGE_CACHE_MAX_ITEMS {
        cache
            .store(
                &format!("node:{index}"),
                "markup",
                60,
                "Title",
                BTreeMap::new(),
            )
            .expect("store indexed cache");
    }
    assert_eq!(
        cache.entry_count().expect("entry count"),
        PAGE_CACHE_MAX_ITEMS
    );
    assert!(cache.load("node:0").expect("oldest lookup").is_none());
    assert!(cache
        .load(&format!("node:{PAGE_CACHE_MAX_ITEMS}"))
        .expect("newest lookup")
        .is_some());
    assert!(cache.total_bytes().expect("cache bytes") <= PAGE_CACHE_MAX_BYTES);

    let unrelated = root.join("unrelated.mu");
    std::fs::write(&unrelated, b"not a cache record").expect("unrelated fixture");
    assert!(cache
        .load(&format!("node:{PAGE_CACHE_MAX_ITEMS}"))
        .expect("deterministic lookup")
        .is_some());
    assert!(
        unrelated.exists(),
        "normal lookup must not scan cleanup candidates"
    );
}

#[test]
fn cache_rejects_single_record_above_byte_budget() {
    let root = temp_dir("oversize");
    let cache = PageCache::new(root.to_path_buf()).expect("create cache");
    let markup = "x".repeat(PAGE_CACHE_MAX_RECORD_BYTES as usize);
    let error = cache
        .store("oversize", &markup, 60, "Title", BTreeMap::new())
        .expect_err("oversized record");
    assert!(error.to_string().contains("byte limit"));
    assert_eq!(cache.entry_count().expect("entry count"), 0);
}

#[test]
fn cache_byte_budget_evicts_oldest_record() {
    let root = temp_dir("byte-budget");
    let cache = PageCache::new(root.to_path_buf()).expect("create cache");
    let markup = "x".repeat(PAGE_CACHE_MAX_RECORD_BYTES as usize - 1024);
    let records = PAGE_CACHE_MAX_BYTES / (markup.len() as u64) + 1;
    for index in 0..records {
        cache
            .store(
                &format!("large:{index}"),
                &markup,
                60,
                "Title",
                BTreeMap::new(),
            )
            .expect("store bounded record");
    }
    assert!(cache.total_bytes().expect("cache bytes") <= PAGE_CACHE_MAX_BYTES);
    assert!(cache.load("large:0").expect("oldest lookup").is_none());
    assert!(cache
        .load(&format!("large:{}", records - 1))
        .expect("newest lookup")
        .is_some());
}

#[test]
fn cache_rebuild_migrates_legacy_expiry_filename_once() {
    let root = temp_dir("legacy-migration");
    let cache = PageCache::new(root.to_path_buf()).expect("create cache");
    let canonical = cache
        .store("legacy.node:/", "markup", 60, "Legacy", BTreeMap::new())
        .expect("store cache")
        .expect("cache path");
    let legacy = root.join(format!(
        "{}_9999999999.mu",
        canonical.file_stem().expect("hash stem").to_string_lossy()
    ));
    std::fs::rename(&canonical, &legacy).expect("legacy rename");
    std::fs::remove_file(root.join(".page-cache-index.json")).expect("remove index");

    let reopened = PageCache::new(root.to_path_buf()).expect("rebuild cache index");
    assert!(reopened
        .load("legacy.node:/")
        .expect("migrated lookup")
        .is_some());
    assert!(!legacy.exists());
}

#[test]
#[ignore = "release-mode cache latency measurement"]
fn measure_cache_index_latency() {
    let root = temp_dir("latency-measurement");
    let cache = PageCache::new(root.to_path_buf()).expect("create cache");
    for index in 0..PAGE_CACHE_MAX_ITEMS {
        cache
            .store(
                &format!("measure:{index}"),
                "measurement payload",
                3_600,
                "Measurement",
                BTreeMap::new(),
            )
            .expect("measurement cache entry");
    }

    let mut indexed = Vec::with_capacity(1_000);
    let mut scan_shape = Vec::with_capacity(1_000);
    for iteration in 0..1_000 {
        let started = std::time::Instant::now();
        assert!(cache
            .load(&format!("measure:{}", iteration % PAGE_CACHE_MAX_ITEMS))
            .expect("indexed measurement lookup")
            .is_some());
        indexed.push(started.elapsed().as_nanos() as u64);

        let started = std::time::Instant::now();
        let count = std::fs::read_dir(&root)
            .expect("scan-shape listing")
            .filter_map(Result::ok)
            .filter(|entry| entry.path().extension().and_then(|value| value.to_str()) == Some("mu"))
            .filter(|entry| entry.metadata().is_ok_and(|metadata| metadata.is_file()))
            .count();
        assert_eq!(count, PAGE_CACHE_MAX_ITEMS);
        scan_shape.push(started.elapsed().as_nanos() as u64);
    }
    indexed.sort_unstable();
    scan_shape.sort_unstable();
    println!("page_cache_entries={PAGE_CACHE_MAX_ITEMS}");
    println!("page_indexed_median_ns={}", indexed[indexed.len() / 2]);
    println!(
        "page_indexed_p95_ns={}",
        indexed[(indexed.len() * 95).div_ceil(100) - 1]
    );
    println!(
        "page_scan_shape_median_ns={}",
        scan_shape[scan_shape.len() / 2]
    );
    println!(
        "page_scan_shape_p95_ns={}",
        scan_shape[(scan_shape.len() * 95).div_ceil(100) - 1]
    );
}
