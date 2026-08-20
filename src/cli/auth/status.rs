use crate::api::ZentaoApi;
use crate::browser;
use crate::cli::{
    load_cookie_for_site, resolve_config_path, resolve_required, style_error, style_header,
    style_success, GlobalArgs,
};
use crate::config;
use anyhow::{anyhow, Context, Result};
use chrono::{TimeZone, Utc};
use clap::Args;

#[derive(Debug, Args)]
pub(crate) struct AuthStatusArgs {
    /// 临时覆盖 Chrome Profile 路径
    #[arg(long)]
    pub(crate) profile: Option<String>,
    /// 显示完整 Cookie 值
    #[arg(long)]
    pub(crate) show_cookie_values: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct CookieTableRow {
    pub(crate) name: String,
    pub(crate) value: String,
    pub(crate) domain: String,
    pub(crate) path: String,
    pub(crate) secure: String,
    pub(crate) http_only: String,
    pub(crate) expires: String,
}

pub(crate) fn run(args: AuthStatusArgs, global: &GlobalArgs) -> Result<()> {
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

pub(crate) fn render_cookie_table(rows: &[CookieTableRow], show_values: bool) -> Vec<String> {
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

pub(crate) fn cookie_presence_label(exists: bool) -> String {
    if exists {
        style_success("[OK]")
    } else {
        style_error("[MISSING]")
    }
}

pub(crate) fn format_cookie_domains_line(domains: &[String]) -> String {
    if domains.is_empty() {
        return style_error("(none) [MISSING]");
    }
    format!("{} {}", domains.join(", "), style_success("[OK]"))
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

fn print_cookie_presence(items: &[browser::BrowserCookieItem], name: &str) {
    let exists = items
        .iter()
        .any(|it| it.name == name && !it.value.is_empty());
    println!("- {}: {}", name, cookie_presence_label(exists));
}

fn format_cookie_expiry(expires_utc: i64) -> String {
    let unix = browser::chrome_expires_utc_to_unix(expires_utc);
    if unix <= 0 {
        return "session".to_string();
    }
    Utc.timestamp_opt(unix, 0)
        .single()
        .map(|dt| dt.format("%Y-%m-%d %H:%M:%S UTC").to_string())
        .unwrap_or_else(|| "unknown".to_string())
}
