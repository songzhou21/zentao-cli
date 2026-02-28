use super::*;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};
use std::thread;

#[derive(Clone)]
struct ResponsePlan {
    path: &'static str,
    status: u16,
    location: Option<&'static str>,
    body: &'static str,
}

fn spawn_test_server(
    plans: Vec<ResponsePlan>,
) -> (String, Arc<Mutex<Vec<String>>>, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind should succeed");
    let addr = listener.local_addr().expect("local addr should exist");
    let seen_cookie = Arc::new(Mutex::new(Vec::new()));
    let seen_cookie_bg = Arc::clone(&seen_cookie);

    let handle = thread::spawn(move || {
        let mut handled = 0usize;
        while handled < plans.len() {
            let (mut stream, _) = listener.accept().expect("accept should succeed");

            let mut buf = [0_u8; 4096];
            let n = stream.read(&mut buf).expect("read should succeed");
            let req = String::from_utf8_lossy(&buf[..n]).to_string();

            if let Some(cookie_line) = req
                .lines()
                .find(|l| l.to_ascii_lowercase().starts_with("cookie:"))
            {
                let value = cookie_line
                    .split_once(':')
                    .map(|(_, v)| v.trim().to_string())
                    .unwrap_or_default();
                seen_cookie_bg
                    .lock()
                    .expect("lock should succeed")
                    .push(value);
            }

            let path = req
                .lines()
                .next()
                .and_then(|l| l.split_whitespace().nth(1))
                .unwrap_or("/");

            let plan = plans
                .iter()
                .find(|p| p.path == path)
                .or_else(|| plans.first())
                .expect("plan should exist");

            let status_text = match plan.status {
                200 => "OK",
                302 => "Found",
                403 => "Forbidden",
                500 => "Internal Server Error",
                _ => "OK",
            };

            let mut resp = format!("HTTP/1.1 {} {}\r\n", plan.status, status_text);
            if let Some(location) = plan.location {
                resp.push_str(&format!("Location: {}\r\n", location));
            }
            resp.push_str(&format!("Content-Length: {}\r\n", plan.body.len()));
            resp.push_str("Connection: close\r\n\r\n");
            resp.push_str(plan.body);
            let _ = stream.write_all(resp.as_bytes());

            handled += 1;
        }
    });

    (format!("http://{}", addr), seen_cookie, handle)
}

// 站点 URL 末尾斜杠应被规范化移除，避免后续拼接双斜杠。
#[test]
fn trim_site_url() {
    let api = ZentaoApi::new("http://example.com/", "v1").unwrap();
    assert_eq!(api.site_url, "http://example.com");
}

// VerifyCookie 访问 my-profile 页面，应覆盖无跳转成功、登录跳转、非预期跳转和 HTTP 非 2xx。
#[test]
fn verify_cookie_table_cases() {
    let cases = [
        (
            "success no redirect",
            vec![ResponsePlan {
                path: "/my-profile.html",
                status: 200,
                location: None,
                body: "ok",
            }],
            "",
            "/my-profile.html",
        ),
        (
            "login redirect",
            vec![
                ResponsePlan {
                    path: "/my-profile.html",
                    status: 302,
                    location: Some("/user-login-abc.html"),
                    body: "",
                },
                ResponsePlan {
                    path: "/user-login-abc.html",
                    status: 200,
                    location: None,
                    body: "login",
                },
            ],
            "cookie 无效或已过期",
            "",
        ),
        (
            "unexpected redirect",
            vec![
                ResponsePlan {
                    path: "/my-profile.html",
                    status: 302,
                    location: Some("/project-index.html"),
                    body: "",
                },
                ResponsePlan {
                    path: "/project-index.html",
                    status: 200,
                    location: None,
                    body: "project",
                },
            ],
            "发生跳转",
            "",
        ),
        (
            "http non 2xx",
            vec![ResponsePlan {
                path: "/my-profile.html",
                status: 500,
                location: None,
                body: "err",
            }],
            "cookie 校验失败: HTTP 500",
            "",
        ),
    ];

    for (_name, plans, want_err, want_url_suffix) in cases {
        let (site, seen_cookie, handle) = spawn_test_server(plans);
        let api = ZentaoApi::new(&site, "v1").expect("api should build");
        let got = api.verify_cookie("zp=test");

        handle.join().expect("server should join");
        let sent = seen_cookie.lock().expect("lock should succeed").clone();
        assert!(
            sent.iter().any(|v| v == "zp=test"),
            "cookie header not sent: {sent:?}"
        );

        if !want_err.is_empty() {
            let err = got.expect_err("should fail");
            assert!(
                err.to_string().contains(want_err),
                "expected err contains {want_err}, got {err}"
            );
            continue;
        }

        let final_url = got.expect("should succeed");
        assert!(
            final_url.ends_with(want_url_suffix),
            "unexpected final url: {final_url}"
        );
    }
}

