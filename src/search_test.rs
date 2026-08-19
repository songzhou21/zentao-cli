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

#[test]
fn parses_resolved_by_html_cell() {
    let html = r#"
        <table id='bugList'>
          <tbody>
            <tr data-id='58676'>
              <td class='c-title'>t</td>
              <td class='c-status'><span>已解决</span></td>
              <td class='c-assignedTo'><span title='牛威龙'>牛威龙</span></td>
              <td class='c-resolvedBy'><span title='周松'>周松</span></td>
            </tr>
          </tbody>
        </table>
    "#;
    let result = parse_search_result(html).expect("parse");
    assert_eq!(result.bugs.len(), 1);
    assert_eq!(result.bugs[0].resolved_by, "周松");
    assert_eq!(result.bugs[0].assigned_to, "牛威龙");
}

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
    assert!(active.title.contains("会议优化5.1"));

    let resolved = result
        .bugs
        .iter()
        .find(|bug| bug.id == 58496)
        .expect("resolved bug");
    assert_eq!(resolved.status, "resolved");
    assert_eq!(resolved.assigned_to, "崔文波");
    assert_eq!(resolved.resolved_by, "周松");

    let closed = result
        .bugs
        .iter()
        .find(|bug| bug.id == 58671)
        .expect("closed bug");
    assert_eq!(closed.status, "closed");
    assert_eq!(closed.assigned_to, "Closed");
    assert_eq!(closed.resolved_by, "肖会中");

    assert!(result
        .total
        .as_deref()
        .is_some_and(|s| s.contains("本页共") && s.contains("64")));
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
