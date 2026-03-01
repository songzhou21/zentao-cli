use crate::api::ZentaoApi;
use crate::browser;
use crate::bug;
use crate::config;
use crate::search;
use anyhow::{anyhow, Context, Result};
use chrono::{TimeZone, Utc};
use clap::{Args, Parser, Subcommand};
use regex::Regex;
use reqwest::Url;
use std::ffi::OsString;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

const IMAGE_DOWNLOAD_DIR: &str = "/tmp/zentao-images";
const DEFAULT_SEARCH_PRODUCT_ID: u64 = 92;

#[derive(Debug, Parser)]
#[command(name = "zentao")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    Cookie(CookieArgs),
    Chrome(ChromeArgs),
    Bug(BugArgs),
    Image(ImageArgs),
    Search(SearchArgs),
}

#[derive(Debug, Args)]
struct CookieArgs {
    #[arg(long)]
    url: Option<String>,
    #[arg(long)]
    profile: Option<String>,
    #[arg(long)]
    verify: bool,
    #[arg(long, default_value = "v1")]
    api_version: String,
    #[arg(long)]
    config: Option<String>,
}

#[derive(Debug, Args)]
struct ChromeArgs {
    #[command(subcommand)]
    command: ChromeSubCommands,
}

#[derive(Debug, Subcommand)]
enum ChromeSubCommands {
    Profile(ProfileArgs),
}

#[derive(Debug, Args)]
struct ProfileArgs {
    #[arg(long)]
    config: Option<String>,
}

#[derive(Debug, Args)]
struct BugArgs {
    #[command(subcommand)]
    command: BugSubCommands,
}

#[derive(Debug, Subcommand)]
enum BugSubCommands {
    Show(BugShowArgs),
}

#[derive(Debug, Args)]
struct BugShowArgs {
    #[arg(value_name = "ID_OR_URL")]
    id_or_url: String,
    #[arg(long)]
    url: Option<String>,
    #[arg(long)]
    profile: Option<String>,
    #[arg(long)]
    config: Option<String>,
    #[arg(long)]
    out: Option<String>,
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
    #[arg(long)]
    url: String,
}

/// 搜索 Bug（支持按指派者、解决者、解决日期等条件筛选）
#[derive(Debug, Args)]
struct SearchArgs {
    /// 指派给（用户名），例如 zhousong
    #[arg(long, value_name = "USER")]
    assigned_to: Option<String>,

    /// 解决者（用户名），例如 zhousong
    #[arg(long, value_name = "USER")]
    resolved_by: Option<String>,

    /// 解决日期起始（含），格式 YYYY-MM-DD
    #[arg(long, value_name = "DATE")]
    resolved_date_from: Option<String>,

    /// 解决日期截止（含），格式 YYYY-MM-DD
    #[arg(long, value_name = "DATE")]
    resolved_date_to: Option<String>,

    /// 站点 URL
    #[arg(long)]
    url: Option<String>,

    /// Chrome profile 路径
    #[arg(long)]
    profile: Option<String>,

    /// 配置文件路径
    #[arg(long)]
    config: Option<String>,

    /// 以 JSON 格式输出搜索结果
    #[arg(long)]
    json: bool,
}

pub fn run(args: Vec<OsString>) -> Result<()> {
    let cli = Cli::try_parse_from(std::iter::once(OsString::from("zentao")).chain(args))
        .map_err(|e| anyhow!(e.to_string()))?;

    match cli.command {
        Commands::Cookie(args) => run_cookie(args),
        Commands::Chrome(args) => run_chrome(args),
        Commands::Bug(args) => run_bug(args),
        Commands::Image(args) => run_image(args),
        Commands::Search(args) => run_search(args),
    }
}