// build_search_form 应生成正确的表单参数。
#[test]
fn build_search_form_defaults() {
    let form = build_search_form(92, "/zentao/bug-browse-92-0-bySearch-myQueryID.html", &[]);

    // Should contain the default field entries
    let find = |k: &str| {
        form.iter()
            .find(|(key, _)| key == k)
            .map(|(_, v)| v.as_str())
    };

    assert_eq!(find("fieldconfirmed"), Some("ZERO"));
    assert_eq!(find("fieldproduct"), Some("92"));
    assert_eq!(find("fieldmodule"), Some("ZERO"));
    assert_eq!(find("fieldseverity"), Some("0"));
    assert_eq!(find("fieldpri"), Some("0"));

    // Slot 1 defaults: assignedTo, operator =, empty value
    assert_eq!(find("field1"), Some("assignedTo"));
    assert_eq!(find("operator1"), Some("="));
    assert_eq!(find("value1"), Some(""));

    // Slot 2 defaults: resolvedDate, operator >=, empty value
    assert_eq!(find("field2"), Some("resolvedDate"));
    assert_eq!(find("operator2"), Some(">="));
    assert_eq!(find("value2"), Some(""));

    // Slot 5 defaults: resolvedDate, operator <=, empty value
    assert_eq!(find("field5"), Some("resolvedDate"));
    assert_eq!(find("operator5"), Some("<="));
    assert_eq!(find("value5"), Some(""));

    // Fixed metadata
    assert_eq!(find("module"), Some("bug"));
    assert_eq!(
        find("actionURL"),
        Some("/zentao/bug-browse-92-0-bySearch-myQueryID.html")
    );
    assert_eq!(find("groupItems"), Some("3"));
    assert_eq!(find("groupAndOr"), Some("and"));
}

// build_search_form 应能正确注入 assignedTo 和日期范围覆盖。
#[test]
fn build_search_form_with_overrides() {
    let overrides = vec![
        ("assignedTo".to_string(), "zhousong".to_string()),
        ("resolvedDate_from".to_string(), "2025-01-01".to_string()),
        ("resolvedDate_to".to_string(), "2025-12-31".to_string()),
    ];
    let form = build_search_form(
        92,
        "/zentao/bug-browse-92-0-bySearch-myQueryID.html",
        &overrides,
    );

    let find = |k: &str| {
        form.iter()
            .find(|(key, _)| key == k)
            .map(|(_, v)| v.as_str())
    };

    // Slot 1: assignedTo = zhousong
    assert_eq!(find("field1"), Some("assignedTo"));
    assert_eq!(find("operator1"), Some("="));
    assert_eq!(find("value1"), Some("zhousong"));

    // Slot 2: resolvedDate >= 2025-01-01
    assert_eq!(find("field2"), Some("resolvedDate"));
    assert_eq!(find("operator2"), Some(">="));
    assert_eq!(find("value2"), Some("2025-01-01"));

    // Slot 5: resolvedDate <= 2025-12-31
    assert_eq!(find("field5"), Some("resolvedDate"));
    assert_eq!(find("operator5"), Some("<="));
    assert_eq!(find("value5"), Some("2025-12-31"));
}

// build_search_form 应将 product_id 填入 fieldproduct。
#[test]
fn build_search_form_product_id() {
    let form = build_search_form(123, "/action", &[]);
    let find = |k: &str| {
        form.iter()
            .find(|(key, _)| key == k)
            .map(|(_, v)| v.as_str())
    };
    assert_eq!(find("fieldproduct"), Some("123"));
    assert_eq!(find("formType"), Some("more123-0-bySearch-myQueryID.html"));
}

