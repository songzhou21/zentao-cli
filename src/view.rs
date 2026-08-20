use crate::bug::{BugDetail, HistoryEvent};
use anyhow::{anyhow, Context, Result};
use regex::Regex;
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
