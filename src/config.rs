use crate::api::{CostInfo, SubscriptionInfo, UserInfo};
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

/// 后端接口形态。由 TQM_PROVIDER 显式声明，不设即 NewApi。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Provider {
    /// new-api：/api/user/self + /api/subscription/self，双认证 header
    NewApi,
    /// sub2api：/v1/usage，只认 Bearer
    Sub2Api,
}

impl Provider {
    fn from_env() -> Result<Self> {
        match std::env::var("TQM_PROVIDER") {
            Err(_) => Ok(Provider::NewApi),
            Ok(v) => match v.trim().to_ascii_lowercase().as_str() {
                "" | "newapi" => Ok(Provider::NewApi),
                "sub2api" => Ok(Provider::Sub2Api),
                other => Err(Error::ApiMessage(format!(
                    "TQM_PROVIDER 只能是 newapi 或 sub2api，收到: {other}"
                ))),
            },
        }
    }
}

#[derive(Debug, Clone)]
pub struct Account {
    pub name: String,
    pub base_url: String,
    pub token: String,
    /// 仅 NewApi 需要；Sub2Api 为 None
    pub user_id: Option<u32>,
    pub provider: Provider,
}

impl Account {
    pub fn from_env() -> Result<Self> {
        let base_url = std::env::var("ANTHROPIC_BASE_URL")
            .map_err(|_| Error::BaseUrlNotSet)?;

        let provider = Provider::from_env()?;

        // 按 provider 严格选取来源，不做链式回退：new-api 的管理接口要独立 token，
        // 回退会把它当 Bearer 发给另一家 host（切 profile 时残留的 TQM_TOKEN 尤其危险）。
        // trim 是必需的：带尾随换行的 key 会让 HeaderValue::from_str 直接 panic。
        let token_var = match provider {
            Provider::NewApi => "TQM_TOKEN",
            Provider::Sub2Api => "ANTHROPIC_API_KEY",
        };
        let token = std::env::var(token_var)
            .ok()
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty())
            .ok_or_else(|| Error::TokenMissing(token_var.into()))?;

        let user_id = match provider {
            Provider::NewApi => Some(
                std::env::var("TQM_USER_ID")
                    .map_err(|_| Error::ApiMessage("TQM_USER_ID not set".into()))?
                    .parse()
                    .map_err(|_| {
                        Error::ApiMessage("TQM_USER_ID must be a positive integer".into())
                    })?,
            ),
            Provider::Sub2Api => None,
        };

        let name = std::env::var("TQM_ACCOUNT_NAME")
            .unwrap_or_else(|_| derive_name(&base_url));

        Ok(Account { name, base_url, token, user_id, provider })
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

/// 缓存里的业务数据，形态随 provider 不同。
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum Snapshot {
    NewApi {
        user: UserInfo,
        subscription: SubscriptionInfo,
    },
    Sub2Api {
        cost: CostInfo,
    },
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CacheEntry {
    pub fetched_at: i64,
    pub account_name: String,
    pub snapshot: Snapshot,
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

pub fn save_cache(account_name: &str, snapshot: Snapshot) -> Result<()> {
    let dir = cache_dir();
    std::fs::create_dir_all(&dir)?;

    let entry = CacheEntry {
        fetched_at: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64,
        account_name: account_name.to_string(),
        snapshot,
    };

    let content = serde_json::to_string_pretty(&entry)?;
    let path = cache_path(account_name);
    let tmp_path = path.with_extension("tmp");
    std::fs::write(&tmp_path, &content)?;
    std::fs::rename(&tmp_path, &path)?;
    Ok(())
}
