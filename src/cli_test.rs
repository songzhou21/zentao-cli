use super::*;
use clap::Parser;
use reqwest::Url;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread;
use tempfile::tempdir;

// 同时提供 CLI 与配置时，CLI 参数优先级最高。
#[test]
fn resolve_required_should_prefer_cli() {
    let got = resolve_required(Some("http://cli"), Some("http://cfg"), "url").unwrap();
    assert_eq!(got, "http://cli");
}

// CLI 缺失时应回退配置值。
#[test]
fn resolve_required_should_fallback_to_config() {
    let got = resolve_required(None, Some("http://cfg"), "url").unwrap();
    assert_eq!(got, "http://cfg");
}

// CLI 与配置都缺失时应返回缺参错误。
#[test]
fn resolve_required_should_error_when_missing() {
    let err = resolve_required(None, None, "url").expect_err("should fail");
    assert!(err.to_string().contains("缺少 url"));
}

// session cookie（expires=0）应输出固定文案 session。
#[test]
fn format_cookie_expiry_session() {
    assert_eq!(format_cookie_expiry(0), "session");
}

// 已知 expires_utc 应转换成预期 UTC 时间字符串。
#[test]
fn format_cookie_expiry_known_timestamp() {
    let expires_utc = (1704067200_i64 + 11_644_473_600_i64) * 1_000_000_i64;
    assert_eq!(format_cookie_expiry(expires_utc), "2024-01-01 00:00:00 UTC");
}

// Chrome epoch 微秒应可精确转换回 Unix 秒。
#[test]
fn chrome_expires_utc_to_unix_known_timestamp() {
    let expires_utc = (1704067200_i64 + 11_644_473_600_i64) * 1_000_000_i64;
    assert_eq!(chrome_expires_utc_to_unix(expires_utc), 1704067200_i64);
}

#[test]
fn parse_bug_input_numeric_has_no_site_url() {
    let got = parse_bug_input("51214").expect("should parse");
    assert_eq!(
        got,
        ParsedBugInput {
            id: 51214,
            site_url: None,
        }
    );
}

#[test]
fn parse_bug_input_detail_url_uses_its_site_url() {
    let got = parse_bug_input("http://shendao.sharexm.cn/zentao/bug-view-51214.html")
        .expect("should parse");
    assert_eq!(
        got,
        ParsedBugInput {
            id: 51214,
            site_url: Some("http://shendao.sharexm.cn/zentao".to_string()),
        }
    );
}

#[test]
fn parse_bug_input_detail_url_strips_query_from_site_url() {
    let got = parse_bug_input("http://shendao.sharexm.cn/zentao/bug-view-51214.html?tid=1")
        .expect("should parse");
    assert_eq!(
        got.site_url.as_deref(),
        Some("http://shendao.sharexm.cn/zentao")
    );
}

#[test]
fn derive_site_url_from_root_bug_url() {
    let url = Url::parse("http://shendao.sharexm.cn/bug-view-51214.html").expect("url");
    let got = derive_site_url_from_bug_url(&url).expect("should derive");
    assert_eq!(got, "http://shendao.sharexm.cn");
}

// 非法输入应返回明确错误。
#[test]
fn parse_bug_input_invalid() {
    let err = parse_bug_input("http://shendao/share/bug-xxx").expect_err("should fail");
    assert!(err.to_string().contains("Bug ID 无效"));
}

#[test]
fn image_download_cli_parse_success() {
    let cli = Cli::try_parse_from([
        "zentao",
        "image",
        "download",
        "--url",
        "http://example.com/a.png",
    ])
    .expect("should parse");

    match cli.command {
        Commands::Image(ImageArgs {
            command: ImageSubCommands::Download(args),
        }) => {
            assert_eq!(args.url, "http://example.com/a.png");
        }
        _ => panic!("unexpected command"),
    }
}

#[test]
fn image_download_cli_requires_url() {
    let err = Cli::try_parse_from(["zentao", "image", "download"]).expect_err("should fail");
    assert!(err.to_string().contains("--url"));
}