fn run_cookie(args: CookieArgs) -> Result<()> {
    let cfg_path = resolve_config_path(args.config.as_deref())?;
    let cfg = config::load_config_optional(&cfg_path)?;

    let site_url = resolve_required(
        args.url.as_deref(),
        cfg.as_ref().map(|c| c.url.as_str()),
        "url",
    )?;

    let profile = args
        .profile
        .as_deref()
        .map(str::to_string)
        .or_else(|| cfg.as_ref().and_then(|c| c.chrome_profile.clone()));

    let cookie = browser::load_zentao_cookie_from_chrome_macos(&site_url, profile.as_deref())?;
    let parsed_site = reqwest::Url::parse(&site_url).context("解析 URL 失败")?;
    let target_host = parsed_site
        .host_str()
        .ok_or_else(|| anyhow!("URL 缺少 host"))?
        .to_string();
    let mut matched_domains: Vec<String> =
        cookie.items.iter().map(|it| it.domain.clone()).collect();
    matched_domains.sort();
    matched_domains.dedup();

    let expiry = cookie
        .items
        .first()
        .map(|it| format_cookie_expiry(it.expires_utc))
        .unwrap_or_else(|| "unknown".to_string());
    println!("Chrome profile: {}", cookie.profile_path);
    println!("目标域名: {}", target_host);
    println!("命中 cookie 域名: {}", matched_domains.join(", "));
    println!("\x1b[1;33m过期时间: {}\x1b[0m", expiry);
    println!("浏览器 cookie 明细:");

    for item in &cookie.items {
        println!(
            "- {}: value={}, domain={}, path={}, secure={}, httpOnly={}",
            item.name, item.value, item.domain, item.path, item.secure, item.http_only
        );
    }

    if args.verify {
        let client = ZentaoApi::new(&site_url, &args.api_version)?;
        match client.verify_cookie(&cookie.cookie_header) {
            Ok(final_url) => println!("\x1b[1;32mcookie 校验成功，最终跳转: {}\x1b[0m", final_url),
            Err(err) => return Err(err),
        }
    }

    Ok(())
}

fn run_chrome(args: ChromeArgs) -> Result<()> {
    match args.command {
        ChromeSubCommands::Profile(p) => run_chrome_profile(p),
    }
}

