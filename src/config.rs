use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum CookieSource {
    #[default]
    File,
    Chrome,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chrome_profile: Option<String>,
    #[serde(default, skip_serializing_if = "is_default_cookie_source")]
    pub cookie_source: CookieSource,
}

pub fn default_config_path() -> Result<PathBuf> {
    let home = dirs::home_dir().ok_or_else(|| anyhow!("无法定位用户主目录"))?;
    Ok(home.join(".zentao").join("config.json"))
}

pub fn default_cookie_file_path() -> Result<PathBuf> {
    let home = dirs::home_dir().ok_or_else(|| anyhow!("无法定位用户主目录"))?;
    Ok(home.join(".zentao").join("cookies"))
}

pub fn load_config(path: &Path) -> Result<Config> {
    let data = fs::read(path).with_context(|| format!("读取配置失败: {}", path.display()))?;
    serde_json::from_slice(&data).with_context(|| format!("解析配置失败: {}", path.display()))
}

pub fn save_config(path: &Path, cfg: &Config) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).context("创建配置目录失败")?;
    }
    let data = serde_json::to_vec_pretty(cfg).context("序列化配置失败")?;
    fs::write(path, data).with_context(|| format!("写入配置失败: {}", path.display()))?;
    Ok(())
}

pub fn load_config_optional(path: &Path) -> Result<Option<Config>> {
    if !path.exists() {
        return Ok(None);
    }
    match load_config(path) {
        Ok(cfg) => Ok(Some(cfg)),
        Err(err) => Err(anyhow!(
            "配置文件存在但无法解析，请修复后重试: {} ({})",
            path.display(),
            err
        )),
    }
}

pub fn load_or_default(path: &Path) -> Result<Config> {
    Ok(load_config_optional(path)?.unwrap_or_default())
}

fn is_default_cookie_source(source: &CookieSource) -> bool {
    matches!(source, CookieSource::File)
}

#[cfg(test)]
#[path = "config_test.rs"]
mod tests;
