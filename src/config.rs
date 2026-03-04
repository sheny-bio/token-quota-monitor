use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize)]
pub struct Config {
    pub general: General,
    pub accounts: Vec<Account>,
    #[serde(default)]
    pub routing: HashMap<String, String>,

    #[serde(skip)]
    pub path: PathBuf,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct General {
    #[serde(default = "default_cache_ttl")]
    pub cache_ttl_seconds: u64,
    #[serde(default = "default_cache_dir")]
    pub cache_dir: String,
    pub default_account: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Account {
    pub name: String,
    pub base_url: String,
    pub username: String,
    #[serde(default)]
    pub session: String,
    #[serde(default)]
    pub user_id: u32,
}

fn default_cache_ttl() -> u64 { 300 }
fn default_cache_dir() -> String { "~/.cache/tqm".to_string() }

impl Config {
    pub fn load(custom_path: Option<&PathBuf>) -> Result<Self> {
        let path = if let Some(p) = custom_path {
            p.clone()
        } else if let Ok(p) = std::env::var("TQM_CONFIG") {
            PathBuf::from(p)
        } else {
            Self::default_path()
        };

        if !path.exists() {
            return Err(Error::ConfigNotFound(path.display().to_string()));
        }

        let content = std::fs::read_to_string(&path)?;
        let mut config: Config = toml::from_str(&content)?;
        config.path = path;
        Ok(config)
    }

    pub fn save(&self) -> Result<()> {
        let dir = self.path.parent().unwrap();
        std::fs::create_dir_all(dir)?;

        let content = toml::to_string_pretty(self)?;

        // Atomic write: write to temp file then rename
        let tmp_path = self.path.with_extension("tmp");
        std::fs::write(&tmp_path, &content)?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&tmp_path, std::fs::Permissions::from_mode(0o600))?;
        }

        std::fs::rename(&tmp_path, &self.path)?;
        Ok(())
    }

    pub fn default_path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("~/.config"))
            .join("tqm")
            .join("config.toml")
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

    pub fn find_account(&self, name: &str) -> Option<&Account> {
        self.accounts.iter().find(|a| a.name == name)
    }

    pub fn find_account_mut(&mut self, name: &str) -> Result<&mut Account> {
        self.accounts
            .iter_mut()
            .find(|a| a.name == name)
            .ok_or_else(|| Error::AccountNotFound(name.to_string()))
    }

    /// Resolve account based on: --account flag > ANTHROPIC_BASE_URL > default_account
    pub fn resolve_account(&self, override_name: Option<&str>) -> Result<&Account> {
        // 1. CLI --account flag
        if let Some(name) = override_name {
            return self.find_account(name)
                .ok_or_else(|| Error::AccountNotFound(name.to_string()));
        }

        // 2. ANTHROPIC_BASE_URL env
        if let Ok(base_url) = std::env::var("ANTHROPIC_BASE_URL") {
            if let Ok(parsed) = url::Url::parse(&base_url) {
                if let Some(host) = parsed.host_str() {
                    if let Some(account_name) = self.routing.get(host) {
                        return self.find_account(account_name)
                            .ok_or_else(|| Error::AccountNotFound(account_name.clone()));
                    }
                }
            }
        }

        // 3. Fallback: default_account
        self.find_account(&self.general.default_account)
            .ok_or(Error::NoDefaultAccount)
    }
}
