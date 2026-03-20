use anyhow::{anyhow, Result};
use chrono::{Datelike, Local, NaiveDate, NaiveDateTime};
use regex::Regex;
use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A single bug row from the search result table.
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
    pub resolved_date: String,
    pub resolution: String,
    pub deadline: String,
}

/// Summary stats from the search result page footer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub bugs: Vec<BugRow>,
    pub total: Option<String>,
}

#[derive(Debug, Clone, Copy)]
pub enum GroupBy {
    TestModule,
    AssignedTo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GroupedSearchResult {
    group_by: String,
    total: Option<String>,
    summary: GroupSummary,
    groups: Vec<GroupBucket>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GroupSummary {
    total: usize,
    unresolved: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GroupBucket {
    name: String,
    total: usize,
    unresolved: usize,
    bugs: Vec<BugRow>,
}

/// Parse the bug browse/search HTML page and extract bug rows from the table.
pub fn parse_search_result(html: &str) -> Result<SearchResult> {
    let doc = Html::parse_document(html);

    // Validate we are on the right page (not a login redirect)
    let title_sel = sel("title");
    if let Some(title_node) = doc.select(&title_sel).next() {
        let title_text = title_node.text().collect::<String>();
        if title_text.contains("登录") {
            return Err(anyhow!("搜索失败: cookie 无效或已过期"));
        }
    }

    // Some Zentao pages slightly vary table id/structure, so keep a few fallbacks.
    let table_sel = sel("table#bugList, form#bugForm table.datatable, form.table-bug table, .main-table.table-bug table.datatable");
    let table = doc
        .select(&table_sel)
        .next()
        .ok_or_else(|| anyhow!("搜索结果页未找到 bug 列表表格"))?;

    let strict_row_sel = sel("tbody tr[data-id], tr[data-id]");
    let loose_row_sel = sel("tbody tr, tr");
    let mut bugs = Vec::new();

    for row in table.select(&strict_row_sel) {
        if let Some(bug) = parse_bug_row(&row) {
            bugs.push(bug);
        }
    }
    if bugs.is_empty() {
        for row in table.select(&loose_row_sel) {
            if let Some(bug) = parse_bug_row(&row) {
                bugs.push(bug);
            }
        }
    }

    // Extract summary from ".table-statistic"
    let stat_sel = sel(".table-statistic");
    let total = doc
        .select(&stat_sel)
        .next()
        .map(|n| n.text().collect::<String>().trim().to_string());

    Ok(SearchResult { bugs, total })
}

fn parse_bug_row(row: &scraper::ElementRef) -> Option<BugRow> {
    let id: u64 = row
        .value()
        .attr("data-id")
        .and_then(|v| v.parse().ok())
        .or_else(|| {
            cell_text(row, "td.c-id a")
                .or_else(|| cell_text(row, "td.c-id"))
                .and_then(|v| v.parse().ok())
        })?;

    let title = cell_text(row, "td.c-title a")
        .or_else(|| cell_text(row, "td.c-title"))
        .unwrap_or_default();

    let severity = cell_attr_or_text(row, "td.c-severity span", "data-severity")
        .or_else(|| cell_text(row, "td.c-severity"))
        .unwrap_or_default();

    let pri = cell_text(row, "td.c-pri span")
        .or_else(|| cell_text(row, "td.c-pri"))
        .unwrap_or_default();

    let confirmed = cell_text(row, "td.c-confirmed span")
        .or_else(|| cell_text(row, "td.c-confirmed"))
        .unwrap_or_default();

    let status = cell_text(row, "td.c-status span")
        .or_else(|| cell_text(row, "td.c-status"))
        .unwrap_or_default();

    let opened_by = cell_text(row, "td.c-openedBy").unwrap_or_default();

    let opened_date = cell_text(row, "td.c-openedDate").unwrap_or_default();

    let assigned_to = cell_attr_or_text(row, "td.c-assignedTo span", "title")
        .or_else(|| cell_text(row, "td.c-assignedTo"))
        .unwrap_or_default();

    let resolved_date = cell_text(row, "td.c-resolvedDate").unwrap_or_default();

    let resolution = cell_text(row, "td.c-resolution").unwrap_or_default();

    let deadline = cell_attr_or_text(row, "td.c-deadline span", "title")
        .or_else(|| cell_attr_or_text(row, "td.c-deadline", "title"))
        .or_else(|| cell_text(row, "td.c-deadline span"))
        .or_else(|| cell_text(row, "td.c-deadline"))
        .unwrap_or_default();

    Some(BugRow {
        id,
        severity,
        pri,
        confirmed,
        title,
        status,
        opened_by,
        opened_date,
        assigned_to,
        resolved_date,
        resolution,
        deadline,
    })
}

fn cell_text(row: &scraper::ElementRef, css: &str) -> Option<String> {
    let s = sel(css);
    row.select(&s)
        .next()
        .map(|n| n.text().collect::<String>().trim().to_string())
        .filter(|t| !t.is_empty())
}

fn cell_attr_or_text(row: &scraper::ElementRef, css: &str, attr: &str) -> Option<String> {
    let s = sel(css);
    row.select(&s).next().and_then(|n| {
        n.value()
            .attr(attr)
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
            .or_else(|| {
                let t = n.text().collect::<String>().trim().to_string();
                if t.is_empty() {
                    None
                } else {
                    Some(t)
                }
            })
    })
}

pub fn render_search_json(result: &SearchResult) -> Result<String> {
    serde_json::to_string_pretty(result).map_err(|e| anyhow!("渲染 JSON 失败: {e}"))
}

pub fn render_search_lines_from_json(json: &str, hide_resolved_date: bool) -> Result<String> {
    let result: SearchResult =
        serde_json::from_str(json).map_err(|e| anyhow!("解析搜索 JSON 失败: {e}"))?;
    let today = Local::now().date_naive();

    let (total, unresolved) = summarize_counts(&result);
    if result.bugs.is_empty() {
        return Ok(format!(
            "搜索到 {} 个 Bug，未解决 {} 个。\n搜索结果为空。\n",
            total, unresolved
        ));
    }

    let mut out = String::new();
    out.push_str(&format!(
        "搜索到 {} 个 Bug，未解决 {} 个。\n\n",
        total, unresolved
    ));
    for (idx, bug) in result.bugs.iter().enumerate() {
        let resolved = is_resolved_bug(bug);
        let resolved_date = normalize_date_for_display(&bug.resolved_date);
        let (deadline_display, deadline_overdue) = if resolved {
            (normalize_date_for_display(&bug.deadline).to_string(), false)
        } else {
            format_deadline_for_display(&bug.deadline, today)
        };
        let title = bug.title.replace('\n', " ").replace('\r', " ");
        let deadline_segment = if deadline_overdue {
            format!("截止日期：\x1b[1;31m{}\x1b[38;5;244m", deadline_display)
        } else {
            format!("截止日期：{}", deadline_display)
        };
        let title_line = format!("{}. [{}] {}", idx + 1, bug.id, title.trim());
        if resolved {
            out.push_str(&format!("\x1b[38;5;247m{}\x1b[0m\n", title_line));
        } else {
            out.push_str(&format!("{title_line}\n"));
        }
        if hide_resolved_date {
            out.push_str(&format!(
                "\x1b[38;5;244m级别：{} ｜ 创建者：{} {} ｜ 指派：{} ｜ {}\x1b[0m\n",
                bug.severity.trim(),
                bug.opened_by.trim(),
                bug.opened_date.trim(),
                bug.assigned_to.trim(),
                deadline_segment,
            ));
        } else {
            out.push_str(&format!(
                "\x1b[38;5;244m级别：{} ｜ 创建者：{} {} ｜ 指派：{} ｜ {} ｜ 解决日期：{}\x1b[0m\n",
                bug.severity.trim(),
                bug.opened_by.trim(),
                bug.opened_date.trim(),
                bug.assigned_to.trim(),
                deadline_segment,
                resolved_date
            ));
        }
        if idx + 1 < result.bugs.len() {
            out.push('\n');
        }
    }

    Ok(out)
}

pub fn render_grouped_search_json(result: &SearchResult, group_by: GroupBy) -> Result<String> {
    let grouped = build_grouped_result(result, group_by);
    serde_json::to_string_pretty(&grouped).map_err(|e| anyhow!("渲染分组 JSON 失败: {e}"))
}

pub fn render_grouped_search_lines_from_json(
    json: &str,
    hide_resolved_date: bool,
) -> Result<String> {
    let mut grouped: GroupedSearchResult =
        serde_json::from_str(json).map_err(|e| anyhow!("解析分组搜索 JSON 失败: {e}"))?;
    let today = Local::now().date_naive();
    sort_groups_by_latest_opened_date_desc(&mut grouped.groups);
    sort_group_bugs_by_opened_date_desc(&mut grouped.groups);

    if grouped.summary.total == 0 {
        return Ok("搜索到 0 个 Bug，未解决 0 个。\n搜索结果为空。\n".to_string());
    }

    let mut out = String::new();
    out.push_str(&format!(
        "搜索到 {} 个 Bug，未解决 {} 个（按 {} 分组）。\n\n",
        grouped.summary.total, grouped.summary.unresolved, grouped.group_by
    ));

    for (group_idx, group) in grouped.groups.iter().enumerate() {
        out.push_str(&format!(
            "{}. {}（{} 个，未解决 {} 个）\n",
            group_idx + 1,
            group.name,
            group.total,
            group.unresolved
        ));
        for (idx, bug) in group.bugs.iter().enumerate() {
            let resolved = is_resolved_bug(bug);
            let resolved_date = normalize_date_for_display(&bug.resolved_date);
            let (deadline_display, deadline_overdue_raw) =
                format_deadline_for_display(&bug.deadline, today);
            let deadline_overdue = if resolved {
                false
            } else {
                deadline_overdue_raw
            };
            let title = bug.title.replace('\n', " ").replace('\r', " ");
            let deadline_segment = if deadline_overdue {
                format!("截止日期：\x1b[1;31m{}\x1b[38;5;244m", deadline_display)
            } else {
                format!("截止日期：{}", deadline_display)
            };
            let title_line = format!("  [{}] {}", bug.id, title.trim());
            if resolved {
                out.push_str(&format!("\x1b[38;5;247m{}\x1b[0m\n", title_line));
            } else {
                out.push_str(&format!("{title_line}\n"));
            }

            if hide_resolved_date {
                out.push_str(&format!(
                    "  \x1b[38;5;244m级别：{} ｜ 创建者：{} {} ｜ 指派：{} ｜ {}\x1b[0m\n",
                    bug.severity.trim(),
                    bug.opened_by.trim(),
                    bug.opened_date.trim(),
                    bug.assigned_to.trim(),
                    deadline_segment,
                ));
            } else {
                out.push_str(&format!(
                    "  \x1b[38;5;244m级别：{} ｜ 创建者：{} {} ｜ 指派：{} ｜ {} ｜ 解决日期：{}\x1b[0m\n",
                    bug.severity.trim(),
                    bug.opened_by.trim(),
                    bug.opened_date.trim(),
                    bug.assigned_to.trim(),
                    deadline_segment,
                    resolved_date
                ));
            }
            if idx + 1 < group.bugs.len() {
                out.push('\n');
            }
        }
        if group_idx + 1 < grouped.groups.len() {
            out.push('\n');
            out.push('\n');
        }
    }

    Ok(out)
}

fn sort_groups_by_latest_opened_date_desc(groups: &mut [GroupBucket]) {
    groups.sort_by(|a, b| {
        let ka = latest_opened_date_rank(a);
        let kb = latest_opened_date_rank(b);
        kb.cmp(&ka).then_with(|| a.name.cmp(&b.name))
    });
}

fn sort_group_bugs_by_opened_date_desc(groups: &mut [GroupBucket]) {
    groups.iter_mut().for_each(|group| {
        group.bugs.sort_by(|a, b| {
            let ka = parse_opened_date_rank(&a.opened_date).unwrap_or(i64::MIN);
            let kb = parse_opened_date_rank(&b.opened_date).unwrap_or(i64::MIN);
            kb.cmp(&ka).then_with(|| a.id.cmp(&b.id))
        });
    });
}

fn latest_opened_date_rank(group: &GroupBucket) -> i64 {
    group
        .bugs
        .iter()
        .filter_map(|bug| parse_opened_date_rank(&bug.opened_date))
        .max()
        .unwrap_or(i64::MIN)
}

fn parse_opened_date_rank(raw: &str) -> Option<i64> {
    parse_opened_date_rank_with_now(raw, Local::now().naive_local())
}

fn parse_opened_date_rank_with_now(raw: &str, now: NaiveDateTime) -> Option<i64> {
    let v = raw.trim();
    if v.is_empty() {
        return None;
    }

    if let Ok(dt) = NaiveDateTime::parse_from_str(v, "%Y-%m-%d %H:%M") {
        return Some(dt.and_utc().timestamp());
    }

    if let Ok(date) = NaiveDate::parse_from_str(v, "%Y-%m-%d") {
        let dt = date.and_hms_opt(0, 0, 0)?;
        return Some(dt.and_utc().timestamp());
    }

    let year = now.date().year();
    let with_year = format!("{year}-{v}");
    if let Ok(mut dt) = NaiveDateTime::parse_from_str(&with_year, "%Y-%m-%d %H:%M") {
        // Zentao 的 opened_date 常见格式为 MM-DD HH:mm，不包含年份。
        // 若按当前年解析后落在“未来”，通常代表上一年记录（例如当前 3 月看到 12-15）。
        if dt > now {
            dt = dt.with_year(year - 1).unwrap_or(dt);
        }
        return Some(dt.and_utc().timestamp());
    }

    None
}

fn build_grouped_result(result: &SearchResult, group_by: GroupBy) -> GroupedSearchResult {
    let (total, unresolved) = summarize_counts(result);
    let mut groups: Vec<GroupBucket> = Vec::new();
    let mut idx_map: HashMap<String, usize> = HashMap::new();

    for bug in &result.bugs {
        let key = group_key_for_bug(bug, group_by);
        let index = if let Some(index) = idx_map.get(&key) {
            *index
        } else {
            let index = groups.len();
            groups.push(GroupBucket {
                name: key.clone(),
                total: 0,
                unresolved: 0,
                bugs: Vec::new(),
            });
            idx_map.insert(key, index);
            index
        };

        let bucket = &mut groups[index];
        bucket.total += 1;
        if !is_resolved_bug(bug) {
            bucket.unresolved += 1;
        }
        bucket.bugs.push(bug.clone());
    }

    sort_groups_by_latest_opened_date_desc(&mut groups);
    sort_group_bugs_by_opened_date_desc(&mut groups);

    GroupedSearchResult {
        group_by: match group_by {
            GroupBy::TestModule => "module".to_string(),
            GroupBy::AssignedTo => "assigned-to".to_string(),
        },
        total: result.total.clone(),
        summary: GroupSummary { total, unresolved },
        groups,
    }
}

fn group_key_for_bug(bug: &BugRow, group_by: GroupBy) -> String {
    match group_by {
        GroupBy::TestModule => {
            extract_test_module_prefix(&bug.title).unwrap_or_else(|| "未匹配模块".to_string())
        }
        GroupBy::AssignedTo => {
            let v = bug.assigned_to.trim();
            if v.is_empty() {
                "未指派".to_string()
            } else {
                v.to_string()
            }
        }
    }
}

fn extract_test_module_prefix(title: &str) -> Option<String> {
    let re = Regex::new(r"^\s*(【[^】]+】)").expect("valid module title regex");
    re.captures(title)
        .and_then(|caps| caps.get(1).map(|m| m.as_str().to_string()))
}

fn normalize_date_for_display(raw: &str) -> &str {
    let v = raw.trim();
    if v.is_empty() || v == "0000-00-00" || v == "00-00 00:00" || v == "0000-00-00 00:00:00" {
        "--"
    } else {
        v
    }
}

fn format_deadline_for_display(raw: &str, today: NaiveDate) -> (String, bool) {
    let deadline = normalize_date_for_display(raw).to_string();
    if deadline == "--" {
        return (deadline, false);
    }

    let date_part = deadline.split_whitespace().next().unwrap_or(&deadline);
    let parsed = match NaiveDate::parse_from_str(date_part, "%Y-%m-%d") {
        Ok(v) => v,
        Err(_) => return (deadline, false),
    };
    let delta = (parsed - today).num_days();
    if delta < 0 {
        (format!("{}（已过{}天）", deadline, -delta), true)
    } else if delta == 0 {
        (format!("{}（今天）", deadline), true)
    } else if delta <= 7 {
        (format!("{}（剩余{}天）", deadline, delta), true)
    } else {
        (format!("{}（剩余{}天）", deadline, delta), false)
    }
}

fn summarize_counts(result: &SearchResult) -> (usize, usize) {
    let (total_from_text, unresolved_from_text) = parse_total_summary(result.total.as_deref());
    let total = total_from_text.unwrap_or(result.bugs.len());
    let unresolved = unresolved_from_text.unwrap_or_else(|| {
        result
            .bugs
            .iter()
            .filter(|b| normalize_date_for_display(&b.resolved_date) == "--")
            .count()
    });
    (total, unresolved)
}

fn parse_total_summary(total: Option<&str>) -> (Option<usize>, Option<usize>) {
    let Some(text) = total else {
        return (None, None);
    };
    let total_re = Regex::new(r"共\s*(\d+)\s*个\s*Bug").expect("valid total regex");
    let unresolved_re = Regex::new(r"未解决\s*(\d+)").expect("valid unresolved regex");

    let total_count = total_re
        .captures(text)
        .and_then(|c| c.get(1))
        .and_then(|m| m.as_str().parse::<usize>().ok());
    let unresolved_count = unresolved_re
        .captures(text)
        .and_then(|c| c.get(1))
        .and_then(|m| m.as_str().parse::<usize>().ok());
    (total_count, unresolved_count)
}

fn is_resolved_bug(bug: &BugRow) -> bool {
    normalize_date_for_display(&bug.resolved_date) != "--"
}

fn sel(css: &str) -> Selector {
    Selector::parse(css).expect("valid selector")
}

#[cfg(test)]
#[path = "search_test.rs"]
mod tests;