#[test]
fn search_cli_parse_json_flag() {
    let cli = Cli::try_parse_from([
        "zentao",
        "search",
        "--assigned-to",
        "zhousong",
        "--json",
        "--debug",
    ])
    .expect("should parse");

    match cli.command {
        Commands::Search(args) => {
            assert_eq!(args.assigned_to.as_deref(), Some("zhousong"));
            assert!(args.json);
            assert!(args.debug);
            assert_eq!(args.page_size, 20);
        }
        _ => panic!("unexpected command"),
    }
}

#[test]
fn search_cli_parse_title_keyword() {
    let cli =
        Cli::try_parse_from(["zentao", "search", "--title", "系统测试"]).expect("should parse");

    match cli.command {
        Commands::Search(args) => {
            assert_eq!(args.title, vec!["系统测试".to_string()]);
        }
        _ => panic!("unexpected command"),
    }
}

#[test]
fn search_cli_parse_title_or_keyword() {
    let cli = Cli::try_parse_from([
        "zentao",
        "search",
        "--title",
        "系统测试",
        "--title-or",
        "线上问题",
    ])
    .expect("should parse");

    match cli.command {
        Commands::Search(args) => {
            assert_eq!(args.title, vec!["系统测试".to_string()]);
            assert_eq!(args.title_or.as_deref(), Some("线上问题"));
        }
        _ => panic!("unexpected command"),
    }
}

#[test]
fn search_cli_parse_repeated_title_keywords() {
    let cli = Cli::try_parse_from([
        "zentao",
        "search",
        "--title",
        "系统测试",
        "--title",
        "线上问题",
    ])
    .expect("should parse");

    match cli.command {
        Commands::Search(args) => {
            assert_eq!(
                args.title,
                vec!["系统测试".to_string(), "线上问题".to_string()]
            );
        }
        _ => panic!("unexpected command"),
    }
}

#[test]
fn search_cli_parse_page_size() {
    let cli = Cli::try_parse_from([
        "zentao",
        "search",
        "--assigned-to",
        "zhousong",
        "--page-size",
        "200",
    ])
    .expect("should parse");

    match cli.command {
        Commands::Search(args) => {
            assert_eq!(args.assigned_to.as_deref(), Some("zhousong"));
            assert_eq!(args.page_size, 200);
        }
        _ => panic!("unexpected command"),
    }
}

#[test]
fn search_cli_parse_resolved_by_and_date_range() {
    let cli = Cli::try_parse_from([
        "zentao",
        "search",
        "--resolved-by",
        "zhousong",
        "--resolved-date-from",
        "2025-11-14",
        "--resolved-date-to",
        "2025-11-22",
    ])
    .expect("should parse");

    match cli.command {
        Commands::Search(args) => {
            assert_eq!(args.resolved_by.as_deref(), Some("zhousong"));
            assert_eq!(args.resolved_date_from.as_deref(), Some("2025-11-14"));
            assert_eq!(args.resolved_date_to.as_deref(), Some("2025-11-22"));
        }
        _ => panic!("unexpected command"),
    }
}

#[test]
fn search_cli_parse_module_and_status() {
    let cli = Cli::try_parse_from([
        "zentao",
        "search",
        "--module",
        "1099",
        "--bug-status",
        "active",
    ])
    .expect("should parse");

    match cli.command {
        Commands::Search(args) => {
            assert_eq!(args.module.as_deref(), Some("1099"));
            assert_eq!(args.bug_status.as_deref(), Some("active"));
        }
        _ => panic!("unexpected command"),
    }
}

