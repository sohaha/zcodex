use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
use clap::Parser;
use codex_core::config::Config;
use codex_core::config::find_codex_home;
use codex_core::plugins::PluginMarketplaceUpgradeOutcome;
use codex_core::plugins::PluginsManager;
use codex_core_plugins::marketplace_add::MarketplaceAddRequest;
use codex_core_plugins::marketplace_add::add_marketplace;
use codex_core_plugins::marketplace_remove::MarketplaceRemoveRequest;
use codex_core_plugins::marketplace_remove::remove_marketplace;
use codex_utils_cli::CliConfigOverrides;

#[derive(Debug, Parser)]
#[command(bin_name = "codex plugin marketplace")]
pub struct MarketplaceCli {
    #[clap(flatten)]
    pub config_overrides: CliConfigOverrides,

    #[command(subcommand)]
    subcommand: MarketplaceSubcommand,
}

#[derive(Debug, clap::Subcommand)]
enum MarketplaceSubcommand {
    Add(AddMarketplaceArgs),
    Upgrade(UpgradeMarketplaceArgs),
    Remove(RemoveMarketplaceArgs),
}

#[derive(Debug, Parser)]
#[command(bin_name = "codex plugin marketplace add")]
struct AddMarketplaceArgs {
    /// 市场源地址。支持 `owner/repo[@ref]`、HTTP(S) Git URL、SSH URL，
    /// 以及本地市场源根目录。
    source: String,

    #[arg(long = "ref", value_name = "REF")]
    ref_name: Option<String>,

    #[arg(
        long = "sparse",
        value_name = "PATH",
        action = clap::ArgAction::Append
    )]
    sparse_paths: Vec<String>,
}

#[derive(Debug, Parser)]
#[command(bin_name = "codex plugin marketplace upgrade")]
struct UpgradeMarketplaceArgs {
    marketplace_name: Option<String>,
}

#[derive(Debug, Parser)]
#[command(bin_name = "codex plugin marketplace remove")]
struct RemoveMarketplaceArgs {
    /// 要移除的已配置市场源名称。
    marketplace_name: String,
}

impl MarketplaceCli {
    pub async fn run(self) -> Result<()> {
        let MarketplaceCli {
            config_overrides,
            subcommand,
        } = self;

        let overrides = config_overrides
            .parse_overrides()
            .map_err(anyhow::Error::msg)?;

        match subcommand {
            MarketplaceSubcommand::Add(args) => run_add(args).await?,
            MarketplaceSubcommand::Upgrade(args) => run_upgrade(overrides, args).await?,
            MarketplaceSubcommand::Remove(args) => run_remove(args).await?,
        }

        Ok(())
    }
}

async fn run_add(args: AddMarketplaceArgs) -> Result<()> {
    let AddMarketplaceArgs {
        source,
        ref_name,
        sparse_paths,
    } = args;

    let codex_home = find_codex_home().context("解析 CODEX_HOME 失败")?;
    let outcome = add_marketplace(
        codex_home.to_path_buf(),
        MarketplaceAddRequest {
            source,
            ref_name,
            sparse_paths,
        },
    )
    .await?;

    if outcome.already_added {
        println!(
            "市场源 `{}` 已从 {} 添加。",
            outcome.marketplace_name, outcome.source_display
        );
    } else {
        println!(
            "已从 {} 添加市场源 `{}`。",
            outcome.source_display, outcome.marketplace_name
        );
    }
    println!(
        "已安装的市场源根目录：{}",
        outcome.installed_root.as_path().display()
    );

    Ok(())
}

async fn run_upgrade(
    overrides: Vec<(String, toml::Value)>,
    args: UpgradeMarketplaceArgs,
) -> Result<()> {
    let UpgradeMarketplaceArgs { marketplace_name } = args;
    let config = Config::load_with_cli_overrides(overrides)
        .await
        .context("加载配置失败")?;
    let codex_home = find_codex_home().context("解析 CODEX_HOME 失败")?;
    let manager = PluginsManager::new(codex_home.to_path_buf());
    let outcome = manager
        .upgrade_configured_marketplaces_for_config(&config, marketplace_name.as_deref())
        .map_err(anyhow::Error::msg)?;
    print_upgrade_outcome(&outcome, marketplace_name.as_deref())
}

