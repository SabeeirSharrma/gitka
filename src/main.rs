mod archive;
mod cli;
mod compress;
mod config;
mod dedup;
mod dirty;
mod encryption;
mod error;
mod recovery;
mod repo;
mod serve;
mod source;
mod sync;
mod usb;

use clap::Parser;
use std::path::{Path, PathBuf};

use cli::{Cli, Commands};
use config::Config;
use error::{GitkaError, Result};

fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();

    // Execute command - init doesn't need config
    match &cli.command {
        Commands::Gui => {
            println!("GUI not yet implemented. Use CLI commands instead.");
            println!("Run `gitka --help` for available commands.");
        }
        Commands::Init { source, target, username, token, gitflare_url, volume_size, dedup, interactive } => {
            cmd_init(source, target, username.as_deref(), token.as_deref(), gitflare_url.as_deref(), *volume_size, *dedup, *interactive)?;
            return Ok(());
        }
        _ => {}
    }

    // Load config for other commands
    let (config, config_path) = load_config(&cli)?;

    match cli.command {
        Commands::Scan => {
            cmd_scan(&config)?;
        }
        Commands::Sync { repos } => {
            cmd_sync(&config, repos)?;
        }
        Commands::Status { repos } => {
            cmd_status(&config, repos)?;
        }
        Commands::Unlock { repo } => {
            cmd_unlock(&config, &repo)?;
        }
        Commands::Lock { repo } => {
            cmd_lock(&config, &repo)?;
        }
        Commands::Serve { repo, stop } => {
            cmd_serve(&config, &repo, stop)?;
        }
        Commands::Verify { repos, verbose } => {
            cmd_verify(&config, repos, verbose)?;
        }
        Commands::Repair { repo } => {
            cmd_repair(&config, &repo)?;
        }
        Commands::Config { set, get } => {
            let mut config = config;
            cmd_config(&mut config, set, get, &config_path)?;
        }
        Commands::Wipe { target, source, username, token, gitflare_url, filesystem, yes } => {
            cmd_wipe(&target, &source, username.as_deref(), token.as_deref(), gitflare_url.as_deref(), filesystem.as_deref(), yes)?;
            return Ok(());
        }
        Commands::Import { ref repo_path, ref name } => {
            let (config, _) = load_config(&cli)?;
            cmd_import(&config, &repo_path, name.as_deref())?;
            return Ok(());
        }
        Commands::TrainDict { ref source } => {
            let (config, _) = load_config(&cli)?;
            cmd_train_dict(&config, source.as_deref())?;
            return Ok(());
        }
        _ => unreachable!(),
    }

    Ok(())
}

/// Load config from file or create default
fn load_config(cli: &Cli) -> Result<(Config, PathBuf)> {
    let config_path = if let Some(path) = &cli.config {
        path.clone()
    } else {
        // Look for config in current directory or home directory
        let current_dir = std::env::current_dir().map_err(|e| GitkaError::Config(e.to_string()))?;
        let home_dir = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));

        let candidates = vec![
            current_dir.join("gitka.toml"),
            current_dir.join(".gitka").join("gitka.toml"),
            home_dir.join(".gitka").join("gitka.toml"),
        ];

        candidates.into_iter()
            .find(|p| p.exists())
            .ok_or_else(|| GitkaError::Config(
                "No config file found. Run `gitka init` to create one.".to_string()
            ))?
    };

    let config = Config::load(&config_path)?;
    Ok((config, config_path))
}

/// Initialize a new Gitka backup
fn cmd_init(source: &str, target: &PathBuf, username: Option<&str>, token: Option<&str>, gitflare_url: Option<&str>, volume_size: Option<u64>, dedup: Option<bool>, interactive: bool) -> Result<()> {
    if interactive {
        return cmd_init_interactive(target);
    }

    println!("Initializing Gitka backup...");
    println!("Source: {}", source);
    println!("Target: {}", target.display());

    // Create directory structure
    std::fs::create_dir_all(target.join("repos").join("archive"))?;
    std::fs::create_dir_all(target.join("extract"))?;
    std::fs::create_dir_all(target.join("tools").join("gitflare"))?;
    std::fs::create_dir_all(target.join("tools").join("recovery"))?;
    std::fs::create_dir_all(target.join("recovery-data"))?;
    std::fs::create_dir_all(target.join(".gitka").join("logs"))?;
    std::fs::create_dir_all(target.join(".gitka").join("repos"))?;
    std::fs::create_dir_all(target.join(".gitka").join("dedup-store"))?;

    // Create config with source settings
    let mut config = Config::default();
    config.target.path = target.clone();

    match source {
        "github" => {
            config.source.github_username = username.map(|s| s.to_string());
            config.source.auth_token = token.map(|s| s.to_string());

            if config.source.github_username.is_none() {
                println!("\n⚠ Warning: No GitHub username provided.");
                println!("  Edit {} to set github_username", target.join(".gitka").join("gitka.toml").display());
            }
        }
        "gitflare" => {
            config.source.gitflare_url = gitflare_url.map(|s| s.to_string());
            config.source.auth_token = token.map(|s| s.to_string());

            if config.source.gitflare_url.is_none() {
                println!("\n⚠ Warning: No GitFlare URL provided.");
                println!("  Edit {} to set gitflare_url", target.join(".gitka").join("gitka.toml").display());
            }
        }
        _ => {
            return Err(GitkaError::Config(format!("Unknown source type: {}", source)));
        }
    }

    // Apply volume splitting
    if let Some(size) = volume_size {
        config.compression.volume_splitting = Some(config::VolumeSplitting { size_mb: size });
        println!("  Volume splitting: {} MB per part", size);
    }

    // Apply dedup setting
    if let Some(dedup_on) = dedup {
        config.compression.dedup = dedup_on;
        println!("  Cross-repo dedup: {}", if dedup_on { "enabled" } else { "disabled" });
    }

    let config_path = target.join(".gitka").join("gitka.toml");
    config.save(&config_path)?;

    println!("✓ Directory structure created");
    println!("✓ Config file created at {}", config_path.display());
    println!("\nNext steps:");
    println!("1. Edit {} to configure your sources", config_path.display());
    println!("2. Run `gitka scan` to discover repos");
    println!("3. Run `gitka sync` to clone and back up repos");

    Ok(())
}

/// Interactive setup wizard
fn cmd_init_interactive(target: &PathBuf) -> Result<()> {
    use std::io::{self, Write};

    println!("🚀 Gitka Setup Wizard");
    println!("=====================\n");

    // Create directory structure
    std::fs::create_dir_all(target.join("repos").join("archive"))?;
    std::fs::create_dir_all(target.join("extract"))?;
    std::fs::create_dir_all(target.join("tools").join("gitflare"))?;
    std::fs::create_dir_all(target.join("tools").join("recovery"))?;
    std::fs::create_dir_all(target.join("recovery-data"))?;
    std::fs::create_dir_all(target.join(".gitka").join("logs"))?;
    std::fs::create_dir_all(target.join(".gitka").join("repos"))?;
    std::fs::create_dir_all(target.join(".gitka").join("dedup-store"))?;

    let mut config = Config::default();
    config.target.path = target.clone();

    // Source type
    println!("📦 Source Configuration");
    println!("-----------------------");
    print!("Source type (github/gitflare) [github]: ");
    io::stdout().flush()?;
    let mut source = String::new();
    io::stdin().read_line(&mut source)?;
    let source = source.trim();
    let source = if source.is_empty() { "github" } else { source };

    match source {
        "github" => {
            print!("GitHub username: ");
            io::stdout().flush()?;
            let mut username = String::new();
            io::stdin().read_line(&mut username)?;
            let username = username.trim().to_string();
            if !username.is_empty() {
                config.source.github_username = Some(username);
            }

            print!("GitHub token (optional, press Enter to skip): ");
            io::stdout().flush()?;
            let mut token = String::new();
            io::stdin().read_line(&mut token)?;
            let token = token.trim().to_string();
            if !token.is_empty() {
                config.source.auth_token = Some(token);
            }
        }
        "gitflare" => {
            print!("GitFlare URL: ");
            io::stdout().flush()?;
            let mut url = String::new();
            io::stdin().read_line(&mut url)?;
            let url = url.trim().to_string();
            if !url.is_empty() {
                config.source.gitflare_url = Some(url);
            }

            print!("GitFlare token (optional, press Enter to skip): ");
            io::stdout().flush()?;
            let mut token = String::new();
            io::stdin().read_line(&mut token)?;
            let token = token.trim().to_string();
            if !token.is_empty() {
                config.source.auth_token = Some(token);
            }
        }
        _ => {
            println!("⚠ Unknown source type: {}. Using github.", source);
        }
    }

    // Compression settings
    println!("\n🗜️  Compression Settings");
    println!("-----------------------");
    print!("Compression tier (auto/low/medium/high) [auto]: ");
    io::stdout().flush()?;
    let mut tier = String::new();
    io::stdin().read_line(&mut tier)?;
    let tier = tier.trim();
    config.compression.tier = match tier {
        "low" => config::CompressionTier::Low,
        "medium" => config::CompressionTier::Medium,
        "high" => config::CompressionTier::High,
        _ => config::CompressionTier::Auto,
    };

    // Volume splitting
    println!("\n📦 Volume Splitting");
    println!("-----------------------");
    println!("Split archives into fixed-size parts (e.g., 700 for CD, 4096 for FAT32).");
    print!("Volume split size in MB (0 = off) [0]: ");
    io::stdout().flush()?;
    let mut volume_size = String::new();
    io::stdin().read_line(&mut volume_size)?;
    let volume_size: u64 = volume_size.trim().parse().unwrap_or(0);
    if volume_size > 0 {
        config.compression.volume_splitting = Some(config::VolumeSplitting { size_mb: volume_size });
        println!("  ✓ Volume splitting: {} MB per part", volume_size);
    }

    // Cross-repo dedup
    println!("\n📦 Cross-Repo Deduplication");
    println!("-----------------------");
    println!("Share common blobs across repos to save space (e.g., shared libraries).");
    print!("Enable cross-repo dedup? (y/n) [y]: ");
    io::stdout().flush()?;
    let mut dedup_input = String::new();
    io::stdin().read_line(&mut dedup_input)?;
    config.compression.dedup = dedup_input.trim().to_lowercase() != "n";
    if config.compression.dedup {
        println!("  ✓ Dedup enabled");
    }

    // Feature toggles
    println!("\n🔧 Feature Toggles");
    println!("-----------------------");
    print!("Enable recovery records? (y/n) [n]: ");
    io::stdout().flush()?;
    let mut recovery = String::new();
    io::stdin().read_line(&mut recovery)?;
    config.toggles.recovery_records = recovery.trim().to_lowercase() == "y";

    print!("Enable encryption? (y/n) [n]: ");
    io::stdout().flush()?;
    let mut encryption = String::new();
    io::stdin().read_line(&mut encryption)?;
    config.toggles.encryption = encryption.trim().to_lowercase() == "y";

    // Save config
    let config_path = target.join(".gitka").join("gitka.toml");
    config.save(&config_path)?;

    println!("\n✅ Setup complete!");
    println!("✓ Directory structure created at {}", target.display());
    println!("✓ Config file created at {}", config_path.display());
    println!("\nNext steps:");
    println!("1. Run `gitka scan` to discover repos");
    println!("2. Run `gitka sync` to clone and back up repos");

    Ok(())
}

