use anyhow::{anyhow, Context, Result};
use reqwest::blocking::Client;

pub struct ZentaoApi {
    site_url: String,
    client: Client,
}

impl ZentaoApi {
    pub fn new(site_url: &str, _api_version: &str) -> Result<Self> {
        let client = Client::builder()
            .build()
            .context("初始化 HTTP 客户端失败")?;
        Ok(Self {
            site_url: site_url.trim_end_matches('/').to_string(),
            client,
        })
    }

    pub fn verify_cookie(&self, cookie: &str) -> Result<String> {
        let verify_url = format!("{}/my-profile.html", self.site_url);
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
        if final_url == verify_url {
            return Ok(final_url);
        }
        if final_url.contains("/user-login-") || final_url.contains("/user-login.") {
            return Err(anyhow!("cookie 无效或已过期"));
        }

        Err(anyhow!(
            "cookie 校验失败: 发生跳转，最终地址: {}",
            final_url
        ))
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
            let redirect_url = if redirect.starts_with("http://") || redirect.starts_with("https://")
            {
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
        if body.trim().is_empty() {
            return Err(anyhow!("获取 bug 详情失败: 页面内容为空"));
        }

        Ok((final_url, body))
    }
}

/// Build the full form body for zentao search-buildQuery.
///
/// Starts with all default empty fields, then overlays user-provided
/// search conditions into the 6 available field slots.
///
/// Field overrides use a compound key to distinguish same-field-name
/// slots that differ by operator:
/// - `"assignedTo"` → slot 1 (operator `=`)
/// - `"resolvedDate_from"` → slot 2 (operator `>=`)
/// - `"resolvedDate_to"` → slot 5 (operator `<=`)
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
            field: "steps",
            operator: "include",
            value: String::new(),
            match_key: "steps",
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

    // Apply overrides by match_key
    for (key, value) in field_overrides {
        for slot in slots.iter_mut() {
            if slot.match_key == key.as_str() {
                slot.value = value.clone();
                break;
            }
        }
    }

    // Keep query shape close to Zentao UI:
    // resolvedBy is always written into slot1, and slot6 is kept empty.
    let resolved_by_value = slots[5].value.clone();
    if !resolved_by_value.trim().is_empty() {
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
