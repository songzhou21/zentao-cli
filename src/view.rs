use crate::bug::{BugDetail, HistoryEvent};
use anyhow::{anyhow, Context, Result};
use regex::Regex;
use scraper::{node::Node, ElementRef, Html};
use serde_json::{json, Map, Value};
use url::Url;

pub(crate) const JSON_FIELDS: &[&str] = &[
    "id",
    "title",
    "priority",
    "state",
    "openedBy",
    "openedDate",
    "assignee",
    "resolvedBy",
    "resolvedDate",
    "resolvedBuild",
    "description",
    "history",
    "images",
    "attachments",
    "url",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ParsedBugInput {
    pub id: u64,
    pub site_url: String,
    pub bug_url: String,
}

pub(crate) fn parse_bug_input(raw: &str, configured_site: Option<&str>) -> Result<ParsedBugInput> {
    let value = raw.trim();
    if value.is_empty() {
        return Err(anyhow!("Bug 输入无效: 输入为空"));
    }
    if let Ok(id) = value.parse::<u64>() {
        let site_url = required_site(configured_site)?;
        return Ok(ParsedBugInput {
            id,
            bug_url: canonical_bug_url(&site_url, id),
            site_url,
        });
    }

    let bug_url = Url::parse(value)
        .map_err(|_| anyhow!("Bug 输入无效: 请输入 Bug ID 或完整的 bug 详情 URL"))?;
    let re = Regex::new(r"bug-view-(\d+)\.html").expect("regex should compile");
    if let Some(caps) = re.captures(bug_url.path()) {
        if let Some(m) = caps.get(1) {
            let id = m
                .as_str()
                .parse::<u64>()
                .map_err(|e| anyhow!("Bug URL 无效: {e}"))?;
            let site_url = derive_site_url_from_bug_url(&bug_url)?;
            return Ok(ParsedBugInput {
                id,
                site_url,
                bug_url: bug_url.to_string(),
            });
        }
    }
    Err(anyhow!(
        "Bug URL 无效: 请输入包含 bug-view-<id>.html 的完整详情 URL"
    ))
}

/// Expand Zentao's `{status,data,md5}` wrapper so `data` is an object, not an escaped string.
pub(crate) fn decode_raw_payload(body: &str) -> Result<Value> {
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return Err(anyhow!("获取 bug 详情失败: 页面内容为空"));
    }
    let mut root: Value = serde_json::from_str(trimmed)
        .map_err(|_| anyhow!("获取 bug 详情失败: 无法解析详情 JSON"))?;
    let Some(Value::String(raw)) = root.get("data").cloned() else {
        return Ok(root);
    };
    let inner: Value =
        serde_json::from_str(&raw).context("获取 bug 详情失败: 无法解析详情 JSON data")?;
    if let Some(obj) = root.as_object_mut() {
        obj.insert("data".to_string(), inner);
    }
    Ok(root)
}

pub(crate) fn canonical_bug_url(site: &str, id: u64) -> String {
    let base = Url::parse(site)
        .map(|mut url| {
            url.set_query(None);
            url.set_fragment(None);
            url.to_string()
        })
        .unwrap_or_else(|_| site.to_string());
    format!("{}/bug-view-{id}.html", base.trim_end_matches('/'))
}

pub(crate) fn render_json(id: u64, site: &str, detail: &BugDetail, fields: &str) -> Result<Value> {
    let fields = parse_json_fields(fields, JSON_FIELDS)?;
    let attachments: Vec<Value> = detail
        .attachments
        .iter()
        .map(|attachment| {
            json!({
                "name": attachment.label,
                "url": attachment.url,
                "details": attachment.details_markdown,
            })
        })
        .collect();
    let mut out = Map::new();
    for field in fields {
        let value = match field.as_str() {
            "id" => json!(id),
            "title" => json!(detail.title),
            "priority" => json!(detail.priority.parse::<u8>().ok()),
            "state" => json!(detail.state),
            "openedBy" => nullable_text(&detail.opened_by),
            "openedDate" => nullable_text(&detail.opened_date),
            "assignee" => nullable_text(&detail.assignee),
            "resolvedBy" => nullable_text(&detail.resolved_by),
            "resolvedDate" => nullable_text(&detail.resolved_date),
            "resolvedBuild" => nullable_text(&detail.resolved_build),
            "description" => json!(detail.description),
            "history" => Value::Array(detail.history.iter().map(history_event_json).collect()),
            "images" => json!(detail.images),
            "attachments" => Value::Array(attachments.clone()),
            "url" => json!(canonical_bug_url(site, id)),
            _ => Value::Null,
        };
        out.insert(field, value);
    }
    Ok(Value::Object(out))
}

