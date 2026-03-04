use crate::browser::BrowserCookieItem;
use anyhow::{anyhow, Context, Result};
use regex::Regex;
use reqwest::blocking::Client;
use reqwest::header::{HeaderMap, SET_COOKIE};
use serde_json::Value;
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

pub struct ZentaoApi {
    site_url: String,
    client: Client,
}

#[derive(Debug, Clone)]
pub struct LoginResult {
    pub cookies: Vec<BrowserCookieItem>,
    pub login_response_body: String,
    pub set_cookies_by_url: Vec<(String, Vec<String>)>,
}

impl ZentaoApi {
    pub fn new(site_url: &str, _api_version: &str) -> Result<Self> {
        Self::new_with_proxy(site_url, _api_version, None)
    }

    pub fn new_with_proxy(site_url: &str, _api_version: &str, proxy: Option<&str>) -> Result<Self> {
        let mut builder = Client::builder().cookie_store(true);
        if let Some(p) = proxy {
            let p = p.trim();
            if !p.is_empty() {
                builder = builder
                    .no_proxy()
                    .proxy(reqwest::Proxy::all(p).with_context(|| format!("代理配置无效: {}", p))?);
            }
        }
        let client = builder.build().context("初始化 HTTP 客户端失败")?;
        Ok(Self {
            site_url: site_url.trim_end_matches('/').to_string(),
            client,
        })
    }

    pub fn verify_cookie(&self, cookie: &str) -> Result<String> {
        let verify_url = format!("{}/", self.site_url);
        let resp = self
            .client
            .get(&verify_url)
            .header("Cookie", cookie)
            .send()
            .with_context(|| format!("请求校验页面失败: {}", verify_url))?;

        let status = resp.status();
        let final_url = resp.url().to_string();

        if !status.is_success() {
            return Err(anyhow!("cookie 校验失败: HTTP {}", status.as_u16()));
        }
        if final_url.contains("/user-login-") || final_url.contains("/user-login.") {
            return Err(anyhow!("cookie 无效或已过期"));
        }

        let body = resp.text().context("读取校验页面失败")?;

        // Check for JS redirect to login page (e.g. self.location='/zentao/user-login-...')
        if let Some(redirect) = extract_js_redirect(&body) {
            if redirect.contains("/user-login-") || redirect.contains("/user-login.") {
                return Err(anyhow!("cookie 无效或已过期"));
            }
        }

        Ok(final_url)
    }

