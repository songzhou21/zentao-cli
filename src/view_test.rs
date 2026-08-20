use super::*;
use crate::bug::{BugDetail, HistoryEvent};
use serde_json::json;
use std::fs;

fn read_bug_fixture(name: &str) -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("bug")
        .join(name);
    fs::read_to_string(path).expect("fixture should exist")
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
fn view_json_exposes_description_and_history_images() {
    let detail = BugDetail {
        title: "标题".to_string(),
        description: r#"<p><img src="http://example.com/description.png" /></p>"#.to_string(),
        images: vec![
            "http://example.com/description.png".to_string(),
            "http://example.com/history.jpeg".to_string(),
        ],
        history: vec![HistoryEvent {
            at: "2026-01-01 00:00:00".to_string(),
            action: "commented".to_string(),
            actor: "周松".to_string(),
            assignee: None,
            changes: vec![],
            comment: Some(r#"<p><img src="http://example.com/history.jpeg" /></p>"#.to_string()),
        }],
        ..BugDetail::default()
    };
    let got = render_json(1, "http://example.com", &detail, "images").expect("json");
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
    let detail = BugDetail {
        title: "标题".to_string(),
        ..BugDetail::default()
    };
    let got = render_json(
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
fn decode_raw_payload_expands_escaped_data_with_readable_chinese() {
    let raw = read_bug_fixture("bug_58441.json");
    assert!(raw.contains("\\u"));
    assert!(!raw.contains("1.2.17-iOS-0831（会议5.1+直播优惠券+广场二期+banner加视频）"));

    let got = decode_raw_payload(&raw).expect("decode");
    let data = got.get("data").expect("data");
    assert!(data.is_object());
    assert_eq!(data["bug"]["id"], json!("58441"));
    assert_eq!(data["bug"]["status"], json!("resolved"));
    assert_eq!(data["bug"]["resolvedBuild"], json!("982"));
    assert_eq!(
        data["builds"]["982"],
        json!("1.2.17-iOS-0831（会议5.1+直播优惠券+广场二期+banner加视频）")
    );
    assert_eq!(data["users"]["zhousong"], json!("周松"));

    let pretty = serde_json::to_string_pretty(&got).expect("pretty");
    assert!(pretty.contains("周松"));
    assert!(pretty.contains("1.2.17-iOS-0831（会议5.1+直播优惠券+广场二期+banner加视频）"));
    assert!(!pretty.contains("\\u5468\\u677e"));
}