#[test]
fn search_cli_parse_group() {
    let cli = Cli::try_parse_from(["zentao", "search", "--group", "module"]).expect("should parse");
    match cli.command {
        Commands::Search(args) => {
            assert!(matches!(args.group, Some(SearchGroupBy::TestModule)));
        }
        _ => panic!("unexpected command"),
    }

    let cli =
        Cli::try_parse_from(["zentao", "search", "--group", "assigned-to"]).expect("should parse");
    match cli.command {
        Commands::Search(args) => {
            assert!(matches!(args.group, Some(SearchGroupBy::AssignedTo)));
        }
        _ => panic!("unexpected command"),
    }
}

#[test]
fn compact_debug_search_form_only_keeps_expected_keys() {
    let form = vec![
        ("fieldtitle".to_string(), "".to_string()),
        ("andOr1".to_string(), "AND".to_string()),
        ("field1".to_string(), "module".to_string()),
        ("operator1".to_string(), "belong".to_string()),
        ("value1".to_string(), "1099".to_string()),
        ("groupAndOr".to_string(), "and".to_string()),
        (
            "formType".to_string(),
            "more92-0-bySearch-myQueryID.html".to_string(),
        ),
    ];

    let compact = compact_debug_search_form(&form);
    assert_eq!(compact.first().map(|x| x.0.as_str()), Some("andOr1"));
    assert_eq!(compact.last().map(|x| x.0.as_str()), Some("formType"));
    assert!(
        compact.iter().all(|(_, v)| !v.is_empty()),
        "should only keep keys present in original form"
    );
    assert!(
        compact.iter().all(|(k, _)| k != "fieldtitle"),
        "should not include default header fields"
    );
}

#[test]
fn render_search_form_lisp_outputs_grouped_expression() {
    let form = vec![
        ("andOr1".to_string(), "AND".to_string()),
        ("field1".to_string(), "module".to_string()),
        ("operator1".to_string(), "belong".to_string()),
        ("value1".to_string(), "1099".to_string()),
        ("andOr2".to_string(), "and".to_string()),
        ("field2".to_string(), "assignedTo".to_string()),
        ("operator2".to_string(), "=".to_string()),
        ("value2".to_string(), "zhousong".to_string()),
        ("andOr3".to_string(), "and".to_string()),
        ("field3".to_string(), "status".to_string()),
        ("operator3".to_string(), "=".to_string()),
        ("value3".to_string(), "active".to_string()),
        ("groupAndOr".to_string(), "and".to_string()),
        ("andOr4".to_string(), "AND".to_string()),
        ("field4".to_string(), "status".to_string()),
        ("operator4".to_string(), "=".to_string()),
        ("value4".to_string(), "".to_string()),
        ("andOr5".to_string(), "and".to_string()),
        ("field5".to_string(), "resolvedDate".to_string()),
        ("operator5".to_string(), "<=".to_string()),
        ("value5".to_string(), "".to_string()),
        ("andOr6".to_string(), "and".to_string()),
        ("field6".to_string(), "resolvedBy".to_string()),
        ("operator6".to_string(), "=".to_string()),
        ("value6".to_string(), "".to_string()),
        ("module".to_string(), "bug".to_string()),
        (
            "actionURL".to_string(),
            "/zentao/bug-browse-92-0-bySearch-myQueryID.html".to_string(),
        ),
        ("groupItems".to_string(), "3".to_string()),
        (
            "formType".to_string(),
            "more92-0-bySearch-myQueryID.html".to_string(),
        ),
    ];

    let got = render_search_form_lisp(&form);
    assert_eq!(
        got,
        "(and (and (module belong \"1099\") (assignedTo = \"zhousong\")) (status = \"active\"))"
    );
}

