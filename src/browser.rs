use aes::Aes128;
use anyhow::{anyhow, Context, Result};
use cbc::Decryptor;
use cipher::{block_padding::Pkcs7, BlockDecryptMut, KeyIvInit};
use pbkdf2::pbkdf2_hmac_array;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use sha1::Sha1;
use std::cmp::Reverse;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};
use tempfile::NamedTempFile;
use url::Url;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserCookieItem {
    pub name: String,
    pub value: String,
    pub domain: String,
    pub path: String,
    pub secure: bool,
    pub http_only: bool,
    pub expires_utc: i64,
    pub creation_utc: i64,
    pub last_access_utc: i64,
}

#[derive(Debug, Clone)]
pub struct BrowserCookieResult {
    pub cookie_header: String,
    pub items: Vec<BrowserCookieItem>,
    pub profile_path: String,
}

pub fn list_chrome_profiles_macos() -> Result<Vec<String>> {
    ensure_macos()?;
    let root = chrome_profiles_root()?;
    collect_chrome_profiles(&root)
}

pub fn load_zentao_cookie_from_chrome_macos(
    site_url: &str,
    profile_override: Option<&str>,
) -> Result<BrowserCookieResult> {
    ensure_macos()?;

    let parsed = Url::parse(site_url).context("解析 URL 失败")?;
    let host = parsed
        .host_str()
        .ok_or_else(|| anyhow!("URL 缺少 host"))?
        .to_string();
    let site_path = normalize_path(parsed.path());

    let profile_dir = match profile_override {
        Some(v) if !v.trim().is_empty() => PathBuf::from(v),
        _ => PathBuf::from(find_latest_chrome_profile()?),
    };

    let db_path = profile_dir.join("Cookies");
    if !db_path.exists() {
        return Err(anyhow!(
            "未找到 Chrome Cookies 数据库: {}",
            db_path.display()
        ));
    }

    let temp = NamedTempFile::new().context("创建临时数据库失败")?;
    let temp_db_path = temp.path().to_path_buf();
    fs::copy(&db_path, &temp_db_path).with_context(|| {
        format!(
            "复制 Cookies 数据库失败: {} -> {}",
            db_path.display(),
            temp_db_path.display()
        )
    })?;
    let _ = copy_if_exists(
        &db_path.with_file_name("Cookies-wal"),
        &temp_db_path.with_file_name(format!(
            "{}-wal",
            temp_db_path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
        )),
    );
    let _ = copy_if_exists(
        &db_path.with_file_name("Cookies-shm"),
        &temp_db_path.with_file_name(format!(
            "{}-shm",
            temp_db_path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
        )),
    );

    let conn = Connection::open(&temp_db_path)
        .with_context(|| format!("打开 Cookies 数据库失败: {}", temp_db_path.display()))?;

    let key = chrome_safe_storage_key()?;

    let mut stmt = conn
        .prepare(
            "SELECT name, value, encrypted_value, path, host_key, is_secure, is_httponly, expires_utc, creation_utc, last_access_utc \
             FROM cookies WHERE host_key LIKE ? AND name IN ('za', 'zentaosid', 'zp', 'keepLogin')",
        )
        .context("查询 Cookies 失败")?;

    let mut rows = stmt
        .query([format!("%{host}")])
        .context("查询 Cookies 失败")?;

    let mut candidates = Vec::new();
    while let Some(row) = rows.next().context("读取 Cookies 失败")? {
        let name: String = row.get(0).context("读取 name 失败")?;
        let value: String = row.get(1).unwrap_or_default();
        let encrypted: Vec<u8> = row.get(2).unwrap_or_default();
        let path: String = row.get(3).unwrap_or_else(|_| "/".to_string());
        let row_host: String = row.get(4).unwrap_or_default();
        let secure: i64 = row.get(5).unwrap_or(0);
        let http_only: i64 = row.get(6).unwrap_or(0);
        let expires_utc: i64 = row.get(7).unwrap_or(0);
        let creation_utc: i64 = row.get(8).unwrap_or(0);
        let last_access_utc: i64 = row.get(9).unwrap_or(0);

        if !host_matches(&host, &row_host) {
            continue;
        }
        if !site_path.starts_with(&normalize_path(&path)) {
            continue;
        }

        let cookie_value = if !value.is_empty() {
            value
        } else {
            decrypt_chrome_cookie_value(&encrypted, &key)?
        };

        candidates.push(BrowserCookieItem {
            name,
            value: cookie_value,
            domain: row_host,
            path,
            secure: secure != 0,
            http_only: http_only != 0,
            expires_utc,
            creation_utc,
            last_access_utc,
        });
    }

    let best_za = choose_best_by_path(&candidates, "za");
    let best_sid = choose_best_by_path(&candidates, "zentaosid");
    let best_zp = choose_best_by_path(&candidates, "zp")
        .ok_or_else(|| anyhow!("Chrome 中未找到匹配站点的 zp cookie"))?;
    let best_keep = choose_best_by_path(&candidates, "keepLogin");

    let mut parts = Vec::new();
    let mut items = Vec::new();

    if let Some(v) = best_keep {
        parts.push(format!("keepLogin={}", v.value));
        items.push(v.clone());
    }
    if let Some(v) = best_za {
        parts.push(format!("za={}", v.value));
        items.push(v.clone());
    }
    if let Some(v) = best_sid {
        parts.push(format!("zentaosid={}", v.value));
        items.push(v.clone());
    }
    parts.push(format!("zp={}", best_zp.value));
    items.push(best_zp.clone());

    Ok(BrowserCookieResult {
        cookie_header: parts.join("; "),
        items,
        profile_path: profile_dir.to_string_lossy().to_string(),
    })
}

