use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "tqm", version, about = "Token Quota Monitor for API proxy services")]
pub struct Cli {
    /// Path to config file
    #[arg(short, long, global = true)]
    pub config: Option<PathBuf>,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Query and display quota/subscription info
    Stats {
        /// Account name (overrides ANTHROPIC_BASE_URL routing)
        #[arg(short, long)]
        account: Option<String>,

        /// Force refresh cache
        #[arg(short, long)]
        refresh: bool,

        /// Output as JSON
        #[arg(long)]
        json: bool,

        /// Suppress output (for background refresh)
        #[arg(long)]
        quiet: bool,
    },
    /// List configured accounts
    Accounts,
    /// Cache management
    Cache {
        #[command(subcommand)]
        action: CacheAction,
    },
}

#[derive(Subcommand)]
pub enum CacheAction {
    /// Clear all cached data
    Clear,
    /// Show cache status
    Status,
}
