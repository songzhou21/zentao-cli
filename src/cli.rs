use crate::api::ZentaoApi;
use crate::browser;
use crate::bug;
use crate::config;
use crate::config::CookieSource;
use crate::cookie_store;
use crate::search;
use anyhow::{anyhow, Context, Result};
use chrono::{Datelike, Duration, Local, NaiveDate, TimeZone, Utc};
use clap::{Args, Parser, Subcommand, ValueEnum};
use regex::Regex;
use reqwest::Url;
use serde_json::{json, Map, Value};
use std::collections::{HashMap, HashSet};
use std::ffi::OsString;
use std::fs;
use std::io::{self, IsTerminal, Read, Write};
use std::path::{Path, PathBuf};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

const IMAGE_DOWNLOAD_DIR: &str = "/tmp/zentao-images";
const BUG_LIST_ID_WIDTH: usize = 6;
const BUG_LIST_STATE_WIDTH: usize = 9;
const BUG_LIST_TITLE_WIDTH: usize = 65;
const BUG_LIST_ASSIGNEE_WIDTH: usize = 10;
const BUG_STATS_ASSIGNEE_WIDTH: usize = 16;
const BUG_STATS_COUNT_WIDTH: usize = 8;
const BUG_STATS_UNASSIGNED: &str = "(未指派)";
/// Closed bugs are not attributed to the list assignee (often the literal "Closed").
const BUG_STATS_CLOSED_BUCKET: &str = "(已关闭)";
const BUG_STATS_TOTAL_LABEL: &str = "合计";
const BUG_STATS_DEFAULT_LIMIT: u32 = 1000;

#[derive(Debug, Parser)]
#[command(name = "zentao", version, about = "在终端管理禅道 Bug")]
struct Cli {
    #[command(flatten)]
    global: GlobalArgs,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Args)]
struct GlobalArgs {
    /// 禅道站点基础 URL（可包含部署子路径）
    #[arg(long, global = true, env = "ZENTAO_SITE")]
    site: Option<String>,
    /// 配置文件路径
    #[arg(long, global = true, env = "ZENTAO_CONFIG")]
    config: Option<String>,
}

#[derive(Debug, Subcommand)]
enum Commands {
    Auth(AuthArgs),
    Bug(BugArgs),
    Config(ConfigArgs),
    Image(ImageArgs),
}

#[derive(Debug, Args)]
struct AuthArgs {
    #[command(subcommand)]
    command: AuthSubCommands,
}

#[derive(Debug, Subcommand)]
enum AuthSubCommands {
    Login(LoginArgs),
    Status(AuthStatusArgs),
    SelectChromeProfile(ProfileArgs),
}

#[derive(Debug, Args)]
struct LoginArgs {
    /// 禅道用户名
    #[arg(long)]
    username: String,
    /// 从标准输入读取密码
    #[arg(long)]
    password_stdin: bool,
    #[arg(long)]
    proxy: Option<String>,
}

#[derive(Debug, Args)]
struct AuthStatusArgs {
    /// 临时覆盖 Chrome Profile 路径
    #[arg(long)]
    profile: Option<String>,
    /// 显示完整 Cookie 值
    #[arg(long)]
    show_cookie_values: bool,
}

#[derive(Debug, Args)]
struct ProfileArgs {}

#[derive(Debug, Args)]
struct BugArgs {
    #[command(subcommand)]
    command: BugSubCommands,
}

#[derive(Debug, Subcommand)]
enum BugSubCommands {
    List(BugListArgs),
    Stats(BugStatsArgs),
    View(BugViewArgs),
}

#[derive(Debug, Args)]
struct BugViewArgs {
    /// Bug ID，或包含 bug-view-<id>.html 的完整 URL
    #[arg(value_name = "ID|URL")]
    bug: String,
    /// 将 Markdown 写入文件
    #[arg(short = 'o', long, conflicts_with = "json")]
    output: Option<String>,
    /// 输出 JSON；可选指定字段：id,title,description,history,images,attachments,url
    #[arg(
        long,
        num_args = 0..=1,
        default_missing_value = "",
        require_equals = true,
        value_name = "FIELDS"
    )]
    json: Option<String>,
}

#[derive(Debug, Args)]
struct ImageArgs {
    #[command(subcommand)]
    command: ImageSubCommands,
}

#[derive(Debug, Subcommand)]
enum ImageSubCommands {
    Download(ImageDownloadArgs),
}

#[derive(Debug, Args)]
struct ImageDownloadArgs {
    /// 图片 URL（仅支持 http/https）
    #[arg(long)]
    url: String,
    /// 下载输出目录，默认 `/tmp/zentao-images`
    #[arg(short = 'o', long)]
    output_dir: Option<String>,
}

#[derive(Debug, Args)]
struct BugListArgs {
    /// 标题关键词（包含匹配）。可重复传入，多个值按 OR 处理，例如 --title A --title B
    #[arg(long, value_name = "KEYWORD")]
    title: Vec<String>,

    /// 指派给（用户名），例如 zhousong
    #[arg(short = 'a', long, value_name = "USER")]
    assignee: Option<String>,

    /// 解决者（用户名），例如 zhousong
    #[arg(long, value_name = "USER")]
    resolved_by: Option<String>,

    /// 解决日期起始（含），格式 YYYY-MM-DD
    #[arg(long, value_name = "DATE")]
    resolved_from: Option<String>,

    /// 解决日期截止（含），格式 YYYY-MM-DD
    #[arg(long, value_name = "DATE")]
    resolved_to: Option<String>,

    /// 解决日期快捷：本周一～本周日（含），与 --month/--day/--resolved-from/--resolved-to 互斥
    #[arg(long, conflicts_with_all = ["month", "day", "resolved_from", "resolved_to"])]
    week: bool,

    /// 解决日期快捷：本月 1 日～月末（含），与 --week/--day/--resolved-from/--resolved-to 互斥
    #[arg(long, conflicts_with_all = ["week", "day", "resolved_from", "resolved_to"])]
    month: bool,

    /// 解决日期快捷：今天，与 --week/--month/--resolved-from/--resolved-to 互斥
    #[arg(long, conflicts_with_all = ["week", "month", "resolved_from", "resolved_to"])]
    day: bool,

    /// 所属模块 ID，例如 1099
    #[arg(long, value_name = "MODULE_ID")]
    module: Option<String>,

    /// Bug 状态；默认 active
    #[arg(short = 's', long, value_enum, default_value_t = BugState::Active)]
    state: BugState,

    /// 产品 ID；未提供时从 ZENTAO_PRODUCT 或配置读取
    #[arg(long, env = "ZENTAO_PRODUCT", value_name = "ID")]
    product: Option<u64>,

    /// 最多返回的 Bug 数量
    #[arg(
        short = 'L',
        long,
        default_value_t = 30,
        value_parser = clap::value_parser!(u32).range(1..),
        value_name = "N"
    )]
    limit: u32,

    /// 表格输出时展示完整标题（默认按显示宽度截断）；不影响搜索条件或 JSON
    #[arg(long, default_value_t = false)]
    full_title: bool,

    /// 纯文本表格：关闭超链接、颜色等交互装饰（仍可与 --full-title 并用）
    #[arg(long, default_value_t = false)]
    plain: bool,

    /// 输出 JSON；可选指定字段：id,title,state,severity,priority,confirmed,openedBy,openedDate,assignee,resolvedDate,resolution,deadline,url
    #[arg(
        long,
        num_args = 0..=1,
        default_missing_value = "",
        require_equals = true,
        value_name = "FIELDS"
    )]
    json: Option<String>,
}

