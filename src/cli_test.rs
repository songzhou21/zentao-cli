use super::*;
use chrono::{NaiveDate, Weekday};
use clap::Parser;
use reqwest::Url;
use serde_json::json;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};
use std::thread;

#[derive(Clone)]
struct ImageResponsePlan {
    path: &'static str,
    status: u16,
    location: Option<&'static str>,
    content_type: &'static str,
    body: &'static [u8],
}

fn spawn_image_server(
    plans: Vec<ImageResponsePlan>,
) -> (String, Arc<Mutex<Vec<String>>>, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind should succeed");
    let addr = listener.local_addr().expect("local addr should exist");
    let cookies = Arc::new(Mutex::new(Vec::new()));
    let cookies_bg = Arc::clone(&cookies);
    let handle = thread::spawn(move || {
        for _ in 0..plans.len() {
            let (mut stream, _) = listener.accept().expect("accept should succeed");
            let mut buf = [0_u8; 4096];
            let n = stream.read(&mut buf).expect("read should succeed");
            let request = String::from_utf8_lossy(&buf[..n]);
            if let Some(value) = request
                .lines()
                .find(|line| line.to_ascii_lowercase().starts_with("cookie:"))
                .and_then(|line| {
                    line.split_once(':')
                        .map(|(_, value)| value.trim().to_string())
                })
            {
                cookies_bg.lock().expect("lock should succeed").push(value);
            }
            let path = request
                .lines()
                .next()
                .and_then(|line| line.split_whitespace().nth(1))
                .unwrap_or("/");
            let plan = plans
                .iter()
                .find(|plan| plan.path == path)
                .expect("expected request path");
            let status_text = match plan.status {
                200 => "OK",
                302 => "Found",
                _ => "Internal Server Error",
            };
            let mut response = format!(
                "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n",
                plan.status,
                status_text,
                plan.content_type,
                plan.body.len()
            );
            if let Some(location) = plan.location {
                response.push_str(&format!("Location: {location}\r\n"));
            }
            response.push_str("\r\n");
            stream
                .write_all(response.as_bytes())
                .expect("write headers should succeed");
            stream
                .write_all(plan.body)
                .expect("write body should succeed");
        }
    });
    (format!("http://{addr}"), cookies, handle)
}

#[test]
fn global_options_and_bug_list_parse() {
    let cli = Cli::try_parse_from([
        "zentao",
        "--site",
        "http://example.com/zentao",
        "--config",
        "/tmp/zentao.json",
        "bug",
        "list",
        "--product",
        "92",
        "-a",
        "zhousong",
        "-s",
        "all",
        "-L",
        "50",
        "--title",
        "会议",
    ])
    .expect("should parse");

    assert_eq!(
        cli.global.site.as_deref(),
        Some("http://example.com/zentao")
    );
    assert_eq!(cli.global.config.as_deref(), Some("/tmp/zentao.json"));
    match cli.command {
        Commands::Bug(BugArgs {
            command: BugSubCommands::List(args),
        }) => {
            assert_eq!(args.product, Some(92));
            assert_eq!(args.assignee.as_deref(), Some("zhousong"));
            assert!(matches!(args.state, BugState::All));
            assert_eq!(args.limit, 50);
            assert_eq!(args.title, vec!["会议"]);
            assert!(!args.full_title);
            assert!(!args.plain);
        }
        _ => panic!("unexpected command"),
    }
}

#[test]
fn bug_list_parses_full_title_flag() {
    let cli = Cli::try_parse_from(["zentao", "bug", "list", "--full-title"]).expect("should parse");
    match cli.command {
        Commands::Bug(BugArgs {
            command: BugSubCommands::List(args),
        }) => assert!(args.full_title),
        _ => panic!("unexpected command"),
    }
}

#[test]
fn bug_list_parses_plain_flag() {
    let cli = Cli::try_parse_from(["zentao", "bug", "list", "--plain"]).expect("should parse");
    match cli.command {
        Commands::Bug(BugArgs {
            command: BugSubCommands::List(args),
        }) => assert!(args.plain),
        _ => panic!("unexpected command"),
    }
}

#[test]
fn bug_list_limit_must_be_positive() {
    assert!(Cli::try_parse_from(["zentao", "bug", "list", "--limit", "0"]).is_err());
}

