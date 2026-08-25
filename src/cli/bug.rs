pub(crate) mod candidates;
pub(crate) mod list;
pub(crate) mod stats;
pub(crate) mod view;

use crate::api::ZentaoApi;
use crate::cache;
use crate::cli::{
    debug_enabled, load_cookie_for_site, resolve_config_path, resolve_required, GlobalArgs,
};
use crate::config;
use crate::search;
use anyhow::{anyhow, Context, Result};
use chrono::{Datelike, Duration, Local, NaiveDate};
use clap::{Args, Subcommand, ValueEnum};
use std::collections::HashMap;
use std::fs;

#[derive(Debug, Args)]
pub(crate) struct BugArgs {
    #[command(subcommand)]
    pub(crate) command: BugSubCommands,
}

#[derive(Debug, Subcommand)]
pub(crate) enum BugSubCommands {
    List(list::BugListArgs),
    Stats(stats::BugStatsArgs),
    View(view::BugViewArgs),
    /// 列出筛选候选（--build 版本 / --module 模块）
    Candidates(candidates::BugCandidatesArgs),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(crate) enum BugSortField {
    #[value(name = "assignedDate")]
    AssignedDate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(crate) enum BugSortOrder {
    Asc,
    Desc,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(crate) enum BugState {
    Active,
    Resolved,
    Closed,
    All,
}

impl BugState {
    pub(crate) fn zentao_value(self) -> Option<&'static str> {
        match self {
            Self::Active => Some("active"),
            Self::Resolved => Some("resolved"),
            Self::Closed => Some("closed"),
            Self::All => None,
        }
    }
}

/// Shared list/stats search filters after clap parsing.
#[derive(Debug, Clone)]
pub(crate) struct BugSearchQuery {
    pub(crate) title: Vec<String>,
    pub(crate) assignee: Option<String>,
    pub(crate) opened_by: Vec<String>,
    pub(crate) resolved_by: Option<String>,
    pub(crate) resolved_from: Option<String>,
    pub(crate) resolved_to: Option<String>,
    pub(crate) module: Option<String>,
    pub(crate) opened_build: Option<String>,
    pub(crate) resolved_build: Option<String>,
    pub(crate) state: Vec<BugState>,
    pub(crate) sort: Option<BugSortField>,
    pub(crate) order: Option<BugSortOrder>,
    pub(crate) product: Option<u64>,
    pub(crate) limit: u32,
}

impl BugSearchQuery {
    pub(crate) fn zentao_order_by(&self) -> Option<String> {
        let field = self.sort?;
        let dir = match self.order.unwrap_or(BugSortOrder::Desc) {
            BugSortOrder::Asc => "asc",
            BugSortOrder::Desc => "desc",
        };
        Some(format!("{}_{dir}", field.zentao_name()))
    }
}

impl BugSortField {
    fn zentao_name(self) -> &'static str {
        match self {
            Self::AssignedDate => "assignedDate",
        }
    }
}

impl From<&list::BugListArgs> for BugSearchQuery {
    fn from(args: &list::BugListArgs) -> Self {
        let (resolved_from, resolved_to) = resolve_resolved_date_range(
            args.week,
            args.month,
            args.day,
            args.resolved_from.clone(),
            args.resolved_to.clone(),
            Local::now().date_naive(),
        );
        Self {
            title: args.title.clone(),
            assignee: args.assignee.clone(),
            opened_by: args.opened_by.clone(),
            resolved_by: args.resolved_by.clone(),
            resolved_from,
            resolved_to,
            module: args.module.clone(),
            opened_build: args.opened_build.clone(),
            resolved_build: args.resolved_build.clone(),
            state: args.state.clone(),
            sort: args.sort,
            order: args.order,
            product: args.product,
            limit: args.limit,
        }
    }
}

impl From<&stats::BugStatsArgs> for BugSearchQuery {
    fn from(args: &stats::BugStatsArgs) -> Self {
        let (resolved_from, resolved_to) = resolve_resolved_date_range(
            args.week,
            args.month,
            args.day,
            args.resolved_from.clone(),
            args.resolved_to.clone(),
            Local::now().date_naive(),
        );
        Self {
            title: args.title.clone(),
            assignee: args.assignee.clone(),
            opened_by: args.opened_by.clone(),
            resolved_by: args.resolved_by.clone(),
            resolved_from,
            resolved_to,
            module: args.module.clone(),
            opened_build: args.opened_build.clone(),
            resolved_build: args.resolved_build.clone(),
            state: args.state.clone(),
            sort: None,
            order: None,
            product: args.product,
            limit: args.limit,
        }
    }
}

pub(crate) fn run(args: BugArgs, global: &GlobalArgs) -> Result<()> {
    match args.command {
        BugSubCommands::List(args) => list::run(args, global),
        BugSubCommands::Stats(args) => stats::run(args, global),
        BugSubCommands::View(args) => view::run(args, global),
        BugSubCommands::Candidates(args) => candidates::run(args, global),
    }
}

/// Expand --week/--month/--day into resolvedDate bounds (inclusive, YYYY-MM-DD).
pub(crate) fn resolve_resolved_date_range(
    week: bool,
    month: bool,
    day: bool,
    resolved_from: Option<String>,
    resolved_to: Option<String>,
    today: NaiveDate,
) -> (Option<String>, Option<String>) {
    if week || month || day {
        let (from, to) = if week {
            reporting_week_bounds(today)
        } else if month {
            calendar_month_bounds(today)
        } else {
            (today, today)
        };
        return (
            Some(from.format("%Y-%m-%d").to_string()),
            Some(to.format("%Y-%m-%d").to_string()),
        );
    }
    (
        resolved_from.map(|s| strip_resolved_time_for_date(s.trim()).to_string()),
        resolved_to.map(|s| strip_resolved_time_for_date(s.trim()).to_string()),
    )
}

/// Keep calendar dates only for resolved bounds (drop trailing hms if present).
fn strip_resolved_time_for_date(raw: &str) -> &str {
    raw.split([' ', 'T'])
        .next()
        .filter(|s| !s.is_empty())
        .unwrap_or(raw)
}

/// Calendar week: Monday through Sunday (inclusive), containing `today`.
pub(crate) fn reporting_week_bounds(today: NaiveDate) -> (NaiveDate, NaiveDate) {
    let days_since_monday = today.weekday().num_days_from_monday() as i64;
    let start = today - Duration::days(days_since_monday);
    let end = start + Duration::days(6);
    (start, end)
}

pub(crate) fn calendar_month_bounds(today: NaiveDate) -> (NaiveDate, NaiveDate) {
    let start =
        NaiveDate::from_ymd_opt(today.year(), today.month(), 1).expect("valid first day of month");
    let end = if today.month() == 12 {
        NaiveDate::from_ymd_opt(today.year() + 1, 1, 1).expect("valid next year")
            - Duration::days(1)
    } else {
        NaiveDate::from_ymd_opt(today.year(), today.month() + 1, 1).expect("valid next month")
            - Duration::days(1)
    };
    (start, end)
}

pub(crate) fn validate_sort_args(query: &BugSearchQuery) -> Result<()> {
    if query.order.is_some() && query.sort.is_none() {
        return Err(anyhow!("--order 需要同时指定 --sort"));
    }
    Ok(())
}

pub(crate) fn execute_bug_search(
    query: &BugSearchQuery,
    global: &GlobalArgs,
) -> Result<(String, search::SearchResult)> {
    validate_search_group_limits(query)?;
    validate_sort_args(query)?;

    let cfg_path = resolve_config_path(global.config.as_deref())?;
    let cfg = config::load_config_optional(&cfg_path)?;

    let site_url = resolve_required(
        global.site.as_deref(),
        cfg.as_ref().map(|c| c.site.as_str()),
        "site",
    )?;

    let product = query
        .product
        .or_else(|| cfg.as_ref().and_then(|c| c.product))
        .ok_or_else(|| anyhow!("缺少 product，请通过 --product、ZENTAO_PRODUCT 或配置文件提供"))?;
    if product == 0 {
        return Err(anyhow!("product 必须是正整数"));
    }

    let api_client = ZentaoApi::new(&site_url)?;
    let cookie = load_cookie_for_site(&site_url, None, cfg.as_ref())?;
    let search_cookie_header = append_search_cookie_page_size(&cookie.cookie_header, query.limit);
    let cache_dir = cache::default_dir()?;
    let mut query = query.clone();
    if needs_build_name_lookup(&query) {
        let builds = cache::load_or_fetch(&cache_dir, product, search::KIND_BUILD, || {
            api_client.fetch_product_browse_json(&cookie.cookie_header, product)
        })?;
        if let Some(raw) = query.opened_build.take() {
            query.opened_build = Some(search::resolve_build_value(&raw, &builds)?);
        }
        if let Some(raw) = query.resolved_build.take() {
            query.resolved_build = Some(search::resolve_build_value(&raw, &builds)?);
        }
        if debug_enabled() {
            eprintln!(
                "[debug] resolved opened-build={:?} resolved-build={:?}",
                query.opened_build, query.resolved_build
            );
        }
    }
    if needs_module_name_lookup(&query) {
        let modules = cache::load_or_fetch(&cache_dir, product, search::KIND_MODULE, || {
            api_client.fetch_product_browse_json(&cookie.cookie_header, product)
        })?;
        if let Some(raw) = query.module.take() {
            query.module = Some(search::resolve_module_value(&raw, &modules)?);
        }
        if debug_enabled() {
            eprintln!("[debug] resolved module={:?}", query.module);
        }
    }
    let field_params = build_search_field_params(&query);

    if debug_enabled() {
        let debug_form = api_client.debug_search_form(product, &field_params)?;
        let compact_form = compact_debug_search_form(&debug_form);
        eprintln!("[debug] search-buildQuery form (andOr1..formType):");
        for line in render_compact_debug_form_lines(&compact_form) {
            eprintln!("{}", line);
        }
        eprintln!("[debug] search-buildQuery lisp:");
        eprintln!("{}", render_search_form_lisp(&compact_form));
    }

    let order_by = query.zentao_order_by();
    let json_body = api_client.search_browse_json(
        &search_cookie_header,
        product,
        &field_params,
        order_by.as_deref(),
    )?;
    if let Ok(debug_path) = std::env::var("ZENTAO_DEBUG_JSON") {
        fs::write(&debug_path, &json_body)
            .with_context(|| format!("写入调试 JSON 失败: {debug_path}"))?;
        eprintln!("[debug] 浏览 JSON 已写入 {debug_path}");
    }
    let mut result = search::parse_browse_json(&json_body)?;
    if let Err(err) = cache::save_from_browse_json(&cache_dir, product, &json_body) {
        if debug_enabled() {
            eprintln!("[debug] 写入候选缓存失败: {err}");
        }
    }
    apply_result_limit(&mut result, query.limit);
    Ok((site_url, result))
}

fn needs_build_name_lookup(query: &BugSearchQuery) -> bool {
    query
        .opened_build
        .as_deref()
        .is_some_and(|value| !search::is_build_id(value))
        || query
            .resolved_build
            .as_deref()
            .is_some_and(|value| !search::is_build_id(value))
}

fn needs_module_name_lookup(query: &BugSearchQuery) -> bool {
    query
        .module
        .as_deref()
        .is_some_and(|value| !search::is_build_id(value))
}

pub(crate) fn apply_result_limit(result: &mut search::SearchResult, limit: u32) {
    result.bugs.truncate(limit as usize);
}

pub(crate) fn validate_search_group_limits(args: &BugSearchQuery) -> Result<()> {
    // Zentao search-buildQuery uses 2 groups with 3 slots each:
    // group1: slot1~3, group2: slot4~6.
    // Multi-value --title / --opened-by / --state each occupy one full group as OR.
    let title_count = args.title.iter().filter(|v| !v.trim().is_empty()).count();
    let opened_by_count = args
        .opened_by
        .iter()
        .filter(|v| !v.trim().is_empty())
        .count();
    let state_raw_count = args.state.len();
    if title_count > 3 {
        return Err(anyhow!(
            "重复 --title 最多支持 3 个值（当前 {} 个）",
            title_count
        ));
    }
    if opened_by_count > 3 {
        return Err(anyhow!(
            "重复 --opened-by 最多支持 3 个值（当前 {} 个）",
            opened_by_count
        ));
    }
    if state_raw_count > 3 {
        return Err(anyhow!(
            "重复 --state 最多支持 3 个值（当前 {} 个）",
            state_raw_count
        ));
    }
    let has_all = args
        .state
        .iter()
        .any(|state| matches!(state, BugState::All));
    let state_values = status_search_values(&args.state);
    if has_all && !state_values.is_empty() {
        return Err(anyhow!("不能将 --state all 与其他状态组合"));
    }
    let state_count = state_values.len();

    let has_title_or = title_count >= 2;
    let has_opened_by_or = opened_by_count >= 2;
    let has_state_or = state_count >= 2;
    let or_count = [has_title_or, has_opened_by_or, has_state_or]
        .into_iter()
        .filter(|flag| *flag)
        .count();

    let mut singles = 0usize;
    if args.module.is_some() {
        singles += 1;
    }
    if args.assignee.is_some() {
        singles += 1;
    }
    if args.resolved_by.is_some() {
        singles += 1;
    }
    if args.resolved_from.is_some() {
        singles += 1;
    }
    if args.resolved_to.is_some() {
        singles += 1;
    }
    if args.opened_build.is_some() {
        singles += 1;
    }
    if args.resolved_build.is_some() {
        singles += 1;
    }
    if title_count == 1 {
        singles += 1;
    }
    if opened_by_count == 1 {
        singles += 1;
    }
    if state_count == 1 {
        singles += 1;
    }

    let hint = active_state_slot_hint(&args.state);
    if or_count >= 3 {
        return Err(anyhow!(
            "重复 --title、重复 --opened-by 与重复 --state 超过两组 OR 条件。{}",
            hint
        ));
    }
    if or_count == 2 {
        if singles > 0 {
            return Err(anyhow!(
                "{}已分别占满两组条件槽，不能再叠加其他筛选。{}",
                or_pair_labels(has_title_or, has_opened_by_or, has_state_or),
                hint
            ));
        }
    } else if or_count == 1 {
        let group2 = if has_title_or {
            title_count
        } else if has_opened_by_or {
            opened_by_count
        } else {
            state_count
        };
        if singles > 3 {
            return Err(anyhow!(
                "每个搜索 group 最多支持 3 个条件（group1={}，group2={}）。{}",
                singles,
                group2,
                hint
            ));
        }
    } else if singles > 6 {
        return Err(anyhow!(
            "当前搜索条件超过 6 个（实际 {} 个），请减少条件。{}",
            singles,
            hint
        ));
    }

    Ok(())
}

fn status_search_values(states: &[BugState]) -> Vec<&'static str> {
    let mut values = Vec::new();
    for state in states {
        if let Some(value) = state.zentao_value() {
            if !values.contains(&value) {
                values.push(value);
            }
        }
    }
    values
}

