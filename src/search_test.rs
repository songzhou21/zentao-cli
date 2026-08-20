use super::*;

#[test]
fn parses_browse_json_fixture() {
    let body = include_str!("../tests/fixtures/search/browse_bysearch_myqueryid.json");
    let result = parse_browse_json(body).expect("parse");
    assert_eq!(result.bugs.len(), 64);

    let active = result
        .bugs
        .iter()
        .find(|bug| bug.id == 58679)
        .expect("active bug");
    assert_eq!(active.status, "active");
    assert_eq!(active.assigned_to, "肖明明");
    assert_eq!(active.resolved_by, "");
    assert_eq!(active.confirmed, "1");
    assert_eq!(active.opened_date, "2026-08-19 10:56:50");
    assert_eq!(active.resolution, "");
    assert!(active.title.contains("会议优化5.1"));

    let resolved = result
        .bugs
        .iter()
        .find(|bug| bug.id == 58496)
        .expect("resolved bug");
    assert_eq!(resolved.status, "resolved");
    assert_eq!(resolved.assigned_to, "崔文波");
    assert_eq!(resolved.resolved_by, "周松");
    assert_eq!(resolved.opened_date, "2026-08-14 15:31:29");
    assert_eq!(resolved.resolved_date, "2026-08-18 10:12:35");
    assert_eq!(resolved.resolution, "fixed");

    let closed = result
        .bugs
        .iter()
        .find(|bug| bug.id == 58671)
        .expect("closed bug");
    assert_eq!(closed.status, "closed");
    assert_eq!(closed.assigned_to, "Closed");
    assert_eq!(closed.resolved_by, "肖会中");

    assert_eq!(result.total.as_deref(), Some("本页共 64 个Bug，未解决 2。"));
}

#[test]
fn parses_assigned_browse_json_fixture() {
    let body = include_str!("../tests/fixtures/search/browse_assigned_to_zhousong.json");
    let result = parse_browse_json(body).expect("parse");
    assert_eq!(result.bugs.len(), 5);
    assert_eq!(result.bugs[0].id, 57659);
    assert_eq!(result.bugs[0].assigned_to, "Closed");
    assert_eq!(result.bugs[0].resolved_by, "周松");
    assert_eq!(result.bugs[0].opened_by, "陈婕");
    assert_eq!(result.bugs[0].opened_date, "2026-07-28 14:56:05");
    assert_eq!(result.bugs[0].resolved_date, "2026-07-29 10:28:21");
    assert_eq!(result.bugs[0].resolution, "fixed");
    assert_eq!(result.bugs[0].confirmed, "1");
    assert_eq!(result.bugs[0].status, "closed");
    assert!(result.bugs.iter().all(|bug| bug.resolved_by == "周松"));
    assert!(result
        .bugs
        .iter()
        .all(|bug| bug.resolved_date.starts_with("2026-07-")));
    assert_eq!(result.total.as_deref(), Some("本页共 5 个Bug，未解决 0。"));
}

#[test]
fn parses_empty_browse_json_fixture() {
    let body = include_str!("../tests/fixtures/search/browse_empty.json");
    let result = parse_browse_json(body).expect("parse");
    assert!(result.bugs.is_empty());
    assert_eq!(result.total.as_deref(), Some("本页共 0 个Bug，未解决 0。"));
}

#[test]
fn browse_json_summary_strips_html_tags() {
    let payload = serde_json::json!({
        "status": "success",
        "data": {
            "bugs": [],
            "summary": "本页共 <strong>10</strong> 个Bug，未解决 <strong>10</strong>。"
        }
    });
    let result = parse_browse_json(&payload.to_string()).expect("parse");
    assert_eq!(
        result.total.as_deref(),
        Some("本页共 10 个Bug，未解决 10。")
    );
}

#[test]
fn parses_browse_json_object_data() {
    let payload = serde_json::json!({
        "status": "success",
        "data": {
            "bugs": [{"id": 1, "status": "active", "resolvedBy": "zhousong"}],
            "users": {"zhousong": "周松"}
        }
    });
    let result = parse_browse_json(&payload.to_string()).expect("parse");
    assert_eq!(result.bugs[0].resolved_by, "周松");
}

#[test]
fn browse_json_rejects_login_html() {
    let err = parse_browse_json("<html><title>用户登录</title></html>").expect_err("login");
    assert!(err.to_string().contains("cookie"));
}
