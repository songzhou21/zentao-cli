use anyhow::{Context, Result};
use dirs::home_dir;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Config {
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    #[serde(default = "default_api_version")]
    pub api_version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chrome_profile: Option<String>,
}

fn default_api_version() -> String {
    "v1".to_string()
}

pub fn default_config_path() -> Result<PathBuf> {
    let home = home_dir().context("无法定位用户主目录")?;
    Ok(home.join(".zentao").join("config.json"))
}

pub fn save_config(path: &Path, config: &Config) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("创建配置目录失败: {}", parent.display()))?;
    }
    let data = serde_json::to_vec_pretty(config).context("序列化配置失败")?;
    fs::write(path, data).with_context(|| format!("写入配置失败: {}", path.display()))?;
    Ok(())
}

pub fn load_config(path: &Path) -> Result<Config> {
    let data = fs::read(path).with_context(|| format!("读取配置失败: {}", path.display()))?;
    let cfg = serde_json::from_slice::<Config>(&data)
        .with_context(|| format!("解析配置失败: {}", path.display()))?;
    Ok(cfg)
}
