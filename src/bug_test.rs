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
    let detail = parse_bug_detail("http://shendao.sharexm.cn/zentao/bug-view-48919.html", &html)
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
}

// 真实 bug 51267（正文含多图）应按顺序生成多张绝对地址图片。
#[test]
fn parse_real_51267_multiple_images_fixture() {
    let html = read_fixture("bug_51267_real.html");
    let detail = parse_bug_detail("http://shendao.sharexm.cn/zentao/bug-view-51267.html", &html)
        .expect("parse should succeed");

    assert!(detail.title.contains("我的->创作中心"));
    assert!(detail.markdown_description.contains("在我的页面进入创作中心"));
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

// 附件列表应追加编号链接；无附件时保持原文。
#[test]
fn append_attachment_links_cases() {
    let out = append_attachment_links("正文", &["http://a".to_string(), "http://b".to_string()]);
    assert!(out.contains("正文"));
    assert!(out.contains("Attachments:"));
    assert!(out.contains("- [attachment#1](http://a)"));
    assert!(out.contains("- [attachment#2](http://b)"));

    let unchanged = append_attachment_links("正文", &[]);
    assert_eq!(unchanged, "正文");
}

// 渲染结果应包含固定结构，避免下游消费格式漂移。
#[test]
fn render_markdown_should_have_sections() {
    let md = render_markdown(
        9,
        &BugDetail {
            title: "标题".to_string(),
            markdown_description: "正文".to_string(),
        },
    );
    assert!(md.contains("# Bug #9 标题"));
    assert!(md.contains("## 描述"));
    assert!(md.contains("正文"));

    let empty = render_markdown(
        10,
        &BugDetail {
            title: "空描述".to_string(),
            markdown_description: "   ".to_string(),
        },
    );
    assert!(empty.contains("(无)"));
}