#[test]
fn default_active_state_slot_limit_error_suggests_state_all() {
    let cli = Cli::try_parse_from([
        "zentao",
        "bug",
        "list",
        "--title",
        "A",
        "--title",
        "B",
        "--module",
        "1",
        "--assignee",
        "alice",
        "--resolved-by",
        "bob",
    ])
    .expect("should parse");
    let Commands::Bug(BugArgs {
        command: BugSubCommands::List(args),
    }) = cli.command
    else {
        panic!("unexpected command");
    };

    let error = validate_search_group_limits(&BugSearchQuery::from(&args))
        .expect_err("default active exceeds group limit");
    assert!(error.to_string().contains("--state all"));

    let cli = Cli::try_parse_from([
        "zentao",
        "bug",
        "list",
        "--title",
        "A",
        "--title",
        "B",
        "--module",
        "1",
        "--assignee",
        "alice",
        "--resolved-by",
        "bob",
        "--state",
        "all",
    ])
    .expect("should parse");
    let Commands::Bug(BugArgs {
        command: BugSubCommands::List(args),
    }) = cli.command
    else {
        panic!("unexpected command");
    };
    validate_search_group_limits(&BugSearchQuery::from(&args))
        .expect("state all releases the slot");
}

#[test]
fn bug_list_parses_opened_by_repeatable() {
    let cli = Cli::try_parse_from([
        "zentao",
        "bug",
        "list",
        "--opened-by",
        "chenjie",
        "--opened-by",
        "niuweilong",
        "--opened-by",
        "cuiwenbo",
        "-s",
        "active",
    ])
    .expect("should parse");
    match cli.command {
        Commands::Bug(BugArgs {
            command: BugSubCommands::List(args),
        }) => {
            assert_eq!(args.opened_by, vec!["chenjie", "niuweilong", "cuiwenbo"]);
            let query = BugSearchQuery::from(&args);
            validate_search_group_limits(&query).expect("3 opened-by + active fits");
            let params = build_search_field_params(&query);
            assert!(params
                .iter()
                .any(|(k, v)| k == "opened_by_or_1" && v == "chenjie"));
            assert!(params
                .iter()
                .any(|(k, v)| k == "opened_by_or_2" && v == "niuweilong"));
            assert!(params
                .iter()
                .any(|(k, v)| k == "opened_by_or_3" && v == "cuiwenbo"));
            assert!(params.iter().any(|(k, v)| k == "status" && v == "active"));
        }
        _ => panic!("unexpected command"),
    }
}

#[test]
fn opened_by_more_than_three_rejected() {
    let cli = Cli::try_parse_from([
        "zentao",
        "bug",
        "list",
        "--opened-by",
        "a",
        "--opened-by",
        "b",
        "--opened-by",
        "c",
        "--opened-by",
        "d",
        "--state",
        "all",
    ])
    .expect("should parse");
    let Commands::Bug(BugArgs {
        command: BugSubCommands::List(args),
    }) = cli.command
    else {
        panic!("unexpected command");
    };
    let error = validate_search_group_limits(&BugSearchQuery::from(&args))
        .expect_err("4 opened-by values exceed max");
    assert!(error.to_string().contains("--opened-by"));
}

#[test]
fn title_or_and_opened_by_or_reject_extra_filters() {
    let cli = Cli::try_parse_from([
        "zentao",
        "bug",
        "list",
        "--title",
        "A",
        "--title",
        "B",
        "--opened-by",
        "chenjie",
        "--opened-by",
        "niuweilong",
        "-a",
        "zhousong",
        "--state",
        "all",
    ])
    .expect("should parse");
    let Commands::Bug(BugArgs {
        command: BugSubCommands::List(args),
    }) = cli.command
    else {
        panic!("unexpected command");
    };
    let error = validate_search_group_limits(&BugSearchQuery::from(&args))
        .expect_err("both multi-OR plus assignee should fail");
    assert!(error.to_string().contains("不能再叠加"));
}

#[test]
fn bug_stats_defaults_to_state_all_and_limit_1000() {
    let cli = Cli::try_parse_from(["zentao", "bug", "stats"]).expect("should parse");
    match cli.command {
        Commands::Bug(BugArgs {
            command: BugSubCommands::Stats(args),
        }) => {
            assert!(matches!(args.state, BugState::All));
            assert_eq!(args.limit, 1000);
            assert!(!args.plain);
            assert!(args.json.is_none());
            assert!(!args.week);
            assert!(!args.month);
            assert!(!args.day);
        }
        _ => panic!("unexpected command"),
    }
}

#[test]
fn reporting_week_bounds_monday_through_sunday() {
    // 2026-08-03 is Monday → week is 2026-08-03 (Mon) .. 2026-08-09 (Sun)
    let monday = NaiveDate::from_ymd_opt(2026, 8, 3).unwrap();
    assert_eq!(
        reporting_week_bounds(monday),
        (
            NaiveDate::from_ymd_opt(2026, 8, 3).unwrap(),
            NaiveDate::from_ymd_opt(2026, 8, 9).unwrap()
        )
    );
    // Wednesday stays in the same Mon–Sun week
    let wednesday = NaiveDate::from_ymd_opt(2026, 8, 5).unwrap();
    assert_eq!(
        reporting_week_bounds(wednesday),
        (
            NaiveDate::from_ymd_opt(2026, 8, 3).unwrap(),
            NaiveDate::from_ymd_opt(2026, 8, 9).unwrap()
        )
    );
    // Sunday is the end of that week
    let sunday = NaiveDate::from_ymd_opt(2026, 8, 9).unwrap();
    assert_eq!(
        reporting_week_bounds(sunday),
        (
            NaiveDate::from_ymd_opt(2026, 8, 3).unwrap(),
            NaiveDate::from_ymd_opt(2026, 8, 9).unwrap()
        )
    );
}

