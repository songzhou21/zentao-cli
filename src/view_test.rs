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

#[test]
fn html_to_markdown_projects_supported_tags() {
    let html = concat!(
        "<p>【基本信息】</p>",
        "<p>见 <a href=\"http://example.com/a\">链接</a></p>",
        r#"<p><img src="http://example.com/zentao/file-read-1.png" /></p>"#,
        "<ol><li>第一步</li><li><br /></li><li>第三步</li></ol>",
    );
    let got = html_to_markdown(html);
    assert_eq!(
        got,
        concat!(
            "【基本信息】\n\n",
            "见 [链接](http://example.com/a)\n\n",
            "![](http://example.com/zentao/file-read-1.png)\n\n",
            "1. 第一步\n",
            "2. 第三步",
        )
    );
}

#[test]
fn markdown_is_projected_from_view_json() {
    let detail = crate::bug::parse_bug_json(
        "http://zentao.test.sharexm.cn/zentao/bug-view-58441.html",
        &read_bug_fixture("bug_58441.json"),
    )
    .expect("parse");
    let mut json =
        render_json(58441, "http://zentao.test.sharexm.cn/zentao", &detail, "").expect("json");
    let markdown = render_markdown(&json);
    assert!(markdown
        .starts_with("# 58441 【线上问题】会议号324594366 ，iqoo neo 9说话，苹果14听不到\n"));
    assert!(markdown.contains("- 状态：resolved\n"));
    assert!(markdown.contains("- 优先级：2\n"));
    assert!(markdown.contains("- 创建者：牛威龙\n"));
    assert!(markdown.contains("- 创建日期：2026-08-13 12:10:03\n"));
    assert!(markdown.contains("- 指派给：牛威龙\n"));
    assert!(markdown.contains("- 解决者：周松\n"));
    assert!(markdown.contains("- 解决日期：2026-08-18 14:57:39\n"));
    assert!(markdown
        .contains("- 上线版本：1.2.17-iOS-0831（会议5.1+直播优惠券+广场二期+banner加视频）\n"));
    assert!(markdown.contains("- 链接：http://zentao.test.sharexm.cn/zentao/bug-view-58441.html\n"));
    assert!(markdown.contains("## 描述\n"));
    assert!(markdown.contains("会议号324594366"));
    assert!(markdown.contains("![](http://zentao.test.sharexm.cn/zentao/file-read-73220.png)"));
    assert!(!markdown.contains("<p>"));
    assert!(!markdown.contains("<img"));
    assert!(markdown.contains("## 历史\n"));
    assert!(markdown.contains(" · 牛威龙 · opened\n"));
    assert!(markdown.contains(" · 张涛 · assigned\n"));
    assert!(markdown.contains("指派给 周松\n"));
    assert!(markdown.contains(" · 周松 · resolved\n"));
    assert!(markdown.contains("- 上线版本："));
    assert!(markdown.contains("原因（背景）："));

    json["title"] = json!("CHANGED");
    json["history"][1]["assignee"] = json!("李四");
    let projected = render_markdown(&json);
    assert!(projected.starts_with("# 58441 CHANGED\n"));
    assert!(projected.contains("指派给 李四\n"));
    assert!(!projected.contains("【线上问题】"));
}

#[test]
fn markdown_projects_attachments_and_images_from_json() {
    let detail = crate::bug::parse_bug_json(
        "http://shendao.sharexm.cn/zentao/bug-view-58688.html",
        &read_bug_fixture("bug_58688.json"),
    )
    .expect("parse");
    let json = render_json(58688, "http://shendao.sharexm.cn/zentao", &detail, "").expect("json");
    let markdown = render_markdown(&json);
    assert!(markdown.starts_with("# 58688 【广场二期】ios-选中的状态没有展示在右上角\n"));
    assert!(markdown.contains("![](http://shendao.sharexm.cn/zentao/file-read-73622.png)"));
    assert!(markdown.contains("![](http://shendao.sharexm.cn/zentao/file-read-73623.png)"));
    assert!(markdown.contains("## 附件\n"));
    assert!(markdown.contains(
        "- [安卓下拉选择状态.mp4](http://shendao.sharexm.cn/zentao/data/upload/1/202608/19114642022437,m)"
    ));
    assert!(markdown.contains(
        "- [ios下拉选择状态.mp4](http://shendao.sharexm.cn/zentao/data/upload/1/202608/19115046021443kl)"
    ));
    assert!(markdown.contains("1. "));
}
