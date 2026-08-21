use crate::api::ZentaoApi;
use crate::cache;
use crate::cli::bug::list::{normalize_table_cell, pad_to_display_width};
use crate::cli::{
    load_cookie_for_site, parse_json_fields, print_json, resolve_config_path, resolve_required,
    style_header, validate_optional_json_fields, GlobalArgs,
};
use crate::config;
use crate::search;
use anyhow::{anyhow, Result};
use clap::{ArgGroup, Args};
use serde_json::{json, Map, Value};

pub(crate) const JSON_FIELDS: &[&str] = &["value", "name"];

#[derive(Debug, Args)]
#[command(group(
    ArgGroup::new("kind")
        .required(true)
        .multiple(false)
        .args(["build", "module"])
))]
pub(crate) struct BugCandidatesArgs {
    /// 列出版本候选；value 用于 --opened-build / --resolved-build
    #[arg(long)]
    pub(crate) build: bool,

    /// 列出模块候选；value 用于 --module
    #[arg(long)]
    pub(crate) module: bool,

    /// 按名称包含过滤（可选）
    #[arg(value_name = "KEYWORD")]
    pub(crate) keyword: Option<String>,

    /// 产品 ID；未提供时从 ZENTAO_PRODUCT 或配置读取
    #[arg(long, env = "ZENTAO_PRODUCT", value_name = "ID")]
    pub(crate) product: Option<u64>,

    /// 输出 JSON；可选指定字段：value,name。省略时默认输出全部字段
    #[arg(
        long,
        num_args = 0..=1,
        default_missing_value = "",
        require_equals = true,
        value_name = "FIELDS"
    )]
    pub(crate) json: Option<String>,
}

pub(crate) fn run(args: BugCandidatesArgs, global: &GlobalArgs) -> Result<()> {
    validate_optional_json_fields(args.json.as_deref(), JSON_FIELDS)?;
    let cfg_path = resolve_config_path(global.config.as_deref())?;
    let cfg = config::load_config_optional(&cfg_path)?;
    let site_url = resolve_required(
        global.site.as_deref(),
        cfg.as_ref().map(|c| c.site.as_str()),
        "site",
    )?;
    let product = args
        .product
        .or_else(|| cfg.as_ref().and_then(|c| c.product))
        .ok_or_else(|| anyhow!("缺少 product，请通过 --product、ZENTAO_PRODUCT 或配置文件提供"))?;
    if product == 0 {
        return Err(anyhow!("product 必须是正整数"));
    }

    let api_client = ZentaoApi::new(&site_url)?;
    let cookie = load_cookie_for_site(&site_url, None, cfg.as_ref())?;
    let cache_dir = cache::default_dir()?;
    let kind = if args.build {
        search::KIND_BUILD
    } else {
        search::KIND_MODULE
    };
    let options = cache::load_or_fetch(&cache_dir, product, kind, || {
        api_client.fetch_product_browse_json(&cookie.cookie_header, product)
    })?;
    let rows = search::filter_options(&options, args.keyword.as_deref())
        .into_iter()
        .map(|option| CandidateRow {
            value: option.value.clone(),
            name: option.name.clone(),
        })
        .collect::<Vec<_>>();
    let name_header = if args.build { "版本" } else { "模块" };
    let fields = args.json.as_deref();
    if let Some(fields) = fields {
        print_json(&render_candidates_json(&rows, fields)?)?;
    } else {
        print!("{}", render_candidates_table(&rows, name_header));
    }
    Ok(())
}

/// 编号列的固定显示宽度。
const CANDIDATE_VALUE_WIDTH: usize = 8;

pub(crate) fn render_candidates_table(rows: &[CandidateRow], name_header: &str) -> String {
    if rows.is_empty() {
        return format!("没有匹配的{name_header}\n");
    }
    let header = format!(
        "{} {}",
        pad_to_display_width("编号", CANDIDATE_VALUE_WIDTH),
        name_header,
    );
    let mut out = format!("{}\n", style_header(&header));
    for row in rows {
        out.push_str(&format!(
            "{} {}\n",
            pad_to_display_width(&row.value, CANDIDATE_VALUE_WIDTH),
            normalize_table_cell(&row.name),
        ));
    }
    out
}

#[derive(Debug, Clone)]
pub(crate) struct CandidateRow {
    pub(crate) value: String,
    pub(crate) name: String,
}

pub(crate) fn render_candidates_json(rows: &[CandidateRow], fields: &str) -> Result<Value> {
    let fields = parse_json_fields(fields, JSON_FIELDS)?;
    Ok(Value::Array(
        rows.iter()
            .map(|row| {
                let mut out = Map::new();
                for field in &fields {
                    let value = match field.as_str() {
                        "value" => json!(row.value),
                        "name" => json!(row.name),
                        _ => Value::Null,
                    };
                    out.insert(field.to_string(), value);
                }
                Value::Object(out)
            })
            .collect(),
    ))
}
