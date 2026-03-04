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
    UrlMappingNotFound { url: String },
    TokenMissing(String),
    NoActiveSubscription,
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
            Error::UrlMappingNotFound { url } => {
                write!(f, "ANTHROPIC_BASE_URL={url} does not match any url_mapping entry")
            }
            Error::TokenMissing(n) => write!(f, "Account '{n}' has no token configured"),
            Error::NoActiveSubscription => write!(f, "No active subscription found"),
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
    let days = sub.remaining_days();

    match sub.usage_percent() {
        Some(pct) => {
            let filled = ((pct / 10.0).round() as usize).min(10);
            let bar: String = "\u{2593}".repeat(filled) + &"\u{2591}".repeat(10 - filled);
            format!(
                "{} ${:.0}/${:.0} {} {:.0}% {:.0}d",
                account_name, remain_usd, total_usd, bar, pct, days,
            )
        }
        None => format!("{} 订阅额度数据异常(total=0), 瞅瞅咋回事~", account_name),
    }
}

// ── Main ────────────────────────────────────────────────────────────────

fn widget_error(e: &Error) -> String {
    match e {
        Error::ConfigNotFound(_) => "tqm: 配置文件离家出走了~".into(),
        Error::AccountNotFound(n) => format!("tqm: 账户 '{n}' 查无此人~"),
        Error::UrlMappingNotFound { url } => {
            format!("tqm: URL({url}) 在 url_mapping 里找不到归宿~")
        }
        Error::TokenMissing(n) => format!("tqm: 账户 '{n}' 还没配 token 呢~"),
        Error::NoActiveSubscription => "tqm: 没有活跃订阅, 续费了吗?".into(),
        Error::Api(_) => "tqm: API 连不上, 网络开小差了~".into(),
        Error::ApiMessage(m) => format!("tqm: API 说: {m}"),
        Error::Cache(_) => "tqm: 首次加载中...".into(),
        Error::Io(_) => "tqm: 文件系统闹脾气了~".into(),
        Error::Json(_) => "tqm: 缓存数据坏掉了, 重新获取中...".into(),
        Error::Toml(_) => "tqm: 配置文件格式不对, TOML 看不懂~".into(),
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
    let account = config.find_account(account_name)?;
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