/// Wipe a removable drive and set up Gitka from scratch
fn cmd_wipe(
    target: &std::path::Path,
    source: &str,
    username: Option<&str>,
    token: Option<&str>,
    gitflare_url: Option<&str>,
    filesystem: Option<&str>,
    skip_confirm: bool,
) -> Result<()> {
    use std::io::{self, Write};

    println!("🔍 Checking target device...\n");

    // 1. Verify target exists
    if !target.exists() {
        return Err(GitkaError::UsbDetection(format!(
            "Target path does not exist: {}",
            target.display()
        )));
    }

    // 2. Check if it's a removable drive — HARD REQUIREMENT
    let is_removable = usb::validate_removable(target)?;
    if !is_removable {
        return Err(GitkaError::UsbDetection(format!(
            "Refusing to wipe: {} is NOT a removable drive.\n\
             Only USB/external drives can be wiped.\n\
             Use `gitka init --target {}` for non-removable drives.",
            target.display(),
            target.display()
        )));
    }

    // 3. Get drive info for display
    let drive_info = usb::get_drive_info(target)?;

    println!("╔══════════════════════════════════════════════════════╗");
    println!("║              ⚠️  DESTRUCTIVE OPERATION  ⚠️           ║");
    println!("╠══════════════════════════════════════════════════════╣");
    println!("║  This will ERASE ALL DATA on:                       ║");
    println!("║                                                      ║");
    println!("║  Path:       {:<39}║", target.display());
    println!("║  Label:      {:<39}║", drive_info.label.as_deref().unwrap_or("(none)"));
    println!("║  Filesystem: {:<39}║", drive_info.fs_type);
    println!("║  Total:      {:<39}║", format_size(drive_info.total_space));
    println!("║  Free:       {:<39}║", format_size(drive_info.free_space));
    println!("║                                                      ║");
    println!("║  All existing files, partitions, and data will be    ║");
    println!("║  permanently destroyed. This cannot be undone.       ║");
    println!("╚══════════════════════════════════════════════════════╝");
    println!();

    // 4. Interactive confirmation
    if !skip_confirm {
        print!("Type YES to confirm wipe of {}: ", target.display());
        io::stdout().flush()?;
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        let input = input.trim();

        if input != "YES" {
            println!("\n❌ Wipe cancelled. No changes made.");
            return Ok(());
        }
    } else {
        println!("⚠ --yes flag: skipping confirmation");
    }

    println!("\n🗑️  Wiping {}...\n", target.display());

    // 5. Format the drive
    let fs_type = determine_fs(&drive_info, filesystem)?;
    format_drive(target, &fs_type)?;

    println!("  ✓ Drive formatted as {}", fs_type);

    // 6. Remount (format unmounts the drive)
    remount_drive(target)?;

    // 7. Create directory structure
    println!("  Creating directory structure...");
    std::fs::create_dir_all(target.join("repos").join("archive"))?;
    std::fs::create_dir_all(target.join("extract"))?;
    std::fs::create_dir_all(target.join("tools").join("gitflare"))?;
    std::fs::create_dir_all(target.join("tools").join("recovery"))?;
    std::fs::create_dir_all(target.join("recovery-data"))?;
    std::fs::create_dir_all(target.join(".gitka").join("logs"))?;
    std::fs::create_dir_all(target.join(".gitka").join("repos"))?;

    // 8. Create config
    println!("  Creating config...");
    let mut config = Config::default();
    config.target.path = target.to_path_buf();

    match source {
        "github" => {
            config.source.github_username = username.map(|s| s.to_string());
            config.source.auth_token = token.map(|s| s.to_string());
        }
        "gitflare" => {
            config.source.gitflare_url = gitflare_url.map(|s| s.to_string());
            config.source.auth_token = token.map(|s| s.to_string());
        }
        _ => {
            return Err(GitkaError::Config(format!("Unknown source type: {}", source)));
        }
    }

    let config_path = target.join(".gitka").join("gitka.toml");
    config.save(&config_path)?;

    println!("  ✓ Config created");

    println!("\n✅ Wipe and setup complete!");
    println!("  Device: {}", target.display());
    println!("  Config: {}", config_path.display());
    println!("\nNext steps:");
    println!("  1. Run `gitka scan` to discover repos");
    println!("  2. Run `gitka sync` to clone and back up repos");

    Ok(())
}

/// Determine filesystem type based on drive size and user preference
fn determine_fs(drive_info: &usb::DriveInfo, preference: Option<&str>) -> Result<String> {
    if let Some(fs) = preference {
        return Ok(fs.to_string());
    }

    // Auto-detect based on drive size
    // < 4GB: vfat (FAT32) — maximum compatibility
    // 4GB - 2TB: ext4 — good balance
    // > 2TB: ext4 — best support for large drives
    if drive_info.total_space < 4 * 1024 * 1024 * 1024 {
        Ok("vfat".to_string())
    } else {
        Ok("ext4".to_string())
    }
}

