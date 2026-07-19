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
    pub command: Option<Commands>,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Launch the GUI (future feature)
    Gui,

    /// List detected USB/removable drives
    Usb {
        /// Emit JSON for machine consumers
        #[arg(long)]
        json: bool,
    },

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

        /// Volume split size in MB (e.g., 700 for CD, 4096 for FAT32)
        #[arg(long)]
        volume_size: Option<u64>,

        /// Enable/disable cross-repo deduplication
        #[arg(long)]
        dedup: Option<bool>,

        /// Run interactive setup wizard
        #[arg(short, long)]
        interactive: bool,
    },

    /// Re-scan sources and target, show size report + budget check
    Scan,

    /// Authenticate a GitHub account and store the token in config
    Auth {
        /// GitHub username or organization
        #[arg(long)]
        username: Option<String>,

        /// GitHub personal access token
        #[arg(long)]
        token: Option<String>,

        /// Verify the token against the GitHub API before saving
        #[arg(long, default_value_t = true)]
        verify: bool,

        /// Show auth status instead of updating credentials
        #[arg(long)]
        status: bool,

        /// Emit JSON for machine consumers
        #[arg(long)]
        json: bool,
    },

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

        /// Emit JSON for machine consumers
        #[arg(long)]
        json: bool,
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

    /// Wipe a removable drive and set up Gitka from scratch
    ///
    /// WARNING: This will ERASE ALL DATA on the target device.
    /// Only works on removable drives (USB, external HDD/SSD).
    Wipe {
        /// Target device path to wipe (e.g., /dev/sdb1, /Volumes/MYUSB)
        #[arg(short, long)]
        target: PathBuf,

        /// Source type: github or gitflare
        #[arg(long, default_value = "github")]
        source: String,

        /// GitHub username or organization (for GitHub source)
        #[arg(long)]
        username: Option<String>,

        /// Authentication token (GitHub PAT or GitFlare token)
        #[arg(long)]
        token: Option<String>,

        /// GitFlare instance URL (for GitFlare source)
        #[arg(long)]
        gitflare_url: Option<String>,

        /// Filesystem type: ext4, vfat (default: auto-detect based on size)
        #[arg(long)]
        filesystem: Option<String>,

        /// Skip confirmation prompt (DANGEROUS — use in scripts only)
        #[arg(long)]
        yes: bool,
    },

    /// Import an existing local git repo into the backup target
    Import {
        /// Path to the local git repository to import
        repo_path: PathBuf,

        /// Optional name for the repo (defaults to directory name)
        #[arg(short, long)]
        name: Option<String>,
    },

    /// Train zstd dictionary for better compression of small files
    TrainDict {
        /// Directory of sample files to train from (default: all repos in archive)
        #[arg(short, long)]
        source: Option<PathBuf>,
    },

    /// Update Gitka CLI and GUI to the latest version
    Update {
        /// Only check for updates without installing
        #[arg(long)]
        check: bool,

        /// Skip rebuilding the GUI
        #[arg(long)]
        no_gui: bool,

        /// Emit JSON for machine consumers
        #[arg(long)]
        json: bool,
    },
}

impl Commands {
    /// Check if this command requires an initialized Gitka drive
    pub fn requires_init(&self) -> bool {
        match self {
            Commands::Init { .. }
            | Commands::Gui
            | Commands::Usb { .. }
            | Commands::Wipe { .. }
            | Commands::Auth { .. }
            | Commands::Update { .. } => false,
            _ => true,
        }
    }
}
