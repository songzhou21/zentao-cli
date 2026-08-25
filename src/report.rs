use crate::search::BugRow;
use crate::view;
use anyhow::{anyhow, Result};
use serde_json::{json, Map, Value};
use std::collections::HashMap;

pub(crate) const UNGROUPED: &str = "其他";
pub(crate) const JSON_FIELDS: &[&str] = &[
    "name",
    "count",
    "id",
    "title",
    "displayTitle",
    "state",
    "resolution",
    "assignee",
    "bucket",
    "url",
    "resolved",
    "closed",
    "other",
    "total",
];
const BUG_JSON_FIELDS: &[&str] = &[
    "id",
    "title",
    "displayTitle",
    "state",
    "resolution",
    "assignee",
    "bucket",
    "url",
];
const SUMMARY_JSON_FIELDS: &[&str] = &["resolved", "closed", "other", "total"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Bucket {
    Resolved,
    Closed,
    Other,
}

impl Bucket {
    fn as_str(self) -> &'static str {
        match self {
            Self::Resolved => "resolved",
            Self::Closed => "closed",
            Self::Other => "other",
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ReportItem {
    pub id: u64,
    pub title: String,
    pub display_title: String,
    pub state: String,
    pub resolution: Option<String>,
    pub assignee: Option<String>,
    pub bucket: Bucket,
    pub url: String,
}

#[derive(Debug, Clone)]
pub(crate) struct ReportGroup {
    pub name: String,
    pub bugs: Vec<ReportItem>,
}

impl ReportGroup {
    pub fn count(&self) -> u32 {
        self.bugs.len() as u32
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ReportSummary {
    pub resolved: u32,
    pub closed: u32,
    pub other: u32,
    pub total: u32,
}

#[derive(Debug, Clone)]
pub(crate) struct BugReport {
    pub resolved_by: Option<String>,
    pub resolved_from: Option<String>,
    pub resolved_to: Option<String>,
    pub fetched_at: String,
    pub sample_size: u32,
    pub limit: u32,
    pub incomplete: bool,
    pub summary: ReportSummary,
    pub groups: Vec<ReportGroup>,
}

/// First `【…】` inner text is the group; no prefix → `其他`.
/// Display title drops a leading `【group】` plus following `/` or whitespace.
pub(crate) fn split_title_prefix(title: &str) -> (String, String) {
    let title = title.trim();
    let Some(start) = title.find('【') else {
        return (UNGROUPED.to_string(), title.to_string());
    };
    let inner_at = start + '【'.len_utf8();
    let Some(rel_end) = title[inner_at..].find('】') else {
        return (UNGROUPED.to_string(), title.to_string());
    };
    let end = inner_at + rel_end;
    let name = title[inner_at..end].trim();
    if name.is_empty() {
        return (UNGROUPED.to_string(), title.to_string());
    }
    let display = if start == 0 {
        title[end + '】'.len_utf8()..]
            .trim_start_matches(['/', ' ', '\t', '\u{3000}'])
            .trim()
            .to_string()
    } else {
        title.to_string()
    };
    let display = if display.is_empty() {
        title.to_string()
    } else {
        display
    };
    (name.to_string(), display)
}

pub(crate) fn build(
    bugs: &[BugRow],
    site: &str,
    limit: u32,
    fetched_at: String,
    resolved_from: Option<String>,
    resolved_to: Option<String>,
    resolved_by: Option<String>,
) -> BugReport {
    let mut grouped: HashMap<String, Vec<ReportItem>> = HashMap::new();
    let mut summary = ReportSummary {
        resolved: 0,
        closed: 0,
        other: 0,
        total: bugs.len() as u32,
    };

    for bug in bugs {
        let (name, display_title) = split_title_prefix(&bug.title);
        let state = canonical_state(&bug.status);
        let bucket = match state {
            "resolved" => Bucket::Resolved,
            "closed" => Bucket::Closed,
            _ => Bucket::Other,
        };
        match bucket {
            Bucket::Resolved => summary.resolved += 1,
            Bucket::Closed => summary.closed += 1,
            Bucket::Other => summary.other += 1,
        }
        grouped.entry(name).or_default().push(ReportItem {
            id: bug.id,
            title: bug.title.clone(),
            display_title,
            state: state.to_string(),
            resolution: nullable_text(&bug.resolution),
            assignee: nullable_text(&bug.assigned_to),
            bucket,
            url: view::canonical_bug_url(site, bug.id),
        });
    }

    let mut groups: Vec<ReportGroup> = grouped
        .into_iter()
        .map(|(name, bugs)| ReportGroup { name, bugs })
        .collect();
    groups.sort_by(|a, b| {
        let a_un = a.name == UNGROUPED;
        let b_un = b.name == UNGROUPED;
        a_un.cmp(&b_un)
            .then_with(|| b.count().cmp(&a.count()))
            .then_with(|| a.name.cmp(&b.name))
    });

    BugReport {
        resolved_by: resolved_by.and_then(|value| {
            let value = value.trim();
            if value.is_empty() {
                None
            } else {
                Some(value.to_string())
            }
        }),
        resolved_from,
        resolved_to,
        fetched_at,
        sample_size: bugs.len() as u32,
        limit,
        incomplete: bugs.len() as u32 >= limit,
        summary,
        groups,
    }
}

pub(crate) fn incomplete_warning(report: &BugReport) -> String {
    format!(
        "warning: 样本已达 limit={}（聚合 {} 条），可能不全；请提高 -L 或收窄筛选",
        report.limit, report.sample_size
    )
}

/// Markdown is a projection of `--json`. Group heading wraps JSON `name` as `【name】`.
pub(crate) fn render_markdown(json: &Value) -> String {
    let mut out = heading_from_json(json);
    out.push('\n');
    if json_u64(json, "sampleSize") == 0 {
        out.push_str("没有找到 Bug\n");
        return out;
    }
    let summary = json.get("summary").unwrap_or(&Value::Null);
    out.push_str(&format!(
        "合计 {}：已解决 {} · 已关闭 {} · 其他 {}\n",
        json_u64(summary, "total"),
        json_u64(summary, "resolved"),
        json_u64(summary, "closed"),
        json_u64(summary, "other"),
    ));
    let Some(groups) = json.get("groups").and_then(Value::as_array) else {
        return out;
    };
    for group in groups {
        let name = json_str(group, "name").unwrap_or(UNGROUPED);
        let count = json_u64(group, "count");
        out.push('\n');
        out.push_str(&format!("【{name}】({count})\n\n"));
        let Some(bugs) = group.get("bugs").and_then(Value::as_array) else {
            continue;
        };
        for bug in bugs {
            let id = json_u64(bug, "id");
            let display = json_str(bug, "displayTitle")
                .or_else(|| json_str(bug, "title"))
                .unwrap_or("");
            out.push_str(&format!("- #{id} {display}\n"));
        }
    }
    out
}

pub(crate) fn render_json(report: &BugReport, fields: &str) -> Result<Value> {
    let fields = parse_json_fields(fields, JSON_FIELDS)?;
    let mut bug_fields = intersect_fields(&fields, BUG_JSON_FIELDS);
    if bug_fields.is_empty() {
        bug_fields = BUG_JSON_FIELDS
            .iter()
            .map(|field| (*field).to_string())
            .collect();
    }
    let mut summary_fields = intersect_fields(&fields, SUMMARY_JSON_FIELDS);
    if summary_fields.is_empty() {
        summary_fields = SUMMARY_JSON_FIELDS
            .iter()
            .map(|field| (*field).to_string())
            .collect();
    }

    let groups: Vec<Value> = report
        .groups
        .iter()
        .map(|group| {
            json!({
                "name": group.name,
                "count": group.count(),
                "bugs": group
                    .bugs
                    .iter()
                    .map(|bug| Value::Object(bug_json(bug, &bug_fields)))
                    .collect::<Vec<_>>(),
            })
        })
        .collect();

    Ok(json!({
        "groupBy": "titlePrefix",
        "resolvedBy": report.resolved_by,
        "sampleSize": report.sample_size,
        "limit": report.limit,
        "incomplete": report.incomplete,
        "fetchedAt": report.fetched_at,
        "resolvedFrom": report.resolved_from,
        "resolvedTo": report.resolved_to,
        "summary": summary_json(&report.summary, &summary_fields),
        "groups": groups,
    }))
}

fn heading_from_json(json: &Value) -> String {
    let title = match json_str(json, "resolvedBy") {
        Some(who) => format!("{who} 解决 Bug"),
        None => "Bug 报告".to_string(),
    };
    match (json_str(json, "resolvedFrom"), json_str(json, "resolvedTo")) {
        (Some(from), Some(to)) => format!("# {title}（{from} ~ {to}）\n"),
        (Some(from), None) => format!("# {title}（from {from}）\n"),
        (None, Some(to)) => format!("# {title}（to {to}）\n"),
        (None, None) => format!("# {title}\n"),
    }
}

fn json_u64(value: &Value, key: &str) -> u64 {
    value.get(key).and_then(Value::as_u64).unwrap_or(0)
}

fn json_str<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
}

fn bug_json(bug: &ReportItem, fields: &[String]) -> Map<String, Value> {
    let mut out = Map::new();
    for field in fields {
        out.insert(field.to_string(), bug_json_value(bug, field));
    }
    out
}

fn bug_json_value(bug: &ReportItem, field: &str) -> Value {
    match field {
        "id" => json!(bug.id),
        "title" => json!(bug.title),
        "displayTitle" => json!(bug.display_title),
        "state" => json!(bug.state),
        "resolution" => match &bug.resolution {
            Some(value) => json!(value),
            None => Value::Null,
        },
        "assignee" => match &bug.assignee {
            Some(value) => json!(value),
            None => Value::Null,
        },
        "bucket" => json!(bug.bucket.as_str()),
        "url" => json!(bug.url),
        _ => Value::Null,
    }
}

fn summary_json(summary: &ReportSummary, fields: &[String]) -> Map<String, Value> {
    let mut out = Map::new();
    for field in fields {
        let value = match field.as_str() {
            "resolved" => summary.resolved,
            "closed" => summary.closed,
            "other" => summary.other,
            "total" => summary.total,
            _ => continue,
        };
        out.insert(field.to_string(), json!(value));
    }
    out
}

fn intersect_fields(fields: &[String], allowed: &[&str]) -> Vec<String> {
    fields
        .iter()
        .filter(|field| allowed.contains(&field.as_str()))
        .cloned()
        .collect()
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

fn canonical_state(raw: &str) -> &'static str {
    match raw.trim() {
        "激活" | "active" => "active",
        "已解决" | "resolved" => "resolved",
        "已关闭" | "closed" => "closed",
        _ => "unknown",
    }
}

fn nullable_text(raw: &str) -> Option<String> {
    let value = raw.trim();
    if value.is_empty() || value == "--" {
        None
    } else {
        Some(value.to_string())
    }
}

#[cfg(test)]
#[path = "report_test.rs"]
mod tests;