fn or_pair_labels(title: bool, opened_by: bool, state: bool) -> String {
    let mut names = Vec::new();
    if title {
        names.push("重复 --title");
    }
    if opened_by {
        names.push("重复 --opened-by");
    }
    if state {
        names.push("重复 --state");
    }
    match names.as_slice() {
        [a, b] => format!("{a} 与 {b} "),
        other => format!("{} ", other.join("、")),
    }
}

fn active_state_slot_hint(states: &[BugState]) -> &'static str {
    let values = status_search_values(states);
    if values.len() == 1 && values[0] == "active" {
        "active 状态（list 默认）占用一个条件槽位；如不需要状态筛选，请使用 --state all 释放该槽位"
    } else if values.len() >= 2 {
        "重复 --state 占用一组条件槽；如不需要状态筛选，请使用 --state all 释放该槽位"
    } else {
        ""
    }
}

pub(crate) fn build_search_field_params(query: &BugSearchQuery) -> Vec<(String, String)> {
    let mut field_params: Vec<(String, String)> = Vec::new();

    let title_values: Vec<String> = query
        .title
        .iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    if title_values.len() >= 2 {
        for (idx, title) in title_values.iter().take(3).enumerate() {
            field_params.push((format!("title_or_{}", idx + 1), title.clone()));
        }
    } else if let Some(keyword) = title_values.first() {
        field_params.push(("title".to_string(), keyword.clone()));
    }

    let opened_by_values: Vec<String> = query
        .opened_by
        .iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    if opened_by_values.len() >= 2 {
        for (idx, user) in opened_by_values.iter().take(3).enumerate() {
            field_params.push((format!("opened_by_or_{}", idx + 1), user.clone()));
        }
    } else if let Some(user) = opened_by_values.first() {
        field_params.push(("openedBy".to_string(), user.clone()));
    }

    if let Some(ref user) = query.assignee {
        field_params.push(("assignedTo".to_string(), user.clone()));
    }
    if let Some(ref user) = query.resolved_by {
        field_params.push(("resolvedBy".to_string(), user.clone()));
    }
    if let Some(ref date_from) = query.resolved_from {
        field_params.push(("resolvedDate_from".to_string(), date_from.clone()));
    }
    if let Some(ref date_to) = query.resolved_to {
        field_params.push(("resolvedDate_to".to_string(), date_to.clone()));
    }
    if let Some(ref module) = query.module {
        field_params.push(("module".to_string(), module.clone()));
    }
    if let Some(ref build) = query.opened_build {
        field_params.push(("openedBuild".to_string(), build.clone()));
    }
    if let Some(ref build) = query.resolved_build {
        field_params.push(("resolvedBuild".to_string(), build.clone()));
    }
    let status_values = status_search_values(&query.state);
    if status_values.len() >= 2 {
        for (idx, status) in status_values.iter().take(3).enumerate() {
            field_params.push((format!("status_or_{}", idx + 1), (*status).to_string()));
        }
    } else if let Some(status) = status_values.first() {
        field_params.push(("status".to_string(), (*status).to_string()));
    }
    field_params
}

