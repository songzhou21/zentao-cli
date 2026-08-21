use crate::cli::bug::{execute_bug_search, BugSearchQuery, BugState};
use crate::cli::{
    ansi_enabled, parse_json_fields, print_json, style_header, validate_optional_json_fields,
    GlobalArgs,
};
use crate::search;
use crate::stats;
use crate::view;
use anyhow::Result;
use clap::Args;
use serde_json::{json, Map, Value};
use std::io::{self, IsTerminal};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

const BUG_LIST_ID_WIDTH: usize = 6;
const BUG_LIST_STATE_WIDTH: usize = 9;
const BUG_LIST_OPENED_BY_WIDTH: usize = 10;
const BUG_LIST_TITLE_WIDTH: usize = 65;
const BUG_LIST_ASSIGNEE_WIDTH: usize = 10;
/// Fits browse-JSON timestamps like `2026-08-20 11:30:31`.
const BUG_LIST_OPENED_DATE_WIDTH: usize = 19;

pub(crate) const LIST_JSON_FIELDS: &[&str] = &[
    "id",
    "title",
    "state",
    "severity",
    "priority",
    "confirmed",
    "openedBy",
    "openedDate",
    "assignee",
    "resolvedBy",
    "resolvedDate",
    "resolution",
    "deadline",
    "url",
];

#[derive(Debug, Args)]
pub(crate) struct BugListArgs {
    /// 标题关键词（包含匹配）。可重复传入，多个值按 OR 处理，例如 --title A --title B
    #[arg(long, value_name = "KEYWORD")]
    pub(crate) title: Vec<String>,

    /// 指派给（用户名），例如 zhousong
    #[arg(short = 'a', long, value_name = "USER")]
    pub(crate) assignee: Option<String>,

    /// 创建者（用户名/账号，例如 chenjie）。可重复传入，多个值按 OR 处理，最多 3 个
    #[arg(long, value_name = "USER")]
    pub(crate) opened_by: Vec<String>,

    /// 解决者（用户名），例如 zhousong
    #[arg(long, value_name = "USER")]
    pub(crate) resolved_by: Option<String>,

    /// 解决日期起始（含），格式 YYYY-MM-DD
    #[arg(long, value_name = "DATE")]
    pub(crate) resolved_from: Option<String>,

    /// 解决日期截止（含），格式 YYYY-MM-DD
    #[arg(long, value_name = "DATE")]
    pub(crate) resolved_to: Option<String>,

    /// 解决日期快捷：本周一～本周日（含），与 --month/--day/--resolved-from/--resolved-to 互斥
    #[arg(long, conflicts_with_all = ["month", "day", "resolved_from", "resolved_to"])]
    pub(crate) week: bool,

    /// 解决日期快捷：本月 1 日～月末（含），与 --week/--day/--resolved-from/--resolved-to 互斥
    #[arg(long, conflicts_with_all = ["week", "day", "resolved_from", "resolved_to"])]
    pub(crate) month: bool,

    /// 解决日期快捷：今天，与 --week/--month/--resolved-from/--resolved-to 互斥
    #[arg(long, conflicts_with_all = ["week", "month", "resolved_from", "resolved_to"])]
    pub(crate) day: bool,

    /// 所属模块 ID，例如 1099
    #[arg(long, value_name = "MODULE_ID")]
    pub(crate) module: Option<String>,

    /// 影响版本 ID，例如 982
    #[arg(long, value_name = "BUILD_ID")]
    pub(crate) opened_build: Option<String>,

    /// 解决版本 ID，例如 982
    #[arg(long, value_name = "BUILD_ID")]
    pub(crate) resolved_build: Option<String>,

    /// Bug 状态；默认 active
    #[arg(short = 's', long, value_enum, value_name = "STATE", default_value_t = BugState::Active)]
    pub(crate) state: BugState,

    /// 产品 ID；未提供时从 ZENTAO_PRODUCT 或配置读取
    #[arg(long, env = "ZENTAO_PRODUCT", value_name = "ID")]
    pub(crate) product: Option<u64>,

