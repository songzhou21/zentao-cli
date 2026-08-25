use super::*;
use crate::cli::bug::{BugArgs, BugSearchQuery, BugSubCommands};
use crate::cli::{Cli, Commands};
use chrono::{Datelike, NaiveDate, Weekday};
use clap::Parser;

#[test]
fn bug_report_defaults_resolved_and_closed() {
    let cli = Cli::try_parse_from(["zentao", "bug", "report"]).expect("parse");
    match cli.command {
        Commands::Bug(BugArgs {
            command: BugSubCommands::Report(args),
        }) => {
            assert_eq!(args.state, vec![BugState::Resolved, BugState::Closed]);
            assert_eq!(args.limit, 1000);
            assert!(args.json.is_none());
            assert!(!args.week);
            assert!(!args.weekly);
            assert!(!args.month);
            assert!(!args.day);
            assert!(args.resolved_by.is_none());
        }
        _ => panic!("unexpected command"),
    }
    assert!(Cli::try_parse_from(["zentao", "bug", "report", "--plain"]).is_err());
    assert!(Cli::try_parse_from(["zentao", "bug", "report", "--full-title"]).is_err());
}

#[test]
fn bug_report_weekly_expands_into_search_query() {
    let cli = Cli::try_parse_from([
        "zentao",
        "bug",
        "report",
        "--weekly",
        "--resolved-by",
        "zhousong",
    ])
    .expect("parse");
    let Commands::Bug(BugArgs {
        command: BugSubCommands::Report(args),
    }) = cli.command
    else {
        panic!("unexpected");
    };
    assert!(args.weekly);
    assert_eq!(args.resolved_by.as_deref(), Some("zhousong"));
    let query = BugSearchQuery::from(&args);
    assert_eq!(query.resolved_by.as_deref(), Some("zhousong"));
    let from =
        NaiveDate::parse_from_str(query.resolved_from.as_deref().unwrap(), "%Y-%m-%d").unwrap();
    let to = NaiveDate::parse_from_str(query.resolved_to.as_deref().unwrap(), "%Y-%m-%d").unwrap();
    assert_eq!(from.weekday(), Weekday::Fri);
    assert_eq!(to.weekday(), Weekday::Thu);
    assert_eq!((to - from).num_days(), 6);
}

#[test]
fn bug_report_date_presets_conflict() {
    assert!(Cli::try_parse_from(["zentao", "bug", "report", "--week", "--weekly"]).is_err());
    assert!(Cli::try_parse_from(["zentao", "bug", "report", "--weekly", "--month"]).is_err());
    assert!(Cli::try_parse_from([
        "zentao",
        "bug",
        "report",
        "--weekly",
        "--resolved-from",
        "2026-01-01",
    ])
    .is_err());
}
