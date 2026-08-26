use crate::api::ZentaoApi;
use crate::bug;
use crate::cli::{
    load_cookie_for_site, print_json, resolve_config_path, validate_optional_json_fields,
    GlobalArgs,
};
use crate::config;
use crate::view;
use anyhow::{Context, Result};
use clap::Args;
use std::fs;

#[derive(Debug, Args)]
pub(crate) struct BugViewArgs {
    /// Bug ID，或包含 bug-view-<id>.html 的完整 URL
    #[arg(value_name = "ID|URL")]
    pub(crate) bug: String,
    /// 输出 JSON；可选指定字段。省略时默认输出全部字段
    #[arg(
        long,
        num_args = 0..=1,
        default_missing_value = "",
        require_equals = true,
        value_name = "FIELDS"
    )]
    pub(crate) json: Option<String>,
    /// 输出禅道详情接口的原始 JSON（解开 data 转义字符串并格式化）
    #[arg(long, default_value_t = false, conflicts_with = "json")]
    pub(crate) raw_json: bool,
    /// 从完整 JSON 投影 Markdown（与 --json / --raw-json 互斥）
    #[arg(
        long,
        default_value_t = false,
        conflicts_with_all = ["json", "raw_json"]
    )]
    pub(crate) markdown: bool,
}

pub(crate) fn run(args: BugViewArgs, global: &GlobalArgs) -> Result<()> {
    validate_optional_json_fields(args.json.as_deref(), view::JSON_FIELDS)?;
    let cfg_path = resolve_config_path(global.config.as_deref())?;
    let cfg = config::load_config_optional(&cfg_path)?;
    let parsed_bug = view::parse_bug_input(
        &args.bug,
        global
            .site
            .as_deref()
            .or_else(|| cfg.as_ref().map(|c| c.site.as_str())),
    )?;

    let api_client = ZentaoApi::new(&parsed_bug.site_url)?;
    let cookie = load_cookie_for_site(&parsed_bug.site_url, None, cfg.as_ref())?;
    let (_, json_body) = api_client.fetch_bug_json(&parsed_bug.bug_url, &cookie.cookie_header)?;
    if let Ok(debug_path) = std::env::var("ZENTAO_DEBUG_JSON") {
        fs::write(&debug_path, &json_body)
            .with_context(|| format!("写入调试 JSON 失败: {debug_path}"))?;
        eprintln!("[debug] 详情 JSON 已写入 {debug_path}");
    }

    if args.raw_json {
        let raw = view::decode_raw_payload(&json_body)?;
        print_json(&raw)?;
        return Ok(());
    }

    let detail = bug::parse_bug_json(&parsed_bug.bug_url, &json_body)?;
    let fields = if args.markdown {
        ""
    } else {
        args.json.as_deref().unwrap_or("")
    };
    let json = view::render_json(parsed_bug.id, &parsed_bug.site_url, &detail, fields)?;
    if args.markdown {
        print!("{}", view::render_markdown(&json));
        return Ok(());
    }
    print_json(&json)?;
    Ok(())
}