#[test]
fn calendar_month_and_day_bounds() {
    let d = NaiveDate::from_ymd_opt(2026, 8, 3).unwrap();
    assert_eq!(
        calendar_month_bounds(d),
        (
            NaiveDate::from_ymd_opt(2026, 8, 1).unwrap(),
            NaiveDate::from_ymd_opt(2026, 8, 31).unwrap()
        )
    );
    let jan = NaiveDate::from_ymd_opt(2026, 1, 15).unwrap();
    assert_eq!(
        calendar_month_bounds(jan),
        (
            NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
            NaiveDate::from_ymd_opt(2026, 1, 31).unwrap()
        )
    );
}

#[test]
fn resolve_resolved_date_range_presets() {
    let today = NaiveDate::from_ymd_opt(2026, 8, 3).unwrap();
    let (from, to) = resolve_resolved_date_range(true, false, false, None, None, today);
    assert_eq!(from.as_deref(), Some("2026-08-03"));
    assert_eq!(to.as_deref(), Some("2026-08-09"));

    let (from, to) = resolve_resolved_date_range(false, true, false, None, None, today);
    assert_eq!(from.as_deref(), Some("2026-08-01"));
    assert_eq!(to.as_deref(), Some("2026-08-31"));

    let (from, to) = resolve_resolved_date_range(false, false, true, None, None, today);
    assert_eq!(from.as_deref(), Some("2026-08-03"));
    assert_eq!(to.as_deref(), Some("2026-08-03"));

    let (from, to) = resolve_resolved_date_range(
        false,
        false,
        false,
        Some("2026-01-01".into()),
        Some("2026-01-31".into()),
        today,
    );
    assert_eq!(from.as_deref(), Some("2026-01-01"));
    assert_eq!(to.as_deref(), Some("2026-01-31"));

    // Explicit times are stripped to calendar dates for query/display consistency.
    let (from, to) = resolve_resolved_date_range(
        false,
        false,
        false,
        Some("2026-01-01 08:30:00".into()),
        Some("2026-01-31 18:00:00".into()),
        today,
    );
    assert_eq!(from.as_deref(), Some("2026-01-01"));
    assert_eq!(to.as_deref(), Some("2026-01-31"));
}

#[test]
fn date_presets_conflict_with_each_other_and_resolved_range() {
    assert!(Cli::try_parse_from(["zentao", "bug", "stats", "--week", "--month"]).is_err());
    assert!(Cli::try_parse_from(["zentao", "bug", "list", "--day", "--week"]).is_err());
    assert!(Cli::try_parse_from([
        "zentao",
        "bug",
        "stats",
        "--week",
        "--resolved-from",
        "2026-01-01",
    ])
    .is_err());
}

#[test]
fn bug_stats_week_expands_into_search_query() {
    let cli = Cli::try_parse_from(["zentao", "bug", "stats", "--week"]).expect("parse");
    let Commands::Bug(BugArgs {
        command: BugSubCommands::Stats(args),
    }) = cli.command
    else {
        panic!("unexpected");
    };
    assert!(args.week);
    let query = BugSearchQuery::from(&args);
    assert!(query.resolved_from.is_some());
    assert!(query.resolved_to.is_some());
    // from is Monday, to is Sunday (calendar dates only)
    let from =
        NaiveDate::parse_from_str(query.resolved_from.as_deref().unwrap(), "%Y-%m-%d").unwrap();
    let to = NaiveDate::parse_from_str(query.resolved_to.as_deref().unwrap(), "%Y-%m-%d").unwrap();
    assert_eq!(from.weekday(), Weekday::Mon);
    assert_eq!(to.weekday(), Weekday::Sun);
    assert_eq!((to - from).num_days(), 6);
}

#[test]
fn bug_stats_rejects_full_title() {
    assert!(Cli::try_parse_from(["zentao", "bug", "stats", "--full-title"]).is_err());
}

