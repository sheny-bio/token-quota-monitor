use crate::api::SubscriptionInfo;
use crate::cache::CacheEntry;

/// Single-line widget output for ccstatusline custom-command
pub fn render_widget(account_name: &str, entry: &CacheEntry) -> String {
    let sub = &entry.subscription;
    let remain_usd = SubscriptionInfo::to_usd(sub.amount_remain);
    let total_usd = SubscriptionInfo::to_usd(sub.amount_total);
    let pct = sub.usage_percent();
    let days = sub.remaining_days();

    // Progress bar (10 chars)
    let filled = ((pct / 10.0).round() as usize).min(10);
    let bar: String = "\u{2593}".repeat(filled) + &"\u{2591}".repeat(10 - filled);

    format!(
        "{} ${:.0}/${:.0} {} {:.0}% {:.0}d",
        account_name, remain_usd, total_usd, bar, pct, days,
    )
}

/// Detailed stats table for `tqm stats`
pub fn render_stats(account_name: &str, entry: &CacheEntry) -> String {
    let sub = &entry.subscription;
    let user = &entry.user;

    let remain_usd = SubscriptionInfo::to_usd(sub.amount_remain);
    let total_usd = SubscriptionInfo::to_usd(sub.amount_total);
    let wallet_usd = SubscriptionInfo::to_usd(user.wallet_quota);
    let pct = sub.usage_percent();
    let days = sub.remaining_days();

    let start = format_timestamp(sub.start_time);
    let end = format_timestamp(sub.end_time);
    let reset = format_timestamp(sub.next_reset_time);

    // Progress bar (20 chars)
    let filled = ((pct / 5.0).round() as usize).min(20);
    let bar: String = "\u{2593}".repeat(filled) + &"\u{2591}".repeat(20 - filled);

    format!(
        "Account:      {}\n\
         Plan:         plan_id={} [{}]\n\
         Subscription: ${:.2} / ${:.2} remaining ({:.2}% used)\n\
         Wallet:       ${:.2}\n\
         Period:       {} ~ {} ({:.1}d left)\n\
         Next Reset:   {}\n\
         Requests:     {}\n\n\
         [{}] {:.2}%",
        account_name,
        sub.plan_id, sub.status,
        remain_usd, total_usd, pct,
        wallet_usd,
        start, end, days,
        reset,
        user.request_count,
        bar, pct,
    )
}

/// JSON output for `tqm stats --json`
pub fn render_json(account_name: &str, entry: &CacheEntry) -> String {
    let sub = &entry.subscription;
    let user = &entry.user;

    serde_json::json!({
        "account": account_name,
        "subscription": {
            "status": sub.status,
            "plan_id": sub.plan_id,
            "total_usd": SubscriptionInfo::to_usd(sub.amount_total),
            "used_usd": SubscriptionInfo::to_usd(sub.amount_used),
            "remain_usd": SubscriptionInfo::to_usd(sub.amount_remain),
            "usage_percent": sub.usage_percent(),
            "remaining_days": sub.remaining_days(),
            "end_time": format_timestamp(sub.end_time),
            "next_reset_time": format_timestamp(sub.next_reset_time),
        },
        "wallet_usd": SubscriptionInfo::to_usd(user.wallet_quota),
        "request_count": user.request_count,
    })
    .to_string()
}

fn format_timestamp(ts: i64) -> String {
    chrono::DateTime::from_timestamp(ts, 0)
        .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
        .unwrap_or_else(|| "N/A".to_string())
}
