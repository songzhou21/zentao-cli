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

#[test]
fn parse_search_result_deadline_from_title_attr() {
    let html = r#"
<!DOCTYPE html>
<html><head><title>Bug - 禅道</title></head><body>
  <form id='bugForm'>
    <table class='datatable'>
      <tr data-id='1'>
        <td class='c-id'><a href='/zentao/bug-view-1.html'>1</a></td>
        <td class='c-severity'><span data-severity='3'></span></td>
        <td class='c-pri'><span>3</span></td>
        <td class='c-confirmed'><span>否</span></td>
        <td class='c-title'><a href='/zentao/bug-view-1.html'>A</a></td>
        <td class='c-status'><span>激活</span></td>
        <td class='c-openedBy'>石秀秀</td>
        <td class='c-openedDate'>02-24 15:43</td>
        <td class='c-assignedTo'><span title='周松'>周松</span></td>
        <td class='c-resolution'></td>
        <td class='c-deadline'><span title='2026-03-10'>03-10</span></td>
      </tr>
    </table>
  </form>
  <div class='table-statistic'>本页共 <strong>1</strong> 个Bug，未解决 <strong>1</strong>。</div>
</body></html>
"#;

    let result = parse_search_result(html).expect("parse should succeed");
    assert_eq!(result.bugs.len(), 1);
    assert_eq!(result.bugs[0].deadline, "2026-03-10");
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
    assert_eq!(
        bugs[0].get("opened_by").and_then(|x| x.as_str()),
        Some("用户甲")
    );
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

    assert!(text.contains("搜索到 2 个 Bug，未解决 2 个。"));
    assert!(text.contains("1. [51276] 【系统测试】添加子社群"));
    assert!(text.contains("2. [48919] 【系统测试】PC登录后"));
    assert!(text.contains("\x1b[38;5;244m级别：3 ｜ 创建者：用户甲 02-24 15:43 ｜ 指派：用户乙 ｜ 截止日期：-- ｜ 解决日期：--\x1b[0m"));
    assert!(text.contains(
        "\x1b[38;5;244m级别：3 ｜ 创建者：用户甲 12-11 11:25 ｜ 指派：用户乙 ｜ 截止日期："
    ));
    assert!(text.contains("2025-12-16（"));
    assert!(text.contains(" ｜ 解决日期：--\x1b[0m"));
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
    assert!(text.contains("搜索到 1 个 Bug，未解决 1 个。"));
    assert!(text.contains("截止日期：-- ｜ 解决日期：--"));
}

#[test]
fn render_search_lines_hide_resolved_date_for_assigned_to() {
    let html = read_fixture("search_assigned_to_zhousong.html");
    let result = parse_search_result(&html).expect("parse should succeed");
    let json = render_search_json(&result).expect("json should render");
    let text = render_search_lines_from_json(&json, true).expect("lines should render");

    assert!(text.contains("搜索到 2 个 Bug，未解决 2 个。"));
    assert!(text.contains("截止日期：--"));
    assert!(!text.contains("解决日期："));
}

#[test]
fn render_search_lines_summary_fallback_to_bug_rows() {
    let json = r#"{
  "bugs": [
    {
      "id": 1,
      "severity": "2",
      "pri": "2",
      "confirmed": "否",
      "title": "t1",
      "status": "激活",
      "opened_by": "a",
      "opened_date": "02-28 18:32",
      "assigned_to": "b",
      "resolved_date": "",
      "resolution": "",
      "deadline": "0000-00-00"
    },
    {
      "id": 2,
      "severity": "2",
      "pri": "2",
      "confirmed": "否",
      "title": "t2",
      "status": "关闭",
      "opened_by": "a",
      "opened_date": "03-01 09:00",
      "assigned_to": "b",
      "resolved_date": "2026-03-01",
      "resolution": "已解决",
      "deadline": "2026-03-02"
    }
  ],
  "total": "统计信息缺失"
}"#;

    let text = render_search_lines_from_json(json, false).expect("lines should render");
    assert!(text.contains("搜索到 2 个 Bug，未解决 1 个。"));
}

#[test]
fn render_search_lines_empty_with_summary() {
    let json = r#"{
  "bugs": [],
  "total": null
}"#;
    let text = render_search_lines_from_json(json, false).expect("lines should render");
    assert_eq!(text, "搜索到 0 个 Bug，未解决 0 个。\n搜索结果为空。\n");
}

