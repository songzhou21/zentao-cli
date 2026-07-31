use super::*;
use clap::Parser;
use reqwest::Url;
use serde_json::json;

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
        }
        _ => panic!("unexpected command"),
    }
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
        "id,state,confirmed,resolvedDate,url",
    )
    .expect("json");
    assert_eq!(
        got,
        json!([{
            "id": 1,
            "state": "active",
            "confirmed": true,
            "resolvedDate": null,
            "url": "http://example.com/zentao/bug-view-1.html"
        }])
    );
}

#[test]
fn unknown_json_field_is_rejected() {
    let err = parse_json_fields("id,unknown", LIST_JSON_FIELDS).expect_err("must fail");
    assert!(err.to_string().contains("不支持 JSON 字段"));
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
fn view_json_exposes_description_and_history_images() {
    let detail = bug::BugDetail {
        title: "标题".to_string(),
        markdown_description: "![截图](http://example.com/description.png)".to_string(),
        markdown_history: "- 备注：![历史图片](http://example.com/history.jpeg)".to_string(),
        attachments: vec![],
    };
    let got =
        render_view_json(1, "http://example.com/bug-view-1.html", &detail, "images").expect("json");
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
fn resolve_output_path_adds_extension() {
    let dir = tempfile::tempdir().expect("tmp");
    let url = Url::parse("http://example.com/file-read-1").unwrap();
    let out = resolve_output_path_from_url(dir.path(), &url);
    assert!(out.ends_with("file-read-1.img"));
}
