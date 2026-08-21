use super::*;
use crate::config;
use crate::config::CookieSource;
use crate::search;
use crate::view;
use chrono::{Datelike, NaiveDate, Weekday};
use clap::Parser;
use serde_json::json;
use std::fs;
use unicode_width::UnicodeWidthStr;

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
fn bug_list_keeps_default_active_with_resolved_date_filters() {
    let cli = Cli::try_parse_from(["zentao", "bug", "list", "--week"]).expect("parse");
    let Commands::Bug(BugArgs {
        command: BugSubCommands::List(args),
    }) = cli.command
    else {
        panic!("unexpected command");
    };
    let query = BugSearchQuery::from(&args);
    assert!(matches!(query.state, BugState::Active));
    assert!(query.resolved_from.is_some());
    assert!(query.resolved_to.is_some());

    let cli = Cli::try_parse_from([
        "zentao",
        "bug",
        "list",
        "--resolved-from",
        "2026-08-01",
        "--resolved-to",
        "2026-08-31",
    ])
    .expect("parse");
    let Commands::Bug(BugArgs {
        command: BugSubCommands::List(args),
    }) = cli.command
    else {
        panic!("unexpected command");
    };
    assert!(matches!(
        BugSearchQuery::from(&args).state,
        BugState::Active
    ));

    let cli = Cli::try_parse_from(["zentao", "bug", "list", "--week", "-s", "all"]).expect("parse");
    let Commands::Bug(BugArgs {
        command: BugSubCommands::List(args),
    }) = cli.command
    else {
        panic!("unexpected command");
    };
    assert!(matches!(BugSearchQuery::from(&args).state, BugState::All));
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
fn bug_list_parses_opened_and_resolved_build() {
    let cli = Cli::try_parse_from([
        "zentao",
        "bug",
        "list",
        "--opened-build",
        "982",
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
    assert_eq!(args.opened_build.as_deref(), Some("982"));
    let query = BugSearchQuery::from(&args);
    assert_eq!(query.opened_build.as_deref(), Some("982"));
    assert!(query.resolved_build.is_none());
    assert!(build_search_field_params(&query)
        .iter()
        .any(|(k, v)| k == "openedBuild" && v == "982"));

    let cli = Cli::try_parse_from(["zentao", "bug", "stats", "--resolved-build", "982"])
        .expect("should parse");
    let Commands::Bug(BugArgs {
        command: BugSubCommands::Stats(args),
    }) = cli.command
    else {
        panic!("unexpected command");
    };
    assert_eq!(args.resolved_build.as_deref(), Some("982"));
    let query = BugSearchQuery::from(&args);
    assert_eq!(query.resolved_build.as_deref(), Some("982"));
    assert!(query.opened_build.is_none());
    assert!(build_search_field_params(&query)
        .iter()
        .any(|(k, v)| k == "resolvedBuild" && v == "982"));
}

#[test]
fn bug_selection_requires_kind_and_parses_build() {
    assert!(Cli::try_parse_from(["zentao", "bug", "selection"]).is_err());
    let cli = Cli::try_parse_from([
        "zentao",
        "bug",
        "selection",
        "--build",
        "会议5.1",
        "--json=value,name",
    ])
    .expect("should parse");
    let Commands::Bug(BugArgs {
        command: BugSubCommands::Selection(args),
    }) = cli.command
    else {
        panic!("unexpected command");
    };
    assert!(args.build);
    assert_eq!(args.keyword.as_deref(), Some("会议5.1"));
    assert_eq!(args.json.as_deref(), Some("value,name"));
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

#[test]
fn bug_stats_rejects_by_flag() {
    assert!(Cli::try_parse_from(["zentao", "bug", "stats", "--by", "resolved-by"]).is_err());
}

#[test]
fn removed_search_and_bug_show_are_rejected() {
    assert!(Cli::try_parse_from(["zentao", "search"]).is_err());
    assert!(Cli::try_parse_from(["zentao", "bug", "show", "1"]).is_err());
    assert!(Cli::try_parse_from([
        "zentao",
        "image",
        "download",
        "--url",
        "http://example.com/a.png"
    ])
    .is_err());
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
fn bug_view_parses_raw_json_flag() {
    let cli = Cli::try_parse_from(["zentao", "bug", "view", "1", "--raw-json"]).expect("parse");
    match cli.command {
        Commands::Bug(BugArgs {
            command: BugSubCommands::View(args),
        }) => {
            assert!(args.raw_json);
            assert!(args.json.is_none());
        }
        _ => panic!("unexpected command"),
    }
}

#[test]
fn bug_view_raw_json_conflicts_with_json() {
    assert!(Cli::try_parse_from(["zentao", "bug", "view", "1", "--raw-json", "--json"]).is_err());
}

#[test]
fn json_fields_are_selected_and_normalized() {
    let result = search::SearchResult {
        bugs: vec![search::BugRow {
            id: 1,
            severity: "2".to_string(),
            pri: "3".to_string(),
            confirmed: "1".to_string(),
            title: "标题".to_string(),
            status: "active".to_string(),
            opened_by: "alice".to_string(),
            opened_date: "2026-07-31 10:00:00".to_string(),
            assigned_to: "bob".to_string(),
            resolved_by: String::new(),
            resolved_date: "0000-00-00 00:00:00".to_string(),
            resolution: "fixed".to_string(),
            deadline: "0000-00-00".to_string(),
        }],
        total: None,
    };
    let got = render_list_json(
        &result,
        "http://example.com/zentao",
        "id,state,confirmed,openedDate,resolvedBy,resolvedDate,resolution,url",
    )
    .expect("json");
    assert_eq!(
        got,
        json!([{
            "id": 1,
            "state": "active",
            "confirmed": true,
            "openedDate": "2026-07-31 10:00:00",
            "resolvedBy": null,
            "resolvedDate": null,
            "resolution": "fixed",
            "url": "http://example.com/zentao/bug-view-1.html"
        }])
    );
}

#[test]
fn list_json_confirmed_is_true_only_for_one() {
    let confirmed_json = |confirmed: &str| {
        render_list_json(
            &search::SearchResult {
                bugs: vec![search::BugRow {
                    id: 1,
                    severity: String::new(),
                    pri: String::new(),
                    confirmed: confirmed.to_string(),
                    title: "t".to_string(),
                    status: "active".to_string(),
                    opened_by: String::new(),
                    opened_date: String::new(),
                    assigned_to: String::new(),
                    resolved_by: String::new(),
                    resolved_date: String::new(),
                    resolution: String::new(),
                    deadline: String::new(),
                }],
                total: None,
            },
            "http://example.com/zentao",
            "confirmed",
        )
        .expect("json")
    };
    assert_eq!(confirmed_json("1"), json!([{ "confirmed": true }]));
    assert_eq!(confirmed_json("0"), json!([{ "confirmed": false }]));
    assert_eq!(confirmed_json(""), json!([{ "confirmed": false }]));
    assert_eq!(confirmed_json("是"), json!([{ "confirmed": false }]));
}

#[test]
fn list_json_from_browse_fixture_keeps_codes_and_full_dates() {
    let parsed = search::parse_browse_json(include_str!(
        "../../tests/fixtures/search/browse_bysearch_myqueryid.json"
    ))
    .expect("parse");
    let bug = parsed
        .bugs
        .iter()
        .find(|bug| bug.id == 58496)
        .cloned()
        .expect("resolved bug");
    let got = render_list_json(
        &search::SearchResult {
            bugs: vec![bug],
            total: None,
        },
        "http://zentao.test.sharexm.cn/zentao",
        "",
    )
    .expect("json");
    assert_eq!(
        got,
        json!([{
            "id": 58496,
            "title": "【会议优化5.1期兼容】创建会议后不会自动分享邀请链接给嘉宾",
            "state": "resolved",
            "severity": 2,
            "priority": 2,
            "confirmed": true,
            "openedBy": "崔文波",
            "openedDate": "2026-08-14 15:31:29",
            "assignee": "崔文波",
            "resolvedBy": "周松",
            "resolvedDate": "2026-08-18 10:12:35",
            "resolution": "fixed",
            "deadline": "2026-08-05",
            "url": "http://zentao.test.sharexm.cn/zentao/bug-view-58496.html"
        }])
    );

    let table = render_bug_list_table(
        &search::SearchResult {
            bugs: parsed
                .bugs
                .iter()
                .find(|bug| bug.id == 58496)
                .cloned()
                .into_iter()
                .collect(),
            total: None,
        },
        true,
        "http://zentao.test.sharexm.cn/zentao",
        false,
        false,
    );
    assert!(table.contains("2026-08-14 15:31:29"));
    assert!(table.contains("崔文波"));
    assert!(table.contains("resolved"));
}

#[test]
fn truncated_warning_fires_at_limit_and_absent_below() {
    assert_eq!(truncated_warning(30, 29), None);
    let at_limit = truncated_warning(30, 30).expect("hit limit warns");
    assert!(at_limit.contains("limit=30"));
    assert!(at_limit.contains("30 条"));
    assert!(at_limit.contains("-L"));
    let over_limit = truncated_warning(30, 62).expect("over limit warns");
    assert!(over_limit.contains("62 条"));
}

#[test]
fn result_limit_applies_to_table_and_json() {
    let row = search::BugRow {
        id: 1,
        severity: "2".to_string(),
        pri: "3".to_string(),
        confirmed: "1".to_string(),
        title: "第一条".to_string(),
        status: "active".to_string(),
        opened_by: "alice".to_string(),
        opened_date: "2026-07-31 10:00:00".to_string(),
        assigned_to: "bob".to_string(),
        resolved_by: String::new(),
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
        opened_date: "2026-07-31 10:00:00".to_string(),
        assigned_to: "alice".to_string(),
        resolved_by: String::new(),
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
    // 编号(6)+sp+状态(9)+sp+创建者(10)+sp+创建日期(19)+sp+标题(65)+sp = 114 before 指派给
    let assignee_byte = row.find("alice").expect("assignee");
    assert_eq!(UnicodeWidthStr::width(&row[..assignee_byte]), 114);
    let opened_by_byte = row.find("陈婕").expect("opened_by");
    assert_eq!(UnicodeWidthStr::width(&row[..opened_by_byte]), 17);
    let opened_date_byte = row.find("2026-07-31 10:00:00").expect("opened_date");
    assert_eq!(UnicodeWidthStr::width(&row[..opened_date_byte]), 28);
    assert!(row.contains("陈婕"));
    assert!(row.contains("2026-07-31 10:00:00"));
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
        opened_date: "2026-07-31 16:36:00".to_string(),
        assigned_to: long_assignee.to_string(),
        resolved_by: String::new(),
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
        opened_date: "2026-07-31 16:36:00".to_string(),
        assigned_to: "alice".to_string(),
        resolved_by: String::new(),
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
        vec!["bug", "selection", "--build", "--json=unknown"],
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
    let fields = parse_json_fields("", view::JSON_FIELDS).expect("all fields");
    assert_eq!(
        fields,
        view::JSON_FIELDS
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
