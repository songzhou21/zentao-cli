mod auth;
mod bug;
mod config;

use crate::browser;
use crate::config as app_config;
use crate::config::CookieSource;
use crate::cookie_store;
use anyhow::{anyhow, Result};
use clap::{Args, Parser, Subcommand};
use serde_json::Value;
use std::ffi::OsString;
use std::io::{self, IsTerminal};
use std::path::{Path, PathBuf};

#[cfg(test)]
pub(crate) use auth::profile::current_profile_marker;
#[cfg(test)]
pub(crate) use auth::status::{
    cookie_presence_label, format_cookie_domains_line, render_cookie_table, CookieTableRow,
};
#[cfg(test)]
pub(crate) use bug::candidates::{render_candidates_table, CandidateRow};
#[cfg(test)]
pub(crate) use bug::list::{
    render_bug_list_table, render_list_json, truncate_for_table, truncated_warning,
    LIST_JSON_FIELDS,
};
#[cfg(test)]
pub(crate) use bug::{
    apply_result_limit, build_search_field_params, calendar_month_bounds, reporting_week_bounds,
    resolve_resolved_date_range, validate_search_group_limits, BugArgs, BugSearchQuery, BugState,
    BugSubCommands,
};
#[cfg(test)]
pub(crate) use config::{set_config_value, ConfigKey};

#[derive(Debug, Parser)]
#[command(name = "zentao", version, about = "在终端管理禅道 Bug")]
pub(crate) struct Cli {
    #[command(flatten)]
    pub(crate) global: GlobalArgs,
    #[command(subcommand)]
    pub(crate) command: Commands,
}

#[derive(Debug, Args)]
pub(crate) struct GlobalArgs {
    /// 禅道站点基础 URL（可包含部署子路径）
    #[arg(long, global = true, env = "ZENTAO_SITE")]
    pub(crate) site: Option<String>,
    /// 配置文件路径
    #[arg(long, global = true, env = "ZENTAO_CONFIG")]
    pub(crate) config: Option<String>,
}

#[derive(Debug, Subcommand)]
#[allow(clippy::large_enum_variant)]
pub(crate) enum Commands {
    Auth(auth::AuthArgs),
    Bug(bug::BugArgs),
    Config(config::ConfigArgs),
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
        Commands::Auth(args) => auth::run(args, &cli.global),
        Commands::Bug(args) => bug::run(args, &cli.global),
        Commands::Config(args) => config::run(args, &cli.global),
    }
    .map_err(RunError::Runtime)
}

pub(crate) fn resolve_config_path(cli_path: Option<&str>) -> Result<PathBuf> {
    if let Some(path) = cli_path {
        let t = path.trim();
        if !t.is_empty() {
            return Ok(Path::new(t).to_path_buf());
        }
    }
    app_config::default_config_path()
}

pub(crate) fn resolve_required(
    from_cli: Option<&str>,
    from_cfg: Option<&str>,
    field: &str,
) -> Result<String> {
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

pub(crate) fn load_cookie_for_site(
    site_url: &str,
    profile_override: Option<&str>,
    cfg: Option<&app_config::Config>,
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
            let path = app_config::default_cookie_file_path()?;
            cookie_store::load_cookie_from_file(site_url, &path)
        }
    }
}

pub(crate) fn parse_json_fields(raw: &str, supported: &[&str]) -> Result<Vec<String>> {
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

pub(crate) fn validate_optional_json_fields(raw: Option<&str>, supported: &[&str]) -> Result<()> {
    if let Some(raw) = raw {
        parse_json_fields(raw, supported)?;
    }
    Ok(())
}

pub(crate) fn print_json(value: &Value) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}

pub(crate) fn debug_enabled() -> bool {
    std::env::var("ZENTAO_DEBUG")
        .map(|v| !matches!(v.as_str(), "" | "0" | "false" | "FALSE"))
        .unwrap_or(false)
}

pub(crate) fn ansi_enabled() -> bool {
    io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none()
}

fn stderr_ansi_enabled() -> bool {
    io::stderr().is_terminal() && std::env::var_os("NO_COLOR").is_none()
}

pub(crate) fn style_header(value: &str) -> String {
    style_ansi(value, "1")
}

pub(crate) fn style_success(value: &str) -> String {
    style_ansi(value, "1;32")
}

pub(crate) fn style_error(value: &str) -> String {
    style_ansi(value, "1;31")
}

/// Yellow warning for stderr (TTY + no NO_COLOR).
pub(crate) fn style_warning(value: &str) -> String {
    if stderr_ansi_enabled() {
        format!("\x1b[1;33m{value}\x1b[0m")
    } else {
        value.to_string()
    }
}

pub(crate) fn style_ansi(value: &str, code: &str) -> String {
    if ansi_enabled() {
        format!("\x1b[{code}m{value}\x1b[0m")
    } else {
        value.to_string()
    }
}

#[cfg(test)]
#[path = "cli_test.rs"]
mod tests;
