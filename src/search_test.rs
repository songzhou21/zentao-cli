use super::*;

fn read_fixture(name: &str) -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("search")
        .join(name);
    std::fs::read_to_string(path).expect("fixture should exist")
}

// 搜索结果页应能解析出 2 条 bug 记录。
#[test]
fn parse_search_result_two_bugs() {
    let html = read_fixture("search_assigned_to_zhousong.html");
    let result = parse_search_result(&html).expect("parse should succeed");

    assert_eq!(result.bugs.len(), 2);

    let first = &result.bugs[0];
    assert_eq!(first.id, 51276);
    assert_eq!(first.severity, "3");
    assert_eq!(first.pri, "3");
    assert_eq!(first.confirmed, "否");
    assert!(first.title.contains("添加子社群"));
    assert_eq!(first.status, "激活");
    assert_eq!(first.opened_by, "用户甲");
    assert_eq!(first.opened_date, "02-24 15:43");
    assert_eq!(first.assigned_to, "用户乙");
    assert!(first.resolved_date.is_empty());
    assert!(first.resolution.is_empty());
    assert_eq!(first.deadline, "0000-00-00");

    let second = &result.bugs[1];
    assert_eq!(second.id, 48919);
    assert!(second.title.contains("PC登录后"));
    assert_eq!(second.assigned_to, "用户乙");
    assert_eq!(second.deadline, "2025-12-16");
}

// 搜索统计信息应能解析。
#[test]
fn parse_search_result_statistics() {
    let html = read_fixture("search_assigned_to_zhousong.html");
    let result = parse_search_result(&html).expect("parse should succeed");

    let total = result.total.as_ref().expect("total should exist");
    assert!(total.contains("2"));
    assert!(total.contains("Bug"));
}

// 表格 id / tr[data-id] 变化时，应回退到 c-id 单元格解析。
#[test]
fn parse_search_result_with_fallback_selectors() {
    let html = r#"
<!DOCTYPE html>
<html><head><title>Bug - 禅道</title></head><body>
  <form id='bugForm'>
    <table class='datatable'>
      <tr>
        <td class='c-id'><a href='/zentao/bug-view-51276.html'>51276</a></td>
        <td class='c-severity'><span data-severity='3'></span></td>
        <td class='c-pri'><span>3</span></td>
        <td class='c-confirmed'><span>否</span></td>
        <td class='c-title'><a href='/zentao/bug-view-51276.html'>A</a></td>
        <td class='c-status'><span>激活</span></td>
        <td class='c-openedBy'>石秀秀</td>
        <td class='c-openedDate'>02-24 15:43</td>
        <td class='c-assignedTo'><span title='周松'>周松</span></td>
        <td class='c-resolution'></td>
        <td class='c-deadline'>0000-00-00</td>
      </tr>
      <tr>
        <td class='c-id'><a href='/zentao/bug-view-48919.html'>48919</a></td>
        <td class='c-severity'><span data-severity='3'></span></td>
        <td class='c-pri'><span>3</span></td>
        <td class='c-confirmed'><span>否</span></td>
        <td class='c-title'><a href='/zentao/bug-view-48919.html'>B</a></td>
        <td class='c-status'><span>激活</span></td>
        <td class='c-openedBy'>石秀秀</td>
        <td class='c-openedDate'>12-11 11:25</td>
        <td class='c-assignedTo'><span title='周松'>周松</span></td>
        <td class='c-resolution'></td>
        <td class='c-deadline'>2025-12-16</td>
      </tr>
    </table>
  </form>
  <div class='table-statistic'>本页共 <strong>2</strong> 个Bug，未解决 <strong>2</strong>。</div>
</body></html>
"#;

    let result = parse_search_result(html).expect("parse should succeed");
    assert_eq!(result.bugs.len(), 2);
    assert_eq!(result.bugs[0].id, 51276);
    assert_eq!(result.bugs[1].id, 48919);
}

// 空结果应返回空 bug 列表，不应报错。
#[test]
fn parse_search_result_empty() {
    let html = read_fixture("search_empty_result.html");
    let result = parse_search_result(&html).expect("parse should succeed");
    assert!(result.bugs.is_empty());
}

