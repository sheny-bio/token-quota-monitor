mod api;
mod cache;
mod cli;
mod config;
mod display;
mod error;

use clap::Parser;
use cli::{CacheAction, Cli, Commands};
use config::Config;
use error::Result;

fn main() {
    let cli = Cli::parse();
    let is_widget_mode = cli.command.is_none();

    let result = run(cli);

    if let Err(e) = result {
        if is_widget_mode {
            // Status line / widget mode: silent fallback
            println!("--");
        } else {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    }
}

fn run(cli: Cli) -> Result<()> {
    match cli.command {
        None => cmd_widget(&cli),
        Some(Commands::Stats { ref account, refresh, json, quiet }) => {
            cmd_stats(&cli, account.as_deref(), refresh, json, quiet)
        }
        Some(Commands::Accounts) => cmd_accounts(&cli),
        Some(Commands::Cache { ref action }) => cmd_cache(&cli, action),
    }
}

/// Default mode: output single-line widget text for ccstatusline
fn cmd_widget(cli: &Cli) -> Result<()> {
    let config = Config::load(cli.config.as_ref())?;
    let account = config.resolve_account(None)?;
    let account_name = account.name.clone();

    match cache::load(&config, &account_name) {
        Ok(entry) => {
            if !entry.is_valid() {
                // Cache expired: spawn background refresh
                spawn_background_refresh(&account_name, cli.config.as_ref());
            }
            // Output stale or fresh data
            println!("{}", display::render_widget(&account_name, &entry));
        }
        Err(_) => {
            // No cache at all: spawn background refresh, output fallback
            spawn_background_refresh(&account_name, cli.config.as_ref());
            println!("{}  --", account_name);
        }
    }
    Ok(())
}

/// `tqm stats`: fetch and display quota info
fn cmd_stats(
    cli: &Cli,
    account_name: Option<&str>,
    refresh: bool,
    json: bool,
    quiet: bool,
) -> Result<()> {
    let config = Config::load(cli.config.as_ref())?;
    let account = config.resolve_account(account_name)?;
    let name = account.name.clone();

    // Try cache first unless --refresh
    if !refresh {
        if let Ok(entry) = cache::load(&config, &name) {
            if entry.is_valid() {
                if !quiet {
                    output_stats(&name, &entry, json);
                }
                return Ok(());
            }
        }
    }

    // Fetch from API
    let client = api::ApiClient::new(account)?;
    let user_info = client.get_user_info()?;
    let subscription = client.get_subscription()?;

    // Save cache
    cache::save(&config, &name, user_info.clone(), subscription.clone())?;

    if !quiet {
        let entry = cache::load(&config, &name)?;
        output_stats(&name, &entry, json);
    }

    Ok(())
}

fn output_stats(name: &str, entry: &cache::CacheEntry, json: bool) {
    if json {
        println!("{}", display::render_json(name, entry));
    } else {
        println!("{}", display::render_stats(name, entry));
    }
}

/// `tqm accounts`: list configured accounts
fn cmd_accounts(cli: &Cli) -> Result<()> {
    let config = Config::load(cli.config.as_ref())?;
    for account in &config.accounts {
        let token_status = if account.token.is_empty() { "no token" } else { "token set" };
        println!(
            "  {} @ {} [{}]",
            account.name, account.base_url, token_status,
        );
    }
    println!("\nDefault: {}", config.general.default_account);
    Ok(())
}

/// `tqm cache clear/status`
fn cmd_cache(cli: &Cli, action: &CacheAction) -> Result<()> {
    let config = Config::load(cli.config.as_ref())?;
    match action {
        CacheAction::Clear => {
            let count = cache::clear(&config)?;
            eprintln!("Cleared {} cache file(s)", count);
        }
        CacheAction::Status => {
            let entries = cache::status(&config)?;
            if entries.is_empty() {
                println!("No cache files found");
            } else {
                for (name, valid, age) in entries {
                    let status = if valid { "valid" } else { "expired" };
                    if age >= 0 {
                        println!("  {} [{}] age={}s", name, status, age);
                    } else {
                        println!("  {} [corrupt]", name);
                    }
                }
            }
        }
    }
    Ok(())
}

/// Fork background process to refresh cache
fn spawn_background_refresh(account_name: &str, config_path: Option<&std::path::PathBuf>) {
    let exe = match std::env::current_exe() {
        Ok(e) => e,
        Err(_) => return,
    };

    let mut cmd = std::process::Command::new(exe);
    cmd.args(["stats", "--refresh", "--quiet", "--account", account_name]);

    if let Some(p) = config_path {
        cmd.args(["--config", &p.to_string_lossy()]);
    }

    cmd.stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());

    let _ = cmd.spawn();
}
