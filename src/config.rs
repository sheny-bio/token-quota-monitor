use crate::api::{SubscriptionInfo, UserInfo};
use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

const CACHE_SOFT_TTL: u64 = 240;
const CACHE_HARD_TTL: u64 = 600;

fn cache_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("~"))
        .join(".cache")
        .join("tqm")
}

// ── Account ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct Account {
    pub name: String,
    pub base_url: String,
    pub token: String,
    pub user_id: u32,
}

impl Account {
    pub fn from_env() -> Result<Self> {
        let base_url = std::env::var("ANTHROPIC_BASE_URL")
            .map_err(|_| Error::BaseUrlNotSet)?;

        let token = std::env::var("TQM_TOKEN")
            .map_err(|_| Error::TokenMissing("TQM_TOKEN".into()))?;
        if token.is_empty() {
            return Err(Error::TokenMissing("TQM_TOKEN".into()));
        }

        let user_id: u32 = std::env::var("TQM_USER_ID")
            .map_err(|_| Error::ApiMessage("TQM_USER_ID not set".into()))?
            .parse()
            .map_err(|_| Error::ApiMessage("TQM_USER_ID must be a positive integer".into()))?;

        let name = std::env::var("TQM_ACCOUNT_NAME")
            .unwrap_or_else(|_| derive_name(&base_url));

        Ok(Account { name, base_url, token, user_id })
    }
}

fn derive_name(url: &str) -> String {
    url.trim_end_matches('/')
        .splitn(2, "://").nth(1).unwrap_or(url)
        .split('/').next().unwrap_or(url)
        .to_string()
}

// ── Cache ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheFreshness {
    Fresh,
    Stale,
    Expired,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CacheEntry {
    pub fetched_at: i64,
    pub account_name: String,
    pub user: UserInfo,
    pub subscription: SubscriptionInfo,
}

impl CacheEntry {
    fn age(&self) -> u64 {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        (now - self.fetched_at).max(0) as u64
    }

    pub fn freshness(&self) -> CacheFreshness {
        let age = self.age();
        if age < CACHE_SOFT_TTL {
            CacheFreshness::Fresh
        } else if age < CACHE_HARD_TTL {
            CacheFreshness::Stale
        } else {
            CacheFreshness::Expired
        }
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
