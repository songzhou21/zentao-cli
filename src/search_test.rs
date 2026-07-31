use super::*;

#[test]
fn parses_search_fixture() {
    let html = include_str!("../tests/fixtures/search/search_assigned_to_zhousong.html");
    let result = parse_search_result(html).expect("parse");
    assert_eq!(result.bugs.len(), 2);
    assert_eq!(result.bugs[0].id, 51276);
    assert_eq!(result.bugs[0].assigned_to, "用户乙");
    assert_eq!(result.bugs[0].deadline, "0000-00-00");
}

#[test]
fn parses_empty_search_fixture() {
    let html = include_str!("../tests/fixtures/search/search_empty_result.html");
    let result = parse_search_result(html).expect("parse");
    assert!(result.bugs.is_empty());
}

#[test]
fn missing_table_is_empty_result() {
    let html = "<html><title>享脉企业版-Bug - 禅道</title><body></body></html>";
    let result = parse_search_result(html).expect("parse");
    assert!(result.bugs.is_empty());
}

#[test]
fn rejects_login_page() {
    let html = "<html><title>用户登录</title><body></body></html>";
    let err = parse_search_result(html).expect_err("must reject");
    assert!(err.to_string().contains("cookie"));
}
