use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "tqm", version, about = "Token Quota Monitor for API proxy services")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Refresh cache in background (internal use)
    #[command(hide = true)]
    Refresh {
        #[arg(short, long)]
        account: String,
    },
}
