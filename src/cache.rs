use crate::api::{SubscriptionInfo, UserInfo};
use crate::config::Config;
use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

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

pub fn cache_path(config: &Config, account_name: &str) -> PathBuf {
    config.cache_dir().join(format!("{}.json", account_name))
}

pub fn load(config: &Config, account_name: &str) -> Result<CacheEntry> {
    let path = cache_path(config, account_name);
    let content = std::fs::read_to_string(&path)
        .map_err(|e| Error::Cache(format!("Read {}: {}", path.display(), e)))?;
    serde_json::from_str(&content)
        .map_err(|e| Error::Cache(format!("Parse {}: {}", path.display(), e)))
}

pub fn save(
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
        ttl_seconds: config.general.cache_ttl_seconds,
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

pub fn clear(config: &Config) -> Result<u32> {
    let dir = config.cache_dir();
    let mut count = 0;
    if dir.exists() {
        for entry in std::fs::read_dir(&dir)? {
            let entry = entry?;
            if entry.path().extension().map_or(false, |e| e == "json") {
                std::fs::remove_file(entry.path())?;
                count += 1;
            }
        }
    }
    Ok(count)
}

pub fn status(config: &Config) -> Result<Vec<(String, bool, i64)>> {
    let dir = config.cache_dir();
    let mut results = Vec::new();
    if dir.exists() {
        for entry in std::fs::read_dir(&dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().map_or(false, |e| e == "json") {
                let name = path.file_stem().unwrap().to_string_lossy().to_string();
                if let Ok(content) = std::fs::read_to_string(&path) {
                    if let Ok(cache_entry) = serde_json::from_str::<CacheEntry>(&content) {
                        let now = SystemTime::now()
                            .duration_since(UNIX_EPOCH)
                            .unwrap()
                            .as_secs() as i64;
                        let age = now - cache_entry.fetched_at;
                        results.push((name, cache_entry.is_valid(), age));
                        continue;
                    }
                }
                results.push((name, false, -1));
            }
        }
    }
    Ok(results)
}