#[test]
fn render_compact_debug_form_lines_outputs_slot_on_single_line() {
    let form = vec![
        ("andOr1".to_string(), "AND".to_string()),
        ("field1".to_string(), "module".to_string()),
        ("operator1".to_string(), "belong".to_string()),
        ("value1".to_string(), "1099".to_string()),
        ("andOr2".to_string(), "and".to_string()),
        ("field2".to_string(), "assignedTo".to_string()),
        ("operator2".to_string(), "=".to_string()),
        ("value2".to_string(), "zhousong".to_string()),
        ("andOr3".to_string(), "and".to_string()),
        ("field3".to_string(), "status".to_string()),
        ("operator3".to_string(), "=".to_string()),
        ("value3".to_string(), "active".to_string()),
        ("groupAndOr".to_string(), "and".to_string()),
        ("andOr4".to_string(), "AND".to_string()),
        ("field4".to_string(), "status".to_string()),
        ("operator4".to_string(), "=".to_string()),
        ("value4".to_string(), "".to_string()),
        ("andOr5".to_string(), "and".to_string()),
        ("field5".to_string(), "resolvedDate".to_string()),
        ("operator5".to_string(), "<=".to_string()),
        ("value5".to_string(), "".to_string()),
        ("andOr6".to_string(), "and".to_string()),
        ("field6".to_string(), "resolvedBy".to_string()),
        ("operator6".to_string(), "=".to_string()),
        ("value6".to_string(), "".to_string()),
        ("module".to_string(), "bug".to_string()),
        (
            "actionURL".to_string(),
            "/zentao/bug-browse-92-0-bySearch-myQueryID.html".to_string(),
        ),
        ("groupItems".to_string(), "3".to_string()),
        (
            "formType".to_string(),
            "more92-0-bySearch-myQueryID.html".to_string(),
        ),
    ];

    let lines = render_compact_debug_form_lines(&form);
    assert_eq!(
        lines[0],
        "andOr1=AND field1=module operator1=belong value1=1099"
    );
    assert_eq!(lines[3], "groupAndOr=and");
}

#[test]
fn render_compact_debug_form_lines_skips_missing_slots() {
    let form = vec![
        ("andOr1".to_string(), "AND".to_string()),
        ("field1".to_string(), "module".to_string()),
        ("operator1".to_string(), "belong".to_string()),
        ("value1".to_string(), "1099".to_string()),
        ("module".to_string(), "bug".to_string()),
        (
            "actionURL".to_string(),
            "/zentao/bug-browse-92-0-bySearch-myQueryID.html".to_string(),
        ),
        ("groupItems".to_string(), "3".to_string()),
        (
            "formType".to_string(),
            "more92-0-bySearch-myQueryID.html".to_string(),
        ),
    ];

    let lines = render_compact_debug_form_lines(&form);
    assert_eq!(
        lines[0],
        "andOr1=AND field1=module operator1=belong value1=1099"
    );
    assert!(lines.iter().all(|l| !l.starts_with("andOr2=")));
    assert!(lines.iter().all(|l| !l.starts_with("groupAndOr=")));
}

#[test]
fn validate_search_group_limits_accepts_title_or_two_groups() {
    let args = SearchArgs {
        title: vec!["系统测试".to_string()],
        title_or: Some("线上问题".to_string()),
        assigned_to: None,
        resolved_by: None,
        resolved_date_from: None,
        resolved_date_to: None,
        module: Some("1099".to_string()),
        bug_status: Some("active".to_string()),
        group: None,
        url: None,
        profile: None,
        config: None,
        json: false,
        debug: false,
        page_size: 20,
    };

    let got = validate_search_group_limits(&args);
    assert!(got.is_ok(), "should pass group limit validation: {got:?}");
}

#[test]
fn validate_search_group_limits_accepts_title_or_with_assigned_and_date() {
    let args = SearchArgs {
        title: vec!["系统测试".to_string(), "线上问题".to_string()],
        title_or: None,
        assigned_to: Some("zhousong".to_string()),
        resolved_by: None,
        resolved_date_from: Some("2025-11-14".to_string()),
        resolved_date_to: Some("2025-11-22".to_string()),
        module: None,
        bug_status: None,
        group: None,
        url: None,
        profile: None,
        config: None,
        json: false,
        debug: false,
        page_size: 20,
    };

    let got = validate_search_group_limits(&args);
    assert!(got.is_ok(), "should pass group limit validation: {got:?}");
}