fn ensure_macos() -> Result<()> {
    if cfg!(target_os = "macos") {
        Ok(())
    } else {
        Err(anyhow!("当前仅支持 macOS"))
    }
}

fn chrome_safe_storage_key() -> Result<Vec<u8>> {
    let services = ["Chrome Safe Storage", "Chromium Safe Storage"];
    let mut last_err = String::new();
    for service in services {
        let output = Command::new("security")
            .args(["find-generic-password", "-w", "-s", service])
            .output();

        match output {
            Ok(out) if out.status.success() => {
                let passphrase = String::from_utf8_lossy(&out.stdout).trim().to_string();
                if passphrase.is_empty() {
                    continue;
                }
                let key = pbkdf2_hmac_array::<Sha1, 16>(passphrase.as_bytes(), b"saltysalt", 1003);
                return Ok(key.to_vec());
            }
            Ok(out) => {
                last_err = String::from_utf8_lossy(&out.stderr).trim().to_string();
            }
            Err(err) => {
                last_err = err.to_string();
            }
        }
    }
    Err(anyhow!(
        "读取 Chrome Safe Storage 失败（已尝试 Chrome/Chromium）: {}",
        last_err
    ))
}

fn decrypt_chrome_cookie_value(encrypted: &[u8], key: &[u8]) -> Result<String> {
    if encrypted.is_empty() {
        return Ok(String::new());
    }

    let payload = if encrypted.starts_with(b"v10") || encrypted.starts_with(b"v11") {
        &encrypted[3..]
    } else {
        encrypted
    };

    if payload.is_empty() || payload.len() % 16 != 0 {
        return Err(anyhow!("解密 Chrome cookie 失败"));
    }

    let iv = [b' '; 16];
    let mut buf = payload.to_vec();
    let plain = Decryptor::<Aes128>::new_from_slices(key, &iv)
        .map_err(|_| anyhow!("初始化 Chrome cookie 解密器失败"))?
        .decrypt_padded_mut::<Pkcs7>(&mut buf)
        .map_err(|_| anyhow!("解密 Chrome cookie 失败"))?;

    if let Ok(text) = String::from_utf8(plain.to_vec()) {
        return Ok(text);
    }
    if plain.len() > 32 {
        if let Ok(text) = String::from_utf8(plain[32..].to_vec()) {
            return Ok(text);
        }
    }

    Err(anyhow!("Chrome cookie 不是有效 UTF-8"))
}