#[test]
fn bug_stats_parses_shared_filters_and_plain() {
    let cli = Cli::try_parse_from([
        "zentao", "bug", "stats", "--title", "会议", "-a", "zhousong", "--module", "1099", "-s",
        "active", "-L", "50", "--plain",
    ])
    .expect("should parse");
    match cli.command {
        Commands::Bug(BugArgs {
            command: BugSubCommands::Stats(args),
        }) => {
            assert_eq!(args.title, vec!["会议"]);
            assert_eq!(args.assignee.as_deref(), Some("zhousong"));
            assert_eq!(args.module.as_deref(), Some("1099"));
            assert!(matches!(args.state, BugState::Active));
            assert_eq!(args.limit, 50);
            assert!(args.plain);
        }
        _ => panic!("unexpected command"),
    }
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
        resolved_date: String::new(),
        resolution: String::new(),
        deadline: String::new(),
    }
}

#[test]
fn aggregate_stats_groups_sorts_by_active_then_resolved() {
    let bugs = vec![
        sample_bug_row(1, "激活", "bob"),
        sample_bug_row(2, "active", "alice"),
        sample_bug_row(3, "已解决", "alice"),
        sample_bug_row(4, "已关闭", "alice"),
        sample_bug_row(5, "closed", "Closed"),
        sample_bug_row(6, "激活", ""),
        sample_bug_row(7, "resolved", "--"),
        // charlie: 0 active, 3 resolved — below anyone with active
        sample_bug_row(8, "resolved", "charlie"),
        sample_bug_row(9, "resolved", "charlie"),
        sample_bug_row(10, "resolved", "charlie"),
    ];
    let stats =
        aggregate_stats_by_assignee(&bugs, 100, "2026-08-03 12:00:00".to_string(), None, None);
    assert_eq!(stats.sample_size, 10);
    assert!(!stats.incomplete);
    assert_eq!(stats.rows.len(), 5);

    // Sort: active desc, then resolved desc, then name
    // alice/unassigned: active=1 resolved=1 (name: 未指派 before alice); bob: 1/0; charlie: 0/3
    assert_eq!(stats.rows[0].assignee, BUG_STATS_UNASSIGNED);
    assert_eq!(stats.rows[0].active, 1);
    assert_eq!(stats.rows[0].resolved, 1);

    assert_eq!(stats.rows[1].assignee, "alice");
    assert_eq!(stats.rows[1].active, 1);
    assert_eq!(stats.rows[1].resolved, 1);

    assert_eq!(stats.rows[2].assignee, "bob");
    assert_eq!(stats.rows[2].active, 1);
    assert_eq!(stats.rows[2].resolved, 0);
    assert_eq!(stats.rows[2].total, 1);

    assert_eq!(stats.rows[3].assignee, "charlie");
    assert_eq!(stats.rows[3].active, 0);
    assert_eq!(stats.rows[3].resolved, 3);

    assert_eq!(stats.rows[4].assignee, BUG_STATS_CLOSED_BUCKET);
    assert_eq!(stats.rows[4].closed, 2);

    assert_eq!(stats.total.active, 3);
    assert_eq!(stats.total.resolved, 5);
    assert_eq!(stats.total.closed, 2);
    assert_eq!(stats.total.total, 10);
}

#[test]
fn aggregate_stats_marks_incomplete_when_sample_hits_limit() {
    let bugs = vec![
        sample_bug_row(1, "active", "alice"),
        sample_bug_row(2, "closed", "alice"),
    ];
    let stats =
        aggregate_stats_by_assignee(&bugs, 2, "2026-08-03 12:00:00".to_string(), None, None);
    assert!(stats.incomplete);
    assert_eq!(stats.sample_size, 2);
    assert_eq!(stats.limit, 2);
}