#[derive(Debug, Args)]
struct BugStatsArgs {
    /// 标题关键词（包含匹配）。可重复传入，多个值按 OR 处理，例如 --title A --title B
    #[arg(long, value_name = "KEYWORD")]
    title: Vec<String>,

    /// 指派给（用户名），例如 zhousong
    #[arg(short = 'a', long, value_name = "USER")]
    assignee: Option<String>,

    /// 解决者（用户名），例如 zhousong
    #[arg(long, value_name = "USER")]
    resolved_by: Option<String>,

    /// 解决日期起始（含），格式 YYYY-MM-DD
    #[arg(long, value_name = "DATE")]
    resolved_from: Option<String>,

    /// 解决日期截止（含），格式 YYYY-MM-DD
    #[arg(long, value_name = "DATE")]
    resolved_to: Option<String>,

    /// 解决日期快捷：本周一～本周日（含），与 --month/--day/--resolved-from/--resolved-to 互斥
    #[arg(long, conflicts_with_all = ["month", "day", "resolved_from", "resolved_to"])]
    week: bool,

    /// 解决日期快捷：本月 1 日～月末（含），与 --week/--day/--resolved-from/--resolved-to 互斥
    #[arg(long, conflicts_with_all = ["week", "day", "resolved_from", "resolved_to"])]
    month: bool,

    /// 解决日期快捷：今天，与 --week/--month/--resolved-from/--resolved-to 互斥
    #[arg(long, conflicts_with_all = ["week", "month", "resolved_from", "resolved_to"])]
    day: bool,

    /// 所属模块 ID，例如 1099
    #[arg(long, value_name = "MODULE_ID")]
    module: Option<String>,

    /// Bug 状态；默认 all（按指派人做全状态剖面）
    #[arg(short = 's', long, value_enum, default_value_t = BugState::All)]
    state: BugState,

    /// 产品 ID；未提供时从 ZENTAO_PRODUCT 或配置读取
    #[arg(long, env = "ZENTAO_PRODUCT", value_name = "ID")]
    product: Option<u64>,

    /// 最多聚合的 Bug 数量（样本上限，不保证全集）
    #[arg(
        short = 'L',
        long,
        default_value_t = BUG_STATS_DEFAULT_LIMIT,
        value_parser = clap::value_parser!(u32).range(1..),
        value_name = "N"
    )]
    limit: u32,

    /// 纯文本表格：关闭表头颜色等交互装饰
    #[arg(long, default_value_t = false)]
    plain: bool,

    /// 输出 JSON；可选指定字段：assignee,active,resolved,closed,total
    #[arg(
        long,
        num_args = 0..=1,
        default_missing_value = "",
        require_equals = true,
        value_name = "FIELDS"
    )]
    json: Option<String>,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum BugState {
    Active,
    Resolved,
    Closed,
    All,
}

/// Shared list/stats search filters after clap parsing.
#[derive(Debug, Clone)]
struct BugSearchQuery {
    title: Vec<String>,
    assignee: Option<String>,
    resolved_by: Option<String>,
    resolved_from: Option<String>,
    resolved_to: Option<String>,
    module: Option<String>,
    state: BugState,
    product: Option<u64>,
    limit: u32,
}

impl From<&BugListArgs> for BugSearchQuery {
    fn from(args: &BugListArgs) -> Self {
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
            resolved_by: args.resolved_by.clone(),
            resolved_from,
            resolved_to,
            module: args.module.clone(),
            state: args.state,
            product: args.product,
            limit: args.limit,
        }
    }
}

impl From<&BugStatsArgs> for BugSearchQuery {
    fn from(args: &BugStatsArgs) -> Self {
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
            resolved_by: args.resolved_by.clone(),
            resolved_from,
            resolved_to,
            module: args.module.clone(),
            state: args.state,
            product: args.product,
            limit: args.limit,
        }
    }
}