fn append_search_cookie_page_size(base_cookie: &str, page_size: u32) -> String {
    let base = base_cookie.trim().trim_end_matches(';').trim();
    if page_size > 0 {
        if base.is_empty() {
            format!("pagerBugBrowse={page_size}")
        } else {
            format!("{base}; pagerBugBrowse={page_size}")
        }
    } else {
        base.to_string()
    }
}

fn compact_debug_search_form(form: &[(String, String)]) -> Vec<(String, String)> {
    let keys = [
        "andOr1",
        "field1",
        "operator1",
        "value1",
        "andOr2",
        "field2",
        "operator2",
        "value2",
        "andOr3",
        "field3",
        "operator3",
        "value3",
        "groupAndOr",
        "andOr4",
        "field4",
        "operator4",
        "value4",
        "andOr5",
        "field5",
        "operator5",
        "value5",
        "andOr6",
        "field6",
        "operator6",
        "value6",
        "module",
        "actionURL",
        "groupItems",
        "formType",
    ];
    let mut map: HashMap<&str, &str> = HashMap::new();
    for (k, v) in form {
        map.insert(k.as_str(), v.as_str());
    }
    keys.iter()
        .filter_map(|k| map.get(k).map(|v| (k.to_string(), (*v).to_string())))
        .collect()
}