    /// 最多返回的 Bug 数量
    #[arg(
        short = 'L',
        long,
        default_value_t = 30,
        value_parser = clap::value_parser!(u32).range(1..),
        value_name = "N"
    )]
    pub(crate) limit: u32,

    /// 表格输出时展示完整标题（默认按显示宽度截断）；不影响搜索条件或 JSON
    #[arg(long, default_value_t = false)]
    pub(crate) full_title: bool,

    /// 纯文本表格：关闭超链接、颜色等交互装饰（仍可与 --full-title 并用）
    #[arg(long, default_value_t = false)]
    pub(crate) plain: bool,

    /// 输出 JSON；可选指定字段：id,title,state,severity,priority,confirmed,openedBy,openedDate,assignee,resolvedBy,resolvedDate,resolution,deadline,url
    #[arg(
        long,
        num_args = 0..=1,
        default_missing_value = "",
        require_equals = true,
        value_name = "FIELDS"
    )]
    pub(crate) json: Option<String>,
}

pub(crate) fn run(args: BugListArgs, global: &GlobalArgs) -> Result<()> {
    validate_optional_json_fields(args.json.as_deref(), LIST_JSON_FIELDS)?;
    let query = BugSearchQuery::from(&args);
    let (site_url, result) = execute_bug_search(&query, global)?;
    if let Some(fields) = args.json.as_deref() {
        let json = render_list_json(&result, &site_url, fields)?;
        print_json(&json)?;
    } else {
        let plain = args.plain;
        print!(
            "{}",
            render_bug_list_table(
                &result,
                args.full_title,
                &site_url,
                !plain && hyperlinks_enabled(),
                !plain,
            )
        );
        if let Some(line) = stats::format_resolved_date_range_line(
            query.resolved_from.as_deref(),
            query.resolved_to.as_deref(),
        ) {
            let styled = !plain && ansi_enabled();
            println!("{}", stats::paint_date_meta(&line, styled));
        }
    }
    Ok(())
}

pub(crate) fn render_list_json(
    result: &search::SearchResult,
    site: &str,
    fields: &str,
) -> Result<Value> {
    let fields = parse_json_fields(fields, LIST_JSON_FIELDS)?;
    Ok(Value::Array(
        result
            .bugs
            .iter()
            .map(|bug| {
                let mut out = Map::new();
                for field in &fields {
                    out.insert(field.to_string(), list_json_value(bug, field, site));
                }
                Value::Object(out)
            })
            .collect(),
    ))
}

pub(crate) fn render_bug_list_table(
    result: &search::SearchResult,
    full_title: bool,
    site: &str,
    hyperlinks: bool,
    styled: bool,
) -> String {
    if result.bugs.is_empty() {
        return "没有找到 Bug\n".to_string();
    }
    // Human table headers are Chinese (same convention as bug stats); JSON field names stay English.
    let header = format!(
        "{} {} {} {} {} {}",
        pad_to_display_width("编号", BUG_LIST_ID_WIDTH),
        pad_to_display_width("状态", BUG_LIST_STATE_WIDTH),
        pad_to_display_width("创建者", BUG_LIST_OPENED_BY_WIDTH),
        pad_to_display_width("创建日期", BUG_LIST_OPENED_DATE_WIDTH),
        pad_to_display_width("标题", BUG_LIST_TITLE_WIDTH),
        pad_to_display_width("指派给", BUG_LIST_ASSIGNEE_WIDTH),
    );
    let mut out = format!(
        "{}\n",
        if styled {
            style_header(&header)
        } else {
            header
        }
    );
    for bug in &result.bugs {
        let state = canonical_state(&bug.status);
        let state_cell = pad_to_display_width(state, BUG_LIST_STATE_WIDTH);
        let state = if styled {
            colorize_state(&state_cell, state)
        } else {
            state_cell
        };
        let mut title = if full_title {
            normalize_table_cell(&bug.title)
        } else {
            truncate_for_table(&bug.title, BUG_LIST_TITLE_WIDTH)
        };
        if hyperlinks {
            title = osc8_hyperlink(&view::canonical_bug_url(site, bug.id), &title);
        }
        out.push_str(&format!(
            "{} {} {} {} {} {}\n",
            pad_to_display_width(&bug.id.to_string(), BUG_LIST_ID_WIDTH),
            state,
            truncate_for_table(&bug.opened_by, BUG_LIST_OPENED_BY_WIDTH),
            pad_to_display_width(bug.opened_date.trim(), BUG_LIST_OPENED_DATE_WIDTH),
            title,
            truncate_for_table(&bug.assigned_to, BUG_LIST_ASSIGNEE_WIDTH),
        ));
    }
    if io::stdout().is_terminal() {
        if let Some(total) = result.total.as_deref() {
            out.push_str(&format!("\n{}\n", total.trim()));
        }
    }
    out
}

