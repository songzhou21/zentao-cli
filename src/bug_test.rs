use super::*;

// 复用既有 HTML fixture，避免在线依赖导致测试不稳定。
fn read_fixture(name: &str) -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("bug")
        .join(name);
    std::fs::read_to_string(path).expect("fixture should exist")
}

// 真实页面样本应能提取标题、关键描述和图片绝对地址。
#[test]
fn parse_real_48919_fixture() {
    let html = read_fixture("bug_48919_real.html");
    let detail = parse_bug_detail(
        "http://shendao.sharexm.cn/zentao/bug-view-48919.html",
        &html,
    )
    .expect("parse should succeed");

    assert!(detail.title.contains("PC登录后"));
    assert!(detail
        .markdown_description
        .contains("PC已登录进入登录确认页面"));
    assert!(!detail.markdown_description.contains(r"\["));
    assert!(!detail.markdown_description.contains(r"\]"));
    assert!(detail.markdown_description.contains("**[基本信息]**"));
    assert!(detail
        .markdown_description
        .contains("http://shendao.sharexm.cn/zentao/file-read-59561.png"));
    assert!(detail
        .markdown_description
        .contains("![img#1](http://shendao.sharexm.cn/zentao/file-read-59561.png)"));
    assert!(!detail.attachments.is_empty());
    assert!(detail.attachments[0].url.contains("/zentao/data/upload/"));
    assert!(detail
        .markdown_history
        .contains("2025-12-11 11:25:47, 由 石秀秀 创建。"));
    assert!(!detail
        .markdown_history
        .contains("修改了 指派给，旧值为 \"liuyang\"，新值为 \"zhousong\"。"));
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
    let html = read_fixture("bug_51267_real.html");
    let detail = parse_bug_detail(
        "http://shendao.sharexm.cn/zentao/bug-view-51267.html",
        &html,
    )
    .expect("parse should succeed");

    assert!(detail.title.contains("我的->创作中心"));
    assert!(detail
        .markdown_description
        .contains("在我的页面进入创作中心"));
    assert!(detail
        .markdown_description
        .contains("![img#1](http://shendao.sharexm.cn/zentao/file-read-62828.jpeg)"));
    assert!(detail
        .markdown_description
        .contains("![img#2](http://shendao.sharexm.cn/zentao/file-read-62827.png)"));
    assert!(detail.markdown_description.contains(
        "![img#1](http://shendao.sharexm.cn/zentao/file-read-62828.jpeg)\n\n![img#2](http://shendao.sharexm.cn/zentao/file-read-62827.png)"
    ));
    assert!(!detail.markdown_description.contains("Attachments:"));
    assert!(!detail.markdown_description.contains(r"\["));
    assert!(!detail.markdown_description.contains(r"\]"));
    assert!(detail.attachments.is_empty());
    assert!(detail
        .markdown_history
        .contains("2026-02-24 13:58:13, 由 孙悦 创建。"));
}

// 真实 bug 48433（含长历史记录）应提取结构化历史，保留文本 diff 和备注，过滤原始 HTML 噪音。
#[test]
fn parse_real_48433_history_fixture() {
    let html = read_fixture("bug_48433_real.html");
    let detail = parse_bug_detail(
        "http://shendao.sharexm.cn/zentao/bug-view-48433.html",
        &html,
    )
    .expect("parse should succeed");

    assert!(detail.title.contains("社群应用"));
    assert!(detail
        .markdown_history
        .contains("- 2025-11-25 16:56:18, 由 石秀秀 创建。"));
    assert!(detail
        .markdown_history
        .contains("- 2026-03-02 17:46:56, 由 刘阳 指派给 周松 。"));
    assert!(!detail.markdown_history.contains("修改了 所属模块"));
    assert!(!detail.markdown_history.contains("修改了 重现步骤"));
    assert!(!detail.markdown_history.contains("004- 测试版本："));
    assert!(!detail.markdown_history.contains("004+ 测试版本：1.13.31"));
    assert!(detail.markdown_history.contains("  - 备注："));
    assert!(detail
        .markdown_history
        .contains("听安卓开发-李小龙说：未避免接口调用频繁所以特意做成了 每次进入相同的聊天，都需要间隔10分钟才会去更新；"));
    assert!(!detail.markdown_history.contains("&lt;p style="));
    assert!(!detail.markdown_history.contains("切换显示"));
}

#[test]
fn parse_real_48919_history_fixture_should_hide_routine_flow_changes() {
    let html = read_fixture("bug_48919_real.html");
    let detail = parse_bug_detail(
        "http://shendao.sharexm.cn/zentao/bug-view-48919.html",
        &html,
    )
    .expect("parse should succeed");

    assert!(detail
        .markdown_history
        .contains("- 2026-01-05 09:05:16, 由 刘阳 指派给 周松 。"));
    assert!(!detail
        .markdown_history
        .contains("修改了 指派给，旧值为 \"liuyang\"，新值为 \"zhousong\"。"));
    assert!(detail
        .markdown_history
        .contains("  - 修改了 严重程度，旧值为 \"2\"，新值为 \"3\"。"));
    assert!(detail
        .markdown_history
        .contains("  - 修改了 优先级 ，旧值为 \"2\"，新值为 \"3\"。"));
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
    let history = extract_history_markdown(&doc).expect("history should parse");

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
    let input = "**[结果]\n![img#1](http://x/1.png)\n![img#2](http://x/2.png)**";
    let out = normalize_bracket_heading_bold_scope(input).expect("normalize should succeed");
    assert_eq!(
        out,
        "**[结果]**\n![img#1](http://x/1.png)\n![img#2](http://x/2.png)"
    );
}

#[test]
fn render_markdown_should_have_sections() {
    let md = render_markdown(
        9,
        &BugDetail {
            title: "标题".to_string(),
            markdown_description: "正文".to_string(),
            markdown_history: "- 创建".to_string(),
            attachments: vec![BugAttachment {
                label: "attachment#1".to_string(),
                url: "http://a".to_string(),
                details_markdown: None,
            }],
        },
    );
    assert!(md.contains("# Bug #9 标题"));
    assert!(md.contains("## 描述"));
    assert!(md.contains("## 历史记录"));
    assert!(md.contains("## 附件"));
    assert!(md.contains("正文"));
    assert!(md.contains("- 创建"));
    assert!(md.contains("[attachment#1](http://a)"));
    assert!(!md.contains("ZIP: `logs.zip`"));

    let empty = render_markdown(
        10,
        &BugDetail {
            title: "空描述".to_string(),
            markdown_description: "   ".to_string(),
            markdown_history: "   ".to_string(),
            attachments: vec![],
        },
    );
    assert!(empty.contains("(无)"));
}