fn render_search_form_lisp(form: &[(String, String)]) -> String {
    #[derive(Clone)]
    struct Clause {
        and_or: String,
        field: String,
        operator: String,
        value: String,
    }

    let map: HashMap<&str, &str> = form.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    let clauses: Vec<Clause> = (1..=6)
        .map(|n| Clause {
            and_or: map
                .get(format!("andOr{n}").as_str())
                .copied()
                .unwrap_or("")
                .to_ascii_lowercase(),
            field: map
                .get(format!("field{n}").as_str())
                .copied()
                .unwrap_or("")
                .to_string(),
            operator: map
                .get(format!("operator{n}").as_str())
                .copied()
                .unwrap_or("")
                .to_string(),
            value: map
                .get(format!("value{n}").as_str())
                .copied()
                .unwrap_or("")
                .to_string(),
        })
        .collect();

    fn atom(c: &Clause) -> String {
        let escaped = c.value.replace('\\', "\\\\").replace('"', "\\\"");
        format!("({} {} \"{}\")", c.field, c.operator, escaped)
    }

    fn group_expr(clauses: &[Clause]) -> Option<String> {
        let filtered: Vec<&Clause> = clauses
            .iter()
            .filter(|c| !c.value.trim().is_empty() && !c.field.trim().is_empty())
            .collect();
        if filtered.is_empty() {
            return None;
        }
        let mut expr = atom(filtered[0]);
        for c in filtered.iter().skip(1) {
            let op = if c.and_or == "or" { "or" } else { "and" };
            expr = format!("({op} {expr} {})", atom(c));
        }
        Some(expr)
    }

    let g1 = group_expr(&clauses[0..3]);
    let g2 = group_expr(&clauses[3..6]);
    match (g1, g2) {
        (Some(left), Some(right)) => {
            let op = if map
                .get("groupAndOr")
                .copied()
                .unwrap_or("and")
                .eq_ignore_ascii_case("or")
            {
                "or"
            } else {
                "and"
            };
            format!("({op} {left} {right})")
        }
        (Some(expr), None) | (None, Some(expr)) => expr,
        (None, None) => "()".to_string(),
    }
}

