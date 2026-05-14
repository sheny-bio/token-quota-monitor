mod api;
mod config;

use clap::{Parser, Subcommand};
use config::Account;
use std::fmt;

// ── Error ────────────────────────────────────────────────────────────────

#[derive(Debug)]
pub enum Error {
    BaseUrlNotSet,
    TokenMissing(String),
    NoActiveSubscription,
    Api(reqwest::Error),
    ApiMessage(String),
    Cache(String),
    Io(std::io::Error),
    Json(serde_json::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::BaseUrlNotSet => write!(f, "ANTHROPIC_BASE_URL is not set"),
            Error::TokenMissing(n) => write!(f, "Token missing: {n}"),
            Error::NoActiveSubscription => write!(f, "No active subscription found"),
            Error::Api(e) => write!(f, "API error: {e}"),
            Error::ApiMessage(m) => write!(f, "API error: {m}"),
            Error::Cache(m) => write!(f, "Cache error: {m}"),
            Error::Io(e) => write!(f, "IO error: {e}"),
            Error::Json(e) => write!(f, "JSON error: {e}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<reqwest::Error> for Error {
    fn from(e: reqwest::Error) -> Self { Error::Api(e) }
}
impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self { Error::Io(e) }
}
impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self { Error::Json(e) }
}

pub type Result<T> = std::result::Result<T, Error>;

// ── CLI ──────────────────────────────────────────────────────────────────

#[derive(Parser)]
#[command(name = "tqm", version, about = "Token Quota Monitor for API proxy services")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Refresh cache in background (internal use)
    #[command(hide = true)]
    Refresh {
        #[arg(short, long)]
        account: String,
    },
}

// ── Display ──────────────────────────────────────────────────────────────

fn render_widget(account_name: &str, entry: &config::CacheEntry) -> String {
    let wallet_usd = api::SubscriptionInfo::to_usd(entry.user.wallet_quota);
    match entry.subscription.usage_percent() {
        Some(pct) => format!("{} ${:.2} {:.0}%", account_name, wallet_usd, 100.0 - pct),
        None => format!("{} ${:.2}", account_name, wallet_usd),
    }
}

fn widget_error(e: &Error) -> String {
    match e {
        Error::BaseUrlNotSet => "tqm:未设URL".into(),
        Error::TokenMissing(_) => "tqm:无token".into(),
        Error::NoActiveSubscription => "tqm:无订阅".into(),
        Error::Api(_) => "tqm:API错误".into(),
        Error::ApiMessage(m) => format!("tqm:{m}"),
        Error::Cache(_) => "tqm:加载中".into(),
        Error::Io(_) => "tqm:IO错误".into(),
        Error::Json(_) => "tqm:缓存损坏".into(),
    }
}

// ── Main ─────────────────────────────────────────────────────────────────

fn main() {
    let cli = Cli::parse();
    let is_widget_mode = cli.command.is_none();

    let result = run(cli);

    if let Err(e) = result {
        if is_widget_mode {
            println!("{}", widget_error(&e));
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

fn cmd_widget() -> Result<()> {
    let account = Account::from_env()?;
    let account_name = account.name.clone();

    match config::load_cache(&account_name) {
        Ok(entry) => {
            use config::CacheFreshness::*;
            match entry.freshness() {
                Fresh => println!("{}", render_widget(&account_name, &entry)),
                Stale => {
                    println!("{}", render_widget(&account_name, &entry));
                    spawn_background_refresh(&account_name);
                }
                Expired => {
                    spawn_background_refresh(&account_name);
                    println!("{} 加载中...", account_name);
                }
            }
        }
        Err(_) => {
            spawn_background_refresh(&account_name);
            println!("{} 加载中...", account_name);
        }
    }
    Ok(())
}

fn cmd_refresh(account_name: &str) -> Result<()> {
    let account = Account::from_env()?;
    let client = api::ApiClient::new(&account)?;
    let user_info = client.get_user_info()?;
    let subscription = client.get_subscription()?;
    config::save_cache(account_name, user_info, subscription)
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
