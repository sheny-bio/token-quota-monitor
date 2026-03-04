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