/// Format a drive with the specified filesystem
#[cfg(target_os = "linux")]
fn format_drive(target: &std::path::Path, fs_type: &str) -> Result<()> {
    use std::process::Command;

    // Unmount first
    let _ = Command::new("umount")
        .arg(target)
        .output();

    let device_str = target.to_string_lossy();

    let output = match fs_type {
        "vfat" => {
            Command::new("mkfs.vfat")
                .args(&["-F", "32", "-n", "GITKA", &device_str])
                .output()
        }
        "ext4" => {
            Command::new("mkfs.ext4")
                .args(&["-F", "-L", "GITKA", &device_str])
                .output()
        }
        "ntfs" => {
            Command::new("mkfs.ntfs")
                .args(&["-f", "-L", "GITKA", &device_str])
                .output()
        }
        _ => {
            return Err(GitkaError::Config(format!(
                "Unsupported filesystem: {}. Use ext4, vfat, or ntfs.",
                fs_type
            )));
        }
    };

    let output = output.map_err(|e| GitkaError::Config(format!("Failed to run mkfs: {}", e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(GitkaError::Config(format!(
            "Format failed: {}",
            stderr.trim()
        )));
    }

    Ok(())
}

/// Format a drive on macOS
#[cfg(target_os = "macos")]
fn format_drive(target: &std::path::Path, fs_type: &str) -> Result<()> {
    use std::process::Command;

    // Unmount first
    let _ = Command::new("diskutil")
        .args(&["unmountDisk", "force", target.to_str().unwrap_or("")])
        .output();

    // Get the disk identifier
    let output = Command::new("diskutil")
        .args(&["info", "-plist", target.to_str().unwrap_or("")])
        .output()
        .map_err(|e| GitkaError::Config(format!("Failed to run diskutil: {}", e)))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let plist: serde_json::Value = serde_json::from_str(&stdout)
        .map_err(|e| GitkaError::Config(format!("Failed to parse diskutil output: {}", e)))?;

    let disk_id = plist["DeviceNode"].as_str().unwrap_or("");

    let output = match fs_type {
        "vfat" | "fat32" => {
            Command::new("diskutil")
                .args(&["eraseDisk", "FAT32", "GITKA", "MBRFormat", disk_id])
                .output()
        }
        "ext4" => {
            // macOS doesn't natively support ext4, use ExFAT instead
            println!("  ⚠ macOS doesn't support ext4 natively. Using ExFAT instead.");
            Command::new("diskutil")
                .args(&["eraseDisk", "ExFAT", "GITKA", "MBRFormat", disk_id])
                .output()
        }
        "exfat" => {
            Command::new("diskutil")
                .args(&["eraseDisk", "ExFAT", "GITKA", "MBRFormat", disk_id])
                .output()
        }
        "ntfs" => {
            Command::new("diskutil")
                .args(&["eraseDisk", "NTFS", "GITKA", "MBRFormat", disk_id])
                .output()
        }
        _ => {
            return Err(GitkaError::Config(format!(
                "Unsupported filesystem: {}. Use vfat, exfat, or ntfs on macOS.",
                fs_type
            )));
        }
    };

    let output = output.map_err(|e| GitkaError::Config(format!("Failed to run diskutil: {}", e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(GitkaError::Config(format!(
            "Format failed: {}",
            stderr.trim()
        )));
    }

    Ok(())
}

/// Format a drive on Windows
#[cfg(target_os = "windows")]
fn format_drive(target: &std::path::Path, fs_type: &str) -> Result<()> {
    use std::process::Command;

    let drive_str = target.to_string_lossy();

    // Use diskpart for formatting on Windows
    let diskpart_script = format!(
        "select volume {}\nformat fs={} label=GITKA quick\n",
        drive_str.trim_end_matches('\\'),
        fs_type
    );

    let output = Command::new("diskpart")
        .args(&["/s", &format!("{}\nexit\n", diskpart_script)])
        .output()
        .map_err(|e| GitkaError::Config(format!("Failed to run diskpart: {}", e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(GitkaError::Config(format!(
            "Format failed: {}",
            stderr.trim()
        )));
    }

    Ok(())
}

/// Remount a drive after formatting
#[cfg(target_os = "linux")]
fn remount_drive(target: &std::path::Path) -> Result<()> {
    use std::process::Command;

    // Wait for kernel to recognize the new filesystem
    std::thread::sleep(std::time::Duration::from_millis(500));

    // Try to mount
    let output = Command::new("mount")
        .arg(target)
        .output()
        .map_err(|e| GitkaError::Config(format!("Failed to mount: {}", e)))?;

    if !output.status.success() {
        // Some systems auto-mount, so this might be OK
        // Check if the mount point is accessible
        if !target.join(".gitka").exists() && !target.exists() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(GitkaError::Config(format!(
                "Failed to mount after format: {}",
                stderr.trim()
            )));
        }
    }

    Ok(())
}

/// Remount a drive on macOS
#[cfg(target_os = "macos")]
fn remount_drive(target: &std::path::Path) -> Result<()> {
    use std::process::Command;

    // macOS auto-mounts after format
    std::thread::sleep(std::time::Duration::from_millis(500));

    // Verify mount
    if !target.exists() {
        return Err(GitkaError::Config(
            "Drive not accessible after format. Check Disk Utility.".to_string(),
        ));
    }

    Ok(())
}

/// Remount a drive on Windows
#[cfg(target_os = "windows")]
fn remount_drive(_target: &std::path::Path) -> Result<()> {
    // Windows auto-mounts after format
    Ok(())
}

/// Format bytes to human-readable size
fn format_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.1} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}

/// Scan sources and target, show size report
fn cmd_scan(config: &Config) -> Result<()> {
    println!("Scanning sources and target...");

    // Detect drives
    let drives = usb::detect_drives(&config.target.mode)?;
    println!("\nDetected drives:");
    for drive in &drives {
        println!(
            "  {} - {} ({:.1} GB free of {:.1} GB) {}",
            drive.mount_point.display(),
            drive.fs_type,
            drive.free_space as f64 / 1_073_741_824.0,
            drive.total_space as f64 / 1_073_741_824.0,
            if drive.is_removable { "[removable]" } else { "" }
        );
    }

    // Discover repos from source
    println!("\nDiscovering repos from source...");
    match source::create_source(&config.source) {
        Ok(source_provider) => {
            match source_provider.list_repos() {
                Ok(remote_repos) => {
                    println!("Found {} repos:", remote_repos.len());
                    for repo in &remote_repos {
                        println!(
                            "  {} ({:.1} MB) {}",
                            repo.full_name,
                            repo.size as f64 / 1_048_576.0,
                            if repo.private { "[private]" } else { "" }
                        );
                    }

                    // Show which repos are configured locally
                    let repo_manager = repo::RepoManager::new(config.clone());
                    let local_repos = repo_manager.list_repos()?;
                    let local_names: Vec<&str> = local_repos.iter().map(|r| r.name.as_str()).collect();

                    println!("\nLocal status:");
                    for remote_repo in &remote_repos {
                        if local_names.contains(&remote_repo.name.as_str()) {
                            let local_repo = local_repos.iter().find(|r| r.name == remote_repo.name).unwrap();
                            println!(
                                "  {} - {:?} ({:.1} MB archive)",
                                remote_repo.name,
                                local_repo.state,
                                local_repo.archive_size as f64 / 1_048_576.0
                            );
                        } else {
                            println!("  {} - not cloned yet", remote_repo.name);
                        }
                    }
                }
                Err(e) => {
                    println!("⚠ Failed to list repos: {}", e);
                    println!("  Check your source configuration and authentication.");
                }
            }
        }
        Err(e) => {
            println!("⚠ No source configured: {}", e);
            println!("  Edit your config to set github_username or gitflare_url.");
        }
    }

    // Budget check
    let repo_manager = repo::RepoManager::new(config.clone());
    let total_archive = repo_manager.total_archive_size()?;
    let total_decompressed = repo_manager.total_decompressed_size()?;
    let free_space = drives.first().map(|d| d.free_space).unwrap_or(0);

    let budget = compress::BudgetCheck::new(free_space, total_decompressed);
    let tier = budget.determine_tier(&config.compression);

    println!("\nBudget:");
    println!("  Archive size: {:.1} MB", total_archive as f64 / 1_048_576.0);
    println!("  Decompressed size: {:.1} MB", total_decompressed as f64 / 1_048_576.0);
    println!("  Free space: {:.1} MB", free_space as f64 / 1_048_576.0);
    println!("  Recommended tier: {:?}", tier);

    if budget.is_over_budget() {
        println!("\n⚠ Warning: Repos may not fit even at maximum compression!");
        println!("  Consider reducing repo count or using a larger drive.");
    }

    Ok(())
}

/// Sync repos
fn cmd_sync(config: &Config, repos: Option<Vec<String>>) -> Result<()> {
    // Load dirty log
    let dirty_log = dirty::DirtyLog::load(config);

    // Crash resilience: detect orphaned extracted repos
    if !dirty_log.is_empty() {
        let repo_manager = repo::RepoManager::new(config.clone());
        let all_repos = repo_manager.list_repos()?;
        let extracted: Vec<String> = all_repos.iter()
            .filter(|r| r.state != repo::RepoState::Archived)
            .map(|r| r.name.clone())
            .collect();

        let orphaned = dirty_log.detect_orphaned(&extracted);
        if !orphaned.is_empty() {
            println!("⚠ Detected {} orphaned repo(s) from an earlier session:", orphaned.len());
            for name in &orphaned {
                println!("  - {} (was left extracted, recompressing)", name);
            }
            println!();

            // Recompress orphaned repos before continuing
            for name in &orphaned {
                println!("Recompressing {}...", name);
                match cmd_lock(config, name) {
                    Ok(()) => println!("  ✓ {} recompressed", name),
                    Err(e) => println!("  ⚠ Failed to recompress {}: {}", name, e),
                }
            }
            println!();
        }
    }

    println!("Syncing repos...");

    // Get source provider
    let source_provider = source::create_source(&config.source)?;
    let auth = source_provider.auth_method();

    // Get list of remote repos
    let remote_repos = source_provider.list_repos()?;

    // Filter repos if specific ones requested
    let repos_to_sync: Vec<&source::RemoteRepo> = match &repos {
        Some(names) => remote_repos.iter()
            .filter(|r| names.contains(&r.name))
            .collect(),
        None => remote_repos.iter().collect(),
    };

    let repo_manager = repo::RepoManager::new(config.clone());

    // Check SolidMode: FullArchive compresses all repos into a single stream
    let is_full_archive = matches!(config.compression.solid, config::SolidMode::FullArchive);

    if is_full_archive && repos_to_sync.len() > 1 {
        // FullArchive mode: compress all repos into a single archive
        println!("FullArchive mode: compressing {} repos into a single stream...", repos_to_sync.len());

        // Create a temporary staging directory
        let staging_dir = std::env::temp_dir().join("gitka-full-archive-staging");
        if staging_dir.exists() {
            std::fs::remove_dir_all(&staging_dir)?;
        }
        std::fs::create_dir_all(&staging_dir)?;

        // Clone/fetch each repo into the staging directory
        for remote_repo in &repos_to_sync {
            let repo_staging = staging_dir.join(&remote_repo.name);
            let local_meta = repo_manager.load_meta(&remote_repo.name).ok();

            if let Some(meta) = local_meta {
                if meta.state == repo::RepoState::Archived {
                    println!("  Skipping {} (archived, use `gitka unlock` first)", remote_repo.name);
                    continue;
                }
                if let Some(extraction_path) = &meta.extraction_path {
                    // Copy extracted repo to staging
                    std::fs::copy(extraction_path, &repo_staging)?;
                }
            } else {
                // Clone new repo to staging
                let archive_dir = config.archive_dir();
                std::fs::create_dir_all(&archive_dir)?;
                match source::clone_repo(remote_repo, &staging_dir, &auth, true) {
                    Ok(_) => {}
                    Err(e) => {
                        println!("  ⚠ Failed to clone {}: {}", remote_repo.name, e);
                        continue;
                    }
                }
            }
        }

        // Compress the entire staging directory into one archive
        let archive_dir = config.archive_dir();
        std::fs::create_dir_all(&archive_dir)?;
        let archive_path = archive_dir.join("full-archive.gitka.zst");

        let mut dedup_store = if config.compression.dedup {
            let mut store = dedup::DedupStore::open(config);
            store.init().ok();
            store.load_index().ok();
            Some(store)
        } else {
            None
        };

        println!("  Compressing all repos...");
        match compress::compress_directory_with_options(&staging_dir, &archive_path, &config.compression, dedup_store.as_mut()) {
            Ok(result) => {
                if let Some(store) = dedup_store.as_ref() {
                    store.save_index().ok();
                }

                // Create metadata for each repo pointing to the shared archive
                for remote_repo in &repos_to_sync {
                    let meta = repo::RepoMeta {
                        name: remote_repo.name.clone(),
                        state: repo::RepoState::Archived,
                        archive_path: PathBuf::from("full-archive.gitka.zst"),
                        archive_hash: Some(compress::calculate_hash(&archive_path)?),
                        archive_size: result.total_size,
                        volume_count: result.volume_count,
                        archive_parts: result.part_files.clone(),
                        decompressed_size: None,
                        last_synced: None,
                        last_verified: None,
                        extraction_path: None,
                        dedup_enabled: config.compression.dedup,
                        dedup_bytes_saved: result.dedup_bytes_saved,
                    };
                    repo_manager.save_meta(&meta)?;
                }

                println!("  ✓ Full archive ({:.1} MB, {} repos)",
                    result.total_size as f64 / 1_048_576.0,
                    repos_to_sync.len());
            }
            Err(e) => {
                println!("  ⚠ Compression failed: {}", e);
            }
        }

        // Clean up staging
        std::fs::remove_dir_all(&staging_dir).ok();
    } else {
        // PerRepo mode (default): compress each repo separately
        for remote_repo in repos_to_sync {
        // Incremental sync: skip repos not in dirty log (unless --repos was specified)
        if repos.is_none() && !dirty_log.is_dirty(&remote_repo.name) {
            // Check if repo exists and is archived — skip fetch entirely
            if let Ok(meta) = repo_manager.load_meta(&remote_repo.name) {
                if meta.state == repo::RepoState::Archived {
                    println!("\nSkipping {} (not modified since last sync)", remote_repo.name);
                    continue;
                }
            }
        }

        println!("\nSyncing {}...", remote_repo.full_name);

        // Check if repo exists locally
        let local_meta = repo_manager.load_meta(&remote_repo.name).ok();

        if let Some(meta) = local_meta {
            // Repo exists, fetch updates
            if meta.state == repo::RepoState::Archived {
                println!("  Repo is archived, skipping fetch. Use `gitka unlock {}` to extract first.", remote_repo.name);
                continue;
            }

            let extraction_path = meta.extraction_path
                .ok_or_else(|| GitkaError::Extraction(format!("No extraction path for {}", remote_repo.name)))?;

            println!("  Fetching updates...");
            source::fetch_repo(&extraction_path, &auth)?;

            // Sync the repo
            match sync::sync_repo(config, &remote_repo.name) {
                Ok(status) => {
                    match status {
                        sync::SyncStatus::Ahead(n) => println!("  Pushed {} commits", n),
                        sync::SyncStatus::Behind(n) => println!("  Pulled {} commits", n),
                        sync::SyncStatus::InSync => println!("  Up to date"),
                        sync::SyncStatus::Diverged { ahead, behind } => {
                            println!("  Diverged ({} ahead, {} behind)", ahead, behind);
                        }
                        sync::SyncStatus::Conflict(msg) => {
                            println!("  CONFLICT: {}", msg);
                        }
                    }
                }
                Err(e) => {
                    println!("  ⚠ Sync failed: {}", e);
                }
            }
        } else {
            // Repo doesn't exist, clone it
            println!("  Cloning new repo...");

            let archive_dir = config.archive_dir();
            std::fs::create_dir_all(&archive_dir)?;

            // Determine shallow based on config
            let repo_config = config.get_repo(&remote_repo.name);
            let shallow = match repo_config {
                Ok(r) => !r.full_history,
                Err(_) => true, // Default to shallow
            };

            match source::clone_repo(remote_repo, &archive_dir, &auth, shallow) {
                Ok(repo_path) => {
                    let repo_size = source::repo_size(&repo_path)?;

                    // Create repo metadata
                    let meta = repo::RepoMeta {
                        name: remote_repo.name.clone(),
                        state: repo::RepoState::Archived,
                        archive_path: PathBuf::from(format!("{}.gitka.zst", remote_repo.name)),
                        archive_hash: None,
                        archive_size: 0,
                        volume_count: 1,
                        archive_parts: Vec::new(),
                        decompressed_size: Some(repo_size),
                        last_synced: None,
                        last_verified: None,
                        extraction_path: None,
                        dedup_enabled: false,
                        dedup_bytes_saved: 0,
                    };

                    repo_manager.save_meta(&meta)?;

                    // Compress the repo
                    println!("  Compressing...");
                    let archive_path = archive_dir.join(format!("{}.gitka.zst", remote_repo.name));

                    // Initialize dedup store if enabled
                    let mut dedup_store = if config.compression.dedup {
                        let mut store = dedup::DedupStore::open(config);
                        store.init().ok();
                        store.load_index().ok();
                        Some(store)
                    } else {
                        None
                    };

                    match compress::compress_directory_with_options(&repo_path, &archive_path, &config.compression, dedup_store.as_mut()) {
                        Ok(result) => {
                            // Calculate hash of first part
                            let hash = compress::calculate_hash(&archive_path)?;

                            // Update metadata with archive info
                            let mut meta = repo_manager.load_meta(&remote_repo.name)?;
                            meta.archive_size = result.total_size;
                            meta.archive_hash = Some(hash);
                            meta.volume_count = result.volume_count;
                            meta.archive_parts = result.part_files.clone();
                            meta.dedup_enabled = config.compression.dedup;
                            meta.dedup_bytes_saved = result.dedup_bytes_saved;
                            repo_manager.save_meta(&meta)?;

                            // Save dedup index
                            if let Some(store) = dedup_store.as_ref() {
                                store.save_index().ok();
                            }

                            let dedup_info = if result.dedup_bytes_saved > 0 {
                                format!(", dedup saved {:.1} MB", result.dedup_bytes_saved as f64 / 1_048_576.0)
                            } else {
                                String::new()
                            };
                            let volume_info = if result.volume_count > 1 {
                                format!(", {} parts", result.volume_count)
                            } else {
                                String::new()
                            };

                            println!("  ✓ Cloned and compressed ({:.1} MB archive, {:.1} MB source{}{})",
                                result.total_size as f64 / 1_048_576.0,
                                repo_size as f64 / 1_048_576.0,
                                volume_info,
                                dedup_info);

                            // Encrypt if enabled (per-volume)
                            if config.toggles.encryption {
                                if let Some(key) = config.get_encryption_key() {
                                    println!("  Encrypting...");
                                    match encryption::encrypt_parts(&archive_path, &result.part_files, &key) {
                                        Ok(enc_size) => {
                                            println!("  ✓ Encrypted ({:.1} MB)", enc_size as f64 / 1_048_576.0);
                                        }
                                        Err(e) => {
                                            println!("  ⚠ Encryption failed: {}", e);
                                        }
                                    }
                                } else {
                                    println!("  ⚠ Encryption enabled but no password configured");
                                }
                            }

                            // Create recovery records if enabled (per-volume)
                            if config.toggles.recovery_records && recovery::is_par2_available() {
                                println!("  Creating recovery records...");
                                let recovery_dir = config.recovery_dir().join(&remote_repo.name);
                                match recovery::create_recovery_parts(&archive_path, &result.part_files, &recovery_dir, 25) {
                                    Ok(infos) => {
                                        let total_recovery: u64 = infos.iter().map(|i| i.recovery_size).sum();
                                        println!("  ✓ Recovery records created ({:.1} MB, {} parts)",
                                            total_recovery as f64 / 1_048_576.0,
                                            infos.len());
                                    }
                                    Err(e) => {
                                        println!("  ⚠ Recovery record creation failed: {}", e);
                                    }
                                }
                            }

                            // Clean up the extracted repo
                            std::fs::remove_dir_all(&repo_path)?;
                        }
                        Err(e) => {
                            println!("  ⚠ Compression failed: {}", e);
                            println!("  Repo remains at: {}", repo_path.display());
                        }
                    }
                }
                Err(e) => {
                    println!("  ⚠ Clone failed: {}", e);
                }
            }
        }
        } // end for remote_repo
    }

    // Clear dirty log only after successful sync
    let mut dirty_log = dirty::DirtyLog::load(config);
    dirty_log.clear_all();
    dirty_log.save(config)?;

    println!("\n✓ Sync complete");
    Ok(())
}

/// Show status
fn cmd_status(config: &Config, repos: Option<Vec<String>>) -> Result<()> {
    let repo_manager = repo::RepoManager::new(config.clone());
    let all_repos = repo_manager.list_repos()?;
    let dirty_log = dirty::DirtyLog::load(config);

    let repos_to_show: Vec<&repo::RepoMeta> = match repos {
        Some(names) => all_repos.iter()
            .filter(|r| names.contains(&r.name))
            .collect(),
        None => all_repos.iter().collect(),
    };

    println!("Repository Status:");
    println!("{:<30} {:<15} {:<15} {:<20} {}", "Name", "State", "Last Synced", "Archive Size", "Session");
    println!("{}", "-".repeat(95));

    for repo in repos_to_show {
        let session_info = if let Some(entry) = dirty_log.entries.get(&repo.name) {
            let age = dirty::format_age(dirty::current_timestamp().saturating_sub(entry.timestamp));
            let action = match entry.action {
                dirty::DirtyAction::Unlock => "unlocked",
                dirty::DirtyAction::Serve => "serving",
                dirty::DirtyAction::Modified => "modified",
            };
            let commits = if entry.commits.len() > 1 {
                format!(" ({} commits)", entry.commits.len() - 1)
            } else {
                String::new()
            };
            let files = if !entry.files_touched.is_empty() {
                format!(", {} files", entry.files_touched.len())
            } else {
                String::new()
            };
            format!("{} {}{}{}", action, age, commits, files)
        } else {
            String::new()
        };

        println!(
            "{:<30} {:<15} {:<15} {:<20} {}",
            repo.name,
            format!("{:?}", repo.state),
            repo.last_synced.as_deref().unwrap_or("never"),
            format!("{:.1} MB", repo.archive_size as f64 / 1_048_576.0),
            session_info,
        );
    }

    // Show dirty log summary
    if !dirty_log.is_empty() {
        println!("\n⚠ {} repo(s) have active sessions:", dirty_log.entries.len());
        for (name, entry) in &dirty_log.entries {
            let age = dirty::format_age(dirty::current_timestamp().saturating_sub(entry.timestamp));
            let action = match entry.action {
                dirty::DirtyAction::Unlock => "unlocked",
                dirty::DirtyAction::Serve => "serving",
                dirty::DirtyAction::Modified => "modified",
            };
            let detail = if !entry.files_touched.is_empty() {
                format!(", {} files touched", entry.files_touched.len())
            } else if entry.commits.len() > 1 {
                format!(", {} commits", entry.commits.len() - 1)
            } else {
                String::new()
            };
            println!("  {} — {} ({}{})", name, action, age, detail);
        }
        println!("\n  Run `gitka lock <repo>` to recompress and close session.");
    }

    Ok(())
}

/// Unlock a repo for offline access
fn cmd_unlock(config: &Config, repo_name: &str) -> Result<()> {
    println!("Unlocking {} for offline access...", repo_name);

    let repo_manager = repo::RepoManager::new(config.clone());
    let mut meta = repo_manager.load_meta(repo_name)?;

    // Check if already extracted
    if meta.state != repo::RepoState::Archived {
        return Err(GitkaError::AlreadyExtracted(repo_name.to_string()));
    }

    // Check workspace eligibility — if repo is in config, check the flag; otherwise default to eligible
    if let Ok(repo_config) = config.get_repo(repo_name) {
        if !repo_config.workspace_eligible {
            return Err(GitkaError::NotWorkspaceEligible(repo_name.to_string()));
        }
    }

    // Budget check
    let free_space = usb::get_drive_info(&config.target.path)?.free_space;
    let decompressed_size = meta.decompressed_size.unwrap_or(0);
    let budget = compress::BudgetCheck::new(free_space, decompressed_size);

    if budget.is_over_budget() {
        println!("⚠ Warning: This repo may not fit for extraction!");
        println!("  Consider using extraction target 'host' instead.");
    }

    // Extract the repo
    let extraction_path = meta.extraction_target(config);
    std::fs::create_dir_all(&extraction_path)?;

    // Decrypt if encrypted (per-volume)
    let archive_path = meta.archive_full_path(config);
    if encryption::is_encrypted(&archive_path)? {
        if let Some(key) = config.get_encryption_key() {
            println!("  Decrypting...");
            // Copy all volume parts to temp location for decryption
            let temp_dir = std::env::temp_dir().join(format!("gitka-{}.tmp", repo_name));
            std::fs::create_dir_all(&temp_dir)?;

            // Copy and decrypt each part
            for part_name in &meta.archive_parts {
                let src = config.archive_dir().join(part_name);
                let dst = temp_dir.join(part_name);
                std::fs::copy(&src, &dst)?;
                encryption::decrypt_file(&dst, &key)?;
            }

            // Decompress from temp (first part)
            let first_part = temp_dir.join(&meta.archive_parts[0]);
            compress::decompress_directory(&first_part, &extraction_path)?;

            // Clean up temp
            std::fs::remove_dir_all(&temp_dir)?;
        } else {
            return Err(GitkaError::Config(
                "Archive is encrypted but no password configured".to_string(),
            ));
        }
    } else {
        // Decompress archive
        compress::decompress_directory(&archive_path, &extraction_path)?;
    }

    // Update metadata
    meta.state = repo::RepoState::ExtractedLocal;
    meta.extraction_path = Some(extraction_path.clone());
    repo_manager.save_meta(&meta)?;

    // Record in dirty log + audit trail (HEAD commit before session)
    let mut dirty_log = dirty::DirtyLog::load(config);
    dirty_log.record_unlock(repo_name);

    // Record the HEAD commit hash as the session baseline
    if let Ok(repo) = git2::Repository::open(&extraction_path) {
        if let Ok(head) = repo.head() {
            if let Some(oid) = head.target() {
                dirty_log.record_commit(repo_name, &oid.to_string());
            }
        }
    }

    dirty_log.save(config)?;

    println!("✓ Repo extracted to {}", extraction_path.display());
    println!("  You can now make commits offline.");
    println!("  Run `gitka lock {}` when done.", repo_name);

    Ok(())
}

/// Lock a repo (recompress and clear extraction)
fn cmd_lock(config: &Config, repo_name: &str) -> Result<()> {
    println!("Locking {}...", repo_name);

    let repo_manager = repo::RepoManager::new(config.clone());
    let mut meta = repo_manager.load_meta(repo_name)?;

    // Check if extracted
    if meta.state == repo::RepoState::Archived {
        return Err(GitkaError::NotExtracted(repo_name.to_string()));
    }

    let extraction_path = meta.extraction_path
        .ok_or_else(|| GitkaError::Extraction(format!("No extraction path for {}", repo_name)))?;

    // Audit trail: record new HEAD and changed files before recompression
    let mut dirty_log = dirty::DirtyLog::load(config);
    let mut session_summary = Vec::new();

    if let Ok(repo) = git2::Repository::open(&extraction_path) {
        // Record new HEAD
        if let Ok(head) = repo.head() {
            if let Some(oid) = head.target() {
                let new_hash = oid.to_string();
                dirty_log.record_commit(repo_name, &new_hash);

                // Compare with baseline to find new commits
                if let Some(entry) = dirty_log.entries.get(repo_name) {
                    if let Some(baseline) = entry.commits.first() {
                        if baseline != &new_hash {
                            // Count new commits
                            if let Ok(base_oid) = git2::Oid::from_str(baseline) {
                                if let Ok(walker) = repo.revwalk() {
                                    let mut revwalk = walker;
                                    revwalk.set_sorting(git2::Sort::TIME).ok();
                                    revwalk.push(oid).ok();
                                    revwalk.hide(base_oid).ok();

                                    let new_commits: Vec<String> = revwalk
                                        .filter_map(|r| r.ok())
                                        .filter_map(|oid| {
                                            repo.find_commit(oid).ok().map(|c| {
                                                let summary = c.summary().unwrap_or("");
                                                format!("{} {}", &oid.to_string()[..8], summary)
                                            })
                                        })
                                        .collect();

                                    if !new_commits.is_empty() {
                                        dirty_log.record_modified(repo_name);
                                        session_summary = new_commits;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // Record touched files via diff
        if let Ok(head) = repo.head() {
            if let Some(oid) = head.target() {
                if let Ok(commit) = repo.find_commit(oid) {
                    if let Ok(tree) = commit.tree() {
                        let mut diff_opts = git2::DiffOptions::new();
                        if let Ok(diff) = repo.diff_tree_to_workdir(Some(&tree), Some(&mut diff_opts)) {
                            for delta_idx in 0..diff.deltas().len() {
                                if let Some(delta) = diff.deltas().nth(delta_idx) {
                                    if let Some(path) = delta.new_file().path() {
                                        dirty_log.record_file_touched(repo_name, &path.to_string_lossy());
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    dirty_log.save(config)?;

    // Recompress
    let archive_path = config.archive_dir().join(format!("{}.gitka.zst", repo_name));
    std::fs::create_dir_all(config.archive_dir())?;

    // Initialize dedup store if enabled
    let mut dedup_store = if config.compression.dedup {
        let mut store = dedup::DedupStore::open(config);
        store.init().ok();
        store.load_index().ok();
        Some(store)
    } else {
        None
    };

    let compress_result = compress::compress_directory_with_options(&extraction_path, &archive_path, &config.compression, dedup_store.as_mut())?;

    // Save dedup index
    if let Some(store) = dedup_store.as_ref() {
        store.save_index().ok();
    }

    // Clean up old volume parts if the number changed
    if meta.volume_count > 1 {
        for i in 2..=meta.volume_count {
            let old_part = archive_path.parent()
                .unwrap_or(Path::new("."))
                .join(format!("{}.{:03}", archive_path.file_name().unwrap_or_default().to_string_lossy(), i));
            if old_part.exists() && !compress_result.part_files.contains(&old_part.file_name().unwrap_or_default().to_string_lossy().to_string()) {
                std::fs::remove_file(&old_part).ok();
            }
        }
    }

    let new_size = compress_result.total_size;
    let new_hash = compress::calculate_hash(&archive_path)?;

    // Verify before deleting old archive
    if config.toggles.verify_after_sync {
        compress::verify_archive(&archive_path)?;
    }

    // Encrypt if enabled (per-volume)
    if config.toggles.encryption {
        if let Some(key) = config.get_encryption_key() {
            println!("  Encrypting...");
            match encryption::encrypt_parts(&archive_path, &compress_result.part_files, &key) {
                Ok(enc_size) => {
                    println!("  ✓ Encrypted ({:.1} MB)", enc_size as f64 / 1_048_576.0);
                }
                Err(e) => {
                    println!("  ⚠ Encryption failed: {}", e);
                }
            }
        }
    }

    // Clean up extraction
    if config.toggles.clear_after_lock {
        std::fs::remove_dir_all(&extraction_path)?;
    }

    // Clean up old recovery records (will be regenerated on next sync)
    let old_recovery_dir = config.recovery_dir().join(repo_name);
    if old_recovery_dir.exists() {
        std::fs::remove_dir_all(&old_recovery_dir)?;
    }

    // Update metadata
    meta.state = repo::RepoState::Archived;
    meta.archive_path = PathBuf::from(format!("{}.gitka.zst", repo_name));
    meta.archive_hash = Some(new_hash);
    meta.archive_size = new_size;
    meta.volume_count = compress_result.volume_count;
    meta.archive_parts = compress_result.part_files;
    meta.dedup_enabled = config.compression.dedup;
    meta.dedup_bytes_saved = compress_result.dedup_bytes_saved;
    meta.extraction_path = None;
    repo_manager.save_meta(&meta)?;

    // Clear from dirty log (recompressed and archived)
    let mut dirty_log = dirty::DirtyLog::load(config);
    dirty_log.clear_repo(repo_name);
    dirty_log.save(config)?;

    println!("✓ Repo locked and recompressed");
    println!("  Archive: {} ({:.1} MB)", archive_path.display(), new_size as f64 / 1_048_576.0);
    if compress_result.volume_count > 1 {
        println!("  Volumes: {} parts", compress_result.volume_count);
    }
    if compress_result.dedup_bytes_saved > 0 {
        println!("  Dedup saved: {:.1} MB", compress_result.dedup_bytes_saved as f64 / 1_048_576.0);
    }

    // Show session audit trail
    if !session_summary.is_empty() {
        println!("\n  📝 Session audit trail ({} new commit(s)):", session_summary.len());
        for commit in &session_summary {
            println!("    • {}", commit);
        }
    } else {
        println!("\n  📝 No new commits during this session");
    }

    Ok(())
}

/// Serve a repo via GitFlare
fn cmd_serve(config: &Config, repo_name: &str, stop: bool) -> Result<()> {
    if stop {
        println!("Stopping GitFlare server for {}...", repo_name);
        match serve::stop(config, repo_name)? {
            true => {
                // Clear from dirty log when server stops (recompressed by stop)
                let mut dirty_log = dirty::DirtyLog::load(config);
                dirty_log.clear_repo(repo_name);
                dirty_log.save(config)?;
                println!("✓ Server stopped")
            }
            false => println!("  No server running for {}", repo_name),
        }
    } else {
        println!("Starting GitFlare server for {}...", repo_name);
        let info = serve::start(config, repo_name)?;

        // Record in dirty log
        let mut dirty_log = dirty::DirtyLog::load(config);
        dirty_log.record_serve(repo_name);
        dirty_log.save(config)?;

        println!("✓ Server started on port {}", info.port);
        println!();
        println!("  Clone URL (local):   {}", info.clone_url());
        println!("  Clone URL (LAN):     {}/{}.git", info.lan_url(), repo_name);
        println!();
        println!("  Other machines on your LAN can clone with:");
        println!("    git clone {}/{}.git", info.lan_url(), repo_name);
        println!();
        println!("  Run `gitka serve {} --stop` to stop.", repo_name);
    }

    Ok(())
}

/// Verify archive integrity
fn cmd_verify(config: &Config, repos: Option<Vec<String>>, verbose: bool) -> Result<()> {
    let repo_manager = repo::RepoManager::new(config.clone());
    let all_repos = repo_manager.list_repos()?;

    let repos_to_verify: Vec<&repo::RepoMeta> = match repos {
        Some(names) => all_repos.iter()
            .filter(|r| names.contains(&r.name))
            .collect(),
        None => all_repos.iter().collect(),
    };

    if repos_to_verify.is_empty() {
        println!("No repos to verify.");
        return Ok(());
    }

    println!("Verifying {} archive(s)...\n", repos_to_verify.len());

    let mut all_ok = true;

    for repo in &repos_to_verify {
        let archive_path = repo.archive_full_path(config);
        let mut checks = Vec::new();

        // 1. Check archive exists
        if !archive_path.exists() {
            println!("✗ {} - archive not found: {}", repo.name, archive_path.display());
            all_ok = false;
            continue;
        }

        // 2. Check file size
        let file_size = std::fs::metadata(&archive_path)
            .map(|m| m.len())
            .unwrap_or(0);
        if file_size == 0 {
            println!("✗ {} - archive is empty", repo.name);
            all_ok = false;
            continue;
        }
        checks.push(format!("size: {:.1} MB", file_size as f64 / 1_048_576.0));

        // 3. Check archive header
        match archive::Archive::open(&archive_path) {
            Ok(_archive) => checks.push("header: OK".to_string()),
            Err(e) => {
                println!("✗ {} - header check failed: {}", repo.name, e);
                all_ok = false;
                continue;
            }
        }

        // 4. Verify compression integrity
        match compress::verify_archive(&archive_path) {
            Ok(()) => checks.push("decompression: OK".to_string()),
            Err(e) => {
                println!("✗ {} - decompression failed: {}", repo.name, e);
                all_ok = false;
                continue;
            }
        }

        // 5. Verify hash if stored
        if let Some(stored_hash) = &repo.archive_hash {
            match compress::calculate_hash(&archive_path) {
                Ok(computed_hash) => {
                    if &computed_hash == stored_hash {
                        checks.push("hash: OK".to_string());
                    } else {
                        println!("✗ {} - hash mismatch!", repo.name);
                        println!("  expected: {}", stored_hash);
                        println!("  computed: {}", computed_hash);
                        all_ok = false;
                        continue;
                    }
                }
                Err(e) => {
                    println!("✗ {} - hash calculation failed: {}", repo.name, e);
                    all_ok = false;
                    continue;
                }
            }
        } else {
            checks.push("hash: not stored".to_string());
        }

        // 6. Check recovery records availability
        let recovery_path = config.recovery_dir().join(format!("{}.par2", repo.name));
        if recovery_path.exists() {
            checks.push("recovery: available".to_string());
        } else {
            checks.push("recovery: none".to_string());
        }

        // 7. Check permissions (not writable by others)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Ok(meta) = std::fs::metadata(&archive_path) {
                let mode = meta.permissions().mode();
                let others_writable = (mode & 0o002) != 0;
                if others_writable {
                    checks.push("perms: world-writable (warning)".to_string());
                } else {
                    checks.push("perms: OK".to_string());
                }
            }
        }

        println!("✓ {} - {}", repo.name, checks.join(", "));
        if verbose {
            println!("  archive: {}", archive_path.display());
            println!("  hash:    {}", repo.archive_hash.as_deref().unwrap_or("n/a"));
            println!();
        }
    }

    if all_ok {
        println!("\nAll archives verified successfully.");
    } else {
        println!("\nSome archives failed verification.");
    }

    Ok(())
}

/// Repair a corrupted repo
fn cmd_repair(config: &Config, repo_name: &str) -> Result<()> {
    println!("Repairing {}...\n", repo_name);

    // Check if par2 is available
    if !recovery::is_par2_available() {
        println!("✗ par2 not found. Install it:");
        println!("  sudo apt install par2");
        println!();
        println!("Recovery records require par2 to create and repair.");
        return Ok(());
    }

    // Get repo metadata
    let repo_manager = repo::RepoManager::new(config.clone());
    let meta = repo_manager.load_meta(repo_name)?;

    let archive_path = meta.archive_full_path(config);
    let recovery_dir = config.recovery_dir().join(repo_name);

    // Check if recovery records exist
    if !recovery_dir.exists() {
        println!("✗ No recovery records found for {}", repo_name);
        println!("  Recovery dir: {}", recovery_dir.display());
        println!();
        println!("To create recovery records, run:");
        println!("  gitka sync --repos {} (with toggles.recovery_records = true)", repo_name);
        return Ok(());
    }

    // Verify recovery records first
    println!("Verifying recovery records...");
    match recovery::verify_recovery(&recovery_dir) {
        Ok(true) => println!("  ✓ Recovery records are valid"),
        Ok(false) => {
            println!("  ✗ Recovery records are corrupted");
            println!("  Cannot repair without valid recovery records");
            return Ok(());
        }
        Err(e) => {
            println!("  ✗ Failed to verify recovery records: {}", e);
            return Ok(());
        }
    }

    // Check if the archive is corrupted
    println!("\nChecking archive integrity...");
    let archive_ok = match compress::verify_archive(&archive_path) {
        Ok(()) => {
            println!("  ✓ Archive is intact");
            true
        }
        Err(e) => {
            println!("  ✗ Archive is corrupted: {}", e);
            false
        }
    };

    if archive_ok {
        println!("\nArchive is not corrupted. No repair needed.");
        return Ok(());
    }

    // Attempt repair
    println!("\nAttempting repair...");
    match recovery::repair_file(&archive_path, &recovery_dir) {
        Ok(()) => {
            println!("  ✓ Repair successful");

            // Verify the repaired file
            println!("\nVerifying repaired archive...");
            match compress::verify_archive(&archive_path) {
                Ok(()) => println!("  ✓ Repaired archive is valid"),
                Err(e) => {
                    println!("  ✗ Repaired archive still has issues: {}", e);
                    println!("  The recovery records may not have enough redundancy.");
                }
            }
        }
        Err(e) => {
            println!("  ✗ Repair failed: {}", e);
            println!();
            println!("  The recovery records may not have enough redundancy.");
            println!("  Consider re-syncing the repo from source.");
        }
    }

    Ok(())
}

/// View/edit config
fn cmd_config(config: &mut Config, set: Option<String>, get: Option<String>, config_path: &std::path::Path) -> Result<()> {
    if let Some(key) = get {
        // Get a config value
        match key.as_str() {
            "source.github_username" => println!("{}", config.source.github_username.as_deref().unwrap_or("not set")),
            "source.gitflare_url" => println!("{}", config.source.gitflare_url.as_deref().unwrap_or("not set")),
            "source.auth_token" => {
                if config.source.auth_token.is_some() {
                    println!("*** (set)");
                } else {
                    println!("not set");
                }
            }
            "target.path" => println!("{}", config.target.path.display()),
            "target.mode" => println!("{:?}", config.target.mode),
            "compression.backend" => println!("{:?}", config.compression.backend),
            "compression.tier" => println!("{:?}", config.compression.tier),
            "compression.dictionary_size_mb" => println!("{}", config.compression.dictionary_size_mb),
            "compression.dedup" => println!("{}", config.compression.dedup),
            "compression.volume_splitting.size_mb" => {
                match &config.compression.volume_splitting {
                    Some(vs) => println!("{}", vs.size_mb),
                    None => println!("off (not set)"),
                }
            }
            "extraction.target" => println!("{:?}", config.extraction.target),
            "toggles.clear_after_lock" => println!("{}", config.toggles.clear_after_lock),
            "toggles.verify_after_sync" => println!("{}", config.toggles.verify_after_sync),
            "toggles.encryption" => println!("{}", config.toggles.encryption),
            "toggles.recovery_records" => println!("{}", config.toggles.recovery_records),
            "integrations.gitflare.port" => {
                println!("{}", config.integrations.gitflare.as_ref().map(|g| g.port).unwrap_or(8080));
            }
            "integrations.gitflare.bind_address" => {
                println!("{}", config.integrations.gitflare.as_ref().map(|g| g.bind_address.as_str()).unwrap_or("0.0.0.0"));
            }
            _ => {
                println!("Unknown config key: {}", key);
                println!("\nAvailable keys:");
                println!("  source.github_username");
                println!("  source.gitflare_url");
                println!("  source.auth_token");
                println!("  target.path");
                println!("  target.mode              (removable | local)");
                println!("  compression.backend      (zstd)");
                println!("  compression.tier         (auto | low | medium | high)");
                println!("  compression.dictionary_size_mb");
                println!("  compression.dedup        (true | false)");
                println!("  compression.volume_splitting.size_mb (number | off)");
                println!("  extraction.target        (usb | host)");
                println!("  toggles.clear_after_lock (true | false)");
                println!("  toggles.verify_after_sync (true | false)");
                println!("  toggles.encryption       (true | false)");
                println!("  toggles.recovery_records (true | false)");
                println!("  integrations.gitflare.port");
                println!("  integrations.gitflare.bind_address");
            }
        }
    } else if let Some(kv) = set {
        // Set a config value: key=value format
        let parts: Vec<&str> = kv.splitn(2, '=').collect();
        if parts.len() != 2 {
            println!("Invalid format. Use: key=value");
            return Ok(());
        }
        let key = parts[0].trim();
        let value = parts[1].trim();

        match key {
            "source.github_username" => config.source.github_username = Some(value.to_string()),
            "source.gitflare_url" => config.source.gitflare_url = Some(value.to_string()),
            "source.auth_token" => config.source.auth_token = Some(value.to_string()),
            "target.path" => config.target.path = std::path::PathBuf::from(value),
            "target.mode" => {
                match value {
                    "removable" => config.target.mode = config::TargetMode::Removable,
                    "local" => config.target.mode = config::TargetMode::Local,
                    _ => { println!("Invalid mode. Use: removable | local"); return Ok(()); }
                }
            }
            "compression.backend" => {
                match value {
                    "zstd" => config.compression.backend = config::CompressionBackend::Zstd,
                    _ => { println!("Invalid backend. Use: zstd"); return Ok(()); }
                }
            }
            "compression.tier" => {
                match value {
                    "auto" => config.compression.tier = config::CompressionTier::Auto,
                    "low" => config.compression.tier = config::CompressionTier::Low,
                    "medium" => config.compression.tier = config::CompressionTier::Medium,
                    "high" => config.compression.tier = config::CompressionTier::High,
                    _ => { println!("Invalid tier. Use: auto | low | medium | high"); return Ok(()); }
                }
            }
            "compression.dictionary_size_mb" => {
                match value.parse::<u32>() {
                    Ok(n) => config.compression.dictionary_size_mb = n,
                    Err(_) => { println!("Invalid number: {}", value); return Ok(()); }
                }
            }
            "compression.dedup" => {
                match value {
                    "true" => config.compression.dedup = true,
                    "false" => config.compression.dedup = false,
                    _ => { println!("Invalid value. Use: true | false"); return Ok(()); }
                }
            }
            "compression.volume_splitting.size_mb" => {
                match value {
                    "off" | "0" => config.compression.volume_splitting = None,
                    val => {
                        match val.parse::<u64>() {
                            Ok(n) if n > 0 => config.compression.volume_splitting = Some(config::VolumeSplitting { size_mb: n }),
                            _ => { println!("Invalid value. Use: a positive number in MB, or 'off' to disable"); return Ok(()); }
                        }
                    }
                }
            }
            "extraction.target" => {
                match value {
                    "usb" => config.extraction.target = config::ExtractionTarget::Usb,
                    "host" => config.extraction.target = config::ExtractionTarget::Host,
                    _ => { println!("Invalid target. Use: usb | host"); return Ok(()); }
                }
            }
            "toggles.clear_after_lock" => {
                match value {
                    "true" => config.toggles.clear_after_lock = true,
                    "false" => config.toggles.clear_after_lock = false,
                    _ => { println!("Invalid value. Use: true | false"); return Ok(()); }
                }
            }
            "toggles.verify_after_sync" => {
                match value {
                    "true" => config.toggles.verify_after_sync = true,
                    "false" => config.toggles.verify_after_sync = false,
                    _ => { println!("Invalid value. Use: true | false"); return Ok(()); }
                }
            }
            "toggles.encryption" => {
                match value {
                    "true" => config.toggles.encryption = true,
                    "false" => config.toggles.encryption = false,
                    _ => { println!("Invalid value. Use: true | false"); return Ok(()); }
                }
            }
            "toggles.recovery_records" => {
                match value {
                    "true" => config.toggles.recovery_records = true,
                    "false" => config.toggles.recovery_records = false,
                    _ => { println!("Invalid value. Use: true | false"); return Ok(()); }
                }
            }
            "integrations.gitflare.port" => {
                match value.parse::<u16>() {
                    Ok(n) => {
                        if config.integrations.gitflare.is_none() {
                            config.integrations.gitflare = Some(config::GitFlareConfig {
                                port: n,
                                bind_address: "0.0.0.0".to_string(),
                            });
                        } else {
                            config.integrations.gitflare.as_mut().unwrap().port = n;
                        }
                    }
                    Err(_) => { println!("Invalid port number: {}", value); return Ok(()); }
                }
            }
            "integrations.gitflare.bind_address" => {
                if config.integrations.gitflare.is_none() {
                    config.integrations.gitflare = Some(config::GitFlareConfig {
                        port: 8080,
                        bind_address: value.to_string(),
                    });
                } else {
                    config.integrations.gitflare.as_mut().unwrap().bind_address = value.to_string();
                }
            }
            _ => {
                println!("Unknown config key: {}", key);
                println!("Run `gitka config --get unknown_key` to see available keys.");
                return Ok(());
            }
        }

        // Save config
        config.save(config_path)?;
        println!("✓ {} = {}", key, value);
    } else {
        // Show full config
        println!("Current configuration:");
        println!("{}", toml::to_string_pretty(config).unwrap());
        println!("\nConfig file: {}", config_path.display());
        println!("\nUse `gitka config --set key=value` to edit.");
        println!("Use `gitka config --get key` to query.");
    }

    Ok(())
}

/// Import an existing local git repo into the backup target
fn cmd_import(config: &Config, repo_path: &std::path::Path, name: Option<&str>) -> Result<()> {
    // Verify it's a git repo
    if !repo_path.join(".git").exists() {
        return Err(GitkaError::Config(format!(
            "Not a git repository: {}",
            repo_path.display()
        )));
    }

    // Determine repo name
    let repo_name = match name {
        Some(n) => n.to_string(),
        None => repo_path.file_name()
            .ok_or_else(|| GitkaError::Config("Invalid repo path".to_string()))?
            .to_string_lossy()
            .to_string(),
    };

    println!("Importing {}...", repo_name);
    println!("  Source: {}", repo_path.display());

    // Get repo size
    let repo_size = source::repo_size(repo_path)?;

    // Compress the repo
    let archive_dir = config.archive_dir();
    std::fs::create_dir_all(&archive_dir)?;

    let archive_path = archive_dir.join(format!("{}.gitka.zst", repo_name));

    // Initialize dedup store if enabled
    let mut dedup_store = if config.compression.dedup {
        let mut store = dedup::DedupStore::open(config);
        store.init().ok();
        store.load_index().ok();
        Some(store)
    } else {
        None
    };

    println!("  Compressing...");
    match compress::compress_directory_with_options(repo_path, &archive_path, &config.compression, dedup_store.as_mut()) {
        Ok(result) => {
            // Save dedup index
            if let Some(store) = dedup_store.as_ref() {
                store.save_index().ok();
            }

            // Create repo metadata
            let meta = repo::RepoMeta {
                name: repo_name.clone(),
                state: repo::RepoState::Archived,
                archive_path: PathBuf::from(format!("{}.gitka.zst", repo_name)),
                archive_hash: Some(compress::calculate_hash(&archive_path)?),
                archive_size: result.total_size,
                volume_count: result.volume_count,
                archive_parts: result.part_files,
                decompressed_size: Some(repo_size),
                last_synced: None,
                last_verified: None,
                extraction_path: None,
                dedup_enabled: config.compression.dedup,
                dedup_bytes_saved: result.dedup_bytes_saved,
            };

            let repo_manager = repo::RepoManager::new(config.clone());
            repo_manager.save_meta(&meta)?;

            // Add to config repos
            let mut config = config.clone();
            if config.get_repo(&repo_name).is_err() {
                config.repos.push(config::RepoConfig {
                    name: repo_name.clone(),
                    workspace_eligible: true,
                    full_history: true,
                    last_synced: None,
                });
                let config_path = config.state_dir().join("gitka.toml");
                config.save(&config_path)?;
            }

            println!("  ✓ Imported ({:.1} MB archive, {:.1} MB source)",
                result.total_size as f64 / 1_048_576.0,
                repo_size as f64 / 1_048_576.0);

            // Encrypt if enabled (per-volume)
            if config.toggles.encryption {
                if let Some(key) = config.get_encryption_key() {
                    println!("  Encrypting...");
                    match encryption::encrypt_parts(&archive_path, &meta.archive_parts, &key) {
                        Ok(enc_size) => {
                            println!("  ✓ Encrypted ({:.1} MB)", enc_size as f64 / 1_048_576.0);
                        }
                        Err(e) => {
                            println!("  ⚠ Encryption failed: {}", e);
                        }
                    }
                }
            }

            // Create recovery records if enabled (per-volume)
            if config.toggles.recovery_records && recovery::is_par2_available() {
                println!("  Creating recovery records...");
                let recovery_dir = config.recovery_dir().join(&repo_name);
                match recovery::create_recovery_parts(&archive_path, &meta.archive_parts, &recovery_dir, 25) {
                    Ok(infos) => {
                        let total_recovery: u64 = infos.iter().map(|i| i.recovery_size).sum();
                        println!("  ✓ Recovery records created ({:.1} MB)",
                            total_recovery as f64 / 1_048_576.0);
                    }
                    Err(e) => {
                        println!("  ⚠ Recovery record creation failed: {}", e);
                    }
                }
            }
        }
        Err(e) => {
            println!("  ⚠ Compression failed: {}", e);
        }
    }

    println!("\n✓ Import complete!");
    println!("  Archive: {}", archive_path.display());
    println!("  Run `gitka status` to see the imported repo.");

    Ok(())
}

/// Train a zstd dictionary for better compression of small files
fn cmd_train_dict(config: &Config, source: Option<&std::path::Path>) -> Result<()> {
    println!("Training zstd dictionary...");

    let source_dir = match source {
        Some(s) => s.to_path_buf(),
        None => {
            // Use the archive directory as source
            let archive_dir = config.archive_dir();
            if !archive_dir.exists() {
                return Err(GitkaError::Config(
                    "No archive directory found. Run `gitka sync` first.".to_string(),
                ));
            }
            archive_dir
        }
    };

    println!("  Source: {}", source_dir.display());
    println!("  Dictionary size: {} MB", config.compression.dictionary_size_mb);

    match compress::train_dictionary(&source_dir, config.compression.dictionary_size_mb) {
        Ok(dict) => {
            // Save the dictionary
            let dict_path = config.archive_dir().join(".trained.dict");
            std::fs::write(&dict_path, &dict)?;

            println!("  ✓ Dictionary trained ({:.1} KB, {} bytes)",
                dict.len() as f64 / 1024.0,
                dict.len());
            println!("  Saved to: {}", dict_path.display());
            println!("\n  The dictionary will be used automatically for future syncs.");
        }
        Err(e) => {
            println!("  ✗ Training failed: {}", e);
            println!("\n  Try running `gitka sync` first to have some repos to train from.");
        }
    }

    Ok(())
}
