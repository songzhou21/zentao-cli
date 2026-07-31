use anyhow::{anyhow, Result};
use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};

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
    let table = doc
        .select(&table_sel)
        .next()
        .ok_or_else(|| anyhow!("搜索结果页未找到 bug 列表表格"))?;

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

#[cfg(test)]
#[path = "search_test.rs"]
mod tests;
