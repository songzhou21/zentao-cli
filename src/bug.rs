use anyhow::{anyhow, Context, Result};
#[cfg(test)]
use regex::Regex;
#[cfg(test)]
use scraper::Selector;
use scraper::{node::Node, ElementRef, Html};
use serde_json::Value;
use std::collections::HashSet;
use url::Url;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BugAttachment {
    pub label: String,
    pub url: String,
    pub details_markdown: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BugDetail {
    pub title: String,
    pub description: String,
    pub history: Vec<HistoryEvent>,
    pub images: Vec<String>,
    pub attachments: Vec<BugAttachment>,
    pub priority: String,
    pub state: String,
    pub opened_by: String,
    pub resolved_by: String,
    pub assignee: String,
    pub resolved_build: String,
    pub opened_date: String,
    pub resolved_date: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryEvent {
    pub at: String,
    pub action: String,
    pub actor: String,
    pub assignee: Option<String>,
    pub changes: Vec<HistoryChange>,
    pub comment: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryChange {
    pub field: String,
    pub label: String,
    pub old: String,
    pub new: String,
}

#[cfg(test)]
#[derive(Debug, Clone, PartialEq, Eq)]
struct HistoryEntry {
    header: String,
    details: Vec<HistoryDetail>,
}

#[cfg(test)]
#[derive(Debug, Clone, PartialEq, Eq)]
enum HistoryDetail {
    Change(String),
    Comment(String),
}

#[cfg(test)]
pub fn parse_bug_detail(page_url: &str, html: &str) -> Result<BugDetail> {
    let doc = Html::parse_document(html);

    let title = extract_title(&doc).ok_or_else(|| anyhow!("未解析到 bug 标题"))?;
    let desc_node = extract_description_node(&doc).ok_or_else(|| anyhow!("未解析到 bug 描述"))?;

    let desc_html = desc_node.inner_html();
    let (description, embedded_attachments, images) = sanitize_html_fragment(&desc_html, page_url)?;
    let attachments = merge_attachments(extract_attachments(&doc, page_url)?, embedded_attachments);

    Ok(BugDetail {
        title,
        description,
        images,
        attachments,
        ..BugDetail::default()
    })
}

/// Parse Zentao's bug-view `.json` payload (`{status,data,md5}`, `data` may be a string).
pub fn parse_bug_json(page_url: &str, body: &str) -> Result<BugDetail> {
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return Err(anyhow!("获取 bug 详情失败: 页面内容为空"));
    }
    if looks_like_login_html(trimmed) {
        return Err(anyhow!("获取 bug 详情失败: cookie 无效或已过期"));
    }

    let root: Value = serde_json::from_str(trimmed).map_err(|_| {
        if trimmed.contains("登录") {
            anyhow!("获取 bug 详情失败: cookie 无效或已过期")
        } else {
            anyhow!("获取 bug 详情失败: 无法解析详情 JSON")
        }
    })?;
    let data = unwrap_bug_payload(root)?;
    let bug = data
        .get("bug")
        .ok_or_else(|| anyhow!("未解析到 bug 标题"))?;
    let title = decode_html_entities(&json_text(bug.get("title")));
    if title.is_empty() {
        return Err(anyhow!("未解析到 bug 标题"));
    }

    let users = data.get("users").and_then(Value::as_object);
    let builds = data.get("builds").and_then(Value::as_object);
    let steps = json_text(bug.get("steps"));
    let mut images = Vec::new();
    let mut image_seen = HashSet::new();
    let (description, embedded_attachments, desc_images) =
        sanitize_html_fragment(&steps, page_url)?;
    extend_unique(&mut images, &mut image_seen, desc_images);
    let attachments = merge_attachments(
        extract_json_files(bug.get("files"), page_url)?,
        embedded_attachments,
    );
    let history = extract_json_history(
        data.get("actions"),
        users,
        builds,
        page_url,
        &mut images,
        &mut image_seen,
    )?;

    Ok(BugDetail {
        title,
        description,
        history,
        images,
        attachments,
        priority: json_text(bug.get("pri")),
        state: canonical_bug_state(&json_text(bug.get("status"))).to_string(),
        opened_by: map_user_account(users, &json_text(bug.get("openedBy"))),
        resolved_by: map_user_account(users, &json_text(bug.get("resolvedBy"))),
        assignee: map_user_account(users, &json_text(bug.get("assignedTo"))),
        resolved_build: map_build(builds, &json_text(bug.get("resolvedBuild"))),
        opened_date: json_text(bug.get("openedDate")),
        resolved_date: json_text(bug.get("resolvedDate")),
    })
}

pub(crate) fn canonical_bug_state(raw: &str) -> &'static str {
    match raw.trim() {
        "激活" | "active" => "active",
        "已解决" | "resolved" => "resolved",
        "已关闭" | "closed" => "closed",
        _ => "unknown",
    }
}

fn looks_like_login_html(body: &str) -> bool {
    let head = body.get(..800).unwrap_or(body);
    head.contains("<title>") && head.contains("登录")
}

fn unwrap_bug_payload(root: Value) -> Result<Value> {
    if let Some(data) = root.get("data") {
        if let Some(raw) = data.as_str() {
            return serde_json::from_str(raw).context("获取 bug 详情失败: 无法解析详情 JSON data");
        }
        if data.is_object() {
            return Ok(data.clone());
        }
    }
    if root.get("bug").is_some() {
        return Ok(root);
    }
    Err(anyhow!("获取 bug 详情失败: 详情 JSON 缺少 bug"))
}

fn json_text(value: Option<&Value>) -> String {
    match value {
        Some(Value::String(s)) => s.trim().to_string(),
        Some(Value::Number(n)) => n.to_string(),
        Some(Value::Bool(b)) => b.to_string(),
        _ => String::new(),
    }
}

fn json_id(value: Option<&Value>) -> Option<u64> {
    let value = value?;
    if let Some(n) = value.as_u64() {
        return Some(n);
    }
    if let Some(n) = value.as_i64() {
        return u64::try_from(n).ok();
    }
    value.as_str()?.trim().parse().ok()
}

fn decode_html_entities(input: &str) -> String {
    input
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#039;", "'")
        .replace("&nbsp;", " ")
}

fn map_user_account(users: Option<&serde_json::Map<String, Value>>, account: &str) -> String {
    let account = account.trim();
    if account.is_empty() || account == "0" {
        return String::new();
    }
    users
        .and_then(|map| map.get(account))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .unwrap_or(account)
        .to_string()
}

fn map_build(builds: Option<&serde_json::Map<String, Value>>, id: &str) -> String {
    let id = id.trim();
    if id.is_empty() {
        return String::new();
    }
    builds
        .and_then(|map| map.get(id))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .unwrap_or(id)
        .to_string()
}

/// Keep document structure; drop presentation tags and attributes.
/// Images are collected from `<img src>` while rewriting the fragment.
fn sanitize_html_fragment(
    html: &str,
    page_url: &str,
) -> Result<(String, Vec<BugAttachment>, Vec<String>)> {
    if html.trim().is_empty() {
        return Ok((String::new(), Vec::new(), Vec::new()));
    }
    let base = Url::parse(page_url).context("解析 bug 页面 URL 失败")?;
    let fragment = Html::parse_fragment(html);
    let mut out = String::new();
    let mut attachments = Vec::new();
    let mut images = Vec::new();
    let mut seen = HashSet::new();
    let mut image_seen = HashSet::new();
    write_children(
        fragment.root_element(),
        &base,
        &mut out,
        &mut attachments,
        &mut seen,
        &mut images,
        &mut image_seen,
    );
    Ok((out.trim().to_string(), attachments, images))
}

fn extend_unique(dest: &mut Vec<String>, seen: &mut HashSet<String>, incoming: Vec<String>) {
    for url in incoming {
        if seen.insert(url.clone()) {
            dest.push(url);
        }
    }
}

fn write_children(
    element: ElementRef<'_>,
    base: &Url,
    out: &mut String,
    attachments: &mut Vec<BugAttachment>,
    seen: &mut HashSet<String>,
    images: &mut Vec<String>,
    image_seen: &mut HashSet<String>,
) {
    for child in element.children() {
        match child.value() {
            Node::Text(text) => out.push_str(&escape_html_text(text)),
            Node::Element(_) => {
                if let Some(node) = ElementRef::wrap(child) {
                    write_element(node, base, out, attachments, seen, images, image_seen);
                }
            }
            _ => {}
        }
    }
}

fn write_element(
    element: ElementRef<'_>,
    base: &Url,
    out: &mut String,
    attachments: &mut Vec<BugAttachment>,
    seen: &mut HashSet<String>,
    images: &mut Vec<String>,
    image_seen: &mut HashSet<String>,
) {
    let name = element.value().name();
    match name {
        "script" | "style" | "button" => {}
        "br" => out.push_str("<br />"),
        "img" => {
            if let Some(src) = element.value().attr("src") {
                if let Ok(abs) = absolutize_url(base, src) {
                    if !abs.is_empty() {
                        if image_seen.insert(abs.clone()) {
                            images.push(abs.clone());
                        }
                        out.push_str(&format!(r#"<img src="{}" />"#, escape_html_attr(&abs)));
                    }
                }
            }
        }
        "a" => {
            let href = element.value().attr("href").unwrap_or("").trim();
            if href.is_empty() || href.to_ascii_lowercase().starts_with("javascript:") {
                write_children(element, base, out, attachments, seen, images, image_seen);
                return;
            }
            let abs = absolutize_url(base, href).unwrap_or_else(|_| href.to_string());
            if abs.to_ascii_lowercase().ends_with(".zip") {
                push_embedded_attachment(&abs, seen, attachments);
            }
            out.push_str(&format!(r#"<a href="{}">"#, escape_html_attr(&abs)));
            write_children(element, base, out, attachments, seen, images, image_seen);
            out.push_str("</a>");
        }
        "p" | "ol" | "ul" | "li" => {
            out.push('<');
            out.push_str(name);
            out.push('>');
            write_children(element, base, out, attachments, seen, images, image_seen);
            out.push_str("</");
            out.push_str(name);
            out.push('>');
        }
        _ => write_children(element, base, out, attachments, seen, images, image_seen),
    }
}

fn escape_html_text(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn escape_html_attr(input: &str) -> String {
    escape_html_text(input).replace('"', "&quot;")
}

fn extract_json_files(files: Option<&Value>, page_url: &str) -> Result<Vec<BugAttachment>> {
    let base = Url::parse(page_url).context("解析 bug 页面 URL 失败")?;
    let mut items: Vec<(u64, &Value)> = match files {
        Some(Value::Object(map)) => map
            .values()
            .map(|file| (json_id(file.get("id")).unwrap_or(0), file))
            .collect(),
        Some(Value::Array(list)) => list
            .iter()
            .map(|file| (json_id(file.get("id")).unwrap_or(0), file))
            .collect(),
        _ => return Ok(Vec::new()),
    };
    items.sort_by_key(|(id, _)| *id);

    let mut attachments = Vec::new();
    let mut seen = HashSet::new();
    for (_, file) in items {
        if json_text(file.get("deleted")) == "1" {
            continue;
        }
        let web_path = json_text(file.get("webPath"));
        if web_path.is_empty() {
            continue;
        }
        let url = match absolutize_url(&base, &web_path) {
            Ok(url) => url,
            Err(_) => continue,
        };
        if !seen.insert(url.clone()) {
            continue;
        }
        let title = json_text(file.get("title"));
        let pathname = json_text(file.get("pathname"));
        let label = if !title.is_empty() {
            title
        } else if !pathname.is_empty() {
            pathname.rsplit('/').next().unwrap_or(&pathname).to_string()
        } else {
            format!("attachment#{}", attachments.len() + 1)
        };
        attachments.push(BugAttachment {
            label,
            url,
            details_markdown: None,
        });
    }
    Ok(attachments)
}

fn extract_json_history(
    actions: Option<&Value>,
    users: Option<&serde_json::Map<String, Value>>,
    builds: Option<&serde_json::Map<String, Value>>,
    page_url: &str,
    images: &mut Vec<String>,
    image_seen: &mut HashSet<String>,
) -> Result<Vec<HistoryEvent>> {
    let mut items: Vec<&Value> = match actions {
        Some(Value::Object(map)) => map.values().collect(),
        Some(Value::Array(list)) => list.iter().collect(),
        _ => return Ok(Vec::new()),
    };
    items.sort_by_key(|action| {
        (
            json_text(action.get("date")),
            json_id(action.get("id")).unwrap_or(0),
        )
    });

    let mut events = Vec::new();
    for action in items {
        if let Some(event) =
            json_history_event(action, users, builds, page_url, images, image_seen)?
        {
            events.push(event);
        }
    }
    Ok(events)
}

fn json_history_event(
    action: &Value,
    users: Option<&serde_json::Map<String, Value>>,
    builds: Option<&serde_json::Map<String, Value>>,
    page_url: &str,
    images: &mut Vec<String>,
    image_seen: &mut HashSet<String>,
) -> Result<Option<HistoryEvent>> {
    let at = json_text(action.get("date"));
    let actor = map_user_account(users, &json_text(action.get("actor")));
    if at.is_empty() || actor.is_empty() {
        return Ok(None);
    }
    let kind = json_text(action.get("action"));
    let extra = json_text(action.get("extra"));
    let assignee = if kind == "assigned" {
        let name = map_user_account(users, &extra);
        if name.is_empty() {
            None
        } else {
            Some(name)
        }
    } else {
        None
    };
    let changes = json_history_changes(action.get("history"), builds);
    let comment_raw = json_text(action.get("comment"));
    let comment = if comment_raw.is_empty() {
        None
    } else {
        let (html, _, comment_images) = sanitize_html_fragment(&comment_raw, page_url)?;
        extend_unique(images, image_seen, comment_images);
        if html.is_empty() {
            None
        } else {
            Some(html)
        }
    };
    Ok(Some(HistoryEvent {
        at,
        action: kind,
        actor,
        assignee,
        changes,
        comment,
    }))
}

fn json_history_changes(
    history: Option<&Value>,
    builds: Option<&serde_json::Map<String, Value>>,
) -> Vec<HistoryChange> {
    let Some(Value::Array(items)) = history else {
        return Vec::new();
    };
    let mut changes = Vec::new();
    for item in items {
        let field = json_text(item.get("field"));
        if field.is_empty() || is_hidden_history_field(&field) {
            continue;
        }
        let Some(label) = history_field_label(&field) else {
            continue;
        };
        let mut old = json_text(item.get("old"));
        let mut new = json_text(item.get("new"));
        if field == "resolvedBuild" || field == "openedBuild" {
            old = map_build(builds, &old);
            new = map_build(builds, &new);
        }
        changes.push(HistoryChange {
            field,
            label: label.to_string(),
            old,
            new,
        });
    }
    changes
}

fn is_hidden_history_field(field: &str) -> bool {
    matches!(
        field,
        "resolution"
            | "resolvedDate"
            | "assignedTo"
            | "consumed"
            | "status"
            | "confirmed"
            | "resolvedBy"
            | "module"
            | "steps"
            | "activatedDate"
            | "activatedCount"
            | "closedDate"
            | "closedBy"
    )
}

fn history_field_label(field: &str) -> Option<&'static str> {
    match field {
        "mailto" => Some("抄送给"),
        "resolvedBuild" => Some("上线版本"),
        "severity" => Some("严重程度"),
        "pri" => Some("优先级"),
        "openedBuild" => Some("影响版本"),
        _ => None,
    }
}

#[cfg(test)]
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

#[cfg(test)]
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

#[cfg(test)]
fn indent_block(input: &str, prefix: &str) -> String {
    input
        .lines()
        .map(|line| format!("{prefix}{line}"))
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
fn absolutize_markdown_image_urls(markdown: &str, page_url: &str) -> Result<String> {
    absolutize_markdown_image_urls_with_prefix(markdown, page_url, "img")
}

#[cfg(test)]
fn absolutize_markdown_image_urls_with_prefix(
    markdown: &str,
    page_url: &str,
    default_alt_prefix: &str,
) -> Result<String> {
    absolutize_markdown_image_urls_with_prefix_and_start(markdown, page_url, default_alt_prefix, 0)
        .map(|(markdown, _)| markdown)
}

#[cfg(test)]
fn absolutize_markdown_image_urls_with_prefix_and_start(
    markdown: &str,
    page_url: &str,
    default_alt_prefix: &str,
    start_idx: usize,
) -> Result<(String, usize)> {
    let base = Url::parse(page_url).context("解析 bug 页面 URL 失败")?;
    let re = Regex::new(r"!\[([^\]]*)\]\(([^)]+)\)").context("构建图片正则失败")?;

    let mut idx = start_idx;
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
            format!("{default_alt_prefix}#{idx}")
        } else {
            alt_raw.to_string()
        };

        out.push_str(&format!("![{}]({})", alt, abs));
        last = m.end();
    }

    out.push_str(&markdown[last..]);
    Ok((out, idx))
}

#[cfg(test)]
fn normalize_markdown(markdown: &str) -> String {
    markdown.replace(r"\[", "[").replace(r"\]", "]")
}

#[cfg(test)]
fn split_adjacent_markdown_images(markdown: &str) -> Result<String> {
    let re = Regex::new(r"\)\s*!\[").context("构建连续图片分隔正则失败")?;
    Ok(re.replace_all(markdown, ")\n\n![").to_string())
}

#[cfg(test)]
fn split_markdown_image_and_following_text(markdown: &str) -> Result<String> {
    let re = Regex::new(r"!\[[^\]]*\]\([^)]+\)").context("构建图片与后续文本分隔正则失败")?;
    let mut out = String::with_capacity(markdown.len() + 16);
    let mut last = 0usize;

    for m in re.find_iter(markdown) {
        out.push_str(&markdown[last..m.start()]);
        out.push_str(m.as_str());

        let next = markdown[m.end()..].chars().next();
        if matches!(next, Some(ch) if !ch.is_whitespace()) {
            out.push_str("\n\n");
        }

        last = m.end();
    }

    out.push_str(&markdown[last..]);
    Ok(out)
}

#[cfg(test)]
fn normalize_bracket_heading_bold_scope(markdown: &str) -> Result<String> {
    let open_re = Regex::new(r"\*\*(\[[^\]]+\])\s*\n").context("构建加粗标题起始正则失败")?;
    let mut out = open_re.replace_all(markdown, "**$1**\n").to_string();

    // 清理因原始转换导致附着在图片前后的残留加粗标记。
    let leading_re =
        Regex::new(r"\*\*(!\[[^\]]*\]\([^)]+\))").context("构建图片前置加粗正则失败")?;
    out = leading_re.replace_all(&out, "$1").to_string();

    let trailing_re =
        Regex::new(r"(!\[[^\]]*\]\([^)]+\))\*\*").context("构建图片后置加粗正则失败")?;
    out = trailing_re.replace_all(&out, "$1").to_string();
    Ok(out)
}

#[cfg(test)]
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

        if line_has_zip
            && (line.contains("report_user_url") || line.contains("report\\_user\\_url"))
        {
            continue;
        }
        kept_lines.push(line);
    }

    (kept_lines.join("\n").trim().to_string(), attachments)
}

fn push_embedded_attachment(
    url: &str,
    seen: &mut HashSet<String>,
    attachments: &mut Vec<BugAttachment>,
) {
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

fn merge_attachments(
    mut primary: Vec<BugAttachment>,
    extra: Vec<BugAttachment>,
) -> Vec<BugAttachment> {
    let mut seen: HashSet<String> = primary.iter().map(|item| item.url.clone()).collect();
    for attachment in extra {
        if seen.insert(attachment.url.clone()) {
            primary.push(attachment);
        }
    }
    primary
}

#[cfg(test)]
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

#[cfg(test)]
fn extract_history_markdown(doc: &Html, page_url: &str) -> Result<String> {
    let list_sel = parse_selector("div.detail.histories ol.histories-list > li");

    let mut lines = Vec::new();
    let mut image_idx = 0usize;
    for li in doc.select(&list_sel) {
        let entry = extract_history_entry(&li, page_url, &mut image_idx)?;
        if entry.header.is_empty() {
            continue;
        }
        lines.push(render_history_entry(&entry));
    }

    Ok(lines.join("\n"))
}

#[cfg(test)]
fn extract_history_entry(
    li: &ElementRef<'_>,
    page_url: &str,
    image_idx: &mut usize,
) -> Result<HistoryEntry> {
    let header = extract_history_header(li);
    let mut details = extract_history_changes(li)?;
    details.extend(extract_history_comments(li, page_url, image_idx)?);
    Ok(HistoryEntry { header, details })
}

#[cfg(test)]
fn extract_history_header(li: &ElementRef<'_>) -> String {
    let mut parts = Vec::new();

    for child in li.children() {
        match child.value() {
            Node::Text(text) => {
                let normalized = normalize_text_whitespace(text);
                if !normalized.is_empty() {
                    parts.push(normalized);
                }
            }
            Node::Element(element) => {
                let name = element.name();
                if matches!(name, "button" | "div" | "blockquote") {
                    continue;
                }
                if let Some(child_ref) = ElementRef::wrap(child) {
                    let normalized =
                        normalize_text_whitespace(&child_ref.text().collect::<String>());
                    if !normalized.is_empty() {
                        parts.push(normalized);
                    }
                }
            }
            _ => {}
        }
    }

    normalize_text_whitespace(&parts.join(" "))
}

#[cfg(test)]
fn extract_history_changes(li: &ElementRef<'_>) -> Result<Vec<HistoryDetail>> {
    let changes_sel = parse_selector(".history-changes");

    let mut details = Vec::new();
    for changes in li.select(&changes_sel) {
        let mut inline_html = String::new();

        for child in changes.children() {
            let Some(child_ref) = ElementRef::wrap(child) else {
                if let Node::Text(text) = child.value() {
                    inline_html.push_str(text);
                }
                continue;
            };

            if has_class(&child_ref, "original") {
                flush_history_change_buffer(&mut inline_html, &mut details)?;
                continue;
            }

            if has_class(&child_ref, "textdiff") {
                flush_history_change_buffer(&mut inline_html, &mut details)?;
                continue;
            }

            if child_ref.value().name() == "blockquote" {
                flush_history_change_buffer(&mut inline_html, &mut details)?;
                continue;
            }

            inline_html.push_str(&child_ref.html());
        }

        flush_history_change_buffer(&mut inline_html, &mut details)?;
    }

    Ok(details)
}

#[cfg(test)]
fn flush_history_change_buffer(
    inline_html: &mut String,
    details: &mut Vec<HistoryDetail>,
) -> Result<()> {
    if inline_html.trim().is_empty() {
        inline_html.clear();
        return Ok(());
    }

    details.extend(parse_change_lines("", Some(inline_html.as_str()))?);
    inline_html.clear();
    Ok(())
}

#[cfg(test)]
fn parse_change_lines(text: &str, source_html: Option<&str>) -> Result<Vec<HistoryDetail>> {
    let br_re = Regex::new(r"\s*<br\s*/?>\s*").context("构建历史换行正则失败")?;
    let segments = if let Some(html) = source_html {
        br_re
            .split(html)
            .map(|part| simplify_history_text(&normalize_markdown(&html2md::parse_html(part))))
            .collect::<Vec<_>>()
    } else {
        vec![simplify_history_text(text)]
    };

    let mut details = Vec::new();
    for segment in segments {
        if segment.is_empty()
            || should_hide_routine_change(&segment)
            || is_rich_text_change(&segment)
        {
            continue;
        }
        details.push(HistoryDetail::Change(segment));
    }
    Ok(details)
}

#[cfg(test)]
fn should_hide_routine_change(segment: &str) -> bool {
    let hidden_fields = [
        "解决方案",
        "解决版本",
        "解决日期",
        "指派给",
        "消耗工时",
        "Bug状态",
        "是否确认",
        "解决者",
        "激活日期",
        "激活次数",
        "关闭日期",
        "所属模块",
    ];

    hidden_fields.iter().any(|field| {
        let normalized = normalize_text_whitespace(field);
        segment.starts_with(&format!("修改了 {}", normalized))
    })
}

#[cfg(test)]
fn is_rich_text_change(segment: &str) -> bool {
    segment
        .trim_end_matches('：')
        .trim_end()
        .ends_with("区别为")
}

#[cfg(test)]
fn extract_history_comments(
    li: &ElementRef<'_>,
    page_url: &str,
    image_idx: &mut usize,
) -> Result<Vec<HistoryDetail>> {
    let comment_sel = parse_selector(".article-content.comment .comment-content");
    let mut comments = Vec::new();

    for comment in li.select(&comment_sel) {
        let markdown = comment_html_to_markdown(&comment.inner_html(), page_url, image_idx)?;
        if !markdown.is_empty() {
            comments.push(HistoryDetail::Comment(markdown));
        }
    }

    Ok(comments)
}

#[cfg(test)]
fn comment_html_to_markdown(html: &str, page_url: &str, image_idx: &mut usize) -> Result<String> {
    let mut markdown = html2md::parse_html(html).trim().to_string();
    let (converted, next_idx) = absolutize_markdown_image_urls_with_prefix_and_start(
        &markdown,
        page_url,
        "history-img",
        *image_idx,
    )?;
    *image_idx = next_idx;
    markdown = converted;
    markdown = split_adjacent_markdown_images(&markdown)?;
    markdown = split_markdown_image_and_following_text(&markdown)?;
    markdown = normalize_bracket_heading_bold_scope(&markdown)?;
    markdown = normalize_markdown(&markdown);
    Ok(markdown
        .lines()
        .map(normalize_text_whitespace)
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string())
}

#[cfg(test)]
fn render_history_entry(entry: &HistoryEntry) -> String {
    let mut out = format!("- {}", entry.header);
    for detail in &entry.details {
        match detail {
            HistoryDetail::Change(change) => {
                out.push('\n');
                out.push_str("  - ");
                out.push_str(change);
            }
            HistoryDetail::Comment(comment) => {
                out.push('\n');
                out.push_str("  - 备注：\n");
                out.push_str(&indent_block(comment, "    "));
            }
        }
    }
    out
}

#[cfg(test)]
fn normalize_text_whitespace(input: &str) -> String {
    input.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
fn simplify_history_text(input: &str) -> String {
    let bold_re = Regex::new(r"\*+").expect("valid emphasis regex");
    let strike_re = Regex::new(r"~~([^~]+)~~").expect("valid strike regex");
    let normalized = normalize_text_whitespace(input);
    let without_emphasis = bold_re.replace_all(&normalized, "").to_string();
    let without_strike = strike_re.replace_all(&without_emphasis, "$1").to_string();
    normalize_text_whitespace(&without_strike)
}

#[cfg(test)]
fn has_class(node: &ElementRef<'_>, class_name: &str) -> bool {
    node.value()
        .attr("class")
        .map(|classes| classes.split_whitespace().any(|item| item == class_name))
        .unwrap_or(false)
}

fn absolutize_url(base: &Url, raw: &str) -> Result<String> {
    if raw.starts_with("data:") || raw.starts_with('#') {
        return Ok(raw.to_string());
    }
    let url = Url::parse(raw).or_else(|_| base.join(raw))?;
    Ok(url.to_string())
}

#[cfg(test)]
fn parse_selector(css: &str) -> Selector {
    Selector::parse(css).expect("valid selector")
}

#[cfg(test)]
#[path = "bug_test.rs"]
mod tests;