#[test]
fn render_search_lines_resolved_title_is_gray() {
    let json = r#"{
  "bugs": [
    {
      "id": 1,
      "severity": "3",
      "pri": "3",
      "confirmed": "否",
      "title": "未解决标题",
      "status": "激活",
      "opened_by": "a",
      "opened_date": "03-01 10:00",
      "assigned_to": "b",
      "resolved_date": "",
      "resolution": "",
      "deadline": "0000-00-00"
    },
    {
      "id": 2,
      "severity": "3",
      "pri": "3",
      "confirmed": "否",
      "title": "已解决标题",
      "status": "已解决",
      "opened_by": "a",
      "opened_date": "03-01 10:01",
      "assigned_to": "b",
      "resolved_date": "2026-03-01",
      "resolution": "已修复",
      "deadline": "2026-03-10"
    }
  ],
  "total": "本页共 2 个Bug，未解决 1。"
}"#;
    let text = render_search_lines_from_json(json, false).expect("lines should render");
    assert!(text.contains("1. [1] 未解决标题"));
    assert!(text.contains("\x1b[38;5;247m2. [2] 已解决标题\x1b[0m"));
    assert!(text.contains("截止日期：2026-03-10 ｜ 解决日期：2026-03-01"));
    assert!(!text.contains("2026-03-10（"));
    assert!(!text.contains("\x1b[1;31m2026-03-10"));
}

#[test]
fn render_grouped_search_json_by_test_module() {
    let json = r#"{
  "bugs": [
    {
      "id": 1,
      "severity": "2",
      "pri": "2",
      "confirmed": "否",
      "title": "【IM数据库改造】A",
      "status": "激活",
      "opened_by": "a",
      "opened_date": "03-01 10:00",
      "assigned_to": "张三",
      "resolved_date": "",
      "resolution": "",
      "deadline": "0000-00-00"
    },
    {
      "id": 2,
      "severity": "2",
      "pri": "2",
      "confirmed": "否",
      "title": "【1-1通话】B",
      "status": "激活",
      "opened_by": "a",
      "opened_date": "03-01 10:01",
      "assigned_to": "李四",
      "resolved_date": "",
      "resolution": "",
      "deadline": "0000-00-00"
    },
    {
      "id": 3,
      "severity": "2",
      "pri": "2",
      "confirmed": "否",
      "title": "【IM数据库改造】C",
      "status": "已解决",
      "opened_by": "a",
      "opened_date": "03-01 10:02",
      "assigned_to": "王五",
      "resolved_date": "2026-03-01",
      "resolution": "已修复",
      "deadline": "2026-03-10"
    }
  ],
  "total": "本页共 3 个Bug，未解决 2。"
}"#;
    let result: SearchResult = serde_json::from_str(json).expect("json should parse");
    let grouped_json =
        render_grouped_search_json(&result, GroupBy::TestModule).expect("should render");
    let v: serde_json::Value = serde_json::from_str(&grouped_json).expect("json should parse");

    assert_eq!(v.get("group_by").and_then(|x| x.as_str()), Some("module"));
    assert_eq!(
        v.get("summary")
            .and_then(|x| x.get("total"))
            .and_then(|x| x.as_u64()),
        Some(3)
    );
    assert_eq!(
        v.get("summary")
            .and_then(|x| x.get("unresolved"))
            .and_then(|x| x.as_u64()),
        Some(2)
    );

    let groups = v
        .get("groups")
        .and_then(|x| x.as_array())
        .expect("groups should be array");
    assert_eq!(groups.len(), 2);
    assert_eq!(
        groups[0].get("name").and_then(|x| x.as_str()),
        Some("【IM数据库改造】")
    );
    assert_eq!(groups[0].get("total").and_then(|x| x.as_u64()), Some(2));
    assert_eq!(
        groups[0].get("unresolved").and_then(|x| x.as_u64()),
        Some(1)
    );
    assert_eq!(
        groups[1].get("name").and_then(|x| x.as_str()),
        Some("【1-1通话】")
    );
    let g0_bugs = groups[0]
        .get("bugs")
        .and_then(|x| x.as_array())
        .expect("bugs should be array");
    assert_eq!(g0_bugs[0].get("id").and_then(|x| x.as_u64()), Some(3));
    assert_eq!(g0_bugs[1].get("id").and_then(|x| x.as_u64()), Some(1));
}

