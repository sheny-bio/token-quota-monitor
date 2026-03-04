use crate::error::{Error, Result};
use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug, Deserialize)]
pub struct Config {
    pub general: General,
    pub accounts: Vec<Account>,
}

#[derive(Debug, Deserialize)]
pub struct General {
    #[serde(default = "default_cache_ttl")]
    pub cache_ttl_seconds: u64,
    #[serde(default = "default_cache_dir")]
    pub cache_dir: String,
    pub default_account: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Account {
    pub name: String,
    pub base_url: String,
    pub token: String,
}

fn default_cache_ttl() -> u64 { 300 }
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
        let name = &self.general.default_account;
        self.accounts.iter().find(|a| a.name == *name)
            .ok_or_else(|| Error::AccountNotFound(name.clone()))
    }
}
