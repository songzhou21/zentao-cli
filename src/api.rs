use anyhow::{anyhow, Context, Result};
use reqwest::Client;

pub struct ZentaoApi {
    site_url: String,
    client: Client,
}

impl ZentaoApi {
    pub fn new(url: &str, _api_version: &str) -> Result<Self> {
        let site_url = url.trim_end_matches('/').to_string();
        let client = Client::builder()
            .build()
            .context("初始化 HTTP 客户端失败")?;
        Ok(Self { site_url, client })
    }

    pub async fn verify_cookie(&self, cookie: &str) -> Result<String> {
        let resp = self
            .client
            .get(&self.site_url)
            .header("Cookie", cookie)
            .send()
            .await
            .context("请求站点首页失败")?;
        let final_url = resp.url().to_string();
        let status = resp.status();

        if !status.is_success() {
            return Err(anyhow!("cookie 校验失败: HTTP {}", status.as_u16()));
        }

        let my_url_prefix = format!("{}/my/", self.site_url);
        if final_url.starts_with(&my_url_prefix) {
            return Ok(final_url);
        }

        if final_url.contains("/user-login-") || final_url.contains("/user-login.") {
            return Err(anyhow!("cookie 无效或已过期"));
        }

        Err(anyhow!(
            "cookie 校验失败: 未命中预期跳转，最终地址: {}",
            final_url
        ))
    }
}
