use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

pub const KIND_BUILD: &str = "build";
pub const KIND_MODULE: &str = "module";

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
    pub assigned_date: String,
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
        .map(|s| s.replace("个Bug", "个 Bug"))
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
        assigned_date: json_text(bug.get("assignedDate")),
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

/// A filter candidate (`value` is the Zentao id, `name` is the display label).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CandidateOption {
    pub value: String,
    pub name: String,
}

/// Parse `builds` / `modules` maps from a browse JSON payload into named kinds.
/// Skips empty id/name (placeholder options) and omits empty kinds.
pub fn parse_browse_kinds(body: &str) -> Result<BTreeMap<String, Vec<CandidateOption>>> {
    let data = parse_browse_data(body)?;
    let mut kinds = BTreeMap::new();
    for (kind, field) in [(KIND_BUILD, "builds"), (KIND_MODULE, "modules")] {
        let rows = parse_id_name_map(data.get(field));
        if !rows.is_empty() {
            kinds.insert(kind.to_string(), rows);
        }
    }
    Ok(kinds)
}

fn parse_browse_data(body: &str) -> Result<Value> {
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return Err(anyhow!("获取候选列表失败: 页面内容为空"));
    }
    if looks_like_login_html(trimmed) {
        return Err(anyhow!("获取候选列表失败: cookie 无效或已过期"));
    }

    let root: Value = serde_json::from_str(trimmed).map_err(|_| {
        if trimmed.contains("登录") {
            anyhow!("获取候选列表失败: cookie 无效或已过期")
        } else {
            anyhow!("获取候选列表失败: 无法解析浏览 JSON")
        }
    })?;
    unwrap_browse_payload(root).map_err(|err| anyhow!("获取候选列表失败: {err}"))
}

fn parse_id_name_map(value: Option<&Value>) -> Vec<CandidateOption> {
    let Some(Value::Object(map)) = value else {
        return Vec::new();
    };
    map.iter()
        .filter_map(|(id, name)| {
            let id = id.trim();
            if id.is_empty() {
                return None;
            }
            let name = json_text(Some(name));
            if name.is_empty() {
                return None;
            }
            Some(CandidateOption {
                value: id.to_string(),
                name,
            })
        })
        .collect()
}

/// Numeric build IDs are sent to Zentao as-is; names need candidate lookup.
pub fn is_build_id(raw: &str) -> bool {
    let trimmed = raw.trim();
    !trimmed.is_empty() && trimmed.chars().all(|c| c.is_ascii_digit())
}

/// Filter candidate options (builds, modules) by keyword; empty keyword returns all.
pub fn filter_options<'a>(
    options: &'a [CandidateOption],
    keyword: Option<&str>,
) -> Vec<&'a CandidateOption> {
    let Some(keyword) = keyword.map(str::trim).filter(|s| !s.is_empty()) else {
        return options.iter().collect();
    };
    let keyword_lower = keyword.to_lowercase();
    options
        .iter()
        .filter(|option| option_matches(option, keyword, &keyword_lower))
        .collect()
}

/// Resolve a user-supplied `--opened-build` / `--resolved-build` value to a Zentao build id.
/// Digits pass through. Otherwise match candidate `value`/`name` (exact, then unique contains).
pub fn resolve_build_value(query: &str, builds: &[CandidateOption]) -> Result<String> {
    let query = query.trim();
    if query.is_empty() {
        return Err(anyhow!("版本筛选不能为空"));
    }
    if is_build_id(query) {
        return Ok(query.to_string());
    }
    if let Some(build) = builds.iter().find(|build| build.value == query) {
        return Ok(build.value.clone());
    }
    let exact: Vec<&CandidateOption> = builds.iter().filter(|build| build.name == query).collect();
    if exact.len() == 1 {
        return Ok(exact[0].value.clone());
    }
    if exact.len() > 1 {
        return Err(ambiguous_build_error(query, &exact));
    }
    let query_lower = query.to_lowercase();
    let fuzzy: Vec<&CandidateOption> = builds
        .iter()
        .filter(|build| option_matches(build, query, &query_lower))
        .collect();
    match fuzzy.len() {
        0 => Err(anyhow!(
            "未找到版本「{query}」。用 `zentao bug candidates --build` 查看候选（value 用于 --opened-build / --resolved-build）"
        )),
        1 => Ok(fuzzy[0].value.clone()),
        _ => Err(ambiguous_build_error(query, &fuzzy)),
    }
}

fn option_matches(option: &CandidateOption, query: &str, query_lower: &str) -> bool {
    option.value == query
        || option.name.contains(query)
        || option.name.to_lowercase().contains(query_lower)
        || option.value.to_lowercase().contains(query_lower)
}

fn ambiguous_build_error(query: &str, matches: &[&CandidateOption]) -> anyhow::Error {
    ambiguous_candidate_error(query, matches, "版本", "zentao bug candidates --build")
}

/// Resolve a user-supplied `--module` value to a Zentao module id.
/// Digits pass through. Otherwise match candidate `value`/`name` (exact, then unique contains).
pub fn resolve_module_value(query: &str, modules: &[CandidateOption]) -> Result<String> {
    let query = query.trim();
    if query.is_empty() {
        return Err(anyhow!("模块筛选不能为空"));
    }
    if is_build_id(query) {
        return Ok(query.to_string());
    }
    if let Some(m) = modules.iter().find(|m| m.value == query) {
        return Ok(m.value.clone());
    }
    let exact: Vec<&CandidateOption> = modules.iter().filter(|m| m.name == query).collect();
    if exact.len() == 1 {
        return Ok(exact[0].value.clone());
    }
    if exact.len() > 1 {
        return Err(ambiguous_module_error(query, &exact));
    }
    let query_lower = query.to_lowercase();
    let fuzzy: Vec<&CandidateOption> = modules
        .iter()
        .filter(|m| option_matches(m, query, &query_lower))
        .collect();
    match fuzzy.len() {
        0 => Err(anyhow!(
            "未找到模块「{query}」。用 `zentao bug candidates --module` 查看候选（value 用于 --module）"
        )),
        1 => Ok(fuzzy[0].value.clone()),
        _ => Err(ambiguous_module_error(query, &fuzzy)),
    }
}

fn ambiguous_module_error(query: &str, matches: &[&CandidateOption]) -> anyhow::Error {
    ambiguous_candidate_error(query, matches, "模块", "zentao bug candidates --module")
}

fn ambiguous_candidate_error(
    query: &str,
    matches: &[&CandidateOption],
    label: &str,
    candidates_cmd: &str,
) -> anyhow::Error {
    let mut lines = vec![format!(
        "{label}「{query}」匹配到 {} 个，请改用 ID：",
        matches.len()
    )];
    for m in matches {
        lines.push(format!("  {}  {}", m.value, m.name));
    }
    lines.push(format!("用 `{candidates_cmd}` 查看全部候选"));
    anyhow!(lines.join("\n"))
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
