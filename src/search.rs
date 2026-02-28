use anyhow::{anyhow, Result};
use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};

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

    let opened_by = cell_text(row, "td.c-openedBy")
        .unwrap_or_default();

    let opened_date = cell_text(row, "td.c-openedDate")
        .unwrap_or_default();

    let assigned_to = cell_attr_or_text(row, "td.c-assignedTo span", "title")
        .or_else(|| cell_text(row, "td.c-assignedTo"))
        .unwrap_or_default();

    let resolved_date = cell_text(row, "td.c-resolvedDate")
        .unwrap_or_default();

    let resolution = cell_text(row, "td.c-resolution")
        .unwrap_or_default();

    let deadline = cell_text(row, "td.c-deadline")
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
    row.select(&s).next().map(|n| {
        n.text().collect::<String>().trim().to_string()
    }).filter(|t| !t.is_empty())
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
                if t.is_empty() { None } else { Some(t) }
            })
    })
}

pub fn render_search_json(result: &SearchResult) -> Result<String> {
    serde_json::to_string_pretty(result).map_err(|e| anyhow!("渲染 JSON 失败: {e}"))
}

pub fn render_search_lines_from_json_with_options(
    json: &str,
    hide_resolved_date: bool,
) -> Result<String> {
    let result: SearchResult =
        serde_json::from_str(json).map_err(|e| anyhow!("解析搜索 JSON 失败: {e}"))?;

    let mut out = String::new();
    let overview = result
        .total
        .as_deref()
        .map(normalize_total_line)
        .unwrap_or_else(|| format!("共 {} 个Bug", result.bugs.len()));
    out.push_str(&overview);
    out.push('\n');
    out.push('\n');
    if hide_resolved_date {
        out.push_str("| ID | 级别 | 标题 | 创建者 | 指派给 | 截止日期 |\n");
        out.push_str("|---:|:----:|------|--------|--------|----------|\n");
    } else {
        out.push_str("| ID | 级别 | 标题 | 创建者 | 指派给 | 解决日期 | 截止日期 |\n");
        out.push_str("|---:|:----:|------|--------|--------|----------|----------|\n");
    }

    for bug in &result.bugs {
        let resolved_date = if bug.resolved_date.trim().is_empty() {
            "-"
        } else {
            bug.resolved_date.trim()
        };
        let deadline = if bug.deadline.trim().is_empty() || bug.deadline.trim() == "0000-00-00" {
            "-"
        } else {
            bug.deadline.trim()
        };
        let title = bug.title.replace('\n', " ").replace('\r', " ");

        if hide_resolved_date {
            out.push_str(&format!(
                "| {} | {} | {} | {} | {} | {} |\n",
                bug.id,
                bug.severity.trim(),
                title.trim(),
                bug.opened_by.trim(),
                bug.assigned_to.trim(),
                deadline,
            ));
        } else {
            out.push_str(&format!(
                "| {} | {} | {} | {} | {} | {} | {} |\n",
                bug.id,
                bug.severity.trim(),
                title.trim(),
                bug.opened_by.trim(),
                bug.assigned_to.trim(),
                resolved_date,
                deadline,
            ));
        }
    }

    Ok(out)
}

fn normalize_total_line(total: &str) -> String {
    total
        .trim()
        .trim_start_matches("本页")
        .trim_end_matches('。')
        .trim()
        .to_string()
}

fn sel(css: &str) -> Selector {
    Selector::parse(css).expect("valid selector")
}

#[cfg(test)]
#[path = "search_test.rs"]
mod tests;
