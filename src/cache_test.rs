use super::*;
use crate::search::{CandidateOption, KIND_BUILD, KIND_MODULE};
use chrono::{Duration, Local};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

fn fixture_browse() -> &'static str {
    include_str!("../tests/fixtures/search/browse_bysearch_myqueryid.json")
}

fn sample_build() -> CandidateOption {
    CandidateOption {
        value: "982".into(),
        name: "1.2.17-iOS-0831".into(),
    }
}

#[test]
fn default_dir_suffix() {
    let path = default_dir().expect("default cache dir");
    let s = path.to_string_lossy().replace('\\', "/");
    assert!(s.ends_with("/.zentao/cache"), "unexpected path: {s}");
}

#[test]
fn file_path_is_product_json() {
    let path = file_path(Path::new("/tmp/cache"), 92);
    assert_eq!(path, Path::new("/tmp/cache/92.json"));
}

#[test]
fn save_and_load_roundtrip() {
    let dir = tempfile::tempdir().expect("temp dir");
    let catalog = Catalog {
        fetched_at: "2026-08-21T11:30:00+08:00".into(),
        kinds: BTreeMap::from([(KIND_BUILD.to_string(), vec![sample_build()])]),
    };
    save(dir.path(), 92, &catalog).expect("save");
    let loaded = load(dir.path(), 92).expect("load").expect("present");
    assert_eq!(loaded, catalog);
    let raw = fs::read_to_string(file_path(dir.path(), 92)).expect("read");
    assert!(raw.contains("\"fetchedAt\""));
    assert!(raw.contains("\"build\""));
    assert!(!raw.contains("\"site\""));
    assert!(!raw.contains("\"product\""));
}

#[test]
fn load_missing_is_none() {
    let dir = tempfile::tempdir().expect("temp dir");
    assert!(load(dir.path(), 92).expect("load").is_none());
}

#[test]
fn load_corrupt_is_none() {
    let dir = tempfile::tempdir().expect("temp dir");
    fs::write(file_path(dir.path(), 92), "{").expect("write");
    assert!(load(dir.path(), 92).expect("load").is_none());
}

#[test]
fn load_fresh_kind_hits_within_ttl() {
    let dir = tempfile::tempdir().expect("temp dir");
    let catalog = Catalog {
        fetched_at: Local::now().to_rfc3339(),
        kinds: BTreeMap::from([(KIND_BUILD.to_string(), vec![sample_build()])]),
    };
    save(dir.path(), 92, &catalog).expect("save");
    let builds = load_fresh_kind(dir.path(), 92, KIND_BUILD)
        .expect("load")
        .expect("hit");
    assert_eq!(builds, vec![sample_build()]);
}

#[test]
fn load_fresh_kind_misses_when_stale() {
    let dir = tempfile::tempdir().expect("temp dir");
    let fetched = (Local::now() - Duration::hours(2)).to_rfc3339();
    let catalog = Catalog {
        fetched_at: fetched,
        kinds: BTreeMap::from([(KIND_BUILD.to_string(), vec![sample_build()])]),
    };
    save(dir.path(), 92, &catalog).expect("save");
    assert!(load_fresh_kind(dir.path(), 92, KIND_BUILD)
        .expect("load")
        .is_none());
}

#[test]
fn load_fresh_kind_misses_when_kind_absent() {
    let dir = tempfile::tempdir().expect("temp dir");
    let catalog = Catalog {
        fetched_at: Local::now().to_rfc3339(),
        kinds: BTreeMap::from([(KIND_BUILD.to_string(), vec![sample_build()])]),
    };
    save(dir.path(), 92, &catalog).expect("save");
    assert!(load_fresh_kind(dir.path(), 92, KIND_MODULE)
        .expect("load")
        .is_none());
}

#[test]
fn save_from_browse_json_stores_build_and_module() {
    let dir = tempfile::tempdir().expect("temp dir");
    let catalog = save_from_browse_json(dir.path(), 92, fixture_browse()).expect("save");
    assert!(is_fresh(&catalog));
    let builds = catalog.kinds.get(KIND_BUILD).expect("build");
    assert!(builds.iter().any(|row| row.value == "982"));
    let modules = catalog.kinds.get(KIND_MODULE).expect("module");
    assert!(modules
        .iter()
        .any(|row| row.value == "1143" && row.name == "/IM"));
    assert!(modules.len() >= 70);
}

#[test]
fn load_or_fetch_uses_cache_when_fresh() {
    let dir = tempfile::tempdir().expect("temp dir");
    save_from_browse_json(dir.path(), 92, fixture_browse()).expect("save");
    let rows = load_or_fetch(dir.path(), 92, KIND_BUILD, || {
        panic!("should not fetch");
    })
    .expect("hit");
    assert!(rows.iter().any(|row| row.value == "982"));
}

#[test]
fn load_or_fetch_fetches_when_missing() {
    let dir = tempfile::tempdir().expect("temp dir");
    let mut fetched = false;
    let rows = load_or_fetch(dir.path(), 92, KIND_BUILD, || {
        fetched = true;
        Ok(fixture_browse().to_string())
    })
    .expect("fetch");
    assert!(fetched);
    assert!(rows.iter().any(|row| row.value == "982"));
    assert!(file_path(dir.path(), 92).exists());
}

#[test]
fn load_or_fetch_fetches_when_stale() {
    let dir = tempfile::tempdir().expect("temp dir");
    let catalog = Catalog {
        fetched_at: (Local::now() - Duration::hours(2)).to_rfc3339(),
        kinds: BTreeMap::from([(KIND_BUILD.to_string(), vec![sample_build()])]),
    };
    save(dir.path(), 92, &catalog).expect("save");
    let mut fetched = false;
    let rows = load_or_fetch(dir.path(), 92, KIND_BUILD, || {
        fetched = true;
        Ok(fixture_browse().to_string())
    })
    .expect("fetch");
    assert!(fetched);
    assert!(rows
        .iter()
        .any(|row| row.value == "982" && row.name.contains("1.2.17-iOS")));
}