    /// Build a search query and fetch the resulting bug browse page.
    ///
    /// The flow mirrors the browser behaviour:
    /// 1. POST form data to `{site}/search-buildQuery.html`
    /// 2. The server responds with a redirect to the bug browse page
    /// 3. Follow the redirect and return the HTML body
    pub fn search_bugs(
        &self,
        cookie: &str,
        product_id: u64,
        form_params: &[(String, String)],
    ) -> Result<String> {
        let search_url = format!("{}/search-buildQuery.html", self.site_url);
        // Derive the path prefix from site_url (e.g. "http://host/zentao" → "/zentao")
        let path_prefix = reqwest::Url::parse(&self.site_url)
            .map(|u| u.path().trim_end_matches('/').to_string())
            .unwrap_or_default();
        let action_url = format!(
            "{}/bug-browse-{}-0-bySearch-myQueryID.html",
            path_prefix, product_id
        );

        // Build the full form body with defaults + user-provided field overrides
        let form = build_search_form(product_id, &action_url, form_params);

        let resp = self
            .client
            .post(&search_url)
            .header("Cookie", cookie)
            .form(&form)
            .send()
            .with_context(|| format!("搜索请求失败: {}", search_url))?;

        let status = resp.status();
        let final_url = resp.url().to_string();
        let mut body = resp.text().context("读取搜索结果页面失败")?;

        if !status.is_success() {
            return Err(anyhow!(
                "搜索失败: HTTP {} ({})",
                status.as_u16(),
                final_url
            ));
        }
        if final_url.contains("/user-login-") || final_url.contains("/user-login.") {
            return Err(anyhow!("搜索失败: cookie 无效或已过期"));
        }

        // Some Zentao responses return a tiny JS bridge page:
        // <script>parent.location='/zentao/bug-browse-...';</script>
        // Follow it to fetch the actual bug table HTML.
        if let Some(redirect) = extract_js_redirect(&body) {
            // Check for JS redirect to login page before following
            if redirect.contains("/user-login-") || redirect.contains("/user-login.") {
                return Err(anyhow!("搜索失败: cookie 无效或已过期"));
            }
            let redirect_url =
                if redirect.starts_with("http://") || redirect.starts_with("https://") {
                    redirect
                } else {
                    let base = reqwest::Url::parse(&format!("{}/", self.site_url))
                        .context("解析站点 URL 失败")?;
                    base.join(&redirect)
                        .map(|u| u.to_string())
                        .with_context(|| format!("拼接搜索跳转地址失败: {}", redirect))?
                };

            let resp2 = self
                .client
                .get(&redirect_url)
                .header("Cookie", cookie)
                .send()
                .with_context(|| format!("请求搜索跳转页面失败: {}", redirect_url))?;

            let status2 = resp2.status();
            let final_url2 = resp2.url().to_string();
            body = resp2.text().context("读取搜索跳转页面失败")?;

            if !status2.is_success() {
                return Err(anyhow!(
                    "搜索失败: HTTP {} ({})",
                    status2.as_u16(),
                    final_url2
                ));
            }
            if final_url2.contains("/user-login-") || final_url2.contains("/user-login.") {
                return Err(anyhow!("搜索失败: cookie 无效或已过期"));
            }
        }

        if body.trim().is_empty() {
            return Err(anyhow!("搜索失败: 页面内容为空"));
        }

        Ok(body)
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
        let body = resp.text().context("读取 bug 页面响应体失败")?.to_string();

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
        // Check for JS redirect to login page
        if let Some(redirect) = extract_js_redirect(&body) {
            if redirect.contains("/user-login-") || redirect.contains("/user-login.") {
                return Err(anyhow!("获取 bug 详情失败: cookie 无效或已过期"));
            }
        }
        if body.trim().is_empty() {
            return Err(anyhow!("获取 bug 详情失败: 页面内容为空"));
        }

        Ok((final_url, body))
    }

