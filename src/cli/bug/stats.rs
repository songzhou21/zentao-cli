use crate::cli::bug::{execute_bug_search, BugSearchQuery, BugState};
use crate::cli::{
    ansi_enabled, print_json, style_warning, validate_optional_json_fields, GlobalArgs,
};
use crate::stats;
use anyhow::Result;
use clap::Args;

#[derive(Debug, Args)]
pub(crate) struct BugStatsArgs {
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

    /// 影响版本：ID 或唯一名称片段，例如 982 或 1.2.17-iOS
    #[arg(long, value_name = "BUILD")]
    pub(crate) opened_build: Option<String>,

    /// 解决版本：ID 或唯一名称片段，例如 982 或 1.2.17-iOS
    #[arg(long, value_name = "BUILD")]
    pub(crate) resolved_build: Option<String>,

    /// Bug 状态；默认 all（按指派人做全状态剖面）
    #[arg(short = 's', long, value_enum, default_value_t = BugState::All)]
    pub(crate) state: BugState,

    /// 产品 ID；未提供时从 ZENTAO_PRODUCT 或配置读取
    #[arg(long, env = "ZENTAO_PRODUCT", value_name = "ID")]
    pub(crate) product: Option<u64>,

    /// 最多聚合的 Bug 数量（样本上限，不保证全集）
    #[arg(
        short = 'L',
        long,
        default_value_t = stats::DEFAULT_LIMIT,
        value_parser = clap::value_parser!(u32).range(1..),
        value_name = "N"
    )]
    pub(crate) limit: u32,

    /// 纯文本表格：关闭表头颜色等交互装饰
    #[arg(long, default_value_t = false)]
    pub(crate) plain: bool,

    /// 输出 JSON；可选指定字段：assignee,active,resolved,solved,closed,total（resolved 在 pending）
    #[arg(
        long,
        num_args = 0..=1,
        default_missing_value = "",
        require_equals = true,
        value_name = "FIELDS"
    )]
    pub(crate) json: Option<String>,
}

pub(crate) fn run(args: BugStatsArgs, global: &GlobalArgs) -> Result<()> {
    validate_optional_json_fields(args.json.as_deref(), stats::JSON_FIELDS)?;
    let query = BugSearchQuery::from(&args);
    let (_site_url, result) = execute_bug_search(&query, global)?;
    let report = stats::aggregate(
        &result.bugs,
        args.limit,
        stats::fetched_at_now(),
        query.resolved_from.clone(),
        query.resolved_to.clone(),
    );
    if report.incomplete {
        eprintln!("{}", style_warning(&stats::incomplete_warning(&report)));
    }
    if let Some(fields) = args.json.as_deref() {
        let json = stats::render_json(&report, fields)?;
        print_json(&json)?;
    } else {
        print!(
            "{}",
            stats::render_table(&report, !args.plain && ansi_enabled())
        );
    }
    Ok(())
}
