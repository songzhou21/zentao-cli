use crate::browser;
use crate::cli::{resolve_config_path, style_success, GlobalArgs};
use crate::config;
use anyhow::{anyhow, Context, Result};
use clap::Args;
use std::io::{self, Write};

#[derive(Debug, Args)]
pub(crate) struct ProfileArgs {}

pub(crate) fn run(_args: ProfileArgs, global: &GlobalArgs) -> Result<()> {
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

pub(crate) fn current_profile_marker(is_current: bool) -> String {
    if is_current {
        format!(" {}", style_success("[当前]"))
    } else {
        String::new()
    }
}
