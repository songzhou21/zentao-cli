use crate::api::ZentaoApi;
use crate::cache;
use crate::cli::{
    load_cookie_for_site, parse_json_fields, print_json, resolve_config_path, resolve_required,
    validate_optional_json_fields, GlobalArgs,
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
        .args(["build"])
))]
pub(crate) struct BugSelectionArgs {
    /// 列出版本候选；value 用于 --opened-build / --resolved-build
    #[arg(long)]
    pub(crate) build: bool,

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

pub(crate) fn run(args: BugSelectionArgs, global: &GlobalArgs) -> Result<()> {
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
    let rows = if args.build {
        let builds = cache::load_or_fetch(&cache_dir, product, search::KIND_BUILD, || {
            api_client.fetch_product_browse_json(&cookie.cookie_header, product)
        })?;
        search::filter_builds(&builds, args.keyword.as_deref())
            .into_iter()
            .map(|build| SelectionRow {
                value: build.value.clone(),
                name: build.name.clone(),
            })
            .collect()
    } else {
        Vec::new()
    };
    let fields = args.json.as_deref().unwrap_or("");
    print_json(&render_selection_json(&rows, fields)?)?;
    Ok(())
}

#[derive(Debug, Clone)]
pub(crate) struct SelectionRow {
    pub(crate) value: String,
    pub(crate) name: String,
}

pub(crate) fn render_selection_json(rows: &[SelectionRow], fields: &str) -> Result<Value> {
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