    pub fn login_with_password(
        &self,
        username: &str,
        password: &str,
        keep_login: bool,
    ) -> Result<LoginResult> {
        let login_page_url = format!("{}/user-login-L3plbnRhby8=.html", self.site_url);
        let login_url = format!("{}/user-login.html", self.site_url);
        let my_url = format!("{}/my/", self.site_url);
        let target_host = reqwest::Url::parse(&self.site_url)
            .context("解析站点 URL 失败")?
            .host_str()
            .ok_or_else(|| anyhow!("站点 URL 缺少 host"))?
            .to_string();

        let mut cookie_map: HashMap<String, BrowserCookieItem> = HashMap::new();
        let mut set_cookies_by_url: Vec<(String, Vec<String>)> = Vec::new();

        let page_resp = self
            .client
            .get(&login_page_url)
            .send()
            .with_context(|| format!("请求登录页失败: {}", login_page_url))?;
        let page_headers = page_resp.headers().clone();
        let page_html = page_resp.text().context("读取登录页失败")?;
        let set_cookie_page = collect_set_cookie_lines(&page_headers);
        merge_cookie_items(
            &mut cookie_map,
            parse_set_cookie_items(&set_cookie_page, &target_host)?,
        );
        set_cookies_by_url.push((login_page_url.clone(), set_cookie_page));

        let verify_rand = parse_verify_rand(&page_html)?;
        let password_hash = md5_hex(&format!("{}{}", md5_hex(password), verify_rand));

        let mut form = vec![
            ("account", username.to_string()),
            ("password", password_hash),
            ("passwordStrength", "2".to_string()),
            ("referer", "/zentao/".to_string()),
            ("verifyRand", verify_rand),
            ("keepLogin", if keep_login { "1" } else { "0" }.to_string()),
        ];
        if keep_login {
            form.push(("keepLogin[]", "on".to_string()));
        }

        let login_resp = self
            .client
            .post(&login_url)
            .header("Accept", "application/json, text/javascript, */*; q=0.01")
            .header(
                "Content-Type",
                "application/x-www-form-urlencoded; charset=UTF-8",
            )
            .header("X-Requested-With", "XMLHttpRequest")
            .header("Origin", site_origin(&self.site_url)?)
            .header("Referer", &login_url)
            .form(&form)
            .send()
            .with_context(|| format!("登录请求失败: {}", login_url))?;
        let login_headers = login_resp.headers().clone();
        let login_response_body = login_resp.text().context("读取登录响应失败")?;
        if !login_response_body.contains("\"result\":\"success\"") {
            return Err(anyhow!(
                "登录失败: {}",
                summarize_login_response(&login_response_body)
            ));
        }
        let set_cookie_login = collect_set_cookie_lines(&login_headers);
        merge_cookie_items(
            &mut cookie_map,
            parse_set_cookie_items(&set_cookie_login, &target_host)?,
        );
        set_cookies_by_url.push((login_url.clone(), set_cookie_login));

        let my_resp = self
            .client
            .get(&my_url)
            .send()
            .with_context(|| format!("请求登录后页面失败: {}", my_url))?;
        let my_headers = my_resp.headers().clone();
        let final_url = my_resp.url().to_string();
        let my_html = my_resp.text().context("读取登录后页面失败")?;
        if final_url.contains("/user-login-") || final_url.contains("/user-login.") {
            return Err(anyhow!("登录后校验失败: 跳转到登录页"));
        }
        if my_html.trim().is_empty() {
            return Err(anyhow!("登录后校验失败: 页面内容为空"));
        }
        let set_cookie_my = collect_set_cookie_lines(&my_headers);
        merge_cookie_items(
            &mut cookie_map,
            parse_set_cookie_items(&set_cookie_my, &target_host)?,
        );
        set_cookies_by_url.push((my_url, set_cookie_my));

        let mut cookies: Vec<BrowserCookieItem> = cookie_map.into_values().collect();
        cookies.sort_by(|a, b| a.name.cmp(&b.name));
        if cookies.is_empty() {
            return Err(anyhow!("登录成功但未捕获到任何 cookie"));
        }

        Ok(LoginResult {
            cookies,
            login_response_body,
            set_cookies_by_url,
        })
    }
}

fn collect_set_cookie_lines(headers: &HeaderMap) -> Vec<String> {
    headers
        .get_all(SET_COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .collect()
}

fn parse_set_cookie_items(lines: &[String], default_host: &str) -> Result<Vec<BrowserCookieItem>> {
    let now_secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| anyhow!("系统时间错误"))?
        .as_secs() as i64;
    let now_chrome = unix_to_chrome_expires_utc(now_secs);
    let mut out = Vec::new();

    for line in lines {
        let mut parts = line.split(';').map(str::trim);
        let first = match parts.next() {
            Some(v) => v,
            None => continue,
        };
        let (name, value) = match first.split_once('=') {
            Some((k, v)) => (k.trim(), v.trim()),
            None => continue,
        };
        if name.is_empty() {
            continue;
        }

        let mut domain = default_host.to_string();
        let mut path = "/".to_string();
        let mut secure = false;
        let mut http_only = false;
        let mut expires_utc = 0_i64;

        for attr in parts {
            let lower = attr.to_ascii_lowercase();
            if lower == "secure" {
                secure = true;
                continue;
            }
            if lower == "httponly" {
                http_only = true;
                continue;
            }
            if let Some((k, v)) = attr.split_once('=') {
                let key = k.trim().to_ascii_lowercase();
                let val = v.trim();
                match key.as_str() {
                    "domain" if !val.is_empty() => domain = val.to_string(),
                    "path" if !val.is_empty() => path = val.to_string(),
                    "max-age" => {
                        if let Ok(sec) = val.parse::<i64>() {
                            expires_utc = unix_to_chrome_expires_utc(now_secs + sec.max(0));
                        }
                    }
                    "expires" => {
                        let _ = val;
                    }
                    _ => {}
                }
            }
        }

        out.push(BrowserCookieItem {
            name: name.to_string(),
            value: value.to_string(),
            domain,
            path,
            secure,
            http_only,
            expires_utc,
            creation_utc: now_chrome,
            last_access_utc: now_chrome,
        });
    }

    Ok(out)
}

