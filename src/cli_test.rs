use super::*;
use clap::Parser;
use reqwest::Url;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread;
use tempfile::tempdir;

// 同时提供 CLI 与配置时，CLI 参数优先级最高。
#[test]
fn resolve_required_should_prefer_cli() {
    let got = resolve_required(Some("http://cli"), Some("http://cfg"), "url").unwrap();
    assert_eq!(got, "http://cli");
}

// CLI 缺失时应回退配置值。
#[test]
fn resolve_required_should_fallback_to_config() {
    let got = resolve_required(None, Some("http://cfg"), "url").unwrap();
    assert_eq!(got, "http://cfg");
}

// CLI 与配置都缺失时应返回缺参错误。
#[test]
fn resolve_required_should_error_when_missing() {
    let err = resolve_required(None, None, "url").expect_err("should fail");
    assert!(err.to_string().contains("缺少 url"));
}

// session cookie（expires=0）应输出固定文案 session。
#[test]
fn format_cookie_expiry_session() {
    assert_eq!(format_cookie_expiry(0), "session");
}

// 已知 expires_utc 应转换成预期 UTC 时间字符串。
#[test]
fn format_cookie_expiry_known_timestamp() {
    let expires_utc = (1704067200_i64 + 11_644_473_600_i64) * 1_000_000_i64;
    assert_eq!(format_cookie_expiry(expires_utc), "2024-01-01 00:00:00 UTC");
}

// Chrome epoch 微秒应可精确转换回 Unix 秒。
#[test]
fn chrome_expires_utc_to_unix_known_timestamp() {
    let expires_utc = (1704067200_i64 + 11_644_473_600_i64) * 1_000_000_i64;
    assert_eq!(chrome_expires_utc_to_unix(expires_utc), 1704067200_i64);
}

// bug 输入为纯数字时应直接解析成 id。
#[test]
fn parse_bug_id_or_url_numeric() {
    assert_eq!(parse_bug_id_or_url("51214").unwrap(), 51214);
}

// bug 输入为详情 URL 时应提取 id。
#[test]
fn parse_bug_id_or_url_detail_url() {
    let url = "http://shendao.sharexm.cn/zentao/bug-view-51214.html";
    assert_eq!(parse_bug_id_or_url(url).unwrap(), 51214);
}

// URL 带查询参数时也应能提取 id。
#[test]
fn parse_bug_id_or_url_with_query() {
    let url = "http://shendao.sharexm.cn/zentao/bug-view-51214.html?tid=1";
    assert_eq!(parse_bug_id_or_url(url).unwrap(), 51214);
}

// 非法输入应返回明确错误。
#[test]
fn parse_bug_id_or_url_invalid() {
    let err = parse_bug_id_or_url("http://shendao/share/bug-xxx").expect_err("should fail");
    assert!(err.to_string().contains("Bug ID 无效"));
}

#[test]
fn image_download_cli_parse_success() {
    let cli = Cli::try_parse_from([
        "zentao",
        "image",
        "download",
        "--url",
        "http://example.com/a.png",
    ])
    .expect("should parse");

    match cli.command {
        Commands::Image(ImageArgs {
            command: ImageSubCommands::Download(args),
        }) => {
            assert_eq!(args.url, "http://example.com/a.png");
        }
        _ => panic!("unexpected command"),
    }
}

#[test]
fn image_download_cli_requires_url() {
    let err = Cli::try_parse_from(["zentao", "image", "download"]).expect_err("should fail");
    assert!(err.to_string().contains("--url"));
}

#[test]
fn validate_image_url_accepts_http_https() {
    assert!(validate_image_url("http://example.com/a.png").is_ok());
    assert!(validate_image_url("https://example.com/a.png").is_ok());
}

#[test]
fn validate_image_url_rejects_non_http_and_empty() {
    assert!(validate_image_url("file:///tmp/a.png").is_err());
    assert!(validate_image_url("   ").is_err());
}

#[test]
fn resolve_output_path_adds_img_extension() {
    let dir = tempdir().expect("tempdir");
    let url = Url::parse("http://example.com/file-read-1").expect("url");
    let out = resolve_output_path_from_url(dir.path(), &url);
    assert_eq!(
        out.file_name().and_then(|n| n.to_str()),
        Some("file-read-1.img")
    );
}

#[test]
fn resolve_output_path_appends_suffix_when_collision() {
    let dir = tempdir().expect("tempdir");
    let existing = dir.path().join("a.png");
    std::fs::write(&existing, b"x").expect("seed file");

    let url = Url::parse("http://example.com/a.png").expect("url");
    let out = resolve_output_path_from_url(dir.path(), &url);
    assert_eq!(out.file_name().and_then(|n| n.to_str()), Some("a(1).png"));
}

#[test]
fn download_single_image_success() {
    let (url, handle) = spawn_once_server(200, b"PNGDATA");
    let dir = tempdir().expect("tempdir");
    let out = dir.path().join("test.png");

    download_single_image(&url, &out).expect("download should succeed");
    let bytes = std::fs::read(&out).expect("read downloaded file");
    assert_eq!(bytes, b"PNGDATA");

    handle.join().expect("server thread");
}

#[test]
fn download_single_image_http_error() {
    let (url, handle) = spawn_once_server(404, b"NOTFOUND");
    let dir = tempdir().expect("tempdir");
    let out = dir.path().join("test.png");

    let err = download_single_image(&url, &out).expect_err("should fail");
    assert!(err.to_string().contains("下载失败: HTTP 404"));

    handle.join().expect("server thread");
}

fn spawn_once_server(status: u16, body: &'static [u8]) -> (Url, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind listener");
    let addr = listener.local_addr().expect("local addr");

    let handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept");
        let mut buf = [0_u8; 1024];
        let _ = stream.read(&mut buf);

        let status_line = match status {
            200 => "HTTP/1.1 200 OK",
            404 => "HTTP/1.1 404 Not Found",
            500 => "HTTP/1.1 500 Internal Server Error",
            _ => "HTTP/1.1 400 Bad Request",
        };
        let headers = format!(
            "{status_line}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        );
        stream.write_all(headers.as_bytes()).expect("write headers");
        stream.write_all(body).expect("write body");
    });

    (
        Url::parse(&format!("http://{}/image.png", addr)).expect("parse server url"),
        handle,
    )
}
