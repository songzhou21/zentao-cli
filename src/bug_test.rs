use super::*;

// 复用既有 HTML / JSON fixture，避免在线依赖导致测试不稳定。
fn read_fixture(name: &str) -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("bug")
        .join(name);
    std::fs::read_to_string(path).expect("fixture should exist")
}

const BUG_48919_URL: &str = "http://shendao.sharexm.cn/zentao/bug-view-48919.html";
const BUG_51267_URL: &str = "http://shendao.sharexm.cn/zentao/bug-view-51267.html";
const BUG_48433_URL: &str = "http://shendao.sharexm.cn/zentao/bug-view-48433.html";
const BUG_58441_URL: &str = "http://zentao.test.sharexm.cn/zentao/bug-view-58441.html";
const BUG_58688_URL: &str = "http://shendao.sharexm.cn/zentao/bug-view-58688.html";

fn assert_48919_description_and_attachments(detail: &BugDetail) {
    assert!(detail.title.contains("PC登录后"));
    assert!(detail.description.contains("PC已登录进入登录确认页面"));
    assert!(detail.description.contains("[基本信息]"));
    let img = "http://shendao.sharexm.cn/zentao/file-read-59561.png";
    assert!(detail
        .description
        .contains(&format!(r#"<img src="{img}" />"#)));
    assert!(!detail.description.contains("!["));
    assert!(detail.images.iter().any(|url| url == img));
    assert!(!detail.description.contains("onload"));
    assert!(!detail.attachments.is_empty());
    assert!(detail.attachments[0].url.contains("/zentao/data/upload/"));
}

fn assert_48919_json_history(detail: &BugDetail) {
    assert!(detail
        .history
        .iter()
        .any(|event| event.at == "2025-12-11 11:25:47" && event.action == "opened"));
    let assigned = detail
        .history
        .iter()
        .find(|event| event.at == "2026-01-05 09:05:16" && event.action == "assigned")
        .expect("assigned");
    assert_eq!(assigned.actor, "刘阳");
    assert_eq!(assigned.assignee.as_deref(), Some("周松"));
    assert!(assigned.changes.is_empty());
    assert!(detail.history.iter().any(|event| {
        event.changes.iter().any(|change| {
            change.field == "severity" && change.label == "严重程度" && change.new == "3"
        })
    }));
    assert!(detail.history.iter().any(|event| {
        event
            .changes
            .iter()
            .any(|change| change.field == "pri" && change.label == "优先级")
    }));
}

fn assert_51267_description(detail: &BugDetail) {
    assert!(detail.title.contains("我的->创作中心"));
    assert!(detail.description.contains("在我的页面进入创作中心"));
    let img1 = "http://shendao.sharexm.cn/zentao/file-read-62828.jpeg";
    let img2 = "http://shendao.sharexm.cn/zentao/file-read-62827.png";
    assert!(detail
        .description
        .contains(&format!(r#"<img src="{img1}" />"#)));
    assert!(detail
        .description
        .contains(&format!(r#"<img src="{img2}" />"#)));
    assert!(detail.description.find(img1).unwrap() < detail.description.find(img2).unwrap());
    assert!(!detail.description.contains("!["));
    assert_eq!(
        detail
            .images
            .iter()
            .filter(|url| *url == img1 || *url == img2)
            .cloned()
            .collect::<Vec<_>>(),
        vec![img1.to_string(), img2.to_string()]
    );
    assert!(!detail.description.contains("Attachments:"));
    assert!(detail.attachments.is_empty());
}

fn history_has_action(detail: &BugDetail, at: &str, action: &str) -> bool {
    detail
        .history
        .iter()
        .any(|event| event.at == at && event.action == action)
}

#[test]
fn parse_bug_json_58688_fixture() {
    let json_detail =
        parse_bug_json(BUG_58688_URL, &read_fixture("bug_58688.json")).expect("parse json");
    let html_detail =
        parse_bug_detail(BUG_58688_URL, &read_fixture("bug_58688_real.html")).expect("parse html");

    assert_eq!(json_detail.title, html_detail.title);
    assert_eq!(
        json_detail.title,
        "【广场二期】ios-选中的状态没有展示在右上角"
    );

    let img1 = "http://shendao.sharexm.cn/zentao/file-read-73622.png";
    let img2 = "http://shendao.sharexm.cn/zentao/file-read-73623.png";
    assert!(json_detail
        .description
        .contains(&format!(r#"<img src="{img1}" />"#)));
    assert!(json_detail
        .description
        .contains(&format!(r#"<img src="{img2}" />"#)));
    assert!(
        json_detail.description.find(img1).unwrap() < json_detail.description.find(img2).unwrap()
    );
    assert!(!json_detail.description.contains("!["));
    assert_eq!(
        json_detail
            .images
            .iter()
            .filter(|url| *url == img1 || *url == img2)
            .cloned()
            .collect::<Vec<_>>(),
        vec![img1.to_string(), img2.to_string()]
    );
    assert!(html_detail
        .description
        .contains(&format!(r#"<img src="{img1}" />"#)));
    assert!(html_detail
        .description
        .contains(&format!(r#"<img src="{img2}" />"#)));
    assert!(!json_detail.description.contains("onload"));

    assert_eq!(json_detail.attachments.len(), 2);
    assert_eq!(json_detail.attachments[0].label, "安卓下拉选择状态.mp4");
    assert_eq!(
        json_detail.attachments[0].url,
        "http://shendao.sharexm.cn/zentao/data/upload/1/202608/19114642022437,m"
    );
    assert_eq!(json_detail.attachments[1].label, "ios下拉选择状态.mp4");
    assert_eq!(
        json_detail.attachments[1].url,
        "http://shendao.sharexm.cn/zentao/data/upload/1/202608/19115046021443kl"
    );
    assert_eq!(html_detail.attachments.len(), 2);

    assert!(history_has_action(
        &json_detail,
        "2026-08-19 11:46:42",
        "opened"
    ));
    assert!(history_has_action(
        &json_detail,
        "2026-08-19 11:50:46",
        "edited"
    ));
    assert_eq!(json_detail.history[0].actor, "疏娟");
}

#[test]
fn parse_bug_json_58441_fixture_keeps_assignment_and_comments() {
    let detail =
        parse_bug_json(BUG_58441_URL, &read_fixture("bug_58441.json")).expect("parse json");

    assert_eq!(
        detail.title,
        "【线上问题】会议号324594366 ，iqoo neo 9说话，苹果14听不到"
    );
    assert_eq!(detail.state, "resolved");
    assert_eq!(detail.priority, "2");
    assert_eq!(detail.opened_by, "牛威龙");
    assert_eq!(detail.resolved_by, "周松");
    assert_eq!(detail.assignee, "牛威龙");
    assert_eq!(
        detail.resolved_build,
        "1.2.17-iOS-0831（会议5.1+直播优惠券+广场二期+banner加视频）"
    );
    assert_eq!(detail.opened_date, "2026-08-13 12:10:03");
    assert_eq!(detail.resolved_date, "2026-08-18 14:57:39");

    assert_eq!(detail.history[0].action, "opened");
    assert_eq!(detail.history[0].actor, "牛威龙");
    assert_eq!(detail.history[1].action, "assigned");
    assert_eq!(detail.history[1].actor, "张涛");
    assert_eq!(detail.history[1].assignee.as_deref(), Some("周松"));
    assert!(detail.history[1].changes.is_empty());
    assert_eq!(detail.history[2].action, "edited");
    assert_eq!(detail.history[2].changes[0].field, "mailto");
    assert_eq!(detail.history[2].changes[0].label, "抄送给");
    assert_eq!(detail.history[2].changes[0].new, ",luomingkong");
    assert_eq!(detail.history[3].action, "resolved");
    assert_eq!(detail.history[3].actor, "周松");
    assert_eq!(detail.history[3].changes[0].field, "resolvedBuild");
    assert_eq!(detail.history[3].changes[0].label, "上线版本");
    assert_eq!(
        detail.history[3].changes[0].new,
        "1.2.17-iOS-0831（会议5.1+直播优惠券+广场二期+banner加视频）"
    );
    let comment = detail.history[3]
        .comment
        .as_deref()
        .expect("resolved comment");
    assert!(comment.contains("原因（背景）："));
    assert!(comment.contains("<p>"));
    assert!(!comment.contains("style="));
    assert!(!comment.contains("class="));
    assert!(!comment.contains("<span"));
    assert!(!comment.contains("onload"));
    assert_eq!(detail.history[4].action, "commented");
    let note = detail.history[4].comment.as_deref().expect("note");
    assert!(note.contains("A 选择“关闭声音”"));
    assert!(note.contains("<ol>"));
    assert!(note.contains("<li>"));
    assert!(!note.contains(".comment-content"));
    assert!(!note.contains("保存 关闭"));
    assert!(!detail.history.iter().any(|event| event
        .changes
        .iter()
        .any(|change| change.field == "assignedTo" || change.field == "resolution")));
}

#[test]
fn parse_bug_json_rejects_login_html() {
    let err = parse_bug_json(
        "http://example.com/zentao/bug-view-1.html",
        "<html><title>用户登录</title></html>",
    )
    .expect_err("login");
    assert!(err.to_string().contains("cookie"));
}

// 真实页面样本应能提取标题、关键描述和图片绝对地址。
#[test]
fn parse_real_48919_fixture() {
    let detail = parse_bug_detail(BUG_48919_URL, &read_fixture("bug_48919_real.html"))
        .expect("parse should succeed");
    assert_48919_description_and_attachments(&detail);
}

#[test]
fn parse_bug_json_48919_matches_html_fixture_behavior() {
    let html_detail =
        parse_bug_detail(BUG_48919_URL, &read_fixture("bug_48919_real.html")).expect("parse html");
    let json_detail =
        parse_bug_json(BUG_48919_URL, &read_fixture("bug_48919.json")).expect("parse json");
    assert_48919_description_and_attachments(&html_detail);
    assert_48919_description_and_attachments(&json_detail);
    assert_48919_json_history(&json_detail);
    assert_eq!(json_detail.title, html_detail.title);
}

#[test]
fn parse_embedded_zip_urls_into_attachments() {
    let markdown = concat!(
        "**[步骤]**\n\n",
        r#"**["report\_user\_url:[https://resource.sharexm.com.cn/im/log/iOS/202603/23/a.zip](https://resource.sharexm.com.cn/im/log/iOS/202603/23/a.zip)","report\_user\_url:[https://resource.sharexm.com.cn/im/log/iOS/202603/23/b.zip](https://resource.sharexm.com.cn/im/log/iOS/202603/23/b.zip)"]**"#,
        "\n\n",
        "1. 转写开始"
    );

    let (cleaned, attachments) = extract_embedded_attachments(markdown);
    assert!(!cleaned.contains("report_user_url"));
    assert!(!cleaned.contains("report\\_user\\_url"));
    assert!(cleaned.contains("1. 转写开始"));
    assert_eq!(attachments.len(), 2);
    assert_eq!(attachments[0].label, "a.zip");
    assert_eq!(attachments[1].label, "b.zip");
}

// 真实 bug 51267（正文含多图）应按顺序生成多张绝对地址图片。
#[test]
fn parse_real_51267_multiple_images_fixture() {
    let detail = parse_bug_detail(BUG_51267_URL, &read_fixture("bug_51267_real.html"))
        .expect("parse should succeed");
    assert_51267_description(&detail);
}

#[test]
fn parse_bug_json_51267_matches_html_fixture_behavior() {
    let html_detail =
        parse_bug_detail(BUG_51267_URL, &read_fixture("bug_51267_real.html")).expect("parse html");
    let json_detail =
        parse_bug_json(BUG_51267_URL, &read_fixture("bug_51267.json")).expect("parse json");
    assert_51267_description(&html_detail);
    assert_51267_description(&json_detail);
    assert!(history_has_action(
        &json_detail,
        "2026-02-24 13:58:13",
        "opened"
    ));
    assert_eq!(json_detail.title, html_detail.title);
}

// 真实 bug 48433（含长历史记录）应提取结构化历史，保留文本 diff 和备注，过滤原始 HTML 噪音。
#[test]
fn parse_real_48433_history_fixture() {
    let doc = Html::parse_document(&read_fixture("bug_48433_real.html"));
    let history = extract_history_markdown(&doc, BUG_48433_URL).expect("history should parse");
    assert!(history.contains("- 2025-11-25 16:56:18, 由 石秀秀 创建。"));
    assert!(history.contains("- 2026-03-02 17:46:56, 由 刘阳 指派给 周松 。"));
    assert!(!history.contains("修改了 所属模块"));
    assert!(!history.contains("修改了 重现步骤"));
    assert!(!history.contains("004- 测试版本："));
    assert!(!history.contains("004+ 测试版本：1.13.31"));
    assert!(history.contains("  - 备注："));
    assert!(history.contains(
        "听安卓开发-李小龙说：未避免接口调用频繁所以特意做成了 每次进入相同的聊天，都需要间隔10分钟才会去更新；"
    ));
    assert!(!history.contains("&lt;p style="));
    assert!(!history.contains("切换显示"));
}

#[test]
fn parse_bug_json_48433_matches_html_fixture_behavior() {
    let html_detail =
        parse_bug_detail(BUG_48433_URL, &read_fixture("bug_48433_real.html")).expect("parse html");
    let json_detail =
        parse_bug_json(BUG_48433_URL, &read_fixture("bug_48433.json")).expect("parse json");
    assert!(html_detail.title.contains("社群应用"));
    assert_eq!(json_detail.title, html_detail.title);
    assert!(history_has_action(
        &json_detail,
        "2025-11-25 16:56:18",
        "opened"
    ));
    let assigned = json_detail
        .history
        .iter()
        .find(|event| event.at == "2026-03-02 17:46:56" && event.action == "assigned")
        .expect("assigned");
    assert_eq!(assigned.assignee.as_deref(), Some("周松"));
    assert!(!json_detail.history.iter().any(|event| event
        .changes
        .iter()
        .any(|change| change.field == "module" || change.field == "steps")));
    assert!(json_detail.history.iter().any(|event| {
        event
            .comment
            .as_deref()
            .is_some_and(|comment| comment.contains("听安卓开发-李小龙说"))
    }));
    assert!(!json_detail
        .history
        .iter()
        .any(|event| event.comment.as_deref().is_some_and(|comment| {
            comment.contains("style=") || comment.contains("切换显示")
        })));
}

#[test]
fn parse_real_48919_history_fixture_should_hide_routine_flow_changes() {
    let doc = Html::parse_document(&read_fixture("bug_48919_real.html"));
    let history = extract_history_markdown(&doc, BUG_48919_URL).expect("history should parse");
    assert!(history.contains("2025-12-11 11:25:47, 由 石秀秀 创建。"));
    assert!(history.contains("- 2026-01-05 09:05:16, 由 刘阳 指派给 周松 。"));
    assert!(!history.contains("修改了 指派给，旧值为 \"liuyang\"，新值为 \"zhousong\"。"));
    assert!(history.contains("  - 修改了 优先级 ，旧值为 \"2\"，新值为 \"3\"。"));
}

#[test]
fn parse_history_should_hide_rich_text_diff_blocks() {
    let html = r#"
<!DOCTYPE html>
<html><body>
<div class='detail histories'>
  <ol class='histories-list'>
    <li>
      2026-03-23 10:31:59, 由 <strong>陈婕</strong> 编辑。
      <div class='history-changes'>
        修改了 <strong><i>重现步骤</i></strong>，区别为：<br />
        <blockquote class='textdiff'>007- old<br />007+ new</blockquote>
      </div>
    </li>
  </ol>
</div>
</body></html>
"#;
    let doc = Html::parse_document(html);
    let history = extract_history_markdown(&doc, "http://example.com/zentao/bug-view-1.html")
        .expect("history should parse");

    assert!(history.contains("- 2026-03-23 10:31:59, 由 陈婕 编辑。"));
    assert!(!history.contains("修改了 重现步骤"));
    assert!(!history.contains("007- old"));
    assert!(!history.contains("007+ new"));
}

// 缺失标题时必须返回明确错误，防止静默输出脏数据。
#[test]
fn parse_missing_title() {
    let html = read_fixture("bug_missing_title.html");
    let err = parse_bug_detail("http://example.com/zentao/bug-view-1.html", &html)
        .expect_err("should fail");
    assert!(err.to_string().contains("未解析到 bug 标题"));
}

// 缺失描述时必须返回明确错误。
#[test]
fn parse_missing_description() {
    let html = read_fixture("bug_missing_desc.html");
    let err = parse_bug_detail("http://example.com/zentao/bug-view-1.html", &html)
        .expect_err("should fail");
    assert!(err.to_string().contains("未解析到 bug 描述"));
}

// Markdown 图片地址应补全；绝对链接和 data URL 应保持不变。
#[test]
fn absolutize_markdown_image_urls_cases() {
    let input = [
        "![ ](/a/1.png)",
        "![x](images/2.jpg)",
        "![y](https://cdn.example.com/3.png)",
        "![d](data:image/png;base64,abc)",
    ]
    .join("\n");

    let out = absolutize_markdown_image_urls(&input, "http://example.com/zentao/bug-view-1.html")
        .expect("convert should succeed");

    assert!(out.contains("![img#1](http://example.com/a/1.png)"));
    assert!(out.contains("![x](http://example.com/zentao/images/2.jpg)"));
    assert!(out.contains("![y](https://cdn.example.com/3.png)"));
    assert!(out.contains("![d](data:image/png;base64,abc)"));
}

#[test]
fn absolutize_markdown_image_urls_with_custom_prefix_cases() {
    let input = ["![](/a/1.png)", "![ ](/a/2.png)"].join("\n");

    let out = absolutize_markdown_image_urls_with_prefix(
        &input,
        "http://example.com/zentao/bug-view-1.html",
        "history-img",
    )
    .expect("convert should succeed");

    assert!(out.contains("![history-img#1](http://example.com/a/1.png)"));
    assert!(out.contains("![history-img#2](http://example.com/a/2.png)"));
}

#[test]
fn split_markdown_image_and_following_text_cases() {
    let out = split_markdown_image_and_following_text(
        "![history-img#1](http://x/1.png)安全-上传文档开关按钮未同步回显错误",
    )
    .expect("split should succeed");

    assert_eq!(
        out,
        "![history-img#1](http://x/1.png)\n\n安全-上传文档开关按钮未同步回显错误"
    );
}

#[test]
fn parse_history_comments_should_absolutize_images_with_global_history_names() {
    let html = r#"
<!DOCTYPE html>
<html><body>
<div class='detail histories'>
  <ol class='histories-list'>
    <li>
      2026-04-08 20:42:41, 由 <strong>陈婕</strong> 添加备注。
      <div class='article-content comment'>
        <div class='comment-content'>
          <p><img src='/zentao/file-read-64873.jpeg' alt=''></p>
          <p><img src='/zentao/file-read-64907.png' alt=''></p>
        </div>
      </div>
    </li>
    <li>
      2026-04-20 17:33:36, 由 <strong>陈婕</strong> 激活。
      <div class='article-content comment'>
        <div class='comment-content'>
          <p><img src='/zentao/file-read-65380.png' alt=''></p>
        </div>
      </div>
    </li>
  </ol>
</div>
</body></html>
"#;
    let doc = Html::parse_document(html);
    let history =
        extract_history_markdown(&doc, "http://shendao.sharexm.cn/zentao/bug-view-52676.html")
            .expect("history should parse");

    assert!(
        history.contains("![history-img#1](http://shendao.sharexm.cn/zentao/file-read-64873.jpeg)")
    );
    assert!(
        history.contains("![history-img#2](http://shendao.sharexm.cn/zentao/file-read-64907.png)")
    );
    assert!(
        history.contains("![history-img#3](http://shendao.sharexm.cn/zentao/file-read-65380.png)")
    );
}

// 转义的方括号应被还原。
#[test]
fn normalize_markdown_unescapes_brackets() {
    let out = normalize_markdown(r"**\[基本信息\]**");
    assert_eq!(out, "**[基本信息]**");
}

// 连续图片应拆成逐行，便于阅读和下游渲染。
#[test]
fn split_adjacent_markdown_images_cases() {
    let out = split_adjacent_markdown_images("![a](http://x/a.png)![b](http://x/b.png)")
        .expect("split should succeed");
    assert_eq!(out, "![a](http://x/a.png)\n\n![b](http://x/b.png)");

    let normalized = split_adjacent_markdown_images("![a](http://x/a.png)\n![b](http://x/b.png)")
        .expect("split should succeed");
    assert_eq!(normalized, "![a](http://x/a.png)\n\n![b](http://x/b.png)");
}

// 形如 **[结果] ... ** 的加粗范围应仅保留在标题，图片不应被加粗。
#[test]
fn normalize_bracket_heading_bold_scope_cases() {
    let input = "**[结果]\n**![img#1](http://x/1.png)\n![img#2](http://x/2.png)**";
    let out = normalize_bracket_heading_bold_scope(input).expect("normalize should succeed");
    assert_eq!(
        out,
        "**[结果]**\n![img#1](http://x/1.png)\n![img#2](http://x/2.png)"
    );
}

#[test]
fn sanitize_html_fragment_drops_styles_and_keeps_structure() {
    let html = r#"<p class="p1" style="color:red"><strong>【基本信息】</strong><span>test</span></p><p><img onload="setImageSize(this,0)" src="/zentao/file-read-1.png" alt="" /></p>"#;
    let (got, attachments, images) =
        sanitize_html_fragment(html, "http://example.com/zentao/bug-view-1.html")
            .expect("sanitize");
    assert!(got.contains("<p>【基本信息】test</p>"));
    assert!(got.contains(r#"<img src="http://example.com/zentao/file-read-1.png" />"#));
    assert!(!got.contains("strong"));
    assert!(!got.contains("style="));
    assert!(!got.contains("onload"));
    assert!(attachments.is_empty());
    assert_eq!(
        images,
        vec!["http://example.com/zentao/file-read-1.png".to_string()]
    );
}

#[test]
fn parse_bug_json_comment_is_sanitized_html_and_images_come_from_img_tags() {
    let body = r#"{
      "status": "success",
      "data": {
        "bug": {
          "title": "t",
          "steps": "<p>desc <img src=\"/zentao/file-read-1.png\" /></p>",
          "pri": "1",
          "status": "active",
          "openedBy": "a",
          "assignedTo": "a"
        },
        "users": {"a": "甲"},
        "actions": {
          "1": {
            "id": "1",
            "date": "2026-01-01 00:00:00",
            "actor": "a",
            "action": "commented",
            "extra": "",
            "history": [],
            "comment": "<p class=\"x\" style=\"color:red\"><img onload=\"x\" src=\"/zentao/file-read-2.png\" /><br/>note</p>"
          }
        }
      }
    }"#;
    let detail = parse_bug_json("http://example.com/zentao/bug-view-1.html", body).expect("parse");
    assert!(detail
        .description
        .contains(r#"<img src="http://example.com/zentao/file-read-1.png" />"#));
    assert!(!detail.description.contains("!["));
    let comment = detail.history[0].comment.as_deref().expect("comment");
    assert!(
        comment.contains(r#"<img src="http://example.com/zentao/file-read-2.png" />"#),
        "comment={comment}"
    );
    assert!(comment.contains("<p>"));
    assert!(comment.contains("note"));
    assert!(!comment.contains("style="));
    assert!(!comment.contains("class="));
    assert!(!comment.contains("onload"));
    assert_eq!(
        detail.images,
        vec![
            "http://example.com/zentao/file-read-1.png".to_string(),
            "http://example.com/zentao/file-read-2.png".to_string()
        ]
    );
}
