use super::*;

// 同时提供 CLI 与配置时，CLI 参数优先级最高。
#[test]
fn resolve_required_should_prefer_cli() {
    let got = resolve_required(Some("http://cli"), Some("http://cfg"), "url").unwrap();
    assert_eq!(got, "http://cli");
}

// CLI 缺失时应回退配置值。
#[test]
fn resolve_required_should_fallback_to_config() {
    let got = resolve_required(None, Some("http://cfg"), "url").unwrap();
    assert_eq!(got, "http://cfg");
}

// CLI 与配置都缺失时应返回缺参错误。
#[test]
fn resolve_required_should_error_when_missing() {
    let err = resolve_required(None, None, "url").expect_err("should fail");
    assert!(err.to_string().contains("缺少 url"));
}

// session cookie（expires=0）应输出固定文案 session。
#[test]
fn format_cookie_expiry_session() {
    assert_eq!(format_cookie_expiry(0), "session");
}

// 已知 expires_utc 应转换成预期 UTC 时间字符串。
#[test]
fn format_cookie_expiry_known_timestamp() {
    let expires_utc = (1704067200_i64 + 11_644_473_600_i64) * 1_000_000_i64;
    assert_eq!(format_cookie_expiry(expires_utc), "2024-01-01 00:00:00 UTC");
}

// Chrome epoch 微秒应可精确转换回 Unix 秒。
#[test]
fn chrome_expires_utc_to_unix_known_timestamp() {
    let expires_utc = (1704067200_i64 + 11_644_473_600_i64) * 1_000_000_i64;
    assert_eq!(chrome_expires_utc_to_unix(expires_utc), 1704067200_i64);
}

// bug 输入为纯数字时应直接解析成 id。
#[test]
fn parse_bug_id_or_url_numeric() {
    assert_eq!(parse_bug_id_or_url("51214").unwrap(), 51214);
}

// bug 输入为详情 URL 时应提取 id。
#[test]
fn parse_bug_id_or_url_detail_url() {
    let url = "http://shendao.sharexm.cn/zentao/bug-view-51214.html";
    assert_eq!(parse_bug_id_or_url(url).unwrap(), 51214);
}

// URL 带查询参数时也应能提取 id。
#[test]
fn parse_bug_id_or_url_with_query() {
    let url = "http://shendao.sharexm.cn/zentao/bug-view-51214.html?tid=1";
    assert_eq!(parse_bug_id_or_url(url).unwrap(), 51214);
}

// 非法输入应返回明确错误。
#[test]
fn parse_bug_id_or_url_invalid() {
    let err = parse_bug_id_or_url("http://shendao/share/bug-xxx").expect_err("should fail");
    assert!(err.to_string().contains("Bug ID 无效"));
}