/// Expand --week/--month/--day into resolvedDate bounds (inclusive, YYYY-MM-DD).
fn resolve_resolved_date_range(
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
fn reporting_week_bounds(today: NaiveDate) -> (NaiveDate, NaiveDate) {
    let days_since_monday = today.weekday().num_days_from_monday() as i64;
    let start = today - Duration::days(days_since_monday);
    let end = start + Duration::days(6);
    (start, end)
}

fn calendar_month_bounds(today: NaiveDate) -> (NaiveDate, NaiveDate) {
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

#[derive(Debug, Clone)]
struct AssigneeStatsRow {
    assignee: String,
    active: u32,
    resolved: u32,
    closed: u32,
    total: u32,
}

#[derive(Debug, Clone)]
struct BugStatsAggregate {
    rows: Vec<AssigneeStatsRow>,
    total: AssigneeStatsRow,
    sample_size: u32,
    limit: u32,
    incomplete: bool,
    /// Local wall-clock time when the search sample was fetched.
    fetched_at: String,
    /// Effective resolvedDate range after --week/--month/--day or explicit flags.
    resolved_from: Option<String>,
    resolved_to: Option<String>,
}

#[derive(Debug, Args)]
struct ConfigArgs {
    #[command(subcommand)]
    command: ConfigSubCommands,
}

#[derive(Debug, Subcommand)]
enum ConfigSubCommands {
    List,
    Get(ConfigGetArgs),
    Set(ConfigSetArgs),
}

#[derive(Debug, Args)]
struct ConfigGetArgs {
    key: ConfigKey,
}

#[derive(Debug, Args)]
struct ConfigSetArgs {
    key: ConfigKey,
    value: String,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum ConfigKey {
    Site,
    Product,
    CookieSource,
    ChromeProfile,
}

impl BugState {
    fn zentao_value(self) -> Option<&'static str> {
        match self {
            Self::Active => Some("active"),
            Self::Resolved => Some("resolved"),
            Self::Closed => Some("closed"),
            Self::All => None,
        }
    }
}

#[derive(Debug)]
pub enum RunError {
    Clap(clap::Error),
    Runtime(anyhow::Error),
}

impl std::fmt::Display for RunError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Clap(error) => error.fmt(formatter),
            Self::Runtime(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for RunError {}

pub fn run(args: Vec<OsString>) -> std::result::Result<(), RunError> {
    let cli = Cli::try_parse_from(std::iter::once(OsString::from("zentao")).chain(args))
        .map_err(RunError::Clap)?;

    match cli.command {
        Commands::Auth(args) => run_auth(args, &cli.global),
        Commands::Bug(args) => run_bug(args, &cli.global),
        Commands::Config(args) => run_config(args, &cli.global),
        Commands::Image(args) => run_image(args, &cli.global),
    }
    .map_err(RunError::Runtime)
}

fn run_auth(args: AuthArgs, global: &GlobalArgs) -> Result<()> {
    match args.command {
        AuthSubCommands::Login(args) => run_login(args, global),
        AuthSubCommands::Status(args) => run_auth_status(args, global),
        AuthSubCommands::SelectChromeProfile(args) => run_chrome_profile(args, global),
    }
}

fn run_config(args: ConfigArgs, global: &GlobalArgs) -> Result<()> {
    let cfg_path = resolve_config_path(global.config.as_deref())?;
    let mut cfg = config::load_or_default(&cfg_path)?;
    match args.command {
        ConfigSubCommands::List => {
            println!("site={}", cfg.site);
            println!(
                "product={}",
                cfg.product.map(|v| v.to_string()).unwrap_or_default()
            );
            println!("cookie-source={}", cookie_source_name(&cfg.cookie_source));
            println!(
                "chrome-profile={}",
                cfg.chrome_profile.as_deref().unwrap_or_default()
            );
        }
        ConfigSubCommands::Get(args) => println!("{}", config_value(&cfg, args.key)),
        ConfigSubCommands::Set(args) => {
            set_config_value(&mut cfg, args.key, &args.value)?;
            config::save_config(&cfg_path, &cfg)?;
        }
    }
    Ok(())
}

fn cookie_source_name(source: &CookieSource) -> &'static str {
    match source {
        CookieSource::Chrome => "chrome",
        CookieSource::File => "file",
    }
}

fn config_value(cfg: &config::Config, key: ConfigKey) -> String {
    match key {
        ConfigKey::Site => cfg.site.clone(),
        ConfigKey::Product => cfg.product.map(|v| v.to_string()).unwrap_or_default(),
        ConfigKey::CookieSource => cookie_source_name(&cfg.cookie_source).to_string(),
        ConfigKey::ChromeProfile => cfg.chrome_profile.clone().unwrap_or_default(),
    }
}

fn set_config_value(cfg: &mut config::Config, key: ConfigKey, raw: &str) -> Result<()> {
    let value = raw.trim();
    match key {
        ConfigKey::Site => {
            reqwest::Url::parse(value).context("site 必须是有效 URL")?;
            cfg.site = value.trim_end_matches('/').to_string();
        }
        ConfigKey::Product => {
            let product: u64 = value.parse().context("product 必须是正整数")?;
            if product == 0 {
                return Err(anyhow!("product 必须是正整数"));
            }
            cfg.product = Some(product);
        }
        ConfigKey::CookieSource => {
            cfg.cookie_source = match value {
                "chrome" => CookieSource::Chrome,
                "file" => CookieSource::File,
                _ => return Err(anyhow!("cookie-source 仅支持 chrome 或 file")),
            };
        }
        ConfigKey::ChromeProfile => {
            cfg.chrome_profile = (!value.is_empty()).then(|| value.to_string());
        }
    }
    Ok(())
}

fn run_auth_status(args: AuthStatusArgs, global: &GlobalArgs) -> Result<()> {
    let cfg_path = resolve_config_path(global.config.as_deref())?;
    let cfg = config::load_config_optional(&cfg_path)?;

    let site_url = resolve_required(
        global.site.as_deref(),
        cfg.as_ref().map(|c| c.site.as_str()),
        "site",
    )?;

    let profile = args
        .profile
        .as_deref()
        .map(str::to_string)
        .or_else(|| cfg.as_ref().and_then(|c| c.chrome_profile.clone()));

    let cookie = load_cookie_for_site(&site_url, profile.as_deref(), cfg.as_ref())?;
    let parsed_site = reqwest::Url::parse(&site_url).context("解析 URL 失败")?;
    let target_host = parsed_site
        .host_str()
        .ok_or_else(|| anyhow!("URL 缺少 host"))?
        .to_string();
    let mut matched_domains: Vec<String> =
        cookie.items.iter().map(|it| it.domain.clone()).collect();
    matched_domains.sort();
    matched_domains.dedup();

    println!("Cookie source: {}", cookie.profile_path);
    println!("目标域名: {}", target_host);
    println!();
    println!(
        "cookie 域名: {}",
        format_cookie_domains_line(&matched_domains)
    );
    println!();
    println!("cookie 状态:");
    print_cookie_presence(&cookie.items, "zentaosid");
    print_cookie_presence(&cookie.items, "za");
    print_cookie_presence(&cookie.items, "zp");
    print_cookie_presence(&cookie.items, "keepLogin");
    println!();
    println!("cookie 明细:");
    let rows = collect_cookie_table_rows(&cookie.items);
    for line in render_cookie_table(&rows, args.show_cookie_values) {
        println!("{}", line);
    }

    let client = ZentaoApi::new(&site_url)?;
    let final_url = client.verify_cookie(&cookie.cookie_header)?;
    println!("\nCookie 校验成功，最终跳转: {final_url}");

    Ok(())
}

fn run_login(args: LoginArgs, global: &GlobalArgs) -> Result<()> {
    let cfg_path = resolve_config_path(global.config.as_deref())?;
    let mut cfg = config::load_or_default(&cfg_path)?;
    let site_url = resolve_required(
        global.site.as_deref(),
        if cfg.site.is_empty() {
            None
        } else {
            Some(cfg.site.as_str())
        },
        "site",
    )?;

    if !args.password_stdin {
        return Err(anyhow!("请使用 --password-stdin 通过标准输入提供密码"));
    }
    let password = read_login_password()?;
    let api = ZentaoApi::new_with_proxy(&site_url, args.proxy.as_deref())?;
    let login = api.login_with_password(&args.username, &password, true)?;
    let persist_items = select_persist_cookie_items(&login.cookies);

    let cookie_file_path = config::default_cookie_file_path()?;
    cookie_store::save_cookie_file(&cookie_file_path, &site_url, &persist_items)?;
    cfg.site = site_url.clone();
    cfg.cookie_source = CookieSource::File;
    config::save_config(&cfg_path, &cfg)?;

    let parsed_login = parse_login_response(&login.login_response_body);
    match parsed_login.result.as_deref() {
        Some("success") => println!(
            "{}",
            style_success(&format!(
                "登录成功，cookie 已保存: {}",
                cookie_file_path.display()
            ))
        ),
        Some("fail") => println!("{}", style_error("登录失败")),
        _ => println!(
            "登录响应: {}",
            format_login_response(&login.login_response_body)
        ),
    }
    if let Some(message) = parsed_login.message.as_deref() {
        if !message.is_empty() {
            println!("服务端消息: {}", message);
        }
    }
    if global.site.is_some() {
        println!("已将 --site 保存为后续命令的默认 site: {site_url}");
    }
    Ok(())
}

fn read_login_password() -> Result<String> {
    let mut password = String::new();
    io::stdin()
        .read_to_string(&mut password)
        .context("读取标准输入密码失败")?;
    let password = password.trim_end_matches(['\r', '\n']).to_string();
    if password.is_empty() {
        return Err(anyhow!("密码不能为空"));
    }
    Ok(password)
}

fn select_persist_cookie_items(
    items: &[browser::BrowserCookieItem],
) -> Vec<browser::BrowserCookieItem> {
    let wanted = ["keepLogin", "za", "zp", "zentaosid"];
    let mut out = Vec::new();
    for name in wanted {
        if let Some(item) = items.iter().find(|it| it.name == name) {
            out.push(item.clone());
        }
    }
    out
}

fn run_chrome_profile(_args: ProfileArgs, global: &GlobalArgs) -> Result<()> {
    let cfg_path = resolve_config_path(global.config.as_deref())?;
    let mut cfg = config::load_or_default(&cfg_path)?;

    let profiles = browser::list_chrome_profiles_macos()?;
    if profiles.is_empty() {
        return Err(anyhow!("未找到可用的 Chrome profile"));
    }

    if let Some(current) = cfg.chrome_profile.as_deref() {
        println!("当前已选择: {}", current);
    } else {
        println!("当前已选择: (未设置)");
    }

    println!("可用 Chrome profiles:");
    for (idx, profile) in profiles.iter().enumerate() {
        let marker =
            current_profile_marker(cfg.chrome_profile.as_deref() == Some(profile.as_str()));
        println!("{}. {}{}", idx + 1, profile, marker);
    }

    print!("请输入编号（输入 q 退出）: ");
    io::stdout().flush().ok();

    let mut input = String::new();
    io::stdin().read_line(&mut input).context("读取输入失败")?;
    let input = input.trim();

    if input.eq_ignore_ascii_case("q") {
        println!("已取消选择");
        return Ok(());
    }

    let index: usize = input
        .parse()
        .map_err(|_| anyhow!("输入无效，请输入数字编号"))?;
    if index == 0 || index > profiles.len() {
        return Err(anyhow!("编号超出范围，请输入 1-{}", profiles.len()));
    }

    let selected = profiles[index - 1].clone();
    cfg.chrome_profile = Some(selected.clone());
    config::save_config(&cfg_path, &cfg)?;

    println!("已保存 chrome_profile: {}", selected);
    println!("配置文件: {}", cfg_path.display());
    Ok(())
}

fn run_bug(args: BugArgs, global: &GlobalArgs) -> Result<()> {
    match args.command {
        BugSubCommands::List(args) => run_bug_list(args, global),
        BugSubCommands::Stats(args) => run_bug_stats(args, global),
        BugSubCommands::View(args) => run_bug_view(args, global),
    }
}

fn run_image(args: ImageArgs, global: &GlobalArgs) -> Result<()> {
    match args.command {
        ImageSubCommands::Download(d) => run_image_download(d, global),
    }
}

fn run_bug_list(args: BugListArgs, global: &GlobalArgs) -> Result<()> {
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
        if let Some(line) = format_resolved_date_range_line(
            query.resolved_from.as_deref(),
            query.resolved_to.as_deref(),
        ) {
            let styled = !plain && ansi_enabled();
            println!("{}", paint_stats_date_meta(&line, styled));
        }
    }
    Ok(())
}

fn run_bug_stats(args: BugStatsArgs, global: &GlobalArgs) -> Result<()> {
    validate_optional_json_fields(args.json.as_deref(), STATS_JSON_FIELDS)?;
    let query = BugSearchQuery::from(&args);
    let (_site_url, result) = execute_bug_search(&query, global)?;
    let stats = aggregate_stats_by_assignee(
        &result.bugs,
        args.limit,
        stats_fetched_at_now(),
        query.resolved_from.clone(),
        query.resolved_to.clone(),
    );
    if stats.incomplete {
        eprintln!(
            "{}",
            style_warning(&format_stats_incomplete_warning(&stats))
        );
    }
    if let Some(fields) = args.json.as_deref() {
        let json = render_stats_json(&stats, fields)?;
        print_json(&json)?;
    } else {
        print!(
            "{}",
            render_bug_stats_table(&stats, !args.plain && ansi_enabled())
        );
    }
    Ok(())
}

fn execute_bug_search(
    query: &BugSearchQuery,
    global: &GlobalArgs,
) -> Result<(String, search::SearchResult)> {
    validate_search_group_limits(query)?;

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
    let field_params = build_search_field_params(query);

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

    let html = api_client.search_bugs(&search_cookie_header, product, &field_params)?;

    if let Ok(debug_path) = std::env::var("ZENTAO_DEBUG_HTML") {
        fs::write(&debug_path, &html)
            .with_context(|| format!("写入调试 HTML 失败: {debug_path}"))?;
        eprintln!("[debug] 搜索结果 HTML 已写入 {debug_path}");
    }

    let mut result = search::parse_search_result(&html)?;
    apply_result_limit(&mut result, query.limit);
    Ok((site_url, result))
}

fn build_search_field_params(query: &BugSearchQuery) -> Vec<(String, String)> {
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
    if let Some(status) = query.state.zentao_value() {
        field_params.push(("status".to_string(), status.to_string()));
    }
    field_params
}

fn apply_result_limit(result: &mut search::SearchResult, limit: u32) {
    result.bugs.truncate(limit as usize);
}

fn validate_search_group_limits(args: &BugSearchQuery) -> Result<()> {
    // Zentao search-buildQuery uses 2 groups with 3 slots each:
    // group1: slot1~3, group2: slot4~6.
    let title_count = args.title.iter().filter(|v| !v.trim().is_empty()).count();
    let has_title_or = title_count >= 2;
    if has_title_or {
        let mut n = 0usize;
        if args.module.is_some() {
            n += 1;
        }
        if args.assignee.is_some() {
            n += 1;
        }
        if args.resolved_by.is_some() {
            n += 1;
        }
        if args.state.zentao_value().is_some() {
            n += 1;
        }
        if args.resolved_from.is_some() {
            n += 1;
        }
        if args.resolved_to.is_some() {
            n += 1;
        }
        if n > 3 {
            return Err(anyhow!(
                "每个搜索 group 最多支持 3 个条件（group1={}，group2={}）。{}",
                n,
                title_count,
                active_state_slot_hint(args.state),
            ));
        }
    } else {
        let mut total = 0usize;
        if args.module.is_some() {
            total += 1;
        }
        if args.assignee.is_some() {
            total += 1;
        }
        if args.resolved_by.is_some() {
            total += 1;
        }
        if args.resolved_from.is_some() {
            total += 1;
        }
        if title_count >= 1 {
            total += 1;
        }
        if args.state.zentao_value().is_some() {
            total += 1;
        }
        if args.resolved_to.is_some() {
            total += 1;
        }
        if total > 6 {
            return Err(anyhow!(
                "当前搜索条件超过 6 个（实际 {} 个），请减少条件。{}",
                total,
                active_state_slot_hint(args.state),
            ));
        }
    }
    if title_count > 3 {
        return Err(anyhow!(
            "重复 --title 最多支持 3 个值（当前 {} 个）",
            title_count
        ));
    }

    Ok(())
}

fn active_state_slot_hint(state: BugState) -> &'static str {
    if matches!(state, BugState::Active) {
        "active 状态（list 未指定 --state 时默认启用）占用一个条件槽位；如不需要状态筛选，请使用 --state all 释放该槽位"
    } else {
        ""
    }
}

