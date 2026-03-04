use crate::config::Account;
use crate::{Error, Result};
use reqwest::blocking::Client;
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION};
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Deserialize)]
struct ApiResponse<T> {
    success: bool,
    message: Option<String>,
    data: Option<T>,
}

#[derive(Debug, Deserialize)]
struct UserData {
    #[serde(default)]
    pub quota: i64,
    #[serde(default)]
    pub used_quota: i64,
    #[serde(default)]
    pub request_count: u64,
}

#[derive(Debug, Deserialize)]
struct SubscriptionResponse {
    subscriptions: Vec<SubscriptionWrapper>,
    #[serde(default)]
    #[allow(dead_code)]
    billing_preference: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SubscriptionWrapper {
    subscription: SubscriptionData,
}

#[derive(Debug, Deserialize)]
struct SubscriptionData {
    id: u32,
    plan_id: u32,
    amount_total: i64,
    amount_used: i64,
    start_time: i64,
    end_time: i64,
    status: String,
    #[serde(default)]
    next_reset_time: i64,
}

// Public structs for cache storage
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UserInfo {
    pub wallet_quota: i64,
    pub used_quota: i64,
    pub request_count: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SubscriptionInfo {
    pub id: u32,
    pub plan_id: u32,
    pub status: String,
    pub amount_total: i64,
    pub amount_used: i64,
    pub amount_remain: i64,
    pub start_time: i64,
    pub end_time: i64,
    pub next_reset_time: i64,
}

impl SubscriptionInfo {
    pub fn usage_percent(&self) -> Option<f64> {
        if self.amount_total == 0 { return None; }
        Some((self.amount_used as f64 / self.amount_total as f64) * 100.0)
    }

    pub fn remaining_days(&self) -> f64 {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        ((self.end_time - now) as f64 / 86400.0).max(0.0)
    }

    pub fn to_usd(quota: i64) -> f64 {
        quota as f64 / 500_000.0
    }
}

pub struct ApiClient {
    client: Client,
    base_url: String,
    token: String,
}

impl ApiClient {
    pub fn new(account: &Account) -> Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(10))
            .build()?;
        Ok(Self {
            client,
            base_url: account.base_url.trim_end_matches('/').to_string(),
            token: account.token.clone(),
        })
    }

    fn auth_headers(&self) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", self.token)).unwrap(),
        );
        headers
    }

    pub fn get_user_info(&self) -> Result<UserInfo> {
        let resp: ApiResponse<UserData> = self
            .client
            .get(format!("{}/api/user/self", self.base_url))
            .headers(self.auth_headers())
            .send()?
            .json()?;

        if !resp.success {
            let msg = resp.message.unwrap_or_else(|| "API 返回了错误但没说原因".into());
            return Err(Error::ApiMessage(msg));
        }

        let data = resp.data.ok_or_else(|| Error::ApiMessage("用户数据为空".into()))?;
        Ok(UserInfo {
            wallet_quota: data.quota,
            used_quota: data.used_quota,
            request_count: data.request_count,
        })
    }

    pub fn get_subscription(&self) -> Result<SubscriptionInfo> {
        let resp: ApiResponse<SubscriptionResponse> = self
            .client
            .get(format!("{}/api/subscription/self", self.base_url))
            .headers(self.auth_headers())
            .send()?
            .json()?;

        if !resp.success {
            let msg = resp.message.unwrap_or_else(|| "API 返回了错误但没说原因".into());
            return Err(Error::ApiMessage(msg));
        }

        let data = resp.data.ok_or_else(|| Error::ApiMessage("订阅数据为空".into()))?;

        // Find the active subscription
        let sub = data
            .subscriptions
            .into_iter()
            .find(|s| s.subscription.status == "active")
            .ok_or(Error::NoActiveSubscription)?;

        let s = sub.subscription;
        Ok(SubscriptionInfo {
            id: s.id,
            plan_id: s.plan_id,
            status: s.status,
            amount_total: s.amount_total,
            amount_used: s.amount_used,
            amount_remain: s.amount_total - s.amount_used,
            start_time: s.start_time,
            end_time: s.end_time,
            next_reset_time: s.next_reset_time,
        })
    }
}