#[test]
fn validate_search_group_limits_rejects_title_or_with_too_many_group1_filters() {
    let args = SearchArgs {
        title: vec!["系统测试".to_string(), "线上问题".to_string()],
        title_or: None,
        assigned_to: Some("zhousong".to_string()),
        resolved_by: Some("lisi".to_string()),
        resolved_date_from: Some("2025-11-14".to_string()),
        resolved_date_to: Some("2025-11-22".to_string()),
        module: Some("1099".to_string()),
        bug_status: Some("active".to_string()),
        group: None,
        url: None,
        profile: None,
        config: None,
        json: false,
        debug: false,
        page_size: 20,
    };

    let err = validate_search_group_limits(&args).expect_err("should reject");
    assert!(err.to_string().contains("每个搜索 group 最多支持 3 个条件"));
}

#[test]
fn append_search_cookie_page_size_appends_cookie() {
    let got = append_search_cookie_page_size("za=1; zentaosid=2", 1000);
    assert_eq!(got, "za=1; zentaosid=2; pagerBugBrowse=1000");
}

#[test]
fn append_search_cookie_page_size_handles_empty_base() {
    let got = append_search_cookie_page_size("  ", 1000);
    assert_eq!(got, "pagerBugBrowse=1000");
}

#[test]
fn login_cli_parse_required_fields() {
    let cli = Cli::try_parse_from([
        "zentao",
        "login",
        "--url",
        "http://example.com/zentao",
        "--username",
        "alice",
        "--password",
        "secret",
        "--proxy",
        "socks5h://127.0.0.1:1080",
    ])
    .expect("should parse");

    match cli.command {
        Commands::Login(args) => {
            assert_eq!(args.url.as_deref(), Some("http://example.com/zentao"));
            assert_eq!(args.username, "alice");
            assert_eq!(args.password, "secret");
            assert_eq!(args.proxy.as_deref(), Some("socks5h://127.0.0.1:1080"));
        }
        _ => panic!("unexpected command"),
    }
}

#[test]
fn format_login_response_decodes_unicode_message() {
    let raw =
        r#"{"result":"fail","message":"\u60a8\u8fd8\u67093\u6b21\u5c1d\u8bd5\u673a\u4f1a\u3002"}"#;
    let got = format_login_response(raw);
    assert!(got.contains("result=fail"));
    assert!(got.contains("您还有3次尝试机会。"));
}

#[test]
fn validate_image_url_accepts_http_https() {
    assert!(validate_image_url("http://example.com/a.png").is_ok());
    assert!(validate_image_url("https://example.com/a.png").is_ok());
}

#[test]
fn validate_image_url_rejects_non_http_and_empty() {
    assert!(validate_image_url("file:///tmp/a.png").is_err());
    assert!(validate_image_url("   ").is_err());
}

#[test]
fn resolve_output_path_adds_img_extension() {
    let dir = tempdir().expect("tempdir");
    let url = Url::parse("http://example.com/file-read-1").expect("url");
    let out = resolve_output_path_from_url(dir.path(), &url);
    assert_eq!(
        out.file_name().and_then(|n| n.to_str()),
        Some("file-read-1.img")
    );
}

#[test]
fn resolve_output_path_appends_suffix_when_collision() {
    let dir = tempdir().expect("tempdir");
    let existing = dir.path().join("a.png");
    std::fs::write(&existing, b"x").expect("seed file");

    let url = Url::parse("http://example.com/a.png").expect("url");
    let out = resolve_output_path_from_url(dir.path(), &url);
    assert_eq!(out.file_name().and_then(|n| n.to_str()), Some("a(1).png"));
}

#[test]
fn download_single_image_success() {
    let (url, handle) = spawn_once_server(200, b"PNGDATA");
    let dir = tempdir().expect("tempdir");
    let out = dir.path().join("test.png");

    download_single_image(&url, &out).expect("download should succeed");
    let bytes = std::fs::read(&out).expect("read downloaded file");
    assert_eq!(bytes, b"PNGDATA");

    handle.join().expect("server thread");
}

