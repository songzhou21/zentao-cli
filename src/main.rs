use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use clap::{Parser, Subcommand};
use crossterm::event::{read, Event, KeyCode, KeyEventKind};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use std::io::{self, Write};
use std::path::Path;
use std::path::PathBuf;

mod api;
mod browser;
mod config;

use api::ZentaoApi;
use browser::{list_chrome_profiles_macos, load_zentao_cookie_from_chrome_macos};
use config::{default_config_path, load_config, save_config, Config};

#[derive(Parser)]
#[command(name = "zentao", version, about = "Zentao CLI")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// 从 Chrome 读取 cookie；可选执行校验
    Cookie {
        /// 禅道地址，例如 https://zentao.example.com
        #[arg(long)]
        url: Option<String>,

        /// Chrome profile 目录（可选，默认优先读取配置中的 chrome_profile）
        #[arg(long)]
        profile: Option<PathBuf>,

        /// 校验 cookie 是否有效
        #[arg(long, default_value_t = false)]
        verify: bool,

        /// API 版本（默认 v1）
        #[arg(long)]
        api_version: Option<String>,

        /// 配置文件路径（默认 ~/.zentao/config.json）
        #[arg(long)]
        config: Option<PathBuf>,
    },

    /// Chrome 相关命令
    Chrome {
        #[command(subcommand)]
        command: ChromeCommands,
    },
}

#[derive(Subcommand)]
enum ChromeCommands {
    /// 列出并选择 Chrome profile，保存到 config.json
    Profile {
        /// 配置文件路径（默认 ~/.zentao/config.json）
        #[arg(long)]
        config: Option<PathBuf>,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Cookie {
            url,
            profile,
            verify,
            api_version,
            config,
        } => {
            let path = config.unwrap_or(default_config_path()?);
            let file_cfg = load_config_optional(&path)?;

            let url = resolve_required(url, file_cfg.as_ref().map(|c| c.url.as_str()), "url")?;
            let api_version = api_version
                .or_else(|| file_cfg.as_ref().map(|c| c.api_version.clone()))
                .unwrap_or_else(|| "v1".to_string());
            let profile = profile.or_else(|| {
                file_cfg
                    .as_ref()
                    .and_then(|c| c.chrome_profile.as_ref())
                    .map(PathBuf::from)
            });

            let api = ZentaoApi::new(&url, &api_version)?;
            let result = load_zentao_cookie_from_chrome_macos(&url, profile.as_deref())?;
            let unified_expiry = result
                .items
                .iter()
                .map(|item| format_cookie_expiry(item.expires_utc))
                .next()
                .unwrap_or_else(|| "unknown".to_string());
            println!("\x1b[1;33m过期时间: {}\x1b[0m", unified_expiry);
            println!("浏览器 cookie 明细:");
            for item in result.items {
                println!(
                    "- {}: value={}, domain={}, path={}, secure={}, httpOnly={}",
                    item.name, item.value, item.domain, item.path, item.secure, item.http_only
                );
            }

            if verify {
                match api.verify_cookie(result.cookie_header.as_str()).await {
                    Ok(final_url) => {
                        println!("\x1b[1;32mcookie 校验成功，最终跳转: {}\x1b[0m", final_url);
                    }
                    Err(err) => {
                        println!("\x1b[1;31mcookie 校验失败: {}\x1b[0m", err);
                        return Err(err);
                    }
                }
            }
        }
        Commands::Chrome { command } => match command {
            ChromeCommands::Profile { config } => {
                let path = config.unwrap_or(default_config_path()?);
                let mut cfg = load_or_default_config(&path)?;
                let current_selected = cfg.chrome_profile.as_ref().map(PathBuf::from);
                let profiles = list_chrome_profiles_macos()?;
                if profiles.is_empty() {
                    return Err(anyhow!("未找到可用的 Chrome profile"));
                }

                if let Some(current) = &current_selected {
                    println!("当前已选择: {}", current.display());
                } else {
                    println!("当前已选择: (未设置)");
                }
                println!("可用 Chrome profiles:");
                for (idx, profile) in profiles.iter().enumerate() {
                    let marker = if current_selected.as_ref() == Some(profile) {
                        " \x1b[1;32m[当前]\x1b[0m"
                    } else {
                        ""
                    };
                    println!("{}. {}{}", idx + 1, profile.display(), marker);
                }
                print!("请输入编号（按 Esc 退出）: ");
                io::stdout().flush()?;

                let choice = match read_profile_choice() {
                    Ok(v) => v,
                    Err(err) if err.to_string() == "已取消" => return Ok(()),
                    Err(err) => return Err(err),
                };
                let index: usize = choice
                    .parse()
                    .map_err(|_| anyhow!("输入无效，请输入数字编号"))?;
                if index == 0 || index > profiles.len() {
                    return Err(anyhow!("编号超出范围，请输入 1-{}", profiles.len()));
                }

                let selected = profiles[index - 1].clone();
                cfg.chrome_profile = Some(selected.to_string_lossy().to_string());
                save_config(&path, &cfg)?;
                println!("已保存 chrome_profile: {}", selected.display());
                println!("配置文件: {}", path.display());
            }
        },
    }