async fn run_remove(args: RemoveMarketplaceArgs) -> Result<()> {
    let RemoveMarketplaceArgs { marketplace_name } = args;
    let codex_home = find_codex_home().context("解析 CODEX_HOME 失败")?;
    let outcome = remove_marketplace(
        codex_home.to_path_buf(),
        MarketplaceRemoveRequest { marketplace_name },
    )
    .await?;

    println!("已移除市场源 `{}`。", outcome.marketplace_name);
    if let Some(installed_root) = outcome.removed_installed_root {
        println!(
            "已移除的市场源安装目录：{}",
            installed_root.as_path().display()
        );
    }

    Ok(())
}

fn print_upgrade_outcome(
    outcome: &PluginMarketplaceUpgradeOutcome,
    marketplace_name: Option<&str>,
) -> Result<()> {
    for error in &outcome.errors {
        eprintln!(
            "升级市场源 `{}` 失败：{}",
            error.marketplace_name, error.message
        );
    }
    if !outcome.all_succeeded() {
        bail!("发生了 {} 个市场源升级失败。", outcome.errors.len());
    }

    let selection_label = marketplace_name.unwrap_or("所有已配置的 Git 市场源");
    if outcome.selected_marketplaces.is_empty() {
        println!("当前没有可升级的已配置 Git 市场源。");
    } else if outcome.upgraded_roots.is_empty() {
        if marketplace_name.is_some() {
            println!("市场源 `{selection_label}` 已经是最新版本。");
        } else {
            println!("所有已配置的 Git 市场源都已经是最新版本。");
        }
    } else if marketplace_name.is_some() {
        println!("已将市场源 `{selection_label}` 升级到最新配置版本。");
        for root in &outcome.upgraded_roots {
            println!("已安装的市场源根目录：{}", root.display());
        }
    } else {
        println!("已升级 {} 个市场源。", outcome.upgraded_roots.len());
        for root in &outcome.upgraded_roots {
            println!("已安装的市场源根目录：{}", root.display());
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn sparse_paths_parse_before_or_after_source() {
        let sparse_before_source =
            AddMarketplaceArgs::try_parse_from(["add", "--sparse", "plugins/foo", "owner/repo"])
                .unwrap();
        assert_eq!(sparse_before_source.source, "owner/repo");
        assert_eq!(sparse_before_source.sparse_paths, vec!["plugins/foo"]);

        let sparse_after_source =
            AddMarketplaceArgs::try_parse_from(["add", "owner/repo", "--sparse", "plugins/foo"])
                .unwrap();
        assert_eq!(sparse_after_source.source, "owner/repo");
        assert_eq!(sparse_after_source.sparse_paths, vec!["plugins/foo"]);

        let repeated_sparse = AddMarketplaceArgs::try_parse_from([
            "add",
            "--sparse",
            "plugins/foo",
            "--sparse",
            "skills/bar",
            "owner/repo",
        ])
        .unwrap();
        assert_eq!(repeated_sparse.source, "owner/repo");
        assert_eq!(
            repeated_sparse.sparse_paths,
            vec!["plugins/foo", "skills/bar"]
        );
    }

    #[test]
    fn upgrade_subcommand_parses_optional_marketplace_name() {
        let upgrade_all = UpgradeMarketplaceArgs::try_parse_from(["upgrade"]).unwrap();
        assert_eq!(upgrade_all.marketplace_name, None);

        let upgrade_one = UpgradeMarketplaceArgs::try_parse_from(["upgrade", "debug"]).unwrap();
        assert_eq!(upgrade_one.marketplace_name.as_deref(), Some("debug"));
    }

    #[test]
    fn remove_subcommand_parses_marketplace_name() {
        let remove = RemoveMarketplaceArgs::try_parse_from(["remove", "debug"]).unwrap();
        assert_eq!(remove.marketplace_name, "debug");
    }
}