/// Kitty / modern terminals: OSC 8 hyperlink around visible text.
pub(crate) fn osc8_hyperlink(url: &str, text: &str) -> String {
    format!("\x1b]8;;{url}\x1b\\{text}\x1b]8;;\x1b\\")
}

fn list_json_value(bug: &search::BugRow, field: &str, site: &str) -> Value {
    match field {
        "id" => json!(bug.id),
        "title" => json!(bug.title),
        "state" => json!(canonical_state(&bug.status)),
        "severity" => json!(bug.severity.parse::<u8>().ok()),
        "priority" => json!(bug.pri.parse::<u8>().ok()),
        "confirmed" => json!(is_confirmed(&bug.confirmed)),
        "openedBy" => nullable_text(&bug.opened_by),
        "openedDate" => nullable_date(&bug.opened_date),
        "assignee" => nullable_text(&bug.assigned_to),
        "resolvedBy" => nullable_text(&bug.resolved_by),
        "resolvedDate" => nullable_date(&bug.resolved_date),
        "resolution" => nullable_text(&bug.resolution),
        "deadline" => nullable_date(&bug.deadline),
        "url" => json!(view::canonical_bug_url(site, bug.id)),
        _ => Value::Null,
    }
}

fn nullable_text(raw: &str) -> Value {
    let value = raw.trim();
    if value.is_empty() || value == "--" {
        Value::Null
    } else {
        json!(value)
    }
}

fn nullable_date(raw: &str) -> Value {
    let value = raw.trim();
    if value.is_empty()
        || matches!(
            value,
            "--" | "0000-00-00" | "00-00 00:00" | "0000-00-00 00:00:00"
        )
    {
        Value::Null
    } else {
        json!(value)
    }
}

fn is_confirmed(raw: &str) -> bool {
    raw.trim() == "1"
}

fn canonical_state(raw: &str) -> &'static str {
    match raw.trim() {
        "激活" | "active" => "active",
        "已解决" | "resolved" => "resolved",
        "已关闭" | "closed" => "closed",
        _ => "unknown",
    }
}

fn hyperlinks_enabled() -> bool {
    io::stdout().is_terminal()
}

fn colorize_state(value: &str, state: &str) -> String {
    let color = match state {
        "active" => "33",
        "resolved" => "32",
        "closed" => "90",
        _ => "31",
    };
    crate::cli::style_ansi(value, color)
}

fn normalize_table_cell(value: &str) -> String {
    value.trim().replace(['\n', '\r'], " ")
}

pub(crate) fn truncate_for_table(value: &str, width: usize) -> String {
    let value = normalize_table_cell(value);
    if UnicodeWidthStr::width(value.as_str()) <= width {
        return pad_to_display_width(&value, width);
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
    pad_to_display_width(&truncated, width)
}

fn pad_to_display_width(value: &str, width: usize) -> String {
    let padding = width.saturating_sub(UnicodeWidthStr::width(value));
    format!("{value}{}", " ".repeat(padding))
}