// set_form_value 应能替换已有 key 的值。
#[test]
fn set_form_value_replaces_existing() {
    let mut form = vec![
        ("key1".to_string(), "old".to_string()),
        ("key2".to_string(), "unchanged".to_string()),
    ];
    set_form_value(&mut form, "key1", "new");
    assert_eq!(form[0].1, "new");
    assert_eq!(form[1].1, "unchanged");
}

// set_form_value 不存在的 key 应保持原样。
#[test]
fn set_form_value_nonexistent_key_is_noop() {
    let mut form = vec![("key1".to_string(), "val".to_string())];
    set_form_value(&mut form, "missing", "value");
    assert_eq!(form.len(), 1);
    assert_eq!(form[0].1, "val");
}

// FetchBugHTML 应覆盖成功、空页面、登录跳转和 HTTP 非 2xx。
#[test]
fn fetch_bug_html_table_cases() {
    let cases = [
        (
            "success",
            vec![ResponsePlan {
                path: "/bug-view-51214.html",
                status: 200,
                location: None,
                body: "<html><body>ok</body></html>",
            }],
            "",
            "ok",
        ),
        (
            "empty body",
            vec![ResponsePlan {
                path: "/bug-view-51214.html",
                status: 200,
                location: None,
                body: "   ",
            }],
            "页面内容为空",
            "",
        ),
        (
            "login redirect",
            vec![
                ResponsePlan {
                    path: "/bug-view-51214.html",
                    status: 302,
                    location: Some("/user-login-abc.html"),
                    body: "",
                },
                ResponsePlan {
                    path: "/user-login-abc.html",
                    status: 200,
                    location: None,
                    body: "login",
                },
            ],
            "cookie 无效或已过期",
            "",
        ),
        (
            "http non 2xx",
            vec![ResponsePlan {
                path: "/bug-view-51214.html",
                status: 403,
                location: None,
                body: "forbidden",
            }],
            "HTTP 403",
            "",
        ),
    ];

    for (_name, plans, want_err, want_body_contains) in cases {
        let (site, seen_cookie, handle) = spawn_test_server(plans);
        let api = ZentaoApi::new(&site, "v1").expect("api should build");
        let got = api.fetch_bug_html(51214, "zp=test");

        handle.join().expect("server should join");
        let sent = seen_cookie.lock().expect("lock should succeed").clone();
        assert!(
            sent.iter().any(|v| v == "zp=test"),
            "cookie header not sent: {sent:?}"
        );

        if !want_err.is_empty() {
            let err = got.expect_err("should fail");
            assert!(
                err.to_string().contains(want_err),
                "expected err contains {want_err}, got {err}"
            );
            continue;
        }

        let (final_url, body) = got.expect("should succeed");
        assert!(
            final_url.contains("/bug-view-51214.html"),
            "unexpected final url: {final_url}"
        );
        assert!(body.contains(want_body_contains), "unexpected body: {body}");
    }
}

// SearchBugs 遇到 JS 跳转页时应跟随 parent.location 获取真实列表页面。
#[test]
fn search_bugs_follow_js_redirect_page() {
    let plans = vec![
        ResponsePlan {
            path: "/search-buildQuery.html",
            status: 200,
            location: None,
            body: "<html><script>parent.location='/bug-browse-92-0-bySearch-myQueryID.html';</script></html>",
        },
        ResponsePlan {
            path: "/bug-browse-92-0-bySearch-myQueryID.html",
            status: 200,
            location: None,
            body: "<html><table id='bugList'><tbody><tr data-id='51276'><td class='c-title'>ok</td></tr></tbody></table></html>",
        },
    ];

    let (site, seen_cookie, handle) = spawn_test_server(plans);
    let api = ZentaoApi::new(&site, "v1").expect("api should build");
    let got = api.search_bugs("zp=test", 92, &[]);

    handle.join().expect("server should join");
    let sent = seen_cookie.lock().expect("lock should succeed").clone();
    assert!(
        sent.iter().any(|v| v == "zp=test"),
        "cookie header not sent: {sent:?}"
    );

    let body = got.expect("search should succeed");
    assert!(
        body.contains("bugList") && body.contains("51276"),
        "unexpected body: {body}"
    );
}
