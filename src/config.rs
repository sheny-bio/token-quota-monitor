use crate::api::{SubscriptionInfo, UserInfo};
use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

const CACHE_TTL: u64 = 300;

fn cache_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("~"))
        .join(".cache")
        .join("tqm")
}

// ── Config ──────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct Config {
    pub accounts: Vec<Account>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Account {
    pub name: String,
    pub base_url: String,
    #[serde(default)]
    pub token: String,
    pub user_id: u32,
}

impl Config {
    pub fn config_path() -> PathBuf {
        if let Ok(p) = std::env::var("TQM_CONFIG") {
            PathBuf::from(p)
        } else {
            dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("~"))
                .join(".config")
                .join("tqm")
                .join("config.toml")
        }
    }

    pub fn load() -> Result<Self> {
        let path = Self::config_path();
        if !path.exists() {
            return Err(Error::ConfigNotFound(path.display().to_string()));
        }
        let content = std::fs::read_to_string(&path)?;
        toml::from_str(&content).map_err(Into::into)
    }

    /// 原子写回配置文件（tmp + rename）
    fn save(&self) -> Result<()> {
        let path = Self::config_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = toml::to_string_pretty(self)
            .map_err(|e| Error::ApiMessage(format!("TOML 序列化失败: {e}")))?;
        let tmp = path.with_extension("tmp");
        std::fs::write(&tmp, &content)?;
        std::fs::rename(&tmp, &path)?;
        Ok(())
    }

    pub fn add_account(name: &str, base_url: String, token: String, user_id: u32) -> Result<()> {
        let mut config = match Self::load() {
            Ok(c) => c,
            Err(Error::ConfigNotFound(_)) => Config { accounts: vec![] },
            Err(e) => return Err(e),
        };

        if config.accounts.iter().any(|a| a.name == name) {
            return Err(Error::ApiMessage(format!("账户 '{name}' 已存在")));
        }

        config.accounts.push(Account { name: name.to_string(), base_url, token, user_id });
        config.save()
    }

    pub fn delete_account(name: &str) -> Result<()> {
        let mut config = Self::load()?;
        let before = config.accounts.len();
        config.accounts.retain(|a| a.name != name);
        if config.accounts.len() == before {
            return Err(Error::AccountNotFound(name.to_string()));
        }
        config.save()
    }

    pub fn resolve_account(&self) -> Result<&Account> {
        let env_url = std::env::var("ANTHROPIC_BASE_URL")
            .map_err(|_| Error::BaseUrlNotSet)?;
        let env_url = env_url.trim_end_matches('/');

        let account = self.accounts.iter()
            .find(|a| env_url == a.base_url.trim_end_matches('/'))
            .ok_or_else(|| Error::UrlMappingNotFound { url: env_url.to_string() })?;

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
        (now - self.fetched_at) < CACHE_TTL as i64
    }
}

fn cache_path(account_name: &str) -> PathBuf {
    cache_dir().join(format!("{}.json", account_name))
}

pub fn load_cache(account_name: &str) -> Result<CacheEntry> {
    let path = cache_path(account_name);
    let content = std::fs::read_to_string(&path)
        .map_err(|e| Error::Cache(format!("Read {}: {}", path.display(), e)))?;
    serde_json::from_str(&content)
        .map_err(|e| Error::Cache(format!("Parse {}: {}", path.display(), e)))
}

pub fn save_cache(
    account_name: &str,
    user: UserInfo,
    subscription: SubscriptionInfo,
) -> Result<()> {
    let dir = cache_dir();
    std::fs::create_dir_all(&dir)?;

    let entry = CacheEntry {
        fetched_at: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64,
        account_name: account_name.to_string(),
        user,
        subscription,
    };

    let content = serde_json::to_string_pretty(&entry)?;
    let path = cache_path(account_name);

    let tmp_path = path.with_extension("tmp");
    std::fs::write(&tmp_path, &content)?;
    std::fs::rename(&tmp_path, &path)?;

    Ok(())
}
