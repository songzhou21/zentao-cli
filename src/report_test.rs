use super::*;
use crate::search;
use serde_json::json;

fn sample_bug(
    id: u64,
    status: &str,
    title: &str,
    resolution: &str,
    assigned_to: &str,
) -> search::BugRow {
    search::BugRow {
        id,
        severity: String::new(),
        pri: String::new(),
        confirmed: String::new(),
        title: title.to_string(),
        status: status.to_string(),
        opened_by: String::new(),
        opened_date: String::new(),
        assigned_date: String::new(),
        assigned_to: assigned_to.to_string(),
        resolved_by: "周松".to_string(),
        resolved_date: String::new(),
        resolution: resolution.to_string(),
        deadline: String::new(),
    }
}

fn build_report(bugs: &[search::BugRow]) -> BugReport {
    build(
        bugs,
        "http://example.com/zentao",
        1000,
        "2026-08-25 16:00:00".to_string(),
        Some("2026-08-01".into()),
        Some("2026-08-31".into()),
        Some("zhousong".into()),
    )
}

#[test]
fn split_title_prefix_takes_first_brackets() {
    assert_eq!(
        split_title_prefix("【系统测试】/IOS端创建循环会议"),
        ("系统测试".into(), "IOS端创建循环会议".into())
    );
    assert_eq!(
        split_title_prefix("【会议优化5.1期】会议创建者移交房主"),
        ("会议优化5.1期".into(), "会议创建者移交房主".into())
    );
    assert_eq!(
        split_title_prefix("【会议优化5.1期兼容】新包主持人关闭举手"),
        ("会议优化5.1期兼容".into(), "新包主持人关闭举手".into())
    );
    assert_eq!(
        split_title_prefix("没有前缀的标题"),
        (UNGROUPED.into(), "没有前缀的标题".into())
    );
    assert_eq!(
        split_title_prefix("前缀在中间【系统测试】仍整句展示"),
        ("系统测试".into(), "前缀在中间【系统测试】仍整句展示".into())
    );
    assert_eq!(
        split_title_prefix("【】空括号"),
        (UNGROUPED.into(), "【】空括号".into())
    );
    assert_eq!(
        split_title_prefix("【系统测试】"),
        ("系统测试".into(), "【系统测试】".into())
    );
}

#[test]
fn buckets_follow_state_only() {
    let report = build_report(&[
        sample_bug(1, "resolved", "【系统测试】待验证", "fixed", "徐晓庆"),
        sample_bug(2, "resolved", "【系统测试】不修", "willnotfix", "徐晓庆"),
        sample_bug(3, "closed", "【系统测试】关闭", "fixed", "Closed"),
        sample_bug(4, "closed", "【系统测试】外部", "external", "Closed"),
        sample_bug(5, "active", "【系统测试】激活", "", "周松"),
    ]);
    assert_eq!(report.summary.resolved, 2);
    assert_eq!(report.summary.closed, 2);
    assert_eq!(report.summary.other, 1);
    assert_eq!(report.summary.total, 5);
    let bugs = &report.groups[0].bugs;
    assert_eq!(bugs[0].bucket, Bucket::Resolved);
    assert_eq!(bugs[1].bucket, Bucket::Resolved);
    assert_eq!(bugs[2].bucket, Bucket::Closed);
    assert_eq!(bugs[3].bucket, Bucket::Closed);
    assert_eq!(bugs[4].bucket, Bucket::Other);
}

#[test]
fn groups_sort_by_count_then_name_with_ungrouped_last() {
    let report = build_report(&[
        sample_bug(1, "closed", "无前缀 A", "fixed", "Closed"),
        sample_bug(2, "closed", "【会议优化5.1期】A", "fixed", "Closed"),
        sample_bug(3, "closed", "【会议优化5.1期】B", "fixed", "Closed"),
        sample_bug(4, "closed", "【线上问题】A", "fixed", "Closed"),
        sample_bug(5, "closed", "【线上问题】B", "fixed", "Closed"),
        sample_bug(6, "closed", "【系统测试】A", "fixed", "Closed"),
    ]);
    let names: Vec<&str> = report.groups.iter().map(|g| g.name.as_str()).collect();
    assert_eq!(
        names,
        vec!["会议优化5.1期", "线上问题", "系统测试", UNGROUPED]
    );
}