#[test]
fn render_grouped_search_json_by_assigned_to() {
    let json = r#"{
  "bugs": [
    {
      "id": 1,
      "severity": "2",
      "pri": "2",
      "confirmed": "否",
      "title": "A",
      "status": "激活",
      "opened_by": "a",
      "opened_date": "03-01 10:00",
      "assigned_to": "张三",
      "resolved_date": "",
      "resolution": "",
      "deadline": "0000-00-00"
    },
    {
      "id": 2,
      "severity": "2",
      "pri": "2",
      "confirmed": "否",
      "title": "B",
      "status": "激活",
      "opened_by": "a",
      "opened_date": "03-01 10:01",
      "assigned_to": "",
      "resolved_date": "",
      "resolution": "",
      "deadline": "0000-00-00"
    }
  ],
  "total": "本页共 2 个Bug，未解决 2。"
}"#;
    let result: SearchResult = serde_json::from_str(json).expect("json should parse");
    let grouped_json =
        render_grouped_search_json(&result, GroupBy::AssignedTo).expect("should render");
    let v: serde_json::Value = serde_json::from_str(&grouped_json).expect("json should parse");

    assert_eq!(
        v.get("group_by").and_then(|x| x.as_str()),
        Some("assigned-to")
    );
    let groups = v
        .get("groups")
        .and_then(|x| x.as_array())
        .expect("groups should be array");
    assert_eq!(groups.len(), 2);
    assert_eq!(
        groups[0].get("name").and_then(|x| x.as_str()),
        Some("未指派")
    );
    assert_eq!(groups[1].get("name").and_then(|x| x.as_str()), Some("张三"));
}

#[test]
fn render_grouped_search_lines_from_json_output() {
    let json = r#"{
  "group_by": "module",
  "total": "本页共 2 个Bug，未解决 2。",
  "summary": {"total": 2, "unresolved": 2},
  "groups": [
    {
      "name": "【IM数据库改造】",
      "total": 1,
      "unresolved": 1,
      "bugs": [
        {
          "id": 1,
          "severity": "2",
          "pri": "2",
          "confirmed": "否",
          "title": "【IM数据库改造】A",
          "status": "激活",
          "opened_by": "a",
          "opened_date": "03-01 10:00",
          "assigned_to": "张三",
          "resolved_date": "",
          "resolution": "",
          "deadline": "0000-00-00"
        }
      ]
    },
    {
      "name": "【1-1通话】",
      "total": 1,
      "unresolved": 1,
      "bugs": [
        {
          "id": 2,
          "severity": "2",
          "pri": "2",
          "confirmed": "否",
          "title": "【1-1通话】B",
          "status": "激活",
          "opened_by": "a",
          "opened_date": "03-01 10:01",
          "assigned_to": "李四",
          "resolved_date": "",
          "resolution": "",
          "deadline": "0000-00-00"
        }
      ]
    }
  ]
}"#;
    let text = render_grouped_search_lines_from_json(json, false).expect("should render");
    assert!(text.contains("搜索到 2 个 Bug，未解决 2 个（按 module 分组）。"));
    assert!(text.contains("1. 【1-1通话】（1 个，未解决 1 个）"));
    assert!(text.contains("2. 【IM数据库改造】（1 个，未解决 1 个）"));
    assert!(text.contains("  [1] 【IM数据库改造】A"));
    assert!(text.contains("  [2] 【1-1通话】B"));
}

#[test]
fn render_grouped_search_lines_sort_groups_by_latest_opened_date_desc() {
    let json = r#"{
  "group_by": "module",
  "total": "本页共 3 个Bug，未解决 3。",
  "summary": {"total": 3, "unresolved": 3},
  "groups": [
    {
      "name": "【IM数据库改造】",
      "total": 2,
      "unresolved": 2,
      "bugs": [
        {
          "id": 1,
          "severity": "2",
          "pri": "2",
          "confirmed": "否",
          "title": "【IM数据库改造】A",
          "status": "激活",
          "opened_by": "a",
          "opened_date": "03-01 10:00",
          "assigned_to": "张三",
          "resolved_date": "",
          "resolution": "",
          "deadline": "0000-00-00"
        },
        {
          "id": 2,
          "severity": "2",
          "pri": "2",
          "confirmed": "否",
          "title": "【IM数据库改造】B",
          "status": "激活",
          "opened_by": "a",
          "opened_date": "03-01 09:00",
          "assigned_to": "张三",
          "resolved_date": "",
          "resolution": "",
          "deadline": "0000-00-00"
        }
      ]
    },
    {
      "name": "【1-1通话】",
      "total": 1,
      "unresolved": 1,
      "bugs": [
        {
          "id": 3,
          "severity": "2",
          "pri": "2",
          "confirmed": "否",
          "title": "【1-1通话】C",
          "status": "激活",
          "opened_by": "a",
          "opened_date": "03-01 11:00",
          "assigned_to": "李四",
          "resolved_date": "",
          "resolution": "",
          "deadline": "0000-00-00"
        }
      ]
    }
  ]
}"#;
    let text = render_grouped_search_lines_from_json(json, false).expect("should render");
    let idx_1v1 = text
        .find("1. 【1-1通话】")
        .expect("should contain 1-1 group");
    let idx_im = text
        .find("2. 【IM数据库改造】")
        .expect("should contain im group");
    assert!(idx_1v1 < idx_im);
}

