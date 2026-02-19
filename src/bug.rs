use anyhow::{anyhow, Context, Result};
use regex::Regex;
use reqwest::Url;
use scraper::{ElementRef, Html, Selector};
use std::collections::HashMap;

pub struct BugDetail {
    pub title: String,
    pub markdown_description: String,
}

pub fn parse_bug_detail(page_url: &str, html: &str) -> Result<BugDetail> {
    let doc = Html::parse_document(html);
    let title = extract_title(&doc).ok_or_else(|| anyhow!("未解析到 bug 标题"))?;
    let desc_node = extract_description_node(&doc).ok_or_else(|| anyhow!("未解析到 bug 描述"))?;
    let desc_html = desc_node.inner_html();

    let markdown_description = absolutize_markdown_image_urls(
        &html2md::parse_html(&desc_html).trim().to_string(),
        page_url,
    )?;
    Ok(BugDetail {
        title,
        markdown_description,
    })
}

fn extract_title(doc: &Html) -> Option<String> {
    // Zentao bug detail page title is usually in: div.page-title > span.text
    if let Ok(sel) = Selector::parse("div.page-title span.text") {
        if let Some(node) = doc.select(&sel).next() {
            if let Some(raw) = node.value().attr("title") {
                let t = raw.trim();
                if !t.is_empty() {
                    return Some(t.to_string());
                }
            }
            let text = join_text(node);
            if !text.is_empty() {
                return Some(text);
            }
        }
    }

    let selectors = [
        ".main-header .title",
        "#titlebar .heading",
        ".heading .title",
        "h1",
    ];

    for css in selectors {
        if let Ok(sel) = Selector::parse(css) {
            if let Some(node) = doc.select(&sel).next() {
                let text = join_text(node);
                if !text.is_empty() {
                    return Some(text);
                }
            }
        }
    }

    if let Ok(sel) = Selector::parse("title") {
        if let Some(node) = doc.select(&sel).next() {
            let text = join_text(node);
            if !text.is_empty() {
                return Some(text.split(" - ").next().unwrap_or(&text).trim().to_string());
            }
        }
    }

    None
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
        if let Ok(sel) = Selector::parse(css) {
            if let Some(node) = doc.select(&sel).next() {
                if !join_text(node).is_empty() || node.value().name() == "img" {
                    return Some(node);
                }
                if node.select(&Selector::parse("img").ok()?).next().is_some() {
                    return Some(node);
                }
            }
        }
    }

    None
}

fn absolutize_markdown_image_urls(markdown: &str, page_url: &str) -> Result<String> {
    let base = Url::parse(page_url).context("解析 bug 页面 URL 失败")?;
    let re = Regex::new(r"!\[([^\]]*)\]\(([^)]+)\)").context("构建图片 markdown 正则失败")?;
    let mut auto_name_counts: HashMap<String, usize> = HashMap::new();
    let replaced = re.replace_all(markdown, |caps: &regex::Captures| {
        let alt_raw = caps.get(1).map(|m| m.as_str()).unwrap_or_default();
        let raw = caps.get(2).map(|m| m.as_str()).unwrap_or_default().trim();
        if raw.is_empty() {
            return caps
                .get(0)
                .map(|m| m.as_str())
                .unwrap_or_default()
                .to_string();
        }
        match absolutize_url(&base, raw) {
            Ok(abs) => {
                let alt = if alt_raw.trim().is_empty() {
                    let image_key = derive_image_key(&abs);
                    let entry = auto_name_counts.entry(image_key.clone()).or_insert(0);
                    *entry += 1;
                    format!("img-{}-{}", image_key, *entry)
                } else {
                    alt_raw.trim().to_string()
                };
                format!("![{}]({})", alt, abs)
            }
            Err(_) => caps
                .get(0)
                .map(|m| m.as_str())
                .unwrap_or_default()
                .to_string(),
        }
    });
    Ok(replaced.into_owned())
}

fn derive_image_key(abs_url: &str) -> String {
    if let Ok(url) = Url::parse(abs_url) {
        if let Some(last) = url.path_segments().and_then(|mut s| s.next_back()) {
            let name = last
                .split('?')
                .next()
                .unwrap_or(last)
                .split('#')
                .next()
                .unwrap_or(last)
                .to_string();
            if !name.is_empty() {
                let stem = name.rsplit_once('.').map(|v| v.0).unwrap_or(name.as_str());
                if let Some(m) = Regex::new(r"(\\d+)")
                    .ok()
                    .and_then(|re| re.find_iter(stem).last())
                {
                    return m.as_str().to_string();
                }
                if !stem.is_empty() {
                    return stem.to_string();
                }
            }
        }
    }
    "unknown".to_string()
}

fn absolutize_url(base: &Url, raw: &str) -> Result<String> {
    if raw.starts_with("data:") || raw.starts_with('#') {
        return Ok(raw.to_string());
    }
    if let Ok(u) = Url::parse(raw) {
        return Ok(u.to_string());
    }
    base.join(raw)
        .with_context(|| format!("拼接图片地址失败: {}", raw))
        .map(|u| u.to_string())
}

fn join_text(node: ElementRef<'_>) -> String {
    node.text()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn render_markdown(id: u64, detail: &BugDetail) -> String {
    let mut out = String::new();
    out.push_str(&format!("# Bug #{} {}\n\n", id, detail.title));
    out.push_str("## 描述\n\n");
    if detail.markdown_description.is_empty() {
        out.push_str("(无)\n\n");
    } else {
        out.push_str(&detail.markdown_description);
        out.push_str("\n\n");
    }

    out
}
