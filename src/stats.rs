use crate::search::BugRow;
use anyhow::{anyhow, Result};
use chrono::Local;
use serde_json::{json, Map, Value};
use std::collections::HashMap;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

pub(crate) const DEFAULT_LIMIT: u32 = 1000;
pub(crate) const JSON_FIELDS: &[&str] = &[
    "assignee", "active", "resolved", "solved", "closed", "total",
];

const PERSON_WIDTH: usize = 16;
const COUNT_WIDTH: usize = 8;
pub(crate) const UNASSIGNED: &str = "(未指派)";
/// Closed bugs with no resolver still need a row so 关闭 footer matches the sample.
pub(crate) const UNRESOLVED: &str = "(未解决)";
pub(crate) const PERSON_HEADER: &str = "人员";
pub(crate) const TOTAL_LABEL: &str = "合计";

#[derive(Debug, Clone)]
pub(crate) struct PersonRow {
    pub assignee: String,
    pub active: u32,
    pub resolved: u32,
    pub solved: u32,
    pub closed: u32,
    pub total: u32,
}

#[derive(Debug, Clone)]
pub(crate) struct BugStats {
    pub rows: Vec<PersonRow>,
    pub total: PersonRow,
    pub sample_size: u32,
    pub limit: u32,
    pub incomplete: bool,
    /// Local wall-clock time when the search sample was fetched.
    pub fetched_at: String,
    /// Effective resolvedDate range after --week/--month/--day or explicit flags.
    pub resolved_from: Option<String>,
    pub resolved_to: Option<String>,
}

pub(crate) fn fetched_at_now() -> String {
    Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
}

pub(crate) fn format_resolved_date_range_line(
    from: Option<&str>,
    to: Option<&str>,
) -> Option<String> {
    match (
        from.map(str::trim)
            .filter(|s| !s.is_empty())
            .map(strip_resolved_time_for_date),
        to.map(str::trim)
            .filter(|s| !s.is_empty())
            .map(strip_resolved_time_for_date),
    ) {
        (Some(from), Some(to)) => Some(format!("解决日期: {from} ~ {to}")),
        (Some(from), None) => Some(format!("解决日期: from {from}")),
        (None, Some(to)) => Some(format!("解决日期: to {to}")),
        (None, None) => None,
    }
}

/// Resolved-date meta line: muted blue (not bright, distinct from names/counts).
pub(crate) fn paint_date_meta(value: &str, styled: bool) -> String {
    paint(value, "34", styled)
}

pub(crate) fn incomplete_warning(stats: &BugStats) -> String {
    format!(
        "warning: 样本已达 limit={}（聚合 {} 条），可能不全；请提高 -L 或收窄筛选",
        stats.limit, stats.sample_size
    )
}

/// 激活 / 待验证: current assignee. 已解决 / 关闭 / 合计: resolver (写出的全部).
pub(crate) fn aggregate(
    bugs: &[BugRow],
    limit: u32,
    fetched_at: String,
    resolved_from: Option<String>,
    resolved_to: Option<String>,
) -> BugStats {
    let mut by_person: HashMap<String, PersonRow> = HashMap::new();

    for bug in bugs {
        let state = canonical_state(&bug.status);
        match state {
            "active" | "resolved" => {
                let key = normalize_assignee(&bug.assigned_to);
                let row = person_row(&mut by_person, &key);
                if state == "active" {
                    row.active += 1;
                } else {
                    row.resolved += 1;
                }
            }
            _ => {}
        }

        let resolver = bug.resolved_by.trim();
        if !resolver.is_empty() && resolver != "--" {
            let row = person_row(&mut by_person, resolver);
            row.total += 1;
            match state {
                "resolved" => row.solved += 1,
                "closed" => row.closed += 1,
                _ => {}
            }
        } else if state == "closed" {
            person_row(&mut by_person, UNRESOLVED).closed += 1;
        }
    }

    let mut rows: Vec<PersonRow> = by_person.into_values().collect();
    // Who wrote the most first; then remaining workload.
    rows.sort_by(|a, b| {
        b.total
            .cmp(&a.total)
            .then_with(|| b.active.cmp(&a.active))
            .then_with(|| b.resolved.cmp(&a.resolved))
            .then_with(|| a.assignee.cmp(&b.assignee))
    });

    finish(
        rows,
        bugs.len() as u32,
        limit,
        fetched_at,
        resolved_from,
        resolved_to,
    )
}

pub(crate) fn render_table(stats: &BugStats, styled: bool) -> String {
    if stats.sample_size == 0 {
        let mut out = String::from("没有找到 Bug\n");
        append_meta_lines(&mut out, stats, styled);
        return out;
    }

    let header = format!(
        "{} {} {} {} {} {}",
        pad(PERSON_HEADER, PERSON_WIDTH),
        pad("激活", COUNT_WIDTH),
        pad("待验证", COUNT_WIDTH),
        pad("已解决", COUNT_WIDTH),
        pad("关闭", COUNT_WIDTH),
        pad("合计", COUNT_WIDTH),
    );
    let mut out = format!(
        "{}\n",
        if styled {
            paint(&header, "1", true)
        } else {
            header
        }
    );

    for row in stats.rows.iter().chain(std::iter::once(&stats.total)) {
        let emphasize = row.assignee == TOTAL_LABEL;
        out.push_str(&format!(
            "{} {} {} {} {} {}\n",
            style_person_cell(&row.assignee, styled),
            style_count_cell(row.active, styled, emphasize),
            style_count_cell(row.resolved, styled, emphasize),
            style_count_cell(row.solved, styled, emphasize),
            style_count_cell(row.closed, styled, emphasize),
            style_count_cell(row.total, styled, emphasize),
        ));
    }
    append_meta_lines(&mut out, stats, styled);
    out
}

