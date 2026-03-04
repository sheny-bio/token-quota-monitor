use crate::api::{SubscriptionInfo, UserInfo};
use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

// ── Config ──────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct Config {
    pub general: General,
    pub accounts: Vec<Account>,
    #[serde(default)]
    pub url_mapping: std::collections::HashMap<String, String>,
}

#[derive(Debug, Deserialize)]
pub struct General {
    #[serde(default = "default_refresh_interval")]
    pub refresh_interval: u64,
    #[serde(default = "default_cache_dir")]
    pub cache_dir: String,
    pub default_account: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Account {
    pub name: String,
    pub base_url: String,
    #[serde(default)]
    pub token: String,
}

fn default_refresh_interval() -> u64 { 300 }
fn default_cache_dir() -> String { "~/.cache/tqm".to_string() }

impl Config {
    pub fn load() -> Result<Self> {
        let path = if let Ok(p) = std::env::var("TQM_CONFIG") {
            PathBuf::from(p)
        } else {
            dirs::config_dir()
                .unwrap_or_else(|| PathBuf::from("~/.config"))
                .join("tqm")
                .join("config.toml")
        };

        if !path.exists() {
            return Err(Error::ConfigNotFound(path.display().to_string()));
        }

        let content = std::fs::read_to_string(&path)?;
        let config: Config = toml::from_str(&content)?;
        Ok(config)
    }

    pub fn cache_dir(&self) -> PathBuf {
        let dir = &self.general.cache_dir;
        if dir.starts_with("~/") {
            if let Some(home) = dirs::home_dir() {
                return home.join(&dir[2..]);
            }
        }
        PathBuf::from(dir)
    }

    pub fn resolve_account(&self) -> Result<&Account> {
        // 优先通过 ANTHROPIC_BASE_URL 环境变量 + url_mapping 匹配账户
        if let Ok(env_url) = std::env::var("ANTHROPIC_BASE_URL") {
            for (domain, account_name) in &self.url_mapping {
                if env_url.contains(domain) {
                    let account = self.find_account(account_name)?;
                    return Self::validate_token(account);
                }
            }
            // URL 设了但 mapping 匹配不上 → 不能静默 fallback
            if !self.url_mapping.is_empty() {
                return Err(Error::UrlMappingNotFound { url: env_url });
            }
        }
        // 没设 ANTHROPIC_BASE_URL 或 url_mapping 为空 → 用 default_account
        let account = self.find_account(&self.general.default_account)?;
        Self::validate_token(account)
    }

    fn validate_token(account: &Account) -> Result<&Account> {
        if account.token.is_empty() {
            return Err(Error::TokenMissing(account.name.clone()));
        }
        Ok(account)
    }

    pub fn find_account(&self, name: &str) -> Result<&Account> {
        self.accounts.iter().find(|a| a.name == name)
            .ok_or_else(|| Error::AccountNotFound(name.to_string()))
    }
}

// ── Cache ───────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct CacheEntry {
    pub fetched_at: i64,
    pub ttl_seconds: u64,
    pub account_name: String,
    pub user: UserInfo,
    pub subscription: SubscriptionInfo,
}

impl CacheEntry {
    pub fn is_valid(&self) -> bool {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        (now - self.fetched_at) < self.ttl_seconds as i64
    }
}

fn cache_path(config: &Config, account_name: &str) -> PathBuf {
    config.cache_dir().join(format!("{}.json", account_name))
}

pub fn load_cache(config: &Config, account_name: &str) -> Result<CacheEntry> {
    let path = cache_path(config, account_name);
    let content = std::fs::read_to_string(&path)
        .map_err(|e| Error::Cache(format!("Read {}: {}", path.display(), e)))?;
    serde_json::from_str(&content)
        .map_err(|e| Error::Cache(format!("Parse {}: {}", path.display(), e)))
}

pub fn save_cache(
    config: &Config,
    account_name: &str,
    user: UserInfo,
    subscription: SubscriptionInfo,
) -> Result<()> {
    let dir = config.cache_dir();
    std::fs::create_dir_all(&dir)?;

    let entry = CacheEntry {
        fetched_at: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64,
        ttl_seconds: config.general.refresh_interval,
        account_name: account_name.to_string(),
        user,
        subscription,
    };

    let content = serde_json::to_string_pretty(&entry)?;
    let path = cache_path(config, account_name);

    // Atomic write
    let tmp_path = path.with_extension("tmp");
    std::fs::write(&tmp_path, &content)?;
    std::fs::rename(&tmp_path, &path)?;

    Ok(())
}