fn run_chrome_profile(args: ProfileArgs) -> Result<()> {
    let cfg_path = resolve_config_path(args.config.as_deref())?;
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
        let marker = if cfg.chrome_profile.as_deref() == Some(profile.as_str()) {
            " \x1b[1;32m[当前]\x1b[0m"
        } else {
            ""
        };
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

fn run_bug(args: BugArgs) -> Result<()> {
    match args.command {
        BugSubCommands::Show(s) => run_bug_show(s),
    }
}

fn run_image(args: ImageArgs) -> Result<()> {
    match args.command {
        ImageSubCommands::Download(d) => run_image_download(d),
    }
}

fn run_search(args: SearchArgs) -> Result<()> {
    let cfg_path = resolve_config_path(args.config.as_deref())?;
    let cfg = config::load_config_optional(&cfg_path)?;

    let site_url = resolve_required(
        args.url.as_deref(),
        cfg.as_ref().map(|c| c.url.as_str()),
        "url",
    )?;

    let profile = args
        .profile
        .as_deref()
        .map(str::to_string)
        .or_else(|| cfg.as_ref().and_then(|c| c.chrome_profile.clone()));

    let api_client = ZentaoApi::new(&site_url, "v1")?;
    let cookie = browser::load_zentao_cookie_from_chrome_macos(&site_url, profile.as_deref())?;

    // Build field overrides from CLI args
    let mut field_params: Vec<(String, String)> = Vec::new();

    if let Some(ref user) = args.assigned_to {
        field_params.push(("assignedTo".to_string(), user.clone()));
    }
    if let Some(ref user) = args.resolved_by {
        field_params.push(("resolvedBy".to_string(), user.clone()));
    }
    if let Some(ref date_from) = args.resolved_date_from {
        field_params.push(("resolvedDate_from".to_string(), date_from.clone()));
    }
    if let Some(ref date_to) = args.resolved_date_to {
        field_params.push(("resolvedDate_to".to_string(), date_to.clone()));
    }

    let html = api_client.search_bugs(
        &cookie.cookie_header,
        DEFAULT_SEARCH_PRODUCT_ID,
        &field_params,
    )?;

    // DEBUG: dump raw HTML for diagnosis
    if std::env::var("ZENTAO_DEBUG").is_ok() {
        let debug_path = "/tmp/zentao-search-debug.html";
        std::fs::write(debug_path, &html).ok();
        eprintln!("[debug] 搜索结果 HTML 已写入 {}", debug_path);
    }

    let result = search::parse_search_result(&html)?;
    let json = search::render_search_json(&result)?;
    if args.json {
        println!("{}", json);
    } else {
        let text = search::render_search_lines_from_json(&json, args.assigned_to.is_some())?;
        print!("{}", text);
    }
    Ok(())
}

fn run_bug_show(args: BugShowArgs) -> Result<()> {
    let bug_id = parse_bug_id_or_url(&args.id_or_url)?;
    let cfg_path = resolve_config_path(args.config.as_deref())?;
    let cfg = config::load_config_optional(&cfg_path)?;

    let site_url = resolve_required(
        args.url.as_deref(),
        cfg.as_ref().map(|c| c.url.as_str()),
        "url",
    )?;

    let profile = args
        .profile
        .as_deref()
        .map(str::to_string)
        .or_else(|| cfg.as_ref().and_then(|c| c.chrome_profile.clone()));

    let api_client = ZentaoApi::new(&site_url, "v1")?;
    let cookie = browser::load_zentao_cookie_from_chrome_macos(&site_url, profile.as_deref())?;
    let (final_url, html) = api_client.fetch_bug_html(bug_id, &cookie.cookie_header)?;

    let detail = bug::parse_bug_detail(&final_url, &html)?;
    let markdown = bug::render_markdown(bug_id, &detail);

    if let Some(out) = args.out.as_deref() {
        let out_path = PathBuf::from(out);
        if let Some(parent) = out_path.parent() {
            fs::create_dir_all(parent).context("创建输出目录失败")?;
        }
        fs::write(&out_path, markdown).with_context(|| format!("写入 Markdown 失败: {}", out))?;
        println!("Markdown 已写入 {}", out);
        return Ok(());
    }

    print!("{}", markdown);
    Ok(())
}

fn run_image_download(args: ImageDownloadArgs) -> Result<()> {
    let image_url = validate_image_url(&args.url)?;
    let out_dir = Path::new(IMAGE_DOWNLOAD_DIR);
    fs::create_dir_all(out_dir).context("创建图片下载目录失败")?;

    let out_path = resolve_output_path_from_url(out_dir, &image_url);
    let started = std::time::Instant::now();
    download_single_image(&image_url, &out_path)?;
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
        .and_then(|segments| segments.filter(|seg| !seg.is_empty()).last())
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

fn download_single_image(url: &Url, out: &Path) -> Result<()> {
    let client = reqwest::blocking::Client::builder()
        .build()
        .context("初始化 HTTP 客户端失败")?;

    let resp = client
        .get(url.clone())
        .send()
        .with_context(|| format!("下载图片失败: {}", url))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(anyhow!("下载失败: HTTP {}", status.as_u16()));
    }
    let body = resp.bytes().context("读取图片响应体失败")?;
    fs::write(out, &body).with_context(|| format!("写入图片失败: {}", out.display()))?;
    Ok(())
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

fn parse_bug_id_or_url(raw: &str) -> Result<u64> {
    let value = raw.trim();
    if value.is_empty() {
        return Err(anyhow!("Bug ID 无效: 输入为空"));
    }
    if let Ok(id) = value.parse::<u64>() {
        return Ok(id);
    }

    let re = Regex::new(r"bug-view-(\d+)\.html").expect("regex should compile");
    if let Some(caps) = re.captures(value) {
        if let Some(m) = caps.get(1) {
            return m
                .as_str()
                .parse::<u64>()
                .map_err(|e| anyhow!("Bug ID 无效: {e}"));
        }
    }
    Err(anyhow!(
        "Bug ID 无效: 请输入数字 ID 或包含 bug-view-<id>.html 的 URL"
    ))
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

#[cfg(test)]
#[path = "cli_test.rs"]
mod tests;
