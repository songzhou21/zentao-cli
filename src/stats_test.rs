use super::*;
use crate::search;
use serde_json::json;

fn parse_browse_fixture() -> search::SearchResult {
    search::parse_browse_json(include_str!(
        "../tests/fixtures/search/browse_bysearch_myqueryid.json"
    ))
    .expect("browse json fixture")
}

fn sample_bug_row(id: u64, status: &str, assigned_to: &str) -> search::BugRow {
    search::BugRow {
        id,
        severity: String::new(),
        pri: String::new(),
        confirmed: String::new(),
        title: format!("bug-{id}"),
        status: status.to_string(),
        opened_by: String::new(),
        opened_date: String::new(),
        assigned_to: assigned_to.to_string(),
        resolved_by: String::new(),
        resolved_date: String::new(),
        resolution: String::new(),
        deadline: String::new(),
    }
}

fn sample_resolved_bug(id: u64, status: &str, resolved_by: &str) -> search::BugRow {
    let mut bug = sample_bug_row(id, status, "");
    bug.resolved_by = resolved_by.to_string();
    bug
}

#[test]
fn aggregate_browse_json_fixture() {
    let result = parse_browse_fixture();
    let stats = aggregate(
        &result.bugs,
        1000,
        "2026-08-03 12:00:00".to_string(),
        None,
        None,
    );
    assert_eq!(stats.sample_size, 64);
    assert!(!stats.incomplete);

    let row = |name: &str| {
        stats
            .rows
            .iter()
            .find(|row| row.assignee == name)
            .unwrap_or_else(|| panic!("missing row {name}"))
    };

    // Captured via: ZENTAO_DEBUG_JSON=... zentao bug stats --title 会议优化5.1
    // 激活 / 待验证 by assignee; 已解决 / 关闭 / 合计 by resolvedBy.
    assert_eq!(row("肖明明").active, 2);
    assert_eq!(row("肖明明").solved, 3);
    assert_eq!(row("肖明明").closed, 12);
    assert_eq!(row("肖明明").total, 15);

    assert_eq!(row("周松").solved, 6);
    assert_eq!(row("周松").closed, 8);
    assert_eq!(row("周松").total, 14);

    assert_eq!(row("肖会中").solved, 3);
    assert_eq!(row("肖会中").closed, 10);
    assert_eq!(row("肖会中").total, 13);

    assert_eq!(row("陈婕").resolved, 7);
    assert_eq!(row("陈婕").total, 0);
    assert_eq!(row("牛威龙").resolved, 6);
    assert_eq!(row("吴昊").resolved, 3);
    assert_eq!(row("崔文波").resolved, 1);

    assert_eq!(stats.rows[0].assignee, "肖明明");
    assert_eq!(stats.total.active, 2);
    assert_eq!(stats.total.resolved, 17);
    assert_eq!(stats.total.solved, 17);
    assert_eq!(stats.total.closed, 45);
    assert_eq!(stats.total.total, 62);

    let table = render_table(&stats, false);
    assert!(table.contains(PERSON_HEADER));
    assert!(table.contains("已解决"));
    assert!(table.contains("肖明明"));
    assert!(table.contains("陈婕"));
}

#[test]
fn aggregate_marks_incomplete_when_sample_hits_limit() {
    let bugs = vec![
        sample_bug_row(1, "active", "alice"),
        sample_bug_row(2, "closed", "alice"),
    ];
    let stats = aggregate(&bugs, 2, "2026-08-03 12:00:00".to_string(), None, None);
    assert!(stats.incomplete);
    assert_eq!(stats.sample_size, 2);
    assert_eq!(stats.limit, 2);
}

#[test]
fn render_json_shape_and_field_subset() {
    let mut closed = sample_bug_row(2, "closed", "Closed");
    closed.resolved_by = "alice".to_string();
    let stats = aggregate(
        &[sample_bug_row(1, "active", "alice"), closed],
        10,
        "2026-08-03 12:00:00".to_string(),
        Some("2026-08-03".into()),
        Some("2026-08-09".into()),
    );
    let full = render_json(&stats, "").expect("json");
    assert_eq!(full["groupBy"], "assignee");
    assert_eq!(full["sampleSize"], 2);
    assert_eq!(full["limit"], 10);
    assert_eq!(full["incomplete"], false);
    assert_eq!(full["fetchedAt"], "2026-08-03 12:00:00");
    assert_eq!(full["resolvedFrom"], "2026-08-03");
    assert_eq!(full["resolvedTo"], "2026-08-09");
    assert!(full.get("teamOpen").is_none());
    assert_eq!(full["rows"].as_array().map(|rows| rows.len()), Some(1));
    assert_eq!(full["rows"][0]["assignee"], "alice");
    assert_eq!(full["rows"][0]["active"], 1);
    assert_eq!(full["rows"][0]["resolved"], 0);
    assert_eq!(full["rows"][0]["solved"], 0);
    assert_eq!(full["rows"][0]["closed"], 1);
    assert_eq!(full["rows"][0]["total"], 1);
    assert!(full["rows"][0].get("openShare").is_none());
    assert!(full["total"].get("assignee").is_none());
    assert_eq!(full["total"]["active"], 1);
    assert_eq!(full["total"]["solved"], 0);
    assert_eq!(full["total"]["closed"], 1);
    assert_eq!(full["total"]["total"], 1);

    let subset = render_json(&stats, "assignee,active").expect("subset");
    assert_eq!(
        subset["rows"][0],
        json!({
            "assignee": "alice",
            "active": 1
        })
    );
    assert_eq!(subset["total"], json!({ "active": 1 }));
    assert_eq!(subset["fetchedAt"], "2026-08-03 12:00:00");
}