pub(crate) fn render_json(stats: &BugStats, fields: &str) -> Result<Value> {
    let fields = parse_json_fields(fields, JSON_FIELDS)?;
    let person_field = "assignee";
    let rows: Vec<Value> = stats
        .rows
        .iter()
        .map(|row| Value::Object(row_json(row, &fields, true, person_field)))
        .collect();
    Ok(json!({
        "groupBy": person_field,
        "sampleSize": stats.sample_size,
        "limit": stats.limit,
        "incomplete": stats.incomplete,
        "fetchedAt": stats.fetched_at,
        "resolvedFrom": stats.resolved_from,
        "resolvedTo": stats.resolved_to,
        "rows": rows,
        "total": row_json(&stats.total, &fields, false, person_field),
    }))
}

fn empty_row(assignee: impl Into<String>) -> PersonRow {
    PersonRow {
        assignee: assignee.into(),
        active: 0,
        resolved: 0,
        solved: 0,
        closed: 0,
        total: 0,
    }
}

fn person_row<'a>(map: &'a mut HashMap<String, PersonRow>, key: &str) -> &'a mut PersonRow {
    map.entry(key.to_string())
        .or_insert_with(|| empty_row(key.to_string()))
}

fn normalize_assignee(raw: &str) -> String {
    let value = raw.trim();
    if value.is_empty() || value == "--" {
        UNASSIGNED.to_string()
    } else {
        value.to_string()
    }
}

fn finish(
    rows: Vec<PersonRow>,
    sample_size: u32,
    limit: u32,
    fetched_at: String,
    resolved_from: Option<String>,
    resolved_to: Option<String>,
) -> BugStats {
    let total = PersonRow {
        assignee: TOTAL_LABEL.to_string(),
        active: rows.iter().map(|r| r.active).sum(),
        resolved: rows.iter().map(|r| r.resolved).sum(),
        solved: rows.iter().map(|r| r.solved).sum(),
        closed: rows.iter().map(|r| r.closed).sum(),
        total: rows.iter().map(|r| r.total).sum(),
    };
    BugStats {
        rows,
        total,
        sample_size,
        limit,
        incomplete: sample_size >= limit,
        fetched_at,
        resolved_from,
        resolved_to,
    }
}

fn append_meta_lines(out: &mut String, stats: &BugStats, styled: bool) {
    out.push('\n');
    if let Some(range) = format_resolved_date_range_line(
        stats.resolved_from.as_deref(),
        stats.resolved_to.as_deref(),
    ) {
        out.push_str(&paint_date_meta(&range, styled));
        out.push('\n');
    }
    out.push_str(&paint(
        &format!("更新时间: {}", stats.fetched_at),
        "37",
        styled,
    ));
    out.push('\n');
}

fn style_person_cell(assignee: &str, styled: bool) -> String {
    let padded = pad(&truncate(assignee, PERSON_WIDTH), PERSON_WIDTH);
    let code = if assignee == TOTAL_LABEL {
        "1;36"
    } else if assignee == UNASSIGNED || assignee == UNRESOLVED {
        "90"
    } else {
        "36"
    };
    paint(&padded, code, styled)
}

fn style_count_cell(value: u32, styled: bool, emphasize: bool) -> String {
    let padded = pad(&value.to_string(), COUNT_WIDTH);
    let code = if emphasize { "1;37" } else { "37" };
    paint(&padded, code, styled)
}

fn paint(value: &str, code: &str, styled: bool) -> String {
    if styled {
        format!("\x1b[{code}m{value}\x1b[0m")
    } else {
        value.to_string()
    }
}

fn row_json(
    row: &PersonRow,
    fields: &[String],
    include_person: bool,
    person_field: &str,
) -> Map<String, Value> {
    let mut out = Map::new();
    for field in fields {
        if field == person_field && !include_person {
            continue;
        }
        out.insert(field.to_string(), json_value(row, field));
    }
    out
}

fn json_value(row: &PersonRow, field: &str) -> Value {
    match field {
        "assignee" => json!(row.assignee),
        "active" => json!(row.active),
        "resolved" => json!(row.resolved),
        "solved" => json!(row.solved),
        "closed" => json!(row.closed),
        "total" => json!(row.total),
        _ => Value::Null,
    }
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

fn strip_resolved_time_for_date(raw: &str) -> &str {
    raw.split([' ', 'T'])
        .next()
        .filter(|s| !s.is_empty())
        .unwrap_or(raw)
}

fn normalize_cell(value: &str) -> String {
    value.trim().replace(['\n', '\r'], " ")
}

fn truncate(value: &str, width: usize) -> String {
    let value = normalize_cell(value);
    if UnicodeWidthStr::width(value.as_str()) <= width {
        return pad(&value, width);
    }

    let ellipsis_width = UnicodeWidthChar::width('…').unwrap_or(1);
    let available = width.saturating_sub(ellipsis_width);
    let mut used = 0usize;
    let mut truncated = String::new();
    for character in value.chars() {
        let character_width = UnicodeWidthChar::width(character).unwrap_or(0);
        if used + character_width > available {
            break;
        }
        truncated.push(character);
        used += character_width;
    }
    truncated.push('…');
    pad(&truncated, width)
}

fn pad(value: &str, width: usize) -> String {
    let padding = width.saturating_sub(UnicodeWidthStr::width(value));
    format!("{value}{}", " ".repeat(padding))
}

#[cfg(test)]
#[path = "stats_test.rs"]
mod tests;
