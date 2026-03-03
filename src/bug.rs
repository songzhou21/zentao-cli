use anyhow::{anyhow, Context, Result};
use regex::Regex;
use scraper::{ElementRef, Html, Selector};
use std::collections::HashSet;
use url::Url;

#[derive(Debug, Clone)]
pub struct BugDetail {
    pub title: String,
    pub markdown_description: String,
    pub markdown_history: String,
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

    let attachments = extract_attachment_urls(&doc, page_url)?;
    markdown = append_attachment_links(&markdown, &attachments);
    markdown = normalize_markdown(&markdown);
    let history = extract_history_markdown(&doc)?;

    Ok(BugDetail {
        title,
        markdown_description: markdown,
        markdown_history: history,
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
    out
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

fn extract_attachment_urls(doc: &Html, page_url: &str) -> Result<Vec<String>> {
    let base = Url::parse(page_url).context("解析 bug 页面 URL 失败")?;

    let detail_sel = parse_selector("div.detail");
    let title_sel = parse_selector(".detail-title");
    let link_sel = parse_selector(".files-list a[href]");

    let mut urls = Vec::new();
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
            if seen.insert(url.clone()) {
                urls.push(url);
            }
        }
    }

    Ok(urls)
}

fn append_attachment_links(markdown: &str, attachment_urls: &[String]) -> String {
    if attachment_urls.is_empty() {
        return markdown.to_string();
    }

    let mut out = markdown.trim().to_string();
    out.push_str("\n\nAttachments:\n");
    for (idx, url) in attachment_urls.iter().enumerate() {
        out.push_str(&format!("- [attachment#{}]({})\n", idx + 1, url));
    }
    out.trim_end().to_string()
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
