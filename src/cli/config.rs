use crate::cli::{resolve_config_path, GlobalArgs};
use crate::config;
use crate::config::CookieSource;
use anyhow::{anyhow, Context, Result};
use clap::{Args, Subcommand, ValueEnum};

#[derive(Debug, Args)]
pub(crate) struct ConfigArgs {
    #[command(subcommand)]
    pub(crate) command: ConfigSubCommands,
}

#[derive(Debug, Subcommand)]
pub(crate) enum ConfigSubCommands {
    List,
    Get(ConfigGetArgs),
    Set(ConfigSetArgs),
}

#[derive(Debug, Args)]
pub(crate) struct ConfigGetArgs {
    pub(crate) key: ConfigKey,
}

#[derive(Debug, Args)]
pub(crate) struct ConfigSetArgs {
    pub(crate) key: ConfigKey,
    pub(crate) value: String,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub(crate) enum ConfigKey {
    Site,
    Product,
    CookieSource,
    ChromeProfile,
}

pub(crate) fn run(args: ConfigArgs, global: &GlobalArgs) -> Result<()> {
    let cfg_path = resolve_config_path(global.config.as_deref())?;
    let mut cfg = config::load_or_default(&cfg_path)?;
    match args.command {
        ConfigSubCommands::List => {
            println!("site={}", cfg.site);
            println!(
                "product={}",
                cfg.product.map(|v| v.to_string()).unwrap_or_default()
            );
            println!("cookie-source={}", cookie_source_name(&cfg.cookie_source));
            println!(
                "chrome-profile={}",
                cfg.chrome_profile.as_deref().unwrap_or_default()
            );
        }
        ConfigSubCommands::Get(args) => println!("{}", config_value(&cfg, args.key)),
        ConfigSubCommands::Set(args) => {
            set_config_value(&mut cfg, args.key, &args.value)?;
            config::save_config(&cfg_path, &cfg)?;
        }
    }
    Ok(())
}

pub(crate) fn set_config_value(cfg: &mut config::Config, key: ConfigKey, raw: &str) -> Result<()> {
    let value = raw.trim();
    match key {
        ConfigKey::Site => {
            reqwest::Url::parse(value).context("site 必须是有效 URL")?;
            cfg.site = value.trim_end_matches('/').to_string();
        }
        ConfigKey::Product => {
            let product: u64 = value.parse().context("product 必须是正整数")?;
            if product == 0 {
                return Err(anyhow!("product 必须是正整数"));
            }
            cfg.product = Some(product);
        }
        ConfigKey::CookieSource => {
            cfg.cookie_source = match value {
                "chrome" => CookieSource::Chrome,
                "file" => CookieSource::File,
                _ => return Err(anyhow!("cookie-source 仅支持 chrome 或 file")),
            };
        }
        ConfigKey::ChromeProfile => {
            cfg.chrome_profile = (!value.is_empty()).then(|| value.to_string());
        }
    }
    Ok(())
}

fn cookie_source_name(source: &CookieSource) -> &'static str {
    match source {
        CookieSource::Chrome => "chrome",
        CookieSource::File => "file",
    }
}

fn config_value(cfg: &config::Config, key: ConfigKey) -> String {
    match key {
        ConfigKey::Site => cfg.site.clone(),
        ConfigKey::Product => cfg.product.map(|v| v.to_string()).unwrap_or_default(),
        ConfigKey::CookieSource => cookie_source_name(&cfg.cookie_source).to_string(),
        ConfigKey::ChromeProfile => cfg.chrome_profile.clone().unwrap_or_default(),
    }
}