fn choose_best_by_path<'a>(
    items: &'a [BrowserCookieItem],
    name: &str,
) -> Option<&'a BrowserCookieItem> {
    items
        .iter()
        .filter(|it| it.name == name && is_cookie_not_expired(it.expires_utc))
        .max_by_key(|it| {
            (
                it.path.len(),
                it.last_access_utc,
                it.creation_utc,
                it.expires_utc,
            )
        })
}

fn is_cookie_not_expired(expires_utc: i64) -> bool {
    if expires_utc <= 0 {
        return true;
    }
    chrome_expires_utc_to_unix(expires_utc) > now_unix()
}

fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

pub fn chrome_expires_utc_to_unix(expires_utc: i64) -> i64 {
    if expires_utc <= 0 {
        return 0;
    }
    (expires_utc / 1_000_000) - 11_644_473_600
}

fn host_matches(target_host: &str, cookie_host: &str) -> bool {
    if cookie_host == target_host {
        return true;
    }
    if let Some(stripped) = cookie_host.strip_prefix('.') {
        return target_host == stripped || target_host.ends_with(&format!(".{stripped}"));
    }
    false
}

fn normalize_path(path: &str) -> String {
    if path.is_empty() {
        return "/".to_string();
    }
    if path.ends_with('/') {
        return path.to_string();
    }
    format!("{path}/")
}

fn find_latest_chrome_profile() -> Result<String> {
    let root = chrome_profiles_root()?;
    let mut profiles = collect_chrome_profiles(&root)?;
    if profiles.is_empty() {
        return Err(anyhow!("未找到 Chrome profile（含 Cookies）"));
    }

    profiles.sort_by_key(|path| {
        let cookies = Path::new(path).join("Cookies");
        fs::metadata(cookies)
            .and_then(|m| m.modified())
            .map(Reverse)
            .ok()
    });

    Ok(profiles
        .first()
        .cloned()
        .ok_or_else(|| anyhow!("未找到 Chrome profile（含 Cookies）"))?)
}

fn chrome_profiles_root() -> Result<PathBuf> {
    let home = dirs::home_dir().ok_or_else(|| anyhow!("无法定位用户主目录"))?;
    Ok(home
        .join("Library")
        .join("Application Support")
        .join("Google")
        .join("Chrome"))
}

fn collect_chrome_profiles(root: &Path) -> Result<Vec<String>> {
    let mut profiles = Vec::new();
    let entries =
        fs::read_dir(root).with_context(|| format!("读取目录失败: {}", root.display()))?;

    for entry in entries {
        let entry = entry.context("读取 profile 目录失败")?;
        let meta = entry.metadata().context("读取 profile 元数据失败")?;
        if !meta.is_dir() {
            continue;
        }

        let name = entry.file_name().to_string_lossy().to_string();
        if name != "Default" && !name.starts_with("Profile ") {
            continue;
        }

        let path = entry.path();
        if path.join("Cookies").exists() {
            profiles.push(path.to_string_lossy().to_string());
        }
    }

    profiles.sort();
    if let Some(index) = profiles.iter().position(|p| {
        Path::new(p)
            .file_name()
            .map(|n| n == "Default")
            .unwrap_or(false)
    }) {
        let default = profiles.remove(index);
        profiles.insert(0, default);
    }

    Ok(profiles)
}

fn copy_if_exists(src: &Path, dst: &Path) -> Result<()> {
    if src.exists() {
        fs::copy(src, dst).with_context(|| format!("复制文件失败: {}", src.display()))?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "browser_test.rs"]
mod tests;
