use super::*;
use std::path::Path;

// 默认配置路径应落在 ~/.zentao/config.json。
#[test]
fn default_config_path_suffix() {
    let p = default_config_path().expect("default path should resolve");
    let s = p.to_string_lossy().replace('\\', "/");
    assert!(s.ends_with("/.zentao/config.json"), "unexpected path: {s}");
}

// 配置写入后应可无损读取关键字段。
#[test]
fn save_and_load_config() {
    let dir = tempfile::tempdir().expect("temp dir should create");
    let path = dir.path().join("nested").join("config.json");

    let cfg = Config {
        url: "http://example.com/zentao".to_string(),
        chrome_profile: Some("/tmp/profile".to_string()),
        cookie_source: CookieSource::File,
    };
    save_config(&path, &cfg).expect("save should succeed");

    let loaded = load_config(&path).expect("load should succeed");
    assert_eq!(loaded.url, cfg.url);
    assert_eq!(loaded.chrome_profile, cfg.chrome_profile);
}

// 文件不存在时，optional 返回 None，default 返回非空默认配置。
#[test]
fn load_optional_and_default_when_missing() {
    let dir = tempfile::tempdir().expect("temp dir should create");
    let missing = dir.path().join("missing.json");

    let got = load_config_optional(&missing).expect("optional load should succeed");
    assert!(got.is_none());

    let got = load_or_default(&missing).expect("default load should succeed");
    assert!(got.url.is_empty());
    assert!(got.chrome_profile.is_none());
    assert_eq!(got.cookie_source, CookieSource::Chrome);
}

// 非法 JSON 应返回“配置文件存在但无法解析”错误。
#[test]
fn load_optional_invalid_json() {
    let dir = tempfile::tempdir().expect("temp dir should create");
    let path = dir.path().join("bad.json");
    std::fs::write(&path, b"{").expect("write should succeed");

    let err = load_config_optional(&path).expect_err("invalid json should fail");
    assert!(
        err.to_string().contains("配置文件存在但无法解析"),
        "unexpected err: {err}"
    );
}

// 空字段不应被序列化，避免写入无意义 null/空串字段。
#[test]
fn save_config_skips_empty_fields() {
    let dir = tempfile::tempdir().expect("temp dir should create");
    let path = dir.path().join("config.json");

    let cfg = Config::default();
    save_config(&path, &cfg).expect("save should succeed");

    let raw = std::fs::read_to_string(Path::new(&path)).expect("read should succeed");
    assert_eq!(raw.trim(), "{}");
}

#[test]
fn cookie_source_file_roundtrip() {
    let dir = tempfile::tempdir().expect("temp dir should create");
    let path = dir.path().join("config.json");

    let cfg = Config {
        cookie_source: CookieSource::File,
        ..Config::default()
    };
    save_config(&path, &cfg).expect("save should succeed");

    let loaded = load_config(&path).expect("load should succeed");
    assert_eq!(loaded.cookie_source, CookieSource::File);

    // Verify that the non-default value "file" IS serialized
    let raw = std::fs::read_to_string(&path).expect("read should succeed");
    assert!(raw.contains("cookie_source"), "non-default cookie_source should be serialized");
}
