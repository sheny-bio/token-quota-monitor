mod api;
mod config;

use clap::{Parser, Subcommand};
use config::Config;
use std::fmt;

// ── Error ───────────────────────────────────────────────────────────────

#[derive(Debug)]
pub enum Error {
    ConfigNotFound(String),
    AccountNotFound(String),
    Api(reqwest::Error),
    ApiMessage(String),
    Cache(String),
    Io(std::io::Error),
    Json(serde_json::Error),
    Toml(toml::de::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::ConfigNotFound(p) => write!(f, "Config not found: {p}"),
            Error::AccountNotFound(n) => write!(f, "Account not found: {n}"),
            Error::Api(e) => write!(f, "API error: {e}"),
            Error::ApiMessage(m) => write!(f, "API error: {m}"),
            Error::Cache(m) => write!(f, "Cache error: {m}"),
            Error::Io(e) => write!(f, "IO error: {e}"),
            Error::Json(e) => write!(f, "JSON error: {e}"),
            Error::Toml(e) => write!(f, "TOML parse error: {e}"),
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
impl From<toml::de::Error> for Error {
    fn from(e: toml::de::Error) -> Self { Error::Toml(e) }
}

pub type Result<T> = std::result::Result<T, Error>;

// ── CLI ─────────────────────────────────────────────────────────────────

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

// ── Display ─────────────────────────────────────────────────────────────

fn render_widget(account_name: &str, entry: &config::CacheEntry) -> String {
    let sub = &entry.subscription;
    let remain_usd = api::SubscriptionInfo::to_usd(sub.amount_remain);
    let total_usd = api::SubscriptionInfo::to_usd(sub.amount_total);
    let pct = sub.usage_percent();
    let days = sub.remaining_days();

    let filled = ((pct / 10.0).round() as usize).min(10);
    let bar: String = "\u{2593}".repeat(filled) + &"\u{2591}".repeat(10 - filled);

    format!(
        "{} ${:.0}/${:.0} {} {:.0}% {:.0}d",
        account_name, remain_usd, total_usd, bar, pct, days,
    )
}

// ── Main ────────────────────────────────────────────────────────────────

fn widget_error(e: &Error) -> String {
    match e {
        Error::ConfigNotFound(_) => "tqm: 未找到配置文件".into(),
        Error::AccountNotFound(n) => format!("tqm: 账户 {n} 未配置"),
        Error::Api(_) => "tqm: 网络请求失败".into(),
        Error::ApiMessage(m) => format!("tqm: {m}"),
        Error::Cache(_) => "tqm: 加载中...".into(),
        Error::Io(_) => "tqm: 文件读写错误".into(),
        Error::Json(_) => "tqm: 数据解析错误".into(),
        Error::Toml(_) => "tqm: 配置格式错误".into(),
    }
}

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
    let config = Config::load()?;
    let account = config.resolve_account()?;
    let account_name = account.name.clone();

    match config::load_cache(&config, &account_name) {
        Ok(entry) => {
            if !entry.is_valid() {
                spawn_background_refresh(&account_name);
            }
            println!("{}", render_widget(&account_name, &entry));
        }
        Err(_) => {
            spawn_background_refresh(&account_name);
            println!("{} 加载中...", account_name);
        }
    }
    Ok(())
}

fn cmd_refresh(account_name: &str) -> Result<()> {
    let config = Config::load()?;
    let account = config.resolve_account()?;
    let client = api::ApiClient::new(account)?;
    let user_info = client.get_user_info()?;
    let subscription = client.get_subscription()?;
    config::save_cache(&config, account_name, user_info, subscription)
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
