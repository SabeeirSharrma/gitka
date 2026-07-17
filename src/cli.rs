#![allow(dead_code)]

use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "gitka",
    about = "A Ventoy-inspired tool for compressed git repo backups on physical media",
    version,
    long_about = "Gitka creates compressed, physical-media local copies of your GitHub/GitFlare repos.\n\n\
                   Features:\n\
                   • Aggressive but safe compression (zstd with auto-tier selection)\n\
                   • Offline commit capability via temporary extraction\n\
                   • LAN-based sharing via bundled GitFlare\n\
                   • Recovery records for corruption protection"
)]
pub struct Cli {
    /// Path to config file (default: <target>/.gitka/gitka.toml)
    #[arg(short, long, global = true)]
    pub config: Option<PathBuf>,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Launch the GUI (future feature)
    Gui,

    /// Initialize a new Gitka backup
    Init {
        /// Source type: github or gitflare
        #[arg(short, long, default_value = "github")]
        source: String,

        /// Target USB/backup drive path
        #[arg(short, long)]
        target: PathBuf,

        /// GitHub username or organization (for GitHub source)
        #[arg(long)]
        username: Option<String>,

        /// Authentication token (GitHub PAT or GitFlare token)
        #[arg(long)]
        token: Option<String>,

        /// GitFlare instance URL (for GitFlare source)
        #[arg(long)]
        gitflare_url: Option<String>,

        /// Run interactive setup wizard
        #[arg(short, long)]
        interactive: bool,
    },

    /// Re-scan sources and target, show size report + budget check
    Scan,

    /// Sync repos (fetch/compare/push/pull/merge loop)
    Sync {
        /// Sync specific repos only (default: all)
        #[arg(short, long)]
        repos: Option<Vec<String>>,
    },

    /// Show per-repo status (ahead/behind/conflict, last synced)
    Status {
        /// Show status for specific repos only
        #[arg(short, long)]
        repos: Option<Vec<String>>,
    },

    /// Extract a repo for local-only offline commit access
    Unlock {
        /// Repository name to extract
        repo: String,
    },

    /// Recompress + clear extraction, end local-only session
    Lock {
        /// Repository name to recompress
        repo: String,
    },

    /// Extract + start GitFlare LAN server
    Serve {
        /// Repository name to serve
        repo: String,

        /// Stop the running server instead of starting
        #[arg(long)]
        stop: bool,
    },

    /// Manual integrity + permissions check
    Verify {
        /// Verify specific repos only (default: all)
        #[arg(short, long)]
        repos: Option<Vec<String>>,

        /// Show detailed output
        #[arg(short, long)]
        verbose: bool,
    },

    /// Use recovery record to fix a corrupted repo
    Repair {
        /// Repository name to repair
        repo: String,
    },

    /// View/edit TOML config
    Config {
        /// Set a config value (key=value format)
        #[arg(short, long)]
        set: Option<String>,

        /// Get a config value
        #[arg(short, long)]
        get: Option<String>,
    },
}

impl Commands {
    /// Check if this command requires an initialized Gitka drive
    pub fn requires_init(&self) -> bool {
        match self {
            Commands::Init { .. } | Commands::Gui => false,
            _ => true,
        }
    }
}