#[test]
fn render_grouped_search_lines_sort_bugs_by_opened_date_desc_in_group() {
    let json = r#"{
  "group_by": "module",
  "total": "本页共 2 个Bug，未解决 2。",
  "summary": {"total": 2, "unresolved": 2},
  "groups": [
    {
      "name": "【IM数据库改造】",
      "total": 2,
      "unresolved": 2,
      "bugs": [
        {
          "id": 20,
          "severity": "2",
          "pri": "2",
          "confirmed": "否",
          "title": "【IM数据库改造】B",
          "status": "激活",
          "opened_by": "a",
          "opened_date": "03-01 09:00",
          "assigned_to": "张三",
          "resolved_date": "",
          "resolution": "",
          "deadline": "0000-00-00"
        },
        {
          "id": 10,
          "severity": "2",
          "pri": "2",
          "confirmed": "否",
          "title": "【IM数据库改造】A",
          "status": "激活",
          "opened_by": "a",
          "opened_date": "03-01 11:00",
          "assigned_to": "张三",
          "resolved_date": "",
          "resolution": "",
          "deadline": "0000-00-00"
        }
      ]
    }
  ]
}"#;
    let text = render_grouped_search_lines_from_json(json, false).expect("should render");
    let idx_10 = text.find("  [10] ").expect("should contain id 10 first");
    let idx_20 = text.find("  [20] ").expect("should contain id 20 second");
    assert!(idx_10 < idx_20);
}

#[test]
fn parse_opened_date_rank_rolls_back_future_mmdd_to_previous_year() {
    let now = chrono::NaiveDate::from_ymd_opt(2026, 3, 2)
        .unwrap()
        .and_hms_opt(12, 0, 0)
        .unwrap();
    let dec = parse_opened_date_rank_with_now("12-15 17:48", now).expect("should parse");
    let feb = parse_opened_date_rank_with_now("02-28 13:47", now).expect("should parse");
    assert!(feb > dec);
}

#[test]
fn render_grouped_search_lines_resolved_bug_keeps_deadline_delta_text() {
    let json = r#"{
  "group_by": "module",
  "total": "本页共 1 个Bug，未解决 0。",
  "summary": {"total": 1, "unresolved": 0},
  "groups": [
    {
      "name": "【系统测试】",
      "total": 1,
      "unresolved": 0,
      "bugs": [
        {
          "id": 1,
          "severity": "2",
          "pri": "2",
          "confirmed": "否",
          "title": "【系统测试】已解决样例",
          "status": "已解决",
          "opened_by": "a",
          "opened_date": "2026-03-01 10:00",
          "assigned_to": "张三",
          "resolved_date": "2026-03-01",
          "resolution": "已修复",
          "deadline": "2099-12-31"
        }
      ]
    }
  ]
}"#;
    let text = render_grouped_search_lines_from_json(json, false).expect("should render");
    assert!(text.contains("截止日期：2099-12-31（剩余"));
    assert!(!text.contains("\x1b[1;31m2099-12-31"));
}

#[test]
fn format_deadline_for_display_overdue_and_highlight_flag() {
    let today = chrono::NaiveDate::from_ymd_opt(2026, 3, 1).expect("valid date");
    let (text, overdue) = format_deadline_for_display("2026-02-26", today);
    assert_eq!(text, "2026-02-26（已过3天）");
    assert!(overdue);
}

#[test]
fn format_deadline_for_display_future_and_today() {
    let today = chrono::NaiveDate::from_ymd_opt(2026, 3, 1).expect("valid date");
    let (future_text, future_overdue) = format_deadline_for_display("2026-03-04", today);
    assert_eq!(future_text, "2026-03-04（剩余3天）");
    assert!(future_overdue);

    let (today_text, today_overdue) = format_deadline_for_display("2026-03-01", today);
    assert_eq!(today_text, "2026-03-01（今天）");
    assert!(today_overdue);
}

#[test]
fn format_deadline_for_display_highlight_within_7_days_inclusive() {
    let today = chrono::NaiveDate::from_ymd_opt(2026, 3, 1).expect("valid date");
    let (d7_text, d7_highlight) = format_deadline_for_display("2026-03-08", today);
    assert_eq!(d7_text, "2026-03-08（剩余7天）");
    assert!(d7_highlight);

    let (d8_text, d8_highlight) = format_deadline_for_display("2026-03-09", today);
    assert_eq!(d8_text, "2026-03-09（剩余8天）");
    assert!(!d8_highlight);
}
