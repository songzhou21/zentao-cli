use super::*;

// Unix 时间戳转 Chrome epoch 微秒，便于断言转换逻辑。
fn unix_to_chrome_expires(unix: i64) -> i64 {
    (unix + 11_644_473_600) * 1_000_000
}

// 覆盖根域、子域命中与不命中三种情况。
#[test]
fn test_host_matches() {
    assert!(host_matches("a.example.com", ".example.com"));
    assert!(host_matches("example.com", "example.com"));
    assert!(!host_matches("bad.com", ".example.com"));
}

// 路径归一化应保证末尾带 /，空路径归一为 /。
#[test]
fn test_normalize_path() {
    assert_eq!(normalize_path(""), "/");
    assert_eq!(normalize_path("/zentao"), "/zentao/");
    assert_eq!(normalize_path("/zentao/"), "/zentao/");
}

// Chrome epoch 与 Unix epoch 转换结果应可预期且稳定。
#[test]
fn test_expires_convert() {
    assert_eq!(chrome_expires_utc_to_unix(0), 0);
    let expires = unix_to_chrome_expires(1704067200);
    assert_eq!(chrome_expires_utc_to_unix(expires), 1704067200);
}

// 同名 cookie 应选择“未过期 + path 最长”的项。
#[test]
fn test_choose_best_by_path() {
    let future = unix_to_chrome_expires(
        (std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time should be valid")
            .as_secs() as i64)
            + 24 * 3600,
    );
    let past = unix_to_chrome_expires(
        (std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time should be valid")
            .as_secs() as i64)
            - 24 * 3600,
    );

    let items = vec![
        BrowserCookieItem {
            name: "zp".to_string(),
            value: "expired".to_string(),
            domain: "example.com".to_string(),
            path: "/zentao/deep/".to_string(),
            secure: false,
            http_only: true,
            expires_utc: past,
        },
        BrowserCookieItem {
            name: "zp".to_string(),
            value: "root".to_string(),
            domain: "example.com".to_string(),
            path: "/".to_string(),
            secure: false,
            http_only: true,
            expires_utc: future,
        },
        BrowserCookieItem {
            name: "zp".to_string(),
            value: "deep".to_string(),
            domain: "example.com".to_string(),
            path: "/zentao/".to_string(),
            secure: false,
            http_only: true,
            expires_utc: future,
        },
    ];

    let best = choose_best_by_path(&items, "zp").expect("best cookie should exist");
    assert_eq!(best.value, "deep");
}