    Ok(())
}

fn chrome_expires_utc_to_unix(expires_utc: i64) -> i64 {
    if expires_utc <= 0 {
        return 0;
    }
    (expires_utc / 1_000_000) - 11_644_473_600
}

fn format_cookie_expiry(expires_utc: i64) -> String {
    let unix = chrome_expires_utc_to_unix(expires_utc);
    if unix <= 0 {
        return "session".to_string();
    }
    DateTime::<Utc>::from_timestamp(unix, 0)
        .map(|dt| dt.format("%Y-%m-%d %H:%M:%S UTC").to_string())
        .unwrap_or_else(|| format!("unix={}", unix))
}

fn resolve_required(
    from_cli: Option<String>,
    from_cfg: Option<&str>,
    field_name: &str,
) -> Result<String> {
    from_cli
        .or_else(|| from_cfg.map(|v| v.to_string()))
        .ok_or_else(|| anyhow!("缺少 {}，请通过命令行参数或配置文件提供", field_name))
}

fn load_or_default_config(path: &Path) -> Result<Config> {
    Ok(match load_config_optional(path)? {
        Some(cfg) => cfg,
        None => Config {
            url: String::new(),
            username: None,
            password: None,
            code: None,
            token: None,
            api_version: "v1".to_string(),
            chrome_profile: None,
        },
    })
}

fn load_config_optional(path: &Path) -> Result<Option<Config>> {
    if !path.exists() {
        return Ok(None);
    }
    let cfg = load_config(path)
        .map_err(|e| anyhow!("配置文件存在但无法解析，请修复后重试: {} ({})", path.display(), e))?;
    Ok(Some(cfg))
}

fn read_profile_choice() -> Result<String> {
    enable_raw_mode()?;
    let _raw_mode_guard = RawModeGuard;
    let mut input = String::new();
    loop {
        if let Event::Key(key) = read()? {
            if key.kind != KeyEventKind::Press {
                continue;
            }
            match key.code {
                KeyCode::Esc => {
                    print!("\r\n已取消选择\r\n");
                    io::stdout().flush()?;
                    return Err(anyhow!("已取消"));
                }
                KeyCode::Enter => {
                    print!("\r\n");
                    io::stdout().flush()?;
                    return Ok(input.trim().to_string());
                }
                KeyCode::Backspace => {
                    if !input.is_empty() {
                        input.pop();
                        print!("\u{8} \u{8}");
                        io::stdout().flush()?;
                    }
                }
                KeyCode::Char(c) if c.is_ascii_digit() => {
                    input.push(c);
                    print!("{}", c);
                    io::stdout().flush()?;
                }
                _ => {}
            }
        }
    }
}

struct RawModeGuard;

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
    }
}
