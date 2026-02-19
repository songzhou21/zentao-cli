use aes::Aes128;
use anyhow::{anyhow, Context, Result};
use cbc::cipher::{block_padding::Pkcs7, BlockDecryptMut, KeyIvInit};
use pbkdf2::pbkdf2_hmac;
use reqwest::Url;
use rusqlite::Connection;
use sha1::Sha1;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::SystemTime;

type Aes128CbcDec = cbc::Decryptor<Aes128>;

#[derive(Clone, Debug)]
pub struct BrowserCookieItem {
    pub name: String,
    pub value: String,
    pub domain: String,
    pub path: String,
    pub secure: bool,
    pub http_only: bool,
    pub expires_utc: i64,
}

#[derive(Clone, Debug)]
pub struct BrowserCookieResult {
    pub cookie_header: String,
    pub items: Vec<BrowserCookieItem>,
}

pub fn load_zentao_cookie_from_chrome_macos(
    site_url: &str,
    profile_override: Option<&Path>,
) -> Result<BrowserCookieResult> {
    #[cfg(not(target_os = "macos"))]
    {
        let _ = site_url;
        let _ = profile_override;
        return Err(anyhow!("当前仅支持 macOS"));
    }

    #[cfg(target_os = "macos")]
    {
        let parsed = Url::parse(site_url).context("解析 URL 失败")?;
        let host = parsed
            .host_str()
            .ok_or_else(|| anyhow!("URL 缺少 host"))?
            .to_string();
        let site_path = normalize_path(parsed.path());

        let profile_dir = match profile_override {
            Some(p) => p.to_path_buf(),
            None => find_latest_chrome_profile()?,
        };
        let db_path = profile_dir.join("Cookies");
        if !db_path.exists() {
            return Err(anyhow!(
                "未找到 Chrome Cookies 数据库: {}",
                db_path.display()
            ));
        }

        // Chrome 运行时数据库常被锁定，复制到临时文件读取更稳定。
        let temp_db = std::env::temp_dir().join(format!(
            "zentao-cookies-{}.sqlite",
            std::process::id()
        ));
        let temp_wal = PathBuf::from(format!("{}-wal", temp_db.display()));
        let temp_shm = PathBuf::from(format!("{}-shm", temp_db.display()));
        fs::copy(&db_path, &temp_db)
            .with_context(|| format!("复制 Cookies 数据库失败: {}", db_path.display()))?;
        let db_wal = PathBuf::from(format!("{}-wal", db_path.display()));
        let db_shm = PathBuf::from(format!("{}-shm", db_path.display()));
        if db_wal.exists() {
            let _ = fs::copy(&db_wal, &temp_wal);
        }
        if db_shm.exists() {
            let _ = fs::copy(&db_shm, &temp_shm);
        }

        let conn = Connection::open_with_flags(
            &temp_db,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .with_context(|| format!("打开 Cookies 数据库失败: {}", temp_db.display()))?;

        let mut stmt = conn.prepare(
            "SELECT name, value, encrypted_value, path, host_key, is_secure, is_httponly, expires_utc
             FROM cookies
             WHERE host_key LIKE ?1
               AND name IN ('za', 'zentaosid', 'zp')",
        )?;

        let like_host = format!("%{}", host);
        let mut rows = stmt.query([like_host])?;

        let mut candidates: Vec<BrowserCookieItem> = Vec::new();

        let key = chrome_safe_storage_key()?;

        while let Some(row) = rows.next()? {
            let name: String = row.get(0)?;
            let value: String = row.get(1)?;
            let encrypted_value: Vec<u8> = row.get(2)?;
            let path: String = row.get(3)?;
            let row_host: String = row.get(4)?;
            let secure: i64 = row.get(5)?;
            let http_only: i64 = row.get(6)?;
            let expires_utc: i64 = row.get(7)?;

            if !host_matches(&host, &row_host) {
                continue;
            }
            if !site_path.starts_with(&normalize_path(&path)) {
                continue;
            }

            let cookie_value = if !value.is_empty() {
                value
            } else {
                decrypt_chrome_cookie_value(&encrypted_value, &key)?
            };
            candidates.push(BrowserCookieItem {
                name,
                value: cookie_value,
                domain: row_host,
                path,
                secure: secure != 0,
                http_only: http_only != 0,
                expires_utc,
            });
        }

        let _ = fs::remove_file(&temp_db);
        let _ = fs::remove_file(&temp_wal);
        let _ = fs::remove_file(&temp_shm);

        let best_za = choose_best_by_path(&candidates, "za");
        let best_sid = choose_best_by_path(&candidates, "zentaosid");
        let best_zp = choose_best_by_path(&candidates, "zp");

        let zp = best_zp
            .as_ref()
            .map(|v| v.value.clone())
            .ok_or_else(|| anyhow!("Chrome 中未找到匹配站点的 zp cookie"))?;
        let mut parts = Vec::new();
        let mut items = Vec::new();
        if let Some(za) = best_za {
            parts.push(format!("za={}", za.value));
            items.push(za);
        }
        if let Some(sid) = best_sid {
            parts.push(format!("zentaosid={}", sid.value));
            items.push(sid);
        }
        parts.push(format!("zp={}", zp));
        if let Some(zp_item) = best_zp {
            items.push(zp_item);
        }
        Ok(BrowserCookieResult {
            cookie_header: parts.join("; "),
            items,
        })
    }
}

pub fn list_chrome_profiles_macos() -> Result<Vec<PathBuf>> {
    #[cfg(not(target_os = "macos"))]
    {
        return Err(anyhow!("当前仅支持 macOS"));
    }

    #[cfg(target_os = "macos")]
    {
        let root = chrome_profiles_root()?;
        collect_chrome_profiles(&root)
    }
}

#[cfg(target_os = "macos")]
fn chrome_safe_storage_key() -> Result<[u8; 16]> {
    let services = ["Chrome Safe Storage", "Chromium Safe Storage"];
    let mut last_err = String::new();
    for service in services {
        let output = Command::new("security")
            .args(["find-generic-password", "-w", "-s", service])
            .output()
            .with_context(|| format!("调用 security 读取 {} 失败", service))?;
        if !output.status.success() {
            last_err = String::from_utf8_lossy(&output.stderr).to_string();
            continue;
        }
        let passphrase = String::from_utf8(output.stdout)
            .context("解析 Chrome Safe Storage 输出失败")?
            .trim()
            .to_string();
        if passphrase.is_empty() {
            continue;
        }

        let mut key = [0u8; 16];
        pbkdf2_hmac::<Sha1>(passphrase.as_bytes(), b"saltysalt", 1003, &mut key);
        return Ok(key);
    }
    Err(anyhow!(
        "读取 Chrome Safe Storage 失败（已尝试 Chrome/Chromium）: {}",
        last_err.trim()
    ))
}

#[cfg(target_os = "macos")]
fn decrypt_chrome_cookie_value(encrypted_value: &[u8], key: &[u8; 16]) -> Result<String> {
    if encrypted_value.is_empty() {
        return Ok(String::new());
    }
    let payload = if encrypted_value.starts_with(b"v10") || encrypted_value.starts_with(b"v11") {
        &encrypted_value[3..]
    } else {
        encrypted_value
    };

    let iv = [b' '; 16];
    let mut buf = payload.to_vec();
    let plaintext = Aes128CbcDec::new_from_slices(key, &iv)
        .map_err(|_| anyhow!("初始化 Chrome cookie 解密器失败"))?
        .decrypt_padded_mut::<Pkcs7>(&mut buf)
        .map_err(|_| anyhow!("解密 Chrome cookie 失败"))?;

    // Newer Chrome schema may prepend a 32-byte host_key hash before cookie value.
    if let Ok(s) = String::from_utf8(plaintext.to_vec()) {
        return Ok(s);
    }
    if plaintext.len() > 32 {
        if let Ok(s) = String::from_utf8(plaintext[32..].to_vec()) {
            return Ok(s);
        }
    }
    Err(anyhow!("Chrome cookie 不是有效 UTF-8"))
}

fn choose_best_by_path(candidates: &[BrowserCookieItem], name: &str) -> Option<BrowserCookieItem> {
    candidates
        .iter()
        .filter(|c| c.name == name && is_cookie_not_expired(c.expires_utc))
        .max_by_key(|c| c.path.len())
        .cloned()
}

fn is_cookie_not_expired(expires_utc: i64) -> bool {
    if expires_utc <= 0 {
        return true;
    }
    let unix = chrome_expires_utc_to_unix(expires_utc);
    unix > current_unix_ts()
}

fn chrome_expires_utc_to_unix(expires_utc: i64) -> i64 {
    (expires_utc / 1_000_000) - 11_644_473_600
}

fn current_unix_ts() -> i64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn host_matches(target_host: &str, cookie_host: &str) -> bool {
    if cookie_host == target_host {
        return true;
    }
    if let Some(stripped) = cookie_host.strip_prefix('.') {
        return target_host == stripped || target_host.ends_with(&format!(".{}", stripped));
    }
    false
}

fn normalize_path(path: &str) -> String {
    let p = if path.is_empty() { "/" } else { path };
    if p.ends_with('/') {
        p.to_string()
    } else {
        format!("{}/", p)
    }
}

#[cfg(target_os = "macos")]
fn find_latest_chrome_profile() -> Result<PathBuf> {
    let root = chrome_profiles_root()?;
    let candidates = collect_chrome_profiles(&root)?;
    if candidates.is_empty() {
        return Err(anyhow!("未找到 Chrome profile（含 Cookies）"));
    }
    let mut with_modified: Vec<(PathBuf, SystemTime)> = candidates
        .into_iter()
        .map(|path| {
            let modified = fs::metadata(path.join("Cookies"))
                .and_then(|m| m.modified())
                .unwrap_or(SystemTime::UNIX_EPOCH);
            (path, modified)
        })
        .collect();
    with_modified.sort_by_key(|(_, t)| *t);
    with_modified
        .pop()
        .map(|(p, _)| p)
        .ok_or_else(|| anyhow!("未找到 Chrome profile（含 Cookies）"))
}

#[cfg(target_os = "macos")]
fn chrome_profiles_root() -> Result<PathBuf> {
    let home = dirs::home_dir().ok_or_else(|| anyhow!("无法定位用户主目录"))?;
    Ok(home
        .join("Library")
        .join("Application Support")
        .join("Google")
        .join("Chrome"))
}

#[cfg(target_os = "macos")]
fn collect_chrome_profiles(root: &Path) -> Result<Vec<PathBuf>> {
    let mut profiles: Vec<PathBuf> = Vec::new();
    for entry in fs::read_dir(root).with_context(|| format!("读取目录失败: {}", root.display()))? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let name = path.file_name().and_then(|s| s.to_str()).unwrap_or_default();
        if name != "Default" && !name.starts_with("Profile ") {
            continue;
        }
        if path.join("Cookies").exists() {
            profiles.push(path);
        }
    }
    profiles.sort_by_key(|p| p.file_name().map(|n| n.to_os_string()).unwrap_or_default());
    if let Some(default_idx) = profiles.iter().position(|p| {
        p.file_name()
            .and_then(|n| n.to_str())
            .map(|n| n == "Default")
            .unwrap_or(false)
    }) {
        let default = profiles.remove(default_idx);
        profiles.insert(0, default);
    }
    Ok(profiles)
}