fn render_compact_debug_form_lines(form: &[(String, String)]) -> Vec<String> {
    let map: HashMap<&str, &str> = form.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    let mut lines = Vec::new();

    for n in 1..=6 {
        let and_or = map.get(format!("andOr{n}").as_str()).copied();
        let field = map.get(format!("field{n}").as_str()).copied();
        let operator = map.get(format!("operator{n}").as_str()).copied();
        let value = map.get(format!("value{n}").as_str()).copied();
        if and_or.is_some() || field.is_some() || operator.is_some() || value.is_some() {
            lines.push(format!(
                "andOr{n}={} field{n}={} operator{n}={} value{n}={}",
                and_or.unwrap_or(""),
                field.unwrap_or(""),
                operator.unwrap_or(""),
                value.unwrap_or("")
            ));
        }
        if n == 3 && map.contains_key("groupAndOr") {
            lines.push(format!("groupAndOr={}", map["groupAndOr"]));
        }
    }

    if let Some(v) = map.get("module").copied() {
        lines.push(format!("module={v}"));
    }
    if let Some(v) = map.get("actionURL").copied() {
        lines.push(format!("actionURL={v}"));
    }
    if let Some(v) = map.get("groupItems").copied() {
        lines.push(format!("groupItems={v}"));
    }
    if let Some(v) = map.get("formType").copied() {
        lines.push(format!("formType={v}"));
    }

    lines
}
