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
const MAIN_JSON_FIELDS: &[&str] = &["assignee", "active", "solved", "closed", "total"];
const PENDING_JSON_FIELDS: &[&str] = &["assignee", "resolved"];

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
    /// 待验证：状态 `resolved`，按当前指派给。
    pub pending: Vec<PersonRow>,
    pub pending_total: PersonRow,
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

/// 主表：激活按当前指派；已解决 / 关闭按解决者；合计 = 激活+已解决+关闭。
/// 待验证单独进 `pending`（当前指派且状态 resolved）。
pub(crate) fn aggregate(
    bugs: &[BugRow],
    limit: u32,
    fetched_at: String,
    resolved_from: Option<String>,
    resolved_to: Option<String>,
) -> BugStats {
    let mut by_person: HashMap<String, PersonRow> = HashMap::new();
    let mut by_pending: HashMap<String, PersonRow> = HashMap::new();

    for bug in bugs {
        let state = canonical_state(&bug.status);
        match state {
            "active" => {
                let key = normalize_assignee(&bug.assigned_to);
                person_row(&mut by_person, &key).active += 1;
            }
            "resolved" => {
                let key = normalize_assignee(&bug.assigned_to);
                person_row(&mut by_pending, &key).resolved += 1;
            }
            _ => {}
        }

        let resolver = bug.resolved_by.trim();
        if !resolver.is_empty() && resolver != "--" {
            let row = person_row(&mut by_person, resolver);
            match state {
                "resolved" => row.solved += 1,
                "closed" => row.closed += 1,
                _ => {}
            }
        } else if state == "closed" {
            person_row(&mut by_person, UNRESOLVED).closed += 1;
        }
    }

    let mut rows: Vec<PersonRow> = by_person.into_values().map(with_column_total).collect();
    rows.sort_by(|a, b| {
        b.total
            .cmp(&a.total)
            .then_with(|| b.active.cmp(&a.active))
            .then_with(|| b.solved.cmp(&a.solved))
            .then_with(|| a.assignee.cmp(&b.assignee))
    });
    let mut pending: Vec<PersonRow> = by_pending.into_values().collect();
    pending.sort_by(|a, b| {
        b.resolved
            .cmp(&a.resolved)
            .then_with(|| a.assignee.cmp(&b.assignee))
    });

    finish(
        rows,
        pending,
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

    let mut out = String::new();
    if !stats.rows.is_empty() {
        append_table(
            &mut out,
            &[PERSON_HEADER, "激活", "已解决", "关闭", "合计"],
            stats.rows.iter().chain(std::iter::once(&stats.total)),
            |row| vec![row.active, row.solved, row.closed, row.total],
            styled,
        );
    }
    if !stats.pending.is_empty() {
        if !out.is_empty() {
            out.push('\n');
        }
        append_table(
            &mut out,
            &[PERSON_HEADER, "待验证"],
            stats
                .pending
                .iter()
                .chain(std::iter::once(&stats.pending_total)),
            |row| vec![row.resolved],
            styled,
        );
    }
    if out.is_empty() {
        out.push_str("没有找到 Bug\n");
    }
    append_meta_lines(&mut out, stats, styled);
    out
}

pub(crate) fn render_json(stats: &BugStats, fields: &str) -> Result<Value> {
    let fields = parse_json_fields(fields, JSON_FIELDS)?;
    let person_field = "assignee";
    let main_fields = intersect_fields(&fields, MAIN_JSON_FIELDS);
    let pending_fields = intersect_fields(&fields, PENDING_JSON_FIELDS);
    let rows: Vec<Value> = stats
        .rows
        .iter()
        .map(|row| Value::Object(row_json(row, &main_fields, true, person_field)))
        .collect();
    let pending_rows: Vec<Value> = stats
        .pending
        .iter()
        .map(|row| Value::Object(row_json(row, &pending_fields, true, person_field)))
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
        "total": row_json(&stats.total, &main_fields, false, person_field),
        "pending": {
            "rows": pending_rows,
            "total": row_json(&stats.pending_total, &pending_fields, false, person_field),
        },
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
    pending: Vec<PersonRow>,
    sample_size: u32,
    limit: u32,
    fetched_at: String,
    resolved_from: Option<String>,
    resolved_to: Option<String>,
) -> BugStats {
    let total = with_column_total(sum_rows(&rows));
    let pending_total = sum_rows(&pending);
    BugStats {
        rows,
        total,
        pending,
        pending_total,
        sample_size,
        limit,
        incomplete: sample_size >= limit,
        fetched_at,
        resolved_from,
        resolved_to,
    }
}

fn with_column_total(mut row: PersonRow) -> PersonRow {
    row.total = row.active + row.solved + row.closed;
    row
}

fn sum_rows(rows: &[PersonRow]) -> PersonRow {
    PersonRow {
        assignee: TOTAL_LABEL.to_string(),
        active: rows.iter().map(|r| r.active).sum(),
        resolved: rows.iter().map(|r| r.resolved).sum(),
        solved: rows.iter().map(|r| r.solved).sum(),
        closed: rows.iter().map(|r| r.closed).sum(),
        total: rows.iter().map(|r| r.total).sum(),
    }
}

fn intersect_fields(fields: &[String], allowed: &[&str]) -> Vec<String> {
    fields
        .iter()
        .filter(|field| allowed.contains(&field.as_str()))
        .cloned()
        .collect()
}

fn append_table<'a, I, F>(out: &mut String, titles: &[&str], rows: I, counts: F, styled: bool)
where
    I: Iterator<Item = &'a PersonRow>,
    F: Fn(&PersonRow) -> Vec<u32>,
{
    let header = titles
        .iter()
        .enumerate()
        .map(|(i, title)| pad(title, if i == 0 { PERSON_WIDTH } else { COUNT_WIDTH }))
        .collect::<Vec<_>>()
        .join(" ");
    out.push_str(&if styled {
        paint(&header, "1", true)
    } else {
        header
    });
    out.push('\n');
    for row in rows {
        let emphasize = row.assignee == TOTAL_LABEL;
        let mut line = style_person_cell(&row.assignee, styled);
        for value in counts(row) {
            line.push(' ');
            line.push_str(&style_count_cell(value, styled, emphasize));
        }
        out.push_str(&line);
        out.push('\n');
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
