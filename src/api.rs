use crate::config::Account;
use crate::{Error, Result};
use reqwest::blocking::Client;
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, HeaderName};
use serde::{Deserialize, Serialize};
use std::time::Duration;

// ── 内部反序列化结构（直接对应 API JSON）────────────────────────────────────

#[derive(Debug, Deserialize)]
struct ApiResponse<T> {
    success: bool,
    message: Option<String>,
    data: Option<T>,
}

/// GET /api/user/self 的 data 字段
#[derive(Debug, Deserialize)]
struct UserData {
    pub id: u32,           // 用户数字 ID，例: 303
    pub username: String,  // 登录用户名，例: "shenny"
    pub display_name: String, // 展示名，例: "shenny"
    pub email: String,     // 注册邮箱，例: "406205391@qq.com"（可为空）
    pub group: String,     // 所属用户组，例: "new-cc"（影响可用模型）
    pub role: i32,         // 角色级别：1=普通用户, 10=管理员, 100=超级管理员
    pub status: i32,       // 账户状态：1=正常, 2=禁用
    #[serde(default)]
    pub quota: i64,        // 钱包余额（积分单位），例: 328926 ≈ $0.66
    #[serde(default)]
    pub used_quota: i64,   // 钱包累计消耗，例: 35638653 ≈ $71.28
    #[serde(default)]
    pub request_count: u64, // 历史总请求次数，例: 841
}

/// GET /api/subscription/self 的 data 字段
#[derive(Debug, Deserialize)]
struct SubscriptionResponse {
    /// 当前有效订阅列表（status=active）
    subscriptions: Vec<SubscriptionWrapper>,
    /// 所有历史订阅（含已过期）
    #[allow(dead_code)]
    all_subscriptions: Vec<SubscriptionWrapper>,
    /// 计费偏好，例: "subscription_first"（先扣订阅额度再扣钱包）
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
    id: u32,           // 订阅记录 ID，例: 22
    user_id: u32,      // 归属用户 ID，例: 303
    plan_id: u32,      // 套餐 ID，例: 2
    amount_total: i64, // 订阅周期总额度（积分），例: 50000000 = $100
    amount_used: i64,  // 已使用额度（积分），例: 8467579 ≈ $16.94
    start_time: i64,   // 订阅开始时间（Unix 秒），例: 1772596242
    end_time: i64,     // 订阅到期时间（Unix 秒），例: 1775274642
    status: String,    // 订阅状态，例: "active" / "expired"
    source: String,    // 来源，例: "order"（购买）
    #[serde(default)]
    last_reset_time: i64,  // 上次重置时间（Unix 秒），例: 1772596242
    #[serde(default)]
    next_reset_time: i64,  // 下次重置时间（Unix 秒），例: 1772985600
    upgrade_group: String, // 订阅激活后升级到的用户组，例: "new-cc"
    prev_user_group: String, // 订阅前的用户组，例: "default"
    created_at: i64,   // 记录创建时间（Unix 秒），例: 1772596242
    updated_at: i64,   // 记录最后更新时间（Unix 秒），例: 1772605136
}

// ── 公开结构（用于缓存持久化与展示）────────────────────────────────────────

/// 用户基本信息，来自 /api/user/self
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UserInfo {
    pub id: u32,             // 用户数字 ID，例: 303
    pub username: String,    // 登录用户名，例: "shenny"
    pub display_name: String, // 展示名，例: "shenny"
    pub email: String,       // 注册邮箱，例: "406205391@qq.com"
    pub group: String,       // 所属用户组，例: "new-cc"
    pub role: i32,           // 角色级别：1=普通用户, 10=管理员, 100=超级管理员
    pub status: i32,         // 账户状态：1=正常, 2=禁用
    pub wallet_quota: i64,   // 钱包剩余积分，例: 328926 ≈ $0.66（对应 API 的 quota）
    pub used_quota: i64,     // 钱包累计消耗积分，例: 35638653 ≈ $71.28
    pub request_count: u64,  // 历史总请求次数，例: 841
}

/// 订阅信息，来自 /api/subscription/self（取 status=active 的第一条）
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SubscriptionInfo {
    pub id: u32,             // 订阅记录 ID，例: 22
    pub user_id: u32,        // 归属用户 ID，例: 303
    pub plan_id: u32,        // 套餐 ID，例: 2
    pub status: String,      // 订阅状态，例: "active"
    pub source: String,      // 来源，例: "order"
    pub amount_total: i64,   // 周期总额度（积分），例: 50000000 = $100
    pub amount_used: i64,    // 已使用额度（积分），例: 8467579 ≈ $16.94
    pub amount_remain: i64,  // 剩余额度（积分，派生值），例: 41532421 ≈ $83.06
    pub start_time: i64,     // 订阅开始时间（Unix 秒），例: 1772596242
    pub end_time: i64,       // 订阅到期时间（Unix 秒），例: 1775274642
    pub last_reset_time: i64,  // 上次额度重置时间（Unix 秒），例: 1772596242
    pub next_reset_time: i64,  // 下次额度重置时间（Unix 秒），例: 1772985600
    pub upgrade_group: String, // 订阅激活后的用户组，例: "new-cc"
    pub prev_user_group: String, // 订阅前的用户组，例: "default"
    pub created_at: i64,     // 记录创建时间（Unix 秒），例: 1772596242
    pub updated_at: i64,     // 记录最后更新时间（Unix 秒），例: 1772605136
}

