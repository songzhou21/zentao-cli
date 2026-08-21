use crate::search::{self, SelectionOption};
use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, Duration, Local};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

const TTL: Duration = Duration::hours(1);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Catalog {
    #[serde(rename = "fetchedAt")]
    pub fetched_at: String,
    pub kinds: BTreeMap<String, Vec<SelectionOption>>,
}

pub fn default_dir() -> Result<PathBuf> {
    let home = dirs::home_dir().ok_or_else(|| anyhow!("无法定位用户主目录"))?;
    Ok(home.join(".zentao").join("cache"))
}

pub fn file_path(dir: &Path, product: u64) -> PathBuf {
    dir.join(format!("{product}.json"))
}

pub fn load(dir: &Path, product: u64) -> Result<Option<Catalog>> {
    let path = file_path(dir, product);
    let data = match fs::read(&path) {
        Ok(data) => data,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(err) => {
            return Err(err).with_context(|| format!("读取候选缓存失败: {}", path.display()));
        }
    };
    match serde_json::from_slice::<Catalog>(&data) {
        Ok(catalog) => Ok(Some(catalog)),
        Err(_) => Ok(None),
    }
}

pub fn save(dir: &Path, product: u64, catalog: &Catalog) -> Result<()> {
    fs::create_dir_all(dir).context("创建候选缓存目录失败")?;
    let path = file_path(dir, product);
    let data = serde_json::to_vec_pretty(catalog).context("序列化候选缓存失败")?;
    fs::write(&path, data).with_context(|| format!("写入候选缓存失败: {}", path.display()))?;
    Ok(())
}

pub fn is_fresh(catalog: &Catalog) -> bool {
    let Ok(fetched) = DateTime::parse_from_rfc3339(&catalog.fetched_at) else {
        return false;
    };
    Local::now().signed_duration_since(fetched) < TTL
}

pub fn load_fresh_kind(
    dir: &Path,
    product: u64,
    kind: &str,
) -> Result<Option<Vec<SelectionOption>>> {
    let Some(catalog) = load(dir, product)? else {
        return Ok(None);
    };
    if !is_fresh(&catalog) {
        return Ok(None);
    }
    Ok(catalog.kinds.get(kind).cloned())
}

fn catalog_from_browse_json(body: &str, fetched_at: impl Into<String>) -> Result<Catalog> {
    Ok(Catalog {
        fetched_at: fetched_at.into(),
        kinds: search::parse_browse_kinds(body)?,
    })
}

pub fn save_from_browse_json(dir: &Path, product: u64, body: &str) -> Result<Catalog> {
    let catalog = catalog_from_browse_json(body, Local::now().to_rfc3339())?;
    save(dir, product, &catalog)?;
    Ok(catalog)
}

pub fn load_or_fetch(
    dir: &Path,
    product: u64,
    kind: &str,
    fetch: impl FnOnce() -> Result<String>,
) -> Result<Vec<SelectionOption>> {
    if let Some(rows) = load_fresh_kind(dir, product, kind)? {
        return Ok(rows);
    }
    let body = fetch()?;
    match save_from_browse_json(dir, product, &body) {
        Ok(catalog) => Ok(catalog.kinds.get(kind).cloned().unwrap_or_default()),
        Err(_) => Ok(search::parse_browse_kinds(&body)?
            .remove(kind)
            .unwrap_or_default()),
    }
}

#[cfg(test)]
#[path = "cache_test.rs"]
mod tests;