/// Markdown is a projection of the full `--json` object.
pub(crate) fn render_markdown(json: &Value) -> String {
    let mut out = String::new();
    let id = json_display(json, "id").unwrap_or_default();
    let title = json_display(json, "title").unwrap_or_default();
    match (id.is_empty(), title.is_empty()) {
        (false, false) => out.push_str(&format!("# {id} {title}\n")),
        (false, true) => out.push_str(&format!("# {id}\n")),
        (true, false) => out.push_str(&format!("# {title}\n")),
        (true, true) => out.push_str("# Bug\n"),
    }

    let meta = [
        ("状态", json_display(json, "state")),
        ("优先级", json_display(json, "priority")),
        ("创建者", json_display(json, "openedBy")),
        ("创建日期", json_display(json, "openedDate")),
        ("指派给", json_display(json, "assignee")),
        ("解决者", json_display(json, "resolvedBy")),
        ("解决日期", json_display(json, "resolvedDate")),
        ("上线版本", json_display(json, "resolvedBuild")),
        ("链接", json_display(json, "url")),
    ];
    let mut wrote_meta = false;
    for (label, value) in meta {
        if let Some(value) = value {
            if !wrote_meta {
                out.push('\n');
                wrote_meta = true;
            }
            out.push_str(&format!("- {label}：{value}\n"));
        }
    }

    let description = json
        .get("description")
        .and_then(Value::as_str)
        .map(html_to_markdown)
        .filter(|value| !value.is_empty());
    if let Some(description) = description {
        out.push_str("\n## 描述\n\n");
        out.push_str(&description);
        out.push('\n');
    }

    let attachments = json
        .get("attachments")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(attachment_markdown_line)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if !attachments.is_empty() {
        out.push_str("\n## 附件\n\n");
        for line in attachments {
            out.push_str(&line);
            out.push('\n');
        }
    }

    let history = json
        .get("history")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if !history.is_empty() {
        out.push_str("\n## 历史\n");
        for event in &history {
            out.push('\n');
            out.push_str(&history_event_markdown(event));
        }
    }

    if !out.ends_with('\n') {
        out.push('\n');
    }
    out
}

fn attachment_markdown_line(item: &Value) -> Option<String> {
    let name = json_display(item, "name").unwrap_or_default();
    let url = json_display(item, "url").unwrap_or_default();
    if name.is_empty() && url.is_empty() {
        return None;
    }
    let mut line = if url.is_empty() {
        format!("- {name}")
    } else if name.is_empty() {
        format!("- {url}")
    } else {
        format!("- [{name}]({url})")
    };
    if let Some(details) = json_display(item, "details") {
        line.push_str("\n\n");
        line.push_str(&details);
    }
    Some(line)
}

