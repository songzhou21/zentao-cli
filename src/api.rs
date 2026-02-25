use anyhow::{anyhow, Context, Result};
use reqwest::blocking::Client;

pub struct ZentaoApi {
    site_url: String,
    client: Client,
}

impl ZentaoApi {
    pub fn new(site_url: &str, _api_version: &str) -> Result<Self> {
        let client = Client::builder().build().context("初始化 HTTP 客户端失败")?;
        Ok(Self {
            site_url: site_url.trim_end_matches('/').to_string(),
            client,
        })
    }

    pub fn verify_cookie(&self, cookie: &str) -> Result<String> {
        let resp = self
            .client
            .get(&self.site_url)
            .header("Cookie", cookie)
            .send()
            .with_context(|| format!("请求站点首页失败: {}", self.site_url))?;

        let status = resp.status();
        let final_url = resp.url().to_string();

        if !status.is_success() {
            return Err(anyhow!("cookie 校验失败: HTTP {}", status.as_u16()));
        }
        if final_url.starts_with(&format!("{}/my/", self.site_url)) {
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

    pub fn fetch_bug_html(&self, bug_id: u64, cookie: &str) -> Result<(String, String)> {
        let bug_url = format!("{}/bug-view-{}.html", self.site_url, bug_id);
        let resp = self
            .client
            .get(&bug_url)
            .header("Cookie", cookie)
            .send()
            .with_context(|| format!("请求 bug 页面失败: {}", bug_url))?;

        let status = resp.status();
        let final_url = resp.url().to_string();
        let body = resp
            .text()
            .context("读取 bug 页面响应体失败")?
            .to_string();

        if !status.is_success() {
            return Err(anyhow!(
                "获取 bug 详情失败: HTTP {} ({})",
                status.as_u16(),
                final_url
            ));
        }
        if final_url.contains("/user-login-") || final_url.contains("/user-login.") {
            return Err(anyhow!("获取 bug 详情失败: cookie 无效或已过期"));
        }
        if body.trim().is_empty() {
            return Err(anyhow!("获取 bug 详情失败: 页面内容为空"));
        }

        Ok((final_url, body))
    }
}

#[cfg(test)]
#[path = "api_test.rs"]
mod tests;