#[test]
fn markdown_uses_json_name_as_heading() {
    let report = build_report(&[
        sample_bug(
            59066,
            "resolved",
            "【系统测试】/IOS端创建循环会议",
            "willnotfix",
            "徐晓庆",
        ),
        sample_bug(
            59063,
            "closed",
            "【会议优化5.1期】会议创建者移交房主成功后",
            "fixed",
            "Closed",
        ),
    ]);
    let json = render_json(&report, "").expect("json");
    let markdown = render_markdown(&json);
    assert!(markdown.starts_with("# zhousong 解决 Bug（2026-08-01 ~ 2026-08-31）\n"));
    assert!(markdown.contains("合计 2：已解决 1 · 已关闭 1 · 其他 0\n"));
    assert!(markdown.contains("【系统测试】(1)\n"));
    assert!(markdown.contains("- #59066 IOS端创建循环会议\n"));
    assert!(!markdown.contains("→"));
    assert!(markdown.contains("【会议优化5.1期】(1)\n"));
    assert!(markdown.contains("- #59063 会议创建者移交房主成功后\n"));
    assert!(!markdown.contains("## "));
    assert!(!markdown.contains("【系统测试】/IOS"));

    assert_eq!(json["groupBy"], "titlePrefix");
    assert_eq!(json["resolvedBy"], "zhousong");
    assert_eq!(json["summary"]["resolved"], 1);
    assert_eq!(json["summary"]["closed"], 1);
    assert_eq!(json["summary"]["other"], 0);
    assert_eq!(json["summary"]["total"], 2);
    let names: Vec<&str> = json["groups"]
        .as_array()
        .unwrap()
        .iter()
        .map(|g| g["name"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"系统测试"));
    assert!(!names.iter().any(|name| name.contains('【')));
    let system = json["groups"]
        .as_array()
        .unwrap()
        .iter()
        .find(|g| g["name"] == "系统测试")
        .unwrap();
    assert_eq!(system["count"], 1);
    assert_eq!(system["bugs"][0]["id"], 59066);
    assert_eq!(system["bugs"][0]["title"], "【系统测试】/IOS端创建循环会议");
    assert_eq!(system["bugs"][0]["displayTitle"], "IOS端创建循环会议");
    assert_eq!(system["bugs"][0]["bucket"], "resolved");
    assert_eq!(system["bugs"][0]["assignee"], "徐晓庆");
    assert_eq!(
        system["bugs"][0]["url"],
        "http://example.com/zentao/bug-view-59066.html"
    );
}

#[test]
fn markdown_is_projected_from_json() {
    let report = build_report(&[sample_bug(
        59066,
        "resolved",
        "【系统测试】/IOS端创建循环会议",
        "willnotfix",
        "徐晓庆",
    )]);
    let mut json = render_json(&report, "").expect("json");
    json["groups"][0]["name"] = json!("改名");
    json["groups"][0]["bugs"][0]["displayTitle"] = json!("CHANGED");
    let markdown = render_markdown(&json);
    assert!(markdown.contains("【改名】(1)\n"));
    assert!(!markdown.contains("## "));
    assert!(markdown.contains("- #59066 CHANGED\n"));
    assert!(!markdown.contains("→"));
    assert!(!markdown.contains("系统测试"));
    assert!(!markdown.contains("IOS端创建循环会议"));
}

#[test]
fn render_json_field_subset_keeps_group_name() {
    let report = build_report(&[sample_bug(
        1,
        "closed",
        "【线上问题】听不到",
        "fixed",
        "Closed",
    )]);
    let subset = render_json(&report, "id,title").expect("subset");
    assert_eq!(subset["groups"][0]["name"], "线上问题");
    assert_eq!(subset["groups"][0]["count"], 1);
    assert_eq!(
        subset["groups"][0]["bugs"][0],
        json!({
            "id": 1,
            "title": "【线上问题】听不到"
        })
    );
    assert_eq!(subset["summary"]["total"], 1);
    assert!(render_json(&report, "module").is_err());
}

#[test]
fn empty_report_is_markdown_heading_and_notice() {
    let report = build_report(&[]);
    let json = render_json(&report, "").expect("json");
    let markdown = render_markdown(&json);
    assert_eq!(
        markdown,
        "# zhousong 解决 Bug（2026-08-01 ~ 2026-08-31）\n\n没有找到 Bug\n"
    );
}

#[test]
fn groups_zhousong_month_browse_fixture() {
    // Captured via:
    // ZENTAO_DEBUG_JSON=tests/fixtures/search/browse_resolved_by_zhousong_month.json \
    //   zentao bug list --resolved-by zhousong --month -s all -L 1000
    let result = search::parse_browse_json(include_str!(
        "../tests/fixtures/search/browse_resolved_by_zhousong_month.json"
    ))
    .expect("fixture");
    let report = build(
        &result.bugs,
        "http://example.com/zentao",
        1000,
        "2026-08-25 16:00:00".to_string(),
        Some("2026-08-01".into()),
        Some("2026-08-31".into()),
        Some("zhousong".into()),
    );
    assert_eq!(report.sample_size, 26);
    assert!(!report.incomplete);
    assert_eq!(report.summary.resolved, 6);
    assert_eq!(report.summary.closed, 20);
    assert_eq!(report.summary.other, 0);
    assert_eq!(report.summary.total, 26);

    let names: Vec<(&str, u32)> = report
        .groups
        .iter()
        .map(|g| (g.name.as_str(), g.count()))
        .collect();
    assert_eq!(
        names,
        vec![
            ("会议优化5.1期", 8),
            ("会议优化5.1期兼容", 8),
            ("系统测试", 6),
            ("线上问题", 3),
            ("会议优化5.1", 1),
        ]
    );

    let json = render_json(&report, "").expect("json");
    let markdown = render_markdown(&json);
    assert!(markdown.contains("【会议优化5.1期】(8)\n"));
    assert!(markdown.contains("【系统测试】(6)\n"));
    assert!(!markdown.contains("## "));
    assert!(markdown.contains("- #59066 IOS端创建循环会议时，选择结束重复到最大日期，不同循环频率生成总场次均超出需求规定最大值。\n"));
    assert!(!markdown.contains("→"));

    assert_eq!(json["groups"][2]["name"], "系统测试");
    assert_eq!(json["groups"][2]["bugs"][0]["id"], 59066);
    assert_eq!(
        json["groups"][2]["bugs"][0]["displayTitle"],
        "IOS端创建循环会议时，选择结束重复到最大日期，不同循环频率生成总场次均超出需求规定最大值。"
    );
    assert_eq!(json["groups"][2]["bugs"][0]["bucket"], "resolved");
}