#[test]
fn aggregate_total_is_bugs_written_by_resolver() {
    let bugs = vec![
        sample_resolved_bug(1, "closed", "周松"),
        sample_resolved_bug(2, "closed", "周松"),
        sample_resolved_bug(3, "resolved", "周松"),
        sample_resolved_bug(4, "closed", "肖明明"),
        sample_resolved_bug(5, "active", ""),
        sample_resolved_bug(6, "resolved", "肖明明"),
    ];
    let stats = aggregate(&bugs, 100, "2026-08-03 12:00:00".to_string(), None, None);
    assert_eq!(stats.rows.len(), 3);
    assert_eq!(stats.rows[0].assignee, "周松");
    assert_eq!(stats.rows[0].closed, 2);
    assert_eq!(stats.rows[0].solved, 1);
    assert_eq!(stats.rows[0].resolved, 0);
    assert_eq!(stats.rows[0].active, 0);
    assert_eq!(stats.rows[0].total, 3);
    assert_eq!(stats.rows[1].assignee, "肖明明");
    assert_eq!(stats.rows[1].closed, 1);
    assert_eq!(stats.rows[1].solved, 1);
    assert_eq!(stats.rows[1].total, 2);
    assert_eq!(stats.rows[2].assignee, UNASSIGNED);
    assert_eq!(stats.rows[2].active, 1);
    assert_eq!(stats.rows[2].total, 0);
    assert_eq!(stats.total.closed, 3);
    assert_eq!(stats.total.solved, 2);
    assert_eq!(stats.total.total, 5);

    let table = render_table(&stats, false);
    assert!(table.contains(PERSON_HEADER));
    assert!(table.contains("已解决"));
    assert!(!table.contains("指派给"));
    assert!(table.contains("周松"));

    let full = render_json(&stats, "").expect("json");
    assert_eq!(full["groupBy"], "assignee");
    assert_eq!(full["rows"][0]["assignee"], "周松");
    assert_eq!(full["rows"][0]["solved"], 1);
    assert_eq!(full["rows"][0]["closed"], 2);
    assert!(full["total"].get("assignee").is_none());
    assert_eq!(full["total"]["total"], 5);

    let subset = render_json(&stats, "assignee,solved,total").expect("subset");
    assert_eq!(
        subset["rows"][0],
        json!({
            "assignee": "周松",
            "solved": 1,
            "total": 3
        })
    );
    assert!(render_json(&stats, "resolvedBy").is_err());
}

#[test]
fn render_table_has_no_duplicate_incomplete_footer() {
    let mut closed = sample_bug_row(2, "closed", "bob");
    closed.resolved_by = "bob".to_string();
    let stats = aggregate(
        &[sample_bug_row(1, "active", ""), closed],
        2,
        "2026-08-03 12:00:00".to_string(),
        Some("2026-08-03".into()),
        Some("2026-08-09".into()),
    );
    let table = render_table(&stats, false);
    assert!(table.contains(PERSON_HEADER));
    assert!(table.contains("待验证"));
    assert!(table.contains("已解决"));
    assert!(table.contains("合计"));
    assert!(!table.contains("未关占比"));
    assert!(!table.contains('%'));
    assert!(table.contains(UNASSIGNED));
    assert!(table.contains("bob"));
    assert!(table.contains(TOTAL_LABEL));
    assert!(table.contains("\n解决日期: 2026-08-03 ~ 2026-08-09\n"));
    assert!(table.contains("\n更新时间: 2026-08-03 12:00:00\n"));
    let lines: Vec<&str> = table.lines().collect();
    assert_eq!(*lines.last().unwrap(), "更新时间: 2026-08-03 12:00:00");
    assert!(lines.contains(&"解决日期: 2026-08-03 ~ 2026-08-09"));
    assert!(!table.contains('\x1b'), "plain/unstyled table has no ANSI");
    assert_eq!(
        format_resolved_date_range_line(Some("2026-08-03 00:00:00"), Some("2026-08-09 23:59:59"))
            .as_deref(),
        Some("解决日期: 2026-08-03 ~ 2026-08-09")
    );
    assert!(format_resolved_date_range_line(None, None).is_none());
    assert!(!table.contains("sample:"));
    assert!(!table.contains("incomplete"));
    assert_eq!(
        incomplete_warning(&stats),
        "warning: 样本已达 limit=2（聚合 2 条），可能不全；请提高 -L 或收窄筛选"
    );

    let colored = render_table(&stats, true);
    assert!(colored.contains("\x1b[36m"), "person names use cyan");
    assert!(colored.contains("\x1b[37m"), "counts use normal white");
    assert!(colored.contains("\x1b[1;36m"), "TOTAL name is bold cyan");
    assert!(
        colored.contains("\x1b[1;37m"),
        "TOTAL counts are bold white"
    );
    assert!(
        colored.contains("\x1b[34m"),
        "resolved date meta uses muted blue"
    );
    assert!(
        colored.contains("\x1b[90m"),
        "system rows (unassigned/closed) still dim"
    );
    assert!(
        colored.contains("\x1b[37m更新时间:"),
        "fetched-at uses readable white, not dim gray"
    );
    assert!(!colored.contains("\x1b[33m"), "avoid flashy yellow counts");
}