#[test]
fn download_single_image_http_error() {
    let (url, handle) = spawn_once_server(404, b"NOTFOUND");
    let dir = tempdir().expect("tempdir");
    let out = dir.path().join("test.png");

    let err = download_single_image(&url, &out).expect_err("should fail");
    assert!(err.to_string().contains("下载失败: HTTP 404"));

    handle.join().expect("server thread");
}

#[test]
fn collect_cookie_table_rows_filters_and_orders_target_cookies() {
    let items = vec![
        browser::BrowserCookieItem {
            name: "lang".to_string(),
            value: "zh-cn".to_string(),
            domain: "example.com".to_string(),
            path: "/".to_string(),
            secure: false,
            http_only: false,
            expires_utc: 0,
            creation_utc: 0,
            last_access_utc: 0,
        },
        browser::BrowserCookieItem {
            name: "zp".to_string(),
            value: "zp-value".to_string(),
            domain: "example.com".to_string(),
            path: "/zentao/".to_string(),
            secure: false,
            http_only: true,
            expires_utc: 0,
            creation_utc: 0,
            last_access_utc: 0,
        },
        browser::BrowserCookieItem {
            name: "za".to_string(),
            value: "zhousong".to_string(),
            domain: "example.com".to_string(),
            path: "/zentao/".to_string(),
            secure: false,
            http_only: true,
            expires_utc: 0,
            creation_utc: 0,
            last_access_utc: 0,
        },
        browser::BrowserCookieItem {
            name: "zentaosid".to_string(),
            value: "sid".to_string(),
            domain: "example.com".to_string(),
            path: "/".to_string(),
            secure: false,
            http_only: true,
            expires_utc: 0,
            creation_utc: 0,
            last_access_utc: 0,
        },
        browser::BrowserCookieItem {
            name: "keepLogin".to_string(),
            value: "on".to_string(),
            domain: "example.com".to_string(),
            path: "/zentao/".to_string(),
            secure: false,
            http_only: true,
            expires_utc: 0,
            creation_utc: 0,
            last_access_utc: 0,
        },
    ];

    let rows = collect_cookie_table_rows(&items);
    let names: Vec<String> = rows.iter().map(|r| r.name.clone()).collect();
    assert_eq!(names, vec!["zentaosid", "za", "zp", "keepLogin"]);
}

#[test]
fn render_cookie_table_contains_header_and_session_expiry() {
    let rows = vec![CookieTableRow {
        name: "zentaosid".to_string(),
        value: "sid".to_string(),
        domain: "example.com".to_string(),
        path: "/".to_string(),
        secure: "false".to_string(),
        http_only: "true".to_string(),
        expires: "session".to_string(),
    }];

    let lines = render_cookie_table(&rows);
    assert!(!lines.is_empty());
    assert!(lines[0].contains("name"));
    assert!(lines[0].contains("httpOnly"));
    assert!(lines[0].contains("expires"));
    let data_line = lines
        .iter()
        .find(|line| line.contains("zentaosid"))
        .expect("should contain zentaosid row");
    assert!(data_line.contains("session"));
}

fn spawn_once_server(status: u16, body: &'static [u8]) -> (Url, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind listener");
    let addr = listener.local_addr().expect("local addr");

    let handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept");
        let mut buf = [0_u8; 1024];
        let _ = stream.read(&mut buf);

        let status_line = match status {
            200 => "HTTP/1.1 200 OK",
            404 => "HTTP/1.1 404 Not Found",
            500 => "HTTP/1.1 500 Internal Server Error",
            _ => "HTTP/1.1 400 Bad Request",
        };
        let headers = format!(
            "{status_line}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        );
        stream.write_all(headers.as_bytes()).expect("write headers");
        stream.write_all(body).expect("write body");
    });

    (
        Url::parse(&format!("http://{}/image.png", addr)).expect("parse server url"),
        handle,
    )
}
