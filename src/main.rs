mod api;
mod cache;
mod cli;
mod config;
mod display;
mod error;

use clap::Parser;
use cli::{Cli, Commands};
use config::Config;
use error::Result;

fn main() {
    let cli = Cli::parse();
    let is_widget_mode = cli.command.is_none();

    let result = run(cli);

    if let Err(e) = result {
        if is_widget_mode {
            println!("--");
        } else {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    }
}

fn run(cli: Cli) -> Result<()> {
    match cli.command {
        None => cmd_widget(),
        Some(Commands::Refresh { ref account }) => cmd_refresh(account),
    }
}

/// Default mode: output single-line widget text for ccstatusline
fn cmd_widget() -> Result<()> {
    let config = Config::load()?;
    let account = config.resolve_account()?;
    let account_name = account.name.clone();

    match cache::load(&config, &account_name) {
        Ok(entry) => {
            if !entry.is_valid() {
                spawn_background_refresh(&account_name);
            }
            println!("{}", display::render_widget(&account_name, &entry));
        }
        Err(_) => {
            spawn_background_refresh(&account_name);
            println!("{}  --", account_name);
        }
    }
    Ok(())
}

/// Hidden: refresh cache for an account
fn cmd_refresh(account_name: &str) -> Result<()> {
    let config = Config::load()?;
    let account = config.resolve_account()?;
    let client = api::ApiClient::new(account)?;
    let user_info = client.get_user_info()?;
    let subscription = client.get_subscription()?;
    cache::save(&config, account_name, user_info, subscription)
}

fn spawn_background_refresh(account_name: &str) {
    let exe = match std::env::current_exe() {
        Ok(e) => e,
        Err(_) => return,
    };

    let mut cmd = std::process::Command::new(exe);
    cmd.args(["refresh", "--account", account_name]);

    cmd.stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());

    let _ = cmd.spawn();
}
