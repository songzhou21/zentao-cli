use crate::api::ZentaoApi;
use crate::browser;
use crate::cli::{resolve_config_path, resolve_required, style_error, style_success, GlobalArgs};
use crate::config;
use crate::config::CookieSource;
use crate::cookie_store;
use anyhow::{anyhow, Context, Result};
use clap::Args;
use serde_json::Value;
use std::io::{self, Read};

#[derive(Debug, Args)]
pub(crate) struct LoginArgs {
    /// 禅道用户名
    #[arg(long)]
    pub(crate) username: String,
    /// 从标准输入读取密码
    #[arg(long)]
    pub(crate) password_stdin: bool,
    #[arg(long)]
    pub(crate) proxy: Option<String>,
}

pub(crate) fn run(args: LoginArgs, global: &GlobalArgs) -> Result<()> {
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
