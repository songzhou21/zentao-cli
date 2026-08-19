use anyhow::{anyhow, Context, Result};
use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A single Bug row as presented by the Zentao browse table.
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

/// Parse the Zentao Bug browse page. Presentation-specific formatting happens in the CLI layer.
pub fn parse_search_result(html: &str) -> Result<SearchResult> {
    let doc = Html::parse_document(html);
    let title_sel = sel("title");
    if let Some(title_node) = doc.select(&title_sel).next() {
        if title_node.text().collect::<String>().contains("登录") {
            return Err(anyhow!("搜索失败: cookie 无效或已过期"));
        }
    }

    let table_sel = sel(
        "table#bugList, form#bugForm table.datatable, form.table-bug table, .main-table.table-bug table.datatable",
    );
    let Some(table) = doc.select(&table_sel).next() else {
        // No list table: treat as empty result (e.g. zero matches).
        return Ok(SearchResult {
            bugs: Vec::new(),
            total: None,
        });
    };

    let strict_row_sel = sel("tbody tr[data-id], tr[data-id]");
    let loose_row_sel = sel("tbody tr, tr");
    let mut bugs: Vec<BugRow> = table
        .select(&strict_row_sel)
        .filter_map(|row| parse_bug_row(&row))
        .collect();
    if bugs.is_empty() {
        bugs = table
            .select(&loose_row_sel)
            .filter_map(|row| parse_bug_row(&row))
            .collect();
    }

    let stat_sel = sel(".table-statistic");
    let total = doc
        .select(&stat_sel)
        .next()
        .map(|node| node.text().collect::<String>().trim().to_string());
    Ok(SearchResult { bugs, total })
}

fn parse_bug_row(row: &scraper::ElementRef) -> Option<BugRow> {
    let id = row
        .value()
        .attr("data-id")
        .and_then(|value| value.parse().ok())
        .or_else(|| {
            cell_text(row, "td.c-id a")
                .or_else(|| cell_text(row, "td.c-id"))
                .and_then(|value| value.parse().ok())
        })?;

    Some(BugRow {
        id,
        title: cell_text(row, "td.c-title a")
            .or_else(|| cell_text(row, "td.c-title"))
            .unwrap_or_default(),
        severity: cell_attr_or_text(row, "td.c-severity span", "data-severity")
            .or_else(|| cell_text(row, "td.c-severity"))
            .unwrap_or_default(),
        pri: cell_text(row, "td.c-pri span")
            .or_else(|| cell_text(row, "td.c-pri"))
            .unwrap_or_default(),
        confirmed: cell_text(row, "td.c-confirmed span")
            .or_else(|| cell_text(row, "td.c-confirmed"))
            .unwrap_or_default(),
        status: cell_text(row, "td.c-status span")
            .or_else(|| cell_text(row, "td.c-status"))
            .unwrap_or_default(),
        opened_by: cell_text(row, "td.c-openedBy").unwrap_or_default(),
        opened_date: cell_text(row, "td.c-openedDate").unwrap_or_default(),
        assigned_to: cell_attr_or_text(row, "td.c-assignedTo span", "title")
            .or_else(|| cell_text(row, "td.c-assignedTo"))
            .unwrap_or_default(),
        resolved_by: cell_attr_or_text(row, "td.c-resolvedBy span", "title")
            .or_else(|| cell_text(row, "td.c-resolvedBy"))
            .unwrap_or_default(),
        resolved_date: cell_text(row, "td.c-resolvedDate").unwrap_or_default(),
        resolution: cell_text(row, "td.c-resolution").unwrap_or_default(),
        deadline: cell_attr_or_text(row, "td.c-deadline span", "title")
            .or_else(|| cell_attr_or_text(row, "td.c-deadline", "title"))
            .or_else(|| cell_text(row, "td.c-deadline span"))
            .or_else(|| cell_text(row, "td.c-deadline"))
            .unwrap_or_default(),
    })
}

fn cell_text(row: &scraper::ElementRef, css: &str) -> Option<String> {
    let selector = sel(css);
    row.select(&selector)
        .next()
        .map(|node| node.text().collect::<String>().trim().to_string())
        .filter(|text| !text.is_empty())
}

fn cell_attr_or_text(row: &scraper::ElementRef, css: &str, attr: &str) -> Option<String> {
    let selector = sel(css);
    row.select(&selector).next().and_then(|node| {
        node.value()
            .attr(attr)
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .or_else(|| {
                let text = node.text().collect::<String>().trim().to_string();
                (!text.is_empty()).then_some(text)
            })
    })
}

fn sel(css: &str) -> Selector {
    Selector::parse(css).expect("valid selector")
}

/// Parse Zentao's browse `.json` payload (`{status,data,md5}`, `data` may be a string).
/// Used when the HTML table omits `resolvedBy`.
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
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    Ok(SearchResult { bugs, total })
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