#[test]
fn render_stats_json_shape_and_field_subset() {
    let stats = aggregate_stats_by_assignee(
        &[
            sample_bug_row(1, "active", "alice"),
            sample_bug_row(2, "closed", "Closed"),
        ],
        10,
        "2026-08-03 12:00:00".to_string(),
        Some("2026-08-03".into()),
        Some("2026-08-09".into()),
    );
    let full = render_stats_json(&stats, "").expect("json");
    assert_eq!(full["groupBy"], "assignee");
    assert_eq!(full["sampleSize"], 2);
    assert_eq!(full["limit"], 10);
    assert_eq!(full["incomplete"], false);
    assert_eq!(full["fetchedAt"], "2026-08-03 12:00:00");
    assert_eq!(full["resolvedFrom"], "2026-08-03");
    assert_eq!(full["resolvedTo"], "2026-08-09");
    assert!(full.get("teamOpen").is_none());
    assert_eq!(full["rows"][0]["assignee"], "alice");
    assert_eq!(full["rows"][0]["active"], 1);
    assert_eq!(full["rows"][0]["closed"], 0);
    assert_eq!(full["rows"][0]["total"], 1);
    assert!(full["rows"][0].get("openShare").is_none());
    assert_eq!(full["rows"][1]["assignee"], BUG_STATS_CLOSED_BUCKET);
    assert_eq!(full["rows"][1]["closed"], 1);
    assert!(full["total"].get("assignee").is_none());
    assert_eq!(full["total"]["active"], 1);
    assert_eq!(full["total"]["closed"], 1);

    let subset = render_stats_json(&stats, "assignee,active").expect("subset");
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
fn render_stats_table_has_no_duplicate_incomplete_footer() {
    let stats = aggregate_stats_by_assignee(
        &[
            sample_bug_row(1, "active", "alice"),
            sample_bug_row(2, "closed", "bob"),
        ],
        2,
        "2026-08-03 12:00:00".to_string(),
        Some("2026-08-03".into()),
        Some("2026-08-09".into()),
    );
    let table = render_bug_stats_table(&stats, false);
    assert!(table.contains("指派给"));
    assert!(table.contains("待验证"));
    assert!(table.contains("合计"));
    assert!(!table.contains("未关占比"));
    assert!(!table.contains('%'));
    assert!(table.contains("alice"));
    assert!(table.contains(BUG_STATS_CLOSED_BUCKET));
    assert!(!table
        .lines()
        .any(|line| line.contains("bob") && !line.contains(BUG_STATS_TOTAL_LABEL)));
    assert!(table.contains(BUG_STATS_TOTAL_LABEL));
    // resolved range + fetched time are own lines under the table
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
    // incomplete is stderr-only; table must not repeat the sample footer
    assert!(!table.contains("sample:"));
    assert!(!table.contains("incomplete"));
    assert_eq!(
        format_stats_incomplete_warning(&stats),
        "warning: 样本已达 limit=2（聚合 2 条），可能不全；请提高 -L 或收窄筛选"
    );

    let colored = render_bug_stats_table(&stats, true);
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

#[test]
fn removed_search_and_bug_show_are_rejected() {
    assert!(Cli::try_parse_from(["zentao", "search"]).is_err());
    assert!(Cli::try_parse_from(["zentao", "bug", "show", "1"]).is_err());
}

#[test]
fn auth_login_has_no_password_argument() {
    assert!(Cli::try_parse_from([
        "zentao",
        "auth",
        "login",
        "--username",
        "alice",
        "--password",
        "secret",
    ])
    .is_err());
    assert!(Cli::try_parse_from([
        "zentao",
        "auth",
        "login",
        "--username",
        "alice",
        "--password-stdin",
        "--cookie-file",
        "/tmp/cookies",
    ])
    .is_err());
}

#[test]
fn bug_view_accepts_id_with_configured_site() {
    let got = parse_bug_input("51214", Some("http://example.com/zentao")).expect("parse id");
    assert_eq!(got.id, 51214);
    assert_eq!(got.site_url, "http://example.com/zentao");
    assert_eq!(got.bug_url, "http://example.com/zentao/bug-view-51214.html");
}

#[test]
fn bug_view_url_uses_its_own_site() {
    let got = parse_bug_input(
        "http://shendao.sharexm.cn/zentao/bug-view-51214.html?tid=1",
        Some("http://ignored.example/zentao"),
    )
    .expect("parse url");
    assert_eq!(got.id, 51214);
    assert_eq!(got.site_url, "http://shendao.sharexm.cn/zentao");
}

#[test]
fn bug_view_id_requires_site() {
    let err = parse_bug_input("51214", None).expect_err("missing site");
    assert!(err.to_string().contains("缺少 site"));
}

#[test]
fn bug_view_json_and_output_conflict() {
    assert!(
        Cli::try_parse_from(["zentao", "bug", "view", "1", "--json", "-o", "bug.json",]).is_err()
    );
}

#[test]
fn json_fields_are_selected_and_normalized() {
    let result = search::SearchResult {
        bugs: vec![search::BugRow {
            id: 1,
            severity: "2".to_string(),
            pri: "3".to_string(),
            confirmed: "是".to_string(),
            title: "标题".to_string(),
            status: "激活".to_string(),
            opened_by: "alice".to_string(),
            opened_date: "07-31 10:00".to_string(),
            assigned_to: "bob".to_string(),
            resolved_date: "00-00 00:00".to_string(),
            resolution: String::new(),
            deadline: "0000-00-00".to_string(),
        }],
        total: None,
    };
    let got = render_list_json(
        &result,
        "http://example.com/zentao",
        "id,state,confirmed,openedDate,resolvedDate,url",
    )
    .expect("json");
    assert_eq!(
        got,
        json!([{
            "id": 1,
            "state": "active",
            "confirmed": true,
            "openedDate": "07-31 10:00",
            "resolvedDate": null,
            "url": "http://example.com/zentao/bug-view-1.html"
        }])
    );
}

#[test]
fn result_limit_applies_to_table_and_json() {
    let row = search::BugRow {
        id: 1,
        severity: "2".to_string(),
        pri: "3".to_string(),
        confirmed: "是".to_string(),
        title: "第一条".to_string(),
        status: "激活".to_string(),
        opened_by: "alice".to_string(),
        opened_date: "07-31 10:00".to_string(),
        assigned_to: "bob".to_string(),
        resolved_date: String::new(),
        resolution: String::new(),
        deadline: String::new(),
    };
    let mut result = search::SearchResult {
        bugs: vec![
            row.clone(),
            search::BugRow {
                id: 2,
                title: "第二条".to_string(),
                ..row
            },
        ],
        total: None,
    };

    apply_result_limit(&mut result, 1);
    assert_eq!(result.bugs.len(), 1);
    let table = render_bug_list_table(&result, false, "http://example.com", false, false);
    assert!(table.contains("第一条"));
    assert!(!table.contains("第二条"));
    let json = render_list_json(&result, "http://example.com", "id,title").expect("json");
    assert_eq!(json, json!([{ "id": 1, "title": "第一条" }]));
}

#[test]
fn bug_table_uses_terminal_display_width_for_cjk_text() {
    let bug = search::BugRow {
        id: 1,
        severity: String::new(),
        pri: String::new(),
        confirmed: String::new(),
        title: "中文标题".to_string(),
        status: "active".to_string(),
        opened_by: "陈婕".to_string(),
        opened_date: "07-31 10:00".to_string(),
        assigned_to: "alice".to_string(),
        resolved_date: String::new(),
        resolution: String::new(),
        deadline: String::new(),
    };
    let table = render_bug_list_table(
        &search::SearchResult {
            bugs: vec![bug],
            total: None,
        },
        false,
        "http://example.com",
        false,
        false,
    );
    let header = table.lines().next().expect("header");
    assert!(header.contains("创建者"));
    assert!(header.contains("创建日期"));
    assert!(header.contains("指派给"));
    let row = table.lines().nth(1).expect("row");
    // 编号(6)+sp+状态(9)+sp+创建者(10)+sp+创建日期(11)+sp+标题(65)+sp = 106 before 指派给
    let assignee_byte = row.find("alice").expect("assignee");
    assert_eq!(UnicodeWidthStr::width(&row[..assignee_byte]), 106);
    let opened_by_byte = row.find("陈婕").expect("opened_by");
    assert_eq!(UnicodeWidthStr::width(&row[..opened_by_byte]), 17);
    let opened_date_byte = row.find("07-31 10:00").expect("opened_date");
    assert_eq!(UnicodeWidthStr::width(&row[..opened_date_byte]), 28);
    assert!(row.contains("陈婕"));
    assert!(row.contains("07-31 10:00"));
    let six_columns = truncate_for_table("中文标题", 6);
    let five_columns = truncate_for_table("中文标题", 5);
    assert_eq!(UnicodeWidthStr::width(six_columns.as_str()), 6);
    assert_eq!(UnicodeWidthStr::width(five_columns.as_str()), 5);
}

#[test]
fn bug_table_full_title_keeps_complete_title_and_still_truncates_assignee() {
    let long_title = "【会议优化5.1期】H5和APP登录同一个账号在同一个直播间会议中展示人数不一致";
    let long_assignee = "特别长的指派人名";
    let bug = search::BugRow {
        id: 57879,
        severity: String::new(),
        pri: String::new(),
        confirmed: String::new(),
        title: format!("{long_title}\n换行"),
        status: "resolved".to_string(),
        opened_by: "牛威龙".to_string(),
        opened_date: "07-31 16:36".to_string(),
        assigned_to: long_assignee.to_string(),
        resolved_date: String::new(),
        resolution: String::new(),
        deadline: String::new(),
    };
    let result = search::SearchResult {
        bugs: vec![bug],
        total: None,
    };

    let truncated = render_bug_list_table(&result, false, "http://example.com", false, false);
    let truncated_row = truncated.lines().nth(1).expect("row");
    assert!(truncated_row.contains('…'));
    assert!(!truncated_row.contains(long_title));

    let full = render_bug_list_table(&result, true, "http://example.com", false, false);
    assert_eq!(
        full.lines().count(),
        2,
        "embedded newlines must not create extra rows"
    );
    let full_row = full.lines().nth(1).expect("row");
    assert!(full_row.contains(long_title));
    assert!(full_row.contains("换行"));
    assert!(!full_row.contains(long_assignee));
    assert!(full_row.contains('…'));
}

#[test]
fn bug_table_title_uses_osc8_hyperlink_when_enabled() {
    let bug = search::BugRow {
        id: 57879,
        severity: String::new(),
        pri: String::new(),
        confirmed: String::new(),
        title: "会议优化标题".to_string(),
        status: "active".to_string(),
        opened_by: String::new(),
        opened_date: "07-31 16:36".to_string(),
        assigned_to: "alice".to_string(),
        resolved_date: String::new(),
        resolution: String::new(),
        deadline: String::new(),
    };
    let result = search::SearchResult {
        bugs: vec![bug],
        total: None,
    };
    let site = "http://example.com/zentao";
    let linked = render_bug_list_table(&result, false, site, true, true);
    let expected_url = "http://example.com/zentao/bug-view-57879.html";
    assert!(linked.contains(&format!("\x1b]8;;{expected_url}\x1b\\")));
    assert!(linked.contains("会议优化标题"));
    assert!(linked.contains("\x1b]8;;\x1b\\"));

    let plain = render_bug_list_table(&result, false, site, false, false);
    assert!(!plain.contains("\x1b]8;;"));
    assert!(!plain.contains('\x1b'));
    assert!(plain.contains("会议优化标题"));
}

#[test]
fn unknown_json_field_is_rejected() {
    let err = parse_json_fields("id,unknown", LIST_JSON_FIELDS).expect_err("must fail");
    assert!(err.to_string().contains("不支持 JSON 字段"));
}

#[test]
fn invalid_json_fields_are_rejected_before_io() {
    let dir = tempfile::tempdir().expect("temp dir");
    let invalid_config = dir.path().join("invalid.json");
    fs::write(&invalid_config, "{").expect("write invalid config");

    for command in [
        vec!["bug", "list", "--json=unknown"],
        vec!["bug", "view", "1", "--json=unknown"],
    ] {
        let mut args = vec![
            OsString::from("--config"),
            invalid_config.as_os_str().to_os_string(),
        ];
        args.extend(command.into_iter().map(OsString::from));
        let error = run(args).expect_err("must fail");
        assert!(error.to_string().contains("不支持 JSON 字段"));
    }
}

#[test]
fn bare_json_selects_all_fields() {
    let fields = parse_json_fields("", VIEW_JSON_FIELDS).expect("all fields");
    assert_eq!(
        fields,
        VIEW_JSON_FIELDS
            .iter()
            .map(|field| (*field).to_string())
            .collect::<Vec<_>>()
    );

    let cli = Cli::try_parse_from(["zentao", "bug", "view", "1", "--json"])
        .expect("bare json should parse");
    match cli.command {
        Commands::Bug(BugArgs {
            command: BugSubCommands::View(args),
        }) => assert_eq!(args.json.as_deref(), Some("")),
        _ => panic!("unexpected command"),
    }
}

#[test]
fn json_fields_require_equals_syntax() {
    let cli = Cli::try_parse_from(["zentao", "bug", "list", "--json=id,title"])
        .expect("equals syntax should parse");
    match cli.command {
        Commands::Bug(BugArgs {
            command: BugSubCommands::List(args),
        }) => assert_eq!(args.json.as_deref(), Some("id,title")),
        _ => panic!("unexpected command"),
    }

    assert!(Cli::try_parse_from(["zentao", "bug", "list", "--json", "id,title"]).is_err());
}

#[test]
fn bare_json_does_not_consume_bug_id() {
    let cli = Cli::try_parse_from(["zentao", "bug", "view", "--json", "57801"])
        .expect("bare json should not consume the bug ID");
    match cli.command {
        Commands::Bug(BugArgs {
            command: BugSubCommands::View(args),
        }) => {
            assert_eq!(args.bug, "57801");
            assert_eq!(args.json.as_deref(), Some(""));
        }
        _ => panic!("unexpected command"),
    }
}

#[test]
fn clap_parameter_errors_keep_clap_exit_code() {
    let error = run(vec![
        OsString::from("bug"),
        OsString::from("list"),
        OsString::from("--limit"),
        OsString::from("0"),
    ])
    .expect_err("invalid limit must fail during clap parsing");

    match error {
        RunError::Clap(error) => assert_eq!(error.exit_code(), 2),
        RunError::Runtime(error) => panic!("expected clap error, got runtime error: {error}"),
    }
}

#[test]
fn view_json_exposes_description_and_history_images() {
    let detail = bug::BugDetail {
        title: "标题".to_string(),
        markdown_description: "![截图](http://example.com/description.png)".to_string(),
        markdown_history: "- 备注：![重复截图](http://example.com/description.png) ![历史图片](http://example.com/history.jpeg)".to_string(),
        attachments: vec![],
    };
    let got = render_view_json(1, "http://example.com", &detail, "images").expect("json");
    assert_eq!(
        got,
        json!({
            "images": [
                "http://example.com/description.png",
                "http://example.com/history.jpeg"
            ]
        })
    );
}

#[test]
fn view_json_url_is_canonical() {
    let detail = bug::BugDetail {
        title: "标题".to_string(),
        markdown_description: String::new(),
        markdown_history: String::new(),
        attachments: vec![],
    };
    let got = render_view_json(
        1,
        "http://example.com/zentao/?tid=temporary",
        &detail,
        "url",
    )
    .expect("json");
    assert_eq!(
        got,
        json!({ "url": "http://example.com/zentao/bug-view-1.html" })
    );
}

#[test]
fn cookie_table_masks_values_by_default() {
    let rows = vec![CookieTableRow {
        name: "zp".to_string(),
        value: "secret".to_string(),
        domain: "example.com".to_string(),
        path: "/".to_string(),
        secure: "false".to_string(),
        http_only: "true".to_string(),
        expires: "session".to_string(),
    }];
    let masked = render_cookie_table(&rows, false).join("\n");
    assert!(masked.contains("***"));
    assert!(!masked.contains("secret"));
    let shown = render_cookie_table(&rows, true).join("\n");
    assert!(shown.contains("secret"));
}

#[test]
fn auth_status_markers_are_plain_text_when_not_a_tty() {
    assert_eq!(cookie_presence_label(true), "[OK]");
    assert_eq!(cookie_presence_label(false), "[MISSING]");
    assert_eq!(current_profile_marker(true), " [当前]");
    assert_eq!(
        format_cookie_domains_line(&["example.com".to_string()]),
        "example.com [OK]"
    );
    assert_eq!(format_cookie_domains_line(&[]), "(none) [MISSING]");
}

#[test]
fn state_maps_to_zentao_values() {
    assert_eq!(BugState::Active.zentao_value(), Some("active"));
    assert_eq!(BugState::All.zentao_value(), None);
}

#[test]
fn config_key_validation() {
    let mut cfg = config::Config::default();
    set_config_value(&mut cfg, ConfigKey::Site, "http://example.com/zentao/").unwrap();
    set_config_value(&mut cfg, ConfigKey::Product, "92").unwrap();
    set_config_value(&mut cfg, ConfigKey::CookieSource, "file").unwrap();
    assert_eq!(cfg.site, "http://example.com/zentao");
    assert_eq!(cfg.product, Some(92));
    assert!(matches!(cfg.cookie_source, CookieSource::File));
    assert!(set_config_value(&mut cfg, ConfigKey::Product, "0").is_err());
}

#[test]
fn image_url_validation_still_rejects_non_http() {
    assert!(validate_image_url("https://example.com/a.png").is_ok());
    assert!(validate_image_url("file:///tmp/a.png").is_err());
    assert!(validate_image_url("").is_err());
}

#[test]
fn image_download_rejects_login_html_and_accepts_images() {
    let cases = [
        (
            "login redirect",
            vec![
                ImageResponsePlan {
                    path: "/image.png",
                    status: 302,
                    location: Some("/user-login-test.html"),
                    content_type: "text/html",
                    body: b"",
                },
                ImageResponsePlan {
                    path: "/user-login-test.html",
                    status: 200,
                    location: None,
                    content_type: "text/html",
                    body: b"<html>login</html>",
                },
            ],
            false,
        ),
        (
            "html response",
            vec![ImageResponsePlan {
                path: "/image.png",
                status: 200,
                location: None,
                content_type: "text/html",
                body: b"<html>login</html>",
            }],
            false,
        ),
        (
            "png response",
            vec![ImageResponsePlan {
                path: "/image.png",
                status: 200,
                location: None,
                content_type: "image/png",
                body: b"\x89PNG\r\n",
            }],
            true,
        ),
    ];

    for (_name, plans, should_succeed) in cases {
        let (site, seen_cookies, handle) = spawn_image_server(plans);
        let url = Url::parse(&format!("{site}/image.png")).expect("image url");
        let dir = tempfile::tempdir().expect("temp dir");
        let out = dir.path().join("image.png");
        let result = download_single_image(&url, "zp=test", &out);

        handle.join().expect("server should join");
        assert!(
            seen_cookies
                .lock()
                .expect("lock should succeed")
                .iter()
                .all(|cookie| cookie == "zp=test"),
            "Cookie header should be sent on every request"
        );
        if should_succeed {
            result.expect("image should download");
            assert_eq!(fs::read(&out).expect("image should exist"), b"\x89PNG\r\n");
        } else {
            result.expect_err("non-image response should fail");
            assert!(!out.exists(), "failed download must not create a file");
        }
    }
}

#[test]
fn image_url_derives_zentao_site_for_cookie_lookup() {
    let image = Url::parse("http://example.com/zentao/file-read-1.png").expect("url");
    assert_eq!(
        derive_site_url_from_image_url(&image).expect("site"),
        "http://example.com/zentao"
    );
}

#[test]
fn resolve_output_path_adds_extension() {
    let dir = tempfile::tempdir().expect("tmp");
    let url = Url::parse("http://example.com/file-read-1").unwrap();
    let out = resolve_output_path_from_url(dir.path(), &url);
    assert!(out.ends_with("file-read-1.img"));
}