fn history_event_markdown(event: &Value) -> String {
    let at = json_display(event, "at").unwrap_or_default();
    let actor = json_display(event, "actor").unwrap_or_default();
    let action = json_display(event, "action").unwrap_or_default();
    let heading = [at.as_str(), actor.as_str(), action.as_str()]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" · ");
    let mut out = if heading.is_empty() {
        "### 历史\n".to_string()
    } else {
        format!("### {heading}\n")
    };

    if let Some(assignee) = json_display(event, "assignee") {
        out.push('\n');
        out.push_str(&format!("指派给 {assignee}\n"));
    }

    if let Some(changes) = event.get("changes").and_then(Value::as_array) {
        let mut wrote = false;
        for change in changes {
            let label = json_display(change, "label")
                .or_else(|| json_display(change, "field"))
                .unwrap_or_default();
            let old = json_display(change, "old").unwrap_or_default();
            let new = json_display(change, "new").unwrap_or_default();
            if label.is_empty() && old.is_empty() && new.is_empty() {
                continue;
            }
            if !wrote {
                out.push('\n');
                wrote = true;
            }
            let name = if label.is_empty() { "变更" } else { &label };
            if old.is_empty() {
                out.push_str(&format!("- {name}：{new}\n"));
            } else {
                out.push_str(&format!("- {name}：{old} → {new}\n"));
            }
        }
    }

    if let Some(comment) = event
        .get("comment")
        .and_then(Value::as_str)
        .map(html_to_markdown)
        .filter(|value| !value.is_empty())
    {
        out.push('\n');
        out.push_str(&comment);
        out.push('\n');
    }
    out
}

fn html_to_markdown(html: &str) -> String {
    if html.trim().is_empty() {
        return String::new();
    }
    let fragment = Html::parse_fragment(html);
    let mut out = String::new();
    write_md_children(fragment.root_element(), &mut out);
    normalize_projected_markdown(&out)
}

fn write_md_children(element: ElementRef<'_>, out: &mut String) {
    for child in element.children() {
        match child.value() {
            Node::Text(text) => out.push_str(&decode_md_text(text)),
            Node::Element(_) => {
                if let Some(node) = ElementRef::wrap(child) {
                    write_md_element(node, out);
                }
            }
            _ => {}
        }
    }
}

fn write_md_element(element: ElementRef<'_>, out: &mut String) {
    match element.value().name() {
        "p" => {
            push_block_break(out);
            write_md_children(element, out);
            push_block_break(out);
        }
        "br" => out.push('\n'),
        "ol" => write_md_list(element, out, true),
        "ul" => write_md_list(element, out, false),
        "li" => write_md_list_item(element, out, None),
        "img" => {
            if let Some(src) = element
                .value()
                .attr("src")
                .map(str::trim)
                .filter(|src| !src.is_empty())
            {
                push_block_break(out);
                out.push_str(&format!("![]({src})"));
                push_block_break(out);
            }
        }
        "a" => {
            let href = element
                .value()
                .attr("href")
                .map(str::trim)
                .unwrap_or_default();
            let mut text = String::new();
            write_md_children(element, &mut text);
            let text = text.trim();
            if href.is_empty() {
                out.push_str(text);
            } else if text.is_empty() || text == href {
                out.push_str(href);
            } else {
                out.push_str(&format!("[{text}]({href})"));
            }
        }
        _ => write_md_children(element, out),
    }
}

fn write_md_list(element: ElementRef<'_>, out: &mut String, ordered: bool) {
    let mut items = Vec::new();
    for child in element.children() {
        if let Some(node) = ElementRef::wrap(child) {
            if node.value().name() == "li" {
                let mut item = String::new();
                write_md_children(node, &mut item);
                let item = item.trim().to_string();
                if !item.is_empty() {
                    items.push(item);
                }
            }
        }
    }
    if items.is_empty() {
        return;
    }
    push_block_break(out);
    for (index, item) in items.iter().enumerate() {
        if ordered {
            out.push_str(&format!("{}. ", index + 1));
        } else {
            out.push_str("- ");
        }
        out.push_str(&item.replace('\n', "\n  "));
        out.push('\n');
    }
    push_block_break(out);
}

fn write_md_list_item(element: ElementRef<'_>, out: &mut String, ordered_index: Option<usize>) {
    let mut item = String::new();
    write_md_children(element, &mut item);
    let item = item.trim();
    if item.is_empty() {
        return;
    }
    if !out.ends_with('\n') && !out.is_empty() {
        out.push('\n');
    }
    match ordered_index {
        Some(index) => out.push_str(&format!("{index}. ")),
        None => out.push_str("- "),
    }
    out.push_str(&item.replace('\n', "\n  "));
    out.push('\n');
}

fn push_block_break(out: &mut String) {
    if out.is_empty() {
        return;
    }
    if out.ends_with("\n\n") {
        return;
    }
    if out.ends_with('\n') {
        out.push('\n');
    } else {
        out.push_str("\n\n");
    }
}

