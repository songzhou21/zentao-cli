use anyhow::{anyhow, Context, Result};
use regex::Regex;
use scraper::{ElementRef, Html, Selector};
use std::collections::HashSet;
use url::Url;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BugAttachment {
    pub label: String,
    pub url: String,
    pub details_markdown: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BugDetail {
    pub title: String,
    pub markdown_description: String,
    pub markdown_history: String,
    pub attachments: Vec<BugAttachment>,
}

pub fn parse_bug_detail(page_url: &str, html: &str) -> Result<BugDetail> {
    let doc = Html::parse_document(html);

    let title = extract_title(&doc).ok_or_else(|| anyhow!("未解析到 bug 标题"))?;
    let desc_node = extract_description_node(&doc).ok_or_else(|| anyhow!("未解析到 bug 描述"))?;

    let desc_html = desc_node.inner_html();
    let mut markdown = html2md::parse_html(&desc_html).trim().to_string();
    markdown = absolutize_markdown_image_urls(&markdown, page_url)?;
    markdown = split_adjacent_markdown_images(&markdown)?;
    markdown = normalize_bracket_heading_bold_scope(&markdown)?;
    markdown = normalize_markdown(&markdown);
    let (markdown, embedded_attachments) = extract_embedded_attachments(&markdown);
    let history = extract_history_markdown(&doc)?;
    let attachments = merge_attachments(extract_attachments(&doc, page_url)?, embedded_attachments);

    Ok(BugDetail {
        title,
        markdown_description: markdown,
        markdown_history: history,
        attachments,
    })
}

fn extract_title(doc: &Html) -> Option<String> {
    let primary = parse_selector("div.page-title span.text");
    if let Some(node) = doc.select(&primary).next() {
        if let Some(attr) = node.value().attr("title") {
            let s = attr.trim();
            if !s.is_empty() {
                return Some(s.to_string());
            }
        }
        let txt = node.text().collect::<String>().trim().to_string();
        if !txt.is_empty() {
            return Some(txt);
        }
    }

    let fallbacks = [
        ".main-header .title",
        "#titlebar .heading",
        ".heading .title",
        "h1",
    ];
    for css in fallbacks {
        let sel = parse_selector(css);
        if let Some(node) = doc.select(&sel).next() {
            let txt = node.text().collect::<String>().trim().to_string();
            if !txt.is_empty() {
                return Some(txt);
            }
        }
    }

    let title_sel = parse_selector("title");
    doc.select(&title_sel).next().and_then(|node| {
        let txt = node.text().collect::<String>().trim().to_string();
        if txt.is_empty() {
            None
        } else {
            Some(
                txt.split(" - ")
                    .next()
                    .map(|s| s.trim().to_string())
                    .unwrap_or_default(),
            )
        }
    })
}

fn extract_description_node<'a>(doc: &'a Html) -> Option<ElementRef<'a>> {
    let selectors = [
        "#legendLife + .detail-content",
        "#legendLife + .content",
        ".detail-content",
        ".article-content",
        "#legendLife",
    ];

    for css in selectors {
        let sel = parse_selector(css);
        if let Some(node) = doc.select(&sel).next() {
            let text = node.text().collect::<String>();
            let has_img = node.select(&parse_selector("img")).next().is_some();
            if !text.trim().is_empty() || has_img {
                return Some(node);
            }
        }
    }
    None
}

pub fn render_markdown(id: u64, detail: &BugDetail) -> String {
    let mut out = String::new();
    out.push_str(&format!("# Bug #{} {}\n\n", id, detail.title));
    out.push_str("## 描述\n\n");
    if detail.markdown_description.trim().is_empty() {
        out.push_str("(无)\n\n");
    } else {
        out.push_str(&detail.markdown_description);
        out.push_str("\n\n");
    }
    out.push_str("## 历史记录\n\n");
    if detail.markdown_history.trim().is_empty() {
        out.push_str("(无)\n\n");
    } else {
        out.push_str(&detail.markdown_history);
        out.push_str("\n\n");
    }
    out.push_str("## 附件\n\n");
    if detail.attachments.is_empty() {
        out.push_str("(无)\n");
    } else {
        out.push_str(&render_attachments(&detail.attachments));
        out.push('\n');
    }
    out
}

