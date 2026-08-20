use crate::cli::{load_cookie_for_site, resolve_config_path, GlobalArgs};
use crate::config;
use anyhow::{anyhow, Context, Result};
use clap::{Args, Subcommand};
use reqwest::Url;
use std::fs;
use std::path::{Path, PathBuf};

const IMAGE_DOWNLOAD_DIR: &str = "/tmp/zentao-images";

#[derive(Debug, Args)]
pub(crate) struct ImageArgs {
    #[command(subcommand)]
    pub(crate) command: ImageSubCommands,
}

#[derive(Debug, Subcommand)]
pub(crate) enum ImageSubCommands {
    Download(ImageDownloadArgs),
}

#[derive(Debug, Args)]
pub(crate) struct ImageDownloadArgs {
    /// 图片 URL（仅支持 http/https）
    #[arg(long)]
    pub(crate) url: String,
    /// 下载输出目录，默认 `/tmp/zentao-images`
    #[arg(short = 'o', long)]
    pub(crate) output_dir: Option<String>,
}

pub(crate) fn run(args: ImageArgs, global: &GlobalArgs) -> Result<()> {
    match args.command {
        ImageSubCommands::Download(d) => run_download(d, global),
    }
}

fn run_download(args: ImageDownloadArgs, global: &GlobalArgs) -> Result<()> {
    let image_url = validate_image_url(&args.url)?;
    let cfg_path = resolve_config_path(global.config.as_deref())?;
    let cfg = config::load_config_optional(&cfg_path)?;
    let cookie_site = global
        .site
        .as_deref()
        .filter(|site| !site.trim().is_empty())
        .map(str::to_string)
        .unwrap_or(derive_site_url_from_image_url(&image_url)?);
    let cookie = load_cookie_for_site(&cookie_site, None, cfg.as_ref())?;

    let out_dir = Path::new(args.output_dir.as_deref().unwrap_or(IMAGE_DOWNLOAD_DIR));
    let out_path = resolve_output_path_from_url(out_dir, &image_url);
    let started = std::time::Instant::now();
    download_single_image(&image_url, &cookie.cookie_header, &out_path)?;
    let elapsed_ms = started.elapsed().as_millis();
    println!(
        "Downloaded: {} -> {} ({}ms)",
        image_url,
        out_path.display(),
        elapsed_ms
    );
    Ok(())
}

pub(crate) fn validate_image_url(raw: &str) -> Result<Url> {
    let v = raw.trim();
    if v.is_empty() {
        return Err(anyhow!("图片 URL 无效"));
    }
    let url = Url::parse(v).map_err(|_| anyhow!("图片 URL 无效"))?;
    match url.scheme() {
        "http" | "https" => Ok(url),
        _ => Err(anyhow!("图片 URL 无效")),
    }
}

pub(crate) fn derive_site_url_from_image_url(url: &Url) -> Result<String> {
    let mut base = url.clone();
    let mut segments: Vec<String> = base
        .path_segments()
        .map(|parts| parts.map(str::to_string).collect())
        .unwrap_or_default();
    if segments.pop().is_none() {
        return Err(anyhow!("图片 URL 无效: 缺少文件路径"));
    }

    let new_path = if segments.is_empty() {
        "/".to_string()
    } else {
        format!("/{}", segments.join("/"))
    };
    base.set_path(&new_path);
    base.set_query(None);
    base.set_fragment(None);
    Ok(base.to_string().trim_end_matches('/').to_string())
}

pub(crate) fn resolve_output_path_from_url(base_dir: &Path, url: &Url) -> PathBuf {
    let name = url
        .path_segments()
        .and_then(|segments| segments.rev().find(|seg| !seg.is_empty()))
        .filter(|seg| !seg.trim().is_empty())
        .unwrap_or("downloaded-image.img");

    let filename = ensure_filename_extension(name);
    unique_file_path(base_dir, &filename)
}

pub(crate) fn download_single_image(url: &Url, cookie_header: &str, out: &Path) -> Result<()> {
    let client = reqwest::blocking::Client::builder()
        .build()
        .context("初始化 HTTP 客户端失败")?;

    let resp = client
        .get(url.clone())
        .header(reqwest::header::COOKIE, cookie_header)
        .send()
        .with_context(|| format!("下载图片失败: {}", url))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(anyhow!("下载失败: HTTP {}", status.as_u16()));
    }
    if is_login_page_url(resp.url()) {
        return Err(anyhow!("图片下载失败：认证已失效，跳转到了登录页"));
    }
    let content_type = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("");
    if !content_type.to_ascii_lowercase().starts_with("image/") {
        return Err(anyhow!(
            "图片下载失败：响应不是图片（Content-Type: {}）",
            if content_type.is_empty() {
                "缺失"
            } else {
                content_type
            }
        ));
    }
    let body = resp.bytes().context("读取图片响应体失败")?;
    if let Some(parent) = out.parent() {
        fs::create_dir_all(parent).context("创建图片下载目录失败")?;
    }
    fs::write(out, &body).with_context(|| format!("写入图片失败: {}", out.display()))?;
    Ok(())
}

fn ensure_filename_extension(filename: &str) -> String {
    let p = Path::new(filename);
    if p.extension().is_some() {
        return filename.to_string();
    }
    format!("{filename}.img")
}

fn unique_file_path(base_dir: &Path, filename: &str) -> PathBuf {
    let mut candidate = base_dir.join(filename);
    if !candidate.exists() {
        return candidate;
    }

    let p = Path::new(filename);
    let stem = p
        .file_stem()
        .and_then(|s| s.to_str())
        .filter(|s| !s.is_empty())
        .unwrap_or("image");
    let ext = p.extension().and_then(|s| s.to_str()).unwrap_or("");

    let mut idx = 1usize;
    loop {
        let next = if ext.is_empty() {
            format!("{stem}({idx})")
        } else {
            format!("{stem}({idx}).{ext}")
        };
        candidate = base_dir.join(next);
        if !candidate.exists() {
            return candidate;
        }
        idx += 1;
    }
}

fn is_login_page_url(url: &Url) -> bool {
    let path = url.path();
    path.contains("/user-login-") || path.contains("/user-login.")
}