fn decode_md_text(input: &str) -> String {
    input.replace('\u{a0}', " ")
}

fn normalize_projected_markdown(markdown: &str) -> String {
    let mut lines: Vec<&str> = markdown.lines().map(str::trim_end).collect();
    while lines.first().is_some_and(|line| line.is_empty()) {
        lines.remove(0);
    }
    while lines.last().is_some_and(|line| line.is_empty()) {
        lines.pop();
    }
    let mut out = String::new();
    let mut blank = false;
    for line in lines {
        if line.is_empty() {
            if !blank {
                out.push('\n');
                blank = true;
            }
            continue;
        }
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(line);
        blank = false;
    }
    out
}

fn json_display(value: &Value, key: &str) -> Option<String> {
    match value.get(key)? {
        Value::Null => None,
        Value::String(text) => {
            let text = text.trim();
            if text.is_empty() {
                None
            } else {
                Some(text.to_string())
            }
        }
        Value::Number(number) => Some(number.to_string()),
        Value::Bool(flag) => Some(flag.to_string()),
        _ => None,
    }
}

fn history_event_json(event: &HistoryEvent) -> Value {
    let mut map = Map::new();
    map.insert("at".to_string(), json!(event.at));
    map.insert("action".to_string(), json!(event.action));
    map.insert("actor".to_string(), json!(event.actor));
    if let Some(assignee) = event.assignee.as_deref().filter(|value| !value.is_empty()) {
        map.insert("assignee".to_string(), json!(assignee));
    }
    if !event.changes.is_empty() {
        let changes: Vec<Value> = event
            .changes
            .iter()
            .map(|change| {
                json!({
                    "field": change.field,
                    "label": change.label,
                    "old": change.old,
                    "new": change.new,
                })
            })
            .collect();
        map.insert("changes".to_string(), Value::Array(changes));
    }
    if let Some(comment) = event.comment.as_deref().filter(|value| !value.is_empty()) {
        map.insert("comment".to_string(), json!(comment));
    }
    Value::Object(map)
}

fn nullable_text(raw: &str) -> Value {
    let value = raw.trim();
    if value.is_empty() || value == "--" {
        Value::Null
    } else {
        json!(value)
    }
}

fn derive_site_url_from_bug_url(url: &Url) -> Result<String> {
    let mut base = url.clone();
    let mut segments: Vec<String> = base
        .path_segments()
        .map(|parts| parts.map(str::to_string).collect())
        .unwrap_or_default();

    let last = segments
        .last()
        .ok_or_else(|| anyhow!("Bug URL 无效: 缺少页面路径"))?;
    if !last.starts_with("bug-view-") {
        return Err(anyhow!("Bug URL 无效: 未找到 bug-view 页面"));
    }
    segments.pop();

    let new_path = if segments.is_empty() {
        "/".to_string()
    } else {
        format!("/{}", segments.join("/"))
    };
    base.set_path(&new_path);
    base.set_query(None);
    base.set_fragment(None);

    Ok(base.to_string().trim_end_matches('/').to_string())
}

fn required_site(configured_site: Option<&str>) -> Result<String> {
    if let Some(site) = configured_site {
        let site = site.trim();
        if !site.is_empty() {
            return Ok(site.to_string());
        }
    }
    Err(anyhow!("缺少 site，请通过命令行参数或配置文件提供"))
}

fn parse_json_fields(raw: &str, supported: &[&str]) -> Result<Vec<String>> {
    if raw.trim().is_empty() {
        return Ok(supported.iter().map(|field| (*field).to_string()).collect());
    }
    let fields: Vec<String> = raw
        .split(',')
        .map(str::trim)
        .filter(|field| !field.is_empty())
        .map(str::to_string)
        .collect();
    for field in &fields {
        if !supported.contains(&field.as_str()) {
            return Err(anyhow!(
                "不支持 JSON 字段 `{field}`；可用字段：{}",
                supported.join(",")
            ));
        }
    }
    Ok(fields)
}

#[cfg(test)]
#[path = "view_test.rs"]
mod tests;