fn render_attachments(attachments: &[BugAttachment]) -> String {
    let mut out = String::new();
    for attachment in attachments {
        out.push_str(&format!("- [{}]({})\n", attachment.label, attachment.url));
        if let Some(details) = attachment.details_markdown.as_deref() {
            out.push_str(&indent_block(details, "  "));
            out.push('\n');
        }
    }
    out.trim_end().to_string()
}

fn indent_block(input: &str, prefix: &str) -> String {
    input
        .lines()
        .map(|line| format!("{prefix}{line}"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn absolutize_markdown_image_urls(markdown: &str, page_url: &str) -> Result<String> {
    let base = Url::parse(page_url).context("解析 bug 页面 URL 失败")?;
    let re = Regex::new(r"!\[([^\]]*)\]\(([^)]+)\)").context("构建图片正则失败")?;

    let mut idx = 0usize;
    let mut out = String::with_capacity(markdown.len() + 64);
    let mut last = 0usize;

    for cap in re.captures_iter(markdown) {
        let m = cap.get(0).expect("full match must exist");
        out.push_str(&markdown[last..m.start()]);

        let alt_raw = cap.get(1).map(|m| m.as_str().trim()).unwrap_or("");
        let raw = cap.get(2).map(|m| m.as_str().trim()).unwrap_or("");

        if raw.is_empty() {
            out.push_str(m.as_str());
            last = m.end();
            continue;
        }

        let abs = absolutize_url(&base, raw).unwrap_or_else(|_| raw.to_string());
        let alt = if alt_raw.is_empty() {
            idx += 1;
            format!("img#{idx}")
        } else {
            alt_raw.to_string()
        };

        out.push_str(&format!("![{}]({})", alt, abs));
        last = m.end();
    }

    out.push_str(&markdown[last..]);
    Ok(out)
}

fn normalize_markdown(markdown: &str) -> String {
    markdown.replace(r"\[", "[").replace(r"\]", "]")
}

fn split_adjacent_markdown_images(markdown: &str) -> Result<String> {
    let re = Regex::new(r"\)\s*!\[").context("构建连续图片分隔正则失败")?;
    Ok(re.replace_all(markdown, ")\n\n![").to_string())
}

fn normalize_bracket_heading_bold_scope(markdown: &str) -> Result<String> {
    let open_re = Regex::new(r"\*\*(\[[^\]]+\])\s*\n").context("构建加粗标题起始正则失败")?;
    let mut out = open_re.replace_all(markdown, "**$1**\n").to_string();

    // 清理因原始转换导致附着在图片后的尾部加粗标记。
    let close_re = Regex::new(r"(!\[[^\]]*\]\([^)]+\))\*\*").context("构建加粗标题结束正则失败")?;
    out = close_re.replace_all(&out, "$1").to_string();
    Ok(out)
}

fn extract_embedded_attachments(markdown: &str) -> (String, Vec<BugAttachment>) {
    let markdown_link_re =
        Regex::new(r#"\((https?://[^)\s]+\.zip)\)"#).expect("valid markdown zip url regex");
    let bare_url_re = Regex::new(r#"https?://[^\s)\]"]+\.zip"#).expect("valid zip url regex");
    let mut seen = HashSet::new();
    let mut attachments = Vec::new();
    let mut kept_lines = Vec::new();

    for line in markdown.lines() {
        let mut line_has_zip = false;

        for caps in markdown_link_re.captures_iter(line) {
            line_has_zip = true;
            let url = caps.get(1).map(|m| m.as_str()).unwrap_or_default();
            push_embedded_attachment(url, &mut seen, &mut attachments);
        }

        if !line_has_zip {
            for m in bare_url_re.find_iter(line) {
                line_has_zip = true;
                let url = m.as_str().trim_end_matches([',', '"']);
                push_embedded_attachment(url, &mut seen, &mut attachments);
            }
        }

        if line_has_zip && (line.contains("report_user_url") || line.contains("report\\_user\\_url")) {
            continue;
        }
        kept_lines.push(line);
    }

    (kept_lines.join("\n").trim().to_string(), attachments)
}

fn push_embedded_attachment(url: &str, seen: &mut HashSet<String>, attachments: &mut Vec<BugAttachment>) {
    if url.is_empty() {
        return;
    }
    let normalized = url.replace(r#"\_"#, "_");
    if !seen.insert(normalized.clone()) {
        return;
    }
    let label = normalized
        .rsplit('/')
        .next()
        .filter(|s| !s.is_empty())
        .unwrap_or("attachment.zip")
        .to_string();
    attachments.push(BugAttachment {
        label,
        url: normalized,
        details_markdown: None,
    });
}

fn merge_attachments(mut primary: Vec<BugAttachment>, extra: Vec<BugAttachment>) -> Vec<BugAttachment> {
    let mut seen: HashSet<String> = primary.iter().map(|item| item.url.clone()).collect();
    for attachment in extra {
        if seen.insert(attachment.url.clone()) {
            primary.push(attachment);
        }
    }
    primary
}

fn extract_attachments(doc: &Html, page_url: &str) -> Result<Vec<BugAttachment>> {
    let base = Url::parse(page_url).context("解析 bug 页面 URL 失败")?;

    let detail_sel = parse_selector("div.detail");
    let title_sel = parse_selector(".detail-title");
    let link_sel = parse_selector(".files-list a[href]");

    let mut attachments = Vec::new();
    let mut seen = HashSet::new();

    for detail in doc.select(&detail_sel) {
        let title = detail
            .select(&title_sel)
            .next()
            .map(|n| n.text().collect::<String>())
            .unwrap_or_default();

        if !title.contains("附件") {
            continue;
        }

        for a in detail.select(&link_sel) {
            let href = match a.value().attr("href") {
                Some(v) => v.trim(),
                None => continue,
            };
            if href.is_empty() {
                continue;
            }
            let lower = href.to_ascii_lowercase();
            if lower.starts_with("javascript:") || href.contains("/file-edit-") {
                continue;
            }

            let url = match absolutize_url(&base, href) {
                Ok(v) => v,
                Err(_) => continue,
            };
            if !seen.insert(url.clone()) {
                continue;
            }

            let label = normalize_text_whitespace(&a.text().collect::<String>());
            attachments.push(BugAttachment {
                label: if label.is_empty() {
                    format!("attachment#{}", attachments.len() + 1)
                } else {
                    label
                },
                url,
                details_markdown: None,
            });
        }
    }

    Ok(attachments)
}

fn extract_history_markdown(doc: &Html) -> Result<String> {
    let list_sel = parse_selector("div.detail.histories ol.histories-list > li");

    let mut lines = Vec::new();
    for li in doc.select(&list_sel) {
        let raw = li.text().collect::<Vec<_>>().join(" ");
        let normalized = normalize_text_whitespace(&raw);
        if !normalized.is_empty() {
            lines.push(format!("- {}", normalized));
        }
    }

    Ok(lines.join("\n"))
}

fn normalize_text_whitespace(input: &str) -> String {
    input.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn absolutize_url(base: &Url, raw: &str) -> Result<String> {
    if raw.starts_with("data:") || raw.starts_with('#') {
        return Ok(raw.to_string());
    }
    let url = Url::parse(raw).or_else(|_| base.join(raw))?;
    Ok(url.to_string())
}

fn parse_selector(css: &str) -> Selector {
    Selector::parse(css).expect("valid selector")
}

#[cfg(test)]
#[path = "bug_test.rs"]
mod tests;