fn merge_cookie_items(
    target: &mut HashMap<String, BrowserCookieItem>,
    incoming: Vec<BrowserCookieItem>,
) {
    for item in incoming {
        target.insert(item.name.clone(), item);
    }
}

fn parse_verify_rand(html: &str) -> Result<String> {
    let re = Regex::new(r#"name=['"]verifyRand['"][^>]*value=['"](\d+)['"]"#)
        .context("初始化 verifyRand 正则失败")?;
    let v = re
        .captures(html)
        .and_then(|c| c.get(1).map(|m| m.as_str().to_string()))
        .ok_or_else(|| anyhow!("登录页缺少 verifyRand"))?;
    Ok(v)
}

fn md5_hex(input: &str) -> String {
    format!("{:x}", md5::compute(input.as_bytes()))
}

fn unix_to_chrome_expires_utc(unix_secs: i64) -> i64 {
    (unix_secs + 11_644_473_600_i64) * 1_000_000_i64
}

fn site_origin(site_url: &str) -> Result<String> {
    let parsed = reqwest::Url::parse(site_url).context("解析站点 URL 失败")?;
    let host = parsed
        .host_str()
        .ok_or_else(|| anyhow!("站点 URL 缺少 host"))?;
    match parsed.port() {
        Some(port) => Ok(format!("{}://{}:{}", parsed.scheme(), host, port)),
        None => Ok(format!("{}://{}", parsed.scheme(), host)),
    }
}

fn summarize_login_response(raw: &str) -> String {
    if let Ok(v) = serde_json::from_str::<Value>(raw) {
        let result = v.get("result").and_then(Value::as_str).unwrap_or("");
        let message = v.get("message").and_then(Value::as_str).unwrap_or("");
        let locate = v.get("locate").and_then(Value::as_str).unwrap_or("");
        let mut parts = Vec::new();
        if !result.is_empty() {
            parts.push(format!("result={}", result));
        }
        if !message.is_empty() {
            parts.push(format!("message={}", message));
        }
        if !locate.is_empty() {
            parts.push(format!("locate={}", locate));
        }
        if !parts.is_empty() {
            return parts.join(", ");
        }
    }
    raw.to_string()
}

/// Build the full form body for zentao search-buildQuery.
///
/// Starts with all default empty fields, then overlays user-provided
/// search conditions into the 6 available field slots.
///
/// Field overrides use a compound key to distinguish same-field-name
/// slots that differ by operator:
/// - `"assignedTo"` → slot 1 (operator `=`)
/// - `"module"` → slot 1 (operator `belong`)
/// - `"title"` → slot 3 (operator `include`)
/// - `"resolvedDate_from"` → slot 2 (operator `>=`)
/// - `"resolvedDate_to"` → slot 5 (operator `<=`)
/// - `"status"` → slot 4 (operator `=`)
/// - `"resolvedBy"` → slot 1 (operator `=`), slot 6 保持空值
fn build_search_form(
    product_id: u64,
    action_url: &str,
    field_overrides: &[(String, String)],
) -> Vec<(String, String)> {
    // Default empty field names for the query builder header
    let default_fields = [
        "title",
        "keywords",
        "steps",
        "assignedTo",
        "resolvedBy",
        "consumed",
        "status",
        "confirmed",
        "product",
        "plan",
        "module",
        "project",
        "severity",
        "pri",
        "type",
        "os",
        "browser",
        "resolution",
        "activatedCount",
        "toTask",
        "toStory",
        "openedBy",
        "closedBy",
        "lastEditedBy",
        "mailto",
        "openedBuild",
        "resolvedBuild",
        "openedDate",
        "assignedDate",
        "resolvedDate",
        "closedDate",
        "lastEditedDate",
        "deadline",
        "id",
    ];

    let mut form: Vec<(String, String)> = Vec::new();

    // Add "field*" params (all empty by default)
    for name in &default_fields {
        let key = format!("field{name}");
        form.push((key, String::new()));
    }

    // Override specific defaults
    set_form_value(&mut form, "fieldconfirmed", "ZERO");
    set_form_value(&mut form, "fieldproduct", &product_id.to_string());
    set_form_value(&mut form, "fieldmodule", "ZERO");
    set_form_value(&mut form, "fieldseverity", "0");
    set_form_value(&mut form, "fieldpri", "0");

    // 6 search slots: (andOr, field, operator, value)
    // Compound keys for matching: "field" or "field_from"/"field_to" for date ranges
    struct Slot {
        and_or: &'static str,
        field: &'static str,
        operator: &'static str,
        value: String,
        /// Key used to match overrides against this slot
        match_key: &'static str,
    }

    let mut slots = vec![
        Slot {
            and_or: "AND",
            field: "assignedTo",
            operator: "=",
            value: String::new(),
            match_key: "assignedTo",
        },
        Slot {
            and_or: "and",
            field: "resolvedDate",
            operator: ">=",
            value: String::new(),
            match_key: "resolvedDate_from",
        },
        Slot {
            and_or: "and",
            field: "keywords",
            operator: "include",
            value: String::new(),
            match_key: "keywords",
        },
        Slot {
            and_or: "AND",
            field: "status",
            operator: "=",
            value: String::new(),
            match_key: "status",
        },
        Slot {
            and_or: "and",
            field: "resolvedDate",
            operator: "<=",
            value: String::new(),
            match_key: "resolvedDate_to",
        },
        Slot {
            and_or: "and",
            field: "resolvedBy",
            operator: "=",
            value: String::new(),
            match_key: "resolvedBy",
        },
    ];

    let slot_index_by_key: HashMap<&str, usize> = slots
        .iter()
        .enumerate()
        .map(|(idx, slot)| (slot.match_key, idx))
        .collect();

    // Apply overrides by match_key
    for (key, value) in field_overrides {
        if key == "module" {
            slots[0].field = "module";
            slots[0].operator = "belong";
            slots[0].value = value.clone();
            continue;
        }
        if key == "title" {
            slots[2].field = "title";
            slots[2].operator = "include";
            slots[2].value = value.clone();
            continue;
        }
        if let Some(&idx) = slot_index_by_key.get(key.as_str()) {
            slots[idx].value = value.clone();
        }
    }

    // Keep query shape close to Zentao UI:
    // resolvedBy is always written into slot1, and slot6 is kept empty.
    let resolved_by_value = slots[5].value.clone();
    if !resolved_by_value.trim().is_empty() && slots[0].field == "assignedTo" {
        slots[0].field = "resolvedBy";
        slots[0].operator = "=";
        slots[0].value = resolved_by_value;
        slots[5].value.clear();
    }

    // Emit slot params
    for (i, slot) in slots.iter().enumerate() {
        let n = i + 1;
        form.push((format!("andOr{n}"), slot.and_or.to_string()));
        form.push((format!("field{n}"), slot.field.to_string()));
        form.push((format!("operator{n}"), slot.operator.to_string()));
        form.push((format!("value{n}"), slot.value.clone()));

        // groupAndOr between group 1 (slots 1-3) and group 2 (slots 4-6)
        if n == 3 {
            form.push(("groupAndOr".to_string(), "and".to_string()));
        }
    }

    // Fixed metadata params
    form.push(("module".to_string(), "bug".to_string()));
    form.push(("actionURL".to_string(), action_url.to_string()));
    form.push(("groupItems".to_string(), "3".to_string()));
    form.push((
        "formType".to_string(),
        format!("more{}-0-bySearch-myQueryID.html", product_id),
    ));

    form
}

fn set_form_value(form: &mut Vec<(String, String)>, key: &str, value: &str) {
    if let Some(entry) = form.iter_mut().find(|(k, _)| k == key) {
        entry.1 = value.to_string();
    }
}

fn extract_js_redirect(html: &str) -> Option<String> {
    let markers = [
        ("parent.location='", '\''),
        ("parent.location=\"", '"'),
        ("self.location='", '\''),
        ("self.location=\"", '"'),
    ];
    for (marker, quote) in markers {
        if let Some(idx) = html.find(marker) {
            let rest = &html[idx + marker.len()..];
            if let Some(end) = rest.find(quote) {
                let target = rest[..end].trim();
                if !target.is_empty() {
                    return Some(target.to_string());
                }
            }
        }
    }
    None
}

#[cfg(test)]
#[path = "api_test.rs"]
mod tests;