impl SubscriptionInfo {
    /// 订阅额度使用百分比，amount_total=0 时返回 None
    pub fn usage_percent(&self) -> Option<f64> {
        if self.amount_total == 0 { return None; }
        Some((self.amount_used as f64 / self.amount_total as f64) * 100.0)
    }

    /// 距订阅到期的剩余天数（已过期返回 0.0）
    pub fn remaining_days(&self) -> f64 {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        ((self.end_time - now) as f64 / 86400.0).max(0.0)
    }

    /// 积分换算为美元：500_000 积分 = $1
    pub fn to_usd(quota: i64) -> f64 {
        quota as f64 / 500_000.0
    }
}

fn subscription_from_response(data: SubscriptionResponse, user_id: u32) -> SubscriptionInfo {
    if let Some(sub) = data
        .subscriptions
        .into_iter()
        .find(|s| s.subscription.status == "active")
    {
        let s = sub.subscription;
        return SubscriptionInfo {
            id: s.id,
            user_id: s.user_id,
            plan_id: s.plan_id,
            status: s.status,
            source: s.source,
            amount_total: s.amount_total,
            amount_used: s.amount_used,
            amount_remain: s.amount_total - s.amount_used,
            start_time: s.start_time,
            end_time: s.end_time,
            last_reset_time: s.last_reset_time,
            next_reset_time: s.next_reset_time,
            upgrade_group: s.upgrade_group,
            prev_user_group: s.prev_user_group,
            created_at: s.created_at,
            updated_at: s.updated_at,
        };
    }

    SubscriptionInfo {
        id: 0,
        user_id,
        plan_id: 0,
        status: "none".to_string(),
        source: "none".to_string(),
        amount_total: 0,
        amount_used: 0,
        amount_remain: 0,
        start_time: 0,
        end_time: 0,
        last_reset_time: 0,
        next_reset_time: 0,
        upgrade_group: "".to_string(),
        prev_user_group: "".to_string(),
        created_at: 0,
        updated_at: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_build_empty_subscription_when_no_active() {
        let data = SubscriptionResponse {
            subscriptions: vec![SubscriptionWrapper {
                subscription: SubscriptionData {
                    id: 1,
                    user_id: 694,
                    plan_id: 2,
                    amount_total: 500000,
                    amount_used: 500000,
                    start_time: 1,
                    end_time: 2,
                    status: "expired".to_string(),
                    source: "order".to_string(),
                    last_reset_time: 1,
                    next_reset_time: 2,
                    upgrade_group: "new-cc".to_string(),
                    prev_user_group: "default".to_string(),
                    created_at: 1,
                    updated_at: 2,
                },
            }],
            all_subscriptions: vec![],
            billing_preference: None,
        };

        let sub = subscription_from_response(data, 694);

        assert_eq!(sub.user_id, 694);
        assert_eq!(sub.status, "none");
        assert_eq!(sub.amount_total, 0);
        assert_eq!(sub.amount_used, 0);
        assert_eq!(sub.amount_remain, 0);
    }
}

// ── HTTP 客户端 ──────────────────────────────────────────────────────────────

pub struct ApiClient {
    client: Client,
    base_url: String, // 例: "https://www.78code.cc"
    token: String,    // Bearer token，来自 config
    user_id: u32,     // New-Api-User header 所需的用户 ID，例: 303
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
            user_id: account.user_id,
        })
    }

    /// New-API 要求同时提供两个认证 header：
    ///   Authorization: Bearer <token>
    ///   New-Api-User: <user_id>
    fn auth_headers(&self) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", self.token)).unwrap(),
        );
        headers.insert(
            HeaderName::from_static("new-api-user"),
            HeaderValue::from(self.user_id),
        );
        headers
    }

    /// 获取当前用户信息，对应 GET /api/user/self
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
            id: data.id,
            username: data.username,
            display_name: data.display_name,
            email: data.email,
            group: data.group,
            role: data.role,
            status: data.status,
            wallet_quota: data.quota,
            used_quota: data.used_quota,
            request_count: data.request_count,
        })
    }

    /// 获取当前用户的活跃订阅，对应 GET /api/subscription/self
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
        Ok(subscription_from_response(data, self.user_id))
    }
}
