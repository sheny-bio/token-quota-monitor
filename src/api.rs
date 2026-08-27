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

// ── sub2api 内部反序列化结构（GET /v1/usage，裸 JSON，无 success 包装）──────

/// GET /v1/usage 的完整响应（只声明用得到的字段，其余 serde 自动忽略）
#[derive(Debug, Deserialize)]
struct UsageResponse {
    /// 逐日用量，完整历史（实测求和等于 usage.total）
    daily_usage: Vec<DailyUsage>,
    usage: UsageTotals,
}

#[derive(Debug, Deserialize)]
struct DailyUsage {
    date: String,      // 形如 "2026-08-27"
    actual_cost: f64,  // 当日实际计费，例: 97.2134154
}

#[derive(Debug, Deserialize)]
struct UsageTotals {
    today: UsageBucket,
    total: UsageBucket,
}

#[derive(Debug, Deserialize)]
struct UsageBucket {
    actual_cost: f64,
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
    #[allow(dead_code)]
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

/// 花费信息，来自 /v1/usage。sub2api 是后付费（mode="unrestricted"），没有余额概念，
/// 所以这里记的是花了多少，而不是还剩多少。
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CostInfo {
    pub month: String,     // 统计月份，例: "2026-08"
    pub today_cost: f64,   // 今日花费（USD），例: 97.2134154
    pub month_cost: f64,   // 当月花费（USD），例: 561.7421451
    pub total_cost: f64,   // 历史总花费（USD），例: 561.7421451
}

/// Unix 秒 → "YYYY-MM"（UTC）。civil_from_days 算法，不为一个月份引日期库。
fn utc_month(secs: i64) -> String {
    let days = secs.div_euclid(86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
    format!("{:04}-{:02}", y, m)
}

fn cost_from_usage(resp: UsageResponse, now_secs: i64) -> CostInfo {
    // 取「本地 UTC 月份」与「服务端最大日期的月份」中较大的那个。
    // "YYYY-MM" 字符串的字典序即时间序，所以 max 就是取较晚的月份。
    //   - 只看服务端最大日期：月初闲置几天就会把上月总额当本月显示（本次要修的 bug）
    //   - 只看 UTC：服务端时区领先 UTC 时，跨月头几小时会把新月份的记录过滤掉
    // 取 max 时两种情形都能兜住。
    // ponytail: 残留失效窗口 = 服务端已跨月但 UTC 未跨月、且该窗口内有用量（UTC+8 下约 8 小时）。
    // 要彻底消除得知道服务端时区，这个 API 不提供。
    let latest = resp
        .daily_usage
        .iter()
        .map(|d| d.date.as_str())
        .max()
        .and_then(|d| d.get(..7))
        .unwrap_or("");

    let month = std::cmp::max(utc_month(now_secs), latest.to_string());

    let month_cost = resp
        .daily_usage
        .iter()
        .filter(|d| d.date.starts_with(&month))
        .map(|d| d.actual_cost)
        .sum();

    CostInfo {
        month,
        today_cost: resp.usage.today.actual_cost,
        month_cost,
        total_cost: resp.usage.total.actual_cost,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn usage_fixture() -> UsageResponse {
        // 实测数据（8 月合计 561.7421451），额外补一条 7 月记录，
        // 确保月份过滤真的过滤掉了东西，而不是恰好等于 total。
        let daily = [
            ("2026-07-31", 42.5),
            ("2026-08-21", 13.27866555),
            ("2026-08-24", 110.351066),
            ("2026-08-25", 145.64933515),
            ("2026-08-26", 195.249663),
            ("2026-08-27", 97.2134154),
        ];
        UsageResponse {
            daily_usage: daily
                .iter()
                .map(|(d, c)| DailyUsage { date: d.to_string(), actual_cost: *c })
                .collect(),
            usage: UsageTotals {
                today: UsageBucket { actual_cost: 97.2134154 },
                total: UsageBucket { actual_cost: 604.2421451 },
            },
        }
    }

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-6
    }

    /// 2026-08-27T00:00:00Z
    const AUG_27: i64 = 1_787_788_800;
    /// 2026-09-04T00:00:00Z —— 跨月后闲置了几天，一条新记录都没有
    const SEP_04: i64 = 1_788_480_000;

    #[test]
    fn should_convert_unix_seconds_to_utc_month() {
        assert_eq!(utc_month(0), "1970-01");
        assert_eq!(utc_month(AUG_27), "2026-08");
        assert_eq!(utc_month(SEP_04), "2026-09");
        // 闰年 2 月末与月初边界
        assert_eq!(utc_month(1_709_164_800), "2024-02"); // 2024-02-29T00:00:00Z
        assert_eq!(utc_month(1_709_251_200), "2024-03"); // 2024-03-01T00:00:00Z
    }

    #[test]
    fn should_aggregate_cost_for_current_month_only() {
        let cost = cost_from_usage(usage_fixture(), AUG_27);

        assert_eq!(cost.month, "2026-08");
        assert!(close(cost.today_cost, 97.2134154), "today = {}", cost.today_cost);
        assert!(close(cost.month_cost, 561.7421451), "month = {}", cost.month_cost);
        assert!(close(cost.total_cost, 604.2421451), "total = {}", cost.total_cost);
        // 7 月那条必须被排除掉
        assert!(cost.month_cost < cost.total_cost);
    }

    /// 回归：跨月后闲置几天，最大日期仍停在上月。此时必须显示新月份的 0，
    /// 而不是把上月总额当本月花费。
    #[test]
    fn should_report_zero_after_month_rollover_with_no_usage() {
        let cost = cost_from_usage(usage_fixture(), SEP_04);

        assert_eq!(cost.month, "2026-09");
        assert!(close(cost.month_cost, 0.0), "month = {}", cost.month_cost);
    }

    /// 服务端时区领先 UTC：UTC 还在 8 月，但服务端已记了 9 月的用量，应认 9 月。
    #[test]
    fn should_prefer_server_month_when_it_is_ahead_of_utc() {
        let mut resp = usage_fixture();
        resp.daily_usage.push(DailyUsage {
            date: "2026-09-01".to_string(),
            actual_cost: 7.5,
        });

        let cost = cost_from_usage(resp, AUG_27);

        assert_eq!(cost.month, "2026-09");
        assert!(close(cost.month_cost, 7.5), "month = {}", cost.month_cost);
    }

    #[test]
    fn should_not_panic_on_empty_daily_usage() {
        let resp = UsageResponse {
            daily_usage: vec![],
            usage: UsageTotals {
                today: UsageBucket { actual_cost: 0.0 },
                total: UsageBucket { actual_cost: 0.0 },
            },
        };

        let cost = cost_from_usage(resp, AUG_27);

        assert_eq!(cost.month, "2026-08");
        assert!(close(cost.month_cost, 0.0));
    }

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
    base_url: String,      // 例: "https://www.78code.cc"
    token: String,         // Bearer token，来自 config
    user_id: Option<u32>,  // New-Api-User header 所需的用户 ID（sub2api 不需要，为 None）
}


impl ApiClient {
    pub fn new(account: &Account) -> Result<Self> {
        Self::with_credentials(&account.base_url, &account.token, account.user_id)
    }

    pub fn with_credentials(base_url: &str, token: &str, user_id: Option<u32>) -> Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(10))
            .build()?;
        Ok(Self {
            client,
            base_url: base_url.trim_end_matches('/').to_string(),
            token: token.to_string(),
            user_id,
        })
    }

    /// New-API 要求同时提供两个认证 header：
    ///   Authorization: Bearer <token>
    ///   New-Api-User: <user_id>
    /// sub2api 只认第一个，此时 user_id 为 None，不带第二个 header。
    fn auth_headers(&self) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", self.token)).unwrap(),
        );
        if let Some(id) = self.user_id {
            headers.insert(
                HeaderName::from_static("new-api-user"),
                HeaderValue::from(id),
            );
        }
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
        Ok(subscription_from_response(data, self.user_id.unwrap_or(0)))
    }

    /// 获取花费统计，对应 GET /v1/usage（sub2api）
    pub fn get_usage(&self) -> Result<CostInfo> {
        let resp: UsageResponse = self
            .client
            .get(format!("{}/v1/usage", self.base_url))
            .headers(self.auth_headers())
            .send()?
            .error_for_status()?
            .json()?;

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        Ok(cost_from_usage(resp, now))
    }
}