#[test]
fn render_search_json_full_fields() {
    let html = read_fixture("search_assigned_to_zhousong.html");
    let result = parse_search_result(&html).expect("parse should succeed");
    let json = render_search_json(&result).expect("json should render");

    let v: serde_json::Value = serde_json::from_str(&json).expect("json should parse");
    let bugs = v
        .get("bugs")
        .and_then(|x| x.as_array())
        .expect("bugs should be array");
    assert_eq!(bugs.len(), 2);

    let first = &bugs[0];
    assert!(first.get("id").is_some());
    assert!(first.get("severity").is_some());
    assert!(first.get("pri").is_some());
    assert!(first.get("confirmed").is_some());
    assert!(first.get("title").is_some());
    assert!(first.get("status").is_some());
    assert!(first.get("opened_by").is_some());
    assert!(first.get("opened_date").is_some());
    assert!(first.get("assigned_to").is_some());
    assert!(first.get("resolved_date").is_some());
    assert!(first.get("resolution").is_some());
    assert!(first.get("deadline").is_some());
}

#[test]
fn render_search_json_expected_content() {
    let html = read_fixture("search_assigned_to_zhousong.html");
    let result = parse_search_result(&html).expect("parse should succeed");
    let json = render_search_json(&result).expect("json should render");

    let v: serde_json::Value = serde_json::from_str(&json).expect("json should parse");
    assert_eq!(
        v.get("total").and_then(|x| x.as_str()),
        Some("本页共 2 个Bug，未解决 2。")
    );

    let bugs = v
        .get("bugs")
        .and_then(|x| x.as_array())
        .expect("bugs should be array");
    assert_eq!(bugs.len(), 2);

    assert_eq!(bugs[0].get("id").and_then(|x| x.as_u64()), Some(51276));
    assert_eq!(bugs[0].get("severity").and_then(|x| x.as_str()), Some("3"));
    assert_eq!(bugs[0].get("opened_by").and_then(|x| x.as_str()), Some("用户甲"));
    assert_eq!(
        bugs[0].get("assigned_to").and_then(|x| x.as_str()),
        Some("用户乙")
    );
    assert_eq!(
        bugs[0].get("resolved_date").and_then(|x| x.as_str()),
        Some("")
    );
}

#[test]
fn render_search_lines_from_json_output() {
    let html = read_fixture("search_assigned_to_zhousong.html");
    let result = parse_search_result(&html).expect("parse should succeed");
    let json = render_search_json(&result).expect("json should render");
    let text = render_search_lines_from_json(&json, false).expect("lines should render");

    assert!(text.contains("1. [51276] 【系统测试】添加子社群"));
    assert!(text.contains("2. [48919] 【系统测试】PC登录后"));
    assert!(text.contains("级别：3 ｜ 创建者：用户甲 02-24 15:43 ｜ 指派：用户乙 ｜ 截止日期：-- ｜ 解决日期：--"));
    assert!(text.contains("级别：3 ｜ 创建者：用户甲 12-11 11:25 ｜ 指派：用户乙 ｜ 截止日期：2025-12-16 ｜ 解决日期：--"));
}

#[test]
fn render_search_lines_zero_resolved_date_as_dash() {
    let json = r#"{
  "bugs": [
    {
      "id": 1,
      "severity": "2",
      "pri": "2",
      "confirmed": "否",
      "title": "t",
      "status": "激活",
      "opened_by": "a",
      "opened_date": "02-28 18:32",
      "assigned_to": "b",
      "resolved_date": "00-00 00:00",
      "resolution": "",
      "deadline": "0000-00-00"
    }
  ],
  "total": "本页共 1 个Bug，未解决 1。"
}"#;

    let text = render_search_lines_from_json(json, false).expect("lines should render");
    assert!(text.contains("截止日期：-- ｜ 解决日期：--"));
}

#[test]
fn render_search_lines_hide_resolved_date_for_assigned_to() {
    let html = read_fixture("search_assigned_to_zhousong.html");
    let result = parse_search_result(&html).expect("parse should succeed");
    let json = render_search_json(&result).expect("json should render");
    let text = render_search_lines_from_json(&json, true).expect("lines should render");

    assert!(text.contains("截止日期：--"));
    assert!(!text.contains("解决日期："));
}