fn normalize_stats_assignee(raw: &str) -> String {
    let value = raw.trim();
    if value.is_empty() || value == "--" {
        BUG_STATS_UNASSIGNED.to_string()
    } else {
        value.to_string()
    }
}

fn empty_stats_row(assignee: impl Into<String>) -> AssigneeStatsRow {
    AssigneeStatsRow {
        assignee: assignee.into(),
        active: 0,
        resolved: 0,
        closed: 0,
        total: 0,
    }
}

fn stats_fetched_at_now() -> String {
    Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
}

fn format_resolved_date_range_line(from: Option<&str>, to: Option<&str>) -> Option<String> {
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

fn aggregate_stats_by_assignee(
    bugs: &[search::BugRow],
    limit: u32,
    fetched_at: String,
    resolved_from: Option<String>,
    resolved_to: Option<String>,
) -> BugStatsAggregate {
    // Person rows only count non-closed bugs by current assignee.
    // Closed bugs are bucketed separately: Zentao list often shows assignee as "Closed".
    let mut by_person: HashMap<String, AssigneeStatsRow> = HashMap::new();
    let mut closed_bucket = empty_stats_row(BUG_STATS_CLOSED_BUCKET);

    for bug in bugs {
        match canonical_state(&bug.status) {
            "closed" => {
                closed_bucket.closed += 1;
                closed_bucket.total += 1;
            }
            state => {
                let key = normalize_stats_assignee(&bug.assigned_to);
                let row = by_person
                    .entry(key.clone())
                    .or_insert_with(|| empty_stats_row(key));
                row.total += 1;
                match state {
                    "active" => row.active += 1,
                    "resolved" => row.resolved += 1,
                    _ => {}
                }
            }
        }
    }

    let mut rows: Vec<AssigneeStatsRow> = by_person.into_values().collect();
    // Who still has the most work: active first, then pending verification.
    rows.sort_by(|a, b| {
        b.active
            .cmp(&a.active)
            .then_with(|| b.resolved.cmp(&a.resolved))
            .then_with(|| a.assignee.cmp(&b.assignee))
    });
    if closed_bucket.total > 0 {
        rows.push(closed_bucket);
    }

    let total = AssigneeStatsRow {
        assignee: BUG_STATS_TOTAL_LABEL.to_string(),
        active: rows.iter().map(|r| r.active).sum(),
        resolved: rows.iter().map(|r| r.resolved).sum(),
        closed: rows.iter().map(|r| r.closed).sum(),
        total: rows.iter().map(|r| r.total).sum(),
    };
    let sample_size = bugs.len() as u32;
    BugStatsAggregate {
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

fn format_stats_fetched_at_line(fetched_at: &str) -> String {
    format!("更新时间: {fetched_at}")
}

/// Apply ANSI only when `styled` is true (caller already gates TTY / --plain / NO_COLOR).
fn paint_stats(value: &str, code: &str, styled: bool) -> String {
    if styled {
        format!("\x1b[{code}m{value}\x1b[0m")
    } else {
        value.to_string()
    }
}

/// Resolved-date meta line: muted blue (not bright, distinct from names/counts).
fn paint_stats_date_meta(value: &str, styled: bool) -> String {
    paint_stats(value, "34", styled)
}

fn style_stats_assignee_cell(assignee: &str, styled: bool) -> String {
    let padded = pad_to_display_width(
        &truncate_for_table(assignee, BUG_STATS_ASSIGNEE_WIDTH),
        BUG_STATS_ASSIGNEE_WIDTH,
    );
    let code = if assignee == BUG_STATS_TOTAL_LABEL {
        "1;36" // bold cyan — 合计
    } else if assignee == BUG_STATS_CLOSED_BUCKET || assignee == BUG_STATS_UNASSIGNED {
        "90" // dim — 系统行
    } else {
        "36" // cyan — 人名
    };
    paint_stats(&padded, code, styled)
}

fn style_stats_count_cell(value: u32, styled: bool, emphasize: bool) -> String {
    let padded = pad_to_display_width(&value.to_string(), BUG_STATS_COUNT_WIDTH);
    // Mid tone: normal white (readable, not flashy yellow / not dim gray).
    let code = if emphasize { "1;37" } else { "37" };
    paint_stats(&padded, code, styled)
}

fn append_stats_meta_lines(out: &mut String, stats: &BugStatsAggregate, styled: bool) {
    out.push('\n');
    if let Some(range) = format_resolved_date_range_line(
        stats.resolved_from.as_deref(),
        stats.resolved_to.as_deref(),
    ) {
        out.push_str(&paint_stats_date_meta(&range, styled));
        out.push('\n');
    }
    // Slightly brighter than dim gray so the footer stays readable.
    out.push_str(&paint_stats(
        &format_stats_fetched_at_line(&stats.fetched_at),
        "37",
        styled,
    ));
    out.push('\n');
}

fn render_bug_stats_table(stats: &BugStatsAggregate, styled: bool) -> String {
    if stats.sample_size == 0 {
        let mut out = String::from("没有找到 Bug\n");
        append_stats_meta_lines(&mut out, stats, styled);
        return out;
    }

    let header = format!(
        "{} {} {} {} {}",
        pad_to_display_width("指派给", BUG_STATS_ASSIGNEE_WIDTH),
        pad_to_display_width("激活", BUG_STATS_COUNT_WIDTH),
        pad_to_display_width("待验证", BUG_STATS_COUNT_WIDTH),
        pad_to_display_width("关闭", BUG_STATS_COUNT_WIDTH),
        pad_to_display_width("合计", BUG_STATS_COUNT_WIDTH),
    );
    let mut out = format!(
        "{}\n",
        if styled {
            style_header(&header)
        } else {
            header
        }
    );

    for row in stats.rows.iter().chain(std::iter::once(&stats.total)) {
        let emphasize = row.assignee == BUG_STATS_TOTAL_LABEL;
        out.push_str(&format!(
            "{} {} {} {} {}\n",
            style_stats_assignee_cell(&row.assignee, styled),
            style_stats_count_cell(row.active, styled, emphasize),
            style_stats_count_cell(row.resolved, styled, emphasize),
            style_stats_count_cell(row.closed, styled, emphasize),
            style_stats_count_cell(row.total, styled, emphasize),
        ));
    }
    append_stats_meta_lines(&mut out, stats, styled);
    out
}

fn format_stats_incomplete_warning(stats: &BugStatsAggregate) -> String {
    format!(
        "warning: 样本已达 limit={}（聚合 {} 条），可能不全；请提高 -L 或收窄筛选",
        stats.limit, stats.sample_size
    )
}

fn render_stats_json(stats: &BugStatsAggregate, fields: &str) -> Result<Value> {
    let fields = parse_json_fields(fields, STATS_JSON_FIELDS)?;
    let rows: Vec<Value> = stats
        .rows
        .iter()
        .map(|row| Value::Object(stats_row_json_body(row, &fields, true)))
        .collect();
    Ok(json!({
        "groupBy": "assignee",
        "sampleSize": stats.sample_size,
        "limit": stats.limit,
        "incomplete": stats.incomplete,
        "fetchedAt": stats.fetched_at,
        "resolvedFrom": stats.resolved_from,
        "resolvedTo": stats.resolved_to,
        "rows": rows,
        "total": stats_row_json_body(&stats.total, &fields, false),
    }))
}

fn stats_row_json_body(
    row: &AssigneeStatsRow,
    fields: &[String],
    include_assignee: bool,
) -> Map<String, Value> {
    let mut out = Map::new();
    for field in fields {
        if field == "assignee" && !include_assignee {
            continue;
        }
        out.insert(field.to_string(), stats_json_value(row, field));
    }
    out
}

fn stats_json_value(row: &AssigneeStatsRow, field: &str) -> Value {
    match field {
        "assignee" => json!(row.assignee),
        "active" => json!(row.active),
        "resolved" => json!(row.resolved),
        "closed" => json!(row.closed),
        "total" => json!(row.total),
        _ => Value::Null,
    }
}

fn debug_enabled() -> bool {
    std::env::var("ZENTAO_DEBUG")
        .map(|v| !matches!(v.as_str(), "" | "0" | "false" | "FALSE"))
        .unwrap_or(false)
}

const LIST_JSON_FIELDS: &[&str] = &[
    "id",
    "title",
    "state",
    "severity",
    "priority",
    "confirmed",
    "openedBy",
    "openedDate",
    "assignee",
    "resolvedDate",
    "resolution",
    "deadline",
    "url",
];

const STATS_JSON_FIELDS: &[&str] = &["assignee", "active", "resolved", "closed", "total"];

const VIEW_JSON_FIELDS: &[&str] = &[
    "id",
    "title",
    "description",
    "history",
    "images",
    "attachments",
    "url",
];

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

fn validate_optional_json_fields(raw: Option<&str>, supported: &[&str]) -> Result<()> {
    if let Some(raw) = raw {
        parse_json_fields(raw, supported)?;
    }
    Ok(())
}

fn render_list_json(result: &search::SearchResult, site: &str, fields: &str) -> Result<Value> {
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

fn list_json_value(bug: &search::BugRow, field: &str, site: &str) -> Value {
    match field {
        "id" => json!(bug.id),
        "title" => json!(bug.title),
        "state" => json!(canonical_state(&bug.status)),
        "severity" => json!(bug.severity.parse::<u8>().ok()),
        "priority" => json!(bug.pri.parse::<u8>().ok()),
        "confirmed" => json!(bug.confirmed.trim() == "是"),
        "openedBy" => nullable_text(&bug.opened_by),
        "openedDate" => nullable_date(&bug.opened_date),
        "assignee" => nullable_text(&bug.assigned_to),
        "resolvedDate" => nullable_date(&bug.resolved_date),
        "resolution" => nullable_text(&bug.resolution),
        "deadline" => nullable_date(&bug.deadline),
        "url" => json!(canonical_bug_url(site, bug.id)),
        _ => Value::Null,
    }
}

fn render_view_json(id: u64, site: &str, detail: &bug::BugDetail, fields: &str) -> Result<Value> {
    let fields = parse_json_fields(fields, VIEW_JSON_FIELDS)?;
    let images = extract_view_images(detail);
    let attachments: Vec<Value> = detail
        .attachments
        .iter()
        .map(|attachment| {
            json!({
                "name": attachment.label,
                "url": attachment.url,
                "details": attachment.details_markdown,
            })
        })
        .collect();
    let mut out = Map::new();
    for field in fields {
        let value = match field.as_str() {
            "id" => json!(id),
            "title" => json!(detail.title),
            "description" => json!(detail.markdown_description),
            "history" => json!(detail.markdown_history),
            "images" => Value::Array(images.clone()),
            "attachments" => Value::Array(attachments.clone()),
            "url" => json!(canonical_bug_url(site, id)),
            _ => Value::Null,
        };
        out.insert(field, value);
    }
    Ok(Value::Object(out))
}

fn canonical_bug_url(site: &str, id: u64) -> String {
    let base = Url::parse(site)
        .map(|mut url| {
            url.set_query(None);
            url.set_fragment(None);
            url.to_string()
        })
        .unwrap_or_else(|_| site.to_string());
    format!("{}/bug-view-{id}.html", base.trim_end_matches('/'))
}

fn extract_view_images(detail: &bug::BugDetail) -> Vec<Value> {
    let image_re = Regex::new(r#"!\[[^\]]*\]\(([^\s)]+)(?:\s+\"[^\"]*\")?\)"#)
        .expect("image markdown regex must compile");
    let mut seen = HashSet::new();
    [
        detail.markdown_description.as_str(),
        detail.markdown_history.as_str(),
    ]
    .into_iter()
    .flat_map(|markdown| {
        image_re
            .captures_iter(markdown)
            .filter_map(|capture| capture.get(1).map(|url| url.as_str()))
    })
    .filter(|url| seen.insert((*url).to_string()))
    .map(|url| json!(url))
    .collect()
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

fn canonical_state(raw: &str) -> &'static str {
    match raw.trim() {
        "激活" | "active" => "active",
        "已解决" | "resolved" => "resolved",
        "已关闭" | "closed" => "closed",
        _ => "unknown",
    }
}

fn print_json(value: &Value) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}

fn render_bug_list_table(
    result: &search::SearchResult,
    full_title: bool,
    site: &str,
    hyperlinks: bool,
    styled: bool,
) -> String {
    if result.bugs.is_empty() {
        return "没有找到 Bug\n".to_string();
    }
    let header = format!(
        "{} {} {} {} OPENED",
        pad_to_display_width("ID", BUG_LIST_ID_WIDTH),
        pad_to_display_width("STATE", BUG_LIST_STATE_WIDTH),
        pad_to_display_width("TITLE", BUG_LIST_TITLE_WIDTH),
        pad_to_display_width("ASSIGNEE", BUG_LIST_ASSIGNEE_WIDTH),
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
            title = osc8_hyperlink(&canonical_bug_url(site, bug.id), &title);
        }
        out.push_str(&format!(
            "{} {} {} {} {}\n",
            pad_to_display_width(&bug.id.to_string(), BUG_LIST_ID_WIDTH),
            state,
            title,
            truncate_for_table(&bug.assigned_to, BUG_LIST_ASSIGNEE_WIDTH),
            bug.opened_date.trim(),
        ));
    }
    if io::stdout().is_terminal() {
        if let Some(total) = result.total.as_deref() {
            out.push_str(&format!("\n{}\n", total.trim()));
        }
    }
    out
}

fn hyperlinks_enabled() -> bool {
    io::stdout().is_terminal()
}

/// Kitty / modern terminals: OSC 8 hyperlink around visible text.
fn osc8_hyperlink(url: &str, text: &str) -> String {
    format!("\x1b]8;;{url}\x1b\\{text}\x1b]8;;\x1b\\")
}

fn ansi_enabled() -> bool {
    io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none()
}

fn stderr_ansi_enabled() -> bool {
    io::stderr().is_terminal() && std::env::var_os("NO_COLOR").is_none()
}

fn style_header(value: &str) -> String {
    style_ansi(value, "1")
}

fn colorize_state(value: &str, state: &str) -> String {
    let color = match state {
        "active" => "33",
        "resolved" => "32",
        "closed" => "90",
        _ => "31",
    };
    style_ansi(value, color)
}

fn style_success(value: &str) -> String {
    style_ansi(value, "1;32")
}

fn style_error(value: &str) -> String {
    style_ansi(value, "1;31")
}

/// Yellow warning for stderr (TTY + no NO_COLOR).
fn style_warning(value: &str) -> String {
    if stderr_ansi_enabled() {
        format!("\x1b[1;33m{value}\x1b[0m")
    } else {
        value.to_string()
    }
}

fn style_ansi(value: &str, code: &str) -> String {
    if ansi_enabled() {
        format!("\x1b[{code}m{value}\x1b[0m")
    } else {
        value.to_string()
    }
}

fn current_profile_marker(is_current: bool) -> String {
    if is_current {
        format!(" {}", style_success("[当前]"))
    } else {
        String::new()
    }
}

fn normalize_table_cell(value: &str) -> String {
    value.trim().replace(['\n', '\r'], " ")
}

fn truncate_for_table(value: &str, width: usize) -> String {
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

fn run_bug_view(args: BugViewArgs, global: &GlobalArgs) -> Result<()> {
    validate_optional_json_fields(args.json.as_deref(), VIEW_JSON_FIELDS)?;
    let cfg_path = resolve_config_path(global.config.as_deref())?;
    let cfg = config::load_config_optional(&cfg_path)?;
    let parsed_bug = parse_bug_input(
        &args.bug,
        global
            .site
            .as_deref()
            .or_else(|| cfg.as_ref().map(|c| c.site.as_str())),
    )?;

    let api_client = ZentaoApi::new(&parsed_bug.site_url)?;
    let cookie = load_cookie_for_site(&parsed_bug.site_url, None, cfg.as_ref())?;
    let (final_url, html) =
        api_client.fetch_bug_html(&parsed_bug.bug_url, &cookie.cookie_header)?;

    let detail = bug::parse_bug_detail(&final_url, &html)?;
    let markdown = bug::render_markdown(parsed_bug.id, &detail);

    if let Some(fields) = args.json.as_deref() {
        let json = render_view_json(parsed_bug.id, &parsed_bug.site_url, &detail, fields)?;
        print_json(&json)?;
        return Ok(());
    }

    if let Some(out) = args.output.as_deref() {
        let out_path = PathBuf::from(out);
        if let Some(parent) = out_path.parent() {
            fs::create_dir_all(parent).context("创建输出目录失败")?;
        }
        fs::write(&out_path, markdown).with_context(|| format!("写入 Markdown 失败: {}", out))?;
        println!("Markdown 已写入 {out}");
        return Ok(());
    }

    print!("{}", markdown);
    Ok(())
}

fn run_image_download(args: ImageDownloadArgs, global: &GlobalArgs) -> Result<()> {
    let image_url = validate_image_url(&args.url)?;
    let cfg_path = resolve_config_path(global.config.as_deref())?;
    let cfg = config::load_config_optional(&cfg_path)?;
    let cookie_site = global
        .site
        .as_deref()
        .filter(|site| !site.trim().is_empty())
        .map(str::to_string)
        .unwrap_or(derive_site_url_from_image_url(&image_url)?);
    let cookie = load_cookie_for_site(&cookie_site, None, cfg.as_ref())?;

    let out_dir = Path::new(args.output_dir.as_deref().unwrap_or(IMAGE_DOWNLOAD_DIR));
    let out_path = resolve_output_path_from_url(out_dir, &image_url);
    let started = std::time::Instant::now();
    download_single_image(&image_url, &cookie.cookie_header, &out_path)?;
    let elapsed_ms = started.elapsed().as_millis();
    println!(
        "Downloaded: {} -> {} ({}ms)",
        image_url,
        out_path.display(),
        elapsed_ms
    );
    Ok(())
}

fn validate_image_url(raw: &str) -> Result<Url> {
    let v = raw.trim();
    if v.is_empty() {
        return Err(anyhow!("图片 URL 无效"));
    }
    let url = Url::parse(v).map_err(|_| anyhow!("图片 URL 无效"))?;
    match url.scheme() {
        "http" | "https" => Ok(url),
        _ => Err(anyhow!("图片 URL 无效")),
    }
}

fn resolve_output_path_from_url(base_dir: &Path, url: &Url) -> PathBuf {
    let name = url
        .path_segments()
        .and_then(|segments| segments.rev().find(|seg| !seg.is_empty()))
        .filter(|seg| !seg.trim().is_empty())
        .unwrap_or("downloaded-image.img");

    let filename = ensure_filename_extension(name);
    unique_file_path(base_dir, &filename)
}

fn ensure_filename_extension(filename: &str) -> String {
    let p = Path::new(filename);
    if p.extension().is_some() {
        return filename.to_string();
    }
    format!("{filename}.img")
}

fn unique_file_path(base_dir: &Path, filename: &str) -> PathBuf {
    let mut candidate = base_dir.join(filename);
    if !candidate.exists() {
        return candidate;
    }

    let p = Path::new(filename);
    let stem = p
        .file_stem()
        .and_then(|s| s.to_str())
        .filter(|s| !s.is_empty())
        .unwrap_or("image");
    let ext = p.extension().and_then(|s| s.to_str()).unwrap_or("");

    let mut idx = 1usize;
    loop {
        let next = if ext.is_empty() {
            format!("{stem}({idx})")
        } else {
            format!("{stem}({idx}).{ext}")
        };
        candidate = base_dir.join(next);
        if !candidate.exists() {
            return candidate;
        }
        idx += 1;
    }
}

fn download_single_image(url: &Url, cookie_header: &str, out: &Path) -> Result<()> {
    let client = reqwest::blocking::Client::builder()
        .build()
        .context("初始化 HTTP 客户端失败")?;

    let resp = client
        .get(url.clone())
        .header(reqwest::header::COOKIE, cookie_header)
        .send()
        .with_context(|| format!("下载图片失败: {}", url))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(anyhow!("下载失败: HTTP {}", status.as_u16()));
    }
    if is_login_page_url(resp.url()) {
        return Err(anyhow!("图片下载失败：认证已失效，跳转到了登录页"));
    }
    let content_type = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("");
    if !content_type.to_ascii_lowercase().starts_with("image/") {
        return Err(anyhow!(
            "图片下载失败：响应不是图片（Content-Type: {}）",
            if content_type.is_empty() {
                "缺失"
            } else {
                content_type
            }
        ));
    }
    let body = resp.bytes().context("读取图片响应体失败")?;
    if let Some(parent) = out.parent() {
        fs::create_dir_all(parent).context("创建图片下载目录失败")?;
    }
    fs::write(out, &body).with_context(|| format!("写入图片失败: {}", out.display()))?;
    Ok(())
}

fn is_login_page_url(url: &Url) -> bool {
    let path = url.path();
    path.contains("/user-login-") || path.contains("/user-login.")
}

fn load_cookie_for_site(
    site_url: &str,
    profile_override: Option<&str>,
    cfg: Option<&config::Config>,
) -> Result<browser::BrowserCookieResult> {
    let source = cfg
        .map(|c| c.cookie_source.clone())
        .unwrap_or(CookieSource::Chrome);
    match source {
        CookieSource::Chrome => {
            let profile = profile_override
                .or_else(|| cfg.and_then(|config| config.chrome_profile.as_deref()));
            browser::load_zentao_cookie_from_chrome_macos(site_url, profile)
        }
        CookieSource::File => {
            let path = config::default_cookie_file_path()?;
            cookie_store::load_cookie_from_file(site_url, &path)
        }
    }
}

fn resolve_required(from_cli: Option<&str>, from_cfg: Option<&str>, field: &str) -> Result<String> {
    if let Some(v) = from_cli {
        let v = v.trim();
        if !v.is_empty() {
            return Ok(v.to_string());
        }
    }
    if let Some(v) = from_cfg {
        let v = v.trim();
        if !v.is_empty() {
            return Ok(v.to_string());
        }
    }
    Err(anyhow!("缺少 {}，请通过命令行参数或配置文件提供", field))
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedBugInput {
    id: u64,
    site_url: String,
    bug_url: String,
}

fn parse_bug_input(raw: &str, configured_site: Option<&str>) -> Result<ParsedBugInput> {
    let value = raw.trim();
    if value.is_empty() {
        return Err(anyhow!("Bug 输入无效: 输入为空"));
    }
    if let Ok(id) = value.parse::<u64>() {
        let site_url = resolve_required(None, configured_site, "site")?;
        return Ok(ParsedBugInput {
            id,
            bug_url: canonical_bug_url(&site_url, id),
            site_url,
        });
    }

    let bug_url = Url::parse(value)
        .map_err(|_| anyhow!("Bug 输入无效: 请输入 Bug ID 或完整的 bug 详情 URL"))?;
    let re = Regex::new(r"bug-view-(\d+)\.html").expect("regex should compile");
    if let Some(caps) = re.captures(bug_url.path()) {
        if let Some(m) = caps.get(1) {
            let id = m
                .as_str()
                .parse::<u64>()
                .map_err(|e| anyhow!("Bug URL 无效: {e}"))?;
            let site_url = derive_site_url_from_bug_url(&bug_url)?;
            return Ok(ParsedBugInput {
                id,
                site_url,
                bug_url: bug_url.to_string(),
            });
        }
    }
    Err(anyhow!(
        "Bug URL 无效: 请输入包含 bug-view-<id>.html 的完整详情 URL"
    ))
}

fn derive_site_url_from_bug_url(url: &Url) -> Result<String> {
    let mut base = url.clone();
    let mut segments: Vec<String> = base
        .path_segments()
        .map(|parts| parts.map(str::to_string).collect())
        .unwrap_or_default();

    let last = segments
        .last()
        .ok_or_else(|| anyhow!("Bug URL 无效: 缺少页面路径"))?;
    if !last.starts_with("bug-view-") {
        return Err(anyhow!("Bug URL 无效: 未找到 bug-view 页面"));
    }
    segments.pop();

    let new_path = if segments.is_empty() {
        "/".to_string()
    } else {
        format!("/{}", segments.join("/"))
    };
    base.set_path(&new_path);
    base.set_query(None);
    base.set_fragment(None);

    Ok(base.to_string().trim_end_matches('/').to_string())
}

fn derive_site_url_from_image_url(url: &Url) -> Result<String> {
    let mut base = url.clone();
    let mut segments: Vec<String> = base
        .path_segments()
        .map(|parts| parts.map(str::to_string).collect())
        .unwrap_or_default();
    if segments.pop().is_none() {
        return Err(anyhow!("图片 URL 无效: 缺少文件路径"));
    }

    let new_path = if segments.is_empty() {
        "/".to_string()
    } else {
        format!("/{}", segments.join("/"))
    };
    base.set_path(&new_path);
    base.set_query(None);
    base.set_fragment(None);
    Ok(base.to_string().trim_end_matches('/').to_string())
}

fn resolve_config_path(cli_path: Option<&str>) -> Result<PathBuf> {
    if let Some(path) = cli_path {
        let t = path.trim();
        if !t.is_empty() {
            return Ok(Path::new(t).to_path_buf());
        }
    }
    config::default_config_path()
}

fn format_cookie_expiry(expires_utc: i64) -> String {
    let unix = chrome_expires_utc_to_unix(expires_utc);
    if unix <= 0 {
        return "session".to_string();
    }
    Utc.timestamp_opt(unix, 0)
        .single()
        .map(|dt| dt.format("%Y-%m-%d %H:%M:%S UTC").to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

fn chrome_expires_utc_to_unix(expires_utc: i64) -> i64 {
    browser::chrome_expires_utc_to_unix(expires_utc)
}

#[derive(Debug, Clone)]
struct CookieTableRow {
    name: String,
    value: String,
    domain: String,
    path: String,
    secure: String,
    http_only: String,
    expires: String,
}

fn collect_cookie_table_rows(items: &[browser::BrowserCookieItem]) -> Vec<CookieTableRow> {
    let order = ["zentaosid", "za", "zp", "keepLogin"];
    let mut out = Vec::new();
    for name in order {
        if let Some(it) = items.iter().find(|v| v.name == name && !v.value.is_empty()) {
            out.push(CookieTableRow {
                name: it.name.clone(),
                value: it.value.clone(),
                domain: it.domain.clone(),
                path: it.path.clone(),
                secure: it.secure.to_string(),
                http_only: it.http_only.to_string(),
                expires: format_cookie_expiry(it.expires_utc),
            });
        }
    }
    out
}

fn render_cookie_table(rows: &[CookieTableRow], show_values: bool) -> Vec<String> {
    let headers = [
        "name", "value", "domain", "path", "secure", "httpOnly", "expires",
    ];
    let mut w_name = headers[0].len();
    let mut w_value = headers[1].len();
    let mut w_domain = headers[2].len();
    let mut w_path = headers[3].len();
    let mut w_secure = headers[4].len();
    let mut w_http_only = headers[5].len();
    let mut w_expires = headers[6].len();

    for r in rows {
        let value = if show_values { &r.value } else { "***" };
        w_name = w_name.max(r.name.len());
        w_value = w_value.max(value.len());
        w_domain = w_domain.max(r.domain.len());
        w_path = w_path.max(r.path.len());
        w_secure = w_secure.max(r.secure.len());
        w_http_only = w_http_only.max(r.http_only.len());
        w_expires = w_expires.max(r.expires.len());
    }

    let fmt = |name: &str,
               value: &str,
               domain: &str,
               path: &str,
               secure: &str,
               http_only: &str,
               expires: &str| {
        format!(
            "{:<w_name$}  {:<w_value$}  {:<w_domain$}  {:<w_path$}  {:<w_secure$}  {:<w_http_only$}  {:<w_expires$}",
            name,
            value,
            domain,
            path,
            secure,
            http_only,
            expires,
            w_name = w_name,
            w_value = w_value,
            w_domain = w_domain,
            w_path = w_path,
            w_secure = w_secure,
            w_http_only = w_http_only,
            w_expires = w_expires
        )
    };

    let mut lines = Vec::new();
    let header = fmt(
        headers[0], headers[1], headers[2], headers[3], headers[4], headers[5], headers[6],
    );
    let sep = format!(
        "{:-<w_name$}  {:-<w_value$}  {:-<w_domain$}  {:-<w_path$}  {:-<w_secure$}  {:-<w_http_only$}  {:-<w_expires$}",
        "",
        "",
        "",
        "",
        "",
        "",
        "",
        w_name = w_name,
        w_value = w_value,
        w_domain = w_domain,
        w_path = w_path,
        w_secure = w_secure,
        w_http_only = w_http_only,
        w_expires = w_expires
    );
    lines.push(style_header(&header));
    lines.push(sep);
    for r in rows {
        lines.push(fmt(
            &r.name,
            if show_values { &r.value } else { "***" },
            &r.domain,
            &r.path,
            &r.secure,
            &r.http_only,
            &r.expires,
        ));
    }
    lines
}

fn format_login_response(raw: &str) -> String {
    let parsed = parse_login_response(raw);
    if parsed.result.is_some() || parsed.message.is_some() || parsed.locate.is_some() {
        let mut parts = Vec::new();
        if let Some(v) = parsed.result {
            if !v.is_empty() {
                parts.push(format!("result={}", v));
            }
        }
        if let Some(v) = parsed.message {
            if !v.is_empty() {
                parts.push(format!("message={}", v));
            }
        }
        if let Some(v) = parsed.locate {
            if !v.is_empty() {
                parts.push(format!("locate={}", v));
            }
        }
        return parts.join(", ");
    }
    raw.to_string()
}

#[derive(Debug, Default)]
struct ParsedLoginResponse {
    result: Option<String>,
    message: Option<String>,
    locate: Option<String>,
}

fn parse_login_response(raw: &str) -> ParsedLoginResponse {
    if let Ok(v) = serde_json::from_str::<Value>(raw) {
        return ParsedLoginResponse {
            result: v
                .get("result")
                .and_then(Value::as_str)
                .map(std::string::ToString::to_string),
            message: v
                .get("message")
                .and_then(Value::as_str)
                .map(std::string::ToString::to_string),
            locate: v
                .get("locate")
                .and_then(Value::as_str)
                .map(std::string::ToString::to_string),
        };
    }
    ParsedLoginResponse::default()
}

fn print_cookie_presence(items: &[browser::BrowserCookieItem], name: &str) {
    let exists = items
        .iter()
        .any(|it| it.name == name && !it.value.is_empty());
    println!("- {}: {}", name, cookie_presence_label(exists));
}

fn cookie_presence_label(exists: bool) -> String {
    if exists {
        style_success("[OK]")
    } else {
        style_error("[MISSING]")
    }
}

fn format_cookie_domains_line(domains: &[String]) -> String {
    if domains.is_empty() {
        return style_error("(none) [MISSING]");
    }
    format!("{} {}", domains.join(", "), style_success("[OK]"))
}

#[cfg(test)]
#[path = "cli_test.rs"]
mod tests;
