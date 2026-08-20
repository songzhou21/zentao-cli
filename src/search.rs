use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A single Bug row from Zentao's browse JSON.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BugRow {
    pub id: u64,
    pub severity: String,
    pub pri: String,
    pub confirmed: String,
    pub title: String,
    pub status: String,
    pub opened_by: String,
    pub opened_date: String,
    pub assigned_to: String,
    pub resolved_by: String,
    pub resolved_date: String,
    pub resolution: String,
    pub deadline: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub bugs: Vec<BugRow>,
    pub total: Option<String>,
}

/// Parse Zentao's browse `.json` payload (`{status,data,md5}`, `data` may be a string).
/// Source for `bug list` / `bug stats` (full timestamps, resolution codes, `resolvedBy`).
pub fn parse_browse_json(body: &str) -> Result<SearchResult> {
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return Err(anyhow!("搜索失败: 页面内容为空"));
    }
    if looks_like_login_html(trimmed) {
        return Err(anyhow!("搜索失败: cookie 无效或已过期"));
    }

    let root: Value = serde_json::from_str(trimmed).map_err(|_| {
        if trimmed.contains("登录") {
            anyhow!("搜索失败: cookie 无效或已过期")
        } else {
            anyhow!("搜索失败: 无法解析浏览 JSON")
        }
    })?;
    let data = unwrap_browse_payload(root)?;
    let users = data.get("users").and_then(Value::as_object);
    let bugs = browse_bug_values(&data)
        .into_iter()
        .filter_map(|bug| parse_browse_json_bug(bug, users))
        .collect();
    let total = data
        .get("summary")
        .and_then(Value::as_str)
        .map(strip_html_tags)
        .filter(|s| !s.is_empty());
    Ok(SearchResult { bugs, total })
}

fn strip_html_tags(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut in_tag = false;
    for c in input.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn looks_like_login_html(body: &str) -> bool {
    let head = body.get(..800).unwrap_or(body);
    head.contains("<title>") && head.contains("登录")
}

fn unwrap_browse_payload(root: Value) -> Result<Value> {
    if let Some(data) = root.get("data") {
        if let Some(raw) = data.as_str() {
            return serde_json::from_str(raw).context("搜索失败: 无法解析浏览 JSON data");
        }
        if data.is_object() {
            return Ok(data.clone());
        }
    }
    if root.get("bugs").is_some() {
        return Ok(root);
    }
    Err(anyhow!("搜索失败: 浏览 JSON 缺少 bugs"))
}

fn browse_bug_values(data: &Value) -> Vec<&Value> {
    match data.get("bugs") {
        Some(Value::Array(items)) => items.iter().collect(),
        Some(Value::Object(map)) => map.values().collect(),
        _ => Vec::new(),
    }
}

fn parse_browse_json_bug(
    bug: &Value,
    users: Option<&serde_json::Map<String, Value>>,
) -> Option<BugRow> {
    let id = json_id(bug.get("id")?)?;
    Some(BugRow {
        id,
        title: json_text(bug.get("title")),
        severity: json_text(bug.get("severity")),
        pri: json_text(bug.get("pri")),
        confirmed: json_text(bug.get("confirmed")),
        status: json_text(bug.get("status")),
        opened_by: map_user_account(users, &json_text(bug.get("openedBy"))),
        opened_date: json_text(bug.get("openedDate")),
        assigned_to: map_user_account(users, &json_text(bug.get("assignedTo"))),
        resolved_by: map_user_account(users, &json_text(bug.get("resolvedBy"))),
        resolved_date: json_text(bug.get("resolvedDate")),
        resolution: json_text(bug.get("resolution")),
        deadline: json_text(bug.get("deadline")),
    })
}

fn json_id(value: &Value) -> Option<u64> {
    if let Some(n) = value.as_u64() {
        return Some(n);
    }
    if let Some(n) = value.as_i64() {
        return u64::try_from(n).ok();
    }
    value.as_str()?.trim().parse().ok()
}

fn json_text(value: Option<&Value>) -> String {
    match value {
        Some(Value::String(s)) => s.trim().to_string(),
        Some(Value::Number(n)) => n.to_string(),
        Some(Value::Bool(b)) => b.to_string(),
        _ => String::new(),
    }
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

#[cfg(test)]
#[path = "search_test.rs"]
mod tests;
